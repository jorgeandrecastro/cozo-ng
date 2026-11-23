# Cozo (standalone executable)

[![Crates.io](https://img.shields.io/crates/v/cozo-ng-bin)](https://crates.io/crates/cozo-ng-bin)
[![docs.rs](https://img.shields.io/docsrs/cozo-ng-bin?label=docs.rs)](https://docs.rs/cozo-ng-bin)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/jorgeandrecastro/cozo-ng/build.yml?branch=main)](https://github.com/jorgeandrecastro/cozo-ng/actions/workflows/build.yml)
[![Docker](https://img.shields.io/github/actions/workflow/status/jorgeandrecastro/cozo-ng/Docker.yaml?branch=main&label=docker)](https://github.com/jorgeandrecastro/cozo-ng/actions/workflows/Docker.yaml)
[![GitHub](https://img.shields.io/github/license/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/blob/main/LICENSE.txt)
[![server](https://img.shields.io/github/v/release/jorgeandrecastro/cozo-ng)](https://github.com/jorgeandrecastro/cozo-ng/releases)

This document describes how to set up cozo (standalone executable).
To learn how to use CozoDB (CozoScript), read the [docs](https://docs.cozodb.org/en/latest/index.html).

Cozo-NG is an independent, community-maintained continuation of
[CozoDB](https://github.com/cozodb/cozo). It
sets its own Rust-first direction: an embedded data layer for AI agents and
GraphRAG combining relational data, Datalog knowledge graphs, and vector
search.

The standalone server is an optional way to run Cozo-NG. The project's primary
maintenance target is the embedded Rust engine; the server is a Rust application
built on that engine.

The `cozo-ng-bin` crate is version `0.15.2` and targets the Cozo-NG `0.13.2`
engine. The engine optimizes stored-relation writes and fixes stale FTS/LSH
index cleanup, `:rm`/`:delete` with `force_collect` on temporary relations, and
`:update` on corrupted values (now an error instead of a panic). These changes
do not alter the server's HTTP interface or the engine storage format.

The `0.14.0` release hardens the HTTP server: fixed token authentication,
no more panic when an SSE client disconnects, blocking work moved off the async
threads, idle transaction timeout, and configurable body size limit and
compression. The HTTP routes are unchanged; see the
[crate changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-bin/CHANGELOG.md)
for the few behaviour differences.
This Rust crate version is separate from standalone binary release assets and
from Docker image tags.

## Crate architecture

This tree covers the standalone Rust executable only. The embedded engine it
uses is maintained separately in [`cozo-core`](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-core/README.md).

```text
cozo-bin/
├── src/
│   ├── main.rs       # CLI entry point; dispatches server and REPL commands
│   ├── server.rs     # HTTP API and server startup
│   ├── client.rs     # Client-side query helpers
│   ├── repl.rs       # Interactive terminal interface
│   ├── index.html    # Minimal browser-based query client
│   └── security.txt  # Server security contact resource
├── tests/
│   └── smoke.sh      # curl-based smoke test of the HTTP API
├── Cargo.toml        # Executable metadata, engine dependency, and features
├── README.md
└── CHANGELOG.md
```

The binary exposes two entry points: `server` for the HTTP API and `repl` for
interactive terminal queries. For a database-focused CLI with embedded and
remote backends, imports, exports, and additional commands, see the
[cozo-ng-cli crate](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-cli/README.md).

## Download

The standalone executable for Cozo-NG, when published, will be available from the [Cozo-NG release page](https://github.com/jorgeandrecastro/cozo-ng/releases).
Look for those with names `cozo-*`.
Those with names `cozo_all-*` supports additional storage backends
such as [TiKV](https://tikv.org/) storage, but are larger.

## Run with Docker

The easiest way to run the server without installing a Rust toolchain or compiling RocksDB is the Docker image. It contains the `cozo-bin` executable built with the `compact` and `storage-rocksdb` features, and stores its data in the `/data` volume.

```bash
docker run --rm -p 127.0.0.1:9070:9070 -v cozo-data:/data \
  ghcr.io/jorgeandrecastro/cozo-bin:main
```

Inside the container the server binds to `0.0.0.0`, so it requires the authentication token described in [the query API](#The-query-API). The token is generated on first start and stored in the data volume. Read it with:

```bash
docker run --rm -v cozo-data:/data --entrypoint cat \
  ghcr.io/jorgeandrecastro/cozo-bin:main /data/db.rocksdb.cozo_auth
```

Then send it in the `x-cozo-auth` header:

```bash
curl -X POST http://127.0.0.1:9070/text-query \
  -H 'content-type: application/json' \
  -H 'x-cozo-auth: YOUR_TOKEN' \
  -d '{"script": "?[] <- [[1, 2, 3]]", "params": {}}'
```

Notes:

* Publish the port on `127.0.0.1` as shown, or place the server behind a reverse proxy, before exposing it to a network.
* Arguments after the image name replace the default command, for example `... cozo-bin:main server --help`.
* Use a version or commit SHA tag instead of `main` for reproducible deployments.
* Images are built by the `Docker.yaml` workflow from the `Dockerfile` at the repository root.

## Starting the server

Run the `cozo-bin` command in a terminal:

```bash
./cozo-bin server
```

This starts an in-memory, non-persistent database.
For more options such as how to run a persistent database with other storage engines,
see `./cozo-bin server -h`

To stop Cozo, press `CTRL-C`, or send `SIGTERM` to the process with e.g. `kill`.

### Server options

Besides the engine, path, bind address and port options, the server accepts:

| Option | Default | Description |
|--------|---------|-------------|
| `--token-table <RELATION>` | none | Use the named relation (columns `token` and `mutable`) as a table of bearer tokens |
| `--tx-timeout <SECONDS>` | `300` | Abort multi-statement transactions idle for longer than this |
| `--max-body-mb <MIB>` | `512` | Maximum size of a request body |
| `--no-compress` | off | Disable HTTP response compression (useful on loopback, or behind a proxy that already compresses) |

## The REPL

Run `./cozo-bin repl` to enter a terminal-based REPL. The engine options can be used when
invoking the executable to choose the backend.

You can use the following meta ops in the REPL:

* `%set <KEY> <VALUE>`: set a parameter that can be used in queries.
* `%unset <KEY>`: unset a parameter.
* `%clear`: unset all parameters.
* `%params`: print all set parameters.
* `%run <FILE>`: run the script contained in `<FILE>`.
* `%import <FILE OR URL>`: import data in JSON format from the file or URL.
* `%save <FILE>`: the result of the next successful query will be saved in JSON format in a file instead of printed on
  screen. If `<FILE>` is omitted, then the effect of any previous `%save` command is nullified.
* `%backup <FILE>`: the current database will be backed up into the file.
* `%restore <FILE>`: restore the data in the backup to the current database. The current database must be empty.

## The query API

Queries are run by sending HTTP POST requests to the server.
By default, the API endpoint is `http://127.0.0.1:9070/text-query`.
A JSON body of the following form is expected:

```json
{
  "script": "<COZOSCRIPT QUERY STRING>",
  "params": {}
}
```

params should be an object of named parameters. For example, if params is `{"num": 1}`,
then `$num` can be used anywhere in your query string where an expression is expected.
Always use params instead of concatenating strings when you need parametrized queries.

Add `"immutable": true` to run a read-only script.

The HTTP API always responds in JSON. If a request is successful, then its `"ok"` field will be `true`,
and the `"rows"` field will contain the data for the resulting relation, and `"headers"` will contain
the headers. If an error occurs, then `"ok"` will contain `false`, the error message will be in `"message"`
and a nicely-formatted diagnostic will be in `"display"` if available.

> Cozo is designed to run in a trusted environment and be used by trusted clients.
> It does not come with elaborate authentication and security features.
> If you must access Cozo remotely, you are responsible for setting up firewalls, encryptions and proxies yourself.
>
> As a guard against users accidentally exposing sensitive data,
> If you bind Cozo to non-loopback addresses,
> Cozo will generate a token string and require all queries
> to provide the token string in the HTTP header field `x-cozo-auth`.
> The warning printed when you start Cozo with a
> non-default binding will tell you where to find the token string.
> This “security measure” is not considered sufficient for any purpose
> and is only intended as a last defence against carelessness.
>
> In some environments, setting the header may be difficult or impossible
> for some of the APIs. In this case you can pass the token in the query parameter `auth`.
>
> When the server is started with `--token-table`, a token from that relation can
> also be sent as `Authorization: Bearer <token>`.

## API

* `POST /text-query`, described above.
* `GET /export/{relations: String}`, where `relations` is a comma-separated list of relations to export.
* `PUT /import`, import data into the database. Data should be in `application/json` MIME type in the body,
  in the same format as returned in the `data` field in the `/export` API.
* `POST /backup`, backup database, should supply a JSON body of the form `{"path": <PATH>}`
* `POST /import-from-backup`, import data into the database from a backup. Should supply a JSON body
  of the form `{"path": <PATH>, "relations": <ARRAY OF RELATION NAMES>}`.
* `POST /transact?write=<bool>`, start a multi-statement transaction; returns its `id`.
  Run queries in it with `POST /transact/{id}` (same body as `/text-query`) and finish it with
  `PUT /transact/{id}` and a body of the form `{"abort": <bool>}`. A transaction left idle for longer
  than `--tx-timeout` seconds is aborted by the server.
* `GET /`, if you open this in your browser and open your developer tools, you will be able to use
  a very simple client to query this database.

> For `import` and `import-from-backup`, triggers are _not_ run for the relations, if any exists.
> If you need to activate triggers, use queries with parameters.

The following are experimental:

* `GET(SSE) /changes/{relation: String}` get changes when mutations are made against a relation, relies
  on [SSE](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events).

## Testing

With a server running, run the smoke test of the HTTP API:

```bash
tests/smoke.sh                                        # local server, no token needed
COZO_AUTH=YOUR_TOKEN tests/smoke.sh http://127.0.0.1:9070   # server that requires a token
SLOW=1 tests/smoke.sh                                 # also test the transaction timeout (start the server with --tx-timeout 5)
```

## Building

Building `cozo-bin` requires a [Rust toolchain](https://rustup.rs). Run

```bash
cargo build --release -p cozo-ng-bin -F compact -F storage-rocksdb
```

The executable is written to `target/release/cozo-bin`. To build the container image yourself, run `docker build -t cozo-bin .` from the repository root.