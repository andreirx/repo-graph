# MAC-1: macOS Installer and Daemon Service

Status: PLANNED
Depends: DIST-1, REL-1
Track: Distribution / Install / Host Integration

**Execution order note:** Runs after HOOK-1 in rollout sequence, but HOOK-1 is not
a build dependency. MAC-1 implements DIST-1 contract; HOOK-1 provides commands that
host integrations call.

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

Per DIST-1 D3, use native macOS paths (not XDG).

```
~/.local/bin/
  rmap                              # CLI binary
  rmap-daemon                       # Daemon binary

~/Library/Application Support/repo-graph/
  config.toml                       # User configuration
  hooks.toml                        # Hook configuration
  install-manifest.json             # Installation record
  databases/                        # Default DB storage
  sessions/                         # Session state files

~/Library/Logs/repo-graph/
  daemon.log                        # Daemon stdout/stderr
  hooks.log                         # Hook execution log

~/Library/LaunchAgents/
  com.repo-graph.rmap-daemon.plist  # launchd user agent
```

## Installation Script

### Entry Point

```bash
#!/bin/bash
# install-macos.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"
source "${SCRIPT_DIR}/lib/macos.sh"

main() {
    check_macos
    check_arch
    parse_args "$@"
    
    if [[ "${BINARY_ONLY:-false}" == "true" ]]; then
        install_binaries
    else
        install_binaries
        create_directories
        install_launchd_service
        start_daemon
        verify_daemon_health
        detect_hosts
        offer_integrations
    fi
    
    write_manifest
    print_success
}

main "$@"
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
    install -m 755 "${ARTIFACT_DIR}/rmap-daemon" "${install_dir}/rmap-daemon"
    
    # Verify installation
    if ! "${install_dir}/rmap" --version > /dev/null 2>&1; then
        error "Binary verification failed"
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

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.repo-graph.rmap-daemon</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>${HOME}/.local/bin/rmap-daemon</string>
        <string>--config</string>
        <string>${HOME}/Library/Application Support/repo-graph/config.toml</string>
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
    local plist_path=~/Library/LaunchAgents/com.repo-graph.rmap-daemon.plist
    
    echo "Installing launchd service..."
    
    # Expand HOME in template
    sed "s|\${HOME}|${HOME}|g" "${SCRIPT_DIR}/templates/com.repo-graph.rmap-daemon.plist" > "${plist_path}"
    
    # Set permissions
    chmod 644 "${plist_path}"
}

start_daemon() {
    echo "Starting daemon..."
    
    # Unload if already loaded (for upgrades)
    launchctl bootout gui/$(id -u)/com.repo-graph.rmap-daemon 2>/dev/null || true
    
    # Load and start
    launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.repo-graph.rmap-daemon.plist
    
    # Wait for startup
    sleep 2
}

verify_daemon_health() {
    echo "Verifying daemon health..."
    
    local max_attempts=5
    local attempt=1
    
    while [[ $attempt -le $max_attempts ]]; do
        if rmap daemon status > /dev/null 2>&1; then
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
    launchctl bootout gui/$(id -u)/com.repo-graph.rmap-daemon 2>/dev/null || true
    
    # Remove service definition
    rm -f ~/Library/LaunchAgents/com.repo-graph.rmap-daemon.plist
    
    # Restore host integration backups
    restore_backups
    
    # Remove binaries (user-local, no sudo needed)
    rm -f ~/.local/bin/rmap
    rm -f ~/.local/bin/rmap-daemon
    
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
    launchctl bootout gui/$(id -u)/com.repo-graph.rmap-daemon 2>/dev/null || true
    
    # Install new binaries
    install_binaries
    
    # Reload service (picks up new binary)
    launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.repo-graph.rmap-daemon.plist
    
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
    # Try to run binary
    if ! ~/.local/bin/rmap --version > /dev/null 2>&1; then
        echo ""
        echo "macOS Gatekeeper blocked the binary."
        echo ""
        echo "To allow, run:"
        echo "  xattr -d com.apple.quarantine ~/.local/bin/rmap"
        echo "  xattr -d com.apple.quarantine ~/.local/bin/rmap-daemon"
        echo ""
        echo "Or: System Preferences → Security & Privacy → Allow"
        echo ""
        
        if confirm "Run xattr commands now?"; then
            xattr -d com.apple.quarantine ~/.local/bin/rmap 2>/dev/null || true
            xattr -d com.apple.quarantine ~/.local/bin/rmap-daemon 2>/dev/null || true
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
  ✓ rmap-daemon: ~/.local/bin/rmap-daemon (0.1.0)

Directories:
  ✓ Config: ~/Library/Application Support/repo-graph
  ✓ Data: ~/Library/Application Support/repo-graph
  ✓ Logs: ~/Library/Logs/repo-graph

Daemon:
  ✓ Service: loaded (com.repo-graph.rmap-daemon)
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

1. `scripts/install-macos.sh`
2. `scripts/lib/macos.sh` (macOS-specific functions)
3. `scripts/templates/com.repo-graph.rmap-daemon.plist`
4. `rmap uninstall` command (macOS path)
5. `rmap doctor` command (macOS path)
6. Installation documentation

## Success Criteria

- Fresh install completes without errors
- Daemon starts and stays running
- Host detection finds installed agents
- Host patching works with backup
- Uninstall cleanly removes everything
- Upgrade preserves configuration
