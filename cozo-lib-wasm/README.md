# Cozo-NG WebAssembly bindings

Cozo-NG is an independent, community-maintained continuation of
[CozoDB](https://github.com/cozodb/cozo). It sets its own Rust-first direction:
an embedded data layer for AI agents and
GraphRAG combining relational data, Datalog knowledge graphs, and vector
search. Maintenance focuses on the embedded Rust engine; this WebAssembly
binding is a community integration surface, not a primary support target.
Contributions to build, test, document, or maintain a Cozo-NG WebAssembly
integration are welcome.

The `cozo-ng-wasm` crate is version `0.13.2` and targets the Cozo-NG `0.13.2`
engine. The engine optimizes stored-relation writes and fixes stale FTS/LSH
index cleanup, `:rm`/`:delete` with `force_collect` on temporary relations, and
`:update` on corrupted values (now an error instead of a panic); the
WebAssembly binding API is unchanged. See the
[crate changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-lib-wasm/CHANGELOG.md).

This crate provides Cozo-NG WebAssembly modules for browsers. If you are
targeting NodeJS, use the [Cozo-NG Node.js binding guide](../cozo-lib-nodejs/README.md):
native code is still _much_ faster than WASM.

This document describes how to build and use the Cozo-NG WASM module.
For CozoScript syntax, consult the
[CozoDB v0.7 language documentation](https://docs.cozodb.org/en/latest/index.html).

## Installation

There is currently no Cozo-NG WebAssembly package published on npm. The
[`cozo-lib-wasm`](https://www.npmjs.com/package/cozo-lib-wasm) npm package is
a separate upstream distribution; installing it does not install this crate.
Build the package from this repository and include the generated JS and WASM
files directly in your project: see the `index.html` example
[here](https://rustwasm.github.io/docs/wasm-bindgen/examples/without-a-bundler.html) for
what is required in your code.

## Usage

After building the crate, import the generated module from `pkg/`:

```js
import init, {CozoDb} from "./pkg/cozo_lib_wasm.js";
```

and call

```js
let db;
init().then(() => {
    db = CozoDb.new();
    // db can only be used after the promise resolves 
})
```

## API

```ts
export class CozoDb {
    free(): void;

    static new(): CozoDb;

    run(script: string, params: string, immutable: boolean): string;

    export_relations(data: string): string;

    // Note that triggers are _not_ run for the relations, if any exists.
    // If you need to activate triggers, use queries with parameters.
    import_relations(data: string): string;
}
```

Note that this API is synchronous. If your computation runs for a long time, 
**it will block the main thread**. If you know that some of your queries are going to be heavy,
you should consider running Cozo in a web worker. However, the published module
may not work across browsers in web workers (look for the row "Support for ECMAScript
modules" [here](https://developer.mozilla.org/en-US/docs/Web/API/Worker/Worker#browser_compatibility)).

The next section contains some pointers for how to alleviate this, but expect a lot of work.

## Compiling

You will need to install [Rust](https://rustup.rs/), [NodeJS with npm](https://nodejs.org/),
and [wasm-pack](https://github.com/rustwasm/wasm-pack) first.

Build from this directory with

```bash
wasm-pack build --target web --release
```

and the environment variable `CARGO_PROFILE_RELEASE_LTO=fat`.

The important option is `--target web`: the above usage instructions only work for this target.
See the documentation [here](https://rustwasm.github.io/wasm-pack/book/commands/build.html#target).

if you are interested in running Cozo in a web worker and expect it to run across browsers,
you will need to use the `--target no-modules` option, and write a lot of gluing code.
See [here](https://rustwasm.github.io/wasm-bindgen/examples/wasm-in-web-worker.html) for tips.