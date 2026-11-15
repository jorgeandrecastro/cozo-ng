# Cozo-NG Python bindings

[![PyPI](https://img.shields.io/pypi/v/cozo-ng)](https://pypi.org/project/cozo-ng/)

Native Python bindings for the independent [Cozo-NG](https://github.com/jorgeandrecastro/cozo-ng)
embedded graph and vector database. The Rust crate is `cozo-ng-python`; the
configured Python distribution is `cozo-ng` version `0.12.0`, and its extension
module is named `cozo_embedded`. See the [crate changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-lib-python/CHANGELOG.md).

## Installation and usage

Once this version has been published to PyPI, install the distribution with:

```sh
python -m pip install cozo-ng
```

The distribution name differs from the module name:

```python
from cozo_embedded import CozoDbPy

db = CozoDbPy("mem", "", "{}")
result = db.run_script("?[x] <- [[1], [2]]", {}, False)
print(result["headers"], result["rows"])
```

The package metadata requires Python 3.7 or newer. The configured build enables
SQLite, RocksDB, graph algorithms and the engine's request utilities.

## Building from source

Install a Rust toolchain, the native build requirements for RocksDB and
[maturin](https://github.com/PyO3/maturin). From the repository root:

```sh
cd cozo-lib-python
maturin build --release -F compact -F storage-rocksdb
```

See the maturin documentation for [development](https://www.maturin.rs/develop.html)
and [building](https://www.maturin.rs/distribution.html).

## PyPI release workflow

The [PyPI Release workflow](https://github.com/jorgeandrecastro/cozo-ng/blob/main/.github/workflows/pypi.yaml)
runs on pushes to `main` or `master`, pushes of tags matching `v*`, and manual
dispatch. Its Linux, Windows and macOS jobs use maturin to build wheels with
`compact` and `storage-rocksdb`; Linux also produces a source distribution and
macOS requests universal2 wheels. Each job uploads its output as a GitHub
Actions artifact.

The PyPI upload job runs only for a tag ref, after all three build jobs succeed.
It downloads their artifacts and uploads them with `--skip-existing`, using the
repository secret `PYPI_API_TOKEN`. Configure that secret with a token
authorized to publish the `cozo-ng` project before triggering a tag release.
The workflow uses the version in `pyproject.toml`; ensure it matches the intended
release and the Rust crate version before publishing. A workflow or version
configuration alone does not mean the distribution has been published.

## Upstream packages

Cozo-NG is an independent continuation of CozoDB. The upstream
[`cozo_embedded`](https://pypi.org/project/cozo_embedded/) distribution and
[PyCozo](https://github.com/cozodb/pycozo) are separate projects, not Cozo-NG
releases. The new distribution retains `cozo_embedded` as its module name.
