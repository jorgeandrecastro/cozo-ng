/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/stored.rs

use std::collections::BTreeSet;

use miette::{bail, Diagnostic, Result};
use smartstring::{LazyCompact, SmartString};
use thiserror::Error;

use crate::data::program::RelationOp;
use crate::data::symb::Symbol;
use crate::data::tuple::Tuple;
use crate::data::value::{DataValue, ValidityTs};
use crate::parse::parse_script;
use crate::runtime::callback::CallbackCollector;
use crate::runtime::relation::{AccessLevel, InputRelationHandle, InsufficientAccessLevel};
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;
use crate::Db;

mod assertions;
mod extractors;
mod flags;
mod indexes;
mod remove;
mod triggers;
mod write;

#[derive(Debug, Error, Diagnostic)]
#[error("attempting to write into relation {0} of arity {1} with data of arity {2}")]
#[diagnostic(code(eval::relation_arity_mismatch))]
struct RelationArityMismatch(String, usize, usize);

#[derive(Debug, Error, Diagnostic)]
#[error("replace op in trigger is not allowed: {0}")]
#[diagnostic(code(eval::replace_in_trigger))]
struct ReplaceInTrigger(String);

#[derive(Debug, Error, Diagnostic)]
#[error("cannot replace relation {0} since it has indices")]
#[diagnostic(code(eval::replace_rel_with_indices))]
struct ReplaceRelationWithIndices(String);

impl<'a> SessionTx<'a> {
    pub(crate) fn execute_relation<'s, S: Storage<'s>>(
        &mut self,
        db: &Db<S>,
        res_iter: impl Iterator<Item = Tuple>,
        op: RelationOp,
        meta: &InputRelationHandle,
        headers: &[Symbol],
        cur_vld: ValidityTs,
        callback_targets: &BTreeSet<SmartString<LazyCompact>>,
        callback_collector: &mut CallbackCollector,
        propagate_triggers: bool,
        force_collect: &str,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut to_clear = vec![];
        let mut replaced_old_triggers = None;
        if op == RelationOp::Replace {
            if !propagate_triggers {
                bail!(ReplaceInTrigger(meta.name.to_string()))
            }
            if let Ok(old_handle) = self.get_relation(&meta.name, true) {
                if !old_handle.indices.is_empty() {
                    bail!(ReplaceRelationWithIndices(old_handle.name.to_string()))
                }
                if old_handle.access_level < AccessLevel::Normal {
                    bail!(InsufficientAccessLevel(
                        old_handle.name.to_string(),
                        "relation replacement".to_string(),
                        old_handle.access_level
                    ));
                }
                if old_handle.has_triggers() {
                    replaced_old_triggers = Some((old_handle.put_triggers, old_handle.rm_triggers))
                }
                for trigger in &old_handle.replace_triggers {
                    let program = parse_script(
                        trigger,
                        &Default::default(),
                        &db.fixed_rules.read().unwrap(),
                        cur_vld,
                    )?
                    .get_single_program()?;

                    let (_, cleanups) = db
                        .run_query(
                            self,
                            program,
                            cur_vld,
                            callback_targets,
                            callback_collector,
                            false,
                        )
                        .map_err(|err| {
                            if err.source_code().is_some() {
                                err
                            } else {
                                err.with_source_code(format!("{trigger}"))
                            }
                        })?;
                    to_clear.extend(cleanups);
                }
                let destroy_res = self.destroy_relation(&meta.name)?;
                if !meta.name.is_temp_store_name() {
                    to_clear.extend(destroy_res);
                }
            }
        }
        let mut relation_store = if op == RelationOp::Replace || op == RelationOp::Create {
            self.create_relation(meta.clone())?
        } else {
            self.get_relation(&meta.name, false)?
        };
        if let Some((old_put, old_retract)) = replaced_old_triggers {
            relation_store.put_triggers = old_put;
            relation_store.rm_triggers = old_retract;
        }
        let InputRelationHandle {
            metadata,
            key_bindings,
            dep_bindings,
            span,
            ..
        } = meta;

        match op {
            RelationOp::Rm | RelationOp::Delete => self.remove_from_relation(
                db,
                res_iter,
                headers,
                cur_vld,
                callback_targets,
                callback_collector,
                propagate_triggers,
                &mut to_clear,
                &relation_store,
                metadata,
                key_bindings,
                op == RelationOp::Delete,
                force_collect,
                *span,
            )?,
            RelationOp::Ensure => self.ensure_in_relation(
                res_iter,
                headers,
                cur_vld,
                &relation_store,
                metadata,
                key_bindings,
                *span,
            )?,
            RelationOp::EnsureNot => self.ensure_not_in_relation(
                res_iter,
                headers,
                cur_vld,
                &relation_store,
                metadata,
                key_bindings,
                *span,
            )?,
            RelationOp::Update => self.update_in_relation(
                db,
                res_iter,
                headers,
                cur_vld,
                callback_targets,
                callback_collector,
                propagate_triggers,
                &mut to_clear,
                &relation_store,
                metadata,
                key_bindings,
                force_collect,
                *span,
            )?,
            RelationOp::Create | RelationOp::Replace | RelationOp::Put | RelationOp::Insert => self
                .put_into_relation(
                    db,
                    res_iter,
                    headers,
                    cur_vld,
                    callback_targets,
                    callback_collector,
                    propagate_triggers,
                    &mut to_clear,
                    &relation_store,
                    metadata,
                    key_bindings,
                    dep_bindings,
                    op == RelationOp::Insert,
                    force_collect,
                    *span,
                )?,
        };

        Ok(to_clear)
    }
}

#[derive(Debug, Error, Diagnostic)]
#[error("Assertion failure for {key:?} of {relation}: {notice}")]
#[diagnostic(code(transact::assertion_failure))]
struct TransactAssertionFailure {
    relation: String,
    key: Vec<DataValue>,
    notice: String,
}