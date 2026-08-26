//! Index-basis / working-tree drift DTO (INDEX-BASIS-1).
//!
//! Repo-graph owns the structure of the *last indexed commit*; git owns the
//! delta. `IndexDrift` is the query-time answer to "which commit do these facts
//! describe, and how far has the working tree moved past it?" — the fact that
//! makes the honesty claim true instead of a footer that says "fresh" while an
//! agent has edited 30 files since the index.
//!
//! This is a **domain sum type**: the states are mutually exclusive, and each
//! variant carries only the data valid in that state. A `bool is_drifted` plus
//! nullable `commits_ahead` / `basis` fields would collapse
//! *not-a-git-repo* / *basis-never-stamped* / *git-error* / *clean* / *drifted*
//! into one ambiguous shape — the exact defect-shaped type the honesty rules
//! forbid. Consumers `match` exhaustively (check's INDEX_DRIFT condition; the
//! orient/explain footer); adding a variant deliberately breaks every match.
//!
//! Pure data + pure render helpers. The daemon (composition root) constructs
//! instances from git + storage; this crate never performs I/O.

use serde::{Deserialize, Serialize};

/// The query-time relationship between the indexed basis commit and the current
/// working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum IndexDrift {
    /// The repo path is not a git repository, so there is no basis commit to
    /// anchor to and no drift to measure. Not a degradation — drift tracking is
    /// simply not applicable (like an un-versioned working directory).
    NotGit,

    /// The snapshot predates basis tracking: `basis_commit` was never stamped,
    /// but the path IS a git repo now. Drift is genuinely UNKNOWN until the next
    /// `rmap refresh` stamps the basis.
    BasisUnknown,

    /// Git could not compute drift, so it is genuinely UNKNOWN — never silently
    /// coerced to "clean". Two shapes fold in here: a recorded basis whose drift
    /// could not be computed (basis sha missing after a history rewrite, or a git
    /// call failed → `basis: Some`), and a git probe that failed before the basis
    /// was even consulted (git absent/unrunnable → `basis` is whatever was
    /// recorded, possibly `None`). `basis: None` renders "index basis: unknown"; a
    /// failed read is never papered over as `NotGit`/Pass.
    Unknown {
        basis: Option<String>,
        reason: String,
    },

    /// A basis is recorded and the working tree matches it: zero commits ahead,
    /// zero changed files. The facts describe the current tree.
    Clean { basis: String },

    /// The working tree has moved past the basis. `files_changed` (M) is every
    /// changed path THAT IS THE READER'S (rmap's own `map` sidecars and OS noise
    /// are excluded upstream and counted in `excluded`); `indexed_changed` (K ≤ M)
    /// is how many of those are files the index actually tracks; `modules` names
    /// the modules those K files belong to (may be empty when module data is
    /// unavailable — an honest omission, not a claim of "no modules"). `excluded`
    /// is the count of changed paths dropped as rmap's own exhaust / OS noise, so
    /// the reader can see what was ignored rather than have it silently hidden
    /// (SELF-POLLUTION-1 §2.2).
    ///
    /// `unreadable` (⊆ `files_changed`) is the count of sidecar-NAMED changed paths
    /// we could NOT read to check the `rmap map` marker (a genuine read failure —
    /// permission/IO — NOT a `NotFound`, which means the file is simply gone). The
    /// honesty rule forbids asserting such a file is "not generated" from a failed
    /// read, so it is COUNTED as the reader's change (the conservative direction)
    /// AND surfaced ("+N unreadable, counted") rather than silently folded in
    /// (operator RULING 3, closing review-3's `.ok()`-collapse finding). Additive
    /// field (`#[serde(default)]`): absent in older JSON → 0.
    Drifted {
        basis: String,
        commits_ahead: u64,
        files_changed: u64,
        indexed_changed: u64,
        modules: Vec<String>,
        /// Additive (review-6 #1): a legacy `drifted` payload without this
        /// field must still deserialize — default 0 means "none excluded".
        #[serde(default)]
        excluded: u64,
        #[serde(default)]
        unreadable: u64,
    },
}

impl IndexDrift {
    /// The abbreviated (7-char) basis sha when one is recorded, else `None`.
    pub fn basis_sha7(&self) -> Option<String> {
        let full: Option<&str> = match self {
            IndexDrift::Unknown { basis, .. } => basis.as_deref(),
            IndexDrift::Clean { basis } | IndexDrift::Drifted { basis, .. } => Some(basis.as_str()),
            IndexDrift::NotGit | IndexDrift::BasisUnknown => None,
        };
        full.map(sha7)
    }

    /// The single reader-facing description of basis + drift + next action. ONE
    /// wording home shared by check's `INDEX_DRIFT` condition summary and the
    /// orient/explain serving footer, so the two surfaces can never diverge.
    pub fn describe(&self) -> String {
        match self {
            IndexDrift::NotGit => {
                "index basis: not a git repo — working-tree drift not tracked".to_string()
            }
            IndexDrift::BasisUnknown => "index basis: unknown (indexed before basis tracking) — \
                 run `rmap refresh` to stamp it"
                .to_string(),
            IndexDrift::Unknown { reason, .. } => {
                // A failed read → unknown WITH the reason; when no basis was ever
                // recorded, say so honestly rather than render an empty sha.
                let basis = match self.basis_sha7() {
                    Some(sha7) => sha7,
                    None => "unknown".to_string(),
                };
                format!(
                    "index basis: {basis}; working-tree drift unknown ({reason}) — \
                     run `rmap refresh`"
                )
            }
            // `Clean`/`Drifted` carry a NON-optional `basis` (a recorded sha), so we
            // destructure it directly and truncate — no `unwrap_or_default()` that
            // could silently render an empty sha for an absent basis (honesty rule #1:
            // a rendered fact is never a silent empty fallback). Absence has exactly
            // one representation and it lives in the `Unknown`/`BasisUnknown` variants,
            // handled above.
            IndexDrift::Clean { basis } => format!(
                "index basis: {}; working tree clean (no drift since index)",
                sha7(basis)
            ),
            IndexDrift::Drifted {
                basis,
                commits_ahead,
                files_changed,
                indexed_changed,
                modules,
                excluded,
                unreadable,
            } => {
                let sha7 = sha7(basis);
                // What we ignored, said out loud (never silently hidden).
                let excluded_clause = if *excluded > 0 {
                    format!(" (+{excluded} tool/OS file{} ignored)", plural(*excluded))
                } else {
                    String::new()
                };
                // Sidecar-named changes we could not read to check the marker: kept in
                // the count (conservative) but flagged, never silently asserted
                // "not generated" (operator RULING 3).
                let unreadable_clause = if *unreadable > 0 {
                    format!(" (+{unreadable} unreadable, counted)")
                } else {
                    String::new()
                };
                if *indexed_changed == 0 {
                    // K=0 calibration (SELF-POLLUTION-1 §2.2): NO indexed file
                    // moved, so the index facts still describe the tracked tree.
                    // This is a Pass (see `makes_check_incomplete`) — informational,
                    // never a `run rmap refresh` nag.
                    format!(
                        "index basis: {sha7}; {files_changed} working-tree file{} changed, \
                         none indexed{excluded_clause}{unreadable_clause} — index facts current",
                        plural(*files_changed),
                    )
                } else {
                    let modules_clause = if modules.is_empty() {
                        String::new()
                    } else {
                        format!(", modules {}", modules.join(", "))
                    };
                    format!(
                        "index basis: {sha7}; HEAD is {commits_ahead} commit{} ahead, \
                         {files_changed} file{} changed in the working tree \
                         ({indexed_changed} indexed{modules_clause}){excluded_clause}\
                         {unreadable_clause} — run `rmap refresh` to re-anchor",
                        plural(*commits_ahead),
                        plural(*files_changed),
                    )
                }
            }
        }
    }

    /// Whether this drift state should make `check`'s `INDEX_DRIFT` condition
    /// `Incomplete` (informational — the facts may be behind the working tree),
    /// as opposed to `Pass`.
    ///
    /// - `Clean` → Pass (facts describe the tree).
    /// - `NotGit` → Pass (drift tracking not applicable — like gate-not-configured
    ///   or enrichment-not-applicable, an honest "no concern here", not unknown).
    /// - `Drifted` with `indexed_changed == 0` → Pass (K=0 calibration,
    ///   SELF-POLLUTION-1 §2.2): NO indexed file moved, so the facts still
    ///   describe the tracked tree. Working-tree changes to non-indexed files (or
    ///   ignored tool/OS exhaust) do not stale the index.
    /// - `Drifted` with `indexed_changed > 0` / `BasisUnknown` / `Unknown` →
    ///   Incomplete (either measured drift of indexed files, or an unknown we must
    ///   not paper over as Pass).
    ///
    /// Never `Fail` by itself (INDEX-BASIS-1 §2.4).
    pub fn makes_check_incomplete(&self) -> bool {
        match self {
            IndexDrift::Drifted {
                indexed_changed, ..
            } => *indexed_changed > 0,
            IndexDrift::BasisUnknown | IndexDrift::Unknown { .. } => true,
            IndexDrift::Clean { .. } | IndexDrift::NotGit => false,
        }
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// The abbreviated 7-char form of a full commit sha, for display. Pure; total on
/// any input (short shas pass through unchanged).
fn sha7(full: &str) -> String {
    full.chars().take(7).collect()
}

#[cfg(test)]
mod tests {
    // review-6 #1: BOTH additive fields must default so a legacy serialized
    // `drifted` payload (pre-`excluded`/`unreadable`) still deserializes.
    #[test]
    fn legacy_drifted_payload_without_additive_fields_deserializes() {
        let legacy = serde_json::json!({
            "state": "drifted",
            "basis": "abc1234",
            "commits_ahead": 1,
            "files_changed": 3,
            "indexed_changed": 2,
            "modules": ["core"]
        });
        let d: IndexDrift =
            serde_json::from_value(legacy).expect("legacy payload must deserialize");
        match d {
            IndexDrift::Drifted {
                excluded,
                unreadable,
                ..
            } => {
                assert_eq!(
                    excluded, 0,
                    "absent additive field defaults to none-excluded"
                );
                assert_eq!(unreadable, 0);
            }
            other => panic!("expected Drifted, got {other:?}"),
        }
    }

    use super::*;

    #[test]
    fn sha7_truncates_only_when_present() {
        assert_eq!(IndexDrift::NotGit.basis_sha7(), None);
        assert_eq!(IndexDrift::BasisUnknown.basis_sha7(), None);
        assert_eq!(
            IndexDrift::Clean {
                basis: "abcdef0123456789".to_string()
            }
            .basis_sha7()
            .as_deref(),
            Some("abcdef0")
        );
    }

    #[test]
    fn clean_and_notgit_pass_others_incomplete() {
        assert!(!IndexDrift::Clean {
            basis: "a".to_string()
        }
        .makes_check_incomplete());
        assert!(!IndexDrift::NotGit.makes_check_incomplete());
        assert!(IndexDrift::BasisUnknown.makes_check_incomplete());
        assert!(IndexDrift::Unknown {
            basis: Some("a".to_string()),
            reason: "boom".to_string()
        }
        .makes_check_incomplete());
        assert!(IndexDrift::Drifted {
            basis: "a".to_string(),
            commits_ahead: 1,
            files_changed: 3,
            indexed_changed: 3,
            modules: vec![],
            excluded: 0,
            unreadable: 0,
        }
        .makes_check_incomplete());
    }

    #[test]
    fn k0_indexed_changed_is_pass_with_informational_line() {
        // SELF-POLLUTION-1 §2.2: 0 INDEXED files changed → Pass (never Incomplete),
        // even though working-tree files moved (here all excluded as rmap's exhaust).
        let k0 = IndexDrift::Drifted {
            basis: "abcdef0123456789".to_string(),
            commits_ahead: 0,
            files_changed: 0,
            indexed_changed: 0,
            modules: vec![],
            excluded: 3_721,
            unreadable: 0,
        };
        assert!(
            !k0.makes_check_incomplete(),
            "K=0 drift must be a Pass, not Incomplete"
        );
        let s = k0.describe();
        assert!(s.contains("none indexed"), "informational K=0 line: {s}");
        assert!(
            s.contains("+3721 tool/OS files ignored"),
            "excluded count surfaced, not silently hidden: {s}"
        );
        assert!(
            !s.contains("rmap refresh"),
            "K=0 is informational, never a refresh nag: {s}"
        );
    }

    #[test]
    fn drifted_reports_excluded_count_on_the_refresh_line() {
        // When indexed files DID change, the line is still the actionable refresh
        // nag, but the excluded tool/OS count is appended (never silently hidden).
        let d = IndexDrift::Drifted {
            basis: "abcdef0123456789".to_string(),
            commits_ahead: 1,
            files_changed: 2,
            indexed_changed: 1,
            modules: vec!["src".to_string()],
            excluded: 5,
            unreadable: 0,
        };
        let s = d.describe();
        assert!(s.contains("(1 indexed, modules src)"), "{s}");
        assert!(s.contains("+5 tool/OS files ignored"), "{s}");
        assert!(s.contains("rmap refresh"), "{s}");
        assert!(d.makes_check_incomplete(), "K>0 stays Incomplete");
    }

    #[test]
    fn drifted_surfaces_unreadable_count_never_silently_folds_it() {
        // operator RULING 3: a sidecar-named change we could not READ (a genuine
        // read failure, not NotFound) is COUNTED as the reader's change AND
        // surfaced "+N unreadable, counted" — never silently asserted "not
        // generated". Rides both the K=0 informational line and the K>0 refresh line.
        let k0 = IndexDrift::Drifted {
            basis: "abcdef0123456789".to_string(),
            commits_ahead: 0,
            files_changed: 2,
            indexed_changed: 0,
            modules: vec![],
            excluded: 3,
            unreadable: 2,
        };
        let s = k0.describe();
        assert!(s.contains("+3 tool/OS files ignored"), "{s}");
        assert!(s.contains("+2 unreadable, counted"), "{s}");
        assert!(s.contains("none indexed"), "{s}");
        assert!(!k0.makes_check_incomplete(), "K=0 stays Pass: {s}");

        let k1 = IndexDrift::Drifted {
            basis: "abcdef0123456789".to_string(),
            commits_ahead: 1,
            files_changed: 3,
            indexed_changed: 1,
            modules: vec!["src".to_string()],
            excluded: 0,
            unreadable: 1,
        };
        let t = k1.describe();
        assert!(t.contains("+1 unreadable, counted"), "{t}");
        assert!(t.contains("rmap refresh"), "{t}");

        // Zero unreadable → no clause (never a "+0 unreadable" noise line).
        let none = IndexDrift::Drifted {
            basis: "abcdef0123456789".to_string(),
            commits_ahead: 1,
            files_changed: 1,
            indexed_changed: 1,
            modules: vec![],
            excluded: 0,
            unreadable: 0,
        };
        assert!(
            !none.describe().contains("unreadable"),
            "{}",
            none.describe()
        );
    }

    #[test]
    fn describe_drifted_singular_plural_and_modules() {
        let one = IndexDrift::Drifted {
            basis: "abcdef01234".to_string(),
            commits_ahead: 1,
            files_changed: 1,
            indexed_changed: 1,
            modules: vec!["src/git".to_string()],
            excluded: 0,
            unreadable: 0,
        };
        let s = one.describe();
        assert!(s.contains("index basis: abcdef0"), "{s}");
        assert!(s.contains("1 commit ahead"), "{s}");
        assert!(s.contains("1 file changed"), "{s}");
        assert!(s.contains("(1 indexed, modules src/git)"), "{s}");
        assert!(s.contains("rmap refresh"), "{s}");

        let many = IndexDrift::Drifted {
            basis: "abcdef01234".to_string(),
            commits_ahead: 3,
            files_changed: 12,
            indexed_changed: 4,
            modules: vec![],
            excluded: 0,
            unreadable: 0,
        };
        let m = many.describe();
        assert!(m.contains("3 commits ahead"), "{m}");
        assert!(m.contains("12 files changed"), "{m}");
        assert!(m.contains("(4 indexed)"), "{m}");
        assert!(!m.contains("modules"), "no modules clause when empty: {m}");
    }

    #[test]
    fn describe_clean_renders_the_recorded_basis_never_empty() {
        let s = IndexDrift::Clean {
            basis: "abcdef0123456789".to_string(),
        }
        .describe();
        assert_eq!(
            s, "index basis: abcdef0; working tree clean (no drift since index)",
            "{s}"
        );
        // Regression guard for review-3 #2: the basis sha is rendered, never a silent
        // empty-string fallback.
        assert!(!s.contains("index basis: ;"), "no empty-sha fallback: {s}");
    }

    #[test]
    fn describe_unknown_and_basis_unknown_state_the_reason() {
        assert!(IndexDrift::BasisUnknown
            .describe()
            .contains("indexed before basis tracking"));
        assert!(IndexDrift::NotGit.describe().contains("not a git repo"));
        let u = IndexDrift::Unknown {
            basis: Some("abcdef01234".to_string()),
            reason: "basis sha not found".to_string(),
        };
        assert!(
            u.describe().contains("index basis: abcdef0"),
            "{}",
            u.describe()
        );
        assert!(u.describe().contains("drift unknown (basis sha not found)"));

        // Git-probe failure with no recorded basis → "index basis: unknown", never
        // an empty sha, and never coerced to clean/not-git.
        let no_basis = IndexDrift::Unknown {
            basis: None,
            reason: "git could not be probed".to_string(),
        };
        assert!(
            no_basis.describe().contains("index basis: unknown"),
            "{}",
            no_basis.describe()
        );
        assert!(no_basis
            .describe()
            .contains("drift unknown (git could not be probed)"));
    }

    #[test]
    fn serde_round_trip_is_tagged() {
        let d = IndexDrift::Drifted {
            basis: "abc".to_string(),
            commits_ahead: 2,
            files_changed: 5,
            indexed_changed: 3,
            modules: vec!["m".to_string()],
            excluded: 7,
            unreadable: 0,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"state\":\"drifted\""), "{json}");
        let back: IndexDrift = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
