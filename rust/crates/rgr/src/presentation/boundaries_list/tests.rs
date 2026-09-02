//! Unit tests for the `boundaries list` presentation renderer (split from `mod.rs`
//! for the 500-line guardrail; see the module-layout note there).

use super::*;
use crate::presentation::surfaces::SurfaceGap;

/// A representative build-static coverage payload (the shape the daemon emits) for the
/// zero-state coverage-form tests.
fn sample_coverage() -> SurfaceCoverage {
    SurfaceCoverage {
        http_detector_families: vec![
            "Java Spring (@RestController/@Controller)".to_string(),
            "Next.js App Router".to_string(),
        ],
        named_uncovered: vec!["C".to_string(), "C++".to_string()],
        material_gap: Some(SurfaceGap::Known {
            named_uncovered: vec!["C".to_string(), "C++".to_string()],
        }),
    }
}

fn sample_list_response() -> BoundariesListResponse {
    BoundariesListResponse {
        command: "boundaries list".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        results: vec![
            BoundaryListEntry {
                boundary_channel_uid: "bc-1".to_string(),
                channel_kind: "http_client".to_string(),
                boundary_scope: "external".to_string(),
                direction: "outbound".to_string(),
                protocol_family: Some("REST".to_string()),
                service_name: Some("UserService".to_string()),
                file_path: Some("src/api/client.ts".to_string()),
                symbol_key: None,
                confidence: 0.9,
                basis: Some("pattern".to_string()),
                surface_uid: Some("surf-1".to_string()),
                surface_display_name: Some("api".to_string()),
                test_composition: "production".to_string(),
                test_composition_unknown_reason: None,
            },
            BoundaryListEntry {
                boundary_channel_uid: "bc-2".to_string(),
                channel_kind: "database".to_string(),
                boundary_scope: "internal".to_string(),
                direction: "bidirectional".to_string(),
                protocol_family: Some("SQL".to_string()),
                service_name: None,
                file_path: Some("src/db/pool.ts".to_string()),
                symbol_key: Some("DbPool.query".to_string()),
                confidence: 0.8,
                basis: Some("import".to_string()),
                surface_uid: None,
                surface_display_name: None,
                test_composition: "production".to_string(),
                test_composition_unknown_reason: None,
            },
        ],
        count: 2,
        filter_kind: None,
        filter_scope: None,
        filter_direction: None,
        filter_family: None,
        filter_file: None,
        filter_file_prefix: None,
        filter_symbol: None,
        surface_coverage: sample_coverage(),
    }
}

fn sample_empty_response() -> BoundariesListResponse {
    BoundariesListResponse {
        command: "boundaries list".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        results: vec![],
        count: 0,
        filter_kind: None,
        filter_scope: None,
        filter_direction: None,
        filter_family: None,
        filter_file: None,
        filter_file_prefix: None,
        filter_symbol: None,
        surface_coverage: sample_coverage(),
    }
}

#[test]
fn list_render_shows_header() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("Boundaries"));
}

#[test]
fn list_render_shows_count() {
    let resp = sample_list_response();
    let output = resp.render_human();
    // Two distinct (file×direction) groups ⇒ "2 boundaries".
    assert!(output.contains("2 boundaries"));
}

#[test]
fn list_render_headline_counts_groups_not_rows() {
    // review-1 #2a regression: the headline counts GROUPS, not rows. Three verbatim
    // rows in ONE (file × direction) group must read as "1 boundary", never "3".
    let mut resp = sample_empty_response();
    let dup = |uid: &str| BoundaryListEntry {
        boundary_channel_uid: uid.to_string(),
        channel_kind: "http".to_string(),
        boundary_scope: "unknown".to_string(),
        direction: "provider".to_string(),
        protocol_family: Some("http".to_string()),
        service_name: None,
        file_path: Some("src/app/api/x/route.ts".to_string()),
        symbol_key: None,
        confidence: 0.9,
        basis: None,
        surface_uid: None,
        surface_display_name: None,
        test_composition: "production".to_string(),
        test_composition_unknown_reason: None,
    };
    resp.results = vec![dup("a"), dup("b"), dup("c")];
    resp.count = 3;
    let output = resp.render_human();
    assert!(
        output.contains("1 boundary\n"),
        "group-based headline:\n{output}"
    );
    assert!(!output.contains("3 boundaries"), "not row-based:\n{output}");
    // The three duplicates still collapse to a single ×3 row in the body.
    assert!(output.contains("×3"), "{output}");
}

#[test]
fn list_render_shows_boundaries() {
    // §2.4: the grouped view is keyed on file × direction (per-service / contract detail
    // lives in `boundaries show`), so it shows the channel kinds and the FILES, one `×N`
    // row per file×direction group.
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("http_client"));
    assert!(output.contains("database"));
    assert!(output.contains("src/api/client.ts"), "{output}");
    assert!(output.contains("src/db/pool.ts"), "{output}");
    assert!(
        output.contains('×'),
        "grouped rows carry a ×N count:\n{output}"
    );
}

#[test]
fn list_render_shows_direction() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("outbound"));
    assert!(output.contains("bidirectional"));
}

#[test]
fn list_render_groups_duplicate_rows_with_count() {
    // The audit defect: N verbatim-duplicate rows. The grouped view collapses them to
    // ONE row with `×N`, and lifts the constant columns out.
    let mut resp = sample_empty_response();
    let dup = |uid: &str| BoundaryListEntry {
        boundary_channel_uid: uid.to_string(),
        channel_kind: "http".to_string(),
        boundary_scope: "unknown".to_string(),
        direction: "provider".to_string(),
        protocol_family: Some("http".to_string()),
        service_name: None,
        file_path: Some("src/app/api/x/route.ts".to_string()),
        symbol_key: None,
        confidence: 0.9,
        basis: None,
        surface_uid: None,
        surface_display_name: None,
        test_composition: "production".to_string(),
        test_composition_unknown_reason: None,
    };
    resp.results = vec![dup("a"), dup("b"), dup("c")];
    resp.count = 3;
    let output = resp.render_human();
    // Constant columns stated once, not per row.
    assert!(output.contains("kind=http"), "{output}");
    assert!(output.contains("scope=unknown"), "{output}");
    // The three duplicates collapse to a single ×3 row.
    assert!(output.contains("×3"), "{output}");
    assert_eq!(
        output.matches("src/app/api/x/route.ts").count(),
        1,
        "{output}"
    );
}

#[test]
fn list_render_summarizes_methods_routes_per_file_direction() {
    // §2.4: the grouped view keys on (file, direction) and summarizes the methods/routes
    // (surface_display_name) — the signal that lived only in `surfaces list`. Two routes
    // in one provider file collapse to ONE group with both routes summarized.
    let mut resp = sample_empty_response();
    let route = |uid: &str, disp: &str| BoundaryListEntry {
        boundary_channel_uid: uid.to_string(),
        channel_kind: "http".to_string(),
        boundary_scope: "unknown".to_string(),
        direction: "provider".to_string(),
        protocol_family: Some("http".to_string()),
        service_name: None,
        file_path: Some("src/app/api/x/route.ts".to_string()),
        symbol_key: None,
        confidence: 0.9,
        basis: None,
        surface_uid: None,
        surface_display_name: Some(disp.to_string()),
        test_composition: "production".to_string(),
        test_composition_unknown_reason: None,
    };
    resp.results = vec![route("a", "GET /api/x"), route("b", "POST /api/x")];
    resp.count = 2;
    let output = resp.render_human();
    // One file×direction group, ×2, with BOTH methods/routes summarized.
    assert!(output.contains("×2"), "{output}");
    assert!(output.contains("GET /api/x"), "{output}");
    assert!(output.contains("POST /api/x"), "{output}");
    // The file appears once (grouped, not two verbatim rows).
    assert_eq!(
        output.matches("src/app/api/x/route.ts").count(),
        1,
        "{output}"
    );
}

/// Build an entry with an explicit test-composition discriminant.
fn entry(uid: &str, file: &str, kind: &str, composition: &str) -> BoundaryListEntry {
    BoundaryListEntry {
        boundary_channel_uid: uid.to_string(),
        channel_kind: kind.to_string(),
        boundary_scope: "internal".to_string(),
        direction: "bidirectional".to_string(),
        protocol_family: Some("amqp".to_string()),
        service_name: None,
        file_path: Some(file.to_string()),
        symbol_key: None,
        confidence: 0.9,
        basis: None,
        surface_uid: None,
        surface_display_name: None,
        test_composition: composition.to_string(),
        test_composition_unknown_reason: if composition == "unknown" {
            Some("no stored is_test fact for the file".to_string())
        } else {
            None
        },
    }
}

#[test]
fn list_render_demotes_test_only_groups_below_main() {
    // FIXTURE-POLLUTION-1 §2.2: rows positively classified test-only are grouped under a
    // labeled trailing section and EXCLUDED from the headline group count; the production
    // rows lead. Never hidden, never name-classified (the discriminant is the stored
    // fact, set by the daemon read).
    let mut resp = sample_empty_response();
    resp.results = vec![
        entry("p1", "src/broker/publish.rs", "amqp_producer", "production"),
        entry(
            "t1",
            "rust/crates/x/tests/fixtures/amqp.rs",
            "amqp_producer",
            "test_only",
        ),
        entry(
            "t2",
            "rust/crates/x/tests/fixtures/kafka.rs",
            "kafka_producer",
            "test_only",
        ),
    ];
    resp.count = 3;
    let output = resp.render_human();

    // Headline count is main-only (1 group), with the demoted test-only surfaces noted.
    assert!(output.contains("1 boundary\n"), "{output}");
    assert!(output.contains("+2 test-only surfaces"), "{output}");
    // The demoted section is present, labeled, and excluded from the headline.
    assert!(
        output.contains("test-only surfaces (2 groups — excluded from the headline counts"),
        "{output}"
    );
    // Production file leads; test-only render below (never hidden).
    let prod_pos = output.find("src/broker/publish.rs").expect("prod row");
    let section_pos = output.find("test-only surfaces (2").expect("section");
    let fixture_pos = output
        .find("rust/crates/x/tests/fixtures/amqp.rs")
        .expect("test-only row shown, not hidden");
    assert!(prod_pos < section_pos, "production leads:\n{output}");
    assert!(
        section_pos < fixture_pos,
        "test-only under the section:\n{output}"
    );
}

#[test]
fn list_render_unknown_rows_stay_in_main_with_marker() {
    // Binding direction rule: an UNKNOWN-composition group (no reachable is_test fact) is
    // NEVER demoted — it stays in the main listing carrying an explicit unknown-with-
    // reason marker, above the demoted test-only section.
    let mut resp = sample_empty_response();
    resp.results = vec![
        entry("p1", "src/broker/publish.rs", "amqp_producer", "production"),
        entry("u1", "vendor/opaque/x.rs", "amqp_producer", "unknown"),
        entry(
            "t1",
            "rust/crates/x/tests/fixtures/amqp.rs",
            "amqp_producer",
            "test_only",
        ),
    ];
    resp.count = 3;
    let output = resp.render_human();

    // Unknown is NOT a demoted surface: only the 1 test-only group is excluded.
    assert!(output.contains("+1 test-only surface"), "{output}");
    // The unknown row is in the MAIN listing carrying its marker with the reason.
    let marker_pos = output
        .find("[test-composition unknown:")
        .expect("unknown marker present in main listing");
    let section_pos = output.find("test-only surfaces (1").expect("section");
    assert!(
        output.contains("vendor/opaque/x.rs"),
        "unknown row shown in main:\n{output}"
    );
    // The marker appears in the main listing, ABOVE the demoted section.
    assert!(
        marker_pos < section_pos,
        "unknown marker in main:\n{output}"
    );
}

#[test]
fn list_render_all_test_only_shows_empty_main_headline() {
    // Conservative + honest: when every group is test-only, the main headline is 0 and
    // the surfaces are still shown under the labeled section (never a false "no
    // boundaries").
    let mut resp = sample_empty_response();
    resp.results = vec![entry(
        "t1",
        "rust/crates/x/tests/fixtures/amqp.rs",
        "amqp_producer",
        "test_only",
    )];
    resp.count = 1;
    let output = resp.render_human();
    assert!(output.contains("0 boundaries"), "{output}");
    assert!(output.contains("+1 test-only surface"), "{output}");
    assert!(
        output.contains("rust/crates/x/tests/fixtures/amqp.rs"),
        "test-only shown:\n{output}"
    );
}

#[test]
fn list_render_shows_scope() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("external"));
    assert!(output.contains("internal"));
}

#[test]
fn list_render_empty_shows_hint() {
    let resp = sample_empty_response();
    let output = resp.render_human();
    assert!(output.contains("hint:"));
    assert!(output.contains("boundaries are interactions"));
}

/// ZEROSTATE-SCOPE-1 §2.2: the zero-state adopts the coverage form (states the tool's
/// coverage + this repo's per-repo gap) and NO LONGER blames the codebase.
#[test]
fn list_render_empty_states_coverage_not_codebase_blame() {
    let resp = sample_empty_response();
    let output = resp.render_human();
    assert!(
        output.contains("No boundary patterns detected."),
        "{output}"
    );
    assert!(
        output.contains("Boundary detection on this build covers Java Spring (@RestController/@Controller), Next.js App Router."),
        "{output}"
    );
    // The per-repo gap names THIS repo's uncovered languages (leveldb's C/C++ shape).
    assert!(
        output.contains("No detector for C, C++ on this build"),
        "{output}"
    );
    // The blaming line is GONE.
    assert!(
        !output.contains("No recognized boundary patterns found in this codebase"),
        "must not blame the codebase:\n{output}"
    );
    assert!(!output.contains("in this codebase"), "{output}");
}

/// STANDING HONESTY RULE 1: a FAILED per-repo language read renders unknown-with-reason in
/// the boundaries zero-state, never a silent omission.
#[test]
fn list_render_empty_gap_unknown_renders_reason() {
    let mut resp = sample_empty_response();
    resp.surface_coverage.material_gap = Some(SurfaceGap::Unknown {
        reason: "db locked".to_string(),
    });
    let output = resp.render_human();
    assert!(
        output
            .contains("could not determine this repo's uncovered frameworks/languages: db locked"),
        "{output}"
    );
}

#[test]
fn list_render_is_deterministic() {
    let resp = sample_list_response();
    let output = resp.render_human();
    // database comes before http_client alphabetically by channel_kind
    let db_pos = output.find("database").unwrap();
    let http_pos = output.find("http_client").unwrap();
    assert!(
        db_pos < http_pos,
        "Boundaries should be sorted by (kind, direction, ...)"
    );
}

#[test]
fn list_render_shows_filter() {
    let mut resp = sample_empty_response();
    resp.filter_kind = Some("http_client".to_string());
    let output = resp.render_human();
    assert!(output.contains("Filtered by: kind=http_client"));
}
