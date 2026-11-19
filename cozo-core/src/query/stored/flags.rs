/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/src/query/stored/flags.rs

use std::collections::BTreeSet;

use miette::{bail, Result};
use smartstring::{LazyCompact, SmartString};

use crate::runtime::relation::{AccessLevel, InsufficientAccessLevel, RelationHandle};

/// Drapeaux calculés une seule fois par opération d'écriture
/// (put / update / remove), au lieu d'être recopiés dans chaque fonction.
pub(super) struct WriteFlags {
    pub is_callback_target: bool,
    pub need_to_collect: bool,
    pub has_indices: bool,
    pub has_hnsw: bool,
    pub has_fts: bool,
    pub has_lsh: bool,
    /// Vrai si l'on doit lire l'ancienne valeur et maintenir
    /// des index, des triggers ou des callbacks.
    pub needs_lookup: bool,
}

impl WriteFlags {
    pub(super) fn new(
        rel: &RelationHandle,
        callback_targets: &BTreeSet<SmartString<LazyCompact>>,
        force_collect: &str,
        propagate_triggers: bool,
        has_triggers: bool,
        action: &str,
    ) -> Result<Self> {
        if rel.access_level < AccessLevel::Protected {
            bail!(InsufficientAccessLevel(
                rel.name.to_string(),
                action.to_string(),
                rel.access_level
            ));
        }
        let is_callback_target = callback_targets.contains(&rel.name) || force_collect == rel.name;
        let need_to_collect = !force_collect.is_empty()
            || (!rel.is_temp && (is_callback_target || (propagate_triggers && has_triggers)));
        let has_indices = !rel.indices.is_empty();
        let has_hnsw = !rel.hnsw_indices.is_empty();
        let has_fts = !rel.fts_indices.is_empty();
        let has_lsh = !rel.lsh_indices.is_empty();
        Ok(Self {
            is_callback_target,
            need_to_collect,
            has_indices,
            has_hnsw,
            has_fts,
            has_lsh,
            needs_lookup: need_to_collect || has_indices || has_hnsw || has_fts || has_lsh,
        })
    }
}