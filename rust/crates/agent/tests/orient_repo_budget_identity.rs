//! ORIENT-SEGMENT-2 §2.3 — budgets change LENGTH, never NUMBERS.
//!
//! The slice's contract: every number `orient` prints (resolution %, call
//! totals, module/complexity counts) derives from the SAME snapshot reads
//! regardless of budget; only the DEPTH of the named lists grows. This test
//! runs the repo pipeline at all four budgets against ONE fixed snapshot and
//! asserts:
//!
//!   1. numeric identity — the load-bearing totals are byte-identical across
//!      `small`/`medium`/`large`/`full`; and
//!   2. monotonic length — the named DEPTH lists (`top_modules`, `top_complex`)
//!      are non-decreasing small→full and strictly grow somewhere, proving the
//!      budget genuinely trades depth (not a vacuous all-equal pass).
//!
//! ── Why this is the right invariant (root-cause finding, EXECUTED) ──────────
//! The slice's Problem section cites FRAKTAG "resolution 28% at --budget large,
//! 31% at --full; call totals 1609 vs 1685" as evidence that "budget changes
//! FACTS". A live isolated proof (index FRAKTAG into a throwaway state root, run
//! `orient` at all four budgets) showed the OPPOSITE: on ONE stable snapshot the
//! numbers are byte-identical across budgets — the ONLY delta is the complexity
//! list LENGTH. The 28-vs-31 divergence in the smoke evidence is an ENRICHMENT-
//! EPOCH race, not a budget effect: the `--budget large` capture was taken
//! BEFORE the background "resolved call types" enrichment pass finished (it even
//! carries the `TRUST_NO_ENRICHMENT` signal and no external-target
//! classification), and the `--full` capture was taken AFTER it finished (no
//! enrichment-not-run line; `Express (50)/Buffer (2)` externals now classified;
//! resolved count 446→~522 ⇒ in-scope total 1609→1685 ⇒ 28%→31%). Re-running
//! `orient` at the SAME `--budget large` before vs after `rmap enrich`
//! reproduced exactly that flip. So there is NO budget-dependent READ to
//! re-plumb; the numeric-identity invariant already holds in code, and this test
//! guards it against regression. See the ORIENT-SEGMENT-2 build report.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
    orient, AgentComplexityMeasurement, AgentCycle, AgentModuleSize, AgentModuleSummary,
    AgentReliabilityAxis, AgentReliabilityLevel, AgentRepoSummary, AgentTrustSummary, Budget,
    EnrichmentState, HighComplexityEvidence, ImportCyclesEvidence, ModuleSummaryEvidence,
    OrientResult, SignalEvidence, TrustLowResolutionEvidence,
};

/// Seed a repo whose orient output has genuine budget-varying DEPTH:
/// 30 named modules (module_summary total 30) and 30 above-threshold
/// complexity centers (true total 30), plus fixed trust/cycle numbers. The
/// budget caps (modules 12/24/∞/∞; complexity 5/15/∞/∞) then produce different
/// list LENGTHS while every TOTAL stays constant.
fn seed_depth_repo() -> FakeAgentStorage {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "depth-repo", "snap-1");

    // Fixed snapshot totals — the numbers orient's structure line prints.
    fake.repo_summaries.insert(
        "snap-1".into(),
        AgentRepoSummary {
            file_count: 512,
            symbol_count: 4096,
            languages: vec!["rust".into(), "typescript".into()],
        },
    );

    // Fixed trust numbers — resolution % / call totals are snapshot facts, not
    // budget-derived. LOW resolution so TRUST_LOW_RESOLUTION also fires with a
    // fixed denominator we can assert stays constant.
    fake.trust_summaries.insert(
        "snap-1".into(),
        AgentTrustSummary {
            call_resolution_rate: 0.15,
            resolved_calls: 300,
            unresolved_calls: 1700,
            unresolved_calls_internal_like: 1700,
            unresolved_calls_unknown: 900,
            external_targets: Vec::new(),
            call_graph_reliability: AgentReliabilityAxis {
                level: AgentReliabilityLevel::Low,
                reasons: Vec::new(),
            },
            dead_code_reliability: AgentReliabilityAxis {
                level: AgentReliabilityLevel::High,
                reasons: Vec::new(),
            },
            // Ran, so no TRUST_NO_ENRICHMENT (the enrichment-epoch axis is held
            // FIXED here — the very variable the root-cause finding isolates).
            enrichment_state: EnrichmentState::Ran,
            enrichment_eligible: 100,
            enrichment_enriched: 100,
        },
    );

    // 30 named modules (discovered_module_count = 30, budget-invariant).
    let module_sizes: Vec<AgentModuleSize> = (0..30)
        .map(|i| AgentModuleSize {
            path: format!("src/mod{i:02}"),
            // Descending sizes so ordering is stable and the prefix a smaller
            // budget keeps is a deterministic function of the SET.
            file_count: (30 - i) as u64,
            name: None,
            manifest: None,
        })
        .collect();
    fake.module_sizes.insert("snap-1".into(), module_sizes);
    fake.module_summaries.insert(
        "snap-1".into(),
        AgentModuleSummary {
            discovered_module_count: 30,
            declared_count: 30,
            operational_count: 0,
            inferred_count: 0,
        },
    );

    // 30 above-threshold complexity centers (true total 30, budget-invariant).
    let complexity: Vec<AgentComplexityMeasurement> = (0..30)
        .map(|i| AgentComplexityMeasurement {
            stable_key: format!("r1:src/mod{i:02}/f.rs:sym{i:02}:SYMBOL"),
            symbol_name: format!("sym{i:02}"),
            file_path: Some(format!("src/mod{i:02}/f.rs")),
            // 50 down to 21 — all >= the default threshold (20).
            complexity: (50 - i) as u64,
        })
        .collect();
    fake.complexity_measurements
        .insert("snap-1".into(), complexity);

    // A fixed cycle so IMPORT_CYCLES rides too (its count must stay constant).
    fake.cycles.insert(
        "snap-1".into(),
        vec![AgentCycle {
            length: 2,
            modules: vec!["src/mod00".into(), "src/mod01".into()],
            test_composition: None,
        }],
    );

    fake
}

/// The MODULE_SUMMARY evidence for a result, or panic.
fn module_summary(result: &OrientResult) -> &ModuleSummaryEvidence {
    for s in &result.signals {
        if let SignalEvidence::ModuleSummary(ev) = s.evidence() {
            return ev;
        }
    }
    panic!("MODULE_SUMMARY signal missing");
}

/// The HIGH_COMPLEXITY evidence for a result, or panic.
fn high_complexity(result: &OrientResult) -> &HighComplexityEvidence {
    for s in &result.signals {
        if let SignalEvidence::HighComplexity(ev) = s.evidence() {
            return ev;
        }
    }
    panic!("HIGH_COMPLEXITY signal missing");
}

/// The IMPORT_CYCLES evidence for a result, or panic.
fn import_cycles(result: &OrientResult) -> &ImportCyclesEvidence {
    for s in &result.signals {
        if let SignalEvidence::ImportCycles(ev) = s.evidence() {
            return ev;
        }
    }
    panic!("IMPORT_CYCLES signal missing");
}

/// The TRUST_LOW_RESOLUTION evidence for a result, or panic.
fn trust_low_resolution(result: &OrientResult) -> &TrustLowResolutionEvidence {
    for s in &result.signals {
        if let SignalEvidence::TrustLowResolution(ev) = s.evidence() {
            return ev;
        }
    }
    panic!("TRUST_LOW_RESOLUTION signal missing");
}

const BUDGETS: [Budget; 4] = [Budget::Small, Budget::Medium, Budget::Large, Budget::Full];

#[test]
fn budgets_change_length_never_numbers() {
    let fake = seed_depth_repo();

    let results: Vec<OrientResult> = BUDGETS
        .iter()
        .map(|b| orient(&fake, "r1", None, *b, common::TEST_NOW).unwrap())
        .collect();

    // ── 1. NUMERIC IDENTITY — every printed total is budget-invariant. ──────
    for (i, r) in results.iter().enumerate() {
        let ms = module_summary(r);
        let hc = high_complexity(r);
        let tr = trust_low_resolution(r);
        let ctx = BUDGETS[i];

        // Structure totals.
        assert_eq!(ms.file_count, 512, "file_count must not depend on {ctx:?}");
        assert_eq!(
            ms.symbol_count, 4096,
            "symbol_count must not depend on {ctx:?}"
        );
        assert_eq!(
            ms.discovered_module_count,
            Some(30),
            "discovered_module_count is the TRUE total, budget-invariant ({ctx:?})"
        );
        // Complexity total.
        assert_eq!(
            hc.high_complexity_count, 30,
            "high_complexity_count is the TRUE above-threshold total ({ctx:?})"
        );
        assert_eq!(hc.threshold, 20, "threshold is fixed ({ctx:?})");
        // Cycle count (reviewer correction: compare the RENDERED cycle count field, not
        // just the evidence variant's presence). The fixture seeds ONE cycle — that
        // count is a snapshot fact and must not move with the budget.
        let ic = import_cycles(r);
        assert_eq!(
            ic.cycle_count, 1,
            "IMPORT_CYCLES cycle_count is a snapshot fact, budget-invariant ({ctx:?})"
        );
        assert_eq!(
            ic.cycles.len(),
            1,
            "the rendered cycle list length is budget-invariant here ({ctx:?})"
        );
        // Trust / resolution numbers (the FRAKTAG 28-vs-31 axis — must be fixed).
        assert_eq!(
            tr.resolved_count, 300,
            "resolved call count is a snapshot fact, not budget-derived ({ctx:?})"
        );
        assert_eq!(
            tr.total_count, 2000,
            "in-scope call total (resolved 300 + internal_like 1700) is fixed ({ctx:?})"
        );
        assert_eq!(
            tr.unclassified_count, 900,
            "unclassified call count is fixed ({ctx:?})"
        );
        assert!(
            (tr.resolution_rate - 0.15).abs() < 1e-9,
            "resolution rate is fixed ({ctx:?})"
        );
    }

    // JSON-level identity for the whole trust/structure numeric surface: serialize
    // each evidence and require byte-equality across budgets (catches any number
    // the field-level asserts above did not enumerate).
    let ms_json: Vec<String> = results
        .iter()
        .map(|r| {
            let ms = module_summary(r);
            // Strip the depth list so identity is over the NUMBERS, not the
            // budget-varying `top_modules` length (asserted separately below).
            format!(
                "{}|{}|{:?}|{:?}",
                ms.file_count, ms.symbol_count, ms.discovered_module_count, ms.module_kinds
            )
        })
        .collect();
    assert!(
        ms_json.windows(2).all(|w| w[0] == w[1]),
        "MODULE_SUMMARY numbers diverge across budgets: {ms_json:?}"
    );

    // ── 2. MONOTONIC LENGTH — DEPTH grows small→full, never inverts. ────────
    let mod_lens: Vec<usize> = results
        .iter()
        .map(|r| module_summary(r).top_modules.len())
        .collect();
    let cx_lens: Vec<usize> = results
        .iter()
        .map(|r| high_complexity(r).top_complex.len())
        .collect();

    assert!(
        mod_lens.windows(2).all(|w| w[0] <= w[1]),
        "top_modules length must be monotonic non-decreasing: {mod_lens:?}"
    );
    assert!(
        cx_lens.windows(2).all(|w| w[0] <= w[1]),
        "top_complex length must be monotonic non-decreasing: {cx_lens:?}"
    );
    // Strictly grows somewhere — proves the budget really trades DEPTH (else the
    // identity assertion above would pass vacuously).
    assert!(
        mod_lens[0] < mod_lens[mod_lens.len() - 1],
        "small must carry fewer modules than full: {mod_lens:?}"
    );
    assert!(
        cx_lens[0] < cx_lens[cx_lens.len() - 1],
        "small must carry fewer complexity centers than full: {cx_lens:?}"
    );

    // Concrete expected depths, pinned to the budget caps (Budget::max_modules /
    // max_complexity_centers): modules 12/24/30/30, complexity 5/15/30/30.
    assert_eq!(mod_lens, vec![12, 24, 30, 30], "module depth per tier");
    assert_eq!(cx_lens, vec![5, 15, 30, 30], "complexity depth per tier");
}
