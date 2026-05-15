//! Fallback policy for daemon unavailability.
//!
//! This module defines which CLI operations are allowed when the daemon
//! is unavailable. The policy is:
//!
//! - **Read-only operations**: May proceed with direct DB access
//! - **Daemon-required operations**: Must fail with actionable error
//!
//! ## Enumerated Read-Only Operations (per RMAPD-2 slice)
//!
//! | Command | Reason |
//! |---------|--------|
//! | `--version` | Static binary metadata |
//! | `--help` | Static help text |
//! | `doctor` (partial) | File/path checks only |
//! | `repo list` | Read-only DB query |
//! | `graph <query>` | Read-only DB query |
//! | `boundaries list` | Read-only DB query |
//! | `inferences list` | Read-only DB query |
//! | `resource list` | Read-only DB query |
//!
//! ## Daemon-Required Operations
//!
//! | Command | Reason |
//! |---------|--------|
//! | `repo add` | DB mutation |
//! | `repo remove` | DB mutation |
//! | `refresh` | Coordination, DB mutation |
//! | `index` | Coordination, DB mutation |
//! | `hook *` | Daemon state dependency |
//!
//! ## Design Principle
//!
//! The daemon owns write coordination. The CLI must NOT silently bypass
//! daemon authority for operations that mutate state or depend on
//! daemon-held coordination.

use std::path::Path;

/// Classification of a CLI operation for fallback purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationClass {
    /// Static operation that never needs daemon (--version, --help).
    Static,

    /// Read-only operation that can fall back to direct DB access.
    ReadOnly,

    /// Operation that requires daemon coordination.
    DaemonRequired,
}

/// Classify a CLI operation by its command path.
///
/// The command path is the sequence of subcommands, e.g., ["repo", "list"]
/// or ["index"].
pub fn classify_operation(command_path: &[&str]) -> OperationClass {
    match command_path {
        // Static operations (no daemon needed ever)
        [] => OperationClass::Static, // --version, --help at root
        ["version"] => OperationClass::Static,
        ["help"] => OperationClass::Static,

        // Read-only operations (can fall back to direct DB)
        ["repo", "list"] => OperationClass::ReadOnly,
        ["graph", ..] => OperationClass::ReadOnly,
        ["boundaries", "list"] => OperationClass::ReadOnly,
        ["boundaries", "links"] => OperationClass::ReadOnly,
        ["inferences", "list"] => OperationClass::ReadOnly,
        ["resource", "list"] => OperationClass::ReadOnly,
        ["stats", ..] => OperationClass::ReadOnly,
        ["modules", "list"] => OperationClass::ReadOnly,
        ["modules", "files"] => OperationClass::ReadOnly,
        ["modules", "show"] => OperationClass::ReadOnly,
        ["callers", ..] => OperationClass::ReadOnly,
        ["callees", ..] => OperationClass::ReadOnly,
        ["imports", ..] => OperationClass::ReadOnly,
        ["deps", ..] => OperationClass::ReadOnly,
        ["cycles", ..] => OperationClass::ReadOnly,
        ["dead", ..] => OperationClass::ReadOnly,
        ["hotspots", ..] => OperationClass::ReadOnly,
        ["path", ..] => OperationClass::ReadOnly,
        ["explain", ..] => OperationClass::ReadOnly,
        ["violations", ..] => OperationClass::ReadOnly,
        ["surfaces", ..] => OperationClass::ReadOnly,
        ["contracts", ..] => OperationClass::ReadOnly,
        ["risk", ..] => OperationClass::ReadOnly,
        ["churn", ..] => OperationClass::ReadOnly,
        ["metrics", ..] => OperationClass::ReadOnly,
        ["docs", ..] => OperationClass::ReadOnly,
        ["orient", ..] => OperationClass::ReadOnly,
        ["check", ..] => OperationClass::ReadOnly,
        ["gate", ..] => OperationClass::ReadOnly,
        ["assess", ..] => OperationClass::ReadOnly,

        // Doctor is special: partially works without daemon
        ["doctor", ..] => OperationClass::ReadOnly,

        // Daemon-required operations (mutations, coordination)
        ["repo", "add"] => OperationClass::DaemonRequired,
        ["repo", "remove"] => OperationClass::DaemonRequired,
        ["index", ..] => OperationClass::DaemonRequired,
        ["refresh", ..] => OperationClass::DaemonRequired,
        ["enrich", ..] => OperationClass::DaemonRequired,

        // All hook commands require daemon
        ["hook", ..] => OperationClass::DaemonRequired,

        // All integrate commands require daemon
        ["integrate", ..] => OperationClass::DaemonRequired,

        // Declare commands modify state
        ["declare", ..] => OperationClass::DaemonRequired,

        // Uninstall should work without daemon (cleanup operation)
        ["uninstall", ..] => OperationClass::ReadOnly,

        // Unknown commands default to daemon-required (safe default)
        _ => OperationClass::DaemonRequired,
    }
}

/// Generate an actionable error message for daemon-required operations.
///
/// The message includes platform-specific instructions for starting the daemon.
pub fn daemon_unavailable_message(socket_path: &Path, operation: &str) -> String {
    let platform_hint = if cfg!(target_os = "macos") {
        "Start with: launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.repo-graph.rmapd.plist"
    } else {
        "Start with: systemctl --user start rmapd"
    };

    format!(
        "Daemon unavailable for '{}'\n\n\
         The daemon is required for this operation but is not running.\n\
         Socket: {}\n\n\
         {}",
        operation,
        socket_path.display(),
        platform_hint
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_static_operations() {
        assert_eq!(classify_operation(&[]), OperationClass::Static);
        assert_eq!(classify_operation(&["version"]), OperationClass::Static);
        assert_eq!(classify_operation(&["help"]), OperationClass::Static);
    }

    #[test]
    fn classify_read_only_operations() {
        assert_eq!(
            classify_operation(&["repo", "list"]),
            OperationClass::ReadOnly
        );
        assert_eq!(
            classify_operation(&["graph", "query"]),
            OperationClass::ReadOnly
        );
        assert_eq!(
            classify_operation(&["boundaries", "list"]),
            OperationClass::ReadOnly
        );
        assert_eq!(
            classify_operation(&["inferences", "list"]),
            OperationClass::ReadOnly
        );
        assert_eq!(
            classify_operation(&["resource", "list"]),
            OperationClass::ReadOnly
        );
        assert_eq!(classify_operation(&["doctor"]), OperationClass::ReadOnly);
        assert_eq!(
            classify_operation(&["modules", "list"]),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn classify_daemon_required_operations() {
        assert_eq!(
            classify_operation(&["repo", "add"]),
            OperationClass::DaemonRequired
        );
        assert_eq!(
            classify_operation(&["repo", "remove"]),
            OperationClass::DaemonRequired
        );
        assert_eq!(
            classify_operation(&["index"]),
            OperationClass::DaemonRequired
        );
        assert_eq!(
            classify_operation(&["refresh"]),
            OperationClass::DaemonRequired
        );
        assert_eq!(
            classify_operation(&["hook", "session-start"]),
            OperationClass::DaemonRequired
        );
        assert_eq!(
            classify_operation(&["integrate", "claude-code"]),
            OperationClass::DaemonRequired
        );
    }

    #[test]
    fn unknown_commands_default_to_daemon_required() {
        // Unknown commands should be daemon-required as safe default
        assert_eq!(
            classify_operation(&["unknown", "command"]),
            OperationClass::DaemonRequired
        );
    }

    #[test]
    fn daemon_unavailable_message_includes_socket_path() {
        let msg = daemon_unavailable_message(Path::new("/tmp/test.sock"), "rmap index");
        assert!(msg.contains("/tmp/test.sock"));
        assert!(msg.contains("rmap index"));
    }
}
