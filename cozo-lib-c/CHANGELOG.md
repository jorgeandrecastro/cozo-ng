# Changelog

Release notes for the `cozo-ng-c` Rust crate. Cozo-NG is an independent,
community-maintained fork of CozoDB; this crate's release does not imply a
matching prebuilt C library release.

## 0.10.0

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was reorganized internally under `cozo-core/src/runtime/db/`; the WebAssembly binding API is unchanged.

## 0.9.0

- Updated the C binding crate to target the Cozo-NG `0.9.0` engine.
- Retained the Rust library name `cozo_c`.
- C API consumers should check the Cozo-NG release assets for any matching
  prebuilt library.
