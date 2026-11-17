/*
 * Copyright 2023, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::net::{Ipv6Addr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderName, Method, Request, Response, StatusCode};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{Html, Sse};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use clap::Args;
use futures::future::BoxFuture;
use futures::stream::Stream;
use itertools::Itertools;
use log::{error, info, warn};
use miette::miette;
use rand::Rng;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::task::spawn_blocking;
use tower_http::auth::{AsyncAuthorizeRequest, AsyncRequireAuthorizationLayer};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

use cozo::{
    format_error_as_json, DataValue, DbInstance, MultiTransaction, NamedRows, ScriptMutability,
    SimpleFixedRule,
};

#[derive(Args, Debug)]
pub(crate) struct ServerArgs {
    /// Database engine, can be `mem`, `sqlite`, `rocksdb` and others.
    #[clap(short, long, default_value_t = String::from("mem"))]
    engine: String,

    /// Path to the directory to store the database
    #[clap(short, long, default_value_t = String::from("cozo.db"))]
    path: String,

    /// Restore from the specified backup before starting the server
    #[clap(long)]
    restore: Option<String>,

    /// Extra config in JSON format
    #[clap(short, long, default_value_t = String::from("{}"))]
    config: String,

    /// Address to bind the service to
    #[clap(short, long, default_value_t = String::from("127.0.0.1"))]
    bind: String,

    /// Port to use
    #[clap(short = 'P', long, default_value_t = 9070)]
    port: u16,

    /// When set, the content of the named table will be used as a token table
    #[clap(long)]
    token_table: Option<String>,

    /// Multi-statement transactions idle for longer than this many seconds are aborted
    #[clap(long, default_value_t = 300)]
    tx_timeout: u64,

    /// Maximum size of a request body, in MiB
    #[clap(long, default_value_t = 512)]
    max_body_mb: u64,

    /// Disable HTTP response compression (saves CPU, useful on loopback/LAN)
    #[clap(long)]
    no_compress: bool,
}

struct TxEntry {
    tx: Arc<MultiTransaction>,
    last_used: Instant,
}

type RuleSenders =
    Arc<Mutex<BTreeMap<u32, (String, crossbeam::channel::Sender<miette::Result<NamedRows>>)>>>;

#[derive(Clone)]
struct DbState {
    db: DbInstance,
    rule_senders: RuleSenders,
    rule_counter: Arc<AtomicU32>,
    tx_counter: Arc<AtomicU64>,
    txs: Arc<Mutex<BTreeMap<u64, TxEntry>>>,
}

#[derive(Clone)]
struct MyAuth {
    skip_auth: bool,
    auth_guard: String,
    token_table: Option<Arc<(String, DbInstance)>>,
}

/// Constant-time comparison, to avoid leaking the token through timing.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Looks the bearer token up in the token table, off the async worker threads.
async fn lookup_token(tt: Arc<(String, DbInstance)>, token: String) -> Option<ScriptMutability> {
    let res = spawn_blocking(move || {
        let (name, db) = tt.as_ref();
        match db.run_script(
            &format!("?[mutable] := *{name} {{ token: $token, mutable }}"),
            BTreeMap::from([(String::from("token"), DataValue::from(token.as_str()))]),
            ScriptMutability::Immutable,
        ) {
            Ok(rows) => rows.rows.first().map(|val| {
                if val[0].get_bool() == Some(true) {
                    ScriptMutability::Mutable
                } else {
                    ScriptMutability::Immutable
                }
            }),
            Err(err) => {
                error!("token table lookup failed: {}", err);
                None
            }
        }
    })
    .await;
    res.ok().flatten()
}

impl AsyncAuthorizeRequest<Body> for MyAuth {
    type RequestBody = Body;
    type ResponseBody = Body;
    type Future = BoxFuture<'static, Result<Request<Body>, Response<Self::ResponseBody>>>;

    fn authorize(&mut self, mut request: Request<Body>) -> Self::Future {
        let skip_auth = self.skip_auth;
        let auth_guard = self.auth_guard.clone();
        let token_table = self.token_table.clone();
        Box::pin(async move {
            if skip_auth {
                request.extensions_mut().insert(ScriptMutability::Mutable);
                return Ok(request);
            }

            let mut mutability: Option<ScriptMutability> = None;

            // 1. `x-cozo-auth` header
            if let Some(h) = request.headers().get("x-cozo-auth") {
                if let Ok(s) = h.to_str() {
                    if ct_eq(s.as_bytes(), auth_guard.as_bytes()) {
                        mutability = Some(ScriptMutability::Mutable);
                    }
                }
            }

            // 2. `?auth=` query parameter
            if mutability.is_none() {
                if let Some(q_str) = request.uri().query() {
                    for pair in q_str.split('&') {
                        if let Some((k, v)) = pair.split_once('=') {
                            if k == "auth" {
                                if ct_eq(v.as_bytes(), auth_guard.as_bytes()) {
                                    mutability = Some(ScriptMutability::Mutable);
                                }
                                break;
                            }
                        }
                    }
                }
            }

            // 3. `Authorization: Bearer <token>` checked against the token table
            if mutability.is_none() {
                if let Some(tt) = token_table {
                    let token = request
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.strip_prefix("Bearer "))
                        .map(str::to_owned);
                    if let Some(token) = token {
                        mutability = lookup_token(tt, token).await;
                    }
                }
            }

            if let Some(mutability) = mutability {
                request.extensions_mut().insert(mutability);
                Ok(request)
            } else {
                let unauthorized_response = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::empty())
                    .unwrap();

                Err(unauthorized_response)
            }
        })
    }
}

pub(crate) async fn server_main(args: ServerArgs) {
    let db = DbInstance::new(&args.engine, &args.path, &args.config).unwrap();
    if let Some(p) = &args.restore {
        if let Err(err) = db.restore_backup(p) {
            error!("{}", err);
            error!("Restore from backup failed, terminate");
            panic!()
        }
    }

    let skip_auth = args.bind == "127.0.0.1";

    let conf_path = if skip_auth {
        "".to_string()
    } else {
        format!("{}.{}.cozo_auth", args.path, args.engine)
    };
    let auth_guard = if skip_auth {
        "".to_string()
    } else {
        match tokio::fs::read_to_string(&conf_path).await {
            Ok(s) => s.trim().to_string(),
            Err(_) => {
                let s = rand::thread_rng()
                    .sample_iter(&rand::distributions::Alphanumeric)
                    .take(64)
                    .map(char::from)
                    .collect();
                tokio::fs::write(&conf_path, &s).await.unwrap();
                s
            }
        }
    };

    let tx_ttl = Duration::from_secs(args.tx_timeout);
    let body_limit = (args.max_body_mb as usize).saturating_mul(1024 * 1024);
    let compress = !args.no_compress;

    let auth_obj = MyAuth {
        skip_auth,
        auth_guard,
        token_table: args.token_table.map(|t| Arc::new((t, db.clone()))),
    };

    let state = DbState {
        db,
        rule_senders: Default::default(),
        rule_counter: Default::default(),
        tx_counter: Default::default(),
        txs: Default::default(),
    };

    // Background task: abort multi-statement transactions that were abandoned by their client.
    {
        let txs = state.txs.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                let expired: Vec<Arc<MultiTransaction>> = {
                    let mut guard = txs.lock().unwrap();
                    let ids: Vec<u64> = guard
                        .iter()
                        .filter(|(_, e)| e.last_used.elapsed() > tx_ttl)
                        .map(|(k, _)| *k)
                        .collect();
                    ids.into_iter()
                        .filter_map(|id| guard.remove(&id))
                        .map(|e| e.tx)
                        .collect()
                };
                for tx in expired {
                    warn!("aborting idle transaction");
                    let _ = spawn_blocking(move || tx.abort()).await;
                }
            }
        });
    }

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_origin(Any)
        .allow_headers([header::CONTENT_TYPE, HeaderName::from_static("x-cozo-auth")]);

    let app = Router::new()
        .route("/text-query", post(text_query))
        .route("/export/{relations}", get(export_relations))
        .route("/import", put(import_relations))
        .route("/backup", post(backup))
        .route("/import-from-backup", post(import_from_backup))
        .route("/changes/{relation}", get(observe_changes))
        .route("/rules/{name}", get(register_rule))
        .route(
            "/rule-result/{id}",
            post(post_rule_result).delete(post_rule_err),
        ) // +keep alive
        .route("/transact", post(start_transact))
        .route("/transact/{id}", post(transact_query).put(finish_query))
        .with_state(state)
        .layer(AsyncRequireAuthorizationLayer::new(auth_obj))
        .fallback(not_found)
        .route("/", get(root))
        .layer(cors);

    let app = if compress {
        app.layer(CompressionLayer::new())
    } else {
        app
    };
    let app = app.layer(DefaultBodyLimit::max(body_limit));

    let addr = if Ipv6Addr::from_str(&args.bind).is_ok() {
        SocketAddr::from_str(&format!("[{}]:{}", args.bind, args.port)).unwrap()
    } else {
        SocketAddr::from_str(&format!("{}:{}", args.bind, args.port)).unwrap()
    };

    if args.bind != "127.0.0.1" {
        warn!("{}", include_str!("./security.txt"));
        info!("The auth token is in the file: {conf_path}");
    }

    info!(
        "Starting Cozo ({}-backed) API at http://{}",
        args.engine, addr
    );

    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[derive(serde_derive::Deserialize)]
struct StartTransactPayload {
    write: bool,
}

async fn start_transact(
    State(st): State<DbState>,
    Query(payload): Query<StartTransactPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let db = st.db.clone();
    let write = payload.write;
    match spawn_blocking(move || db.multi_transaction(write)).await {
        Ok(tx) => {
            let id = st.tx_counter.fetch_add(1, Ordering::SeqCst);
            st.txs.lock().unwrap().insert(
                id,
                TxEntry {
                    tx: Arc::new(tx),
                    last_used: Instant::now(),
                },
            );
            (StatusCode::OK, json!({"ok": true, "id": id}).into())
        }
        Err(err) => internal_error(err),
    }
}

async fn transact_query(
    State(st): State<DbState>,
    Path(id): Path<u64>,
    Json(payload): Json<QueryPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tx = {
        let mut guard = st.txs.lock().unwrap();
        match guard.get_mut(&id) {
            None => return (StatusCode::NOT_FOUND, json!({"ok": false}).into()),
            Some(entry) => {
                entry.last_used = Instant::now();
                entry.tx.clone()
            }
        }
    };
    let src = payload.script.clone();
    let result = spawn_blocking(move || {
        let params = payload
            .params
            .into_iter()
            .map(|(k, v)| (k, DataValue::from(v)))
            .collect();
        let query = payload.script;
        tx.run_script(&query, params)
    })
    .await;
    match result {
        Ok(Ok(res)) => (StatusCode::OK, res.into_json().into()),
        Ok(Err(err)) => (
            StatusCode::BAD_REQUEST,
            format_error_as_json(err, Some(&src)).into(),
        ),
        Err(err) => internal_error(err),
    }
}

#[derive(serde_derive::Deserialize)]
struct FinishTransactPayload {
    abort: bool,
}

async fn finish_query(
    State(st): State<DbState>,
    Path(id): Path<u64>,
    Json(payload): Json<FinishTransactPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tx = match st.txs.lock().unwrap().remove(&id) {
        None => return (StatusCode::NOT_FOUND, json!({"ok": false}).into()),
        Some(entry) => entry.tx,
    };
    let abort = payload.abort;
    let res = spawn_blocking(move || if abort { tx.abort() } else { tx.commit() }).await;
    match res {
        Ok(Ok(_)) => (StatusCode::OK, json!({"ok": true}).into()),
        Ok(Err(err)) => (
            StatusCode::BAD_REQUEST,
            json!({"ok": false, "message": err.to_string()}).into(),
        ),
        Err(err) => internal_error(err),
    }
}

#[derive(serde_derive::Deserialize)]
struct QueryPayload {
    script: String,
    params: BTreeMap<String, serde_json::Value>,
    immutable: Option<bool>,
}

async fn text_query(
    Extension(mutability): Extension<ScriptMutability>,
    State(st): State<DbState>,
    Json(payload): Json<QueryPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let immutable = match mutability {
        ScriptMutability::Mutable => payload.immutable.unwrap_or(false),
        ScriptMutability::Immutable => true,
    };
    let result = spawn_blocking(move || {
        // the JSON -> DataValue conversion can be costly for big params, keep it off the async threads
        let params = payload
            .params
            .into_iter()
            .map(|(k, v)| (k, DataValue::from(v)))
            .collect();
        st.db.run_script_fold_err(
            &payload.script,
            params,
            if immutable {
                ScriptMutability::Immutable
            } else {
                ScriptMutability::Mutable
            },
        )
    })
    .await;
    match result {
        Ok(res) => wrap_json(res),
        Err(err) => internal_error(err),
    }
}

async fn export_relations(
    State(st): State<DbState>,
    Path(relations): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let relations = relations
        .split(',')
        .filter_map(|t| {
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect_vec();
    let result = spawn_blocking(move || st.db.export_relations(relations.iter())).await;
    match result {
        Ok(Ok(s)) => {
            let s: serde_json::Map<_, _> = s.into_iter().map(|(k, v)| (k, v.into_json())).collect();
            let ret = json!({"ok": true, "data": s});
            (StatusCode::OK, ret.into())
        }
        Ok(Err(err)) => {
            let ret = json!({"ok": false, "message": err.to_string()});
            (StatusCode::BAD_REQUEST, ret.into())
        }
        Err(err) => internal_error(err),
    }
}

async fn import_relations(
    State(st): State<DbState>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    // parsing a large payload into NamedRows is CPU-bound: do it in the blocking pool too
    let result = spawn_blocking(move || -> miette::Result<()> {
        let obj = payload
            .as_object()
            .ok_or_else(|| miette!("payload must be a JSON object"))?;
        let mut ret = BTreeMap::new();
        for (k, v) in obj {
            ret.insert(k.to_string(), NamedRows::from_json(v)?);
        }
        st.db.import_relations(ret)
    })
    .await;
    match result {
        Ok(Ok(_)) => (StatusCode::OK, json!({"ok": true}).into()),
        Ok(Err(err)) => {
            let ret = json!({"ok": false, "message": err.to_string()});
            (StatusCode::BAD_REQUEST, ret.into())
        }
        Err(err) => internal_error(err),
    }
}

#[derive(serde_derive::Deserialize)]
struct BackupPayload {
    path: String,
}

async fn backup(
    State(st): State<DbState>,
    Json(payload): Json<BackupPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let result = spawn_blocking(move || st.db.backup_db(payload.path)).await;

    match result {
        Ok(Ok(())) => {
            let ret = json!({"ok": true});
            (StatusCode::OK, ret.into())
        }
        Ok(Err(err)) => {
            let ret = json!({"ok": false, "message": err.to_string()});
            (StatusCode::BAD_REQUEST, ret.into())
        }
        Err(err) => internal_error(err),
    }
}

#[derive(serde_derive::Deserialize)]
struct BackupImportPayload {
    path: String,
    relations: Vec<String>,
}

async fn import_from_backup(
    State(st): State<DbState>,
    Json(payload): Json<BackupImportPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let result =
        spawn_blocking(move || st.db.import_from_backup(&payload.path, &payload.relations)).await;

    match result {
        Ok(Ok(())) => {
            let ret = json!({"ok": true});
            (StatusCode::OK, ret.into())
        }
        Ok(Err(err)) => {
            let ret = json!({"ok": false, "message": err.to_string()});
            (StatusCode::BAD_REQUEST, ret.into())
        }
        Err(err) => internal_error(err),
    }
}

#[derive(serde_derive::Deserialize)]
struct RuleRegisterOptions {
    arity: usize,
}

async fn post_rule_result(
    State(st): State<DbState>,
    Path(id): Path<u32>,
    Json(res): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let res = match NamedRows::from_json(&res) {
        Ok(res) => res,
        Err(err) => {
            if let Some((_, ch)) = st.rule_senders.lock().unwrap().remove(&id) {
                let _ = ch.send(Err(miette!("downstream posted malformed result")));
            }
            return (
                StatusCode::BAD_REQUEST,
                json!({"ok": false, "message": err.to_string()}).into(),
            );
        }
    };
    let sender = st.rule_senders.lock().unwrap().remove(&id);
    if let Some((_, ch)) = sender {
        match ch.send(Ok(res)) {
            Ok(_) => (StatusCode::OK, json!({"ok": true}).into()),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"ok": false, "message": err.to_string()}).into(),
            ),
        }
    } else {
        (StatusCode::NOT_FOUND, json!({"ok": false}).into())
    }
}

async fn post_rule_err(
    State(st): State<DbState>,
    Path(id): Path<u32>,
) -> (StatusCode, Json<serde_json::Value>) {
    let sender = st.rule_senders.lock().unwrap().remove(&id);
    if let Some((_, ch)) = sender {
        match ch.send(Err(miette!("downstream cancelled computation"))) {
            Ok(_) => (StatusCode::OK, json!({"ok": true}).into()),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"ok": false, "message": err.to_string()}).into(),
            ),
        }
    } else {
        (StatusCode::NOT_FOUND, json!({"ok": false}).into())
    }
}

/// Unregisters the rule when the SSE stream goes away, and fails every request
/// still waiting for an answer from this (now gone) downstream.
struct RuleGuard {
    name: String,
    db: DbInstance,
    rule_senders: RuleSenders,
}

impl Drop for RuleGuard {
    fn drop(&mut self) {
        info!("dropping rules SSE {}", self.name);
        let _ = self.db.unregister_fixed_rule(&self.name);
        let mut guard = self.rule_senders.lock().unwrap();
        let ids: Vec<u32> = guard
            .iter()
            .filter(|(_, v)| v.0 == self.name)
            .map(|(k, _)| *k)
            .collect();
        for id in ids {
            if let Some((_, s)) = guard.remove(&id) {
                let _ = s.send(Err(miette!("rule stream closed")));
            }
        }
    }
}

async fn register_rule(
    State(st): State<DbState>,
    Path(name): Path<String>,
    Query(rule_opts): Query<RuleRegisterOptions>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (rule, task_receiver) = SimpleFixedRule::rule_with_channel(rule_opts.arity);
    let (down_sender, mut down_receiver) = tokio::sync::mpsc::channel(16);
    let mut errored = None;

    if let Err(err) = st.db.register_fixed_rule(name.clone(), rule) {
        errored = Some(err);
    } else {
        let rule_senders = st.rule_senders.clone();
        let rule_counter = st.rule_counter.clone();
        let rule_name = name.clone();

        // Dedicated OS thread: this loop lives as long as the rule is registered,
        // so it must not occupy a rayon worker (used by the query engine) nor a tokio thread.
        std::thread::spawn(move || {
            for (inputs, options, sender) in task_receiver {
                let id = rule_counter.fetch_add(1, Ordering::AcqRel);
                let inputs: serde_json::Value =
                    inputs.into_iter().map(|ip| ip.into_json()).collect();
                let options: serde_json::Value = options
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::from(v)))
                    .collect();
                if down_sender.blocking_send((id, inputs, options)).is_err() {
                    let _ = sender.send(Err(miette!("cannot send request to downstream")));
                } else {
                    rule_senders
                        .lock()
                        .unwrap()
                        .insert(id, (rule_name.clone(), sender));
                }
            }
        });
    }

    // Created before the stream so the rule is cleaned up even if the client
    // disconnects before the stream is first polled.
    let guard = if errored.is_none() {
        Some(RuleGuard {
            name: name.clone(),
            db: st.db.clone(),
            rule_senders: st.rule_senders.clone(),
        })
    } else {
        None
    };

    let stream = async_stream::stream! {
        if let Some(err) = errored {
            let item = json!({"type": "register-error", "error": err.to_string()});
            yield Ok(Event::default().json_data(item).unwrap());
        } else {
            info!("starting rule SSE {}", name);
            let _guard = guard;
            while let Some((id, inputs, options)) = down_receiver.recv().await {
                let item = json!({"type": "request", "id": id, "inputs": inputs, "options": options});
                yield Ok(Event::default().json_data(item).unwrap());
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

struct ChangesGuard {
    id: u32,
    db: DbInstance,
    relation: String,
}

impl Drop for ChangesGuard {
    fn drop(&mut self) {
        info!("dropping changes SSE {}: {}", self.relation, self.id);
        self.db.unregister_callback(self.id);
    }
}

async fn observe_changes(
    State(st): State<DbState>,
    Path(relation): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (id, recv) = st.db.register_callback(&relation, None);
    // Bigger buffer: the engine's callback blocks writers when this channel is full.
    let (sender, mut receiver) = tokio::sync::mpsc::channel(256);

    // Created before the stream: unregisters the callback even if the stream is never polled.
    let guard = ChangesGuard {
        id,
        db: st.db.clone(),
        relation: relation.clone(),
    };

    spawn_blocking(move || {
        for data in recv {
            // the client is gone: stop instead of panicking
            if sender.blocking_send(data).is_err() {
                break;
            }
        }
    });
    let stream = async_stream::stream! {
        info!("starting changes SSE {}: {}", relation, id);
        let _guard = guard;
        while let Some((op, new, old)) = receiver.recv().await {
            let item = json!({"op": op.to_string(), "new_rows": new.into_json(), "old_rows": old.into_json()});
            yield Ok(Event::default().json_data(item).unwrap());
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn root() -> Html<&'static str> {
    Html(include_str!("./index.html"))
}

fn internal_error<E>(err: E) -> (StatusCode, Json<serde_json::Value>)
where
    E: std::error::Error,
{
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"ok": false, "message": err.to_string()}).into(),
    )
}

fn wrap_json(json: serde_json::Value) -> (StatusCode, Json<serde_json::Value>) {
    let code = if let Some(serde_json::Value::Bool(true)) = json.get("ok") {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    (code, json.into())
}

pub async fn not_found(uri: axum::http::Uri) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        json!({"ok": false, "message": format!("No route {}", uri)}).into(),
    )
}