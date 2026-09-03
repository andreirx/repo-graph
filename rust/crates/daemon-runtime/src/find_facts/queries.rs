//! FIND-FACTS-1 (§2.1) — the seven per-class fact reads behind the `find` FACTS
//! tier. Each function queries ONE class from its OWN authoritative source and maps
//! the rows to [`FactHit`]s; the parent module owns the taxonomy, dedup, and cap.
//!
//! Honesty (STANDING RULE, review-4 item 2): a read FAILURE propagates as `Err` so
//! the class renders `unavailable (<reason>)`. An absent path is the explicit
//! [`HitPath::Unknown`] with its reason (never a silent omission that reads as "no
//! path"), and a dynamic HTTP route carries its recorded `route_unknown_reason`
//! (never a bare `<dynamic route>` placeholder). No `unwrap_or`/`.ok()` collapses a
//! rendered or classified read to a fabricated default.
//!
//! Certainty honesty (review-5): a class's rendered hits must match the certainty
//! LAYER its parent tags it with. The `dependency` class (`extracted` = declared
//! manifest names) therefore emits ONLY declared-manifest categories and renders
//! `unavailable (<reason>)` when parsed-manifest provenance is unreadable/absent — a
//! Layer-2 observed-but-undeclared import is never laundered into an extracted fact.
//! See [`dependencies`].

use repo_graph_storage::StorageConnection;

use super::{finalize, like_fetch_limit, rank, ClassHits, FactHit, HitPath};

pub(super) fn symbols(
    storage: &StorageConnection,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    // FIND-RANK-1 (review-0 blocking fix): the AUTHORITATIVE symbol order is the pure
    // Rust comparator `rank`, whose precedence — non-test → KIND WEIGHT → match quality →
    // qualified-name length → path → stable_key — a bounded SQL window ordered by
    // (is_test, name ASC) does NOT reproduce. Under a 200-row window, 200+ lexically-early
    // non-test lesser-kind matches (e.g. `VARIABLE`s) could CROWD OUT a prominent
    // production `FUNCTION`/`CLASS` whose name sorts late — excluding it from the window
    // entirely, so the comparator never sees it and the visible cap is NOT the global
    // top-N (the contract's §2.1 guarantee). We therefore fetch the COMPLETE matching set
    // (`usize::MAX`) and rank all of it here; the comparator stays the SINGLE source of
    // truth (never duplicated into SQL), and the matched count is EXACT — the whole set
    // was seen, so it is never a `+N` floor. This is what `--exact`/`--full` already
    // fetched; default and full now differ ONLY in the display cap `finalize` applies.
    let rows = storage
        .find_fact_symbols(snapshot_uid, query, usize::MAX)
        .map_err(|e| format!("symbol fact read failed: {e}"))?;

    // Rank the complete set into the ratified display order. Borrow-only views over
    // `rows`; `finalize` then dedups + applies the display cap to the sorted hits.
    let mut views: Vec<rank::SymbolRank> = rows
        .iter()
        .map(|r| rank::SymbolRank {
            name: &r.name,
            qualified_name: r.qualified_name.as_deref(),
            is_test: r.is_test,
            subtype: r.subtype.as_deref(),
            path: r.path.as_deref(),
            stable_key: &r.stable_key,
            // FIND-EVIDENCE-1: the stored anchor line + the ONE evidence line derived
            // from stored facts (doc-comment first line, else signature) — computed ONCE
            // per row here, read only for its PRESENCE by the tie-break comparator.
            line: r.line,
            evidence: rank::evidence_line(r.doc_comment.as_deref(), r.signature.as_deref()),
        })
        .collect();
    rank::sort_symbols(&mut views, query);

    let hits = views
        .iter()
        .map(|v| FactHit {
            display: v
                .qualified_name
                .filter(|q| !q.is_empty())
                .unwrap_or(v.name)
                .to_string(),
            // A symbol ALWAYS belongs to a file; a `None` here is the LEFT JOIN
            // finding no `files` row for the node's `file_uid` — an UNKNOWN owning
            // file, surfaced with its reason, never a silent "no path" (review-4
            // item 2 / STANDING HONESTY RULE).
            path: match v.path {
                Some(p) => HitPath::Known(p.to_string()),
                None => HitPath::Unknown(
                    "owning file unresolved (no files row for this symbol's file_uid)".to_string(),
                ),
            },
            key: Some(v.stable_key.to_string()),
            // Class-level render command (`explain <key>`); no per-hit override.
            next_command: None,
            // FIND-EVIDENCE-1: the `path:line` anchor + the stored evidence line. The
            // symbol class is the only one carrying a per-symbol span/doc today.
            line: v.line,
            evidence: v.evidence.clone(),
        })
        .collect();
    // Never saturated: the whole matching set was fetched, so `matched` is EXACT.
    Ok(finalize(hits, full, false))
}

pub(super) fn files(
    storage: &StorageConnection,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    let limit = like_fetch_limit(full);
    let rows = storage
        .find_fact_files(snapshot_uid, query, limit)
        .map_err(|e| format!("file fact read failed: {e}"))?;
    let saturated = rows.len() >= limit;
    let hits = rows
        .into_iter()
        .map(|r| FactHit {
            display: r.path.clone(),
            path: HitPath::Known(r.path.clone()),
            key: Some(r.path),
            next_command: None,
            // File-granular: no per-symbol span or doc — the path IS the anchor.
            line: None,
            evidence: None,
        })
        .collect();
    Ok(finalize(hits, full, saturated))
}

pub(super) fn modules(
    storage: &StorageConnection,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    let limit = like_fetch_limit(full);
    let rows = storage
        .find_fact_modules(snapshot_uid, query, limit)
        .map_err(|e| format!("module fact read failed: {e}"))?;
    let saturated = rows.len() >= limit;
    let hits = rows
        .into_iter()
        .map(|r| FactHit {
            // `canonical_root_path` is NOT NULL in the row; the module always has a
            // declared root, so the path is always Known.
            display: r
                .display_name
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| r.canonical_root_path.clone()),
            path: HitPath::Known(r.canonical_root_path.clone()),
            key: Some(r.canonical_root_path),
            next_command: None,
            // Module hits are directory/declaration-granular — no per-symbol line/doc.
            line: None,
            evidence: None,
        })
        .collect();
    Ok(finalize(hits, full, saturated))
}

/// HTTP routes (§2.1): filter the SHARED unified HTTP surface set by substring over
/// method / route / source file — the same rows `boundaries`/`surfaces` render, so
/// a hit never depends on another renderer's budget (§2.1). Fetched whole then
/// filtered in Rust, so `matched` is exact (no floor).
pub(super) fn http_surfaces(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    let needle = query.to_lowercase();
    let rows = crate::http_boundary_read::unified_http_surfaces(storage, repo_uid, snapshot_uid)
        .map_err(|e| format!("http-surface fact read unavailable: {e}"))?;
    let hits = rows
        .into_iter()
        .filter(|r| {
            // An ABSENT route contributes nothing to the match — it is not coerced
            // to an empty string that would silently "match" a needle (review-4
            // item 2: no `unwrap_or("")` on a classified read). Method and source
            // file are always present.
            let route_matches = r
                .route
                .as_deref()
                .is_some_and(|route| route.to_lowercase().contains(&needle));
            r.http_method.to_lowercase().contains(&needle)
                || route_matches
                || r.source_file.to_lowercase().contains(&needle)
        })
        .map(|r| {
            // A dynamic/unreadable route renders the RECORDED reason, never a bare
            // `<dynamic route>` placeholder (review-4 item 2). When the reason itself
            // was not recorded upstream, say so — the labeled degraded form, never a
            // fabricated reason.
            let route = match (&r.route, &r.route_unknown_reason) {
                (Some(route), _) => route.clone(),
                (None, Some(reason)) => format!("<dynamic route — {reason}>"),
                (None, None) => "<dynamic route — reason not recorded upstream>".to_string(),
            };
            FactHit {
                display: format!("{} {} {}", r.direction, r.http_method.to_uppercase(), route),
                path: HitPath::Known(r.source_file),
                key: None,
                next_command: None,
                // Route hits carry a source file, not a stored symbol span/doc.
                line: None,
                evidence: None,
            }
        })
        .collect();
    Ok(finalize(hits, full, false))
}

/// Dependency names (§2.1): the DECLARED-MANIFEST package names the SAME deps compose
/// `deps list` runs produces, filtered by substring. Reuses the interactive command's
/// read so the names cannot diverge; the ecosystem/manifest preamble mirrors
/// `handle_deps_list`.
///
/// Certainty honesty (review-5): this class is tagged `extracted`, whose SOURCE the
/// slice defines as "the declared manifest package names". Two guards keep the tag
/// truthful:
///   1. CATEGORY FILTER — only [`DeclaredAndUsed`]/[`DeclaredButUnobserved`] entries
///      (the packages actually present in a manifest) are emitted. The reconciler also
///      yields `ObservedButUndeclared` (imported, NOT in any manifest — a Layer-2
///      inference), `UnknownExternalLike` (unclassifiable), and `RuntimeBuiltin`; none
///      of those is a declared manifest name, so presenting them as `extracted` would
///      lie about certainty (VISION "never describe Layers 2–4 as Layer 0 truth").
///   2. PROVENANCE GUARD — if the parsed-manifest provenance could not be read
///      (`Unavailable` — corrupt/unreadable diagnostics) OR the snapshot predates
///      provenance tracking (`Absent`), we cannot attest which names came from a
///      manifest, so the whole class renders `unavailable (<reason>)` rather than a
///      possibly-misleading declared set (STANDING HONESTY RULE — a class we cannot
///      honestly source is surfaced with its reason, never a fabricated-looking empty).
///
/// [`DeclaredAndUsed`]: repo_graph_module_queries::DependencyCategory::DeclaredAndUsed
/// [`DeclaredButUnobserved`]: repo_graph_module_queries::DependencyCategory::DeclaredButUnobserved
pub(super) fn dependencies(
    storage: &StorageConnection,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    use repo_graph_agent::AgentStorageRead;
    use repo_graph_module_queries::{
        compose_dependency_summaries, deps_runtime_builtins, ComposeDependenciesInput,
        DependencyCategory, ProvenanceRead,
    };

    let needle = query.to_lowercase();
    let language_counts = storage
        .query_file_count_by_language(snapshot_uid)
        .map_err(|e| format!("dependency fact read unavailable: {e}"))?;
    let ecosystem = crate::reader_context::dominant_deps_ecosystem(&language_counts).to_string();
    let runtime_builtins = deps_runtime_builtins(&ecosystem);
    let manifest_provenance = crate::deps_headline::read_manifest_provenance(storage, snapshot_uid);

    // PROVENANCE GUARD (review-5 item 2): without readable parsed-manifest provenance
    // the `extracted` certainty of a "declared manifest name" cannot be honored — the
    // class is unavailable-with-reason, not an honest-looking empty.
    match &manifest_provenance {
        ProvenanceRead::Tracked(_) => {}
        ProvenanceRead::Unavailable { reason } => {
            return Err(format!("manifest provenance unavailable: {reason}"));
        }
        ProvenanceRead::Absent => {
            return Err("manifest provenance unavailable: snapshot indexed before \
                 manifest-provenance tracking"
                .to_string());
        }
    }

    let input = ComposeDependenciesInput {
        snapshot_uid,
        runtime_builtins,
        ecosystem,
        manifest_provenance,
    };
    let result = compose_dependency_summaries(storage, &input)
        .map_err(|e| format!("dependency fact read unavailable: {e}"))?;

    let hits = result
        .summaries
        .into_iter()
        .flat_map(|s| s.entries.into_iter())
        // CATEGORY FILTER (review-5 item 1): only declared-manifest categories are the
        // `extracted` fact this class claims to be.
        .filter(|e| {
            matches!(
                e.category,
                DependencyCategory::DeclaredAndUsed | DependencyCategory::DeclaredButUnobserved
            )
        })
        .map(|e| e.package)
        .filter(|p| p.to_lowercase().contains(&needle))
        .map(|package| FactHit {
            display: package.clone(),
            // A dependency package is not file-anchored: the class has no path
            // dimension (never an unknown-with-reason — there is nothing to know).
            path: HitPath::None,
            key: Some(package),
            next_command: None,
            // A manifest package name carries no source span or doc-comment.
            line: None,
            evidence: None,
        })
        .collect();
    Ok(finalize(hits, full, false))
}

/// Framework inference identifiers (§2.1): the DISTINCT inference kinds recorded for
/// the snapshot (`react_component`, `spring_container_managed`, …), matched by
/// substring. The identifier IS the stored fact; each hit carries its occurrence
/// count. Rendered by `inferences`.
pub(super) fn frameworks(
    storage: &StorageConnection,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    use std::collections::BTreeMap;

    let needle = query.to_lowercase();
    let rows = storage
        .list_inferences_for_snapshot(snapshot_uid, None)
        .map_err(|e| format!("framework fact read unavailable: {e}"))?;
    let mut per_kind: BTreeMap<String, u64> = BTreeMap::new();
    for r in rows {
        *per_kind.entry(r.kind).or_insert(0) += 1;
    }
    let hits = per_kind
        .into_iter()
        .filter(|(kind, _)| kind.to_lowercase().contains(&needle))
        .map(|(kind, count)| FactHit {
            display: format!("{kind} ({count} occurrence(s))"),
            // A framework inference kind spans many files: no single owning path.
            path: HitPath::None,
            key: Some(kind),
            next_command: None,
            // A framework inference kind spans many files: no single span/doc.
            line: None,
            evidence: None,
        })
        .collect();
    Ok(finalize(hits, full, false))
}

/// Governance DECLARATIONS (§2.1, review-6 re-home — operator-ratified 2026-08-30):
/// the ACTIVE boundary/requirement/quality-policy declarations whose stored
/// `target_stable_key` matches, each pointing at the command that RENDERS that
/// declaration kind — `boundary` → `rmap violations`, `requirement`/`quality_policy`
/// → `rmap gate` (both verified renderers). This REPLACES the former
/// `surface_entrypoints` corpus, which had NO renderer: its emitted `surfaces list`
/// next-command exited without ever showing the entrypoint, dead-ending the reader.
///
/// Per-hit next command (unlike the other six classes) VARIES by the row's declaration
/// kind, so each hit carries its own `next_command`; the group therefore has no single
/// class-level render command (see [`super::FactClass::render_command`]). All three
/// renderers are whole-listing commands, so no per-hit argument is folded in
/// (`key: None`), and a declaration is not file-anchored (`HitPath::None`).
///
/// Repo-scoped: declarations carry a NULL `snapshot_uid` (authored inputs, not snapshot
/// facts), so this passes `repo_uid`, matching what `violations`/`gate` read.
pub(super) fn boundary_declarations(
    storage: &StorageConnection,
    repo_uid: &str,
    query: &str,
    full: bool,
) -> Result<ClassHits, String> {
    let limit = like_fetch_limit(full);
    let rows = storage
        .find_fact_declarations(repo_uid, query, limit)
        .map_err(|e| format!("boundary fact read failed: {e}"))?;
    let saturated = rows.len() >= limit;
    // Kind → (display label, RENDERING command). This local match — not a `GovDeclKind`
    // type — is the smallest honest form: one call site, and the storage `IN (...)`
    // filter already restricts the set, so the `_` arm is a code-vs-SQL divergence
    // GUARD, not a live path. It surfaces the whole class as unavailable-with-reason
    // (never silently dropping the row) if the two ever fall out of sync (STANDING
    // HONESTY RULE — a fact we cannot honestly render is surfaced with its reason).
    let mut hits = Vec::with_capacity(rows.len());
    for r in rows {
        let (label, command): (&str, &'static str) = match r.kind.as_str() {
            "boundary" => ("boundary", "violations"),
            "requirement" => ("requirement", "gate"),
            "quality_policy" => ("quality-policy", "gate"),
            other => {
                return Err(format!(
                    "boundary fact read returned an unrenderable declaration kind '{other}' \
                     (storage kind filter and the renderer map diverged)"
                ));
            }
        };
        hits.push(FactHit {
            // Pointer identity: the declaration KIND + its stored target key. `find`
            // teaches the next move; `violations`/`gate` render the full declaration.
            display: format!("{label} declaration · {}", r.target_stable_key),
            // A governance declaration is not anchored to one source file — no path
            // dimension (never an unknown-with-reason; there is nothing to know).
            path: HitPath::None,
            key: None,
            // The renderer for THIS declaration kind — the per-hit next move.
            next_command: Some(command),
            // A governance declaration is not anchored to a source span/doc-comment.
            line: None,
            evidence: None,
        });
    }
    Ok(finalize(hits, full, saturated))
}
