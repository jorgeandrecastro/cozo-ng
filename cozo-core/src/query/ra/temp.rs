/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use either::{Left, Right};
use itertools::Itertools;
use miette::Result;
use std::collections::{BTreeMap, BTreeSet};

use crate::data::expr::{compute_bounds, eval_bytecode_pred, Bytecode, Expr};
use crate::data::program::MagicSymbol;
use crate::data::symb::Symbol;
use crate::data::tuple::{Tuple, TupleIter};
use crate::data::value::DataValue;
use crate::parse::SourceSpan;
use crate::runtime::temp_store::EpochStore;
use crate::utils::swap_option_result;

use super::{eliminate_from_tuple, filter_iter, flatten_err, invert_option_err, join_is_prefix};

#[derive(Debug)]
pub(crate) struct TempStoreRA {
    pub(crate) bindings: Vec<Symbol>,
    pub(crate) storage_key: MagicSymbol,
    pub(crate) filters: Vec<Expr>,
    pub(crate) filters_bytecodes: Vec<(Vec<Bytecode>, SourceSpan)>,
    pub(crate) span: SourceSpan,
}

impl TempStoreRA {
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
            self.filters_bytecodes.push((e.compile()?, e.span()))
        }
        Ok(())
    }

    pub(super) fn iter<'a>(
        &'a self,
        delta_rule: Option<&MagicSymbol>,
        stores: &'a BTreeMap<MagicSymbol, EpochStore>,
    ) -> Result<TupleIter<'a>> {
        let storage = stores.get(&self.storage_key).unwrap();

        let scan_epoch = match delta_rule {
            None => false,
            Some(name) => *name == self.storage_key,
        };
        let it = if scan_epoch {
            Left(storage.delta_all_iter().map(|t| Ok(t.into_tuple())))
        } else {
            Right(storage.all_iter().map(|t| Ok(t.into_tuple())))
        };
        Ok(if self.filters.is_empty() {
            Box::new(it)
        } else {
            Box::new(filter_iter(self.filters_bytecodes.clone(), it))
        })
    }
    pub(super) fn neg_join<'a>(
        &'a self,
        left_iter: TupleIter<'a>,
        (left_join_indices, right_join_indices): (Vec<usize>, Vec<usize>),
        eliminate_indices: BTreeSet<usize>,
        stores: &'a BTreeMap<MagicSymbol, EpochStore>,
    ) -> Result<TupleIter<'a>> {
        let storage = stores.get(&self.storage_key).unwrap();
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

                        'outer: for found in storage.prefix_iter(&prefix) {
                            for (left_idx, right_idx) in
                                left_join_indices.iter().zip(right_join_indices.iter())
                            {
                                if tuple[*left_idx] != *found.get(*right_idx) {
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
            for tuple in storage.all_iter() {
                let to_join: Box<[DataValue]> = right_join_indices
                    .iter()
                    .map(|i| tuple.get(*i).clone())
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
    pub(super) fn prefix_join<'a>(
        &'a self,
        left_iter: TupleIter<'a>,
        (left_join_indices, right_join_indices): (Vec<usize>, Vec<usize>),
        eliminate_indices: BTreeSet<usize>,
        delta_rule: Option<&MagicSymbol>,
        stores: &'a BTreeMap<MagicSymbol, EpochStore>,
    ) -> Result<TupleIter<'a>> {
        let storage = stores.get(&self.storage_key).unwrap();

        let mut right_invert_indices = right_join_indices.iter().enumerate().collect_vec();
        right_invert_indices.sort_by_key(|(_, b)| **b);
        let left_to_prefix_indices = right_invert_indices
            .into_iter()
            .map(|(a, _)| left_join_indices[a])
            .collect_vec();
        let scan_epoch = match delta_rule {
            None => false,
            Some(name) => *name == self.storage_key,
        };
        let mut skip_range_check = false;
        let it = left_iter
            .map_ok(move |tuple| {
                let prefix = left_to_prefix_indices
                    .iter()
                    .map(|i| tuple[*i].clone())
                    .collect_vec();
                let mut stack = vec![];

                if !skip_range_check && !self.filters.is_empty() {
                    let other_bindings = &self.bindings[right_join_indices.len()..];
                    let (l_bound, u_bound) = match compute_bounds(&self.filters, other_bindings) {
                        Ok(b) => b,
                        _ => (vec![], vec![]),
                    };
                    if !l_bound.iter().all(|v| *v == DataValue::Null)
                        || !u_bound.iter().all(|v| *v == DataValue::Bot)
                    {
                        let mut lower_bound = prefix.clone();
                        lower_bound.extend(l_bound);
                        let mut upper_bound = prefix;
                        upper_bound.extend(u_bound);
                        let it = if scan_epoch {
                            Left(storage.delta_range_iter(&lower_bound, &upper_bound, true))
                        } else {
                            Right(storage.range_iter(&lower_bound, &upper_bound, true))
                        };
                        return Left(
                            it.map(move |res_found| -> Result<Option<Tuple>> {
                                if self.filters.is_empty() {
                                    let mut ret = tuple.clone();
                                    ret.extend(res_found.into_iter().cloned());
                                    Ok(Some(ret))
                                } else {
                                    let found = res_found.into_tuple();
                                    for (p, span) in self.filters_bytecodes.iter() {
                                        if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                            return Ok(None);
                                        }
                                    }
                                    let mut ret = tuple.clone();
                                    ret.extend(found);
                                    Ok(Some(ret))
                                }
                            })
                            .filter_map(swap_option_result),
                        );
                    }
                }
                skip_range_check = true;

                let it = if scan_epoch {
                    Left(storage.delta_prefix_iter(&prefix))
                } else {
                    Right(storage.prefix_iter(&prefix))
                };

                Right(
                    it.map(move |res_found| -> Result<Option<Tuple>> {
                        if self.filters.is_empty() {
                            let mut ret = tuple.clone();
                            ret.extend(res_found.into_iter().cloned());
                            Ok(Some(ret))
                        } else {
                            let found = res_found.into_tuple();
                            for (p, span) in self.filters_bytecodes.iter() {
                                if !eval_bytecode_pred(p, &found, &mut stack, *span)? {
                                    return Ok(None);
                                }
                            }
                            let mut ret = tuple.clone();
                            ret.extend(found);
                            Ok(Some(ret))
                        }
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
