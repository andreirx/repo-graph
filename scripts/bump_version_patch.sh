#!/bin/bash
# ── bump_version_patch.sh ──────────────────────────────────────────
#
# Increments the patch version component in the workspace manifest.
# Example: 0.1.0 → 0.1.1
#
# Usage: ./scripts/bump_version_patch.sh
#
# This script modifies rust/Cargo.toml only. It does NOT commit or tag.
# Use cut_release_patch.sh for the full release workflow.
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
# The version line looks like: version = "0.1.0"
CURRENT=$(grep -A10 '^\[workspace\.package\]' "$CARGO_TOML" | grep '^version = "' | sed 's/version = "\(.*\)"/\1/')

if [[ -z "$CURRENT" ]]; then
    echo "ERROR: Could not extract version from [workspace.package] in $CARGO_TOML"
    exit 1
fi

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

# Handle pre-release suffix if present (e.g., 0.1.0-alpha.1)
PRERELEASE=""
if [[ "$PATCH" == *"-"* ]]; then
    PRERELEASE="-${PATCH#*-}"
    PATCH="${PATCH%%-*}"
fi

# Increment patch
NEW_PATCH=$((PATCH + 1))
NEW_VERSION="${MAJOR}.${MINOR}.${NEW_PATCH}${PRERELEASE}"

# Update Cargo.toml using sed
# Match the version line within [workspace.package] section
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS sed requires backup extension with -i
    sed -i '' "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" "$CARGO_TOML"
else
    # GNU sed
    sed -i "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" "$CARGO_TOML"
fi

echo "Version bumped: ${CURRENT} → ${NEW_VERSION}"
