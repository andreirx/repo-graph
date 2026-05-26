//! Platform-native path resolution for repo-graph daemon infrastructure.
//!
//! This crate provides canonical path resolution for daemon socket, config,
//! data, and log directories. It is the single source of truth for path
//! resolution across both the CLI (`rgr`) and daemon (`rmapd`).
//!
//! # Design Principles
//!
//! 1. **Canonical home lookup**: Uses OS-level passwd entry (`getpwuid_r`) to
//!    determine the user's home directory, not `$HOME`. This ensures stable
//!    paths across sandboxed shells and agent environments.
//!
//! 2. **Override support**: `RMAP_SOCKET_PATH` env var overrides socket path
//!    for testing and emergency overrides only.
//!
//! 3. **Migration fallback**: During transition, probes legacy env-derived
//!    path when canonical path is not reachable. Uses socket reachability,
//!    not just file existence, to handle stale socket files.
//!
//! # Platform Paths
//!
//! | Platform | Config | Data | Logs | Socket |
//! |----------|--------|------|------|--------|
//! | macOS | ~/Library/Application Support/repo-graph | same | ~/Library/Logs/repo-graph | data/daemon.sock |
//! | Linux | ~/.config/rmap | ~/.local/share/rmap | data/logs | data/daemon.sock |
//!
//! # Why Not `dirs`?
//!
//! The `dirs` crate reads `$HOME`, which may be altered in sandboxed shells,
//! agent environments (Codex), or container views. For user preferences, that
//! is appropriate. For daemon socket rendezvous, it is not.
//!
//! A daemon socket is infrastructure identity, not session-local preference.
//! It must be findable regardless of session environment.
//!
//! # Module Structure
//!
//! - `home` — Canonical and legacy home directory resolution
//! - `dirs` — Data, config, logs, sessions, databases directories
//! - `socket` — Daemon socket path resolution with migration fallback

mod dirs;
mod home;
mod socket;

// Re-export from home module
pub use home::{canonical_home, effective_uid, legacy_home};

// Re-export from dirs module
pub use dirs::{
    config_dir, data_dir, databases_dir, ensure_dir, legacy_data_dir, logs_dir, sessions_dir,
    APP_NAME,
};

// Re-export from socket module
pub use socket::{
    daemon_socket_path, daemon_socket_path_with_diagnostics, is_using_legacy_fallback,
    legacy_fallback_warning, PathResolutionDiagnostics, ResolutionReason,
};
