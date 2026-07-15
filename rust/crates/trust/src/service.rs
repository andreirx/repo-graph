//! Trust service — two-layer computation + assembly.
//!
//! Layer 1 (pure): `compute_trust_report` takes a fully-assembled
//! `TrustComputationInput` and returns a `TrustReport`. No storage
//! access, no I/O, no SQL knowledge.
//!
//! Layer 2 (storage-backed): `assemble_trust_report` takes a
//! `&impl TrustStorageRead` implementor and builds the input by
//! pulling raw data through the trait, then delegates to layer 1.
//!
//! Mirror of `src/core/trust/service.ts`.
//!
//! ── Lock deviation: `human_label_for_category` ───────────────
//!
//! The R4 lock listed `humanLabelForCategory` as "display-time
//! rendering, stays TS." However, the TS `computeTrustReport`
//! calls this function inside the report builder to populate
//! `TrustCategoryRow.label`. The label is part of the `TrustReport`
//! DTO parity surface, not a CLI display concern. Without it, the
//! Rust report produces different category rows than the TS report.
//!
//! The function is 8 static string mappings plus a fallback. It is
//! ported here as `pub(crate)` — a narrow DTO-completeness
//! exception, not a general invitation to port display helpers.

use std::ops::ControlFlow;

use repo_graph_classification::derive_blast_radius;
use repo_graph_classification::types::{BlastRadiusLevel, UnresolvedEdgeCategory};

use crate::rules::{
    self, count_suspicious_zero_connectivity_modules, group_path_prefix_cycles_by_ancestor,
    sum_unresolved_calls, sum_unresolved_imports, ModuleForSuspicionCheck, PathPrefixCycleInput,
};
use crate::storage_port::{
    BasisCodeCountRow, ClassificationCountRow, ExternalDependencyAttribution,
    PathPrefixModuleCycle, TrustModuleStats, TrustStorageRead, TrustUnresolvedEdgeSample,
    UnresolvedEdgeClassification,
};
use crate::types::{
    EnrichmentStatus, EnrichmentTopType, ExtractionDiagnostics, ModuleTrustRow, ReliabilityLevel,
    TrustBasisClassificationRow, TrustCategoryRow, TrustClassificationRow, TrustDowngrades,
    TrustExternalDependencyAttribution, TrustNamedDependencyRow, TrustReliability, TrustReport,
    TrustSummary, UnknownCallsBlastRadiusBreakdown,
};

/// The bound on how many named library dependencies the reader-frame breakdown lists
/// individually (ATTRIBUTION-1). Applied at the storage read (count-desc, name-asc);
/// dependencies beyond it are surfaced honestly as an aggregate "other declared
/// dependencies" tail, never dropped. Ten keeps the section compact while covering the
/// dependency set of a typical module/crate.
pub const TOP_NAMED_DEPENDENCIES_LIMIT: u32 = 10;

// ── Error type for assembly layer ────────────────────────────────

/// Error from the storage-backed assembly layer.
///
/// `Storage(E)` wraps errors from `TrustStorageRead` methods.
/// `JsonParse` wraps failures when parsing JSON strings
/// (diagnostics_json, toolchain_json) into typed Rust structs.
///
/// The bound `E: Display` is required only for the `Display` impl
/// on `TrustAssemblyError` itself. No `Debug` bound is imposed on
/// `E` at the type level; the `#[derive(Debug)]` works because
/// `E` is constrained to `Debug` only where the derive is used.
#[derive(Debug)]
pub enum TrustAssemblyError<E> {
    /// A `TrustStorageRead` method returned an error.
    Storage(E),
    /// A JSON field contained malformed JSON.
    JsonParse {
        field: &'static str,
        source: serde_json::Error,
    },
}

impl<E: std::fmt::Display> std::fmt::Display for TrustAssemblyError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "storage error: {}", e),
            Self::JsonParse { field, source } => {
                write!(f, "failed to parse {}: {}", field, source)
            }
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for TrustAssemblyError<E> {}

// ── Cooperative cancellation (DAEMON-CANCEL-3) ───────────────────

/// A cooperative cancellation checkpoint (DAEMON-CANCEL-3).
///
/// A `&mut` closure the assembly layer calls at bounded intervals inside the heavy
/// unresolved-sample loop (up to 100_000 rows; `service.ts`'s `computeTrustReport`
/// inner loop); returning [`ControlFlow::Break`] tells the loop to abandon its
/// (read-only, so discardable) work. This is the trust-crate spelling of the same
/// `&mut dyn FnMut() -> ControlFlow<()>` shape the agent crate's `AgentCancelCheck`
/// uses — std types only, so the trust crate stays free of any daemon /
/// graph-algorithms dependency. The daemon's trust/check worker builds the concrete
/// closure from a `CancelFlag` (latched by the transport thread on peer-disconnect);
/// a no-op closure (`|| ControlFlow::Continue(())`) reproduces the non-cancellable
/// behavior byte-for-byte.
pub type TrustCancelCheck<'a> = &'a mut dyn FnMut() -> ControlFlow<()>;

/// Outcome of [`assemble_trust_report_cancellable`]: the assembled report, or the
/// client disconnected mid-assembly (the cooperative checkpoint broke inside the
/// unresolved-sample loop).
///
/// On a worker thread the supervising transport thread classifies the disconnect
/// independently (it returns `Supervised::Cancelled` the moment the heartbeat write
/// fails, which is also what latches the `CancelFlag` the checkpoint observes), so a
/// returned `Cancelled` is the worker honestly reporting that it stopped early — the
/// dispatcher never serves it. The variant exists so the cancellable path NEVER
/// returns a silently-partial report as if it were complete (the
/// [`TrustReport`](crate::types::TrustReport) is boxed to keep the enum small).
pub enum TrustReportOutcome {
    /// The report was fully assembled (served to a still-connected peer).
    Ready(Box<TrustReport>),
    /// The peer disconnected mid-assembly; the partial work is discarded.
    Cancelled,
}

// ── Input DTO for pure computation ───────────────────────────────

/// Fully-assembled data bundle for `compute_trust_report`.
///
/// Every field is pre-fetched and pre-parsed. The pure computation
/// layer does not do I/O, SQL, or JSON parsing of external strings
/// (except `metadata_json` on unresolved edge samples, which is
/// parsed inline with silent-ignore on failure, matching TS).
///
/// The assembly function (`assemble_trust_report`) builds this
/// from `TrustStorageRead` queries + JSON parsing.
pub struct TrustComputationInput {
    // ── Identity ──────────────────────────────────────────────
    pub snapshot_uid: String,
    pub basis_commit: Option<String>,

    // ── Pre-parsed from storage JSON strings ──────────────────
    pub toolchain: Option<serde_json::Map<String, serde_json::Value>>,
    pub diagnostics: Option<ExtractionDiagnostics>,

    // ── Storage reads (already fetched) ───────────────────────
    pub file_paths: Vec<String>,
    pub module_stats: Vec<TrustModuleStats>,
    pub path_prefix_cycles: Vec<PathPrefixModuleCycle>,
    pub active_entrypoint_count: usize,
    pub resolved_calls: u64,

    // ── Classification data (pre-fetched) ─────────────────────
    /// Classification counts filtered to CALLS-family categories.
    /// Used for Variant A reweighting (external vs internal-like).
    pub calls_classification_counts: Vec<ClassificationCountRow>,
    /// Classification counts unfiltered (all categories).
    /// Used for the classifications section of the report.
    pub all_classification_counts: Vec<ClassificationCountRow>,
    /// ATTRIBUTION-1: basis-code counts unfiltered (all categories). The finer axis
    /// used to build the reader-frame attribution breakdown. Empty when the snapshot
    /// has no unresolved edges (mirrors `all_classification_counts`).
    pub all_basis_code_counts: Vec<BasisCodeCountRow>,
    /// ATTRIBUTION-1 iteration 3: the reader-frame attribution of the external-import
    /// unresolved references — each named by its DECLARED dependency across all three call
    /// bases (the provenance join), plus the named/unidentified totals reconciling the
    /// class. Empty/zero when the snapshot has no external-import references.
    pub external_dependencies: ExternalDependencyAttribution,
    /// Unresolved edge samples with classification = "unknown".
    /// Used for blast-radius breakdown and enrichment status.
    /// Empty if `all_classification_counts` was empty (assembly
    /// skips the query in that case).
    pub unknown_calls_samples: Vec<TrustUnresolvedEdgeSample>,
}

// ── Human labels (lock deviation, DTO-completeness) ──────────────

/// Map a category key to its human-readable label.
///
/// Mirror of `humanLabelForCategory` from
/// `src/core/diagnostics/unresolved-edge-categories.ts:51`.
///
/// **Lock deviation:** the R4 lock listed this as "display-time
/// rendering, stays TS." Ported here because the TS
/// `computeTrustReport` calls it inside the report builder, making
/// the label part of the `TrustReport` DTO parity surface. This
/// is a narrow DTO-completeness exception. The function stays
/// `pub(crate)` — it is not part of the public API.
pub(crate) fn human_label_for_category(category: &str) -> String {
    match category {
        "imports_file_not_found" => "IMPORTS (file not found)".into(),
        "imports_ambiguous_match" => "IMPORTS (ambiguous match)".into(),
        "instantiates_class_not_found" => "INSTANTIATES (class not found)".into(),
        "implements_interface_not_found" => "IMPLEMENTS (interface not found)".into(),
        "calls_this_wildcard_method_needs_type_info" => {
            "CALLS this.*.method (needs type info)".into()
        }
        "calls_this_method_needs_class_context" => "CALLS this.method (needs class context)".into(),
        "calls_obj_method_needs_type_info" => "CALLS obj.method (needs type info)".into(),
        "calls_function_ambiguous_or_missing" => "CALLS function (ambiguous or missing)".into(),
        "other" => "OTHER (unclassified)".into(),
        _ => category.to_string(),
    }
}

// ── Caveats builder ──────────────────────────────────────────────

/// Build the caveats list based on reliability levels.
///
/// Mirror of `buildCaveats` from `service.ts:366`.
fn build_caveats(
    diagnostics_available: bool,
    import_graph_level: ReliabilityLevel,
    call_graph_level: ReliabilityLevel,
    dead_code_level: ReliabilityLevel,
    change_impact_level: ReliabilityLevel,
) -> Vec<String> {
    let mut caveats = Vec::new();
    if !diagnostics_available {
        caveats.push(
            "Extraction diagnostics unavailable for this snapshot. Re-index to populate.".into(),
        );
    }
    if call_graph_level != ReliabilityLevel::HIGH {
        // RELIABILITY-REFRAME-1: reader frame — the subject is the READER's calls, not a
        // grade of repo-graph's call-graph pipeline. Band-only here (the caveat builder has
        // levels, not the rate); the numeric "your code's calls M% resolved" rides the
        // Reliability axis render. This caveat is in the `trust` crate (below `agent`), so it
        // cannot consume `agent::reliability` without inverting the dependency rule — the
        // shared vocabulary is the RATE derivation, which this band-only string does not touch.
        caveats.push(format!(
            "Your code's calls resolve at {:?} reliability on this repo. \
			 Do not use callers/callees for safety-critical decisions without verification.",
            call_graph_level
        ));
    }
    // Dead-code caveat removed: `rmap dead` surface is disabled.
    // Internal dead_code_reliability computation is preserved for
    // future use but not surfaced to users.
    let _ = dead_code_level;
    if import_graph_level != ReliabilityLevel::HIGH {
        caveats.push(format!(
            "Import-graph reliability is {:?}. \
			 Module fan-in/fan-out and change-impact propagation may undercount relationships.",
            import_graph_level
        ));
    }
    if change_impact_level != ReliabilityLevel::HIGH {
        caveats.push(format!(
            "Change-impact reliability is {:?}. \
			 Impacted-module sets may be incomplete on this repo.",
            change_impact_level
        ));
    }
    caveats.push(
        "Cycle payloads currently emit leaf module names only; \
		 full stable keys are not in the user-facing `graph cycles` output."
            .into(),
    );
    caveats
}

// ── Layer 1: Pure computation ────────────────────────────────────

/// Compute a `TrustReport` from a fully-assembled data bundle.
///
/// This function is PURE. No storage access, no I/O, no SQL.
/// All data is pre-fetched in `TrustComputationInput`.
///
/// Mirror of `computeTrustReport` from `service.ts:63`, with the
/// storage-fetching phase factored out into `assemble_trust_report`.
pub fn compute_trust_report(input: &TrustComputationInput) -> TrustReport {
    // DAEMON-CANCEL-3: the pure computation delegates to the cancellable body with a
    // never-breaking checkpoint, so every existing caller is byte-identical.
    match compute_trust_report_cancellable(input, &mut || ControlFlow::Continue(())) {
        ControlFlow::Continue(report) => report,
        ControlFlow::Break(()) => unreachable!("no-op cancel checkpoint never breaks"),
    }
}

/// Cancellable variant of [`compute_trust_report`] (DAEMON-CANCEL-3).
///
/// Identical except the Phase-5 unresolved-sample loop (up to 100_000 rows) consults
/// `cancel`; on [`ControlFlow::Break`] the whole computation is abandoned
/// ([`ControlFlow::Break`] out, read-only ⇒ nothing to roll back). All other phases
/// are cheap fixed-size work and are left un-checkpointed — NARROW scope: only the
/// demonstrated heavy sample loop is threaded.
pub fn compute_trust_report_cancellable(
    input: &TrustComputationInput,
    cancel: TrustCancelCheck<'_>,
) -> ControlFlow<(), TrustReport> {
    let diagnostics_available = input.diagnostics.is_some();

    // ── Phase 1: Detection rules ─────────────────────────────
    let framework_heavy = rules::detect_framework_heavy_suspicion(&input.file_paths);

    let module_stats_for_rules: Vec<ModuleForSuspicionCheck> = input
        .module_stats
        .iter()
        .map(|m| ModuleForSuspicionCheck {
            qualified_name: m.path.clone(),
            fan_in: m.fan_in,
            fan_out: m.fan_out,
            file_count: m.file_count,
        })
        .collect();
    let suspicious_module_count =
        count_suspicious_zero_connectivity_modules(&module_stats_for_rules);

    let alias_resolution = rules::detect_alias_resolution_suspicion(suspicious_module_count);

    let cycle_inputs: Vec<PathPrefixCycleInput> = input
        .path_prefix_cycles
        .iter()
        .map(|c| PathPrefixCycleInput {
            ancestor_stable_key: c.ancestor_stable_key.clone(),
        })
        .collect();
    let cycles_by_ancestor = group_path_prefix_cycles_by_ancestor(&cycle_inputs);
    let total_cycles = input.path_prefix_cycles.len();
    let registry_pattern =
        rules::detect_registry_pattern_suspicion(&cycles_by_ancestor, total_cycles);

    let missing_entrypoints =
        rules::detect_missing_entrypoint_declarations(input.active_entrypoint_count);

    // ── Phase 2: Reliability formulas ────────────────────────
    let unresolved_calls = input
        .diagnostics
        .as_ref()
        .map(sum_unresolved_calls)
        .unwrap_or(0);
    let unresolved_imports = input
        .diagnostics
        .as_ref()
        .map(sum_unresolved_imports)
        .unwrap_or(0);

    // Variant A reweighting: external_library_candidate calls are
    // excluded from the internal-like denominator.
    let unresolved_calls_external = input
        .calls_classification_counts
        .iter()
        .find(|r| r.classification == UnresolvedEdgeClassification::ExternalLibraryCandidate)
        .map(|r| r.count)
        .unwrap_or(0);
    let unresolved_calls_internal_like = unresolved_calls.saturating_sub(unresolved_calls_external);

    // RELIABILITY-REFRAME-1 (review-3 §2): the UNCLASSIFIED (`unknown`) portion of the
    // in-scope denominator, READ from the SAME already-fetched classification counts (the
    // classification axis is CONSUMED, not modified). `internal_like` = external-excluded =
    // internal-candidate ∪ unknown, so labelling it "known internal" is a false certainty;
    // this counter lets a reader surface fire the conservative-rate caveat. Clamped to the
    // in-scope denominator: the classification table and the diagnostics `unresolved_calls`
    // are separate reads, so `unknown` can nominally exceed `internal_like`; a share > 100%
    // would be nonsense, so the honest ceiling is "all of the in-scope denominator".
    let unresolved_calls_unknown = input
        .calls_classification_counts
        .iter()
        .find(|r| r.classification == UnresolvedEdgeClassification::Unknown)
        .map(|r| r.count)
        .unwrap_or(0)
        .min(unresolved_calls_internal_like);

    let import_graph_reliability = rules::compute_import_graph_reliability(
        alias_resolution.triggered,
        registry_pattern.triggered,
        unresolved_imports,
    );

    let call_graph_reliability =
        rules::compute_call_graph_reliability(input.resolved_calls, unresolved_calls_internal_like);

    let dead_code_reliability = rules::compute_dead_code_reliability(
        missing_entrypoints.triggered,
        registry_pattern.triggered,
        framework_heavy.triggered,
        call_graph_reliability.level,
    );

    let change_impact_reliability = rules::compute_change_impact_reliability(
        alias_resolution.triggered,
        registry_pattern.triggered,
        import_graph_reliability.level,
    );

    // ── Phase 3: Category rows ───────────────────────────────
    let mut categories: Vec<TrustCategoryRow> = match &input.diagnostics {
        Some(diag) => diag
            .unresolved_breakdown
            .iter()
            .map(|(category, &unresolved)| TrustCategoryRow {
                label: human_label_for_category(category),
                category: category.clone(),
                unresolved,
            })
            .collect(),
        None => vec![],
    };
    // Sort: unresolved desc, then category asc as tie-break.
    categories.sort_by(|a, b| {
        b.unresolved
            .cmp(&a.unresolved)
            .then_with(|| a.category.cmp(&b.category))
    });

    // ── Phase 4: Classification rows ─────────────────────────
    let mut classifications: Vec<TrustClassificationRow> = input
        .all_classification_counts
        .iter()
        .map(|r| {
            // Serialize the typed enum to its snake_case string for
            // the report DTO (TrustClassificationRow.classification
            // is a String, matching the TS output format).
            let classification_str = serde_json::to_value(r.classification)
                .ok()
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s),
                    _ => None,
                })
                .unwrap_or_else(|| format!("{:?}", r.classification));
            TrustClassificationRow {
                classification: classification_str,
                count: r.count,
            }
        })
        .collect();
    // Sort: count desc, then classification asc as tie-break.
    classifications.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.classification.cmp(&b.classification))
    });

    // ── Phase 4b: Basis-code rows (ATTRIBUTION-1) ────────────
    // The finer axis. Same serialize-typed-enum-to-string discipline as the
    // classification rows above, and the same count-desc / code-asc ordering — the
    // rgr presentation layer maps each basis code to a reader-frame attribution class.
    let mut basis_classifications: Vec<TrustBasisClassificationRow> = input
        .all_basis_code_counts
        .iter()
        .map(|r| {
            let basis_code_str = serde_json::to_value(r.basis_code)
                .ok()
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s),
                    _ => None,
                })
                .unwrap_or_else(|| format!("{:?}", r.basis_code));
            TrustBasisClassificationRow {
                basis_code: basis_code_str,
                count: r.count,
            }
        })
        .collect();
    basis_classifications.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.basis_code.cmp(&b.basis_code))
    });

    // ── Phase 4c: External-dependency attribution (ATTRIBUTION-1 iteration 3) ─────────
    // The provenance join's result: the top declared dependencies (already bounded +
    // ordered count-desc, name-asc by the storage read) plus the named/unidentified totals
    // that reconcile the class. The report/wire mirror is a field-identical map (port →
    // report), exactly like the basis rows above but with no enum to serialize.
    let external_dependencies = TrustExternalDependencyAttribution {
        top: input
            .external_dependencies
            .top
            .iter()
            .map(|d| TrustNamedDependencyRow {
                name: d.name.clone(),
                count: d.count,
            })
            .collect(),
        total_named: input.external_dependencies.total_named,
        unidentified: input.external_dependencies.unidentified,
    };

    // ── Phase 5: Blast radius + enrichment ───────────────────
    //
    // `enrichment_eligible_count` is returned alongside the
    // `Option<EnrichmentStatus>` so downstream consumers can
    // tell "no eligible samples" from "eligible samples but
    // enrichment phase did not run". Both states currently
    // collapse to `enrichment_status = None`; the counter is
    // the disambiguator. Used by the agent storage adapter
    // to map into `EnrichmentState::{NotApplicable, NotRun}`
    // without ambiguity. See the field doc on `TrustReport`.
    let (unknown_calls_blast_radius, enrichment_status, enrichment_eligible_count) = if !input
        .all_classification_counts
        .is_empty()
    {
        // DAEMON-CANCEL-3: the up-to-100_000-row sample loop is the heavy path; it
        // consults `cancel` per chunk and Breaks the whole computation out on
        // disconnect.
        match compute_blast_radius_and_enrichment_cancellable(&input.unknown_calls_samples, cancel)
        {
            ControlFlow::Continue(v) => v,
            ControlFlow::Break(()) => return ControlFlow::Break(()),
        }
    } else {
        (None, None, 0)
    };

    // ── Phase 6: Module rows ─────────────────────────────────
    // Explicit sort by qualified_name for deterministic output.
    // The storage SQL happens to ORDER BY qualified_name, but
    // this is a public pure function — any caller can build
    // TrustComputationInput in arbitrary order. The sort here
    // guarantees the same logical input always produces the
    // same output regardless of input ordering.
    let mut modules: Vec<ModuleTrustRow> = input
        .module_stats
        .iter()
        .map(|m| {
            let suspicious = m.fan_in == 0
                && m.fan_out == 0
                && m.file_count >= 2
                && m.path != "."
                && !m.path.is_empty();
            let mut trust_notes = Vec::new();
            if suspicious {
                trust_notes.push("alias_resolution_candidate".to_string());
            }
            ModuleTrustRow {
                module_stable_key: m.stable_key.clone(),
                qualified_name: m.path.clone(),
                fan_in: m.fan_in,
                fan_out: m.fan_out,
                file_count: m.file_count,
                suspicious_zero_connectivity: suspicious,
                trust_notes,
            }
        })
        .collect();
    modules.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

    // ── Phase 7: Caveats ─────────────────────────────────────
    let caveats = build_caveats(
        diagnostics_available,
        import_graph_reliability.level,
        call_graph_reliability.level,
        dead_code_reliability.level,
        change_impact_reliability.level,
    );

    // ── Phase 8: Call resolution rate ─────────────────────────
    let call_resolution_rate = {
        let total = input.resolved_calls + unresolved_calls_internal_like;
        if total > 0 {
            input.resolved_calls as f64 / total as f64
        } else {
            1.0
        }
    };

    // ── Assemble report ──────────────────────────────────────
    ControlFlow::Continue(TrustReport {
        snapshot_uid: input.snapshot_uid.clone(),
        display_name: None, // Populated by daemon handler
        basis_commit: input.basis_commit.clone(),
        toolchain: input.toolchain.clone(),
        diagnostics_version: input.diagnostics.as_ref().map(|d| d.diagnostics_version),
        summary: TrustSummary {
            edges_total: input
                .diagnostics
                .as_ref()
                .map(|d| d.edges_total)
                .unwrap_or(0),
            edges_resolved: input
                .diagnostics
                .as_ref()
                .map(|d| d.edges_total)
                .unwrap_or(0),
            unresolved_total: input
                .diagnostics
                .as_ref()
                .map(|d| d.unresolved_total)
                .unwrap_or(0),
            resolved_calls: input.resolved_calls,
            unresolved_calls,
            unresolved_calls_external,
            unresolved_calls_internal_like,
            call_resolution_rate,
            reliability: TrustReliability {
                import_graph: import_graph_reliability,
                call_graph: call_graph_reliability,
                dead_code: dead_code_reliability,
                change_impact: change_impact_reliability,
            },
            triggered_downgrades: TrustDowngrades {
                framework_heavy_suspicion: framework_heavy,
                registry_pattern_suspicion: registry_pattern,
                missing_entrypoint_declarations: missing_entrypoints,
                alias_resolution_suspicion: alias_resolution,
            },
        },
        categories,
        classifications,
        basis_classifications,
        external_dependencies,
        unknown_calls_blast_radius,
        enrichment_status,
        modules,
        caveats,
        diagnostics_available,
        enrichment_eligible_count,
        unresolved_calls_unknown,
    })
}

// ── Blast-radius + enrichment computation ────────────────────────

/// Compute the blast-radius breakdown and enrichment status from
/// unknown-classified CALLS samples.
///
/// Mirrors the inner loop in `computeTrustReport` at
/// service.ts:214-288. Uses typed `UnresolvedEdgeCategory` and
/// `derive_blast_radius` from the classification crate — no raw
/// string comparisons where typed enums exist.
///
/// DAEMON-CANCEL-3: this is the demonstrated heavy trust path (up to 100_000 samples,
/// each deriving a blast radius and parsing enrichment metadata JSON). It consults
/// `cancel` once per [`CHUNK`](self) rows pulled and returns
/// [`ControlFlow::Break`] on disconnect, so a disconnected peer's in-flight
/// trust/check abandons the loop instead of grinding through every sample with no
/// consumer. The per-chunk cadence bounds the cooperative-flag polling cost while
/// still abandoning a full materialization within a bounded number of rows. Read-only
/// ⇒ the partial counters are simply dropped.
fn compute_blast_radius_and_enrichment_cancellable(
    samples: &[TrustUnresolvedEdgeSample],
    cancel: TrustCancelCheck<'_>,
) -> ControlFlow<
    (),
    (
        Option<UnknownCallsBlastRadiusBreakdown>,
        Option<EnrichmentStatus>,
        u64,
    ),
> {
    // Poll the cooperative checkpoint once per this many samples — frequent enough to
    // bail within a bounded window, sparse enough that the flag read is negligible
    // against the per-sample work.
    const CHUNK: usize = 1024;
    let mut breakdown = UnknownCallsBlastRadiusBreakdown {
        low: 0,
        medium: 0,
        high: 0,
    };
    let mut enriched_count: u64 = 0;
    let mut eligible_count: u64 = 0;
    let mut enrichment_was_run = false;

    // BTreeMap for deterministic ordering. The key includes `is_external` so the SAME simple name
    // appearing as BOTH an internal and an external receiver (e.g. a std `Error` and an in-repo
    // `Error`, which the resolver collapses to the same bare name) is counted SEPARATELY per
    // classification. Keying by name alone would freeze the first row's flag for every same-name row,
    // and the likely-external read projection (EY1-A) could then report a false external count.
    let mut type_counts: std::collections::BTreeMap<(String, bool), u64> =
        std::collections::BTreeMap::new();

    for (i, sample) in samples.iter().enumerate() {
        // DAEMON-CANCEL-3: cooperative checkpoint — bail the whole loop on disconnect.
        if i % CHUNK == 0 && cancel().is_break() {
            return ControlFlow::Break(());
        }
        // Filter to CALLS-family only (typed check, not string).
        if !sample.category.is_calls_category() {
            continue;
        }

        // Derive blast radius per-row.
        let assessment = derive_blast_radius(
            sample.category,
            sample.basis_code,
            sample.source_node_visibility.as_deref(),
        );
        match assessment.blast_radius {
            BlastRadiusLevel::Low => breakdown.low += 1,
            BlastRadiusLevel::Medium => breakdown.medium += 1,
            BlastRadiusLevel::High => breakdown.high += 1,
            BlastRadiusLevel::NotApplicable => {}
        }

        // Enrichment status: only for calls_obj_method_needs_type_info.
        if sample.category == UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
            eligible_count += 1;
            if let Some(ref meta_str) = sample.metadata_json {
                // Silent-ignore on malformed JSON, matching TS try-catch.
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                    if let Some(enrichment) = meta.get("enrichment") {
                        enrichment_was_run = true;
                        if enrichment.get("receiverType").is_some() {
                            enriched_count += 1;
                            let type_name = enrichment
                                .get("typeDisplayName")
                                .and_then(|v| v.as_str())
                                .or_else(|| enrichment.get("receiverType").and_then(|v| v.as_str()))
                                .unwrap_or("")
                                .to_string();
                            let is_ext = enrichment
                                .get("isExternalType")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            *type_counts.entry((type_name, is_ext)).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }

    let blast_radius = Some(breakdown);

    // Build enrichment status only if enrichment was actually run.
    // Null = no enrichment markers found.
    // Populated with enriched=0 = enrichment ran but resolved zero types.
    //
    // The returned `eligible_count` is the number of
    // `CallsObjMethodNeedsTypeInfo` samples regardless of
    // whether the enrichment phase ran. Downstream consumers
    // use it alongside `enrichment_status.is_none()` to
    // distinguish "no eligible samples at all" (count == 0)
    // from "eligible samples existed but enrichment phase did
    // not run" (count > 0, status == None). Without this
    // counter the two states are indistinguishable through the
    // public DTO.
    let enrichment = if enrichment_was_run {
        let mut all_types: Vec<EnrichmentTopType> = type_counts
            .into_iter()
            .map(|((type_name, is_external), count)| EnrichmentTopType {
                type_name,
                count,
                is_external,
            })
            .collect();
        // Sort: count desc, then type_name asc as tie-break. `all_types` is fully
        // sorted, so BOTH derived lists below inherit a deterministic order.
        all_types.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.type_name.cmp(&b.type_name))
        });

        // RELIABILITY-REFRAME-1 (review-3 §3): FILTER to external FIRST, truncate AFTER —
        // so the reader's coverage map is the true top-N EXTERNAL targets. Deriving it from
        // the top-15-MIXED `top_types` (as consumers did before) could drop an external
        // ranked below 15th overall even when it is a top external. `all_types` is already
        // count-desc/name-asc, so `filter` preserves that order; the tie-break is inherited.
        let top_external_types: Vec<EnrichmentTopType> = all_types
            .iter()
            .filter(|t| t.is_external)
            .take(15)
            .cloned()
            .collect();

        // `top_types` stays the established MIXED top-15 (byte-identical for every existing
        // consumer, incl. the daemon `--json` reader).
        let mut top_types = all_types;
        top_types.truncate(15);
        Some(EnrichmentStatus {
            eligible: eligible_count,
            enriched: enriched_count,
            top_types,
            top_external_types,
        })
    } else {
        None
    };

    ControlFlow::Continue((blast_radius, enrichment, eligible_count))
}

// ── Layer 2: Storage-backed assembly ─────────────────────────────

/// Fetch data from storage, parse JSON, and delegate to
/// `compute_trust_report`.
///
/// This is the thin orchestration layer. It calls
/// `TrustStorageRead` methods, parses the JSON strings, and
/// builds the `TrustComputationInput`. The actual report
/// computation is pure and happens in `compute_trust_report`.
///
/// Mirrors `computeTrustReport` from `service.ts:63` — the
/// storage-fetching parts only.
pub fn assemble_trust_report<S: TrustStorageRead>(
    storage: &S,
    repo_uid: &str,
    snapshot_uid: &str,
    basis_commit: Option<&str>,
    toolchain_json: Option<&str>,
) -> Result<TrustReport, TrustAssemblyError<S::Error>> {
    // DAEMON-CANCEL-3: delegate to the cancellable sibling with a never-breaking
    // checkpoint, so every existing (non-daemon) caller is byte-identical and never
    // observes the `Cancelled` outcome.
    match assemble_trust_report_cancellable(
        storage,
        repo_uid,
        snapshot_uid,
        basis_commit,
        toolchain_json,
        &mut || ControlFlow::Continue(()),
    )? {
        TrustReportOutcome::Ready(report) => Ok(*report),
        TrustReportOutcome::Cancelled => unreachable!("no-op cancel checkpoint never breaks"),
    }
}

/// Cancellable variant of [`assemble_trust_report`] (DAEMON-CANCEL-3).
///
/// Identical storage fetches and JSON parsing, but threads `cancel` into the pure
/// computation's Phase-5 unresolved-sample loop (the demonstrated heavy trust path).
/// The SQL reads themselves (notably `compute_module_stats` and the up-to-100_000-row
/// `query_unresolved_edges`) are NOT checkpointed here — an opaque `SELECT` has no
/// Rust frame to poll; the daemon's trust/check worker runs this whole function under
/// CANCEL-2's `sqlite3_interrupt` supervisor, which aborts whichever statement is
/// in-flight on disconnect. So the two cancellation mechanisms compose: the interrupt
/// for the SQL, this cooperative `cancel` for the pure sample loop. On a sample-loop
/// break the function returns [`TrustReportOutcome::Cancelled`] (read-only ⇒ the
/// partial report is discarded).
pub fn assemble_trust_report_cancellable<S: TrustStorageRead>(
    storage: &S,
    repo_uid: &str,
    snapshot_uid: &str,
    basis_commit: Option<&str>,
    toolchain_json: Option<&str>,
    cancel: TrustCancelCheck<'_>,
) -> Result<TrustReportOutcome, TrustAssemblyError<S::Error>> {
    // ── Parse JSON strings ───────────────────────────────────
    let diagnostics: Option<ExtractionDiagnostics> = {
        let json_str = storage
            .get_snapshot_extraction_diagnostics(snapshot_uid)
            .map_err(TrustAssemblyError::Storage)?;
        match json_str {
            Some(s) => {
                let parsed =
                    serde_json::from_str(&s).map_err(|e| TrustAssemblyError::JsonParse {
                        field: "extraction_diagnostics_json",
                        source: e,
                    })?;
                Some(parsed)
            }
            None => None,
        }
    };

    let toolchain: Option<serde_json::Map<String, serde_json::Value>> = match toolchain_json {
        Some(s) => {
            let parsed = serde_json::from_str(s).map_err(|e| TrustAssemblyError::JsonParse {
                field: "toolchain_json",
                source: e,
            })?;
            Some(parsed)
        }
        None => None,
    };

    // ── Fetch from storage ───────────────────────────────────
    let file_paths = storage
        .get_file_paths_by_repo(repo_uid)
        .map_err(TrustAssemblyError::Storage)?;

    let module_stats = storage
        .compute_module_stats(snapshot_uid)
        .map_err(TrustAssemblyError::Storage)?;

    let path_prefix_cycles = storage
        .find_path_prefix_module_cycles(snapshot_uid)
        .map_err(TrustAssemblyError::Storage)?;

    let active_entrypoint_count = storage
        .count_active_declarations(repo_uid, "entrypoint")
        .map_err(TrustAssemblyError::Storage)?;

    let resolved_calls = storage
        .count_edges_by_type(snapshot_uid, "CALLS")
        .map_err(TrustAssemblyError::Storage)?;

    // CALLS-family classification counts (Variant A reweighting).
    let calls_filter = vec![
        UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo,
        UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
        UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
        UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
    ];
    let calls_classification_counts = storage
        .count_unresolved_edges_by_classification(
            &crate::storage_port::CountByClassificationInput {
                snapshot_uid: snapshot_uid.into(),
                filter_categories: calls_filter,
            },
        )
        .map_err(TrustAssemblyError::Storage)?;

    // All classification counts (no filter).
    let all_classification_counts = storage
        .count_unresolved_edges_by_classification(
            &crate::storage_port::CountByClassificationInput {
                snapshot_uid: snapshot_uid.into(),
                filter_categories: vec![],
            },
        )
        .map_err(TrustAssemblyError::Storage)?;

    // ATTRIBUTION-1: basis-code counts (no filter — the full unresolved set). The
    // finer companion to the classification counts above, used for the reader-frame
    // attribution breakdown.
    let all_basis_code_counts = storage
        .count_unresolved_edges_by_basis_code(snapshot_uid)
        .map_err(TrustAssemblyError::Storage)?;

    // ATTRIBUTION-1 iteration 3: the external-dependency attribution (the provenance join) —
    // the top declared dependencies (bounded, count-desc) across all three external-import
    // bases + the named/unidentified totals that reconcile the class.
    let external_dependencies = storage
        .attribute_external_dependencies(snapshot_uid, TOP_NAMED_DEPENDENCIES_LIMIT)
        .map_err(TrustAssemblyError::Storage)?;

    // Unknown CALLS samples (conditional: only if there are
    // classification counts).
    let unknown_calls_samples = if !all_classification_counts.is_empty() {
        storage
            .query_unresolved_edges(&crate::storage_port::QueryUnresolvedEdgesInput {
                snapshot_uid: snapshot_uid.into(),
                classification: UnresolvedEdgeClassification::Unknown,
                limit: 100_000,
            })
            .map_err(TrustAssemblyError::Storage)?
    } else {
        vec![]
    };

    // ── Delegate to pure computation ─────────────────────────
    let input = TrustComputationInput {
        snapshot_uid: snapshot_uid.to_string(),
        basis_commit: basis_commit.map(|s| s.to_string()),
        toolchain,
        diagnostics,
        file_paths,
        module_stats,
        path_prefix_cycles,
        active_entrypoint_count,
        resolved_calls,
        calls_classification_counts,
        all_classification_counts,
        all_basis_code_counts,
        external_dependencies,
        unknown_calls_samples,
    };

    // ── Delegate to pure computation (cancellable sample loop) ─
    match compute_trust_report_cancellable(&input, cancel) {
        ControlFlow::Continue(report) => Ok(TrustReportOutcome::Ready(Box::new(report))),
        ControlFlow::Break(()) => Ok(TrustReportOutcome::Cancelled),
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_port::{
        BasisCodeCountRow, ClassificationCountRow, CountByClassificationInput,
        ExternalDependencyAttribution, NamedDependencyCount, PathPrefixModuleCycle,
        QueryUnresolvedEdgesInput, TrustModuleStats, TrustUnresolvedEdgeSample,
        UnresolvedEdgeBasisCode, UnresolvedEdgeClassification,
    };
    use std::collections::BTreeMap;

    /// Build a minimal input with all-empty data. Produces a report
    /// with HIGH reliability everywhere (no downgrades, no caveats
    /// except the permanent cycle caveat).
    fn minimal_input() -> TrustComputationInput {
        TrustComputationInput {
            snapshot_uid: "snap1".into(),
            basis_commit: None,
            toolchain: None,
            diagnostics: None,
            file_paths: vec![],
            module_stats: vec![],
            path_prefix_cycles: vec![],
            active_entrypoint_count: 1, // >0 to avoid missing_entrypoint trigger
            resolved_calls: 0,
            calls_classification_counts: vec![],
            all_classification_counts: vec![],
            all_basis_code_counts: vec![],
            external_dependencies: ExternalDependencyAttribution::default(),
            unknown_calls_samples: vec![],
        }
    }

    // ── Pure computation: minimal ────────────────────────────

    #[test]
    fn minimal_input_produces_all_high_reliability() {
        let report = compute_trust_report(&minimal_input());
        assert_eq!(report.snapshot_uid, "snap1");
        assert_eq!(
            report.summary.reliability.import_graph.level,
            ReliabilityLevel::HIGH
        );
        assert_eq!(
            report.summary.reliability.call_graph.level,
            ReliabilityLevel::HIGH
        );
        assert_eq!(
            report.summary.reliability.dead_code.level,
            ReliabilityLevel::HIGH
        );
        assert_eq!(
            report.summary.reliability.change_impact.level,
            ReliabilityLevel::HIGH
        );
        assert!(!report.diagnostics_available);
        assert_eq!(report.categories.len(), 0);
        assert_eq!(report.classifications.len(), 0);
        assert!(report.unknown_calls_blast_radius.is_none());
        assert!(report.enrichment_status.is_none());
    }

    #[test]
    fn compute_carries_external_dependency_attribution_from_input_to_report() {
        // ATTRIBUTION-1 iteration 3: the provenance-join result (top + totals) flows
        // input → report unchanged (order-preserving); the render surface later names them.
        let mut input = minimal_input();
        input.external_dependencies = ExternalDependencyAttribution {
            top: vec![
                NamedDependencyCount {
                    name: "serde".into(),
                    count: 5,
                },
                NamedDependencyCount {
                    name: "tokio".into(),
                    count: 2,
                },
            ],
            total_named: 9,
            unidentified: 4,
        };
        let report = compute_trust_report(&input);
        assert_eq!(
            report.external_dependencies,
            TrustExternalDependencyAttribution {
                top: vec![
                    TrustNamedDependencyRow {
                        name: "serde".into(),
                        count: 5
                    },
                    TrustNamedDependencyRow {
                        name: "tokio".into(),
                        count: 2
                    },
                ],
                total_named: 9,
                unidentified: 4,
            }
        );
    }

    #[test]
    fn minimal_input_has_permanent_cycle_caveat() {
        let report = compute_trust_report(&minimal_input());
        assert!(report.caveats.iter().any(|c| c.contains("Cycle payloads")));
        // Only the permanent caveat (no diagnostics caveat because
        // diagnostics_available is false, which adds its own).
        // diagnostics=None → diagnostics_available=false → caveat added.
        assert!(report
            .caveats
            .iter()
            .any(|c| c.contains("Extraction diagnostics unavailable")));
    }

    // ── Framework heavy → dead code LOW ──────────────────────

    #[test]
    fn framework_heavy_triggers_dead_code_low() {
        let mut input = minimal_input();
        // 5 tsx files out of 10 → 50% ratio, well above 20% threshold.
        input.file_paths = (0..5)
            .map(|i| format!("src/component_{}.tsx", i))
            .chain((0..5).map(|i| format!("src/util_{}.ts", i)))
            .collect();
        let report = compute_trust_report(&input);
        assert!(
            report
                .summary
                .triggered_downgrades
                .framework_heavy_suspicion
                .triggered
        );
        assert_eq!(
            report.summary.reliability.dead_code.level,
            ReliabilityLevel::LOW
        );
    }

    // ── Variant A reweighting ────────────────────────────────

    #[test]
    fn variant_a_reweighting_excludes_external_calls() {
        let mut input = minimal_input();
        let mut breakdown = BTreeMap::new();
        breakdown.insert("calls_obj_method_needs_type_info".into(), 100);
        input.diagnostics = Some(ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 200,
            unresolved_total: 100,
            unresolved_breakdown: breakdown,
        });
        input.resolved_calls = 50;
        // 80 of the 100 unresolved are external_library_candidate.
        input.calls_classification_counts = vec![
            ClassificationCountRow {
                classification: UnresolvedEdgeClassification::ExternalLibraryCandidate,
                count: 80,
            },
            ClassificationCountRow {
                classification: UnresolvedEdgeClassification::Unknown,
                count: 20,
            },
        ];

        let report = compute_trust_report(&input);
        // Internal-like = 100 - 80 = 20. Rate = 50 / (50 + 20) ≈ 0.714.
        assert_eq!(report.summary.unresolved_calls_external, 80);
        assert_eq!(report.summary.unresolved_calls_internal_like, 20);
        // 71.4% is between 50% and 85% → MEDIUM.
        assert_eq!(
            report.summary.reliability.call_graph.level,
            ReliabilityLevel::MEDIUM
        );
    }

    // ── Category rows sorted by unresolved desc ──────────────

    #[test]
    fn category_rows_sorted_by_unresolved_desc() {
        let mut input = minimal_input();
        let mut breakdown = BTreeMap::new();
        breakdown.insert("calls_obj_method_needs_type_info".into(), 5);
        breakdown.insert("imports_file_not_found".into(), 20);
        breakdown.insert("other".into(), 1);
        input.diagnostics = Some(ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 100,
            unresolved_total: 26,
            unresolved_breakdown: breakdown,
        });

        let report = compute_trust_report(&input);
        assert_eq!(report.categories.len(), 3);
        assert_eq!(report.categories[0].category, "imports_file_not_found");
        assert_eq!(report.categories[0].unresolved, 20);
        assert_eq!(report.categories[0].label, "IMPORTS (file not found)");
        assert_eq!(
            report.categories[1].category,
            "calls_obj_method_needs_type_info"
        );
        assert_eq!(report.categories[1].unresolved, 5);
        assert_eq!(report.categories[2].category, "other");
        assert_eq!(report.categories[2].unresolved, 1);
    }

    // ── Classification rows sorted by count desc ─────────────

    #[test]
    fn classification_rows_sorted_by_count_desc_then_key_asc() {
        let mut input = minimal_input();
        input.all_classification_counts = vec![
            ClassificationCountRow {
                classification: UnresolvedEdgeClassification::Unknown,
                count: 10,
            },
            ClassificationCountRow {
                classification: UnresolvedEdgeClassification::ExternalLibraryCandidate,
                count: 10,
            },
            ClassificationCountRow {
                classification: UnresolvedEdgeClassification::InternalCandidate,
                count: 5,
            },
        ];

        let report = compute_trust_report(&input);
        assert_eq!(report.classifications.len(), 3);
        // Same count (10) → sorted by classification asc.
        assert_eq!(
            report.classifications[0].classification,
            "external_library_candidate"
        );
        assert_eq!(report.classifications[1].classification, "unknown");
        assert_eq!(
            report.classifications[2].classification,
            "internal_candidate"
        );
        assert_eq!(report.classifications[2].count, 5);
    }

    // ── Blast radius breakdown ───────────────────────────────

    #[test]
    fn blast_radius_counts_per_level() {
        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 3,
        }];
        input.unknown_calls_samples = vec![
            // External import → low blast radius.
            TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
                basis_code: UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
                source_node_visibility: Some("export".into()),
                metadata_json: None,
            },
            // Same-file symbol, exported → low blast radius.
            TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
                basis_code: UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol,
                source_node_visibility: Some("export".into()),
                metadata_json: None,
            },
            // Internal import → medium blast radius.
            TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
                basis_code: UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport,
                source_node_visibility: Some("export".into()),
                metadata_json: None,
            },
        ];

        let report = compute_trust_report(&input);
        let br = report.unknown_calls_blast_radius.unwrap();
        assert_eq!(br.low, 2);
        assert_eq!(br.medium, 1);
        assert_eq!(br.high, 0);
    }

    // ── Enrichment status ────────────────────────────────────

    #[test]
    fn enrichment_null_when_no_enrichment_markers() {
        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 1,
        }];
        input.unknown_calls_samples = vec![TrustUnresolvedEdgeSample {
            category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
            source_node_visibility: None,
            metadata_json: None, // No metadata → no enrichment marker.
        }];

        let report = compute_trust_report(&input);
        // JSON-shape contract: enrichment_status is None when
        // no sample carried the enrichment marker. Parity with
        // TS is preserved by keeping this return shape
        // unchanged.
        assert!(report.enrichment_status.is_none());
        // Internal disambiguator: the sample count is
        // surfaced separately so downstream consumers can
        // tell this case (eligible > 0, phase did not run)
        // apart from the "no eligible samples at all" case.
        // The counter is `#[serde(skip)]` and never enters
        // the parity contract.
        assert_eq!(
            report.enrichment_eligible_count, 1,
            "phase-did-not-run case must report its eligible count"
        );
    }

    #[test]
    fn enrichment_eligible_count_is_zero_when_no_samples_at_all() {
        // Empty classification counts → upstream skips the
        // compute function entirely. The counter must be 0 so
        // the agent adapter maps this to NotApplicable, not
        // NotRun. Regression pin for the spike-follow-up P2
        // review: the previous behavior conflated this case
        // with "phase did not run" and would have emitted a
        // spurious TRUST_NO_ENRICHMENT signal plus a
        // confidence penalty on repos with nothing to enrich.
        let input = minimal_input();
        let report = compute_trust_report(&input);
        assert!(report.enrichment_status.is_none());
        assert_eq!(report.enrichment_eligible_count, 0);
    }

    #[test]
    fn enrichment_eligible_count_is_zero_when_samples_are_not_calls_obj_method() {
        // Samples exist but NONE are in
        // `CallsObjMethodNeedsTypeInfo`. The enrichment phase
        // only counts that one category as eligible, so the
        // counter stays 0, the status stays None, and the
        // adapter correctly reports NotApplicable.
        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 2,
        }];
        input.unknown_calls_samples = vec![
            TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
                basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
                source_node_visibility: None,
                metadata_json: None,
            },
            TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
                basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
                source_node_visibility: None,
                metadata_json: None,
            },
        ];

        let report = compute_trust_report(&input);
        assert!(
            report.enrichment_status.is_none(),
            "non-CallsObjMethodNeedsTypeInfo samples do not trigger status"
        );
        assert_eq!(
            report.enrichment_eligible_count, 0,
            "only CallsObjMethodNeedsTypeInfo samples increment the counter"
        );
    }

    #[test]
    fn enrichment_populated_when_markers_present() {
        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 2,
        }];
        input.unknown_calls_samples = vec![
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
				basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
				source_node_visibility: None,
				metadata_json: Some(
					r#"{"enrichment":{"receiverType":"Map","typeDisplayName":"Map","isExternalType":true}}"#.into(),
				),
			},
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
				basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
				source_node_visibility: None,
				metadata_json: Some(
					r#"{"enrichment":{"receiverType":"MyClass","isExternalType":false}}"#.into(),
				),
			},
		];

        let report = compute_trust_report(&input);
        let es = report.enrichment_status.unwrap();
        assert_eq!(es.eligible, 2);
        assert_eq!(es.enriched, 2);
        assert_eq!(es.top_types.len(), 2);
        // Sorted by count desc, then type_name asc.
        // Both have count=1, so sorted alphabetically: Map, MyClass.
        assert_eq!(es.top_types[0].type_name, "Map");
        assert!(es.top_types[0].is_external);
        assert_eq!(es.top_types[1].type_name, "MyClass");
        assert!(!es.top_types[1].is_external);
    }

    // ENRICH-YIELD-2 EY1-A read-path regression: the SAME simple name resolving to BOTH an external
    // and an internal receiver (e.g. std `Error` vs an in-repo `Error` — the resolver discards the
    // qualified path, so both arrive as bare `Error`) must be counted SEPARATELY per classification.
    // Keying the aggregation by name alone froze the first row's `isExternalType` for every same-name
    // row, which would let the likely-external projection report an internal-inflated external count.
    #[test]
    fn enrichment_same_name_both_classes_counted_separately() {
        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 5,
        }];
        let ext = || TrustUnresolvedEdgeSample {
            category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
            source_node_visibility: None,
            metadata_json: Some(
                r#"{"enrichment":{"receiverType":"Error","isExternalType":true}}"#.into(),
            ),
        };
        let internal = || TrustUnresolvedEdgeSample {
            category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
            source_node_visibility: None,
            metadata_json: Some(
                r#"{"enrichment":{"receiverType":"Error","isExternalType":false}}"#.into(),
            ),
        };
        // 3 external `Error`, 2 internal `Error`. If classification collapsed, we'd see one entry of
        // count 5; the fix keeps them apart.
        input.unknown_calls_samples = vec![ext(), ext(), ext(), internal(), internal()];

        let report = compute_trust_report(&input);
        let es = report.enrichment_status.unwrap();
        assert_eq!(es.top_types.len(), 2, "two entries: one per classification");
        let external = es
            .top_types
            .iter()
            .find(|t| t.is_external)
            .expect("an external `Error` entry");
        let internal = es
            .top_types
            .iter()
            .find(|t| !t.is_external)
            .expect("an internal `Error` entry");
        assert_eq!(external.type_name, "Error");
        assert_eq!(internal.type_name, "Error");
        assert_eq!(
            external.count, 3,
            "external count is not inflated by internal rows"
        );
        assert_eq!(internal.count, 2);
    }

    #[test]
    fn enrichment_populated_with_zero_enriched_when_enrichment_ran_but_no_types() {
        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 1,
        }];
        input.unknown_calls_samples = vec![TrustUnresolvedEdgeSample {
            category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
            source_node_visibility: None,
            // Enrichment key exists but no receiverType → enrichment ran, resolved 0.
            metadata_json: Some(r#"{"enrichment":{}}"#.into()),
        }];

        let report = compute_trust_report(&input);
        let es = report.enrichment_status.unwrap();
        assert_eq!(es.eligible, 1);
        assert_eq!(es.enriched, 0);
        assert_eq!(es.top_types.len(), 0);
        assert_eq!(es.top_external_types.len(), 0);
    }

    // RELIABILITY-REFRAME-1 (review-3 §3): `top_external_types` must FILTER to external FIRST
    // and truncate AFTER, so a genuine top external is never dropped by `top_types`' top-15-MIXED
    // cut. Here 15 INTERNAL receiver types each out-count the externals, pushing the externals
    // past rank 15 in the mixed order — the pre-fix "top-15-mixed THEN filter" would return an
    // EMPTY external list. Two equal-count externals also pin the deterministic name-asc tie-break.
    #[test]
    fn top_external_types_filter_then_truncate_keeps_external_below_15th_mixed() {
        fn receiver_sample(type_name: &str, is_external: bool) -> TrustUnresolvedEdgeSample {
            TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
                basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
                source_node_visibility: None,
                metadata_json: Some(format!(
                    r#"{{"enrichment":{{"receiverType":"{type_name}","isExternalType":{is_external}}}}}"#
                )),
            }
        }

        let mut input = minimal_input();
        input.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 200,
        }];
        let mut samples = Vec::new();
        // 15 internal receiver types, count 10 each → they fill the entire mixed top-15.
        for i in 0..15 {
            for _ in 0..10 {
                samples.push(receiver_sample(&format!("Internal{i:02}"), false));
            }
        }
        // 2 external receiver types, count 5 each → they rank 16th/17th in the mixed order.
        for _ in 0..5 {
            samples.push(receiver_sample("TokioRuntime", true));
            samples.push(receiver_sample("SerdeValue", true));
        }
        input.unknown_calls_samples = samples;

        let es = compute_trust_report(&input)
            .enrichment_status
            .expect("enrichment ran");

        // The MIXED top-15 is all internal — the externals are beyond it (the pre-fix hazard).
        assert_eq!(es.top_types.len(), 15);
        assert!(
            !es.top_types.iter().any(|t| t.is_external),
            "no external survived the mixed top-15 cut: {:?}",
            es.top_types
        );
        // Filter-THEN-truncate recovers BOTH externals, count-desc then NAME-ASC on the tie.
        assert_eq!(
            es.top_external_types.len(),
            2,
            "both externals selected from ALL externals: {:?}",
            es.top_external_types
        );
        assert_eq!(es.top_external_types[0].type_name, "SerdeValue"); // name-asc tie-break
        assert_eq!(es.top_external_types[1].type_name, "TokioRuntime");
        assert!(es
            .top_external_types
            .iter()
            .all(|t| t.is_external && t.count == 5));
    }

    // ── Module suspicious flag ───────────────────────────────

    #[test]
    fn module_suspicious_zero_connectivity_flagged() {
        let mut input = minimal_input();
        input.module_stats = vec![
            TrustModuleStats {
                stable_key: "r1:src/orphan:MODULE".into(),
                path: "src/orphan".into(),
                fan_in: 0,
                fan_out: 0,
                file_count: 3,
            },
            TrustModuleStats {
                stable_key: "r1:src/connected:MODULE".into(),
                path: "src/connected".into(),
                fan_in: 2,
                fan_out: 1,
                file_count: 5,
            },
        ];

        let report = compute_trust_report(&input);
        assert_eq!(report.modules.len(), 2);
        // Sorted by qualified_name: src/connected, src/orphan.
        assert_eq!(report.modules[0].qualified_name, "src/connected");
        assert!(!report.modules[0].suspicious_zero_connectivity);
        assert!(report.modules[0].trust_notes.is_empty());
        assert_eq!(report.modules[1].qualified_name, "src/orphan");
        assert!(report.modules[1].suspicious_zero_connectivity);
        assert_eq!(
            report.modules[1].trust_notes,
            vec!["alias_resolution_candidate"]
        );
    }

    #[test]
    fn root_module_not_flagged_suspicious() {
        let mut input = minimal_input();
        input.module_stats = vec![TrustModuleStats {
            stable_key: "r1:.:MODULE".into(),
            path: ".".into(),
            fan_in: 0,
            fan_out: 0,
            file_count: 10,
        }];

        let report = compute_trust_report(&input);
        assert!(!report.modules[0].suspicious_zero_connectivity);
    }

    #[test]
    fn module_rows_sorted_by_qualified_name_regardless_of_input_order() {
        let mut input = minimal_input();
        // Feed modules in reverse alphabetical order.
        input.module_stats = vec![
            TrustModuleStats {
                stable_key: "r1:src/z:MODULE".into(),
                path: "src/z".into(),
                fan_in: 0,
                fan_out: 0,
                file_count: 1,
            },
            TrustModuleStats {
                stable_key: "r1:src/a:MODULE".into(),
                path: "src/a".into(),
                fan_in: 0,
                fan_out: 0,
                file_count: 1,
            },
            TrustModuleStats {
                stable_key: "r1:src/m:MODULE".into(),
                path: "src/m".into(),
                fan_in: 0,
                fan_out: 0,
                file_count: 1,
            },
        ];

        let report = compute_trust_report(&input);
        assert_eq!(report.modules.len(), 3);
        assert_eq!(report.modules[0].qualified_name, "src/a");
        assert_eq!(report.modules[1].qualified_name, "src/m");
        assert_eq!(report.modules[2].qualified_name, "src/z");
    }

    // ── Caveats ──────────────────────────────────────────────

    #[test]
    fn caveats_for_non_high_levels() {
        let mut input = minimal_input();
        // Force LOW call graph by having lots of unresolved, few resolved.
        let mut breakdown = BTreeMap::new();
        breakdown.insert("calls_function_ambiguous_or_missing".into(), 100);
        input.diagnostics = Some(ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 110,
            unresolved_total: 100,
            unresolved_breakdown: breakdown,
        });
        input.resolved_calls = 10;
        // No entrypoints → missing_entrypoint triggered.
        input.active_entrypoint_count = 0;

        let report = compute_trust_report(&input);
        assert!(report
            .caveats
            .iter()
            .any(|c| c.contains("Your code's calls resolve at LOW reliability")));
        // Dead-code caveat removed: `rmap dead` surface is disabled.
    }

    // ── Call resolution rate edge case ────────────────────────

    #[test]
    fn call_resolution_rate_is_one_when_no_calls() {
        let input = minimal_input();
        let report = compute_trust_report(&input);
        assert_eq!(report.summary.call_resolution_rate, 1.0);
    }

    // ── human_label_for_category ─────────────────────────────

    #[test]
    fn human_labels_match_ts_labels() {
        assert_eq!(
            human_label_for_category("imports_file_not_found"),
            "IMPORTS (file not found)"
        );
        assert_eq!(
            human_label_for_category("calls_obj_method_needs_type_info"),
            "CALLS obj.method (needs type info)"
        );
        assert_eq!(human_label_for_category("other"), "OTHER (unclassified)");
        // Fallback: unknown category returns itself.
        assert_eq!(human_label_for_category("something_new"), "something_new");
    }

    // ── Assembly layer tests ─────────────────────────────────
    //
    // These test the assembly function directly using a mock
    // TrustStorageRead implementation. They pin:
    //   - storage error propagation
    //   - malformed toolchain JSON
    //   - malformed diagnostics JSON

    /// Mock storage that returns configurable results.
    struct MockStorage {
        diagnostics_json: Option<String>,
        file_paths: Vec<String>,
        module_stats: Vec<TrustModuleStats>,
        path_prefix_cycles: Vec<PathPrefixModuleCycle>,
        active_entrypoint_count: usize,
        resolved_calls: u64,
        calls_classification_counts: Vec<ClassificationCountRow>,
        all_classification_counts: Vec<ClassificationCountRow>,
        all_basis_code_counts: Vec<BasisCodeCountRow>,
        external_dependencies: ExternalDependencyAttribution,
        unknown_calls_samples: Vec<TrustUnresolvedEdgeSample>,
        /// If set, all methods return this error.
        force_error: Option<String>,
    }

    impl MockStorage {
        fn ok() -> Self {
            Self {
                diagnostics_json: None,
                file_paths: vec![],
                module_stats: vec![],
                path_prefix_cycles: vec![],
                active_entrypoint_count: 1,
                resolved_calls: 0,
                calls_classification_counts: vec![],
                all_classification_counts: vec![],
                all_basis_code_counts: vec![],
                external_dependencies: ExternalDependencyAttribution::default(),
                unknown_calls_samples: vec![],
                force_error: None,
            }
        }

        fn err_result<T>(&self) -> Result<T, String> {
            Err(self.force_error.clone().unwrap())
        }
    }

    impl TrustStorageRead for MockStorage {
        type Error = String;

        fn get_snapshot_extraction_diagnostics(
            &self,
            _snapshot_uid: &str,
        ) -> Result<Option<String>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.diagnostics_json.clone())
        }

        fn count_edges_by_type(
            &self,
            _snapshot_uid: &str,
            _edge_type: &str,
        ) -> Result<u64, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.resolved_calls)
        }

        fn count_active_declarations(&self, _repo_uid: &str, _kind: &str) -> Result<usize, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.active_entrypoint_count)
        }

        fn count_unresolved_edges_by_classification(
            &self,
            input: &CountByClassificationInput,
        ) -> Result<Vec<ClassificationCountRow>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            if input.filter_categories.is_empty() {
                Ok(self.all_classification_counts.clone())
            } else {
                Ok(self.calls_classification_counts.clone())
            }
        }

        fn count_unresolved_edges_by_basis_code(
            &self,
            _snapshot_uid: &str,
        ) -> Result<Vec<BasisCodeCountRow>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.all_basis_code_counts.clone())
        }

        fn attribute_external_dependencies(
            &self,
            _snapshot_uid: &str,
            _limit: u32,
        ) -> Result<ExternalDependencyAttribution, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.external_dependencies.clone())
        }

        fn query_unresolved_edges(
            &self,
            _input: &QueryUnresolvedEdgesInput,
        ) -> Result<Vec<TrustUnresolvedEdgeSample>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.unknown_calls_samples.clone())
        }

        fn find_path_prefix_module_cycles(
            &self,
            _snapshot_uid: &str,
        ) -> Result<Vec<PathPrefixModuleCycle>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.path_prefix_cycles.clone())
        }

        fn compute_module_stats(
            &self,
            _snapshot_uid: &str,
        ) -> Result<Vec<TrustModuleStats>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.module_stats.clone())
        }

        fn get_file_paths_by_repo(&self, _repo_uid: &str) -> Result<Vec<String>, String> {
            if self.force_error.is_some() {
                return self.err_result();
            }
            Ok(self.file_paths.clone())
        }
    }

    #[test]
    fn assembly_produces_report_from_mock_storage() {
        let mock = MockStorage::ok();
        let result = assemble_trust_report(&mock, "r1", "snap1", None, None);
        let report = result.unwrap();
        assert_eq!(report.snapshot_uid, "snap1");
        assert!(!report.diagnostics_available);
    }

    #[test]
    fn assembly_propagates_storage_error() {
        let mock = MockStorage {
            force_error: Some("database locked".into()),
            ..MockStorage::ok()
        };
        let result = assemble_trust_report(&mock, "r1", "snap1", None, None);
        match result {
            Err(TrustAssemblyError::Storage(msg)) => {
                assert_eq!(msg, "database locked");
            }
            other => panic!("expected Storage error, got {:?}", other),
        }
    }

    #[test]
    fn assembly_errors_on_malformed_toolchain_json() {
        let mock = MockStorage::ok();
        let result = assemble_trust_report(&mock, "r1", "snap1", None, Some("{invalid json"));
        match result {
            Err(TrustAssemblyError::JsonParse { field, .. }) => {
                assert_eq!(field, "toolchain_json");
            }
            other => panic!("expected JsonParse for toolchain, got {:?}", other),
        }
    }

    #[test]
    fn assembly_errors_on_malformed_diagnostics_json() {
        let mut mock = MockStorage::ok();
        mock.diagnostics_json = Some("{not valid json!!}".into());
        let result = assemble_trust_report(&mock, "r1", "snap1", None, None);
        match result {
            Err(TrustAssemblyError::JsonParse { field, .. }) => {
                assert_eq!(field, "extraction_diagnostics_json");
            }
            other => panic!("expected JsonParse for diagnostics, got {:?}", other),
        }
    }

    // ── DAEMON-CANCEL-3: sample-loop cooperative cancellation ─────────
    //
    // Deterministic (no timing, no daemon): prove the Phase-5 unresolved-sample loop
    // honors the cooperative checkpoint and surfaces `TrustReportOutcome::Cancelled`.
    // The daemon's worker+`sqlite3_interrupt` covers the SQL; this covers the pure
    // loop the slice names ("checkpoint the large unresolved-sample processing loop").

    /// `n` unknown CALLS samples (the heaviest per-row shape: a
    /// `CallsObjMethodNeedsTypeInfo` with enrichment metadata, so each iteration both
    /// derives a blast radius AND parses JSON — exactly the 100_000-row cost the slice
    /// flags).
    fn unknown_calls_samples(n: usize) -> Vec<TrustUnresolvedEdgeSample> {
        (0..n)
            .map(|_| TrustUnresolvedEdgeSample {
                category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
                basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
                source_node_visibility: Some("export".into()),
                metadata_json: Some(
                    r#"{"enrichment":{"receiverType":"Map","typeDisplayName":"Map","isExternalType":true}}"#
                        .into(),
                ),
            })
            .collect()
    }

    #[test]
    fn cancellable_assembly_breaks_the_sample_loop_mid_flight() {
        // 3000 samples ⇒ the loop polls the checkpoint at i = 0, 1024, 2048. A checkpoint
        // that breaks on its SECOND poll proves MID-loop cancellation (≈1024 of 3000
        // samples processed, not all) — fully deterministic, no wall-clock.
        let mut mock = MockStorage::ok();
        mock.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 3000,
        }];
        mock.unknown_calls_samples = unknown_calls_samples(3000);

        let mut polls = 0usize;
        let mut cancel = || {
            polls += 1;
            if polls >= 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let outcome =
            assemble_trust_report_cancellable(&mock, "r1", "snap1", None, None, &mut cancel)
                .expect("storage ok");
        assert!(
            matches!(outcome, TrustReportOutcome::Cancelled),
            "a checkpoint that breaks mid sample-loop must yield TrustReportOutcome::Cancelled"
        );
        assert_eq!(
            polls, 2,
            "the loop must stop at the breaking poll, not run to the end"
        );
    }

    #[test]
    fn cancellable_assembly_completes_and_matches_baseline_with_noop_checkpoint() {
        // Same input, never-breaking checkpoint ⇒ Ready, and the report is byte-identical
        // to the non-cancellable `assemble_trust_report` (the delegation is transparent).
        let mut mock = MockStorage::ok();
        mock.all_classification_counts = vec![ClassificationCountRow {
            classification: UnresolvedEdgeClassification::Unknown,
            count: 10,
        }];
        mock.unknown_calls_samples = unknown_calls_samples(10);

        let outcome =
            assemble_trust_report_cancellable(&mock, "r1", "snap1", None, None, &mut || {
                ControlFlow::Continue(())
            })
            .expect("storage ok");
        let report = match outcome {
            TrustReportOutcome::Ready(r) => *r,
            TrustReportOutcome::Cancelled => panic!("no-op checkpoint must not cancel"),
        };
        let baseline = assemble_trust_report(&mock, "r1", "snap1", None, None).expect("baseline");
        assert_eq!(
            report.unknown_calls_blast_radius, baseline.unknown_calls_blast_radius,
            "cancellable (no-op) path must compute the full loop, matching the baseline"
        );
        assert!(
            report.unknown_calls_blast_radius.is_some(),
            "the samples must have produced a blast-radius breakdown"
        );
    }
}
