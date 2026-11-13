# Cozo-NG Rust core (`cozo-ng`)

[![Crates.io](https://img.shields.io/crates/v/cozo-ng)](https://crates.io/crates/cozo-ng)

This crate contains the Rust implementation of Cozo-NG, a community continuation of CozoDB `0.7.6`. The original upstream has had no active maintenance for roughly two to three years; this fork continues maintenance independently.

The published package is named `cozo-ng`, while its Rust library name remains `cozo`. Existing code can therefore keep using `use cozo::*` by declaring the dependency as:

```toml
[dependencies]
cozo = { package = "cozo-ng", version = "0.8" }
```

See the [Cozo-NG repository](https://github.com/jorgeandrecastro/cozo-ng) for project status and contribution information.