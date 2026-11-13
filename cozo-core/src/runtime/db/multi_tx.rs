/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::iter;

use crossbeam::channel::{Receiver, Sender};
use miette::Result;

use crate::data::functions::current_validity;
use crate::parse::parse_script;
use crate::storage::Storage;

use super::{Db, NamedRows, TransactionPayload};

impl<'s, S: Storage<'s>> Db<S> {
    /// Run a multi-transaction. A command should be sent to `payloads`, and the result should be
    /// retrieved from `results`. A transaction ends when it receives a `Commit` or `Abort`,
    /// or when a query is not successful. After a transaction ends, sending / receiving from
    /// the channels will fail.
    ///
    /// Write transactions _may_ block other reads, but we guarantee that this does not happen
    /// for the RocksDB backend.
    pub fn run_multi_transaction(
        &'s self,
        is_write: bool,
        payloads: Receiver<TransactionPayload>,
        results: Sender<Result<NamedRows>>,
    ) {
        let tx = if is_write {
            self.transact_write()
        } else {
            self.transact()
        };
        let mut cleanups: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let mut tx = match tx {
            Ok(tx) => tx,
            Err(err) => {
                let _ = results.send(Err(err));
                return;
            }
        };

        let ts = current_validity();
        let callback_targets = self.current_callback_targets();
        let mut callback_collector = BTreeMap::new();
        let mut write_locks = BTreeMap::new();

        for payload in payloads {
            match payload {
                TransactionPayload::Commit => {
                    for (lower, upper) in cleanups {
                        if let Err(err) = tx.store_tx.del_range_from_persisted(&lower, &upper) {
                            eprintln!("{err:?}")
                        }
                    }

                    let _ = results.send(tx.commit_tx().map(|_| NamedRows::default()));
                    #[cfg(not(target_arch = "wasm32"))]
                    if !callback_collector.is_empty() {
                        self.send_callbacks(callback_collector)
                    }

                    break;
                }
                TransactionPayload::Abort => {
                    let _ = results.send(Ok(NamedRows::default()));
                    break;
                }
                TransactionPayload::Query((script, params)) => {
                    let p =
                        match parse_script(&script, &params, &self.fixed_rules.read().unwrap(), ts)
                        {
                            Ok(p) => p,
                            Err(err) => {
                                if results.send(Err(err)).is_err() {
                                    break;
                                } else {
                                    continue;
                                }
                            }
                        };

                    let p = match p.get_single_program() {
                        Ok(p) => p,
                        Err(err) => {
                            if results.send(Err(err)).is_err() {
                                break;
                            } else {
                                continue;
                            }
                        }
                    };
                    if let Some(write_lock_name) = p.needs_write_lock() {
                        match write_locks.entry(write_lock_name) {
                            Entry::Vacant(e) => {
                                let lock = self
                                    .obtain_relation_locks(iter::once(e.key()))
                                    .pop()
                                    .unwrap();
                                e.insert(lock);
                            }
                            Entry::Occupied(_) => {}
                        }
                    }

                    let res = self.execute_single_program(
                        p,
                        &mut tx,
                        &mut cleanups,
                        ts,
                        &callback_targets,
                        &mut callback_collector,
                    );
                    if results.send(res).is_err() {
                        break;
                    }
                }
            }
        }
    }
}