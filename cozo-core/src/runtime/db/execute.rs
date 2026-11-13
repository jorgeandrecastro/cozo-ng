/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
#[allow(unused_imports)]
use std::thread;
#[allow(unused_imports)]
use std::time::Duration;

use either::{Left, Right};
use itertools::Itertools;
use miette::{bail, ensure, Diagnostic, Report, Result, WrapErr};
use smartstring::{LazyCompact, SmartString};
use thiserror::Error;

use crate::data::program::{InputProgram, QueryAssertion, RelationOp, ReturnMutation};
use crate::data::tuple::Tuple;
use crate::data::value::ValidityTs;
use crate::parse::SourceSpan;
use crate::runtime::callback::CallbackCollector;
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;

use super::{
    seconds_since_the_epoch, Db, NamedRows, Poison, RunningQueryCleanup, RunningQueryHandle,
};

impl<'s, S: Storage<'s>> Db<S> {
    pub(crate) fn execute_single_program(
        &'s self,
        p: InputProgram,
        tx: &mut SessionTx<'_>,
        cleanups: &mut Vec<(Vec<u8>, Vec<u8>)>,
        cur_vld: ValidityTs,
        callback_targets: &BTreeSet<SmartString<LazyCompact>>,
        callback_collector: &mut CallbackCollector,
    ) -> Result<NamedRows> {
        #[allow(unused_variables)]
        let sleep_opt = p.out_opts.sleep;
        let (q_res, q_cleanups) =
            self.run_query(tx, p, cur_vld, callback_targets, callback_collector, true)?;
        cleanups.extend(q_cleanups);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(secs) = sleep_opt {
            thread::sleep(Duration::from_micros((secs * 1000000.) as u64));
        }
        Ok(q_res)
    }

    pub(super) fn execute_single(
        &'s self,
        cur_vld: ValidityTs,
        p: InputProgram,
        read_only: bool,
    ) -> Result<NamedRows, Report> {
        let mut callback_collector = BTreeMap::new();
        let write_lock_names = p.needs_write_lock();
        let is_write = write_lock_names.is_some();
        if read_only && is_write {
            bail!("write lock required for read-only query");
        }
        let write_lock = self.obtain_relation_locks(write_lock_names.iter());
        let _write_lock_guards = if is_write {
            Some(write_lock[0].read().unwrap())
        } else {
            None
        };
        let callback_targets = if is_write {
            self.current_callback_targets()
        } else {
            Default::default()
        };
        let mut cleanups = vec![];
        let res;
        {
            let mut tx = if is_write {
                self.transact_write()?
            } else {
                self.transact()?
            };

            res = self.execute_single_program(
                p,
                &mut tx,
                &mut cleanups,
                cur_vld,
                &callback_targets,
                &mut callback_collector,
            )?;

            for (lower, upper) in cleanups {
                tx.store_tx.del_range_from_persisted(&lower, &upper)?;
            }

            tx.commit_tx()?;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !callback_collector.is_empty() {
            self.send_callbacks(callback_collector)
        }

        Ok(res)
    }

    /// This is the entry to query evaluation
    pub(crate) fn run_query(
        &self,
        tx: &mut SessionTx<'_>,
        input_program: InputProgram,
        cur_vld: ValidityTs,
        callback_targets: &BTreeSet<SmartString<LazyCompact>>,
        callback_collector: &mut CallbackCollector,
        top_level: bool,
    ) -> Result<(NamedRows, Vec<(Vec<u8>, Vec<u8>)>)> {
        // cleanups contain stored relations that should be deleted at the end of query
        let mut clean_ups = vec![];

        // Some checks in case the query specifies mutation
        if let Some((meta, op, _)) = &input_program.out_opts.store_relation {
            if *op == RelationOp::Create {
                #[derive(Debug, Error, Diagnostic)]
                #[error("Stored relation {0} conflicts with an existing one")]
                #[diagnostic(code(eval::stored_relation_conflict))]
                struct StoreRelationConflict(String);

                ensure!(
                    !tx.relation_exists(&meta.name)?,
                    StoreRelationConflict(meta.name.to_string())
                )
            } else if *op != RelationOp::Replace {
                #[derive(Debug, Error, Diagnostic)]
                #[error("Stored relation {0} not found")]
                #[diagnostic(code(eval::stored_relation_not_found))]
                struct StoreRelationNotFoundError(String);

                let existing = tx.get_relation(&meta.name, false)?;

                ensure!(
                    tx.relation_exists(&meta.name)?,
                    StoreRelationNotFoundError(meta.name.to_string())
                );

                existing.ensure_compatible(
                    meta,
                    *op == RelationOp::Rm || *op == RelationOp::Delete || *op == RelationOp::Update,
                )?;
            }
        };

        // query compilation
        let entry_head_or_default = input_program.get_entry_out_head_or_default()?;
        let (normalized_program, out_opts) = input_program.into_normalized_program(tx)?;
        let (stratified_program, store_lifetimes) = normalized_program.into_stratified_program()?;
        let program = stratified_program.magic_sets_rewrite(tx)?;
        let compiled = tx.stratified_magic_compile(program)?;

        // poison is used to terminate queries early
        let poison = Poison::default();
        if let Some(secs) = out_opts.timeout {
            poison.set_timeout(secs)?;
        }
        // give the query an ID and store it so that it can be queried and cancelled
        let id = self.queries_count.fetch_add(1, Ordering::AcqRel);

        // time the query
        let since_the_epoch = seconds_since_the_epoch()?;

        let handle = RunningQueryHandle {
            started_at: since_the_epoch,
            poison: poison.clone(),
        };
        self.running_queries.lock().unwrap().insert(id, handle);

        // RAII cleanups of running query handle
        let _guard = RunningQueryCleanup {
            id,
            running_queries: self.running_queries.clone(),
        };

        let total_num_to_take = if out_opts.sorters.is_empty() {
            out_opts.num_to_take()
        } else {
            None
        };

        let num_to_skip = if out_opts.sorters.is_empty() {
            out_opts.offset
        } else {
            None
        };

        // the real evaluation
        let (result_store, early_return) = tx.stratified_magic_evaluate(
            &compiled,
            store_lifetimes,
            total_num_to_take,
            num_to_skip,
            poison,
        )?;

        // deal with assertions
        if let Some(assertion) = &out_opts.assertion {
            match assertion {
                QueryAssertion::AssertNone(span) => {
                    if let Some(tuple) = result_store.all_iter().next() {
                        #[derive(Debug, Error, Diagnostic)]
                        #[error(
                            "The query is asserted to return no result, but a tuple {0:?} is found"
                        )]
                        #[diagnostic(code(eval::assert_none_failure))]
                        struct AssertNoneFailure(Tuple, #[label] SourceSpan);
                        bail!(AssertNoneFailure(tuple.into_tuple(), *span))
                    }
                }
                QueryAssertion::AssertSome(span) => {
                    if result_store.all_iter().next().is_none() {
                        #[derive(Debug, Error, Diagnostic)]
                        #[error("The query is asserted to return some results, but returned none")]
                        #[diagnostic(code(eval::assert_some_failure))]
                        struct AssertSomeFailure(#[label] SourceSpan);
                        bail!(AssertSomeFailure(*span))
                    }
                }
            }
        }

        if !out_opts.sorters.is_empty() {
            // sort outputs if required
            let sorted_result =
                tx.sort_and_collect(result_store, &out_opts.sorters, &entry_head_or_default)?;
            let sorted_iter = if let Some(offset) = out_opts.offset {
                Left(sorted_result.into_iter().skip(offset))
            } else {
                Right(sorted_result.into_iter())
            };
            let sorted_iter = if let Some(limit) = out_opts.limit {
                Left(sorted_iter.take(limit))
            } else {
                Right(sorted_iter)
            };
            if let Some((meta, relation_op, returning)) = &out_opts.store_relation {
                let to_clear = tx
                    .execute_relation(
                        self,
                        sorted_iter,
                        *relation_op,
                        meta,
                        &entry_head_or_default,
                        cur_vld,
                        callback_targets,
                        callback_collector,
                        top_level,
                        if *returning == ReturnMutation::Returning {
                            &meta.name.name
                        } else {
                            ""
                        },
                    )
                    .wrap_err_with(|| format!("when executing against relation '{}'", meta.name))?;
                clean_ups.extend(to_clear);
                let returned_rows =
                    tx.get_returning_rows(callback_collector, &meta.name, returning)?;
                Ok((returned_rows, clean_ups))
            } else {
                // not sorting outputs
                let rows: Vec<Tuple> = sorted_iter.collect_vec();
                Ok((
                    NamedRows::new(
                        entry_head_or_default
                            .iter()
                            .map(|s| s.to_string())
                            .collect_vec(),
                        rows,
                    ),
                    clean_ups,
                ))
            }
        } else {
            let scan = if early_return {
                Right(Left(
                    result_store.early_returned_iter().map(|t| t.into_tuple()),
                ))
            } else if out_opts.limit.is_some() || out_opts.offset.is_some() {
                let limit = out_opts.limit.unwrap_or(usize::MAX);
                let offset = out_opts.offset.unwrap_or(0);
                Right(Right(
                    result_store
                        .all_iter()
                        .skip(offset)
                        .take(limit)
                        .map(|t| t.into_tuple()),
                ))
            } else {
                Left(result_store.all_iter().map(|t| t.into_tuple()))
            };

            if let Some((meta, relation_op, returning)) = &out_opts.store_relation {
                let to_clear = tx
                    .execute_relation(
                        self,
                        scan,
                        *relation_op,
                        meta,
                        &entry_head_or_default,
                        cur_vld,
                        callback_targets,
                        callback_collector,
                        top_level,
                        if *returning == ReturnMutation::Returning {
                            &meta.name.name
                        } else {
                            ""
                        },
                    )
                    .wrap_err_with(|| format!("when executing against relation '{}'", meta.name))?;
                clean_ups.extend(to_clear);
                let returned_rows =
                    tx.get_returning_rows(callback_collector, &meta.name, returning)?;

                Ok((returned_rows, clean_ups))
            } else {
                let rows: Vec<Tuple> = scan.collect_vec();

                Ok((
                    NamedRows::new(
                        entry_head_or_default
                            .iter()
                            .map(|s| s.to_string())
                            .collect_vec(),
                        rows,
                    ),
                    clean_ups,
                ))
            }
        }
    }
}