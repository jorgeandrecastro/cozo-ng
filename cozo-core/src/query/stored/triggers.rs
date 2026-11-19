/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/stored/triggers.rs

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use itertools::Itertools;
use miette::Result;
use smartstring::{LazyCompact, SmartString};

use crate::data::expr::Expr;
use crate::data::program::{FixedRuleApply, InputInlineRulesOrFixed, InputProgram};
use crate::data::symb::Symbol;
use crate::data::value::{DataValue, ValidityTs};
use crate::fixed_rule::utilities::constant::Constant;
use crate::fixed_rule::FixedRuleHandle;
use crate::parse::parse_script;
use crate::runtime::callback::{CallbackCollector, CallbackOp};
use crate::runtime::relation::RelationHandle;
use crate::runtime::transact::SessionTx;
use crate::storage::Storage;
use crate::{Db, NamedRows};

impl<'a> SessionTx<'a> {
    pub(super) fn collect_mutations<'s, S: Storage<'s>>(
        &mut self,
        db: &Db<S>,
        cur_vld: ValidityTs,
        callback_targets: &BTreeSet<SmartString<LazyCompact>>,
        callback_collector: &mut CallbackCollector,
        propagate_triggers: bool,
        to_clear: &mut Vec<(Vec<u8>, Vec<u8>)>,
        relation_store: &RelationHandle,
        is_callback_target: bool,
        new_tuples: Vec<DataValue>,
        old_tuples: Vec<DataValue>,
    ) -> Result<()> {
        let mut bindings = relation_store
            .metadata
            .keys
            .iter()
            .map(|k| Symbol::new(k.name.clone(), Default::default()))
            .collect_vec();
        let v_bindings = relation_store
            .metadata
            .non_keys
            .iter()
            .map(|k| Symbol::new(k.name.clone(), Default::default()));
        bindings.extend(v_bindings);

        let kv_bindings = bindings;
        if propagate_triggers {
            for trigger in &relation_store.put_triggers {
                let mut program = parse_script(
                    trigger,
                    &Default::default(),
                    &db.fixed_rules.read().unwrap(),
                    cur_vld,
                )?
                .get_single_program()?;

                make_const_rule(
                    &mut program,
                    "_new",
                    kv_bindings.clone(),
                    new_tuples.to_vec(),
                );
                make_const_rule(
                    &mut program,
                    "_old",
                    kv_bindings.clone(),
                    old_tuples.to_vec(),
                );

                let (_, cleanups) = db
                    .run_query(
                        self,
                        program,
                        cur_vld,
                        callback_targets,
                        callback_collector,
                        false,
                    )
                    .map_err(|err| {
                        if err.source_code().is_some() {
                            err
                        } else {
                            err.with_source_code(format!("{trigger} "))
                        }
                    })?;
                to_clear.extend(cleanups);
            }
        }

        if is_callback_target {
            let target_collector = callback_collector
                .entry(relation_store.name.clone())
                .or_default();
            let headers = kv_bindings
                .into_iter()
                .map(|k| k.name.to_string())
                .collect_vec();
            target_collector.push((
                CallbackOp::Put,
                NamedRows::new(
                    headers.clone(),
                    new_tuples
                        .into_iter()
                        .map(|v| match v {
                            DataValue::List(l) => l,
                            _ => unreachable!(),
                        })
                        .collect_vec(),
                ),
                NamedRows::new(
                    headers,
                    old_tuples
                        .into_iter()
                        .map(|v| match v {
                            DataValue::List(l) => l,
                            _ => unreachable!(),
                        })
                        .collect_vec(),
                ),
            ))
        }
        Ok(())
    }
}

pub(super) fn make_const_rule(
    program: &mut InputProgram,
    rule_name: &str,
    bindings: Vec<Symbol>,
    data: Vec<DataValue>,
) {
    let rule_symbol = Symbol::new(SmartString::from(rule_name), Default::default());
    let mut options = BTreeMap::new();
    options.insert(
        SmartString::from("data"),
        Expr::Const {
            val: DataValue::List(data),
            span: Default::default(),
        },
    );
    let bindings_arity = bindings.len();
    program.prog.insert(
        rule_symbol,
        InputInlineRulesOrFixed::Fixed {
            fixed: FixedRuleApply {
                fixed_handle: FixedRuleHandle {
                    name: Symbol::new("Constant", Default::default()),
                },
                rule_args: vec![],
                options: Arc::new(options),
                head: bindings,
                arity: bindings_arity,
                span: Default::default(),
                fixed_impl: Arc::new(Box::new(Constant)),
            },
        },
    );
}
