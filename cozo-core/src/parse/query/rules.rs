/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use itertools::Itertools;
use miette::{bail, ensure, Diagnostic, Result};
use smartstring::{LazyCompact, SmartString};
use thiserror::Error;

use crate::data::aggr::{parse_aggr, Aggregation};
use crate::data::expr::Expr;
use crate::data::functions::{str2vld, MAX_VALIDITY_TS};
use crate::data::program::{FixedRuleApply, FixedRuleArg, InputInlineRulesOrFixed, InputProgram};
use crate::data::symb::{Symbol, PROG_ENTRY};
use crate::data::value::{DataValue, ValidityTs};
use crate::fixed_rule::utilities::constant::Constant;
use crate::fixed_rule::{FixedRuleHandle, FixedRuleNotFoundError};
use crate::parse::expr::build_expr;
use crate::parse::{ExtractSpan, Pair, Rule, SourceSpan};
use crate::FixedRule;

pub(super) fn parse_rule_head(
    src: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
) -> Result<(
    Symbol,
    Vec<Symbol>,
    Vec<Option<(Aggregation, Vec<DataValue>)>>,
)> {
    let mut src = src.into_inner();
    let name = src.next().unwrap();
    let mut args = vec![];
    let mut aggrs = vec![];
    for p in src {
        let (arg, aggr) = parse_rule_head_arg(p, param_pool)?;
        args.push(arg);
        aggrs.push(aggr);
    }
    Ok((Symbol::new(name.as_str(), name.extract_span()), args, aggrs))
}

#[derive(Error, Diagnostic, Debug)]
#[diagnostic(code(parser::aggr_not_found))]
#[error("Aggregation '{0}' not found")]
struct AggrNotFound(String, #[label] SourceSpan);

fn parse_rule_head_arg(
    src: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
) -> Result<(Symbol, Option<(Aggregation, Vec<DataValue>)>)> {
    let src = src.into_inner().next().unwrap();
    Ok(match src.as_rule() {
        Rule::var => (Symbol::new(src.as_str(), src.extract_span()), None),
        Rule::aggr_arg => {
            let mut inner = src.into_inner();
            let aggr_p = inner.next().unwrap();
            let aggr_name = aggr_p.as_str();
            let var = inner.next().unwrap();
            let args: Vec<_> = inner
                .map(|v| -> Result<DataValue> { build_expr(v, param_pool)?.eval_to_const() })
                .try_collect()?;
            (
                Symbol::new(var.as_str(), var.extract_span()),
                Some((
                    parse_aggr(aggr_name)
                        .ok_or_else(|| AggrNotFound(aggr_name.to_string(), aggr_p.extract_span()))?
                        .clone(),
                    args,
                )),
            )
        }
        _ => unreachable!(),
    })
}

#[derive(Debug, Error, Diagnostic)]
#[error("bad specification of validity")]
#[diagnostic(code(parser::bad_validity_spec))]
struct BadValiditySpecification(#[label] SourceSpan);

pub(super) fn parse_fixed_rule(
    src: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
    fixed_rules: &BTreeMap<String, Arc<Box<dyn FixedRule>>>,
    cur_vld: ValidityTs,
) -> Result<(Symbol, FixedRuleApply)> {
    let mut src = src.into_inner();
    let (out_symbol, head, aggr) = parse_rule_head(src.next().unwrap(), param_pool)?;

    #[derive(Debug, Error, Diagnostic)]
    #[error("fixed rule cannot be combined with aggregation")]
    #[diagnostic(code(parser::fixed_aggr_conflict))]
    struct AggrInfixedError(#[label] SourceSpan);

    #[derive(Debug, Error, Diagnostic)]
    #[error("fixed rule cannot have duplicate bindings")]
    #[diagnostic(code(parser::duplicate_bindings_for_fixed_rule))]
    struct DuplicateBindingError(#[label] SourceSpan);

    for (a, v) in aggr.iter().zip(head.iter()) {
        ensure!(a.is_none(), AggrInfixedError(v.span))
    }

    let mut seen_bindings = BTreeSet::new();
    let mut binding_gen_id = 0;

    let name_pair = src.next().unwrap();
    let fixed_name = &name_pair.as_str();
    let mut rule_args: Vec<FixedRuleArg> = vec![];
    let mut options: BTreeMap<SmartString<LazyCompact>, Expr> = Default::default();
    let args_list = src.next().unwrap();
    let args_list_span = args_list.extract_span();

    for nxt in args_list.into_inner() {
        match nxt.as_rule() {
            Rule::fixed_rel => {
                let inner = nxt.into_inner().next().unwrap();
                let span = inner.extract_span();
                match inner.as_rule() {
                    Rule::fixed_rule_rel => {
                        let mut els = inner.into_inner();
                        let name = els.next().unwrap();
                        let mut bindings = Vec::with_capacity(els.size_hint().1.unwrap_or(4));
                        for v in els {
                            let s = v.as_str();
                            if s == "_" {
                                let symb =
                                    Symbol::new(format!("*_*{binding_gen_id}"), v.extract_span());
                                binding_gen_id += 1;
                                bindings.push(symb);
                            } else {
                                if !seen_bindings.insert(s) {
                                    bail!(DuplicateBindingError(v.extract_span()))
                                }
                                let symb = Symbol::new(s, v.extract_span());
                                bindings.push(symb);
                            }
                        }
                        rule_args.push(FixedRuleArg::InMem {
                            name: Symbol::new(name.as_str(), name.extract_span()),
                            bindings,
                            span,
                        })
                    }
                    Rule::fixed_relation_rel => {
                        let mut els = inner.into_inner();
                        let name = els.next().unwrap();
                        let mut bindings = vec![];
                        let mut valid_at = None;
                        for v in els {
                            match v.as_rule() {
                                Rule::var => {
                                    let s = v.as_str();
                                    if s == "_" {
                                        let symb = Symbol::new(
                                            format!("*_*{binding_gen_id}"),
                                            v.extract_span(),
                                        );
                                        binding_gen_id += 1;
                                        bindings.push(symb);
                                    } else {
                                        if !seen_bindings.insert(s) {
                                            bail!(DuplicateBindingError(v.extract_span()))
                                        }
                                        bindings.push(Symbol::new(v.as_str(), v.extract_span()))
                                    }
                                }
                                Rule::validity_clause => {
                                    let vld_inner = v.into_inner().next().unwrap();
                                    let vld_expr = build_expr(vld_inner, param_pool)?;
                                    valid_at = Some(expr2vld_spec(vld_expr, cur_vld)?)
                                }
                                _ => unreachable!(),
                            }
                        }
                        rule_args.push(FixedRuleArg::Stored {
                            name: Symbol::new(
                                name.as_str().strip_prefix('*').unwrap(),
                                name.extract_span(),
                            ),
                            bindings,
                            valid_at,
                            span,
                        })
                    }
                    Rule::fixed_named_relation_rel => {
                        let mut els = inner.into_inner();
                        let name = els.next().unwrap();
                        let mut bindings = BTreeMap::new();
                        let mut valid_at = None;
                        for p in els {
                            match p.as_rule() {
                                Rule::fixed_named_relation_arg_pair => {
                                    let mut vs = p.into_inner();
                                    let kp = vs.next().unwrap();
                                    let k = SmartString::from(kp.as_str());
                                    let v = match vs.next() {
                                        Some(vp) => {
                                            if !seen_bindings.insert(vp.as_str()) {
                                                bail!(DuplicateBindingError(vp.extract_span()))
                                            }
                                            Symbol::new(vp.as_str(), vp.extract_span())
                                        }
                                        None => {
                                            if !seen_bindings.insert(kp.as_str()) {
                                                bail!(DuplicateBindingError(kp.extract_span()))
                                            }
                                            Symbol::new(k.clone(), kp.extract_span())
                                        }
                                    };
                                    bindings.insert(k, v);
                                }
                                Rule::validity_clause => {
                                    let vld_inner = p.into_inner().next().unwrap();
                                    let vld_expr = build_expr(vld_inner, param_pool)?;
                                    valid_at = Some(expr2vld_spec(vld_expr, cur_vld)?)
                                }
                                _ => unreachable!(),
                            }
                        }

                        rule_args.push(FixedRuleArg::NamedStored {
                            name: Symbol::new(
                                name.as_str().strip_prefix(':').unwrap(),
                                name.extract_span(),
                            ),
                            bindings,
                            valid_at,
                            span,
                        })
                    }
                    _ => unreachable!(),
                }
            }
            Rule::fixed_opt_pair => {
                let mut inner = nxt.into_inner();
                let name = inner.next().unwrap().as_str();
                let val = inner.next().unwrap();
                let val = build_expr(val, param_pool)?;
                options.insert(SmartString::from(name), val);
            }
            _ => unreachable!(),
        }
    }

    let fixed = FixedRuleHandle::new(fixed_name, name_pair.extract_span());

    let fixed_impl = fixed_rules
        .get(&fixed.name as &str)
        .ok_or_else(|| FixedRuleNotFoundError(fixed.name.to_string(), name_pair.extract_span()))?;
    fixed_impl.init_options(&mut options, args_list_span)?;
    let arity = fixed_impl.arity(&options, &head, name_pair.extract_span())?;

    ensure!(
        head.is_empty() || arity == head.len(),
        FixedRuleHeadArityMismatch(arity, head.len(), args_list_span)
    );

    Ok((
        out_symbol,
        FixedRuleApply {
            fixed_handle: fixed,
            rule_args,
            options: Arc::new(options),
            head,
            arity,
            span: args_list_span,
            fixed_impl: fixed_impl.clone(),
        },
    ))
}

#[derive(Debug, Error, Diagnostic)]
#[error("Fixed rule head arity mismatch")]
#[diagnostic(code(parser::fixed_rule_head_arity_mismatch))]
#[diagnostic(help("Expected arity: {0}, number of arguments given: {1}"))]
pub(super) struct FixedRuleHeadArityMismatch(
    pub(super) usize,
    pub(super) usize,
    #[label]
    pub(super) SourceSpan,
);

#[derive(Debug, Error, Diagnostic)]
#[error("Encountered empty row for constant rule")]
#[diagnostic(code(parser::const_rule_empty_row))]
pub(super) struct EmptyRowForConstRule(#[label] pub(super) SourceSpan);

pub(super) fn make_empty_const_rule(prog: &mut InputProgram, bindings: &[Symbol]) {
    let entry_symbol = Symbol::new(PROG_ENTRY, Default::default());
    let mut options = BTreeMap::new();
    options.insert(
        SmartString::from("data"),
        Expr::Const {
            val: DataValue::List(vec![]),
            span: Default::default(),
        },
    );
    prog.prog.insert(
        entry_symbol,
        InputInlineRulesOrFixed::Fixed {
            fixed: FixedRuleApply {
                fixed_handle: FixedRuleHandle {
                    name: Symbol::new("Constant", Default::default()),
                },
                rule_args: vec![],
                options: Arc::new(options),
                head: bindings.to_vec(),
                arity: bindings.len(),
                span: Default::default(),
                fixed_impl: Arc::new(Box::new(Constant)),
            },
        },
    );
}

pub(super) fn expr2vld_spec(expr: Expr, cur_vld: ValidityTs) -> Result<ValidityTs> {
    let vld_span = expr.span();
    match expr.eval_to_const()? {
        DataValue::Num(n) => {
            let microseconds = n.get_int().ok_or(BadValiditySpecification(vld_span))?;
            Ok(ValidityTs(Reverse(microseconds)))
        }
        DataValue::Str(s) => match &s as &str {
            "NOW" => Ok(cur_vld),
            "END" => Ok(MAX_VALIDITY_TS),
            s => Ok(str2vld(s).map_err(|_| BadValiditySpecification(vld_span))?),
        },
        _ => {
            bail!(BadValiditySpecification(vld_span))
        }
    }
}
