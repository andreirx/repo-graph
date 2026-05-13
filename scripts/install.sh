#!/bin/bash
# repo-graph installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/andreirx/repo-graph/main/scripts/install.sh | bash
#
# Options (via environment variables):
#   RMAP_VERSION=0.1.0      # Install specific version (default: latest)
#   RMAP_INSTALL_DIR=~/.local/bin  # Install directory
#   RMAP_BINARY_ONLY=1      # Skip daemon service and integrations
#   RMAP_NON_INTERACTIVE=1  # Non-interactive mode (no prompts)
#
# Options (via command line):
#   --version <ver>         # Install specific version
#   --binary-only           # Skip daemon service and integrations
#   --non-interactive       # Non-interactive mode
#   --integrate <hosts>     # Comma-separated hosts to integrate (claude-code,codex)
#   --source               # Build from source (requires Rust toolchain)
#
# See: docs/slices/dist-1-distribution-install-contract.md
#      docs/slices/rel-1-release-pipeline.md

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# Configuration
# ─────────────────────────────────────────────────────────────────────────────

# GitHub repository
REPO_OWNER="${RMAP_REPO_OWNER:-andreirx}"
REPO_NAME="repo-graph"
RELEASES_URL="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases"
DOWNLOAD_BASE="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download"

# Default install directory (user-local, no sudo required)
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

# Platform-specific paths
if [[ "$(uname -s)" == "Darwin" ]]; then
    CONFIG_DIR="${HOME}/Library/Application Support/repo-graph"
    DATA_DIR="${HOME}/Library/Application Support/repo-graph"
    LOG_DIR="${HOME}/Library/Logs/repo-graph"
else
    CONFIG_DIR="${HOME}/.config/rmap"
    DATA_DIR="${HOME}/.local/share/rmap"
    LOG_DIR="${HOME}/.local/share/rmap/logs"
fi

# State
VERSION=""
INSTALL_DIR=""
BINARY_ONLY="${RMAP_BINARY_ONLY:-false}"
NON_INTERACTIVE="${RMAP_NON_INTERACTIVE:-false}"
BUILD_FROM_SOURCE=false
INTEGRATE_HOSTS=""

# ─────────────────────────────────────────────────────────────────────────────
# Utilities
# ─────────────────────────────────────────────────────────────────────────────

info() {
    echo "[info] $*"
}

warn() {
    echo "[warn] $*" >&2
}

error() {
    echo "[error] $*" >&2
    exit 1
}

confirm() {
    local prompt="$1"
    local default="${2:-n}"

    if [[ "${NON_INTERACTIVE}" == "true" ]]; then
        return 0
    fi

    if [[ "${default}" == "y" ]]; then
        read -r -p "${prompt} [Y/n]: " response
        [[ -z "${response}" || "${response}" =~ ^[Yy] ]]
    else
        read -r -p "${prompt} [y/N]: " response
        [[ "${response}" =~ ^[Yy] ]]
    fi
}

command_exists() {
    command -v "$1" &> /dev/null
}

# ─────────────────────────────────────────────────────────────────────────────
# Platform Detection
# ─────────────────────────────────────────────────────────────────────────────

detect_platform() {
    local os arch

    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"

    # Normalize architecture
    case "${arch}" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            arch="aarch64"
            ;;
        *)
            error "Unsupported architecture: ${arch}"
            ;;
    esac

    # Normalize platform
    case "${os}" in
        darwin)
            PLATFORM="darwin"
            ;;
        linux)
            PLATFORM="linux"
            ;;
        *)
            error "Unsupported platform: ${os}"
            ;;
    esac

    ARCH="${arch}"
    info "Detected platform: ${PLATFORM}-${ARCH}"
}

# ─────────────────────────────────────────────────────────────────────────────
# Toolchain Detection (informational only, not required for binary install)
# ─────────────────────────────────────────────────────────────────────────────

detect_toolchains() {
    info "Detecting toolchains..."

    # Rust (required for source build)
    if command_exists rustc; then
        local rust_version
        rust_version="$(rustc --version 2>/dev/null | awk '{print $2}')"
        info "  Rust: ${rust_version}"
        RUST_AVAILABLE=true
    else
        info "  Rust: not found"
        RUST_AVAILABLE=false
    fi

    # Cargo
    if command_exists cargo; then
        local cargo_version
        cargo_version="$(cargo --version 2>/dev/null | awk '{print $2}')"
        info "  Cargo: ${cargo_version}"
    fi

    # Node.js (not required for rmap, informational only)
    if command_exists node; then
        local node_version
        node_version="$(node --version 2>/dev/null)"
        info "  Node.js: ${node_version}"
    else
        info "  Node.js: not found (not required)"
    fi

    # npm (not required for rmap, informational only)
    if command_exists npm; then
        local npm_version
        npm_version="$(npm --version 2>/dev/null)"
        info "  npm: ${npm_version}"
    else
        info "  npm: not found (not required)"
    fi

    # Note about requirements
    if [[ "${BUILD_FROM_SOURCE}" == "true" && "${RUST_AVAILABLE}" != "true" ]]; then
        error "Source build requested but Rust toolchain not found. Install from: https://rustup.rs"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Version Resolution
# ─────────────────────────────────────────────────────────────────────────────

resolve_version() {
    if [[ -n "${VERSION}" && "${VERSION}" != "latest" ]]; then
        info "Using specified version: ${VERSION}"
        return
    fi

    info "Fetching latest version..."

    if ! command_exists curl; then
        error "curl is required but not found"
    fi

    # Get latest release from GitHub API
    local latest_tag
    latest_tag="$(curl -fsSL "${RELEASES_URL}/latest" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

    if [[ -z "${latest_tag}" ]]; then
        error "Could not determine latest version. Specify version with --version or RMAP_VERSION"
    fi

    # Remove 'v' prefix
    VERSION="${latest_tag#v}"
    info "Latest version: ${VERSION}"
}

# ─────────────────────────────────────────────────────────────────────────────
# Download and Verify
# ─────────────────────────────────────────────────────────────────────────────

download_binary() {
    local artifact="rmap-${VERSION}-${PLATFORM}-${ARCH}.tar.gz"
    local download_url="${DOWNLOAD_BASE}/v${VERSION}/${artifact}"
    local checksum_url="${download_url}.sha256"

    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap "rm -rf '${tmp_dir}'" EXIT

    info "Downloading ${artifact}..."
    if ! curl -fsSL -o "${tmp_dir}/${artifact}" "${download_url}"; then
        error "Failed to download ${artifact} from ${download_url}"
    fi

    info "Downloading checksum..."
    if ! curl -fsSL -o "${tmp_dir}/${artifact}.sha256" "${checksum_url}"; then
        error "Failed to download checksum"
    fi

    info "Verifying checksum..."
    cd "${tmp_dir}"
    if command_exists shasum; then
        if ! shasum -a 256 -c "${artifact}.sha256"; then
            error "Checksum verification failed!"
        fi
    elif command_exists sha256sum; then
        if ! sha256sum -c "${artifact}.sha256"; then
            error "Checksum verification failed!"
        fi
    else
        warn "No checksum tool found (shasum or sha256sum). Skipping verification."
    fi

    info "Extracting archive..."
    tar -xzf "${artifact}"

    # Move binaries to install location
    EXTRACTED_DIR="rmap-${VERSION}-${PLATFORM}-${ARCH}"
    if [[ ! -f "${EXTRACTED_DIR}/rmap" ]]; then
        error "CLI binary (rmap) not found in archive"
    fi
    if [[ ! -f "${EXTRACTED_DIR}/rmapd" ]]; then
        error "Daemon binary (rmapd) not found in archive"
    fi

    # Create install directory if needed
    mkdir -p "${INSTALL_DIR}"

    info "Installing CLI to ${INSTALL_DIR}/rmap..."
    install -m 755 "${EXTRACTED_DIR}/rmap" "${INSTALL_DIR}/rmap"

    info "Installing daemon to ${INSTALL_DIR}/rmapd..."
    install -m 755 "${EXTRACTED_DIR}/rmapd" "${INSTALL_DIR}/rmapd"

    # Verify installation
    if ! "${INSTALL_DIR}/rmap" --version > /dev/null 2>&1; then
        # On macOS, Gatekeeper may block unsigned binaries
        if [[ "${PLATFORM}" == "darwin" ]]; then
            warn ""
            warn "macOS Gatekeeper may have blocked the binaries."
            warn "To allow, run:"
            warn "  xattr -d com.apple.quarantine ${INSTALL_DIR}/rmap"
            warn "  xattr -d com.apple.quarantine ${INSTALL_DIR}/rmapd"
            warn ""
            warn "Or: System Preferences -> Security & Privacy -> Allow"
            warn ""

            if confirm "Run xattr commands now?"; then
                xattr -d com.apple.quarantine "${INSTALL_DIR}/rmap" 2>/dev/null || true
                xattr -d com.apple.quarantine "${INSTALL_DIR}/rmapd" 2>/dev/null || true

                if "${INSTALL_DIR}/rmap" --version > /dev/null 2>&1; then
                    info "Binaries unblocked successfully"
                else
                    error "Binary verification still failed after unblocking"
                fi
            fi
        else
            error "Binary verification failed"
        fi
    fi

    local installed_version
    installed_version="$("${INSTALL_DIR}/rmap" --version 2>/dev/null | head -1)"
    info "Installed: ${installed_version}"
}

# ─────────────────────────────────────────────────────────────────────────────
# Source Build
# ─────────────────────────────────────────────────────────────────────────────

build_from_source() {
    info "Building from source..."

    if [[ "${RUST_AVAILABLE}" != "true" ]]; then
        error "Rust toolchain required for source build. Install from: https://rustup.rs"
    fi

    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap "rm -rf '${tmp_dir}'" EXIT

    info "Cloning repository..."
    if ! command_exists git; then
        error "git is required for source build"
    fi

    git clone --depth 1 --branch "v${VERSION}" \
        "https://github.com/${REPO_OWNER}/${REPO_NAME}.git" \
        "${tmp_dir}/repo-graph"

    cd "${tmp_dir}/repo-graph/rust"

    info "Building release binaries..."
    cargo build --release -p repo-graph-rgr

    # Install both binaries
    mkdir -p "${INSTALL_DIR}"
    install -m 755 "target/release/rmap" "${INSTALL_DIR}/rmap"
    install -m 755 "target/release/rmapd" "${INSTALL_DIR}/rmapd"

    local installed_version
    installed_version="$("${INSTALL_DIR}/rmap" --version 2>/dev/null | head -1)"
    info "Installed: ${installed_version}"
}

# ─────────────────────────────────────────────────────────────────────────────
# Directory Setup
# ─────────────────────────────────────────────────────────────────────────────

create_directories() {
    info "Creating directories..."

    mkdir -p "${CONFIG_DIR}"
    mkdir -p "${DATA_DIR}/databases"
    mkdir -p "${DATA_DIR}/sessions"
    mkdir -p "${LOG_DIR}"

    # Set permissions
    chmod 700 "${CONFIG_DIR}"
    chmod 700 "${DATA_DIR}"
    chmod 700 "${LOG_DIR}"

    info "  Config: ${CONFIG_DIR}"
    info "  Data: ${DATA_DIR}"
    info "  Logs: ${LOG_DIR}"
}

# ─────────────────────────────────────────────────────────────────────────────
# PATH Setup
# ─────────────────────────────────────────────────────────────────────────────

setup_path() {
    # Check if install dir is in PATH
    if [[ ":${PATH}:" == *":${INSTALL_DIR}:"* ]]; then
        info "${INSTALL_DIR} is already in PATH"
        return
    fi

    info "${INSTALL_DIR} is not in PATH"

    if [[ "${NON_INTERACTIVE}" == "true" ]]; then
        warn "Add to PATH manually: export PATH=\"${INSTALL_DIR}:\$PATH\""
        return
    fi

    # Detect shell profile
    local shell_profile=""
    local shell_name=""

    case "${SHELL}" in
        */zsh)
            shell_name="zsh"
            shell_profile="${HOME}/.zshrc"
            ;;
        */bash)
            shell_name="bash"
            if [[ -f "${HOME}/.bash_profile" ]]; then
                shell_profile="${HOME}/.bash_profile"
            else
                shell_profile="${HOME}/.bashrc"
            fi
            ;;
        *)
            shell_profile="${HOME}/.profile"
            ;;
    esac

    if confirm "Add ${INSTALL_DIR} to PATH in ${shell_profile}?"; then
        local path_line="export PATH=\"${INSTALL_DIR}:\$PATH\""

        # Check if already added
        if grep -q "${INSTALL_DIR}" "${shell_profile}" 2>/dev/null; then
            info "PATH entry already exists in ${shell_profile}"
        else
            echo "" >> "${shell_profile}"
            echo "# repo-graph" >> "${shell_profile}"
            echo "${path_line}" >> "${shell_profile}"
            info "Added to ${shell_profile}"
            info ""
            info "Run 'source ${shell_profile}' or start a new terminal to update PATH"
        fi
    else
        warn "Add to PATH manually: export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Daemon Service (stub - implementation in MAC-1/LINUX-1)
# ─────────────────────────────────────────────────────────────────────────────

setup_daemon_service() {
    if [[ "${BINARY_ONLY}" == "true" ]]; then
        info "Skipping daemon service (--binary-only)"
        return
    fi

    info ""
    info "Daemon service setup is available but not yet implemented."
    info "The daemon can be run manually: rmap daemon"
    info ""
    info "Service setup will be available in a future release."
    info "See: docs/slices/mac-1-macos-installer.md (macOS)"
    info "     docs/slices/linux-1-linux-installer.md (Linux)"

    # TODO: Implement in MAC-1 and LINUX-1
    # - macOS: launchd user agent
    # - Linux: systemd --user unit
}

# ─────────────────────────────────────────────────────────────────────────────
# Host Integration (stub - implementation in CLAUDE-1/CODEX-1)
# ─────────────────────────────────────────────────────────────────────────────

detect_hosts() {
    if [[ "${BINARY_ONLY}" == "true" ]]; then
        return
    fi

    info ""
    info "Detecting agent hosts..."

    # Claude Code
    if [[ -f "${HOME}/.claude/settings.json" ]]; then
        info "  Claude Code: found (~/.claude/settings.json)"
        DETECTED_CLAUDE=true
    elif [[ -d "${HOME}/.claude" ]]; then
        info "  Claude Code: directory exists (no settings)"
        DETECTED_CLAUDE=true
    else
        DETECTED_CLAUDE=false
    fi

    # Codex
    if [[ -f "${HOME}/.codex/hooks.json" ]] || [[ -d "${HOME}/.codex" ]]; then
        info "  Codex: found"
        DETECTED_CODEX=true
    else
        DETECTED_CODEX=false
    fi

    # Note about integrations
    if [[ "${DETECTED_CLAUDE}" == "true" || "${DETECTED_CODEX}" == "true" ]]; then
        info ""
        info "Host integrations can be installed separately:"
        info "  rmap integrate claude-code"
        info "  rmap integrate codex"
        info ""
        info "Integration commands will be available after HOOK-1 and CLAUDE-1/CODEX-1 are implemented."
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Install Manifest
# ─────────────────────────────────────────────────────────────────────────────

write_manifest() {
    local manifest_path="${CONFIG_DIR}/install-manifest.json"

    info "Writing install manifest to ${manifest_path}..."

    local installed_at
    installed_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

    cat > "${manifest_path}" << EOF
{
  "schema_version": "1",
  "installed_at": "${installed_at}",
  "installer_version": "1.0.0",
  "platform": "${PLATFORM}",
  "arch": "${ARCH}",
  "install_mode": "user",
  "components": {
    "rmap": {
      "path": "${INSTALL_DIR}/rmap",
      "version": "${VERSION}"
    },
    "rmapd": {
      "path": "${INSTALL_DIR}/rmapd",
      "version": "${VERSION}"
    }
  },
  "directories": {
    "config": "${CONFIG_DIR}",
    "data": "${DATA_DIR}",
    "logs": "${LOG_DIR}"
  }
}
EOF
}

# ─────────────────────────────────────────────────────────────────────────────
# Argument Parsing
# ─────────────────────────────────────────────────────────────────────────────

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version)
                VERSION="$2"
                shift 2
                ;;
            --binary-only)
                BINARY_ONLY=true
                shift
                ;;
            --non-interactive)
                NON_INTERACTIVE=true
                shift
                ;;
            --source)
                BUILD_FROM_SOURCE=true
                shift
                ;;
            --integrate)
                INTEGRATE_HOSTS="$2"
                shift 2
                ;;
            --help|-h)
                echo "repo-graph installer"
                echo ""
                echo "Usage: install.sh [options]"
                echo ""
                echo "Options:"
                echo "  --version <ver>       Install specific version (default: latest)"
                echo "  --binary-only         Skip daemon service and integrations"
                echo "  --non-interactive     Non-interactive mode (no prompts)"
                echo "  --source              Build from source (requires Rust)"
                echo "  --integrate <hosts>   Comma-separated hosts (claude-code,codex)"
                echo "  --help                Show this help"
                echo ""
                echo "Environment variables:"
                echo "  RMAP_VERSION          Install specific version"
                echo "  RMAP_INSTALL_DIR      Install directory (default: ~/.local/bin)"
                echo "  RMAP_BINARY_ONLY      Set to 1 for binary-only install"
                echo "  RMAP_NON_INTERACTIVE  Set to 1 for non-interactive mode"
                exit 0
                ;;
            *)
                error "Unknown option: $1"
                ;;
        esac
    done

    # Apply environment variables
    VERSION="${VERSION:-${RMAP_VERSION:-latest}}"
    INSTALL_DIR="${RMAP_INSTALL_DIR:-${DEFAULT_INSTALL_DIR}}"

    if [[ "${RMAP_BINARY_ONLY:-}" == "1" ]]; then
        BINARY_ONLY=true
    fi

    if [[ "${RMAP_NON_INTERACTIVE:-}" == "1" ]]; then
        NON_INTERACTIVE=true
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────────────

main() {
    echo ""
    echo "repo-graph installer"
    echo "===================="
    echo ""

    parse_args "$@"

    detect_platform
    detect_toolchains
    resolve_version

    echo ""

    if [[ "${BUILD_FROM_SOURCE}" == "true" ]]; then
        build_from_source
    else
        download_binary
    fi

    if [[ "${BINARY_ONLY}" != "true" ]]; then
        create_directories
        write_manifest
        setup_daemon_service
        detect_hosts
    fi

    setup_path

    echo ""
    echo "===================="
    echo "Installation complete"
    echo ""
    echo "  CLI:     ${INSTALL_DIR}/rmap"
    echo "  Daemon:  ${INSTALL_DIR}/rmapd"
    echo "  Version: ${VERSION}"
    echo ""
    echo "Quick start:"
    echo "  rmap --help          # Show available commands"
    echo "  rmapd                # Run daemon"
    echo "  rmap index <repo>    # Index a repository"
    echo ""

    if [[ "${BINARY_ONLY}" != "true" ]]; then
        echo "Configuration: ${CONFIG_DIR}"
        echo ""
    fi

    echo "Uninstall:"
    echo "  rm ${INSTALL_DIR}/rmap ${INSTALL_DIR}/rmapd"
    if [[ "${BINARY_ONLY}" != "true" ]]; then
        echo "  rm -rf ${CONFIG_DIR}"
        echo "  rm -rf ${DATA_DIR}"
    fi
    echo ""
}

main "$@"
