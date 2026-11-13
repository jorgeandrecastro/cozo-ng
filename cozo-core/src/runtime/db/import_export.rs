/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;

use itertools::Itertools;
use miette::{bail, miette, Diagnostic, Result};
use smartstring::SmartString;
use thiserror::Error;

use crate::data::functions::current_validity;
use crate::data::json::JsonValue;
use crate::data::relation::ColumnDef;
use crate::data::tuple::{Tuple, TupleT};
use crate::data::value::DataValue;
use crate::runtime::relation::{extend_tuple_from_v, AccessLevel, InsufficientAccessLevel};
use crate::storage::Storage;
use crate::decode_tuple_from_kv;

use super::{Db, ImportIntoIndex, NamedRows};

impl<'s, S: Storage<'s>> Db<S> {
    /// Export relations to JSON data.
    ///
    /// `relations` contains names of the stored relations to export.
    pub fn export_relations<I, T>(&'s self, relations: I) -> Result<BTreeMap<String, NamedRows>>
    where
        T: AsRef<str>,
        I: Iterator<Item = T>,
    {
        let tx = self.transact()?;
        let mut ret: BTreeMap<String, NamedRows> = BTreeMap::new();
        for rel in relations {
            let handle = tx.get_relation(rel.as_ref(), false)?;
            let size_hint = handle.metadata.keys.len() + handle.metadata.non_keys.len();

            if handle.access_level < AccessLevel::ReadOnly {
                bail!(InsufficientAccessLevel(
                    handle.name.to_string(),
                    "data export".to_string(),
                    handle.access_level
                ));
            }

            let mut cols = handle
                .metadata
                .keys
                .iter()
                .map(|col| col.name.clone())
                .collect_vec();
            cols.extend(
                handle
                    .metadata
                    .non_keys
                    .iter()
                    .map(|col| col.name.clone())
                    .collect_vec(),
            );

            let start = Tuple::default().encode_as_key(handle.id);
            let end = Tuple::default().encode_as_key(handle.id.next());

            let mut rows = vec![];
            for data in tx.store_tx.range_scan(&start, &end) {
                let (k, v) = data?;
                let tuple = decode_tuple_from_kv(&k, &v, Some(size_hint));
                rows.push(tuple);
            }
            let headers = cols.iter().map(|col| col.to_string()).collect_vec();
            ret.insert(rel.as_ref().to_string(), NamedRows::new(headers, rows));
        }
        Ok(ret)
    }

    /// Import relations. The argument `data` accepts data in the shape of
    /// what was returned by [Self::export_relations].
    /// The target stored relations must already exist in the database.
    /// Any associated indices will be updated.
    ///
    /// Note that triggers and callbacks are _not_ run for the relations, if any exists.
    /// If you need to activate triggers or callbacks, use queries with parameters.
    pub fn import_relations(&'s self, data: BTreeMap<String, NamedRows>) -> Result<()> {
        #[derive(Debug, Diagnostic, Error)]
        #[error("cannot import data for relation '{0}': {1}")]
        #[diagnostic(code(import::bad_data))]
        struct BadDataForRelation(String, JsonValue);

        let rel_names = data.keys().map(SmartString::from).collect_vec();
        let locks = self.obtain_relation_locks(rel_names.iter());
        let _guards = locks.iter().map(|l| l.read().unwrap()).collect_vec();

        let cur_vld = current_validity();

        let mut tx = self.transact_write()?;

        for (relation_op, in_data) in data {
            let is_delete;
            let relation: &str = match relation_op.strip_prefix('-') {
                None => {
                    is_delete = false;
                    &relation_op
                }
                Some(s) => {
                    is_delete = true;
                    s
                }
            };
            if relation.contains(':') {
                bail!(ImportIntoIndex(relation.to_string()))
            }
            let handle = tx.get_relation(relation, false)?;
            let has_indices = !handle.indices.is_empty();

            if handle.access_level < AccessLevel::Protected {
                bail!(InsufficientAccessLevel(
                    handle.name.to_string(),
                    "data import".to_string(),
                    handle.access_level
                ));
            }

            let header2idx: BTreeMap<_, _> = in_data
                .headers
                .iter()
                .enumerate()
                .map(|(i, k)| -> Result<(&str, usize)> { Ok((k as &str, i)) })
                .try_collect()?;

            let key_indices: Vec<_> = handle
                .metadata
                .keys
                .iter()
                .map(|col| -> Result<(usize, &ColumnDef)> {
                    let idx = header2idx.get(&col.name as &str).ok_or_else(|| {
                        miette!(
                            "required header {} not found for relation {}",
                            col.name,
                            relation
                        )
                    })?;
                    Ok((*idx, col))
                })
                .try_collect()?;

            let val_indices: Vec<_> = if is_delete {
                vec![]
            } else {
                handle
                    .metadata
                    .non_keys
                    .iter()
                    .map(|col| -> Result<(usize, &ColumnDef)> {
                        let idx = header2idx.get(&col.name as &str).ok_or_else(|| {
                            miette!(
                                "required header {} not found for relation {}",
                                col.name,
                                relation
                            )
                        })?;
                        Ok((*idx, col))
                    })
                    .try_collect()?
            };

            for row in in_data.rows {
                let keys: Vec<_> = key_indices
                    .iter()
                    .map(|(i, col)| -> Result<DataValue> {
                        let v = row
                            .get(*i)
                            .ok_or_else(|| miette!("row too short: {:?}", row))?;
                        col.typing.coerce(v.clone(), cur_vld)
                    })
                    .try_collect()?;
                let k_store = handle.encode_key_for_store(&keys, Default::default())?;
                if has_indices {
                    if let Some(existing) = tx.store_tx.get(&k_store, false)? {
                        let mut old = keys.clone();
                        extend_tuple_from_v(&mut old, &existing);
                        if is_delete || old != row {
                            for (idx_rel, extractor) in handle.indices.values() {
                                let idx_tup =
                                    extractor.iter().map(|i| old[*i].clone()).collect_vec();
                                let encoded =
                                    idx_rel.encode_key_for_store(&idx_tup, Default::default())?;
                                tx.store_tx.del(&encoded)?;
                            }
                        }
                    }
                }
                if is_delete {
                    tx.store_tx.del(&k_store)?;
                } else {
                    let vals: Vec<_> = val_indices
                        .iter()
                        .map(|(i, col)| -> Result<DataValue> {
                            let v = row
                                .get(*i)
                                .ok_or_else(|| miette!("row too short: {:?}", row))?;
                            col.typing.coerce(v.clone(), cur_vld)
                        })
                        .try_collect()?;
                    let v_store = handle.encode_val_only_for_store(&vals, Default::default())?;
                    tx.store_tx.put(&k_store, &v_store)?;
                    if has_indices {
                        let mut kv = keys;
                        kv.extend(vals);
                        for (idx_rel, extractor) in handle.indices.values() {
                            let idx_tup = extractor.iter().map(|i| kv[*i].clone()).collect_vec();
                            let encoded =
                                idx_rel.encode_key_for_store(&idx_tup, Default::default())?;
                            tx.store_tx.put(&encoded, &[])?;
                        }
                    }
                }
            }
        }
        tx.commit_tx()?;
        Ok(())
    }
}