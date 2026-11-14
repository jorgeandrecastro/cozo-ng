# Changelog

Release notes for the `cozo-ng-python` Rust crate. Cozo-NG is an independent
fork of CozoDB. This crate release does not publish or update the upstream
PyPI distribution.

## 0.10.0

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was reorganized internally under `cozo-core/src/runtime/db/`; the WebAssembly binding API is unchanged.

## 0.9.0

- Updated the Python binding crate to target the Cozo-NG `0.9.0` engine.
- Kept PyO3 on the `0.22` API used by this binding; a future PyO3 major-version update requires migrating the binding code first.
- Retained the Rust library name `cozo_embedded`.
- Community contributions to build, test, document, and maintain Python
  bindings for Cozo-NG remain welcome.
