# Changelog

All notable changes to `cozo-ng-cli` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 0.2.0

### Added
- `clean`: data cleaning and preparation with [`cozo-ng-cleaner`](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cleaner/CHANGELOG.md) (normalization, typing,
  validation, deduplication, chunking, deterministic ids), writing a file or importing with `--into`; `--rejects`,
  `--dry-run`, `--example-config`. Behind the `cleaner` feature, enabled by default.

### Changed
- Updated the engine dependency to Cozo-NG `0.12.0`.
- Imports also coerce string values in typed columns to the destination relation's types.

## 0.1.0

### Added
- First crates.io release of the `cozo-ng` command-line tool.
- Query, REPL, schema, import/export, backup/restore, vector search,
  graph algorithms, remote server access, and shell completions.
