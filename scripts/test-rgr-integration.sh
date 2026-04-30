#!/usr/bin/env bash
# test-rgr-integration.sh — run integration tests for repo-graph-rgr
#
# Usage:
#   ./scripts/test-rgr-integration.sh                     # all integration tests
#   ./scripts/test-rgr-integration.sh modules_list        # specific test file
#   ./scripts/test-rgr-integration.sh modules envelope    # multiple test files
#
# Integration tests are in rust/crates/rgr/tests/*.rs
# This wrapper filters to command-specific test files.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/rust/Cargo.toml"
PACKAGE="repo-graph-rgr"

if [[ ! -f "$MANIFEST_PATH" ]]; then
    echo "error: manifest not found at $MANIFEST_PATH" >&2
    exit 2
fi

# If no args, run all tests
if [[ $# -eq 0 ]]; then
    echo "Running: all integration tests for $PACKAGE"
    exec cargo test --manifest-path "$MANIFEST_PATH" -p "$PACKAGE"
fi

# Run each named test file
for test_name in "$@"; do
    echo "Running: cargo test --test $test_name"
    cargo test --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --test "$test_name"
done
