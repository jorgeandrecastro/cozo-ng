# Cozo-lib-java

This crate provides the JNI bindings for using Cozo in Java/JVM languages/Android.

Cozo-NG is an independent fork and community-maintained continuation of
[CozoDB](https://github.com/cozodb/cozo), not an official upstream release. It
sets its own Rust-first direction: an embedded data layer for AI agents and
GraphRAG combining relational data, Datalog knowledge graphs, and vector
search. Maintenance focuses on the embedded Rust engine; this JNI binding is
a community integration surface and may not have release parity with the Rust
crate. Contributions to build, test, document, or maintain it are welcome.

The `cozo-ng-java` crate is version `0.9.0` and targets the Cozo-NG 0.9
engine. Java/JVM packages and binaries linked below remain upstream
distributions, not Cozo-NG 0.9 releases. See the [crate changelog](CHANGELOG.md).

You do not use this crate directly. Instead, use:

* [Cozo Java](https://github.com/cozodb/cozo-lib-java) for Java or other JVM languages
* [Cozo Clojure](https://github.com/cozodb/cozo-clj) for Clojure on JVM (you can also use the Java library, but this one is nicer)
* [Cozo Android](https://github.com/cozodb/cozo-lib-android) for Android

These are upstream projects and binaries, not Cozo-NG releases. Keep reading
if you want to build this repository's JNI binding against the Cozo-NG Rust
engine or contribute to its maintenance.

## Building for JDK

With the Rust toolchain installed,
```bash
cargo build --release -p cozo-ng-java -F storage-rocksdb
```

## Building for Android

Building for Android is not easy, and we will be very sketchy.

The first thing to note is that you should omit `-F storage-rocksdb` from the build command above,
unless you are prepared to manually change lots of `build.rs` flags in 
[cozorocks](../cozorocks) to build the RocksDB dependency.

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