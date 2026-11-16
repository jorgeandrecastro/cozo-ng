/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeSet;

use itertools::Itertools;
use miette::{bail, Result};
use smartstring::{LazyCompact, SmartString};

use crate::data::relation::StoredRelationMetadata;
use crate::data::symb::Symbol;
use crate::data::tuple::Tuple;
use crate::data::value::{DataValue, ValidityTs};
use crate::parse::parse_script;
use crate::runtime::callback::{CallbackCollector, CallbackOp};
use crate::runtime::relation::{
    extend_tuple_from_v, AccessLevel, InsufficientAccessLevel, RelationHandle,
};
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;
use crate::{Db, NamedRows, SourceSpan, StoreTx};

use super::extractors::make_extractors;
use super::triggers::make_const_rule;
use super::TransactAssertionFailure;

impl<'a> SessionTx<'a> {
    pub(super) fn remove_from_relation<'s, S: Storage<'s>>(
        &mut self,
        db: &Db<S>,
        res_iter: impl Iterator<Item = Tuple>,
        headers: &[Symbol],
        cur_vld: ValidityTs,
        callback_targets: &BTreeSet<SmartString<LazyCompact>>,
        callback_collector: &mut CallbackCollector,
        propagate_triggers: bool,
        to_clear: &mut Vec<(Vec<u8>, Vec<u8>)>,
        relation_store: &RelationHandle,
        metadata: &StoredRelationMetadata,
        key_bindings: &[Symbol],
        check_exists: bool,
        force_collect: &str,
        span: SourceSpan,
    ) -> Result<()> {
        let is_callback_target =
            callback_targets.contains(&relation_store.name) || force_collect == relation_store.name;

        if relation_store.access_level < AccessLevel::Protected {
            bail!(InsufficientAccessLevel(
                relation_store.name.to_string(),
                "row removal".to_string(),
                relation_store.access_level
            ));
        }
        let key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let need_to_collect = !force_collect.is_empty()
            || (!relation_store.is_temp
                && (is_callback_target
                    || (propagate_triggers && !relation_store.rm_triggers.is_empty())));
        let has_indices = !relation_store.indices.is_empty();
        let has_hnsw_indices = !relation_store.hnsw_indices.is_empty();
        let has_fts_indices = !relation_store.fts_indices.is_empty();
        let has_lsh_indices = !relation_store.lsh_indices.is_empty();
        let fts_processors = self.make_fts_lsh_processors(relation_store)?;
        let mut new_tuples: Vec<DataValue> = vec![];
        let mut old_tuples: Vec<DataValue> = vec![];
        let mut stack = vec![];

        for tuple in res_iter {
            let extracted: Vec<DataValue> = key_extractors
                .iter()
                .map(|ex| ex.extract_data(&tuple, cur_vld))
                .try_collect()?;
            let key = relation_store.encode_key_for_store(&extracted, span)?;
            if check_exists {
                let exists = if relation_store.is_temp {
                    self.temp_store_tx.exists(&key, false)?
                } else {
                    self.store_tx.exists(&key, false)?
                };
                if !exists {
                    bail!(TransactAssertionFailure {
                        relation: relation_store.name.to_string(),
                        key: extracted,
                        notice: "key does not exists in database".to_string()
                    });
                }
            }
            if need_to_collect
                || has_indices
                || has_hnsw_indices
                || has_fts_indices
                || has_lsh_indices
            {
                if let Some(existing) = self.store_tx.get(&key, false)? {
                    let mut tup = extracted.clone();
                    extend_tuple_from_v(&mut tup, &existing);
                    self.del_in_fts(relation_store, &mut stack, &fts_processors, &tup)?;
                    self.del_in_lsh(relation_store, &tup)?;
                    if has_indices {
                        for (idx_rel, extractor) in relation_store.indices.values() {
                            let idx_tup = extractor.iter().map(|i| tup[*i].clone()).collect_vec();
                            let encoded =
                                idx_rel.encode_key_for_store(&idx_tup, Default::default())?;
                            self.store_tx.del(&encoded)?;
                        }
                    }
                    if has_hnsw_indices {
                        for (idx_handle, _) in relation_store.hnsw_indices.values() {
                            self.hnsw_remove(relation_store, idx_handle, &extracted)?;
                        }
                    }
                    if need_to_collect {
                        old_tuples.push(DataValue::List(tup));
                    }
                }
                if need_to_collect {
                    new_tuples.push(DataValue::List(extracted.clone()));
                }
            }
            if relation_store.is_temp {
                self.temp_store_tx.del(&key)?;
            } else {
                self.store_tx.del(&key)?;
            }
        }

        // triggers and callbacks
        if need_to_collect && !new_tuples.is_empty() {
            let k_bindings = relation_store
                .metadata
                .keys
                .iter()
                .map(|k| Symbol::new(k.name.clone(), Default::default()))
                .collect_vec();

            let v_bindings = relation_store
                .metadata
                .non_keys
                .iter()
                .map(|k| Symbol::new(k.name.clone(), Default::default()));
            let mut kv_bindings = k_bindings.clone();
            kv_bindings.extend(v_bindings);
            let kv_bindings = kv_bindings;

            if propagate_triggers {
                for trigger in &relation_store.rm_triggers {
                    let mut program = parse_script(
                        trigger,
                        &Default::default(),
                        &db.fixed_rules.read().unwrap(),
                        cur_vld,
                    )?
                    .get_single_program()?;

                    make_const_rule(&mut program, "_new", k_bindings.clone(), new_tuples.clone());

                    make_const_rule(
                        &mut program,
                        "_old",
                        kv_bindings.clone(),
                        old_tuples.clone(),
                    );

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
                                err.with_source_code(format!("{trigger} "))
                            }
                        })?;
                    to_clear.extend(cleanups);
                }
            }

            if is_callback_target {
                let target_collector = callback_collector
                    .entry(relation_store.name.clone())
                    .or_default();
                target_collector.push((
                    CallbackOp::Rm,
                    NamedRows::new(
                        k_bindings
                            .into_iter()
                            .map(|k| k.name.to_string())
                            .collect_vec(),
                        new_tuples
                            .into_iter()
                            .map(|v| match v {
                                DataValue::List(l) => l,
                                _ => unreachable!(),
                            })
                            .collect_vec(),
                    ),
                    NamedRows::new(
                        kv_bindings
                            .into_iter()
                            .map(|k| k.name.to_string())
                            .collect_vec(),
                        old_tuples
                            .into_iter()
                            .map(|v| match v {
                                DataValue::List(l) => l,
                                _ => unreachable!(),
                            })
                            .collect_vec(),
                    ),
                ))
            }
        }
        Ok(())
    }
}
