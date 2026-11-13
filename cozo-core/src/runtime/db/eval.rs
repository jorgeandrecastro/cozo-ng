/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::{BTreeMap, BTreeSet};

use miette::Result;

use crate::data::value::DataValue;
use crate::parse::parse_expressions;
use crate::Symbol;

/// Evaluate a string expression in the context of a set of parameters and variables
pub fn evaluate_expressions(
    src: &str,
    params: &BTreeMap<String, DataValue>,
    vars: &BTreeMap<String, DataValue>,
) -> Result<DataValue> {
    _evaluate_expressions(src, params, vars).map_err(|err| {
        if err.source().is_none() {
            err.with_source_code(format!("{src} "))
        } else {
            err
        }
    })
}

/// Get the variables referenced in a string expression
pub fn get_variables(src: &str, params: &BTreeMap<String, DataValue>) -> Result<BTreeSet<String>> {
    _get_variables(src, params).map_err(|err| {
        if err.source().is_none() {
            err.with_source_code(format!("{src} "))
        } else {
            err
        }
    })
}

fn _evaluate_expressions(
    src: &str,
    params: &BTreeMap<String, DataValue>,
    vars: &BTreeMap<String, DataValue>,
) -> Result<DataValue> {
    let mut expr = parse_expressions(src, params)?;
    let mut ctx = vec![];
    let mut binding_map = BTreeMap::new();
    for (i, (k, v)) in vars.iter().enumerate() {
        ctx.push(v.clone());
        binding_map.insert(Symbol::new(k, Default::default()), i);
    }
    expr.fill_binding_indices(&binding_map)?;
    expr.eval(&ctx)
}

fn _get_variables(src: &str, params: &BTreeMap<String, DataValue>) -> Result<BTreeSet<String>> {
    let expr = parse_expressions(src, params)?;
    expr.get_variables()
}