# LINUX-1: Linux Installer and Daemon Service

Status: PLANNED
Depends: DIST-1, REL-1
Track: Distribution / Install / Host Integration

**Execution order note:** Follows MAC-1 in rollout sequence (macOS-first platform priority),
but MAC-1 is not a build dependency. Both implement the same DIST-1 contract independently.
HOOK-1 provides commands that host integrations call, but is not a build dependency.

## Objective

Implement the Linux-specific installer that deploys repo-graph binaries, configures
the daemon as a systemd user service, and provides host integration.

## Platform Specification

- **Target:** Linux with systemd (Ubuntu 20.04+, Debian 11+, Fedora 35+, Arch)
- **Architecture:** x86_64 primary
- **Init system:** systemd (user units)
- **Shell:** bash, zsh

## Directory Layout

```
~/.local/bin/
  rmap                              # CLI binary
  rmap-daemon                       # Daemon binary

~/.config/rmap/
  config.toml                       # User configuration
  hooks.toml                        # Hook configuration
  install-manifest.json             # Installation record

~/.config/systemd/user/
  rmap-daemon.service               # systemd user service

~/.local/share/rmap/
  logs/
    daemon.log                      # Daemon logs
    hooks.log                       # Hook execution log
  databases/                        # Default DB storage
  sessions/                         # Session state files
```

## Installation Script

### Entry Point

```bash
#!/bin/bash
# install-linux.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"
source "${SCRIPT_DIR}/lib/linux.sh"

main() {
    check_linux
    check_arch
    check_systemd
    parse_args "$@"
    
    if [[ "${BINARY_ONLY:-false}" == "true" ]]; then
        install_binaries
    else
        install_binaries
        create_directories
        install_systemd_service
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

### systemd Detection

```bash
check_systemd() {
    if ! command -v systemctl &> /dev/null; then
        echo "Warning: systemd not found"
        echo "Daemon service will not be installed"
        echo "You can run the daemon manually: rmap-daemon"
        NO_SYSTEMD=true
        return
    fi
    
    # Check if user session is available
    if ! systemctl --user status &> /dev/null; then
        echo "Warning: systemd user session not available"
        echo "Try: loginctl enable-linger $USER"
        NO_SYSTEMD=true
    fi
}
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
    
    # Verify
    if ! "${install_dir}/rmap" --version > /dev/null 2>&1; then
        error "Binary verification failed"
    fi
}
```

## systemd User Service

### Service Unit

```ini
# ~/.config/systemd/user/rmap-daemon.service
[Unit]
Description=repo-graph daemon
Documentation=https://github.com/anthropics/repo-graph
After=default.target

[Service]
Type=simple
ExecStart=~/.local/bin/rmap-daemon --config %h/.config/rmap/config.toml
Restart=on-failure
RestartSec=10

# Logging
StandardOutput=append:%h/.local/share/rmap/logs/daemon.log
StandardError=append:%h/.local/share/rmap/logs/daemon.log

# Environment
Environment=RMAP_LOG_LEVEL=info

# Resource limits
Nice=10
IOSchedulingClass=idle

[Install]
WantedBy=default.target
```

### Service Installation

```bash
install_systemd_service() {
    if [[ "${NO_SYSTEMD:-false}" == "true" ]]; then
        echo "Skipping systemd service (not available)"
        return
    fi
    
    local service_dir=~/.config/systemd/user
    local service_file="${service_dir}/rmap-daemon.service"
    
    echo "Installing systemd user service..."
    
    mkdir -p "${service_dir}"
    
    # Copy service file (template expansion done by systemd via %h)
    cp "${SCRIPT_DIR}/templates/rmap-daemon.service" "${service_file}"
    
    # Reload systemd
    systemctl --user daemon-reload
}

start_daemon() {
    if [[ "${NO_SYSTEMD:-false}" == "true" ]]; then
        echo "Starting daemon manually..."
        nohup rmap-daemon --config ~/.config/rmap/config.toml \
            >> ~/.local/share/rmap/logs/daemon.log 2>&1 &
        echo $! > ~/.local/share/rmap/daemon.pid
        return
    fi
    
    echo "Starting daemon service..."
    
    # Enable and start
    systemctl --user enable rmap-daemon.service
    systemctl --user start rmap-daemon.service
    
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
    
    error "Daemon health check failed"
    echo "Check logs: ~/.local/share/rmap/logs/daemon.log"
    
    if [[ "${NO_SYSTEMD:-false}" != "true" ]]; then
        echo "Check service: systemctl --user status rmap-daemon"
    fi
    
    return 1
}
```

## Host Detection (Linux)

Same as macOS, adjusted for Linux paths:

```bash
detect_claude_code() {
    local global_config=~/.claude/settings.json
    
    if [[ -f "${global_config}" ]]; then
        echo "  Claude Code: found (global config)"
        DETECTED_HOSTS+=("claude-code:global:${global_config}")
        return 0
    fi
    
    return 1
}

detect_codex() {
    local global_config=~/.codex/hooks.json
    local codex_dir=~/.codex
    
    if [[ -f "${global_config}" ]] || [[ -d "${codex_dir}" ]]; then
        echo "  Codex: found"
        DETECTED_HOSTS+=("codex:global:${codex_dir}/hooks.json")
        return 0
    fi
    
    return 1
}

detect_cursor() {
    # Cursor on Linux - check common locations
    local cursor_config=~/.config/Cursor/mcp.json
    
    if [[ -d ~/.config/Cursor ]]; then
        echo "  Cursor: found"
        DETECTED_HOSTS+=("cursor:mcp:${cursor_config}")
        return 0
    fi
    
    return 1
}
```

## Uninstallation

```bash
uninstall_linux() {
    echo "Uninstalling repo-graph..."
    
    # Stop and disable service
    if [[ "${NO_SYSTEMD:-false}" != "true" ]]; then
        systemctl --user stop rmap-daemon.service 2>/dev/null || true
        systemctl --user disable rmap-daemon.service 2>/dev/null || true
        rm -f ~/.config/systemd/user/rmap-daemon.service
        systemctl --user daemon-reload
    else
        # Kill manual daemon
        if [[ -f ~/.local/share/rmap/daemon.pid ]]; then
            kill $(cat ~/.local/share/rmap/daemon.pid) 2>/dev/null || true
            rm -f ~/.local/share/rmap/daemon.pid
        fi
    fi
    
    # Restore host integrations
    restore_backups
    
    # Remove binaries
    # Remove binaries (user-local, no sudo needed)
    rm -f ~/.local/bin/rmap
    rm -f ~/.local/bin/rmap-daemon
    
    # Prompt for data removal
    if confirm "Remove configuration and data?"; then
        rm -rf ~/.config/rmap
        rm -rf ~/.local/share/rmap
    fi
    
    echo "Uninstallation complete"
}
```

## User Linger

For daemon to run without active login session:

```bash
enable_linger() {
    if ! loginctl show-user "$USER" | grep -q "Linger=yes"; then
        echo "Enabling user linger for daemon persistence..."
        
        if command -v loginctl &> /dev/null; then
            sudo loginctl enable-linger "$USER"
        else
            echo "Warning: loginctl not available"
            echo "Daemon may stop when you log out"
        fi
    fi
}
```

## Distribution-Specific Notes

### Ubuntu/Debian

```bash
# Dependencies usually present
# May need: sudo apt install -y curl jq
```

### Fedora/RHEL

```bash
# Dependencies usually present
# May need: sudo dnf install -y curl jq
```

### Arch Linux

```bash
# Dependencies usually present
# May need: sudo pacman -S curl jq
```

### Alpine/musl

Requires musl-linked binary (separate artifact):

```
rmap-0.1.0-linux-x86_64-musl.tar.gz
```

## Diagnostics

### rmap doctor (Linux)

```
$ rmap doctor

repo-graph health check (Linux)

System:
  Distribution: Ubuntu 22.04
  Init: systemd
  User linger: enabled

Binaries:
  ✓ rmap: ~/.local/bin/rmap (0.1.0)
  ✓ rmap-daemon: ~/.local/bin/rmap-daemon (0.1.0)

Directories:
  ✓ Config: ~/.config/rmap
  ✓ Data: ~/.local/share/rmap
  ✓ Logs: ~/.local/share/rmap/logs

Daemon:
  ✓ Service: enabled (rmap-daemon.service)
  ✓ Status: active (running)
  ✓ Health: ok

Host Integrations:
  ✓ Claude Code: installed (global)
  ○ Codex: not installed
  ○ Cursor: not installed
```

## Testing

### Installation Tests

- Fresh install on Ubuntu
- Fresh install on Fedora
- Fresh install on Arch
- Non-systemd fallback (Alpine, WSL1)
- Upgrade from previous version

### Service Tests

- Service starts on boot
- Service restarts after crash
- User linger works
- Manual mode works

### Distribution Tests

- Ubuntu 20.04, 22.04, 24.04
- Debian 11, 12
- Fedora 38, 39
- Arch (rolling)

## Out of Scope (LINUX-1)

- ARM64 Linux (later)
- musl/Alpine builds (later)
- Non-systemd init (OpenRC, runit)
- Package manager distribution (apt, dnf, pacman)

## Deliverables

1. `scripts/install-linux.sh`
2. `scripts/lib/linux.sh`
3. `scripts/templates/rmap-daemon.service`
4. `rmap uninstall` command (Linux path)
5. `rmap doctor` command (Linux path)
6. Distribution testing documentation

## Success Criteria

- Fresh install works on Ubuntu, Fedora, Arch
- systemd service works correctly
- Non-systemd fallback works
- Host detection finds installed agents
- Uninstall cleanly removes everything
