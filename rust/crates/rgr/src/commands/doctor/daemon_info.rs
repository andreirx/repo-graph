//! `daemon_info`-derived doctor probes.
//!
//! One `daemon_info` round-trip feeds three probes:
//! - `authority_policy` (STATE-ROOT-SEPARATION-1) — preserved byte-for-byte.
//! - `daemon_memory` (DOCTOR-RESOURCE-REPORT) — the daemon's own RSS.
//! - `total_storage` (DOCTOR-RESOURCE-REPORT) — summed `databases/` size across repos.
//!
//! **Abstraction note (per repo structural guardrail):** extracted from the parent
//! `doctor` module because the daemon-info parse/format grew that file past the
//! 500-line guardrail. One concrete current caller: [`super::execute_doctor`], via
//! [`probes`]. Axis of variation: none claimed — this is a cohesion/size split, not a
//! variation seam. Rejected alternative: leaving it inline in `doctor/mod.rs` (keeps
//! the file over the structural-guardrail limit, which the slice forbids).
//!
//! The resource probes ALWAYS pass: a diagnostic metric being unreadable must never
//! flip the `rmap doctor` health verdict. Only `authority_policy` carries a real
//! daemon-down failure.

use crate::daemon_client::DaemonClient;
use crate::platform::ProbeResult;

use super::format_size;

/// Query `daemon_info` ONCE and derive the daemon-info probes.
///
/// One round-trip feeds three probes: `authority_policy` (STATE-ROOT-SEPARATION-1)
/// plus `daemon_memory` + `total_storage` (DOCTOR-RESOURCE-REPORT). This replaces the
/// former single-purpose `state_root_mode_probe` — the authority-policy output is
/// preserved byte-for-byte; folding into one call avoids a second `daemon_info`
/// round-trip (and a second `databases/` walk) just to add the resource probes.
///
/// On daemon-unreachable the authority-policy probe FAILS (unchanged contract — the
/// daemon being down is a real fault), while the resource probes degrade to a PASSING
/// "unavailable": a diagnostic metric must never flip the `healthy` verdict.
pub(super) fn probes() -> Vec<ProbeResult> {
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => return unreachable_probes("daemon unavailable", format!("{}", e)),
    };

    match client.request("daemon_info", None) {
        Ok(response) => probes_from_response(&response),
        Err(e) => unreachable_probes("query failed", format!("{}", e)),
    }
}

/// Degraded probe set when `daemon_info` cannot be reached.
///
/// `authority_policy` FAILS (preserves the pre-existing daemon-down contract, so the
/// health verdict still flips); `daemon_memory` + `total_storage` degrade to a PASSING
/// "unavailable" (resource diagnostics never flip `healthy`).
fn unreachable_probes(authority_msg: &str, detail: String) -> Vec<ProbeResult> {
    vec![
        ProbeResult {
            name: "authority_policy".to_string(),
            passed: false,
            message: authority_msg.to_string(),
            details: Some(detail),
        },
        ProbeResult {
            name: "daemon_memory".to_string(),
            passed: true,
            message: "unavailable".to_string(),
            details: Some("daemon unreachable".to_string()),
        },
        ProbeResult {
            name: "total_storage".to_string(),
            passed: true,
            message: "unavailable".to_string(),
            details: Some("daemon unreachable".to_string()),
        },
    ]
}

/// Build the daemon-info-derived probes from a successful `daemon_info` response.
///
/// Pure (no I/O) — this is the parse/format seam the doctor probe tests target.
/// Produces, in order:
/// - `authority_policy` (STATE-ROOT-SEPARATION-1) — preserved exactly.
/// - `daemon_memory` (DOCTOR-RESOURCE-REPORT) — daemon RSS: current live footprint,
///   with the peak high-water mark in parentheses. The headline "did the daemon
///   balloon?" line.
/// - `total_storage` (DOCTOR-RESOURCE-REPORT) — summed `databases/` size across N repos.
///
/// The resource probes ALWAYS pass: a missing/`null` metric renders "unavailable" and
/// must never flip `healthy`. `databases_total_bytes` distinguishes `null` (unknown)
/// from `0` (known-zero) — only `null` is "unavailable".
fn probes_from_response(response: &serde_json::Value) -> Vec<ProbeResult> {
    let mut probes = Vec::with_capacity(3);

    // authority_policy: byte-for-byte the former state_root_mode_probe output.
    let authority_writes = response
        .get("authority_writes_allowed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    probes.push(if authority_writes {
        ProbeResult {
            name: "authority_policy".to_string(),
            passed: true,
            message: "baselines, aliases, declarations: allowed".to_string(),
            details: None,
        }
    } else {
        ProbeResult {
            name: "authority_policy".to_string(),
            passed: true, // Not a failure - sandbox mode is valid operation
            message: "baselines, aliases, declarations: blocked (sandbox mode)".to_string(),
            details: Some("authority writes require socket daemon".to_string()),
        }
    });

    // daemon_memory: current RSS primary (live footprint), peak in parentheses.
    let current = response.get("rss_bytes").and_then(|v| v.as_u64());
    let peak = response.get("rss_peak_bytes").and_then(|v| v.as_u64());
    let memory_message = match (current, peak) {
        (Some(c), Some(p)) => format!("{} (peak {})", format_size(c as i64), format_size(p as i64)),
        (Some(c), None) => format_size(c as i64),
        (None, Some(p)) => format!("unavailable (peak {})", format_size(p as i64)),
        (None, None) => "unavailable".to_string(),
    };
    probes.push(ProbeResult {
        name: "daemon_memory".to_string(),
        passed: true,
        message: memory_message,
        details: None,
    });

    // total_storage: summed databases/ size across all repos.
    let total = response
        .get("databases_total_bytes")
        .and_then(|v| v.as_u64());
    let repo_count = response
        .get("repo_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let repo_word = if repo_count == 1 { "repo" } else { "repos" };
    let storage_message = match total {
        Some(bytes) => format!(
            "{} across {} {}",
            format_size(bytes as i64),
            repo_count,
            repo_word
        ),
        None => format!("unavailable ({} {})", repo_count, repo_word),
    };
    probes.push(ProbeResult {
        name: "total_storage".to_string(),
        passed: true,
        message: storage_message,
        details: None,
    });

    probes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn find<'a>(probes: &'a [ProbeResult], name: &str) -> &'a ProbeResult {
        probes
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("probe '{}' present", name))
    }

    // DOCTOR-RESOURCE-REPORT: the doctor probe parses + formats the daemon_info
    // resource fields into the human "Resources" lines.
    #[test]
    fn parses_and_formats_real_resource_numbers() {
        let response = json!({
            "authority_writes_allowed": true,
            "rss_bytes": 47_u64 * 1024 * 1024,       // 47 MiB current
            "rss_peak_bytes": 78_u64 * 1024 * 1024,  // 78 MiB peak
            "databases_total_bytes": 1_610_612_736_u64, // 1.5 GiB
            "repo_count": 3
        });

        let probes = probes_from_response(&response);

        let memory = find(&probes, "daemon_memory");
        assert!(memory.passed);
        assert_eq!(memory.message, "47.0 MB (peak 78.0 MB)");

        let storage = find(&probes, "total_storage");
        assert!(storage.passed);
        assert_eq!(storage.message, "1.50 GB across 3 repos");

        // authority_policy preserved exactly.
        let auth = find(&probes, "authority_policy");
        assert!(auth.passed);
        assert_eq!(auth.message, "baselines, aliases, declarations: allowed");
    }

    // DOCTOR-RESOURCE-REPORT graceful degradation: a `null` (unavailable) metric must
    // render "unavailable" AND keep the probe passing — `healthy` must not flip.
    #[test]
    fn unavailable_metrics_degrade_to_passing() {
        let response = json!({
            "authority_writes_allowed": false,
            "rss_bytes": serde_json::Value::Null,
            "rss_peak_bytes": serde_json::Value::Null,
            "databases_total_bytes": serde_json::Value::Null,
            "repo_count": 2
        });

        let probes = probes_from_response(&response);

        let memory = find(&probes, "daemon_memory");
        assert!(memory.passed, "unreadable metric must NOT flip healthy");
        assert_eq!(memory.message, "unavailable");

        let storage = find(&probes, "total_storage");
        assert!(storage.passed, "unreadable metric must NOT flip healthy");
        assert_eq!(storage.message, "unavailable (2 repos)");

        // No resource probe failed → it cannot drag `healthy` to false.
        assert!(probes
            .iter()
            .filter(|p| matches!(p.name.as_str(), "daemon_memory" | "total_storage"))
            .all(|p| p.passed));
    }

    // current unavailable but peak present → still surface the peak; and a real 0-byte
    // databases dir is known-zero, NOT "unavailable".
    #[test]
    fn peak_only_and_known_zero_storage() {
        let response = json!({
            "authority_writes_allowed": true,
            "rss_bytes": serde_json::Value::Null,
            "rss_peak_bytes": 78_u64 * 1024 * 1024,
            "databases_total_bytes": 0,
            "repo_count": 1
        });

        let probes = probes_from_response(&response);
        assert_eq!(
            find(&probes, "daemon_memory").message,
            "unavailable (peak 78.0 MB)"
        );
        assert_eq!(find(&probes, "total_storage").message, "0 B across 1 repo");
    }

    // Daemon down: authority_policy fails (real fault) but resources stay passing.
    #[test]
    fn unreachable_daemon_keeps_resources_passing() {
        let probes = unreachable_probes("daemon unavailable", "boom".to_string());
        assert!(!find(&probes, "authority_policy").passed);
        assert!(find(&probes, "daemon_memory").passed);
        assert!(find(&probes, "total_storage").passed);
        assert_eq!(find(&probes, "daemon_memory").message, "unavailable");
    }
}
