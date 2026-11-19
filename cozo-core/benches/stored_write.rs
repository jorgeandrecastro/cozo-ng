/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// cozo-core/benches/stored_write.rs
//
// Benchmarks du chemin d'écriture des relations stockées
// (`query/stored/{write,remove}.rs`).
//
// Lancer avec :
//   cargo +nightly bench -p cozo-ng --features nightly-benchmarks --bench stored_write
//

#![feature(test)]

extern crate test;

use std::collections::BTreeMap;

use cozo::{DataValue, DbInstance, ScriptMutability};
use test::Bencher;

/// Nombre de lignes par itération (relations simples).
const ROWS: usize = 10_000;
/// Nombre de lignes par itération (HNSW, plus coûteux).
const HNSW_ROWS: usize = 1_000;
/// Dimension des embeddings (mettre 1536 pour simuler OpenAI).
const DIM: usize = 128;

fn new_db() -> DbInstance {
    DbInstance::new("mem", "", "").unwrap()
}

fn run(db: &DbInstance, script: &str, params: BTreeMap<String, DataValue>) {
    db.run_script(script, params, ScriptMutability::Mutable)
        .unwrap_or_else(|e| panic!("script failed: {script}\n{e:?}"));
}

fn params(rows: Vec<DataValue>) -> BTreeMap<String, DataValue> {
    let mut p = BTreeMap::new();
    p.insert("rows".to_string(), DataValue::List(rows));
    p
}

fn embedding(i: usize) -> DataValue {
    DataValue::List(
        (0..DIM)
            .map(|j| DataValue::from(((i * 31 + j) % 97) as f64 / 97.0))
            .collect(),
    )
}

/// Lignes (id, name, score)
fn plain_rows(n: usize, tag: &str) -> Vec<DataValue> {
    (0..n)
        .map(|i| {
            DataValue::List(vec![
                DataValue::from(i as i64),
                DataValue::from(format!("{tag}-{i}")),
                DataValue::from(i as f64 * 0.5),
            ])
        })
        .collect()
}

/// Lignes (id, name, embedding)
fn emb_rows(n: usize, tag: &str) -> Vec<DataValue> {
    (0..n)
        .map(|i| {
            DataValue::List(vec![
                DataValue::from(i as i64),
                DataValue::from(format!("{tag}-{i}")),
                embedding(i),
            ])
        })
        .collect()
}

/// Lignes (id, body) pour le full-text
fn doc_rows(n: usize, tag: &str) -> Vec<DataValue> {
    (0..n)
        .map(|i| {
            DataValue::List(vec![
                DataValue::from(i as i64),
                DataValue::from(format!("{tag} document number {i} about graphs and vectors")),
            ])
        })
        .collect()
}

// ---------------------------------------------------------------- put

#[bench]
fn put_plain(b: &mut Bencher) {
    let db = new_db();
    run(&db, ":create plain {id: Int => name: String, score: Float}", Default::default());
    let rows = plain_rows(ROWS, "a");
    b.iter(|| {
        run(
            &db,
            "?[id, name, score] <- $rows :put plain {id => name, score}",
            params(rows.clone()),
        );
    });
}

#[bench]
fn put_with_index(b: &mut Bencher) {
    let db = new_db();
    run(&db, ":create idx_rel {id: Int => name: String, score: Float}", Default::default());
    run(&db, "::index create idx_rel:by_name {name}", Default::default());
    let rows = plain_rows(ROWS, "a");
    b.iter(|| {
        run(
            &db,
            "?[id, name, score] <- $rows :put idx_rel {id => name, score}",
            params(rows.clone()),
        );
    });
}

#[bench]
fn put_overwrite_changing_value_with_index(b: &mut Bencher) {
    // Chaque itération alterne les valeurs : force le chemin « clé existante + valeur différente ».
    let db = new_db();
    run(&db, ":create idx_rel {id: Int => name: String, score: Float}", Default::default());
    run(&db, "::index create idx_rel:by_name {name}", Default::default());
    let rows_a = plain_rows(ROWS, "a");
    let rows_b = plain_rows(ROWS, "b");
    run(
        &db,
        "?[id, name, score] <- $rows :put idx_rel {id => name, score}",
        params(rows_a.clone()),
    );
    let mut flip = false;
    b.iter(|| {
        flip = !flip;
        let rows = if flip { rows_b.clone() } else { rows_a.clone() };
        run(
            &db,
            "?[id, name, score] <- $rows :put idx_rel {id => name, score}",
            params(rows),
        );
    });
}

// ------------------------------------------------------------- update

#[bench]
fn update_column_without_index(b: &mut Bencher) {
    // Met à jour une colonne légère sur des lignes contenant un gros embedding
    // (c'est le cas où l'on évitait un clone du vecteur).
    let db = new_db();
    run(
        &db,
        &format!(":create emb {{id: Int => name: String, v: <F32; {DIM}>}}"),
        Default::default(),
    );
    run(
        &db,
        "?[id, name, v] <- $rows :put emb {id => name, v}",
        params(emb_rows(ROWS, "init")),
    );
    let upd: Vec<DataValue> = (0..ROWS)
        .map(|i| DataValue::List(vec![DataValue::from(i as i64), DataValue::from(format!("u-{i}"))]))
        .collect();
    b.iter(|| {
        run(
            &db,
            "?[id, name] <- $rows :update emb {id, name}",
            params(upd.clone()),
        );
    });
}

#[bench]
fn update_column_with_index(b: &mut Bencher) {
    let db = new_db();
    run(&db, ":create upd {id: Int => name: String, score: Float}", Default::default());
    run(&db, "::index create upd:by_name {name}", Default::default());
    run(
        &db,
        "?[id, name, score] <- $rows :put upd {id => name, score}",
        params(plain_rows(ROWS, "init")),
    );
    let upd: Vec<DataValue> = (0..ROWS)
        .map(|i| DataValue::List(vec![DataValue::from(i as i64), DataValue::from(format!("u-{i}"))]))
        .collect();
    b.iter(|| {
        run(
            &db,
            "?[id, name] <- $rows :update upd {id, name}",
            params(upd.clone()),
        );
    });
}

// ------------------------------------------------------------- remove

#[bench]
fn remove_then_reinsert_plain(b: &mut Bencher) {
    // Mesure rm + put (il faut remettre les lignes pour la prochaine itération).
    // À comparer avec `put_plain` pour isoler le coût du rm.
    let db = new_db();
    run(&db, ":create rm_rel {id: Int => name: String, score: Float}", Default::default());
    let rows = plain_rows(ROWS, "a");
    let keys: Vec<DataValue> = (0..ROWS)
        .map(|i| DataValue::List(vec![DataValue::from(i as i64)]))
        .collect();
    run(
        &db,
        "?[id, name, score] <- $rows :put rm_rel {id => name, score}",
        params(rows.clone()),
    );
    b.iter(|| {
        run(&db, "?[id] <- $rows :rm rm_rel {id}", params(keys.clone()));
        run(
            &db,
            "?[id, name, score] <- $rows :put rm_rel {id => name, score}",
            params(rows.clone()),
        );
    });
}

#[bench]
fn remove_then_reinsert_with_index(b: &mut Bencher) {
    let db = new_db();
    run(&db, ":create rm_idx {id: Int => name: String, score: Float}", Default::default());
    run(&db, "::index create rm_idx:by_name {name}", Default::default());
    let rows = plain_rows(ROWS, "a");
    let keys: Vec<DataValue> = (0..ROWS)
        .map(|i| DataValue::List(vec![DataValue::from(i as i64)]))
        .collect();
    run(
        &db,
        "?[id, name, score] <- $rows :put rm_idx {id => name, score}",
        params(rows.clone()),
    );
    b.iter(|| {
        run(&db, "?[id] <- $rows :rm rm_idx {id}", params(keys.clone()));
        run(
            &db,
            "?[id, name, score] <- $rows :put rm_idx {id => name, score}",
            params(rows.clone()),
        );
    });
}

// ---------------------------------------------------------- FTS / HNSW

#[bench]
fn put_overwrite_fts_only(b: &mut Bencher) {
    // Relation avec UN SEUL index FTS (pas d'index classique) :
    // couvre le cas où del_in_fts doit être appelé à l'écrasement.
    let db = new_db();
    run(&db, ":create docs {id: Int => body: String}", Default::default());
    run(
        &db,
        "::fts create docs:fts {extractor: body, tokenizer: Simple, filters: [Lowercase]}",
        Default::default(),
    );
    let rows_a = doc_rows(HNSW_ROWS, "alpha");
    let rows_b = doc_rows(HNSW_ROWS, "beta");
    run(&db, "?[id, body] <- $rows :put docs {id => body}", params(rows_a.clone()));
    let mut flip = false;
    b.iter(|| {
        flip = !flip;
        let rows = if flip { rows_b.clone() } else { rows_a.clone() };
        run(&db, "?[id, body] <- $rows :put docs {id => body}", params(rows));
    });
}

#[bench]
fn put_hnsw(b: &mut Bencher) {
    let db = new_db();
    run(
        &db,
        &format!(":create items {{id: Int => name: String, v: <F32; {DIM}>}}"),
        Default::default(),
    );
    run(
        &db,
        &format!(
            "::hnsw create items:vec {{dim: {DIM}, m: 16, dtype: F32, fields: [v], \
             distance: Cosine, ef_construction: 50}}"
        ),
        Default::default(),
    );
    let rows = emb_rows(HNSW_ROWS, "a");
    b.iter(|| {
        run(
            &db,
            "?[id, name, v] <- $rows :put items {id => name, v}",
            params(rows.clone()),
        );
    });
}