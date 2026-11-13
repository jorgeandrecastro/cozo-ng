<img src="static/logo_c.png" width="200" height="175" alt="Logo">

[![Crates.io](https://img.shields.io/crates/v/cozo-ng)](https://crates.io/crates/cozo-ng)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng?label=docs.rs)](https://docs.rs/cozo-ng)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/jorgeandrecastro/cozo-ng/build.yml?branch=main)](https://github.com/jorgeandrecastro/cozo-ng/actions/workflows/build.yml)
[![GitHub](https://img.shields.io/github/license/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt)

# `Cozo-NG` *(Community-maintained fork of CozoDB)*

> **Maintenance notice:** The original [CozoDB repository](https://github.com/cozodb/cozo) has remained at `v0.7.6` without active maintenance for roughly two to three years. Maintenance resumes here with Cozo-NG `0.8.0`. This is an independent community fork, not an official upstream release.
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

## 🚀 Fork & Maintenance Roadmap

Cozo-NG resumes maintenance from the upstream `v0.7.6` codebase. The initial `0.8.0` release refreshes compatible Rust dependencies and moves the core workspace to Rust edition 2024 while retaining the `cozo` library name.

Maintenance priorities:
* **Core first:** Keep the Rust library and standalone server building, tested, and documented; keep the library name `cozo` for straightforward migration.
* **Dependencies:** Review and refresh dependencies regularly, check RustSec advisories, and include compatibility fixes and notable upgrades in release notes.
* **Storage safety:** Treat on-disk format and migration behavior as compatibility-sensitive. Test storage backends before changing their dependencies or behavior, and document any required migration.
* **Bindings:** Maintain a binding when its build and tests can be exercised in CI; clearly label third-party packages that are still upstream-only.
* **Community:** Triage issues and review contributions in this repository; no response-time guarantee is implied for a community-maintained project.

See the [0.8.0 changelog](CHANGELOG.md) and [maintenance and contribution guidelines](CONTRIBUTING.md).

---

### Previous Upstream Milestones (`v0.6` – `v0.7`)

* **v0.7:** Added MinHash-LSH for near-duplicate search, full-text search, and native JSON value support. See [upstream v0.7 notes](https://docs.cozodb.org/en/latest/releases/v0.7.html).
* **v0.6:** Introduced HNSW (Hierarchical Navigable Small World) vector search indices integrated directly into Datalog unification and recursive queries, protected by MVCC transactions. See [upstream v0.6 notes](https://docs.cozodb.org/en/latest/releases/v0.6.html).

## Introduction

Cozo-NG (CozoDB) is a general-purpose, transactional, relational database that uses **Datalog** for query, is **embeddable** but can also handle huge amounts of data and concurrency, and focuses on **graph** data, **vector search**, and algorithms. It supports **time travel** and it is **performant**!

### What does _embeddable_ mean here?

A database is almost surely embedded if you can use it on a device which _never_ connects to any network. SQLite is embedded. MySQL/Postgres/Oracle are client-server.

> A database is _embedded_ if it runs in the same process as your main program. This is in contradistinction to _client-server_ databases, where your program connects to a database server via a client library. Embedded databases generally require no setup and can be used in a much wider range of environments.
>
> Cozo-NG is _embeddable_ instead of merely _embedded_ since you can also run it as a standalone HTTP server in client-server mode, making better use of server resources and allowing higher concurrency.

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
cozo = { package = "cozo-ng", version = "0.8" }
```

Because `cozo-ng` preserves the internal library name `cozo`, your Rust imports remain unchanged:

```rust
use cozo::*;
```

### Supported Storage Backends & Targets

| Target / Environment                                     | Platform support                                                                                                        | Storage |
|----------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|---------|
| [Rust (`cozo-ng`)](https://docs.rs/cozo-ng/)             | Source only, usable on any [platform](https://doc.rust-lang.org/nightly/rustc/platform-support.html) with `std` support | MQRST   |
| [Standalone HTTP server](./cozo-bin)                     | Linux (x86_64, ARM64), Mac (ARM64, x86_64), Windows (x86_64)                                                            | MQRST   |
| [C/C++/FFI (`cozo-lib-c`)](./cozo-lib-c)                 | Linux (x86_64, ARM64), Mac (ARM64, x86_64), Windows (x86_64)                                                            | MQR     |
| [WebAssembly (`cozo-lib-wasm`)](./cozo-lib-wasm)         | Modern browsers supporting [WebAssembly](https://developer.mozilla.org/en-US/docs/WebAssembly#browser_compatibility)    | M       |
| [NodeJS (`cozo-lib-nodejs`)](./cozo-lib-nodejs)          | Linux (x86_64, ARM64), Mac (ARM64, x86_64), Windows (x86_64)                                                            | MQR     |

*(Note: External language bindings from the original organization such as Python, Java, Clojure, Android, Swift, and Go are currently unmaintained upstream; community PRs to restore them in `cozo-ng` are welcome.)*

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

For Rust users, there is no wrapper—you call the query engine directly. For standalone server (`cozo-bin`), C FFI, or WASM targets, thin wrappers translate the synchronous Rust API into HTTP endpoints, C ABI calls, or JS bindings.

## Status of the project

Maintenance of Cozo-NG resumes with `0.8.0`. This community fork focuses first on the Rust core; support for each optional backend and language binding depends on CI coverage and available maintainers. The upstream `0.7.6` project has had no active maintenance for roughly two to three years.

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