//! Shared history-diagnosis DTO for the quantity surfaces (`churn` / `hotspots`
//! / `risk`).
//!
//! CHURN-SHALLOW-1 §2: the daemon diagnoses the repo's git-history shape at query
//! time and ships it in an additive `history` block. This is the CLI-side view of
//! that block — a tagged union mirroring `repo_graph_git::HistoryShape` plus an
//! `Unknown` cell for a failed git read (honesty rule #1: a failed read is stated
//! with its reason, never coerced to a guessed shape).
//!
//! The type is deliberately NOT the git-crate domain enum: the daemon→CLI boundary
//! carries raw JSON DTOs, and this presenter owns the reader-facing wording. Three
//! concrete callers share it (churn/hotspots/risk presenters) so the framing can
//! never drift across the three surfaces.

use serde::Deserialize;

/// The history shape as the quantity surfaces receive it on the wire.
///
/// Internally tagged by `kind`; variant names are snake_case to match the daemon's
/// `diagnose_history_json`. A FIXED taxonomy — adding a cell breaks the exhaustive
/// matches in [`Self::cascade_line`] and `churn::ChurnResponse::render_human`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryDiagnosis {
    /// Multi-commit, non-shallow history with activity in the window — count means
    /// what it says; no caveat.
    Healthy,
    /// Shallow clone or single-commit history: the count is a snapshot / import, not
    /// windowed activity. `commits_in_window` distinguishes the two shapes the real
    /// fixtures exhibit — `> 0` (VCMI: a recent depth-1 clone whose whole tree lands
    /// in-window → reframe the count as the import) vs `0` (a stale shallow clone whose
    /// only commit predates the window → state the newest date + `git fetch --unshallow`,
    /// NOT `--since`). `head_commit_date` is that newest commit.
    ShallowOrSingle {
        commits_available: u64,
        is_shallow: bool,
        head_commit_date: String,
        commits_in_window: u64,
    },
    /// Real history, zero commits in the window; the determinable cause + a concrete
    /// `--since` widening.
    ZeroInWindow {
        head_commit_date: String,
        suggested_since: String,
    },
    /// No commits at all (unborn repo / not a git repo).
    NoHistory,
    /// A git read failed — diagnosis unknown, with the reason.
    Unknown { reason: String },
}

impl HistoryDiagnosis {
    /// CHURN-SHALLOW-1 §2.2: the ONE-sentence inherited-diagnosis line for the
    /// cascading surfaces (`hotspots` / `risk`), naming the churn history they
    /// inherit. `None` for [`Self::Healthy`] (nothing to caveat). `window_label` is
    /// the human window (e.g. "last 90 days").
    pub fn cascade_line(&self, window_label: &str) -> Option<String> {
        match self {
            HistoryDiagnosis::Healthy => None,
            HistoryDiagnosis::ShallowOrSingle {
                commits_available,
                is_shallow,
                head_commit_date,
                commits_in_window,
            } => {
                let noun = commit_noun(*commits_available);
                Some(if *is_shallow {
                    if *commits_in_window > 0 {
                        if *commits_available > 1 {
                            // Depth>1 shallow (review-1 #1): genuine recent changes on a
                            // truncated history — the ranking is real, just incomplete. NOT
                            // an imported snapshot, NOT "no recent activity".
                            format!(
                                "history is shallow — {commits_available} {noun} available; the \
                                 ranking reflects only this shallow clone's commits (older \
                                 history truncated), not the full record"
                            )
                        } else {
                            // Depth-1 clone: the single whole-tree commit IS the import.
                            format!(
                                "history is shallow — {commits_available} {noun} available (clone \
                                 depth limits churn); the ranking reflects the imported snapshot, \
                                 not recent activity"
                            )
                        }
                    } else {
                        format!(
                            "history is shallow — {commits_available} {noun} available (newest: \
                             {head_commit_date}); nothing in the {window_label}, and clone depth \
                             is the limit — git fetch --unshallow for real history"
                        )
                    }
                } else if *commits_in_window > 0 {
                    "history has a single commit — the ranking reflects the initial import, \
                     not recent activity"
                        .to_string()
                } else {
                    format!(
                        "the repository has a single commit ({head_commit_date}), before the \
                         {window_label}; the ranking has no recent activity"
                    )
                })
            }
            HistoryDiagnosis::ZeroInWindow {
                head_commit_date,
                suggested_since,
            } => Some(format!(
                "no churn in the {window_label} (HEAD commit: {head_commit_date}) — the \
                 ranking has no recent activity; try --since {suggested_since}"
            )),
            HistoryDiagnosis::NoHistory => {
                Some("no git history available — the ranking has no churn input".to_string())
            }
            HistoryDiagnosis::Unknown { reason } => Some(format!(
                "history diagnosis unknown ({reason}) — the ranking's churn basis could \
                 not be established"
            )),
        }
    }

    /// CHURN-SHALLOW-1 §2.1: the churn summary block — everything between the "File
    /// Churn (…)" header and the file table. Replaces the bare count line + the old
    /// either/or hedge with a shape-appropriate frame:
    ///   - shallow/single: a caveat + the count reframed as the import snapshot;
    ///   - zero-in-window: the count + the determinable cause and a `--since` nudge;
    ///   - no-history: the count + "no git history available";
    ///   - unknown: the count + unknown-with-reason (never the hedge);
    ///   - healthy: the plain count line (unchanged framing).
    ///
    /// A file table still follows iff `count > 0` (the caller's rule) — for the
    /// shallow/single cell the import files ARE shown, under the honest frame.
    pub fn churn_summary(&self, count: usize, window_label: &str) -> String {
        let files = if count == 1 { "file" } else { "files" };
        match self {
            HistoryDiagnosis::Healthy => format!("{count} {files} changed\n"),
            HistoryDiagnosis::ShallowOrSingle {
                commits_available,
                is_shallow,
                head_commit_date,
                commits_in_window,
            } => {
                let noun = commit_noun(*commits_available);
                if *commits_in_window > 0 {
                    if *is_shallow && *commits_available > 1 {
                        // Depth>1 shallow clone (review-1 #1): the in-window commits are
                        // GENUINE recent changes — NOT a whole-tree import snapshot. The
                        // only honest caveat is that older history is truncated, so the
                        // count reflects ONLY the commits this shallow clone actually holds.
                        // Never "imported snapshot", never "not recent activity".
                        format!(
                            "history is shallow — {commits_available} {noun} available; older \
                             history is truncated, so the count below reflects only this shallow \
                             clone's commits, not the full record — git fetch --unshallow for \
                             complete churn history\n\
                             \n{count} {files} changed (shallow history)\n"
                        )
                    } else {
                        // commits_available == 1: a single whole-tree commit IS the import
                        // snapshot — reframe it, never "N files changed" as recent churn. The
                        // import files still follow in the table. is_shallow → depth-1 clone
                        // (advise unshallow); else → a genuine single-commit repo.
                        let caveat = if *is_shallow {
                            format!(
                                "history is shallow — {commits_available} {noun} available; this \
                                 clone's depth limits churn to the imported snapshot, not recent \
                                 activity\n"
                            )
                        } else {
                            "the repository has a single commit — the whole tree is its initial \
                             import, not recent change\n"
                                .to_string()
                        };
                        let advice = if *is_shallow {
                            " — git fetch --unshallow for real history"
                        } else {
                            ""
                        };
                        format!(
                            "{caveat}\n{count} {files} in the imported snapshot \
                             (not {window_label} churn{advice})\n"
                        )
                    }
                } else {
                    // Nothing in the window (django/leveldb/…): state the newest commit +
                    // the CORRECT next action. `--since` is withheld here — for a shallow
                    // clone it would misleadingly imply deeper history is reachable.
                    let body = if *is_shallow {
                        format!(
                            "\nhistory is shallow — {commits_available} {noun} available \
                             (newest: {head_commit_date}); clone depth, not a quiet window, is \
                             why nothing changed in the {window_label}. Run `git fetch \
                             --unshallow` for full churn history.\n"
                        )
                    } else {
                        format!(
                            "\nthe repository has a single commit ({head_commit_date}); it falls \
                             before the {window_label}.\n"
                        )
                    };
                    format!("{count} {files} changed\n{body}")
                }
            }
            HistoryDiagnosis::ZeroInWindow {
                head_commit_date,
                suggested_since,
            } => format!(
                "{count} {files} changed\n\nno files changed in the {window_label} \
                 (HEAD commit: {head_commit_date}) — try --since {suggested_since}\n"
            ),
            HistoryDiagnosis::NoHistory => format!(
                "{count} {files} changed\n\nno git history available (the repository has \
                 no commits).\n"
            ),
            HistoryDiagnosis::Unknown { reason } => {
                let mut s = format!("{count} {files} changed\n");
                if count == 0 {
                    s.push_str(&format!(
                        "\nhistory diagnosis unknown ({reason}) — cannot say whether this is \
                         a quiet window or a truncated history.\n"
                    ));
                } else {
                    s.push_str(&format!("\nnote: history diagnosis unknown ({reason}).\n"));
                }
                s
            }
        }
    }
}

/// "commit" / "commits" for a count.
fn commit_noun(n: u64) -> &'static str {
    if n == 1 {
        "commit"
    } else {
        "commits"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shallow with the imported snapshot IN the window (VCMI shape).
    fn shallow_in_window(is_shallow: bool, n: u64) -> HistoryDiagnosis {
        HistoryDiagnosis::ShallowOrSingle {
            commits_available: n,
            is_shallow,
            head_commit_date: "2026-08-01".to_string(),
            commits_in_window: 1,
        }
    }

    /// Shallow depth>1 with in-window commits (a truncated-but-real history).
    fn shallow_multi_in_window(n: u64) -> HistoryDiagnosis {
        HistoryDiagnosis::ShallowOrSingle {
            commits_available: n,
            is_shallow: true,
            head_commit_date: "2026-08-30".to_string(),
            commits_in_window: 3,
        }
    }

    /// Shallow with the only commit OUT of the window (django shape).
    fn shallow_out_of_window(n: u64, head: &str) -> HistoryDiagnosis {
        HistoryDiagnosis::ShallowOrSingle {
            commits_available: n,
            is_shallow: true,
            head_commit_date: head.to_string(),
            commits_in_window: 0,
        }
    }

    #[test]
    fn deserializes_each_kind() {
        let cases = [
            r#"{"kind":"healthy"}"#,
            r#"{"kind":"no_history"}"#,
            r#"{"kind":"shallow_or_single","commits_available":1,"is_shallow":true,"head_commit_date":"2026-08-01","commits_in_window":1}"#,
            r#"{"kind":"zero_in_window","head_commit_date":"2024-03-15","suggested_since":"2024-03-15"}"#,
            r#"{"kind":"unknown","reason":"git absent"}"#,
        ];
        for c in cases {
            serde_json::from_str::<HistoryDiagnosis>(c).unwrap_or_else(|e| panic!("{c}: {e}"));
        }
    }

    #[test]
    fn churn_shallow_in_window_reframes_count_not_as_recent_churn() {
        // VCMI shape: the whole-tree import must NOT read as 90-day churn.
        let out = shallow_in_window(true, 1).churn_summary(2072, "last 90 days");
        assert!(out.contains("history is shallow"), "{out}");
        assert!(out.contains("1 commit available"), "{out}");
        assert!(out.contains("2072 files in the imported snapshot"), "{out}");
        assert!(out.contains("not last 90 days churn"), "{out}");
        assert!(out.contains("git fetch --unshallow"), "{out}");
        // The misleading "N files changed" framing is gone.
        assert!(!out.contains("2072 files changed"), "{out}");
    }

    #[test]
    fn churn_shallow_out_of_window_gives_unshallow_advice_not_since() {
        // django/leveldb shape (measured): shallow depth-1, the only commit predates
        // the window → 0 files. The correct next action is `unshallow`, NOT `--since`
        // (which would misleadingly imply deeper history is reachable by widening).
        let out = shallow_out_of_window(1, "2026-05-08").churn_summary(0, "last 90 days");
        assert!(out.contains("0 files changed"), "{out}");
        assert!(out.contains("history is shallow"), "{out}");
        assert!(out.contains("newest: 2026-05-08"), "{out}");
        assert!(out.contains("git fetch --unshallow"), "{out}");
        assert!(
            !out.contains("--since"),
            "no misleading --since for a shallow clone: {out}"
        );
        assert!(!out.contains("or no git history available"), "{out}");
    }

    #[test]
    fn churn_shallow_depth_gt1_in_window_is_truncated_not_import() {
        // review-1 #1: a depth>1 shallow clone with in-window commits holds GENUINE
        // recent changes. It must NOT be reframed as an "imported snapshot" or called
        // "not recent activity" — only that the count reflects the truncated history.
        let out = shallow_multi_in_window(12).churn_summary(40, "last 90 days");
        assert!(out.contains("history is shallow"), "{out}");
        assert!(out.contains("12 commits available"), "{out}");
        assert!(out.contains("40 files changed (shallow history)"), "{out}");
        assert!(out.contains("git fetch --unshallow"), "{out}");
        // The depth-1 "import snapshot" framing must NOT appear for a real depth>1 history.
        assert!(!out.contains("imported snapshot"), "{out}");
        assert!(!out.contains("not recent activity"), "{out}");
        assert!(!out.contains("initial import"), "{out}");
    }

    #[test]
    fn cascade_shallow_depth_gt1_in_window_is_truncated_not_import() {
        // review-1 #1: hotspots/risk inherit the same honest depth>1 framing.
        let line = shallow_multi_in_window(12)
            .cascade_line("last 90 days")
            .unwrap();
        assert!(line.contains("history is shallow"), "{line}");
        assert!(line.contains("12 commits available"), "{line}");
        assert!(line.contains("truncated"), "{line}");
        assert!(!line.contains("imported snapshot"), "{line}");
        assert!(!line.contains("not recent activity"), "{line}");
    }

    #[test]
    fn churn_single_commit_in_window_framing() {
        let out = shallow_in_window(false, 1).churn_summary(500, "last 90 days");
        assert!(out.contains("single commit"), "{out}");
        assert!(out.contains("500 files in the imported snapshot"), "{out}");
        // A genuine single-commit repo IS its full history — no unshallow advice.
        assert!(!out.contains("unshallow"), "{out}");
    }

    #[test]
    fn churn_zero_in_window_states_cause_and_suggestion() {
        // django shape: the either/or hedge is replaced by the determinable cause.
        let out = HistoryDiagnosis::ZeroInWindow {
            head_commit_date: "2024-03-15".into(),
            suggested_since: "2024-03-15".into(),
        }
        .churn_summary(0, "last 90 days");
        assert!(out.contains("0 files changed"), "{out}");
        assert!(out.contains("HEAD commit: 2024-03-15"), "{out}");
        assert!(out.contains("try --since 2024-03-15"), "{out}");
        assert!(!out.contains("or no git history available"), "{out}");
    }

    #[test]
    fn churn_no_history_states_it() {
        let out = HistoryDiagnosis::NoHistory.churn_summary(0, "last 90 days");
        assert!(out.contains("no git history available"), "{out}");
        assert!(!out.contains(" or "), "no either/or hedge: {out}");
    }

    #[test]
    fn churn_unknown_names_reason_not_hedge() {
        let out = HistoryDiagnosis::Unknown {
            reason: "git command failed".into(),
        }
        .churn_summary(0, "last 90 days");
        assert!(
            out.contains("history diagnosis unknown (git command failed)"),
            "{out}"
        );
    }

    #[test]
    fn churn_healthy_is_plain_count() {
        let out = HistoryDiagnosis::Healthy.churn_summary(3, "last 90 days");
        assert_eq!(out, "3 files changed\n");
    }

    #[test]
    fn cascade_healthy_is_none_others_some() {
        assert!(HistoryDiagnosis::Healthy
            .cascade_line("last 90 days")
            .is_none());
        assert!(shallow_in_window(true, 1)
            .cascade_line("last 90 days")
            .unwrap()
            .contains("shallow"));
        // Out-of-window shallow cascade names unshallow, not --since.
        let oow = shallow_out_of_window(1, "2026-05-08")
            .cascade_line("last 90 days")
            .unwrap();
        assert!(oow.contains("unshallow"), "{oow}");
        assert!(!oow.contains("--since"), "{oow}");
        assert!(HistoryDiagnosis::NoHistory
            .cascade_line("last 90 days")
            .unwrap()
            .contains("no git history"));
        let z = HistoryDiagnosis::ZeroInWindow {
            head_commit_date: "2024-03-15".into(),
            suggested_since: "2024-03-15".into(),
        };
        assert!(z
            .cascade_line("last 90 days")
            .unwrap()
            .contains("--since 2024-03-15"));
    }
}
