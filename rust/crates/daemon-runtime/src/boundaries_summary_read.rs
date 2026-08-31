//! HTTP-SURFACE-COHERENCE-1 §2.3 — read-time reconciliation of the `boundaries
//! summary` counts with the ONE unified HTTP aggregation.
//!
//! `get_boundary_interaction_summary` counts ONLY the boundary-interaction family.
//! When a repo's HTTP providers live in the legacy `project_surfaces` family
//! (FRAKTAG's 47 Express routes), that summary told a contradictory story: it
//! printed `By direction: 52 consumer` (0 providers) directly beside the union's
//! `47 providers, 46 consumers` — the exact review-2 finding-2 incoherence.
//!
//! This module reconciles the summary's COUNT breakdowns with the same unified
//! rows the surfaces footer and `boundaries list` count, as a DELTA on the base
//! summary: subtract the boundary-family HTTP rows, add the unified HTTP rows.
//! The delta is EMPTY when a snapshot has no HTTP surface in either family, so a
//! non-HTTP repo (leveldb) is byte-for-byte unchanged by construction.
//!
//! What the unified rows carry vs not (honest downgrade): they carry the real
//! `direction` (provider/consumer) and are HTTP by definition (channel_kind /
//! protocol_family = `http`). They do NOT carry the boundary-interaction family's
//! per-row detection provenance (`boundary_scope`, `basis`) — read-time
//! cross-family reconciliation cannot reconstruct it — so the reconciled rows
//! bucket as `unknown` there. That keeps EVERY breakdown summing to the same
//! total (no "93 http but 52 unknown" cross-breakdown mismatch) at the cost of
//! the boundary family's `api_call` basis label on the rows the union absorbs —
//! an acceptable trade for one coherent HTTP story.
//!
//! Abstraction record — module: `boundaries_summary_read`; concrete current user:
//! `ServiceDispatcher::handle_boundaries_summary`; axis: reconcile the summary's
//! COUNT breakdowns with the ONE §2.3 aggregation, off the 8.9k-line dispatch.rs;
//! rejected simpler alternative: splicing only an extra HTTP line while leaving
//! `total`/`by_direction` boundary-family-only (the review-2 finding — a visible
//! provider/consumer contradiction remained).

use std::collections::BTreeMap;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, BoundaryInteractionSummary, ChannelKind,
};
use repo_graph_storage::StorageConnection;

use crate::http_boundary_read::unified_http_surfaces;
use crate::http_surface_union::UnifiedHttpSurface;
use crate::test_composition::TestComposition;

/// Build the `boundaries summary` response with its COUNT breakdowns reconciled
/// to the unified HTTP aggregation (§2.3). `base` is the boundary-interaction
/// summary; the HTTP portion of every count is swapped for the union's. Also
/// splices the explicit `http_surface_providers`/`consumers` line (the labelled
/// HTTP callout) or a degraded marker.
///
/// `Err(reason)` only when a required read fails (reader-framed).
pub(crate) fn summary_response_json(
    repo_uid: &str,
    snapshot_uid: &str,
    storage: &StorageConnection,
    base: &BoundaryInteractionSummary,
) -> Result<serde_json::Value, String> {
    // The boundary-family HTTP rows we REPLACE, and the unified rows we add.
    let mut http_filter = BoundaryInteractionFilter::new();
    http_filter.channel_kind = Some(ChannelKind::Http);
    let bh = storage
        .list_boundary_interactions(snapshot_uid, &http_filter)
        .map_err(|e| degraded("boundary http rows", e))?;
    let unified = unified_http_surfaces(storage, repo_uid, snapshot_uid)?;

    let (u_providers, u_consumers) = (
        unified.iter().filter(|r| r.direction == "provider").count(),
        unified.iter().filter(|r| r.direction == "consumer").count(),
    );
    let u_total = unified.len();

    // FIXTURE-POLLUTION-1 §2.2 + binding direction rule + review-1 #2b / review-2 #1: the
    // tri-state test-composition of the reconciled set (the stored `is_test` fact, never a
    // name), split into TWO additive disclosures of the SAME breakdown shape:
    //   - `test_only`     — the POSITIVELY test-only portion the presentation SUBTRACTS from
    //                       the headline and renders as a trailing demoted section.
    //   - `unknown`       — the surfaces with NO reachable `is_test` evidence. These are
    //                       NEVER demoted (binding direction rule): they STAY in the headline
    //                       counts, disclosed with their reason so a reader knows the headline
    //                       is production+unknown, not confirmed-production.
    // `summary` itself stays the FULL reconciled object (byte-identical to the pre-slice
    // payload — the subtraction is a display concern), so a repo with neither test-only nor
    // unknown surfaces emits neither key and is unchanged.
    let partition = build_composition_partition(repo_uid, snapshot_uid, storage, &unified)?;

    let mut summary = serde_json::to_value(base).map_err(|e| degraded("summary", e))?;
    if let serde_json::Value::Object(map) = &mut summary {
        // total_surfaces / total_channels
        adjust_scalar(map, "totalSurfaces", u_total as i64 - bh.len() as i64);
        let bh_channels: i64 = bh.iter().map(|it| it.channel_count as i64).sum();
        adjust_scalar(map, "totalChannels", -bh_channels);

        // by_channel_kind / by_protocol_family: HTTP bucket ← union total.
        let http_delta = u_total as i64 - bh.len() as i64;
        adjust_bucket(
            map,
            "byChannelKind",
            "channelKind",
            &delta1("http", http_delta),
        );
        adjust_bucket(
            map,
            "byProtocolFamily",
            "protocolFamily",
            &delta1("http", http_delta),
        );

        // by_direction: subtract each boundary-http row's direction, add the
        // union's provider/consumer split.
        let mut dir: BTreeMap<String, i64> = BTreeMap::new();
        for it in &bh {
            *dir.entry(it.direction.as_str().to_string()).or_default() -= 1;
        }
        *dir.entry("provider".to_string()).or_default() += u_providers as i64;
        *dir.entry("consumer".to_string()).or_default() += u_consumers as i64;
        adjust_bucket(map, "byDirection", "direction", &dir);

        // by_boundary_scope / by_basis: subtract the boundary-http rows' real
        // scope/basis, add the union rows as `unknown` (not carried at read time).
        let mut scope: BTreeMap<String, i64> = BTreeMap::new();
        let mut basis: BTreeMap<String, i64> = BTreeMap::new();
        for it in &bh {
            *scope
                .entry(it.boundary_scope.as_str().to_string())
                .or_default() -= 1;
            *basis.entry(it.basis.as_str().to_string()).or_default() -= 1;
        }
        *scope.entry("unknown".to_string()).or_default() += u_total as i64;
        *basis.entry("unknown".to_string()).or_default() += u_total as i64;
        adjust_bucket(map, "byBoundaryScope", "boundaryScope", &scope);
        adjust_bucket(map, "byBasis", "basis", &basis);

        // files_with_boundaries: add the unified rows' source files (legacy
        // provider files the boundary summary never listed).
        merge_files(map, unified.iter().map(|r| r.source_file.as_str()));
    }

    let mut response = serde_json::json!({
        "command": "boundaries summary",
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "summary": summary,
        // The explicit labelled HTTP callout (same union the surfaces footer prints).
        "http_surface_providers": u_providers,
        "http_surface_consumers": u_consumers,
    });
    // FIXTURE-POLLUTION-1 §2.2/§2.4 (review-1 #2b, review-2 #1): each disclosure is emitted
    // ONLY when its portion is non-empty, so a repo with neither (leveldb, glamCRM's
    // production routes) stays byte-identical to the pre-slice payload (no keys). The
    // presentation subtracts `test_only_summary` for the headline and renders it trailing; it
    // discloses `unknown_composition` WITHOUT subtracting (unknown stays in the headline).
    if let serde_json::Value::Object(ref mut map) = response {
        if let Some(test_only) = partition.test_only {
            map.insert("test_only_summary".to_string(), test_only);
        }
        if let Some(unknown) = partition.unknown {
            map.insert("unknown_composition".to_string(), unknown);
        }
    }
    // The reconciled `by_direction` and the labelled callout agree by
    // construction — both are the union's (u_providers, u_consumers).
    debug_assert_eq!(u_providers + u_consumers, u_total);
    Ok(response)
}

/// The two additive test-composition disclosures the reconciled row set produces. Each is
/// `None` when its portion is empty, so the caller omits the key and the payload stays
/// byte-identical for a repo with neither test-only nor unknown surfaces.
struct CompositionPartition {
    /// The POSITIVELY test-only sub-summary the presentation subtracts from the headline.
    test_only: Option<serde_json::Value>,
    /// The UNKNOWN-composition disclosure (count + reasons) the presentation shows WITHOUT
    /// subtracting — unknown surfaces stay in the headline (binding direction rule).
    unknown: Option<serde_json::Value>,
}

/// FIXTURE-POLLUTION-1 §2.2/§2.4 + binding direction rule (review-2 #1): classify every
/// reconciled row — `(non-HTTP boundary rows) + (unified HTTP rows)` — by the shared
/// tri-state `TestComposition` (the stored `is_test` fact, never a name) and split it into
/// two same-shaped additive disclosures:
///
/// - `TestOnly` rows accumulate into the `test_only` sub-summary the presentation SUBTRACTS
///   from the headline (a demoted trailing section).
/// - `Unknown` rows (no reachable `is_test` evidence) accumulate into the `unknown`
///   disclosure — a surface count plus the DISTINCT reader-framed reasons. They are NEVER
///   subtracted: they stay in the headline, disclosed so the reader knows those surfaces are
///   unprovable, not confirmed production.
/// - `Production` rows contribute to neither (they are the conservative headline default,
///   reached only with positive evidence).
///
/// Files are classified CONSERVATIVELY for the test-only file demotion: a file appears in the
/// test-only file list only when EVERY reconciled row on it is `TestOnly` (any
/// production/unknown row keeps it in the headline — never hide a real file).
///
/// `Err` only on a failed required read (reader-framed).
fn build_composition_partition(
    repo_uid: &str,
    snapshot_uid: &str,
    storage: &StorageConnection,
    unified: &[UnifiedHttpSurface],
) -> Result<CompositionPartition, String> {
    let files = storage
        .get_files_by_repo(repo_uid)
        .map_err(|e| degraded("tracked files", e))?;
    let is_test_by_path: BTreeMap<&str, bool> =
        files.iter().map(|f| (f.path.as_str(), f.is_test)).collect();
    let all = storage
        .list_boundary_interactions(snapshot_uid, &BoundaryInteractionFilter::new())
        .map_err(|e| degraded("boundary rows", e))?;

    let mut acc = CompositionAcc::default();
    // Non-HTTP boundary rows (the HTTP boundary rows are replaced by the unified set).
    for it in all.iter().filter(|it| it.channel_kind.as_str() != "http") {
        let comp = TestComposition::from_is_test_fact(
            is_test_by_path.get(it.source_file.as_str()).copied(),
            &it.source_file,
        );
        acc.note_file(&it.source_file, matches!(comp, TestComposition::TestOnly));
        match comp {
            TestComposition::TestOnly => {
                acc.total_surfaces += 1;
                acc.total_channels += it.channel_count as usize;
                *acc.by_kind
                    .entry(it.channel_kind.as_str().to_string())
                    .or_default() += 1;
                *acc.by_scope
                    .entry(it.boundary_scope.as_str().to_string())
                    .or_default() += 1;
                *acc.by_direction
                    .entry(it.direction.as_str().to_string())
                    .or_default() += 1;
                *acc.by_family
                    .entry(it.protocol_family.as_str().to_string())
                    .or_default() += 1;
                *acc.by_basis
                    .entry(it.basis.as_str().to_string())
                    .or_default() += 1;
            }
            TestComposition::Unknown(reason) => acc.note_unknown(reason),
            TestComposition::Production => {}
        }
    }
    // Unified HTTP rows bucket EXACTLY as the reconciliation added them: kind/family = http,
    // scope/basis = unknown, direction = the row's real direction.
    for r in unified {
        let comp = TestComposition::from_is_test_fact(r.is_test, &r.source_file);
        acc.note_file(&r.source_file, matches!(comp, TestComposition::TestOnly));
        match comp {
            TestComposition::TestOnly => {
                acc.total_surfaces += 1;
                *acc.by_kind.entry("http".to_string()).or_default() += 1;
                *acc.by_family.entry("http".to_string()).or_default() += 1;
                *acc.by_scope.entry("unknown".to_string()).or_default() += 1;
                *acc.by_basis.entry("unknown".to_string()).or_default() += 1;
                *acc.by_direction.entry(r.direction.clone()).or_default() += 1;
                if r.direction == "provider" {
                    acc.http_providers += 1;
                } else if r.direction == "consumer" {
                    acc.http_consumers += 1;
                }
            }
            TestComposition::Unknown(reason) => acc.note_unknown(reason),
            TestComposition::Production => {}
        }
    }

    Ok(acc.finish())
}

/// Accumulator for [`build_composition_partition`]: the test-only sub-breakdowns, the
/// unknown-composition tally, plus a per-file "has a non-test-only reconciled row" flag for
/// the conservative file rule.
#[derive(Default)]
struct CompositionAcc {
    total_surfaces: usize,
    total_channels: usize,
    by_kind: BTreeMap<String, i64>,
    by_scope: BTreeMap<String, i64>,
    by_direction: BTreeMap<String, i64>,
    by_family: BTreeMap<String, i64>,
    by_basis: BTreeMap<String, i64>,
    http_providers: usize,
    http_consumers: usize,
    /// path -> saw a NON-test-only reconciled row (⇒ keep in the headline file list). An
    /// UNKNOWN row counts as non-test-only here: an unprovable surface never demotes a file.
    file_has_non_test_only: BTreeMap<String, bool>,
    /// Count of reconciled rows whose composition is UNKNOWN (no reachable `is_test` fact).
    unknown_surfaces: usize,
    /// The DISTINCT reader-framed reasons behind those unknown rows (sorted; deduped).
    unknown_reasons: std::collections::BTreeSet<String>,
}

impl CompositionAcc {
    fn note_file(&mut self, path: &str, is_test_only: bool) {
        let e = self
            .file_has_non_test_only
            .entry(path.to_string())
            .or_insert(false);
        *e = *e || !is_test_only;
    }

    fn note_unknown(&mut self, reason: String) {
        self.unknown_surfaces += 1;
        self.unknown_reasons.insert(reason);
    }

    fn finish(self) -> CompositionPartition {
        let unknown = (self.unknown_surfaces > 0).then(|| {
            serde_json::json!({
                "surfaces": self.unknown_surfaces,
                "reasons": self.unknown_reasons.iter().cloned().collect::<Vec<_>>(),
            })
        });
        let test_only = (self.total_surfaces > 0).then(|| self.test_only_json());
        CompositionPartition { test_only, unknown }
    }

    fn test_only_json(&self) -> serde_json::Value {
        // A file is test-only ONLY if it appeared AND every reconciled row on it was
        // test-only (conservative: any production/unknown row keeps it in the headline).
        let files: Vec<&String> = self
            .file_has_non_test_only
            .iter()
            .filter(|(_, has_non)| !**has_non)
            .map(|(path, _)| path)
            .collect();
        serde_json::json!({
            "totalSurfaces": self.total_surfaces,
            "totalChannels": self.total_channels,
            "byChannelKind": bucket_array(&self.by_kind, "channelKind"),
            "byBoundaryScope": bucket_array(&self.by_scope, "boundaryScope"),
            "byDirection": bucket_array(&self.by_direction, "direction"),
            "byProtocolFamily": bucket_array(&self.by_family, "protocolFamily"),
            "byBasis": bucket_array(&self.by_basis, "basis"),
            "filesWithBoundaries": files,
            "http_surface_providers": self.http_providers,
            "http_surface_consumers": self.http_consumers,
        })
    }
}

/// A `[{ <label_field>: k, count: n }, …]` array from a positive-count map, in the base
/// summary's shape (positive counts only; a zero never appears — we only ever incremented).
fn bucket_array(counts: &BTreeMap<String, i64>, label_field: &str) -> Vec<serde_json::Value> {
    counts
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(k, n)| serde_json::json!({ label_field: k, "count": n }))
        .collect()
}

/// A single-key delta map.
fn delta1(key: &str, n: i64) -> BTreeMap<String, i64> {
    let mut m = BTreeMap::new();
    if n != 0 {
        m.insert(key.to_string(), n);
    }
    m
}

/// PAYLOAD CONTRACT NOTE (review-4 #2, reasoned not waved): the maps merged here
/// are BOTH produced by our own serializers in this crate, whose stated
/// convention is that zero-count scalars/entries are OMITTED (this very module
/// drops entries that fall to ≤0). Absence therefore MEANS zero by the payload's
/// own contract — the `unwrap_or(0)` defaults below are that contract applied,
/// not a collapse of an external unknown (the standing honesty rule governs
/// classifying/rendering EXTERNAL reality; internal payload merging follows the
/// payload's contract). A non-string `label` — impossible from our serializer —
/// is skipped with a warn rather than renamed to "" (see `adjust_bucket`).
///
/// Add `delta` to an integer scalar field (clamped at 0).
fn adjust_scalar(map: &mut serde_json::Map<String, serde_json::Value>, key: &str, delta: i64) {
    if delta == 0 {
        return;
    }
    let cur = map.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
    let next = (cur + delta).max(0);
    map.insert(key.to_string(), serde_json::json!(next));
}

/// Apply a per-key delta to a `[{ <label_field>: k, count: n }, …]` breakdown
/// array: adjust existing keys, drop entries that fall to ≤0, and append new keys
/// with a positive residual. Order is irrelevant (the presenter re-sorts).
fn adjust_bucket(
    map: &mut serde_json::Map<String, serde_json::Value>,
    array_key: &str,
    label_field: &str,
    deltas: &BTreeMap<String, i64>,
) {
    if deltas.values().all(|d| *d == 0) {
        return;
    }
    let mut remaining = deltas.clone();
    let mut out: Vec<serde_json::Value> = Vec::new();
    if let Some(serde_json::Value::Array(items)) = map.get(array_key) {
        for item in items {
            let Some(label) = item
                .get(label_field)
                .and_then(|v| v.as_str())
                .map(str::to_string)
            else {
                // Our serializer always writes string labels; a non-string here is a
                // malformed base — keep the item untouched rather than renaming it "".
                eprintln!("warning: boundaries summary entry with non-string {label_field}; left unadjusted");
                out.push(item.clone());
                continue;
            };
            let count = item.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            let delta = remaining.remove(&label).unwrap_or(0);
            let next = count + delta;
            if next > 0 {
                out.push(serde_json::json!({ label_field: label, "count": next }));
            }
        }
    }
    for (label, delta) in remaining {
        if delta > 0 {
            out.push(serde_json::json!({ label_field: label, "count": delta }));
        }
    }
    map.insert(array_key.to_string(), serde_json::Value::Array(out));
}

/// Merge extra file paths into `filesWithBoundaries`, deduped and sorted (matches
/// the base's deterministic ordering contract).
fn merge_files<'a>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    extra: impl Iterator<Item = &'a str>,
) {
    let mut files: std::collections::BTreeSet<String> = map
        .get("filesWithBoundaries")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let before = files.len();
    files.extend(extra.map(str::to_string));
    if files.len() == before {
        return; // no new files → leave the array byte-identical
    }
    map.insert(
        "filesWithBoundaries".to_string(),
        serde_json::json!(files.into_iter().collect::<Vec<_>>()),
    );
}

/// Reader-framed degradation message.
fn degraded(context: &str, err: impl std::fmt::Display) -> String {
    format!("boundaries summary {context} read failed (degraded): {err}")
}

#[cfg(test)]
#[path = "boundaries_summary_read_tests.rs"]
mod tests;
