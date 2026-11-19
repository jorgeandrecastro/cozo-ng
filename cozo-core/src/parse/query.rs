/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/parse/query.rs

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use either::{Left, Right};
use itertools::Itertools;
use miette::{bail, ensure, Diagnostic, LabeledSpan, Report, Result};
use pest::Parser;
use smartstring::SmartString;
use thiserror::Error;

use crate::data::program::{
    FixedRuleApply, InputInlineRulesOrFixed, InputProgram, QueryAssertion, QueryOutOptions,
    RelationOp, ReturnMutation, SortDir,
};
use crate::data::relation::{ColType, ColumnDef, NullableColType, StoredRelationMetadata};
use crate::data::symb::Symbol;
use crate::data::value::{DataValue, ValidityTs};
use crate::fixed_rule::utilities::constant::Constant;
use crate::fixed_rule::FixedRuleHandle;
use crate::parse::expr::build_expr;
use crate::parse::schema::parse_schema;
use crate::parse::{CozoScriptParser, ExtractSpan, Pairs, Rule, SourceSpan};
use crate::runtime::relation::InputRelationHandle;
use crate::FixedRule;

mod atoms;
mod rules;

use self::atoms::parse_rule;
use self::rules::{
    make_empty_const_rule, parse_fixed_rule, parse_rule_head, EmptyRowForConstRule,
    FixedRuleHeadArityMismatch,
};

#[derive(Error, Diagnostic, Debug)]
#[error("Query option {0} is not constant")]
#[diagnostic(code(parser::option_not_constant))]
struct OptionNotConstantError(&'static str, #[label] SourceSpan, #[related] [Report; 1]);

#[derive(Error, Diagnostic, Debug)]
#[error("Query option {0} requires a non-negative integer")]
#[diagnostic(code(parser::option_not_non_neg))]
struct OptionNotNonNegIntError(&'static str, #[label] SourceSpan);

#[derive(Error, Diagnostic, Debug)]
#[error("Query option {0} requires a positive integer")]
#[diagnostic(code(parser::option_not_pos))]
struct OptionNotPosIntError(&'static str, #[label] SourceSpan);

#[derive(Error, Diagnostic, Debug)]
#[error("Query option {0} requires a boolean")]
#[diagnostic(code(parser::option_not_bool))]
struct OptionNotBoolError(&'static str, #[label] SourceSpan);

#[derive(Debug)]
struct MultipleRuleDefinitionError(String, Vec<SourceSpan>);

#[derive(Debug, Error, Diagnostic)]
#[error("Multiple query output assertions defined")]
#[diagnostic(code(parser::multiple_out_assert))]
struct DuplicateQueryAssertion(#[label] SourceSpan);

#[derive(Debug, Error, Diagnostic)]
#[error("Multiple query yields defined")]
#[diagnostic(code(parser::multiple_yields))]
struct DuplicateYield(#[label] SourceSpan);

impl Error for MultipleRuleDefinitionError {}

impl Display for MultipleRuleDefinitionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The rule '{0}' cannot have multiple definitions since it contains non-Horn clauses",
            self.0
        )
    }
}

impl Diagnostic for MultipleRuleDefinitionError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("parser::mult_rule_def"))
    }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(
            self.1.iter().map(|s| LabeledSpan::new_with_span(None, s)),
        ))
    }
}

fn merge_spans(symbs: &[Symbol]) -> SourceSpan {
    let mut fst = symbs.first().unwrap().span;
    for nxt in symbs.iter().skip(1) {
        fst = fst.merge(nxt.span);
    }
    fst
}

pub(crate) fn parse_query(
    src: Pairs<'_>,
    param_pool: &BTreeMap<String, DataValue>,
    fixed_rules: &BTreeMap<String, Arc<Box<dyn FixedRule>>>,
    cur_vld: ValidityTs,
) -> Result<InputProgram> {
    let mut progs: BTreeMap<Symbol, InputInlineRulesOrFixed> = Default::default();
    let mut out_opts: QueryOutOptions = Default::default();
    let mut disable_magic_rewrite = false;

    let mut stored_relation = None;
    let mut returning_mutation = ReturnMutation::NotReturning;

    for pair in src {
        match pair.as_rule() {
            Rule::rule => {
                let (name, rule) = parse_rule(pair, param_pool, cur_vld)?;

                match progs.entry(name) {
                    Entry::Vacant(e) => {
                        e.insert(InputInlineRulesOrFixed::Rules { rules: vec![rule] });
                    }
                    Entry::Occupied(mut e) => {
                        let key = e.key().to_string();
                        match e.get_mut() {
                            InputInlineRulesOrFixed::Rules { rules: rs } => {
                                #[derive(Debug, Error, Diagnostic)]
                                #[error("Rule {0} has multiple definitions with conflicting heads")]
                                #[diagnostic(code(parser::head_aggr_mismatch))]
                                #[diagnostic(help("The arity of each rule head must match. In addition, any aggregation \
                            applied must be the same."))]
                                struct RuleHeadMismatch(
                                    String,
                                    #[label] SourceSpan,
                                    #[label] SourceSpan,
                                );
                                let prev = rs.first().unwrap();
                                ensure!(prev.aggr == rule.aggr, {
                                    RuleHeadMismatch(
                                        key,
                                        merge_spans(&prev.head),
                                        merge_spans(&rule.head),
                                    )
                                });

                                rs.push(rule);
                            }
                            InputInlineRulesOrFixed::Fixed { fixed } => {
                                let fixed_span = fixed.span;
                                bail!(MultipleRuleDefinitionError(
                                    e.key().name.to_string(),
                                    vec![rule.span, fixed_span]
                                ))
                            }
                        }
                    }
                }
            }
            Rule::fixed_rule => {
                let rule_span = pair.extract_span();
                let (name, apply) = parse_fixed_rule(pair, param_pool, fixed_rules, cur_vld)?;

                match progs.entry(name) {
                    Entry::Vacant(e) => {
                        e.insert(InputInlineRulesOrFixed::Fixed { fixed: apply });
                    }
                    Entry::Occupied(e) => {
                        let found_name = e.key().name.to_string();
                        let mut found_span = match e.get() {
                            InputInlineRulesOrFixed::Rules { rules } => {
                                rules.iter().map(|r| r.span).collect_vec()
                            }
                            InputInlineRulesOrFixed::Fixed { fixed } => vec![fixed.span],
                        };
                        found_span.push(rule_span);
                        bail!(MultipleRuleDefinitionError(found_name, found_span));
                    }
                }
            }
            Rule::const_rule => {
                let span = pair.extract_span();
                let mut src = pair.into_inner();
                let (name, mut head, aggr) = parse_rule_head(src.next().unwrap(), param_pool)?;

                if let Some(found) = progs.get(&name) {
                    let mut found_span = match found {
                        InputInlineRulesOrFixed::Rules { rules } => {
                            rules.iter().map(|r| r.span).collect_vec()
                        }
                        InputInlineRulesOrFixed::Fixed { fixed } => {
                            vec![fixed.span]
                        }
                    };
                    found_span.push(span);
                    bail!(MultipleRuleDefinitionError(
                        name.name.to_string(),
                        found_span
                    ));
                }

                #[derive(Debug, Error, Diagnostic)]
                #[error("Constant rules cannot have aggregation application")]
                #[diagnostic(code(parser::aggr_in_const_rule))]
                struct AggrInConstRuleError(#[label] SourceSpan);

                for (a, v) in aggr.iter().zip(head.iter()) {
                    ensure!(a.is_none(), AggrInConstRuleError(v.span));
                }
                let data_part = src.next().unwrap();
                let data_part_str = data_part.as_str();
                let data = build_expr(data_part.clone(), param_pool)?;
                let mut options = BTreeMap::new();
                options.insert(SmartString::from("data"), data);
                let handle = FixedRuleHandle {
                    name: Symbol::new("Constant", span),
                };
                let fixed_impl = Box::new(Constant);
                fixed_impl.init_options(&mut options, span)?;
                let arity = fixed_impl.arity(&options, &head, span)?;

                ensure!(arity != 0, EmptyRowForConstRule(span));
                ensure!(
                    head.is_empty() || arity == head.len(),
                    FixedRuleHeadArityMismatch(arity, head.len(), span)
                );
                if head.is_empty() && name.is_prog_entry() {
                    if let Ok(mut datalist) =
                        CozoScriptParser::parse(Rule::param_list, data_part_str)
                    {
                        for s in datalist.next().unwrap().into_inner() {
                            if s.as_rule() == Rule::param {
                                head.push(Symbol::new(
                                    s.as_str().strip_prefix('$').unwrap(),
                                    Default::default(),
                                ));
                            }
                        }
                    }
                }
                progs.insert(
                    name,
                    InputInlineRulesOrFixed::Fixed {
                        fixed: FixedRuleApply {
                            fixed_handle: handle,
                            rule_args: vec![],
                            options: Arc::new(options),
                            head,
                            arity,
                            span,
                            fixed_impl: Arc::new(fixed_impl),
                        },
                    },
                );
            }
            Rule::timeout_option => {
                let pair = pair.into_inner().next().unwrap();
                let span = pair.extract_span();
                let timeout = build_expr(pair, param_pool)?
                    .eval_to_const()
                    .map_err(|err| OptionNotConstantError("timeout", span, [err]))?
                    .get_float()
                    .ok_or(OptionNotNonNegIntError("timeout", span))?;
                if timeout > 0. {
                    out_opts.timeout = Some(timeout);
                } else {
                    out_opts.timeout = None;
                }
            }
            Rule::sleep_option => {
                #[cfg(target_arch = "wasm32")]
                bail!(":sleep is not supported under WASM");

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let pair = pair.into_inner().next().unwrap();
                    let span = pair.extract_span();
                    let sleep = build_expr(pair, param_pool)?
                        .eval_to_const()
                        .map_err(|err| OptionNotConstantError("sleep", span, [err]))?
                        .get_float()
                        .ok_or(OptionNotNonNegIntError("sleep", span))?;
                    ensure!(sleep > 0., OptionNotPosIntError("sleep", span));
                    out_opts.sleep = Some(sleep);
                }
            }
            Rule::limit_option => {
                let pair = pair.into_inner().next().unwrap();
                let span = pair.extract_span();
                let limit = build_expr(pair, param_pool)?
                    .eval_to_const()
                    .map_err(|err| OptionNotConstantError("limit", span, [err]))?
                    .get_non_neg_int()
                    .ok_or(OptionNotNonNegIntError("limit", span))?;
                out_opts.limit = Some(limit as usize);
            }
            Rule::offset_option => {
                let pair = pair.into_inner().next().unwrap();
                let span = pair.extract_span();
                let offset = build_expr(pair, param_pool)?
                    .eval_to_const()
                    .map_err(|err| OptionNotConstantError("offset", span, [err]))?
                    .get_non_neg_int()
                    .ok_or(OptionNotNonNegIntError("offset", span))?;
                out_opts.offset = Some(offset as usize);
            }
            Rule::sort_option => {
                for part in pair.into_inner() {
                    let mut var = "";
                    let mut dir = SortDir::Asc;
                    let mut span = part.extract_span();
                    for a in part.into_inner() {
                        match a.as_rule() {
                            Rule::out_arg => {
                                var = a.as_str();
                                span = a.extract_span();
                            }
                            Rule::sort_asc => dir = SortDir::Asc,
                            Rule::sort_desc => dir = SortDir::Dsc,
                            _ => unreachable!(),
                        }
                    }
                    out_opts.sorters.push((Symbol::new(var, span), dir));
                }
            }
            Rule::returning_option => {
                returning_mutation = ReturnMutation::Returning;
            }
            Rule::relation_option => {
                let span = pair.extract_span();
                let mut args = pair.into_inner();
                let op = match args.next().unwrap().as_rule() {
                    Rule::relation_create => RelationOp::Create,
                    Rule::relation_replace => RelationOp::Replace,
                    Rule::relation_put => RelationOp::Put,
                    Rule::relation_insert => RelationOp::Insert,
                    Rule::relation_update => RelationOp::Update,
                    Rule::relation_rm => RelationOp::Rm,
                    Rule::relation_delete => RelationOp::Delete,
                    Rule::relation_ensure => RelationOp::Ensure,
                    Rule::relation_ensure_not => RelationOp::EnsureNot,
                    _ => unreachable!(),
                };

                let name_p = args.next().unwrap();
                let name = Symbol::new(name_p.as_str(), name_p.extract_span());
                match args.next() {
                    None => stored_relation = Some(Left((name, span, op))),
                    Some(schema_p) => {
                        let (mut metadata, mut key_bindings, mut dep_bindings) =
                            parse_schema(schema_p)?;
                        if !matches!(op, RelationOp::Create | RelationOp::Replace) {
                            key_bindings.extend(dep_bindings);
                            dep_bindings = vec![];
                            metadata.keys.extend(metadata.non_keys);
                            metadata.non_keys = vec![];
                        }
                        stored_relation = Some(Right((
                            InputRelationHandle {
                                name,
                                metadata,
                                key_bindings,
                                dep_bindings,
                                span,
                            },
                            op,
                        )))
                    }
                }
            }
            Rule::assert_none_option => {
                ensure!(
                    out_opts.assertion.is_none(),
                    DuplicateQueryAssertion(pair.extract_span())
                );
                out_opts.assertion = Some(QueryAssertion::AssertNone(pair.extract_span()))
            }
            Rule::assert_some_option => {
                ensure!(
                    out_opts.assertion.is_none(),
                    DuplicateQueryAssertion(pair.extract_span())
                );
                out_opts.assertion = Some(QueryAssertion::AssertSome(pair.extract_span()))
            }
            Rule::disable_magic_rewrite_option => {
                let pair = pair.into_inner().next().unwrap();
                let span = pair.extract_span();
                let val = build_expr(pair, param_pool)?
                    .eval_to_const()
                    .map_err(|err| OptionNotConstantError("disable_magic_rewrite", span, [err]))?
                    .get_bool()
                    .ok_or(OptionNotBoolError("disable_magic_rewrite", span))?;
                disable_magic_rewrite = val;
            }
            Rule::EOI => break,
            r => unreachable!("{:?}", r),
        }
    }

    let mut prog = InputProgram {
        prog: progs,
        out_opts,
        disable_magic_rewrite,
    };

    if prog.prog.is_empty() {
        if let Some((
            InputRelationHandle {
                key_bindings,
                dep_bindings,
                ..
            },
            RelationOp::Create,
            _,
        )) = &prog.out_opts.store_relation
        {
            let mut bindings = key_bindings.clone();
            bindings.extend_from_slice(dep_bindings);
            make_empty_const_rule(&mut prog, &bindings);
        }
    }

    // let head_arity = prog.get_entry_arity()?;

    match stored_relation {
        None => {}
        Some(Left((name, span, op))) => {
            let head = prog.get_entry_out_head()?;
            for symb in &head {
                symb.ensure_valid_field()?;
            }

            let metadata = StoredRelationMetadata {
                keys: head
                    .iter()
                    .map(|s| ColumnDef {
                        name: s.name.clone(),
                        typing: NullableColType {
                            coltype: ColType::Any,
                            nullable: true,
                        },
                        default_gen: None,
                    })
                    .collect(),
                non_keys: vec![],
            };

            let handle = InputRelationHandle {
                name,
                metadata,
                key_bindings: head,
                dep_bindings: vec![],
                span,
            };
            prog.out_opts.store_relation = Some((handle, op, returning_mutation))
        }
        Some(Right((h, o))) => prog.out_opts.store_relation = Some((h, o, returning_mutation)),
    }

    if prog.prog.is_empty() {
        if let Some((handle, RelationOp::Create, _)) = &prog.out_opts.store_relation {
            let mut bindings = handle.dep_bindings.clone();
            bindings.extend_from_slice(&handle.key_bindings);
            make_empty_const_rule(&mut prog, &bindings);
        }
    }

    if !prog.out_opts.sorters.is_empty() {
        #[derive(Debug, Error, Diagnostic)]
        #[error("Sort key '{0}' not found")]
        #[diagnostic(code(parser::sort_key_not_found))]
        struct SortKeyNotFound(String, #[label] SourceSpan);

        let head_args = prog.get_entry_out_head()?;

        for (sorter, _) in &prog.out_opts.sorters {
            ensure!(
                head_args.contains(sorter),
                SortKeyNotFound(sorter.to_string(), sorter.span)
            )
        }
    }

    #[derive(Debug, Error, Diagnostic)]
    #[error("Input relation '{0}' has no keys")]
    #[diagnostic(code(parser::relation_has_no_keys))]
    struct RelationHasNoKeys(String, #[label] SourceSpan);

    let empty_mutation_head = match &prog.out_opts.store_relation {
        None => false,
        Some((handle, _, _)) => {
            if handle.key_bindings.is_empty() {
                if handle.dep_bindings.is_empty() {
                    true
                } else {
                    bail!(RelationHasNoKeys(handle.name.to_string(), handle.span));
                }
            } else {
                false
            }
        }
    };

    if empty_mutation_head {
        let head_args = prog.get_entry_out_head()?;
        if let Some((handle, _, _)) = &mut prog.out_opts.store_relation {
            if head_args.is_empty() {
                bail!(RelationHasNoKeys(handle.name.to_string(), handle.span));
            }
            handle.key_bindings = head_args.clone();
            handle.metadata.keys = head_args
                .iter()
                .map(|s| ColumnDef {
                    name: s.name.clone(),
                    typing: NullableColType {
                        coltype: ColType::Any,
                        nullable: true,
                    },
                    default_gen: None,
                })
                .collect();
        } else {
            unreachable!()
        }
    }

    Ok(prog)
}
