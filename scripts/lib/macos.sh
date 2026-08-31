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
# Daemon Unix socket. MUST match platform-paths::compute_socket_path_from_home
# (macOS canonical: <home>/Library/Application Support/repo-graph/daemon.sock) and
# scripts/dev-install-local.sh. Used ONLY as the canonical DEFAULT for the honest
# daemon-start failure message (INSTALL-ROBUSTNESS-2 B), under a RMAP_SOCKET_PATH
# override — the liveness predicate itself is the socket_ping probe read from
# `rmap doctor --json` (see daemon_socket_answers in install.template.sh).
MACOS_SOCKET_PATH="${MACOS_CONFIG_DIR}/daemon.sock"

# ─────────────────────────────────────────────────────────────────────────────
# launchd Service Management
# ─────────────────────────────────────────────────────────────────────────────

# Emit launchd plist content to stdout.
# Used by install_launchd_plist. Can be overridden in bundled mode.
# The daemon PATH: static base + the dirs the toolchain actually needs. tsserver is a
# `#!/usr/bin/env node` script, so the launchd daemon MUST carry a node dir or every
# enrichment session dies exit-127 with stderr discarded (bitten 2026-08-31: silent
# "resolved 0/N" on every repo). Resolved at INSTALL time from the installing shell;
# a missing node is a LOUD warning, never silent.
rmap_toolchain_path() {
    local base="/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin"
    local extra=""
    local node_bin
    node_bin="$(command -v node 2>/dev/null || true)"
    if [[ -n "${node_bin}" ]]; then
        extra="$(dirname "${node_bin}")"
    else
        echo "WARNING: no 'node' on the installing shell's PATH — the daemon will not be able to run tsserver enrichment (TypeScript call resolution will silently stay at baseline). Install node or re-run dev-install from a shell with node." >&2
    fi
    [[ -d /opt/homebrew/bin ]] && extra="${extra:+${extra}:}/opt/homebrew/bin"
    printf '%s' "${base}${extra:+:${extra}}"
}

emit_macos_plist() {
    local toolchain_path
    toolchain_path="$(rmap_toolchain_path)"
    # Try template file first (modular mode)
    if [[ -n "${SCRIPT_DIR:-}" ]] && [[ -f "${SCRIPT_DIR}/templates/${MACOS_PLIST_NAME}" ]]; then
        sed "s|@RMAP_TOOLCHAIN_PATH@|${toolchain_path}|" "${SCRIPT_DIR}/templates/${MACOS_PLIST_NAME}"
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
#
# INSTALL-ROBUSTNESS-2 (B): launchctl bootstrap's exit code is deliberately NOT
# treated as the daemon-health verdict. bootstrap routinely returns nonzero for
# reasons that do NOT mean the daemon failed — the label is already bootstrapped
# (a prior run / launchd got there first), a transient bootstrap race, etc. —
# even when the daemon is up or is about to come up. The source of truth is the
# caller's verify_daemon_health socket-probe loop (a late-arriving daemon still
# flips it to success). So a nonzero here is a WARNING, never a fatal `error`:
# `error` calls exit 1 and would abort the whole install BEFORE the socket is
# ever probed — the exact false-failure this slice removes.
start_launchd_service() {
    local plist_path="${MACOS_LAUNCHAGENTS_DIR}/${MACOS_PLIST_NAME}"
    local gui_uid

    gui_uid="$(id -u)"

    info "Starting daemon service..."

    # Unload if already loaded (handles upgrades gracefully)
    launchctl bootout "gui/${gui_uid}/${MACOS_SERVICE_LABEL}" 2>/dev/null || true

    # Request load/start. Do NOT abort on a nonzero bootstrap — the socket probe
    # in setup_macos_daemon_service is the arbiter of success/failure.
    if launchctl bootstrap "gui/${gui_uid}" "${plist_path}"; then
        info "  Service loaded: ${MACOS_SERVICE_LABEL}"
    else
        warn "  launchctl bootstrap returned nonzero — continuing; the daemon socket probe decides."
    fi
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
#
# INSTALL-ROBUSTNESS-2 (B): verify_daemon_health MOVED to the shared template
# (install.template.sh) so there is ONE definition in the bundled installer. The
# old copy here and its twin in lib/linux.sh collided under injection — the Linux
# copy won and ran on macOS, statting a Linux pidfile that never exists here and
# reporting a running daemon as "failed". The new predicate is socket liveness (the
# socket_ping probe from `rmap doctor --json`, in daemon_socket_answers), which is
# platform-agnostic, so it is defined once, in the template. is_daemon_running()/
# get_daemon_pid() above are now UNUSED by the install path (kept for potential
# external/lifecycle callers; the report's PID comes from the collision-free
# daemon_pid_best_effort() in the template).

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
#
# INSTALL-ROBUSTNESS-2 (B): socket liveness is the source of truth. Sets the
# DAEMON_OUTCOME global (started / already running / failed) for the final summary.
setup_macos_daemon_service() {
    # Ensure log directory exists
    mkdir -p "${MACOS_LOG_DIR}"
    chmod 700 "${MACOS_LOG_DIR}"

    # Already running? If the socket answers (launchd got there first, or a prior
    # run started the daemon), do NOT start anything — report it and succeed.
    if daemon_socket_answers; then
        local pid
        pid="$(daemon_pid_best_effort)"
        info "Daemon already running (pid: ${pid:-unknown}) — leaving it in place."
        DAEMON_OUTCOME="already running"
        return 0
    fi

    install_launchd_plist
    start_launchd_service

    # The retry predicate is the socket probe, NOT start_launchd_service's exit
    # status — a late-arriving daemon (launchd throttling) flips this to success.
    if verify_daemon_health 5 2; then
        local pid
        pid="$(daemon_pid_best_effort)"
        info "  Daemon is answering on the socket (pid: ${pid:-unknown})"
        DAEMON_OUTCOME="started"
        return 0
    fi

    # Report failure ONLY when the socket never answered after the retry budget —
    # and name the two facts the user needs to act: the socket path probed and where
    # the daemon log lives. Name the ACTUAL probed path by asking rgr which socket it
    # resolved (resolved_socket_path parses `rmap doctor --json`) rather than
    # reconstructing it here: rgr's resolver can pick the LEGACY socket over canonical
    # in the migration case (canonical unreachable, legacy reachable-but-ping-fails), so
    # a bash ${RMAP_SOCKET_PATH:-${MACOS_SOCKET_PATH}} could name the WRONG path. The
    # canonical default is passed only as the last-resort fallback (JSON unparseable);
    # it still honors an RMAP_SOCKET_PATH override, which rgr reports back verbatim.
    local probed_socket_path
    probed_socket_path="$(resolved_socket_path "${RMAP_SOCKET_PATH:-${MACOS_SOCKET_PATH}}")"
    DAEMON_OUTCOME="failed"
    warn ""
    warn "Daemon did not answer on its socket after the retry budget."
    warn "  Socket probed: ${probed_socket_path}"
    warn "  Daemon log:    ${MACOS_LOG_DIR}/daemon.log"
    warn "  Start manually:  rmapd"
    warn "  Inspect service: launchctl print gui/$(id -u)/${MACOS_SERVICE_LABEL}"
    return 1
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
