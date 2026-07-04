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
/// health verdict still flips); `daemon_memory` + `total_storage` + `activity` degrade to a
/// PASSING "unavailable" (diagnostics never flip `healthy`).
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
        ProbeResult {
            name: "activity".to_string(),
            passed: true,
            message: "unavailable".to_string(),
            details: Some("daemon unreachable".to_string()),
        },
    ]
}

/// Humanise a large count for the activity line (42000 → "42k", 1_600_000 → "1.6M").
fn humanize_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

/// Humanise an elapsed duration for "started N ago".
fn humanize_secs_ago(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h{}m ago", secs / 3600, (secs % 3600) / 60)
    }
}

/// DAEMON-VISIBILITY-1 (D): the daemon's current activity line for `rmap doctor`.
///
/// Renders the daemon's in-flight write op(s) from `daemon_info.active_operations`:
/// "indexing <repo>: <phase> 42k/160k files, started 6m ago", or "idle" when nothing is
/// running. ALWAYS passes (activity is informational — it never flips the health verdict).
fn activity_probe(response: &serde_json::Value) -> ProbeResult {
    let ops = response
        .get("active_operations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if ops.is_empty() {
        // DAEMON-VISIBILITY-1 (D2): idle daemon reports "idle; last snapshot <repo> @ <time>" so the
        // reader who "indexed 15 minutes ago" sees completion is observable — NOT a bare "idle" that
        // reads like "nothing ever happened". `last_snapshot` is null (bare "idle") only when no repo
        // has ever completed an index.
        let message = match response.get("last_snapshot") {
            Some(ls) if ls.is_object() => {
                let repo = ls.get("repo").and_then(|v| v.as_str()).unwrap_or("<repo>");
                let at = ls
                    .get("at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown time");
                format!("idle; last snapshot {repo} @ {at}")
            }
            _ => "idle".to_string(),
        };
        return ProbeResult {
            name: "activity".to_string(),
            passed: true,
            message,
            details: None,
        };
    }

    let render_op = |op: &serde_json::Value| -> String {
        // Reader-frame verb + repo. `kind` is a machine token; map it to a gerund.
        let verb = match op.get("kind").and_then(|v| v.as_str()) {
            Some("index") => "indexing",
            Some("refresh") => "refreshing",
            Some("enrich") => "enriching",
            _ => "working on",
        };
        let repo = op.get("repo").and_then(|v| v.as_str()).unwrap_or("<repo>");
        let ago = op
            .get("started_secs_ago")
            .and_then(|v| v.as_u64())
            .map(humanize_secs_ago)
            .unwrap_or_else(|| "just now".to_string());

        // Phase + counters: "extraction 42k/160k files". `total == 0` = unknown denominator.
        let phase = op.get("phase").and_then(|v| v.as_str());
        let current = op.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = op.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let progress = match (phase, total) {
            (Some(ph), t) if t > 0 => {
                format!(
                    "{ph} {}/{} files, ",
                    humanize_count(current),
                    humanize_count(t)
                )
            }
            (Some(ph), 0) if current > 0 => format!("{ph} {} files, ", humanize_count(current)),
            (Some(ph), _) => format!("{ph}, "),
            (None, _) => String::new(),
        };
        format!("{verb} {repo}: {progress}started {ago}")
    };

    let mut message = render_op(&ops[0]);
    if ops.len() > 1 {
        message.push_str(&format!(" (+{} more)", ops.len() - 1));
    }

    ProbeResult {
        name: "activity".to_string(),
        passed: true,
        message,
        details: None,
    }
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

    // DAEMON-VISIBILITY-1 (D): what the daemon is doing right now (idle / indexing <repo> …).
    probes.push(activity_probe(response));

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

    // DAEMON-VISIBILITY-1 (D): idle daemon with NO prior index → bare "idle" (last_snapshot null).
    #[test]
    fn activity_probe_idle_when_no_ops() {
        let response = json!({ "active_operations": [] });
        let probe = activity_probe(&response);
        assert!(probe.passed);
        assert_eq!(probe.message, "idle");
    }

    // DAEMON-VISIBILITY-1 (D2): idle daemon WITH a completed index → "idle; last snapshot <repo> @
    // <time>". This is the completion-observable fact the day-2 reader (who "indexed 15 minutes ago")
    // needs — a bare "idle" reads like "nothing ever happened".
    #[test]
    fn activity_probe_idle_reports_last_snapshot() {
        let response = json!({
            "active_operations": [],
            "last_snapshot": { "repo": "my-repo", "at": "2026-07-04T09:15:00.000Z" }
        });
        let probe = activity_probe(&response);
        assert!(probe.passed, "activity never flips health");
        assert_eq!(
            probe.message, "idle; last snapshot my-repo @ 2026-07-04T09:15:00.000Z",
            "idle must name the last snapshot's repo + time: {}",
            probe.message
        );
    }

    // Idle with a `null` last_snapshot (field present but null) still degrades to the bare "idle".
    #[test]
    fn activity_probe_idle_null_last_snapshot_is_bare_idle() {
        let response = json!({ "active_operations": [], "last_snapshot": serde_json::Value::Null });
        let probe = activity_probe(&response);
        assert_eq!(probe.message, "idle");
    }

    // DAEMON-VISIBILITY-1 (D): an in-flight index renders "indexing <repo>: <phase> N/M files,
    // started …" — the headline `rmap doctor` activity line.
    #[test]
    fn activity_probe_renders_in_flight_index() {
        let response = json!({
            "active_operations": [
                { "kind": "index", "repo": "/repos/big", "phase": "extracting",
                  "current": 42_000, "total": 160_000, "started_secs_ago": 372 }
            ]
        });
        let probe = activity_probe(&response);
        assert!(probe.passed, "activity never flips health");
        assert!(
            probe.message.contains("indexing /repos/big"),
            "{}",
            probe.message
        );
        assert!(
            probe.message.contains("extracting 42k/160k files"),
            "{}",
            probe.message
        );
        assert!(
            probe.message.contains("started 6m ago"),
            "{}",
            probe.message
        );
    }

    #[test]
    fn humanizers_are_coarse() {
        assert_eq!(humanize_count(42_000), "42k");
        assert_eq!(humanize_count(1_600_000), "1.6M");
        assert_eq!(humanize_count(500), "500");
        assert_eq!(humanize_secs_ago(45), "45s ago");
        assert_eq!(humanize_secs_ago(372), "6m ago");
    }
}
