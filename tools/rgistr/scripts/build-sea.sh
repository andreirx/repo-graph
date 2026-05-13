#!/bin/bash
# ── build-sea.sh ───────────────────────────────────────────────────
#
# Build Node Single Executable Application (SEA) for rgistr.
#
# Requires:
#   - Node.js 20+ (with SEA support)
#   - npm run bundle (produces build/rgistr.bundle.cjs)
#   - postject (must be installed via npm ci - pinned in package.json)
#
# Usage:
#   ./scripts/build-sea.sh [platform] [arch]
#
# Examples:
#   ./scripts/build-sea.sh                    # Auto-detect current platform
#   ./scripts/build-sea.sh darwin aarch64     # macOS ARM64
#   ./scripts/build-sea.sh linux x86_64       # Linux x86_64
#
# Output:
#   build/rgistr-{platform}-{arch}
#
# See: docs/slices/rgistr-1-binary-packaging.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RGISTR_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$RGISTR_ROOT/build"
BUNDLE_FILE="$BUILD_DIR/rgistr.bundle.cjs"

# Platform detection
PLATFORM="${1:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
ARCH="${2:-$(uname -m)}"

# Normalize architecture
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Normalize platform
case "$PLATFORM" in
  darwin|Darwin) PLATFORM="darwin" ;;
  linux|Linux) PLATFORM="linux" ;;
  *) echo "Unsupported platform: $PLATFORM"; exit 1 ;;
esac

SEA_CONFIG="$BUILD_DIR/sea-config.json"
SEA_BLOB="$BUILD_DIR/sea-prep.blob"
OUTPUT_BINARY="$BUILD_DIR/rgistr-${PLATFORM}-${ARCH}"

echo "=== Building rgistr SEA binary ==="
echo "Platform: $PLATFORM"
echo "Architecture: $ARCH"
echo ""

# 1. Ensure bundle exists
if [[ ! -f "$BUNDLE_FILE" ]]; then
    echo "Bundle not found. Running npm run bundle..."
    cd "$RGISTR_ROOT"
    npm run bundle
fi

# 2. Create SEA configuration
echo "Creating SEA configuration..."
cat > "$SEA_CONFIG" << EOF
{
  "main": "$BUNDLE_FILE",
  "output": "$SEA_BLOB",
  "disableExperimentalSEAWarning": true,
  "useSnapshot": false,
  "useCodeCache": true
}
EOF

# 3. Generate SEA blob
echo "Generating SEA blob..."
node --experimental-sea-config "$SEA_CONFIG"

# 4. Copy Node binary
echo "Copying Node binary..."
cp "$(command -v node)" "$OUTPUT_BINARY"

# 5. Remove existing signature (macOS only)
if [[ "$PLATFORM" == "darwin" ]]; then
    echo "Removing existing signature (macOS)..."
    codesign --remove-signature "$OUTPUT_BINARY" 2>/dev/null || true
fi

# 6. Inject SEA blob using postject (pinned in package.json)
echo "Injecting SEA blob..."

# Use locally installed postject (no network fetch)
# postject must be in devDependencies
if [[ ! -f "$RGISTR_ROOT/node_modules/.bin/postject" ]]; then
    echo "ERROR: postject not found in node_modules"
    echo "Run 'npm install' first"
    exit 1
fi

# Different resource name for macOS vs Linux
if [[ "$PLATFORM" == "darwin" ]]; then
    "$RGISTR_ROOT/node_modules/.bin/postject" "$OUTPUT_BINARY" NODE_SEA_BLOB "$SEA_BLOB" \
        --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2 \
        --macho-segment-name NODE_SEA
else
    "$RGISTR_ROOT/node_modules/.bin/postject" "$OUTPUT_BINARY" NODE_SEA_BLOB "$SEA_BLOB" \
        --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2
fi

# 7. Re-sign (macOS only, ad-hoc signature)
if [[ "$PLATFORM" == "darwin" ]]; then
    echo "Re-signing binary (ad-hoc)..."
    codesign --sign - "$OUTPUT_BINARY"
fi

# 8. Make executable
chmod +x "$OUTPUT_BINARY"

# 9. Report results
BINARY_SIZE=$(ls -lh "$OUTPUT_BINARY" | awk '{print $5}')
echo ""
echo "=== Build complete ==="
echo "Binary: $OUTPUT_BINARY"
echo "Size: $BINARY_SIZE"
echo ""
echo "Test with:"
echo "  $OUTPUT_BINARY --version"
echo "  $OUTPUT_BINARY --help"
