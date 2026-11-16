/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;

use itertools::Itertools;
use miette::{bail, Diagnostic, IntoDiagnostic, Result};
use pest::Parser;
use rmp_serde::Serializer;
use serde::Serialize;
use smartstring::SmartString;
use thiserror::Error;

use crate::data::relation::{ColType, ColumnDef, NullableColType, StoredRelationMetadata};
use crate::data::tuple::TupleT;
use crate::data::value::DataValue;
use crate::parse::expr::build_expr;
use crate::parse::sys::{FtsIndexConfig, HnswIndexConfig, MinHashLshConfig};
use crate::parse::{CozoScriptParser, Rule};
use crate::runtime::minhash_lsh::{HashPermutations, LshParams, Weights};
use crate::runtime::transact::SessionTx;
use crate::utils::TempCollector;

impl<'a> SessionTx<'a> {
    pub(crate) fn create_minhash_lsh_index(&mut self, config: &MinHashLshConfig) -> Result<()> {
        // Get relation handle
        let mut rel_handle = self.get_relation(&config.base_relation, true)?;

        // Check if index already exists
        if rel_handle.has_index(&config.index_name) {
            bail!(IndexAlreadyExists(
                config.index_name.to_string(),
                config.index_name.to_string()
            ));
        }

        let inv_idx_keys = rel_handle.metadata.keys.clone();
        let inv_idx_vals = vec![ColumnDef {
            name: SmartString::from("minhash"),
            typing: NullableColType {
                coltype: ColType::Bytes,
                nullable: false,
            },
            default_gen: None,
        }];

        let mut idx_keys = vec![ColumnDef {
            name: SmartString::from("hash"),
            typing: NullableColType {
                coltype: ColType::Bytes,
                nullable: false,
            },
            default_gen: None,
        }];
        for k in rel_handle.metadata.keys.iter() {
            idx_keys.push(ColumnDef {
                name: format!("src_{}", k.name).into(),
                typing: k.typing.clone(),
                default_gen: None,
            });
        }
        let idx_vals = vec![];

        let idx_handle = self.write_idx_relation(
            &config.base_relation,
            &config.index_name,
            idx_keys,
            idx_vals,
        )?;

        let inv_idx_handle = self.write_idx_relation(
            &config.base_relation,
            &format!("{}:inv", config.index_name),
            inv_idx_keys,
            inv_idx_vals,
        )?;

        // add index to relation
        let params = LshParams::find_optimal_params(
            config.target_threshold.0,
            config.n_perm,
            &Weights(
                config.false_positive_weight.0,
                config.false_negative_weight.0,
            ),
        );
        let num_perm = params.b * params.r;
        let perms = HashPermutations::new(num_perm);
        let manifest = MinHashLshIndexManifest {
            base_relation: config.base_relation.clone(),
            index_name: config.index_name.clone(),
            extractor: config.extractor.clone(),
            n_gram: config.n_gram,
            tokenizer: config.tokenizer.clone(),
            filters: config.filters.clone(),
            num_perm,
            n_bands: params.b,
            n_rows_in_band: params.r,
            threshold: config.target_threshold.0,
            perms: perms.as_bytes().to_vec(),
        };

        // populate index
        let tokenizer =
            self.tokenizers
                .get(&idx_handle.name, &manifest.tokenizer, &manifest.filters)?;
        let parsed = CozoScriptParser::parse(Rule::expr, &manifest.extractor)
            .into_diagnostic()?
            .next()
            .unwrap();
        let mut code_expr = build_expr(parsed, &Default::default())?;
        let binding_map = rel_handle.raw_binding_map();
        code_expr.fill_binding_indices(&binding_map)?;
        let extractor = code_expr.compile()?;

        let mut stack = vec![];

        let hash_perms = manifest.get_hash_perms();
        let mut existing = TempCollector::default();
        for tuple in rel_handle.scan_all(self) {
            existing.push(tuple?);
        }

        for tuple in existing.into_iter() {
            self.put_lsh_index_item(
                &tuple,
                &extractor,
                &mut stack,
                &tokenizer,
                &rel_handle,
                &idx_handle,
                &inv_idx_handle,
                &manifest,
                &hash_perms,
            )?;
        }

        rel_handle.lsh_indices.insert(
            manifest.index_name.clone(),
            (idx_handle, inv_idx_handle, manifest),
        );

        // update relation metadata
        let new_encoded =
            vec![DataValue::from(&rel_handle.name as &str)].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        rel_handle
            .serialize(&mut Serializer::new(&mut meta_val))
            .unwrap();
        self.store_tx.put(&new_encoded, &meta_val)?;

        Ok(())
    }

    pub(crate) fn create_fts_index(&mut self, config: &FtsIndexConfig) -> Result<()> {
        // Get relation handle
        let mut rel_handle = self.get_relation(&config.base_relation, true)?;

        // Check if index already exists
        if rel_handle.has_index(&config.index_name) {
            bail!(IndexAlreadyExists(
                config.index_name.to_string(),
                config.index_name.to_string()
            ));
        }

        // Build key columns definitions
        let mut idx_keys: Vec<ColumnDef> = vec![ColumnDef {
            name: SmartString::from("word"),
            typing: NullableColType {
                coltype: ColType::String,
                nullable: false,
            },
            default_gen: None,
        }];

        for k in rel_handle.metadata.keys.iter() {
            idx_keys.push(ColumnDef {
                name: format!("src_{}", k.name).into(),
                typing: k.typing.clone(),
                default_gen: None,
            });
        }

        let col_type = NullableColType {
            coltype: ColType::List {
                eltype: Box::new(NullableColType {
                    coltype: ColType::Int,
                    nullable: false,
                }),
                len: None,
            },
            nullable: false,
        };

        let non_idx_keys: Vec<ColumnDef> = vec![
            ColumnDef {
                name: SmartString::from("offset_from"),
                typing: col_type.clone(),
                default_gen: None,
            },
            ColumnDef {
                name: SmartString::from("offset_to"),
                typing: col_type.clone(),
                default_gen: None,
            },
            ColumnDef {
                name: SmartString::from("position"),
                typing: col_type,
                default_gen: None,
            },
            ColumnDef {
                name: SmartString::from("total_length"),
                typing: NullableColType {
                    coltype: ColType::Int,
                    nullable: false,
                },
                default_gen: None,
            },
        ];

        let idx_handle = self.write_idx_relation(
            &config.base_relation,
            &config.index_name,
            idx_keys,
            non_idx_keys,
        )?;

        // add index to relation
        let manifest = FtsIndexManifest {
            base_relation: config.base_relation.clone(),
            index_name: config.index_name.clone(),
            extractor: config.extractor.clone(),
            tokenizer: config.tokenizer.clone(),
            filters: config.filters.clone(),
        };

        // populate index
        let tokenizer =
            self.tokenizers
                .get(&idx_handle.name, &manifest.tokenizer, &manifest.filters)?;

        let parsed = CozoScriptParser::parse(Rule::expr, &manifest.extractor)
            .into_diagnostic()?
            .next()
            .unwrap();
        let mut code_expr = build_expr(parsed, &Default::default())?;
        let binding_map = rel_handle.raw_binding_map();
        code_expr.fill_binding_indices(&binding_map)?;
        let extractor = code_expr.compile()?;

        let mut stack = vec![];

        let mut existing = TempCollector::default();
        for tuple in rel_handle.scan_all(self) {
            existing.push(tuple?);
        }
        for tuple in existing.into_iter() {
            let key_part = &tuple[..rel_handle.metadata.keys.len()];
            if rel_handle.exists(self, key_part)? {
                self.del_fts_index_item(
                    &tuple,
                    &extractor,
                    &mut stack,
                    &tokenizer,
                    &rel_handle,
                    &idx_handle,
                )?;
            }
            self.put_fts_index_item(
                &tuple,
                &extractor,
                &mut stack,
                &tokenizer,
                &rel_handle,
                &idx_handle,
            )?;
        }

        rel_handle
            .fts_indices
            .insert(manifest.index_name.clone(), (idx_handle, manifest));

        // update relation metadata
        let new_encoded =
            vec![DataValue::from(&rel_handle.name as &str)].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        rel_handle
            .serialize(&mut Serializer::new(&mut meta_val))
            .unwrap();
        self.store_tx.put(&new_encoded, &meta_val)?;

        Ok(())
    }

    pub(crate) fn create_hnsw_index(&mut self, config: &HnswIndexConfig) -> Result<()> {
        // Get relation handle
        let mut rel_handle = self.get_relation(&config.base_relation, true)?;

        // Check if index already exists
        if rel_handle.has_index(&config.index_name) {
            bail!(IndexAlreadyExists(
                config.index_name.to_string(),
                config.index_name.to_string()
            ));
        }

        // Check that what we are indexing are really vectors
        if config.vec_fields.is_empty() {
            bail!("Cannot create HNSW index without vector fields");
        }
        let mut vec_field_indices = vec![];
        for field in config.vec_fields.iter() {
            let mut found = false;
            for (i, col) in rel_handle
                .metadata
                .keys
                .iter()
                .chain(rel_handle.metadata.non_keys.iter())
                .enumerate()
            {
                if col.name == *field {
                    let mut col_type = col.typing.coltype.clone();
                    if let ColType::List { eltype, .. } = &col_type {
                        col_type = eltype.coltype.clone();
                    }

                    if let ColType::Vec { eltype, len } = col_type {
                        if eltype != config.dtype {
                            bail!("Cannot create HNSW index with field {} of type {:?} (expected {:?})", field, eltype, config.dtype);
                        }
                        if len != config.vec_dim {
                            bail!("Cannot create HNSW index with field {} of dimension {} (expected {})", field, len, config.vec_dim);
                        }
                    } else {
                        bail!("Cannot create HNSW index with non-vector field {}", field)
                    }

                    found = true;
                    vec_field_indices.push(i);
                    break;
                }
            }
            if !found {
                bail!("Cannot create HNSW index with non-existent field {}", field);
            }
        }

        // Build key columns definitions
        let mut idx_keys: Vec<ColumnDef> = vec![ColumnDef {
            // layer -1 stores the self-loops
            name: SmartString::from("layer"),
            typing: NullableColType {
                coltype: ColType::Int,
                nullable: false,
            },
            default_gen: None,
        }];
        // for self-loops, fr and to are identical
        for prefix in ["fr", "to"] {
            for col in rel_handle.metadata.keys.iter() {
                let mut col = col.clone();
                col.name = SmartString::from(format!("{}_{}", prefix, col.name));
                idx_keys.push(col);
            }
            idx_keys.push(ColumnDef {
                name: SmartString::from(format!("{}__field", prefix)),
                typing: NullableColType {
                    coltype: ColType::Int,
                    nullable: false,
                },
                default_gen: None,
            });
            idx_keys.push(ColumnDef {
                name: SmartString::from(format!("{}__sub_idx", prefix)),
                typing: NullableColType {
                    coltype: ColType::Int,
                    nullable: false,
                },
                default_gen: None,
            });
        }

        // Build non-key columns definitions
        let non_idx_keys = vec![
            // For self-loops, stores the number of neighbours
            ColumnDef {
                name: SmartString::from("dist"),
                typing: NullableColType {
                    coltype: ColType::Float,
                    nullable: false,
                },
                default_gen: None,
            },
            // For self-loops, stores a hash of the neighbours, for conflict detection
            ColumnDef {
                name: SmartString::from("hash"),
                typing: NullableColType {
                    coltype: ColType::Bytes,
                    nullable: true,
                },
                default_gen: None,
            },
            ColumnDef {
                name: SmartString::from("ignore_link"),
                typing: NullableColType {
                    coltype: ColType::Bool,
                    nullable: false,
                },
                default_gen: None,
            },
        ];
        // create index relation
        let idx_handle = self.write_idx_relation(
            &config.base_relation,
            &config.index_name,
            idx_keys,
            non_idx_keys,
        )?;

        // add index to relation
        let manifest = HnswIndexManifest {
            base_relation: config.base_relation.clone(),
            index_name: config.index_name.clone(),
            vec_dim: config.vec_dim,
            dtype: config.dtype,
            vec_fields: vec_field_indices,
            distance: config.distance,
            ef_construction: config.ef_construction,
            m_neighbours: config.m_neighbours,
            m_max: config.m_neighbours,
            m_max0: config.m_neighbours * 2,
            level_multiplier: 1. / (config.m_neighbours as f64).ln(),
            index_filter: config.index_filter.clone(),
            extend_candidates: config.extend_candidates,
            keep_pruned_connections: config.keep_pruned_connections,
        };

        // populate index
        let mut all_tuples = TempCollector::default();
        for tuple in rel_handle.scan_all(self) {
            all_tuples.push(tuple?);
        }
        let filter = if let Some(f_code) = &manifest.index_filter {
            let parsed = CozoScriptParser::parse(Rule::expr, f_code)
                .into_diagnostic()?
                .next()
                .unwrap();
            let mut code_expr = build_expr(parsed, &Default::default())?;
            let binding_map = rel_handle.raw_binding_map();
            code_expr.fill_binding_indices(&binding_map)?;
            code_expr.compile()?
        } else {
            vec![]
        };
        let filter = if filter.is_empty() {
            None
        } else {
            Some(&filter)
        };
        let mut stack = vec![];
        for tuple in all_tuples.into_iter() {
            self.hnsw_put(
                &manifest,
                &rel_handle,
                &idx_handle,
                filter,
                &mut stack,
                &tuple,
            )?;
        }

        rel_handle
            .hnsw_indices
            .insert(config.index_name.clone(), (idx_handle, manifest));

        // update relation metadata
        let new_encoded =
            vec![DataValue::from(&config.base_relation as &str)].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        rel_handle
            .serialize(&mut Serializer::new(&mut meta_val))
            .unwrap();
        self.store_tx.put(&new_encoded, &meta_val)?;

        Ok(())
    }

    fn write_idx_relation(
        &mut self,
        base_name: &str,
        idx_name: &str,
        idx_keys: Vec<ColumnDef>,
        non_idx_keys: Vec<ColumnDef>,
    ) -> Result<RelationHandle> {
        let key_bindings = idx_keys
            .iter()
            .map(|col| Symbol::new(col.name.clone(), Default::default()))
            .collect();
        let dep_bindings = non_idx_keys
            .iter()
            .map(|col| Symbol::new(col.name.clone(), Default::default()))
            .collect();
        let idx_handle = InputRelationHandle {
            name: Symbol::new(format!("{}:{}", base_name, idx_name), Default::default()),
            metadata: StoredRelationMetadata {
                keys: idx_keys,
                non_keys: non_idx_keys,
            },
            key_bindings,
            dep_bindings,
            span: Default::default(),
        };
        let idx_handle = self.create_relation(idx_handle)?;
        Ok(idx_handle)
    }

    pub(crate) fn create_index(
        &mut self,
        rel_name: &Symbol,
        idx_name: &Symbol,
        cols: &[Symbol],
    ) -> Result<()> {
        // Get relation handle
        let mut rel_handle = self.get_relation(rel_name, true)?;

        // Check if index already exists
        if rel_handle.has_index(&idx_name.name) {
            bail!(IndexAlreadyExists(
                idx_name.name.to_string(),
                rel_name.name.to_string()
            ));
        }

        // Build column definitions
        let mut col_defs = vec![];
        'outer: for col in cols.iter() {
            for orig_col in rel_handle
                .metadata
                .keys
                .iter()
                .chain(rel_handle.metadata.non_keys.iter())
            {
                if orig_col.name == col.name {
                    col_defs.push(orig_col.clone());
                    continue 'outer;
                }
            }

            #[derive(Debug, Error, Diagnostic)]
            #[error("column {0} in index {1} for relation {2} not found")]
            #[diagnostic(code(tx::col_in_idx_not_found))]
            pub(crate) struct ColInIndexNotFound(String, String, String);

            bail!(ColInIndexNotFound(
                col.name.to_string(),
                idx_name.name.to_string(),
                rel_name.name.to_string()
            ));
        }

        'outer: for key in rel_handle.metadata.keys.iter() {
            for col in cols.iter() {
                if col.name == key.name {
                    continue 'outer;
                }
            }
            col_defs.push(key.clone());
        }

        let key_bindings = col_defs
            .iter()
            .map(|col| Symbol::new(col.name.clone(), Default::default()))
            .collect_vec();
        let idx_meta = StoredRelationMetadata {
            keys: col_defs,
            non_keys: vec![],
        };

        // create index relation
        let idx_handle = InputRelationHandle {
            name: Symbol::new(
                format!("{}:{}", rel_name.name, idx_name.name),
                Default::default(),
            ),
            metadata: idx_meta,
            key_bindings,
            dep_bindings: vec![],
            span: Default::default(),
        };

        let idx_handle = self.create_relation(idx_handle)?;

        // populate index
        let extraction_indices = idx_handle
            .metadata
            .keys
            .iter()
            .map(|col| {
                for (i, kc) in rel_handle.metadata.keys.iter().enumerate() {
                    if kc.name == col.name {
                        return i;
                    }
                }
                for (i, kc) in rel_handle.metadata.non_keys.iter().enumerate() {
                    if kc.name == col.name {
                        return i + rel_handle.metadata.keys.len();
                    }
                }
                unreachable!()
            })
            .collect_vec();

        if self.store_tx.supports_par_put() {
            for tuple in rel_handle.scan_all(self) {
                let tuple = tuple?;
                let extracted = extraction_indices
                    .iter()
                    .map(|idx| tuple[*idx].clone())
                    .collect_vec();
                let key = idx_handle.encode_key_for_store(&extracted, Default::default())?;
                self.store_tx.par_put(&key, &[])?;
            }
        } else {
            let mut existing = TempCollector::default();
            for tuple in rel_handle.scan_all(self) {
                existing.push(tuple?);
            }
            for tuple in existing.into_iter() {
                let extracted = extraction_indices
                    .iter()
                    .map(|idx| tuple[*idx].clone())
                    .collect_vec();
                let key = idx_handle.encode_key_for_store(&extracted, Default::default())?;
                self.store_tx.put(&key, &[])?;
            }
        }

        // add index to relation
        rel_handle
            .indices
            .insert(idx_name.name.clone(), (idx_handle, extraction_indices));

        // update relation metadata
        let new_encoded =
            vec![DataValue::from(&rel_name.name as &str)].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        rel_handle
            .serialize(&mut Serializer::new(&mut meta_val))
            .unwrap();
        self.store_tx.put(&new_encoded, &meta_val)?;

        Ok(())
    }

    pub(crate) fn remove_index(
        &mut self,
        rel_name: &Symbol,
        idx_name: &Symbol,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut rel = self.get_relation(rel_name, true)?;
        let is_lsh = rel.lsh_indices.contains_key(&idx_name.name);
        let is_fts = rel.fts_indices.contains_key(&idx_name.name);
        if is_lsh || is_fts {
            self.tokenizers.named_cache.write().unwrap().clear();
            self.tokenizers.hashed_cache.write().unwrap().clear();
        }
        if rel.indices.remove(&idx_name.name).is_none()
            && rel.hnsw_indices.remove(&idx_name.name).is_none()
            && rel.lsh_indices.remove(&idx_name.name).is_none()
            && rel.fts_indices.remove(&idx_name.name).is_none()
        {
            #[derive(Debug, Error, Diagnostic)]
            #[error("index {0} for relation {1} not found")]
            #[diagnostic(code(tx::idx_not_found))]
            pub(crate) struct IndexNotFound(String, String);

            bail!(IndexNotFound(idx_name.to_string(), rel_name.to_string()));
        }

        let mut to_clean =
            self.destroy_relation(&format!("{}:{}", rel_name.name, idx_name.name))?;
        if is_lsh {
            to_clean.extend(
                self.destroy_relation(&format!("{}:{}:inv", rel_name.name, idx_name.name))?,
            );
        }

        let new_encoded =
            vec![DataValue::from(&rel_name.name as &str)].encode_as_key(RelationId::SYSTEM);
        let mut meta_val = vec![];
        rel.serialize(&mut Serializer::new(&mut meta_val)).unwrap();
        self.store_tx.put(&new_encoded, &meta_val)?;

        Ok(to_clean)
    }

}
