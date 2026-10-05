//! `doctor` probes derived from the client CONFIGURATION alone — no connection is made
//! (DOCTOR-FALLBACK-STATE-ROOT-1, RG-REQ-011-L12).
//!
//! **Abstraction note (per repo structural guardrail):** a child module of `platform` because
//! `platform/mod.rs` is over the 500-line guardrail (CLAUDE.md:87; D-DFSR-PATHS-1 option A).
//! Concrete current users: `commands::doctor::execute_doctor` (the early probes, the note
//! predicate for the service-probe tone) and the macOS adapter's `doctor_probes` (the service-note
//! detail). Rejected alternative: inline in `platform/mod.rs` / `commands/doctor/mod.rs` — both are
//! over the guardrail.
//!
//! The configuration is read with the client's OWN predicates, so the probe and the client cannot
//! disagree: the transport with `TransportMode::from_env()` (never through a `DaemonClient`, whose
//! construction can fail), the state root with `std::env::var("RMAP_STATE_ROOT")` — the predicate
//! `daemon_client/mod.rs:154` and `stdio_transport.rs:179-180` apply (D-DFSR-NONUTF8-ROOT-1).

use std::env::VarError;
use std::path::PathBuf;

use crate::daemon_client::TransportMode;

use super::ProbeResult;

/// The state root the client is CONFIGURED with, read before any connection.
///
/// A sandbox-local root is only known after an `auto` permission fallback, so it is not a state
/// of this type (D-DFSR-FALLBACK-NOTE-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RootConfig {
    /// `RMAP_STATE_ROOT` is absent.
    Global,
    /// `RMAP_STATE_ROOT` is present and read as valid UTF-8.
    Override(PathBuf),
    /// `RMAP_STATE_ROOT` is present but is not valid UTF-8; the client ignores it.
    InvalidUtf8,
}

/// The client configuration `doctor` reports before any connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorConfig {
    pub transport: TransportMode,
    pub root: RootConfig,
}

/// THE ONE configuration reader: the configured transport and the configured state root.
pub(crate) fn doctor_config() -> DoctorConfig {
    let root = match std::env::var("RMAP_STATE_ROOT") {
        Ok(p) => RootConfig::Override(PathBuf::from(p)),
        Err(VarError::NotPresent) => RootConfig::Global,
        Err(VarError::NotUnicode(_)) => RootConfig::InvalidUtf8,
    };
    DoctorConfig {
        transport: TransportMode::from_env(),
        root,
    }
}

/// The `transport` and `state_root` probes that `doctor` emits FIRST under a forced stdio
/// transport. Empty under `auto`/`socket`: there the two probes stay after the connection attempt.
pub(crate) fn early_config_probes(cfg: &DoctorConfig) -> Vec<ProbeResult> {
    if cfg.transport != TransportMode::Stdio {
        return Vec::new();
    }
    // The configured mode only: no `active:` claim exists before a connection, and the
    // permission-denied cause belongs to an observed `auto` fallback, not to forced stdio.
    let transport = ProbeResult::pass("transport", "stdio (configured)");
    let state_root = match &cfg.root {
        RootConfig::Override(p) => {
            ProbeResult::pass("state_root", format!("override ({})", p.display()))
        }
        RootConfig::Global => ProbeResult::pass("state_root", "global"),
        RootConfig::InvalidUtf8 => ProbeResult::fail(
            "state_root",
            "RMAP_STATE_ROOT is set but is not valid UTF-8; the client ignores it (global root, or a sandbox-local root after an auto fallback)",
        ),
    };
    vec![transport, state_root]
}

/// THE ONE note predicate: the global service probe is a passing note when the transport is
/// forced stdio, or the state root is a configured override (a valid UTF-8 `RMAP_STATE_ROOT`).
pub(crate) fn service_note_applies(cfg: &DoctorConfig) -> bool {
    cfg.transport == TransportMode::Stdio || matches!(cfg.root, RootConfig::Override(_))
}

/// The detail the global service note carries: only what the configuration proves.
#[cfg(target_os = "macos")]
pub(crate) fn service_note_detail(cfg: &DoctorConfig) -> Option<String> {
    if cfg.transport == TransportMode::Stdio {
        // A configuration fact: true whether or not the stdio daemon spawn succeeds
        // (D-DFSR-STDIO-DETAIL-1).
        return Some(
            "stdio transport configured; this service is not used by a stdio client".to_string(),
        );
    }
    match &cfg.root {
        // No identity claim: with `RMAP_STATE_ROOT` alone the client may still reach the global
        // socket (D-DFSR-SCOPE-1 Correction 2).
        RootConfig::Override(p) => Some(format!(
            "measures the global launchd service; this run's configured state root is {}",
            p.display()
        )),
        RootConfig::Global | RootConfig::InvalidUtf8 => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        check_daemon_socket_with, granular_socket_probes_with, socket_resolution_probes_with,
    };
    use super::*;
    use crate::cli::paths::{PathResolutionDiagnostics, ResolutionReason};

    /// A path whose parent directory does not exist, so the socket is missing.
    fn missing_socket() -> PathBuf {
        PathBuf::from("/nonexistent-DOCTOR-FALLBACK-STATE-ROOT-1/rmap.sock")
    }

    /// Resolution diagnostics for an `RMAP_SOCKET_PATH` override that points at a missing socket —
    /// the isolated-root configuration — built as a value (no environment read).
    fn override_diag(path: PathBuf) -> PathResolutionDiagnostics {
        PathResolutionDiagnostics {
            effective_uid: 501,
            env_home: Some("/Users/someone".to_string()),
            canonical_home: Some(PathBuf::from("/Users/someone")),
            override_active: true,
            override_path: Some(path.clone()),
            canonical_socket_path: Some(PathBuf::from(
                "/Users/someone/Library/Application Support/repo-graph/daemon.sock",
            )),
            legacy_socket_path: None,
            chosen_path: Some(path),
            resolution_reason: ResolutionReason::Override,
        }
    }

    fn cfg(transport: TransportMode, root: RootConfig) -> DoctorConfig {
        DoctorConfig { transport, root }
    }

    fn all_socket_probes(
        mode: TransportMode,
        diag: &PathResolutionDiagnostics,
    ) -> Vec<ProbeResult> {
        let mut probes = vec![check_daemon_socket_with(mode, diag)];
        probes.extend(socket_resolution_probes_with(mode, diag));
        probes.extend(granular_socket_probes_with(mode, diag));
        probes
    }

    fn find<'a>(probes: &'a [ProbeResult], name: &str) -> &'a ProbeResult {
        probes
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("probe {name} missing: {probes:?}"))
    }

    const NON_UTF8_TEXT: &str = "RMAP_STATE_ROOT is set but is not valid UTF-8; the client ignores it (global root, or a sandbox-local root after an auto fallback)";

    // RG-REQ-011-L12: under FORCED stdio, `transport: stdio (configured)` then `state_root` come
    // first, from the configuration alone; the three `state_root` forms (override, global, the
    // failed non-UTF-8 probe); under auto/socket nothing early; the non-UTF-8 value is not an
    // override, so it yields no service note.
    #[test]
    fn doctor_emits_transport_and_state_root_before_socket_probes() {
        let root = PathBuf::from("/private/tmp/iso-root/state");

        let probes = early_config_probes(&cfg(TransportMode::Stdio, RootConfig::Override(root)));
        assert_eq!(probes.len(), 2, "{probes:?}");
        assert_eq!(probes[0].name, "transport");
        assert_eq!(probes[0].message, "stdio (configured)");
        assert!(probes[0].passed);
        assert!(
            probes[0].details.is_none(),
            "no `active:` claim and no permission-denied cause before a connection: {probes:?}"
        );
        assert!(!probes[0].message.contains("active:"));
        assert_eq!(probes[1].name, "state_root");
        assert_eq!(probes[1].message, "override (/private/tmp/iso-root/state)");
        assert!(probes[1].passed);

        let probes = early_config_probes(&cfg(TransportMode::Stdio, RootConfig::Global));
        assert_eq!(probes.len(), 2, "{probes:?}");
        assert_eq!(probes[0].message, "stdio (configured)");
        assert_eq!(probes[1].name, "state_root");
        assert_eq!(probes[1].message, "global");
        assert!(probes[1].passed);

        let probes = early_config_probes(&cfg(TransportMode::Stdio, RootConfig::InvalidUtf8));
        assert_eq!(probes.len(), 2, "{probes:?}");
        assert_eq!(probes[0].message, "stdio (configured)");
        assert_eq!(probes[1].name, "state_root");
        assert_eq!(probes[1].message, NON_UTF8_TEXT);
        assert!(!probes[1].passed, "a non-UTF-8 value is a counted failure");

        for transport in [TransportMode::Auto, TransportMode::Socket] {
            for root in [
                RootConfig::Global,
                RootConfig::Override(PathBuf::from("/private/tmp/iso-root/state")),
                RootConfig::InvalidUtf8,
            ] {
                assert!(
                    early_config_probes(&cfg(transport, root.clone())).is_empty(),
                    "{transport:?}/{root:?}: the two probes stay after the connection attempt"
                );
            }
            assert!(
                !service_note_applies(&cfg(transport, RootConfig::InvalidUtf8)),
                "{transport:?}: a non-UTF-8 value is not an override"
            );
            assert!(!service_note_applies(&cfg(transport, RootConfig::Global)));
            assert!(service_note_applies(&cfg(
                transport,
                RootConfig::Override(PathBuf::from("/private/tmp/iso-root/state"))
            )));
        }
        for root in [
            RootConfig::Global,
            RootConfig::Override(PathBuf::from("/private/tmp/iso-root/state")),
            RootConfig::InvalidUtf8,
        ] {
            assert!(service_note_applies(&cfg(TransportMode::Stdio, root)));
        }
    }

    // RG-REQ-011-L12: under forced stdio the five socket-related probes are `n/a (stdio transport)`
    // passes, with no crash diagnosis; the other resolution probes keep their facts; the
    // post-connection `transport`/`state_root` are not emitted a second time.
    #[test]
    fn doctor_stdio_transport_marks_socket_probes_not_applicable() {
        let diag = override_diag(missing_socket());
        let probes = all_socket_probes(TransportMode::Stdio, &diag);
        for name in [
            "daemon_socket",
            "socket_file",
            "socket_connect",
            "socket_ping",
            "socket_path",
        ] {
            let p = find(&probes, name);
            assert!(p.passed, "{name}: {p:?}");
            assert_eq!(p.message, "n/a (stdio transport)", "{name}");
            assert!(p.details.is_none(), "{name}: {p:?}");
        }
        for p in &probes {
            assert!(!p.message.contains("may have crashed"), "{p:?}");
            assert!(
                !p.details
                    .as_deref()
                    .unwrap_or("")
                    .contains("may have crashed"),
                "{p:?}"
            );
        }
        assert_eq!(find(&probes, "effective_uid").message, "501");
        assert_eq!(find(&probes, "env_home").message, "/Users/someone");
        assert_eq!(find(&probes, "canonical_home").message, "/Users/someone");
        assert_eq!(
            find(&probes, "socket_resolution").message,
            "RMAP_SOCKET_PATH override"
        );
        assert_eq!(
            find(&probes, "socket_override").message,
            missing_socket().display().to_string()
        );
        assert!(probes.iter().all(|p| p.passed), "{probes:?}");
        assert!(!probes
            .iter()
            .any(|p| p.name == "transport" || p.name == "state_root"));
        assert_eq!(probes.len(), 10, "{probes:?}");
    }

    // P-DFSR-02: under auto AND socket transport a missing socket keeps today's failures and texts.
    #[test]
    fn doctor_auto_and_socket_transport_keep_socket_failures() {
        let sock = missing_socket();
        let shown = sock.display().to_string();
        let diag = override_diag(sock);
        for mode in [TransportMode::Auto, TransportMode::Socket] {
            let probes = all_socket_probes(mode, &diag);
            let p = find(&probes, "daemon_socket");
            assert!(!p.passed, "{mode:?}");
            assert_eq!(p.message, format!("socket not found: {shown}"));
            let p = find(&probes, "socket_file");
            assert!(!p.passed, "{mode:?}");
            assert_eq!(p.message, format!("not found: {shown}"));
            for name in ["socket_connect", "socket_ping"] {
                let p = find(&probes, name);
                assert!(!p.passed, "{mode:?} {name}");
                assert_eq!(p.message, "skipped (socket missing)", "{mode:?} {name}");
            }
            let p = find(&probes, "socket_path");
            assert!(!p.passed, "{mode:?}");
            assert_eq!(p.message, format!("{shown} (not found)"));
            assert!(
                !probes
                    .iter()
                    .any(|p| p.message.contains("n/a (stdio transport)")),
                "{mode:?}: {probes:?}"
            );
        }
    }
}
