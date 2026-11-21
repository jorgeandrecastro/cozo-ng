# Changelog

Release notes for the `cozo-ng-rocks` crate. This crate is part of the
independently maintained Cozo-NG project and is not an official CozoDB upstream
release.

## 0.6.1

### Changed

- Aligned the adapter release with Cozo-NG `0.13.1`. No adapter API or storage
  behavior changes.

## 0.6.0

### Changed

- Aligned the adapter release with Cozo-NG `0.13.0`. No adapter API or storage
  behavior changes.

## 0.5.2

### Changed

- Published the adapter patch alongside the Cozo-NG `0.12.2` query-module
  refactoring. No adapter API or storage behavior changes.

## 0.5.1

### Changed

- Updated the adapter release for Cozo-NG `0.12.1`.

## 0.5.0

### Changed

- Updated the adapter release for Cozo-NG `0.12.0`.

## 0.4.1

### Changed

- Published the adapter release aligned with Cozo-NG `0.11.1`; no adapter API or storage behavior changes.

## 0.4.0

### Added

- Documented in the README how to use the RocksDB backend without compiling it, through the prebuilt `ghcr.io/jorgeandrecastro/cozo-bin` Docker image, and pointed to the repository `Dockerfile` as a reference for Debian build dependencies.

### Notes

- Documentation only. The adapter API, its storage behavior, and its version are unchanged, so no new crates.io release is required.

## 0.3.2

### Changed

- Aligned the adapter release with the Cozo-NG 0.11 engine. The RocksDB adapter API and storage behavior are unchanged.

## 0.3.1

### Changed

- Aligned this adapter release with the Cozo-NG 0.10 engine, whose database runtime was reorganized under `cozo-core/src/runtime/db/`. The refactor does not change the RocksDB adapter API.

## 0.3.0

- Updated build dependencies.
- Aligned the RocksDB adapter release with the Cozo-NG 0.9 engine.
- Kept the Rust library name `cozorocks`; this low-level implementation crate is not an upstream CozoDB release.

## 0.2.3

- Bumped the crate version for a Cozo-NG maintenance release.
- No Rust API or storage behavior changes were included in that version bump.