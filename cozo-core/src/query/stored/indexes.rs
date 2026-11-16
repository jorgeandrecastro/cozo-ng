/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;
use std::sync::Arc;

use itertools::Itertools;
use miette::{IntoDiagnostic, Result};
use pest::Parser;
use smartstring::{LazyCompact, SmartString};

use crate::data::expr::Bytecode;
use crate::data::value::DataValue;
use crate::fts::tokenizer::TextAnalyzer;
use crate::parse::expr::build_expr;
use crate::parse::{CozoScriptParser, Rule};
use crate::runtime::minhash_lsh::HashPermutations;
use crate::runtime::relation::RelationHandle;
use crate::runtime::transact::SessionTx;
use crate::StoreTx;

impl<'a> SessionTx<'a> {
    pub(super) fn put_in_fts(
        &mut self,
        rel_handle: &RelationHandle,
        stack: &mut Vec<DataValue>,
        processors: &BTreeMap<SmartString<LazyCompact>, (Arc<TextAnalyzer>, Vec<Bytecode>)>,
        new_kv: &[DataValue],
    ) -> Result<()> {
        for (k, (idx_handle, _)) in rel_handle.fts_indices.iter() {
            let (tokenizer, extractor) = processors.get(k).unwrap();
            self.put_fts_index_item(new_kv, extractor, stack, tokenizer, rel_handle, idx_handle)?;
        }
        Ok(())
    }

    pub(super) fn del_in_fts(
        &mut self,
        rel_handle: &RelationHandle,
        stack: &mut Vec<DataValue>,
        processors: &BTreeMap<SmartString<LazyCompact>, (Arc<TextAnalyzer>, Vec<Bytecode>)>,
        old_kv: &[DataValue],
    ) -> Result<()> {
        for (k, (idx_handle, _)) in rel_handle.fts_indices.iter() {
            let (tokenizer, extractor) = processors.get(k).unwrap();
            self.del_fts_index_item(old_kv, extractor, stack, tokenizer, rel_handle, idx_handle)?;
        }
        Ok(())
    }

    pub(super) fn put_in_lsh(
        &mut self,
        rel_handle: &RelationHandle,
        stack: &mut Vec<DataValue>,
        processors: &BTreeMap<SmartString<LazyCompact>, (Arc<TextAnalyzer>, Vec<Bytecode>)>,
        new_kv: &[DataValue],
        hash_perms_map: &BTreeMap<SmartString<LazyCompact>, HashPermutations>,
    ) -> Result<()> {
        for (k, (idx_handle, inv_idx_handle, manifest)) in rel_handle.lsh_indices.iter() {
            let (tokenizer, extractor) = processors.get(k).unwrap();
            self.put_lsh_index_item(
                new_kv,
                extractor,
                stack,
                tokenizer,
                rel_handle,
                idx_handle,
                inv_idx_handle,
                manifest,
                hash_perms_map.get(k).unwrap(),
            )?;
        }
        Ok(())
    }

    pub(super) fn del_in_lsh(
        &mut self,
        rel_handle: &RelationHandle,
        old_kv: &[DataValue],
    ) -> Result<()> {
        for (idx_handle, inv_idx_handle, _) in rel_handle.lsh_indices.values() {
            self.del_lsh_index_item(old_kv, None, idx_handle, inv_idx_handle)?;
        }
        Ok(())
    }

    pub(super) fn update_in_hnsw(
        &mut self,
        relation_store: &RelationHandle,
        stack: &mut Vec<DataValue>,
        hnsw_filters: &BTreeMap<SmartString<LazyCompact>, Vec<Bytecode>>,
        new_kv: &[DataValue],
    ) -> Result<()> {
        for (name, (idx_handle, idx_manifest)) in relation_store.hnsw_indices.iter() {
            let filter = hnsw_filters.get(name);
            self.hnsw_put(
                idx_manifest,
                relation_store,
                idx_handle,
                filter,
                stack,
                new_kv,
            )?;
        }
        Ok(())
    }

    pub(super) fn make_lsh_hash_perms(
        &self,
        relation_store: &RelationHandle,
    ) -> BTreeMap<SmartString<LazyCompact>, HashPermutations> {
        let mut perms = BTreeMap::new();
        for (name, (_, _, manifest)) in relation_store.lsh_indices.iter() {
            perms.insert(name.clone(), manifest.get_hash_perms());
        }
        perms
    }

    pub(super) fn make_fts_lsh_processors(
        &self,
        relation_store: &RelationHandle,
    ) -> Result<BTreeMap<SmartString<LazyCompact>, (Arc<TextAnalyzer>, Vec<Bytecode>)>> {
        let mut processors = BTreeMap::new();
        for (name, (_, manifest)) in relation_store.fts_indices.iter() {
            let tokenizer = self
                .tokenizers
                .get(name, &manifest.tokenizer, &manifest.filters)?;

            let parsed = CozoScriptParser::parse(Rule::expr, &manifest.extractor)
                .into_diagnostic()?
                .next()
                .unwrap();
            let mut code_expr = build_expr(parsed, &Default::default())?;
            let binding_map = relation_store.raw_binding_map();
            code_expr.fill_binding_indices(&binding_map)?;
            let extractor = code_expr.compile()?;
            processors.insert(name.clone(), (tokenizer, extractor));
        }
        for (name, (_, _, manifest)) in relation_store.lsh_indices.iter() {
            let tokenizer = self
                .tokenizers
                .get(name, &manifest.tokenizer, &manifest.filters)?;

            let parsed = CozoScriptParser::parse(Rule::expr, &manifest.extractor)
                .into_diagnostic()?
                .next()
                .unwrap();
            let mut code_expr = build_expr(parsed, &Default::default())?;
            let binding_map = relation_store.raw_binding_map();
            code_expr.fill_binding_indices(&binding_map)?;
            let extractor = code_expr.compile()?;
            processors.insert(name.clone(), (tokenizer, extractor));
        }
        Ok(processors)
    }

    pub(super) fn make_hnsw_filters(
        relation_store: &RelationHandle,
    ) -> Result<BTreeMap<SmartString<LazyCompact>, Vec<Bytecode>>> {
        let mut hnsw_filters = BTreeMap::new();
        for (name, (_, manifest)) in relation_store.hnsw_indices.iter() {
            if let Some(f_code) = &manifest.index_filter {
                let parsed = CozoScriptParser::parse(Rule::expr, f_code)
                    .into_diagnostic()?
                    .next()
                    .unwrap();
                let mut code_expr = build_expr(parsed, &Default::default())?;
                let binding_map = relation_store.raw_binding_map();
                code_expr.fill_binding_indices(&binding_map)?;
                hnsw_filters.insert(name.clone(), code_expr.compile()?);
            }
        }
        Ok(hnsw_filters)
    }
}

impl<'a> SessionTx<'a> {
    pub(super) fn update_in_index(
        &mut self,
        relation_store: &RelationHandle,
        new_kv: &[DataValue],
        old_kv: &[DataValue],
    ) -> Result<()> {
        for (idx_rel, idx_extractor) in relation_store.indices.values() {
            let idx_tup_old = idx_extractor
                .iter()
                .map(|i| old_kv[*i].clone())
                .collect_vec();
            let encoded_old = idx_rel.encode_key_for_store(&idx_tup_old, Default::default())?;
            self.store_tx.del(&encoded_old)?;

            let idx_tup_new = idx_extractor
                .iter()
                .map(|i| new_kv[*i].clone())
                .collect_vec();
            let encoded_new = idx_rel.encode_key_for_store(&idx_tup_new, Default::default())?;
            self.store_tx.put(&encoded_new, &[])?;
        }
        Ok(())
    }
}
