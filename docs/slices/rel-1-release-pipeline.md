# REL-1: Release Pipeline

Status: IMPLEMENTED (pipeline operational; version authority enforcement in REL-SUPPORT-1)
Depends: DIST-1
Track: Distribution / Install / Host Integration
Follow-on: REL-SUPPORT-1 (version authority enforcement)

## Objective

Establish GitHub Actions pipeline to build and publish cross-platform binary artifacts
for repo-graph releases.

## Scope

**REL-1 covers:**
- GitHub Actions workflow for building release artifacts
- Artifact naming, packaging, and checksums
- GitHub Releases publication
- Bootstrap installer script (downloads and installs binaries)

**REL-1 does NOT cover:**
- Daemon service registration (MAC-1, LINUX-1)
- Host integration patching (CLAUDE-1, CODEX-1)
- Code signing and notarization (MAC-2)
- Auto-update mechanism (UPDATE-1)

The install script in REL-1 is a **bootstrap installer** — it installs binaries and
creates directories, but daemon service setup and host integrations are stubs that
print guidance. Full installer functionality is completed by MAC-1, LINUX-1, CLAUDE-1,
and CODEX-1.

## Artifact Matrix

### Must-Have (v1)

| Platform | Architecture | Target Triple | Priority |
|----------|--------------|---------------|----------|
| macOS | ARM64 | `aarch64-apple-darwin` | P0 |
| Linux | x86_64 | `x86_64-unknown-linux-gnu` | P0 |

### Later

| Platform | Architecture | Target Triple | Priority |
|----------|--------------|---------------|----------|
| macOS | x86_64 | `x86_64-apple-darwin` | P1 |
| Linux | x86_64 musl | `x86_64-unknown-linux-musl` | P2 |
| Windows | x86_64 | `x86_64-pc-windows-msvc` | DEFERRED |

## Artifact Specification

### Naming Convention

```
rmap-{version}-{platform}-{arch}.tar.gz
rmap-{version}-{platform}-{arch}.tar.gz.sha256

Examples:
rmap-0.1.0-darwin-aarch64.tar.gz
rmap-0.1.0-darwin-aarch64.tar.gz.sha256
rmap-0.1.0-linux-x86_64.tar.gz
rmap-0.1.0-linux-x86_64.tar.gz.sha256
```

### Archive Contents

```
rmap-0.1.0-darwin-aarch64/
  rmap                      # CLI binary
  rmapd                     # Daemon binary
  LICENSE
  README.md
  CHANGELOG.md
```

### Checksum Format

```
sha256:abc123...  rmap-0.1.0-darwin-aarch64.tar.gz
```

## GitHub Actions Workflow

### Trigger Conditions

```yaml
on:
  push:
    tags:
      - 'v*'
  # Manual dispatch removed: releases must come from tags to ensure version integrity.
  # To release: create and push a tag (e.g., git tag v0.1.0 && git push origin v0.1.0)
```

### Build Matrix

```yaml
jobs:
  build:
    strategy:
      matrix:
        include:
          - os: macos-14          # ARM64 runner
            target: aarch64-apple-darwin
            platform: darwin
            arch: aarch64
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            platform: linux
            arch: x86_64
```

### Build Steps

```yaml
steps:
  - uses: actions/checkout@v4
  
  - name: Install Rust toolchain
    uses: dtolnay/rust-toolchain@stable
    with:
      targets: ${{ matrix.target }}
  
  - name: Build release binaries
    run: |
      cd rust
      # Build CLI binary from repo-graph-rgr crate
      cargo build --release --target ${{ matrix.target }} -p repo-graph-rgr
      # Build daemon binary from rmapd crate (separate crate per RMAPD-1)
      cargo build --release --target ${{ matrix.target }} -p rmapd
  
  - name: Package artifacts
    run: |
      mkdir -p dist/rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}
      cp rust/target/${{ matrix.target }}/release/rmap dist/rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}/
      cp rust/target/${{ matrix.target }}/release/rmapd dist/rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}/
      cp LICENSE README.md CHANGELOG.md dist/rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}/
      cd dist
      tar -czvf rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}.tar.gz rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}
      shasum -a 256 rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}.tar.gz > rmap-${{ env.VERSION }}-${{ matrix.platform }}-${{ matrix.arch }}.tar.gz.sha256
  
  - name: Upload artifacts
    uses: actions/upload-artifact@v4
    with:
      name: rmap-${{ matrix.platform }}-${{ matrix.arch }}
      path: dist/*.tar.gz*
```

### Release Job

```yaml
  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true
      
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          files: artifacts/*
          generate_release_notes: true
          draft: true  # Manual review before publish
```

## Version Management

### Version Authority

> **Note:** This section defines the target contract. Enforcement is implemented
> by REL-SUPPORT-1. Until that slice is complete, crate versions may not inherit
> from workspace and CI validation is not enforced.

The **workspace manifest** is the canonical source of version truth.

```toml
# rust/Cargo.toml
[workspace.package]
version = "0.1.0"
```

All release-bearing crates inherit this version:

```toml
# rust/crates/rgr/Cargo.toml
# rust/crates/rmapd/Cargo.toml
# rust/crates/daemon-runtime/Cargo.toml
[package]
version.workspace = true
```

**Git tag must equal manifest version.** The release workflow validates this.

### Version Bump Policy

| Bump Type | When | Initiator |
|-----------|------|-----------|
| Major | Breaking changes, major features | Human decision |
| Minor | New features, non-breaking | Human decision |
| Patch | Bug fixes, security patches | Human decision |

**Builds do not mutate version.** A build must be:
- Reproducible
- Side-effect free
- Non-mutating

Version changes happen only through intentional release-cut scripts.

### Release-Cut Workflow

1. Human runs `cut_release_{patch,minor,major}`
2. Script bumps workspace version
3. Script verifies crate inheritance
4. Script runs validation gates
5. Script commits version bump
6. Script creates tag `vX.Y.Z`
7. Human pushes commit + tag
8. Release workflow validates and publishes

See REL-SUPPORT-1 for implementation of bump and cut scripts.

### Release Consistency Checks

The release workflow **must fail** if any of these do not match:

| Source | Must Equal |
|--------|------------|
| Git tag | `v{workspace.package.version}` |
| `rmap --version` | `{workspace.package.version}` |
| `rmapd --version` | `{workspace.package.version}` |
| Archive filename | `rmap-{workspace.package.version}-...` |
| Release title | `repo-graph v{workspace.package.version}` |

This ensures the release is internally consistent and traceable.

### Tag Format

```
v{major}.{minor}.{patch}
v0.1.0
v1.0.0-beta.1
```

### Pre-release Support

```
v0.1.0-alpha.1  → Pre-release, not promoted to latest
v0.1.0-beta.1   → Pre-release, not promoted to latest
v0.1.0-rc.1     → Release candidate, not promoted to latest
v0.1.0          → Full release, promoted to latest
```

## Install Script

### Script Location

```
https://raw.githubusercontent.com/andreirx/repo-graph/main/scripts/install.sh
```

### Usage

```bash
curl -fsSL https://raw.githubusercontent.com/andreirx/repo-graph/main/scripts/install.sh | bash
```

### Script Behavior

```bash
#!/bin/bash
set -euo pipefail

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Normalize architecture
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Normalize platform
case "$OS" in
  darwin) PLATFORM="darwin" ;;
  linux) PLATFORM="linux" ;;
  *) echo "Unsupported platform: $OS"; exit 1 ;;
esac

# Get latest version or use specified
VERSION="${RMAP_VERSION:-latest}"
if [ "$VERSION" = "latest" ]; then
  VERSION=$(curl -fsSL https://api.github.com/repos/andreirx/repo-graph/releases/latest | grep tag_name | cut -d'"' -f4 | sed 's/^v//')
fi

# Download and verify
ARTIFACT="rmap-${VERSION}-${PLATFORM}-${ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/andreirx/repo-graph/releases/download/v${VERSION}/${ARTIFACT}"

echo "Downloading ${ARTIFACT}..."
curl -fsSL -o "${ARTIFACT}" "${DOWNLOAD_URL}"
curl -fsSL -o "${ARTIFACT}.sha256" "${DOWNLOAD_URL}.sha256"

echo "Verifying checksum..."
if ! shasum -a 256 -c "${ARTIFACT}.sha256"; then
  echo "Checksum verification failed!"
  exit 1
fi

# Extract and install
echo "Installing..."
tar -xzf "${ARTIFACT}"
# ... (rest of installation per DIST-1 contract)
```

## CI Integration

### PR Builds

PRs build but do not release:

```yaml
on:
  pull_request:
    paths:
      - 'rust/**'
      - '.github/workflows/release.yml'

jobs:
  build:
    # Same matrix as release, but no release job
```

### Nightly Builds (Optional)

```yaml
on:
  schedule:
    - cron: '0 0 * * *'  # Daily at midnight UTC

jobs:
  nightly:
    # Build and upload to nightly release
```

## Artifact Hosting

### Primary: GitHub Releases

All release artifacts hosted on GitHub Releases.

### Secondary: CDN (Future)

For faster downloads, consider CDN distribution:
- Cloudflare R2
- AWS S3 + CloudFront

## Security Considerations

### Checksum Verification

All artifacts include SHA-256 checksums. Install script verifies before installation.

### Reproducible Builds (Future)

Goal: Same source + same toolchain = identical binary.

Requirements:
- Pin Rust toolchain version in CI
- Pin all dependencies via Cargo.lock
- Document build environment

### Code Signing (MAC-2)

macOS code signing and notarization is separate slice (MAC-2).
Until MAC-2, macOS binaries are unsigned — users must allow in Security settings.

## Monitoring

### Release Metrics

Track via GitHub Insights:
- Download counts per artifact
- Download counts per version
- Geographic distribution (if using CDN)

### Build Health

- Build success rate
- Build duration trends
- Artifact size trends

## Out of Scope (REL-1)

- Code signing and notarization (MAC-2)
- Windows builds (WIN-1)
- Auto-update mechanism (UPDATE-1)
- CDN distribution (future optimization)

## Deliverables

1. `.github/workflows/release.yml`
2. `scripts/install.sh`
3. Release process documentation
4. Version management documentation

## Success Criteria

- Tag push triggers build for all platforms in matrix
- Artifacts are correctly named and checksummed
- GitHub Release is created with all artifacts
- Install script downloads and verifies correctly
- PR builds complete without releasing
