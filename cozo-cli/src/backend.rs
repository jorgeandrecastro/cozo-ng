use std::collections::BTreeMap;

use cozo::{DataValue, DbInstance, NamedRows, ScriptMutability};
use miette::{miette, Result};
use serde_json::{json, Value as JsonValue};

use crate::target::Target;
use crate::util::to_json;

/// Everything the commands need from a database, local or remote.
pub trait Backend {
    fn run(&self, script: &str, params: BTreeMap<String, DataValue>, mutable: bool) -> Result<NamedRows>;
    fn backup(&self, path: &str) -> Result<()>;
    fn restore(&self, path: &str) -> Result<()>;
}

pub fn connect(target: &Target, token: Option<String>) -> Result<Box<dyn Backend>> {
    match target {
        Target::Embedded { engine, path } => {
            let db = DbInstance::new(engine, path, "")
                .map_err(|e| e.wrap_err(format!("cannot open {}", target.describe())))?;
            Ok(Box::new(Embedded { db }))
        }
        Target::Remote { url } => Ok(Box::new(Remote { base: url.trim_end_matches('/').to_string(), token })),
    }
}

pub struct Embedded {
    db: DbInstance,
}

impl Backend for Embedded {
    fn run(&self, script: &str, params: BTreeMap<String, DataValue>, mutable: bool) -> Result<NamedRows> {
        let m = if mutable { ScriptMutability::Mutable } else { ScriptMutability::Immutable };
        // Attach the source so Cozo's diagnostics can underline the faulty span.
        self.db.run_script(script, params, m).map_err(|e| e.with_source_code(script.to_string()))
    }

    fn backup(&self, path: &str) -> Result<()> {
        self.db.backup_db(path)
    }

    fn restore(&self, path: &str) -> Result<()> {
        self.db.restore_backup(path)
    }
}

pub struct Remote {
    base: String,
    token: Option<String>,
}

impl Remote {
    fn post(&self, path: &str, body: &JsonValue) -> Result<JsonValue> {
        let mut req = minreq::post(format!("{}{}", self.base, path))
            .with_header("content-type", "application/json")
            .with_timeout(600);
        if let Some(t) = &self.token {
            req = req.with_header("x-cozo-auth", t.as_str());
        }
        let resp = req
            .with_body(body.to_string())
            .send()
            .map_err(|e| miette!("request to {} failed: {e}", self.base))?;
        if matches!(resp.status_code, 401 | 403) {
            return Err(miette!("authentication failed: pass --token or set COZO_AUTH"));
        }
        let text = resp.as_str().map_err(|e| miette!("invalid response: {e}"))?;
        let value: JsonValue =
            serde_json::from_str(text).map_err(|_| miette!("unexpected response ({}): {text}", resp.status_code))?;
        if value.get("ok") == Some(&JsonValue::Bool(false)) {
            let msg = value
                .get("display")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("message").and_then(|v| v.as_str()))
                .unwrap_or("remote error");
            return Err(miette!("{}", strip_ansi(msg).trim_end()));
        }
        Ok(value)
    }
}

/// Removes terminal colour codes: the server renders its diagnostics for a terminal.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn named_rows(v: &JsonValue) -> Result<NamedRows> {
    let mut nr = NamedRows::from_json(v)?;
    if let Some(next) = v.get("next").filter(|n| n.is_object()) {
        nr.next = Some(Box::new(named_rows(next)?));
    }
    Ok(nr)
}

impl Backend for Remote {
    fn run(&self, script: &str, params: BTreeMap<String, DataValue>, mutable: bool) -> Result<NamedRows> {
        let params: serde_json::Map<String, JsonValue> =
            params.iter().map(|(k, v)| (k.clone(), to_json(v))).collect();
        let body = json!({ "script": script, "params": params, "immutable": !mutable });
        named_rows(&self.post("/text-query", &body)?)
    }

    fn backup(&self, path: &str) -> Result<()> {
        self.post("/backup", &json!({ "path": path }))?;
        Ok(())
    }

    fn restore(&self, _path: &str) -> Result<()> {
        Err(miette!(
            "restore is not available over HTTP: start the server with `cozo-bin server --restore <file>`, \
             or restore into a local database"
        ))
    }
}
