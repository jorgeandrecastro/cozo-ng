/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;

use itertools::Itertools;
use miette::{ensure, Diagnostic, Result};
use smartstring::{LazyCompact, SmartString};
use thiserror::Error;

use crate::data::expr::Expr;
use crate::data::program::{
    InputAtom, InputInlineRule, InputNamedFieldRelationApplyAtom, InputRelationApplyAtom,
    InputRuleApplyAtom, SearchInput, Unification,
};
use crate::data::symb::Symbol;
use crate::data::value::{DataValue, ValidityTs};
use crate::parse::expr::build_expr;
use crate::parse::{ExtractSpan, Pair, Rule, SourceSpan};

use super::rules::{expr2vld_spec, parse_rule_head};

pub(super) fn parse_rule(
    src: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
    cur_vld: ValidityTs,
) -> Result<(Symbol, InputInlineRule)> {
    let span = src.extract_span();
    let mut src = src.into_inner();
    let head = src.next().unwrap();
    let head_span = head.extract_span();
    let (name, head, aggr) = parse_rule_head(head, param_pool)?;

    #[derive(Debug, Error, Diagnostic)]
    #[error("Horn-clause rule cannot have empty rule head")]
    #[diagnostic(code(parser::empty_horn_rule_head))]
    struct EmptyRuleHead(#[label] SourceSpan);

    ensure!(!head.is_empty(), EmptyRuleHead(head_span));
    let body = src.next().unwrap();
    let mut body_clauses = vec![];
    let mut ignored_counter = 0;
    for atom_src in body.into_inner() {
        body_clauses.push(parse_disjunction(
            atom_src,
            param_pool,
            cur_vld,
            &mut ignored_counter,
        )?)
    }

    Ok((
        name,
        InputInlineRule {
            head,
            aggr,
            body: body_clauses,
            span,
        },
    ))
}

fn parse_disjunction(
    pair: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
    cur_vld: ValidityTs,
    ignored_counter: &mut u32,
) -> Result<InputAtom> {
    let span = pair.extract_span();
    let res: Vec<_> = pair
        .into_inner()
        .filter_map(|v| match v.as_rule() {
            Rule::or_op => None,
            _ => Some(parse_atom(v, param_pool, cur_vld, ignored_counter)),
        })
        .try_collect()?;
    Ok(if res.len() == 1 {
        res.into_iter().next().unwrap()
    } else {
        InputAtom::Disjunction { inner: res, span }
    })
}

fn parse_atom(
    src: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
    cur_vld: ValidityTs,
    ignored_counter: &mut u32,
) -> Result<InputAtom> {
    Ok(match src.as_rule() {
        Rule::rule_body => {
            let span = src.extract_span();
            let grouped: Vec<_> = src
                .into_inner()
                .map(|v| parse_disjunction(v, param_pool, cur_vld, ignored_counter))
                .try_collect()?;
            InputAtom::Conjunction {
                inner: grouped,
                span,
            }
        }
        Rule::disjunction => parse_disjunction(src, param_pool, cur_vld, ignored_counter)?,
        Rule::negation => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            src.next().unwrap();
            let inner = parse_atom(src.next().unwrap(), param_pool, cur_vld, ignored_counter)?;
            InputAtom::Negation {
                inner: inner.into(),
                span,
            }
        }
        Rule::expr => {
            let expr = build_expr(src, param_pool)?;
            InputAtom::Predicate { inner: expr }
        }
        Rule::unify => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            let var = src.next().unwrap();
            let mut symb = Symbol::new(var.as_str(), var.extract_span());
            if symb.is_ignored_symbol() {
                symb.name = format!("*^*{}", *ignored_counter).into();
                *ignored_counter += 1;
            }
            let expr = build_expr(src.next().unwrap(), param_pool)?;
            InputAtom::Unification {
                inner: Unification {
                    binding: symb,
                    expr,
                    one_many_unif: false,
                    span,
                },
            }
        }
        Rule::unify_multi => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            let var = src.next().unwrap();
            let mut symb = Symbol::new(var.as_str(), var.extract_span());
            if symb.is_ignored_symbol() {
                symb.name = format!("*^*{}", *ignored_counter).into();
                *ignored_counter += 1;
            }
            src.next().unwrap();
            let expr = build_expr(src.next().unwrap(), param_pool)?;
            InputAtom::Unification {
                inner: Unification {
                    binding: symb,
                    expr,
                    one_many_unif: true,
                    span,
                },
            }
        }
        Rule::rule_apply => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            let name = src.next().unwrap();
            let args: Vec<_> = src
                .next()
                .unwrap()
                .into_inner()
                .map(|v| build_expr(v, param_pool))
                .try_collect()?;
            InputAtom::Rule {
                inner: InputRuleApplyAtom {
                    name: Symbol::new(name.as_str(), name.extract_span()),
                    args,
                    span,
                },
            }
        }
        Rule::relation_apply => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            let name = src.next().unwrap();
            let args: Vec<_> = src
                .next()
                .unwrap()
                .into_inner()
                .map(|v| build_expr(v, param_pool))
                .try_collect()?;
            let valid_at = match src.next() {
                None => None,
                Some(vld_clause) => {
                    let vld_expr = build_expr(vld_clause.into_inner().next().unwrap(), param_pool)?;
                    Some(expr2vld_spec(vld_expr, cur_vld)?)
                }
            };
            InputAtom::Relation {
                inner: InputRelationApplyAtom {
                    name: Symbol::new(&name.as_str()[1..], name.extract_span()),
                    args,
                    valid_at,
                    span,
                },
            }
        }
        Rule::search_apply => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            let name_p = src.next().unwrap();
            let name_segs = name_p.as_str().split(':').collect_vec();

            #[derive(Debug, Error, Diagnostic)]
            #[error("Search head must be of the form `relation_name:index_name`")]
            #[diagnostic(code(parser::invalid_search_head))]
            struct InvalidSearchHead(#[label] SourceSpan);

            ensure!(
                name_segs.len() == 2,
                InvalidSearchHead(name_p.extract_span())
            );
            let relation = Symbol::new(name_segs[0], name_p.extract_span());
            let index = Symbol::new(name_segs[1], name_p.extract_span());
            let bindings: BTreeMap<SmartString<LazyCompact>, Expr> = src
                .next()
                .unwrap()
                .into_inner()
                .map(|arg| extract_named_apply_arg(arg, param_pool))
                .try_collect()?;
            let parameters: BTreeMap<SmartString<LazyCompact>, Expr> = src
                .map(|arg| extract_named_apply_arg(arg, param_pool))
                .try_collect()?;

            let opts = SearchInput {
                relation,
                index,
                bindings,
                span,
                parameters,
            };

            InputAtom::Search { inner: opts }
        }
        Rule::relation_named_apply => {
            let span = src.extract_span();
            let mut src = src.into_inner();
            let name_p = src.next().unwrap();
            let name = Symbol::new(&name_p.as_str()[1..], name_p.extract_span());
            let args = src
                .next()
                .unwrap()
                .into_inner()
                .map(|arg| extract_named_apply_arg(arg, param_pool))
                .try_collect()?;
            let valid_at = match src.next() {
                None => None,
                Some(vld_clause) => {
                    let vld_expr = build_expr(vld_clause.into_inner().next().unwrap(), param_pool)?;
                    Some(expr2vld_spec(vld_expr, cur_vld)?)
                }
            };
            InputAtom::NamedFieldRelation {
                inner: InputNamedFieldRelationApplyAtom {
                    name,
                    args,
                    span,
                    valid_at,
                },
            }
        }
        r => unreachable!("{:?}", r),
    })
}

fn extract_named_apply_arg(
    pair: Pair<'_>,
    param_pool: &BTreeMap<String, DataValue>,
) -> Result<(SmartString<LazyCompact>, Expr)> {
    let mut inner = pair.into_inner();
    let name_p = inner.next().unwrap();
    let name = SmartString::from(name_p.as_str());
    let arg = match inner.next() {
        Some(a) => build_expr(a, param_pool)?,
        None => Expr::Binding {
            var: Symbol::new(name.clone(), name_p.extract_span()),
            tuple_pos: None,
        },
    };
    Ok((name, arg))
}

