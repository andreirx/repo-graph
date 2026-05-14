#!/bin/bash
# Linux-specific installer functions for repo-graph.
#
# Sourced by scripts/install.sh after platform detection.
# Contains systemd user service management and manual fallback.
#
# Path contract: Must match rust/crates/rgr/src/cli/paths.rs
# See: docs/slices/linux-1-linux-installer.md

# ─────────────────────────────────────────────────────────────────────────────
# Constants (must match paths.rs and DIST-1 D3)
# ─────────────────────────────────────────────────────────────────────────────

LINUX_CONFIG_DIR="${HOME}/.config/rmap"
LINUX_DATA_DIR="${HOME}/.local/share/rmap"
LINUX_LOG_DIR="${HOME}/.local/share/rmap/logs"
LINUX_SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
LINUX_SERVICE_NAME="rmapd.service"
LINUX_PID_FILE="${LINUX_DATA_DIR}/daemon.pid"

# Service mode: "systemd" or "manual"
LINUX_SERVICE_MODE=""

# ─────────────────────────────────────────────────────────────────────────────
# systemd Detection
# ─────────────────────────────────────────────────────────────────────────────

# Check if systemd is available and user session works.
# Sets LINUX_SERVICE_MODE to "systemd" or "manual".
detect_systemd() {
    LINUX_SERVICE_MODE="manual"  # default fallback

    if ! command -v systemctl &> /dev/null; then
        info "systemd not found - using manual daemon mode"
        return
    fi

    # Check if user session is available
    if ! systemctl --user status &> /dev/null 2>&1; then
        info "systemd user session not available - using manual daemon mode"
        info "  (Enable with: loginctl enable-linger $USER)"
        return
    fi

    LINUX_SERVICE_MODE="systemd"
    info "systemd user session available"
}

# ─────────────────────────────────────────────────────────────────────────────
# systemd Service Management
# ─────────────────────────────────────────────────────────────────────────────

# Emit systemd unit content to stdout.
# Used by install_systemd_unit. Can be overridden in bundled mode.
emit_linux_service() {
    # Try template file first (modular mode)
    if [[ -n "${SCRIPT_DIR:-}" ]] && [[ -f "${SCRIPT_DIR}/templates/${LINUX_SERVICE_NAME}" ]]; then
        cat "${SCRIPT_DIR}/templates/${LINUX_SERVICE_NAME}"
        return
    fi

    # Fallback: embedded template (bundled mode)
    cat << 'SERVICE_TEMPLATE'
[Unit]
Description=repo-graph daemon
Documentation=https://github.com/andreirx/repo-graph
After=default.target

[Service]
Type=simple
ExecStart=%h/.local/bin/rmapd
Restart=on-failure
RestartSec=10

StandardOutput=append:%h/.local/share/rmap/logs/daemon.log
StandardError=append:%h/.local/share/rmap/logs/daemon.log

Environment=RMAP_LOG_LEVEL=info

Nice=10
IOSchedulingClass=idle

[Install]
WantedBy=default.target
SERVICE_TEMPLATE
}

# Install the systemd unit file from template.
install_systemd_unit() {
    local unit_path="${LINUX_SYSTEMD_USER_DIR}/${LINUX_SERVICE_NAME}"

    info "Installing systemd user service..."

    # Ensure systemd user directory exists
    mkdir -p "${LINUX_SYSTEMD_USER_DIR}"

    # Write unit file (systemd expands %h, no substitution needed)
    emit_linux_service > "${unit_path}"

    # Set correct permissions (644 for unit files)
    chmod 644 "${unit_path}"

    # Reload systemd to pick up new unit
    systemctl --user daemon-reload

    info "  Installed: ${unit_path}"
}

# Enable and start the daemon via systemd.
start_systemd_service() {
    info "Starting daemon service..."

    # Stop if already running (handles upgrades gracefully)
    systemctl --user stop "${LINUX_SERVICE_NAME}" 2>/dev/null || true

    # Enable (start on login)
    if ! systemctl --user enable "${LINUX_SERVICE_NAME}"; then
        warn "Failed to enable service"
    fi

    # Start
    if ! systemctl --user start "${LINUX_SERVICE_NAME}"; then
        error "Failed to start systemd service"
    fi

    info "  Service started: ${LINUX_SERVICE_NAME}"
}

# Stop and disable the daemon via systemd.
stop_systemd_service() {
    info "Stopping daemon service..."

    if systemctl --user stop "${LINUX_SERVICE_NAME}" 2>/dev/null; then
        info "  Service stopped: ${LINUX_SERVICE_NAME}"
    else
        info "  Service was not running"
    fi

    if systemctl --user disable "${LINUX_SERVICE_NAME}" 2>/dev/null; then
        info "  Service disabled"
    fi
}

# Remove the systemd unit file.
remove_systemd_unit() {
    local unit_path="${LINUX_SYSTEMD_USER_DIR}/${LINUX_SERVICE_NAME}"

    if [[ -f "${unit_path}" ]]; then
        rm -f "${unit_path}"
        systemctl --user daemon-reload
        info "  Removed: ${unit_path}"
    fi
}

# Check if daemon is running via systemd.
# Returns 0 if running, 1 if not.
is_systemd_daemon_running() {
    systemctl --user is-active --quiet "${LINUX_SERVICE_NAME}" 2>/dev/null
}

# Get daemon PID from systemd.
get_systemd_daemon_pid() {
    systemctl --user show "${LINUX_SERVICE_NAME}" --property=MainPID --value 2>/dev/null
}

# ─────────────────────────────────────────────────────────────────────────────
# Manual Daemon Mode (non-systemd fallback)
# ─────────────────────────────────────────────────────────────────────────────

# Start daemon manually with PID file.
start_manual_daemon() {
    info "Starting daemon manually..."

    # Ensure directories exist
    mkdir -p "${LINUX_LOG_DIR}"

    # Stop existing daemon if running
    stop_manual_daemon 2>/dev/null || true

    # Start daemon in background
    nohup "${INSTALL_DIR}/rmapd" >> "${LINUX_LOG_DIR}/daemon.log" 2>&1 &
    local pid=$!

    # Write PID file
    echo "${pid}" > "${LINUX_PID_FILE}"

    info "  Daemon started (pid: ${pid})"
    info "  PID file: ${LINUX_PID_FILE}"
}

# Stop manually-started daemon.
stop_manual_daemon() {
    if [[ ! -f "${LINUX_PID_FILE}" ]]; then
        info "  No PID file found"
        return 0
    fi

    local pid
    pid="$(cat "${LINUX_PID_FILE}" 2>/dev/null)"

    if [[ -z "${pid}" ]]; then
        rm -f "${LINUX_PID_FILE}"
        return 0
    fi

    info "Stopping daemon (pid: ${pid})..."

    if kill -0 "${pid}" 2>/dev/null; then
        # Process exists, send SIGTERM
        kill "${pid}" 2>/dev/null || true

        # Wait up to 5 seconds for graceful shutdown
        local count=0
        while kill -0 "${pid}" 2>/dev/null && [[ ${count} -lt 5 ]]; do
            sleep 1
            ((count++))
        done

        # Force kill if still running
        if kill -0 "${pid}" 2>/dev/null; then
            warn "  Daemon did not stop gracefully, sending SIGKILL"
            kill -9 "${pid}" 2>/dev/null || true
        fi
    fi

    rm -f "${LINUX_PID_FILE}"
    info "  Daemon stopped"
}

# Check if daemon is running via PID file.
# Returns 0 if running, 1 if not.
is_manual_daemon_running() {
    if [[ ! -f "${LINUX_PID_FILE}" ]]; then
        return 1
    fi

    local pid
    pid="$(cat "${LINUX_PID_FILE}" 2>/dev/null)"

    if [[ -z "${pid}" ]]; then
        return 1
    fi

    kill -0 "${pid}" 2>/dev/null
}

# Get daemon PID from PID file.
get_manual_daemon_pid() {
    if [[ -f "${LINUX_PID_FILE}" ]]; then
        cat "${LINUX_PID_FILE}" 2>/dev/null
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Unified Interface (dispatches based on service mode)
# ─────────────────────────────────────────────────────────────────────────────

# Check if daemon is running (any mode).
is_daemon_running() {
    if [[ "${LINUX_SERVICE_MODE}" == "systemd" ]]; then
        is_systemd_daemon_running
    else
        is_manual_daemon_running
    fi
}

# Get daemon PID (any mode).
get_daemon_pid() {
    if [[ "${LINUX_SERVICE_MODE}" == "systemd" ]]; then
        get_systemd_daemon_pid
    else
        get_manual_daemon_pid
    fi
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
    warn "Check logs: ${LINUX_LOG_DIR}/daemon.log"

    if [[ "${LINUX_SERVICE_MODE}" == "systemd" ]]; then
        warn "Check service: systemctl --user status ${LINUX_SERVICE_NAME}"
    fi

    return 1
}

# ─────────────────────────────────────────────────────────────────────────────
# Full Service Setup (called by install.sh)
# ─────────────────────────────────────────────────────────────────────────────

# Complete Linux daemon service setup.
# Called by install.sh when BINARY_ONLY is false.
# Sets LINUX_SERVICE_MODE and returns it for manifest writing.
setup_linux_daemon_service() {
    # Ensure log directory exists
    mkdir -p "${LINUX_LOG_DIR}"
    chmod 700 "${LINUX_LOG_DIR}"

    # Detect available service mode
    detect_systemd

    if [[ "${LINUX_SERVICE_MODE}" == "systemd" ]]; then
        install_systemd_unit
        start_systemd_service
    else
        start_manual_daemon
    fi

    if ! verify_daemon_health 5 2; then
        warn ""
        warn "Daemon did not start successfully."
        warn "You can start it manually: rmapd"
        if [[ "${LINUX_SERVICE_MODE}" == "systemd" ]]; then
            warn "Or check: systemctl --user status ${LINUX_SERVICE_NAME}"
        fi
        return 1
    fi

    # Export for manifest writing
    export LINUX_SERVICE_MODE
    return 0
}

# ─────────────────────────────────────────────────────────────────────────────
# Uninstall Support
# ─────────────────────────────────────────────────────────────────────────────

# Complete Linux service uninstall.
# Called by rmap uninstall or standalone uninstaller.
# Handles both systemd and manual modes based on what's present.
uninstall_linux_daemon_service() {
    # Try systemd first
    if [[ -f "${LINUX_SYSTEMD_USER_DIR}/${LINUX_SERVICE_NAME}" ]]; then
        stop_systemd_service
        remove_systemd_unit
    fi

    # Also clean up manual mode artifacts if present
    if [[ -f "${LINUX_PID_FILE}" ]]; then
        stop_manual_daemon
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# User Linger (optional, for daemon persistence without login)
# ─────────────────────────────────────────────────────────────────────────────

# Check if user linger is enabled.
is_linger_enabled() {
    if command -v loginctl &> /dev/null; then
        loginctl show-user "$USER" 2>/dev/null | grep -q "Linger=yes"
    else
        return 1
    fi
}

# Suggest enabling linger if not already enabled.
suggest_linger() {
    if [[ "${LINUX_SERVICE_MODE}" != "systemd" ]]; then
        return
    fi

    if ! is_linger_enabled; then
        info ""
        info "Note: To keep the daemon running after logout, enable user linger:"
        info "  sudo loginctl enable-linger $USER"
    fi
}
