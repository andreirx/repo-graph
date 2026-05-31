#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # repo-graph-livegraph — in-memory LiveGraph runtime substrate (LIVEGRAPH-RUNTIME-1)
//!
//! Holds `PartitionIr`s by id with per-partition **epoch** + **residency**, keeps the
//! always-resident **xref** summary, and answers a cross-partition `callers` lookup as a
//! **trust-labelled** [`AnswerEnvelope`] (via `repo-graph-trust-model`).
//!
//! It is **fed** already-ingested `PartitionIr` (D1 accept + atomic swap) — it does NOT run
//! indexers, persist, or touch the CLI. Deps: `repo-graph-ir` + `repo-graph-trust-model` only.
//! Headless API (no query migration). See docs/slices/livegraph-runtime-1.md.

use repo_graph_ir::{IdentitySource, PartitionIr};
use repo_graph_trust_model::{
    classify_answer, AnswerClass, AnswerEnvelope, CompletenessInput, DegradationReason,
    FreshnessState, Granularity, IdentityBasis, LanguageSupport, QueryCompleteness,
    QueryGranularity,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Per-partition epoch (bumped on each swap; D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PartitionEpoch(pub u64);

/// Global xref epoch (bumped on each partition swap; D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct XrefEpoch(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshStatus {
    Current,
    Refreshing,
    Stale,
    RefreshFailed,
}

fn status_freshness(s: RefreshStatus) -> FreshnessState {
    match s {
        RefreshStatus::Current => FreshnessState::Fresh,
        RefreshStatus::Refreshing => FreshnessState::PrecisionPending,
        RefreshStatus::Stale => FreshnessState::Stale,
        RefreshStatus::RefreshFailed => FreshnessState::RefreshFailed,
    }
}

fn freshness_rank(f: FreshnessState) -> u8 {
    match f {
        FreshnessState::Fresh => 0,
        FreshnessState::PrecisionPending => 1,
        FreshnessState::Stale => 2,
        FreshnessState::RefreshFailed => 3,
        FreshnessState::Unavailable => 4,
    }
}

fn basis_of(src: IdentitySource) -> IdentityBasis {
    match src {
        IdentitySource::AstAdopted => IdentityBasis::AstAdopted,
        IdentitySource::ScipSynthesizedFallback => IdentityBasis::ScipSynthesized,
        IdentitySource::AstFileScope => IdentityBasis::AstFileScope,
    }
}

struct Slot {
    epoch: PartitionEpoch,
    status: RefreshStatus,
    ir: Option<PartitionIr>, // Some = resident; None = non-resident (summary retained)
    language: LanguageSupport,
    defines: HashMap<String, IdentityBasis>, // cross-partition key -> def basis (retained on unload)
    ref_counts: HashMap<String, usize>,      // cross-partition key -> reference count (retained)
}

/// The payload of a `callers` answer (`AnswerEnvelope<CallersAnswer>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallersAnswer {
    /// Per-partition reference counts (always from the xref summary).
    pub per_partition_counts: BTreeMap<String, usize>,
    /// (partition, caller key) identities — only for resident referencing partitions.
    pub caller_identities: Vec<(String, String)>,
    /// Contributing partition epochs (D3: the answer records which epochs it was built from).
    pub contributing_epochs: BTreeMap<String, u64>,
}

/// The payload of a `callees` answer (`AnswerEnvelope<CalleesAnswer>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalleesAnswer {
    /// Per-defining-partition callee counts (which partitions own the callees, and how many).
    pub per_partition_counts: BTreeMap<String, usize>,
    /// (callee key, defining partition) identities — populated for `CallerDetail`. The partition is
    /// `None` when the callee has no known defining partition (`UnresolvedAlias`).
    pub callee_identities: Vec<(String, Option<String>)>,
    /// Contributing partition epochs (D3).
    pub contributing_epochs: BTreeMap<String, u64>,
}

/// The in-memory LiveGraph runtime substrate.
///
/// **callers/callees residency asymmetry (LIVEGRAPH-RUNTIME-1 + QUERY-MIGRATION-1):** the
/// always-resident xref retains only DEFINITIONS + INCOMING reference counts, so `callers` can
/// answer at partition-summary level while referencing partitions are non-resident. `callees` needs
/// OUTGOING adjacency, which is NOT retained, so it requires the target's defining partition to be
/// resident. Summary-level `callees` is deferred until a measured memory model decides whether to
/// retain outgoing adjacency (ratified: not added in this slice).
#[derive(Default)]
pub struct LiveGraph {
    slots: HashMap<String, Slot>,
    xref_epoch: u64,
}

/// xref contribution of a partition: cross-partition key (an IR `CanonicalKey`) → def basis, and
/// reference counts (one per outgoing edge's destination key).
fn contribution(ir: &PartitionIr) -> (HashMap<String, IdentityBasis>, HashMap<String, usize>) {
    let mut defines = HashMap::new();
    for n in &ir.nodes {
        defines.insert(n.key.as_str().to_string(), basis_of(n.identity_source));
    }
    let mut ref_counts: HashMap<String, usize> = HashMap::new();
    for e in &ir.edges {
        *ref_counts.entry(e.dst.as_str().to_string()).or_default() += 1;
    }
    (defines, ref_counts)
}

/// Classify a cross-partition answer via the trust policy. `languages` supplies the (currently
/// policy-unused) representative language — the least-mature contributing language as a conservative
/// placeholder; the query-visible language set travels on the envelope, not here.
/// TECH DEBT: `CompletenessInput.language` is a single field and `classify_answer` does not yet
/// consume it; when the policy becomes language-aware it should read the full contributing set.
fn classify_cross_partition(
    bases: &[IdentityBasis],
    freshness: FreshnessState,
    reasons: &[DegradationReason],
    languages: &BTreeSet<LanguageSupport>,
) -> AnswerClass {
    let representative = languages
        .iter()
        .next_back()
        .copied()
        .unwrap_or(LanguageSupport::TypeScriptPrimary);
    let input = CompletenessInput {
        granularity: QueryGranularity::CallGraph,
        bases: bases.to_vec(),
        freshness,
        degradation_reasons: reasons.to_vec(),
        language: representative,
    };
    classify_answer(&input).0
}

/// Apply the SCIP-dependent refresh + residency rules and build the trust envelope shared by
/// `callers`/`callees`. A cross-partition lookup is SCIP-dependent: a pending SCIP refresh CANNOT be
/// `Exact` (trust invariant 6 — no `NotScipDependent` proof for a cross-partition answer), so
/// `PrecisionPending` → `Partial`; a non-resident contributing partition (`missing`) also forces
/// `Partial`. Never an exact-empty for a missing/stale state.
///
/// PRECONDITION: a `Partial` classification must be accompanied by a degradation reason, a missing
/// partition, or a non-`Fresh` freshness. A call-graph-incomplete defining basis (e.g.
/// `AstFileScope`) with none of those is not yet mapped to a `DegradationReason` and would panic the
/// `partial` constructor; unreachable with current call-graph fixtures (recorded follow-up).
fn finalize_envelope<T>(
    data: T,
    class_c: AnswerClass,
    freshness: FreshnessState,
    reasons: Vec<DegradationReason>,
    missing: Vec<String>,
    languages: BTreeSet<LanguageSupport>,
) -> AnswerEnvelope<T> {
    let scip_dependent_refresh = freshness == FreshnessState::PrecisionPending;
    let residency_incomplete = !missing.is_empty() && class_c == AnswerClass::Exact;
    let final_class = if scip_dependent_refresh || residency_incomplete {
        AnswerClass::Partial
    } else {
        class_c
    };
    match final_class {
        AnswerClass::Unavailable => {
            AnswerEnvelope::unavailable(DegradationReason::UnresolvedAlias, freshness, languages)
        }
        AnswerClass::Stale => {
            AnswerEnvelope::stale(data, freshness, reasons, missing, Vec::new(), languages)
                .expect("stale invariant holds")
        }
        AnswerClass::Partial => AnswerEnvelope::partial(
            Some(data),
            reasons,
            missing,
            freshness,
            Vec::new(),
            languages,
        )
        .expect("partial invariant holds"),
        AnswerClass::Exact => AnswerEnvelope::exact(
            data,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            Vec::new(),
            languages,
        )
        .expect("exact invariant holds"),
    }
}

impl LiveGraph {
    /// Empty runtime.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a partition resident (D2 explicit load): stores its IR + xref contribution, status
    /// `Current`, next epoch.
    pub fn load_partition(&mut self, id: &str, ir: PartitionIr, language: LanguageSupport) {
        let (defines, ref_counts) = contribution(&ir);
        let epoch = PartitionEpoch(self.slots.get(id).map(|s| s.epoch.0 + 1).unwrap_or(1));
        self.slots.insert(
            id.to_string(),
            Slot {
                epoch,
                status: RefreshStatus::Current,
                ir: Some(ir),
                language,
                defines,
                ref_counts,
            },
        );
    }

    /// Unload a partition's detail (D2 explicit unload): drops the IR but KEEPS the xref summary.
    pub fn unload_partition(&mut self, id: &str) {
        if let Some(s) = self.slots.get_mut(id) {
            s.ir = None;
        }
    }

    /// Begin a refresh: serve last-good, freshness becomes `PrecisionPending`.
    pub fn begin_refresh(&mut self, id: &str) {
        self.set_status(id, RefreshStatus::Refreshing);
    }
    /// Mark a partition stale (inputs changed; refresh not yet run).
    pub fn mark_stale(&mut self, id: &str) {
        self.set_status(id, RefreshStatus::Stale);
    }
    /// Mark a refresh failed: keep last-good epoch, freshness `RefreshFailed`.
    pub fn mark_refresh_failed(&mut self, id: &str) {
        self.set_status(id, RefreshStatus::RefreshFailed);
    }
    fn set_status(&mut self, id: &str, st: RefreshStatus) {
        if let Some(s) = self.slots.get_mut(id) {
            s.status = st;
        }
    }

    /// Accept a new `PartitionIr` and ATOMICALLY swap (D1): replace IR, bump epoch + xref epoch,
    /// status `Current`. The runtime never runs an indexer — the producer is upstream.
    pub fn swap_partition(&mut self, id: &str, new_ir: PartitionIr) {
        let (defines, ref_counts) = contribution(&new_ir);
        let epoch = PartitionEpoch(self.slots.get(id).map(|s| s.epoch.0 + 1).unwrap_or(1));
        let language = self
            .slots
            .get(id)
            .map(|s| s.language)
            .unwrap_or(LanguageSupport::TypeScriptPrimary);
        self.slots.insert(
            id.to_string(),
            Slot {
                epoch,
                status: RefreshStatus::Current,
                ir: Some(new_ir),
                language,
                defines,
                ref_counts,
            },
        );
        self.xref_epoch += 1;
    }

    /// The global xref epoch.
    pub fn xref_epoch(&self) -> XrefEpoch {
        XrefEpoch(self.xref_epoch)
    }
    /// A partition's current epoch, if known.
    pub fn partition_epoch(&self, id: &str) -> Option<PartitionEpoch> {
        self.slots.get(id).map(|s| s.epoch)
    }

    /// `callers(target)` — the trust-labelled cross-partition lookup (the headless Test API
    /// surface). Residency → class, epoch/refresh → freshness, identity → degradation, all routed
    /// through `repo-graph-trust-model`. Never an exact-empty for missing/stale state.
    pub fn callers(&self, target: &str, detail: Granularity) -> AnswerEnvelope<CallersAnswer> {
        let defining: Option<(String, IdentityBasis)> = self
            .slots
            .iter()
            .find_map(|(id, s)| s.defines.get(target).map(|b| (id.clone(), *b)));
        let referencing: BTreeMap<String, usize> = self
            .slots
            .iter()
            .filter_map(|(id, s)| s.ref_counts.get(target).map(|c| (id.clone(), *c)))
            .collect();

        // Not in the xref at all → Unavailable (null ≠ empty).
        if defining.is_none() && referencing.is_empty() {
            return AnswerEnvelope::unavailable(
                DegradationReason::UnresolvedAlias,
                FreshnessState::Unavailable,
                BTreeSet::new(),
            );
        }

        // Contributing partitions = referencing ∪ defining.
        let mut contributing: BTreeSet<String> = referencing.keys().cloned().collect();
        if let Some((id, _)) = &defining {
            contributing.insert(id.clone());
        }

        // Worst freshness + contributing epochs + the D1 contributing-language UNION (no collapse).
        let (freshness, contributing_epochs, languages) =
            self.fold_contributing(contributing.iter());

        // Bases: the defining basis + (CallerDetail) resident callers' bases.
        let mut bases: Vec<IdentityBasis> = Vec::new();
        if let Some((_, b)) = &defining {
            bases.push(*b);
        }

        // Build the payload + residency-missing list (CallerDetail needs resident IR).
        let mut caller_identities = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        if detail == Granularity::CallerDetail {
            for id in referencing.keys() {
                match self.slots.get(id) {
                    Some(s) if s.ir.is_some() => {
                        let ir = s.ir.as_ref().unwrap();
                        for e in &ir.edges {
                            if e.dst.as_str() == target {
                                caller_identities.push((id.clone(), e.src.as_str().to_string()));
                                if let Some(n) =
                                    ir.nodes.iter().find(|n| n.key.as_str() == e.src.as_str())
                                {
                                    bases.push(basis_of(n.identity_source));
                                }
                            }
                        }
                    }
                    _ => missing.push(id.clone()),
                }
            }
        }

        // Identity degradation reasons derived from bases (SCIP-synthesized → fallback identity).
        let mut reasons: Vec<DegradationReason> = Vec::new();
        if bases.contains(&IdentityBasis::ScipSynthesized) {
            reasons.push(DegradationReason::ScipFallbackIdentity);
        }

        let data = CallersAnswer {
            per_partition_counts: referencing,
            caller_identities,
            contributing_epochs,
        };

        let class_c = classify_cross_partition(&bases, freshness, &reasons, &languages);
        finalize_envelope(data, class_c, freshness, reasons, missing, languages)
    }

    /// Fold contributing partition ids into `(worst freshness, contributing epochs, contributing
    /// language set)`. The language set is the D1 query-visible UNION — every contributing
    /// partition's `LanguageSupport`, never a collapsed/last-wins value.
    fn fold_contributing<'a>(
        &self,
        ids: impl Iterator<Item = &'a String>,
    ) -> (
        FreshnessState,
        BTreeMap<String, u64>,
        BTreeSet<LanguageSupport>,
    ) {
        let mut freshness = FreshnessState::Fresh;
        let mut epochs = BTreeMap::new();
        let mut languages = BTreeSet::new();
        for id in ids {
            if let Some(s) = self.slots.get(id) {
                let f = status_freshness(s.status);
                if freshness_rank(f) > freshness_rank(freshness) {
                    freshness = f;
                }
                epochs.insert(id.clone(), s.epoch.0);
                languages.insert(s.language);
            }
        }
        (freshness, epochs, languages)
    }

    /// `callees(target)` — the symmetric OUTGOING-edge lookup (D2, QUERY-MIGRATION-1). Reads
    /// target's outgoing edges from its defining partition's IR, so that partition MUST be resident
    /// (the always-resident xref retains only incoming adjacency — see the callers/callees residency
    /// asymmetry note on [`LiveGraph`]). Each callee destination is resolved to its defining
    /// partition via the retained `defines` summary; those partitions need NOT be resident. The
    /// contributing-language set unions the target partition + every resolved callee partition.
    pub fn callees(&self, target: &str, detail: Granularity) -> AnswerEnvelope<CalleesAnswer> {
        let defining: Option<(String, IdentityBasis)> = self
            .slots
            .iter()
            .find_map(|(id, s)| s.defines.get(target).map(|b| (id.clone(), *b)));
        let (def_part, def_basis) = match defining {
            Some(d) => d,
            // Unknown target → Unavailable (D1: contributing languages empty).
            None => {
                return AnswerEnvelope::unavailable(
                    DegradationReason::UnresolvedAlias,
                    FreshnessState::Unavailable,
                    BTreeSet::new(),
                )
            }
        };
        let def_slot = self.slots.get(&def_part).expect("defining slot exists");

        // Defining partition non-resident → cannot read outgoing edges → Partial(missing=[def_part]).
        if def_slot.ir.is_none() {
            return AnswerEnvelope::partial(
                None,
                Vec::new(),
                vec![def_part.clone()],
                status_freshness(def_slot.status),
                Vec::new(),
                BTreeSet::from([def_slot.language]),
            )
            .expect("partial with missing partition is valid");
        }

        // Resident: read target's outgoing edges; resolve each callee to its defining partition.
        let ir = def_slot.ir.as_ref().unwrap();
        let mut bases: Vec<IdentityBasis> = vec![def_basis];
        let mut callee_identities: Vec<(String, Option<String>)> = Vec::new();
        let mut per_partition_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut contributing: BTreeSet<String> = BTreeSet::from([def_part.clone()]);
        let mut has_unresolved = false;

        for e in &ir.edges {
            if e.src.as_str() != target {
                continue;
            }
            let callee_key = e.dst.as_str().to_string();
            let owner: Option<(String, IdentityBasis)> = self
                .slots
                .iter()
                .find_map(|(id, s)| s.defines.get(&callee_key).map(|b| (id.clone(), *b)));
            match owner {
                Some((owner_id, owner_basis)) => {
                    bases.push(owner_basis);
                    contributing.insert(owner_id.clone());
                    *per_partition_counts.entry(owner_id.clone()).or_default() += 1;
                    if detail == Granularity::CallerDetail {
                        callee_identities.push((callee_key, Some(owner_id)));
                    }
                }
                // Callee with no known defining partition (XPART unresolved) → Partial.
                None => {
                    has_unresolved = true;
                    if detail == Granularity::CallerDetail {
                        callee_identities.push((callee_key, None));
                    }
                }
            }
        }

        let mut reasons: Vec<DegradationReason> = Vec::new();
        if has_unresolved {
            reasons.push(DegradationReason::UnresolvedAlias);
        }
        if bases.contains(&IdentityBasis::ScipSynthesized) {
            reasons.push(DegradationReason::ScipFallbackIdentity);
        }

        let (freshness, contributing_epochs, languages) =
            self.fold_contributing(contributing.iter());

        let data = CalleesAnswer {
            per_partition_counts,
            callee_identities,
            contributing_epochs,
        };

        // Callee partitions need not be resident (resolved via the retained defines summary), so the
        // resident path has NO missing partitions; degradation comes from freshness or unresolved
        // callees. callees is SCIP-dependent (cross-partition resolution): PrecisionPending → Partial.
        let class_c = classify_cross_partition(&bases, freshness, &reasons, &languages);
        finalize_envelope(data, class_c, freshness, reasons, Vec::new(), languages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_ir::{
        CanonicalKey, EdgeBasis, EdgeType, IrEdge, IrNode, Partition, PartitionId, PartitionKind,
        Provenance,
    };

    fn part(id: &str) -> Partition {
        Partition {
            id: PartitionId::new(id),
            kind: PartitionKind::TsPackage,
            root: "/x".into(),
            indexer: "scip-typescript".into(),
            indexer_version: "0.4.0".into(),
            build_inputs_hash: "h".into(),
        }
    }
    fn prov() -> Provenance {
        Provenance {
            indexer: "scip-typescript".into(),
            indexer_version: "0.4.0".into(),
            scip_symbol_id: None,
            build_inputs_hash: "h".into(),
        }
    }
    fn node(key: &str, src: IdentitySource) -> IrNode {
        IrNode {
            key: CanonicalKey::from_existing(key),
            subtype: "FUNCTION".into(),
            name: key.into(),
            range: None,
            partition_id: PartitionId::new("p"),
            identity_source: src,
            provenance: prov(),
        }
    }
    fn edge(src: &str, dst: &str) -> IrEdge {
        IrEdge {
            src: CanonicalKey::from_existing(src),
            dst: CanonicalKey::from_existing(dst),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
        }
    }
    fn ir(id: &str, nodes: Vec<IrNode>, edges: Vec<IrEdge>) -> PartitionIr {
        PartitionIr {
            partition: part(id),
            nodes,
            edges,
        }
    }

    // engine defines + internally references `engine.foo`; api references it. Target = engine.foo.
    fn engine() -> PartitionIr {
        ir(
            "engine",
            vec![
                node("engine.foo", IdentitySource::AstAdopted),
                node("engine.bar", IdentitySource::AstAdopted),
            ],
            vec![edge("engine.bar", "engine.foo")],
        )
    }
    fn api() -> PartitionIr {
        ir(
            "api",
            vec![node("api.caller", IdentitySource::AstAdopted)],
            vec![edge("api.caller", "engine.foo")],
        )
    }
    fn both() -> LiveGraph {
        let mut lg = LiveGraph::new();
        lg.load_partition("engine", engine(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("api", api(), LanguageSupport::TypeScriptPrimary);
        lg
    }

    #[test]
    fn case1_both_resident_is_exact() {
        let lg = both();
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Exact);
        assert_eq!(a.freshness(), FreshnessState::Fresh);
        assert!(a.missing_partitions().is_empty());
        // both engine (internal) and api callers present
        assert_eq!(a.data().unwrap().caller_identities.len(), 2);
    }

    #[test]
    fn case2_caller_nonresident_detail_partial_summary_exact() {
        let mut lg = both();
        lg.unload_partition("api"); // summary kept, detail dropped
        let detail = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_eq!(detail.class(), AnswerClass::Partial);
        assert_eq!(detail.missing_partitions(), ["api"]);
        assert!(detail.degradation_reasons().is_empty()); // residency, not identity
        let summary = lg.callers("engine.foo", Granularity::PartitionSummary);
        assert_eq!(summary.class(), AnswerClass::Exact);
        assert!(summary.missing_partitions().is_empty());
    }

    #[test]
    fn case3_xref_absent_is_unavailable_not_empty() {
        let lg = both();
        let a = lg.callers("nonexistent.symbol", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Unavailable);
        assert!(a.data().is_none()); // null ≠ empty
        assert!(!a.degradation_reasons().is_empty());
    }

    #[test]
    fn case4_stale_contributing_partition_is_stale() {
        let mut lg = both();
        lg.mark_stale("api");
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(a.data().is_some()); // last-good served, not empty
    }

    #[test]
    fn refresh_pending_returns_partial_precision_pending() {
        let mut lg = both();
        lg.begin_refresh("api");
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        // callers is SCIP-dependent: a pending SCIP refresh is Partial + PrecisionPending, NOT Exact
        // (Exact + PrecisionPending is admissible only with a NotScipDependent proof, which callers
        // — a cross-partition lookup — does not have).
        assert_ne!(a.class(), AnswerClass::Exact);
        assert_eq!(a.class(), AnswerClass::Partial);
        assert_eq!(a.freshness(), FreshnessState::PrecisionPending);
        assert!(a.data().is_some()); // last-good served
    }

    #[test]
    fn case6_refresh_failed_keeps_last_good() {
        let mut lg = both();
        lg.mark_refresh_failed("api");
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_eq!(a.freshness(), FreshnessState::RefreshFailed);
        assert_eq!(a.class(), AnswerClass::Stale);
        assert!(a.data().is_some()); // last-good preserved
    }

    #[test]
    fn case7_no_exact_empty_for_degraded_states() {
        // Unavailable / Partial(missing) / Stale must never be an Exact empty result.
        let mut lg = both();
        lg.unload_partition("api");
        let partial = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_ne!(partial.class(), AnswerClass::Exact);
        let unavailable = lg.callers("nope", Granularity::CallerDetail);
        assert_ne!(unavailable.class(), AnswerClass::Exact);
        assert!(unavailable.data().is_none());
    }

    #[test]
    fn accept_swap_bumps_epoch_atomically() {
        let mut lg = both();
        let e0 = lg.partition_epoch("api").unwrap();
        let x0 = lg.xref_epoch();
        lg.swap_partition("api", api());
        assert!(lg.partition_epoch("api").unwrap() > e0);
        assert!(lg.xref_epoch() > x0);
        // after a successful swap, freshness is Fresh again
        assert_eq!(
            lg.callers("engine.foo", Granularity::CallerDetail)
                .freshness(),
            FreshnessState::Fresh
        );
    }

    // ── QUERY-MIGRATION-1: mixed-language fixtures + contributing-language union ──

    // engine(TS) defines foo/bar/entry; entry calls foo (TS), rust.helper (Rust), cpp.helper (Cpp).
    fn engine_mixed() -> PartitionIr {
        ir(
            "engine",
            vec![
                node("engine.foo", IdentitySource::AstAdopted),
                node("engine.bar", IdentitySource::AstAdopted),
                node("engine.entry", IdentitySource::AstAdopted),
            ],
            vec![
                edge("engine.bar", "engine.foo"),
                edge("engine.entry", "engine.foo"),
                edge("engine.entry", "rust.helper"),
                edge("engine.entry", "cpp.helper"),
            ],
        )
    }
    fn rustmod() -> PartitionIr {
        ir(
            "rustmod",
            vec![
                node("rust.caller", IdentitySource::AstAdopted),
                node("rust.helper", IdentitySource::AstAdopted),
            ],
            vec![edge("rust.caller", "engine.foo")],
        )
    }
    fn cppmod() -> PartitionIr {
        ir(
            "cppmod",
            vec![
                node("cpp.caller", IdentitySource::AstAdopted),
                node("cpp.helper", IdentitySource::AstAdopted),
            ],
            vec![edge("cpp.caller", "engine.foo")],
        )
    }
    // engine(TS) + api(TS) + rustmod(Rust) + cppmod(Cpp); all reference engine.foo.
    fn mixed() -> LiveGraph {
        let mut lg = LiveGraph::new();
        lg.load_partition("engine", engine_mixed(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("api", api(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("rustmod", rustmod(), LanguageSupport::RustPartialBeta);
        lg.load_partition("cppmod", cppmod(), LanguageSupport::CppGuarded);
        lg
    }

    #[test]
    fn callers_contributing_languages_union() {
        let lg = mixed();
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        let langs = a.contributing_languages();
        assert!(langs.contains(&LanguageSupport::TypeScriptPrimary));
        assert!(langs.contains(&LanguageSupport::RustPartialBeta));
        assert!(langs.contains(&LanguageSupport::CppGuarded));
    }

    #[test]
    fn callees_contributing_languages_union() {
        let lg = mixed();
        let a = lg.callees("engine.entry", Granularity::CallerDetail);
        let langs = a.contributing_languages();
        assert!(langs.contains(&LanguageSupport::TypeScriptPrimary));
        assert!(langs.contains(&LanguageSupport::RustPartialBeta));
        assert!(langs.contains(&LanguageSupport::CppGuarded));
        assert_eq!(a.data().unwrap().callee_identities.len(), 3);
    }

    #[test]
    fn mixed_language_answer_has_all_languages() {
        let lg = mixed();
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_eq!(
            *a.contributing_languages(),
            BTreeSet::from([
                LanguageSupport::TypeScriptPrimary,
                LanguageSupport::CppGuarded,
                LanguageSupport::RustPartialBeta,
            ])
        );
    }

    #[test]
    fn no_last_wins_language_collapse() {
        // Regression: the runtime must NOT collapse contributing partitions to one arbitrary value.
        let lg = mixed();
        let a = lg.callees("engine.entry", Granularity::CallerDetail);
        assert!(a.contributing_languages().len() > 1);
        assert!(a
            .contributing_languages()
            .contains(&LanguageSupport::TypeScriptPrimary));
        assert!(a
            .contributing_languages()
            .contains(&LanguageSupport::RustPartialBeta));
    }

    // ── callees core semantics (mirror the callers cases) ──

    #[test]
    fn callees_all_resolved_resident_is_exact() {
        let lg = mixed();
        let a = lg.callees("engine.entry", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Exact);
        assert_eq!(a.freshness(), FreshnessState::Fresh);
        assert!(a.missing_partitions().is_empty());
    }

    #[test]
    fn callees_target_partition_nonresident_is_partial_missing() {
        let mut lg = mixed();
        lg.unload_partition("engine"); // target engine.entry's defining partition
        let a = lg.callees("engine.entry", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Partial);
        assert_eq!(a.missing_partitions(), ["engine"]);
        assert!(a.data().is_none());
        // residency degradation, not identity; language still known from the retained summary
        assert!(a.degradation_reasons().is_empty());
        assert!(a
            .contributing_languages()
            .contains(&LanguageSupport::TypeScriptPrimary));
    }

    #[test]
    fn callees_unknown_target_is_unavailable() {
        let lg = mixed();
        let a = lg.callees("nonexistent.symbol", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Unavailable);
        assert!(a.data().is_none()); // null ≠ empty
        assert!(a.contributing_languages().is_empty());
    }

    #[test]
    fn callees_unresolved_callee_is_partial() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "lone",
            ir(
                "lone",
                vec![node("lone.entry", IdentitySource::AstAdopted)],
                vec![edge("lone.entry", "ghost.callee")], // defined in no partition
            ),
            LanguageSupport::TypeScriptPrimary,
        );
        let a = lg.callees("lone.entry", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Partial);
        assert!(a
            .degradation_reasons()
            .contains(&DegradationReason::UnresolvedAlias));
        // the unresolved callee is listed with no owning partition
        assert_eq!(
            a.data().unwrap().callee_identities,
            vec![("ghost.callee".to_string(), None)]
        );
    }

    #[test]
    fn callees_stale_callee_partition_is_stale() {
        let mut lg = mixed();
        lg.mark_stale("rustmod"); // a contributing callee partition
        let a = lg.callees("engine.entry", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(a.data().is_some()); // last-good served
    }
}
