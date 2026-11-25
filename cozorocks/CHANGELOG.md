# Changelog

Release notes for the `cozo-ng-rocks` crate. This crate is part of the
independently maintained Cozo-NG project and is not an official CozoDB upstream
release.

## 0.7.0

### Added

- `DbBuilder::block_cache_size(bytes)` sets the size of the shared LRU block
  cache. The existing `DbOpts::block_cache_size` field is now honored; `0`
  keeps the RocksDB default cache.

- `WriteBatch`, created with `RocksDb::new_write_batch()` and written with
  `RocksDb::write(&mut batch)`. It groups many puts into a single atomic RocksDB
  write (one WAL append instead of one per key), using the same write path and
  options as `RocksDb::raw_put`. `WriteBatch::put`, `len`, `is_empty`,
  `size_in_bytes` and `clear` are available.
- `SstWriter` and `WriteBatch` are now re-exported from the crate root.

### Fixed

- `open_db` no longer discards the block-based table options when
  `use_bloom_filter` is enabled. The bloom filter is now applied on top of the
  existing table options (`block_size`, index/filter block caching and
  pinning, `format_version`, or the settings loaded from an options file)
  instead of replacing them with a default `BlockBasedTableOptions`.
- The block cache size was previously hard-coded to 1 GiB and only applied
  when an options file was loaded. It now uses the configured size and is
  applied in every case.
- When loading an options file, the block cache is applied to the column
  family that is actually used, and an options file without any column family
  or without block-based table options is handled without undefined behavior.

### Changed

- Databases opened with the bloom filter enabled now use the intended table
  options (16 KiB blocks, pinned L0 index/filter blocks). This only affects
  SST files written after the upgrade; existing files stay readable.
- No other adapter API changes.

### Notes

- Because the effective RocksDB table options change, this release bumps the
  minor version rather than the patch version.
- Engine-side callers should set an explicit block cache size through
  `block_cache_size`; Cozo-NG's RocksDB storage does so by default.

## 0.6.2

### Changed

- Added docs.rs metadata and rustdoc landing documentation.
- Added a `DOCS_RS` build guard so documentation builds do not compile
  vendored C++/RocksDB sources. No adapter API or storage behavior changes.

## 0.6.1

### Changed

- Aligned the adapter release with Cozo-NG `0.13.1`. No adapter API or storage
  behavior changes.

## 0.6.0

### Changed

- Aligned the adapter release with Cozo-NG `0.13.0`. No adapter API or storage
  behavior changes.

## 0.5.2

### Changed

- Published the adapter patch alongside the Cozo-NG `0.12.2` query-module
  refactoring. No adapter API or storage behavior changes.

## 0.5.1

### Changed

- Updated the adapter release for Cozo-NG `0.12.1`.

## 0.5.0

### Changed

- Updated the adapter release for Cozo-NG `0.12.0`.

## 0.4.1

### Changed

- Published the adapter release aligned with Cozo-NG `0.11.1`; no adapter API or storage behavior changes.

## 0.4.0

### Added

- Documented in the README how to use the RocksDB backend without compiling it, through the prebuilt `ghcr.io/jorgeandrecastro/cozo-bin` Docker image, and pointed to the repository `Dockerfile` as a reference for Debian build dependencies.

### Notes

- Documentation only. The adapter API, its storage behavior, and its version are unchanged, so no new crates.io release is required.

## 0.3.2

### Changed

- Aligned the adapter release with the Cozo-NG 0.11 engine. The RocksDB adapter API and storage behavior are unchanged.

## 0.3.1

### Changed

- Aligned this adapter release with the Cozo-NG 0.10 engine, whose database runtime was reorganized under `cozo-core/src/runtime/db/`. The refactor does not change the RocksDB adapter API.

## 0.3.0

- Updated build dependencies.
- Aligned the RocksDB adapter release with the Cozo-NG 0.9 engine.
- Kept the Rust library name `cozorocks`; this low-level implementation crate is not an upstream CozoDB release.

## 0.2.3

- Bumped the crate version for a Cozo-NG maintenance release.
- No Rust API or storage behavior changes were included in that version bump.