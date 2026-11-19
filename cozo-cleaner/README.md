# cozo-ng-cleaner

[![Crates.io](https://img.shields.io/crates/v/cozo-ng-cleaner)](https://crates.io/crates/cozo-ng-cleaner)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng-cleaner?label=docs.rs)](https://docs.rs/cozo-ng-cleaner)

Data cleaning and preparation pipelines for [Cozo-NG](https://github.com/jorgeandrecastro/cozo-ng), AI agents and GraphRAG.
The `cozo-ng-cleaner` crate version is `0.1.3`. Cozo-NG `0.13.0` improves
stored-relation writes, but the standalone cleaner does not depend on the
engine, so those changes do not affect its behavior or API.
See the [changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cleaner/CHANGELOG.md) for release notes.

Real-world data arrives with stray whitespace, `N/A` strings, numbers stored as text, duplicates, HTML and documents too long
to embed. `cozo-ng-cleaner` turns it into rows you can trust, **and tells you exactly what it did to every row**.

* **Declarative**: a pipeline is an ordered list of steps, written in TOML or built in Rust.
* **Nothing disappears silently**: every input row ends up in the output, is counted as removed (empty rows, duplicates), or is
  listed in the rejects with its original row number and the reason.
* **RAG-ready**: HTML stripping, Unicode normalization, overlapping text chunks, deterministic ids.
* **Independent of the database**: no dependency on the Cozo engine. Use it as a library, or through `cozo-ng clean` from the CLI.

## From the command line

With the [`cozo-ng` CLI](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cli/README.md) (feature `cleaner`, on by default):

```sh
cargo install cozo-ng-cli
cozo-ng clean data.csv                                    # standard pipeline, result on stdout (NDJSON)
cozo-ng clean data.csv -c pipeline.toml -o clean.jsonl --rejects rejects.jsonl
cozo-ng clean data.csv -c pipeline.toml --dry-run         # report only, write nothing
cozo-ng -d sqlite:kb.db clean docs.csv -c rag.toml --into chunk   # clean and import in one go
cozo-ng clean --example-config > pipeline.toml            # a complete, commented starting point
```

`--into` writes to an existing relation and converts strings to the column types (`Int`, `Float`, `Bool`, vectors…).

## As a library

```toml
[dependencies]
cozo-ng-cleaner = "0.1"
```

```rust
use cozo_ng_cleaner::{io, Pipeline};

let table = io::read_table("id,name\n1,  Ada \n1,  Ada \n2,N/A\n", io::Format::Csv)?;

let out = Pipeline::from_toml(r#"
    [[steps]]
    op = "normalize_text"
    [[steps]]
    op = "nulls"
    [[steps]]
    op = "cast"
    types = { id = "int" }
    [[steps]]
    op = "dedup"
    by = ["id"]
"#)?.run(table)?;

println!("{}", out.report);          // what every step did
for r in &out.rejects { println!("row {}: {}", r.row, r.reason); }
io::write_table(&out.table, io::Format::Jsonl, &mut std::io::stdout())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Pipelines can also be built in code: `Pipeline::new().step(Step::normalize_text()).step(Step::DropEmptyRows)`,
or start from `Pipeline::standard()`.

## Steps

Each step is a `[[steps]]` table with `op = "<name>"`. Steps with `columns` apply to every column when it is empty or absent.
Unknown steps and unknown fields are errors, so typos never go unnoticed.

| `op` | Parameters | Effect |
|------|------------|--------|
| `trim` | `columns` | strip leading / trailing whitespace |
| `normalize_text` | `columns`, `unicode` (`nfc`\|`nfkc`\|`none`), `whitespace` (`collapse`\|`keep_lines`\|`none`), `strip_control`, `strip_zero_width`, `strip_html`, `lowercase` | Unicode normalization, whitespace, control / zero-width characters, HTML |
| `nulls` | `columns`, `values` | placeholder strings become nulls; default `""`, `null`, `none`, `na`, `n/a`, `nan`, `nil`, `-` (trimmed, case-insensitive) |
| `drop_empty_rows` | | remove rows whose cells are all null or blank |
| `rename` | `map = { old = "new" }` | rename columns |
| `select` | `columns` | keep these columns, in this order |
| `drop` | `columns` | remove columns |
| `snake_case_headers` | | `Unit Price (EUR)` → `unit_price_eur`, `firstName` → `first_name` |
| `cast` | `types = { col = "int"\|"float"\|"bool"\|"string" }`, `on_error` (`null`\|`reject`\|`keep`), `decimal_comma` | convert types; accepts `1 234`, `1,234.5`, `+7`, `oui`/`non`, `yes`/`no`… |
| `fill` | `values = { col = value }` | replace nulls |
| `require` | `columns` | reject rows with a null or blank cell |
| `validate` | `rules = [{ column, min, max, min_len, max_len, pattern, one_of }]` | reject rows that break a rule (nulls are skipped; combine with `require`) |
| `dedup` | `by` (empty = whole row), `keep` (`first`\|`last`) | remove duplicates |
| `add_id` | `column` (default `id`), `from` (empty = all columns), `prefix` | deterministic id column |
| `chunk` | `column`, `size` (default 1000), `overlap` (default 100), `index_column` (default `chunk_index`, `""` = none) | one row per text chunk |

Steps run in order, so order matters: normalize before you test for nulls, cast before you validate numbers, dedup before you chunk.

### Preparing documents for embedding

```toml
[[steps]]
op = "snake_case_headers"

[[steps]]
op = "normalize_text"
whitespace = "keep_lines"      # keep paragraph breaks so chunks can follow them
strip_html = true

[[steps]]
op = "nulls"

[[steps]]
op = "cast"
types = { doc_id = "int" }

[[steps]]
op = "require"
columns = ["doc_id", "body"]

[[steps]]
op = "dedup"
by = ["doc_id"]

[[steps]]
op = "chunk"
column = "body"
size = 800
overlap = 100

[[steps]]
op = "add_id"
column = "chunk_id"
from = ["doc_id", "chunk_index"]
```

Run on a small file, the CLI reports:

```text
rows: 4 in, 3 out, 1 rejected
   1. snake_case_headers 4 cells changed
   2. normalize_text     4 cells changed
   3. nulls              2 cells changed
   4. cast               3 cells changed
   5. require            1 rows rejected
   6. dedup              1 rows removed
   7. chunk              1 rows added
   8. add_id             3 cells changed
```

and the rejects file keeps the original row number and the row as it was:

```json
{"row":4,"step":"require","reason":"column `doc_id` is empty","record":{"doc_id":null,"title":"Orphan","body":"no id here","published":"2024-03-01"}}
```

## Behaviour worth knowing

* **Chunking** counts characters, not tokens. Cuts prefer a paragraph break, then a sentence end, then any whitespace, within the last
  40% of the window; chunks never start in the middle of a word, and a word longer than `size` is cut. Use `whitespace = "keep_lines"`
  in `normalize_text` beforehand if you want paragraph-aware cuts, since the default `collapse` removes line breaks.
  Rows whose text is null or empty are rejected rather than dropped silently.
* **Ids** are FNV-1a 64-bit hashes (16 hex digits) of the chosen columns. They are stable across runs, platforms and Rust versions, which
  makes re-imports idempotent. They are not cryptographic and not collision-proof; do not use them for security.
* **CSV and TSV** cells are read as strings; add a `cast` step to get numbers and booleans. JSON and JSONL keep their types, and the
  columns are the union of all keys.
* **Decimal commas**: with `decimal_comma = true`, `.` is dropped as a thousands separator, so `2.50` becomes `250`. Without it, `,` is
  dropped, so `1,5` becomes `15`. Pick the option that matches your data.
* **Memory**: tables are processed in memory.
* **Not included**: date parsing, language detection, embeddings.

## API overview

| Item | Purpose |
|------|---------|
| `Pipeline` | `from_toml`, `to_toml`, `standard`, `step`, `check`, `run` |
| `Step`, `Rule`, … | the step definitions (also `Serialize` / `Deserialize`) |
| `Table` | columns + rows of `serde_json::Value` |
| `Output` | `table`, `rejects`, `report` |
| `Report`, `StepReport`, `Reject` | accounting of what happened |
| `io::{read_table, write_table, write_rejects, Format}` | CSV, TSV, JSON, JSONL |
| `chunk_text`, `normalize`, `snake_case`, `fnv1a_hex` | the building blocks, usable on their own |
| `EXAMPLE_CONFIG` | the commented example pipeline |

Errors are `CleanError`: `Config`, `UnknownColumn { step, column }`, `Io`, `Parse`.

## Development

```sh
cargo test -p cozo-ng-cleaner
```

Licensed under MPL-2.0, like the rest of Cozo-NG.
