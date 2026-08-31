//! ATTRIBUTION-1: the ONE shared module for attributing where the reader's
//! UNRESOLVED references go — vocabulary AND the typed `basis_code → class` mapping.
//!
//! RELIABILITY-REFRAME-1 shipped the aggregate coverage map ("16% of calls go into
//! external libraries — `Value`, `Vec`, `String`"). This module completes the
//! per-class story: it NAMES the reader's unresolved references in the reader's
//! world — "library call → serde", "standard library / runtime module",
//! "runtime/system built-in", "your own code (call target not resolved)", "framework
//! / dynamic dispatch", "couldn't attribute" — instead of the internal classifier
//! vocabulary (`external_library_candidate`, `internal_candidate`, and the 17
//! `basis_code` values) the `trust` "Classification" section used to leak.
//!
//! ## The ONE module (ATTR1-MAPPING-BOUNDARY, operator-RATIFIED 2026-07-15)
//!
//! Slice §1.4 mandates ONE shared mapping module consumed by every renderer, and §3
//! mandates it be EXHAUSTIVE over the typed basis codes ("a new code fails
//! compilation, not silently unknown"). Review-1 escalated the earlier rgr/agent
//! split; the operator ratified option A: `agent` MAY depend on
//! `repo-graph-classification` (an inner leaf crate — outer → inner, cycle-free), so
//! the typed [`attribution_class`] match lives HERE beside the vocabulary. `agent` is
//! the one crate every current and future renderer reaches (`check::evaluate` is a
//! reducer in this crate; the `trust` / `orient` reader surfaces are in `rgr`, which
//! depends on `agent`), exactly as [`crate::reliability`] owns the shared
//! call-reliability wording. One home for the wording AND the mapping ⇒ no renderer
//! can fork either.
//!
//! `agent` still does NOT depend on `repo-graph-trust` (the documented boundary):
//! the wire basis code arrives as a `String` on `TrustBasisClassificationRow`, and
//! [`attribution_breakdown`] takes neutral `(&str, count)` pairs — trust types never
//! cross into this crate.
//!
//! ## Honesty: what "named dependency" means, and where it degrades
//! (VISION: labels speak the reader's language; unknown is never zero)
//!
//! The coarse 4-value `UnresolvedEdgeClassification` FOLDS third-party dependencies,
//! the standard library, and runtime globals into one `external_library_candidate`
//! bucket. The reader needs those apart, so [`AttributionClass`] has a distinct
//! variant for each, dispatched on the FINER `UnresolvedEdgeBasisCode` axis.
//!
//! Within the "library call" ([`AttributionClass::ExternalDependency`]) class, each
//! reference is NAMED by the DECLARED manifest dependency it resolves to. Naming is NOT done
//! in this module — it is the STORAGE provenance join
//! (`repo_graph_classification::resolve_external_dependency_name`, reused so the reduction
//! matches the classifier exactly), which covers ALL THREE external-import bases:
//!
//!   - [`UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency`]: the import specifier
//!     itself (Rust `use serde::…`, Python `import json`), reduced to the declared name.
//!   - [`UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport`]: the call receiver (`app`
//!     in `app.listen`) traced through the file's import bindings to the binding's specifier,
//!     reduced to the declared name (`express`).
//!   - [`UnresolvedEdgeBasisCode::CalleeMatchesExternalImport`]: the callee (`useState`)
//!     traced through the import bindings likewise (`react`).
//!
//! The scoped path (`repo_graph_indexer::types`) and the raw call expression (`app.listen`)
//! are NEVER shown — only the reduced declared name (`repo-graph-indexer`, `express`). A
//! reference resolving to no declared dependency (no import binding introduced the identifier,
//! or its specifier matches no manifest entry) degrades to the honest "library call
//! (dependency not identified)" — never a fabricated name.
//!
//! Dependency VERSION is discarded at extraction (`PackageDependencySet` is names-only;
//! edge metadata is `{rawPath|specifier}`), so a "serde 1.x" claim would be FABRICATED.
//! The basis markers state the heuristic basis, that versions are not recorded, and the
//! Java/Gradle limitation (R3), so the reader never over-trusts the attribution.

use repo_graph_classification::types::UnresolvedEdgeBasisCode;

/// The reader-frame class an unresolved reference is attributed to — WHERE, in the
/// reader's world, the call goes.
///
/// The six variants are the honest split of the classifier's 4-value classification:
/// `external_library_candidate` fans out into [`Self::ExternalDependency`] /
/// [`Self::StandardLibrary`] / [`Self::SystemRuntimeBuiltin`] by basis code;
/// `internal_candidate` → [`Self::OwnCodeUnresolved`]; `framework_boundary_candidate`
/// → [`Self::DynamicDispatch`]; `unknown` → [`Self::Unattributed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionClass {
    /// The reference leaves the reader's source into a third-party dependency — an
    /// import specifier that matched a declared manifest dependency, or a
    /// receiver/callee that came in via an external import.
    ExternalDependency,
    /// The reference targets the language's standard library / a runtime module
    /// (`std::…`, `path`, `node:fs`) — out of source scope by design, but NOT a
    /// third-party dependency.
    StandardLibrary,
    /// The reference targets a runtime/system built-in global (`Map`, `Date`,
    /// `process`) — provided by the runtime, not declared in the reader's source or
    /// its manifest.
    SystemRuntimeBuiltin,
    /// The reference targets the reader's OWN code but could not be resolved
    /// (relative/alias import, same-file symbol, `this` receiver, Rust
    /// crate-internal). An in-scope resolution gap — named as the reader's own.
    OwnCodeUnresolved,
    /// The reference is dispatched through framework / runtime wiring (an Express
    /// route/middleware registration) — a dynamic-dispatch surface, not a static
    /// call edge.
    DynamicDispatch,
    /// No classification signal matched — the classifier could not attribute the
    /// target at all.
    Unattributed,
}

impl AttributionClass {
    /// The reader-frame label — WHERE this class of unresolved reference goes, in the
    /// reader's world. Wildcard-free (slice §3: a new reader class fails compilation,
    /// never a silent "unknown"). Carries NO internal vocabulary.
    pub fn reader_label(self) -> &'static str {
        match self {
            Self::ExternalDependency => "library call (external dependency)",
            Self::StandardLibrary => "standard library / runtime module",
            Self::SystemRuntimeBuiltin => "runtime/system built-in (language global)",
            Self::OwnCodeUnresolved => "your own code (call target not resolved)",
            Self::DynamicDispatch => "framework / dynamic dispatch (runtime wiring)",
            Self::Unattributed => "couldn't attribute",
        }
    }

    /// The reader-frame NEXT ACTION for this class, when there is one (VISION:
    /// degradation/orientation states a concrete next step). Only references that
    /// leave into external code (a dependency, the standard library, a runtime
    /// built-in) have a "go look there" target the reader can follow; own-code /
    /// dynamic-dispatch / unattributed references do not. Wildcard-free for the same
    /// compile-fail guarantee as [`Self::reader_label`].
    pub fn follow_hint(self) -> Option<&'static str> {
        match self {
            Self::ExternalDependency => Some("follow to that dependency's crate / package docs"),
            Self::StandardLibrary => Some("follow to the language's standard-library docs"),
            Self::SystemRuntimeBuiltin => Some("follow to the runtime's built-in / global docs"),
            Self::OwnCodeUnresolved | Self::DynamicDispatch | Self::Unattributed => None,
        }
    }
}

// ── The typed mapping (moved here per ATTR1-MAPPING-BOUNDARY option A) ───────────────

/// Map a classifier basis code to its reader-frame attribution class.
///
/// The ONE exhaustive, wildcard-free mapping (slice §1.4). No wildcard arm: a new
/// `UnresolvedEdgeBasisCode` variant fails to compile here until it is assigned a
/// reader class (the compile-time exhaustiveness guarantee, slice §3).
pub fn attribution_class(basis: UnresolvedEdgeBasisCode) -> AttributionClass {
    use UnresolvedEdgeBasisCode as B;
    match basis {
        // External third-party dependency: a specifier that matched a declared
        // manifest dependency, or a receiver/callee that came in via an external
        // import. The DECLARED name of each (across all three) is resolved by the
        // storage provenance join, not here.
        B::SpecifierMatchesPackageDependency
        | B::ReceiverMatchesExternalImport
        | B::CalleeMatchesExternalImport => AttributionClass::ExternalDependency,
        // Standard library / runtime module (`std::…`, `path`, `node:fs`).
        B::SpecifierMatchesRuntimeModule => AttributionClass::StandardLibrary,
        // Runtime/system built-in global (`Map`, `Date`, `process`).
        B::ReceiverMatchesRuntimeGlobal | B::CalleeMatchesRuntimeGlobal => {
            AttributionClass::SystemRuntimeBuiltin
        }
        // The reader's own code (relative/alias import, same-file symbol, `this`
        // receiver, Rust crate-internal) — an in-scope resolution gap.
        B::SpecifierMatchesProjectAlias
        | B::ReceiverMatchesInternalImport
        | B::ReceiverMatchesSameFileSymbol
        | B::CalleeMatchesSameFileSymbol
        | B::CalleeMatchesInternalImport
        | B::ThisReceiverImpliesInternal
        | B::RelativeImportTargetUnresolved
        | B::RustCrateInternalModuleHeuristic => AttributionClass::OwnCodeUnresolved,
        // Framework runtime wiring (Express route/middleware registration).
        B::ExpressRouteRegistration | B::ExpressMiddlewareRegistration => {
            AttributionClass::DynamicDispatch
        }
        // No classification signal matched.
        B::NoSupportingSignal => AttributionClass::Unattributed,
    }
}

/// Parse a wire basis-code string back into the typed enum via serde's rename-aware
/// deserialization. `None` for an unrecognized code (an older/newer daemon carrying a
/// basis code this build predates) — the caller folds it into the honest "other"
/// bucket, never dropping the count and never leaking the raw code. Mirrors
/// `reliability::band_from_wire` (string → typed with a `None` fallback).
pub fn parse_basis_code(code: &str) -> Option<UnresolvedEdgeBasisCode> {
    serde_json::from_value(serde_json::Value::String(code.to_string())).ok()
}

// ── The reader-frame breakdown ───────────────────────────────────────────────────────

/// The reader-frame attribution breakdown, aggregated from the basis-code counts.
///
/// Multiple basis codes fold into one reader class (e.g. three "external" bases →
/// [`AttributionClass::ExternalDependency`]); this holds the per-class totals and the
/// [`Self::other`] bucket for unrecognized wire codes. The "library call" class is rendered
/// per-named-dependency from the STORAGE provenance join (the declared-dependency names +
/// the named/unidentified totals), NOT split here — this breakdown only supplies the class
/// TOTALS (used for count-descending ordering and to know which classes to render).
pub struct AttributionBreakdown {
    /// Per reader class, count-descending (canonical class order breaks ties for
    /// determinism). Zero-count classes are omitted.
    pub classes: Vec<(AttributionClass, u64)>,
    /// References whose wire basis code was unrecognized (folded, count preserved).
    pub other: u64,
}

impl AttributionBreakdown {
    /// No attributed references AND no unrecognized ones — the renderer emits no
    /// section (never a heading with nothing under it).
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.other == 0
    }
}

/// Canonical class order — the deterministic tie-break when two classes have equal
/// counts (a stable sort by count-desc then preserves this order). Also the iteration
/// order used to build the per-class totals.
const CANONICAL_ORDER: [AttributionClass; 6] = [
    AttributionClass::ExternalDependency,
    AttributionClass::StandardLibrary,
    AttributionClass::SystemRuntimeBuiltin,
    AttributionClass::OwnCodeUnresolved,
    AttributionClass::DynamicDispatch,
    AttributionClass::Unattributed,
];

fn ordinal(class: AttributionClass) -> usize {
    CANONICAL_ORDER
        .iter()
        .position(|&c| c == class)
        .expect("every AttributionClass is in CANONICAL_ORDER")
}

/// Aggregate the basis-code counts into a reader-frame breakdown (per-class totals +
/// the honest "other" bucket), deterministically ordered count-desc. The library
/// named/unidentified split is rendered separately from the storage join's
/// `ExternalDependencyAttribution` — this aggregate carries class totals only.
///
/// Takes NEUTRAL `(wire basis code, count)` pairs — never a `repo-graph-trust` type —
/// so this crate keeps its documented no-dependency-on-`repo-graph-trust` boundary. An
/// unrecognized wire code folds into [`AttributionBreakdown::other`] (count preserved,
/// raw code never surfaced), the runtime analogue of the compile-time exhaustiveness
/// [`attribution_class`] guarantees for known codes.
pub fn attribution_breakdown<'a>(
    rows: impl IntoIterator<Item = (&'a str, u64)>,
) -> AttributionBreakdown {
    let mut counts = [0u64; CANONICAL_ORDER.len()];
    let mut other = 0u64;
    for (basis_code, count) in rows {
        match parse_basis_code(basis_code) {
            Some(basis) => counts[ordinal(attribution_class(basis))] += count,
            None => other += count,
        }
    }
    let mut classes: Vec<(AttributionClass, u64)> = CANONICAL_ORDER
        .iter()
        .copied()
        .map(|c| (c, counts[ordinal(c)]))
        .filter(|&(_, n)| n > 0)
        .collect();
    // Count-descending; equal counts keep CANONICAL_ORDER (stable sort).
    classes.sort_by(|a, b| b.1.cmp(&a.1));
    AttributionBreakdown { classes, other }
}

// ── Line builders (the SINGLE source of the reader-frame wording) ─────────────────────

/// "N reference" / "N references" — the count noun, singular at 1. Keeps the
/// per-class breakdown line grammatical without forking pluralization across renderers.
pub fn count_references(n: u64) -> String {
    format!("{} reference{}", n, if n == 1 { "" } else { "s" })
}

/// One reader-frame CLASS-total line: `"<reader label>: <N reference(s)>[ — <hint>]"`.
/// Used for every class EXCEPT `ExternalDependency`, which renders per-named-dependency
/// lines instead (via [`named_dependency_line`]).
pub fn attribution_line(class: AttributionClass, count: u64) -> String {
    let base = format!("{}: {}", class.reader_label(), count_references(count));
    match class.follow_hint() {
        Some(hint) => format!("{base} — {hint}"),
        None => base,
    }
}

/// One NAMED library-dependency line: `"library call → <dep>: <N reference(s)>"`
/// (slice §1.2 / review-1 REVISE #1). `dep` is the DECLARED base dependency returned by
/// the three-path storage join (specifier / receiver-import / callee-import bases, scoped
/// specifiers reduced to the declared name — `serde`, not `serde::de`); the version is
/// never appended (not recorded). The section's basis markers carry the follow hint + the heuristic /
/// version / Java-Gradle honesty, so each named line stays compact.
pub fn named_dependency_line(dep: &str, count: u64) -> String {
    format!("library call → {}: {}", dep, count_references(count))
}

/// The honest "identified but beyond the shown top-N" tail line: declared dependencies
/// whose specifiers were named but did not make the bounded list. Distinct from
/// [`dependency_not_identified_line`] — these ARE identified, just not individually
/// listed.
pub fn more_named_dependencies_line(refs: u64) -> String {
    format!(
        "library call → other declared dependencies: {}",
        count_references(refs)
    )
}

/// The honest missing-name degradation line (slice §1.2 / review-1 REVISE #1):
/// `ExternalDependency` references the storage provenance join could NOT resolve to a
/// declared dependency — no import binding introduced the identifier, or its specifier
/// matched no manifest entry — across any of the three external-import bases. The dependency
/// cannot be named from the facts storage carries, so it is not fabricated.
pub fn dependency_not_identified_line(refs: u64) -> String {
    format!(
        "library call (dependency not identified): {}",
        count_references(refs)
    )
}

// ── First-party (this repo's own crates/packages) — TRUST-FIRSTPARTY-1 ────────────────────────
//
// A reference whose resolved DECLARED dependency name matches a package THIS repo's parsed
// manifests declare as their own (workspace member / declared package) is NOT a third-party
// library — it is the reader's OWN code, reachable in-repo. It must never render as a "library
// call" with a crates.io/package-docs follow (the exact defect TRUST-FIRSTPARTY-1 fixes). These
// builders give it its own reader-frame wording + an IN-REPO next move.

/// One NAMED first-party line: `"internal crate/package → <name>: <N reference(s)> (this repo)"`.
/// `name` is the DECLARED manifest package name (`repo-graph-storage`, `@glamcrm/core`) that
/// matched one of THIS repo's own parsed manifests — a workspace member / declared package, from
/// structural manifest facts, never a name prefix. The "(this repo)" marker + the in-repo follow
/// hint ([`FIRST_PARTY_FOLLOW`]) keep the reader oriented to their own code, not to a dependency.
pub fn first_party_line(name: &str, count: u64) -> String {
    format!(
        "internal crate/package → {}: {} (this repo)",
        name,
        count_references(count)
    )
}

/// The "identified first-party crates beyond the shown top-N" tail — repo-own packages that ARE
/// named but did not make the bounded list. The first-party analogue of
/// [`more_named_dependencies_line`].
pub fn more_first_party_line(refs: u64) -> String {
    format!(
        "internal crate/package → other workspace crates: {} (this repo)",
        count_references(refs)
    )
}

/// The IN-REPO next move for first-party references (VISION: degradation/orientation states a
/// concrete next step, in the reader's frame). Unlike the external follow hint ("follow to that
/// dependency's crate / package docs"), this points the agent back INTO the repo — these are its
/// own crates, discoverable with the tool itself.
pub const FIRST_PARTY_FOLLOW: &str =
    "these are this repo's own crates/packages — explore with `rmap explain <symbol>` or open \
     their module, not external dependency docs";

/// TRUST-FIRSTPARTY-1 (review-1 §2): the shown first-party rows sum to MORE than the reported
/// first-party total — the two do not reconcile. Rendered in place of a saturated-to-zero
/// remainder that would hide the inconsistency (STANDING HONESTY RULE 1 / architecture rule 6:
/// unknown WITH REASON, never a fabricated 0). Unreachable in a coherent report (the shown rows are
/// a truncation of the counted first-party set, so their sum is always `<= first_party_total`).
pub const FIRST_PARTY_REMAINDER_UNRECONCILED: &str =
    "internal crate/package → workspace-crate remainder unavailable: the listed first-party crates \
     exceed the reported first-party total — the report is internally inconsistent";

/// The reader-frame label for unresolved references whose wire basis code the renderer
/// did not recognize (an older/newer daemon carrying a basis code this build predates).
/// An honest catch-all so the count is never lost and the raw code never leaks.
pub const OTHER_UNRESOLVED_LABEL: &str = "other (attribution unavailable)";

/// The EY1-A honesty basis for the reframed breakdown: the attribution is a heuristic
/// per-reference classification (import specifier / same-file symbol / receiver origin
/// / runtime name-set signals), NOT a compiler-verified fact.
pub const ATTRIBUTION_BASIS: &str =
    "basis: heuristic per-reference attribution (import specifier / same-file symbol / \
     receiver origin / runtime name-set signals), not compiler-verified";

/// The honest PROVENANCE-degradation marker (slice §1.2 / review-0 #3), rendered on every
/// attribution section. A named library dependency is the DECLARED manifest dependency a
/// reference resolved to — across all three external-import bases the storage provenance join
/// covers: by its import specifier ([`UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency`]),
/// or by the import binding that introduced its receiver
/// ([`UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport`]) or callee
/// ([`UnresolvedEdgeBasisCode::CalleeMatchesExternalImport`]), each reduced to the declared
/// name. A reference resolving to no declared dependency degrades to "dependency not
/// identified" (never a fabricated name). Dependency VERSIONS are not recorded by the
/// extractor, so none is ever claimed; Java/Gradle manifest identification is prefix-heuristic
/// only (R3).
pub const PROVENANCE_BASIS: &str =
    "provenance: a named dependency is the declared manifest dependency a reference resolved \
     to — by its import specifier, or by the import that introduced its receiver or callee — \
     matched against your declared dependencies (versions are not recorded; a reference that \
     resolved to no declared dependency reads \"dependency not identified\"; Java/Gradle \
     identification is heuristic — R3)";

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CLASSES: [AttributionClass; 6] = [
        AttributionClass::ExternalDependency,
        AttributionClass::StandardLibrary,
        AttributionClass::SystemRuntimeBuiltin,
        AttributionClass::OwnCodeUnresolved,
        AttributionClass::DynamicDispatch,
        AttributionClass::Unattributed,
    ];

    #[test]
    fn every_reader_label_is_distinct_and_non_empty() {
        let mut labels: Vec<&str> = ALL_CLASSES.iter().map(|c| c.reader_label()).collect();
        labels.sort_unstable();
        let before = labels.len();
        labels.dedup();
        assert_eq!(before, labels.len(), "reader labels must be distinct");
        assert!(ALL_CLASSES.iter().all(|c| !c.reader_label().is_empty()));
    }

    #[test]
    fn no_reader_label_or_line_leaks_an_internal_code() {
        // VISION: labels speak the reader's language. No reader label / line may
        // contain a raw classifier snake_case code or basis-code fragment.
        let mut lines: Vec<String> = ALL_CLASSES
            .iter()
            .map(|c| c.reader_label().into())
            .collect();
        lines.push(named_dependency_line("serde", 3));
        lines.push(more_named_dependencies_line(4));
        lines.push(dependency_not_identified_line(5));
        lines.push(attribution_line(AttributionClass::StandardLibrary, 2));
        // TRUST-FIRSTPARTY-1: the first-party lines must also carry no internal code.
        lines.push(first_party_line("repo-graph-storage", 8));
        lines.push(more_first_party_line(3));
        lines.push(FIRST_PARTY_FOLLOW.to_string());
        for line in lines {
            for leak in [
                "_candidate",
                "external_library",
                "internal_candidate",
                "framework_boundary",
                "basis_code",
                "specifier_matches",
                "receiver_matches",
                "callee_matches",
                "no_supporting_signal",
            ] {
                assert!(
                    !line.contains(leak),
                    "leaked internal code `{leak}` in: {line}"
                );
            }
        }
    }

    #[test]
    fn only_external_classes_carry_a_follow_hint() {
        assert!(AttributionClass::ExternalDependency.follow_hint().is_some());
        assert!(AttributionClass::StandardLibrary.follow_hint().is_some());
        assert!(AttributionClass::SystemRuntimeBuiltin
            .follow_hint()
            .is_some());
        assert!(AttributionClass::OwnCodeUnresolved.follow_hint().is_none());
        assert!(AttributionClass::DynamicDispatch.follow_hint().is_none());
        assert!(AttributionClass::Unattributed.follow_hint().is_none());
    }

    #[test]
    fn named_and_degraded_library_lines_match_the_reviewed_wording() {
        // review-1 REVISE #1: named dependency present + missing-name degradation.
        assert_eq!(
            named_dependency_line("serde", 12),
            "library call → serde: 12 references"
        );
        assert_eq!(
            named_dependency_line("json", 1),
            "library call → json: 1 reference"
        );
        assert_eq!(
            dependency_not_identified_line(15),
            "library call (dependency not identified): 15 references"
        );
        assert_eq!(
            more_named_dependencies_line(6),
            "library call → other declared dependencies: 6 references"
        );
    }

    #[test]
    fn first_party_lines_are_internal_framed_with_in_repo_next_move() {
        // TRUST-FIRSTPARTY-1: repo-own crates render as internal, NEVER a library call, with an
        // in-repo (not crates.io) next move.
        assert_eq!(
            first_party_line("repo-graph-storage", 8),
            "internal crate/package → repo-graph-storage: 8 references (this repo)"
        );
        assert_eq!(
            first_party_line("@glamcrm/core", 1),
            "internal crate/package → @glamcrm/core: 1 reference (this repo)"
        );
        assert_eq!(
            more_first_party_line(2),
            "internal crate/package → other workspace crates: 2 references (this repo)"
        );
        // The next move points INTO the repo, not to external docs.
        assert!(FIRST_PARTY_FOLLOW.contains("rmap explain"));
        assert!(!FIRST_PARTY_FOLLOW.contains("crate / package docs"));
        assert!(!first_party_line("x", 1).contains("library call"));
    }

    #[test]
    fn attribution_line_composes_label_count_and_hint_for_non_library_classes() {
        assert_eq!(
            attribution_line(AttributionClass::OwnCodeUnresolved, 12),
            "your own code (call target not resolved): 12 references"
        );
        assert_eq!(
            attribution_line(AttributionClass::StandardLibrary, 1),
            "standard library / runtime module: 1 reference — follow to the language's \
             standard-library docs"
        );
    }

    #[test]
    fn count_references_is_singular_at_one() {
        assert_eq!(count_references(0), "0 references");
        assert_eq!(count_references(1), "1 reference");
        assert_eq!(count_references(2), "2 references");
    }

    #[test]
    fn basis_markers_are_honest_and_carry_no_internal_code() {
        assert!(ATTRIBUTION_BASIS.contains("heuristic"));
        assert!(ATTRIBUTION_BASIS.contains("not compiler-verified"));
        // review-0 #3: version + Java/Gradle honesty is stated, never fabricated.
        assert!(PROVENANCE_BASIS.contains("versions are not recorded"));
        assert!(PROVENANCE_BASIS.contains("Java/Gradle"));
        assert!(PROVENANCE_BASIS.contains("import specifier"));
        for marker in [ATTRIBUTION_BASIS, PROVENANCE_BASIS, OTHER_UNRESOLVED_LABEL] {
            assert!(!marker.contains("_candidate"));
            assert!(!marker.contains("basis_code"));
        }
    }

    // ── Typed mapping (moved here per ATTR1-MAPPING-BOUNDARY option A) ──────────────

    /// Every basis-code variant paired with the reader class it MUST map to. If a new
    /// `UnresolvedEdgeBasisCode` variant is added, [`attribution_class`]'s wildcard-free
    /// match fails to compile FIRST (the primary guarantee); this table is the
    /// secondary assertion that the mapping is the intended one.
    const EXPECTED: &[(UnresolvedEdgeBasisCode, AttributionClass)] = &[
        (
            UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency,
            AttributionClass::ExternalDependency,
        ),
        (
            UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport,
            AttributionClass::ExternalDependency,
        ),
        (
            UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
            AttributionClass::ExternalDependency,
        ),
        (
            UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule,
            AttributionClass::StandardLibrary,
        ),
        (
            UnresolvedEdgeBasisCode::ReceiverMatchesRuntimeGlobal,
            AttributionClass::SystemRuntimeBuiltin,
        ),
        (
            UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal,
            AttributionClass::SystemRuntimeBuiltin,
        ),
        (
            UnresolvedEdgeBasisCode::SpecifierMatchesProjectAlias,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::ReceiverMatchesSameFileSymbol,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::CalleeMatchesInternalImport,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::RelativeImportTargetUnresolved,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::RustCrateInternalModuleHeuristic,
            AttributionClass::OwnCodeUnresolved,
        ),
        (
            UnresolvedEdgeBasisCode::ExpressRouteRegistration,
            AttributionClass::DynamicDispatch,
        ),
        (
            UnresolvedEdgeBasisCode::ExpressMiddlewareRegistration,
            AttributionClass::DynamicDispatch,
        ),
        (
            UnresolvedEdgeBasisCode::NoSupportingSignal,
            AttributionClass::Unattributed,
        ),
    ];

    #[test]
    fn every_basis_code_maps_to_its_expected_reader_class() {
        // 17 basis codes → 6 reader classes. The count pins that no variant was
        // dropped from EXPECTED when the classifier vocabulary last changed.
        assert_eq!(EXPECTED.len(), 17, "all 17 basis codes must be covered");
        for &(basis, expected) in EXPECTED {
            assert_eq!(attribution_class(basis), expected, "basis {basis:?}");
        }
    }

    #[test]
    fn the_three_external_axes_are_kept_apart() {
        // review-0 #2: the coarse classification folded these three into one bucket.
        assert_eq!(
            attribution_class(UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency),
            AttributionClass::ExternalDependency
        );
        assert_eq!(
            attribution_class(UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule),
            AttributionClass::StandardLibrary
        );
        assert_eq!(
            attribution_class(UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal),
            AttributionClass::SystemRuntimeBuiltin
        );
    }

    #[test]
    fn parse_basis_code_round_trips_and_rejects_unknown() {
        assert_eq!(
            parse_basis_code("specifier_matches_package_dependency"),
            Some(UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency)
        );
        assert_eq!(parse_basis_code("some_future_basis_code"), None);
        assert_eq!(parse_basis_code(""), None);
    }

    #[test]
    fn breakdown_folds_multiple_bases_into_class_totals() {
        // The three external bases collapse to ONE ExternalDependency class total (2+3+1=6);
        // the named/unidentified split is NOT computed here (it comes from the storage
        // provenance join). One stdlib = 4. Count-desc ⇒ external first.
        let rows = [
            ("specifier_matches_package_dependency", 2u64),
            ("receiver_matches_external_import", 3),
            ("callee_matches_external_import", 1),
            ("specifier_matches_runtime_module", 4),
        ];
        let b = attribution_breakdown(rows.iter().map(|&(c, n)| (c, n)));
        assert_eq!(
            b.classes,
            vec![
                (AttributionClass::ExternalDependency, 6),
                (AttributionClass::StandardLibrary, 4),
            ]
        );
        assert_eq!(b.other, 0);
        assert!(!b.is_empty());
    }

    #[test]
    fn breakdown_folds_unrecognized_code_into_other_and_skips_zeroes() {
        let rows = [
            ("no_supporting_signal", 5u64),
            ("some_future_basis_code", 3),
            ("callee_matches_runtime_global", 0),
        ];
        let b = attribution_breakdown(rows.iter().map(|&(c, n)| (c, n)));
        assert_eq!(b.classes, vec![(AttributionClass::Unattributed, 5)]);
        assert_eq!(b.other, 3, "unknown code preserved in the other bucket");
    }

    #[test]
    fn breakdown_of_empty_rows_is_empty() {
        let empty: [(&str, u64); 0] = [];
        assert!(attribution_breakdown(empty).is_empty());
    }
}
