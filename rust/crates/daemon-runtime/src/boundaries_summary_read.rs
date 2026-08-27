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

    let response = serde_json::json!({
        "command": "boundaries summary",
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "summary": summary,
        // The explicit labelled HTTP callout (same union the surfaces footer prints).
        "http_surface_providers": u_providers,
        "http_surface_consumers": u_consumers,
    });
    // The reconciled `by_direction` and the labelled callout agree by
    // construction — both are the union's (u_providers, u_consumers).
    debug_assert_eq!(u_providers + u_consumers, u_total);
    Ok(response)
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
mod tests {
    use super::*;

    fn base_summary() -> serde_json::Value {
        // FRAKTAG-shaped base: 52 boundary-http consumers, 0 providers.
        serde_json::json!({
            "totalSurfaces": 52,
            "totalChannels": 0,
            "byChannelKind": [{"channelKind": "http", "count": 52}],
            "byBoundaryScope": [{"boundaryScope": "unknown", "count": 52}],
            "byDirection": [{"direction": "consumer", "count": 52}],
            "byProtocolFamily": [{"protocolFamily": "http", "count": 52}],
            "byBasis": [{"basis": "api_call", "count": 52}],
            "filesWithBoundaries": ["web/a.ts"]
        })
    }

    #[test]
    fn adjust_scalar_clamps_and_adds() {
        let mut m = serde_json::Map::new();
        m.insert("totalSurfaces".to_string(), serde_json::json!(52));
        adjust_scalar(&mut m, "totalSurfaces", 41); // 52 - 52(bh) + 93(union) = 93
        assert_eq!(m["totalSurfaces"], serde_json::json!(93));
    }

    #[test]
    fn adjust_bucket_replaces_direction_split() {
        // The FRAKTAG contradiction: base says 52 consumer / 0 provider; the
        // reconciled split must be 47 provider / 46 consumer.
        let serde_json::Value::Object(mut map) = base_summary() else {
            unreachable!()
        };
        let mut dir: BTreeMap<String, i64> = BTreeMap::new();
        dir.insert("consumer".to_string(), -52 + 46);
        dir.insert("provider".to_string(), 47);
        adjust_bucket(&mut map, "byDirection", "direction", &dir);
        let arr = map["byDirection"].as_array().unwrap();
        let get = |k: &str| {
            arr.iter()
                .find(|e| e["direction"] == serde_json::json!(k))
                .map(|e| e["count"].as_i64().unwrap())
                .unwrap_or(0)
        };
        assert_eq!(get("provider"), 47);
        assert_eq!(get("consumer"), 46);
    }

    #[test]
    fn empty_deltas_leave_arrays_untouched() {
        // No HTTP in either family → every delta is zero → the summary is
        // byte-identical (the leveldb byte-parity guarantee, in miniature).
        let serde_json::Value::Object(mut map) = base_summary() else {
            unreachable!()
        };
        let snapshot = serde_json::Value::Object(map.clone());
        adjust_bucket(&mut map, "byDirection", "direction", &BTreeMap::new());
        adjust_scalar(&mut map, "totalSurfaces", 0);
        assert_eq!(serde_json::Value::Object(map), snapshot);
    }

    #[test]
    fn bucket_drops_zeroed_entries() {
        let serde_json::Value::Object(mut map) = base_summary() else {
            unreachable!()
        };
        // Remove all 52 api_call, add 93 unknown → api_call gone, unknown present.
        let mut basis: BTreeMap<String, i64> = BTreeMap::new();
        basis.insert("api_call".to_string(), -52);
        basis.insert("unknown".to_string(), 93);
        adjust_bucket(&mut map, "byBasis", "basis", &basis);
        let arr = map["byBasis"].as_array().unwrap();
        assert!(arr
            .iter()
            .all(|e| e["basis"] != serde_json::json!("api_call")));
        assert_eq!(
            arr.iter()
                .find(|e| e["basis"] == serde_json::json!("unknown"))
                .unwrap()["count"],
            serde_json::json!(93)
        );
    }
}
