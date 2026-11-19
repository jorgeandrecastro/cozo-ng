/*
 * Copyright 2023, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/runtime/hnsw/operations.rs

use super::*;

impl<'a> SessionTx<'a> {
    pub(crate) fn hnsw_remove(
        &mut self,
        orig_table: &RelationHandle,
        idx_table: &RelationHandle,
        tuple: &[DataValue],
    ) -> Result<()> {
        let mut prefix = vec![DataValue::from(0)];
        prefix.extend_from_slice(&tuple[0..orig_table.metadata.keys.len()]);
        let candidates: FxHashSet<_> = idx_table
            .scan_prefix(self, &prefix)
            .filter_map(|t| match t {
                Ok(t) => Some({
                    (
                        t[1..orig_table.metadata.keys.len() + 1].to_vec(),
                        t[orig_table.metadata.keys.len() + 1].get_int().unwrap() as usize,
                        t[orig_table.metadata.keys.len() + 2].get_int().unwrap() as i32,
                    )
                }),
                Err(_) => None,
            })
            .collect();
        for (tuple_key, idx, subidx) in candidates {
            self.hnsw_remove_vec(&tuple_key, idx, subidx, orig_table, idx_table)?;
        }
        Ok(())
    }
    pub(super) fn hnsw_remove_vec(
        &mut self,
        tuple_key: &[DataValue],
        idx: usize,
        subidx: i32,
        orig_table: &RelationHandle,
        idx_table: &RelationHandle,
    ) -> Result<()> {
        let compound_key = (tuple_key.to_vec(), idx, subidx);
        // Go down the layers and remove all the links
        let mut encountered_singletons = false;
        for neg_layer in 0i64.. {
            let layer = -neg_layer;
            let mut self_key = vec![DataValue::from(layer)];
            for _ in 0..2 {
                self_key.extend_from_slice(tuple_key);
                self_key.push(DataValue::from(idx as i64));
                self_key.push(DataValue::from(subidx as i64));
            }
            let self_key_bytes = idx_table.encode_key_for_store(&self_key, Default::default())?;
            if self.store_tx.exists(&self_key_bytes, false)? {
                self.store_tx.del(&self_key_bytes)?;
            } else {
                break;
            }

            let neigbours = self
                .hnsw_get_neighbours(&compound_key, layer, idx_table, true)?
                .collect_vec();
            encountered_singletons |= neigbours.is_empty();
            for (neighbour_key, _) in neigbours {
                // REMARK: this still has some probability of disconnecting the graph.
                // Should we accept that as a consequence of the probabilistic nature of the algorithm?
                let mut out_key = vec![DataValue::from(layer)];
                out_key.extend_from_slice(tuple_key);
                out_key.push(DataValue::from(idx as i64));
                out_key.push(DataValue::from(subidx as i64));
                out_key.extend_from_slice(&neighbour_key.0);
                out_key.push(DataValue::from(neighbour_key.1 as i64));
                out_key.push(DataValue::from(neighbour_key.2 as i64));
                let out_key_bytes = idx_table.encode_key_for_store(&out_key, Default::default())?;
                self.store_tx.del(&out_key_bytes)?;
                let mut in_key = vec![DataValue::from(layer)];
                in_key.extend_from_slice(&neighbour_key.0);
                in_key.push(DataValue::from(neighbour_key.1 as i64));
                in_key.push(DataValue::from(neighbour_key.2 as i64));
                in_key.extend_from_slice(tuple_key);
                in_key.push(DataValue::from(idx as i64));
                in_key.push(DataValue::from(subidx as i64));
                let in_key_bytes = idx_table.encode_key_for_store(&in_key, Default::default())?;
                self.store_tx.del(&in_key_bytes)?;
                let mut neighbour_self_key = vec![DataValue::from(layer)];
                for _ in 0..2 {
                    neighbour_self_key.extend_from_slice(&neighbour_key.0);
                    neighbour_self_key.push(DataValue::from(neighbour_key.1 as i64));
                    neighbour_self_key.push(DataValue::from(neighbour_key.2 as i64));
                }
                let neighbour_val_bytes = self
                    .store_tx
                    .get(
                        &idx_table.encode_key_for_store(&neighbour_self_key, Default::default())?,
                        false,
                    )?
                    .unwrap();
                let mut neighbour_val: Vec<DataValue> =
                    rmp_serde::from_slice(&neighbour_val_bytes[ENCODED_KEY_MIN_LEN..]).unwrap();
                neighbour_val[0] = DataValue::from(neighbour_val[0].get_float().unwrap() - 1.);
                self.store_tx.put(
                    &idx_table.encode_key_for_store(&neighbour_self_key, Default::default())?,
                    &idx_table.encode_val_only_for_store(&neighbour_val, Default::default())?,
                )?;
            }
        }

        if encountered_singletons {
            // the entry point is removed, we need to do something
            let ep_res = idx_table
                .scan_bounded_prefix(
                    self,
                    &[],
                    &[DataValue::from(i64::MIN)],
                    &[DataValue::from(1)],
                )
                .next();
            let mut canary_key = vec![DataValue::from(1)];
            for _ in 0..2 {
                for _ in 0..orig_table.metadata.keys.len() {
                    canary_key.push(DataValue::Null);
                }
                canary_key.push(DataValue::Null);
                canary_key.push(DataValue::Null);
            }
            let canary_key_bytes =
                idx_table.encode_key_for_store(&canary_key, Default::default())?;
            if let Some(ep) = ep_res {
                let ep = ep?;
                let target_key_bytes = idx_table.encode_key_for_store(&ep, Default::default())?;
                let bottom_level = ep[0].get_int().unwrap();
                // canary value is for conflict detection: prevent the scenario of disconnected graphs at all levels
                let canary_value = [
                    DataValue::from(bottom_level),
                    DataValue::Bytes(target_key_bytes),
                    DataValue::from(false),
                ];
                let canary_value_bytes =
                    idx_table.encode_val_only_for_store(&canary_value, Default::default())?;
                self.store_tx.put(&canary_key_bytes, &canary_value_bytes)?;
            } else {
                // HA! we have removed the last item in the index
                self.store_tx.del(&canary_key_bytes)?;
            }
        }

        Ok(())
    }
    pub(crate) fn hnsw_knn(
        &self,
        q: Vector,
        config: &HnswSearch,
        filter_bytecode: &Option<(Vec<Bytecode>, SourceSpan)>,
        stack: &mut Vec<DataValue>,
    ) -> Result<Vec<Tuple>> {
        if q.len() != config.manifest.vec_dim {
            bail!("query vector dimension mismatch");
        }
        let q = match (q, config.manifest.dtype) {
            (v @ Vector::F32(_), VecElementType::F32) => v,
            (v @ Vector::F64(_), VecElementType::F64) => v,
            (Vector::F32(v), VecElementType::F64) => Vector::F64(v.mapv(|x| x as f64)),
            (Vector::F64(v), VecElementType::F32) => Vector::F32(v.mapv(|x| x as f32)),
        };

        let mut vec_cache = VectorCache {
            cache: Default::default(),
            distance: config.manifest.distance,
        };

        let ep_res = config
            .idx_handle
            .scan_bounded_prefix(
                self,
                &[],
                &[DataValue::from(i64::MIN)],
                &[DataValue::from(1)],
            )
            .next();
        if let Some(ep) = ep_res {
            let ep = ep?;
            let bottom_level = ep[0].get_int().unwrap();
            let ep_idx = match ep[config.base_handle.metadata.keys.len() + 1].get_int() {
                Some(x) => x as usize,
                None => {
                    // this occurs if the index is empty
                    return Ok(vec![])
                },
            };
            let ep_t_key = ep[1..config.base_handle.metadata.keys.len() + 1].to_vec();
            let ep_subidx = ep[config.base_handle.metadata.keys.len() + 2]
                .get_int()
                .unwrap() as i32;
            let ep_key = (ep_t_key, ep_idx, ep_subidx);
            vec_cache.ensure_key(&ep_key, &config.base_handle, self)?;
            let ep_distance = vec_cache.v_dist(&q, &ep_key);
            let mut found_nn = PriorityQueue::new();
            found_nn.push(ep_key, OrderedFloat(ep_distance));
            for current_level in bottom_level..0 {
                self.hnsw_search_level(
                    &q,
                    1,
                    current_level,
                    &config.base_handle,
                    &config.idx_handle,
                    &mut found_nn,
                    &mut vec_cache,
                )?;
            }
            self.hnsw_search_level(
                &q,
                config.ef,
                0,
                &config.base_handle,
                &config.idx_handle,
                &mut found_nn,
                &mut vec_cache,
            )?;
            if found_nn.is_empty() {
                return Ok(vec![]);
            }

            if config.filter.is_none() {
                while found_nn.len() > config.k {
                    found_nn.pop();
                }
            }

            let mut ret = vec![];

            while let Some((cand_key, OrderedFloat(distance))) = found_nn.pop() {
                if let Some(r) = config.radius {
                    if distance > r {
                        continue;
                    }
                }

                let mut cand_tuple = config
                    .base_handle
                    .get(self, &cand_key.0)?
                    .ok_or_else(|| miette!("corrupted index"))?;

                // make sure the order is the same as in all_bindings()!!!
                if config.bind_field.is_some() {
                    let field = if cand_key.1 < config.base_handle.metadata.keys.len() {
                        config.base_handle.metadata.keys[cand_key.1].name.clone()
                    } else {
                        config.base_handle.metadata.non_keys
                            [cand_key.1 - config.base_handle.metadata.keys.len()]
                        .name
                        .clone()
                    };
                    cand_tuple.push(DataValue::Str(field));
                }
                if config.bind_field_idx.is_some() {
                    cand_tuple.push(if cand_key.2 < 0 {
                        DataValue::Null
                    } else {
                        DataValue::from(cand_key.2 as i64)
                    });
                }
                if config.bind_distance.is_some() {
                    cand_tuple.push(DataValue::from(distance));
                }
                if config.bind_vector.is_some() {
                    let vec = if cand_key.2 < 0 {
                        cand_tuple[cand_key.1].clone()
                    } else {
                        match &cand_tuple[cand_key.1] {
                            DataValue::List(v) => v[cand_key.2 as usize].clone(),
                            v => bail!("corrupted index value {:?}", v),
                        }
                    };
                    cand_tuple.push(vec);
                }

                if let Some((code, span)) = filter_bytecode {
                    if !eval_bytecode_pred(code, &cand_tuple, stack, *span)? {
                        continue;
                    }
                }

                ret.push(cand_tuple);
            }
            ret.reverse();
            ret.truncate(config.k);

            Ok(ret)
        } else {
            Ok(vec![])
        }
    }
}
