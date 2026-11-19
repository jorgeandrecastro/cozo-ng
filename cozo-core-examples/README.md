# Cozo-NG core examples

This internal workspace crate contains examples using the Cozo-NG Rust engine.
The `0.2.1` crate uses Cozo-NG `0.13.0`, which optimizes stored-relation writes
and fixes stale FTS/LSH index cleanup, `:rm`/`:delete` with `force_collect` on
temporary relations, and `:update` on corrupted values (now an error instead
of a panic). These engine changes do not alter the examples' behavior.
See the [changelog](CHANGELOG.md).
