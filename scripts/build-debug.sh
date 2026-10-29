#!/usr/bin/env bash
# Debug build for the e2e harness.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
cargo build "$@"
