//! Presentation layer for the `check` command.
//!
//! # CLI-OUT-1
//!
//! Transforms daemon check response (OrientResult envelope) into
//! human-readable plain text focused on the verdict.
//!
//! ## Human Output Structure
//!
//! ```text
//! Repo: billing-service
//! Verdict: FAIL
//!
//! Failing conditions
//!   - GATE_PASS: Gate is failing (1 of 3 obligations).
//!   - NO_STALE_FILES: 2 stale files detected in snapshot.
//!
//! Passing conditions
//!   - SNAPSHOT_EXISTS: Snapshot exists.
//!   - CALL_GRAPH_RELIABLE: Your code's calls 95% resolved (HIGH).
//! ```

use repo_graph_coherence::CoherenceEnvelope;
use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized check response from daemon.
///
/// Check uses the OrientResult envelope but with CHECK_* signals. CHECK-LIVEGRAPH-IMPL: the daemon now
/// returns a `CoherenceEnvelope<CoherentOrientResult>` (the wrapper is the top level), so this projects the
/// wrapper's inner `value`, and each signal is a LEAF `CoherenceEnvelope<CheckSignal>` (contract D7) — the
/// inner `CheckSignal` is pristine; provenance/trust/freshness ride in the wrapper siblings. The renderer
/// reads each `.value`. The honest MEET freshness rides on the ROOT envelope (`render_check_envelope`).
#[derive(Debug, Deserialize)]
pub struct CheckResponse {
    pub repo: String,
    /// Human-readable repo name for CLI display.
    /// Populated by daemon from registry alias or path basename.
    /// When present, prefer this over `repo` (which is internal UID).
    #[serde(default)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub snapshot: String,
    #[allow(dead_code)]
    pub confidence: String,
    /// Each signal is a LEAF `CoherenceEnvelope<CheckSignal>` (CHECK-LIVEGRAPH-IMPL / contract D7). The
    /// renderer reads each `.value` (the pristine `CheckSignal`).
    #[serde(default)]
    pub signals: Vec<CoherenceEnvelope<CheckSignal>>,
}

#[derive(Debug, Deserialize)]
pub struct CheckSignal {
    pub code: String,
    pub severity: String,
    pub summary: String,
    #[serde(default)]
    pub evidence: Option<CheckEvidence>,
}

#[derive(Debug, Deserialize)]
pub struct CheckEvidence {
    #[serde(default)]
    pub conditions: Vec<ConditionEvidence>,
    #[serde(default)]
    pub fail_conditions: Vec<ConditionEvidence>,
    #[serde(default)]
    pub incomplete_conditions: Vec<ConditionEvidence>,
    #[serde(default)]
    pub passing: Vec<ConditionEvidence>,
}

#[derive(Debug, Deserialize)]
pub struct ConditionEvidence {
    pub code: String,
    pub status: String,
    pub summary: String,
}

/// INDEX-BASIS-1: condition codes emitted for one-release JSON/CI compatibility but
/// SUPPRESSED from human output (a duplicate would only add noise). `STALE_FILES` is
/// the deprecated alias of the honestly-named `UNPARSED_FILES`; both carry the same
/// status/data, so hiding the alias never hides a distinct verdict.
const HUMAN_SUPPRESSED_CONDITION_CODES: &[&str] = &["STALE_FILES"];

/// Whether a condition should appear in human output (false for deprecated aliases).
fn shown_in_human(c: &ConditionEvidence) -> bool {
    !HUMAN_SUPPRESSED_CONDITION_CODES.contains(&c.code.as_str())
}

// ── Human Rendering ──────────────────────────────────────────────────────────

/// Render check's coherence-wrapped daemon response (CHECK-LIVEGRAPH-IMPL).
///
/// The daemon returns `CoherenceEnvelope<CoherentOrientResult>`; the `--json` path prints it verbatim, and
/// this human path projects its inner `value` into [`CheckResponse`]. It renders the verdict + conditions
/// exactly as before, with the honest ROOT MEET freshness appended to the verdict line
/// (`PASS@Fresh` / `FAIL@Stale` / `INCOMPLETE@Unavailable` — spec §5 W2) so the human surface shows the new
/// 2-axis model. check has NO `trust_briefing` (D-CHECK-2), so there is no degradation section; the
/// verdict/condition text is otherwise byte-unchanged.
pub fn render_check_envelope(env: &CoherenceEnvelope<CheckResponse>) -> String {
    env.value
        .render_human_with_freshness(&format!("{:?}", env.freshness))
}

/// Derive `rmap check`'s process EXIT CODE from the daemon's wrapped result JSON (CHECK-LIVEGRAPH-IMPL
/// §1f/§3e — check's CI contract).
///
/// The daemon returns `CoherenceEnvelope<CoherentOrientResult>`, so the verdict signal now lives at
/// `result["value"]["signals"][*]["value"]["code"]` — NOT the pre-wrapper top-level `result["signals"]`.
/// Reading the now-dead top-level path would resolve to `null` and silently return exit 2 for EVERY check,
/// INCLUDING a PASS (a green repo would report failure) — the anti-silent-break hazard the spec flags as
/// load-bearing (§3e CRITICAL / §5 CW5). The value mapping is preserved verbatim from the pre-wrapper
/// behaviour: `CHECK_PASS` → 0, `CHECK_FAIL` → 1, `CHECK_INCOMPLETE` → 2, and verdict-not-found → 2 (the
/// `.unwrap_or(2)` fallback — INCOMPLETE and "no verdict signal" both map to 2).
///
/// This projection is co-located with [`render_check_envelope`] (the OTHER wire→CLI read) on purpose:
/// §3e requires the exit-code extraction and the human-render deserialization to move in lockstep with the
/// daemon's wrapper, so they live and are tested together and cannot silently drift. `run_check_cmd`
/// computes this ONCE, before the human/`--json` mode branch, so both modes share the identical exit code.
pub fn check_exit_code(result: &serde_json::Value) -> u8 {
    result["value"]["signals"]
        .as_array()
        .and_then(|signals| {
            signals.iter().find_map(|leaf| {
                leaf["value"]["code"].as_str().and_then(|code| match code {
                    "CHECK_PASS" => Some(0),
                    "CHECK_FAIL" => Some(1),
                    "CHECK_INCOMPLETE" => Some(2),
                    _ => None,
                })
            })
        })
        .unwrap_or(2)
}

impl CheckResponse {
    /// Render the check response as human-readable plain text, WITHOUT a freshness suffix (legacy /
    /// non-coherent callers and unit tests). Coherent callers use [`render_check_envelope`].
    pub fn render_human(&self) -> String {
        self.render_inner(None)
    }

    /// Render with the coherence MEET freshness appended to the verdict line (`Verdict: PASS@Fresh`).
    /// CHECK-LIVEGRAPH-IMPL §5 W2 — the human surface for the new freshness axis.
    pub fn render_human_with_freshness(&self, freshness: &str) -> String {
        self.render_inner(Some(freshness))
    }

    fn render_inner(&self, freshness: Option<&str>) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        // CLI-OUT-2B: prefer display_name (human-readable) over internal repo UID
        let repo_display = self.display_name.as_deref().unwrap_or(&self.repo);
        out.push_str(&kv_line("Repo", repo_display));

        // Determine verdict from signals; append the MEET freshness suffix when coherent (W2).
        let verdict = self.determine_verdict();
        let verdict_line = match freshness {
            Some(f) => format!("{verdict}@{f}"),
            None => verdict,
        };
        out.push_str(&kv_line("Verdict", &verdict_line));
        out.push('\n');

        // ── Condition details ──────────────────────────────────────
        if let Some(signal) = self.find_check_signal() {
            out.push_str(&self.render_conditions(signal));
        }

        out.trim_end().to_string()
    }

    fn determine_verdict(&self) -> String {
        for leaf in &self.signals {
            match leaf.value.code.as_str() {
                "CHECK_PASS" => return "PASS".to_string(),
                "CHECK_FAIL" => return "FAIL".to_string(),
                "CHECK_INCOMPLETE" => return "INCOMPLETE".to_string(),
                _ => {}
            }
        }
        "UNKNOWN".to_string()
    }

    fn find_check_signal(&self) -> Option<&CheckSignal> {
        self.signals.iter().map(|leaf| &leaf.value).find(|s| {
            matches!(
                s.code.as_str(),
                "CHECK_PASS" | "CHECK_FAIL" | "CHECK_INCOMPLETE"
            )
        })
    }

    fn render_conditions(&self, signal: &CheckSignal) -> String {
        let mut out = String::new();

        let evidence = match &signal.evidence {
            Some(e) => e,
            None => return out,
        };

        // Incomplete conditions (if any)
        if evidence.incomplete_conditions.iter().any(shown_in_human) {
            out.push_str(&heading("Incomplete conditions"));
            for c in evidence
                .incomplete_conditions
                .iter()
                .filter(|c| shown_in_human(c))
            {
                out.push_str(&bullet(&format!("{}: {}", c.code, c.summary)));
            }
            out.push('\n');
        }

        // Failing conditions (if any)
        if evidence.fail_conditions.iter().any(shown_in_human) {
            out.push_str(&heading("Failing conditions"));
            for c in evidence
                .fail_conditions
                .iter()
                .filter(|c| shown_in_human(c))
            {
                out.push_str(&bullet(&format!("{}: {}", c.code, c.summary)));
            }
            out.push('\n');
        }

        // Passing conditions
        // For CHECK_PASS, use .conditions; for others, use .passing
        let passing = if signal.code == "CHECK_PASS" {
            &evidence.conditions
        } else {
            &evidence.passing
        };

        if passing.iter().any(shown_in_human) {
            out.push_str(&heading("Passing conditions"));
            for c in passing.iter().filter(|c| shown_in_human(c)) {
                out.push_str(&bullet(&format!("{}: {}", c.code, c.summary)));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_coherence::{FreshnessState, Provenance, TrustPosture};

    /// Wrap a `CheckSignal` as a LEAF `CoherenceEnvelope<CheckSignal>` (CHECK-LIVEGRAPH-IMPL D7). The
    /// renderer only reads the inner `.value`, so the SQLite snapshot posture is a fine stand-in.
    fn leaf(sig: CheckSignal) -> CoherenceEnvelope<CheckSignal> {
        CoherenceEnvelope::sqlite_leaf(sig, false)
    }

    /// A root `CoherenceEnvelope<CheckResponse>` carrying the given response + ROOT freshness — the wire
    /// shape `render_check_envelope` consumes.
    fn root(resp: CheckResponse, freshness: FreshnessState) -> CoherenceEnvelope<CheckResponse> {
        CoherenceEnvelope::new(
            resp,
            Provenance::sqlite(),
            TrustPosture::snapshot_exact(),
            freshness,
        )
    }

    fn minimal_response() -> CheckResponse {
        CheckResponse {
            repo: "test-repo".to_string(),
            display_name: None,
            snapshot: "snap-123".to_string(),
            confidence: "high".to_string(),
            signals: vec![],
        }
    }

    #[test]
    fn render_shows_repo_name() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes.".to_string(),
            evidence: None,
        })];
        let out = r.render_human();
        assert!(out.contains("Repo: test-repo"));
    }

    #[test]
    fn render_shows_pass_verdict() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes: all 3 conditions pass.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "pass".to_string(),
                    summary: "Snapshot exists.".to_string(),
                }],
                fail_conditions: vec![],
                incomplete_conditions: vec![],
                passing: vec![],
            }),
        })];
        let out = r.render_human();
        assert!(out.contains("Verdict: PASS"));
        assert!(out.contains("Passing conditions"));
        assert!(out.contains("SNAPSHOT_EXISTS: Snapshot exists."));
    }

    #[test]
    fn render_shows_fail_verdict() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_FAIL".to_string(),
            severity: "high".to_string(),
            summary: "Check fails: 1 condition failing.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![],
                fail_conditions: vec![ConditionEvidence {
                    code: "GATE_PASS".to_string(),
                    status: "fail".to_string(),
                    summary: "Gate is failing.".to_string(),
                }],
                incomplete_conditions: vec![],
                passing: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "pass".to_string(),
                    summary: "Snapshot exists.".to_string(),
                }],
            }),
        })];
        let out = r.render_human();
        assert!(out.contains("Verdict: FAIL"));
        assert!(out.contains("Failing conditions"));
        assert!(out.contains("GATE_PASS: Gate is failing."));
        assert!(out.contains("Passing conditions"));
    }

    #[test]
    fn render_shows_incomplete_verdict() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_INCOMPLETE".to_string(),
            severity: "medium".to_string(),
            summary: "Check incomplete: 1 condition missing data.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![],
                fail_conditions: vec![],
                incomplete_conditions: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "incomplete".to_string(),
                    summary: "No snapshot found.".to_string(),
                }],
                passing: vec![],
            }),
        })];
        let out = r.render_human();
        assert!(out.contains("Verdict: INCOMPLETE"));
        assert!(out.contains("Incomplete conditions"));
        assert!(out.contains("SNAPSHOT_EXISTS: No snapshot found."));
    }

    #[test]
    fn render_hides_internal_fields() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes.".to_string(),
            evidence: None,
        })];
        let out = r.render_human();
        assert!(!out.contains("snap-123"), "snapshot should be hidden");
    }

    // ── CHECK-LIVEGRAPH-IMPL §5 W2: the freshness suffix on the verdict line ──

    #[test]
    fn envelope_render_appends_freshness_suffix_to_verdict() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes.".to_string(),
            evidence: None,
        })];
        // PASS over a Fresh snapshot renders `Verdict: PASS@Fresh` (the new 2-axis surface).
        let out = render_check_envelope(&root(r, FreshnessState::Fresh));
        assert!(out.contains("Verdict: PASS@Fresh"), "got: {out}");
        // No degradation/trust section: check has no trust_briefing.
        assert!(!out.contains("Degradation"));
    }

    #[test]
    fn envelope_render_shows_unavailable_for_no_snapshot() {
        let mut r = minimal_response();
        r.signals = vec![leaf(CheckSignal {
            code: "CHECK_INCOMPLETE".to_string(),
            severity: "medium".to_string(),
            summary: "Check incomplete.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![],
                fail_conditions: vec![],
                incomplete_conditions: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "incomplete".to_string(),
                    summary: "No READY snapshot. Index the repo first.".to_string(),
                }],
                passing: vec![],
            }),
        })];
        let out = render_check_envelope(&root(r, FreshnessState::Unavailable));
        assert!(
            out.contains("Verdict: INCOMPLETE@Unavailable"),
            "got: {out}"
        );
    }

    #[test]
    fn deserialize_wrapped_envelope_from_daemon_json() {
        // CHECK-LIVEGRAPH-IMPL wire shape: the top level is `CoherenceEnvelope<CoherentOrientResult>`; the
        // CLI projects `value` into CheckResponse, and each signal is a leaf with its own `value`.
        let json = r#"{
            "value": {
                "schema": "rgr.agent.v1",
                "command": "check",
                "repo": "my-app",
                "snapshot": "snap-abc",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "high",
                "signals": [
                    {
                        "value": {
                            "code": "CHECK_PASS",
                            "severity": "low",
                            "category": "check",
                            "summary": "Check passes: all 3 conditions pass.",
                            "evidence": {
                                "conditions": [
                                    { "code": "SNAPSHOT_EXISTS", "status": "pass", "summary": "Snapshot exists." }
                                ]
                            }
                        },
                        "provenance": { "source": ["sqlite", "declaration"] },
                        "trust": { "class": "Exact", "completeness": "Complete" },
                        "freshness": "Fresh"
                    }
                ],
                "limits": [],
                "next": [],
                "truncated": false
            },
            "provenance": { "source": ["sqlite", "declaration"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        }"#;

        let env: CoherenceEnvelope<CheckResponse> = serde_json::from_str(json).unwrap();
        assert_eq!(env.value.repo, "my-app");
        assert_eq!(env.value.signals.len(), 1);
        assert_eq!(env.value.signals[0].value.code, "CHECK_PASS");
        // The verdict line carries the ROOT MEET freshness.
        let out = render_check_envelope(&env);
        assert!(out.contains("Verdict: PASS@Fresh"), "got: {out}");
    }

    // ── CHECK-LIVEGRAPH-IMPL §1f/§3e/§5 CW5: EXIT-CODE PARITY over the wrapped shape ──
    //
    // These pin check's CI contract (PASS=0 / FAIL=1 / INCOMPLETE=2 / not-found=2) against the EXACT
    // `result["value"]["signals"][*]["value"]["code"]` path the CLI reads after the wrapper landed. The
    // daemon-emitted wire shape is independently proven by the rmapd dispatch test
    // (`check_returns_coherence_envelope_shape` in tests/daemon_dispatch.rs); together they show the daemon
    // emits the wrapped shape AND the CLI maps that shape to the correct exit code.

    /// A minimal daemon-wrapped result carrying ONE verdict leaf with `code` — only what
    /// [`check_exit_code`] reads (`value.signals[*].value.code`).
    fn wrapped_verdict(code: &str) -> serde_json::Value {
        serde_json::json!({
            "value": { "signals": [ { "value": { "code": code } } ] },
            "provenance": { "source": ["sqlite", "declaration"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        })
    }

    #[test]
    fn exit_code_pass_is_zero() {
        assert_eq!(check_exit_code(&wrapped_verdict("CHECK_PASS")), 0);
    }

    #[test]
    fn exit_code_fail_is_one() {
        assert_eq!(check_exit_code(&wrapped_verdict("CHECK_FAIL")), 1);
    }

    #[test]
    fn exit_code_incomplete_is_two() {
        assert_eq!(check_exit_code(&wrapped_verdict("CHECK_INCOMPLETE")), 2);
    }

    #[test]
    fn exit_code_finds_verdict_among_multiple_leaves() {
        // Real output carries the verdict alongside SNAPSHOT_INFO; the scan must find the verdict
        // regardless of leaf order (here SNAPSHOT_INFO precedes a FAIL verdict).
        let result = serde_json::json!({
            "value": { "signals": [
                { "value": { "code": "SNAPSHOT_INFO" } },
                { "value": { "code": "CHECK_FAIL" } }
            ] }
        });
        assert_eq!(check_exit_code(&result), 1);
    }

    #[test]
    fn exit_code_missing_verdict_signal_is_two() {
        // No verdict leaf at all (only SNAPSHOT_INFO) → the `.unwrap_or(2)` fallback. INCOMPLETE and
        // "no verdict found" both map to 2 (the pre-wrapper semantics, preserved).
        let result = serde_json::json!({
            "value": { "signals": [ { "value": { "code": "SNAPSHOT_INFO" } } ] }
        });
        assert_eq!(check_exit_code(&result), 2);
    }

    #[test]
    fn exit_code_ignores_dead_top_level_signals_path() {
        // ANTI-SILENT-BREAK REGRESSION (§3e CRITICAL): a PASS verdict placed at the PRE-WRAPPER top-level
        // `signals` path (no `value.signals`) must NOT be read — it resolves to the fallback 2, proving the
        // extractor does not silently honour the dead path. If `check_exit_code` ever regressed to reading
        // `result["signals"]`, this PASS would wrongly yield 0 and a green repo could report 2 in reverse;
        // pinning 2 here locks the extractor onto the wrapped path.
        let result = serde_json::json!({
            "signals": [ { "value": { "code": "CHECK_PASS" } } ]
        });
        assert_eq!(
            check_exit_code(&result),
            2,
            "the dead top-level signals path must not be honoured"
        );
    }
}
