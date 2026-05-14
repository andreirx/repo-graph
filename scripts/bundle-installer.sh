#!/bin/bash
# Bundle the modular installer into a single self-contained script.
#
# Usage: ./bundle-installer.sh > dist/install.sh
#
# This script is run during release to produce the public installer artifact.
# The bundled installer contains all platform-specific code inline, so it
# works with `curl | bash` without needing sibling files.
#
# See: docs/slices/dist-1-distribution-install-contract.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source files
MAIN_INSTALLER="${SCRIPT_DIR}/install.sh"
MACOS_LIB="${SCRIPT_DIR}/lib/macos.sh"
LINUX_LIB="${SCRIPT_DIR}/lib/linux.sh"

# Verify all source files exist
for f in "$MAIN_INSTALLER" "$MACOS_LIB" "$LINUX_LIB"; do
    if [[ ! -f "$f" ]]; then
        echo "Error: Required file not found: $f" >&2
        exit 1
    fi
done

# Output header
cat << 'HEADER'
#!/bin/bash
# repo-graph installer (bundled)
#
# GENERATED FILE - Do not edit directly.
# Source: scripts/bundle-installer.sh
#
# Usage:
#   curl -fsSL https://github.com/andreirx/repo-graph/releases/download/vX.Y.Z/install.sh | bash
#
# Or with specific version:
#   curl -fsSL https://github.com/andreirx/repo-graph/releases/download/v0.1.2/install.sh | bash
#
# Options (via environment variables):
#   RMAP_VERSION=X.Y.Z      # Override version (default: uses installer's embedded version)
#   RMAP_INSTALL_DIR=path   # Install directory (default: ~/.local/bin)
#   RMAP_BINARY_ONLY=1      # Skip daemon service and integrations
#   RMAP_NON_INTERACTIVE=1  # Non-interactive mode (no prompts)

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# Bundled Mode Marker
# ─────────────────────────────────────────────────────────────────────────────

# This installer is bundled - SCRIPT_DIR is intentionally empty.
# Platform libs and templates are embedded below.
SCRIPT_DIR=""

HEADER

# Extract and output macOS lib (skip shebang and initial comment block)
echo ""
echo "# ─────────────────────────────────────────────────────────────────────────────"
echo "# macOS Platform Functions (bundled from lib/macos.sh)"
echo "# ─────────────────────────────────────────────────────────────────────────────"
# Skip to first real section (Constants)
tail -n +10 "$MACOS_LIB"

# Extract and output Linux lib
echo ""
echo "# ─────────────────────────────────────────────────────────────────────────────"
echo "# Linux Platform Functions (bundled from lib/linux.sh)"
echo "# ─────────────────────────────────────────────────────────────────────────────"
# Skip to first real section (Constants)
tail -n +10 "$LINUX_LIB"

# Extract main installer (skip shebang, header, and SCRIPT_DIR detection since we set it above)
echo ""
echo "# ─────────────────────────────────────────────────────────────────────────────"
echo "# Main Installer (bundled from install.sh)"
echo "# ─────────────────────────────────────────────────────────────────────────────"
# Skip to Configuration section (line 36 onwards, past Script Location Detection)
tail -n +36 "$MAIN_INSTALLER"
