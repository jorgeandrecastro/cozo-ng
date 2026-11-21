# Changelog

Release notes for the `cozo-ng-python` Rust crate. Cozo-NG is an independent
continuation of CozoDB. This crate release does not publish or update the upstream
PyPI distribution.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.14.1] - 2026-11-21

The engine dependency is Cozo-NG `0.13.1`. No Python binding API changes.

## [0.14.0] - 2026-11-19

The engine dependency is Cozo-NG `0.13.0`. The engine optimizes
stored-relation writes and fixes stale FTS/LSH index cleanup,
`:rm`/`:delete` with `force_collect` on temporary relations, and `:update` on
corrupted values (now an error instead of a panic). No Python binding API
changes.

## [0.13.0] - 2026-11-17

The engine dependency stays at Cozo-NG `0.12.2`.

### Fixed
- `eval_expressions` and `variables` no longer panic on invalid parameters;
  they raise a regular Python exception.
- Callbacks registered with `register_callback` run on a dedicated thread
  instead of permanently occupying a rayon worker.
- Exceptions raised inside callbacks are printed with their Python traceback.
- `bytearray` conversion no longer uses `unsafe`.
- Integers outside the `i64` range raise `ValueError` instead of silently
  falling back to a float.

### Changed
- **Breaking:** the minimum supported Python version is now 3.9 (`abi3-py39`).
- **Breaking:** `unregister_fixed_rule` raises on a closed database instead of
  returning `False`, like the other methods.
- **Breaking:** the "database closed" error is now the plain message
  `database closed` instead of a raw JSON string.
- Unsupported Python types raise `TypeError` instead of a generic `Exception`.
- `register_callback` and `register_fixed_rule` raise `TypeError` when the
  callback is not callable.
- `eval_expressions` releases the GIL while evaluating.
- Multi-transaction `abort` and `commit` errors use the same conversion as the
  other methods.
- Internal refactoring: shared error conversion and closed-database handling.
- Added `rust-version = "1.85"` (required by edition 2024).

### Added
- Type stubs (`cozo_embedded.pyi`) and the `py.typed` marker.
- `pyproject.toml` with maturin configuration.
- README: type mapping, API overview, examples and compatibility warning.

### Removed
- The `compact-single-threaded` feature, identical to `compact`.
- The `rayon` dependency, no longer used.

## [0.12.2]

- Prepared the Python binding for the Cozo-NG `0.12.2` query-module
  refactoring and compilation fixes. No Python binding API changes.

## [0.12.1]

- Updated the engine dependency to Cozo-NG `0.12.1`; no Python binding API changes.

## [0.12.0]

- Updated the engine dependency to Cozo-NG `0.12.0`.

## [0.11.1]

- Updated the engine dependency to Cozo-NG `0.11.1`; no Python binding API changes.

## [0.11.0]

- Updated the Python binding crate to target the Cozo-NG `0.11.0` engine.
- No Python binding API changes are included in this release.

## [0.10.0]

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was
  reorganized internally under `cozo-core/src/runtime/db/`; the Python binding
  API is unchanged.

## [0.9.0]

- Updated the Python binding crate to target the Cozo-NG `0.9.0` engine.
- Kept PyO3 on the `0.22` API used by this binding; a future PyO3 major-version
  update requires migrating the binding code first (`gil-refs` is deprecated).
- Retained the Rust library name `cozo_embedded`.
- Community contributions to build, test, document, and maintain Python
  bindings for Cozo-NG remain welcome.