//! CYCLES-COMPLETENESS-AUDIT-1: the BASELINE PROVIDER for the module-import-cycle completeness
//! certificate. Computes a [`BaselineInput`](repo_graph_livegraph::module_cycle_cert::BaselineInput) from
//! CURRENT-TRUTH sources AT THE AUDIT BOUNDARY (SQLite language inventory + filesystem tsconfig discovery),
//! so the PURE evaluator (which stays SQLite-free) can return something other than `UnknownBaselineMissing`.
//!
//! THE BOUNDARY (ratified): this module MAY read SQLite + the filesystem (it runs at a diagnostic boundary,
//! never per query). The certificate EVALUATOR
//! ([`evaluate_module_cycle_completeness`](repo_graph_livegraph::module_cycle_cert::evaluate_module_cycle_completeness))
//! consumes the produced [`BaselineInput`] and the in-memory snapshot only -- it NEVER touches SQLite. The
//! audit response below is READ-ONLY: it discovers + reads + evaluates; it does NOT refresh/load partitions
//! (the caller loads them first via `rmap dev livegraph-refresh`). It is NOT a default migration.
//!
//! Non-TS rule (D3-A, ratified): any non-null `files.language` outside the TS family is a non-TS code
//! source -> `has_non_ts_cycle_source = true` (CONSERVATIVE; never a false `Complete`). The narrower
//! "import-bearing non-TS files" refinement is the recorded follow-up CYCLES-COMPLETENESS-LANGUAGE-PRECISION-1.

use crate::state::RepoState;
use repo_graph_livegraph::module_cycle_cert::{
    certificate_inputs_fingerprint, evaluate_module_cycle_completeness, BaselineInput,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// The language-support policy version (the LiveGraph supports the TS family today). Bump when the
/// supported-language set changes -> every certificate re-evaluates (rides in the inputs fingerprint).
pub const LANGUAGE_SUPPORT_VERSION: u32 = 1;

/// The import-completeness policy version: which import classes the policy treats as
/// uncaptured/cycle-relevant. Bump when that policy changes (e.g. a future IMPORTS-PACKAGE-RESOLUTION-1
/// declares resolved package imports non-cycle-relevant) -> every certificate re-evaluates.
pub const IMPORT_COMPLETENESS_POLICY_VERSION: u32 = 1;

/// The LiveGraph-supported TS family (D3-A). A non-null `files.language` value OUTSIDE this set is a non-TS
/// CODE source: the indexer vocabulary (`indexer/routing.rs::detect_language`) is CLOSED + code-only, so
/// there is no doc/data/"unknown" value to exclude -- a non-null value is always a real code language.
const TS_FAMILY: &[&str] = &["typescript", "tsx", "javascript", "jsx"];

/// Classify the SQLite language inventory: `(has_non_ts_cycle_source, sorted distinct non-TS languages)`.
/// PURE. CONSERVATIVE: ANY non-TS-family code language -> `true` (the TS-only LiveGraph cannot have covered
/// it, so the module-cycle graph may be incomplete). Never produces a false `Complete`.
fn classify_non_ts_languages(languages: &[String]) -> (bool, Vec<String>) {
    let mut non_ts: Vec<String> = languages
        .iter()
        .filter(|l| !TS_FAMILY.contains(&l.as_str()))
        .cloned()
        .collect();
    non_ts.sort();
    non_ts.dedup();
    (!non_ts.is_empty(), non_ts)
}

/// Deterministic 64-bit FNV-1a over the snapshot_uid -> the `repo_index_epoch` the certificate fingerprint
/// uses for invalidation. Deterministic ACROSS processes (unlike std `DefaultHasher`'s randomized
/// `RandomState`), so the fingerprint is stable for a given index and changes iff the snapshot changes
/// (a re-index produces a new snapshot_uid -> a new epoch -> a busted certificate).
fn index_epoch_from_snapshot(snapshot_uid: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in snapshot_uid.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Build the [`BaselineInput`] from the discovered TS partition set + the SQLite language inventory + the
/// snapshot epoch + the policy versions. PURE given its inputs (the SQLite/filesystem reads happen in the
/// caller, at the boundary). Exposed for the audit response + tests.
fn build_baseline(
    expected_partition_ids: BTreeSet<String>,
    languages: &[String],
    snapshot_uid: &str,
) -> (BaselineInput, Vec<String>) {
    let (has_non_ts, non_ts_languages) = classify_non_ts_languages(languages);
    let baseline = BaselineInput {
        expected_partition_ids,
        has_non_ts_cycle_source: has_non_ts,
        repo_index_epoch: index_epoch_from_snapshot(snapshot_uid),
        language_support_version: LANGUAGE_SUPPORT_VERSION,
        import_completeness_policy_version: IMPORT_COMPLETENESS_POLICY_VERSION,
    };
    (baseline, non_ts_languages)
}

/// READ-ONLY module-cycle completeness audit (the dev diagnostic). Discovers the expected TS partition set
/// (filesystem), reads the SQLite language inventory (audit-time), snapshots the CURRENT LiveGraph, runs the
/// SQLite-free evaluator, and reports the certificate + the evidence (observed languages + D3-B SQLite
/// module-cycle corroboration). Does NOT refresh/load partitions; does NOT change any default.
pub fn cycle_completeness_audit_response(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
    repo_root: &str,
    include_fixtures: bool,
) -> Result<Value, String> {
    // 1. SHARED discovery (ENUMERATION-1 D1/D2): the EXPECTED TS partition set (fixture-excluded unless
    //    --include-fixtures). The SAME function `livegraph-refresh --all-discovered` loads from -> the
    //    expected set and the load plan cannot drift. (repo-relative roots -> partition ids.)
    let discovered =
        crate::partition_discovery::discover_partition_roots(repo_root, include_fixtures);
    let expected_partition_ids: BTreeSet<String> = discovered
        .included
        .iter()
        .map(|sr| crate::livegraph_refresh::derive_partition_target(repo_root, sr).1)
        .collect();
    // The EXCLUDED fixture tsconfigs (repo-relative dir + reason) -- surfaced so an exclusion is never silent.
    let excluded_fixture_partitions: Vec<Value> = discovered
        .excluded
        .iter()
        .map(|(dir, reason)| json!({ "dir": dir, "reason": reason }))
        .collect();

    // 2. SQLite language inventory (D3-A): non-TS evidence (audit-time read; NOT the evaluator).
    let languages = repo_state
        .storage
        .distinct_file_languages(repo_uid)
        .map_err(|e| e.to_string())?;
    let (baseline, non_ts_languages) =
        build_baseline(expected_partition_ids.clone(), &languages, snapshot_uid);

    // 3. The PURE in-memory snapshot (read-only) + the LiveGraph module-cycle count (for corroboration).
    let (snapshot, livegraph_module_cycle_count) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => (
                lg.module_cycle_live_state(),
                lg.module_import_cycles()
                    .data()
                    .map(|d| d.cycles.len())
                    .unwrap_or(0),
            ),
            None => (Default::default(), 0),
        }
    };

    // 4. The SQLite-free evaluator + the invalidation fingerprint.
    let certificate = evaluate_module_cycle_completeness(&snapshot, Some(&baseline));
    let fingerprint = certificate_inputs_fingerprint(&snapshot, Some(&baseline));

    // 5. D3-B corroboration (NOT an evaluator input): does SQLite see MORE module cycles than the
    //    TS-only LiveGraph? If so AND non-TS code is present, the non-TS completeness risk is corroborated.
    let sqlite_module_cycle_count = repo_state
        .storage
        .find_cycles(snapshot_uid, "module")
        .map_err(|e| e.to_string())?
        .len();
    let non_ts_corroborated = baseline.has_non_ts_cycle_source
        && sqlite_module_cycle_count > livegraph_module_cycle_count;

    let loaded_fresh_set: BTreeSet<&str> = snapshot
        .partitions
        .iter()
        .filter(|p| p.fresh)
        .map(|p| p.id.as_str())
        .collect();
    let loaded_fresh: Vec<String> = loaded_fresh_set.iter().map(|s| s.to_string()).collect();
    // B (ratified): the EXPECTED partitions not loaded+fresh -- the explicit reason behind an
    // `IncompleteMissingPartitions` headline (precedence runs missing-partitions BEFORE unsupported-language,
    // so this is surfaced ALONGSIDE the non-TS evidence, never instead of it).
    let missing_expected_partitions: Vec<String> = expected_partition_ids
        .iter()
        .filter(|e| !loaded_fresh_set.contains(e.as_str()))
        .cloned()
        .collect();
    let o = &snapshot.observation_classes;

    Ok(json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "kind": "module-import",
        "certificate": certificate.as_str(),
        "permits_livegraph_default": certificate.permits_livegraph_default(),
        "certificate_inputs_fingerprint": fingerprint,
        "baseline": {
            "expected_partition_ids": expected_partition_ids,
            "has_non_ts_cycle_source": baseline.has_non_ts_cycle_source,
            "repo_index_epoch": baseline.repo_index_epoch,
            "language_support_version": LANGUAGE_SUPPORT_VERSION,
            "import_completeness_policy_version": IMPORT_COMPLETENESS_POLICY_VERSION,
        },
        "evidence": {
            "observed_languages": languages,
            "non_ts_languages": non_ts_languages,
            "loaded_fresh_partitions": loaded_fresh,
            "missing_expected_partitions": missing_expected_partitions,
            "excluded_fixture_partitions": excluded_fixture_partitions,
            "observation_classes": {
                "has_package_external": o.has_package_external,
                "has_dynamic": o.has_dynamic,
                "has_unresolved_after_overlay": o.has_unresolved_after_overlay,
            },
            "sqlite_module_cycle_count": sqlite_module_cycle_count,
            "livegraph_module_cycle_count": livegraph_module_cycle_count,
            "non_ts_corroborated_by_sqlite_cycles": non_ts_corroborated,
        },
        "note": "read-only audit; the certificate evaluator is SQLite-free; this is NOT a default migration",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_pure_ts_repo_is_not_non_ts() {
        let langs = vec![
            "typescript".to_string(),
            "tsx".to_string(),
            "javascript".to_string(),
            "jsx".to_string(),
        ];
        let (has_non_ts, non_ts) = classify_non_ts_languages(&langs);
        assert!(!has_non_ts, "TS family alone is not non-TS");
        assert!(non_ts.is_empty());
    }

    #[test]
    fn classify_mixed_repo_flags_non_ts_code() {
        let langs = vec![
            "typescript".to_string(),
            "rust".to_string(),
            "c".to_string(),
        ];
        let (has_non_ts, non_ts) = classify_non_ts_languages(&langs);
        assert!(has_non_ts, "a non-TS code language -> non-TS source");
        assert_eq!(non_ts, vec!["c".to_string(), "rust".to_string()]);
    }

    #[test]
    fn classify_empty_inventory_is_not_non_ts() {
        let (has_non_ts, non_ts) = classify_non_ts_languages(&[]);
        assert!(!has_non_ts);
        assert!(non_ts.is_empty());
    }

    #[test]
    fn index_epoch_is_deterministic_and_snapshot_sensitive() {
        let a = index_epoch_from_snapshot("repo/2026-01-01/abc");
        let b = index_epoch_from_snapshot("repo/2026-01-01/abc");
        let c = index_epoch_from_snapshot("repo/2026-01-02/def");
        assert_eq!(a, b, "same snapshot -> same epoch (stable fingerprint)");
        assert_ne!(a, c, "a re-index (new snapshot_uid) -> a new epoch");
    }

    #[test]
    fn build_baseline_carries_non_ts_and_epoch() {
        let expected: BTreeSet<String> = ["packages/a".to_string(), "packages/b".to_string()]
            .into_iter()
            .collect();
        let (baseline, non_ts) = build_baseline(
            expected.clone(),
            &["typescript".to_string(), "rust".to_string()],
            "snap-1",
        );
        assert_eq!(baseline.expected_partition_ids, expected);
        assert!(baseline.has_non_ts_cycle_source);
        assert_eq!(non_ts, vec!["rust".to_string()]);
        assert_eq!(
            baseline.repo_index_epoch,
            index_epoch_from_snapshot("snap-1")
        );
        assert_eq!(baseline.language_support_version, LANGUAGE_SUPPORT_VERSION);
        assert_eq!(
            baseline.import_completeness_policy_version,
            IMPORT_COMPLETENESS_POLICY_VERSION
        );
    }
}
