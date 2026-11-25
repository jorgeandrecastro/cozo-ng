# Cozo-NG core examples

This internal workspace crate contains examples using the Cozo-NG Rust engine.
The `0.2.3` crate uses Cozo-NG `0.14.0`, which adds RocksDB block cache and write batch support
and fixes stale FTS/LSH index cleanup, `:rm`/`:delete` with `force_collect` on
temporary relations, and `:update` on corrupted values (now an error instead
of a panic). These engine changes do not alter the examples' behavior.
See the [changelog](CHANGELOG.md).
