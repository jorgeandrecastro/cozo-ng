<img src="static/logo_c.png" width="200" height="175" alt="Logo">

[![Crates.io](https://img.shields.io/crates/v/cozo-ng)](https://crates.io/crates/cozo-ng)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng?label=docs.rs)](https://docs.rs/cozo-ng)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/jorgeandrecastro/cozo-ng/build.yml?branch=main)](https://github.com/jorgeandrecastro/cozo-ng/actions/workflows/build.yml)
[![GitHub](https://img.shields.io/github/license/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt)

# cozo-ng : The Embedded Graph + Vector Database for AI Agents & GraphRAG in Rust

> **Cozo-NG** is an independent fork and community-maintained continuation of [CozoDB](https://github.com/cozodb/cozo), currently focused on the Rust engine. It is not an official upstream release. Cozo-NG sets its own direction: a Rust-first embedded database for AI agents and GraphRAG, combining relational data, Datalog knowledge graphs, and vector search.
>
> *If you are migrating from the original `cozo` crate, change the dependency package to `cozo-ng` while keeping the dependency key `cozo`; the library name remains `cozo`, so your Rust code does not need to change.*

### Table of contents

1. [Introduction](#Introduction)
2. [Getting started](#Getting-started)
3. [Install](#Install)
4. [Architecture](#Architecture)
5. [Status of the project](#Status-of-the-project)
6. [Links](#Links)
7. [Licensing and contributing](#Licensing-and-contributing)

## 🚀 Roadmap: v0.9 (Current Focus)

Cozo-NG resumes maintenance from the upstream `v0.7.6` codebase. Version `0.8.0` refreshed the Rust workspace and moved it to edition 2024 while retaining the `cozo` library name. The `0.9.0` release continues this independent maintenance with a Rust-first focus on the embedded engine and its use as a data layer for AI agents and GraphRAG.

The focus is a high-performance, transactional, embedded Rust database that brings together relational storage, Datalog-powered knowledge graphs, and HNSW vector search. It is designed to give Rust AI agents and GraphRAG applications a local, in-process data layer.

Why Cozo-NG for AI and local-first applications?
* **Embedded GraphRAG building block:** Compose vector similarity search, relational filters, and multi-hop graph traversal without requiring a separate graph or vector database service.
* **Rust-native and transactional:** Run in the application's process and use Cozo-NG's MVCC transaction model directly from Rust.
* **One local data layer:** Keep structured relationships and semantic embeddings close to the application, reducing operational components for local-first and agentic systems.

The v0.9 roadmap:
* [x] **Fork initialization:** Establish the independent project, licensing, crate namespace, and repository structure.
* [ ] **Dependency modernization:** Finish validating updated dependencies across supported Rust toolchains, storage features, and platforms.
* [x] **Rust-first scope:** Focus official maintenance on the Rust database engine and its Rust APIs. Keep `cozo` as the library name for migration compatibility.
* [ ] **Core performance and DX:** Refine query execution and improve examples, diagnostics, and developer documentation.
* [ ] **Storage safety:** Treat on-disk formats and migration behavior as compatibility-sensitive; test backend changes and document migrations.
* **Community bindings:** Python, Node.js, WASM, Java, Swift, C, and other non-Rust bindings are welcome community efforts. Collaborations and pull requests to build, test, document, or maintain them are warmly welcome, but they are not currently part of the core maintenance commitment.

See the [0.9.0 changelog](CHANGELOG.md) and [maintenance and contribution guidelines](CONTRIBUTING.md).

---

### Previous Upstream Milestones (`v0.6` – `v0.7`)

* **v0.7:** Added MinHash-LSH for near-duplicate search, full-text search, and native JSON value support. See [upstream v0.7 notes](https://docs.cozodb.org/en/latest/releases/v0.7.html).
* **v0.6:** Introduced HNSW (Hierarchical Navigable Small World) vector search indices integrated directly into Datalog unification and recursive queries, protected by MVCC transactions. See [upstream v0.6 notes](https://docs.cozodb.org/en/latest/releases/v0.6.html).

## Introduction

Cozo-NG is an embedded, transactional database engine written in Rust. It combines relational data, **Datalog** queries and recursion for knowledge graphs, and **HNSW vector search**. Its in-process design makes it a building block for local-first applications, AI agents, and GraphRAG systems that need structured relationships and semantic retrieval together.

Cozo-NG can also be run through a standalone HTTP server, but the Rust library is the project's primary interface and maintenance target.

### What does _embeddable_ mean here?

A database is almost surely embedded if you can use it on a device which _never_ connects to any network. SQLite is embedded. MySQL/Postgres/Oracle are client-server.

> A database is _embedded_ if it runs in the same process as your main program. This is in contradistinction to _client-server_ databases, where your program connects to a database server via a client library. Embedded databases generally require no setup and can be used in a much wider range of environments.
>
> Cozo-NG is embedded in your Rust process by default. A separate standalone HTTP server is also available for deployments that need a client-server interface; it is an optional way to run the engine, not a requirement.

### Why _graphs_?

Because data are inherently interconnected. Most insights about data can only be obtained if you take this interconnectedness into account.

> Most existing _graph_ databases start by requiring you to shoehorn your data into the labelled-property graph model. Cozo-NG uses the traditional relational model, which is much easier to work with for storing data, more versatile, and handles graph data naturally. Even more importantly, the most piercing insights about data usually come from graph structures _implicit_ several levels deep in your data. The relational model, being an _algebra_, deals with this cleanly and composably.

### What is so cool about _Datalog_?

Datalog can express all _relational_ queries. _Recursion_ in Datalog is much easier to express, much more powerful, and usually runs faster than in SQL. Datalog is also extremely composable: you can build your queries piece by piece.

> Recursion is especially important for graph queries. Cozo's dialect of Datalog supercharges it even further by allowing recursion through a safe subset of aggregations, and by providing extremely efficient built-in algorithms (such as PageRank, Dijkstra, Community Detection) for graph analysis.
>
> Rules in Datalog act like functions in a programming language. Decomposing a query into rules makes it clearer and more maintainable without losing efficiency—unlike monolithic nested `SELECT-FROM-WHERE` queries in SQL.

### Time travel?

Time travel in the database setting means tracking changes to data over time and allowing queries to be logically executed at a specific point in time to get a historical view of the data.

> In Cozo, instead of having all data automatically support time travel, you decide whether to enable this capability per relation so you only pay the storage cost where you actually need it. For more background, see the [upstream time-travel writeup](https://docs.cozodb.org/en/latest/releases/v0.4.html).

### How performant?

Benchmarks from the upstream engine on a 2020 Mac Mini with the RocksDB persistent storage engine:

* Running OLTP queries for a relation with 1.6M rows: ~100K QPS for mixed read/write/update transactional queries, and >250K QPS for read-only queries, with peak database memory usage around 50MB.
* Backup speed: ~1M rows/s; restore speed: ~400K rows/s, insensitive to relation size.
* OLAP queries: ~1 second to scan a table with 1.6M rows.
* Two-hop graph traversal completes in <1ms for a graph with 1.6M vertices and 31M edges.
* PageRank completes in ~50ms for 10K vertices / 120K edges, ~1s for 100K vertices / 1.7M edges, and ~30s for 1.6M vertices / 32M edges.

More performance details are available in the [upstream benchmarks](https://docs.cozodb.org/en/latest/releases/v0.3.html).

## Getting started

To learn the query language syntax and concepts, you can read the [CozoDB tutorial](https://docs.cozodb.org/en/latest/tutorial.html) or test queries in the legacy [CozoDB WASM demo](https://www.cozodb.org/wasm-demo/).

### Teasers

In the following examples, `*route` is a relation with two columns `fr` and `to`, representing a flight route between airports, and `FRA` is Frankfurt Airport.

How many airports are directly connected to `FRA`?

```datalog
?[count_unique(to)] := *route{fr: 'FRA', to}
```

| count_unique(to) |
|------------------|
| 310              |

How many airports are reachable from `FRA` by one stop?

```datalog
?[count_unique(to)] := *route{fr: 'FRA', to: stop},
                       *route{fr: stop, to}
```

| count_unique(to) |
|------------------|
| 2222             |

How many airports are reachable from `FRA` by any number of stops?

```datalog
reachable[to] := *route{fr: 'FRA', to}
reachable[to] := reachable[stop], *route{fr: stop, to}
?[count_unique(to)] := reachable[to]
```

| count_unique(to) |
|------------------|
| 3462             |

What are the two most difficult-to-reach airports by the minimum number of hops required, starting from `FRA`?

```datalog
shortest_paths[to, shortest(path)] := *route{fr: 'FRA', to},
                                      path = ['FRA', to]
shortest_paths[to, shortest(path)] := shortest_paths[stop, prev_path],
                                      *route{fr: stop, to},
                                      path = append(prev_path, to)
?[to, path, p_len] := shortest_paths[to, path], p_len = length(path)

:order -p_len
:limit 2
```

| to  | path                                                | p_len |
|-----|-----------------------------------------------------|-------|
| YPO | `["FRA","YYZ","YTS","YMO","YFA","ZKE","YAT","YPO"]` | 8     |
| BVI | `["FRA","AUH","BNE","ISA","BQL","BEU","BVI"]`       | 7     |

What is the shortest path between `FRA` and `YPO`, by actual distance travelled?

```datalog
start[] <- [['FRA']]
end[] <- [['YPO']]
?[src, dst, distance, path] <~ ShortestPathDijkstra(*route[], start[], end[])
```

| src | dst | distance | path                                                      |
|-----|-----|----------|-----------------------------------------------------------|
| FRA | YPO | 4544.0   | `["FRA","YUL","YVO","YKQ","YMO","YFA","ZKE","YAT","YPO"]` |

Cozo-NG preserves CozoDB's diagnostic error reporting:

```datalog
?[x, Y] := x = 1, y = x + 1
```

```text
eval::unbound_symb_in_head

  × Symbol 'Y' in rule head is unbound
   ╭────
 1 │ ?[x, Y] := x = 1, y = x + 1
   ·      ─
   ╰────
  help: Note that symbols occurring only in negated positions are not considered bound
```

## Install

### Rust (Primary Target)

Add `cozo-ng` to your `Cargo.toml`:

```toml
[dependencies]
cozo = { package = "cozo-ng", version = "0.9" }
```

Because `cozo-ng` preserves the internal library name `cozo`, your Rust imports remain unchanged:

```rust
use cozo::*;
```

### Supported Storage Backends & Targets

The Rust crate is the primary supported target. The wrappers listed below are
community-maintained surfaces: their presence in this repository does not imply
release parity or a support commitment. Community contributions to any binding
are welcome.

| Target / Environment                                     | Platform support                                                                                                        | Storage |
|----------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|---------|
| [Rust (`cozo-ng`)](https://docs.rs/cozo-ng/)             | Source only, usable on any [platform](https://doc.rust-lang.org/nightly/rustc/platform-support.html) with `std` support | MQRST   |
| [Standalone HTTP server](./cozo-bin)                     | Linux (x86_64, ARM64), Mac (ARM64, x86_64), Windows (x86_64)                                                            | MQRST   |
| [C/C++/FFI (`cozo-lib-c`)](./cozo-lib-c)                 | Linux (x86_64, ARM64), Mac (ARM64, x86_64), Windows (x86_64)                                                            | MQR     |
| [WebAssembly (`cozo-lib-wasm`)](./cozo-lib-wasm)         | Modern browsers supporting [WebAssembly](https://developer.mozilla.org/en-US/docs/WebAssembly#browser_compatibility)    | M       |
| [NodeJS (`cozo-lib-nodejs`)](./cozo-lib-nodejs)          | Linux (x86_64, ARM64), Mac (ARM64, x86_64), Windows (x86_64)                                                            | MQR     |

Python, Java, Swift, Node.js, WebAssembly, C, and other non-Rust bindings are not current core-maintenance priorities. The original upstream packages may not contain Cozo-NG changes. Please coordinate with the project if you would like to contribute or take responsibility for one of these bindings.

Storage legend:
* **M**: In-memory, non-persistent backend
* **Q**: [SQLite](https://www.sqlite.org/) storage backend
* **R**: [RocksDB](http://rocksdb.org/) storage backend
* **S**: [Sled](https://github.com/spacejam/sled) storage backend
* **T**: [TiKV](https://tikv.org/) distributed storage backend

### Tuning the RocksDB backend

When creating a Cozo instance with the RocksDB backend, you provide a path to a data directory. If you place a file named `options` inside this directory, the engine will load it as a [RocksDB options file](https://github.com/facebook/rocksdb/wiki/RocksDB-Options-File).

To customize safely, run your database once, copy `data/OPTIONS-XXXXXX` from your database directory, rename it to `options`, and adjust only the parameters you understand.

## Architecture

Cozo-NG consists of three stacked layers, with each layer calling only into the layer below:

```text
+-----------------------------------+
|           (User code)             |
+-----------------------------------+
|   Language / Environment Wrapper  |
+-----------------------------------+
|           Query Engine            |
+-----------------------------------+
|          Storage Engine           |
+-----------------------------------+
|        (Operating system)         |
+-----------------------------------+
```

### Storage engine

The storage engine defines a storage `trait` providing a key-value store for binary data with range scan capabilities across multiple backends (Memory, SQLite, RocksDB, Sled, TiKV), or a custom backend implemented in Rust.

Rows are serialized using a _row-oriented_ binary format implementing the [memcomparable format](https://github.com/facebook/mysql-5.6/wiki/MyRocks-record-format#memcomparable-format) for keys so lexicographical sorting matches logical value order.

### Query engine

The query engine handles:
* Function, aggregation, and graph algorithm definitions
* Database schema and MVCC transactions
* CozoScript / Datalog query compilation and execution

### Language/environment wrapper

For Rust users, there is no wrapper—you call the query engine directly. The standalone server and language bindings are separate integration surfaces around that core. Rust is the maintained first-class interface; non-Rust bindings welcome community maintainers and contributions.

## Status of the project

Cozo-NG is a Rust-first community continuation of CozoDB. Core maintenance focuses on the Rust database engine and its storage backends. Other language bindings are community-driven and do not have a guaranteed maintenance or release schedule; collaborators are welcome to contribute and maintain them.

This project follows semantic versioning where practical. While the package is below `1.0`, minor releases may include breaking API or storage-format changes; such changes must be called out in the changelog with migration guidance. Bug fixes and security fixes should be released as patch updates when possible.

Bug reports, dependency updates, and pull requests are actively welcomed.

## Links

* [Cozo-NG Repository](https://github.com/jorgeandrecastro/cozo-ng)
* [Cozo-NG Issue Tracker](https://github.com/jorgeandrecastro/cozo-ng/issues)
* [Crates.io (`cozo-ng`)](https://crates.io/crates/cozo-ng)
* [Rust API Docs (`docs.rs`)](https://docs.rs/cozo-ng/)
* [Changelog](CHANGELOG.md)
* [Upstream Documentation (CozoDB v0.7)](https://docs.cozodb.org/en/latest/)
* [Original Upstream Repository](https://github.com/cozodb/cozo)

## Licensing and contributing

This project remains licensed under the **Mozilla Public License 2.0 (MPL-2.0)** or later, identical to the original CozoDB project. See [LICENSE.txt](LICENSE.txt) for full details.

See [CONTRIBUTING.md](CONTRIBUTING.md) if you are interested in contributing to `Cozo-NG`.