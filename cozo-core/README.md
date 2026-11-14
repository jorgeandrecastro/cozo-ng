# Cozo-NG Rust core (`cozo-ng`)

[![Crates.io](https://img.shields.io/crates/v/cozo-ng)](https://crates.io/crates/cozo-ng)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng?label=docs.rs)](https://docs.rs/cozo-ng)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/jorgeandrecastro/cozo-ng/build.yml?branch=main)](https://github.com/jorgeandrecastro/cozo-ng/actions/workflows/build.yml)
[![GitHub](https://img.shields.io/github/license/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt)

The Rust engine is the primary supported and maintained part of Cozo-NG: an embedded, transactional database that brings relational data, Datalog knowledge graphs, and HNSW vector search together in-process. It is designed as a Rust-native data layer for local-first applications, AI agents, and GraphRAG systems.

Cozo-NG is an independent fork and community-maintained continuation of CozoDB `0.7.6`, not an official upstream release. It sets its own Rust-first direction: an embedded data layer for AI agents and GraphRAG that combines relational data, Datalog knowledge graphs, and vector search. Language bindings are community-driven; contributions to build, test, and maintain them are welcome.

This is the `cozo-ng` engine crate's `0.10.1` release. The former
`src/runtime/db.rs` implementation is organized into focused modules under
`src/runtime/db/`; the `cozo::runtime::db` module path and public API are
preserved. See the [crate changelog](CHANGELOG.md) for details.

The published package is named `cozo-ng`, while its Rust library name remains `cozo`. Existing code can therefore keep using `use cozo::*` by declaring the dependency as:

```toml
[dependencies]
cozo = { package = "cozo-ng", version = "0.10" }
```

See the [Cozo-NG repository](https://github.com/jorgeandrecastro/cozo-ng) for project status and contribution information.

## Features

- **Embedded and transactional:** Run the engine in-process with MVCC transactions; no separate database service is required.
- **Knowledge graphs with Datalog:** Express recursive, multi-hop relationships as composable queries.
- **Vector search:** Use the engine's HNSW indexing alongside structured relational and graph data.
- **Storage choices:** Select an in-memory or persistent backend through Cargo features, including SQLite and RocksDB.
- **Rust-native:** Use the library directly from Rust. The standalone server and non-Rust bindings are additional integration options, not the project's primary maintenance focus.

For examples, feature selection, and storage details, see the [main README](../README.md) and the [API documentation](https://docs.rs/cozo-ng).

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
│   ├── query/             # Query planning, compilation, optimization, evaluation
│   ├── runtime/           # Database runtime, transactions, indexes, execution
│   │   └── db/            # Database operations, execution, backup, and import/export
│   ├── fixed_rule/        # Built-in graph algorithms and query utilities
│   ├── fts/               # Full-text search
│   ├── storage/           # Memory and persistent storage backends
│   └── utils.rs           # Shared core utilities
├── tests/                 # Integration tests for the Rust engine
├── benches/               # Rust engine benchmarks
├── Cargo.toml             # Core crate metadata, dependencies, and features
├── README.md
└── CHANGELOG.md
```

The main execution path is `parse/` → `query/` → `runtime/` → `storage/`.
`data/` provides the shared representations used throughout that path, while
`fixed_rule/` and `fts/` supply graph and full-text search capabilities.
