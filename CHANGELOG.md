# Changelog

Notable changes to Cozo-NG are documented here. Cozo-NG is an independent community continuation of CozoDB; upstream release history is not the release history of this fork.

## 0.9.0

### Changed

- Clarified Cozo-NG's independent project direction: Rust-first maintenance of the embedded database engine, positioned as a local data layer for AI agents and GraphRAG.
- Kept non-Rust bindings open to community contributions without making them part of the core maintenance commitment.
- Updated the Rust server and binding crates to version `0.9.0` and the RocksDB adapter to `0.3.0`; these crate versions do not imply releases of the upstream npm, PyPI, CocoaPods, or other packages.

### Fixed

- Made Rayon a required engine dependency because query evaluation and transaction execution use it even when default features are disabled.
- Enabled Rayon when the `graph-algo` feature is selected so graph algorithms use the intended parallel implementations.
- Updated the engine's `rand` and `thiserror` dependencies.
- Migrated the engine's random-number generation calls to Rand 0.10.

## 0.8.1

### Fixed

- Made Rayon a required engine dependency because query evaluation and transaction execution use it even when default features are disabled.
- Updated workspace bindings to depend on `cozo-ng 0.8.1`, so their published packages resolve the fixed engine release.

## 0.8.0

### Added

- Resumed independent community maintenance of the CozoDB Rust core.
- Added this changelog and documented the project's maintenance, contribution, compatibility, and release policies.
- Preserved the Rust library name `cozo` so existing Rust imports can remain unchanged when migrating to the `cozo-ng` package.

### Changed

- Renamed the published core package to `cozo-ng`; the library target remains `cozo`.
- Prefixed the RocksDB and binding/server Cargo packages with `cozo-ng-` to avoid publishing under upstream crate names, while preserving native library names where practical.
- Moved the maintained Rust workspace to edition 2024.
- Refreshed the lockfile to current versions compatible with the declared dependency ranges, including updates across Serde, Tokio, Pest, UUID, Rayon, SQLite, and WASM tooling.
- Renamed the RocksDB binding package to `cozo-ng-rocks 0.2.0` and retained its Rust library name `cozorocks`.

### Compatibility

- To migrate from the original `cozo` crate while keeping `use cozo::*`, declare `cozo = { package = "cozo-ng", version = "0.8" }`.
- This is a pre-1.0 release. API and storage-format compatibility is not guaranteed across minor releases; consult this changelog before upgrading.
