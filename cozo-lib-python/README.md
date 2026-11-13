# cozo-lib-python

[![pypi](https://img.shields.io/pypi/v/cozo_embedded)](https://pypi.org/project/cozo_embedded/)

Native bindings for embedding [CozoDB](https://www.cozodb.org) in Python, providing the
`cozo_embedded` package.

Cozo-NG focuses maintenance on the embedded Rust engine. Python support is
community-driven, with no guaranteed release or compatibility schedule. The
PyPI package linked below is an upstream distribution and is not a Cozo-NG
release. Contributions to build, test, document, or maintain Python bindings
for Cozo-NG are warmly welcome.

The linked [PyCozo](https://github.com/cozodb/pycozo) package is the upstream
Python distribution and depends on the upstream binding; it does not currently
provide a Cozo-NG release. This directory is a starting point for community
work to build and maintain a Python integration for the fork.

To build this package, you need to install the Rust toolchain
as well as the [maturin](https://github.com/PyO3/maturin) python package.
Then run

```bash
maturin build -F compact -F storage-rocksdb --release
```

Refer maturin's docs for more information about how to [develop](https://www.maturin.rs/develop.html)
and [build](https://www.maturin.rs/distribution.html) this package.