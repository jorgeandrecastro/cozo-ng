/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::btree_map::Entry;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::Ordering;

use crossbeam::sync::ShardedLock;
#[cfg(not(target_arch = "wasm32"))]
use crossbeam::channel::{bounded, unbounded, Receiver};
use miette::{bail, Result};
use smartstring::{LazyCompact, SmartString};

use crate::fixed_rule::DEFAULT_FIXED_RULES;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::callback::{CallbackDeclaration, CallbackOp};
use crate::storage::Storage;
use crate::FixedRule;

#[cfg(not(target_arch = "wasm32"))]
use super::NamedRows;
use super::Db;

impl<'s, S: Storage<'s>> Db<S> {
    /// Register a custom fixed rule implementation.
    pub fn register_fixed_rule<R>(&self, name: String, rule_impl: R) -> Result<()>
    where
        R: FixedRule + 'static,
    {
        match self.fixed_rules.write().unwrap().entry(name) {
            Entry::Vacant(ent) => {
                ent.insert(Arc::new(Box::new(rule_impl)));
                Ok(())
            }
            Entry::Occupied(ent) => {
                bail!(
                    "A fixed rule with the name {} is already registered",
                    ent.key()
                )
            }
        }
    }

    /// Unregister a custom fixed rule implementation.
    pub fn unregister_fixed_rule(&self, name: &str) -> Result<bool> {
        if DEFAULT_FIXED_RULES.contains_key(name) {
            bail!("Cannot unregister builtin fixed rule {}", name);
        }
        Ok(self.fixed_rules.write().unwrap().remove(name).is_some())
    }

    /// Register callback channel to receive changes when the requested relation are successfully committed.
    /// The returned ID can be used to unregister the callback channel.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn register_callback(
        &self,
        relation: &str,
        capacity: Option<usize>,
    ) -> (u32, Receiver<(CallbackOp, NamedRows, NamedRows)>) {
        let (sender, receiver) = if let Some(c) = capacity {
            bounded(c)
        } else {
            unbounded()
        };
        let cb = CallbackDeclaration {
            dependent: SmartString::from(relation),
            sender,
        };

        let mut guard = self.event_callbacks.write().unwrap();
        let new_id = self.callback_count.fetch_add(1, Ordering::SeqCst);
        guard
            .1
            .entry(SmartString::from(relation))
            .or_default()
            .insert(new_id);

        guard.0.insert(new_id, cb);
        (new_id, receiver)
    }

    /// Unregister callbacks/channels to run when changes to relations are committed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn unregister_callback(&self, id: u32) -> bool {
        let mut guard = self.event_callbacks.write().unwrap();
        let ret = guard.0.remove(&id);
        if let Some(cb) = &ret {
            guard.1.get_mut(&cb.dependent).unwrap().remove(&id);

            if guard.1.get(&cb.dependent).unwrap().is_empty() {
                guard.1.remove(&cb.dependent);
            }
        }
        ret.is_some()
    }

    pub(crate) fn obtain_relation_locks<'a, T: Iterator<Item = &'a SmartString<LazyCompact>>>(
        &'s self,
        rels: T,
    ) -> Vec<Arc<ShardedLock<()>>> {
        let mut collected = vec![];
        let mut pending = vec![];
        {
            let locks = self.relation_locks.read().unwrap();
            for rel in rels {
                match locks.get(rel) {
                    None => {
                        pending.push(rel);
                    }
                    Some(lock) => collected.push(lock.clone()),
                }
            }
        }
        if !pending.is_empty() {
            let mut locks = self.relation_locks.write().unwrap();
            for rel in pending {
                let lock = locks.entry(rel.clone()).or_default().clone();
                collected.push(lock);
            }
        }
        collected
    }
}