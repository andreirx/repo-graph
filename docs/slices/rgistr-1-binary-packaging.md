# RGISTR-1: rgistr Binary Packaging

Status: PLANNED
Depends: REL-SUPPORT-1
Track: Distribution / Install / Host Integration

## Objective

Package `rgistr` (the LLM-powered code registration tool) as a standalone binary
and include it in the repo-graph release archives alongside `rmap` and `rmapd`.

## Problem Statement

Current state:
- `rgistr` is a Node.js ESM CLI requiring `node >=20`
- Users must have Node.js installed to run it
- Not included in release archives

Target state:
- `rgistr` packaged as self-contained binary via Node SEA
- Included in same release archive as `rmap`/`rmapd`
- No Node.js installation required on target machine

## Scope

### In Scope

1. **Bundle rgistr to single file** (esbuild)
2. **Node SEA binary generation** for macOS arm64 and Linux x86_64
3. **GitHub Actions integration** (extend release.yml)
4. **Archive inclusion** (same archive as rmap/rmapd)
5. **Installer update** (install rgistr alongside rmap/rmapd)
6. **Version injection** (replace hardcoded version with build-time constant)

### Out of Scope

- Separate installer for rgistr (uses same installer as rmap)
- Windows packaging (deferred with WIN-1)
- macOS code signing (deferred with MAC-2)
- Auto-update for rgistr specifically

## Technical Approach

### Node Single Executable Applications (SEA)

Node.js 20+ provides official SEA support. Process:

1. Bundle all modules into single JS file
2. Generate SEA configuration blob
3. Inject blob into Node binary copy
4. Result: standalone executable with embedded script

Reference: https://nodejs.org/api/single-executable-applications.html

### Why SEA over alternatives

| Option | Pros | Cons |
|--------|------|------|
| Node SEA | Official, maintained, no native addons in rgistr | Requires bundling first |
| pkg | Mature ecosystem | Third-party, maintenance risk |
| nexe | Alternative packager | Less active than SEA |
| Deno compile | Clean single binary | Would require porting |

**Decision:** Node SEA is the correct path for this codebase.

## Implementation

### Phase 1: Bundler Setup

Add esbuild to bundle rgistr:

```json
// tools/rgistr/package.json
{
  "devDependencies": {
    "esbuild": "^0.20.0"
  },
  "scripts": {
    "bundle": "esbuild src/cli.ts --bundle --platform=node --format=esm --outfile=build/rgistr.bundle.mjs"
  }
}
```

**Bundling contract:**
- Bundle ALL runtime dependencies into the SEA input
- No `--external` flags unless empirically proven necessary
- If bundling fails for a specific dependency, stop and re-scope before proceeding

`undici` is used by rgistr but is not a native-addon package. It should bundle normally.
Only if bundling is empirically broken should alternatives be considered.

### Phase 2: SEA Build Script

Create `tools/rgistr/scripts/build-sea.sh`:

```bash
#!/bin/bash
set -euo pipefail

# Requires: Node 20+, platform-specific

PLATFORM="${1:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
ARCH="${2:-$(uname -m)}"

# Normalize
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
esac

case "$PLATFORM" in
  darwin|Darwin) PLATFORM="darwin" ;;
  linux|Linux) PLATFORM="linux" ;;
esac

BUILD_DIR="build"
BUNDLE_FILE="${BUILD_DIR}/rgistr.bundle.mjs"
SEA_CONFIG="${BUILD_DIR}/sea-config.json"
SEA_BLOB="${BUILD_DIR}/sea-prep.blob"
OUTPUT_BINARY="${BUILD_DIR}/rgistr-${PLATFORM}-${ARCH}"

# 1. Bundle
npm run bundle

# 2. Create SEA config
cat > "${SEA_CONFIG}" << EOF
{
  "main": "${BUNDLE_FILE}",
  "output": "${SEA_BLOB}",
  "disableExperimentalSEAWarning": true
}
EOF

# 3. Generate blob
node --experimental-sea-config "${SEA_CONFIG}"

# 4. Copy Node binary
cp "$(command -v node)" "${OUTPUT_BINARY}"

# 5. Remove signature (macOS)
if [[ "$PLATFORM" == "darwin" ]]; then
  codesign --remove-signature "${OUTPUT_BINARY}"
fi

# 6. Inject blob
npx postject "${OUTPUT_BINARY}" NODE_SEA_BLOB "${SEA_BLOB}" \
  --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2

# 7. Re-sign (macOS, ad-hoc for now)
if [[ "$PLATFORM" == "darwin" ]]; then
  codesign --sign - "${OUTPUT_BINARY}"
fi

echo "Built: ${OUTPUT_BINARY}"
```

### Phase 3: GitHub Actions Integration

Extend `.github/workflows/release.yml`:

```yaml
- name: Setup Node.js
  uses: actions/setup-node@v4
  with:
    node-version: '20'

- name: Build rgistr binary
  working-directory: tools/rgistr
  run: |
    npm ci
    npm run bundle
    ./scripts/build-sea.sh ${{ matrix.platform }} ${{ matrix.arch }}

- name: Prepare artifact directory
  run: |
    # ... existing rmap/rmapd copy ...
    
    # Copy rgistr binary
    cp "tools/rgistr/build/rgistr-${{ matrix.platform }}-${{ matrix.arch }}" \
       "dist/${ARTIFACT_NAME}/rgistr"
```

### Phase 4: Version Alignment

rgistr must report the same version as rmap/rmapd for release coherence.

**Current state (to be replaced):**
`tools/rgistr/src/cli.ts` currently hardcodes `.version('0.2.0')`.
This must be removed and replaced with build-time injection.

**Gated on REL-SUPPORT-1:** Version injection depends on the canonical workspace
version existing in `rust/Cargo.toml`. Until REL-SUPPORT-1 is complete, this
phase cannot be implemented.

**Implementation:** Inject workspace version at bundle time:

```bash
# In bundle script
VERSION=$(grep -E '^version = "' ../../rust/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
esbuild src/cli.ts --bundle --platform=node --format=esm \
  --define:__VERSION__="'${VERSION}'" \
  --outfile=build/rgistr.bundle.mjs
```

Then in rgistr source (replace hardcoded version):
```typescript
declare const __VERSION__: string;
const VERSION = typeof __VERSION__ !== 'undefined' ? __VERSION__ : '0.0.0-dev';

// In CLI setup:
program.version(VERSION);
```

## Dependencies

### Build-time
- Node.js 20+ (pinned in workflow)
- esbuild
- postject (for SEA injection)

### Runtime (embedded in binary)
- Node.js runtime (packaged inside SEA binary)
- No external dependencies required

### User-provided at runtime
- `OPENAI_API_KEY` or equivalent for LLM access
- Network access to LLM endpoints

## Archive Structure

After RGISTR-1, release archives contain:

```
rmap-{version}-{platform}-{arch}/
  rmap                      # Rust CLI binary
  rmapd                     # Rust daemon binary
  rgistr                    # Node SEA binary
  LICENSE
  README.md
  CHANGELOG.md
```

## Installer Impact

The bootstrap installer (`scripts/install.sh`) must be updated to:

1. **Verify `rgistr` exists in archive** (alongside rmap/rmapd verification)
2. **Install `rgistr` to `~/.local/bin/rgistr`** (same as other binaries)
3. **Update install manifest** to track rgistr version

Current installer only handles `rmap` and `rmapd`. This is a required change, not optional.

## Testing

### Local validation
```bash
cd tools/rgistr
npm run bundle
./scripts/build-sea.sh
./build/rgistr-darwin-aarch64 --version
./build/rgistr-darwin-aarch64 --help
```

### CI validation
- Build succeeds on both platforms
- Binary executes `--version` without Node installed
- Binary executes `--help` without errors
- Archive contains all three binaries

## Deliverables

1. `tools/rgistr/package.json` — esbuild dependency, bundle script
2. `tools/rgistr/scripts/build-sea.sh` — SEA build script
3. `tools/rgistr/src/cli.ts` — hardcoded version replaced with injected constant
4. Updated `.github/workflows/release.yml` — rgistr build jobs
5. Updated `scripts/install.sh` — rgistr verification and installation
6. Version injection mechanism (gated on REL-SUPPORT-1)
7. Updated archive contents documentation

## Success Criteria

1. `rgistr` binary runs on macOS arm64 without Node.js installed
2. `rgistr` binary runs on Linux x86_64 without Node.js installed
3. `rgistr --version` reports same version as `rmap --version`
4. Release archive contains `rmap`, `rmapd`, and `rgistr`
5. CI workflow builds and packages all three binaries
6. Installer (`scripts/install.sh`) successfully installs all three binaries
7. Hardcoded version in `src/cli.ts` is removed

## Risk Factors

### Bundling failures
If any dependency fails to bundle:
1. Stop and investigate the specific failure
2. Do not speculatively externalize
3. Re-scope if necessary (may require dependency replacement)

`undici` is pure JavaScript and should bundle. Only escalate if empirically broken.

### SEA stability
Node SEA is marked "Active development". Monitor for breaking changes.
Pin Node version in CI to avoid surprises.

### Binary size
SEA binaries include full Node runtime (~40-80MB depending on platform).
This is acceptable for a developer tool. Document expected sizes.

## Future Extensions

- Windows SEA packaging (WIN-1)
- macOS notarization for rgistr (MAC-2)
- rgistr-specific update channel (if needed)
- rgistr shell completions
