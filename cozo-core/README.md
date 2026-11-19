# Cozo-NG Rust core (`cozo-ng`)

[![Crates.io](https://img.shields.io/crates/v/cozo-ng)](https://crates.io/crates/cozo-ng)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng?label=docs.rs)](https://docs.rs/cozo-ng)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/jorgeandrecastro/cozo-ng/build.yml?branch=main)](https://github.com/jorgeandrecastro/cozo-ng/actions/workflows/build.yml)
[![GitHub](https://img.shields.io/github/license/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt)

The Rust engine is the primary supported and maintained part of Cozo-NG: an embedded, transactional database that brings relational data, Datalog knowledge graphs, and HNSW vector search together in-process. It is designed as a Rust-native data layer for local-first applications, AI agents, and GraphRAG systems.

Cozo-NG is an independent fork and community-maintained continuation of CozoDB `0.7.6`, not an official upstream release. It sets its own Rust-first direction: an embedded data layer for AI agents and GraphRAG that combines relational data, Datalog knowledge graphs, and vector search. Language bindings are community-driven; contributions to build, test, and maintain them are welcome.

The `cozo-ng` engine crate is version `0.13.0`. The query parser, HNSW
runtime, relation runtime, relational algebra, and stored-relation query
execution are organized into focused modules. Version `0.13.0` optimizes the
stored-relation write path (`:put`, `:update`, `:rm`) and adds a dedicated
write benchmark. See the
[crate changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-core/CHANGELOG.md)
for details.

The published package is named `cozo-ng`, while its Rust library name remains `cozo`. Existing code can therefore keep using `use cozo::*` by declaring the dependency as:

```toml
[dependencies]
cozo = { package = "cozo-ng", version = "0.13" }
```

See the [Cozo-NG repository](https://github.com/jorgeandrecastro/cozo-ng) for project status and contribution information.

## Features

- **Embedded and transactional:** Run the engine in-process with MVCC transactions; no separate database service is required.
- **Knowledge graphs with Datalog:** Express recursive, multi-hop relationships as composable queries.
- **Vector search:** Use the engine's HNSW indexing alongside structured relational and graph data.
- **Storage choices:** Select an in-memory or persistent backend through Cargo features, including SQLite and RocksDB.
- **Rust-native:** Use the library directly from Rust. The standalone server (also available as a [Docker image](https://github.com/jorgeandrecastro/cozo-ng/blob/main/README.md#run-with-docker)) and non-Rust bindings are additional integration options, not the project's primary maintenance focus.

For examples, feature selection, and storage details, see the [main README](https://github.com/jorgeandrecastro/cozo-ng/blob/main/README.md), the [CLI guide](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cli/README.md), and the [API documentation](https://docs.rs/cozo-ng).

## Benchmarks

The benchmarks use Rust's unstable `test` framework and are excluded from
stable builds by default. Enable them explicitly with a nightly toolchain:

```sh
cargo +nightly bench -p cozo-ng --features nightly-benchmarks --bench pokec
cargo +nightly bench -p cozo-ng --features nightly-benchmarks --bench wiki_pagerank
cargo +nightly bench -p cozo-ng --features nightly-benchmarks,storage-rocksdb --bench time_travel
cargo +nightly bench -p cozo-ng --features nightly-benchmarks --bench stored_write
```

The Pokec benchmark requires `COZO_BENCH_POKEC_DIR` and
`COZO_BENCH_POKEC_BATCH`, and the Wikipedia benchmark requires
`COZO_BENCH_WIKI_DIR`; the directories must contain the corresponding datasets.
The time-travel benchmark creates a RocksDB database in the current directory.
The `stored_write` benchmark is self-contained and uses the in-memory backend:
it measures `:put`, `:update`, `:rm`, full-text and HNSW writes, with and
without secondary indices.
The `nightly-benchmarks` feature must not be enabled with a stable compiler.

## Crate architecture

This tree describes the Rust core crate only. Language bindings and application
integrations live in separate workspace crates and are intentionally not part
of this overview.

```text
cozo-core/
├── src/
│   ├── lib.rs             # Public Rust API and database entry points
│   ├── cozoscript.pest    # CozoScript grammar
│   ├── data/              # Values, expressions, relations, functions, aggregates
│   ├── parse/             # Query, schema, expression, and command parsing
│   │   └── query/          # Query atoms and rule parsing
│   ├── query/             # Query planning, compilation, optimization, evaluation
│   │   ├── ra/            # Relational algebra operators, joins, and scans
│   │   └── stored/        # Stored-relation operations, indexes, triggers, write flags
│   ├── runtime/           # Database runtime, transactions, indexes, execution
│   │   ├── db/            # Database operations, execution, backup, and import/export
│   │   ├── hnsw/          # HNSW insertion, search, and removal
│   │   └── relation/      # Relation operations and index creation
│   ├── fixed_rule/        # Built-in graph algorithms and query utilities
│   ├── fts/               # Full-text search
│   ├── storage/           # Memory and persistent storage backends
│   └── utils.rs           # Shared core utilities
├── tests/                 # Integration tests for the Rust engine
├── benches/               # Rust engine benchmarks (pokec, wiki_pagerank, time_travel, stored_write)
├── Cargo.toml             # Core crate metadata, dependencies, and features
├── README.md
└── CHANGELOG.md
```

The main execution path is `parse/` → `query/` → `runtime/` → `storage/`.
`data/` provides the shared representations used throughout that path, while
`fixed_rule/` and `fts/` supply graph and full-text search capabilities.