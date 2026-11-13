## Contributing

Thanks for your interest in contributing to Cozo-NG, a community-maintained fork of CozoDB.

Please open issues and pull requests in the [Cozo-NG repository](https://github.com/jorgeandrecastro/cozo-ng). Contributions to this fork do not require signing the original upstream project's CLA.

## Project priorities

Maintenance starts with the Rust core and standalone server. Changes to optional storage backends and language bindings should include build or test coverage for the affected target where feasible. Upstream-only npm and Swift distributions are not releases of this fork.

## Contribution checklist

- For substantial changes, open an issue first so the approach can be discussed before implementation.
- Include focused tests for behavior changes and run the narrowest relevant tests; run `cargo check --workspace` for cross-crate changes.
- Keep user-facing documentation and `CHANGELOG.md` in sync with behavior and compatibility changes.
- Treat storage formats and migrations as compatibility-sensitive; explain any migration required.
- Avoid unrelated dependency updates. For dependency refreshes, validate the workspace and record notable changes.

## Release policy

Releases are made from this repository and documented in `CHANGELOG.md`. Before publishing Rust crates, verify the workspace, run package or publish dry-runs, and ensure published path dependencies have already been released to crates.io. The project is pre-1.0: minor releases may contain breaking changes, which must be highlighted with migration guidance.