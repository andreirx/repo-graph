#!/usr/bin/env bash
# clean-build.sh — clean Rust build artifacts to reclaim disk space
#
# Usage:
#   ./scripts/clean-build.sh           # clean debug only (default)
#   ./scripts/clean-build.sh --all     # clean debug + release
#   ./scripts/clean-build.sh --dry-run # show what would be removed
#
# What gets cleaned:
#   - target/debug/         (incremental compilation, test binaries)
#   - target/.rustc_info.json
#   - target/.cargo-lock
#   - target/release/ (only with --all)
#
# Note: rust-analyzer may hold files open. This script will attempt to
# stop it before cleaning. It will restart automatically when your
# editor reconnects.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_DIR="$REPO_ROOT/rust"
TARGET_DIR="$RUST_DIR/target"

CLEAN_ALL=false
DRY_RUN=false

usage() {
    echo "usage: $0 [--all] [--dry-run]" >&2
    echo "" >&2
    echo "  --all      — also remove target/release" >&2
    echo "  --dry-run  — show what would be removed without deleting" >&2
    exit 1
}

# Parse flags
while [[ $# -gt 0 ]]; do
    case "$1" in
        --all)
            CLEAN_ALL=true
            shift
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "error: unknown flag $1" >&2
            usage
            ;;
    esac
done

# Check target exists
if [[ ! -d "$TARGET_DIR" ]]; then
    echo "Nothing to clean: $TARGET_DIR does not exist"
    exit 0
fi

# Calculate size
size_of() {
    du -sh "$1" 2>/dev/null | cut -f1 || echo "0"
}

echo "=============================================="
echo "Rust build artifact cleanup"
echo "Target: $TARGET_DIR"
echo "=============================================="
echo ""

# Calculate total size before
TOTAL_BEFORE=$(size_of "$TARGET_DIR")
echo "Total target/ before: $TOTAL_BEFORE"
echo ""

# Collect paths to clean
PATHS_TO_CLEAN=()

if [[ -d "$TARGET_DIR/debug" ]]; then
    SIZE=$(size_of "$TARGET_DIR/debug")
    echo "  target/debug: $SIZE"
    PATHS_TO_CLEAN+=("$TARGET_DIR/debug")
fi

if [[ -f "$TARGET_DIR/.rustc_info.json" ]]; then
    PATHS_TO_CLEAN+=("$TARGET_DIR/.rustc_info.json")
fi

if [[ -f "$TARGET_DIR/.cargo-lock" ]]; then
    PATHS_TO_CLEAN+=("$TARGET_DIR/.cargo-lock")
fi

if [[ -d "$TARGET_DIR/tmp" ]]; then
    PATHS_TO_CLEAN+=("$TARGET_DIR/tmp")
fi

if [[ -d "$TARGET_DIR/flycheck0" ]]; then
    PATHS_TO_CLEAN+=("$TARGET_DIR/flycheck0")
fi

if [[ "$CLEAN_ALL" == "true" && -d "$TARGET_DIR/release" ]]; then
    SIZE=$(size_of "$TARGET_DIR/release")
    echo "  target/release: $SIZE"
    PATHS_TO_CLEAN+=("$TARGET_DIR/release")
fi

echo ""

if [[ ${#PATHS_TO_CLEAN[@]} -eq 0 ]]; then
    echo "Nothing to clean."
    exit 0
fi

if [[ "$DRY_RUN" == "true" ]]; then
    echo "DRY RUN — would remove:"
    for path in "${PATHS_TO_CLEAN[@]}"; do
        echo "  $path"
    done
    exit 0
fi

# Stop rust-analyzer to release file handles
echo "Stopping rust-analyzer (will restart automatically)..."
pkill -f rust-analyzer 2>/dev/null || true
sleep 1

# Perform cleanup
echo "Cleaning..."
for path in "${PATHS_TO_CLEAN[@]}"; do
    if [[ -e "$path" ]]; then
        echo "  removing: $(basename "$path")"
        rm -rf "$path"
    fi
done

echo ""

# Calculate size after
if [[ -d "$TARGET_DIR" ]]; then
    TOTAL_AFTER=$(size_of "$TARGET_DIR")
    echo "Total target/ after: $TOTAL_AFTER"
else
    echo "target/ removed entirely"
fi

echo ""
echo "Done. Next build will be slower (cold cache)."
