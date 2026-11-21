# Cozo-NG Java/JNI bindings

This crate provides the JNI bindings for using Cozo in Java/JVM languages/Android.

Cozo-NG is an independent, community-maintained continuation of
[CozoDB](https://github.com/cozodb/cozo). It sets its own Rust-first direction:
an embedded data layer for AI agents and
GraphRAG combining relational data, Datalog knowledge graphs, and vector
search. Maintenance focuses on the embedded Rust engine; this JNI binding is
a community integration surface and may not have release parity with the Rust
crate. Contributions to build, test, document, or maintain it are welcome.

The `cozo-ng-java` crate is version `0.13.1` and targets the Cozo-NG `0.13.1`
engine. The engine optimizes stored-relation writes and fixes stale FTS/LSH
index cleanup, `:rm`/`:delete` with `force_collect` on temporary relations, and
`:update` on corrupted values (now an error instead of a panic); the Java
binding API is unchanged. Java/JVM packages and binaries linked below are
separate upstream distributions, not Cozo-NG releases. See the
[crate changelog](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozo-lib-java/CHANGELOG.md).

## Using Cozo-NG from the JVM

This repository contains the Cozo-NG JNI native bridge, but does not publish a
Cozo-NG Java artifact or a supported public Java wrapper. The Java, Clojure,
and Android projects below are separate upstream projects and do not use this
crate or provide Cozo-NG builds:

* [Cozo Java](https://github.com/cozodb/cozo-lib-java)
* [Cozo Clojure](https://github.com/cozodb/cozo-clj)
* [Cozo Android](https://github.com/cozodb/cozo-lib-android)

Use the build instructions below to build the native bridge from this
repository, or contribute a maintained JVM wrapper.

## Building for JDK

With the Rust toolchain installed,
```bash
cargo build --release -p cozo-ng-java -F storage-rocksdb
```

## Building for Android

Building for Android is not easy, and we will be very sketchy.

The first thing to note is that you should omit `-F storage-rocksdb` from the build command above,
unless you are prepared to manually change lots of `build.rs` flags in 
[cozorocks](https://github.com/jorgeandrecastro/cozo-ng/blob/main/cozorocks/README.md) to build the RocksDB dependency.

Then, in addition to adding Android targets to the Rust toolchain, 
you also need to set up the Android NDK
cross-compilation and libraries paths, etc.
This is notoriously hard to get right, but fortunately 
you can just use the Docker image [here](https://github.com/cross-rs/cross)
which has everything set up for you.

When everything is set up correctly, the following command show complete without errors:

```bash
for TARGET in aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android; do
  cross build -p cozo-ng-java --release --target=$TARGET
done
```

For running on modern Android phones, the single target `aarch64-linux-android` is probably enough.