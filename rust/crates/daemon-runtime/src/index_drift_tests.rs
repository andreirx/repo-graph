//! Unit tests for `index_drift` (split out to keep the module under the 500-line
//! structural guardrail — operator RULING 3). Included via `#[path]` as a submodule of
//! `index_drift`, so `use super::*` reaches the private items under test.

use super::*;

/// Build a diagnostics blob string carrying the `index_basis` record (plus, optionally,
/// pre-existing keys) — the shape compose persists and `parse_basis_outcome` reads back.
fn blob_with(outcome: &BasisOutcome, extra: &str) -> String {
    let mut map = serde_json::Map::new();
    if !extra.is_empty() {
        let e: serde_json::Value = serde_json::from_str(extra).unwrap();
        for (k, v) in e.as_object().unwrap() {
            map.insert(k.clone(), v.clone());
        }
    }
    map.insert(
        INDEX_BASIS_DIAG_KEY.to_string(),
        serde_json::to_value(outcome).unwrap(),
    );
    serde_json::to_string(&serde_json::Value::Object(map)).unwrap()
}

#[test]
fn module_of_matches_on_path_boundary() {
    let roots = vec!["src/http".to_string(), "src".to_string()];
    let mut roots = roots;
    roots.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

    assert_eq!(
        module_of("src/http/handler.ts", &roots).as_deref(),
        Some("src/http"),
        "most-specific root wins"
    );
    assert_eq!(
        module_of("src/main.ts", &roots).as_deref(),
        Some("src"),
        "falls back to the containing root"
    );
    assert_eq!(
        module_of("lib/x.ts", &roots),
        None,
        "no root owns it → no module"
    );
}

#[test]
fn module_of_no_false_prefix() {
    let roots = vec!["src/a".to_string()];
    assert_eq!(module_of("src/ab.ts", &roots), None);
    assert_eq!(module_of("src/a", &roots).as_deref(), Some("src/a"));
    assert_eq!(module_of("src/a/b.ts", &roots).as_deref(), Some("src/a"));
}

#[test]
fn basis_none_state_maps_recorded_record_never_reprobes() {
    // operator RULING 3: the no-basis branch is resolved from the WRITE-time recorded
    // `BasisOutcome`, NOT a live HEAD re-probe. Recorded unborn Failure surfaces its
    // reason verbatim → "repository has no commits yet" (NOT the pre-slice wording —
    // closing review-4's false claim for an unborn-then-committed repo).
    let unborn = basis_none_state(Ok(Some(BasisOutcome::Failure {
        reason: "repository has no commits yet".to_string(),
    })));
    match &unborn {
        IndexDrift::Unknown { basis, reason } => {
            assert_eq!(*basis, None, "no basis to anchor to");
            assert_eq!(reason, "repository has no commits yet", "{reason}");
        }
        other => panic!("expected Unknown for a recorded unborn failure, got {other:?}"),
    }
    assert!(unborn.makes_check_incomplete(), "Incomplete, never Pass");
    assert!(
        !unborn.describe().contains("indexed before basis tracking"),
        "unborn must not be mis-attributed to pre-slice: {}",
        unborn.describe()
    );
    assert!(
        unborn.describe().contains("repository has no commits yet"),
        "{}",
        unborn.describe()
    );

    // Recorded generic HEAD-unreadable failure → Unknown surfacing the write-time reason,
    // NEVER the empty-repo wording.
    let unreadable = basis_none_state(Ok(Some(BasisOutcome::Failure {
        reason: "git HEAD unreadable at index time (fatal: detected dubious ownership in \
                 repository at '/repo')"
            .to_string(),
    })));
    match &unreadable {
        IndexDrift::Unknown { basis, reason } => {
            assert_eq!(*basis, None);
            assert!(
                reason.contains("git HEAD unreadable at index time")
                    && reason.contains("dubious ownership"),
                "the write-time reason is surfaced: {reason}"
            );
            assert!(
                !reason.contains("no commits yet"),
                "a generic failure must not claim an empty repo: {reason}"
            );
        }
        other => panic!("expected Unknown for a recorded generic failure, got {other:?}"),
    }
    assert!(unreadable.makes_check_incomplete());

    // Recorded NonGit → NotGit (the query need not re-probe; the record is authoritative).
    assert_eq!(
        basis_none_state(Ok(Some(BasisOutcome::NonGit))),
        IndexDrift::NotGit
    );

    // Recorded Basis on a NULL-basis snapshot is INCONSISTENT (compose never writes it) →
    // Unknown-with-reason, NEVER a silent clean/pre-slice.
    let inconsistent = basis_none_state(Ok(Some(BasisOutcome::Basis {
        commit: "abc123".to_string(),
    })));
    match &inconsistent {
        IndexDrift::Unknown { basis, reason } => {
            assert_eq!(*basis, None);
            assert!(
                reason.contains("inconsistent index-basis record"),
                "{reason}"
            );
        }
        other => panic!("expected Unknown for an inconsistent record, got {other:?}"),
    }
    assert_ne!(inconsistent, IndexDrift::BasisUnknown);

    // NO recorded outcome on a git repo → the snapshot predates basis tracking.
    assert_eq!(basis_none_state(Ok(None)), IndexDrift::BasisUnknown);

    // Diagnostics blob UNREADABLE → genuinely Unknown-with-reason, NEVER a false
    // BasisUnknown/clean (honesty rule #1).
    let unreadable_blob = basis_none_state(Err(
        "extraction diagnostics unreadable (database is locked)".to_string(),
    ));
    match &unreadable_blob {
        IndexDrift::Unknown { basis, reason } => {
            assert_eq!(*basis, None);
            assert!(reason.contains("database is locked"), "{reason}");
        }
        other => panic!("expected Unknown for an unreadable blob, got {other:?}"),
    }
    assert_ne!(unreadable_blob, IndexDrift::BasisUnknown);
    assert!(!unreadable_blob
        .describe()
        .contains("indexed before basis tracking"));
}

#[test]
fn basis_outcome_from_probe_classifies_non_failing_arms() {
    // The Ok(Some)/Ok(None) arms never touch the path (no positive probe needed), so a
    // dummy path suffices; the Err arm is exercised by `classify_head_failure` below.
    let dummy = Path::new(".");

    // Ok(Some) → Basis carrying the sha (and the column value).
    let basis = basis_outcome_from_probe(dummy, Ok(Some("deadbeef".to_string())));
    assert_eq!(
        basis,
        BasisOutcome::Basis {
            commit: "deadbeef".to_string()
        }
    );
    assert_eq!(basis.basis_commit().as_deref(), Some("deadbeef"));

    // Ok(None) → NonGit (recorded, and NULL column).
    let non_git = basis_outcome_from_probe(dummy, Ok(None));
    assert_eq!(non_git, BasisOutcome::NonGit);
    assert_eq!(non_git.basis_commit(), None);
}

#[test]
fn classify_head_failure_claims_unborn_only_on_positive_probe() {
    use repo_graph_git::GitError;

    // The reviewer-flagged stderr (`ambiguous argument 'HEAD': unknown revision`) is
    // emitted by BOTH an unborn repo AND a committed repo with a broken HEAD — so the
    // SAME error object drives all three arms; only the POSITIVE probe result differs.
    let head_err = GitError::CommandFailed {
        command: "git rev-parse HEAD".to_string(),
        exit_code: Some(128),
        stderr: "fatal: ambiguous argument 'HEAD': unknown revision or path not in the \
                 working tree."
            .to_string(),
    };

    // Probe POSITIVELY establishes unborn (empty commit graph) → "no commits yet".
    match classify_head_failure(&head_err, Ok(true)) {
        BasisOutcome::Failure { reason } => {
            assert_eq!(reason, "repository has no commits yet", "{reason}")
        }
        other => panic!("positive-unborn → Failure(no commits), got {other:?}"),
    }

    // Probe says the repo HAS commits (broken HEAD, NOT empty) → generic reason carrying
    // the ORIGINAL HEAD error, NEVER the empty-repo wording. This is review-9 #1: the same
    // `unknown revision` stderr must NOT be classified unborn when commits exist.
    match classify_head_failure(&head_err, Ok(false)) {
        BasisOutcome::Failure { reason } => {
            assert!(
                reason.contains("git HEAD unreadable at index time")
                    && reason.contains("unknown revision"),
                "committed-broken-HEAD → generic reason: {reason}"
            );
            assert_ne!(
                reason, "repository has no commits yet",
                "a repo WITH commits must never borrow the empty-repo wording: {reason}"
            );
        }
        other => panic!("has-commits → Failure(generic), got {other:?}"),
    }

    // The positive probe itself FAILED → we cannot establish unborn, so we must NOT claim
    // it (honest degradation) → generic reason.
    let probe_err = GitError::CommandFailed {
        command: "git rev-list -n 1 --all".to_string(),
        exit_code: Some(128),
        stderr: "fatal: unknown repository extension found".to_string(),
    };
    match classify_head_failure(&head_err, Err(probe_err)) {
        BasisOutcome::Failure { reason } => {
            assert!(
                reason.contains("git HEAD unreadable at index time"),
                "probe-failed → generic reason: {reason}"
            );
            assert!(
                !reason.contains("no commits yet"),
                "an un-establishable unborn state must NOT be claimed: {reason}"
            );
        }
        other => panic!("probe-failed → Failure(generic), got {other:?}"),
    }
}

#[test]
fn parse_basis_outcome_round_trips_and_is_honest_on_malformed() {
    // A blob carrying the record (alongside pre-existing trust keys) round-trips.
    let outcome = BasisOutcome::Failure {
        reason: "repository has no commits yet".to_string(),
    };
    let blob = blob_with(&outcome, r#"{"diagnostics_version":1,"edges_total":42}"#);
    assert!(
        blob.contains("\"edges_total\":42"),
        "sibling keys preserved: {blob}"
    );
    assert_eq!(parse_basis_outcome(Some(&blob)).unwrap(), Some(outcome));

    // A NonGit record round-trips.
    assert_eq!(
        parse_basis_outcome(Some(&blob_with(&BasisOutcome::NonGit, ""))).unwrap(),
        Some(BasisOutcome::NonGit)
    );

    // No blob / no key → Ok(None) (a genuine "no recorded outcome" = pre-slice history).
    assert_eq!(parse_basis_outcome(None).unwrap(), None);
    assert_eq!(
        parse_basis_outcome(Some(r#"{"edges_total":1}"#)).unwrap(),
        None
    );

    // Not-valid-JSON and malformed-key → Err (never silently None — honesty rule #1).
    assert!(parse_basis_outcome(Some("not-json{")).is_err());
    assert!(parse_basis_outcome(Some(r#"{"index_basis":{"outcome":"bogus"}}"#)).is_err());
}

#[test]
fn unresolved_repo_is_unknown_with_reason_never_basis_unknown() {
    // review-3 finding 1: a query-time storage MISS or READ ERROR means git was never
    // reached — it does NOT establish that the snapshot "predates basis tracking", so it
    // must render `Unknown`-with-reason, NEVER `BasisUnknown` and NEVER a false clean.
    let miss = unresolved_repo_drift(
        None,
        "repo metadata not found in storage; cannot resolve repo path to compute drift".to_string(),
    );
    assert_ne!(
        miss,
        IndexDrift::BasisUnknown,
        "a storage miss is not pre-slice history"
    );
    match &miss {
        IndexDrift::Unknown { basis, reason } => {
            assert_eq!(*basis, None);
            assert!(
                reason.contains("not found in storage"),
                "reason surfaced: {reason}"
            );
        }
        other => panic!("expected Unknown for a storage miss, got {other:?}"),
    }
    assert!(
        miss.makes_check_incomplete(),
        "Incomplete, never silently Pass"
    );
    assert!(!miss.describe().contains("indexed before basis tracking"));

    let err = unresolved_repo_drift(
        Some("abc123def456".to_string()),
        "repo metadata could not be read from storage to compute drift (database is locked)"
            .to_string(),
    );
    match err {
        IndexDrift::Unknown { basis, reason } => {
            assert_eq!(
                basis.as_deref(),
                Some("abc123def456"),
                "recorded basis carried"
            );
            assert!(
                reason.contains("database is locked"),
                "storage error preserved: {reason}"
            );
        }
        other => panic!("expected Unknown carrying the basis, got {other:?}"),
    }

    assert!(matches!(
        unresolved_repo_drift(Some(String::new()), "x".to_string()),
        IndexDrift::Unknown { basis: None, .. }
    ));
}

#[test]
fn git_reason_only_attributes_missing_revision_to_rewritten_basis() {
    use repo_graph_git::GitError;
    let missing = git_reason(&GitError::CommandFailed {
        command: "git rev-list --count deadbeef..HEAD".to_string(),
        exit_code: Some(128),
        stderr: "fatal: ambiguous argument 'deadbeef..HEAD': unknown revision or path not \
                 in the working tree."
            .to_string(),
    });
    assert!(
        missing.contains("history may have been rewritten"),
        "a missing-basis failure is attributed to a rewritten history: {missing}"
    );

    let unrelated = git_reason(&GitError::CommandFailed {
        command: "git status --porcelain -z --untracked-files=all".to_string(),
        exit_code: Some(128),
        stderr: "fatal: Unable to create '/repo/.git/index.lock': File exists.".to_string(),
    });
    assert!(
        !unrelated.contains("history may have been rewritten"),
        "an unrelated git failure must not claim a rewritten basis: {unrelated}"
    );
    assert!(
        unrelated.contains("git status --porcelain -z") && unrelated.contains("index.lock"),
        "the true failing command + stderr are surfaced: {unrelated}"
    );
}

#[test]
fn cap_modules_folds_overflow() {
    let mut set = BTreeSet::new();
    for i in 0..(MODULE_NAME_CAP + 3) {
        set.insert(format!("m{i:02}"));
    }
    let out = cap_modules(set);
    assert_eq!(out.len(), MODULE_NAME_CAP + 1);
    assert_eq!(out.last().unwrap(), "+3 more");
}
