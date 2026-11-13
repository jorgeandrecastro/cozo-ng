# Cozo C lib

[![C](https://img.shields.io/github/v/release/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/releases)

This directory contains the source of the Cozo C API.

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
cargo build --release -p cozo_c -F compact -F storage-rocksdb
```
