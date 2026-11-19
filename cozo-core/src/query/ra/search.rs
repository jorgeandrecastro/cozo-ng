/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/ra/search.rs

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use either::{Left, Right};
use itertools::Itertools;
use miette::{bail, Result};
use smartstring::SmartString;

use crate::data::expr::{compute_bounds, eval_bytecode_pred, Bytecode, Expr};
use crate::data::program::{FtsSearch, HnswSearch, MagicSymbol};
use crate::data::symb::Symbol;
use crate::data::tuple::{Tuple, TupleIter};
use crate::data::value::{DataValue, ValidityTs};
use crate::parse::SourceSpan;
use crate::runtime::minhash_lsh::LshSearch;
use crate::runtime::relation::RelationHandle;
use crate::runtime::temp_store::EpochStore;
use crate::runtime::transact::SessionTx;
use crate::utils::swap_option_result;

use super::{
    eliminate_from_tuple, filter_iter, flatten_err, invert_option_err, join_is_prefix, RelAlgebra,
};

#[derive(Debug)]
pub(crate) struct StoredRA {
    pub(crate) bindings: Vec<Symbol>,
    pub(crate) storage: RelationHandle,
    pub(crate) filters: Vec<Expr>,
    pub(crate) filters_bytecodes: Vec<(Vec<Bytecode>, SourceSpan)>,
    pub(crate) span: SourceSpan,
}

#[derive(Debug)]
pub(crate) struct HnswSearchRA {
    pub(crate) parent: Box<RelAlgebra>,
    pub(crate) hnsw_search: HnswSearch,
    pub(crate) filter_bytecode: Option<(Vec<Bytecode>, SourceSpan)>,
    pub(crate) own_bindings: Vec<Symbol>,
}

#[derive(Debug)]
pub(crate) struct LshSearchRA {
    pub(crate) parent: Box<RelAlgebra>,
    pub(crate) lsh_search: LshSearch,
    pub(crate) filter_bytecode: Option<(Vec<Bytecode>, SourceSpan)>,
    pub(crate) own_bindings: Vec<Symbol>,
}

impl LshSearchRA {
    pub(super) fn fill_binding_indices_and_compile(&mut self) -> Result<()> {
        self.parent.fill_binding_indices_and_compile()?;
        if self.lsh_search.filter.is_some() {
            let bindings: BTreeMap<_, _> = self
                .own_bindings
                .iter()
                .cloned()
                .enumerate()
                .map(|(a, b)| (b, a))
                .collect();
            let filter = self.lsh_search.filter.as_mut().unwrap();
            filter.fill_binding_indices(&bindings)?;
            self.filter_bytecode = Some((filter.compile()?, filter.span()));
        }
        Ok(())
    }
    pub(super) fn iter<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        delta_rule: Option<&MagicSymbol>,
        stores: &'a BTreeMap<MagicSymbol, EpochStore>,
    ) -> Result<TupleIter<'a>> {
        let bindings = self.parent.bindings_after_eliminate();
        let mut bind_idx = usize::MAX;
        for (i, b) in bindings.iter().enumerate() {
            if *b == self.lsh_search.query {
                bind_idx = i;
                break;
            }
        }
        let config = self.lsh_search.clone();
        let filter_code = self.filter_bytecode.clone();
        let mut stack = vec![];
        let perms = config.manifest.get_hash_perms();
        let tokenizer = tx.tokenizers.get(
            &config.idx_handle.name,
            &config.manifest.tokenizer,
            &config.manifest.filters,
        )?;

        let it = self
            .parent
            .iter(tx, delta_rule, stores)?
            .map_ok(move |tuple| -> Result<_> {
                let res = tx.lsh_search(
                    &tuple[bind_idx],
                    &config,
                    &mut stack,
                    &filter_code,
                    &perms,
                    &tokenizer,
                )?;
                Ok(res.into_iter().map(move |t| {
                    let mut r = tuple.clone();
                    r.extend(t);
                    r
                }))
            })
            .map(flatten_err)
            .flatten_ok();
        Ok(Box::new(it))
    }
}

#[derive(Debug)]
pub(crate) struct FtsSearchRA {
    pub(crate) parent: Box<RelAlgebra>,
    pub(crate) fts_search: FtsSearch,
    pub(crate) filter_bytecode: Option<(Vec<Bytecode>, SourceSpan)>,
    pub(crate) own_bindings: Vec<Symbol>,
}

impl FtsSearchRA {
    pub(super) fn fill_binding_indices_and_compile(&mut self) -> Result<()> {
        self.parent.fill_binding_indices_and_compile()?;
        if self.fts_search.filter.is_some() {
            let bindings: BTreeMap<_, _> = self
                .own_bindings
                .iter()
                .cloned()
                .enumerate()
                .map(|(a, b)| (b, a))
                .collect();
            let filter = self.fts_search.filter.as_mut().unwrap();
            filter.fill_binding_indices(&bindings)?;
            self.filter_bytecode = Some((filter.compile()?, filter.span()));
        }
        Ok(())
    }
    pub(super) fn iter<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        delta_rule: Option<&MagicSymbol>,
        stores: &'a BTreeMap<MagicSymbol, EpochStore>,
    ) -> Result<TupleIter<'a>> {
        let bindings = self.parent.bindings_after_eliminate();
        let mut bind_idx = usize::MAX;
        for (i, b) in bindings.iter().enumerate() {
            if *b == self.fts_search.query {
                bind_idx = i;
                break;
            }
        }
        let config = self.fts_search.clone();
        let filter_code = self.filter_bytecode.clone();
        let mut stack = vec![];
        let mut idf_cache = Default::default();
        let tokenizer = tx.tokenizers.get(
            &config.idx_handle.name,
            &config.manifest.tokenizer,
            &config.manifest.filters,
        )?;
        let it = self
            .parent
            .iter(tx, delta_rule, stores)?
            .map_ok(move |tuple| -> Result<_> {
                let q = match tuple[bind_idx].clone() {
                    DataValue::Str(s) => s,
                    DataValue::List(l) => {
                        let mut coll = SmartString::new();
                        for d in l {
                            match d {
                                DataValue::Str(s) => {
                                    if !coll.is_empty() {
                                        coll.write_str(" OR ").unwrap();
                                    }
                                    coll.write_str(&s).unwrap();
                                }
                                d => bail!("Expected string for FTS search, got {:?}", d),
                            }
                        }
                        coll
                    }
                    d => bail!("Expected string for FTS search, got {:?}", d),
                };

                let res = tx.fts_search(
                    &q,
                    &config,
                    &filter_code,
                    &tokenizer,
                    &mut stack,
                    &mut idf_cache,
                )?;
                Ok(res.into_iter().map(move |t| {
                    let mut r = tuple.clone();
                    r.extend(t);
                    r
                }))
            })
            .map(flatten_err)
            .flatten_ok();
        Ok(Box::new(it))
    }
}

impl HnswSearchRA {
    pub(super) fn fill_binding_indices_and_compile(&mut self) -> Result<()> {
        self.parent.fill_binding_indices_and_compile()?;
        if self.hnsw_search.filter.is_some() {
            let bindings: BTreeMap<_, _> = self
                .own_bindings
                .iter()
                .cloned()
                .enumerate()
                .map(|(a, b)| (b, a))
                .collect();
            let filter = self.hnsw_search.filter.as_mut().unwrap();
            filter.fill_binding_indices(&bindings)?;
            self.filter_bytecode = Some((filter.compile()?, filter.span()));
        }
        Ok(())
    }
    pub(super) fn iter<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        delta_rule: Option<&MagicSymbol>,
        stores: &'a BTreeMap<MagicSymbol, EpochStore>,
    ) -> Result<TupleIter<'a>> {
        let bindings = self.parent.bindings_after_eliminate();
        let mut bind_idx = usize::MAX;
        for (i, b) in bindings.iter().enumerate() {
            if *b == self.hnsw_search.query {
                bind_idx = i;
                break;
            }
        }
        let config = self.hnsw_search.clone();
        let filter_code = self.filter_bytecode.clone();
        let mut stack = vec![];
        let it = self
            .parent
            .iter(tx, delta_rule, stores)?
            .map_ok(move |tuple| -> Result<_> {
                let v = match tuple[bind_idx].clone() {
                    DataValue::Vec(v) => v,
                    d => bail!("Expected vector, got {:?}", d),
                };

                let res = tx.hnsw_knn(v, &config, &filter_code, &mut stack)?;
                Ok(res.into_iter().map(move |t| {
                    let mut r = tuple.clone();
                    r.extend(t);
                    r
                }))
            })
            .map(flatten_err)
            .flatten_ok();
        Ok(Box::new(it))
    }
}

#[derive(Debug)]
pub(crate) struct StoredWithValidityRA {
    pub(crate) bindings: Vec<Symbol>,
    pub(crate) storage: RelationHandle,
    pub(crate) filters: Vec<Expr>,
    pub(crate) filters_bytecodes: Vec<(Vec<Bytecode>, SourceSpan)>,
    pub(crate) valid_at: ValidityTs,
    pub(crate) span: SourceSpan,
}

impl StoredWithValidityRA {
    pub(super) fn fill_binding_indices_and_compile(&mut self) -> Result<()> {
        let bindings: BTreeMap<_, _> = self
            .bindings
            .iter()
            .cloned()
            .enumerate()
            .map(|(a, b)| (b, a))
            .collect();
        for e in self.filters.iter_mut() {
            e.fill_binding_indices(&bindings)?;
            self.filters_bytecodes.push((e.compile()?, e.span()));
        }
        Ok(())
    }
    pub(super) fn iter<'a>(&'a self, tx: &'a SessionTx<'_>) -> Result<TupleIter<'a>> {
        let it = self.storage.skip_scan_all(tx, self.valid_at);
        Ok(if self.filters.is_empty() {
            Box::new(it)
        } else {
            Box::new(filter_iter(self.filters_bytecodes.clone(), it))
        })
    }
    pub(super) fn prefix_join<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        left_iter: TupleIter<'a>,
        (left_join_indices, right_join_indices): (Vec<usize>, Vec<usize>),
        eliminate_indices: BTreeSet<usize>,
    ) -> Result<TupleIter<'a>> {
        let mut right_invert_indices = right_join_indices.iter().enumerate().collect_vec();
        right_invert_indices.sort_by_key(|(_, b)| **b);
        let left_to_prefix_indices = right_invert_indices
            .into_iter()
            .map(|(a, _)| left_join_indices[a])
            .collect_vec();

        let mut skip_range_check = false;

        let it = left_iter
            .map_ok(move |tuple| {
                let prefix = left_to_prefix_indices
                    .iter()
                    .map(|i| tuple[*i].clone())
                    .collect_vec();

                if !skip_range_check && !self.filters.is_empty() {
                    let other_bindings =
                        &self.bindings[right_join_indices.len()..self.storage.metadata.keys.len()];
                    let (l_bound, u_bound) = match compute_bounds(&self.filters, other_bindings) {
                        Ok(b) => b,
                        _ => (vec![], vec![]),
                    };
                    if !l_bound.iter().all(|v| *v == DataValue::Null)
                        || !u_bound.iter().all(|v| *v == DataValue::Bot)
                    {
                        let mut stack = vec![];
                        return Left(
                            self.storage
                                .skip_scan_bounded_prefix(
                                    tx,
                                    &prefix,
                                    &l_bound,
                                    &u_bound,
                                    self.valid_at,
                                )
                                .map(move |res_found| -> Result<Option<Tuple>> {
                                    let found = res_found?;
                                    for (p, span) in self.filters_bytecodes.iter() {
                                        if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                            return Ok(None);
                                        }
                                    }
                                    let mut ret = tuple.clone();
                                    ret.extend(found);
                                    Ok(Some(ret))
                                })
                                .filter_map(swap_option_result),
                        );
                    }
                }
                skip_range_check = true;
                let mut stack = vec![];
                Right(
                    self.storage
                        .skip_scan_prefix(tx, &prefix, self.valid_at)
                        .map(move |res_found| -> Result<Option<Tuple>> {
                            let found = res_found?;
                            for (p, span) in self.filters_bytecodes.iter() {
                                if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                    return Ok(None);
                                }
                            }
                            let mut ret = tuple.clone();
                            ret.extend(found);
                            Ok(Some(ret))
                        })
                        .filter_map(swap_option_result),
                )
            })
            .flatten_ok()
            .map(flatten_err);
        Ok(if eliminate_indices.is_empty() {
            Box::new(it)
        } else {
            Box::new(it.map_ok(move |t| eliminate_from_tuple(t, &eliminate_indices)))
        })
    }
}

impl StoredRA {
    pub(super) fn fill_binding_indices_and_compile(&mut self) -> Result<()> {
        let bindings: BTreeMap<_, _> = self
            .bindings
            .iter()
            .cloned()
            .enumerate()
            .map(|(a, b)| (b, a))
            .collect();
        for e in self.filters.iter_mut() {
            e.fill_binding_indices(&bindings)?;
            self.filters_bytecodes.push((e.compile()?, e.span()));
        }
        Ok(())
    }

    pub(super) fn point_lookup_join<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        left_iter: TupleIter<'a>,
        key_len: usize,
        left_to_prefix_indices: Vec<usize>,
        eliminate_indices: BTreeSet<usize>,
        left_join_indices: Vec<usize>,
        right_join_indices: Vec<usize>,
    ) -> Result<TupleIter<'a>> {
        let mut stack = vec![];

        let it = left_iter
            .map_ok(move |tuple| -> Result<Option<Tuple>> {
                let prefix = left_to_prefix_indices
                    .iter()
                    .map(|i| tuple[*i].clone())
                    .collect_vec();
                let key = &prefix[0..key_len];
                match self.storage.get(tx, key)? {
                    None => Ok(None),
                    Some(found) => {
                        for (lk, rk) in left_join_indices.iter().zip(right_join_indices.iter()) {
                            if tuple[*lk] != found[*rk] {
                                return Ok(None);
                            }
                        }
                        for (p, span) in self.filters_bytecodes.iter() {
                            if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                return Ok(None);
                            }
                        }
                        let mut ret = tuple;
                        ret.extend(found);
                        Ok(Some(ret))
                    }
                }
            })
            .flatten_ok()
            .filter_map(invert_option_err);
        Ok(if eliminate_indices.is_empty() {
            Box::new(it)
        } else {
            Box::new(it.map_ok(move |t| eliminate_from_tuple(t, &eliminate_indices)))
        })
    }

    pub(super) fn prefix_join<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        left_iter: TupleIter<'a>,
        (left_join_indices, right_join_indices): (Vec<usize>, Vec<usize>),
        eliminate_indices: BTreeSet<usize>,
    ) -> Result<TupleIter<'a>> {
        let mut right_invert_indices = right_join_indices.iter().enumerate().collect_vec();
        right_invert_indices.sort_by_key(|(_, b)| **b);
        let left_to_prefix_indices = right_invert_indices
            .into_iter()
            .map(|(a, _)| left_join_indices[a])
            .collect_vec();

        let key_len = self.storage.metadata.keys.len();
        if left_to_prefix_indices.len() >= key_len {
            return self.point_lookup_join(
                tx,
                left_iter,
                key_len,
                left_to_prefix_indices,
                eliminate_indices,
                left_join_indices,
                right_join_indices,
            );
        }

        let mut skip_range_check = false;
        // In some cases, maybe we can stop as soon as we get one result?
        let it = left_iter
            .map_ok(move |tuple| {
                let prefix = left_to_prefix_indices
                    .iter()
                    .map(|i| tuple[*i].clone())
                    .collect_vec();
                let mut stack = vec![];

                if !skip_range_check && !self.filters.is_empty() {
                    let other_bindings =
                        &self.bindings[right_join_indices.len()..self.storage.metadata.keys.len()];
                    let (l_bound, u_bound) = match compute_bounds(&self.filters, other_bindings) {
                        Ok(b) => b,
                        _ => (vec![], vec![]),
                    };
                    if !l_bound.iter().all(|v| *v == DataValue::Null)
                        || !u_bound.iter().all(|v| *v == DataValue::Bot)
                    {
                        return Left(
                            self.storage
                                .scan_bounded_prefix(tx, &prefix, &l_bound, &u_bound)
                                .map(move |res_found| -> Result<Option<Tuple>> {
                                    let found = res_found?;
                                    for (p, span) in self.filters_bytecodes.iter() {
                                        if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                            return Ok(None);
                                        }
                                    }
                                    let mut ret = tuple.clone();
                                    ret.extend(found);
                                    Ok(Some(ret))
                                })
                                .filter_map(swap_option_result),
                        );
                    }
                }
                skip_range_check = true;
                Right(
                    self.storage
                        .scan_prefix(tx, &prefix)
                        .map(move |res_found| -> Result<Option<Tuple>> {
                            let found = res_found?;
                            for (p, span) in self.filters_bytecodes.iter() {
                                if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                    return Ok(None);
                                }
                            }
                            let mut ret = tuple.clone();
                            ret.extend(found);
                            Ok(Some(ret))
                        })
                        .filter_map(swap_option_result),
                )
            })
            .flatten_ok()
            .map(flatten_err);
        Ok(if eliminate_indices.is_empty() {
            Box::new(it)
        } else {
            Box::new(it.map_ok(move |t| eliminate_from_tuple(t, &eliminate_indices)))
        })
    }

    pub(super) fn neg_join<'a>(
        &'a self,
        tx: &'a SessionTx<'_>,
        left_iter: TupleIter<'a>,
        (left_join_indices, right_join_indices): (Vec<usize>, Vec<usize>),
        eliminate_indices: BTreeSet<usize>,
    ) -> Result<TupleIter<'a>> {
        debug_assert!(!right_join_indices.is_empty());
        let mut right_invert_indices = right_join_indices.iter().enumerate().collect_vec();
        right_invert_indices.sort_by_key(|(_, b)| **b);
        let mut left_to_prefix_indices = vec![];
        for (ord, (idx, ord_sorted)) in right_invert_indices.iter().enumerate() {
            if ord != **ord_sorted {
                break;
            }
            left_to_prefix_indices.push(left_join_indices[*idx]);
        }

        if join_is_prefix(&right_join_indices) {
            Ok(Box::new(
                left_iter
                    .map_ok(move |tuple| -> Result<Option<Tuple>> {
                        let prefix = left_to_prefix_indices
                            .iter()
                            .map(|i| tuple[*i].clone())
                            .collect_vec();

                        'outer: for found in self.storage.scan_prefix(tx, &prefix) {
                            let found = found?;
                            for (left_idx, right_idx) in
                                left_join_indices.iter().zip(right_join_indices.iter())
                            {
                                if tuple[*left_idx] != found[*right_idx] {
                                    continue 'outer;
                                }
                            }
                            return Ok(None);
                        }

                        Ok(Some(if !eliminate_indices.is_empty() {
                            tuple
                                .into_iter()
                                .enumerate()
                                .filter_map(|(i, v)| {
                                    if eliminate_indices.contains(&i) {
                                        None
                                    } else {
                                        Some(v)
                                    }
                                })
                                .collect_vec()
                        } else {
                            tuple
                        }))
                    })
                    .map(flatten_err)
                    .filter_map(invert_option_err),
            ))
        } else {
            let mut right_join_vals = BTreeSet::new();

            for tuple in self.storage.scan_all(tx) {
                let tuple = tuple?;
                let to_join: Box<[DataValue]> = right_join_indices
                    .iter()
                    .map(|i| tuple[*i].clone())
                    .collect();
                right_join_vals.insert(to_join);
            }
            Ok(Box::new(
                left_iter
                    .map_ok(move |tuple| -> Result<Option<Tuple>> {
                        let left_join_vals: Box<[DataValue]> = left_join_indices
                            .iter()
                            .map(|i| tuple[*i].clone())
                            .collect();
                        if right_join_vals.contains(&left_join_vals) {
                            return Ok(None);
                        }

                        Ok(Some(if !eliminate_indices.is_empty() {
                            tuple
                                .into_iter()
                                .enumerate()
                                .filter_map(|(i, v)| {
                                    if eliminate_indices.contains(&i) {
                                        None
                                    } else {
                                        Some(v)
                                    }
                                })
                                .collect_vec()
                        } else {
                            tuple
                        }))
                    })
                    .map(flatten_err)
                    .filter_map(invert_option_err),
            ))
        }
    }

    pub(super) fn iter<'a>(&'a self, tx: &'a SessionTx<'_>) -> Result<TupleIter<'a>> {
        let it = self.storage.scan_all(tx);
        Ok(if self.filters.is_empty() {
            Box::new(it)
        } else {
            Box::new(filter_iter(self.filters_bytecodes.clone(), it))
        })
    }
}
