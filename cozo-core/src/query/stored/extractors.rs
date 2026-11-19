/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/stored/extractors.rs

use std::collections::BTreeSet;

use itertools::Itertools;
use miette::{Diagnostic, Result, WrapErr};
use thiserror::Error;

use crate::data::expr::Expr;
use crate::data::relation::{ColumnDef, NullableColType};
use crate::data::symb::Symbol;
use crate::data::tuple::Tuple;
use crate::data::value::{DataValue, ValidityTs};

pub(super) enum DataExtractor {
    DefaultExtractor(Expr, NullableColType),
    IndexExtractor(usize, NullableColType),
}

impl DataExtractor {
    pub(super) fn extract_data(&self, tuple: &Tuple, cur_vld: ValidityTs) -> Result<DataValue> {
        Ok(match self {
            DataExtractor::DefaultExtractor(expr, typ) => typ
                .coerce(expr.clone().eval_to_const()?, cur_vld)
                .wrap_err_with(|| format!("when processing tuple {tuple:?}"))?,
            DataExtractor::IndexExtractor(i, typ) => typ
                .coerce(tuple[*i].clone(), cur_vld)
                .wrap_err_with(|| format!("when processing tuple {tuple:?}"))?,
        })
    }
}

pub(super) fn make_extractors(
    stored: &[ColumnDef],
    input: &[ColumnDef],
    bindings: &[Symbol],
    tuple_headers: &[Symbol],
) -> Result<Vec<DataExtractor>> {
    stored
        .iter()
        .map(|s| make_extractor(s, input, bindings, tuple_headers))
        .try_collect()
}

pub(super) fn make_update_extractors(
    stored: &[ColumnDef],
    input: &[ColumnDef],
    bindings: &[Symbol],
    tuple_headers: &[Symbol],
) -> Result<Vec<Option<DataExtractor>>> {
    let input_keys: BTreeSet<_> = input.iter().map(|b| &b.name).collect();
    let mut extractors = Vec::with_capacity(stored.len());
    for col in stored.iter() {
        if input_keys.contains(&col.name) {
            extractors.push(Some(make_extractor(col, input, bindings, tuple_headers)?));
        } else {
            extractors.push(None);
        }
    }
    Ok(extractors)
}

fn make_extractor(
    stored: &ColumnDef,
    input: &[ColumnDef],
    bindings: &[Symbol],
    tuple_headers: &[Symbol],
) -> Result<DataExtractor> {
    for (inp_col, inp_binding) in input.iter().zip(bindings.iter()) {
        if inp_col.name == stored.name {
            for (idx, tuple_head) in tuple_headers.iter().enumerate() {
                if tuple_head == inp_binding {
                    return Ok(DataExtractor::IndexExtractor(idx, stored.typing.clone()));
                }
            }
        }
    }
    if let Some(expr) = &stored.default_gen {
        Ok(DataExtractor::DefaultExtractor(
            expr.clone(),
            stored.typing.clone(),
        ))
    } else {
        #[derive(Debug, Error, Diagnostic)]
        #[error("cannot make extractor for column {0}")]
        #[diagnostic(code(eval::unable_to_make_extractor))]
        struct UnableToMakeExtractor(String);
        Err(UnableToMakeExtractor(stored.name.to_string()).into())
    }
}
