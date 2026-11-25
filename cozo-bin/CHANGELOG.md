# Changelog

Release notes for the `cozo-ng-bin` Rust crate. This is part of the independent
Cozo-NG project, not an official CozoDB upstream release. Crate versions do not
necessarily correspond to downloadable standalone binary releases or to Docker
image tags.

## 0.16.0

- Updated the engine dependency to Cozo-NG `0.14.0`.
- No HTTP route, request/response format, or engine storage-format changes.

## 0.15.2

- Updated the engine dependency to Cozo-NG `0.13.2`.
- Added docs.rs metadata and rustdoc landing documentation.
- No HTTP route, request/response format, or engine storage-format changes.

## 0.15.1

- Updated the engine dependency to Cozo-NG `0.13.1`.
- No HTTP route, request/response format, or engine storage-format changes.

## 0.15.0 

### Changed

- Updated the engine dependency to Cozo-NG `0.13.0`. The engine optimizes
  stored-relation writes and fixes stale FTS/LSH index cleanup,
  `:rm`/`:delete` with `force_collect` on temporary relations, and `:update`
  on corrupted values (now an error instead of a panic).
- No HTTP route, request/response format, or engine storage-format changes.

## 0.14.0

Server hardening and robustness release. The engine dependency stays at
Cozo-NG `0.12.2`. The HTTP routes and request/response formats are unchanged;
see "Changed" for the few behaviour differences.

### Added

- New `server` option `--tx-timeout <SECONDS>` (default `300`): multi-statement
  transactions that stay idle for longer than this are aborted by a background
  task, so abandoned transactions no longer stay in memory forever.
- New `server` option `--max-body-mb <MIB>` (default `512`): maximum size of a
  request body.
- New `server` option `--no-compress` to disable HTTP response compression.
- Added `tests/smoke.sh`, a `curl`-based smoke test of the HTTP API (token
  authentication, queries, immutable mode, export, transaction commit, abort and
  timeout).

### Fixed

- Fixed the authentication logic: a valid `Authorization: Bearer` token from the
  token table was rejected whenever the request had a query string. The
  `x-cozo-auth` header, the `auth` query parameter and the Bearer token are now
  checked independently.
- Fixed a panic in `GET /changes/{relation}` when the SSE client disconnected.
- Fixed the SSE callback (`/changes`) and the fixed rule (`/rules`) staying
  registered when the client disconnected before the stream was first polled.
- Fixed requests waiting on a disconnected `/rules` client staying in memory;
  they now fail with an error when the stream is closed.

### Changed

- Authentication tokens are compared in constant time.
- Blocking work (token table lookup, starting, committing and aborting
  transactions, JSON conversion of query parameters and imports) now runs in the
  blocking thread pool instead of the async worker threads.
- `/rules` now runs its dispatch loop on a dedicated thread instead of a rayon
  worker, which is also used by the query engine.
- The `/changes` event buffer grew from 1 to 256 events, so slow SSE clients are
  less likely to stall writers.
- Request bodies are now limited to 512 MiB by default (they were unlimited).
  Use `--max-body-mb` to change the limit.
- Transaction ids returned by `POST /transact` are now 64-bit integers.

## 0.13.2

### Changed

- Prepared the standalone server for the Cozo-NG `0.12.2` engine, including
  its query-module refactoring and compilation fixes. No server API changes.

## 0.13.1

### Changed

- Updated the engine dependency to Cozo-NG `0.12.1`; no server API changes.

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