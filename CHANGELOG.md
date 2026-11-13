# Changelog

Notable changes to Cozo-NG are documented here. Cozo-NG is an independent community continuation of CozoDB; upstream release history is not the release history of this fork.

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
