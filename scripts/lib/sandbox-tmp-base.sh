# shellcheck shell=bash
# Sandbox temp base for isolated state roots: sets RG_SANDBOX_TMP_BASE.
#
# The shell twin of repo_graph_platform_paths::sandbox_temp_base()
# (rust/crates/platform-paths/src/sandbox.rs), rule D-PTMP-ROOT-1 option A:
# one fixed directory per OS, the macOS private temp directory on Darwin and
# the system temp directory elsewhere. Set unconditionally, with no environment
# input (never the session's temp-directory variable). A state root under this
# base is classified SandboxLocal by the daemon.
#
# Sourced, not executed. Not an installer module (build-installer.sh injects
# lib/macos.sh and lib/linux.sh by name).
case "$(uname -s)" in Darwin) RG_SANDBOX_TMP_BASE=/private/tmp ;; *) RG_SANDBOX_TMP_BASE=/tmp ;; esac
