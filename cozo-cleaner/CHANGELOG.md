# Changelog

All notable changes to `cozo-ng-cleaner` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/) (below `1.0`, minor releases may contain breaking changes).

## 0.1.5

### Changed

- Aligned documentation and release metadata with Cozo-NG `0.14.0`.
- No cleaning API or behavior changes.

## 0.1.4

### Changed

- Added docs.rs metadata and rustdoc documentation for the crates.io release.
- No cleaning API or behavior changes.

## 0.1.3 

### Changed

- Prepared a patch release alongside Cozo-NG `0.13.0`. The standalone cleaner
  has no engine dependency, so the engine's stored-relation write changes do
  not affect its behavior or API.

## 0.1.1 - 2026-11-16

### Changed
- Updated the engine dependency to Cozo-NG `0.12.1`.
- No CLI API changes.

## 0.1.0 - 2026-11-15

First release.

### Added
- `Pipeline`: ordered list of cleaning steps, loadable from TOML (`from_toml`, `to_toml`), buildable in Rust, with
  `Pipeline::standard()` as a sensible default. Unknown steps and unknown fields are rejected.
- Steps: `trim`, `normalize_text` (Unicode NFC/NFKC, whitespace modes, control and zero-width characters, HTML stripping,
  lowercasing), `nulls`, `drop_empty_rows`, `rename`, `select`, `drop`, `snake_case_headers`, `cast` (int, float, bool, string,
  thousands separators and decimal commas), `fill`, `require`, `validate` (`min`, `max`, `min_len`, `max_len`, `pattern`,
  `one_of`), `dedup` (`first` / `last`), `add_id` and `chunk`.
- Row accounting: every input row is in the output, counted as removed, or listed in `Output::rejects` with its original
  1-based row number, the step and the reason. `Report` summarizes each step.
- `chunk_text`: overlapping text chunks cut at paragraph breaks, sentence ends or whitespace, safe on multi-byte text.
- Deterministic ids with `add_id` (FNV-1a 64, stable across platforms and Rust versions).
- `io` module: read and write CSV, TSV, JSON and JSONL; rejects as JSONL.
- `EXAMPLE_CONFIG`, a complete commented pipeline.
- Integration with the `cozo-ng` CLI as `cozo-ng clean` (feature `cleaner` of `cozo-ng-cli`).
