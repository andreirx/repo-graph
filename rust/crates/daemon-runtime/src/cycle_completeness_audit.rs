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

use crate::livegraph_feed::import_cert_fingerprint;
use crate::state::RepoState;
use repo_graph_livegraph::module_cycle_cert::{
    certificate_inputs_fingerprint, evaluate_module_cycle_completeness, BaselineInput,
};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::ops::ControlFlow;

/// The language-support policy version (the LiveGraph supports the TS family today). Bump when the
/// supported-language set changes -> every certificate re-evaluates (rides in the inputs fingerprint).
pub const LANGUAGE_SUPPORT_VERSION: u32 = 1;

/// The import-completeness policy version: which import classes the policy treats as
/// uncaptured/cycle-relevant. v2 = IMPORTS-PACKAGE-RESOLUTION-1 (ExternalPackageNonLocal benign). v3 =
/// IMPORTS-TSCONFIG-PATHS-1 (a RESOLVED tsconfig path alias -> non-blocking; unresolved -> has_alias_unresolved).
/// v4 = IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1 (a node_modules/@types-resolvable external is benign, not just a
/// directly-declared dep). v5 = IMPORTS-DYNAMIC-CLASSIFICATION-1 (a LITERAL dynamic import is classified like
/// its static counterpart; only a NON-LITERAL `import(expr)` blocks). v6 = IMPORTS-ASSET-AND-LITERAL-EXT-1 (a
/// relative asset import `.css`/`.svg`/... is benign non-cycle-relevant; a literal-source-extension import
/// resolves to the exact FILE). Any bump re-evaluates every prior cert.
pub const IMPORT_COMPLETENESS_POLICY_VERSION: u32 = 6;

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
/// caller, at the boundary). Exposed (`pub(crate)`) for the audit response, the `imports --engine livegraph`
/// module-cycle completeness field (IMPORTS-LIVEGRAPH-CLI-1), and tests -- the SINGLE baseline assembly (the
/// policy versions ride here) so the two consumers cannot drift.
pub(crate) fn build_baseline(
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

/// W-B-EPOCH-IMPL-2D (AUDIT-EPOCH-WITNESS): the cycle-completeness audit's single-coherent-epoch IDENTITY.
///
/// The audit is a COMPARATOR (SQLite module cycles ⋈ LiveGraph module cycles + the SQLite language baseline),
/// NOT a serve fastpath. Its whole purpose is to run on RED/incomplete repos and report the real
/// incompleteness, so it CANNOT gate on the GREEN no-loss serve-eligibility witness
/// ([`RequestEpoch::fingerprint`](crate::livegraph_feed::RequestEpoch) / `cycles_cert_eligibility`): that
/// witness is `None` on a RED repo, which would make the audit UNAVAILABLE exactly when it is needed.
///
/// The two stores version INDEPENDENTLY, so each is pinned by a different mechanism:
/// - **SQLite** — pinned to `snapshot_uid`. Every SQLite read is snapshot-parameterized:
///   `find_cycles(snapshot_uid)` AND the language baseline via `distinct_file_languages_for_snapshot(snapshot_uid)`
///   (`file_versions ⋈ files`, W-B-EPOCH-IMPL-2D). The pin has TWO parts (precise, per review-2): the cycle rows
///   and the language FILE-SET are pinned to N BY CONSTRUCTION (snapshot-keyed rows / the immutable `file_versions`
///   manifest PK); the language VALUE per file is read from the mutable `files` table but is epoch-invariant by the
///   UPSTREAM `file_uid -> language` invariant (file_uid embeds the path; `detect_language` is path-pure — the full
///   citation chain lives on `distinct_file_languages_for_snapshot`). So the SQLite side cannot shift for a reader
///   pinned to `snapshot_uid` — it needs no witness and no check-after.
/// - **LiveGraph** — an in-memory store with NO snapshot dimension, so it cannot be query-pinned. The audit
///   instead pins a RAW resident-LiveGraph fingerprint (GREEN-or-RED) as an epoch-IDENTITY witness
///   (`resident_livegraph_fingerprint`) and verifies it has NOT moved across the whole comparison (the
///   check-after in [`cycle_completeness_audit_response`]). A move means a concurrent refresh swapped the
///   LiveGraph mid-audit, so a SQLite@N-vs-LiveGraph@N+1 comparison would be a FALSE incompleteness; the audit
///   honest-degrades instead.
///
/// DISTINCT from [`RequestEpoch`](crate::livegraph_feed::RequestEpoch) BY DESIGN: that type's `fingerprint`
/// is documented as a GREEN-eligibility witness (`Some` ONLY when a no-loss cert is GREEN at the resident
/// state) and MUST NOT hold a raw fingerprint — putting a raw fingerprint there would be a contract lie (the
/// 1st AUDIT-EPOCH attempt was blocked in review for exactly that overloading). This witness is the RAW
/// resident fingerprint regardless of any cert verdict.
///
/// Abstraction ledger:
/// - **What:** a `{ snapshot_uid, resident_livegraph_fingerprint }` value capturing the audit's TWO
///   epoch-identity inputs ONCE, at one anchor instant under the handler's read guard. (The language baseline is
///   NOT a third field: it is pinned to `snapshot_uid` by the snapshot-scoped query, read in the response.)
/// - **Concrete current users:** `handle_cycle_completeness_audit` (the capture) +
///   [`cycle_completeness_audit_response`] (the fingerprint check-after). One handler, one response fn.
/// - **Axis of variation:** epoch-IDENTITY pinning for a COMPARATOR — distinct from `RequestEpoch`'s
///   GREEN-eligibility pinning for a SERVER.
/// - **Rejected simpler:** reusing `RequestEpoch` + the GREEN `cycles_cert_eligibility` — REJECTED: it yields
///   nothing on RED, so the audit would be unavailable on the incomplete repos it exists to diagnose;
///   overloading `RequestEpoch.fingerprint` with a raw fingerprint — REJECTED: the contract lie review blocked.
/// - **Rejected for the LANGUAGE pin (review-1):** a captured-at-anchor `captured_languages` witness +
///   two-read check-after (the 2D iteration-1 design) — REJECTED: it could not bind the FIRST language read to
///   `snapshot_uid` across the resolve->capture window, so a `files` shift between the snapshot resolve and the
///   capture read could pass the two-read check yet still straddle epochs. The snapshot-scoped query pins the file
///   SET by construction (the `file_versions` manifest); the language VALUE is pinned by the upstream
///   `file_uid -> language` invariant (documented on the query). A `nodes ⋈ files` join was ALSO rejected (it
///   drops a non-TS code file with zero nodes -> a FALSE `Complete`); `file_versions ⋈ files` does NOT drop
///   no-node files (the snapshot's file manifest holds every tracked file), so it is both pinned AND conservative
///   — see `distinct_file_languages_for_snapshot`.
pub struct AuditEpoch {
    /// The pinned READY snapshot uid — EVERY SQLite read in the audit resolves to THIS: `find_cycles` AND the
    /// language baseline (`distinct_file_languages_for_snapshot`, W-B-EPOCH-IMPL-2D). The cycle rows + the language
    /// file-SET are pinned to one snapshot by construction; the language VALUE is epoch-invariant by the upstream
    /// `file_uid -> language` invariant (documented on the query). So a concurrent re-index that publishes a newer
    /// snapshot — or rewrites the repo-scoped `files` rows ON CONFLICT — cannot shift the audit's SQLite side.
    pub snapshot_uid: String,
    /// The RAW resident LiveGraph fingerprint at capture (GREEN-or-RED): `import_cert_fingerprint` over the
    /// resident partitions for same-epoch coherence, or the empty-partition fingerprint when the LiveGraph is
    /// cold. NOT gated on any no-loss cert verdict — this is an epoch-IDENTITY witness the audit checks did
    /// NOT move (the LiveGraph has no snapshot dimension to query-pin against), NOT a serve-eligibility witness.
    pub resident_livegraph_fingerprint: String,
}

impl AuditEpoch {
    /// Capture the audit's coherent-epoch identity under the caller's read guard, at one anchor instant: the
    /// pinned snapshot uid + the RAW resident LiveGraph fingerprint (GREEN-or-RED). The handler calls this once,
    /// immediately after resolving the snapshot, mirroring how the other mixed-read handlers capture their
    /// `RequestEpoch` — but with a raw-IDENTITY witness (not the GREEN eligibility witness). INFALLIBLE: it
    /// reads only the in-memory LiveGraph (the SQLite language baseline is no longer captured here — it is
    /// pinned to `snapshot_uid` by `distinct_file_languages_for_snapshot` and read in the response, so capture
    /// touches no fallible SQLite path).
    pub fn capture(repo_state: &RepoState, snapshot_uid: &str) -> AuditEpoch {
        AuditEpoch {
            snapshot_uid: snapshot_uid.to_string(),
            resident_livegraph_fingerprint: resident_livegraph_fingerprint(
                repo_state,
                snapshot_uid,
            ),
        }
    }
}

/// The RAW resident LiveGraph fingerprint for `snapshot_uid` (GREEN-or-RED): the import-cert fingerprint over
/// the resident partitions, or the empty-partition fingerprint when the LiveGraph is cold (so a cold->resident
/// transition still MOVES the fingerprint and is caught). NOT gated on any no-loss cert — it is the epoch
/// IDENTITY, computed exactly as the no-loss certs' invalidation key but read RAW (no verdict peek). Shared by
/// the handler's [`AuditEpoch::capture`] and the response's check-after so the two cannot drift.
fn resident_livegraph_fingerprint(repo_state: &RepoState, snapshot_uid: &str) -> String {
    let guard = repo_state.livegraph.read();
    match guard.as_ref() {
        Some(lg) => import_cert_fingerprint(&lg.live_partitions(), snapshot_uid),
        None => import_cert_fingerprint(&[], snapshot_uid),
    }
}

/// The HONEST-DEGRADE response when the audit could not be computed at a single coherent epoch — i.e. a
/// concurrent refresh swapped the resident LiveGraph mid-audit (the SQLite side cannot cause this: it is wholly
/// pinned to `snapshot_uid`). A DISTINCT shape — `audit_status: "incoherent_epoch"`, `audit_coherent: false`,
/// and DELIBERATELY NO `certificate` — so it can NEVER be mistaken for a real (RED/incomplete) verdict. The
/// agent's contract: retry when settled. This is the ONLY honest answer: a SQLite@N-vs-LiveGraph@N+1 comparison
/// would be a spurious incompleteness caused purely by epoch skew, which for an AUDIT is doubly bad (it both
/// violates the Fact-Certainty Model and fabricates a finding).
///
/// Surfaces the captured-vs-observed LiveGraph fingerprint so the reader sees the LiveGraph moved (it is the
/// ONLY store that can move under the captured epoch — the language baseline and the cycle rows are both pinned
/// to `snapshot_uid` by construction, so they are never the cause).
///
/// (Kept as a NAMED helper despite one caller: it names the slice's core safety output — the honest-degrade —
/// so the safety path is greppable and reviewable rather than buried as an inline `json!` mid-response.)
fn incoherent_epoch_response(
    repo_uid: &str,
    snapshot_uid: &str,
    captured_fingerprint: &str,
    observed_fingerprint: &str,
) -> Value {
    json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "kind": "module-import",
        "audit_status": "incoherent_epoch",
        "audit_coherent": false,
        "detail": "audit could not be computed at a single coherent epoch; retry when settled",
        "note": "the resident LiveGraph swapped during the audit (a concurrent refresh moved its fingerprint). Reporting a SQLite-vs-LiveGraph comparison across that straddle would be a FALSE incompleteness, so the audit honest-degrades instead of emitting a certificate. The SQLite side (cycles + language baseline) is pinned to snapshot_uid and cannot be the cause.",
        "captured_livegraph_fingerprint": captured_fingerprint,
        "observed_livegraph_fingerprint": observed_fingerprint,
    })
}

/// READ-ONLY module-cycle completeness audit (the dev diagnostic), made EPOCH-IDENTITY-COHERENT
/// (W-B-EPOCH-IMPL-2D). Discovers the expected TS partition set (filesystem), reads the SQLite language
/// inventory + the SQLite module cycles (PINNED to `epoch.snapshot_uid`), snapshots the CURRENT LiveGraph,
/// runs the SQLite-free evaluator, and reports the certificate + the evidence — but ONLY after verifying the
/// whole comparison ran at a SINGLE coherent epoch: the resident LiveGraph fingerprint must STILL equal the
/// captured identity (`epoch.resident_livegraph_fingerprint`), checked AFTER the comparison. If it moved (a concurrent refresh swapped the LiveGraph mid-audit), it HONEST-DEGRADES —
/// NEVER a FALSE incompleteness from comparing SQLite@N against LiveGraph@N+1. On a steady epoch it audits at
/// ANY fingerprint (GREEN or RED): the audit is a COMPARATOR, not a serve fastpath, so it is NOT gated on a
/// GREEN no-loss cert (that would make it unavailable on the RED/incomplete repos it exists to diagnose). Does
/// NOT refresh/load partitions; does NOT change any default.
///
/// Whole-epoch pinning of the SQLite reads (the "WHOLE audit" coherence): both SQLite reads resolve to
/// `epoch.snapshot_uid`. `find_cycles(snapshot_uid)` is snapshot-parameterized, and the language baseline reads
/// `distinct_file_languages_for_snapshot(snapshot_uid)` (`file_versions ⋈ files`, W-B-EPOCH-IMPL-2D) — NOT the
/// repo-scoped `distinct_file_languages`, which has no snapshot dimension and would straddle epochs under W-B. The
/// pin has TWO parts (precise, per review-2): the cycle rows + the language FILE-SET are pinned to N by
/// construction (snapshot-keyed rows / the immutable `file_versions` manifest PK), and the language VALUE per file
/// — read from the mutable `files` table — is epoch-invariant by the upstream `file_uid -> language` invariant
/// (file_uid embeds the path; `detect_language` is path-pure; full citation chain on the query). So the precise
/// claim is NOT "a concurrent `files` write cannot happen" (a re-index DOES rewrite the row ON CONFLICT) but "the
/// only language such a write can put on N's file_uid is the one N already has." The prior READY snapshot's rows +
/// `file_versions` manifest are retained until a separate, exclusive prune. So the SQLite side cannot shift for a
/// reader pinned to `snapshot_uid`: no captured-language witness and no language check-after are needed (the 2D
/// iteration-1 design used those to work around the lack of a snapshot-scoped query, but a captured witness could
/// not bind the FIRST language read across the resolve->capture window — review-1's blocking gap; the
/// snapshot-scoped query + the value invariant close it at the root). The ONLY store that can move under the
/// captured epoch is the LiveGraph (no snapshot dimension), which is what the fingerprint check-after below catches. (Under the current W-A coordinator the read guard already excludes
/// concurrent writers for the request's whole duration; this check is what keeps the audit coherent once IMPL-3
/// relaxes W-B.)
///
/// DAEMON-CANCEL-1: the two Tarjan SCC traversals (the LiveGraph module cycles + the SQLite module cycles)
/// thread the cooperative `cancel` checkpoint, so a peer disconnect mid-traversal surfaces as
/// `StorageError::Cancelled` (the handler maps it to `ErrorCode::Cancelled`, never `InternalError`).
pub fn cycle_completeness_audit_response(
    repo_state: &RepoState,
    conn: &StorageConnection,
    repo_uid: &str,
    epoch: &AuditEpoch,
    repo_root: &str,
    include_fixtures: bool,
    // The cooperative cancellation checkpoint (DAEMON-CANCEL-1), threaded into the two Tarjan SCC traversals.
    // Spelled as the raw closure type (identical to `repo_graph_algorithms::CancelCheck`) so `daemon-runtime`
    // needs no new dependency edge on the algorithms crate.
    cancel: &mut dyn FnMut() -> ControlFlow<()>,
) -> Result<Value, StorageError> {
    let snapshot_uid = epoch.snapshot_uid.as_str();

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

    // 2. Language baseline (D3-A): non-TS evidence (NOT the evaluator). Read PINNED to `epoch.snapshot_uid` via
    //    `distinct_file_languages_for_snapshot` (`file_versions ⋈ files`) — the file SET is bound to the SAME
    //    snapshot as `find_cycles` by the manifest join, and the language VALUE is epoch-invariant by the upstream
    //    `file_uid -> language` invariant (see the query's doc comment), so there is no resolve->capture straddle
    //    (review-1's blocking gap) and no need for a captured witness or a language check-after. `file_versions`
    //    holds every tracked file in the snapshot (incl. a non-TS code file with zero nodes), so the conservative
    //    non-TS inventory is not dropped (a `nodes ⋈ files` join would drop it -> a false `Complete`).
    let languages = conn.distinct_file_languages_for_snapshot(snapshot_uid)?;
    let (baseline, non_ts_languages) =
        build_baseline(expected_partition_ids.clone(), &languages, snapshot_uid);

    // 3. The PURE in-memory snapshot (read-only) + the LiveGraph module-cycle count (for corroboration), under
    //    ONE LiveGraph read guard. `module_import_cycles_cancellable` threads the cancel checkpoint
    //    (DAEMON-CANCEL-1) so the LiveGraph Tarjan abandons on a peer disconnect.
    let (snapshot, livegraph_module_cycle_count) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                let count = lg
                    .module_import_cycles_cancellable(&mut *cancel)
                    // The only error is `repo_graph_algorithms::Cancelled` (a peer-disconnect mid-Tarjan);
                    // map it to the storage Cancelled channel without naming the algorithms type.
                    .map_err(|_| StorageError::Cancelled)?
                    .data()
                    .map(|d| d.cycles.len())
                    .unwrap_or(0);
                (lg.module_cycle_live_state(), count)
            }
            None => (Default::default(), 0),
        }
    };

    // 4. The SQLite-free evaluator + the invalidation fingerprint.
    let certificate = evaluate_module_cycle_completeness(&snapshot, Some(&baseline));
    let fingerprint = certificate_inputs_fingerprint(&snapshot, Some(&baseline));

    // 5. D3-B corroboration (NOT an evaluator input): does SQLite see MORE module cycles than the
    //    TS-only LiveGraph? If so AND non-TS code is present, the non-TS completeness risk is corroborated.
    //    PINNED to `epoch.snapshot_uid`; cancellable Tarjan (DAEMON-CANCEL-1).
    let sqlite_module_cycle_count = conn
        .find_cycles_cancellable(snapshot_uid, "module", &mut *cancel)?
        .len();
    let non_ts_corroborated = baseline.has_non_ts_cycle_source
        && sqlite_module_cycle_count > livegraph_module_cycle_count;

    // 6. LIVEGRAPH EPOCH-IDENTITY COHERENCE (the check-after). The SQLite side is already pinned to
    //    `epoch.snapshot_uid` by construction (find_cycles + the snapshot-scoped language baseline), so the ONLY
    //    store that can have moved is the LiveGraph. Re-peek its resident fingerprint and compare against the
    //    captured identity (`epoch.resident_livegraph_fingerprint`). Partition epochs are MONOTONIC (a refresh
    //    bumps the epoch in place and never restores an old one, so a fingerprint never recurs), hence a stable
    //    fingerprint here proves the LiveGraph did not move across the WHOLE audit window — the cycle data in the
    //    middle was read at the captured epoch. If it moved (a concurrent refresh swapped the LiveGraph
    //    mid-audit), the SQLite@N-vs-LiveGraph@N+1 comparison would be a FALSE incompleteness -> HONEST-DEGRADE.
    let resident_fingerprint_after = resident_livegraph_fingerprint(repo_state, snapshot_uid);
    if resident_fingerprint_after != epoch.resident_livegraph_fingerprint {
        return Ok(incoherent_epoch_response(
            repo_uid,
            snapshot_uid,
            &epoch.resident_livegraph_fingerprint,
            &resident_fingerprint_after,
        ));
    }

    // 7. The COHERENT audit result — the EXISTING output shape (byte-identical to the pre-W-B-EPOCH audit on a
    //    steady repo; the coherence check above adds NO field on the coherent path).
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
                "has_external_nonlocal_benign": o.has_external_nonlocal,
                "has_asset_nonrelevant_benign": o.has_asset_nonrelevant,
                "has_workspace_local_unedgeable": o.has_workspace_local_unedgeable,
                "has_unresolved_package": o.has_unresolved_package,
                "has_alias_unresolved": o.has_alias_unresolved,
                "has_dynamic_unresolved": o.has_dynamic_unresolved,
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

    // W-B-EPOCH-IMPL-2D NOTE: the SQLite side (cycles + language baseline) is pinned to `snapshot_uid` — the cycle
    // rows + the language file-SET by construction (`find_cycles` + the `file_versions` manifest join in
    // `distinct_file_languages_for_snapshot`), and the language VALUE by the upstream `file_uid -> language`
    // invariant — so the only check-after is the LiveGraph resident-fingerprint comparison (a trivial `!=`, proven
    // end-to-end by `mid_audit_livegraph_swap_honest_degrades_never_false_incompleteness` below). The language pin
    // is proven at the storage layer: the file-SET (no-drop + snapshot-scoping) by
    // `distinct_file_languages_for_snapshot_pins_and_does_not_drop_no_node_files`, and the VALUE invariant (an
    // existing snapshot file re-indexed, plus the upstream-vs-storage boundary) by
    // `distinct_file_languages_for_snapshot_is_epoch_invariant_under_existing_file_reindex` (both in queries.rs);
    // end-to-end the set-pin is proven by `repo_scoped_files_shift_after_capture_does_not_pollute_pinned_language_baseline`
    // below.

    /// W-B-EPOCH-IMPL-2D integration proofs: a real warm `RepoState` (a ready SQLite snapshot + the committed
    /// synthetic SCIP partition resident in the LiveGraph), exercising `cycle_completeness_audit_response`
    /// end-to-end. Mirrors the `trust_coherence` integration scaffolding (synthetic fixture + swap helper).
    mod epoch_coherence {
        use super::super::{
            cycle_completeness_audit_response, resident_livegraph_fingerprint, AuditEpoch,
        };
        use crate::state::RepoState;
        use repo_graph_livegraph::LiveGraph;
        use repo_graph_livegraph_feed::feed_partition;
        use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
        use repo_graph_storage::types::{
            CreateSnapshotInput, FileVersion, Repo, TrackedFile, UpdateSnapshotStatusInput,
        };
        use repo_graph_storage::StorageConnection;
        use repo_graph_trust_model::LanguageSupport;
        use serde_json::Value;
        use std::ops::ControlFlow;
        use std::path::Path;
        use tempfile::tempdir;

        const REPO: &str = "repo_audit_epoch_e2e";

        /// A never-breaking cancellation checkpoint (no peer disconnect in a headless test).
        fn never_cancel() -> impl FnMut() -> ControlFlow<()> {
            || ControlFlow::Continue(())
        }

        /// A minimal SQLite db: a repo + a ready snapshot + one `files` row AND one `file_versions` row per
        /// supplied language. The audit now reads the language inventory PINNED to the snapshot via
        /// `distinct_file_languages_for_snapshot` (`file_versions ⋈ files`), so the manifest rows are required
        /// for the join to see the files. Returns the snapshot_uid.
        fn build_db(dir: &Path, languages: &[&str]) -> String {
            let db_path = dir.join("repo.db");
            let mut conn = StorageConnection::open(&db_path).expect("open storage");
            conn.add_repo(&Repo {
                repo_uid: REPO.to_string(),
                name: REPO.to_string(),
                root_path: ".".to_string(),
                default_branch: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .expect("add repo");
            let snap = conn
                .create_snapshot(&CreateSnapshotInput {
                    repo_uid: REPO.to_string(),
                    kind: "full".to_string(),
                    basis_ref: None,
                    basis_commit: None,
                    parent_snapshot_uid: None,
                    label: None,
                    toolchain_json: None,
                })
                .expect("create snapshot");
            let snapshot_uid = snap.snapshot_uid;
            conn.update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snapshot_uid.clone(),
                status: "ready".to_string(),
                completed_at: None,
            })
            .expect("ready snapshot");
            let files: Vec<TrackedFile> = languages
                .iter()
                .enumerate()
                .map(|(i, lang)| TrackedFile {
                    file_uid: format!("f{i}"),
                    repo_uid: REPO.to_string(),
                    path: format!("src/file{i}.x"),
                    language: Some((*lang).to_string()),
                    is_test: false,
                    is_generated: false,
                    is_excluded: false,
                })
                .collect();
            conn.upsert_files(&files).expect("upsert files");
            // The snapshot's file manifest: one `file_versions` row per file, tying each file_uid to THIS
            // snapshot so `distinct_file_languages_for_snapshot` sees it.
            let file_versions: Vec<FileVersion> = files
                .iter()
                .map(|f| FileVersion {
                    snapshot_uid: snapshot_uid.clone(),
                    file_uid: f.file_uid.clone(),
                    content_hash: "h".to_string(),
                    ast_hash: None,
                    extractor: None,
                    parse_status: "skipped".to_string(),
                    size_bytes: Some(1),
                    line_count: Some(1),
                    indexed_at: "2026-01-01T00:00:00Z".to_string(),
                })
                .collect();
            conn.upsert_file_versions(&file_versions)
                .expect("upsert file_versions");
            snapshot_uid
        }

        /// Ingest the committed synthetic SCIP fixture (producer-free; the SAME fixture trust/orient/explain
        /// e2e use).
        fn synthetic_outcome() -> IngestOutcome {
            let root = format!(
                "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
                env!("CARGO_MANIFEST_DIR")
            );
            let scip =
                std::fs::read(format!("{root}/index.scip")).expect("read committed index.scip");
            let index = decode_index(&scip).expect("decode scip");
            ingest_partition(
                &index,
                &root,
                "synthetic",
                "synthetic",
                "scip-typescript",
                "0.4.0",
                "h",
                "",
            )
        }

        /// A warm `RepoState`: a ready snapshot (with `languages` in `files`) + the resident synthetic TS
        /// partition. Returns the `TempDir` (keep alive — dropping it deletes the db), the state, and the
        /// snapshot_uid. The audit's `repo_root` is the `TempDir` (no tsconfig -> empty partition discovery).
        fn warm_state(languages: &[&str]) -> (tempfile::TempDir, RepoState, String) {
            let dir = tempdir().unwrap();
            let snapshot_uid = build_db(dir.path(), languages);
            let state =
                RepoState::open(&dir.path().join("repo.db"), REPO).expect("open repo state");
            let mut lg = LiveGraph::new();
            feed_partition(
                &mut lg,
                "synthetic",
                synthetic_outcome(),
                LanguageSupport::TypeScriptPrimary,
            );
            *state.livegraph.write() = Some(lg);
            (dir, state, snapshot_uid)
        }

        /// Simulate a refresh's mid-request LiveGraph swap: re-feed the synthetic partition, bumping its epoch
        /// in place (epochs are MONOTONIC), so the resident `import_cert_fingerprint` MOVES and an identity
        /// captured before the swap no longer matches. Mirrors `trust_coherence`'s `swap_livegraph`.
        fn swap_livegraph(state: &RepoState) {
            feed_partition(
                state
                    .livegraph
                    .write()
                    .as_mut()
                    .expect("resident livegraph"),
                "synthetic",
                synthetic_outcome(),
                LanguageSupport::TypeScriptPrimary,
            );
        }

        /// PROOF — IDENTITY-EXACTNESS: the captured epoch-identity fingerprint IS the EXACT raw resident
        /// fingerprint at capture (GREEN-or-RED). It is captured RAW — no GREEN no-loss cert is stored, yet the
        /// identity is non-empty and exact — which is what distinguishes the ratified raw-identity witness from
        /// the rejected GREEN-eligibility witness (the latter would be `None` here, with no green cert).
        #[test]
        fn captured_identity_is_the_exact_raw_resident_fingerprint() {
            let (_dir, state, snapshot_uid) = warm_state(&["typescript"]);
            // No `cycles_cert`/`callgraph_cert` is GREEN here -> a GREEN-eligibility witness would be None.
            let epoch = AuditEpoch::capture(&state, &snapshot_uid);
            let raw = resident_livegraph_fingerprint(&state, &snapshot_uid);
            assert_eq!(
                epoch.resident_livegraph_fingerprint, raw,
                "the captured identity IS the exact raw resident fingerprint"
            );
            assert!(
                !epoch.resident_livegraph_fingerprint.is_empty()
                    && epoch.resident_livegraph_fingerprint.contains("synthetic"),
                "a resident partition yields a non-empty raw fingerprint (GREEN-or-RED), no cert needed"
            );
            assert_eq!(epoch.snapshot_uid, snapshot_uid);
        }

        /// PROOF — EPOCH-IDENTITY COHERENCE (steady): on a steady epoch the audit runs coherently and returns
        /// the EXISTING audit shape (a real certificate + evidence + baseline), NOT the degrade shape.
        #[test]
        fn steady_epoch_audits_coherently_with_the_existing_shape() {
            let (dir, state, snapshot_uid) = warm_state(&["typescript"]);
            let conn = state.storage().expect("storage");
            let epoch = AuditEpoch::capture(&state, &snapshot_uid);
            let repo_root = dir.path().to_str().unwrap();
            let mut cancel = never_cancel();
            let v = cycle_completeness_audit_response(
                &state,
                &conn,
                REPO,
                &epoch,
                repo_root,
                false,
                &mut cancel,
            )
            .expect("audit runs");
            // Coherent: the existing shape (certificate + evidence + baseline), NOT the incoherent-epoch degrade.
            assert!(
                v.get("certificate").is_some(),
                "the steady audit emits a certificate"
            );
            assert!(v.get("evidence").is_some());
            assert!(v.get("baseline").is_some());
            assert_eq!(
                v["audit_status"],
                Value::Null,
                "no degrade marker on the coherent path (existing shape preserved)"
            );
            assert_eq!(v["snapshot_uid"], Value::String(snapshot_uid));
            assert_eq!(v["kind"], "module-import");
        }

        /// PROOF — EPOCH-IDENTITY COHERENCE (mid-audit move) / the feature's core safety: if the resident
        /// LiveGraph fingerprint MOVES mid-audit (a swap), the audit HONEST-DEGRADES — `audit_status:
        /// incoherent_epoch`, NO certificate — NEVER a false incompleteness from SQLite@N + LiveGraph@N+1.
        #[test]
        fn mid_audit_livegraph_swap_honest_degrades_never_false_incompleteness() {
            let (dir, state, snapshot_uid) = warm_state(&["typescript"]);
            let conn = state.storage().expect("storage");
            let repo_root = dir.path().to_str().unwrap().to_string();
            // Capture the identity at epoch N.
            let epoch = AuditEpoch::capture(&state, &snapshot_uid);

            // Steady (no swap): coherent, real certificate.
            let mut cancel = never_cancel();
            let steady = cycle_completeness_audit_response(
                &state,
                &conn,
                REPO,
                &epoch,
                &repo_root,
                false,
                &mut cancel,
            )
            .expect("steady audit runs");
            assert!(steady.get("certificate").is_some());
            assert_eq!(steady["audit_status"], Value::Null);

            // A refresh swaps the LiveGraph to epoch N+1; the captured identity no longer matches -> the
            // check-after fires -> HONEST-DEGRADE.
            swap_livegraph(&state);
            let mut cancel2 = never_cancel();
            let degraded = cycle_completeness_audit_response(
                &state,
                &conn,
                REPO,
                &epoch,
                &repo_root,
                false,
                &mut cancel2,
            )
            .expect("audit returns (degraded, not errored)");
            assert_eq!(degraded["audit_status"], "incoherent_epoch");
            assert_eq!(degraded["audit_coherent"], Value::Bool(false));
            assert!(
                degraded.get("certificate").is_none(),
                "the honest-degrade NEVER emits a certificate (no false incompleteness)"
            );
            assert_ne!(
                degraded["captured_livegraph_fingerprint"],
                degraded["observed_livegraph_fingerprint"],
                "the degrade records the captured vs the moved (observed) fingerprint"
            );
        }

        /// PROOF — RED-REPO-STILL-AUDITS (distinguishes the identity-witness from the rejected GREEN-witness):
        /// on a RED/incomplete repo (a non-TS source -> the GREEN no-loss cert is NOT eligible) the audit STILL
        /// RUNS and REPORTS the real incompleteness; it is NOT made unavailable by a GREEN gate.
        #[test]
        fn red_repo_still_audits_and_reports_real_incompleteness() {
            // A non-TS code language (rust) in the SQLite inventory -> has_non_ts_cycle_source -> the TS-only
            // LiveGraph cannot be proven complete (a GREEN no-loss cert would be RED/ineligible here).
            let (dir, state, snapshot_uid) = warm_state(&["typescript", "rust"]);
            let conn = state.storage().expect("storage");
            let epoch = AuditEpoch::capture(&state, &snapshot_uid);
            let repo_root = dir.path().to_str().unwrap();
            let mut cancel = never_cancel();
            let v = cycle_completeness_audit_response(
                &state,
                &conn,
                REPO,
                &epoch,
                repo_root,
                false,
                &mut cancel,
            )
            .expect("audit runs on a RED repo");
            // It RAN (coherent shape), NOT degraded/unavailable.
            assert_eq!(
                v["audit_status"],
                Value::Null,
                "the audit is NOT gated off on a RED repo (it is a comparator, not a GREEN-gated serve)"
            );
            assert!(
                v.get("certificate").is_some(),
                "a RED repo still gets a certificate"
            );
            // It REPORTS the real incompleteness: the non-TS source is surfaced and the green default is denied.
            assert_eq!(v["baseline"]["has_non_ts_cycle_source"], Value::Bool(true));
            assert_eq!(
                v["evidence"]["non_ts_languages"],
                serde_json::json!(["rust"]),
                "the real non-TS incompleteness evidence is reported"
            );
            assert_eq!(
                v["permits_livegraph_default"],
                Value::Bool(false),
                "a non-TS-incomplete certificate denies the green default (real incompleteness)"
            );
        }

        /// PROOF — WHOLE-EPOCH PINNING (the review-1 straddle, end-to-end): the language baseline is PINNED to
        /// `epoch.snapshot_uid` via `distinct_file_languages_for_snapshot` (`file_versions ⋈ files`), so a
        /// repo-scoped `files` shift AFTER capture is correctly INVISIBLE — the audit stays coherent at N and
        /// reports N's true inventory, NEVER a mix of `find_cycles@N` + a later language inventory.
        ///
        /// This is the corrected refutation of review-1's blocking gap. The iteration-1 design captured the
        /// language inventory via a repo-scoped read + a two-read check-after, which could NOT bind the FIRST
        /// read across the resolve->capture window (a `files` shift in that window passed the check yet straddled
        /// epochs). The snapshot-scoped query closes it AT THE ROOT: the read is parameterized by `snapshot_uid`,
        /// so a concurrent `files` mutation that adds a file NOT in snapshot N's manifest cannot enter N's
        /// baseline. The audit does not need to DETECT the shift and degrade — the shift simply cannot reach it.
        #[test]
        fn repo_scoped_files_shift_after_capture_does_not_pollute_pinned_language_baseline() {
            let (dir, state, snapshot_uid) = warm_state(&["typescript"]);
            let mut conn = state.storage().expect("storage");
            let repo_root = dir.path().to_str().unwrap().to_string();
            // Capture the epoch identity at N — snapshot N's manifest holds only the typescript file.
            let epoch = AuditEpoch::capture(&state, &snapshot_uid);

            // Steady (no mutation): coherent, real certificate, NO non-TS source.
            let mut cancel = never_cancel();
            let steady = cycle_completeness_audit_response(
                &state,
                &conn,
                REPO,
                &epoch,
                &repo_root,
                false,
                &mut cancel,
            )
            .expect("steady audit runs");
            assert!(steady.get("certificate").is_some());
            assert_eq!(steady["audit_status"], Value::Null);
            assert_eq!(
                steady["baseline"]["has_non_ts_cycle_source"],
                Value::Bool(false),
                "snapshot N had no non-TS source"
            );

            // A concurrent index adds a NON-TS file to the repo-scoped `files` table AFTER capture, but WITHOUT
            // a `file_versions` row in snapshot N (it belongs to a newer/forming snapshot). This is exactly the
            // resolve->capture (and post-capture) `files` shift review-1 flagged as a straddle.
            conn.upsert_files(&[TrackedFile {
                file_uid: "f-rust".to_string(),
                repo_uid: REPO.to_string(),
                path: "src/lib.rs".to_string(),
                language: Some("rust".to_string()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            }])
            .expect("add a non-TS file to the repo-scoped files table");

            // Contrast: the REPO-SCOPED read now leaks rust (the old, unsafe behavior); the SNAPSHOT-SCOPED read
            // the audit uses does NOT — snapshot N's manifest has no rust file.
            assert_eq!(
                conn.distinct_file_languages(REPO).unwrap(),
                vec!["rust".to_string(), "typescript".to_string()],
                "the repo-scoped read shifted (would have polluted the baseline)"
            );
            assert_eq!(
                conn.distinct_file_languages_for_snapshot(&snapshot_uid)
                    .unwrap(),
                vec!["typescript".to_string()],
                "the snapshot-scoped read the audit uses is PINNED to N — rust excluded"
            );

            // The audit re-runs at the SAME captured epoch N: STILL coherent, STILL no non-TS source. The
            // repo-scoped shift never reached the pinned baseline — no straddle, no false incompleteness, and no
            // false `Complete` either (snapshot N genuinely had no rust file).
            let mut cancel2 = never_cancel();
            let after = cycle_completeness_audit_response(
                &state,
                &conn,
                REPO,
                &epoch,
                &repo_root,
                false,
                &mut cancel2,
            )
            .expect("audit still runs coherently");
            assert_eq!(
                after["audit_status"],
                Value::Null,
                "no degrade: the SQLite side is pinned to N, so the repo-scoped shift is invisible"
            );
            assert!(after.get("certificate").is_some());
            assert_eq!(
                after["baseline"]["has_non_ts_cycle_source"],
                Value::Bool(false),
                "the pinned baseline still reflects snapshot N ([typescript]) — NOT mixed with the later rust file"
            );
            assert_eq!(
                after["evidence"]["observed_languages"],
                serde_json::json!(["typescript"]),
                "the audit reports N's true inventory, not the polluted repo-scoped one"
            );
        }
    }
}
