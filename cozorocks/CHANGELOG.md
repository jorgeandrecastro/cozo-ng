# Changelog

Release notes for the `cozo-ng-rocks` crate. This crate is part of the
independently maintained Cozo-NG fork and is not an official CozoDB upstream
release.


## 0.3.1

### Changed

- Split the database runtime implementation into focused modules under `runtime/db/`, while preserving the existing public and crate-level module paths (cozo-ng) .

## 0.3.0

- Updated build dependencies.
- Aligned the RocksDB adapter release with the Cozo-NG 0.9 engine.
- Kept the Rust library name `cozorocks`; this low-level implementation crate is not an upstream CozoDB release.

## 0.2.3

- Bumped the crate version for a Cozo-NG maintenance release.
- No Rust API or storage behavior changes were included in that version bump.
