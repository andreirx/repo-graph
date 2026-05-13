#!/bin/bash
# ── cut_release_patch.sh ───────────────────────────────────────────
#
# Full patch release workflow:
# 1. Bump patch version (0.1.0 → 0.1.1)
# 2. Validate workspace (cargo check, clippy)
# 3. Commit version bump
# 4. Create annotated tag
# 5. Print push instructions
#
# Usage: ./scripts/cut_release_patch.sh
#
# Does NOT push automatically. Review the commit and tag before pushing.
#
# See: docs/slices/rel-support-1-version-authority.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_TOML="$REPO_ROOT/rust/Cargo.toml"

# Check for uncommitted changes
if ! git -C "$REPO_ROOT" diff --quiet || ! git -C "$REPO_ROOT" diff --cached --quiet; then
    echo "ERROR: Working directory has uncommitted changes."
    echo "Commit or stash changes before cutting a release."
    exit 1
fi

# Bump version
echo "=== Bumping patch version ==="
"$SCRIPT_DIR/bump_version_patch.sh"

# Get new version
VERSION=$(grep -A10 '^\[workspace\.package\]' "$CARGO_TOML" | grep '^version = "' | sed 's/version = "\(.*\)"/\1/')
TAG="v${VERSION}"

echo ""
echo "=== Validating workspace ==="
cd "$REPO_ROOT/rust"

echo "--- cargo fmt --check ---"
cargo fmt --all -- --check

echo "--- cargo check --workspace ---"
cargo check --workspace

echo "--- cargo clippy ---"
cargo clippy --all-targets -- -D warnings

echo "--- cargo test (skip parity) ---"
cargo test --workspace -- --skip parity

cd "$REPO_ROOT"

echo ""
echo "=== Creating release commit ==="
# Stage workspace manifest and lockfile (lockfile updates with workspace versions)
git add "$CARGO_TOML" "$REPO_ROOT/rust/Cargo.lock"
git commit -m "release: ${TAG}"

echo ""
echo "=== Creating annotated tag ==="
git tag -a "${TAG}" -m "Release ${TAG}"

echo ""
echo "============================================================"
echo "Release prepared: ${TAG}"
echo "============================================================"
echo ""
echo "To publish:"
echo "  git push origin main"
echo "  git push origin ${TAG}"
echo ""
echo "To abort (before pushing):"
echo "  git tag -d ${TAG}"
echo "  git reset --hard HEAD~1"
echo ""
