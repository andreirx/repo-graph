# MAC-1: macOS Installer and Daemon Service

Status: PLANNED
Depends: DIST-1, REL-1, HOOK-1 (runtime)
Track: Distribution / Install / Host Integration

**Execution order note:** Runs after HOOK-1 in rollout sequence, but HOOK-1 is not
a build dependency. MAC-1 implements DIST-1 contract; HOOK-1 provides commands that
host integrations call.

## Path Authority

**Contract:** DIST-1 D3 defines the platform-native directory layout.

**Runtime reference:** `rust/crates/rgr/src/cli/paths.rs` is the authoritative
implementation for CLI/runtime path resolution.

**Installer/service conformance:** The installer (`scripts/install.sh`) and
launchd service template must conform to the same path contract as `cli/paths.rs`.
Path drift between installer, runtime, and service is a product bug.

| Path | Source |
|------|--------|
| Config/data | `paths::config_dir()` / `paths::data_dir()` |
| Logs | `paths::logs_dir()` |
| Sessions | `paths::sessions_dir()` |
| Databases | `paths::databases_dir()` |

## Objective

Implement the macOS-specific installer that deploys repo-graph binaries, configures
the daemon as a launchd user service, and provides host integration.

## Platform Specification

- **Target:** macOS 12+ (Monterey and later)
- **Architectures:** ARM64 (Apple Silicon) primary, x86_64 secondary
- **Service manager:** launchd (user agent)
- **Shell:** zsh (default since Catalina), bash supported
- **Privilege model:** User-local by default (no sudo), per DIST-1 D2

## Directory Layout (Native macOS Paths)

Per DIST-1 D3, use native macOS paths (not XDG). Must match `cli/paths.rs`.

```
~/.local/bin/
  rmap                              # CLI binary
  rmapd                             # Daemon binary
  rgistr                            # Policy hints binary

~/Library/Application Support/repo-graph/
  hooks.toml                        # Hook configuration (IMPLEMENTED - HOOK-1)
  install-manifest.json             # Installation record (IMPLEMENTED - REL-1)
  databases/                        # Default DB storage
  sessions/                         # Session state files (IMPLEMENTED - HOOK-1)

~/Library/Logs/repo-graph/
  daemon.log                        # Daemon stdout/stderr
  hooks.log                         # Hook execution log

~/Library/LaunchAgents/
  com.repo-graph.rmapd.plist        # launchd user agent
```

**Configuration surface status:**

| File | Status | Implemented By |
|------|--------|----------------|
| `hooks.toml` | IMPLEMENTED | HOOK-1 |
| `install-manifest.json` | IMPLEMENTED | REL-1 |
| `config.toml` (daemon config) | NOT IMPLEMENTED | Future slice |

Note: Daemon configuration (`config.toml`) is deferred. The daemon currently has
no configurable parameters beyond environment variables.

## Installation Script

### Architecture

**Single entry point:** `scripts/install.sh` (unified cross-platform installer)

**Platform-specific logic:** `scripts/lib/macos.sh` (sourced after platform detection)

The unified installer already exists (REL-1) and handles:
- Platform/arch detection
- Version resolution
- Binary download and verification
- PATH setup
- Manifest writing

MAC-1 adds macOS-specific service logic to `scripts/lib/macos.sh`:
- launchd service installation
- Daemon health verification
- Host integration patching

### Integration Point

After platform detection in `install.sh`:

```bash
# Source platform-specific module
if [[ "${PLATFORM}" == "darwin" ]]; then
    source "${SCRIPT_DIR}/lib/macos.sh"
fi
```

The unified installer calls platform functions when `BINARY_ONLY != true`:

```bash
if [[ "${BINARY_ONLY}" != "true" ]]; then
    create_directories
    setup_daemon_service      # calls macos.sh functions on Darwin
    detect_hosts
    offer_integrations
fi
```

### Binary Installation

```bash
install_binaries() {
    local install_dir="${INSTALL_DIR:-$HOME/.local/bin}"
    
    echo "Installing binaries to ${install_dir}..."
    
    # Create user-local bin directory if needed
    mkdir -p "${install_dir}"
    
    # Install binaries (user-local, no sudo)
    install -m 755 "${ARTIFACT_DIR}/rmap" "${install_dir}/rmap"
    install -m 755 "${ARTIFACT_DIR}/rmapd" "${install_dir}/rmapd"
    
    # Verify installation
    if ! "${install_dir}/rmap" --version > /dev/null 2>&1; then
        error "CLI binary verification failed"
    fi
    if ! "${install_dir}/rmapd" --version > /dev/null 2>&1; then
        error "Daemon binary verification failed"
    fi
}
```

### Directory Creation

```bash
create_directories() {
    echo "Creating directories..."
    
    mkdir -p ~/.local/bin
    mkdir -p ~/Library/Application\ Support/repo-graph/databases
    mkdir -p ~/Library/Application\ Support/repo-graph/sessions
    mkdir -p ~/Library/Logs/repo-graph
    
    # Set permissions
    chmod 700 ~/Library/Application\ Support/repo-graph
    chmod 700 ~/Library/Logs/repo-graph
}
```

## launchd Service

### Plist Template

Note: `--config` argument is omitted. Daemon config (`config.toml`) is not yet
implemented. The daemon currently accepts no configuration file; `rmapd --config`
is reserved but ignored.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.repo-graph.rmapd</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>${HOME}/.local/bin/rmapd</string>
    </array>
    
    <key>RunAtLoad</key>
    <true/>
    
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    
    <key>StandardOutPath</key>
    <string>${HOME}/Library/Logs/repo-graph/daemon.log</string>
    
    <key>StandardErrorPath</key>
    <string>${HOME}/Library/Logs/repo-graph/daemon.log</string>
    
    <key>EnvironmentVariables</key>
    <dict>
        <key>RMAP_LOG_LEVEL</key>
        <string>info</string>
    </dict>
    
    <key>ProcessType</key>
    <string>Background</string>
    
    <key>LowPriorityIO</key>
    <true/>
    
    <key>ThrottleInterval</key>
    <integer>10</integer>
</dict>
</plist>
```

### Service Installation

```bash
install_launchd_service() {
    local plist_path=~/Library/LaunchAgents/com.repo-graph.rmapd.plist
    
    echo "Installing launchd service..."
    
    # Expand HOME in template
    sed "s|\${HOME}|${HOME}|g" "${SCRIPT_DIR}/templates/com.repo-graph.rmapd.plist" > "${plist_path}"
    
    # Set permissions
    chmod 644 "${plist_path}"
}

start_daemon() {
    echo "Starting daemon..."
    
    # Unload if already loaded (for upgrades)
    launchctl bootout gui/$(id -u)/com.repo-graph.rmapd 2>/dev/null || true
    
    # Load and start
    launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.repo-graph.rmapd.plist
    
    # Wait for startup
    sleep 2
}

verify_daemon_health() {
    echo "Verifying daemon health..."
    
    local max_attempts=5
    local attempt=1
    
    while [[ $attempt -le $max_attempts ]]; do
        if rmapd --status > /dev/null 2>&1; then
            echo "Daemon is healthy"
            return 0
        fi
        
        echo "  Attempt $attempt/$max_attempts: waiting..."
        sleep 2
        ((attempt++))
    done
    
    error "Daemon health check failed after $max_attempts attempts"
    echo "Check logs: ~/Library/Logs/repo-graph/daemon.log"
    return 1
}
```

## Host Detection (macOS)

### Claude Code

```bash
detect_claude_code() {
    local global_config=~/.claude/settings.json
    local detected=false
    
    if [[ -f "${global_config}" ]]; then
        echo "  Claude Code: found (global config)"
        DETECTED_HOSTS+=("claude-code:global:${global_config}")
        detected=true
    fi
    
    # Check for Claude Code app
    if [[ -d "/Applications/Claude.app" ]]; then
        echo "  Claude Code: app installed"
    fi
    
    $detected
}
```

### Codex

```bash
detect_codex() {
    local global_config=~/.codex/hooks.json
    local codex_dir=~/.codex
    
    if [[ -f "${global_config}" ]]; then
        echo "  Codex: found (global config)"
        DETECTED_HOSTS+=("codex:global:${global_config}")
        return 0
    elif [[ -d "${codex_dir}" ]]; then
        echo "  Codex: found (directory exists, no hooks)"
        DETECTED_HOSTS+=("codex:global:${codex_dir}/hooks.json")
        return 0
    fi
    
    return 1
}
```

### Cursor

```bash
detect_cursor() {
    local mcp_config=~/.cursor/mcp.json
    
    if [[ -d "/Applications/Cursor.app" ]] || [[ -d "${HOME}/Applications/Cursor.app" ]]; then
        echo "  Cursor: app installed"
        DETECTED_HOSTS+=("cursor:mcp:${mcp_config}")
        return 0
    fi
    
    return 1
}
```

## Integration Patching

### Claude Code Patch

```bash
patch_claude_code() {
    local config_path="$1"
    local backup_path="${config_path}.rmap-backup"
    
    # Backup
    cp "${config_path}" "${backup_path}"
    record_backup "claude-code" "${config_path}" "${backup_path}"
    
    # Patch using jq
    local hooks_json=$(cat <<'EOF'
{
  "hooks": {
    "SessionStart": [
      {"command": "rmap hook session-start", "timeout": 30000}
    ],
    "PostToolUse": [
      {
        "matcher": {"tool_name": ["Edit", "Write", "MultiEdit"]},
        "command": "rmap hook post-edit --files \"$TOOL_OUTPUT_FILES\"",
        "timeout": 60000
      }
    ],
    "PreCompact": [
      {"command": "rmap hook pre-compact", "timeout": 10000}
    ],
    "Stop": [
      {"command": "rmap hook stop", "timeout": 30000}
    ]
  }
}
EOF
)
    
    if [[ -f "${config_path}" ]]; then
        # Merge with existing
        jq -s '.[0] * .[1]' "${config_path}" <(echo "${hooks_json}") > "${config_path}.tmp"
        mv "${config_path}.tmp" "${config_path}"
    else
        # Create new
        mkdir -p "$(dirname "${config_path}")"
        echo "${hooks_json}" > "${config_path}"
    fi
}
```

## Uninstallation

```bash
uninstall_macos() {
    echo "Uninstalling repo-graph..."
    
    # Stop and unload service
    launchctl bootout gui/$(id -u)/com.repo-graph.rmapd 2>/dev/null || true
    
    # Remove service definition
    rm -f ~/Library/LaunchAgents/com.repo-graph.rmapd.plist
    
    # Restore host integration backups
    restore_backups
    
    # Remove binaries (user-local, no sudo needed)
    rm -f ~/.local/bin/rmap
    rm -f ~/.local/bin/rmapd
    
    # Prompt for data removal
    if confirm "Remove configuration and data?"; then
        rm -rf ~/Library/Application\ Support/repo-graph
        rm -rf ~/Library/Logs/repo-graph
    fi
    
    echo "Uninstallation complete"
}
```

## Upgrade Path

```bash
upgrade_macos() {
    local current_version=$(rmap --version 2>/dev/null | head -1 || echo "0.0.0")
    
    echo "Current version: ${current_version}"
    echo "New version: ${VERSION}"
    
    # Stop daemon
    launchctl bootout gui/$(id -u)/com.repo-graph.rmapd 2>/dev/null || true
    
    # Install new binaries
    install_binaries
    
    # Reload service (picks up new binary)
    launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.repo-graph.rmapd.plist
    
    # Verify
    verify_daemon_health
    
    # Update manifest
    update_manifest_version
}
```

## Gatekeeper Handling (Unsigned Binaries)

Until MAC-2 (code signing), binaries are unsigned.

### First Run Warning

```bash
handle_gatekeeper() {
    # Try to run binaries
    if ! ~/.local/bin/rmap --version > /dev/null 2>&1; then
        echo ""
        echo "macOS Gatekeeper blocked the binaries."
        echo ""
        echo "To allow, run:"
        echo "  xattr -d com.apple.quarantine ~/.local/bin/rmap"
        echo "  xattr -d com.apple.quarantine ~/.local/bin/rmapd"
        echo ""
        echo "Or: System Preferences → Security & Privacy → Allow"
        echo ""
        
        if confirm "Run xattr commands now?"; then
            xattr -d com.apple.quarantine ~/.local/bin/rmap 2>/dev/null || true
            xattr -d com.apple.quarantine ~/.local/bin/rmapd 2>/dev/null || true
        fi
    fi
}
```

## Diagnostics

### rmap doctor (macOS)

```
$ rmap doctor

repo-graph health check (macOS)

Binaries:
  ✓ rmap: ~/.local/bin/rmap (0.1.0)
  ✓ rmapd: ~/.local/bin/rmapd (0.1.0)

Directories:
  ✓ Config: ~/Library/Application Support/repo-graph
  ✓ Data: ~/Library/Application Support/repo-graph
  ✓ Logs: ~/Library/Logs/repo-graph

Daemon:
  ✓ Service: loaded (com.repo-graph.rmapd)
  ✓ Status: running (pid 12345)
  ✓ Health: ok

Host Integrations:
  ✓ Claude Code: installed (global)
  ○ Codex: not installed
  ○ Cursor: not installed

Recent logs:
  [2024-01-15 10:30:00] Daemon started
  [2024-01-15 10:30:01] Listening on /tmp/rmap.sock
```

## Testing

### Installation Tests

- Fresh install on clean macOS
- Upgrade from previous version
- Uninstall and reinstall
- Binary-only mode
- Non-interactive mode

### Service Tests

- Service starts on boot
- Service restarts after crash
- Service stops cleanly
- Log rotation works

### Integration Tests

- Claude Code detection and patching
- Codex detection and patching
- Backup and restore

## Out of Scope (MAC-1)

- Code signing and notarization (MAC-2)
- x86_64 Intel support (later priority)
- Homebrew formula (future)

## Deliverables

1. `scripts/lib/macos.sh` (macOS-specific functions, sourced by unified installer)
2. `scripts/templates/com.repo-graph.rmapd.plist` (launchd service template)
3. Updates to `scripts/install.sh` (source macos.sh, call service functions)
4. `rmap uninstall` command — CLI entrypoint + macOS platform adapter
5. `rmap doctor` command — CLI entrypoint + macOS platform adapter
6. `rust/crates/rgr/src/platform/macos.rs` — platform adapter module
7. Installation documentation updates

**Architecture note:** `rmap uninstall` and `rmap doctor` are CLI entrypoints backed
by platform adapter modules. The adapter isolates launchd operations from generic
command logic. Implementation follows manifest-driven behavior per DIST-1 D6.

## Success Criteria

- Fresh install completes without errors
- Daemon starts and stays running
- Host detection finds installed agents
- Host patching works with backup
- Uninstall cleanly removes everything
- Upgrade preserves configuration
