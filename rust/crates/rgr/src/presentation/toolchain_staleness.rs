//! TOOLCHAIN-STALENESS-1 (RG-REQ-001-L06, LD-10): the one human line for the daemon's
//! `value.toolchain_staleness` on `orient` and `check`.
//!
//! The line compares the served snapshot's toolchain STAMP with the running rmap; it never says
//! which rmap produced a fact (after a refresh the stamp names the refreshing toolchain only). It is
//! a signal, not a verdict: `check` renders it beside its `INDEX_DRIFT` row and its verdict, its
//! conditions and its exit code do not read it.
//!
//! The remedy's `<path>` is the CLI's own request path (the canonical directory `rmap orient` /
//! `rmap check` sent as `repo`); `rmap repo rebuild` resolves it like the daemon resolved the request
//! — the exact registered path or its longest registered ancestor — so it runs as printed from any
//! directory inside the repository.

use repo_graph_agent::dto::toolchain_staleness::{
    ComponentDifference, ReindexState, ToolchainStaleness,
};

use crate::presentation::{bullet, shell_quote_arg};

/// The toolchain line, or `None` when the status is current or absent (an older daemon).
pub(crate) fn toolchain_staleness_line(
    status: Option<&ToolchainStaleness>,
    request_path: &str,
) -> Option<String> {
    let (head, reindex) = match status? {
        ToolchainStaleness::Current => return None,
        ToolchainStaleness::Stale {
            differences,
            reindex,
        } => {
            let parts: Vec<String> = differences.iter().map(describe_difference).collect();
            (
                format!(
                    "index toolchain differs from running rmap ({})",
                    parts.join(", ")
                ),
                reindex,
            )
        }
        ToolchainStaleness::Unknown { reason, reindex } => {
            (format!("toolchain status unknown ({reason})"), reindex)
        }
    };
    let remedy = format!("run rmap repo rebuild {}", shell_quote_arg(request_path));
    let suffix = match reindex {
        ReindexState::Queued => "re-index queued".to_string(),
        ReindexState::Running => "re-indexing in the background".to_string(),
        // The disabled reason (opt-out or stdio) reaches JSON consumers only; the line names the
        // remedy that exists on this build.
        ReindexState::Disabled { .. } => remedy,
        ReindexState::Failed { failure } => format!("re-index failed ({failure}); {remedy}"),
    };
    Some(format!("{head} \u{2014} {suffix}"))
}

/// `<component> <snapshot_version|absent> → <current_version|absent>`.
fn describe_difference(d: &ComponentDifference) -> String {
    let side = |v: &Option<String>| match v {
        Some(version) => version.clone(),
        None => "absent".to_string(),
    };
    format!(
        "{} {} \u{2192} {}",
        d.component,
        side(&d.snapshot_version),
        side(&d.current_version)
    )
}

/// Push one `check` condition row (`  - CODE: summary`) and, directly after an `INDEX_DRIFT` row,
/// the toolchain line. Returns `true` iff the toolchain line was pushed.
pub(crate) fn push_check_row(
    out: &mut String,
    code: &str,
    summary: &str,
    toolchain_line: Option<&str>,
) -> bool {
    out.push_str(&bullet(&format!("{code}: {summary}")));
    match (toolchain_line, code) {
        (Some(line), "INDEX_DRIFT") => {
            out.push_str(&bullet(line));
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::check::{check_exit_code, render_check_envelope_at, CheckResponse};
    use repo_graph_coherence::CoherenceEnvelope;
    use serde_json::{json, Value};

    const REPO: &str = "/work/my repo";

    fn one() -> Vec<ComponentDifference> {
        vec![ComponentDifference {
            component: "cpp-core".to_string(),
            snapshot_version: Some("0.0.0".to_string()),
            current_version: Some("0.1.0".to_string()),
        }]
    }

    fn stale(reindex: ReindexState) -> ToolchainStaleness {
        ToolchainStaleness::Stale {
            differences: one(),
            reindex,
        }
    }

    fn line(status: &ToolchainStaleness, path: &str) -> String {
        toolchain_staleness_line(Some(status), path).expect("a line")
    }

    #[test]
    fn stale_queued_line_names_each_component_and_the_queued_suffix() {
        let status = ToolchainStaleness::Stale {
            differences: vec![
                ComponentDifference {
                    component: "cpp-core".to_string(),
                    snapshot_version: Some("0.1.0".to_string()),
                    current_version: Some("0.2.0".to_string()),
                },
                ComponentDifference {
                    component: "kotlin-core".to_string(),
                    snapshot_version: None,
                    current_version: Some("0.1.0".to_string()),
                },
                ComponentDifference {
                    component: "indexer".to_string(),
                    snapshot_version: Some("indexer-x".to_string()),
                    current_version: None,
                },
            ],
            reindex: ReindexState::Queued,
        };
        assert_eq!(
            line(&status, REPO),
            "index toolchain differs from running rmap (cpp-core 0.1.0 \u{2192} 0.2.0, \
             kotlin-core absent \u{2192} 0.1.0, indexer indexer-x \u{2192} absent) \u{2014} re-index queued"
        );
    }

    #[test]
    fn stale_running_line_has_the_background_suffix() {
        assert_eq!(
            line(&stale(ReindexState::Running), REPO),
            "index toolchain differs from running rmap (cpp-core 0.0.0 \u{2192} 0.1.0) \u{2014} \
             re-indexing in the background"
        );
    }

    #[test]
    fn disabled_line_names_the_shell_quoted_rebuild_command() {
        for reason in [
            "RMAP_AUTO_REINDEX=off",
            "stdio transport \u{2014} a background re-index cannot outlive the request",
        ] {
            let l = line(
                &stale(ReindexState::Disabled {
                    reason: reason.to_string(),
                }),
                REPO,
            );
            assert_eq!(
                l,
                "index toolchain differs from running rmap (cpp-core 0.0.0 \u{2192} 0.1.0) \u{2014} \
                 run rmap repo rebuild '/work/my repo'"
            );
            // The disabled reason reaches JSON consumers only; the line names no store.
            assert!(!l.contains(reason), "{l}");
            assert!(!l.contains(".db"), "{l}");
        }
        // A path of safe characters is left bare, as every printed cursor is.
        assert!(line(
            &stale(ReindexState::Disabled {
                reason: "RMAP_AUTO_REINDEX=off".to_string()
            }),
            "/work/leveldb"
        )
        .ends_with("\u{2014} run rmap repo rebuild /work/leveldb"));
    }

    #[test]
    fn failed_line_names_the_failure_reason_and_the_rebuild_command() {
        assert_eq!(
            line(
                &stale(ReindexState::Failed {
                    failure: "repo_path does not exist or is not a directory: /work/my repo"
                        .to_string()
                }),
                REPO
            ),
            "index toolchain differs from running rmap (cpp-core 0.0.0 \u{2192} 0.1.0) \u{2014} \
             re-index failed (repo_path does not exist or is not a directory: /work/my repo); \
             run rmap repo rebuild '/work/my repo'"
        );
    }

    #[test]
    fn unknown_line_names_the_unreadable_reason_with_each_suffix() {
        let reason = "this index has no toolchain stamp";
        let cases = [
            (ReindexState::Queued, "re-index queued".to_string()),
            (
                ReindexState::Running,
                "re-indexing in the background".to_string(),
            ),
            (
                ReindexState::Disabled {
                    reason: "RMAP_AUTO_REINDEX=off".to_string(),
                },
                "run rmap repo rebuild '/work/my repo'".to_string(),
            ),
            (
                ReindexState::Failed {
                    failure: "boom".to_string(),
                },
                "re-index failed (boom); run rmap repo rebuild '/work/my repo'".to_string(),
            ),
        ];
        for (reindex, suffix) in cases {
            let status = ToolchainStaleness::Unknown {
                reason: reason.to_string(),
                reindex,
            };
            assert_eq!(
                line(&status, REPO),
                format!("toolchain status unknown ({reason}) \u{2014} {suffix}")
            );
        }
    }

    #[test]
    fn current_and_absent_render_no_line() {
        assert_eq!(
            toolchain_staleness_line(Some(&ToolchainStaleness::Current), REPO),
            None
        );
        assert_eq!(toolchain_staleness_line(None, REPO), None);
    }

    /// A check result JSON (the daemon's `CoherenceEnvelope` wire shape) with one verdict signal
    /// whose evidence carries `conditions`, and an optional `toolchain_staleness` member.
    fn check_json(code: &str, evidence: Value, toolchain: Option<Value>) -> Value {
        let mut value = json!({
            "schema": "rgr.agent.v1",
            "command": "check",
            "repo": "repo_1",
            "display_name": "leveldb",
            "snapshot": "snap-1",
            "confidence": "high",
            "signals": [{
                "value": {
                    "code": code,
                    "severity": "low",
                    "category": "gate",
                    "summary": "verdict",
                    "evidence": evidence
                },
                "provenance": { "source": ["sqlite"] },
                "trust": { "class": "Exact", "completeness": "Complete" },
                "freshness": "Fresh"
            }]
        });
        if let Some(t) = toolchain {
            value["toolchain_staleness"] = t;
        }
        json!({
            "value": value,
            "provenance": { "source": ["sqlite"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        })
    }

    fn render_check(result: &Value, path: &str) -> String {
        let env: CoherenceEnvelope<CheckResponse> =
            serde_json::from_value(result.clone()).expect("check envelope decodes");
        render_check_envelope_at(&env, path)
    }

    fn stale_disabled_json() -> Value {
        json!({
            "state": "stale",
            "differences": [{"component":"cpp-core","snapshot_version":"0.0.0","current_version":"0.1.0"}],
            "reindex": "disabled",
            "reindex_disabled_reason": "RMAP_AUTO_REINDEX=off"
        })
    }

    #[test]
    fn check_renders_the_toolchain_line_directly_after_the_index_drift_row() {
        let expected_line = "  - index toolchain differs from running rmap (cpp-core 0.0.0 \u{2192} 0.1.0) \u{2014} run rmap repo rebuild '/work/my repo'";
        // INDEX_DRIFT in the incomplete section (a drifted tree).
        let incomplete = json!({
            "incomplete_conditions": [
                {"code":"INDEX_DRIFT","status":"incomplete","summary":"HEAD is 1 commit ahead"},
                {"code":"UNPARSED_FILES","status":"incomplete","summary":"2 unparsed"}
            ],
            "passing": [{"code":"SNAPSHOT_EXISTS","status":"pass","summary":"Snapshot exists."}]
        });
        // INDEX_DRIFT among the passing conditions of a PASS verdict.
        let passing = json!({
            "conditions": [
                {"code":"SNAPSHOT_EXISTS","status":"pass","summary":"Snapshot exists."},
                {"code":"INDEX_DRIFT","status":"pass","summary":"working tree clean"},
                {"code":"CALL_GRAPH_RELIABLE","status":"pass","summary":"95% resolved"}
            ]
        });
        for (code, evidence) in [("CHECK_INCOMPLETE", incomplete), ("CHECK_PASS", passing)] {
            let out = render_check(
                &check_json(code, evidence, Some(stale_disabled_json())),
                REPO,
            );
            let lines: Vec<&str> = out.lines().collect();
            let at = lines
                .iter()
                .position(|l| l.starts_with("  - INDEX_DRIFT: "))
                .expect("drift row");
            assert_eq!(lines[at + 1], expected_line, "{out}");
            assert_eq!(out.matches("index toolchain differs").count(), 1, "{out}");
        }
        // No INDEX_DRIFT row renders → directly under the Verdict line.
        let no_drift = json!({
            "conditions": [{"code":"SNAPSHOT_EXISTS","status":"pass","summary":"Snapshot exists."}]
        });
        let out = render_check(
            &check_json("CHECK_PASS", no_drift, Some(stale_disabled_json())),
            REPO,
        );
        let lines: Vec<&str> = out.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("Verdict: "))
            .expect("verdict");
        assert_eq!(lines[at + 1], expected_line, "{out}");
    }

    #[test]
    fn check_verdict_and_exit_code_are_unchanged_by_toolchain_staleness() {
        let evidence = json!({
            "fail_conditions": [{"code":"GATE_PASS","status":"fail","summary":"gate failing"}],
            "incomplete_conditions": [{"code":"INDEX_DRIFT","status":"incomplete","summary":"HEAD is 1 commit ahead"}],
            "passing": [{"code":"SNAPSHOT_EXISTS","status":"pass","summary":"Snapshot exists."}]
        });
        for code in ["CHECK_PASS", "CHECK_FAIL", "CHECK_INCOMPLETE"] {
            let without = check_json(code, evidence.clone(), None);
            let base = render_check(&without, REPO);
            for status in [
                json!({"state":"current"}),
                stale_disabled_json(),
                json!({"state":"unknown","reason":"this index has no toolchain stamp","reindex":"failed","reindex_failure":"boom"}),
                json!({"state":"stale","differences":[{"component":"indexer","snapshot_version":"0.9.0","current_version":"1.0.0"}],"reindex":"running"}),
            ] {
                let with = check_json(code, evidence.clone(), Some(status.clone()));
                assert_eq!(
                    check_exit_code(&with),
                    check_exit_code(&without),
                    "{status}"
                );
                let out = render_check(&with, REPO);
                let strip = |s: &str| -> Vec<String> {
                    s.lines()
                        .filter(|l| {
                            !l.contains("index toolchain differs")
                                && !l.contains("toolchain status unknown")
                        })
                        .map(str::to_string)
                        .collect()
                };
                // Identical Verdict line and conditions; the only difference is the one line.
                assert_eq!(strip(&out), strip(&base), "{status}");
                let verdict = |s: &str| {
                    s.lines()
                        .find(|l| l.starts_with("Verdict: "))
                        .map(str::to_string)
                };
                assert_eq!(verdict(&out), verdict(&base));
                let extra = out.lines().count() - base.lines().count();
                assert_eq!(extra, usize::from(status["state"] != "current"), "{out}");
            }
        }
    }

    #[test]
    fn remedy_names_the_request_path_of_a_repository_subdirectory() {
        let sub = "/work/my repo/sub dir";
        let status = stale_disabled_json();
        let out = render_check(
            &check_json(
                "CHECK_PASS",
                json!({"conditions": []}),
                Some(status.clone()),
            ),
            sub,
        );
        assert!(
            out.contains("run rmap repo rebuild '/work/my repo/sub dir'"),
            "{out}"
        );
        let orient_json = json!({
            "value": {
                "schema": "rgr.agent.v1", "command": "orient", "repo": "repo_1",
                "snapshot": "snap-1",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "high",
                "signals": [], "limits": [], "next": [], "truncated": false,
                "index_drift": { "state": "clean", "basis": "7ee830d02b623e8f" },
                "toolchain_staleness": status
            },
            "provenance": { "source": ["sqlite"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        });
        let env: CoherenceEnvelope<crate::presentation::orient::OrientResponse> =
            serde_json::from_value(orient_json).unwrap();
        let out = crate::presentation::orient::render_orient_envelope_at(
            &env,
            crate::presentation::orient::OrientDepth::Small,
            sub,
        );
        assert!(
            out.contains("run rmap repo rebuild '/work/my repo/sub dir'"),
            "{out}"
        );
    }
}
