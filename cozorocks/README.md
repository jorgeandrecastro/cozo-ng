# `cozo-ng-rocks`

[![Crates.io](https://img.shields.io/crates/v/cozo-ng-rocks)](https://crates.io/crates/cozo-ng-rocks)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng-rocks?label=docs.rs)](https://docs.rs/cozo-ng-rocks)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt)

`cozo-ng-rocks` is the low-level RocksDB storage adapter for the Rust-first
[Cozo-NG](https://github.com/jorgeandrecastro/cozo-ng) embedded database. It
wraps the RocksDB C++ API with [`cxx`](https://cxx.rs/) and provides the
database, transaction, iterator, and status types used by the Cozo-NG engine.

The crates.io package is named `cozo-ng-rocks`; its Rust library name remains
`cozorocks`. Cozo-NG is an independent fork and community-maintained
continuation of CozoDB, not an official upstream release. It follows its own
Rust-first direction: an embedded data layer for AI agents and GraphRAG
combining relational data, Datalog knowledge graphs, and vector search. This
crate is maintained as a Rust engine component within that project.

> **Most Rust applications should depend on [`cozo-ng`](https://crates.io/crates/cozo-ng)
> instead.** This crate exposes storage-engine internals, not CozoScript, the
> Cozo query API, or a standalone database. It is an implementation component
> of the Rust engine; its API and storage behavior may change as Cozo-NG is maintained.

## Add the dependency

The dependency key can remain `cozorocks`, matching the library name:

```toml
[dependencies]
cozorocks = { package = "cozo-ng-rocks", version = "0.2" }
```

In the Cozo-NG workspace, the engine already declares this dependency when its
`storage-rocksdb` feature is enabled. Applications using the engine normally
do not need to depend on this adapter directly.

## Example

This minimal example opens a RocksDB database and writes and reads a key in a
transaction:

```rust,no_run
use cozorocks::DbBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = DbBuilder::default()
        .path("./rocks-data")
        .create_if_missing(true)
        .build()?;

    let mut tx = db.transact().start();
    tx.put(b"greeting", b"hello, Cozo-NG")?;

    if let Some(value) = tx.get(b"greeting", false)? {
        assert_eq!(value.as_ref(), b"hello, Cozo-NG");
    }

    tx.commit()?;
    Ok(())
}
```

`RocksDbStatus` implements `std::error::Error` and is returned for RocksDB
failures. A missing key is represented as `Ok(None)` by `Tx::get`; it is not an
error.

## What is exposed

The library re-exports these main types:

- `DbBuilder` and `RocksDb` for opening and managing a database.
- `TxBuilder` and `Tx` for transactions, snapshots, savepoints, and writes.
- `PinSlice` for values pinned by RocksDB while they are being read.
- `IterBuilder` and `DbIter` for bounded, forward or reverse iteration.
- `SstWriter` for writing SST files and ingesting them into a database.
- `RocksDbStatus` and RocksDB status enums for error details.

See the [API documentation](https://docs.rs/cozo-ng-rocks) for the full list of
methods and their behavior.

## Build requirements

Building this crate compiles native C++ code. A Rust toolchain supported by the
workspace and a C++17-capable compiler are required. On Windows, use a supported
MSVC or MinGW toolchain. The RocksDB sources are bundled with the crate, so a
system RocksDB installation is not normally required; compression libraries
are built through the Rust dependencies.

By default, the bundled RocksDB source is compiled. To link against an existing
RocksDB build instead, set `ROCKSDB_LIB_DIR`. Set `ROCKSDB_STATIC` to link a
static library rather than a shared library. `ROCKSDB_INCLUDE_DIR` can point to
the matching RocksDB headers. The library and headers must be compatible.

The `io-uring` feature is optional and intended for Linux. It requires the
`liburing` development files and a working `pkg-config` configuration:

```toml
cozorocks = { package = "cozo-ng-rocks", version = "0.2", features = ["io-uring"] }
```

The `jemalloc` feature enables the optional `tikv-jemalloc-sys` dependency. It
is normally enabled through Cozo-NG's `jemalloc` feature rather than directly.

## Build and test

From the Cozo-NG repository root:

```sh
cargo check -p cozo-ng-rocks
cargo test -p cozo-ng-rocks
cargo doc -p cozo-ng-rocks --open
```

To compile the main engine with the RocksDB backend:

```sh
cargo check -p cozo-ng --features storage-rocksdb
```

## Maintenance and license

This crate is maintained as part of the Cozo-NG workspace. Please report issues
and send pull requests to the
[Cozo-NG repository](https://github.com/jorgeandrecastro/cozo-ng). See the
[contribution guidelines](https://github.com/jorgeandrecastro/cozo-ng/blob/main/CONTRIBUTING.md)
for project priorities and contribution guidance.

The crate is distributed under the
[Mozilla Public License 2.0](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt). It includes RocksDB source code;
see the license and notice files included with those sources for their terms.
