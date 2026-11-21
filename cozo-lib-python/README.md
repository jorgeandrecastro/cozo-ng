# Cozo-NG Python bindings

[![PyPI](https://img.shields.io/pypi/v/cozo-ng)](https://pypi.org/project/cozo-ng/)

Native Python bindings for the independent [Cozo-NG](https://github.com/jorgeandrecastro/cozo-ng)
embedded graph and vector database.

| | |
|---|---|
| Rust crate | `cozo-ng-python` (`0.14.1`) |
| Python distribution | `cozo-ng` |
| Python module | `cozo_embedded` |
| Engine | Cozo-NG `0.13.1` |
| Python | 3.9+ (stable ABI, `abi3-py39`) |

See the [crate changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-lib-python/CHANGELOG.md).

Cozo-NG `0.13.1` includes the `0.13.0` stored-relation write optimizations and
fixes for stale FTS/LSH index cleanup, `:rm`/`:delete` with `force_collect` on
temporary relations, and `:update` on corrupted values (now an error instead
of a panic); the Python binding API is unchanged.

## ⚠️ Compatibility

`cozo-ng` and the upstream [`cozo_embedded`](https://pypi.org/project/cozo_embedded/)
package both provide the `cozo_embedded` module. Do not install both in the
same environment.

## Installation

```sh
python -m pip install cozo-ng
```

The distribution name (`cozo-ng`) differs from the module name (`cozo_embedded`).
The published wheels are built with SQLite, RocksDB, graph algorithms and the
engine's request utilities (`compact` + `storage-rocksdb`).

## Quick start

```python
from cozo_embedded import CozoDbPy

db = CozoDbPy("mem", "", "{}")
result = db.run_script("?[x] <- [[1], [2]]", {}, False)
print(result["headers"], result["rows"])
```

## API overview

### `CozoDbPy(engine, path, options)`

| Method | Description |
|---|---|
| `run_script(query, params, immutable)` | Runs a script and returns `{"headers", "rows", "next"}`. |
| `multi_transact(write)` | Opens a multi-statement transaction (`CozoDbMulTx`). |
| `register_callback(rel, callback)` | Calls `callback(op, new_rows, old_rows)` on relation changes. Returns an id. |
| `unregister_callback(id)` | Removes a callback. Returns `False` if closed or unknown. |
| `register_fixed_rule(name, arity, callback)` | Registers a Python fixed rule: `callback(inputs, options) -> rows`. |
| `unregister_fixed_rule(name)` | Removes a fixed rule. |
| `export_relations(relations)` / `import_relations(data)` | Export or import relations as named rows. |
| `backup(path)` / `restore(path)` / `import_from_backup(path, relations)` | Backup and restore. |
| `close()` | Closes the database. Returns `True` if it was open. |

Once closed, every method except `unregister_callback` and `close` raises an
exception with the message `database closed`.

### `CozoDbMulTx`

`run_script(query, params)`, `commit()`, `abort()`.

### Module functions

- `eval_expressions(query, params, bindings)`: evaluates an expression.
- `variables(query, params)`: returns the set of variables used by a query.

## Type mapping

| Python | Cozo |
|---|---|
| `None` | `Null` |
| `bool` | `Bool` |
| `int` | `Int` (signed 64-bit) |
| `float` | `Float` |
| `str` | `String` |
| `bytes`, `bytearray` | `Bytes` |
| `list`, `tuple` | `List` |
| `dict` | `Json` |

- Integers outside the signed 64-bit range raise `ValueError`.
- Unsupported types raise `TypeError`.
- On the way back, UUIDs and regexes are returned as `str`, validities as
  `[timestamp, is_assert]`, and vectors as lists of floats.

## Errors

Errors raised by a query are `Exception` instances whose first argument
(`e.args[0]`) is the decoded JSON diagnostics. Other errors (closed database,
I/O, backup) carry a plain text message.

## Examples

### Transactions

```python
from cozo_embedded import CozoDbPy

db = CozoDbPy("mem", "", "{}")

tx = db.multi_transact(True)
tx.run_script(":create t {id: Int => v: String}", {})
tx.run_script("?[id, v] <- [[1, 'a']] :put t {id => v}", {})
tx.commit()
```

### Callbacks

Callbacks run on a dedicated thread. An exception raised in a callback is
printed with its traceback and does not stop the listener.

```python
cb_id = db.register_callback("t", lambda op, new, old: print(op, new, old))
db.run_script("?[id, v] <- [[2, 'b']] :put t {id => v}", {}, False)
db.unregister_callback(cb_id)
```

### Fixed rules

```python
def double(inputs, options):
    return [[row[0] * 2] for row in inputs[0]]

db.register_fixed_rule("Double", 1, double)
db.run_script("?[y] <~ Double(*t[id, v])", {}, True)
```

### Error handling

```python
try:
    db.run_script("?[x] <- bad", {}, True)
except Exception as e:
    print(e.args[0])  # decoded JSON diagnostics
```

## Building from source

Requirements: a Rust toolchain (1.85 or newer, edition 2024), the native build
requirements for RocksDB, and [maturin](https://github.com/PyO3/maturin).
From the repository root:

```sh
cd cozo-lib-python

# Local development (installs into the active virtualenv)
maturin develop --release -F compact -F storage-rocksdb

# Distributable wheel
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
The workflow uses the version declared in the Rust crate; make sure it matches
the intended release before tagging.

## Upstream packages

Cozo-NG is an independent continuation of CozoDB. The upstream
[`cozo_embedded`](https://pypi.org/project/cozo_embedded/) distribution and
[PyCozo](https://github.com/cozodb/pycozo) are separate projects, not Cozo-NG
releases. This distribution keeps `cozo_embedded` as its module name.