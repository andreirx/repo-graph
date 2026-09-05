#!/bin/bash
# ── cut_release_minor.sh ───────────────────────────────────────────
#
# Full minor release workflow:
# 1. Validate workspace (cargo check, clippy, test)
# 2. Bump minor version (0.1.5 → 0.2.0)
# 3. Commit version bump
# 4. Create annotated tag
# 5. Print push instructions
#
# Usage: ./scripts/cut_release_minor.sh
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

# Validate BEFORE bumping version (so failure doesn't leave dirty state)
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

# Bump version (only after validation passes)
echo ""
echo "=== Bumping minor version ==="
"$SCRIPT_DIR/bump_version_minor.sh"

# Re-resolve workspace member versions into Cargo.lock NOW — the bump edits Cargo.toml
# only; without this the lock's version stamps are stale at commit time and the release
# build dirties it afterwards (bitten v0.16.0, 2026-09-04). Offline: no dependency change.
(cd "$REPO_ROOT/rust" && cargo update --workspace --offline)

# Get new version
VERSION=$(grep -A10 '^\[workspace\.package\]' "$CARGO_TOML" | grep '^version = "' | sed 's/version = "\(.*\)"/\1/')
TAG="v${VERSION}"

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

# ── Post-release cleanup (operator directive 2026-08-26) ────────────────
# On SUCCESS (set -e guarantees we only reach here after validate/bump/tag),
# reclaim the debug build artifacts the validation run just produced
# (~tens of GB). Debug-only: target/release/ is PRESERVED so the follow-up
# dev-install-local.sh does not pay a full rebuild. Never fails the release:
# the tag already exists; cleanup trouble is reported, not fatal.
echo ""
echo "=== Post-release cleanup (debug artifacts) ==="
if "$SCRIPT_DIR/clean-build.sh"; then
    echo "  Cleanup done (release artifacts preserved for dev-install)."
else
    echo "  WARNING: clean-build.sh failed — release is DONE and tagged; run cleanup manually." >&2
fi
