# REL-SUPPORT-1: Version Authority and Release-Cut Scripts

Status: PLANNED
Depends: REL-1
Track: Distribution / Install / Host Integration

## Objective

Establish workspace-level version authority with enforcement scripts and CI validation.
Ensure all release artifacts are internally consistent and traceable to a single version source.

## Problem Statement

Current state (post v0.1.0):
- Crates have independent version declarations
- No automated version bump tooling
- Release workflow does not validate version consistency
- Tag/manifest/binary version agreement is not enforced

Target state:
- Single canonical version in workspace manifest
- All release crates inherit workspace version
- Bump scripts for controlled version changes
- Cut-release scripts for full release workflow
- CI validation blocks mismatched releases

## Scope

### In Scope

1. **Workspace Version Setup**
   - Add `[workspace.package] version` to `rust/Cargo.toml`
   - Convert release crates to `version.workspace = true`

2. **Bump Scripts**
   - `scripts/bump_version_patch.sh`
   - `scripts/bump_version_minor.sh`
   - `scripts/bump_version_major.sh`

3. **Cut-Release Scripts**
   - `scripts/cut_release_patch.sh`
   - `scripts/cut_release_minor.sh`
   - `scripts/cut_release_major.sh`

4. **Release Workflow Validation**
   - Tag == workspace version check
   - Binary `--version` == workspace version check
   - Release blocking on mismatch

### Out of Scope

- Build metadata / build numbers (future, if dev channel needed)
- Changelog automation (separate slice)
- Semantic release tooling (cargo-release, etc.)

## Implementation

### 1. Workspace Version

```toml
# rust/Cargo.toml
[workspace]
members = [...]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/andreirx/repo-graph"
```

### 2. Crate Inheritance

Release-bearing crates:

```toml
# rust/crates/rgr/Cargo.toml
[package]
name = "repo-graph-rgr"
version.workspace = true

# rust/crates/rmapd/Cargo.toml
[package]
name = "rmapd"
version.workspace = true

# rust/crates/daemon-runtime/Cargo.toml
[package]
name = "repo-graph-daemon-runtime"
version.workspace = true
```

Internal crates (not shipped) can remain at `version = "0.1.0"` or also inherit.
Decision: inherit for consistency, simpler maintenance.

### 3. Bump Scripts

Each bump script:
1. Reads current version from `rust/Cargo.toml`
2. Increments appropriate component
3. Writes updated version
4. Prints old → new version

```bash
# scripts/bump_version_patch.sh
#!/bin/bash
set -euo pipefail

CARGO_TOML="rust/Cargo.toml"

# Extract current version
CURRENT=$(grep -E '^version = "' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')

# Parse components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

# Increment patch
NEW_PATCH=$((PATCH + 1))
NEW_VERSION="${MAJOR}.${MINOR}.${NEW_PATCH}"

# Update Cargo.toml
sed -i.bak "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" "$CARGO_TOML"
rm -f "${CARGO_TOML}.bak"

echo "Version bumped: ${CURRENT} → ${NEW_VERSION}"
```

Similar for minor (reset patch to 0) and major (reset minor and patch to 0).

### 4. Cut-Release Scripts

Each cut-release script:
1. Calls appropriate bump script
2. Runs `cargo check --workspace` to validate
3. Runs `cargo clippy --all-targets -- -D warnings`
4. Commits with message `release: vX.Y.Z`
5. Creates tag `vX.Y.Z`
6. Prints push instructions (does not auto-push)

```bash
# scripts/cut_release_patch.sh
#!/bin/bash
set -euo pipefail

# Bump version
./scripts/bump_version_patch.sh

# Get new version
VERSION=$(grep -E '^version = "' rust/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
TAG="v${VERSION}"

# Validate
echo "Validating workspace..."
cd rust
cargo check --workspace
cargo clippy --all-targets -- -D warnings
cd ..

# Commit
git add rust/Cargo.toml
git commit -m "release: ${TAG}"

# Tag
git tag -a "${TAG}" -m "Release ${TAG}"

echo ""
echo "Release prepared: ${TAG}"
echo ""
echo "To publish:"
echo "  git push origin main"
echo "  git push origin ${TAG}"
```

### 5. Release Workflow Validation

Add validation step to `.github/workflows/release.yml`:

```yaml
- name: Validate version consistency
  run: |
    # Extract tag version
    TAG_VERSION="${GITHUB_REF_NAME#v}"
    
    # Extract workspace version
    MANIFEST_VERSION=$(grep -E '^version = "' rust/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    
    # Compare
    if [ "$TAG_VERSION" != "$MANIFEST_VERSION" ]; then
      echo "ERROR: Tag version ($TAG_VERSION) does not match manifest version ($MANIFEST_VERSION)"
      exit 1
    fi
    
    echo "Version consistency validated: $TAG_VERSION"

- name: Validate binary versions
  run: |
    TAG_VERSION="${GITHUB_REF_NAME#v}"
    
    # Build binaries first (already done in build step)
    RMAP_VERSION=$(./rust/target/${{ matrix.target }}/release/rmap --version | awk '{print $2}')
    RMAPD_VERSION=$(./rust/target/${{ matrix.target }}/release/rmapd --version | awk '{print $2}')
    
    if [ "$RMAP_VERSION" != "$TAG_VERSION" ]; then
      echo "ERROR: rmap --version ($RMAP_VERSION) does not match tag ($TAG_VERSION)"
      exit 1
    fi
    
    if [ "$RMAPD_VERSION" != "$TAG_VERSION" ]; then
      echo "ERROR: rmapd --version ($RMAPD_VERSION) does not match tag ($TAG_VERSION)"
      exit 1
    fi
    
    echo "Binary versions validated: $TAG_VERSION"
```

## Deliverables

1. `rust/Cargo.toml` with `[workspace.package] version`
2. Updated crate Cargo.toml files with `version.workspace = true`
3. `scripts/bump_version_patch.sh`
4. `scripts/bump_version_minor.sh`
5. `scripts/bump_version_major.sh`
6. `scripts/cut_release_patch.sh`
7. `scripts/cut_release_minor.sh`
8. `scripts/cut_release_major.sh`
9. Updated `.github/workflows/release.yml` with validation steps

## Success Criteria

1. `[workspace.package] version` is the single source of version truth
2. All release crates report matching versions via `--version`
3. Bump scripts correctly increment version components
4. Cut-release scripts produce valid, committable release state
5. Release workflow fails if tag != manifest != binary version
6. v0.2.0 release uses the new workflow successfully

## Migration

Since v0.1.0 is already released:
1. Implement workspace version setup
2. First use of cut-release scripts will be for v0.1.1 or v0.2.0
3. Validate new workflow before next release

## Future Extensions

- `cargo-release` integration (if needed)
- Changelog generation from conventional commits
- Build metadata for dev/nightly channels
- Automated dependency version bumps
