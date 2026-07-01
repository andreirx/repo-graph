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
/// full uncapped) actually bite, so the ladder's progression is observable.
fn ladder_fixture() -> OrientResponse {
    use serde_json::json;
    let mut r = nginx_like();
    r.limits = vec![Limit {
        code: "GATE_NOT_CONFIGURED".to_string(),
        summary: "No active requirement declarations.".to_string(),
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
fn medium_caps_complexity_large_expands_full_uncapped() {
    // The complexity table is the ladder's uncapped-at-full section: medium shows
    // a scannable top-10 (honest "+N more" tail), large a larger detailed table,
    // `--full` the complete set with NO truncation tail. Proves large < full.
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
        medium.contains("+70 more above threshold — rmap hotspots"),
        "medium's capped table carries an honest tail:\n{medium}"
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
        large.contains("+30 more above threshold — rmap hotspots"),
        "large's capped table carries an honest tail:\n{large}"
    );

    // Full: uncapped — every center (f79) with NO truncation tail; large < full.
    assert!(
        full.contains("src/f79.c — fn79"),
        "full is uncapped:\n{full}"
    );
    assert!(
        !full.contains("more above threshold"),
        "full has no truncation tail (it is complete):\n{full}"
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
            out.contains("Reliability: call-graph 42% resolved (LOW)"),
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
