/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/stored/write.rs

use std::collections::BTreeSet;

use itertools::Itertools;
use miette::{bail, Result};
use smartstring::{LazyCompact, SmartString};

use crate::data::relation::StoredRelationMetadata;
use crate::data::symb::Symbol;
use crate::data::tuple::{Tuple, ENCODED_KEY_MIN_LEN};
use crate::data::value::{DataValue, ValidityTs};
use crate::runtime::callback::CallbackCollector;
use crate::runtime::relation::{extend_tuple_from_v, RelationHandle};
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;
use crate::{Db, SourceSpan, StoreTx};

use super::extractors::{make_extractors, make_update_extractors};
use super::flags::WriteFlags;
use super::TransactAssertionFailure;

impl<'a> SessionTx<'a> {
    pub(super) fn put_into_relation<'s, S: Storage<'s>>(
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
        dep_bindings: &[Symbol],
        is_insert: bool,
        force_collect: &str,
        span: SourceSpan,
    ) -> Result<()> {
        let f = WriteFlags::new(
            relation_store,
            callback_targets,
            force_collect,
            propagate_triggers,
            !relation_store.put_triggers.is_empty(),
            "row insertion",
        )?;

        let mut key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let mut new_tuples: Vec<DataValue> = vec![];
        let mut old_tuples: Vec<DataValue> = vec![];

        let val_extractors = if metadata.non_keys.is_empty() {
            make_extractors(
                &relation_store.metadata.non_keys,
                &metadata.keys,
                key_bindings,
                headers,
            )?
        } else {
            make_extractors(
                &relation_store.metadata.non_keys,
                &metadata.non_keys,
                dep_bindings,
                headers,
            )?
        };
        key_extractors.extend(val_extractors);
        let arity = key_extractors.len();
        let mut stack = vec![];
        let hnsw_filters = Self::make_hnsw_filters(relation_store)?;
        let fts_lsh_processors = self.make_fts_lsh_processors(relation_store)?;
        let lsh_perms = self.make_lsh_hash_perms(relation_store);

        for tuple in res_iter {
            let mut extracted: Vec<DataValue> = Vec::with_capacity(arity);
            for ex in &key_extractors {
                extracted.push(ex.extract_data(&tuple, cur_vld)?);
            }

            let key = relation_store.encode_key_for_store(&extracted, span)?;

            if is_insert {
                let already_exists = if relation_store.is_temp {
                    self.temp_store_tx.exists(&key, true)?
                } else {
                    self.store_tx.exists(&key, true)?
                };

                if already_exists {
                    bail!(TransactAssertionFailure {
                        relation: relation_store.name.to_string(),
                        key: extracted,
                        notice: "key exists in database".to_string()
                    });
                }
            }

            let val = relation_store.encode_val_for_store(&extracted, span)?;

            if f.needs_lookup {
                // En mode insert, la clé est absente par construction : on évite un `get`.
                let existing = if is_insert {
                    None
                } else {
                    self.store_tx.get(&key, false)?
                };

                if let Some(existing) = existing {
                    let mut tup = extracted[0..relation_store.metadata.keys.len()].to_vec();
                    extend_tuple_from_v(&mut tup, &existing);
                    // CORRECTIF PROBABLE : la suppression FTS/LSH ne doit pas dépendre
                    // de `has_indices` (une relation peut n'avoir qu'un index FTS/LSH).
                    // À valider avec un test avant de merger (voir notes).
                    if extracted != tup {
                        if f.has_indices {
                            self.update_in_index(relation_store, &extracted, &tup)?;
                        }
                        self.del_in_fts(relation_store, &mut stack, &fts_lsh_processors, &tup)?;
                        self.del_in_lsh(relation_store, &tup)?;
                    }

                    if f.need_to_collect {
                        old_tuples.push(DataValue::List(tup));
                    }
                } else if f.has_indices {
                    for (idx_rel, extractor) in relation_store.indices.values() {
                        let idx_tup_new = extractor
                            .iter()
                            .map(|i| extracted[*i].clone())
                            .collect_vec();
                        let encoded_new =
                            idx_rel.encode_key_for_store(&idx_tup_new, Default::default())?;
                        self.store_tx.put(&encoded_new, &[])?;
                    }
                }

                self.update_in_hnsw(relation_store, &mut stack, &hnsw_filters, &extracted)?;
                self.put_in_fts(relation_store, &mut stack, &fts_lsh_processors, &extracted)?;
                self.put_in_lsh(
                    relation_store,
                    &mut stack,
                    &fts_lsh_processors,
                    &extracted,
                    &lsh_perms,
                )?;

                if f.need_to_collect {
                    new_tuples.push(DataValue::List(extracted));
                }
            }

            if relation_store.is_temp {
                self.temp_store_tx.put(&key, &val)?;
            } else {
                self.store_tx.put(&key, &val)?;
            }
        }

        if f.need_to_collect && !new_tuples.is_empty() {
            self.collect_mutations(
                db,
                cur_vld,
                callback_targets,
                callback_collector,
                propagate_triggers,
                to_clear,
                relation_store,
                f.is_callback_target,
                new_tuples,
                old_tuples,
            )?;
        }
        Ok(())
    }
}

impl<'a> SessionTx<'a> {
    pub(super) fn update_in_relation<'s, S: Storage<'s>>(
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
        force_collect: &str,
        span: SourceSpan,
    ) -> Result<()> {
        let f = WriteFlags::new(
            relation_store,
            callback_targets,
            force_collect,
            propagate_triggers,
            !relation_store.put_triggers.is_empty(),
            "row update",
        )?;

        let key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let mut new_tuples: Vec<DataValue> = vec![];
        let mut old_tuples: Vec<DataValue> = vec![];

        let val_extractors = make_update_extractors(
            &relation_store.metadata.non_keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let mut stack = vec![];
        let hnsw_filters = Self::make_hnsw_filters(relation_store)?;
        let fts_lsh_processors = self.make_fts_lsh_processors(relation_store)?;
        let lsh_perms = self.make_lsh_hash_perms(relation_store);
        let arity = relation_store.arity();

        for tuple in res_iter {
            let mut new_kv: Vec<DataValue> = Vec::with_capacity(arity);
            for ex in &key_extractors {
                new_kv.push(ex.extract_data(&tuple, cur_vld)?);
            }

            let key = relation_store.encode_key_for_store(&new_kv, span)?;
            let original_val_bytes = if relation_store.is_temp {
                self.temp_store_tx.get(&key, true)?
            } else {
                self.store_tx.get(&key, true)?
            };
            let mut original_val: Tuple = match original_val_bytes {
                None => {
                    bail!(TransactAssertionFailure {
                        relation: relation_store.name.to_string(),
                        key: new_kv,
                        notice: "key to update does not exist".to_string()
                    })
                }
                Some(v) => rmp_serde::from_slice(&v[ENCODED_KEY_MIN_LEN..]).map_err(|e| {
                    miette::miette!("corrupted value in relation {}: {e}", relation_store.name)
                })?,
            };

            // old_kv n'est construit que si index / trigger / callback en ont besoin.
            let old_kv: Option<Vec<DataValue>> = if f.needs_lookup {
                let mut v = Vec::with_capacity(arity);
                v.extend_from_slice(&new_kv);
                v.extend_from_slice(&original_val);
                Some(v)
            } else {
                None
            };

            for (i, extractor) in val_extractors.iter().enumerate() {
                let v = match extractor {
                    None if old_kv.is_some() => original_val[i].clone(),
                    // pas de clone (ex. gros embedding) quand old_kv n'est pas utilisé
                    None => std::mem::replace(&mut original_val[i], DataValue::Null),
                    Some(ex) => ex.extract_data(&tuple, cur_vld)?,
                };
                new_kv.push(v);
            }
            let new_val = relation_store.encode_val_for_store(&new_kv, span)?;

            if let Some(old_kv) = old_kv {
                self.del_in_fts(relation_store, &mut stack, &fts_lsh_processors, &old_kv)?;
                self.del_in_lsh(relation_store, &old_kv)?;
                self.update_in_index(relation_store, &new_kv, &old_kv)?;

                if f.need_to_collect {
                    old_tuples.push(DataValue::List(old_kv));
                }

                self.update_in_hnsw(relation_store, &mut stack, &hnsw_filters, &new_kv)?;
                self.put_in_fts(relation_store, &mut stack, &fts_lsh_processors, &new_kv)?;
                self.put_in_lsh(
                    relation_store,
                    &mut stack,
                    &fts_lsh_processors,
                    &new_kv,
                    &lsh_perms,
                )?;

                if f.need_to_collect {
                    new_tuples.push(DataValue::List(new_kv));
                }
            }

            if relation_store.is_temp {
                self.temp_store_tx.put(&key, &new_val)?;
            } else {
                self.store_tx.put(&key, &new_val)?;
            }
        }

        if f.need_to_collect && !new_tuples.is_empty() {
            self.collect_mutations(
                db,
                cur_vld,
                callback_targets,
                callback_collector,
                propagate_triggers,
                to_clear,
                relation_store,
                f.is_callback_target,
                new_tuples,
                old_tuples,
            )?;
        }
        Ok(())
    }
}