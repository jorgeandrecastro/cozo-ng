# cozo-ng CLI (`cozo-ng-cli`)

[![Crates.io](https://img.shields.io/crates/v/cozo-ng-cli)](https://crates.io/crates/cozo-ng-cli)

The command-line tool for [Cozo-NG](https://github.com/jorgeandrecastro/cozo-ng): one binary to query, import, export,
back up and explore a database, **embedded or remote**, with the same commands.
Version `0.2.1` targets the Cozo-NG `0.12.1` engine and includes the latest core
engine improvements; data cleaning is provided
through `cozo-ng-cleaner`. See the [CLI changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cli/CHANGELOG.md).

```text
cozo-ng -d sqlite:./app.db  query "?[count(id)] := *doc{id}"
cozo-ng -d http://localhost:9070 --token $TOKEN  query "::relations"
```

* **One URI for everything**: `mem:`, `sqlite:<file>`, `rocksdb:<dir>`, or `http(s)://` for a running `cozo-bin` server / Docker image.
* **Script-friendly**: table on a terminal, NDJSON when piped, plus `json` and `csv`; stdin input; proper exit codes.
* **Cozo diagnostics preserved**: the underlined, `miette`-style errors of the engine are shown as-is.
* **A real REPL**: multi-line input, persistent history, completion of relations / columns / functions, syntax colouring.
* **Built for AI and GraphRAG**: CSV/JSONL import with embeddings, data cleaning and chunking, HNSW index creation, vector search, PageRank / neighbours / shortest path.

## Install

From crates.io:

```sh
cargo install cozo-ng-cli                          # SQLite, graph algorithms and cleaner (default)
cargo install cozo-ng-cli --features full          # + RocksDB (needs a C++ toolchain, slow to compile)
```

From a workspace checkout, use `cargo install --path cozo-cli`.

| Feature     | Default | Effect |
|-------------|:-------:|--------|
| `sqlite`    | yes | `sqlite:` databases, `backup` / `restore` |
| `graph-algo`| yes | `graph …` commands and graph algorithms in scripts |
| `cleaner`   | yes | `clean` command ([`cozo-ng-cleaner`](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cleaner/README.md)) |
| `rocksdb`   | no  | `rocksdb:` databases |
| `full`      | no  | all of the above |

Shell completions: `cozo-ng completions bash|zsh|fish|powershell|elvish`.

## Choosing a database

The database comes from `-d/--db`, then `$COZO_DB`, then `default_db` in the config file, then falls back to an in-memory `mem:` database.

| URI | Meaning |
|-----|---------|
| `mem:` | in-memory, discarded on exit (handy for `?[] <- [[1,2,3]]`) |
| `sqlite:./app.db` | SQLite file (a bare `*.db` path is also taken as SQLite) |
| `rocksdb:./data` | RocksDB directory (a bare path without `.db` is taken as RocksDB) |
| `http://host:9070` | remote server; token via `--token` or `$COZO_AUTH` (sent as `x-cozo-auth`) |
| `prod` | an alias from the config file |

## Commands

| Command | What it does |
|---------|--------------|
| `query <script\|->` | run a script; `-p name=value` binds `$name`; `--immutable`; `--timer` |
| `run <file\|->` | run a script file |
| `repl` | interactive shell |
| `relations` | list relations |
| `schema show [REL]` / `schema dump [REL]` | columns, or `:create` statements that recreate the schema |
| `import <file\|-> -r REL` | CSV / JSON / JSONL → relation, in batches |
| `clean <file>` | clean and prepare data (normalize, type, validate, dedup, chunk), then write a file or `--into` a relation |
| `export -r REL` | relation → table / json / ndjson / csv |
| `backup <file>` / `restore <file>` | SQLite backup and restore |
| `explain <script>` | query plan |
| `doctor` | connectivity, size on disk, relations and row counts |
| `vec create-index` / `vec search` | HNSW vector index and nearest-neighbour search |
| `graph pagerank` / `neighbors` / `path` | graph algorithms on an edge relation |
| `completions <shell>` | shell completion script |

Global options: `-d/--db`, `--token`, `-f/--format table|json|ndjson|csv`, `--truncate N` (table cell width, `0` = no limit).

## Examples

### Query

```console
$ cozo-ng query "?[a, b] <- [[1, 'x'], [2, 'y']]" -f table
┌───┬───┐
│ a ┆ b │
╞═══╪═══╡
│ 1 ┆ x │
│ 2 ┆ y │
└───┴───┘
(2 rows)

$ echo '?[x] := x = $n' | cozo-ng query - -p n=42
{"x":42}
```

Errors keep Cozo's diagnostics and the process exits with status 1:

```console
$ cozo-ng query "?[x, Y] := x = 1, y = x + 1"
Error: eval::unbound_symb_in_head

  × Symbol 'Y' in rule head is unbound
   ╭────
 1 │ ?[x, Y] := x = 1, y = x + 1
   ·      ─
   ╰────
```

### Import and export

```sh
cozo-ng -d sqlite:app.db query ':create route {fr: String, to: String => dist: Float default 1.0}'
cozo-ng -d sqlite:app.db import routes.csv -r route          # header row = column names, values typed from the schema
cozo-ng -d sqlite:app.db export -r route -f csv > routes.csv
cozo-ng -d sqlite:app.db export -r route -c fr,to --limit 10 -f json
```

`import` reads CSV (`--delimiter`, `--no-header`), JSON (an array of objects or arrays) or JSONL, chooses the format from the file
extension (`-i` overrides it), and writes `--batch` rows (default 1000) per transaction with `--op put|insert|update|ensure|rm`.
Vector columns can be given as JSON arrays (`"[0.1, 0.2, 0.3]"` in CSV, a real array in JSON/JSONL).
Each batch is its own transaction: if a batch fails, earlier batches stay committed and the error says how many rows went in.

### Clean data before it goes in

`clean` runs a pipeline from [`cozo-ng-cleaner`](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cleaner/README.md) and then writes a file or imports straight into a relation.

```sh
cozo-ng clean --example-config > pipeline.toml                 # a complete, commented starting point
cozo-ng clean docs.csv -c pipeline.toml --dry-run              # report only
cozo-ng clean docs.csv -c pipeline.toml -o clean.jsonl --rejects rejects.jsonl
cozo-ng -d sqlite:kb.db clean docs.csv -c pipeline.toml --into chunk
```

```console
$ cozo-ng clean docs.csv -c pipeline.toml --rejects rejects.jsonl -q
{"chunk_id":"45b6c118185ec5cb","doc_id":1,"title":"Intro to Graphs","chunk_index":0,"body":"Graphs model relationships. …"}
…
```

Without `-c`, a standard pipeline runs (normalize text, placeholders to nulls, drop empty rows, drop duplicates).
Rejected rows keep their original row number and the reason. The report goes to stderr (hide it with `-q`).
With `--into`, the relation must exist; strings are converted to its column types. The `cleaner` feature is on by default
(`--no-default-features --features sqlite,graph-algo` builds without it).

### Schema as code

```console
$ cozo-ng schema dump > schema.cozo
$ cat schema.cozo
# relation route
{ :create route { fr: String, to: String => dist: Float default 1 } }

$ cozo-ng -d sqlite:copy.db run schema.cozo           # recreate it elsewhere
```

### Vector search (GraphRAG)

```sh
cozo-ng query ':create doc {id: Int => title: String, year: Int, emb: <F32; 384>}'
cozo-ng import docs.jsonl -r doc                                   # {"id":1,"title":"…","year":2020,"emb":[…]}
cozo-ng vec create-index doc --field emb --dim 384 --distance cosine

cozo-ng vec search doc --vector-file query.json -k 5 -f table
cozo-ng vec search doc --vector '[0.1, 0.2, …]' -k 10 --filter 'year > 2020' -c id,title
```

`vec search` returns the distance plus every non-vector column (override with `-c`), ordered by distance.
Options: `--index` (default `hnsw`), `--ef`, `--radius`, `--filter`. `--vector-file -` reads from stdin.
The CLI searches with an embedding you provide; it does not compute embeddings.

### Graph algorithms

The first two columns of the relation are taken as `from` and `to`.

```console
$ cozo-ng graph pagerank route --top 3
$ cozo-ng graph neighbors route FRA --hops 2
{"node":"CDG","hops":1}
{"node":"JFK","hops":1}
{"node":"LAX","hops":2}
$ cozo-ng graph path route FRA SFO
{"src":"FRA","dst":"SFO","distance":3.0,"path":["FRA","CDG","LAX","SFO"]}
```

Node values are read as JSON when they parse (`42` is an integer) and as strings otherwise (`FRA`).

### Remote servers and Docker

```sh
docker run --rm -p 127.0.0.1:9070:9070 -v cozo-data:/data ghcr.io/jorgeandrecastro/cozo-bin:main
TOKEN=$(docker run --rm -v cozo-data:/data --entrypoint cat ghcr.io/jorgeandrecastro/cozo-bin:main /data/db.rocksdb.cozo_auth)

export COZO_DB=http://127.0.0.1:9070 COZO_AUTH=$TOKEN
cozo-ng doctor
cozo-ng import routes.csv -r route
```

Over HTTP, `backup <path>` writes the file **on the server** (inside the container). `restore` is not available remotely
(start the server with `cozo-bin server --restore <file>` or restore into a local database).

## The REPL

```console
$ cozo-ng -d sqlite:app.db repl
cozo-ng 0.1.0 — sqlite:app.db  (.help for help, Ctrl-D to quit)
cozo> reachable[to] := *route{fr: 'FRA', to}
  ... reachable[to] := reachable[stop], *route{fr: stop, to}
  ... ?[count_unique(to)] := reachable[to]
```

When is a script submitted?

* An entry rule (`?[…] := …`), a `::sysop`, a `:create`-style statement or a `{ … }` block runs on Enter as soon as brackets and strings are closed.
* A line ending in `\` always continues (use it to add `:order` / `:limit` after the entry rule).
* A blank line always submits what has been typed.
* Rules without an entry rule keep waiting for more input.

Completion (Tab) knows relations after `*`, the columns of `*rel{`, `::` and `:` statements, built-in functions and graph algorithms.
History is saved in `<data dir>/cozo-ng/history`.

| Command | |
|---------|-|
| `.help` | help |
| `.relations` | list relations |
| `.schema [REL]` | `:create` statements |
| `.timer on\|off` | show timings |
| `.format table\|json\|ndjson\|csv` | output format |
| `.read FILE` | run a script file |
| `.output FILE\|off` | send results to a file |
| `.quit` | leave (Ctrl-D also works) |

`cozo-ng repl < session.txt` works too: the same rules apply, without the editor.

## Configuration

Optional file at `~/.config/cozo-ng/config.toml` (`%APPDATA%\cozo-ng\config.toml` on Windows):

```toml
default_db = "dev"
token = "…"                 # default token for remote targets

[aliases]
dev  = "sqlite:./dev.db"
prod = "https://cozo.example.com"
```

Environment: `COZO_DB`, `COZO_AUTH`.

## Notes and limits

* Names given on the command line (relations, columns, indices) are restricted to identifiers, because they are placed into scripts.
* `schema dump` rebuilds `:create` statements from the catalog (types, keys, default values); triggers and secondary indices are not included.
* `import` / `export` load the whole file or result in memory.
* `restore` works on embedded databases only.

## Development

```sh
cargo test -p cozo-ng-cli          # unit tests and end-to-end tests (assert_cmd, SQLite in temp dirs)
cargo run  -p cozo-ng-cli -- --help
```

Licensed under MPL-2.0, like the rest of Cozo-NG.
