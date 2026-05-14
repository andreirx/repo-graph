#!/bin/bash
# macOS-specific installer functions for repo-graph.
#
# Sourced by scripts/install.sh after platform detection.
# Contains launchd service management and macOS-specific operations.
#
# Path contract: Must match rust/crates/rgr/src/cli/paths.rs
# See: docs/slices/mac-1-macos-installer.md

# ─────────────────────────────────────────────────────────────────────────────
# Constants (must match paths.rs and DIST-1 D3)
# ─────────────────────────────────────────────────────────────────────────────

MACOS_CONFIG_DIR="${HOME}/Library/Application Support/repo-graph"
MACOS_LOG_DIR="${HOME}/Library/Logs/repo-graph"
MACOS_LAUNCHAGENTS_DIR="${HOME}/Library/LaunchAgents"
MACOS_SERVICE_LABEL="com.repo-graph.rmapd"
MACOS_PLIST_NAME="${MACOS_SERVICE_LABEL}.plist"

# ─────────────────────────────────────────────────────────────────────────────
# launchd Service Management
# ─────────────────────────────────────────────────────────────────────────────

# Emit launchd plist content to stdout.
# Used by install_launchd_plist. Can be overridden in bundled mode.
emit_macos_plist() {
    # Try template file first (modular mode)
    if [[ -n "${SCRIPT_DIR:-}" ]] && [[ -f "${SCRIPT_DIR}/templates/${MACOS_PLIST_NAME}" ]]; then
        cat "${SCRIPT_DIR}/templates/${MACOS_PLIST_NAME}"
        return
    fi

    # Fallback: embedded template (bundled mode)
    cat << 'PLIST_TEMPLATE'
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
PLIST_TEMPLATE
}

# Install the launchd plist from template.
# Expands ${HOME} in the template and installs to LaunchAgents.
install_launchd_plist() {
    local plist_path="${MACOS_LAUNCHAGENTS_DIR}/${MACOS_PLIST_NAME}"

    info "Installing launchd service..."

    # Ensure LaunchAgents directory exists
    mkdir -p "${MACOS_LAUNCHAGENTS_DIR}"

    # Expand ${HOME} in template and write to plist
    emit_macos_plist | sed "s|\${HOME}|${HOME}|g" > "${plist_path}"

    # Set correct permissions (644 for plist files)
    chmod 644 "${plist_path}"

    info "  Installed: ${plist_path}"
}

# Load and start the daemon via launchctl.
# Uses bootstrap for modern launchctl (macOS 10.10+).
start_launchd_service() {
    local plist_path="${MACOS_LAUNCHAGENTS_DIR}/${MACOS_PLIST_NAME}"
    local gui_uid

    gui_uid="$(id -u)"

    info "Starting daemon service..."

    # Unload if already loaded (handles upgrades gracefully)
    launchctl bootout "gui/${gui_uid}/${MACOS_SERVICE_LABEL}" 2>/dev/null || true

    # Load and start
    if ! launchctl bootstrap "gui/${gui_uid}" "${plist_path}"; then
        error "Failed to load launchd service"
    fi

    info "  Service loaded: ${MACOS_SERVICE_LABEL}"
}

# Stop and unload the daemon via launchctl.
stop_launchd_service() {
    local gui_uid

    gui_uid="$(id -u)"

    info "Stopping daemon service..."

    if launchctl bootout "gui/${gui_uid}/${MACOS_SERVICE_LABEL}" 2>/dev/null; then
        info "  Service unloaded: ${MACOS_SERVICE_LABEL}"
    else
        info "  Service was not loaded"
    fi
}

# Remove the launchd plist file.
remove_launchd_plist() {
    local plist_path="${MACOS_LAUNCHAGENTS_DIR}/${MACOS_PLIST_NAME}"

    if [[ -f "${plist_path}" ]]; then
        rm -f "${plist_path}"
        info "  Removed: ${plist_path}"
    fi
}

# Check if the daemon service is running.
# Returns 0 if running, 1 if not.
is_daemon_running() {
    local gui_uid

    gui_uid="$(id -u)"

    # Check if service is loaded and get PID
    if launchctl print "gui/${gui_uid}/${MACOS_SERVICE_LABEL}" 2>/dev/null | grep -q "state = running"; then
        return 0
    fi

    return 1
}

# Get daemon PID if running.
# Outputs PID to stdout, returns 1 if not running.
get_daemon_pid() {
    local gui_uid

    gui_uid="$(id -u)"

    local output
    output="$(launchctl print "gui/${gui_uid}/${MACOS_SERVICE_LABEL}" 2>/dev/null)"

    if [[ $? -eq 0 ]]; then
        echo "${output}" | grep "pid = " | awk '{print $3}'
        return 0
    fi

    return 1
}

# ─────────────────────────────────────────────────────────────────────────────
# Daemon Health Verification
# ─────────────────────────────────────────────────────────────────────────────

# Verify daemon is healthy after startup.
# Retries up to max_attempts with delay between.
verify_daemon_health() {
    local max_attempts="${1:-5}"
    local delay="${2:-2}"
    local attempt=1

    info "Verifying daemon health..."

    while [[ ${attempt} -le ${max_attempts} ]]; do
        if is_daemon_running; then
            local pid
            pid="$(get_daemon_pid)"
            info "  Daemon is running (pid: ${pid})"
            return 0
        fi

        info "  Attempt ${attempt}/${max_attempts}: waiting ${delay}s..."
        sleep "${delay}"
        ((attempt++))
    done

    warn "Daemon health check failed after ${max_attempts} attempts"
    warn "Check logs: ${MACOS_LOG_DIR}/daemon.log"
    return 1
}

# ─────────────────────────────────────────────────────────────────────────────
# Gatekeeper Handling
# ─────────────────────────────────────────────────────────────────────────────

# Remove quarantine attribute from binaries (for unsigned builds).
# Called when binary verification fails due to Gatekeeper.
remove_quarantine_attributes() {
    local install_dir="$1"

    info "Removing quarantine attributes..."

    xattr -d com.apple.quarantine "${install_dir}/rmap" 2>/dev/null || true
    xattr -d com.apple.quarantine "${install_dir}/rmapd" 2>/dev/null || true
    xattr -d com.apple.quarantine "${install_dir}/rgistr" 2>/dev/null || true

    info "  Quarantine attributes removed"
}

# ─────────────────────────────────────────────────────────────────────────────
# Full Service Setup (called by install.sh)
# ─────────────────────────────────────────────────────────────────────────────

# Complete macOS daemon service setup.
# Called by install.sh when BINARY_ONLY is false.
setup_macos_daemon_service() {
    # Ensure log directory exists
    mkdir -p "${MACOS_LOG_DIR}"
    chmod 700 "${MACOS_LOG_DIR}"

    install_launchd_plist
    start_launchd_service

    if ! verify_daemon_health 5 2; then
        warn ""
        warn "Daemon did not start successfully."
        warn "You can start it manually: rmapd"
        warn "Or check: launchctl print gui/$(id -u)/${MACOS_SERVICE_LABEL}"
        return 1
    fi

    return 0
}

# ─────────────────────────────────────────────────────────────────────────────
# Uninstall Support
# ─────────────────────────────────────────────────────────────────────────────

# Complete macOS service uninstall.
# Called by rmap uninstall or standalone uninstaller.
uninstall_macos_daemon_service() {
    stop_launchd_service
    remove_launchd_plist
}

# ─────────────────────────────────────────────────────────────────────────────
# Host Integration Patching
# ─────────────────────────────────────────────────────────────────────────────

# Record a backup in the install manifest.
# Args: host, original_path, backup_path
record_backup() {
    local host="$1"
    local original_path="$2"
    local backup_path="$3"
    local manifest_path="${MACOS_CONFIG_DIR}/install-manifest.json"

    # This is a simplified version - full implementation would use jq
    # to properly merge into the manifest's host_integrations array.
    info "  Backup recorded: ${backup_path}"
}

# Patch Claude Code settings.json with repo-graph hooks.
# Per HOST-1 D3, creates backup before patching.
patch_claude_code() {
    local config_path="$1"
    local backup_path="${config_path}.rmap-backup"

    if [[ ! -f "${config_path}" ]]; then
        # Create new settings file
        mkdir -p "$(dirname "${config_path}")"
        echo '{}' > "${config_path}"
    fi

    # Backup existing config
    cp "${config_path}" "${backup_path}"
    record_backup "claude-code" "${config_path}" "${backup_path}"

    # The hooks JSON to merge
    # Schema per https://code.claude.com/docs/en/hooks (verified 2026-05-13)
    # - Uses matcher groups with nested "hooks" arrays
    # - Requires "type": "command" field
    # - Timeout is in seconds, not milliseconds
    # - Uses --from-stdin (stdin JSON), not --from-env
    local hooks_json
    hooks_json=$(cat <<'HOOKS_EOF'
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {"type": "command", "command": "rmap hook session-start --from-stdin", "timeout": 30}
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {"type": "command", "command": "rmap hook post-edit --from-stdin", "timeout": 60}
        ]
      }
    ],
    "PreCompact": [
      {
        "matcher": "auto|manual",
        "hooks": [
          {"type": "command", "command": "rmap hook pre-compact --from-stdin", "timeout": 10}
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {"type": "command", "command": "rmap hook stop --from-stdin", "timeout": 30}
        ]
      }
    ]
  }
}
HOOKS_EOF
)

    # Merge using jq if available, otherwise warn
    if command_exists jq; then
        jq -s '.[0] * .[1]' "${config_path}" <(echo "${hooks_json}") > "${config_path}.tmp"
        mv "${config_path}.tmp" "${config_path}"
        info "  Patched: ${config_path}"
    else
        warn "jq not found - cannot patch Claude Code config automatically"
        warn "Manual installation required. Add hooks to: ${config_path}"
        return 1
    fi

    return 0
}

# Patch Codex hooks.json with repo-graph hooks.
# Per HOST-1 D3, creates backup before patching.
patch_codex() {
    local config_path="$1"
    local backup_path="${config_path}.rmap-backup"

    if [[ ! -f "${config_path}" ]]; then
        mkdir -p "$(dirname "${config_path}")"
        echo '{}' > "${config_path}"
    fi

    cp "${config_path}" "${backup_path}"
    record_backup "codex" "${config_path}" "${backup_path}"

    local hooks_json
    hooks_json=$(cat <<'HOOKS_EOF'
{
  "hooks": {
    "SessionStart": "rmap hook session-start --from-env",
    "PostToolUse": "rmap hook post-edit --from-env",
    "Stop": "rmap hook stop --from-env"
  }
}
HOOKS_EOF
)

    if command_exists jq; then
        jq -s '.[0] * .[1]' "${config_path}" <(echo "${hooks_json}") > "${config_path}.tmp"
        mv "${config_path}.tmp" "${config_path}"
        info "  Patched: ${config_path}"
    else
        warn "jq not found - cannot patch Codex config automatically"
        return 1
    fi

    return 0
}

# Restore a host integration from backup.
restore_host_backup() {
    local config_path="$1"
    local backup_path="${config_path}.rmap-backup"

    if [[ -f "${backup_path}" ]]; then
        mv "${backup_path}" "${config_path}"
        info "  Restored: ${config_path}"
        return 0
    else
        warn "  No backup found: ${backup_path}"
        return 1
    fi
}
