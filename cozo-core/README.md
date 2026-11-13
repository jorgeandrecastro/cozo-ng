# Cozo-NG Rust core (`cozo-ng`)

[![Crates.io](https://img.shields.io/crates/v/cozo-ng)](https://crates.io/crates/cozo-ng)

The Rust engine is the primary supported and maintained part of Cozo-NG: an embedded, transactional database that brings relational data, Datalog knowledge graphs, and HNSW vector search together in-process. It is designed as a Rust-native data layer for local-first applications, AI agents, and GraphRAG systems.

Cozo-NG is an independent community continuation of CozoDB `0.7.6`, not an official upstream release. The project is Rust-first; language bindings are community-driven and contributions to build, test, and maintain them are welcome.

The published package is named `cozo-ng`, while its Rust library name remains `cozo`. Existing code can therefore keep using `use cozo::*` by declaring the dependency as:

```toml
[dependencies]
cozo = { package = "cozo-ng", version = "0.8" }
```

See the [Cozo-NG repository](https://github.com/jorgeandrecastro/cozo-ng) for project status and contribution information.

## Features

- **Embedded and transactional:** Run the engine in-process with MVCC transactions; no separate database service is required.
- **Knowledge graphs with Datalog:** Express recursive, multi-hop relationships as composable queries.
- **Vector search:** Use the engine's HNSW indexing alongside structured relational and graph data.
- **Storage choices:** Select an in-memory or persistent backend through Cargo features, including SQLite and RocksDB.
- **Rust-native:** Use the library directly from Rust. The standalone server and non-Rust bindings are additional integration options, not the project's primary maintenance focus.

For examples, feature selection, and storage details, see the [main README](../README.md) and the [API documentation](https://docs.rs/cozo-ng).