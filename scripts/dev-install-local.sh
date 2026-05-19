#!/usr/bin/env bash
#
# dev-install-local.sh - Developer-only local build install/refresh
#
# Builds rmap and rmapd from source and installs to ~/.local/bin,
# restarting the daemon service. This is NOT a user-facing install
# command - it is a maintainer workflow for testing local changes.
#
# PLATFORM: macOS only (launchd). Linux support pending.
#
# USAGE:
#   ./scripts/dev-install-local.sh
#
# With optional cargo features (e.g., performance tracing):
#   CARGO_FEATURES="repo-graph-daemon-runtime/perf-trace" ./scripts/dev-install-local.sh
#
# PREREQUISITES:
#   - Rust toolchain (cargo)
#   - Running from repo-graph source tree root
#   - launchd service already installed (via install.sh)
#
# WHAT IT DOES:
#   1. Verifies source tree
#   2. Builds release binaries
#   3. Stops daemon gracefully (launchctl bootout)
#   4. Removes stale socket
#   5. Installs binaries atomically to ~/.local/bin
#   6. Restarts daemon (launchctl bootstrap)
#   7. Validates: versions, doctor, socket reachable
#
set -euo pipefail

# ── Constants ─────────────────────────────────────────────────────────────────

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly INSTALL_DIR="${HOME}/.local/bin"
readonly SOCKET_PATH="${HOME}/Library/Application Support/repo-graph/daemon.sock"
readonly LAUNCHD_LABEL="com.repo-graph.rmapd"
readonly LAUNCHD_PLIST="${HOME}/Library/LaunchAgents/${LAUNCHD_LABEL}.plist"
readonly LOG_FILE="${HOME}/Library/Logs/repo-graph/daemon.log"

# ── Helpers ───────────────────────────────────────────────────────────────────

info()  { echo "==> $*"; }
warn()  { echo "WARN: $*" >&2; }
error() { echo "ERROR: $*" >&2; exit 1; }

check_platform() {
    if [[ "$(uname -s)" != "Darwin" ]]; then
        error "This script is macOS-only. Linux support pending."
    fi
}

check_source_tree() {
    if [[ ! -f "${REPO_ROOT}/rust/Cargo.toml" ]] || [[ ! -d "${REPO_ROOT}/rust/crates" ]]; then
        error "Must run from repo-graph source tree root. rust/Cargo.toml or rust/crates not found."
    fi
    info "Source tree verified: ${REPO_ROOT}"
}

check_rust_toolchain() {
    if ! command -v cargo &>/dev/null; then
        error "Rust toolchain not found. Install via rustup: https://rustup.rs"
    fi
    local rust_version
    rust_version=$(rustc --version 2>/dev/null || echo "unknown")
    info "Rust toolchain: ${rust_version}"
}

check_launchd_service_exists() {
    if [[ ! -f "${LAUNCHD_PLIST}" ]]; then
        error "launchd service not installed: ${LAUNCHD_PLIST}
Run the full installer first: ./scripts/install.sh"
    fi
}

# ── Build ─────────────────────────────────────────────────────────────────────

build_release() {
    info "Building release binaries..."
    cd "${REPO_ROOT}/rust"

    local cargo_cmd=(cargo build --release --bin rmap --bin rmapd)

    # Optional feature flags via environment variable
    # Usage: CARGO_FEATURES="repo-graph-daemon-runtime/perf-trace" ./scripts/dev-install-local.sh
    if [[ -n "${CARGO_FEATURES:-}" ]]; then
        cargo_cmd+=(--features "${CARGO_FEATURES}")
        info "  Features: ${CARGO_FEATURES}"
    fi

    "${cargo_cmd[@]}"

    if [[ ! -x "${REPO_ROOT}/rust/target/release/rmap" ]]; then
        error "rmap binary not found after build"
    fi
    if [[ ! -x "${REPO_ROOT}/rust/target/release/rmapd" ]]; then
        error "rmapd binary not found after build"
    fi

    info "Build complete"
}

# ── Service Lifecycle ─────────────────────────────────────────────────────────

stop_daemon_graceful() {
    info "Stopping daemon service..."

    local uid
    uid=$(id -u)

    # Graceful stop via launchctl bootout
    if launchctl bootout "gui/${uid}/${LAUNCHD_LABEL}" 2>/dev/null; then
        info "  Service stopped gracefully"
    else
        info "  Service was not running or already stopped"
    fi

    # Wait briefly for process to exit
    sleep 1

    # Check if any rmapd processes remain
    if pgrep -x rmapd &>/dev/null; then
        warn "rmapd process still running after graceful stop"
        info "  Sending SIGTERM..."
        pkill -TERM -x rmapd 2>/dev/null || true
        sleep 2

        # Last resort: SIGKILL
        if pgrep -x rmapd &>/dev/null; then
            warn "rmapd still running, sending SIGKILL..."
            pkill -KILL -x rmapd 2>/dev/null || true
            sleep 1
        fi
    fi

    # Verify stopped
    if pgrep -x rmapd &>/dev/null; then
        error "Failed to stop rmapd process"
    fi

    info "  Daemon stopped"
}

remove_stale_socket() {
    if [[ -e "${SOCKET_PATH}" ]]; then
        info "Removing stale socket..."
        rm -f "${SOCKET_PATH}"
    fi
}

install_binaries_atomic() {
    info "Installing binaries to ${INSTALL_DIR}..."

    mkdir -p "${INSTALL_DIR}"

    local tmp_rmap tmp_rmapd
    tmp_rmap=$(mktemp "${INSTALL_DIR}/rmap.XXXXXX")
    tmp_rmapd=$(mktemp "${INSTALL_DIR}/rmapd.XXXXXX")

    # Copy to temp files
    cp "${REPO_ROOT}/rust/target/release/rmap" "${tmp_rmap}"
    cp "${REPO_ROOT}/rust/target/release/rmapd" "${tmp_rmapd}"
    chmod 755 "${tmp_rmap}" "${tmp_rmapd}"

    # Atomic move
    mv -f "${tmp_rmap}" "${INSTALL_DIR}/rmap"
    mv -f "${tmp_rmapd}" "${INSTALL_DIR}/rmapd"

    info "  Binaries installed"
}

start_daemon() {
    info "Starting daemon service..."

    local uid
    uid=$(id -u)

    if launchctl bootstrap "gui/${uid}" "${LAUNCHD_PLIST}"; then
        info "  Service started"
    else
        error "Failed to start daemon service. Check: ${LOG_FILE}"
    fi

    # Wait for startup
    sleep 2
}

# ── Validation ────────────────────────────────────────────────────────────────

validate_installation() {
    info "Validating installation..."

    local failed=0

    # Version checks
    local rmap_version rmapd_version
    if rmap_version=$("${INSTALL_DIR}/rmap" --version 2>&1); then
        info "  rmap: ${rmap_version}"
    else
        warn "  rmap --version failed"
        failed=1
    fi

    if rmapd_version=$("${INSTALL_DIR}/rmapd" --version 2>&1); then
        info "  rmapd: ${rmapd_version}"
    else
        warn "  rmapd --version failed"
        failed=1
    fi

    # Socket reachable
    if [[ -S "${SOCKET_PATH}" ]]; then
        info "  Socket: ${SOCKET_PATH} (exists)"
    else
        warn "  Socket not found: ${SOCKET_PATH}"
        failed=1
    fi

    # Daemon process running
    if pgrep -x rmapd &>/dev/null; then
        local pid
        pid=$(pgrep -x rmapd | head -1)
        info "  Daemon: running (pid ${pid})"
    else
        warn "  Daemon process not running"
        failed=1
    fi

    # Doctor check (if available)
    if "${INSTALL_DIR}/rmap" doctor &>/dev/null; then
        info "  Doctor: healthy"
    else
        # Doctor might not exist yet or might fail for other reasons
        # This is a soft failure
        warn "  Doctor: check failed or not available"
    fi

    if [[ $failed -ne 0 ]]; then
        error "Validation failed. Check logs: ${LOG_FILE}"
    fi

    info "Validation passed"
}

# ── Main ──────────────────────────────────────────────────────────────────────

main() {
    echo ""
    echo "dev-install-local.sh - Developer local build refresh"
    echo "====================================================="
    echo ""

    check_platform
    check_source_tree
    check_rust_toolchain
    check_launchd_service_exists

    echo ""
    build_release

    echo ""
    stop_daemon_graceful
    remove_stale_socket
    install_binaries_atomic
    start_daemon

    echo ""
    validate_installation

    echo ""
    echo "====================================================="
    echo "Done. Local build installed and daemon restarted."
    echo ""
    echo "Installed:"
    echo "  ${INSTALL_DIR}/rmap"
    echo "  ${INSTALL_DIR}/rmapd"
    echo ""
}

main "$@"
