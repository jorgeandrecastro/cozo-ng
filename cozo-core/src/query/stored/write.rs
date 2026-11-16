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
use crate::data::tuple::{Tuple, ENCODED_KEY_MIN_LEN};
use crate::data::value::{DataValue, ValidityTs};
use crate::runtime::callback::CallbackCollector;
use crate::runtime::relation::{
    extend_tuple_from_v, AccessLevel, InsufficientAccessLevel, RelationHandle,
};
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;
use crate::{Db, SourceSpan, StoreTx};

use super::extractors::{make_extractors, make_update_extractors};
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
        let is_callback_target =
            callback_targets.contains(&relation_store.name) || force_collect == relation_store.name;

        if relation_store.access_level < AccessLevel::Protected {
            bail!(InsufficientAccessLevel(
                relation_store.name.to_string(),
                "row insertion".to_string(),
                relation_store.access_level
            ));
        }

        let mut key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let need_to_collect = !force_collect.is_empty()
            || (!relation_store.is_temp
                && (is_callback_target
                    || (propagate_triggers && !relation_store.put_triggers.is_empty())));
        let has_indices = !relation_store.indices.is_empty();
        let has_hnsw_indices = !relation_store.hnsw_indices.is_empty();
        let has_fts_indices = !relation_store.fts_indices.is_empty();
        let has_lsh_indices = !relation_store.lsh_indices.is_empty();
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
        let mut stack = vec![];
        let hnsw_filters = Self::make_hnsw_filters(relation_store)?;
        let fts_lsh_processors = self.make_fts_lsh_processors(relation_store)?;
        let lsh_perms = self.make_lsh_hash_perms(relation_store);

        for tuple in res_iter {
            let extracted: Vec<DataValue> = key_extractors
                .iter()
                .map(|ex| ex.extract_data(&tuple, cur_vld))
                .try_collect()?;

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

            if need_to_collect
                || has_indices
                || has_hnsw_indices
                || has_fts_indices
                || has_lsh_indices
            {
                if let Some(existing) = self.store_tx.get(&key, false)? {
                    let mut tup = extracted[0..relation_store.metadata.keys.len()].to_vec();
                    extend_tuple_from_v(&mut tup, &existing);
                    if has_indices && extracted != tup {
                        self.update_in_index(relation_store, &extracted, &tup)?;
                        self.del_in_fts(relation_store, &mut stack, &fts_lsh_processors, &tup)?;
                        self.del_in_lsh(relation_store, &tup)?;
                    }

                    if need_to_collect {
                        old_tuples.push(DataValue::List(tup));
                    }
                } else if has_indices {
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

                if need_to_collect {
                    new_tuples.push(DataValue::List(extracted));
                }
            }

            if relation_store.is_temp {
                self.temp_store_tx.put(&key, &val)?;
            } else {
                self.store_tx.put(&key, &val)?;
            }
        }

        if need_to_collect && !new_tuples.is_empty() {
            self.collect_mutations(
                db,
                cur_vld,
                callback_targets,
                callback_collector,
                propagate_triggers,
                to_clear,
                relation_store,
                is_callback_target,
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
        let is_callback_target =
            callback_targets.contains(&relation_store.name) || force_collect == relation_store.name;

        if relation_store.access_level < AccessLevel::Protected {
            bail!(InsufficientAccessLevel(
                relation_store.name.to_string(),
                "row update".to_string(),
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
                    || (propagate_triggers && !relation_store.put_triggers.is_empty())));
        let has_indices = !relation_store.indices.is_empty();
        let has_hnsw_indices = !relation_store.hnsw_indices.is_empty();
        let has_fts_indices = !relation_store.fts_indices.is_empty();
        let has_lsh_indices = !relation_store.lsh_indices.is_empty();
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

        for tuple in res_iter {
            let mut new_kv: Vec<DataValue> = key_extractors
                .iter()
                .map(|ex| ex.extract_data(&tuple, cur_vld))
                .try_collect()?;

            let key = relation_store.encode_key_for_store(&new_kv, span)?;
            let original_val_bytes = if relation_store.is_temp {
                self.temp_store_tx.get(&key, true)?
            } else {
                self.store_tx.get(&key, true)?
            };
            let original_val: Tuple = match original_val_bytes {
                None => {
                    bail!(TransactAssertionFailure {
                        relation: relation_store.name.to_string(),
                        key: new_kv,
                        notice: "key to update does not exist".to_string()
                    })
                }
                Some(v) => rmp_serde::from_slice(&v[ENCODED_KEY_MIN_LEN..]).unwrap(),
            };
            let mut old_kv = Vec::with_capacity(relation_store.arity());
            old_kv.extend_from_slice(&new_kv);
            old_kv.extend_from_slice(&original_val);
            new_kv.reserve_exact(relation_store.arity());
            for (i, extractor) in val_extractors.iter().enumerate() {
                match extractor {
                    None => {
                        new_kv.push(original_val[i].clone());
                    }
                    Some(ex) => {
                        let val = ex.extract_data(&tuple, cur_vld)?;
                        new_kv.push(val);
                    }
                }
            }
            let new_val = relation_store.encode_val_for_store(&new_kv, span)?;

            if need_to_collect
                || has_indices
                || has_hnsw_indices
                || has_fts_indices
                || has_lsh_indices
            {
                self.del_in_fts(relation_store, &mut stack, &fts_lsh_processors, &old_kv)?;
                self.del_in_lsh(relation_store, &old_kv)?;
                self.update_in_index(relation_store, &new_kv, &old_kv)?;

                if need_to_collect {
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

                if need_to_collect {
                    new_tuples.push(DataValue::List(new_kv));
                }
            }

            if relation_store.is_temp {
                self.temp_store_tx.put(&key, &new_val)?;
            } else {
                self.store_tx.put(&key, &new_val)?;
            }
        }

        if need_to_collect && !new_tuples.is_empty() {
            self.collect_mutations(
                db,
                cur_vld,
                callback_targets,
                callback_collector,
                propagate_triggers,
                to_clear,
                relation_store,
                is_callback_target,
                new_tuples,
                old_tuples,
            )?;
        }
        Ok(())
    }
}
