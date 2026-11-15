# Changelog

Release notes for the `cozo-ng-bin` Rust crate. This is part of the independent
Cozo-NG fork, not an official CozoDB upstream release. Crate versions do not
necessarily correspond to downloadable standalone binary releases or to Docker
image tags.

## 0.13.0

### Changed

- Updated the engine dependency to Cozo-NG `0.12.0`.

## 0.12.1

### Changed

- Updated the engine dependency to Cozo-NG `0.11.1` for the crates.io release; no server API changes.

## 0.12.0

### Fixed

- Updated the HTTP server routes to the axum 0.8 path syntax (`/{param}` instead of `/:param`) for `/export`, `/changes`, `/rules`, `/rule-result` and `/transact`. With the previous syntax the router panicked at startup.

### Added

- Added a Docker image of the standalone server, built from the `Dockerfile` at the repository root with the `compact` and `storage-rocksdb` features and published to `ghcr.io/jorgeandrecastro/cozo-bin` by the `Docker.yaml` workflow.
- Documented the Docker image in the README, including how to read the authentication token that the server generates when it binds to a non-loopback address.

### Changed

- Corrected the README to use the real executable name `cozo-bin` and the build command `cargo build --release -p cozo-ng-bin -F compact -F storage-rocksdb`.

## 0.11.0

### Changed

- Updated the standalone server crate release to follow the Cozo-NG 0.10 engine, whose database runtime was reorganized under `cozo-core/src/runtime/db/` without changing the engine's public API.

## 0.10.0

- Updated the server crate to use the Cozo-NG `0.10.0` engine.
- Kept the standalone executable name `cozo-bin` and the `server` and `repl` entry points.
- This crate release does not imply a separate prebuilt server binary release.

## 0.9.0

- Updated the server crate to use the Cozo-NG `0.9.0` engine.
- Retained the executable name `cozo-bin`.
- No separate prebuilt server binary release is implied by this crate version.