//! Promotion funnel taxonomy + aggregation (ENRICH-YIELD-1).
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the reader-facing taxonomy of the promotion filter's rejections — for each
//!   first-rejection reason the 8-gate filter ([`crate::promotion`]) can emit, its gate number, its
//!   stable machine code, and a *reader-frame* label ("type resolves to 2+ classes (ambiguous)", not
//!   "gate 5") — PLUS the pure aggregator ([`PromotionFunnel`]) that turns a promotion run's
//!   candidate/promoted/first-rejection counts into a conserved, deterministically-ordered breakdown.
//! - **Concrete current users:** [`crate::promotion::promote_edges`] (its `skip` records a
//!   [`RejectionClass`], the single source of truth for the reason strings), and
//!   [`crate::status::PromotionReport::funnel`], which the daemon's enrichment pass + the manual
//!   `rmap enrich` handler read to put the funnel on the product surface (doctor line + detail; the
//!   `enrich` `promotion` JSON).
//! - **Named axis of variation:** the *reader-facing* rejection vocabulary (labels, gate grouping)
//!   is a distinct concern from the *filter mechanism* (the gate predicates). Labels change for
//!   reader-context reasons (VISION: "labels speak the reader's language"); predicates change for
//!   correctness reasons — different actors, different volatility. Splitting them also keeps
//!   `promotion.rs` (already over the 500-line structural guardrail) from absorbing this.
//! - **Rejected simpler alternative:** a free `fn (reason_str) -> (gate, label)` reverse-map inside
//!   `promotion.rs`. Rejected because a string→gate table drifts silently from the filter it
//!   describes (the exact "a name is not its semantics" trap): the enum makes `gate()`/`reader_label()`
//!   an *exhaustive `match`*, so a future gate/reason cannot compile without being classified.
//!
//! # Certainty
//!
//! The funnel is a Layer-1 extracted fact ABOUT the promotion pass: deterministic counts of a
//! deterministic filter. It is not itself a promotion decision and changes nothing about what gets
//! promoted — it only makes the already-computed rejections visible. Conservation
//! (`candidates == promoted + rejected`) is an invariant of the filter (each candidate is promoted
//! xor skipped exactly once), asserted by [`PromotionFunnel::conserves`].

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// The first-rejecting gate a promotion candidate hit, as a reader-facing class.
///
/// The variants are 1:1 with the reason strings [`crate::promotion::promote_edges`] records in
/// `skipped_reasons` — this enum owns those strings ([`RejectionClass::reason_code`]) so the filter
/// and the taxonomy cannot disagree on spelling. `gate()` groups them by the 8-gate docstring number
/// (a machine field for the per-gate analysis, never rendered as "gate N" to the reader);
/// `reader_label()` is the reader-frame text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectionClass {
    /// Gate 1 — the unresolved edge is not an object-method call the filter resolves.
    WrongCategory,
    /// Gate 3 — the receiver type was not resolved by the compiler (resolution failed). Gate 2 (the
    /// config-opt-in placeholder) never rejects, so the resolution-failed case is attributed here.
    NoCompilerEnrichment,
    /// Gate 4 — the resolved receiver type is external to this repo (a library/crate type).
    ExternalType,
    /// Gate 4 (usable-type precondition) — resolved, but no usable type name was produced.
    NoTypeName,
    /// Gate 7 — the receiver type is a union/intersection of 2+ types.
    UnionOrIntersection,
    /// Gate 8 — the call uses optional-chaining or index access, not a plain method call.
    OptionalOrElementAccess,
    /// Gate 8 — the call chain is deeper than `receiver.method` / `this.field.method`.
    NotSimpleReceiverMethod,
    /// Gate 5 — the receiver type is not a type defined in this repo.
    TypeNotInGraph,
    /// Gate 5 — the receiver type resolves to a non-class symbol.
    TypeNotAClass,
    /// Gate 5 — the type resolves to 2+ classes (ambiguous).
    AmbiguousClassMultipleDefinitions,
    /// Gate 6 — the method is not defined on the resolved class.
    MethodNotFoundOnClass,
    /// Gate 6 — the method is overloaded on the class (2+ definitions).
    AmbiguousMethodOverloaded,
}

impl RejectionClass {
    /// Every class, in filter-evaluation order — the iteration source for coverage tests and the
    /// classification round-trip. (Not sorted for display; the funnel sorts by count.)
    pub const ALL: [RejectionClass; 12] = [
        Self::WrongCategory,
        Self::NoCompilerEnrichment,
        Self::ExternalType,
        Self::NoTypeName,
        Self::UnionOrIntersection,
        Self::OptionalOrElementAccess,
        Self::NotSimpleReceiverMethod,
        Self::TypeNotInGraph,
        Self::TypeNotAClass,
        Self::AmbiguousClassMultipleDefinitions,
        Self::MethodNotFoundOnClass,
        Self::AmbiguousMethodOverloaded,
    ];

    /// The stable machine code — the exact string `promote_edges` records in `skipped_reasons`. This
    /// is the ONE definition of each reason string; the filter uses it, so the two cannot drift.
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::WrongCategory => "wrong_category",
            Self::NoCompilerEnrichment => "no_compiler_enrichment",
            Self::ExternalType => "external_type",
            Self::NoTypeName => "no_type_name",
            Self::UnionOrIntersection => "union_or_intersection",
            Self::OptionalOrElementAccess => "optional_or_element_access",
            Self::NotSimpleReceiverMethod => "not_simple_receiver_method",
            Self::TypeNotInGraph => "type_not_in_graph",
            Self::TypeNotAClass => "type_not_a_class",
            Self::AmbiguousClassMultipleDefinitions => "ambiguous_class_multiple_definitions",
            Self::MethodNotFoundOnClass => "method_not_found_on_class",
            Self::AmbiguousMethodOverloaded => "ambiguous_method_overloaded",
        }
    }

    /// The 8-gate docstring number this rejection belongs to (a machine grouping key for the per-gate
    /// analysis — NOT a reader-facing label). Several reasons share a gate (gate 5 has three, gate 8
    /// has two); that is intentional — the gate groups, the class discriminates.
    pub fn gate(self) -> u8 {
        match self {
            Self::WrongCategory => 1,
            Self::NoCompilerEnrichment => 3,
            Self::ExternalType | Self::NoTypeName => 4,
            Self::TypeNotInGraph
            | Self::TypeNotAClass
            | Self::AmbiguousClassMultipleDefinitions => 5,
            Self::MethodNotFoundOnClass | Self::AmbiguousMethodOverloaded => 6,
            Self::UnionOrIntersection => 7,
            Self::OptionalOrElementAccess | Self::NotSimpleReceiverMethod => 8,
        }
    }

    /// The reader-frame label — describes the reader's own call/type, not our pipeline (VISION:
    /// "labels speak the reader's language, not ours"). Never says "gate N", "unresolved", etc.
    pub fn reader_label(self) -> &'static str {
        match self {
            Self::WrongCategory => "call isn't an object-method call we resolve",
            Self::NoCompilerEnrichment => "receiver type was not resolved by the compiler",
            Self::ExternalType => "receiver type is external to this repo (a library type)",
            Self::NoTypeName => "resolved receiver has no usable type name",
            Self::UnionOrIntersection => "receiver type is a union/intersection of 2+ types",
            Self::OptionalOrElementAccess => {
                "call uses optional-chaining or index access, not a plain method call"
            }
            Self::NotSimpleReceiverMethod => "call chain is deeper than receiver.method",
            Self::TypeNotInGraph => "receiver type isn't a type defined in this repo",
            Self::TypeNotAClass => "receiver type resolves to a non-class symbol",
            Self::AmbiguousClassMultipleDefinitions => "type resolves to 2+ classes (ambiguous)",
            Self::MethodNotFoundOnClass => "method isn't defined on the resolved class",
            Self::AmbiguousMethodOverloaded => "method is overloaded on the class (2+ definitions)",
        }
    }

    /// Recover the class from a `skipped_reasons` machine code, or `None` if it is not a known
    /// promotion-rejection code (defensive: the funnel surfaces an unknown code honestly rather than
    /// dropping it — see [`PromotionFunnel::from_counts`]).
    pub fn classify(code: &str) -> Option<RejectionClass> {
        Self::ALL.into_iter().find(|c| c.reason_code() == code)
    }
}

/// The promotion filter's gate stages — ALL EIGHT documented gates (`docs/TECH-DEBT.md § 8-Gate
/// Promotion Filter`), **in the order [`crate::promotion::promote_edges`] evaluates them** — and that
/// is deliberately NOT `1..=8`. The filter runs the cheap syntactic gates (7 union/intersection, 8
/// call shape) BEFORE the graph-lookup gates (5 unique class, 6 unique method), because 5/6 need a
/// symbol-table query and 7/8 are string checks. So the honest evaluation order is
/// **1 → 2 → 3 → 4 → 7 → 8 → 5 → 6**, and "candidates entering gate N" only forms a conserving
/// waterfall in THIS order (see [`PromotionFunnel::conserves`]).
///
/// Gate 2 ("config opt-in") is a NO-OP placeholder — no config surface gates promotion today, so it
/// rejects nothing (no [`RejectionClass`] maps to it). It is still its own stage so the waterfall
/// shows the complete per-gate (1–8) accounting the slice requires: its `entered` mirrors gate 1's
/// survivors and its `rejected` is always 0 ("N reached → 0 filtered out here"), which is the honest
/// rendering of a gate that filters nothing.
///
/// Each entry is `(gate_number, reader-frame stage label)`. The `u8` is the `TECH-DEBT` gate number —
/// a machine grouping key, keyed identically to [`RejectionClass::gate`], never rendered as "gate N"
/// to the reader. The label describes, in the reader's own terms, what PASSING the gate means about
/// *their* call (VISION: "labels speak the reader's language, not ours").
///
/// INVARIANT: this order MUST match `promote_edges`' actual check order. It is not free to drift — a
/// mismatch makes the ground-truth `entered` counts (recorded live by the pass, keyed by gate number)
/// fail the waterfall conservation check in [`PromotionFunnel::conserves`], which the
/// `promotion::tests` funnel tests assert over the real filter. So a reorder that forgets this table
/// breaks a test rather than silently mislabeling the funnel.
const GATE_STAGES_IN_EVAL_ORDER: [(u8, &str); 8] = [
    (1, "call is a method call whose receiver type we resolve"),
    (2, "resolving this kind of call is enabled"),
    (3, "receiver type was resolved by the compiler"),
    (
        4,
        "receiver type is defined in this repo (not a library type)",
    ),
    (
        7,
        "receiver type is a single type (not a union/intersection)",
    ),
    (
        8,
        "call is a direct receiver.method (no chaining or indexing)",
    ),
    (5, "receiver type maps to exactly one class we can see"),
    (6, "the called method is uniquely defined on that class"),
];

/// One gate's slice of the promotion waterfall (ENRICH-YIELD-1 §2.1) — how many candidates REACHED
/// this gate and how many it FIRST-rejected. Presented in evaluation order (see
/// [`GATE_STAGES_IN_EVAL_ORDER`]), so `entered - rejected` of one stage equals the `entered` of the
/// next, down to `promoted`. The DTO the daemon puts on the product surface beside the per-class
/// [`RejectionTally`] breakdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateStage {
    /// The `TECH-DEBT` gate number (machine field for analysis; never rendered as "gate N").
    pub gate: u8,
    /// Reader-frame description of what PASSING this gate means about the reader's call/type.
    pub label: String,
    /// Candidates that REACHED this gate — ground truth, counted live by the pass as each candidate
    /// arrives (not derived), so the per-gate accounting cannot silently drift from the filter.
    pub entered: usize,
    /// Candidates this gate FIRST-rejected (= Σ of this gate's [`RejectionClass`] first-rejections).
    pub rejected: usize,
}

/// One rejection class's contribution to the funnel — reader-frame label + machine code + gate +
/// count. The DTO the daemon puts on the product surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectionTally {
    /// Stable machine code (the `skipped_reasons` key) — for keying/analysis, not display.
    pub reason: String,
    /// The 8-gate grouping number (machine field; never rendered as "gate N" text).
    pub gate: u8,
    /// Reader-frame label — the text a surface shows the reader.
    pub label: String,
    /// How many candidates were first-rejected by this class.
    pub count: usize,
}

/// The promotion funnel: candidates in → promoted out, with the first-rejection breakdown of the
/// rest. A pure aggregation of a promotion run's counts; deterministic ordering; conservation
/// checkable ([`Self::conserves`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionFunnel {
    /// Candidates that entered the filter. These are the **resolved** candidates: the loader
    /// (`load_promotion_candidates`) pre-filters to `origin = compiler` with a receiver type, so
    /// `candidates` IS the "total resolved" set — the denominator of the 3.5% (`promoted /
    /// candidates`). Gates 1–3 therefore reject ~nothing in production; they are defensive.
    pub candidates: usize,
    /// Candidates promoted to resolved call-graph edges (all gates passed).
    pub promoted: usize,
    /// Candidates rejected (= sum of `rejections[*].count`); `candidates - promoted` when conserved.
    pub rejected: usize,
    /// Per-**class** first-rejection tallies, dominant first (count desc, then gate asc, then code
    /// asc) — the "why did it not promote" view + the headline's top rejections.
    pub rejections: Vec<RejectionTally>,
    /// Per-**gate** waterfall (ENRICH-YIELD-1 §2.1) in EVALUATION order: for each gate, how many
    /// candidates reached it (`entered`, ground truth from the pass) and how many it first-rejected.
    /// Empty when the pass recorded no per-gate entries (zero-work, or a flat-only construction), so
    /// a fabricated waterfall is never shown. See [`GateStage`] / [`GATE_STAGES_IN_EVAL_ORDER`].
    pub gates: Vec<GateStage>,
}

impl PromotionFunnel {
    /// Build the funnel from a promotion run's `candidates`/`promoted` totals, its first-rejection
    /// `skipped_reasons` map (reason code → count), and its ground-truth `gate_entered` map (gate
    /// number → candidates that reached that gate, recorded live by [`crate::promotion::promote_edges`]).
    ///
    /// Each `skipped_reasons` key is classified via [`RejectionClass::classify`]; an unrecognized key
    /// (should never occur — `promote_edges` only emits [`RejectionClass`] codes) is surfaced honestly
    /// with `gate = 0` and its raw code as the label, NEVER dropped, so a miscount can be seen rather
    /// than hidden. Both the per-class ordering and the per-gate waterfall are deterministic (VISION:
    /// same input → same output).
    ///
    /// `gate_entered` empty → `gates` empty (no ground-truth entry accounting to show). In production
    /// the pass always supplies it, so a real run always carries the waterfall.
    pub fn from_counts(
        candidates: usize,
        promoted: usize,
        skipped_reasons: &HashMap<String, usize>,
        gate_entered: &BTreeMap<u8, usize>,
    ) -> Self {
        let mut rejections: Vec<RejectionTally> = skipped_reasons
            .iter()
            .filter(|(_, &count)| count > 0)
            .map(|(code, &count)| match RejectionClass::classify(code) {
                Some(class) => RejectionTally {
                    reason: class.reason_code().to_string(),
                    gate: class.gate(),
                    label: class.reader_label().to_string(),
                    count,
                },
                None => RejectionTally {
                    reason: code.clone(),
                    gate: 0,
                    label: code.clone(),
                    count,
                },
            })
            .collect();

        // Dominant first, then a stable tiebreak so the order is fully determined.
        rejections.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then(a.gate.cmp(&b.gate))
                .then(a.reason.cmp(&b.reason))
        });

        let rejected = rejections.iter().map(|r| r.count).sum();

        // Per-gate waterfall, in evaluation order. `entered` is ground truth (from the pass); each
        // gate's `rejected` is the sum of the first-rejection classes that belong to it (grouped by
        // `RejectionClass::gate`, the SAME mapping — one source of truth). Empty gate_entered (zero
        // work / flat-only) → no waterfall.
        let gates: Vec<GateStage> = if gate_entered.is_empty() {
            Vec::new()
        } else {
            GATE_STAGES_IN_EVAL_ORDER
                .iter()
                .map(|&(gate, label)| GateStage {
                    gate,
                    label: label.to_string(),
                    entered: gate_entered.get(&gate).copied().unwrap_or(0),
                    rejected: RejectionClass::ALL
                        .iter()
                        .filter(|c| c.gate() == gate)
                        .map(|c| skipped_reasons.get(c.reason_code()).copied().unwrap_or(0))
                        .sum(),
                })
                .collect()
        };

        Self {
            candidates,
            promoted,
            rejected,
            rejections,
            gates,
        }
    }

    /// The conservation invariant. The flat form: every candidate is promoted xor rejected exactly
    /// once, so `candidates == promoted + rejected`. The per-gate form (when the waterfall is present):
    /// gate 1 is entered by every candidate; each gate's survivors (`entered - rejected`) are the next
    /// gate's `entered`; the last gate's survivors are `promoted`. Because `entered` is ground truth
    /// and the stages are laid out in [`GATE_STAGES_IN_EVAL_ORDER`], this waterfall check FAILS if that
    /// table ever drifts from `promote_edges`' real check order — turning a silent mislabel into a
    /// test failure (the `promotion::tests` funnel tests assert this over the real filter).
    pub fn conserves(&self) -> bool {
        if self.candidates != self.promoted + self.rejected {
            return false;
        }
        let Some(first) = self.gates.first() else {
            return true; // flat-only funnel (no waterfall to check)
        };
        if first.entered != self.candidates {
            return false;
        }
        for pair in self.gates.windows(2) {
            if pair[0].entered.checked_sub(pair[0].rejected) != Some(pair[1].entered) {
                return false;
            }
        }
        let last = self.gates.last().expect("non-empty: first matched");
        last.entered.checked_sub(last.rejected) == Some(self.promoted)
    }

    /// The top `n` rejection classes (already sorted dominant-first) — the headline "top rejecting
    /// gates" for the completion report.
    pub fn top(&self, n: usize) -> &[RejectionTally] {
        let take = n.min(self.rejections.len());
        &self.rejections[..take]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pairs: &[(&str, usize)]) -> HashMap<String, usize> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn entered(pairs: &[(u8, usize)]) -> BTreeMap<u8, usize> {
        pairs.iter().map(|(k, v)| (*k, *v)).collect()
    }

    // Every class round-trips through its machine code — the single-source-of-truth guarantee the
    // reverse-map alternative could not give. If a future variant is added without a code, ALL and
    // classify() disagree and this fails.
    #[test]
    fn every_class_classifies_from_its_own_code() {
        for class in RejectionClass::ALL {
            assert_eq!(
                RejectionClass::classify(class.reason_code()),
                Some(class),
                "class {class:?} must classify from its own reason_code()"
            );
        }
    }

    // Reader-frame discipline (VISION): no external label narrates our pipeline. This is the guard
    // that keeps "gate 5" / "unresolved" / "enrichment phase" out of the reader's view.
    #[test]
    fn reader_labels_speak_the_readers_language() {
        for class in RejectionClass::ALL {
            let label = class.reader_label();
            let lower = label.to_lowercase();
            for banned in ["gate ", "unresolved", "enrichment", "promot", "candidate"] {
                assert!(
                    !lower.contains(banned),
                    "class {class:?} label narrates our pipeline (contains {banned:?}): {label}"
                );
            }
        }
    }

    // Codes are unique (no two variants share a machine key) and gates are all in the 1..=8 range.
    #[test]
    fn codes_are_unique_and_gates_in_range() {
        let mut seen = std::collections::HashSet::new();
        for class in RejectionClass::ALL {
            assert!(
                seen.insert(class.reason_code()),
                "duplicate code: {class:?}"
            );
            assert!(
                (1..=8).contains(&class.gate()),
                "gate out of range for {class:?}: {}",
                class.gate()
            );
        }
    }

    // Conservation + dominant-first ordering, the two invariants surfaces depend on.
    #[test]
    fn funnel_conserves_and_orders_dominant_first() {
        // 100 candidates, 40 promoted, 60 rejected across three classes.
        let skipped = counts(&[
            ("external_type", 30),
            ("method_not_found_on_class", 20),
            ("ambiguous_class_multiple_definitions", 10),
        ]);
        let f = PromotionFunnel::from_counts(100, 40, &skipped, &BTreeMap::new());

        assert_eq!(f.rejected, 60);
        assert!(f.conserves(), "40 promoted + 60 rejected == 100 candidates");
        // Dominant first.
        assert_eq!(f.rejections[0].reason, "external_type");
        assert_eq!(f.rejections[0].count, 30);
        assert_eq!(f.rejections[0].gate, 4);
        assert_eq!(
            f.rejections[0].label,
            "receiver type is external to this repo (a library type)"
        );
        assert_eq!(
            f.rejections[2].reason,
            "ambiguous_class_multiple_definitions"
        );
        assert_eq!(f.top(2).len(), 2);
    }

    // Deterministic tiebreak: equal counts order by gate asc, then code asc — same input, same output.
    #[test]
    fn equal_counts_break_ties_deterministically() {
        let skipped = counts(&[
            ("method_not_found_on_class", 5), // gate 6
            ("external_type", 5),             // gate 4
            ("type_not_in_graph", 5),         // gate 5
        ]);
        let f = PromotionFunnel::from_counts(20, 5, &skipped, &BTreeMap::new());
        let order: Vec<&str> = f.rejections.iter().map(|r| r.reason.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "external_type",
                "type_not_in_graph",
                "method_not_found_on_class"
            ],
            "equal counts sort by gate asc"
        );
    }

    // Honest zero-work: no candidates → an empty, conserved funnel (0 == 0 + 0), NOT a phantom class.
    #[test]
    fn zero_work_is_empty_and_conserves() {
        let f = PromotionFunnel::from_counts(0, 0, &HashMap::new(), &BTreeMap::new());
        assert_eq!(f.candidates, 0);
        assert_eq!(f.rejected, 0);
        assert!(f.rejections.is_empty());
        assert!(f.gates.is_empty(), "zero work → no per-gate waterfall");
        assert!(f.conserves());
        assert!(f.top(3).is_empty());
    }

    // An unknown code (defensive path) is surfaced with gate 0 + raw label, never dropped.
    #[test]
    fn unknown_code_is_surfaced_not_dropped() {
        let f = PromotionFunnel::from_counts(
            5,
            0,
            &counts(&[("some_new_reason", 5)]),
            &BTreeMap::new(),
        );
        assert_eq!(f.rejected, 5);
        assert_eq!(f.rejections[0].reason, "some_new_reason");
        assert_eq!(f.rejections[0].gate, 0);
        assert!(f.conserves());
    }

    // Zero-count entries are dropped (a reason present with count 0 is measured-absent, not shown).
    #[test]
    fn zero_count_entries_are_not_shown() {
        let f =
            PromotionFunnel::from_counts(3, 3, &counts(&[("external_type", 0)]), &BTreeMap::new());
        assert!(f.rejections.is_empty());
        assert!(f.conserves());
    }

    // ── Per-gate waterfall (ENRICH-YIELD-1 §2.1) ───────────────────────────────────────────────────

    // The waterfall is laid out in EVALUATION order (1, 2, 3, 4, 7, 8, 5, 6 — NOT 1..=8), each gate
    // carries ground-truth `entered` + its summed `rejected`, and the whole thing conserves
    // (entered − rejected of one gate == entered of the next, down to promoted).
    #[test]
    fn gate_waterfall_is_in_evaluation_order_and_conserves() {
        // 100 resolved candidates. Rejections: gate 4 external ×30, gate 7 union ×5, gate 5 ambiguous
        // ×15, gate 6 overloaded ×10 → 60 rejected, 40 promoted. Ground-truth entry counts follow the
        // real filter's order (7/8 before 5/6).
        let skipped = counts(&[
            ("external_type", 30),                        // gate 4
            ("union_or_intersection", 5),                 // gate 7
            ("ambiguous_class_multiple_definitions", 15), // gate 5
            ("ambiguous_method_overloaded", 10),          // gate 6
        ]);
        let gate_entered = entered(&[
            (1, 100),
            (2, 100), // config opt-in placeholder — no-op, mirrors gate 1's survivors
            (3, 100),
            (4, 100),
            (7, 70), // 100 − 30 external
            (8, 65), // 70 − 5 union
            (5, 65),
            (6, 50), // 65 − 15 ambiguous class
        ]);
        let f = PromotionFunnel::from_counts(100, 40, &skipped, &gate_entered);

        // Gates appear in EVALUATION order, not numeric order — all eight documented gates.
        let order: Vec<u8> = f.gates.iter().map(|g| g.gate).collect();
        assert_eq!(order, vec![1, 2, 3, 4, 7, 8, 5, 6], "gates in eval order");

        // Per-gate entered + rejected are ground truth / summed classes.
        let gate = |n: u8| f.gates.iter().find(|g| g.gate == n).unwrap();
        // Gate 2 is a no-op placeholder: entrants pass through, nothing filtered.
        assert_eq!((gate(2).entered, gate(2).rejected), (100, 0));
        assert_eq!((gate(4).entered, gate(4).rejected), (100, 30));
        assert_eq!((gate(7).entered, gate(7).rejected), (70, 5));
        assert_eq!((gate(8).entered, gate(8).rejected), (65, 0));
        assert_eq!((gate(5).entered, gate(5).rejected), (65, 15));
        assert_eq!((gate(6).entered, gate(6).rejected), (50, 10));

        // The waterfall conserves end-to-end (this is what catches an eval-order drift).
        assert!(f.conserves(), "per-gate waterfall conserves: {f:?}");
        // 50 entered gate 6, 10 rejected → 40 survive == promoted.
        assert_eq!(gate(6).entered - gate(6).rejected, f.promoted);
    }

    // A waterfall whose ground-truth `entered` does not chain (here: gate 7 claims MORE entrants than
    // gate 4 had survivors) is caught by conserves() — the guard that turns an eval-order table drift
    // into a failure rather than a silent mislabel.
    #[test]
    fn non_chaining_waterfall_fails_conservation() {
        let skipped = counts(&[("external_type", 30)]); // gate 4
        let bad = entered(&[
            (1, 100),
            (2, 100),
            (3, 100),
            (4, 100),
            (7, 80), // WRONG: should be 70 (100 − 30); 80 breaks the chain
            (8, 80),
            (5, 80),
            (6, 80),
        ]);
        let f = PromotionFunnel::from_counts(100, 70, &skipped, &bad);
        assert!(
            !f.conserves(),
            "a non-chaining waterfall must not report conserved"
        );
    }

    // Reader-frame discipline for the per-gate STAGE labels too (VISION): a stage label describes the
    // reader's call/type, never our pipeline ("gate N", "unresolved", …).
    #[test]
    fn gate_stage_labels_speak_the_readers_language() {
        for (_, label) in GATE_STAGES_IN_EVAL_ORDER {
            let lower = label.to_lowercase();
            for banned in ["gate ", "unresolved", "enrichment", "promot", "candidate"] {
                assert!(
                    !lower.contains(banned),
                    "stage label narrates our pipeline (contains {banned:?}): {label}"
                );
            }
        }
    }

    // The stage gate numbers are exactly the evaluation-order set the filter uses — and cover ALL
    // EIGHT documented gates (`docs/TECH-DEBT.md`). Gate 2 (the config-opt-in placeholder) is present
    // as a no-op stage: eval order is 1,2,3,4,7,8,5,6 (7/8 before 5/6), whose SET is {1..8}.
    #[test]
    fn gate_stage_numbers_are_the_evaluation_order_set() {
        let nums: Vec<u8> = GATE_STAGES_IN_EVAL_ORDER.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            nums,
            vec![1, 2, 3, 4, 7, 8, 5, 6],
            "gates in evaluation order"
        );

        // The complete ordered set of all eight documented gates is represented (review-1 item 1): the
        // sorted stage numbers are exactly 1..=8, none missing, none duplicated.
        let mut sorted = nums.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            (1u8..=8).collect::<Vec<_>>(),
            "all eight documented gates must appear as stages"
        );

        // Every RejectionClass gate is covered by some stage (so no class's rejections are orphaned
        // from the waterfall). Gate 2 has no class — it is a no-op placeholder that never rejects.
        for class in RejectionClass::ALL {
            assert!(
                nums.contains(&class.gate()),
                "class {class:?} (gate {}) has no stage in the waterfall",
                class.gate()
            );
        }
    }
}
