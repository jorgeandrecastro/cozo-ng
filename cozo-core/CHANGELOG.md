# Changelog

Release notes for the `cozo-ng` engine crate. Cozo-NG is an independent,
community-maintained fork of CozoDB; these are Cozo-NG releases, not upstream
CozoDB releases. The package is `cozo-ng`, while the Rust library remains
`cozo`.

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