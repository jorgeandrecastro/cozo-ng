/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/stored/remove.rs

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
use crate::runtime::relation::{extend_tuple_from_v, RelationHandle};
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;
use crate::{Db, NamedRows, SourceSpan, StoreTx};

use super::extractors::make_extractors;
use super::flags::WriteFlags;
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
        let f = WriteFlags::new(
            relation_store,
            callback_targets,
            force_collect,
            propagate_triggers,
            !relation_store.rm_triggers.is_empty(),
            "row removal",
        )?;

        let key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let fts_processors = self.make_fts_lsh_processors(relation_store)?;
        let mut new_tuples: Vec<Tuple> = vec![];
        let mut old_tuples: Vec<Tuple> = vec![];
        let mut stack = vec![];
        let n_keys = key_extractors.len();

        for tuple in res_iter {
            let mut extracted: Vec<DataValue> = Vec::with_capacity(n_keys);
            for ex in &key_extractors {
                extracted.push(ex.extract_data(&tuple, cur_vld)?);
            }
            let key = relation_store.encode_key_for_store(&extracted, span)?;

            // Une seule lecture quand on a besoin de la valeur existante.
            let existing = if f.needs_lookup {
                if relation_store.is_temp {
                    self.temp_store_tx.get(&key, false)?
                } else {
                    self.store_tx.get(&key, false)?
                }
            } else {
                None
            };

            if check_exists {
                let exists = if f.needs_lookup {
                    existing.is_some()
                } else if relation_store.is_temp {
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

            if f.needs_lookup {
                if let Some(existing) = existing {
                    let mut tup = extracted.clone();
                    extend_tuple_from_v(&mut tup, &existing);
                    self.del_in_fts(relation_store, &mut stack, &fts_processors, &tup)?;
                    self.del_in_lsh(relation_store, &tup)?;
                    if f.has_indices {
                        for (idx_rel, extractor) in relation_store.indices.values() {
                            let idx_tup = extractor.iter().map(|i| tup[*i].clone()).collect_vec();
                            let encoded =
                                idx_rel.encode_key_for_store(&idx_tup, Default::default())?;
                            self.store_tx.del(&encoded)?;
                        }
                    }
                    if f.has_hnsw {
                        for (idx_handle, _) in relation_store.hnsw_indices.values() {
                            self.hnsw_remove(relation_store, idx_handle, &extracted)?;
                        }
                    }
                    if f.need_to_collect {
                        old_tuples.push(tup);
                    }
                }
                if f.need_to_collect {
                    // move : plus de clone ici
                    new_tuples.push(extracted);
                }
            }

            if relation_store.is_temp {
                self.temp_store_tx.del(&key)?;
            } else {
                self.store_tx.del(&key)?;
            }
        }

        // triggers et callbacks
        if f.need_to_collect && !new_tuples.is_empty() {
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

            if propagate_triggers && !relation_store.rm_triggers.is_empty() {
                // Conversion faite une seule fois, pas une fois par trigger.
                let new_values: Vec<DataValue> = new_tuples
                    .iter()
                    .cloned()
                    .map(DataValue::List)
                    .collect();
                let old_values: Vec<DataValue> = old_tuples
                    .iter()
                    .cloned()
                    .map(DataValue::List)
                    .collect();

                for trigger in &relation_store.rm_triggers {
                    let mut program = parse_script(
                        trigger,
                        &Default::default(),
                        &db.fixed_rules.read().unwrap(),
                        cur_vld,
                    )?
                    .get_single_program()?;

                    make_const_rule(&mut program, "_new", k_bindings.clone(), new_values.clone());
                    make_const_rule(&mut program, "_old", kv_bindings.clone(), old_values.clone());

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

            if f.is_callback_target {
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
                        new_tuples,
                    ),
                    NamedRows::new(
                        kv_bindings
                            .into_iter()
                            .map(|k| k.name.to_string())
                            .collect_vec(),
                        old_tuples,
                    ),
                ))
            }
        }
        Ok(())
    }
}