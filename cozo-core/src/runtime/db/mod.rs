/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

mod backup;
mod eval;
mod execute;
mod explain;
mod import_export;
mod multi_tx;
mod named_rows;
mod registry;
mod running;
mod sys_ops;

use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
#[allow(unused_imports)]
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam::sync::ShardedLock;
use miette::{Diagnostic, Result};
use smartstring::{LazyCompact, SmartString};
use thiserror::Error;

use crate::data::functions::current_validity;
use crate::data::tuple::{Tuple, TupleT};
use crate::data::value::{DataValue, ValidityTs};
use crate::fixed_rule::DEFAULT_FIXED_RULES;
use crate::fts::TokenizerCache;
use crate::parse::{parse_script, CozoScript};
#[allow(unused_imports)]
use crate::runtime::callback::EventCallbackRegistry;
use crate::runtime::relation::RelationId;
use crate::runtime::transact::SessionTx;
use crate::storage::temp::TempStorage;
use crate::storage::Storage;
use crate::FixedRule;

// Re-exports: keep the public / crate-level paths identical to the old `db.rs`
pub use eval::{evaluate_expressions, get_variables};
pub use named_rows::{NamedRows, Payload, TransactionPayload};
pub use running::Poison;
#[allow(unused_imports)]
pub(crate) use running::{seconds_since_the_epoch, RunningQueryCleanup, RunningQueryHandle};

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
pub struct DbManifest {
    pub storage_version: u64,
}

/// Whether a script is mutable or immutable.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ScriptMutability {
    /// The script is mutable.
    Mutable,
    /// The script is immutable.
    Immutable,
}

/// The database object of Cozo.
#[derive(Clone)]
pub struct Db<S> {
    pub(crate) db: S,
    temp_db: TempStorage,
    relation_store_id: Arc<AtomicU64>,
    pub(crate) queries_count: Arc<AtomicU64>,
    pub(crate) running_queries: Arc<Mutex<BTreeMap<u64, RunningQueryHandle>>>,
    pub(crate) fixed_rules: Arc<ShardedLock<BTreeMap<String, Arc<Box<dyn FixedRule>>>>>,
    pub(crate) tokenizers: Arc<TokenizerCache>,
    #[cfg(not(target_arch = "wasm32"))]
    callback_count: Arc<AtomicU32>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) event_callbacks: Arc<ShardedLock<EventCallbackRegistry>>,
    relation_locks: Arc<ShardedLock<BTreeMap<SmartString<LazyCompact>, Arc<ShardedLock<()>>>>>,
}

impl<S> Debug for Db<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Db")
    }
}

#[derive(Debug, Diagnostic, Error)]
#[error("Initialization of database failed")]
#[diagnostic(code(db::init))]
pub(crate) struct BadDbInit(#[help] pub(crate) String);

#[derive(Debug, Error, Diagnostic)]
#[error("Cannot import data into relation {0} as it is an index")]
#[diagnostic(code(tx::import_into_index))]
pub(crate) struct ImportIntoIndex(pub(crate) String);

impl<'s, S: Storage<'s>> Db<S> {
    /// Create a new database object with the given storage.
    /// You must call [`initialize`](Self::initialize) immediately after creation.
    /// Due to lifetime restrictions we are not able to call that for you automatically.
    pub fn new(storage: S) -> Result<Self> {
        let ret = Self {
            db: storage,
            temp_db: Default::default(),
            relation_store_id: Default::default(),
            queries_count: Default::default(),
            running_queries: Default::default(),
            fixed_rules: Arc::new(ShardedLock::new(DEFAULT_FIXED_RULES.clone())),
            tokenizers: Arc::new(Default::default()),
            #[cfg(not(target_arch = "wasm32"))]
            callback_count: Default::default(),
            // callback_receiver: Arc::new(receiver),
            #[cfg(not(target_arch = "wasm32"))]
            event_callbacks: Default::default(),
            relation_locks: Default::default(),
        };
        Ok(ret)
    }

    /// Must be called after creation of the database to initialize the runtime state.
    pub fn initialize(&'s self) -> Result<()> {
        self.load_last_ids()?;
        Ok(())
    }

    /// This returns the set of fixed rule implementations for this specific backend.
    pub fn get_fixed_rules(&'s self) -> BTreeMap<String, Arc<Box<dyn FixedRule>>> {
        return self.fixed_rules.read().unwrap().clone();
    }

    /// Run the CozoScript passed in. The `params` argument is a map of parameters.
    pub fn run_script(
        &'s self,
        payload: &str,
        params: BTreeMap<String, DataValue>,
        mutability: ScriptMutability,
    ) -> Result<NamedRows> {
        self.run_script_ast(
            parse_script(
                payload,
                &params,
                &self.get_fixed_rules(),
                current_validity(),
            )?,
            current_validity(),
            mutability,
        )
    }

    /// Run the CozoScript passed in. The `params` argument is a map of parameters.
    pub fn run_script_read_only(
        &'s self,
        payload: &str,
        params: BTreeMap<String, DataValue>,
    ) -> Result<NamedRows> {
        self.run_script(payload, params, ScriptMutability::Immutable)
    }

    /// Run the AST CozoScript passed in.
    pub fn run_script_ast(
        &'s self,
        payload: CozoScript,
        cur_vld: ValidityTs,
        mutability: ScriptMutability,
    ) -> Result<NamedRows> {
        let read_only = mutability == ScriptMutability::Immutable;
        match payload {
            CozoScript::Single(p) => self.execute_single(cur_vld, p, read_only),
            CozoScript::Imperative(ps) => self.execute_imperative(cur_vld, &ps, read_only),
            CozoScript::Sys(op) => self.run_sys_op(op, read_only),
        }
    }

    fn compact_relation(&'s self) -> Result<()> {
        let l = Tuple::default().encode_as_key(RelationId(0));
        let u = vec![DataValue::Bot].encode_as_key(RelationId(u64::MAX));
        self.db.range_compact(&l, &u)?;
        Ok(())
    }

    fn load_last_ids(&'s self) -> Result<()> {
        let mut tx = self.transact_write()?;
        self.relation_store_id
            .store(tx.init_storage()?.0, Ordering::Release);
        tx.commit_tx()?;
        Ok(())
    }

    pub(crate) fn transact(&'s self) -> Result<SessionTx<'_>> {
        let ret = SessionTx {
            store_tx: Box::new(self.db.transact(false)?),
            temp_store_tx: self.temp_db.transact(true)?,
            relation_store_id: self.relation_store_id.clone(),
            temp_store_id: Default::default(),
            tokenizers: self.tokenizers.clone(),
        };
        Ok(ret)
    }

    pub(crate) fn transact_write(&'s self) -> Result<SessionTx<'_>> {
        let ret = SessionTx {
            store_tx: Box::new(self.db.transact(true)?),
            temp_store_tx: self.temp_db.transact(true)?,
            relation_store_id: self.relation_store_id.clone(),
            temp_store_id: Default::default(),
            tokenizers: self.tokenizers.clone(),
        };
        Ok(ret)
    }
}