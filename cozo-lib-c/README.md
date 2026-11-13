# Cozo C lib

[![C](https://img.shields.io/github/v/release/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/releases)

This directory contains the source of the Cozo C API.

Cozo-NG is an independent fork and community-maintained continuation of
[CozoDB](https://github.com/cozodb/cozo), not an official upstream release. Its
own direction is Rust-first: an embedded data layer for AI agents and GraphRAG,
combining relational data, Datalog knowledge graphs, and vector search. The
Rust crate is the primary supported interface. This C binding is a community
integration surface; it may not have release parity with the Rust engine.
Contributions to build, test, document, or maintain it are welcome.

This document describes how to set up the C library.
To learn how to use CozoDB (CozoScript), read the [docs](https://docs.cozodb.org/en/latest/index.html).

Pre-built libraries, when published for Cozo-NG, will be available from the [Cozo-NG release page](https://github.com/jorgeandrecastro/cozo-ng/releases); look for those starting with `libcozo_c`.

The API is contained in this single [header file](./cozo_c.h).

An example for using the API is [here](./example.c).

To build and run the example:

```bash
gcc -L../target/release/ -lcozo_c example.c -o example && ./example
```

# Building Cozo from source

You need to install the [Rust toolchain](https://www.rust-lang.org/tools/install) on your system. Then:

```bash
cargo build --release -p cozo-ng-c -F compact -F storage-rocksdb
```
