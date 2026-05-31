#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # repo-graph-trust-model — Stage B trust / freshness / identity vocabulary (TRUST-MODEL-REBASE-1)
//!
//! (The existing `repo-graph-trust` crate is the shipped v1 SQLite trust-reporting service for the
//! outgoing substrate — a different, non-pure crate, untouched by this slice.)
//!
//! Pure-domain types. **Zero deps on scip / sqlite / tree-sitter / daemon / `repo-graph-ir` /
//! `repo-graph-scip-ingest`** — the most stable, abstract layer; everything trust-related depends
//! inward on it. Optional `serde` feature (off by default) for later query/API DTOs.
//!
//! Design rules (STAGE-C-ENTRY-DECISION + this slice):
//! - [`IdentityBasis`] labels are **descriptive only** — no global completeness.
//! - Completeness is **query-contextual** ([`classify_answer`]): the same basis is `Complete` for
//!   one [`QueryGranularity`] and `Degraded` for another.
//! - Invariants are enforced at the [`AnswerEnvelope`] layer via smart constructors (illegal
//!   states unrepresentable); the basis alone never decides exactness.
//! - `null` ≠ empty: unknown / unaddressable is [`AnswerClass::Unavailable`], never an empty result.

// ── Axis 1: answer class + delivery granularity ───────────────────

/// The completeness class of a query answer (XPART-PROVE-1A answer-class contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AnswerClass {
    /// Complete and trustworthy: every required basis is complete for the query and data is fresh.
    Exact,
    /// Resident facts plus an explicit, non-empty set of missing / degraded reasons.
    Partial,
    /// Cannot be answered (no entry / not indexed). Distinguishable from an `Exact` empty result.
    Unavailable,
    /// Served from a non-fresh (last-good) epoch during / after a refresh.
    Stale,
}

/// The granularity an answer is delivered at (distinct from the query intent [`QueryGranularity`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Granularity {
    /// Per-partition reference counts only (no caller identities).
    PartitionSummary,
    /// Caller identities for resident partitions.
    CallerDetail,
}

// ── Axis 2: freshness ─────────────────────────────────────────────

/// Freshness of the data backing an answer (REFRESH-PROBE-1 epoch contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FreshnessState {
    /// Resident epoch is the latest; no refresh in flight.
    Fresh,
    /// Refresh in flight; the last-good epoch is served meanwhile.
    Stale,
    /// Two-speed gap: AST fast delta applied, SCIP slow refresh lagging.
    PrecisionPending,
    /// Refresh errored; the last-good epoch is kept.
    RefreshFailed,
    /// No xref entry / partition not indexed.
    Unavailable,
}

// ── Axis 3: identity basis (DESCRIPTIVE ONLY — no global completeness) ──

/// How a node/edge's identity was obtained. **Descriptive only**: a basis is *not* globally
/// complete or degraded — completeness is computed per query by [`classify_answer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IdentityBasis {
    /// Value-level `(file,range)` AST join (INGEST-CORE-1).
    AstAdopted,
    /// SCIP-descriptor synthesized; no AST join (INGEST-CORE-1).
    ScipSynthesized,
    /// File / module-scope structural node (INGEST-CORE-1).
    AstFileScope,
    /// Published→source via declaration-map sources + descriptor-exact (XPART-PROVE-1B).
    DeclarationMapExact,
    /// Unique code-descriptor match under the strict predicate (XPART-PROVE-1B).
    NameExactUnique,
    /// Value fact attached on range AND terminal-name correspondence (CJOIN-PROVE-2).
    RangeNameConfirmed,
    /// Identity kept at the raw SCIP anchor; value fact NOT attached (CJOIN-PROVE-2).
    RawAnchored,
}

impl IdentityBasis {
    /// Whether this basis depends on SCIP-backed state (vs AST-derived). Used by invariant 6:
    /// under `PrecisionPending`, only non-SCIP (AST) bases may still be `Exact`.
    pub fn is_scip_backed(self) -> bool {
        match self {
            IdentityBasis::AstAdopted | IdentityBasis::AstFileScope => false,
            IdentityBasis::ScipSynthesized
            | IdentityBasis::DeclarationMapExact
            | IdentityBasis::NameExactUnique
            | IdentityBasis::RangeNameConfirmed
            | IdentityBasis::RawAnchored => true,
        }
    }
}

// ── Axis 4: degradation reason (orthogonal to basis) ──────────────

/// WHY a fact is degraded — a separate axis from [`IdentityBasis`]. A fact carries a basis (how)
/// AND, when degraded, one or more of these (why). Never conflate the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DegradationReason {
    /// TS `typeLiteralNN`, compilation-unit-relative, not cross-partition-addressable (XPART-1B).
    AnonymousStructuralMember,
    /// Package without declaration maps / complex `exports` (XPART-ST3 Residual 2).
    UnreconciledExportSurface,
    /// Alias reconciliation found multiple candidates (XPART-1B).
    AmbiguousAlias,
    /// Alias reconciliation found zero candidates (XPART-1B).
    UnresolvedAlias,
    /// C/C++ value fact withheld; range matched but name correspondence failed (CJOIN-PROVE-2).
    RawAnchoredByFailedNameGuard,
    /// Identity is `ScipSynthesized`, no value-level join (TS fallback; Rust dominant).
    ScipFallbackIdentity,
    /// Whole-workspace indexing unsupported (Rust; RUST-INGEST-PROVE-1).
    UnsupportedWorkspaceMode,
    /// Producer emitted a definition not in its document (Rust def-not-in-document; if bounded).
    DefinitionOutsideDocument,
    /// Duplicate symbol canonicalized by the deterministic dedup rule; provenance alias kept (Rust).
    DuplicateCanonicalized,
}

// ── Axis 5: language support maturity ─────────────────────────────

/// Per-language support maturity (a separate, query-visible axis). Language lives here +
/// provenance, never inside a basis name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LanguageSupport {
    /// TypeScript — declaration-map-backed named boundaries proven (XPART-1B).
    TypeScriptPrimary,
    /// C/C++ — value join only under range + terminal-name correspondence (CJOIN-PROVE-2).
    CppGuarded,
    /// Rust — per-crate only; B-very-slow-async; SCIP-fallback identity dominant (RUST-INGEST-PROVE-1).
    RustPartialBeta,
}

// ── Provenance DTO ────────────────────────────────────────────────

/// Alias / reconciliation provenance — a raw DTO of simple owned fields (no framework types).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProvenanceBasis {
    /// Package name.
    pub package_name: String,
    /// Package version.
    pub package_version: String,
    /// Public export path, if applicable.
    pub export_path: Option<String>,
    /// Declaration file the published symbol lives in, if applicable.
    pub declaration_file: Option<String>,
    /// Declaration map consulted, if any.
    pub declaration_map: Option<String>,
    /// Source file resolved, if any.
    pub source_file: Option<String>,
    /// The identity basis that produced this provenance.
    pub basis: IdentityBasis,
}

// ── Derived completeness verdict ──────────────────────────────────

/// The derived completeness verdict, computed query-contextually by [`classify_answer`] — never
/// read from the basis alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QueryCompleteness {
    /// Every required basis is complete for the query granularity and there are no degradations.
    Complete,
    /// At least one basis is not complete for this query, or a degradation reason applies.
    Degraded,
    /// Cannot be assessed (no bases / unavailable).
    Unknown,
}

// ── Query intent granularity (the QUESTION) ───────────────────────

/// The query intent — what is being asked. Completeness of a given [`IdentityBasis`] is relative
/// to this. (e.g. `AstFileScope` is complete for `FileReference` but not `CallGraph`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QueryGranularity {
    /// "Which files reference X?"
    FileReference,
    /// "Which callables call X?"
    CallGraph,
    /// "Which symbol owns this fact?"
    SymbolOwnership,
    /// "Where is this raw fact observed?"
    RawObservation,
    /// Compiler-derived reference identity.
    ReferenceIdentity,
    /// Governance / A1-stable canonical identity.
    GovernanceStableIdentity,
}

/// Whether a basis is complete **for a specific query granularity** (the policy content; conservative
/// — anything not explicitly complete is degraded, never falsely complete).
fn basis_complete_for(basis: IdentityBasis, g: QueryGranularity) -> bool {
    use IdentityBasis as B;
    use QueryGranularity as G;
    match (basis, g) {
        // Value-level AST identity: complete for any query.
        (B::AstAdopted, _) => true,
        // Reconciled cross-partition value identity: complete for value-level queries.
        (B::DeclarationMapExact | B::NameExactUnique, G::CallGraph)
        | (B::DeclarationMapExact | B::NameExactUnique, G::SymbolOwnership)
        | (B::DeclarationMapExact | B::NameExactUnique, G::ReferenceIdentity)
        | (B::DeclarationMapExact | B::NameExactUnique, G::GovernanceStableIdentity) => true,
        // C/C++ value-confirmed: complete for value queries (no governance-stable key assumed).
        (B::RangeNameConfirmed, G::CallGraph)
        | (B::RangeNameConfirmed, G::SymbolOwnership)
        | (B::RangeNameConfirmed, G::ReferenceIdentity) => true,
        // File/module-scope: complete only for file-reference questions.
        (B::AstFileScope, G::FileReference) => true,
        // SCIP-synthesized: complete for a compiler-derived reference identity only.
        (B::ScipSynthesized, G::ReferenceIdentity) => true,
        // Raw anchor: complete for "where observed", not for symbol ownership.
        (B::RawAnchored, G::RawObservation) | (B::RawAnchored, G::ReferenceIdentity) => true,
        // Conservative default: never falsely complete.
        _ => false,
    }
}

/// Inputs to the query-contextual completeness policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessInput {
    /// What is being asked.
    pub granularity: QueryGranularity,
    /// The identity bases the answer depends on.
    pub bases: Vec<IdentityBasis>,
    /// Freshness of the backing data.
    pub freshness: FreshnessState,
    /// Any degradation reasons already known.
    pub degradation_reasons: Vec<DegradationReason>,
    /// The language support contract in effect.
    pub language: LanguageSupport,
}

/// The pure completeness policy: compute the `(AnswerClass, QueryCompleteness)` for a query from
/// granularity + bases + freshness + degradation reasons + language. **Query-contextual** — the
/// same basis classifies differently under different granularities.
pub fn classify_answer(input: &CompletenessInput) -> (AnswerClass, QueryCompleteness) {
    if input.bases.is_empty() || input.freshness == FreshnessState::Unavailable {
        return (AnswerClass::Unavailable, QueryCompleteness::Unknown);
    }
    let bases_complete = input
        .bases
        .iter()
        .all(|b| basis_complete_for(*b, input.granularity));
    let complete = bases_complete && input.degradation_reasons.is_empty();
    match input.freshness {
        FreshnessState::Unavailable => (AnswerClass::Unavailable, QueryCompleteness::Unknown),
        FreshnessState::Stale | FreshnessState::RefreshFailed => {
            (AnswerClass::Stale, QueryCompleteness::Degraded)
        }
        FreshnessState::PrecisionPending => {
            // Invariant 6: Exact under PrecisionPending only if NOT SCIP-dependent (AST-only).
            let scip_dependent = input.bases.iter().any(|b| b.is_scip_backed());
            if complete && !scip_dependent {
                (AnswerClass::Exact, QueryCompleteness::Complete)
            } else {
                (AnswerClass::Partial, QueryCompleteness::Degraded)
            }
        }
        FreshnessState::Fresh => {
            if complete {
                (AnswerClass::Exact, QueryCompleteness::Complete)
            } else {
                (AnswerClass::Partial, QueryCompleteness::Degraded)
            }
        }
    }
}

// ── Invariant-6 proof token ───────────────────────────────────────

/// Proof that an answer does not depend on SCIP-backed state (every basis is AST-derived). The
/// ONLY way to mint an `Exact` answer under `FreshnessState::PrecisionPending` (invariant 6).
#[derive(Debug, Clone, Copy)]
pub struct NotScipDependent(());

impl NotScipDependent {
    /// Construct only if `bases` is non-empty and every basis is non-SCIP (AST-derived).
    pub fn prove(bases: &[IdentityBasis]) -> Option<Self> {
        if !bases.is_empty() && bases.iter().all(|b| !b.is_scip_backed()) {
            Some(NotScipDependent(()))
        } else {
            None
        }
    }
}

// ── Errors ────────────────────────────────────────────────────────

/// Why an [`AnswerEnvelope`] smart constructor rejected an illegal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustError {
    /// `exact` requires `FreshnessState::Fresh` (or `PrecisionPending` with a [`NotScipDependent`] proof).
    ExactRequiresFresh,
    /// `exact` requires `QueryCompleteness::Complete`.
    ExactRequiresComplete,
    /// `partial` requires a non-empty set of degradation reasons.
    PartialRequiresReasons,
    /// `stale` must not be labelled `FreshnessState::Fresh`.
    StaleMustNotBeFresh,
}

impl core::fmt::Display for TrustError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            TrustError::ExactRequiresFresh => {
                "Exact requires Fresh (or PrecisionPending + NotScipDependent)"
            }
            TrustError::ExactRequiresComplete => "Exact requires Complete completeness",
            TrustError::PartialRequiresReasons => "Partial requires non-empty degradation reasons",
            TrustError::StaleMustNotBeFresh => "Stale must not be Fresh",
        };
        f.write_str(s)
    }
}

impl std::error::Error for TrustError {}

// ── AnswerEnvelope — invariant enforcement (illegal states unrepresentable) ──

/// A query answer with its trust/freshness/completeness labels. Constructed ONLY via the smart
/// constructors, which enforce the six invariants — a runtime cannot mint an unjustified `Exact`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AnswerEnvelope<T> {
    class: AnswerClass,
    freshness: FreshnessState,
    completeness: QueryCompleteness,
    data: Option<T>,
    degradation_reasons: Vec<DegradationReason>,
    /// Partitions whose non-residency makes the answer incomplete (XPART `missing=[...]`). A
    /// SEPARATE axis from `degradation_reasons`: residency incompleteness is NOT an identity
    /// degradation. (`String` ids now; a typed `PartitionId` is a recorded follow-up.)
    missing_partitions: Vec<String>,
    provenance: Vec<ProvenanceBasis>,
}

impl<T> AnswerEnvelope<T> {
    /// `Exact`: requires `data` present, `freshness == Fresh`, `completeness == Complete`, and no
    /// degradation reasons. (For the `PrecisionPending` exception use [`AnswerEnvelope::exact_precision_pending`].)
    pub fn exact(
        data: T,
        completeness: QueryCompleteness,
        freshness: FreshnessState,
        provenance: Vec<ProvenanceBasis>,
    ) -> Result<Self, TrustError> {
        if freshness != FreshnessState::Fresh {
            return Err(TrustError::ExactRequiresFresh);
        }
        if completeness != QueryCompleteness::Complete {
            return Err(TrustError::ExactRequiresComplete);
        }
        Ok(Self {
            class: AnswerClass::Exact,
            freshness,
            completeness,
            data: Some(data),
            degradation_reasons: Vec::new(),
            missing_partitions: Vec::new(),
            provenance,
        })
    }

    /// `Exact` under `PrecisionPending` — admissible ONLY with a [`NotScipDependent`] proof
    /// (invariant 6: the answer must not depend on SCIP-backed state). Requires `Complete`.
    pub fn exact_precision_pending(
        data: T,
        completeness: QueryCompleteness,
        _proof: NotScipDependent,
        provenance: Vec<ProvenanceBasis>,
    ) -> Result<Self, TrustError> {
        if completeness != QueryCompleteness::Complete {
            return Err(TrustError::ExactRequiresComplete);
        }
        Ok(Self {
            class: AnswerClass::Exact,
            freshness: FreshnessState::PrecisionPending,
            completeness,
            data: Some(data),
            degradation_reasons: Vec::new(),
            missing_partitions: Vec::new(),
            provenance,
        })
    }

    /// `Partial`: resident facts plus AT LEAST ONE of — a non-empty degradation-reason set OR a
    /// non-empty missing-partitions list (invariant 2; residency is a separate axis from identity
    /// degradation).
    pub fn partial(
        data: Option<T>,
        reasons: Vec<DegradationReason>,
        missing_partitions: Vec<String>,
        freshness: FreshnessState,
        provenance: Vec<ProvenanceBasis>,
    ) -> Result<Self, TrustError> {
        if reasons.is_empty() && missing_partitions.is_empty() {
            return Err(TrustError::PartialRequiresReasons);
        }
        Ok(Self {
            class: AnswerClass::Partial,
            freshness,
            completeness: QueryCompleteness::Degraded,
            data,
            degradation_reasons: reasons,
            missing_partitions,
            provenance,
        })
    }

    /// `Unavailable`: an explicit (typed) reason, NO data (invariant 3 + `null` ≠ empty).
    pub fn unavailable(reason: DegradationReason, freshness: FreshnessState) -> Self {
        Self {
            class: AnswerClass::Unavailable,
            freshness,
            completeness: QueryCompleteness::Unknown,
            data: None,
            degradation_reasons: vec![reason],
            missing_partitions: Vec::new(),
            provenance: Vec::new(),
        }
    }

    /// `Stale`: last-good data served from a non-fresh epoch. Rejects `Fresh` (invariant 4).
    pub fn stale(
        last_good_data: T,
        freshness: FreshnessState,
        reasons: Vec<DegradationReason>,
        missing_partitions: Vec<String>,
        provenance: Vec<ProvenanceBasis>,
    ) -> Result<Self, TrustError> {
        if freshness == FreshnessState::Fresh {
            return Err(TrustError::StaleMustNotBeFresh);
        }
        Ok(Self {
            class: AnswerClass::Stale,
            freshness,
            completeness: QueryCompleteness::Degraded,
            data: Some(last_good_data),
            degradation_reasons: reasons,
            missing_partitions,
            provenance,
        })
    }

    /// The answer class.
    pub fn class(&self) -> AnswerClass {
        self.class
    }
    /// The freshness state.
    pub fn freshness(&self) -> FreshnessState {
        self.freshness
    }
    /// The completeness verdict.
    pub fn completeness(&self) -> QueryCompleteness {
        self.completeness
    }
    /// The data, if present. `None` does NOT mean "known-zero" — read [`AnswerEnvelope::class`].
    pub fn data(&self) -> Option<&T> {
        self.data.as_ref()
    }
    /// The degradation reasons (empty only for `Exact`).
    pub fn degradation_reasons(&self) -> &[DegradationReason] {
        &self.degradation_reasons
    }
    /// Partitions whose non-residency makes the answer incomplete (the residency axis; empty for
    /// `Exact`).
    pub fn missing_partitions(&self) -> &[String] {
        &self.missing_partitions
    }
    /// The provenance bases.
    pub fn provenance(&self) -> &[ProvenanceBasis] {
        &self.provenance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_answer: completeness is QUERY-CONTEXTUAL (the D2 point) ──

    fn input(
        g: QueryGranularity,
        bases: Vec<IdentityBasis>,
        fresh: FreshnessState,
    ) -> CompletenessInput {
        CompletenessInput {
            granularity: g,
            bases,
            freshness: fresh,
            degradation_reasons: Vec::new(),
            language: LanguageSupport::TypeScriptPrimary,
        }
    }

    #[test]
    fn same_basis_is_context_dependent() {
        // AstFileScope: Complete for file-reference, Degraded for call-graph.
        let file = input(
            QueryGranularity::FileReference,
            vec![IdentityBasis::AstFileScope],
            FreshnessState::Fresh,
        );
        assert_eq!(
            classify_answer(&file),
            (AnswerClass::Exact, QueryCompleteness::Complete)
        );
        let call = input(
            QueryGranularity::CallGraph,
            vec![IdentityBasis::AstFileScope],
            FreshnessState::Fresh,
        );
        assert_eq!(
            classify_answer(&call),
            (AnswerClass::Partial, QueryCompleteness::Degraded)
        );
    }

    #[test]
    fn raw_anchored_context_dependent() {
        let obs = input(
            QueryGranularity::RawObservation,
            vec![IdentityBasis::RawAnchored],
            FreshnessState::Fresh,
        );
        assert_eq!(classify_answer(&obs).0, AnswerClass::Exact);
        let owner = input(
            QueryGranularity::SymbolOwnership,
            vec![IdentityBasis::RawAnchored],
            FreshnessState::Fresh,
        );
        assert_eq!(classify_answer(&owner).0, AnswerClass::Partial);
    }

    #[test]
    fn scip_synthesized_context_dependent() {
        let refid = input(
            QueryGranularity::ReferenceIdentity,
            vec![IdentityBasis::ScipSynthesized],
            FreshnessState::Fresh,
        );
        assert_eq!(classify_answer(&refid).0, AnswerClass::Exact);
        let gov = input(
            QueryGranularity::GovernanceStableIdentity,
            vec![IdentityBasis::ScipSynthesized],
            FreshnessState::Fresh,
        );
        assert_eq!(classify_answer(&gov).0, AnswerClass::Partial);
    }

    #[test]
    fn degradation_reason_forces_partial() {
        let mut i = input(
            QueryGranularity::CallGraph,
            vec![IdentityBasis::DeclarationMapExact],
            FreshnessState::Fresh,
        );
        assert_eq!(classify_answer(&i).0, AnswerClass::Exact);
        i.degradation_reasons
            .push(DegradationReason::AnonymousStructuralMember);
        assert_eq!(classify_answer(&i).0, AnswerClass::Partial);
    }

    #[test]
    fn empty_bases_or_unavailable_freshness_is_unavailable() {
        let none = input(QueryGranularity::CallGraph, vec![], FreshnessState::Fresh);
        assert_eq!(
            classify_answer(&none),
            (AnswerClass::Unavailable, QueryCompleteness::Unknown)
        );
        let unav = input(
            QueryGranularity::CallGraph,
            vec![IdentityBasis::AstAdopted],
            FreshnessState::Unavailable,
        );
        assert_eq!(classify_answer(&unav).0, AnswerClass::Unavailable);
    }

    #[test]
    fn stale_freshness_yields_stale_class() {
        let i = input(
            QueryGranularity::CallGraph,
            vec![IdentityBasis::AstAdopted],
            FreshnessState::Stale,
        );
        assert_eq!(classify_answer(&i).0, AnswerClass::Stale);
    }

    // ── Invariant 6: PrecisionPending Exact only when NOT SCIP-dependent ──

    #[test]
    fn precision_pending_exact_only_when_not_scip_dependent() {
        // AST-only (not SCIP-backed) → Exact admissible under PrecisionPending.
        let ast = input(
            QueryGranularity::CallGraph,
            vec![IdentityBasis::AstAdopted],
            FreshnessState::PrecisionPending,
        );
        assert_eq!(classify_answer(&ast).0, AnswerClass::Exact);
        // SCIP-backed → Partial under PrecisionPending.
        let scip = input(
            QueryGranularity::CallGraph,
            vec![IdentityBasis::DeclarationMapExact],
            FreshnessState::PrecisionPending,
        );
        assert_eq!(classify_answer(&scip).0, AnswerClass::Partial);
    }

    #[test]
    fn not_scip_dependent_proof_only_for_ast_bases() {
        assert!(
            NotScipDependent::prove(&[IdentityBasis::AstAdopted, IdentityBasis::AstFileScope])
                .is_some()
        );
        assert!(NotScipDependent::prove(&[
            IdentityBasis::AstAdopted,
            IdentityBasis::ScipSynthesized
        ])
        .is_none());
        assert!(NotScipDependent::prove(&[]).is_none());
    }

    // ── AnswerEnvelope smart constructors (illegal states unrepresentable) ──

    #[test]
    fn exact_requires_fresh_and_complete() {
        assert!(AnswerEnvelope::exact(
            1u32,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            vec![]
        )
        .is_ok());
        // not fresh → rejected
        assert_eq!(
            AnswerEnvelope::exact(
                1u32,
                QueryCompleteness::Complete,
                FreshnessState::Stale,
                vec![]
            )
            .unwrap_err(),
            TrustError::ExactRequiresFresh
        );
        // not complete → rejected
        assert_eq!(
            AnswerEnvelope::exact(
                1u32,
                QueryCompleteness::Degraded,
                FreshnessState::Fresh,
                vec![]
            )
            .unwrap_err(),
            TrustError::ExactRequiresComplete
        );
    }

    #[test]
    fn exact_precision_pending_needs_proof() {
        let proof = NotScipDependent::prove(&[IdentityBasis::AstAdopted]).unwrap();
        let env = AnswerEnvelope::exact_precision_pending(
            1u32,
            QueryCompleteness::Complete,
            proof,
            vec![],
        )
        .unwrap();
        assert_eq!(env.class(), AnswerClass::Exact);
        assert_eq!(env.freshness(), FreshnessState::PrecisionPending);
    }

    #[test]
    fn partial_with_no_reason_and_no_missing_partition_rejected() {
        assert_eq!(
            AnswerEnvelope::partial(Some(1u32), vec![], vec![], FreshnessState::Fresh, vec![])
                .unwrap_err(),
            TrustError::PartialRequiresReasons
        );
    }

    #[test]
    fn partial_with_missing_partition_is_valid() {
        let env = AnswerEnvelope::partial(
            Some(1u32),
            vec![],
            vec!["engine".to_string()],
            FreshnessState::Fresh,
            vec![],
        )
        .unwrap();
        assert_eq!(env.class(), AnswerClass::Partial);
        assert!(env.degradation_reasons().is_empty());
        assert_eq!(env.missing_partitions(), ["engine"]);
    }

    #[test]
    fn partial_with_reason_and_missing_partition_valid() {
        let env = AnswerEnvelope::partial(
            Some(1u32),
            vec![DegradationReason::ScipFallbackIdentity],
            vec!["api".to_string()],
            FreshnessState::Fresh,
            vec![],
        )
        .unwrap();
        assert_eq!(env.class(), AnswerClass::Partial);
        assert!(!env.degradation_reasons().is_empty());
        assert!(!env.missing_partitions().is_empty());
    }

    #[test]
    fn exact_with_missing_partition_rejected() {
        // No constructor yields Exact with missing partitions: exact() always sets missing empty.
        // Residency-incompleteness must be expressed via partial().
        let env = AnswerEnvelope::exact(
            1u32,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            vec![],
        )
        .unwrap();
        assert!(env.missing_partitions().is_empty());
    }

    #[test]
    fn unavailable_is_not_empty() {
        let env: AnswerEnvelope<u32> = AnswerEnvelope::unavailable(
            DegradationReason::UnresolvedAlias,
            FreshnessState::Unavailable,
        );
        assert_eq!(env.class(), AnswerClass::Unavailable);
        assert!(env.data().is_none());
        // null ≠ empty: an Unavailable answer is distinguishable from an Exact empty result.
        assert!(!env.degradation_reasons().is_empty());
    }

    #[test]
    fn stale_must_not_be_fresh() {
        assert_eq!(
            AnswerEnvelope::stale(1u32, FreshnessState::Fresh, vec![], vec![], vec![]).unwrap_err(),
            TrustError::StaleMustNotBeFresh
        );
        assert!(AnswerEnvelope::stale(1u32, FreshnessState::Stale, vec![], vec![], vec![]).is_ok());
    }
}
