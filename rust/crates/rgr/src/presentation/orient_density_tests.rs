//! Progressive budget-ladder render tests for the dense `orient` command
//! (ORIENT-DENSITY-1, TECH-DEBT C5).
//!
//! A second `#[cfg(test)] #[path]` child of `orient` (the `trust_tests.rs` idiom),
//! split from `orient_tests.rs` so neither test file grows past the >500-line
//! structural guardrail (review-1). Reuses the shared `nginx_like` fixture from
//! the sibling `tests` module. Pins the 4-tier gradient
//! (small ⊂ medium ⊂ large ⊂ full) and the honesty posture kept at every tier.

use super::tests::nginx_like;
use super::*;

/// A richer nginx-shaped fixture: `nginx_like` (package-group topology, module
/// list, cycles, docs, degraded call-graph) PLUS limits + next steps and 80
/// named complexity centers, all carried in evidence (`high_complexity_count ==
/// len`). The 80 centers let the per-tier complexity caps (medium 10, large 50,
/// full ORIENT_FULL_LONG_TAIL_CAP=200) actually bite, so the ladder's progression
/// is observable; 80 < 200, so `--full` shows all 80 within its cap (no tail).
fn ladder_fixture() -> OrientResponse {
    use serde_json::json;
    let mut r = nginx_like();
    r.limits = vec![Limit {
        code: "GATE_NOT_CONFIGURED".to_string(),
        summary: "No active requirement declarations.".to_string(),
        reasons: Vec::new(),
    }];
    r.next = vec![NextAction {
        kind: "check".to_string(),
        repo: "nginx".to_string(),
        target: None,
        reason: "Verify current state.".to_string(),
    }];
    // 80 named centers, count == len, so `--full` shows all with NO "+N more"
    // while medium (10) and large (50) each carry an honest capped tail.
    let top: Vec<_> = (0..80)
        .map(|i| json!({"symbol": format!("fn{i}"), "file": format!("src/f{i}.c"), "complexity": 300 - i}))
        .collect();
    for leaf in &mut r.signals {
        if leaf.value.code == "HIGH_COMPLEXITY" {
            leaf.value.evidence = Some(json!({
                "high_complexity_count": 80, "threshold": 20, "top_complex": top
            }));
        }
    }
    r
}

#[test]
fn budget_ladder_is_progressive_small_medium_large() {
    // ORIENT-DENSITY-1 (TECH-DEBT C5): the budget tiers must be a genuine
    // progressive-disclosure gradient, not the old bimodal small≈medium≪large
    // cliff. Assert small < medium < large in rendered size, that medium carries
    // KEY STRUCTURE small omits, and that medium is a real middle (no 30× cliff).
    let r = ladder_fixture();
    let small = r.render_human(OrientDepth::Small);
    let medium = r.render_human(OrientDepth::Medium);
    let large = r.render_human(OrientDepth::Large);

    // 1. Strictly increasing rendered size.
    assert!(
        small.len() < medium.len(),
        "small ({}) must be smaller than medium ({})",
        small.len(),
        medium.len()
    );
    assert!(
        medium.len() < large.len(),
        "medium ({}) must be smaller than large ({})",
        medium.len(),
        large.len()
    );

    // 2. Medium carries the KEY STRUCTURE small omits: the package-group topology
    //    AND the declared/inferred module list (as dedicated sections).
    assert!(
        medium.contains("Package groups (directory/package topology"),
        "medium must add the package-groups list:\n{medium}"
    );
    assert!(
        medium.contains("Modules (declared/inferred, by size)"),
        "medium must add the module list:\n{medium}"
    );
    assert!(
        !small.contains("Package groups (directory/package topology"),
        "small omits the package-groups list (it is medium's structure):\n{small}"
    );
    assert!(
        !small.contains("Modules (declared/inferred, by size)"),
        "small omits the module list:\n{small}"
    );

    // 3. Medium is a REAL middle — well beyond the old bimodality (medium ≈ 1.5×
    //    small) — yet meaningfully smaller than large (the C5 finding was a ~32×
    //    medium→large jump; assert no 30× cliff).
    assert!(
        medium.len() > small.len() * 2,
        "medium must be a real middle, not ≈ small: small={}, medium={}",
        small.len(),
        medium.len()
    );
    assert!(
        large.len() < medium.len() * 30,
        "no 30× cliff between medium ({}) and large ({})",
        medium.len(),
        large.len()
    );
}

#[test]
fn medium_caps_complexity_large_expands_full_within_long_tail_cap() {
    // The complexity table's ladder: medium shows a scannable top-10 (honest "+N more"
    // tail), large a larger detailed table, `--full` up to ORIENT_FULL_LONG_TAIL_CAP=200
    // (ECONOMY-2 §2.3 — NOT uncapped). This fixture's 80 centers are within the 200 cap, so
    // `--full` shows all 80 with NO truncation tail. Proves large < full.
    let r = ladder_fixture();
    let medium = r.render_human(OrientDepth::Medium);
    let large = r.render_human(OrientDepth::Large);
    let full = r.render_human(OrientDepth::Full);

    // Medium: top-10 present (f9), the 11th (f10) NOT, honest capped tail (80-10).
    assert!(
        medium.contains("src/f9.c — fn9"),
        "medium shows the top-10:\n{medium}"
    );
    assert!(
        !medium.contains("src/f10.c — fn10"),
        "medium caps the complexity table at 10:\n{medium}"
    );
    assert!(
        medium.contains("+70 more above threshold (showing 10) — rmap hotspots"),
        "medium's capped table carries an honest tail STATING the bound:\n{medium}"
    );

    // Large: expands to top-50 (f49 present, f50 not), still an honest tail (80-50).
    assert!(
        large.contains("src/f49.c — fn49"),
        "large expands the detailed table to 50:\n{large}"
    );
    assert!(
        !large.contains("src/f50.c — fn50"),
        "large caps the detailed table at 50:\n{large}"
    );
    assert!(
        large.contains("+30 more above threshold (showing 50) — rmap hotspots"),
        "large's capped table carries an honest tail STATING the bound:\n{large}"
    );

    // Full: the 80 centers are within the 200-cap, so every center (f79) shows with NO
    // truncation tail (capped, but not reached here); large < full.
    assert!(
        full.contains("src/f79.c — fn79"),
        "full shows all 80 within the long-tail cap:\n{full}"
    );
    assert!(
        !full.contains("more above threshold"),
        "full has no truncation tail (80 < 200 cap — complete):\n{full}"
    );
    assert!(
        large.len() < full.len(),
        "large ({}) < full ({})",
        large.len(),
        full.len()
    );
}

#[test]
fn honesty_posture_present_at_every_tier() {
    // ORIENT-DENSITY-1 stop-condition: the honesty posture is load-bearing at
    // EVERY budget — the ladder trades DEPTH, never honesty. Assert the
    // reliability caveat (D1), the relationship next-action (D5), and the Serving
    // footer (D3) all render at small AND medium AND large. Driven through the
    // envelope wrapper (the Serving footer is a wrapper responsibility).
    let mut value = ladder_fixture(); // degraded call-graph → reliability caveat fires
    value.relationship_next_action = Some(
        "semantic resolution unavailable for C on this build; verify call/dead claims against source"
            .to_string(),
    );
    let env = CoherenceEnvelope::sqlite_leaf(value, false);

    for depth in [OrientDepth::Small, OrientDepth::Medium, OrientDepth::Large] {
        let out = render_orient_envelope(&env, depth);
        assert!(
            out.contains("Reliability: your code's calls 42% resolved (LOW)"),
            "reliability caveat (D1) dropped at {depth:?}:\n{out}"
        );
        assert!(
            out.contains("semantic resolution unavailable for C"),
            "relationship next-action (D5) dropped at {depth:?}:\n{out}"
        );
        assert!(
            out.contains("Serving"),
            "Serving footer (D3) dropped at {depth:?}:\n{out}"
        );
    }
}

/// MODULE-MODEL-2 §13 D7: on a >100-group tree the PRIMARY orientation surface
/// stays BOUNDED at every budget tier, the omission count is TRUE (it counts ALL
/// groups, not the displayed ones), and the headline count reflects the COMPLETE
/// set. Human bounding is presentation-only — the evidence (JSON) is untouched.
#[test]
fn d7_scale_bounded_human_true_omission_at_every_tier() {
    use serde_json::json;
    let mut r = nginx_like();
    // 130 package groups, size-DESC (g000 largest) — the fold's output shape.
    let groups: Vec<_> = (0..130)
        .map(|i| json!({"name": format!("g{i:03}"), "file_count": 1000 - i, "test_file_count": 0}))
        .collect();
    for leaf in &mut r.signals {
        if leaf.value.code == "MODULE_SUMMARY" {
            if let Some(ev) = leaf.value.evidence.as_mut() {
                ev["package_groups"] = json!(groups);
            }
        }
    }

    // The headline count is the COMPLETE total at EVERY tier — never the displayed count.
    for depth in [
        OrientDepth::Small,
        OrientDepth::Medium,
        OrientDepth::Large,
        OrientDepth::Full,
    ] {
        let out = r.render_human(depth);
        assert!(
            out.contains("130 package groups"),
            "headline count must be the complete total at {depth:?}:\n{out}"
        );
    }

    // SMALL: headline names a bounded set + "+N more" (the only pointer — no section).
    let small = r.render_human(OrientDepth::Small);
    assert!(
        small.contains("+122 more"),
        "small points to the rest (130-8):\n{small}"
    );
    assert!(
        !small.contains("Package groups (directory/package topology"),
        "small renders no dedicated section:\n{small}"
    );

    // MEDIUM: section capped at 20 + TRUE omission (130 - 20 = 110).
    let medium = r.render_human(OrientDepth::Medium);
    assert!(medium.contains("Package groups (directory/package topology"));
    assert!(
        medium.contains("… and 110 more groups (showing 20) — see `stats --json` / `modules`"),
        "medium omission must be true and STATE the bound:\n{medium}"
    );
    assert!(
        medium.contains("g000 — 1000 files"),
        "top group shown:\n{medium}"
    );
    assert!(
        medium.contains("g019 — 981 files"),
        "20th group shown:\n{medium}"
    );
    assert!(
        !medium.contains("g020 — "),
        "medium caps the section at 20:\n{medium}"
    );

    // LARGE: section capped at 50 + TRUE omission (130 - 50 = 80).
    let large = r.render_human(OrientDepth::Large);
    assert!(
        large.contains("… and 80 more groups (showing 50) — see `stats --json` / `modules`"),
        "large omission must be true and STATE the bound:\n{large}"
    );
    assert!(
        large.contains("g049 — "),
        "50th group shown at large:\n{large}"
    );
    assert!(
        !large.contains("g050 — "),
        "large caps the section at 50:\n{large}"
    );

    // FULL: ECONOMY-2 (§2.3) caps the package-group section at `ORIENT_FULL_LONG_TAIL_CAP`
    // (200) — SUPERSEDING ORIENT-SEGMENT-2 §2.4's earlier `--full` UNCAP (which measured as a
    // 314 KB dump). This fixture's 130 groups are BELOW the 200 cap, so `--full` still renders
    // EVERY group (all 130) with NO omission line — the cap simply is not reached here. Above
    // 200 the tail elides with the honest `… and N more groups (showing 200) — …` line
    // (proven in `orient_seg2_tests`). The headline still names the complete total and the JSON
    // evidence is untouched — human bounding is presentation-only.
    let full = r.render_human(OrientDepth::Full);
    assert!(
        !full.contains("more groups — see `stats --json`"),
        "130 groups < the 200 cap → full renders every group, no omission line:\n{full}"
    );
    assert!(
        full.contains("g050 — "),
        "51st group shown at full (large capped it):\n{full}"
    );
    assert!(
        full.contains("g129 — 871 files"),
        "last (130th) group shown at full — the complete breakdown:\n{full}"
    );
    // The headline still names the COMPLETE total (asserted in the tier loop above),
    // and the JSON evidence is untouched — human bounding is presentation-only.
}

/// MODULE-MODEL-2 ROOT-MANIFEST-POLYGLOT: when the daemon attaches the reader-frame
/// limitation marker (`root_manifest_limitation` in MODULE_SUMMARY evidence), the
/// orient package-groups SECTION renders it as a visible note — not hidden in a
/// comment. Present-half + absent-half in one test (the operator's named case,
/// render side). The section renders at medium+ (small omits it, riding JSON).
#[test]
fn root_manifest_limitation_marker_renders_in_package_groups_section() {
    use serde_json::json;
    let groups = json!([
        {"name": "alpha", "file_count": 9, "test_file_count": 0},
        {"name": "beta", "file_count": 4, "test_file_count": 0},
    ]);
    let marker = "root package.json not folded — nested toolchains present; \
                  root-owned directories shown as directory groups";

    // Present: evidence carries the marker → it renders in the section (medium+).
    let mut with = nginx_like();
    for leaf in &mut with.signals {
        if leaf.value.code == "MODULE_SUMMARY" {
            if let Some(ev) = leaf.value.evidence.as_mut() {
                ev["package_groups"] = groups.clone();
                ev["root_manifest_limitation"] = json!(marker);
            }
        }
    }
    let out = with.render_human(OrientDepth::Medium);
    let section = out
        .split("Package groups (directory/package topology")
        .nth(1)
        .expect("package-groups section present");
    let marker_pos = section
        .find("root package.json not folded")
        .expect("marker renders inside the section");
    let first_group = section.find("alpha — ").expect("group row present");
    assert!(
        marker_pos < first_group,
        "marker must precede the group rows:\n{section}"
    );

    // Absent: no marker field → no note (nothing suppressed, no noise).
    let mut without = nginx_like();
    for leaf in &mut without.signals {
        if leaf.value.code == "MODULE_SUMMARY" {
            if let Some(ev) = leaf.value.evidence.as_mut() {
                ev["package_groups"] = groups.clone();
            }
        }
    }
    let out = without.render_human(OrientDepth::Medium);
    assert!(
        !out.contains("not folded"),
        "no marker when evidence carries none:\n{out}"
    );
}

// ── ORIENT-SEGMENT-2 §2.3: budgets change LENGTH, never NUMBERS (rgr path) ──────

/// The agent-side guard (`agent/tests/orient_repo_budget_identity.rs`) proves the
/// USE CASE emits identical numbers across budgets. This companion proves the same
/// through the rgr PRESENTATION path (reviewer correction: "compare the RENDERED
/// numeric fields incl. the cycle counts, through the rgr presentation path"): the
/// load-bearing facts render byte-identically at every depth while the rendered
/// LENGTH grows monotonically. (FRAKTAG's 28-vs-31 was an enrichment-EPOCH change
/// between the audit's two captures — NOT a budget effect; see the agent test's
/// header and the build report for the reproduction.)
#[test]
fn rendered_numbers_are_budget_invariant_length_is_monotonic() {
    let r = nginx_like();
    let depths = [
        OrientDepth::Small,
        OrientDepth::Medium,
        OrientDepth::Large,
        OrientDepth::Full,
    ];
    let renders: Vec<String> = depths.iter().map(|d| r.render_human(*d)).collect();

    // 1. NUMERIC IDENTITY through rgr — every load-bearing fact (structure totals,
    // the top complexity value, and the CYCLE COUNT) renders identically at EVERY
    // depth. A budget that changed any of these would drop one of these substrings.
    for fact in [
        "397 file",        // file_count (structure line)
        "5000 symbol",     // symbol_count (structure line)
        "6 package group", // package-group total
        "cx 89",           // top complexity center value
        "3 import cycle",  // IMPORT_CYCLES cycle_count (the reviewer's named field)
    ] {
        for (i, out) in renders.iter().enumerate() {
            assert!(
                out.contains(fact),
                "fact {fact:?} must render at {:?} (numbers never depend on the budget):\n{out}",
                depths[i]
            );
        }
    }

    // 2. MONOTONIC LENGTH — depth trades DETAIL, never inverts, and genuinely grows.
    let lens: Vec<usize> = renders.iter().map(|s| s.len()).collect();
    assert!(
        lens.windows(2).all(|w| w[0] <= w[1]),
        "rendered length must be monotonic non-decreasing small->full: {lens:?}"
    );
    assert!(
        lens[0] < lens[lens.len() - 1],
        "small must render shorter than full (budget really trades depth): {lens:?}"
    );
}

/// ANCHORS-EVERYWHERE-1 (§4): the orient complexity BREAKDOWN row (SYMBOL-level `file — symbol`)
/// anchors `file:line` when a line is present, and renders the bare file when absent (never a
/// fabricated line). The file-deduped HEADLINE (`Complexity centers:`) stays unanchored — a file
/// rollup spans many symbols and has no single line.
#[test]
fn complexity_breakdown_anchors_line_headline_stays_unanchored() {
    use serde_json::json;
    let mut r = nginx_like();
    // Two centers in the SAME file (so the headline dedups to one file rollup): one carries a
    // line, one does not.
    let top = json!([
        {"symbol": "hot", "file": "src/a.c", "line": 42, "complexity": 40},
        {"symbol": "warm", "file": "src/a.c", "complexity": 30}
    ]);
    for leaf in &mut r.signals {
        if leaf.value.code == "HIGH_COMPLEXITY" {
            leaf.value.evidence = Some(json!({
                "high_complexity_count": 2, "threshold": 20, "top_complex": top
            }));
        }
    }
    let out = r.render_human(OrientDepth::Medium);
    // Breakdown: the symbol WITH a line anchors `file:line — symbol`.
    assert!(
        out.contains("src/a.c:42 — hot (cx 40)"),
        "breakdown row anchors file:line:\n{out}"
    );
    // The symbol WITHOUT a line renders the bare file (no fabricated anchor).
    assert!(
        out.contains("src/a.c — warm (cx 30)"),
        "breakdown row without a line renders bare file:\n{out}"
    );
    // The dense HEADLINE names the file rollup WITHOUT a line.
    assert!(
        out.contains("Complexity centers: src/a.c (cx 40)"),
        "file-rollup headline is unanchored:\n{out}"
    );
    assert!(
        !out.contains("Complexity centers: src/a.c:42"),
        "the headline must not pick one symbol's line for the file rollup:\n{out}"
    );
}
