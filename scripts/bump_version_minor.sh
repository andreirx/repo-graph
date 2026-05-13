#!/bin/bash
# ── bump_version_minor.sh ──────────────────────────────────────────
#
# Increments the minor version component, resets patch to 0.
# Example: 0.1.5 → 0.2.0
#
# Usage: ./scripts/bump_version_minor.sh
#
# This script modifies rust/Cargo.toml only. It does NOT commit or tag.
# Use cut_release_minor.sh for the full release workflow.
#
# See: docs/slices/rel-support-1-version-authority.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_TOML="$REPO_ROOT/rust/Cargo.toml"

if [[ ! -f "$CARGO_TOML" ]]; then
    echo "ERROR: $CARGO_TOML not found"
    exit 1
fi

# Extract current version from [workspace.package] section
CURRENT=$(grep -A10 '^\[workspace\.package\]' "$CARGO_TOML" | grep '^version = "' | sed 's/version = "\(.*\)"/\1/')

if [[ -z "$CURRENT" ]]; then
    echo "ERROR: Could not extract version from [workspace.package] in $CARGO_TOML"
    exit 1
fi

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

# Strip any pre-release suffix from patch for parsing
if [[ "$PATCH" == *"-"* ]]; then
    PATCH="${PATCH%%-*}"
fi

# Increment minor, reset patch
NEW_MINOR=$((MINOR + 1))
NEW_VERSION="${MAJOR}.${NEW_MINOR}.0"

# Update Cargo.toml
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" "$CARGO_TOML"
else
    sed -i "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" "$CARGO_TOML"
fi

echo "Version bumped: ${CURRENT} → ${NEW_VERSION}"
