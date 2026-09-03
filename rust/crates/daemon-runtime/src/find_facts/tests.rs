//! Unit tests for the FACTS-tier taxonomy/shape/dedup ([`super`] = `find_facts`).
//!
//! Relocated out of `find_facts.rs` so the parent production module stays under the
//! 500-line structural guardrail (review-7 item 1). A child test module — NOT a
//! production abstraction: it compiles only under `#[cfg(test)]`, has one implicit
//! user (the parent's test run), and reaches the parent's crate-private taxonomy via
//! `use super::*` exactly as the sibling `queries` module does.

use super::*;

fn hit(display: &str, path: HitPath, key: Option<&str>) -> FactHit {
    FactHit {
        display: display.to_string(),
        path,
        key: key.map(str::to_string),
        next_command: None,
        line: None,
        evidence: None,
    }
}

#[test]
fn class_labels_and_commands_are_verified_pairs() {
    // The label→command→certainty triples are the witness manifest; adding a
    // class breaks this exhaustive listing (ALL has fixed len 7). The six
    // single-renderer classes carry a class-level render command; `boundary`'s
    // renderer varies per declaration kind, so its class command is `None`
    // (review-6 re-home) — the per-hit `next_command` carries the move instead.
    let triples: Vec<(&str, Option<&str>, &str)> = FactClass::ALL
        .iter()
        .map(|c| (c.label(), c.render_command(), c.certainty_tag()))
        .collect();
    assert_eq!(
        triples,
        vec![
            ("symbol", Some("explain"), "extracted"),
            ("file", Some("explain"), "extracted"),
            ("module", Some("map --dry-run"), "inferred"),
            ("http-surface", Some("boundaries list"), "inferred"),
            ("dependency", Some("deps list"), "extracted"),
            ("framework", Some("inferences list"), "hint"),
            ("boundary", None, "governance"),
        ]
    );
}

#[test]
fn hit_command_is_runnable_per_class() {
    // explain/map fold the hit key into a runnable target; the list classes
    // render the whole listing (the key is not a per-hit argument).
    assert_eq!(
        FactClass::Symbol.hit_command(Some("glamCRM:src/bnr.ts:BNRService")),
        Some("explain glamCRM:src/bnr.ts:BNRService".to_string())
    );
    assert_eq!(
        FactClass::File.hit_command(Some("src/bnr.ts")),
        Some("explain src/bnr.ts".to_string())
    );
    // `map` writes MAP.md by default (rgr map.rs); the emitted per-hit form is the
    // NON-MUTATING `map --dry-run <path>` (review-2 item 1). Flag before positional
    // is accepted by `parse_map_args` in any order.
    assert_eq!(
        FactClass::Module.hit_command(Some("packages/api")),
        Some("map --dry-run packages/api".to_string())
    );
    // A key with a space is shell-quoted so the line is copy-paste runnable.
    assert_eq!(
        FactClass::File.hit_command(Some("src/my file.ts")),
        Some("explain 'src/my file.ts'".to_string())
    );
    // List classes ignore the key — their command is the whole-listing move.
    assert_eq!(
        FactClass::HttpSurface.hit_command(None),
        Some("boundaries list".to_string())
    );
    assert_eq!(
        FactClass::Dependency.hit_command(Some("lodash")),
        Some("deps list".to_string())
    );
    assert_eq!(
        FactClass::Framework.hit_command(Some("react_component")),
        Some("inferences list".to_string())
    );
    // `boundary`'s renderer varies per hit → no class-level command; the hit's own
    // `next_command` (violations|gate) carries the move (review-6 re-home).
    assert_eq!(FactClass::Boundary.hit_command(None), None);
}

#[test]
fn finalize_dedups_by_path_and_key() {
    // Two rows with the same (path, key) collapse to one; a different key stays.
    let hits = vec![
        hit(
            "bnrService",
            HitPath::Known("src/bnr.ts".into()),
            Some("k1"),
        ),
        hit(
            "bnrService",
            HitPath::Known("src/bnr.ts".into()),
            Some("k1"),
        ),
        hit("bnrClient", HitPath::Known("src/bnr.ts".into()), Some("k2")),
    ];
    let out = finalize(hits, false, false);
    assert_eq!(out.matched, 2, "deduped to two distinct keys");
    assert_eq!(out.hits.len(), 2);
    assert!(!out.matched_is_floor);
}

#[test]
fn finalize_caps_with_exact_remainder_when_not_saturated() {
    let hits: Vec<FactHit> = (0..PER_CLASS_DISPLAY_CAP + 5)
        .map(|i| {
            hit(
                &format!("s{i}"),
                HitPath::Known(format!("f{i}")),
                Some(&format!("k{i}")),
            )
        })
        .collect();
    let out = finalize(hits, false, false);
    assert_eq!(out.hits.len(), PER_CLASS_DISPLAY_CAP);
    assert_eq!(
        out.matched,
        PER_CLASS_DISPLAY_CAP + 5,
        "matched is the exact total"
    );
    assert!(!out.matched_is_floor, "not saturated → exact, not a floor");
}

#[test]
fn finalize_floor_marked_only_when_saturated_and_not_full() {
    let hits: Vec<FactHit> = (0..PER_CLASS_DISPLAY_CAP + 1)
        .map(|i| {
            hit(
                &format!("s{i}"),
                HitPath::Known(format!("f{i}")),
                Some(&format!("k{i}")),
            )
        })
        .collect();
    let capped = finalize(hits.clone(), false, true);
    assert!(capped.matched_is_floor, "saturated + not full → floor");
    // --full lifts the cap AND clears the floor (a full run fetched everything).
    let full = finalize(hits, true, true);
    assert!(!full.matched_is_floor);
    assert_eq!(full.hits.len(), PER_CLASS_DISPLAY_CAP + 1);
}

#[test]
fn finalize_dedups_pathless_hits_by_key() {
    // A `None`-path class (dependency/framework) dedups by key alone; distinct
    // keys survive, identical keys collapse.
    let hits = vec![
        hit("lodash", HitPath::None, Some("lodash")),
        hit("lodash", HitPath::None, Some("lodash")),
        hit("react", HitPath::None, Some("react")),
    ];
    let out = finalize(hits, false, false);
    assert_eq!(out.matched, 2, "deduped pathless by key");
}
