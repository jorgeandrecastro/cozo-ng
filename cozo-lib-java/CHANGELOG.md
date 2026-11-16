# Changelog

Release notes for the `cozo-ng-java` JNI Rust crate. Cozo-NG is an independent
fork of CozoDB. This crate release does not publish or update the upstream
Java, Android, or other JVM packages listed in the README.

## 0.12.1

- Updated the engine dependency to Cozo-NG `0.12.1`; no Java/JNI API changes.

## 0.12.0

- Updated the engine dependency to Cozo-NG `0.12.0`.

## 0.11.1

- Updated the engine dependency to Cozo-NG `0.11.1`; no Java/JNI API changes.

## 0.11.0

- Updated the JNI crate to target the Cozo-NG `0.11.0` engine.
- No Java/JNI API changes are included in this release.

## 0.10.0

### Changed
- Aligned documentation and harmonized dependency versions across the project.
- Updated the engine dependency to Cozo-NG 0.10.1. Its database runtime was reorganized internally under `cozo-core/src/runtime/db/`; the Java/JNI API is unchanged.

## 0.9.0

- Updated the JNI crate to target the Cozo-NG `0.9.0` engine.
- Retained the Rust library name `cozo_java`.
- Community contributions to build, test, document, and maintain the binding
  remain welcome.
