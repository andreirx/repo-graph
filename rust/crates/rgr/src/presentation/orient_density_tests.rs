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
        medium.contains("… and 110 more groups — see `stats --json` / `modules`"),
        "medium omission must be true:\n{medium}"
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
        large.contains("… and 80 more groups — see `stats --json` / `modules`"),
        "large omission must be true:\n{large}"
    );
    assert!(
        large.contains("g049 — "),
        "50th group shown at large:\n{large}"
    );
    assert!(
        !large.contains("g050 — "),
        "large caps the section at 50:\n{large}"
    );

    // FULL: package groups stay BOUNDED (§13 D7) — the SAME top-50 as `large`,
    // NOT uncapped. `--full` uncaps only the complexity table (see
    // `medium_caps_complexity_large_expands_full_uncapped`), never the package
    // topology, which on a monorepo scales with directories into the thousands —
    // the exact overrun D7 exists to bound on the primary surface. Same TRUE
    // omission as `large` (130 - 50 = 80); the COMPLETE 130-group set rides JSON.
    let full = r.render_human(OrientDepth::Full);
    assert!(
        full.contains("… and 80 more groups — see `stats --json` / `modules`"),
        "full omission must be true — bounded at EVERY tier, incl. --full:\n{full}"
    );
    assert!(
        full.contains("g049 — "),
        "50th group shown at full:\n{full}"
    );
    assert!(
        !full.contains("g050 — "),
        "full caps the package-group section at 50 (§13 D7 — NOT uncapped):\n{full}"
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
