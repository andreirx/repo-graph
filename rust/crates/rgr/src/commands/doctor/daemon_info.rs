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
        ProbeResult {
            name: "retention".to_string(),
            passed: true,
            message: "unavailable".to_string(),
            details: Some("daemon unreachable".to_string()),
        },
        ProbeResult {
            name: "enrichment".to_string(),
            passed: true,
            message: "unavailable".to_string(),
            details: Some("daemon unreachable".to_string()),
        },
        ProbeResult {
            name: "orphan_storage".to_string(),
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
            Some("retention") => "reclaiming",
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

/// SNAPSHOT-RETENTION-1 (honesty surface): the last background retention pass outcome for `rmap
/// doctor`.
///
/// The auto retention pass is asynchronous, so the `rmap index` reply only says cleanup was "queued";
/// the pruned/reclaimed RESULT lands here (fed by `daemon_info.last_retention`). Renders, keyed on the
/// honest `vacuum_status`:
/// - "cleanup: pruned N snapshot(s), reclaimed X on disk, T ago"  (VACUUM ran)
/// - "cleanup: pruned N snapshot(s) (disk reclaim deferred — below threshold), T ago"  (recyclable)
/// - "cleanup: pruned N snapshot(s) (disk reclaim deferred — repo was being read), T ago"  (reader-safe)
/// - "cleanup: last pass had nothing to prune, T ago"
/// - "cleanup: none yet"  (no pass since daemon start).
///
/// The two "deferred" reasons are kept DISTINCT so the operator can tell a cheap-and-correct skip
/// (below threshold; pages recycle) from a reader-yield (a VACUUM the next pass will retry). ALWAYS
/// passes (informational — never flips `healthy`).
fn retention_probe(response: &serde_json::Value) -> ProbeResult {
    let message = match response.get("last_retention") {
        Some(lr) if lr.is_object() => {
            let pruned = lr.get("pruned_count").and_then(|v| v.as_i64()).unwrap_or(0);
            let non_ready = lr
                .get("non_ready_reclaimed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let removed = pruned + non_ready as i64;
            let reclaimed = lr
                .get("reclaimed_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            // `vacuum_status` is the authoritative fate; fall back to `reclaimed > 0` only for an older
            // daemon that predates the field (so the line stays honest across a version skew).
            let vacuum_status = lr.get("vacuum_status").and_then(|v| v.as_str());
            let ago = lr
                .get("finished_secs_ago")
                .and_then(|v| v.as_u64())
                .map(humanize_secs_ago)
                .unwrap_or_else(|| "recently".to_string());
            if removed == 0 {
                format!("cleanup: last pass had nothing to prune, {ago}")
            } else {
                let ran = matches!(vacuum_status, Some("ran"))
                    || (vacuum_status.is_none() && reclaimed > 0);
                if ran {
                    format!(
                        "cleanup: pruned {removed} snapshot(s), reclaimed {} on disk, {ago}",
                        format_size(reclaimed as i64)
                    )
                } else {
                    let reason = match vacuum_status {
                        Some("deferred_readers_active") => " — repo was being read",
                        Some("below_threshold") => " — below threshold",
                        _ => "",
                    };
                    format!(
                        "cleanup: pruned {removed} snapshot(s) (disk reclaim deferred{reason}), {ago}"
                    )
                }
            }
        }
        // Field present-but-null, or absent (older daemon) → no pass has completed yet.
        _ => "cleanup: none yet".to_string(),
    };
    ProbeResult {
        name: "retention".to_string(),
        passed: true,
        message,
        details: None,
    }
}

/// Render the per-language honest skips of a COMPLETED mixed run (slice §3.2) as reader-frame
/// "; skipped <lang>: <reason>" clauses — the `reason` already carries the install next-action
/// ("jdtls not found — set JDTLS_PATH to your jdtls launcher"). Empty when nothing was skipped.
///
/// This is what makes a MIXED run (e.g. Rust resolved, Java toolchain-absent) show WHY Java was
/// skipped and how to fix it on `rmap doctor` — not just the bare language name (review-0 item 2).
/// The all-skipped state (nothing ran) surfaces the same reasons via its own branch below.
fn skip_detail_clauses(skipped: Option<&Vec<serde_json::Value>>) -> String {
    match skipped {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(|s| {
                let lang = s.get("language").and_then(|v| v.as_str())?;
                let reason = s.get("reason").and_then(|v| v.as_str())?;
                Some(format!("; skipped {lang}: {reason}"))
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// ENRICH-YIELD-1: the " (top rejections: <label> N, <label> N)" clause appended to the doctor
/// enrichment headline, or empty when there is no funnel / nothing was rejected. Names the top TWO
/// reader-frame classes (the daemon already sorted `rejections` dominant-first). Reader labels only —
/// never the machine `reason`/`gate`, so the headline stays in the reader's language.
fn enrichment_funnel_headline(funnel: Option<&serde_json::Value>) -> String {
    let Some(rejections) = funnel
        .and_then(|f| f.get("rejections"))
        .and_then(|r| r.as_array())
    else {
        return String::new();
    };
    let parts: Vec<String> = rejections
        .iter()
        .take(2)
        .filter_map(|r| {
            let label = r.get("label").and_then(|v| v.as_str())?;
            let count = r.get("count").and_then(|v| v.as_u64())?;
            Some(format!("{label} {count}"))
        })
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!(" (top rejections: {})", parts.join(", "))
    }
}

/// ENRICH-YIELD-1 (review-2 item 3): the promotion clause of the doctor completion headline. States
/// the funnel's OWN denominator — the count of **resolved candidates** that reached the promotion
/// filter (`promotion_funnel.candidates`) — so "promoted P" is read against THAT population, not the
/// larger `enriched`/`eligible` counts (a different, upstream population: eligible unresolved edges →
/// resolved receiver types → the compiler-origin candidates that actually enter the filter). Numerator
/// and denominator both come from the funnel so the ratio is internally consistent. Mirrors
/// `daemon_runtime::enrich_pass::promoted_clause` (the oplog headline) so the two product surfaces
/// render the same relationship. Falls back to the bare "promoted P" (from `promoted_count`) when no
/// funnel carries a denominator — an older daemon's JSON, or a zero-candidate pass — an honest "no
/// denominator to show", never a fabricated one.
fn enrichment_promoted_clause(funnel: Option<&serde_json::Value>, promoted_count: u64) -> String {
    if let Some(f) = funnel {
        let candidates = f.get("candidates").and_then(|v| v.as_u64()).unwrap_or(0);
        if candidates > 0 {
            let promoted = f
                .get("promoted")
                .and_then(|v| v.as_u64())
                .unwrap_or(promoted_count);
            return format!("promoted {promoted}/{candidates} resolved candidates");
        }
    }
    format!("promoted {promoted_count}")
}

/// ENRICH-YIELD-1: the full promotion-funnel breakdown for the doctor `details` surface — the
/// "least-new-surface" queryable breakdown (reuses `ProbeResult.details`, already rendered; no new
/// command/flag). Renders TWO reader-frame views of the daemon's `promotion_funnel` JSON:
///
/// 1. the **per-gate waterfall** (`gates`, §2.1 requirement) in the filter's evaluation order — for
///    each gate a candidate reached, how many *reached* it and how many it *filtered out*; and
/// 2. the **dominant rejection reasons** (`rejections`, per class) — the retained per-class detail.
///
/// All labels are reader-frame (the daemon already put the "gate N" numbers only in machine fields).
/// `None` when there is no funnel / nothing to show, so a zero-work pass shows no phantom breakdown.
/// Lines join with the doctor detail indent convention (`\n        `).
fn enrichment_funnel_details(funnel: Option<&serde_json::Value>) -> Option<String> {
    let funnel = funnel?;
    let candidates = funnel
        .get("candidates")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let promoted = funnel.get("promoted").and_then(|v| v.as_u64()).unwrap_or(0);
    let rejected = funnel.get("rejected").and_then(|v| v.as_u64()).unwrap_or(0);

    let gates = funnel.get("gates").and_then(|g| g.as_array());
    let rejections = funnel.get("rejections").and_then(|r| r.as_array());

    let has_gates = gates.is_some_and(|g| !g.is_empty());
    let has_rejections = rejections.is_some_and(|r| !r.is_empty());
    if !has_gates && !has_rejections {
        return None; // nothing to show (zero-work / older daemon without a funnel)
    }

    let mut lines = vec![format!(
        "promotion funnel: {candidates} resolved candidates → {promoted} promoted, {rejected} rejected"
    )];

    // Per-gate waterfall (evaluation order), showing only the gates candidates actually reached.
    if let Some(gates) = gates {
        let mut waterfall = Vec::new();
        for g in gates {
            let (Some(label), Some(entered)) = (
                g.get("label").and_then(|v| v.as_str()),
                g.get("entered").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            if entered == 0 {
                continue; // the funnel already collapsed before this gate
            }
            let rej = g.get("rejected").and_then(|v| v.as_u64()).unwrap_or(0);
            if rej > 0 {
                let pct = (rej as f64 / entered as f64) * 100.0;
                waterfall.push(format!(
                    "  {label}: {entered} reached → {rej} filtered out here ({pct:.0}%)"
                ));
            } else {
                waterfall.push(format!(
                    "  {label}: {entered} reached → 0 filtered out here"
                ));
            }
        }
        if !waterfall.is_empty() {
            lines.push("gate-by-gate (in filter order):".to_string());
            lines.extend(waterfall);
        }
    }

    // Dominant rejection reasons (per class, dominant first) — the retained per-class detail, each as
    // a share of the resolved candidates (the 3.5%'s denominator).
    if let Some(rejections) = rejections.filter(|r| !r.is_empty()) {
        lines.push("top rejection reasons:".to_string());
        for r in rejections {
            let (Some(label), Some(count)) = (
                r.get("label").and_then(|v| v.as_str()),
                r.get("count").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            if candidates > 0 {
                let pct = (count as f64 / candidates as f64) * 100.0;
                lines.push(format!("  {label}: {count} ({pct:.0}% of resolved)"));
            } else {
                lines.push(format!("  {label}: {count}"));
            }
        }
    }

    Some(lines.join("\n        "))
}

/// ENRICH-LIFECYCLE-1 (D3): the enrichment lifecycle line for `rmap doctor`, from
/// `daemon_info.{last_enrichment, enrichment_enabled, enrichment_activity}`. The full lifecycle
/// across states (shown here as the DISPLAYED doctor line):
/// - "enrichment: disabled (RMAP_AUTO_ENRICH)"  (opted out)
/// - "enrichment: queued — a background pass is scheduled"  (spawned, not yet holding the write lock)
/// - "enrichment: running — resolving receiver types now"  (first pass, before it records)
/// - "enrichment: resolved N/M receiver types, promoted P, T ago"  (completed;
///   "; skipped <lang>: <reason>" per toolchain-absent language on a mixed run)
/// - "enrichment: skipped — <reason>, T ago"  (every eligible language had no toolchain)
/// - "enrichment: none yet — runs after the next index"  (no pass, none in flight)
///
/// The `"enrichment: "` prefix on each displayed line is supplied by the doctor renderer's probe
/// LABEL (`[ok] enrichment: …`, `print_probe_labeled` keyed on `name`). This probe's `message`
/// therefore carries only the state text (e.g. "disabled (RMAP_AUTO_ENRICH)"), NOT the label — if
/// the message repeated its own name the line would render "enrichment: enrichment: …". (The
/// retention probe follows the same rule: its message leads with "cleanup:", a different word.)
///
/// `enrichment_activity` ("idle"|"queued"|"running") is what lets this line tell "queued" from the
/// false "none yet — runs after the next index" (review-0 item 1): a pass IS in flight, so "none yet"
/// would be a lie. A running pass is ALSO shown by the `activity` line ("enriching <repo>"); "queued"
/// is not, so a queued pass behind a last-completed one is surfaced here too. ALWAYS passes
/// (informational — never flips `healthy`); a missing toolchain is an honest skip, not a health failure.
fn enrichment_probe(response: &serde_json::Value) -> ProbeResult {
    // Disabled wins: nothing runs, so no last-pass line would be honest.
    let enabled = response
        .get("enrichment_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    // The live lifecycle activity (slice §3.7). Absent (older daemon) → "idle", preserving the
    // pre-`enrichment_activity` rendering byte-for-byte.
    let activity = response
        .get("enrichment_activity")
        .and_then(|v| v.as_str())
        .unwrap_or("idle");
    let message = if !enabled {
        "disabled (RMAP_AUTO_ENRICH)".to_string()
    } else {
        match response.get("last_enrichment") {
            Some(le) if le.is_object() => {
                let ago = le
                    .get("finished_secs_ago")
                    .and_then(|v| v.as_u64())
                    .map(humanize_secs_ago)
                    .unwrap_or_else(|| "recently".to_string());
                let skipped = le.get("skipped").and_then(|v| v.as_array());
                let base = match le.get("state").and_then(|v| v.as_str()) {
                    Some("skipped") => {
                        // No language ran — surface the reader-frame reason(s) + install next-action.
                        let reasons: Vec<String> = skipped
                            .into_iter()
                            .flatten()
                            .filter_map(|s| s.get("reason").and_then(|v| v.as_str()))
                            .map(|r| r.to_string())
                            .collect();
                        let why = if reasons.is_empty() {
                            "no resolver toolchain for the eligible languages".to_string()
                        } else {
                            reasons.join("; ")
                        };
                        format!("skipped — {why}, {ago}")
                    }
                    _ => {
                        let enriched = le
                            .get("enriched_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let eligible = le
                            .get("eligible_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let promoted = le
                            .get("promoted_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        // A mixed run shows the missing-toolchain reason + next action per skipped
                        // language, not just its name (review-0 item 2; slice §3.2).
                        let skip = skip_detail_clauses(skipped);
                        if eligible == 0 {
                            format!("up to date (nothing to resolve), {ago}{skip}")
                        } else {
                            // ENRICH-YIELD-1: the completion headline. review-2 item 3: render the
                            // promotion's OWN denominator (resolved candidates → promoted) via
                            // `enrichment_promoted_clause`, NOT the enriched/eligible population; then
                            // name the top rejecting classes (reader frame) so the line says WHY
                            // promotion banked so few.
                            let funnel = le.get("promotion_funnel");
                            let promoted_clause = enrichment_promoted_clause(funnel, promoted);
                            let funnel_note = enrichment_funnel_headline(funnel);
                            format!(
                                "resolved {enriched}/{eligible} receiver types, {promoted_clause}{funnel_note}, {ago}{skip}"
                            )
                        }
                    }
                };
                // A newer pass may be queued behind the last-completed one (running is already on the
                // `activity` line as "enriching <repo>"; queued is not, so surface it here).
                match activity {
                    "queued" => format!("{base}; a newer pass is queued"),
                    _ => base,
                }
            }
            // No pass has recorded yet — but one may be queued/running RIGHT NOW, in which case "none
            // yet" is false (review-0 item 1). Speak the live state.
            _ => match activity {
                "running" => "running — resolving receiver types now".to_string(),
                "queued" => "queued — a background pass is scheduled".to_string(),
                _ => "none yet — runs after the next index".to_string(),
            },
        }
    };
    // ENRICH-YIELD-1: the full per-gate reader-frame breakdown on the doctor `details` surface — the
    // chosen least-new-surface "queryable full breakdown" (§2.2). Only when enrichment is enabled and
    // the last pass carried a funnel; absent otherwise (honest "no data").
    let details = if enabled {
        response
            .get("last_enrichment")
            .filter(|le| le.is_object())
            .and_then(|le| enrichment_funnel_details(le.get("promotion_funnel")))
    } else {
        None
    };

    ProbeResult {
        name: "enrichment".to_string(),
        passed: true,
        message,
        details,
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

    // SNAPSHOT-RETENTION-1 (D): what the last background cleanup pass reclaimed.
    probes.push(retention_probe(response));

    // ENRICH-LIFECYCLE-1 (D3): the enrichment lifecycle (completed / skipped / disabled / none yet).
    probes.push(enrichment_probe(response));

    // FORGET-REPO-1 §2.2: the orphan-storage line (orphan DB files + bytes, dead-path registry
    // entries, stray sidecars) with a concrete next action per class.
    probes.push(orphan_probe(response));

    probes
}

/// FORGET-REPO-1 §2.2: the orphan-storage line for `rmap doctor`.
///
/// Renders the three orphan classes from `daemon_info.orphans` — each with a concrete next action:
/// - orphan DB files + stray sidecars → `rmap maintenance gc`
/// - dead-path registry entries → `rmap repo remove <path>`
///
/// ALWAYS passes: orphaned storage is a cleanup opportunity surfaced for discovery (VISION:
/// discovery over enforcement), not a broken install — it must not flip the `rmap doctor` health
/// verdict. A `scan_error` (the daemon could not list `databases/`) is reported as an honest unknown,
/// never as "0 orphans". When the block is absent (older daemon) the line degrades to "unavailable".
fn orphan_probe(response: &serde_json::Value) -> ProbeResult {
    let Some(orphans) = response.get("orphans").filter(|o| o.is_object()) else {
        return ProbeResult {
            name: "orphan_storage".to_string(),
            passed: true,
            message: "unavailable".to_string(),
            details: None,
        };
    };

    if let Some(err) = orphans.get("scan_error").and_then(|v| v.as_str()) {
        return ProbeResult {
            name: "orphan_storage".to_string(),
            passed: true,
            message: format!("unknown — could not scan databases/: {err}"),
            details: None,
        };
    }

    let orphan_db_count = orphans
        .get("orphan_db_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let orphan_db_bytes = orphans
        .get("orphan_db_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let stray_count = orphans
        .get("stray_sidecar_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let stray_bytes = orphans
        .get("stray_sidecar_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let dead = orphans
        .get("dead_path_entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let reclaimable = orphans
        .get("reclaimable_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if orphan_db_count == 0 && stray_count == 0 && dead.is_empty() {
        return ProbeResult {
            name: "orphan_storage".to_string(),
            passed: true,
            message: "none".to_string(),
            details: None,
        };
    }

    // Headline: the reclaimable total; details: one line per class with its next action.
    let message = format!(
        "{} reclaimable across orphaned storage",
        format_size(reclaimable as i64)
    );
    let mut lines: Vec<String> = Vec::new();
    if orphan_db_count > 0 {
        lines.push(format!(
            "{orphan_db_count} orphan DB file(s) ({}) — run `rmap maintenance gc`",
            format_size(orphan_db_bytes as i64)
        ));
    }
    if stray_count > 0 {
        lines.push(format!(
            "{stray_count} stray sidecar(s) ({}) — run `rmap maintenance gc`",
            format_size(stray_bytes as i64)
        ));
    }
    if !dead.is_empty() {
        lines.push(format!(
            "{} registered repo(s) at a path that no longer exists:",
            dead.len()
        ));
        for d in &dead {
            let path = d.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            // review-1 #4: use the daemon's shell-quoted `next_action` so a path with spaces pastes as
            // ONE argument. Fall back to the bare form only for a daemon that predates the field.
            let action = d
                .get("next_action")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("rmap repo remove {path}"));
            lines.push(format!("  {path} — run `{action}`"));
        }
    }

    ProbeResult {
        name: "orphan_storage".to_string(),
        passed: true,
        message,
        details: Some(lines.join("\n        ")),
    }
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

    // ── FORGET-REPO-1 §2.2: the orphan-storage doctor line ──────────────────────────────────────

    // All three orphan classes render with a concrete next action; the probe never flips health.
    #[test]
    fn orphan_probe_renders_all_three_classes_with_next_actions() {
        let response = json!({ "orphans": {
            "orphan_db_count": 28, "orphan_db_bytes": 4_187_593_011_u64,
            "stray_sidecar_count": 2, "stray_sidecar_bytes": 40_960_u64,
            "reclaimable_bytes": 4_187_633_971_u64,
            "dead_path_entries": [ {"path": "/private/tmp/test_repo", "repo": "test_repo"} ],
            "scan_error": serde_json::Value::Null,
        }});
        let probe = orphan_probe(&response);
        assert!(
            probe.passed,
            "orphaned storage is discovery, never flips health"
        );
        assert!(probe.message.contains("reclaimable"), "{}", probe.message);
        let details = probe.details.expect("class breakdown present");
        assert!(details.contains("28 orphan DB file(s)"), "{details}");
        assert!(details.contains("rmap maintenance gc"), "{details}");
        assert!(details.contains("2 stray sidecar(s)"), "{details}");
        // Dead-path entry names the exact per-path next action.
        assert!(
            details.contains("rmap repo remove /private/tmp/test_repo"),
            "{details}"
        );
    }

    // review-1 #4: a dead-path repo whose path contains a space renders the daemon's SHELL-QUOTED
    // next action verbatim, so it can be copy/pasted as one `rmap repo remove` argument.
    #[test]
    fn orphan_probe_renders_shell_quoted_next_action_for_spaced_path() {
        let response = json!({ "orphans": {
            "orphan_db_count": 0, "orphan_db_bytes": 0,
            "stray_sidecar_count": 0, "stray_sidecar_bytes": 0,
            "reclaimable_bytes": 0,
            "dead_path_entries": [ {
                "path": "/Users/me/My Repo/proj", "repo": "proj",
                "next_action": "rmap repo remove '/Users/me/My Repo/proj'"
            } ],
            "scan_error": serde_json::Value::Null,
        }});
        let probe = orphan_probe(&response);
        assert!(probe.passed);
        let details = probe.details.expect("dead-path class present");
        assert!(
            details.contains("rmap repo remove '/Users/me/My Repo/proj'"),
            "the copy-pasteable quoted command is rendered: {details}"
        );
    }

    // No orphans → a quiet "none" (still passing).
    #[test]
    fn orphan_probe_none_when_clean() {
        let response = json!({ "orphans": {
            "orphan_db_count": 0, "orphan_db_bytes": 0,
            "stray_sidecar_count": 0, "stray_sidecar_bytes": 0,
            "reclaimable_bytes": 0, "dead_path_entries": [], "scan_error": serde_json::Value::Null,
        }});
        let probe = orphan_probe(&response);
        assert!(probe.passed);
        assert_eq!(probe.message, "none");
    }

    // A listing failure is an honest unknown, NOT "0 orphans".
    #[test]
    fn orphan_probe_scan_error_is_unknown_not_zero() {
        let response = json!({ "orphans": {
            "orphan_db_count": 0, "scan_error": "cannot list databases/: permission denied",
        }});
        let probe = orphan_probe(&response);
        assert!(probe.passed, "a scan failure must not flip health");
        assert!(probe.message.contains("unknown"), "{}", probe.message);
        assert!(
            probe.message.contains("permission denied"),
            "{}",
            probe.message
        );
    }

    // Older daemon (no `orphans` block) → "unavailable", not a fabricated "none".
    #[test]
    fn orphan_probe_absent_block_is_unavailable() {
        let probe = orphan_probe(&json!({}));
        assert!(probe.passed);
        assert_eq!(probe.message, "unavailable");
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

    // SNAPSHOT-RETENTION-1 (review-0 #4): the doctor activity line renders an in-flight background
    // retention pass as "reclaiming <repo>" — the honesty surface for "doctor shows the pass as an
    // active op". The daemon stamps `OpKind::Retention` (slug "retention") in the SAME activity
    // registry as index/refresh; this proves the doctor render path speaks the reader's frame
    // ("reclaiming"), not the internal slug. Pairs with the daemon-runtime
    // `daemon_info_surfaces_the_active_retention_op` proof that the daemon actually emits it.
    #[test]
    fn activity_probe_renders_in_flight_retention() {
        let response = json!({
            "active_operations": [
                { "kind": "retention", "repo": "my-repo", "phase": serde_json::Value::Null,
                  "current": 0, "total": 0, "started_secs_ago": 3 }
            ]
        });
        let probe = activity_probe(&response);
        assert!(probe.passed, "activity never flips health");
        assert!(
            probe.message.contains("reclaiming my-repo"),
            "the retention pass renders in the reader's frame, not the internal slug: {}",
            probe.message
        );
    }

    // SNAPSHOT-RETENTION-1: the doctor cleanup line reports the last pass's pruned/reclaimed — the
    // honesty surface for the async pass (the `rmap index` reply only says "queued").
    #[test]
    fn retention_probe_reports_last_pass() {
        let response = json!({
            "last_retention": {
                "repo": "my-repo", "pruned_count": 1, "non_ready_reclaimed": 0,
                "reclaimed_bytes": 1_610_612_736_u64, "vacuum_status": "ran", "finished_secs_ago": 45
            }
        });
        let probe = retention_probe(&response);
        assert!(probe.passed, "retention line never flips health");
        assert!(
            probe.message.contains("pruned 1 snapshot(s)"),
            "{}",
            probe.message
        );
        assert!(
            probe.message.contains("reclaimed 1.50 GB"),
            "{}",
            probe.message
        );
        assert!(probe.message.contains("45s ago"), "{}", probe.message);
    }

    #[test]
    fn retention_probe_none_yet_and_nothing_pruned() {
        // No pass yet (field absent) or field present-but-null → "none yet".
        assert_eq!(retention_probe(&json!({})).message, "cleanup: none yet");
        assert_eq!(
            retention_probe(&json!({ "last_retention": serde_json::Value::Null })).message,
            "cleanup: none yet"
        );
        // A pass that pruned nothing says so honestly.
        let nothing = json!({ "last_retention": {
            "pruned_count": 0, "non_ready_reclaimed": 0, "reclaimed_bytes": 0, "finished_secs_ago": 3
        }});
        assert!(
            retention_probe(&nothing)
                .message
                .contains("nothing to prune"),
            "{}",
            retention_probe(&nothing).message
        );
    }

    #[test]
    fn retention_probe_deferred_vacuum_is_honest() {
        // Rows pruned but VACUUM deferred BELOW THRESHOLD (the recyclable steady-state case) → says so
        // with its reason; no fabricated reclaim.
        let below = json!({ "last_retention": {
            "pruned_count": 2, "non_ready_reclaimed": 0, "reclaimed_bytes": 0,
            "vacuum_status": "below_threshold", "finished_secs_ago": 10
        }});
        let below_msg = retention_probe(&below).message;
        assert!(
            below_msg.contains("pruned 2 snapshot(s)")
                && below_msg.contains("deferred — below threshold"),
            "{below_msg}"
        );

        // Rows pruned but VACUUM deferred because a READER was active → the operator sees the reason is
        // reader contention (a VACUUM the next pass retries), NOT a below-threshold skip. This is the
        // honest reader-vs-VACUUM surface (never a raw busy error).
        let readers = json!({ "last_retention": {
            "pruned_count": 1, "non_ready_reclaimed": 0, "reclaimed_bytes": 0,
            "vacuum_status": "deferred_readers_active", "finished_secs_ago": 4
        }});
        let readers_msg = retention_probe(&readers).message;
        assert!(
            readers_msg.contains("pruned 1 snapshot(s)")
                && readers_msg.contains("deferred — repo was being read"),
            "{readers_msg}"
        );
    }

    // ── ENRICH-LIFECYCLE-1: the enrichment lifecycle line across states ────────────────────────────

    #[test]
    fn enrichment_probe_disabled_wins() {
        // Opt-out is authoritative: nothing runs, so no last-pass line would be honest.
        let response =
            json!({ "enrichment_enabled": false, "last_enrichment": serde_json::Value::Null });
        let probe = enrichment_probe(&response);
        assert!(probe.passed, "enrichment line never flips health");
        // The message carries only the state; the doctor renderer's LABEL adds "enrichment: ".
        assert_eq!(probe.message, "disabled (RMAP_AUTO_ENRICH)");
    }

    #[test]
    fn enrichment_probe_reports_completed_pass() {
        let response = json!({
            "enrichment_enabled": true,
            "last_enrichment": {
                "repo": "my-repo", "state": "completed",
                "eligible_count": 100, "enriched_count": 81, "promoted_count": 40,
                "enrichment_rate": 81.0, "skipped": [], "finished_secs_ago": 12
            }
        });
        let msg = enrichment_probe(&response).message;
        assert!(
            msg.contains("resolved 81/100 receiver types") && msg.contains("promoted 40"),
            "{msg}"
        );
        assert!(msg.contains("12s ago"), "{msg}");
    }

    // ENRICH-YIELD-1: a completed pass with a promotion funnel names the top rejecting classes in the
    // headline (reader frame) AND carries the full per-gate breakdown on the details surface.
    #[test]
    fn enrichment_probe_surfaces_promotion_funnel() {
        let response = json!({
            "enrichment_enabled": true,
            "last_enrichment": {
                "repo": "r", "state": "completed",
                "eligible_count": 100, "enriched_count": 81, "promoted_count": 40,
                "enrichment_rate": 81.0, "skipped": [], "finished_secs_ago": 7,
                "promotion_funnel": {
                    "candidates": 78, "promoted": 40, "rejected": 38,
                    "rejections": [
                        {"reason":"external_type","gate":4,"label":"receiver type is external to this repo (a library type)","count":30},
                        {"reason":"method_not_found_on_class","gate":6,"label":"method isn't defined on the resolved class","count":8}
                    ],
                    "gates": [
                        {"gate":1,"label":"call is a method call whose receiver type we resolve","entered":78,"rejected":0},
                        {"gate":2,"label":"resolving this kind of call is enabled","entered":78,"rejected":0},
                        {"gate":3,"label":"receiver type was resolved by the compiler","entered":78,"rejected":0},
                        {"gate":4,"label":"receiver type is defined in this repo (not a library type)","entered":78,"rejected":30},
                        {"gate":7,"label":"receiver type is a single type (not a union/intersection)","entered":48,"rejected":0},
                        {"gate":8,"label":"call is a direct receiver.method (no chaining or indexing)","entered":48,"rejected":0},
                        {"gate":5,"label":"receiver type maps to exactly one class we can see","entered":48,"rejected":0},
                        {"gate":6,"label":"the called method is uniquely defined on that class","entered":48,"rejected":8}
                    ]
                }
            }
        });
        let probe = enrichment_probe(&response);
        // Headline: the enrichment metric (resolved 81/100 receiver types) is preserved AND the
        // dominant reader-frame class is named.
        assert!(
            probe.message.contains("resolved 81/100 receiver types"),
            "{}",
            probe.message
        );
        // review-2 item 3: the promotion clause states the funnel's OWN denominator — the resolved
        // candidates (78) that entered the filter, NOT the enriched (81) or eligible (100) counts. The
        // 100/81/78/40 fixture has all four values differing, so any conflation is visible here.
        assert!(
            probe.message.contains("promoted 40/78 resolved candidates"),
            "the candidates→promoted denominator is rendered in the completion headline: {}",
            probe.message
        );
        assert!(
            !probe.message.contains("40/81") && !probe.message.contains("40/100"),
            "promotion must NOT be denominated against the enriched/eligible population: {}",
            probe.message
        );
        assert!(
            probe.message.contains(
                "top rejections: receiver type is external to this repo (a library type) 30"
            ),
            "the dominant class is named in the headline: {}",
            probe.message
        );
        assert!(
            !probe.message.contains("gate"),
            "no pipeline-internal 'gate N' in the reader line: {}",
            probe.message
        );
        // Details: BOTH the per-gate waterfall (§2.1) and the per-class breakdown, reader frame.
        let details = probe.details.expect("funnel breakdown present in details");
        assert!(
            details.contains("78 resolved candidates → 40 promoted"),
            "{details}"
        );
        // Per-gate waterfall (evaluation order, entering → filtered-out), reader frame.
        assert!(
            details.contains("gate-by-gate (in filter order):"),
            "{details}"
        );
        // Gate 2 (the config-opt-in placeholder) renders as a no-op stage: entrants reached, nothing
        // filtered — the honest rendering of a gate that rejects nothing (review-1 item 1).
        assert!(
            details.contains(
                "resolving this kind of call is enabled: 78 reached → 0 filtered out here"
            ),
            "gate 2 renders as a no-op stage: {details}"
        );
        assert!(
            details.contains(
                "receiver type is defined in this repo (not a library type): 78 reached → 30 filtered out here (38%)"
            ),
            "per-gate entering+rejected line present: {details}"
        );
        assert!(
            details.contains(
                "the called method is uniquely defined on that class: 48 reached → 8 filtered out here (17%)"
            ),
            "a later gate shows its own entrants (48, not 78) — the waterfall narrowed: {details}"
        );
        // Retained per-class dominant reasons, each as a share of resolved.
        assert!(
            details.contains(
                "receiver type is external to this repo (a library type): 30 (38% of resolved)"
            ),
            "{details}"
        );
        assert!(
            details.contains("method isn't defined on the resolved class: 8 (10% of resolved)"),
            "{details}"
        );
        // Reader frame throughout the details too — no "gate N".
        assert!(
            !details.contains("gate 4") && !details.contains("gate 6"),
            "{details}"
        );
    }

    // No funnel (older daemon, or a toolchain-skipped pass) → the line and details are unchanged: no
    // "top rejections" clause, no phantom breakdown (honest "no data").
    #[test]
    fn enrichment_probe_without_funnel_is_unchanged() {
        let response = json!({
            "enrichment_enabled": true,
            "last_enrichment": {
                "repo": "r", "state": "completed",
                "eligible_count": 100, "enriched_count": 81, "promoted_count": 40,
                "enrichment_rate": 81.0, "skipped": [], "finished_secs_ago": 7
            }
        });
        let probe = enrichment_probe(&response);
        assert!(
            !probe.message.contains("top rejections"),
            "{}",
            probe.message
        );
        assert!(probe.details.is_none(), "no funnel → no details");
    }

    #[test]
    fn enrichment_probe_reports_toolchain_skip_with_reason() {
        // A language had eligible edges but no toolchain → the reader-frame skip + install next-action.
        let response = json!({
            "enrichment_enabled": true,
            "last_enrichment": {
                "repo": "my-repo", "state": "skipped",
                "eligible_count": 0, "enriched_count": 0, "promoted_count": 0, "enrichment_rate": 0.0,
                "skipped": [{ "language": "rust", "reason": "rust-analyzer not found — install rust-analyzer (rustup component add rust-analyzer)" }],
                "finished_secs_ago": 5
            }
        });
        let msg = enrichment_probe(&response).message;
        assert!(
            msg.starts_with("skipped — rust-analyzer not found"),
            "the skip surfaces the reader-frame reason (no self-prefixed label): {msg}"
        );
        assert!(msg.contains("rustup component add rust-analyzer"), "{msg}");
    }

    #[test]
    fn enrichment_probe_none_yet_when_no_pass() {
        // No pass yet (absent or null) with enrichment ON → "none yet".
        assert_eq!(
            enrichment_probe(&json!({ "enrichment_enabled": true })).message,
            "none yet — runs after the next index"
        );
        assert_eq!(
            enrichment_probe(
                &json!({ "enrichment_enabled": true, "last_enrichment": serde_json::Value::Null })
            )
            .message,
            "none yet — runs after the next index"
        );
    }

    // Regression (ENRICH-LIFECYCLE-1 operator finding): the doctor line must read "enrichment: …"
    // ONCE, not "enrichment: enrichment: …". The renderer prepends the probe `name` as a label
    // (`print_probe_labeled`), so the message must NOT itself begin with "enrichment:". Compose the
    // displayed line the way the renderer does and assert a single prefix, across every state.
    #[test]
    fn enrichment_probe_message_never_double_prefixes_the_label() {
        let states = [
            json!({ "enrichment_enabled": false, "last_enrichment": serde_json::Value::Null }),
            json!({ "enrichment_enabled": true }),
            json!({ "enrichment_enabled": true, "last_enrichment": {
                "repo": "r", "state": "completed", "eligible_count": 100, "enriched_count": 81,
                "promoted_count": 40, "enrichment_rate": 81.0, "skipped": [], "finished_secs_ago": 3
            }}),
            json!({ "enrichment_enabled": true, "last_enrichment": {
                "repo": "r", "state": "skipped", "eligible_count": 0, "enriched_count": 0,
                "promoted_count": 0, "enrichment_rate": 0.0,
                "skipped": [{ "language": "rust", "reason": "rust-analyzer not found — install it" }],
                "finished_secs_ago": 3
            }}),
        ];
        for state in states {
            let probe = enrichment_probe(&state);
            assert!(
                !probe.message.starts_with("enrichment:"),
                "message must not carry the label (the renderer adds it): {}",
                probe.message
            );
            // The renderer prints "{name}: {message}" → this is the displayed doctor line.
            let displayed = format!("{}: {}", probe.name, probe.message);
            assert!(
                !displayed.contains("enrichment: enrichment:"),
                "doctor line double-prefixed: {displayed}"
            );
        }
    }

    // review-0 item 2 / slice §3.2: a MIXED run — some languages resolved, others toolchain-absent —
    // must surface the missing-toolchain reason + install next-action for EACH skipped language on
    // doctor, not just the bare language name. Here Rust resolved; Java was skipped for want of jdtls.
    #[test]
    fn enrichment_probe_reports_mixed_run_with_skip_reasons() {
        let response = json!({
            "enrichment_enabled": true,
            "enrichment_activity": "idle",
            "last_enrichment": {
                "repo": "my-repo", "state": "completed",
                "eligible_count": 50, "enriched_count": 40, "promoted_count": 12,
                "enrichment_rate": 80.0,
                "skipped": [{ "language": "java", "reason": "jdtls not found — set JDTLS_PATH to your jdtls launcher" }],
                "finished_secs_ago": 8
            }
        });
        let msg = enrichment_probe(&response).message;
        assert!(
            msg.contains("resolved 40/50 receiver types") && msg.contains("promoted 12"),
            "the run that DID happen is still reported: {msg}"
        );
        // The skip is NOT just the language name — it carries the reason AND the install next-action.
        assert!(
            msg.contains("skipped java: jdtls not found"),
            "a mixed-run skip names the language AND why: {msg}"
        );
        assert!(
            msg.contains("set JDTLS_PATH to your jdtls launcher"),
            "a mixed-run skip carries the install next-action, not just the language name: {msg}"
        );
    }

    // review-0 item 1 / slice §3.7: a queued-but-not-yet-recorded pass must NOT render as the false
    // "none yet — runs after the next index". With no last_enrichment but `enrichment_activity` =
    // "queued", the line tells the truth: a pass is scheduled.
    #[test]
    fn enrichment_probe_queued_is_not_none_yet() {
        let response = json!({ "enrichment_enabled": true, "enrichment_activity": "queued" });
        let msg = enrichment_probe(&response).message;
        assert!(
            msg.contains("queued") && !msg.contains("none yet"),
            "a queued pass must not render as 'none yet': {msg}"
        );
    }

    // review-0 item 1: the FIRST pass, running before it records, is likewise not "none yet".
    #[test]
    fn enrichment_probe_running_first_pass_is_not_none_yet() {
        let response = json!({ "enrichment_enabled": true, "enrichment_activity": "running" });
        let msg = enrichment_probe(&response).message;
        assert!(
            msg.contains("running") && !msg.contains("none yet"),
            "a running first pass must not render as 'none yet': {msg}"
        );
    }

    // review-0 item 1: a newer pass queued BEHIND a last-completed one is surfaced (the completed line
    // stays truthful; the queued state is appended — running is left to the `activity` line).
    #[test]
    fn enrichment_probe_surfaces_queued_behind_completed() {
        let response = json!({
            "enrichment_enabled": true,
            "enrichment_activity": "queued",
            "last_enrichment": {
                "repo": "my-repo", "state": "completed",
                "eligible_count": 100, "enriched_count": 81, "promoted_count": 40,
                "enrichment_rate": 81.0, "skipped": [], "finished_secs_ago": 30
            }
        });
        let msg = enrichment_probe(&response).message;
        assert!(
            msg.contains("resolved 81/100 receiver types")
                && msg.contains("a newer pass is queued"),
            "the last result stays truthful AND the newer queued pass is surfaced: {msg}"
        );
    }

    // review-0 item 1 (regression): with nothing in flight ("idle", or the field absent on an older
    // daemon) the null-report line is UNCHANGED — still "none yet — runs after the next index".
    #[test]
    fn enrichment_probe_idle_is_still_none_yet() {
        assert_eq!(
            enrichment_probe(&json!({ "enrichment_enabled": true, "enrichment_activity": "idle" }))
                .message,
            "none yet — runs after the next index"
        );
        // Older daemon (no `enrichment_activity` field) defaults to idle → byte-for-byte unchanged.
        assert_eq!(
            enrichment_probe(&json!({ "enrichment_enabled": true })).message,
            "none yet — runs after the next index"
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
