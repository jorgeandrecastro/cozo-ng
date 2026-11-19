/*
 * Copyright 2023, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/runtime/hnsw.rs

use crate::data::expr::{eval_bytecode_pred, Bytecode};
use crate::data::program::HnswSearch;
use crate::data::relation::VecElementType;
use crate::data::tuple::{Tuple, ENCODED_KEY_MIN_LEN};
use crate::data::value::Vector;
use crate::parse::sys::HnswDistance;
use crate::runtime::relation::RelationHandle;
use crate::runtime::transact::SessionTx;
use crate::{DataValue, SourceSpan};
use itertools::Itertools;
use miette::{bail, miette, Result};
use ordered_float::OrderedFloat;
use priority_queue::PriorityQueue;
use rand::RngExt;
use rustc_hash::{FxHashMap, FxHashSet};
use smartstring::{LazyCompact, SmartString};
use std::cmp::{max, Reverse};

#[derive(Debug, Clone, PartialEq, serde_derive::Serialize, serde_derive::Deserialize)]
pub(crate) struct HnswIndexManifest {
    pub(crate) base_relation: SmartString<LazyCompact>,
    pub(crate) index_name: SmartString<LazyCompact>,
    pub(crate) vec_dim: usize,
    pub(crate) dtype: VecElementType,
    pub(crate) vec_fields: Vec<usize>,
    pub(crate) distance: HnswDistance,
    pub(crate) ef_construction: usize,
    pub(crate) m_neighbours: usize,
    pub(crate) m_max: usize,
    pub(crate) m_max0: usize,
    pub(crate) level_multiplier: f64,
    pub(crate) index_filter: Option<String>,
    pub(crate) extend_candidates: bool,
    pub(crate) keep_pruned_connections: bool,
}

impl HnswIndexManifest {
    fn get_random_level(&self) -> i64 {
        let mut rng = rand::rng();
        let uniform_num: f64 = rng.random_range(0.0..1.0);
        let r = -uniform_num.ln() * self.level_multiplier;
        // the level is the largest integer smaller than r
        -(r.floor() as i64)
    }
}

type CompoundKey = (Tuple, usize, i32);

struct VectorCache {
    cache: FxHashMap<CompoundKey, Vector>,
    distance: HnswDistance,
}

impl VectorCache {
    fn insert(&mut self, k: CompoundKey, v: Vector) {
        self.cache.insert(k, v);
    }
    fn dist(&self, v1: &Vector, v2: &Vector) -> f64 {
        match self.distance {
            HnswDistance::L2 => match (v1, v2) {
                (Vector::F32(a), Vector::F32(b)) => {
                    let diff = a - b;
                    diff.dot(&diff) as f64
                }
                (Vector::F64(a), Vector::F64(b)) => {
                    let diff = a - b;
                    diff.dot(&diff)
                }
                _ => panic!("Cannot compute L2 distance between {:?} and {:?}", v1, v2),
            },
            HnswDistance::Cosine => match (v1, v2) {
                (Vector::F32(a), Vector::F32(b)) => {
                    let a_norm = a.dot(a) as f64;
                    let b_norm = b.dot(b) as f64;
                    let dot = a.dot(b) as f64;
                    1.0 - dot / (a_norm * b_norm).sqrt()
                }
                (Vector::F64(a), Vector::F64(b)) => {
                    let a_norm = a.dot(a);
                    let b_norm = b.dot(b);
                    let dot = a.dot(b);
                    1.0 - dot / (a_norm * b_norm).sqrt()
                }
                _ => panic!(
                    "Cannot compute cosine distance between {:?} and {:?}",
                    v1, v2
                ),
            },
            HnswDistance::InnerProduct => match (v1, v2) {
                (Vector::F32(a), Vector::F32(b)) => {
                    let dot = a.dot(b);
                    1. - dot as f64
                }
                (Vector::F64(a), Vector::F64(b)) => {
                    let dot = a.dot(b);
                    1. - dot
                }
                _ => panic!("Cannot compute inner product between {:?} and {:?}", v1, v2),
            },
        }
    }
    fn v_dist(&self, v: &Vector, key: &CompoundKey) -> f64 {
        let v2 = self.cache.get(key).unwrap();
        self.dist(v, v2)
    }
    fn k_dist(&self, k1: &CompoundKey, k2: &CompoundKey) -> f64 {
        let v1 = self.cache.get(k1).unwrap();
        let v2 = self.cache.get(k2).unwrap();
        self.dist(v1, v2)
    }
    fn get_key(&self, key: &CompoundKey) -> &Vector {
        self.cache.get(key).unwrap()
    }
    fn ensure_key(
        &mut self,
        key: &CompoundKey,
        handle: &RelationHandle,
        tx: &SessionTx<'_>,
    ) -> Result<()> {
        if !self.cache.contains_key(key) {
            match handle.get(tx, &key.0)? {
                Some(tuple) => {
                    let mut field = &tuple[key.1];
                    if key.2 >= 0 {
                        match field {
                            DataValue::List(l) => {
                                field = &l[key.2 as usize];
                            }
                            _ => bail!("Cannot interpret {} as list", field),
                        }
                    }
                    match field {
                        DataValue::Vec(v) => {
                            self.cache.insert(key.clone(), v.clone());
                        }
                        _ => bail!("Cannot interpret {} as vector", field),
                    }
                }
                None => bail!("Cannot find compound key for HNSW: {:?}", key),
            }
        }
        Ok(())
    }
}

mod insert;
mod operations;

#[cfg(test)]
mod tests {
    use rand::RngExt;
    use std::collections::BTreeMap;

    #[test]
    fn test_random_level() {
        let m = 20;
        let mult = 1. / (m as f64).ln();
        let mut rng = rand::rng();
        let mut collected = BTreeMap::new();
        for _ in 0..10000 {
            let uniform_num: f64 = rng.random_range(0.0..1.0);
            let r = -uniform_num.ln() * mult;
            // the level is the largest integer smaller than r
            let level = -(r.floor() as i64);
            collected.entry(level).and_modify(|x| *x += 1).or_insert(1);
        }
        println!("{:?}", collected);
    }
}
