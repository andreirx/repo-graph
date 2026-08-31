//! `deps list` headline + provenance read (DEPS-LIST-REWRITE-1 §2.2/§2.3).
//!
//! Crate-private pure helpers factored OUT of the 9371-line `dispatch.rs` (gap 7 — new logic
//! goes in its own module, never grows the god-file):
//!
//! 1. [`read_manifest_provenance`] — reads the persisted parsed-manifest records from the
//!    extraction-diagnostics blob (the `deps_manifests` key, written before the Ready flip; mirrors
//!    `index_basis_probe::read_basis_outcome`). Returns a [`ProvenanceRead`] quad-state that keeps
//!    the "no exact file" causes distinct (predates tracking vs. read failure vs. corruption).
//! 2. [`compute_unattributed`] — the §2.3 unattributed-imports headline (the number that replaces
//!    glamCRM's false `count: 0`), pure over the compose result.
//! 3. [`ResolutionState`] + [`build_deps_list_response`]/[`module_json`] — the §2.4 tri-state
//!    resolution posture (downgraded / clean / UNKNOWN-with-reason) and the additive JSON payload.
//! 4. [`manifest_context_json`] — maps the `ManifestContext` domain enum to the additive-compatible
//!    JSON (`manifest_path` string|null + the specific `manifest_context` reason when unavailable).

use repo_graph_module_queries::{
    ComposeDependenciesResult, DependencyCategory, ManifestContext, ManifestProvenance,
    ModuleDependencySummary, ProvenanceRead,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::TrustStorageRead;

use crate::deps_coverage::{CoverageStatus, ManifestCoverage};
use crate::deps_ecosystem_presence::{ecosystem_presence_json, EcosystemPresence};
use crate::reader_context::deps_reader_context_note;

/// Diagnostics-blob key the index writes parsed-manifest provenance under (mirror of
/// `repo_index::compose::DEPS_MANIFESTS_DIAG_KEY` — the wire contract across the index/query split).
const DEPS_MANIFESTS_DIAG_KEY: &str = "deps_manifests";

/// Diagnostics-blob key the index writes the PRESENT-manifest denominator under (mirror of
/// `repo_index::compose::DEPS_MANIFESTS_PRESENT_DIAG_KEY`). A `{ecosystem: count}` map, kept
/// separate from `deps_manifests` so a scanned-but-unparsed manifest is never counted as parsed.
const DEPS_MANIFESTS_PRESENT_DIAG_KEY: &str = "deps_manifests_present";

/// Read the persisted parsed-manifest provenance for a snapshot (§2.2; operator rulings 2026-08-26
/// and ruling 3 item 2). The FOUR outcomes are kept distinct — an old snapshot must never be
/// reported with a corrupt-blob's reason, nor vice versa:
///
/// - diagnostics blob unreadable → [`ProvenanceRead::Unavailable`] (its own read-failure reason).
/// - blob ABSENT, or present but WITHOUT the `deps_manifests` key → [`ProvenanceRead::Absent`]:
///   the snapshot predates provenance tracking → renders "indexed before provenance tracking".
/// - blob not valid JSON, or the `deps_manifests` value malformed → [`ProvenanceRead::Unavailable`]
///   (corruption reason — NOT "predates tracking").
/// - key present and well-formed → [`ProvenanceRead::Tracked`] (possibly empty = "tracked, none").
///
/// A `Unavailable` cause is never silently fabricated into a path (standing honesty rule).
pub(crate) fn read_manifest_provenance(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> ProvenanceRead {
    let blob = match TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("warning: deps provenance — extraction diagnostics unreadable ({e})");
            return ProvenanceRead::Unavailable {
                reason: format!("extraction diagnostics unreadable: {e}"),
            };
        }
    };
    parse_manifest_provenance(blob.as_deref())
}

/// Pure parse of the `deps_manifests` array from a diagnostics blob (unit-testable without storage).
/// Distinguishes predates-tracking (absent) from corruption (unreadable) per ruling 3 item 2.
fn parse_manifest_provenance(blob: Option<&str>) -> ProvenanceRead {
    let s = match blob {
        Some(s) => s,
        None => return ProvenanceRead::Absent, // no diagnostics at all — predates tracking.
    };
    let value: serde_json::Value = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: deps provenance — diagnostics not valid JSON ({e})");
            return ProvenanceRead::Unavailable {
                reason: format!("extraction diagnostics not valid JSON: {e}"),
            };
        }
    };
    let entry = match value.get(DEPS_MANIFESTS_DIAG_KEY) {
        Some(e) => e,
        // Blob exists (basis/other diagnostics) but no deps_manifests key — predates tracking.
        None => return ProvenanceRead::Absent,
    };
    match serde_json::from_value::<Vec<ManifestProvenance>>(entry.clone()) {
        Ok(records) => ProvenanceRead::Tracked(records),
        Err(e) => {
            eprintln!("warning: deps provenance — deps_manifests malformed ({e})");
            ProvenanceRead::Unavailable {
                reason: format!("provenance record malformed: {e}"),
            }
        }
    }
}

/// The scanned-manifest DENOMINATOR read outcome for one ecosystem (the §2.2 / ruling-3 item-4
/// workspace-coverage line). A sum type, NOT `Option<usize>`: this storage read is fallible AND its
/// result feeds a RENDERED coverage line, so a read FAILURE must surface as unknown-with-reason — it
/// can never collapse into the same `None` as a legitimate not-applicable absence and then be
/// laundered by the caller into a fabricated denominator (STANDING HONESTY RULE #1; DEPS-ATTRIB-2
/// review-3 / operator ruling 2026-08-31, closing the 15th recurrence of the fallible-read → `None`
/// → fabricated-count class — the prior code read this blob a SECOND time, independently of
/// [`read_manifest_provenance`], and a failure of THAT read was silently laundered into the parsed
/// total by `build_deps_list_response`).
///
/// Abstraction (pre-ratified crate-private sum type): WHAT = the present-denominator read outcome;
/// CALLERS = [`read_manifests_present`]/[`parse_manifests_present`] (produce), [`build_deps_list_response`]
/// (consume); AXIS = a fallible read feeding a RENDERED line grows operations over a FIXED outcome
/// set → sum type + exhaustive match, a read failure a DISTINCT fact from a legitimate absence;
/// REJECTED SIMPLER = `Option<usize>`, which merges failure and absence into one `None` (the exact
/// defect being fixed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PresentDenominator {
    /// Exactly `n` manifests of this ecosystem were scanned on disk (the definite denominator).
    Count(usize),
    /// No denominator applies AND no read failed: the diagnostics blob is absent (snapshot predates
    /// provenance tracking — io NotFound is the only "absent"), the present-manifest key is absent
    /// (snapshot predates the present-count sub-record), or this ecosystem is absent from the map.
    /// The coverage line falls back to the parsed-manifest total it DID compute (honest byte-parity
    /// for co-written snapshots — the index co-writes the present count alongside the parsed records).
    NotApplicable,
    /// The denominator read FAILED — the diagnostics blob was unreadable (storage error), its JSON
    /// was corrupt, or this ecosystem's count value was non-integer. The scanned total is genuinely
    /// UNKNOWN; the caller renders unknown-with-reason, never a fabricated count.
    Unavailable { reason: String },
}

/// Read the scanned-manifest denominator for `ecosystem` (the §2.2 / ruling-3 item-4 coverage line):
/// the count of manifests SCANNED on disk — distinct from the PARSED-provenance record, so a
/// scanned-but-never-dep-parsed workspace `package.json` (the amodx 9-of-43 gap) counts toward the
/// denominator WITHOUT being laundered into the parsed record (review-3 item 2).
///
/// A storage read failure is [`PresentDenominator::Unavailable`] — NOT a `None` the caller fabricates
/// over (review-3 / operator ruling 2026-08-31). Only io-NotFound (blob absent) is a legitimate
/// [`PresentDenominator::NotApplicable`] absence. This read is independent of [`read_manifest_provenance`];
/// its failure is surfaced by THIS return value, never assumed to be caught by the sibling read.
pub(crate) fn read_manifests_present(
    storage: &StorageConnection,
    snapshot_uid: &str,
    ecosystem: &str,
) -> PresentDenominator {
    let blob = match TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("warning: deps coverage — extraction diagnostics unreadable ({e})");
            return PresentDenominator::Unavailable {
                reason: format!("scanned-manifest count unreadable: {e}"),
            };
        }
    };
    parse_manifests_present(blob.as_deref(), ecosystem)
}

/// Pure parse of one ecosystem's present-manifest count from a diagnostics blob (unit-testable).
/// Distinguishes a legitimate absence ([`PresentDenominator::NotApplicable`] — blob / present-key /
/// ecosystem absent) from corruption ([`PresentDenominator::Unavailable`] — a non-JSON blob or a
/// present-but-non-integer count value): a fallible read feeding a rendered line never merges a
/// failure into an absence (STANDING HONESTY RULE #1; review-3).
fn parse_manifests_present(blob: Option<&str>, ecosystem: &str) -> PresentDenominator {
    let s = match blob {
        Some(s) => s,
        None => return PresentDenominator::NotApplicable, // blob absent — predates tracking.
    };
    let value: serde_json::Value = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: deps coverage — present-count diagnostics not valid JSON ({e})");
            return PresentDenominator::Unavailable {
                reason: format!("scanned-manifest count: diagnostics not valid JSON: {e}"),
            };
        }
    };
    let present = match value.get(DEPS_MANIFESTS_PRESENT_DIAG_KEY) {
        Some(p) => p,
        None => return PresentDenominator::NotApplicable, // key absent — predates present tracking.
    };
    match present.get(ecosystem) {
        None => PresentDenominator::NotApplicable, // ecosystem absent from the map — no read failed.
        Some(v) => match v.as_u64() {
            Some(n) => PresentDenominator::Count(n as usize),
            None => PresentDenominator::Unavailable {
                reason: format!(
                    "scanned-manifest count for {ecosystem} malformed (not a non-negative integer)"
                ),
            },
        },
    }
}

/// Resolution-state posture for the §2.4 honesty label (operator ruling 3 item 1). A tri-state, NOT
/// a bool: a failed trust-overlay read is UNKNOWN-with-reason, never silently rendered as "clean"
/// (which would restate the audit's false-certainty case for `@fraktag/engine`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolutionState {
    /// `trust`'s alias/workspace downgrade is active on this snapshot — declared-unobserved rows may
    /// be UNRESOLVED imports, so they render with the honesty label and capped confidence.
    Downgraded,
    /// The overlay was assembled and no downgrade applies — full confidence is honest.
    Clean,
    /// The trust overlay could not be assembled — resolution state is genuinely UNKNOWN. Rows are
    /// capped and labelled rather than asserting certainty we do not have.
    Unknown { reason: String },
}

impl ResolutionState {
    /// The stable JSON tag for this posture.
    fn tag(&self) -> &'static str {
        match self {
            ResolutionState::Downgraded => "downgraded",
            ResolutionState::Clean => "clean",
            ResolutionState::Unknown { .. } => "unknown",
        }
    }

    /// Whether a `declared_but_unobserved` row's certainty must be capped + labelled: true for both
    /// an active downgrade and an unknown state (neither may assert 1.0 "declared and truly unused").
    fn caps_declared_unobserved(&self) -> bool {
        !matches!(self, ResolutionState::Clean)
    }

    /// The per-entry honesty note for a capped `declared_but_unobserved` row.
    fn entry_note(&self) -> Option<&'static str> {
        match self {
            ResolutionState::Downgraded => Some("declared — imports not resolved on this index"),
            ResolutionState::Unknown { .. } => {
                Some("declared — resolution state unknown on this index")
            }
            ResolutionState::Clean => None,
        }
    }
}

/// The §2.3 unattributed-imports headline.
pub(crate) struct Unattributed {
    /// External references not attributed to any parsed manifest (the headline number).
    pub count: usize,
    /// Human/JSON reason string (the "WHY" the slice requires beside the count).
    pub reason: String,
}

/// Compute the §2.3 headline over the WHOLE repo (before any module filter) so a per-module view
/// never hides repo-level unattributed imports.
///
/// `scoped_classified` = references the reconciler placed under a manifest-scoped module;
/// `total_rejected` = call-expression text the §2.1 gate dropped (never imports). What remains is
/// imports whose files sit outside any parsed manifest scope — surfaced instead of a false `0`.
/// For `none-detected` (no manifest reader ran) the whole external-import count is unattributed.
pub(crate) fn compute_unattributed(
    result: &ComposeDependenciesResult,
    ecosystem: &str,
    repo_languages: &[String],
) -> Unattributed {
    let scoped_classified: usize = result
        .summaries
        .iter()
        .filter(|s| s.manifest_scope_available)
        .flat_map(|s| s.entries.iter())
        .map(|e| e.import_count)
        .sum();
    let total_rejected: usize = result
        .summaries
        .iter()
        .map(|s| s.rejected_non_specifier)
        .sum();
    let unattributed = result
        .total_external_imports
        .saturating_sub(scoped_classified + total_rejected);

    if ecosystem == "none-detected" {
        return Unattributed {
            count: result.total_external_imports,
            reason: deps_reader_context_note(repo_languages, result.total_external_imports),
        };
    }
    let reason = if unattributed > 0 {
        format!(
            "{} of {} external references not attributed to a declared manifest \
             (imported files outside a parsed manifest scope)",
            unattributed, result.total_external_imports
        )
    } else {
        "all external references attributed or classified".to_string()
    };
    Unattributed {
        count: unattributed,
        reason,
    }
}

/// Sum of `rejected_non_specifier` across all summaries (the §2.1 dropped-fragment total, surfaced
/// so the headline math is auditable).
pub(crate) fn total_rejected(result: &ComposeDependenciesResult) -> usize {
    result
        .summaries
        .iter()
        .map(|s| s.rejected_non_specifier)
        .sum()
}

/// Map the `ManifestContext` domain enum to additive-compatible JSON fields:
/// `(manifest_path: Option<String>, manifest_context_note: Option<String>)`.
/// Parsed → exact path; ProvenanceUnavailable → null path + the SPECIFIC unknown-with-reason note
/// (ruling 3 item 2 — predates / read-failure / corruption carried verbatim); Absent → null + none.
pub(crate) fn manifest_context_json(ctx: &ManifestContext) -> (Option<String>, Option<String>) {
    (
        ctx.path().map(|p| p.to_string()),
        ctx.unavailable_note().map(|n| n.to_string()),
    )
}

/// Assemble the full `deps list` JSON payload (DEPS-LIST-REWRITE-1 §2.2/§2.3/§2.4/§2.5).
///
/// Factored out of `dispatch::handle_deps_list` so the dispatch arm stays wiring (guardrail: this
/// slice does not grow the 9k-line `dispatch.rs`). Pure over the compose result plus the already-
/// computed headline/downgrade inputs — no storage or daemon access. Emits the §2.3 unattributed
/// headline as the first meaningful envelope fields, the §2.4 per-entry resolution label with
/// capped confidence, and the §2.2 exact `manifest_path` (or the unknown-with-reason note), never a
/// fabricated path. `result` is consumed — its `summaries` move into the payload.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_deps_list_response(
    repo_uid: &str,
    snapshot_uid: &str,
    ecosystem: &str,
    repo_languages: &[String],
    result: ComposeDependenciesResult,
    module_filter: Option<&str>,
    resolution: &ResolutionState,
    unattributed: &Unattributed,
    total_rejected: usize,
    manifests_present: &PresentDenominator,
    manifest_coverage: Option<CoverageStatus>,
    other_ecosystems: &[EcosystemPresence],
) -> serde_json::Value {
    let total_external_imports = result.total_external_imports;

    // Filter to a specific module if requested (same match rule as before the extraction).
    let summaries: Vec<ModuleDependencySummary> = if let Some(filter) = module_filter {
        result
            .summaries
            .into_iter()
            .filter(|s| {
                s.module == filter
                    || s.module.ends_with(&format!("/{filter}"))
                    || s.module.starts_with(&format!("{filter}/"))
            })
            .collect()
    } else {
        result.summaries
    };

    let results: Vec<serde_json::Value> = summaries
        .iter()
        .map(|s| module_json(s, resolution))
        .collect();
    let count = results.len();

    let mut response = serde_json::json!({
        "command": "deps list",
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        // §2.3: the unattributed headline leads the payload.
        "unattributed_external_imports": unattributed.count,
        "unattributed_reason": unattributed.reason,
        // §2.4: resolution posture is a tri-state tag, not a lone bool — `resolution_downgraded`
        // stays additive-compatible (true ONLY when actually downgraded), while `resolution_state`
        // distinguishes "clean" from "unknown" so a failed overlay read is never read as certainty.
        "resolution_state": resolution.tag(),
        "resolution_downgraded": *resolution == ResolutionState::Downgraded,
        "results": results,
        "count": count,
        "ecosystem": ecosystem,
        "total_external_imports": total_external_imports,
        "rejected_non_specifier_total": total_rejected,
    });

    if let serde_json::Value::Object(ref mut map) = response {
        // §2.4: carry the specific reason when the resolution state is unknown.
        if let ResolutionState::Unknown { reason } = resolution {
            map.insert(
                "resolution_note".to_string(),
                serde_json::json!(format!("resolution-state unknown ({reason})")),
            );
        }
        // Workspace coverage — whole-repo view only (a per-module drill would make the present-vs-
        // attributed ratio misleading). Driven by `manifest_coverage` (review-1 item 2 — a read that
        // FEEDS the split failing must SURFACE, never silently drop the whole line):
        //   Computed → render the split, using the scanned `manifests_present` denominator when known,
        //     else the parsed total we DID compute (never a fabricated 0; present and the provenance
        //     records are co-written by the index, so a Computed result virtually always has a Some
        //     present — the fallback only guards a corrupt present sub-field, and still tells the truth).
        //   Unknown → the provenance blob or the owned-files read failed → render unknown-WITH-REASON,
        //     carrying the present denominator if we still know it, none otherwise. NEVER a silent
        //     omission, NEVER a false 0 (operator binding 2026-08-31; STANDING HONESTY RULE #1).
        //   None → computed-known not-applicable (old snapshot / no manifests) → omit (byte-parity).
        if module_filter.is_none() {
            match &manifest_coverage {
                Some(CoverageStatus::Computed(cov)) => match manifests_present {
                    // Definite scanned denominator → render the split against it.
                    PresentDenominator::Count(p) => insert_coverage_split(map, *p, cov),
                    // No scanned count tracked (old snapshot) and no read failed → fall back to the
                    // parsed total we DID compute (honest byte-parity; the index co-writes the present
                    // count with the parsed records, so this only guards a pre-present-key snapshot).
                    PresentDenominator::NotApplicable => {
                        insert_coverage_split(map, cov.total_parsed(), cov)
                    }
                    // The scanned-count read FAILED → the denominator is genuinely UNKNOWN. Do NOT
                    // fabricate it from the parsed total (that was the review-3 defect); surface the
                    // coverage as unknown-with-reason (STANDING HONESTY RULE #1; operator ruling
                    // 2026-08-31). The computed split is WITHHELD — an "X of ?" ratio has no honest
                    // denominator to state.
                    PresentDenominator::Unavailable { reason } => {
                        map.insert(
                            "manifests_coverage_unavailable".to_string(),
                            serde_json::json!(reason),
                        );
                    }
                },
                Some(CoverageStatus::Unknown { reason }) => {
                    map.insert(
                        "manifests_coverage_unavailable".to_string(),
                        serde_json::json!(reason),
                    );
                    // Carry the scanned denominator only when we DEFINITELY have it (a definite
                    // count); an absence or a failed present-read contributes no number here.
                    if let PresentDenominator::Count(present) = manifests_present {
                        if *present > 0 {
                            map.insert("manifests_present".to_string(), serde_json::json!(present));
                        }
                    }
                }
                None => {}
            }
        }

        // DEPS-ATTRIB-2 §2.4 (ruling DR-JAVA-NOREADER = Option 2): the DEFAULT view states the truth
        // of every materially-present ecosystem OTHER than the rendered dominant one — attributed
        // deps, unknown-with-reason, or computed-true absence — so a materially-present ecosystem
        // (glamCRM's Java half) is never SILENTLY absent. Empty on a single-ecosystem repo (django,
        // FRAKTAG) → byte-parity. Only the whole-repo default view carries it (never a module drill).
        if module_filter.is_none() && !other_ecosystems.is_empty() {
            map.insert(
                "other_ecosystems".to_string(),
                serde_json::json!(other_ecosystems
                    .iter()
                    .map(ecosystem_presence_json)
                    .collect::<Vec<_>>()),
            );
        }
    }

    // HONEST-DEGRADATION-IMPL-2 (D2): reader-context note for a no-manifest-reader language,
    // retained (additive) alongside the §2.3 headline.
    if ecosystem == "none-detected" {
        if let serde_json::Value::Object(ref mut map) = response {
            map.insert(
                "reader_context".to_string(),
                serde_json::json!(deps_reader_context_note(
                    repo_languages,
                    total_external_imports
                )),
            );
        }
    }
    if let Some(m) = module_filter {
        if let serde_json::Value::Object(ref mut map) = response {
            map.insert("module_filter".to_string(), serde_json::json!(m));
        }
    }
    response
}

/// Insert the §2.3 coverage split (`manifests_present` denominator + `manifests_attributed` +
/// `manifests_no_indexed_source`) when the denominator is non-zero; a zero denominator omits the line
/// (old snapshot / no manifests — byte-parity). Shared by the definite-count and parsed-total-fallback
/// arms so the two honest paths render identically. `manifests_no_indexed_source` is the ONLY count
/// that may render as "govern no indexed source" (parsed manifests computed to have zero indexed files
/// under them); the remainder of `present - attributed` (scanned-but-unparsed manifests) renders as its
/// own honest clause in the presenter, never laundered into the excuse.
fn insert_coverage_split(
    map: &mut serde_json::Map<String, serde_json::Value>,
    denominator: usize,
    cov: &ManifestCoverage,
) {
    if denominator == 0 {
        return;
    }
    map.insert(
        "manifests_present".to_string(),
        serde_json::json!(denominator),
    );
    map.insert(
        "manifests_attributed".to_string(),
        serde_json::json!(cov.attributed),
    );
    map.insert(
        "manifests_no_indexed_source".to_string(),
        serde_json::json!(cov.no_indexed_source),
    );
    // review-4 blocker 2 / §2.3: parsed manifests with indexed source that no module owns. Emitted
    // ONLY when non-zero — additive, and keeps the JSON byte-stable on the common (all-attributed)
    // path where an older consumer / capture never saw these keys. When present, the presenter renders
    // the honest "indexed source not attributed" clause instead of laundering these into the excuse.
    if cov.indexed_unattributed_manifests > 0 {
        map.insert(
            "manifests_indexed_unattributed".to_string(),
            serde_json::json!(cov.indexed_unattributed_manifests),
        );
        map.insert(
            "manifests_indexed_unattributed_files".to_string(),
            serde_json::json!(cov.indexed_unattributed_files),
        );
    }
}

/// One module's JSON object: §2.2 exact `manifest_path`/`manifest_context` and §2.4 per-entry
/// resolution labels + capped confidence (downgrade OR unknown, never silent certainty).
fn module_json(s: &ModuleDependencySummary, resolution: &ResolutionState) -> serde_json::Value {
    let caps = resolution.caps_declared_unobserved();
    let note = resolution.entry_note();
    let entries: Vec<serde_json::Value> = s
        .entries
        .iter()
        .map(|e| {
            let cap_this = caps && e.category == DependencyCategory::DeclaredButUnobserved;
            let confidence = if cap_this {
                e.confidence.min(0.5)
            } else {
                e.confidence
            };
            let mut obj = serde_json::json!({
                "package": e.package,
                "category": format_category(e.category),
                "import_count": e.import_count,
                "confidence": confidence,
            });
            if cap_this {
                if let (serde_json::Value::Object(ref mut m), Some(note)) = (&mut obj, note) {
                    m.insert("resolution_note".to_string(), serde_json::json!(note));
                }
            }
            obj
        })
        .collect();

    let (manifest_path, manifest_context_note) = manifest_context_json(&s.manifest_context);
    let mut module_obj = serde_json::json!({
        "module": s.module,
        "manifest_path": manifest_path,
        "manifest_scope_available": s.manifest_scope_available,
        "declared_and_used": s.declared_and_used_count(),
        "declared_but_unobserved": s.declared_but_unobserved_count(),
        "observed_but_undeclared": s.observed_but_undeclared_count(),
        "runtime_builtins": s.runtime_builtins_count(),
        "unknown_external_like": s.unknown_external_like_count(),
        "rejected_non_specifier": s.rejected_non_specifier,
        "entries": entries,
    });
    if let Some(note) = manifest_context_note {
        if let serde_json::Value::Object(ref mut m) = module_obj {
            m.insert("manifest_context".to_string(), serde_json::json!(note));
        }
    }
    module_obj
}

/// Map a `DependencyCategory` to its stable JSON tag.
fn format_category(cat: DependencyCategory) -> &'static str {
    match cat {
        DependencyCategory::DeclaredAndUsed => "declared_and_used",
        DependencyCategory::DeclaredButUnobserved => "declared_but_unobserved",
        DependencyCategory::ObservedButUndeclared => "observed_but_undeclared",
        DependencyCategory::RuntimeBuiltin => "runtime_builtin",
        DependencyCategory::UnknownExternalLike => "unknown_external_like",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps_coverage::ManifestCoverage;

    #[test]
    fn absent_blob_and_absent_key_are_absent_not_unavailable() {
        // Ruling 3 item 2: predates-tracking is its OWN state, distinct from a read failure.
        assert_eq!(parse_manifest_provenance(None), ProvenanceRead::Absent);
        assert_eq!(
            parse_manifest_provenance(Some(r#"{"edges_total":3}"#)),
            ProvenanceRead::Absent
        );
    }

    #[test]
    fn present_key_parses_records_including_empty() {
        let blob =
            r#"{"deps_manifests":[{"path":"a/build.gradle.kts","dir":"a","ecosystem":"java"}]}"#;
        let recs = match parse_manifest_provenance(Some(blob)) {
            ProvenanceRead::Tracked(r) => r,
            other => panic!("expected Tracked, got {other:?}"),
        };
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].path, "a/build.gradle.kts");
        assert_eq!(recs[0].ecosystem, "java");
        // Empty list = "tracked, none parsed" — Tracked([]), NOT Absent.
        assert_eq!(
            parse_manifest_provenance(Some(r#"{"deps_manifests":[]}"#)),
            ProvenanceRead::Tracked(vec![])
        );
    }

    fn summary(
        module: &str,
        ctx: ManifestContext,
        declared_unobserved: &[&str],
    ) -> ModuleDependencySummary {
        let entries = declared_unobserved
            .iter()
            .map(|p| repo_graph_module_queries::DependencyEntry {
                package: p.to_string(),
                category: DependencyCategory::DeclaredButUnobserved,
                import_count: 0,
                dependency_class: None,
                confidence: 1.0,
                raw_specifiers: vec![],
            })
            .collect();
        ModuleDependencySummary {
            module: module.to_string(),
            manifest_context: ctx,
            manifest_scope_available: true,
            entries,
            rejected_non_specifier: 0,
        }
    }

    fn result_of(summaries: Vec<ModuleDependencySummary>) -> ComposeDependenciesResult {
        ComposeDependenciesResult {
            summaries,
            total_external_imports: 3,
        }
    }

    #[test]
    fn payload_omits_prohibited_field_and_carries_tri_state() {
        // Ruling 3 item 5: `modules_without_manifest_context` is deleted from the payload entirely.
        let res = result_of(vec![summary(
            "app",
            ManifestContext::Parsed {
                path: "app/package.json".into(),
            },
            &[],
        )]);
        let un = Unattributed {
            count: 0,
            reason: "all attributed".into(),
        };
        let payload = build_deps_list_response(
            "repo",
            "snap",
            "npm",
            &[],
            res,
            None,
            &ResolutionState::Clean,
            &un,
            0,
            &PresentDenominator::Count(1),
            Some(CoverageStatus::Computed(ManifestCoverage {
                attributed: 1,
                no_indexed_source: 0,
                indexed_unattributed_manifests: 0,
                indexed_unattributed_files: 0,
            })),
            &[],
        );
        let obj = payload.as_object().unwrap();
        assert!(
            !obj.contains_key("modules_without_manifest_context"),
            "prohibited field present: {payload}"
        );
        assert_eq!(obj.get("resolution_state").unwrap(), "clean");
        assert_eq!(obj.get("resolution_downgraded").unwrap(), false);
        assert_eq!(obj.get("manifests_present").unwrap(), 1);
        assert_eq!(obj.get("manifests_attributed").unwrap(), 1);
        assert_eq!(obj.get("manifests_no_indexed_source").unwrap(), 0);
    }

    #[test]
    fn unknown_resolution_caps_confidence_and_labels_per_entry() {
        // Ruling 3 item 1: unknown resolution state caps declared-unobserved certainty + labels it,
        // never asserts 1.0.
        let res = result_of(vec![summary(
            "app",
            ManifestContext::Parsed {
                path: "app/package.json".into(),
            },
            &["leftpad"],
        )]);
        let un = Unattributed {
            count: 0,
            reason: "x".into(),
        };
        let payload = build_deps_list_response(
            "repo",
            "snap",
            "npm",
            &[],
            res,
            None,
            &ResolutionState::Unknown {
                reason: "overlay read failed: disk".into(),
            },
            &un,
            0,
            &PresentDenominator::Count(1),
            Some(CoverageStatus::Computed(ManifestCoverage {
                attributed: 1,
                no_indexed_source: 0,
                indexed_unattributed_manifests: 0,
                indexed_unattributed_files: 0,
            })),
            &[],
        );
        let obj = payload.as_object().unwrap();
        assert_eq!(obj.get("resolution_state").unwrap(), "unknown");
        assert!(obj
            .get("resolution_note")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("overlay read failed"));
        let entry = &payload["results"][0]["entries"][0];
        assert_eq!(entry["confidence"], 0.5);
        assert_eq!(
            entry["resolution_note"],
            "declared — resolution state unknown on this index"
        );
    }

    #[test]
    fn present_count_reads_separate_denominator_key_per_ecosystem() {
        // review-3 item 2: the denominator comes from the SEPARATE `deps_manifests_present` key —
        // the count of scanned manifests — NOT from the parsed record. The amodx 9-of-43 shape:
        // 43 package.json present, only some parsed.
        let blob = r#"{"deps_manifests":[{"path":"package.json","dir":"","ecosystem":"npm"}],
                       "deps_manifests_present":{"npm":43,"python":0,"cargo":0}}"#;
        assert_eq!(
            parse_manifests_present(Some(blob), "npm"),
            PresentDenominator::Count(43)
        );
        assert_eq!(
            parse_manifests_present(Some(blob), "python"),
            PresentDenominator::Count(0)
        );
        // Ecosystem absent from the map → NotApplicable (no read failed), never a false 0.
        assert_eq!(
            parse_manifests_present(Some(blob), "java"),
            PresentDenominator::NotApplicable
        );
        // Old snapshot: present key absent entirely → NotApplicable (coverage falls back to parsed).
        assert_eq!(
            parse_manifests_present(Some(r#"{"deps_manifests":[]}"#), "npm"),
            PresentDenominator::NotApplicable
        );
        // No blob at all → io-NotFound absence → NotApplicable (never a fabricated 0).
        assert_eq!(
            parse_manifests_present(None, "npm"),
            PresentDenominator::NotApplicable
        );
    }

    #[test]
    fn present_count_corruption_is_unavailable_with_reason_not_absence() {
        // review-3 / operator ruling 2026-08-31: a CORRUPT present-count read (non-JSON blob, or a
        // non-integer count value) is a read FAILURE — Unavailable-with-reason, NEVER merged into the
        // same NotApplicable absence that the caller fabricates a denominator over.
        match parse_manifests_present(Some("not json{"), "npm") {
            PresentDenominator::Unavailable { reason } => {
                assert!(reason.contains("not valid JSON"), "{reason}")
            }
            other => panic!("expected Unavailable for corrupt blob, got {other:?}"),
        }
        let non_integer = r#"{"deps_manifests_present":{"npm":"lots"}}"#;
        match parse_manifests_present(Some(non_integer), "npm") {
            PresentDenominator::Unavailable { reason } => {
                assert!(reason.contains("malformed"), "{reason}")
            }
            other => panic!("expected Unavailable for non-integer count, got {other:?}"),
        }
    }

    #[test]
    fn computed_split_with_failed_present_read_renders_coverage_unknown_not_fabricated() {
        // review-3 (operator-ratified): the split WAS computed (provenance + owned-files reads
        // succeeded), but the SEPARATE present-count read FAILED. The denominator must NOT be
        // fabricated from the parsed total — the coverage renders unknown-with-reason, and no
        // `manifests_present`/`manifests_attributed`/`manifests_no_indexed_source` split is emitted.
        let res = result_of(vec![summary(
            "app",
            ManifestContext::Parsed {
                path: "app/package.json".into(),
            },
            &[],
        )]);
        let un = Unattributed {
            count: 0,
            reason: "all attributed".into(),
        };
        let payload = build_deps_list_response(
            "repo",
            "snap",
            "npm",
            &[],
            res,
            None,
            &ResolutionState::Clean,
            &un,
            0,
            &PresentDenominator::Unavailable {
                reason: "scanned-manifest count unreadable: disk error".into(),
            },
            Some(CoverageStatus::Computed(ManifestCoverage {
                attributed: 7,
                no_indexed_source: 0,
                indexed_unattributed_manifests: 0,
                indexed_unattributed_files: 0,
            })),
            &[],
        );
        let obj = payload.as_object().unwrap();
        assert_eq!(
            obj.get("manifests_coverage_unavailable").unwrap(),
            "scanned-manifest count unreadable: disk error"
        );
        assert!(
            !obj.contains_key("manifests_present"),
            "must not fabricate a denominator on a failed present read: {payload}"
        );
        assert!(
            !obj.contains_key("manifests_attributed")
                && !obj.contains_key("manifests_no_indexed_source"),
            "must not emit an 'X of ?' split with no honest denominator: {payload}"
        );
    }

    #[test]
    fn computed_split_not_applicable_present_falls_back_to_parsed_total() {
        // Old snapshot: present count untracked (NotApplicable), but provenance + owned-files DID
        // compute the split → the denominator falls back to the parsed total (attributed +
        // no_indexed_source), a real computed quantity (byte-parity for pre-present-key snapshots).
        let res = result_of(vec![summary(
            "app",
            ManifestContext::Parsed {
                path: "app/package.json".into(),
            },
            &[],
        )]);
        let un = Unattributed {
            count: 0,
            reason: "all attributed".into(),
        };
        let payload = build_deps_list_response(
            "repo",
            "snap",
            "npm",
            &[],
            res,
            None,
            &ResolutionState::Clean,
            &un,
            0,
            &PresentDenominator::NotApplicable,
            Some(CoverageStatus::Computed(ManifestCoverage {
                attributed: 3,
                no_indexed_source: 1,
                indexed_unattributed_manifests: 0,
                indexed_unattributed_files: 0,
            })),
            &[],
        );
        let obj = payload.as_object().unwrap();
        assert_eq!(obj.get("manifests_present").unwrap(), 4); // 3 + 1 parsed total
        assert_eq!(obj.get("manifests_attributed").unwrap(), 3);
        assert!(!obj.contains_key("manifests_coverage_unavailable"));
    }

    #[test]
    fn malformed_json_and_value_are_unavailable_with_reason() {
        // Ruling 3 item 2: corruption carries its OWN reason, never "indexed before tracking".
        match parse_manifest_provenance(Some("not json{")) {
            ProvenanceRead::Unavailable { reason } => assert!(reason.contains("not valid JSON")),
            other => panic!("expected Unavailable, got {other:?}"),
        }
        match parse_manifest_provenance(Some(r#"{"deps_manifests":42}"#)) {
            ProvenanceRead::Unavailable { reason } => assert!(reason.contains("malformed")),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
