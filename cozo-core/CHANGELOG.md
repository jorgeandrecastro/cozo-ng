# Changelog

Release notes for the `cozo-ng` engine crate. Cozo-NG is an independent,
community-maintained continuation of CozoDB; these are Cozo-NG releases, not
upstream CozoDB releases. The package is `cozo-ng`, while the Rust library
remains `cozo`.

## 0.13.2

- Patch release for publication and documentation.
- Added docs.rs metadata and a crate README. No engine API, behavior, or
  storage-format changes.

## 0.13.1

- Patch release aligned with the workspace bindings and RocksDB adapter.
- No engine API, behavior, or storage-format changes.

## 0.13.0

### Changed

- Optimized the stored-relation write path (`:put`, `:insert`, `:update`, `:rm`,
  `:delete`) in `query/stored/`:
  - Added a shared `WriteFlags` helper (`query/stored/flags.rs`) so the access
    check and the index/trigger/callback flags are computed in one place.
  - `:insert` no longer reads the existing row (the key is known to be absent).
  - `:delete` performs a single store read instead of `exists` followed by `get`.
  - `:update` only builds the previous row when indices, triggers or callbacks
    need it, and no longer clones untouched columns (for example large
    embeddings) otherwise.
  - Rows are now built with pre-sized buffers, and `:rm`/`:delete` keep their
    collected tuples as `Vec<Tuple>` instead of wrapping them in
    `DataValue::List`.
  - Trigger inputs for `:rm`/`:delete` are converted once instead of once per
    trigger.
- Moved the `Replace` error types in `query/stored.rs` to module level.
- Reading a corrupted value during `:update` now returns an error instead of
  panicking.
- No public API or storage-format changes.

### Fixed

- `:put` over an existing key now removes stale full-text (FTS) and LSH index
  entries even when the relation has no regular index.
- `:rm`/`:delete` on a temporary relation with `force_collect` now reads the
  previous values from the temporary store.

### Added

- New benchmark `benches/stored_write.rs` covering `:put`, `:update`, `:rm`,
  FTS and HNSW write paths. Run it with
  `cargo +nightly bench -p cozo-ng --features nightly-benchmarks --bench stored_write`.

## 0.12.2 

### Changed

- Split relational algebra and stored-relation query execution into focused
  modules without intentionally changing engine behavior or public interfaces.
- Fixed module visibility and imports introduced by the split.
- Replaced deprecated UUID and iterator APIs and removed an unused import.
- Updated the optional RocksDB adapter dependency to `cozo-ng-rocks 0.5.2`.

## 0.12.1

### Changed

- Split the query parser, HNSW runtime, and relation runtime into focused
  modules, preserving existing module paths and behavior.
- Updated the optional RocksDB adapter dependency to `cozo-ng-rocks 0.5.1`.
- No engine API or storage-format changes.

## 0.12.0

### Changed

- Updated the optional RocksDB adapter dependency to `cozo-ng-rocks 0.5.0`.

## 0.11.1

### Changed

- Updated the optional RocksDB adapter dependency to `cozo-ng-rocks 0.4.1`.
- No engine API or storage-format changes.

## 0.11.0

### Changed

- Released the engine as `cozo-ng 0.11.0`, keeping the Rust library name `cozo` and the `cozo::runtime::db` module path.
- Updated the documentation and the dependency example to `version = "0.11"`.
- No changes to the engine's public API or storage format are introduced by this release.

## 0.10.1

### Changed
- Aligned documentation and harmonized dependency versions across the project.

## 0.10.0

### Changed

- Moved the database runtime from the single `src/runtime/db.rs` file into focused modules under `src/runtime/db/`, preserving the existing `cozo::runtime::db` module path and public API.

## 0.9.0

### Fixed

- Made Rayon a required dependency because query evaluation and transaction execution use it regardless of default features.
- Enabled Rayon in the `graph-algo` feature so graph algorithms use their intended parallel implementations.

### Changed

- Updated the `rand` dependency to `0.10`.
- Migrated random-number generation to Rand 0.10's `rng`, `RngExt`, and `random_*` APIs.
- Updated the `thiserror` dependency to `2`.
- Continued the Rust-first focus on the embedded engine as a local data layer for AI agents and GraphRAG.

## 0.8.1

### Fixed

- Made Rayon a required engine dependency for builds with default features disabled.

## 0.8.0

### Changed

- Resumed independent maintenance from the CozoDB `0.7.6` codebase.
- Published the package as `cozo-ng` while preserving the Rust library name `cozo` for migration compatibility.
- Moved the maintained Rust workspace to edition 2024 and refreshed its dependencies.