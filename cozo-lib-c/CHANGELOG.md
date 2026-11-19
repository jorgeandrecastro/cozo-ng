# Changelog

Release notes for the `cozo-ng-c` Rust crate. Cozo-NG is an independent,
community-maintained fork of CozoDB; this crate's release does not imply a
matching prebuilt C library release.

## 0.13.0 

- Updated the engine dependency to Cozo-NG `0.13.0`. The engine optimizes
  stored-relation writes and fixes stale FTS/LSH index cleanup,
  `:rm`/`:delete` with `force_collect` on temporary relations, and `:update`
  on corrupted values (now an error instead of a panic).
- No C binding API changes.

## 0.12.1

- Updated the engine dependency to Cozo-NG `0.12.1`; no C API changes.

## 0.12.0

- Updated the engine dependency to Cozo-NG `0.12.0`.

## 0.11.1

- Updated the engine dependency to Cozo-NG `0.11.1`; no C API changes.

## 0.11.0

- Updated the C binding crate to target the Cozo-NG `0.11.0` engine.
- No C API changes are included in this release.

## 0.10.0

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was reorganized internally under `cozo-core/src/runtime/db/`; the C binding API is unchanged.

## 0.9.0

- Updated the C binding crate to target the Cozo-NG `0.9.0` engine.
- Retained the Rust library name `cozo_c`.
- C API consumers should check the Cozo-NG release assets for any matching
  prebuilt library.
