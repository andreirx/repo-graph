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

use repo_graph_import_resolver::{
    dirname, file_key_path, resolve_imports, FileInventory, ImportCandidate,
    ResolvedImportEdgeCandidate,
};
use repo_graph_ir::{
    CanonicalKey, EdgeBasis, IdentitySource, ImportResolution, PartitionIr, Provenance, SourceRange,
};
use repo_graph_trust_model::{
    classify_answer, AnswerClass, AnswerEnvelope, CompletenessInput, DegradationReason,
    FreshnessState, Granularity, IdentityBasis, LanguageSupport, NotScipDependent,
    QueryCompleteness, QueryGranularity,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// MODULE-AGGREGATION-1 (D5): compare LiveGraph-derived MODULE cycles to SQLite `rmap cycles` + class the
/// divergences. A separate module (the 500-line guardrail keeps it out of this file).
pub mod module_cycle_compare;

use module_cycle_compare::{ObsResolution, ObservationView};

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

/// Merge degradation reasons into `out` without duplicates (order-preserving). Used to fold
/// partition-level reasons (PRODUCER-ABSENT-1) into a query's identity-derived reasons.
fn merge_partition_reasons(
    out: &mut Vec<DegradationReason>,
    more: impl Iterator<Item = DegradationReason>,
) {
    for r in more {
        if !out.contains(&r) {
            out.push(r);
        }
    }
}

struct Slot {
    epoch: PartitionEpoch,
    status: RefreshStatus,
    ir: Option<PartitionIr>, // Some = resident; None = non-resident (summary retained)
    language: LanguageSupport,
    defines: HashMap<String, IdentityBasis>, // cross-partition key -> def basis (retained on unload)
    ref_counts: HashMap<String, usize>,      // cross-partition key -> reference count (retained)
    value_facts: Vec<ValueFact>,             // VALUE-JOIN-1: value facts (retained on unload)
    value_facts_epoch: Option<PartitionEpoch>, // D7: partition epoch the facts were loaded for
    // PRODUCER-ABSENT-1: partition-level degradation reasons (e.g. `ProducerUnavailable` when this
    // partition was warm-loaded with the producer absent). A fresh `load_partition` (a successful
    // producer refresh) starts empty, which clears any prior `ProducerUnavailable`.
    partition_degradation_reasons: BTreeSet<DegradationReason>,
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

/// The payload of a `path` answer (`AnswerEnvelope<PathAnswer>`; PATH-CYCLES-LIVEGRAPH-1). A shortest
/// path over the STRICT call graph (`SyntaxConfirmedCall` edges). Empty `nodes` = no path (the class
/// distinguishes a PROVEN no-path (`Exact`) from an incomplete traversal (`Partial`)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAnswer {
    /// The path node keys in order (`from` .. `to`); empty if no path.
    pub nodes: Vec<String>,
    /// The path edges `(src, dst)` in order; empty if no path.
    pub edges: Vec<(String, String)>,
    /// Contributing partition epochs (the partitions the path's nodes/edges came from).
    pub contributing_epochs: BTreeMap<String, u64>,
}

// ── VALUE-JOIN-1 value-fact model (D6 separate channel; D1 cyclomatic complexity) ──

/// One kind of value-layer fact (D1: cyclomatic complexity only this slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueFactKind {
    /// Cyclomatic complexity of a function/method.
    CyclomaticComplexity,
}

/// What a value fact is attached to. A `RawAnchor` is NOT a canonical identity (D3 — do not
/// overload `CanonicalKey` for raw anchors); `SourceRange` already carries the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSubject {
    /// Attached to a canonical symbol identity.
    Symbol(CanonicalKey),
    /// Attached only to a source range (ownership not certified).
    RawAnchor(SourceRange),
}

/// A value-layer fact attached under the trust model. The measured `value` is a true observation;
/// the `basis` governs ONLY the ownership claim (the key semantic rule: a value fact is not less
/// true because it is raw-anchored — only the ownership claim is degraded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFact {
    /// What the fact is attached to.
    pub subject: ValueSubject,
    /// The fact kind.
    pub kind: ValueFactKind,
    /// The measured value.
    pub value: u32,
    /// The identity basis governing the ownership claim.
    pub basis: IdentityBasis,
    /// The source range the fact was observed at, if known.
    pub source_range: Option<SourceRange>,
    /// External-producer provenance.
    pub provenance: Provenance,
}

/// The payload of a `value_facts` answer (`AnswerEnvelope<ValueFactsAnswer>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFactsAnswer {
    /// The symbol-owned (or ownership-degraded) value facts for the queried symbol.
    pub facts: Vec<ValueFact>,
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
    /// IMPORTS-XPART-WIRING-1 (D2): the in-memory cross-partition import overlay — `StaticUnresolved`
    /// import observations upgraded into node-resolved FILE -> FILE edges over the RESIDENT FILE
    /// inventory (the resolver's output, basis `AstImportFileInventoryResolved`). Rebuilt eagerly on
    /// every load/swap/unload (D3) so it is always coherent with the resident set; NEVER serialized
    /// (per-partition cache coherence, F1 — it is derived, not a partition fact).
    xpart_overlay: Vec<ResolvedImportEdgeCandidate>,
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
                value_facts: Vec::new(),
                value_facts_epoch: None,
                partition_degradation_reasons: BTreeSet::new(),
            },
        );
        self.rebuild_xpart_overlay(); // D3: keep the cross-partition overlay coherent with the new set.
    }

    /// Unload a partition's detail (D2 explicit unload): drops the IR but KEEPS the xref summary.
    pub fn unload_partition(&mut self, id: &str) {
        if let Some(s) = self.slots.get_mut(id) {
            s.ir = None;
        }
        // D3: the unloaded partition's FILE nodes leave the inventory, so any cross-partition edge
        // touching it can no longer resolve -> the rebuild drops it (acceptance #4 degradation).
        self.rebuild_xpart_overlay();
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

    /// Add a partition-level degradation reason (PRODUCER-ABSENT-1). Surfaced by `callers` / `callees`
    /// / `value_facts` answers that touch this partition. No-op if the partition is not loaded. A fresh
    /// `load_partition` / `swap_partition` (a producer refresh) starts with none, clearing these.
    pub fn add_partition_degradation(&mut self, id: &str, reason: DegradationReason) {
        if let Some(s) = self.slots.get_mut(id) {
            s.partition_degradation_reasons.insert(reason);
        }
    }

    /// The partition-level degradation reasons for `id` (PRODUCER-ABSENT-1), empty if none / unloaded.
    fn partition_reasons(&self, id: &str) -> Vec<DegradationReason> {
        self.slots
            .get(id)
            .map(|s| s.partition_degradation_reasons.iter().copied().collect())
            .unwrap_or_default()
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
        // D7: carry the prior epoch's value facts forward as last-good; their `value_facts_epoch`
        // stays the OLD epoch, so `value_facts` detects the mismatch and reports them `Stale` until
        // reloaded for the new epoch (never silently attached to the new graph epoch).
        let (value_facts, value_facts_epoch) = self
            .slots
            .get(id)
            .map(|s| (s.value_facts.clone(), s.value_facts_epoch))
            .unwrap_or_default();
        self.slots.insert(
            id.to_string(),
            Slot {
                epoch,
                status: RefreshStatus::Current,
                ir: Some(new_ir),
                language,
                defines,
                ref_counts,
                value_facts,
                value_facts_epoch,
                // A swap is a fresh producer IR -> clears any prior `ProducerUnavailable` (PRODUCER-ABSENT-1).
                partition_degradation_reasons: BTreeSet::new(),
            },
        );
        self.xref_epoch += 1;
        self.rebuild_xpart_overlay(); // D3: a swapped IR can add/remove cross-partition imports.
    }

    /// Load value facts for a partition (VALUE-JOIN-1, D6 separate channel). Stamps the current
    /// partition epoch (D7) — a later swap without reload makes these facts detectably `Stale`.
    /// No-op if the partition is not loaded (value facts attach to an existing partition).
    pub fn load_value_facts(&mut self, id: &str, facts: Vec<ValueFact>) {
        if let Some(s) = self.slots.get_mut(id) {
            s.value_facts = facts;
            s.value_facts_epoch = Some(s.epoch);
        }
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
        // PRODUCER-ABSENT-1: surface each contributing partition's partition-level degradation reasons
        // (e.g. `ProducerUnavailable` when that partition was warm-loaded producer-absent).
        merge_partition_reasons(
            &mut reasons,
            contributing
                .iter()
                .flat_map(|id| self.partition_reasons(id)),
        );

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
        // PRODUCER-ABSENT-1: include contributing partitions' partition-level degradation reasons.
        merge_partition_reasons(
            &mut reasons,
            contributing
                .iter()
                .flat_map(|id| self.partition_reasons(id)),
        );

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

    /// `value_facts(symbol)` — the headless value-fact lookup (VALUE-JOIN-1, D5). Returns the
    /// symbol-owned value facts as a trust-labelled `AnswerEnvelope` (`SymbolOwnership` granularity).
    /// The KEY SEMANTIC RULE: ownership is degraded — NOT the measured value — when the basis does
    /// not certify ownership. Epoch-bound (D7): facts from a superseded partition epoch are `Stale`.
    pub fn value_facts(&self, symbol: &str) -> AnswerEnvelope<ValueFactsAnswer> {
        // Resolve the defining partition; unknown symbol → Unavailable (null ≠ empty).
        let def_part = match self
            .slots
            .iter()
            .find_map(|(id, s)| s.defines.get(symbol).map(|_| id.clone()))
        {
            Some(id) => id,
            None => {
                return AnswerEnvelope::unavailable(
                    DegradationReason::UnresolvedAlias,
                    FreshnessState::Unavailable,
                    BTreeSet::new(),
                )
            }
        };
        let slot = self.slots.get(&def_part).expect("defining slot exists");
        let languages = BTreeSet::from([slot.language]);

        // Symbol-subject value facts for this symbol (RawAnchor-subject facts are stored but not
        // symbol-retrievable — range retrieval is a follow-up).
        let facts: Vec<ValueFact> = slot
            .value_facts
            .iter()
            .filter(|f| matches!(&f.subject, ValueSubject::Symbol(k) if k.as_str() == symbol))
            .cloned()
            .collect();
        if facts.is_empty() {
            // Known symbol, no value fact (e.g. not a function) → Unavailable, not empty.
            return AnswerEnvelope::unavailable(
                DegradationReason::UnresolvedAlias,
                FreshnessState::Unavailable,
                languages,
            );
        }

        let mut contributing_epochs = BTreeMap::new();
        contributing_epochs.insert(def_part.clone(), slot.epoch.0);
        let basis = facts[0].basis; // D1: one value-fact kind (complexity) per symbol
        let data = ValueFactsAnswer {
            facts,
            contributing_epochs,
        };

        // Non-resident defining partition → Partial + missing (retained last-good).
        if slot.ir.is_none() {
            return AnswerEnvelope::partial(
                Some(data),
                Vec::new(),
                vec![def_part],
                status_freshness(slot.status),
                Vec::new(),
                languages,
            )
            .expect("partial with missing partition is valid");
        }

        // Freshness: partition status, bumped to at least `Stale` on an epoch mismatch (D7).
        let base = status_freshness(slot.status);
        let epoch_mismatch = slot.value_facts_epoch != Some(slot.epoch);
        let freshness =
            if epoch_mismatch && freshness_rank(base) < freshness_rank(FreshnessState::Stale) {
                FreshnessState::Stale
            } else {
                base
            };

        // Ownership (D2): owned iff the basis is `SymbolOwnership`-complete (derived via the policy).
        let owned = classify_answer(&CompletenessInput {
            granularity: QueryGranularity::SymbolOwnership,
            bases: vec![basis],
            freshness: FreshnessState::Fresh,
            degradation_reasons: Vec::new(),
            language: slot.language,
        })
        .0 == AnswerClass::Exact;

        // Ownership degradation is carried in the reasons regardless of class (the VALUE stays true;
        // only the ownership claim is degraded). For TS the reachable non-owned basis is
        // `ScipSynthesized` (fallback identity).
        let mut reasons: Vec<DegradationReason> = if owned {
            Vec::new()
        } else {
            vec![DegradationReason::ScipFallbackIdentity]
        };
        // PRODUCER-ABSENT-1: include the defining partition's partition-level degradation reasons (e.g.
        // `ProducerUnavailable` when warm-loaded producer-absent). The Stale branch below carries them.
        merge_partition_reasons(&mut reasons, self.partition_reasons(&def_part).into_iter());

        // Precedence: FRESHNESS DOMINATES the answer class (consistent with callers/callees + the
        // epoch contract). Stale/RefreshFailed last-good → `Stale`; ownership degradation rides in
        // `degradation_reasons`, never by downgrading a stale answer to `Partial`.
        match freshness {
            // Fresh: ownership decides — owned → Exact; degraded → Partial.
            FreshnessState::Fresh => {
                if owned {
                    AnswerEnvelope::exact(
                        data,
                        QueryCompleteness::Complete,
                        FreshnessState::Fresh,
                        Vec::new(),
                        languages,
                    )
                    .expect("exact invariant holds")
                } else {
                    AnswerEnvelope::partial(
                        Some(data),
                        reasons,
                        Vec::new(),
                        FreshnessState::Fresh,
                        Vec::new(),
                        languages,
                    )
                    .expect("partial with reason is valid")
                }
            }
            // PrecisionPending: an AST-owned fact stays Exact via NotScipDependent (the invariant-6
            // path callers/callees avoid); a degraded or SCIP-backed owned fact → Partial + PP.
            FreshnessState::PrecisionPending => match (owned, NotScipDependent::prove(&[basis])) {
                (true, Some(proof)) => AnswerEnvelope::exact_precision_pending(
                    data,
                    QueryCompleteness::Complete,
                    proof,
                    Vec::new(),
                    languages,
                )
                .expect("exact_precision_pending invariant holds"),
                _ => AnswerEnvelope::partial(
                    Some(data),
                    reasons,
                    Vec::new(),
                    FreshnessState::PrecisionPending,
                    Vec::new(),
                    languages,
                )
                .expect("partial precision-pending is valid"),
            },
            // Stale / RefreshFailed (incl. epoch mismatch): freshness dominates → Stale, last-good
            // served, ownership degradation carried in the reasons.
            _ => AnswerEnvelope::stale(data, freshness, reasons, Vec::new(), Vec::new(), languages)
                .expect("stale invariant holds"),
        }
    }

    /// `path(from, to)` — shortest path over the STRICT call graph (PATH-CYCLES-LIVEGRAPH-1). BFS over
    /// RESIDENT partitions' `SyntaxConfirmedCall` edges (the acceptable basis for path semantics; the
    /// xref summary lacks outgoing adjacency, so traversal needs the resident IR).
    ///
    /// Trust (D3, corrected): a FOUND path is `Exact` ONLY if every partition on the path is resident +
    /// Fresh (a found path is all-`SyntaxConfirmedCall` by construction, so the basis condition holds);
    /// a stale path partition → `Stale` (freshness dominates). A NO-PATH result is `Exact` ONLY if the
    /// reachable region was proven complete — every expanded node was defined in a resident + Fresh
    /// partition. If traversal reached a node it could not fully expand (its defining partition is
    /// non-resident / stale / unknown) → `Partial`, NEVER a confident exact-empty "no path".
    pub fn path(&self, from: &str, to: &str) -> AnswerEnvelope<PathAnswer> {
        use std::collections::VecDeque;

        // Precompute node -> defining partition (from `defines`, resident or not).
        let mut owner: HashMap<&str, &str> = HashMap::new();
        for (pid, s) in &self.slots {
            for k in s.defines.keys() {
                owner.insert(k.as_str(), pid.as_str());
            }
        }

        // Strict call-graph adjacency from RESIDENT partitions: src -> [(dst, partition)] over
        // `SyntaxConfirmedCall` edges only. Also collect the set of KNOWN node keys.
        let mut adj: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut known: BTreeSet<String> = owner.keys().map(|k| k.to_string()).collect();
        for (pid, s) in &self.slots {
            if let Some(ir) = &s.ir {
                for e in &ir.edges {
                    if e.basis == EdgeBasis::SyntaxConfirmedCall {
                        let src = e.src.as_str().to_string();
                        let dst = e.dst.as_str().to_string();
                        known.insert(src.clone());
                        known.insert(dst.clone());
                        adj.entry(src).or_default().push((dst, pid.clone()));
                    }
                }
            }
        }

        // Unknown source or target → Unavailable (null ≠ empty).
        if !known.contains(from) || !known.contains(to) {
            return AnswerEnvelope::unavailable(
                DegradationReason::UnresolvedAlias,
                FreshnessState::Unavailable,
                BTreeSet::new(),
            );
        }

        // A node is FULLY EXPANDABLE iff it is defined in a resident + Fresh partition (its outgoing
        // call edges are all loaded). Otherwise expansion is incomplete.
        let fully_expandable = |node: &str| -> bool {
            match owner.get(node) {
                Some(pid) => self
                    .slots
                    .get(*pid)
                    .map(|s| s.ir.is_some() && status_freshness(s.status) == FreshnessState::Fresh)
                    .unwrap_or(false),
                None => false,
            }
        };

        // BFS.
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut parent: HashMap<String, (String, String)> = HashMap::new(); // node -> (prev, edge partition)
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut degraded = false; // reached a node we could not fully expand
        visited.insert(from.to_string());
        queue.push_back(from.to_string());
        let mut found = false;
        while let Some(n) = queue.pop_front() {
            if n == to {
                found = true;
                break;
            }
            if !fully_expandable(&n) {
                degraded = true;
            }
            if let Some(neighbors) = adj.get(&n) {
                for (dst, pid) in neighbors {
                    if visited.insert(dst.clone()) {
                        parent.insert(dst.clone(), (n.clone(), pid.clone()));
                        queue.push_back(dst.clone());
                    }
                }
            }
        }

        // Freshness/languages/epochs over a set of partition ids.
        let collect_parts = |parts: &BTreeSet<String>| {
            let mut epochs = BTreeMap::new();
            let mut languages = BTreeSet::new();
            let mut worst = FreshnessState::Fresh;
            for pid in parts {
                if let Some(s) = self.slots.get(pid) {
                    epochs.insert(pid.clone(), s.epoch.0);
                    languages.insert(s.language);
                    let f = status_freshness(s.status);
                    if freshness_rank(f) > freshness_rank(worst) {
                        worst = f;
                    }
                }
            }
            if languages.is_empty() {
                languages.insert(LanguageSupport::TypeScriptPrimary);
            }
            (epochs, languages, worst)
        };

        if found {
            // Reconstruct the path (nodes + edges) and the partitions it touches.
            let mut nodes_rev = vec![to.to_string()];
            let mut edges_rev: Vec<(String, String)> = Vec::new();
            let mut parts: BTreeSet<String> = BTreeSet::new();
            if let Some(p) = owner.get(to) {
                parts.insert(p.to_string());
            }
            let mut cur = to.to_string();
            while let Some((prev, pid)) = parent.get(&cur).cloned() {
                edges_rev.push((prev.clone(), cur.clone()));
                parts.insert(pid.clone());
                if let Some(p) = owner.get(prev.as_str()) {
                    parts.insert(p.to_string());
                }
                nodes_rev.push(prev.clone());
                cur = prev;
                if cur == from {
                    break;
                }
            }
            nodes_rev.reverse();
            edges_rev.reverse();
            let (epochs, languages, worst) = collect_parts(&parts);
            let data = PathAnswer {
                nodes: nodes_rev,
                edges: edges_rev,
                contributing_epochs: epochs,
            };
            // Edges are all SyntaxConfirmedCall by construction → basis condition holds.
            return if worst == FreshnessState::Fresh {
                AnswerEnvelope::exact(
                    data,
                    QueryCompleteness::Complete,
                    FreshnessState::Fresh,
                    Vec::new(),
                    languages,
                )
                .expect("path exact invariant holds")
            } else {
                // A path partition is non-Fresh → freshness dominates → Stale (path still served).
                AnswerEnvelope::stale(data, worst, Vec::new(), Vec::new(), Vec::new(), languages)
                    .expect("path stale invariant holds")
            };
        }

        // No path. Languages from the source's partition (+ any).
        let mut languages = BTreeSet::new();
        if let Some(pid) = owner.get(from) {
            if let Some(s) = self.slots.get(*pid) {
                languages.insert(s.language);
            }
        }
        if languages.is_empty() {
            languages.insert(LanguageSupport::TypeScriptPrimary);
        }
        let empty = PathAnswer {
            nodes: Vec::new(),
            edges: Vec::new(),
            contributing_epochs: BTreeMap::new(),
        };
        if degraded {
            // Incomplete traversal → NEVER a confident exact-empty "no path"; Partial.
            AnswerEnvelope::partial(
                Some(empty),
                vec![DegradationReason::UnresolvedAlias],
                Vec::new(),
                FreshnessState::Fresh,
                Vec::new(),
                languages,
            )
            .expect("path partial invariant holds")
        } else {
            // The reachable region was proven complete (resident + Fresh) → Exact no-path.
            AnswerEnvelope::exact(
                empty,
                QueryCompleteness::Complete,
                FreshnessState::Fresh,
                Vec::new(),
                languages,
            )
            .expect("path exact no-path invariant holds")
        }
    }

    /// Read-only DISPLAY lookup (PATH-LIVEGRAPH-DEFAULT-1): the `SourceRange` of `key` from the resident
    /// IR, or `None` if no resident partition defines `key` or the node carries no range. This is purely
    /// presentation metadata for rendering a path's `file:line` — it does NOT participate in `path()`
    /// traversal, completeness, or trust semantics (a missing range never changes an answer's class). The
    /// daemon's default (`Auto`) path serve GATES on this being present for every rendered node so the
    /// human default never renders `:0`; explicit `--engine livegraph` may still serve without it.
    pub fn node_location(&self, key: &CanonicalKey) -> Option<SourceRange> {
        self.slots
            .values()
            .filter_map(|s| s.ir.as_ref())
            .find_map(|ir| ir.node(key))
            .and_then(|n| n.range.clone())
    }

    /// Rebuild the cross-partition import overlay (IMPORTS-XPART-WIRING-1 D3). Pure + in-memory: build the
    /// repo-relative FILE inventory from ALL resident slots' FILE-scope node keys, turn each resident
    /// `StaticUnresolved` import observation into an `ImportCandidate` (its importing FILE key looked up
    /// from that SAME inventory via `source_file`), run the resolver, and store the resolved FILE -> FILE
    /// edges. Cheap (hashmap lookups, no I/O); called on every load/swap/unload so the overlay is always
    /// coherent with the resident set.
    ///
    /// Only `StaticUnresolved` observations are candidates: `StaticResolved` imports are already
    /// node-resolved intra-partition `AstImport` edges (in `ir.edges`); `PackageExternal` /
    /// `DynamicUnsupported` are out of the resolver's scope. An observation whose importing file is not
    /// resident is skipped (`file_key_for` -> `None`): no src FILE node to anchor the edge.
    fn rebuild_xpart_overlay(&mut self) {
        let inventory = FileInventory::from_file_keys(
            self.slots
                .values()
                .filter_map(|s| s.ir.as_ref())
                .flat_map(|ir| {
                    ir.nodes
                        .iter()
                        .filter(|n| n.identity_source == IdentitySource::AstFileScope)
                        .map(|n| n.key.as_str().to_string())
                }),
        );
        let mut candidates: Vec<ImportCandidate> = Vec::new();
        for s in self.slots.values() {
            if let Some(ir) = &s.ir {
                for o in &ir.import_observations {
                    if o.resolution != ImportResolution::StaticUnresolved {
                        continue;
                    }
                    if let Some(src_key) = inventory.file_key_for(&o.source_file) {
                        candidates.push(ImportCandidate {
                            source_file_key: src_key.to_string(),
                            raw_specifier: o.raw_specifier.clone(),
                        });
                    }
                }
            }
        }
        self.xpart_overlay = resolve_imports(&inventory, candidates).resolved;
    }

    /// The current cross-partition import overlay edges (IMPORTS-XPART-WIRING-1): resolved FILE -> FILE
    /// edges (basis `AstImportFileInventoryResolved`) over the resident set. Read-only runtime artifact,
    /// never persisted. Empty when no cross-partition import resolves over the resident inventory (e.g. a
    /// single loaded partition).
    pub fn xpart_import_edges(&self) -> &[ResolvedImportEdgeCandidate] {
        &self.xpart_overlay
    }

    /// FILE import-cycle detection (CYCLES-LIVEGRAPH-1 + IMPORTS-XPART-WIRING-1): Tarjan SCC over the
    /// RESIDENT FILE import graph — intra-partition `EdgeBasis::AstImport` edges UNION the in-memory
    /// cross-partition overlay (`AstImportFileInventoryResolved`); no Calls/References/Module edges.
    /// Honestly scoped via the [`ImportCycleScope`] flag set (D5): resolved-relative, node-resolved FILE
    /// imports only — NOT all TS imports. A FOUND cycle is REAL within that captured graph (positive
    /// evidence) and MAY now span partitions via the overlay. A NO-CYCLE result is `Exact` ONLY when
    /// EVERY partition is resident + Fresh + TS-primary (whole-graph completeness — a non-resident
    /// partition's outgoing import adjacency, and any overlay edge touching it, is dropped on unload, so
    /// it could hide a cycle). Non-resident / non-TS partitions → `Partial` (listed as missing); a stale
    /// partition → `Stale`. This is a DIFFERENT question from `rmap cycles` (MODULE-import), never
    /// comparable to it.
    pub fn file_import_cycles(&self) -> AnswerEnvelope<FileImportCyclesAnswer> {
        use repo_graph_algorithms::{find_sccs, DirectedEdge};
        let (file_edges, scope) = self.file_import_edges();
        let edges: Vec<DirectedEdge> = file_edges
            .iter()
            .map(|(s, d)| DirectedEdge {
                source: s.clone(),
                target: d.clone(),
            })
            .collect();
        let (missing, worst, languages, epochs) = self.whole_graph_completeness();
        // SCC → cycles (find_sccs pre-filters to size > 1).
        let cycles: Vec<FileImportCycle> = find_sccs(&edges)
            .cycles
            .iter()
            .map(|c| FileImportCycle {
                members: c.members.clone(),
            })
            .collect();
        let data = FileImportCyclesAnswer {
            cycles,
            scope,
            contributing_epochs: epochs,
        };
        capture_envelope(data, missing, worst, languages)
    }

    /// The captured FILE import edge universe (`(src_key, dst_key)` pairs) + the D5 scope flag set, shared
    /// by `file_import_cycles` and `module_import_cycles` (MODULE-AGGREGATION-1): RESIDENT partitions'
    /// intra-partition `AstImport` edges UNION the in-memory cross-partition overlay
    /// (`AstImportFileInventoryResolved`). Both are resolved-relative FILE -> FILE imports.
    fn file_import_edges(&self) -> (Vec<(String, String)>, ImportCycleScope) {
        let mut edges: Vec<(String, String)> = Vec::new();
        let mut intra_count: usize = 0;
        for s in self.slots.values() {
            if let Some(ir) = &s.ir {
                for e in &ir.edges {
                    if e.basis == EdgeBasis::AstImport {
                        edges.push((e.src.as_str().to_string(), e.dst.as_str().to_string()));
                        intra_count += 1;
                    }
                }
            }
        }
        let xpart_edge_count = self.xpart_overlay.len();
        for e in &self.xpart_overlay {
            edges.push((e.src_file_key.clone(), e.dst_file_key.clone()));
        }
        // D5 (scope flag set): the graph UNIVERSE, NOT completeness. `cross_partition` reflects ACTUAL
        // contribution (false = no cross-partition edge in the universe, not that resolution was skipped).
        let scope = ImportCycleScope {
            captured_resolved_relative: true,
            intra_partition: intra_count > 0,
            cross_partition: xpart_edge_count > 0,
            xpart_edge_count,
        };
        (edges, scope)
    }

    /// Whole-graph completeness fold shared by `file_import_cycles` + `module_import_cycles` (D4): EVERY
    /// partition must be resident + TS-primary to be IN the captured scope; a partition failing either is
    /// `missing` (its imports are not analyzable). Returns (missing sorted, worst freshness, contributing
    /// languages, per-partition epochs).
    fn whole_graph_completeness(
        &self,
    ) -> (
        Vec<String>,
        FreshnessState,
        BTreeSet<LanguageSupport>,
        BTreeMap<String, u64>,
    ) {
        let mut missing: Vec<String> = Vec::new();
        let mut worst = FreshnessState::Fresh;
        let mut languages: BTreeSet<LanguageSupport> = BTreeSet::new();
        let mut epochs: BTreeMap<String, u64> = BTreeMap::new();
        for (pid, s) in &self.slots {
            epochs.insert(pid.clone(), s.epoch.0);
            languages.insert(s.language);
            let resident = s.ir.is_some();
            let ts = matches!(s.language, LanguageSupport::TypeScriptPrimary);
            if !resident || !ts {
                missing.push(pid.clone());
            }
            let f = status_freshness(s.status);
            if freshness_rank(f) > freshness_rank(worst) {
                worst = f;
            }
        }
        if languages.is_empty() {
            languages.insert(LanguageSupport::TypeScriptPrimary);
        }
        missing.sort();
        (missing, worst, languages, epochs)
    }

    /// MODULE import-cycle detection (MODULE-AGGREGATION-1): the SAME captured FILE import graph as
    /// `file_import_cycles`, AGGREGATED to MODULE granularity. `module(file) = dirname(repo-relative path)`
    /// (matching the SQLite `rmap cycles` identity; reuses the resolver's proven key parser); intra-module
    /// (self) edges are SKIPPED and module edges DEDUPED before Tarjan. The answer INHERITS
    /// file_import_cycles' completeness (D4) and is honestly scoped (`module_aggregated`; the captured FILE
    /// graph it aggregated; NEVER "all module cycles" — the FILE-graph completeness caveat propagates up).
    pub fn module_import_cycles(&self) -> AnswerEnvelope<ModuleImportCyclesAnswer> {
        use repo_graph_algorithms::{find_sccs, DirectedEdge};
        let (file_edges, file_scope) = self.file_import_edges();
        // Aggregate FILE edges -> MODULE edges: dirname identity, SKIP self-module, DEDUP (BTreeSet).
        let mut module_pairs: BTreeSet<(String, String)> = BTreeSet::new();
        for (src_key, dst_key) in &file_edges {
            if let (Some(sm), Some(dm)) = (module_path_of(src_key), module_path_of(dst_key)) {
                if sm != dm {
                    module_pairs.insert((sm, dm));
                }
            }
        }
        let edges: Vec<DirectedEdge> = module_pairs
            .iter()
            .map(|(s, d)| DirectedEdge {
                source: s.clone(),
                target: d.clone(),
            })
            .collect();
        let (missing, worst, languages, epochs) = self.whole_graph_completeness();
        let cycles: Vec<ModuleImportCycle> = find_sccs(&edges)
            .cycles
            .iter()
            .map(|c| ModuleImportCycle {
                members: c.members.clone(),
            })
            .collect();
        let data = ModuleImportCyclesAnswer {
            cycles,
            scope: ModuleImportCycleScope {
                file_scope,
                module_aggregated: true,
            },
            contributing_epochs: epochs,
        };
        capture_envelope(data, missing, worst, languages)
    }

    /// The resident MODULE paths (MODULE-CYCLES-COMPARE-CLASSIFY-1 D5): `module_path_of` each resident
    /// FILE-scope node key. These are the dirname module identities the LiveGraph actually has — the
    /// classifier uses them to tell a non-resident cycle module from an identity divergence.
    pub fn resident_module_paths(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for s in self.slots.values() {
            if let Some(ir) = &s.ir {
                for n in &ir.nodes {
                    if n.identity_source == IdentitySource::AstFileScope {
                        if let Some(m) = module_path_of(n.key.as_str()) {
                            out.insert(m);
                        }
                    }
                }
            }
        }
        out
    }

    /// Resident import OBSERVATIONS grouped by their source MODULE (MODULE-CYCLES-COMPARE-CLASSIFY-1 D5):
    /// `dirname(source_file)` -> the [`ObservationView`]s of that module's files. The IR-free view the pure
    /// classifier consumes (it maps `ImportResolution` -> `ObsResolution` here). Observations at the repo
    /// root (no module) are skipped.
    pub fn import_observations_by_module(&self) -> BTreeMap<String, Vec<ObservationView>> {
        let mut out: BTreeMap<String, Vec<ObservationView>> = BTreeMap::new();
        for s in self.slots.values() {
            if let Some(ir) = &s.ir {
                for o in &ir.import_observations {
                    let module = dirname(&o.source_file);
                    if module.is_empty() {
                        continue;
                    }
                    out.entry(module.to_string())
                        .or_default()
                        .push(ObservationView {
                            raw_specifier: o.raw_specifier.clone(),
                            resolution: match o.resolution {
                                ImportResolution::StaticResolved => ObsResolution::StaticResolved,
                                ImportResolution::StaticUnresolved => {
                                    ObsResolution::StaticUnresolved
                                }
                                ImportResolution::PackageExternal => ObsResolution::PackageExternal,
                                ImportResolution::DynamicUnsupported => {
                                    ObsResolution::DynamicUnsupported
                                }
                            },
                            is_re_export: o.is_re_export,
                            is_type_only: o.is_type_only,
                        });
                }
            }
        }
        out
    }
}

/// The MODULE path of a FILE key (MODULE-AGGREGATION-1): `dirname(repo-relative path of the FILE key)`.
/// `None` if the key is not a FILE key OR the file is at the repo root (no module) — matching the SQLite
/// cycle path's `get_module_path`. REUSES the resolver's proven `file_key_path` (first-colon = repo
/// boundary; `repo_uid` has no colon) rather than new colon slicing (the ratified key-parse safety).
fn module_path_of(file_key: &str) -> Option<String> {
    let path = file_key_path(file_key)?;
    let dir = dirname(path);
    if dir.is_empty() {
        None
    } else {
        Some(dir.to_string())
    }
}

/// Finalize a CAPTURED-graph cycle answer with the shared completeness semantics (D4), generic over the
/// payload: all resident + Fresh + TS -> Exact WITHIN SCOPE; a `missing` partition -> Partial (found
/// cycles stay real); else (all resident, non-Fresh) -> Stale. Reused by file + module cycle answers.
fn capture_envelope<T>(
    data: T,
    missing: Vec<String>,
    worst: FreshnessState,
    languages: BTreeSet<LanguageSupport>,
) -> AnswerEnvelope<T> {
    if missing.is_empty() && worst == FreshnessState::Fresh {
        AnswerEnvelope::exact(
            data,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            Vec::new(),
            languages,
        )
        .expect("captured-graph exact invariant holds")
    } else if !missing.is_empty() {
        AnswerEnvelope::partial(
            Some(data),
            Vec::new(),
            missing,
            worst,
            Vec::new(),
            languages,
        )
        .expect("captured-graph partial invariant holds")
    } else {
        AnswerEnvelope::stale(data, worst, Vec::new(), Vec::new(), Vec::new(), languages)
            .expect("captured-graph stale invariant holds")
    }
}

/// A FILE import cycle (CYCLES-LIVEGRAPH-1): the file-scope node keys forming a strongly-connected
/// import ring (an SCC of size > 1 over the captured import graph).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImportCycle {
    /// File-scope node keys in the cycle (reverse finishing order, from Tarjan).
    pub members: Vec<String>,
}

/// The graph UNIVERSE a [`FileImportCyclesAnswer`] was computed over (CYCLES-LIVEGRAPH-1 D2;
/// IMPORTS-XPART-WIRING-1 D5 — a FLAG SET, not a single label). It is explicitly NOT "all TS imports":
/// only resolved-relative, node-resolved FILE imports are captured. This describes the SCC INPUT, NOT
/// completeness — the answer class + `missing` list carry whether that universe was fully resident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportCycleScope {
    /// Always true for this query family: cycles are over the CAPTURED resolved-relative FILE import
    /// graph. Non-relative/package, unresolved-even-cross-partition, dynamic, and re-export imports are
    /// NOT captured (IMPORTS-MODULE-INGEST-1) and the answer makes no claim about them.
    pub captured_resolved_relative: bool,
    /// True iff >= 1 RESIDENT partition-local FILE import edge (basis `AstImport`) entered the universe.
    pub intra_partition: bool,
    /// True iff >= 1 CROSS-partition resolved import edge (the in-memory overlay, basis
    /// `AstImportFileInventoryResolved`) entered the universe. FALSE does NOT mean cross-partition
    /// resolution was skipped — it means none was in the universe (a single loaded partition, or no
    /// import resolved across the resident inventory). Equals `xpart_edge_count > 0`.
    pub cross_partition: bool,
    /// Exact count of cross-partition overlay edges included (D5 contribution precision — the
    /// user-ratified tie-break against the participation-vs-contribution ambiguity).
    pub xpart_edge_count: usize,
}

/// Answer for [`LiveGraph::file_import_cycles`]: the detected cycles + the scope they cover + epochs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImportCyclesAnswer {
    /// Detected cycles (SCCs of size > 1) over the captured FILE import graph.
    pub cycles: Vec<FileImportCycle>,
    /// The scope this answer covers — never "all TS imports".
    pub scope: ImportCycleScope,
    /// Epoch per contributing partition (every slot, resident or not).
    pub contributing_epochs: BTreeMap<String, u64>,
}

/// A MODULE import cycle (MODULE-AGGREGATION-1): module identities (repo-relative directory paths) forming
/// a strongly-connected ring over the DIRECTORY-aggregated FILE import graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleImportCycle {
    /// Module identities (repo-relative directory paths) in the cycle (Tarjan reverse finishing order).
    pub members: Vec<String>,
}

/// The scope of a [`ModuleImportCyclesAnswer`] (MODULE-AGGREGATION-1 D4): the FILE-graph scope it was
/// aggregated FROM, plus the directory-aggregation marker. It is NEVER "all module cycles" — the FILE
/// graph's completeness caveat (package / path-alias / dynamic / re-export NOT captured) propagates up, so
/// these module cycles are a SUBSET of a complete module-cycle answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleImportCycleScope {
    /// The captured FILE import graph these module cycles were aggregated from.
    pub file_scope: ImportCycleScope,
    /// Always true: cycles are DIRECTORY-aggregated (`module = dirname(file)`). Records that this is a
    /// DERIVED module view of the captured FILE graph, not an independent/complete module graph.
    pub module_aggregated: bool,
}

/// Answer for [`LiveGraph::module_import_cycles`]: the derived MODULE cycles + the scope + epochs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleImportCyclesAnswer {
    /// Detected MODULE cycles (SCCs of size > 1) over the directory-aggregated FILE import graph.
    pub cycles: Vec<ModuleImportCycle>,
    /// The scope — the captured FILE graph + module aggregation; never "all module cycles".
    pub scope: ModuleImportCycleScope,
    /// Epoch per contributing partition (every slot, resident or not).
    pub contributing_epochs: BTreeMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_ir::{
        CanonicalKey, EdgeBasis, EdgeType, ImportEdgeMeta, ImportObservation, ImportResolution,
        IrEdge, IrNode, Partition, PartitionId, PartitionKind, Provenance,
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
    /// A node carrying a source range (for `node_location` display tests).
    fn node_at(key: &str, file: &str, line: u32) -> IrNode {
        IrNode {
            range: Some(SourceRange {
                file: file.into(),
                start_line: line,
                start_col: 0,
                end_line: line,
                end_col: 1,
            }),
            ..node(key, IdentitySource::AstAdopted)
        }
    }
    fn edge(src: &str, dst: &str) -> IrEdge {
        IrEdge {
            src: CanonicalKey::from_existing(src),
            dst: CanonicalKey::from_existing(dst),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
            import: None,
        }
    }
    fn ir(id: &str, nodes: Vec<IrNode>, edges: Vec<IrEdge>) -> PartitionIr {
        PartitionIr {
            partition: part(id),
            nodes,
            edges,
            import_observations: Vec::new(),
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

    // ── node_location (PATH-LIVEGRAPH-DEFAULT-1 display lookup) ────────

    #[test]
    fn node_location_returns_range_when_resident_with_range() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![node_at("p.foo", "src/foo.ts", 12)], vec![]),
            LanguageSupport::TypeScriptPrimary,
        );
        let r = lg.node_location(&CanonicalKey::from_existing("p.foo"));
        assert_eq!(r.as_ref().map(|r| r.file.as_str()), Some("src/foo.ts"));
        assert_eq!(r.map(|r| r.start_line), Some(12));
    }

    #[test]
    fn node_location_none_when_node_has_no_range() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![node("p.foo", IdentitySource::AstAdopted)], vec![]),
            LanguageSupport::TypeScriptPrimary,
        );
        assert!(lg
            .node_location(&CanonicalKey::from_existing("p.foo"))
            .is_none());
    }

    #[test]
    fn node_location_none_when_partition_nonresident() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![node_at("p.foo", "src/foo.ts", 12)], vec![]),
            LanguageSupport::TypeScriptPrimary,
        );
        lg.unload_partition("p"); // IR dropped; only the `defines` summary remains.
        assert!(lg
            .node_location(&CanonicalKey::from_existing("p.foo"))
            .is_none());
    }

    #[test]
    fn node_location_none_when_unknown_key() {
        let lg = both();
        assert!(lg
            .node_location(&CanonicalKey::from_existing("does.not.exist"))
            .is_none());
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

    // PRODUCER-ABSENT-1: a partition warm-loaded producer-absent is marked Stale +
    // `ProducerUnavailable`; queries touching it must surface that reason (D4).
    #[test]
    fn producer_unavailable_partition_reason_propagates_to_callers() {
        let mut lg = both();
        lg.mark_stale("engine");
        lg.add_partition_degradation("engine", DegradationReason::ProducerUnavailable);
        let a = lg.callers("engine.foo", Granularity::CallerDetail);
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(
            a.degradation_reasons()
                .contains(&DegradationReason::ProducerUnavailable),
            "callers must surface ProducerUnavailable: {:?}",
            a.degradation_reasons()
        );
    }

    #[test]
    fn producer_unavailable_partition_reason_propagates_to_callees() {
        let mut lg = both();
        lg.mark_stale("engine");
        lg.add_partition_degradation("engine", DegradationReason::ProducerUnavailable);
        let a = lg.callees("engine.bar", Granularity::CallerDetail);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(
            a.degradation_reasons()
                .contains(&DegradationReason::ProducerUnavailable),
            "callees must surface ProducerUnavailable: {:?}",
            a.degradation_reasons()
        );
    }

    #[test]
    fn producer_unavailable_partition_reason_propagates_to_value_facts() {
        let mut lg = LiveGraph::new();
        lg.load_partition("engine", engine(), LanguageSupport::TypeScriptPrimary);
        lg.load_value_facts(
            "engine",
            vec![ValueFact {
                subject: ValueSubject::Symbol(CanonicalKey::from_existing("engine.foo")),
                kind: ValueFactKind::CyclomaticComplexity,
                value: 3,
                basis: IdentityBasis::AstAdopted,
                source_range: None,
                provenance: prov(),
            }],
        );
        lg.mark_stale("engine");
        lg.add_partition_degradation("engine", DegradationReason::ProducerUnavailable);
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(
            a.degradation_reasons()
                .contains(&DegradationReason::ProducerUnavailable),
            "value_facts must surface ProducerUnavailable: {:?}",
            a.degradation_reasons()
        );
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

    // ── VALUE-JOIN-1: value facts (D1 complexity, D5 value_facts, D7 epoch coherence) ──

    fn complexity_fact(key: &str, value: u32, basis: IdentityBasis) -> ValueFact {
        ValueFact {
            subject: ValueSubject::Symbol(CanonicalKey::from_existing(key)),
            kind: ValueFactKind::CyclomaticComplexity,
            value,
            basis,
            source_range: None,
            provenance: prov(),
        }
    }
    // engine(TS) resident, defines engine.foo (AstAdopted).
    fn vf_lg() -> LiveGraph {
        let mut lg = LiveGraph::new();
        lg.load_partition("engine", engine(), LanguageSupport::TypeScriptPrimary);
        lg
    }

    #[test]
    fn symbol_owned_complexity_exact_for_ast_adopted_ts() {
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Exact);
        assert_eq!(a.freshness(), FreshnessState::Fresh);
        assert_eq!(a.data().unwrap().facts.len(), 1);
        assert_eq!(a.data().unwrap().facts[0].value, 7);
        assert_eq!(a.data().unwrap().facts[0].basis, IdentityBasis::AstAdopted);
    }

    #[test]
    fn raw_anchored_complexity_partial_not_exact_for_symbol_ownership() {
        let mut lg = vf_lg();
        // Keyed to the symbol, but the basis does NOT certify ownership (TS fallback identity).
        lg.load_value_facts(
            "engine",
            vec![complexity_fact(
                "engine.foo",
                9,
                IdentityBasis::ScipSynthesized,
            )],
        );
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Partial);
        assert_ne!(a.class(), AnswerClass::Exact);
        // KEY RULE: the value is NOT less true — only ownership is degraded.
        assert_eq!(a.data().unwrap().facts[0].value, 9);
        assert!(a
            .degradation_reasons()
            .contains(&DegradationReason::ScipFallbackIdentity));
    }

    #[test]
    fn missing_value_fact_unavailable_not_empty() {
        let lg = vf_lg(); // no value facts loaded
                          // Known symbol, no complexity fact → Unavailable (null ≠ empty).
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Unavailable);
        assert!(a.data().is_none());
        assert!(!a.degradation_reasons().is_empty());
        // Unknown symbol is likewise Unavailable.
        let u = lg.value_facts("nonexistent.symbol");
        assert_eq!(u.class(), AnswerClass::Unavailable);
        assert!(u.data().is_none());
    }

    #[test]
    fn value_fact_epoch_mismatch_stale_or_precision_pending() {
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        lg.swap_partition("engine", engine()); // bumps epoch; facts not reloaded → epoch mismatch
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(a.data().is_some()); // last-good served, never empty
    }

    #[test]
    fn partition_swap_without_value_reload_marks_value_facts_stale() {
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        assert_eq!(lg.value_facts("engine.foo").class(), AnswerClass::Exact);
        lg.swap_partition("engine", engine());
        let after = lg.value_facts("engine.foo");
        assert_ne!(after.class(), AnswerClass::Exact);
        assert_eq!(after.class(), AnswerClass::Stale);
        // Reloading for the new epoch restores Exact (D7).
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        assert_eq!(lg.value_facts("engine.foo").class(), AnswerClass::Exact);
    }

    #[test]
    fn nonresident_partition_value_facts_partial_or_unavailable() {
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        lg.unload_partition("engine"); // ir dropped; value facts retained
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Partial);
        assert_eq!(a.missing_partitions(), ["engine"]);
        assert!(a.data().is_some()); // last-good retained
    }

    #[test]
    fn contributing_languages_preserved() {
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        let a = lg.value_facts("engine.foo");
        assert_eq!(
            *a.contributing_languages(),
            BTreeSet::from([LanguageSupport::TypeScriptPrimary])
        );
    }

    #[test]
    fn ast_owned_value_fact_exact_under_precision_pending() {
        // A value fact on an AST-adopted identity is AST-LOCAL, so under PrecisionPending it stays
        // Exact via the NotScipDependent proof — the invariant-6 path callers/callees avoid.
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact("engine.foo", 7, IdentityBasis::AstAdopted)],
        );
        lg.begin_refresh("engine"); // PrecisionPending, epoch unchanged → no mismatch
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Exact);
        assert_eq!(a.freshness(), FreshnessState::PrecisionPending);
    }

    #[test]
    fn raw_anchored_stale_value_fact_returns_stale_with_reason() {
        // Precedence: freshness dominates. A non-owned (raw-anchored) fact on a STALE partition is
        // class Stale (not Partial); the ownership degradation rides in degradation_reasons.
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact(
                "engine.foo",
                9,
                IdentityBasis::ScipSynthesized,
            )],
        );
        lg.mark_stale("engine");
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        assert!(a
            .degradation_reasons()
            .contains(&DegradationReason::ScipFallbackIdentity));
        assert_eq!(a.data().unwrap().facts[0].value, 9); // value preserved
    }

    #[test]
    fn raw_anchored_refresh_failed_value_fact_returns_stale_with_reason() {
        let mut lg = vf_lg();
        lg.load_value_facts(
            "engine",
            vec![complexity_fact(
                "engine.foo",
                9,
                IdentityBasis::ScipSynthesized,
            )],
        );
        lg.mark_refresh_failed("engine");
        let a = lg.value_facts("engine.foo");
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::RefreshFailed);
        assert!(a
            .degradation_reasons()
            .contains(&DegradationReason::ScipFallbackIdentity));
    }

    // ── PATH-CYCLES-LIVEGRAPH-1: path() ──
    // Fixtures: both() = engine (defines engine.foo/engine.bar; edge engine.bar->engine.foo) + api
    // (defines api.caller; edge api.caller->engine.foo). All edges are SyntaxConfirmedCall.

    #[test]
    fn path_found_exact_when_all_resident_fresh() {
        let lg = both();
        let a = lg.path("api.caller", "engine.foo");
        assert_eq!(a.class(), AnswerClass::Exact);
        assert_eq!(a.freshness(), FreshnessState::Fresh);
        let d = a.data().expect("path data");
        assert_eq!(
            d.nodes,
            vec!["api.caller".to_string(), "engine.foo".to_string()]
        );
        assert_eq!(
            d.edges,
            vec![("api.caller".to_string(), "engine.foo".to_string())]
        );
    }

    #[test]
    fn path_found_stale_when_path_partition_stale() {
        let mut lg = both();
        lg.mark_stale("api"); // the partition that owns the path edge
        let a = lg.path("api.caller", "engine.foo");
        assert_eq!(a.class(), AnswerClass::Stale);
        assert_eq!(a.freshness(), FreshnessState::Stale);
        // the path is still served (last-good), not dropped
        assert!(!a.data().expect("stale path data").nodes.is_empty());
    }

    #[test]
    fn no_path_exact_when_reachable_region_complete() {
        // engine.bar is NOT reachable from api.caller (no edge into engine.bar), and the whole
        // reachable region (api.caller -> engine.foo) is resident + Fresh -> proven no-path.
        let lg = both();
        let a = lg.path("api.caller", "engine.bar");
        assert_eq!(a.class(), AnswerClass::Exact);
        assert!(a.data().expect("no-path data").nodes.is_empty());
    }

    #[test]
    fn no_path_partial_when_frontier_partition_nonresident() {
        // Unload engine: api.caller -> engine.foo is still resident, but engine.foo's defining
        // partition is non-resident, so the traversal cannot prove no-path -> Partial, not exact-empty.
        let mut lg = both();
        lg.unload_partition("engine");
        let a = lg.path("api.caller", "engine.bar");
        assert_eq!(a.class(), AnswerClass::Partial);
        assert_ne!(a.class(), AnswerClass::Exact);
    }

    #[test]
    fn unknown_source_or_target_unavailable() {
        let lg = both();
        assert_eq!(
            lg.path("does.not.exist", "engine.foo").class(),
            AnswerClass::Unavailable
        );
        assert_eq!(
            lg.path("api.caller", "does.not.exist").class(),
            AnswerClass::Unavailable
        );
    }

    // ── file_import_cycles (CYCLES-LIVEGRAPH-1) ───────────────────────

    fn import_edge(src: &str, dst: &str) -> IrEdge {
        IrEdge {
            src: CanonicalKey::from_existing(src),
            dst: CanonicalKey::from_existing(dst),
            edge_type: EdgeType::Imports,
            basis: EdgeBasis::AstImport,
            provenance: prov(),
            import: Some(ImportEdgeMeta {
                raw_specifier: "./x".into(),
                resolved_path: "x".into(),
                resolution: ImportResolution::StaticResolved,
            }),
        }
    }
    fn ref_edge(src: &str, dst: &str) -> IrEdge {
        IrEdge {
            src: CanonicalKey::from_existing(src),
            dst: CanonicalKey::from_existing(dst),
            edge_type: EdgeType::References,
            basis: EdgeBasis::DerivedReference,
            provenance: prov(),
            import: None,
        }
    }

    #[test]
    fn file_import_no_cycle_complete_is_exact_empty_scoped() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![], vec![import_edge("a", "b")]),
            LanguageSupport::TypeScriptPrimary,
        );
        let a = lg.file_import_cycles();
        assert_eq!(a.class(), AnswerClass::Exact);
        assert_eq!(a.freshness(), FreshnessState::Fresh);
        let d = a.data().expect("data");
        assert!(d.cycles.is_empty(), "no cycle over a->b");
        assert_eq!(
            d.scope,
            ImportCycleScope {
                captured_resolved_relative: true,
                intra_partition: true,
                cross_partition: false,
                xpart_edge_count: 0,
            },
            "single partition with one intra import edge -> intra true, cross false"
        );
    }

    #[test]
    fn file_import_cycle_ab_is_exact_with_cycle() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir(
                "p",
                vec![],
                vec![import_edge("a", "b"), import_edge("b", "a")],
            ),
            LanguageSupport::TypeScriptPrimary,
        );
        let a = lg.file_import_cycles();
        assert_eq!(a.class(), AnswerClass::Exact);
        let d = a.data().expect("data");
        assert_eq!(d.cycles.len(), 1, "one a<->b import cycle");
        let mut members = d.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn file_import_nonresident_partition_is_partial_not_exact() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p1",
            ir("p1", vec![], vec![import_edge("a", "b")]),
            LanguageSupport::TypeScriptPrimary,
        );
        lg.load_partition(
            "p2",
            ir("p2", vec![], vec![import_edge("c", "d")]),
            LanguageSupport::TypeScriptPrimary,
        );
        lg.unload_partition("p2"); // non-resident -> its import adjacency is gone, scope incomplete
        let a = lg.file_import_cycles();
        assert_eq!(
            a.class(),
            AnswerClass::Partial,
            "a non-resident partition can never yield an Exact no-cycle"
        );
        assert_ne!(a.class(), AnswerClass::Exact);
    }

    #[test]
    fn file_import_stale_partition_is_stale_not_exact() {
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![], vec![import_edge("a", "b")]),
            LanguageSupport::TypeScriptPrimary,
        );
        lg.mark_stale("p");
        let a = lg.file_import_cycles();
        assert_eq!(
            a.class(),
            AnswerClass::Stale,
            "stale partition -> not Exact"
        );
        assert_eq!(a.freshness(), FreshnessState::Stale);
    }

    #[test]
    fn file_import_ignores_calls_cycle() {
        // A CALLS cycle (a<->b) must NOT register as an import cycle.
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![], vec![edge("a", "b"), edge("b", "a")]),
            LanguageSupport::TypeScriptPrimary,
        );
        let a = lg.file_import_cycles();
        assert_eq!(a.class(), AnswerClass::Exact);
        assert!(
            a.data().expect("data").cycles.is_empty(),
            "CALLS edges are not import edges"
        );
    }

    #[test]
    fn file_import_ignores_references_cycle() {
        // A REFERENCES cycle (a<->b) must NOT register as an import cycle.
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir("p", vec![], vec![ref_edge("a", "b"), ref_edge("b", "a")]),
            LanguageSupport::TypeScriptPrimary,
        );
        let a = lg.file_import_cycles();
        assert_eq!(a.class(), AnswerClass::Exact);
        assert!(
            a.data().expect("data").cycles.is_empty(),
            "REFERENCES edges are not import edges"
        );
    }

    #[test]
    fn file_import_unloaded_partition_loses_adjacency_and_degrades() {
        // A cross-partition import cycle: p1 has a->b, p2 has b->a. Both resident -> cycle is Exact.
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p1",
            ir("p1", vec![], vec![import_edge("a", "b")]),
            LanguageSupport::TypeScriptPrimary,
        );
        lg.load_partition(
            "p2",
            ir("p2", vec![], vec![import_edge("b", "a")]),
            LanguageSupport::TypeScriptPrimary,
        );
        let resident = lg.file_import_cycles();
        assert_eq!(resident.class(), AnswerClass::Exact);
        assert_eq!(
            resident.data().expect("data").cycles.len(),
            1,
            "a<->b found"
        );

        // Unload p2 -> its outgoing import edge (b->a) is gone (adjacency lost): no cycle visible AND
        // completeness degrades to Partial (NOT a confident Exact no-cycle).
        lg.unload_partition("p2");
        let degraded = lg.file_import_cycles();
        assert_eq!(degraded.class(), AnswerClass::Partial);
        assert!(
            degraded.data().expect("data").cycles.is_empty(),
            "the b->a edge is gone with p2's IR"
        );
    }

    // ── cross-partition import OVERLAY (IMPORTS-XPART-WIRING-1) ─────────
    //
    // The cases above hand-build a cross-partition `AstImport` EDGE in each partition. These cover the
    // real wiring: a `StaticUnresolved` import OBSERVATION (a relative import whose target is NOT a FILE
    // node in its own partition) is upgraded into a node-resolved FILE -> FILE overlay edge ONLY once the
    // target partition is resident — purely in memory, never in any `PartitionIr`.

    /// A FILE-scope node (its key is the repo-relative FILE key the resolver inventory indexes).
    fn file_node(key: &str) -> IrNode {
        node(key, IdentitySource::AstFileScope)
    }
    /// A locally-unresolved relative import observation (the overlay's candidate class).
    fn unresolved_obs(source_file: &str, raw_specifier: &str) -> ImportObservation {
        ImportObservation {
            source_file: source_file.to_string(),
            raw_specifier: raw_specifier.to_string(),
            resolution: ImportResolution::StaticUnresolved,
            is_re_export: false,
            is_type_only: false,
            is_side_effect: false,
        }
    }
    /// A partition IR carrying import observations (the `ir` helper above always has none).
    fn ir_obs(
        id: &str,
        nodes: Vec<IrNode>,
        edges: Vec<IrEdge>,
        obs: Vec<ImportObservation>,
    ) -> PartitionIr {
        PartitionIr {
            partition: part(id),
            nodes,
            edges,
            import_observations: obs,
        }
    }
    /// pkg-a: `packages/a/src/main.ts` imports `../../b/src/foo` (unresolved within a).
    fn pkg_a_imports_b() -> PartitionIr {
        ir_obs(
            "a",
            vec![file_node("repo:packages/a/src/main.ts:FILE")],
            vec![],
            vec![unresolved_obs("packages/a/src/main.ts", "../../b/src/foo")],
        )
    }
    /// pkg-b: `packages/b/src/foo.ts` imports `../../a/src/main` (unresolved within b) -> mutual.
    fn pkg_b_imports_a() -> PartitionIr {
        ir_obs(
            "b",
            vec![file_node("repo:packages/b/src/foo.ts:FILE")],
            vec![],
            vec![unresolved_obs("packages/b/src/foo.ts", "../../a/src/main")],
        )
    }
    /// pkg-b with NO imports (just the target FILE node).
    fn pkg_b_leaf() -> PartitionIr {
        ir_obs(
            "b",
            vec![file_node("repo:packages/b/src/foo.ts:FILE")],
            vec![],
            vec![],
        )
    }

    #[test]
    fn xpart_overlay_resolves_cross_partition_edge_only_when_target_resident() {
        // Acceptance #2 + eager rebuild (D3): the edge appears only once the target partition loads.
        let mut lg = LiveGraph::new();
        lg.load_partition("a", pkg_a_imports_b(), LanguageSupport::TypeScriptPrimary);
        assert!(
            lg.xpart_import_edges().is_empty(),
            "target partition b not resident yet -> nothing resolves"
        );

        lg.load_partition("b", pkg_b_leaf(), LanguageSupport::TypeScriptPrimary);
        let edges = lg.xpart_import_edges();
        assert_eq!(
            edges.len(),
            1,
            "a/main -> b/foo resolves once b is resident"
        );
        assert_eq!(edges[0].src_file_key, "repo:packages/a/src/main.ts:FILE");
        assert_eq!(edges[0].dst_file_key, "repo:packages/b/src/foo.ts:FILE");
        assert_eq!(
            edges[0].basis,
            EdgeBasis::AstImportFileInventoryResolved,
            "overlay edges carry the resolved basis (D4)"
        );
    }

    #[test]
    fn xpart_overlay_forms_cross_partition_file_import_cycle() {
        // Acceptance #3: mutual cross-partition imports -> a file-import CYCLE via the overlay. The cycle
        // is built ENTIRELY from the runtime overlay (both partitions have empty `ir.edges`), so
        // `intra_partition == false` is the evidence the cycle was NEVER a persisted edge (acceptance #5:
        // overlay is runtime-only; `PartitionIr` has no overlay field to serialize).
        let mut lg = LiveGraph::new();
        lg.load_partition("a", pkg_a_imports_b(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("b", pkg_b_imports_a(), LanguageSupport::TypeScriptPrimary);

        let a = lg.file_import_cycles();
        assert_eq!(a.class(), AnswerClass::Exact, "both resident + Fresh + TS");
        let d = a.data().expect("data");
        assert_eq!(
            d.cycles.len(),
            1,
            "a/main <-> b/foo is one cross-partition cycle"
        );
        let members: BTreeSet<&str> = d.cycles[0].members.iter().map(String::as_str).collect();
        assert!(members.contains("repo:packages/a/src/main.ts:FILE"));
        assert!(members.contains("repo:packages/b/src/foo.ts:FILE"));
        // D5 scope flag set: cross-partition contributed (2 overlay edges); no intra AstImport edge.
        assert_eq!(
            d.scope,
            ImportCycleScope {
                captured_resolved_relative: true,
                intra_partition: false,
                cross_partition: true,
                xpart_edge_count: 2,
            }
        );
    }

    #[test]
    fn unload_rebuilds_overlay_without_the_edge_and_degrades() {
        // Acceptance #4: unloading a cycle partition drops the overlay edge (rebuild D3) AND degrades the
        // answer to Partial (whole-graph completeness) — never a stale edge, never a confident no-cycle.
        let mut lg = LiveGraph::new();
        lg.load_partition("a", pkg_a_imports_b(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("b", pkg_b_imports_a(), LanguageSupport::TypeScriptPrimary);
        assert_eq!(
            lg.xpart_import_edges().len(),
            2,
            "mutual overlay edges present"
        );

        lg.unload_partition("b");
        assert!(
            lg.xpart_import_edges().is_empty(),
            "b non-resident -> a/main's import cannot resolve and b's observation is gone"
        );
        let degraded = lg.file_import_cycles();
        assert_eq!(
            degraded.class(),
            AnswerClass::Partial,
            "b non-resident -> incomplete"
        );
        assert!(
            degraded.missing_partitions().contains(&"b".to_string()),
            "b listed missing: {:?}",
            degraded.missing_partitions()
        );
        let d = degraded
            .data()
            .expect("partial still carries last-good data");
        assert!(d.cycles.is_empty(), "the cross-partition cycle is gone");
        assert!(!d.scope.cross_partition, "no overlay edge after unload");
        assert_eq!(d.scope.xpart_edge_count, 0);
    }

    // ── module_import_cycles (MODULE-AGGREGATION-1) ────────────────────
    //
    // module(file) = dirname(repo-relative path of the FILE key). FILE keys here are
    // `repo:{dir}/{file}.ts:FILE` so module aggregation has real directory identities.

    #[test]
    fn module_cycle_from_cross_dir_file_imports() {
        // Files in DIFFERENT dirs importing each other -> a MODULE cycle (dirA <-> dirB).
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir(
                "p",
                vec![
                    file_node("repo:dirA/a.ts:FILE"),
                    file_node("repo:dirB/b.ts:FILE"),
                ],
                vec![
                    import_edge("repo:dirA/a.ts:FILE", "repo:dirB/b.ts:FILE"),
                    import_edge("repo:dirB/b.ts:FILE", "repo:dirA/a.ts:FILE"),
                ],
            ),
            LanguageSupport::TypeScriptPrimary,
        );
        let a = lg.module_import_cycles();
        assert_eq!(a.class(), AnswerClass::Exact);
        let d = a.data().expect("data");
        assert_eq!(d.cycles.len(), 1, "dirA <-> dirB module cycle");
        let members: BTreeSet<&str> = d.cycles[0].members.iter().map(String::as_str).collect();
        assert!(
            members.contains("dirA") && members.contains("dirB"),
            "{members:?}"
        );
        assert!(d.scope.module_aggregated);
        assert!(d.scope.file_scope.captured_resolved_relative);
    }

    #[test]
    fn module_self_edge_same_dir_is_skipped() {
        // Two files in the SAME dir importing each other -> NO module cycle (self-module skipped), even
        // though they DO form a FILE cycle. This is the key file-vs-module distinction.
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir(
                "p",
                vec![
                    file_node("repo:dir/a.ts:FILE"),
                    file_node("repo:dir/b.ts:FILE"),
                ],
                vec![
                    import_edge("repo:dir/a.ts:FILE", "repo:dir/b.ts:FILE"),
                    import_edge("repo:dir/b.ts:FILE", "repo:dir/a.ts:FILE"),
                ],
            ),
            LanguageSupport::TypeScriptPrimary,
        );
        assert!(
            lg.module_import_cycles()
                .data()
                .expect("data")
                .cycles
                .is_empty(),
            "same-dir imports aggregate to a self-module edge -> skipped"
        );
        assert_eq!(
            lg.file_import_cycles().data().expect("data").cycles.len(),
            1,
            "the FILES still cycle"
        );
    }

    #[test]
    fn module_edges_dedup_duplicate_file_imports() {
        // Two distinct files in dirA both import dirB -> ONE module edge (dedup); reciprocated -> a single
        // 2-module cycle (members exactly {dirA, dirB}, no duplicate module node).
        let mut lg = LiveGraph::new();
        lg.load_partition(
            "p",
            ir(
                "p",
                vec![
                    file_node("repo:dirA/a1.ts:FILE"),
                    file_node("repo:dirA/a2.ts:FILE"),
                    file_node("repo:dirB/b.ts:FILE"),
                ],
                vec![
                    import_edge("repo:dirA/a1.ts:FILE", "repo:dirB/b.ts:FILE"),
                    import_edge("repo:dirA/a2.ts:FILE", "repo:dirB/b.ts:FILE"), // duplicate dirA->dirB
                    import_edge("repo:dirB/b.ts:FILE", "repo:dirA/a1.ts:FILE"),
                ],
            ),
            LanguageSupport::TypeScriptPrimary,
        );
        let d = lg.module_import_cycles().data().expect("data").clone();
        assert_eq!(d.cycles.len(), 1);
        assert_eq!(
            d.cycles[0].members.len(),
            2,
            "exactly dirA + dirB; the duplicate file import deduped to one module edge"
        );
    }

    #[test]
    fn module_cycle_uses_xpart_overlay() {
        // FIXTURE-equivalent: the cross-partition OVERLAY edges feed module aggregation. pkg-a/src/main.ts
        // <-> pkg-b/src/foo.ts aggregates to packages/a/src <-> packages/b/src -- the SAME modules SQLite
        // `rmap cycles` reports on the xpart-monorepo fixture.
        let mut lg = LiveGraph::new();
        lg.load_partition("a", pkg_a_imports_b(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("b", pkg_b_imports_a(), LanguageSupport::TypeScriptPrimary);
        let d = lg.module_import_cycles().data().expect("data").clone();
        assert_eq!(
            d.cycles.len(),
            1,
            "cross-partition module cycle via the overlay"
        );
        let m: BTreeSet<&str> = d.cycles[0].members.iter().map(String::as_str).collect();
        assert!(
            m.contains("packages/a/src") && m.contains("packages/b/src"),
            "{m:?}"
        );
        assert!(d.scope.file_scope.cross_partition, "overlay contributed");
        assert!(d.scope.module_aggregated);
    }

    #[test]
    fn module_cycles_inherit_completeness_degradation() {
        // D4: the module answer inherits file_import_cycles' completeness exactly.
        let mut lg = LiveGraph::new();
        lg.load_partition("a", pkg_a_imports_b(), LanguageSupport::TypeScriptPrimary);
        lg.load_partition("b", pkg_b_imports_a(), LanguageSupport::TypeScriptPrimary);
        assert_eq!(lg.module_import_cycles().class(), AnswerClass::Exact);

        lg.mark_stale("a"); // both resident, one stale -> Stale.
        assert_eq!(lg.module_import_cycles().class(), AnswerClass::Stale);

        lg.unload_partition("b"); // b non-resident -> Partial + missing (overlay also drops).
        let degraded = lg.module_import_cycles();
        assert_eq!(degraded.class(), AnswerClass::Partial);
        assert!(degraded.missing_partitions().contains(&"b".to_string()));
    }
}
