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

/// The in-memory LiveGraph runtime substrate.
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
            );
        }

        // Contributing partitions = referencing ∪ defining.
        let mut contributing: BTreeSet<String> = referencing.keys().cloned().collect();
        if let Some((id, _)) = &defining {
            contributing.insert(id.clone());
        }

        // Freshness = worst (least fresh) contributing status; record contributing epochs (D3).
        let mut freshness = FreshnessState::Fresh;
        let mut contributing_epochs = BTreeMap::new();
        let mut language = LanguageSupport::TypeScriptPrimary;
        for id in &contributing {
            if let Some(s) = self.slots.get(id) {
                let f = status_freshness(s.status);
                if freshness_rank(f) > freshness_rank(freshness) {
                    freshness = f;
                }
                contributing_epochs.insert(id.clone(), s.epoch.0);
                language = s.language;
            }
        }

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

        // Classify by identity + freshness (residency applied after — a separate axis).
        let input = CompletenessInput {
            granularity: QueryGranularity::CallGraph,
            bases,
            freshness,
            degradation_reasons: reasons.clone(),
            language,
        };
        let (class_c, _completeness) = classify_answer(&input);

        // `callers` is a cross-partition, SCIP-dependent query: a pending SCIP refresh means the
        // answer CANNOT be Exact (trust invariant 6 — there is no NotScipDependent proof for a
        // cross-partition lookup), so PrecisionPending → Partial. A non-resident referencing
        // partition also forces Partial (residency). Both are valid non-Fresh / residency Partials.
        let scip_dependent_refresh = freshness == FreshnessState::PrecisionPending;
        let residency_incomplete = !missing.is_empty() && class_c == AnswerClass::Exact;
        let final_class = if scip_dependent_refresh || residency_incomplete {
            AnswerClass::Partial
        } else {
            class_c
        };

        match final_class {
            AnswerClass::Unavailable => {
                AnswerEnvelope::unavailable(DegradationReason::UnresolvedAlias, freshness)
            }
            AnswerClass::Stale => {
                AnswerEnvelope::stale(data, freshness, reasons, missing, Vec::new())
                    .expect("stale invariant holds")
            }
            AnswerClass::Partial => {
                AnswerEnvelope::partial(Some(data), reasons, missing, freshness, Vec::new())
                    .expect("partial invariant holds")
            }
            // final_class == Exact only when freshness == Fresh (PrecisionPending → Partial above).
            // callers NEVER uses exact_precision_pending — it is SCIP-dependent.
            AnswerClass::Exact => AnswerEnvelope::exact(
                data,
                QueryCompleteness::Complete,
                FreshnessState::Fresh,
                Vec::new(),
            )
            .expect("exact invariant holds"),
        }
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
}
