# Changelog

Release notes for the `cozo-ng-swift` Rust crate. Cozo-NG is an independent
fork of CozoDB. This crate release does not publish or update upstream
CocoaPods packages or prebuilt Swift archives.

## 0.10.0

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was reorganized internally under `cozo-core/src/runtime/db/`; the WebAssembly binding API is unchanged.

## 0.9.0

- Updated the Swift binding crate to target the Cozo-NG `0.9.0` engine.
- Retained the Rust library name `cozo_swift`.
- Community contributions to build, test, document, and maintain the Swift
  integration remain welcome.
