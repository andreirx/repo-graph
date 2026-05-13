# DIST-1: Distribution and Install Contract

Status: IMPLEMENTED
Depends: None
Track: Distribution / Install / Host Integration

## Objective

Define the contract for repo-graph distribution and installation. This is a design
slice — it produces specifications that implementation slices (MAC-1, LINUX-1) build against.

## Core Decisions

### D1: Binary-First Distribution

Users should not need a Rust toolchain to install repo-graph.

**Rationale:**
- If the goal is adoption on real projects, requiring toolchain is self-sabotage
- Toolchain detection is for diagnostics and optional source-build fallback
- Not a prerequisite for basic install

**Contract:**
- Default install path downloads pre-built binary
- Source build is opt-in fallback for security-conscious users
- Installer detects platform/arch and selects correct artifact

### D2: User-Local Install by Default (LOCKED)

**Decision:** User-local installation without sudo is the default.

**Rationale:**
- Cleaner privilege boundary (no mixed user/system ownership)
- Easier uninstall and repair
- No admin friction for initial adoption
- Host integration (Claude/Codex hooks) is user-scoped anyway
- User daemon services (launchd user agent, systemd --user) match this model

**Contract:**
- Binaries install to user-owned location (not /usr/local/bin)
- No sudo required for default install
- All directories are user-owned
- Daemon runs as user service, not system service
- System-wide install is opt-in (`--system` flag) for multi-user machines

**User-local binary locations:**
- `~/.local/bin/rmap` — CLI binary
- `~/.local/bin/rmapd` — daemon binary

Add `~/.local/bin` to PATH in shell profile.

### D3: Platform-Native Directory Layout (LOCKED)

Use platform-native paths, not XDG-on-macOS.

**macOS (native paths):**
```
~/.local/bin/
  rmap                    # CLI binary
  rmapd                   # Daemon binary

~/Library/Application Support/repo-graph/
  config.toml
  hooks.toml
  install-manifest.json

~/Library/Logs/repo-graph/
  daemon.log
  hooks.log

~/Library/Application Support/repo-graph/
  databases/
  sessions/

~/Library/LaunchAgents/
  com.repo-graph.rmapd.plist
```

**Linux (XDG paths):**
```
~/.local/bin/
  rmap                    # CLI binary
  rmapd                   # Daemon binary

~/.config/rmap/
  config.toml
  hooks.toml
  install-manifest.json

~/.local/share/rmap/
  logs/
  databases/
  sessions/

~/.config/systemd/user/
  rmapd.service
```

### D4: Toolchain Detection Contract

Installer detects development toolchains for diagnostics and optional source fallback.

**Detected toolchains:**

| Toolchain | Detection Method | Required For |
|-----------|------------------|--------------|
| Rust | `rustc --version`, `cargo --version` | Source build fallback |
| Node.js | `node --version` | TypeScript CLI (legacy) |
| npm | `npm --version` | TypeScript CLI (legacy) |

**Detection behavior:**
1. Run detection commands, capture version or "not found"
2. Report results in installer output and manifest
3. Absence is **not fatal** — binary install proceeds
4. Source fallback requires Rust; if missing, source fallback unavailable

**Detection states:**

| State | Meaning | Behavior |
|-------|---------|----------|
| `found` | Command succeeded, version captured | Available for use |
| `not_found` | Command not in PATH | Not available |
| `unusable` | Command found but failed (e.g., broken install) | Treated as not found |

**Detection output:**
```
Toolchain detection:
  Rust: 1.75.0 (cargo 1.75.0)
  Node.js: not found
  npm: not found

Note: Node.js/npm not required for rmap (Rust binary).
```

**Manifest records:**
```json
{
  "toolchain_detection": {
    "rust": {"status": "found", "version": "1.75.0", "path": "{detected_path}"},
    "cargo": {"status": "found", "version": "1.75.0", "path": "{detected_path}"},
    "node": {"status": "not_found"},
    "npm": {"status": "not_found"}
  }
}
```

**Source fallback behavior:**

Source fallback is opt-in via `--source` flag. It is NOT automatic.

```
$ curl -fsSL https://raw.githubusercontent.com/{OWNER}/repo-graph/main/scripts/install.sh | bash -s -- --source
```

When `--source` is specified:
1. Verify Rust toolchain is available and usable
2. If not: error with instructions to install Rust
3. Clone repo-graph source (specific tag/version)
4. Run `cargo build --release`
5. Install built binaries to user-local location
6. Record `install_mode: "source"` in manifest

**Why opt-in only:**
- Binary install is faster and simpler for most users
- Source build requires ~5-10 minutes and downloads dependencies
- Source fallback is for security-conscious users who want to verify

**Detection is NOT cached** between installer runs. Each run re-detects.

### D5: Full Installer Scope

Installation includes daemon deployment and lifecycle management, not just CLI binary.

**Installer must:**
1. Detect platform and architecture
2. Detect toolchains (D4)
3. Install `rmap` CLI binary to user-local location
4. Install `rmapd` daemon binary to user-local location
5. Create platform-native directories (D3)
6. Register user-level daemon service (invokes `rmapd`)
7. Start daemon service
8. Verify daemon health
9. Detect supported agent hosts
10. Offer integration for detected hosts (with backup)
11. Write install manifest
12. Update shell profile for PATH (prompt user)
13. Provide uninstall path

### D6: Install Manifest Model

Every installation writes a manifest recording what was done.

**Manifest contains (macOS example):**
```json
{
  "schema_version": "1",
  "installed_at": "2024-01-15T10:30:00Z",
  "installer_version": "0.1.0",
  "platform": "darwin",
  "arch": "aarch64",
  "install_mode": "user",
  "components": {
    "rmap": {
      "path": "~/.local/bin/rmap",
      "version": "0.1.0",
      "checksum": "sha256:..."
    },
    "rmapd": {
      "path": "~/.local/bin/rmapd",
      "version": "0.1.0",
      "checksum": "sha256:..."
    }
  },
  "directories": {
    "config": "~/Library/Application Support/repo-graph",
    "logs": "~/Library/Logs/repo-graph",
    "data": "~/Library/Application Support/repo-graph"
  },
  "service": {
    "type": "launchd",
    "path": "~/Library/LaunchAgents/com.repo-graph.rmapd.plist",
    "status": "running"
  },
  "toolchain_detection": {
    "rust": {"found": true, "version": "1.75.0"},
    "node": {"found": false}
  },
  "host_integrations": [
    {
      "host": "claude-code",
      "config_path": "~/.claude/settings.json",
      "backup_path": "~/.claude/settings.json.rmap-backup",
      "patched_at": "2024-01-15T10:30:05Z"
    }
  ],
  "path_updated": {
    "shell": "zsh",
    "profile": "~/.zshrc",
    "line_added": "export PATH=\"$HOME/.local/bin:$PATH\""
  }
}
```

**Manifest location:**
- macOS: `~/Library/Application Support/repo-graph/install-manifest.json`
- Linux: `~/.config/rmap/install-manifest.json`

### D4: Uninstall Contract

Uninstall must be complete and reversible.

**Uninstall steps:**
1. Stop daemon service
2. Unregister daemon service
3. Restore host integration backups
4. Remove host integration patches
5. Remove binaries
6. Remove runtime data (optional, prompt user)
7. Remove config (optional, prompt user)
8. Remove manifest

**Uninstall command:** `rmap uninstall` or standalone uninstaller script

### D8: Version Compatibility

Installer checks version compatibility before upgrade.

**Rules:**
- Major version mismatch: require explicit `--force` flag
- Minor version mismatch: warn but proceed
- Patch version: silent upgrade
- DB migration: daemon handles on startup, not installer

## Installation Modes

### Interactive Mode (Default)

```
$ curl -fsSL https://raw.githubusercontent.com/{OWNER}/repo-graph/main/scripts/install.sh | bash

Detecting platform... macOS ARM64
Detecting toolchains...
  Rust: 1.75.0
  Node.js: not found (not required)

Downloading rmap v0.1.0...
Installing CLI to ~/.local/bin/rmap...
Installing daemon to ~/.local/bin/rmapd...
Creating directories...
  ~/Library/Application Support/repo-graph/
  ~/Library/Logs/repo-graph/

Registering daemon service...
Starting daemon...
Verifying daemon health... OK

Detected agent hosts:
  [1] Claude Code (~/.claude/settings.json)
  [2] Codex CLI (~/.codex/hooks.json)

Install integrations? [1,2,all,none]: all

Backing up ~/.claude/settings.json...
Patching Claude Code hooks...

Backing up ~/.codex/hooks.json...
Patching Codex hooks...

Add ~/.local/bin to PATH? [Y/n]: y
Adding to ~/.zshrc...

Installation complete.
  CLI: rmap --version
  Daemon: rmapd --status
  Uninstall: rmap uninstall
```

### Non-Interactive Mode

```
$ curl -fsSL https://raw.githubusercontent.com/{OWNER}/repo-graph/main/scripts/install.sh | bash -s -- \
    --non-interactive \
    --integrate claude-code,codex
```

### Binary-Only Mode

```
$ curl -fsSL https://raw.githubusercontent.com/{OWNER}/repo-graph/main/scripts/install.sh | bash -s -- \
    --binary-only
```

Skips daemon service registration and host integration.

## Artifact Naming Convention

```
rmap-{version}-{platform}-{arch}.tar.gz
rmap-{version}-{platform}-{arch}.tar.gz.sha256

Examples:
rmap-0.1.0-darwin-aarch64.tar.gz
rmap-0.1.0-darwin-aarch64.tar.gz.sha256
rmap-0.1.0-linux-x86_64.tar.gz
rmap-0.1.0-linux-x86_64.tar.gz.sha256
```

## Checksum Verification

Installer always verifies checksums before installing.

```
$ sha256sum -c rmap-0.1.0-darwin-aarch64.tar.gz.sha256
rmap-0.1.0-darwin-aarch64.tar.gz: OK
```

## Error Handling

### Platform Not Supported

```
Error: Platform not supported: windows-x86_64
Supported platforms: darwin-aarch64, darwin-x86_64, linux-x86_64
```

### Daemon Health Check Failed

```
Error: Daemon health check failed after 3 attempts
Troubleshooting:
  1. Check logs: ~/.local/share/rmap/logs/daemon.log
  2. Check service: launchctl list | grep rmap
  3. Manual start: rmapd --foreground
```

### Host Integration Backup Failed

```
Error: Could not backup ~/.claude/settings.json
Reason: Permission denied
Skipping Claude Code integration. Run manually:
  rmap integrate claude-code
```

## Security Considerations

1. **HTTPS only** for artifact download
2. **Checksum verification** before any file operations
3. **No sudo by default** — user-local installation
4. **Backup before patch** — always create backup of host configs
5. **Manifest audit trail** — all operations recorded

## Out of Scope (DIST-1)

- Code signing and notarization (MAC-2)
- Auto-update mechanism (UPDATE-1)
- Windows support (WIN-1)
- Platform-specific implementation details (MAC-1, LINUX-1)

## Deliverables

1. This contract document (normative)
2. Artifact naming specification
3. Manifest schema (JSON Schema)
4. Directory layout specification per platform
5. Error message catalog

## Success Criteria

- Contract is complete enough to implement MAC-1 and LINUX-1
- No ambiguity in install/uninstall behavior
- Security model is explicit
- Error handling is specified
