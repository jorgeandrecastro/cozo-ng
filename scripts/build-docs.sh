#!/usr/bin/env bash

set -euo pipefail

# Build documentation locally in the same lightweight configuration used on
# docs.rs. Native RocksDB and optional heavy features are not needed for API docs.
export DOCS_RS=1
cargo doc --workspace --no-deps