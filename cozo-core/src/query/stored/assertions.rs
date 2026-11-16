/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use itertools::Itertools;
use miette::{bail, Result};

use crate::data::relation::StoredRelationMetadata;
use crate::data::symb::Symbol;
use crate::data::tuple::Tuple;
use crate::data::value::{DataValue, ValidityTs};
use crate::runtime::relation::{AccessLevel, InsufficientAccessLevel, RelationHandle};
use crate::runtime::transact::SessionTx;
use crate::{SourceSpan, StoreTx};

use super::extractors::make_extractors;
use super::TransactAssertionFailure;

impl<'a> SessionTx<'a> {
    pub(super) fn ensure_not_in_relation(
        &mut self,
        res_iter: impl Iterator<Item = Tuple>,
        headers: &[Symbol],
        cur_vld: ValidityTs,
        relation_store: &RelationHandle,
        metadata: &StoredRelationMetadata,
        key_bindings: &[Symbol],
        span: SourceSpan,
    ) -> Result<()> {
        if relation_store.access_level < AccessLevel::ReadOnly {
            bail!(InsufficientAccessLevel(
                relation_store.name.to_string(),
                "row check".to_string(),
                relation_store.access_level
            ));
        }

        let key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        for tuple in res_iter {
            let extracted: Vec<DataValue> = key_extractors
                .iter()
                .map(|ex| ex.extract_data(&tuple, cur_vld))
                .try_collect()?;
            let key = relation_store.encode_key_for_store(&extracted, span)?;
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
                })
            }
        }
        Ok(())
    }

    pub(super) fn ensure_in_relation(
        &mut self,
        res_iter: impl Iterator<Item = Tuple>,
        headers: &[Symbol],
        cur_vld: ValidityTs,
        relation_store: &RelationHandle,
        metadata: &StoredRelationMetadata,
        key_bindings: &[Symbol],
        span: SourceSpan,
    ) -> Result<()> {
        if relation_store.access_level < AccessLevel::ReadOnly {
            bail!(InsufficientAccessLevel(
                relation_store.name.to_string(),
                "row check".to_string(),
                relation_store.access_level
            ));
        }

        let mut key_extractors = make_extractors(
            &relation_store.metadata.keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;

        let val_extractors = make_extractors(
            &relation_store.metadata.non_keys,
            &metadata.keys,
            key_bindings,
            headers,
        )?;
        key_extractors.extend(val_extractors);

        for tuple in res_iter {
            let extracted: Vec<DataValue> = key_extractors
                .iter()
                .map(|ex| ex.extract_data(&tuple, cur_vld))
                .try_collect()?;

            let key = relation_store.encode_key_for_store(&extracted, span)?;
            let val = relation_store.encode_val_for_store(&extracted, span)?;

            let existing = if relation_store.is_temp {
                self.temp_store_tx.get(&key, true)?
            } else {
                self.store_tx.get(&key, true)?
            };
            match existing {
                None => {
                    bail!(TransactAssertionFailure {
                        relation: relation_store.name.to_string(),
                        key: extracted,
                        notice: "key does not exist in database".to_string()
                    })
                }
                Some(v) => {
                    if &v as &[u8] != &val as &[u8] {
                        bail!(TransactAssertionFailure {
                            relation: relation_store.name.to_string(),
                            key: extracted,
                            notice: "key exists in database, but value does not match".to_string()
                        })
                    }
                }
            }
        }
        Ok(())
    }
}
