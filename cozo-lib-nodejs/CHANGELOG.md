# Changelog

Release notes for the `cozo-ng-node` Rust crate. Cozo-NG is an independent fork
of CozoDB. This crate release does not publish or update the upstream npm
package `cozo-node`.

## 0.12.2 (unreleased)

- Prepared the Node.js binding for the Cozo-NG `0.12.2` query-module
  refactoring and compilation fixes. No Node.js binding API changes.

## 0.12.1

- Updated the engine dependency to Cozo-NG `0.12.1`; no Node.js binding API changes.

## 0.12.0

- Updated the engine dependency to Cozo-NG `0.12.0`.

## 0.11.1

- Updated the engine dependency to Cozo-NG `0.11.1`; no Node.js binding API changes.

## 0.11.0

- Updated the Node.js binding crate to target the Cozo-NG `0.11.0` engine.
- No Node.js binding API changes are included in this release.

## 0.10.0

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was reorganized internally under `cozo-core/src/runtime/db/`; the Node.js binding API is unchanged.

## 0.9.0

- Updated the Node.js binding crate to target the Cozo-NG `0.9.0` engine.
- Retained the Rust library name `cozo_node`.
- Community contributions to build, test, document, and maintain the binding
  remain welcome.
