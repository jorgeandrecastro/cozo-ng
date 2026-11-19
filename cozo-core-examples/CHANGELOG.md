# Changelog

## 0.2.1 

### Changed

- Updated the engine dependency to Cozo-NG `0.13.0`. The engine optimizes
  stored-relation writes and fixes stale FTS/LSH index cleanup,
  `:rm`/`:delete` with `force_collect` on temporary relations, and `:update`
  on corrupted values (now an error instead of a panic). No example behavior
  changes.
