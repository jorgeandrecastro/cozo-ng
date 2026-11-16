/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;

use std::sync::atomic::Ordering;

use miette::{bail, Diagnostic, Result};
use rmp_serde::Serializer;
use serde::Serialize;
use smartstring::SmartString;
use thiserror::Error;

use crate::data::tuple::TupleT;
use crate::runtime::transact::SessionTx;

impl<'a> SessionTx<'a> {
    pub(crate) fn relation_exists(&self, name: &str) -> Result<bool> {
        let key = DataValue::from(name);
        let encoded = vec![key].encode_as_key(RelationId::SYSTEM);
        if name.starts_with('_') {
            self.temp_store_tx.exists(&encoded, false)
        } else {
            self.store_tx.exists(&encoded, false)
        }
    }
    pub(crate) fn set_relation_triggers(
        &mut self,
        name: &Symbol,
        puts: &[String],
        rms: &[String],
        replaces: &[String],
    ) -> Result<()> {
        if name.name.starts_with('_') {
            bail!("Cannot set triggers for temp store")
        }
        let mut original = self.get_relation(name, true)?;
        if original.access_level < AccessLevel::Protected {
            bail!(InsufficientAccessLevel(
                original.name.to_string(),
                "set triggers".to_string(),
                original.access_level
            ))
        }
        original.put_triggers = puts.to_vec();
        original.rm_triggers = rms.to_vec();
        original.replace_triggers = replaces.to_vec();

        let name_key =
            vec![DataValue::Str(original.name.clone())].encode_as_key(RelationId::SYSTEM);

        let mut meta_val = vec![];
        original
            .serialize(&mut Serializer::new(&mut meta_val).with_struct_map())
            .unwrap();
        self.store_tx.put(&name_key, &meta_val)?;

        Ok(())
    }
    pub(crate) fn create_relation(
        &mut self,
        input_meta: InputRelationHandle,
    ) -> Result<RelationHandle> {
        let key = DataValue::Str(input_meta.name.name.clone());
        let encoded = vec![key].encode_as_key(RelationId::SYSTEM);

        let is_temp = input_meta.name.is_temp_store_name();

        if is_temp {
            if self.store_tx.exists(&encoded, true)? {
                bail!(RelNameConflictError(input_meta.name.to_string()))
            };
        } else if self.temp_store_tx.exists(&encoded, true)? {
            bail!(RelNameConflictError(input_meta.name.to_string()))
        }

        let metadata = input_meta.metadata.clone();
        let last_id = if is_temp {
            self.temp_store_id.fetch_add(1, Ordering::Relaxed) as u64
        } else {
            self.relation_store_id.fetch_add(1, Ordering::SeqCst)
        };
        let meta = RelationHandle {
            name: input_meta.name.name,
            id: RelationId::new(last_id + 1),
            metadata,
            put_triggers: vec![],
            rm_triggers: vec![],
            replace_triggers: vec![],
            access_level: AccessLevel::Normal,
            is_temp,
            indices: Default::default(),
            hnsw_indices: Default::default(),
            fts_indices: Default::default(),
            lsh_indices: Default::default(),
            description: Default::default(),
        };

        let name_key = vec![DataValue::Str(meta.name.clone())].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        meta.serialize(&mut Serializer::new(&mut meta_val).with_struct_map())
            .unwrap();
        let tuple = vec![DataValue::Null];
        let t_encoded = tuple.encode_as_key(RelationId::SYSTEM);

        if is_temp {
            self.temp_store_tx.put(&encoded, &meta.id.raw_encode())?;
            self.temp_store_tx.put(&name_key, &meta_val)?;
            self.temp_store_tx.put(&t_encoded, &meta.id.raw_encode())?;
        } else {
            self.store_tx.put(&encoded, &meta.id.raw_encode())?;
            self.store_tx.put(&name_key, &meta_val)?;
            self.store_tx.put(&t_encoded, &meta.id.raw_encode())?;
        }

        Ok(meta)
    }
    pub(crate) fn get_relation(&self, name: &str, lock: bool) -> Result<RelationHandle> {
        #[derive(Error, Diagnostic, Debug)]
        #[error("Cannot find requested stored relation '{0}'")]
        #[diagnostic(code(query::relation_not_found))]
        struct StoredRelationNotFoundError(String);

        let key = DataValue::from(name);
        let encoded = vec![key].encode_as_key(RelationId::SYSTEM);

        let found = if name.starts_with('_') {
            self.temp_store_tx
                .get(&encoded, lock)?
                .ok_or_else(|| StoredRelationNotFoundError(name.to_string()))?
        } else {
            self.store_tx
                .get(&encoded, lock)?
                .ok_or_else(|| StoredRelationNotFoundError(name.to_string()))?
        };
        let metadata = RelationHandle::decode(&found)?;
        Ok(metadata)
    }
    pub(crate) fn describe_relation(&mut self, name: &str, description: &str) -> Result<()> {
        let mut meta = self.get_relation(name, true)?;

        meta.description = SmartString::from(description);
        let name_key = vec![DataValue::Str(meta.name.clone())].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        meta.serialize(&mut Serializer::new(&mut meta_val).with_struct_map())
            .unwrap();
        if meta.is_temp {
            self.temp_store_tx.put(&name_key, &meta_val)?;
        } else {
            self.store_tx.put(&name_key, &meta_val)?;
        }

        Ok(())
    }
    pub(crate) fn destroy_relation(&mut self, name: &str) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let is_temp = name.starts_with('_');
        let mut to_clean = vec![];

        // if name.starts_with('_') {
        //     bail!("Cannot destroy temp relation");
        // }
        let store = self.get_relation(name, true)?;
        if !store.has_no_index() {
            bail!(
                "Cannot remove stored relation `{}` with indices attached.",
                name
            );
        }
        if store.access_level < AccessLevel::Normal {
            bail!(InsufficientAccessLevel(
                store.name.to_string(),
                "relation removal".to_string(),
                store.access_level
            ))
        }

        for k in store.indices.keys() {
            let more_to_clean = self.destroy_relation(&format!("{name}:{k}"))?;
            to_clean.extend(more_to_clean);
        }

        for k in store.hnsw_indices.keys() {
            let more_to_clean = self.destroy_relation(&format!("{name}:{k}"))?;
            to_clean.extend(more_to_clean);
        }

        let key = DataValue::from(name);
        let encoded = vec![key].encode_as_key(RelationId::SYSTEM);
        if is_temp {
            self.temp_store_tx.del(&encoded)?;
        } else {
            self.store_tx.del(&encoded)?;
        }
        let lower_bound = Tuple::default().encode_as_key(store.id);
        let upper_bound = Tuple::default().encode_as_key(store.id.next());
        to_clean.push((lower_bound, upper_bound));
        Ok(to_clean)
    }
    pub(crate) fn set_access_level(&mut self, rel: &Symbol, level: AccessLevel) -> Result<()> {
        let mut meta = self.get_relation(rel, true)?;
        meta.access_level = level;

        let name_key = vec![DataValue::Str(meta.name.clone())].encode_as_key(RelationId::SYSTEM);

        let mut meta_val = vec![];
        meta.serialize(&mut Serializer::new(&mut meta_val).with_struct_map())
            .unwrap();
        self.store_tx.put(&name_key, &meta_val)?;

        Ok(())
    }

    pub(crate) fn rename_relation(&mut self, old: &Symbol, new: &Symbol) -> Result<()> {
        if old.name.starts_with('_') || new.name.starts_with('_') {
            bail!("Bad name given");
        }
        let new_key = DataValue::Str(new.name.clone());
        let new_encoded = vec![new_key].encode_as_key(RelationId::SYSTEM);

        if self.store_tx.exists(&new_encoded, true)? {
            bail!(RelNameConflictError(new.name.to_string()))
        };

        let old_key = DataValue::Str(old.name.clone());
        let old_encoded = vec![old_key].encode_as_key(RelationId::SYSTEM);

        let mut rel = self.get_relation(old, true)?;
        if rel.access_level < AccessLevel::Normal {
            bail!(InsufficientAccessLevel(
                rel.name.to_string(),
                "renaming relation".to_string(),
                rel.access_level
            ));
        }
        rel.name = new.name.clone();

        let mut meta_val = vec![];
        rel.serialize(&mut Serializer::new(&mut meta_val)).unwrap();
        self.store_tx.del(&old_encoded)?;
        self.store_tx.put(&new_encoded, &meta_val)?;

        Ok(())
    }
    pub(crate) fn rename_temp_relation(&mut self, old: Symbol, new: Symbol) -> Result<()> {
        let new_key = DataValue::Str(new.name.clone());
        let new_encoded = vec![new_key].encode_as_key(RelationId::SYSTEM);

        if self.temp_store_tx.exists(&new_encoded, true)? {
            bail!(RelNameConflictError(new.name.to_string()))
        };

        let old_key = DataValue::Str(old.name.clone());
        let old_encoded = vec![old_key].encode_as_key(RelationId::SYSTEM);

        let mut rel = self.get_relation(&old, true)?;
        rel.name = new.name;

        let mut meta_val = vec![];
        rel.serialize(&mut Serializer::new(&mut meta_val)).unwrap();
        self.temp_store_tx.del(&old_encoded)?;
        self.temp_store_tx.put(&new_encoded, &meta_val)?;

        Ok(())
    }
}
