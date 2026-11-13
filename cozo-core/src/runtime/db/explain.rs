/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use itertools::Itertools;
use miette::Result;
use serde_json::json;

use crate::data::json::JsonValue;
use crate::data::value::DataValue;
use crate::query::compile::{CompiledProgram, CompiledRule, CompiledRuleSet};
use crate::query::ra::{
    FilteredRA, FtsSearchRA, HnswSearchRA, InnerJoin, LshSearchRA, NegJoin, RelAlgebra, ReorderRA,
    StoredRA, StoredWithValidityRA, TempStoreRA, UnificationRA,
};
use crate::storage::Storage;

use super::{Db, NamedRows};

impl<'s, S: Storage<'s>> Db<S> {
    pub(super) fn explain_compiled(&self, strata: &[CompiledProgram]) -> Result<NamedRows> {
        let mut ret: Vec<JsonValue> = vec![];
        const STRATUM: &str = "stratum";
        const ATOM_IDX: &str = "atom_idx";
        const OP: &str = "op";
        const RULE_IDX: &str = "rule_idx";
        const RULE_NAME: &str = "rule";
        const REF_NAME: &str = "ref";
        const OUT_BINDINGS: &str = "out_relation";
        const JOINS_ON: &str = "joins_on";
        const FILTERS: &str = "filters/expr";

        let headers = vec![
            STRATUM.to_string(),
            RULE_IDX.to_string(),
            RULE_NAME.to_string(),
            ATOM_IDX.to_string(),
            OP.to_string(),
            REF_NAME.to_string(),
            JOINS_ON.to_string(),
            FILTERS.to_string(),
            OUT_BINDINGS.to_string(),
        ];

        for (stratum, p) in strata.iter().enumerate() {
            let mut clause_idx = -1;
            for (rule_name, v) in p {
                match v {
                    CompiledRuleSet::Rules(rules) => {
                        for CompiledRule { aggr, relation, .. } in rules.iter() {
                            clause_idx += 1;
                            let mut ret_for_relation = vec![];
                            let mut rel_stack = vec![relation];
                            let mut idx = 0;
                            let mut atom_type = "out";
                            for (a, _) in aggr.iter().flatten() {
                                if a.is_meet {
                                    if atom_type == "out" {
                                        atom_type = "meet_aggr_out";
                                    }
                                } else {
                                    atom_type = "aggr_out";
                                }
                            }

                            ret_for_relation.push(json!({
                                STRATUM: stratum,
                                ATOM_IDX: idx,
                                OP: atom_type,
                                RULE_IDX: clause_idx,
                                RULE_NAME: rule_name.to_string(),
                                OUT_BINDINGS: relation.bindings_after_eliminate().into_iter().map(|v| v.to_string()).collect_vec()
                            }));
                            idx += 1;

                            while let Some(rel) = rel_stack.pop() {
                                let (atom_type, ref_name, joins_on, filters) = match rel {
                                    r @ RelAlgebra::Fixed(..) => {
                                        if r.is_unit() {
                                            continue;
                                        }
                                        ("fixed", json!(null), json!(null), json!(null))
                                    }
                                    RelAlgebra::TempStore(TempStoreRA {
                                        storage_key,
                                        filters,
                                        ..
                                    }) => (
                                        "load_mem",
                                        json!(storage_key.to_string()),
                                        json!(null),
                                        json!(filters.iter().map(|f| f.to_string()).collect_vec()),
                                    ),
                                    RelAlgebra::Stored(StoredRA {
                                        storage, filters, ..
                                    }) => (
                                        "load_stored",
                                        json!(format!(":{}", storage.name)),
                                        json!(null),
                                        json!(filters.iter().map(|f| f.to_string()).collect_vec()),
                                    ),
                                    RelAlgebra::StoredWithValidity(StoredWithValidityRA {
                                        storage,
                                        filters,
                                        ..
                                    }) => (
                                        "load_stored_with_validity",
                                        json!(format!(":{}", storage.name)),
                                        json!(null),
                                        json!(filters.iter().map(|f| f.to_string()).collect_vec()),
                                    ),
                                    RelAlgebra::Join(inner) => {
                                        if inner.left.is_unit() {
                                            rel_stack.push(&inner.right);
                                            continue;
                                        }
                                        let t = inner.join_type();
                                        let InnerJoin {
                                            left,
                                            right,
                                            joiner,
                                            ..
                                        } = inner.as_ref();
                                        rel_stack.push(left);
                                        rel_stack.push(right);
                                        (t, json!(null), json!(joiner.as_map()), json!(null))
                                    }
                                    RelAlgebra::NegJoin(inner) => {
                                        let t = inner.join_type();
                                        let NegJoin {
                                            left,
                                            right,
                                            joiner,
                                            ..
                                        } = inner.as_ref();
                                        rel_stack.push(left);
                                        rel_stack.push(right);
                                        (t, json!(null), json!(joiner.as_map()), json!(null))
                                    }
                                    RelAlgebra::Reorder(ReorderRA { relation, .. }) => {
                                        rel_stack.push(relation);
                                        ("reorder", json!(null), json!(null), json!(null))
                                    }
                                    RelAlgebra::Filter(FilteredRA {
                                        parent,
                                        filters: pred,
                                        ..
                                    }) => {
                                        rel_stack.push(parent);
                                        (
                                            "filter",
                                            json!(null),
                                            json!(null),
                                            json!(pred.iter().map(|f| f.to_string()).collect_vec()),
                                        )
                                    }
                                    RelAlgebra::Unification(UnificationRA {
                                        parent,
                                        binding,
                                        expr,
                                        is_multi,
                                        ..
                                    }) => {
                                        rel_stack.push(parent);
                                        (
                                            if *is_multi { "multi-unify" } else { "unify" },
                                            json!(binding.name),
                                            json!(null),
                                            json!(expr.to_string()),
                                        )
                                    }
                                    RelAlgebra::HnswSearch(HnswSearchRA {
                                        hnsw_search, ..
                                    }) => (
                                        "hnsw_index",
                                        json!(format!(":{}", hnsw_search.query.name)),
                                        json!(hnsw_search.query.name),
                                        json!(hnsw_search
                                            .filter
                                            .iter()
                                            .map(|f| f.to_string())
                                            .collect_vec()),
                                    ),
                                    RelAlgebra::FtsSearch(FtsSearchRA { fts_search, .. }) => (
                                        "fts_index",
                                        json!(format!(":{}", fts_search.query.name)),
                                        json!(fts_search.query.name),
                                        json!(fts_search
                                            .filter
                                            .iter()
                                            .map(|f| f.to_string())
                                            .collect_vec()),
                                    ),
                                    RelAlgebra::LshSearch(LshSearchRA { lsh_search, .. }) => (
                                        "lsh_index",
                                        json!(format!(":{}", lsh_search.query.name)),
                                        json!(lsh_search.query.name),
                                        json!(lsh_search
                                            .filter
                                            .iter()
                                            .map(|f| f.to_string())
                                            .collect_vec()),
                                    ),
                                };
                                ret_for_relation.push(json!({
                                    STRATUM: stratum,
                                    ATOM_IDX: idx,
                                    OP: atom_type,
                                    RULE_IDX: clause_idx,
                                    RULE_NAME: rule_name.to_string(),
                                    REF_NAME: ref_name,
                                    OUT_BINDINGS: rel.bindings_after_eliminate().into_iter().map(|v| v.to_string()).collect_vec(),
                                    JOINS_ON: joins_on,
                                    FILTERS: filters,
                                }));
                                idx += 1;
                            }
                            ret_for_relation.reverse();
                            ret.extend(ret_for_relation)
                        }
                    }
                    CompiledRuleSet::Fixed(_) => ret.push(json!({
                        STRATUM: stratum,
                        ATOM_IDX: 0,
                        OP: "algo",
                        RULE_IDX: 0,
                        RULE_NAME: rule_name.to_string(),
                    })),
                }
            }
        }

        let rows = ret
            .into_iter()
            .map(|m| {
                headers
                    .iter()
                    .map(|i| DataValue::from(m.get(i).unwrap_or(&JsonValue::Null)))
                    .collect_vec()
            })
            .collect_vec();

        Ok(NamedRows::new(headers, rows))
    }
}