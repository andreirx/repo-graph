//! Doctor summary framing: the cwd snapshot verdict, the health-vs-snapshot status line, and the
//! per-probe display tone.
//!
//! **Abstraction note (per repo structural guardrail):** extracted from the parent `doctor` module
//! because the CONTRADICTION-SWEEP-1 §1 verdict/tone code pushed `doctor/mod.rs` past the 500-line
//! guardrail (review-1 #5). One concrete current caller: [`super::execute_doctor`] /
//! [`super::print_human_output`]. Axis of variation: none claimed — a cohesion/size split, not a
//! variation seam. Rejected alternative: leaving it inline in `doctor/mod.rs` (keeps the file over the
//! structural-guardrail limit, which the slice forbids).
//!
//! These items reach into the parent's private [`super::ProbeOutput`] / [`super::Summary`] — the
//! doctor-local DTOs they refine — so they stay `pub(super)` and child-of-`doctor` rather than moving
//! the DTOs too.

use crate::daemon_client::DaemonClient;

use super::{ProbeOutput, Summary};

/// The cwd repo's `check` verdict for doctor's summary (CONTRADICTION-SWEEP-1 §1).
///
/// review-1 #4: NOT serialized — doctor's JSON contract is unchanged. This drives ONLY the human
/// summary line (`super::print_human_output`), which is why the type is a plain domain struct with no
/// `Serialize` derive and the parent field is `#[serde(skip)]`.
#[derive(Debug)]
pub(super) struct SnapshotVerdict {
    pub repo: String,
    pub check: SnapshotCheck,
}

/// The verdict outcome for the cwd repo's snapshot. Two mutually-exclusive states:
/// - `Verdict` — the verdict word from `rmap check`'s OWN exit-code mapping (PASS/FAIL/INCOMPLETE), so
///   doctor cannot disagree with `rmap check` on one snapshot.
/// - `Unavailable` — an honest unknown-WITH-REASON for a *resolvable* cwd whose verdict READ failed
///   (review-1 #2: daemon unreachable / malformed reply). Never a fabricated verdict, and — per the
///   STANDING HONESTY RULE — never silently dropped: only a genuinely unresolvable cwd (no path) or a
///   not-indexed cwd (the io-NotFound analog — there IS no snapshot) omits the clause entirely.
#[derive(Debug)]
pub(super) enum SnapshotCheck {
    Verdict(&'static str),
    Unavailable(String),
}

/// CONTRADICTION-SWEEP-1 §1: the cwd repo's `check` verdict for doctor's summary line.
///
/// Returns:
/// - `None` (clause OMITTED) — the cwd is not a resolvable indexed repo: either the path itself cannot
///   be determined (`current_dir`/`canonicalize`/`file_name` fail = genuinely unresolvable cwd), or the
///   repo is simply NOT INDEXED (the io-NotFound analog — there is no snapshot, so there is no verdict
///   to name). An honest omission, never a fabricated verdict.
/// - `Some(SnapshotCheck::Verdict(_))` — the cwd resolved and `check` returned a verdict, derived from
///   the SAME [`check_exit_code`](crate::presentation::check::check_exit_code) mapping `rmap check`
///   uses (0→PASS, 1→FAIL, 2/other→INCOMPLETE), so the two cannot disagree on one snapshot.
/// - `Some(SnapshotCheck::Unavailable(reason))` — the cwd resolved to a repo but the verdict READ
///   FAILED for a reason OTHER than not-indexed (daemon unreachable / malformed reply). review-1 #2:
///   this renders an explicit "unavailable (reason)" clause rather than being silently suppressed with
///   `.ok()?`; a failed read is unknown, not absent.
///
/// A daemon-down state is therefore surfaced twice-honestly: `authority_policy` already FAILS (the
/// health verdict flips), AND this clause names the snapshot verdict as unavailable rather than hiding
/// it.
pub(super) fn cwd_check_verdict() -> Option<SnapshotVerdict> {
    // Genuinely unresolvable cwd → omit (no repo to name).
    let repo_path = std::env::current_dir().ok()?.canonicalize().ok()?;
    let repo = repo_path.file_name()?.to_string_lossy().to_string();

    // The cwd IS resolvable from here on: a failure to READ the verdict is unknown-WITH-REASON, not an
    // omission (review-1 #2). Only a not-indexed repo (no snapshot exists) collapses back to omission.
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            return Some(SnapshotVerdict {
                repo,
                check: SnapshotCheck::Unavailable(format!("daemon unavailable: {e}")),
            })
        }
    };
    let params = serde_json::json!({ "repo": repo_path.to_string_lossy() });
    let result = match client.request("check", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("{e}");
            // Not-indexed is legitimate ABSENCE (io-NotFound analog), not a read failure → omit, exactly
            // as `storage_summary_probes` treats the same signal.
            if msg.contains("not indexed") {
                return None;
            }
            return Some(SnapshotVerdict {
                repo,
                check: SnapshotCheck::Unavailable(format!("check read failed: {msg}")),
            });
        }
    };
    let word = match crate::presentation::check::check_exit_code(&result) {
        0 => "PASS",
        1 => "FAIL",
        _ => "INCOMPLETE",
    };
    Some(SnapshotVerdict {
        repo,
        check: SnapshotCheck::Verdict(word),
    })
}

/// The doctor human-output marker for a probe. Three tones, mutually exclusive: `Fail` (a real health
/// failure, `[FAIL]`), `Note` (a healthy-but-degraded advisory, `[note]`), `Ok` (`[ok]`). Kept separate
/// from `ProbeResult.passed` (health) because a degraded-yet-passing outcome needs a distinct marker
/// WITHOUT flipping the health verdict — the enrichment-`[ok]`-on-a-0-promotion-pass defect (§1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ProbeTone {
    #[default]
    Ok,
    Note,
    Fail,
}

impl ProbeTone {
    /// The bracketed marker rendered before the probe label.
    pub(super) fn marker(self) -> &'static str {
        match self {
            ProbeTone::Ok => "ok",
            ProbeTone::Note => "note",
            ProbeTone::Fail => "FAIL",
        }
    }
}

/// §1: a completed-but-0-promotion enrichment pass is healthy (stays counted as `passed`) yet must
/// render `[note]`, not `[ok]` — the marker the operator saw wrongly show green on a 0/881-promotion
/// pass. Applied here, over the built `ProbeOutput` list, so the wiring is unit-tested without a daemon
/// round-trip.
pub(super) fn apply_degraded_enrichment_tone(
    probes: &mut [ProbeOutput],
    enrichment_degraded: bool,
) {
    if enrichment_degraded {
        if let Some(p) = probes.iter_mut().find(|p| p.name == "enrichment") {
            p.tone = ProbeTone::Note;
        }
    }
}

/// §1: the doctor summary line. Frames the verdict as DAEMON/INSTALL health ("daemon healthy (N/N
/// checks)") and NAMES the cwd repo's separate `check` verdict (snapshot quality) in its own clause when
/// resolvable — omitted honestly otherwise. Pure, so the exact contract wording is unit-tested without a
/// daemon round-trip.
pub(super) fn status_line(summary: &Summary, verdict: &Option<SnapshotVerdict>) -> String {
    let verdict_clause = match verdict {
        Some(v) => {
            let check = match &v.check {
                SnapshotCheck::Verdict(w) => (*w).to_string(),
                SnapshotCheck::Unavailable(reason) => format!("unavailable ({reason})"),
            };
            format!("; snapshot verdicts: {} check {}", v.repo, check)
        }
        None => String::new(),
    };
    if summary.healthy {
        format!(
            "Status: daemon healthy ({}/{} checks){}",
            summary.passed, summary.total, verdict_clause
        )
    } else {
        format!(
            "Status: daemon UNHEALTHY ({}/{} checks failed){}",
            summary.failed, summary.total, verdict_clause
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::ProbeResult;

    fn summary(passed: usize, failed: usize) -> Summary {
        Summary {
            total: passed + failed,
            passed,
            failed,
            healthy: failed == 0,
        }
    }

    // §1: doctor frames its verdict as DAEMON/INSTALL health and NAMES the cwd repo's check verdict in
    // its own clause — the two are no longer conflated (doctor "healthy" no longer implies a clean check).
    #[test]
    fn status_line_names_snapshot_check_verdict_when_resolvable() {
        let line = status_line(
            &summary(28, 0),
            &Some(SnapshotVerdict {
                repo: "glamCRM".to_string(),
                check: SnapshotCheck::Verdict("FAIL"),
            }),
        );
        assert_eq!(
            line,
            "Status: daemon healthy (28/28 checks); snapshot verdicts: glamCRM check FAIL"
        );
        // The old conflating "healthy (N/N checks passed)" framing is gone.
        assert!(!line.contains("checks passed"), "{line}");
    }

    // §1: an UNHEALTHY daemon still names the snapshot verdict separately (the two axes are independent).
    #[test]
    fn status_line_unhealthy_still_names_verdict() {
        let line = status_line(
            &summary(26, 2),
            &Some(SnapshotVerdict {
                repo: "amodx".to_string(),
                check: SnapshotCheck::Verdict("PASS"),
            }),
        );
        assert_eq!(
            line,
            "Status: daemon UNHEALTHY (2/28 checks failed); snapshot verdicts: amodx check PASS"
        );
    }

    // §1: unresolvable/not-indexed cwd → the clause is OMITTED honestly, never a fabricated verdict.
    #[test]
    fn status_line_omits_verdict_when_cwd_unresolvable() {
        let line = status_line(&summary(28, 0), &None);
        assert_eq!(line, "Status: daemon healthy (28/28 checks)");
        assert!(!line.contains("snapshot verdicts"), "{line}");
    }

    // review-1 #2: a RESOLVABLE cwd whose verdict READ failed renders an explicit "unavailable (reason)"
    // clause — the failure is surfaced as unknown-with-reason, NOT silently suppressed.
    #[test]
    fn status_line_names_unavailable_verdict_with_reason() {
        let line = status_line(
            &summary(27, 1),
            &Some(SnapshotVerdict {
                repo: "glamCRM".to_string(),
                check: SnapshotCheck::Unavailable(
                    "daemon unavailable: connection refused".to_string(),
                ),
            }),
        );
        assert_eq!(
            line,
            "Status: daemon UNHEALTHY (1/28 checks failed); snapshot verdicts: glamCRM check unavailable (daemon unavailable: connection refused)"
        );
    }

    // §1: the display tone drives the marker; a healthy-but-degraded probe renders [note], not [ok].
    #[test]
    fn probe_tone_markers_are_distinct() {
        assert_eq!(ProbeTone::Ok.marker(), "ok");
        assert_eq!(ProbeTone::Note.marker(), "note");
        assert_eq!(ProbeTone::Fail.marker(), "FAIL");
    }

    // §1: the degraded flag flips ONLY the enrichment probe to [note], and only when set — the
    // health-passed enrichment probe stays passed (its `passed` bit is untouched; only the marker moves).
    #[test]
    fn degraded_flag_marks_only_enrichment_note() {
        let mut probes: Vec<ProbeOutput> = vec![
            ProbeResult::pass("enrichment", "resolved 700/881, promoted 0").into(),
            ProbeResult::pass("storage", "db: 1 GB").into(),
        ];
        apply_degraded_enrichment_tone(&mut probes, true);
        let enrich = probes.iter().find(|p| p.name == "enrichment").unwrap();
        assert_eq!(enrich.tone, ProbeTone::Note, "degraded enrichment → [note]");
        assert!(
            enrich.passed,
            "still health-passed (a degraded yield is not ill-health)"
        );
        assert_eq!(
            probes.iter().find(|p| p.name == "storage").unwrap().tone,
            ProbeTone::Ok,
            "other probes are untouched"
        );

        // Not degraded → enrichment stays [ok].
        let mut probes: Vec<ProbeOutput> =
            vec![ProbeResult::pass("enrichment", "resolved 81/100, promoted 40").into()];
        apply_degraded_enrichment_tone(&mut probes, false);
        assert_eq!(probes[0].tone, ProbeTone::Ok);
    }
}
