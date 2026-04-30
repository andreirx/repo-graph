#!/usr/bin/env bash
# test-rgr-crate.sh — cargo test wrapper for repo-graph-rgr
#
# Usage:
#   ./scripts/test-rgr-crate.sh                           # all tests
#   ./scripts/test-rgr-crate.sh --test modules_list       # filtered
#   ./scripts/test-rgr-crate.sh --test envelope_contract  # specific test file
#
# Fixes common operator errors:
#   - wrong manifest path
#   - wrong package name
#   - running from wrong directory

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/rust/Cargo.toml"
PACKAGE="repo-graph-rgr"

if [[ ! -f "$MANIFEST_PATH" ]]; then
    echo "error: manifest not found at $MANIFEST_PATH" >&2
    exit 2
fi

echo "Running: cargo test -p $PACKAGE $*"
echo "Manifest: $MANIFEST_PATH"
echo ""

exec cargo test --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" "$@"
