//! Unit tests for the `boundaries summary` presentation renderer (split from `mod.rs`
//! for the 500-line guardrail; see the module-layout note there).

use super::*;
use crate::presentation::surfaces::{SurfaceCoverage, SurfaceGap};

/// A representative build-static coverage payload for the zero-state coverage-form tests.
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

fn sample_summary_response() -> BoundariesSummaryResponse {
    BoundariesSummaryResponse {
        command: "boundaries summary".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        summary: Some(BoundarySummary {
            total_surfaces: 3,
            total_channels: 5,
            by_channel_kind: vec![
                CategoryCount {
                    category: "http_client".to_string(),
                    count: 3,
                },
                CategoryCount {
                    category: "database".to_string(),
                    count: 2,
                },
            ],
            by_boundary_scope: vec![
                CategoryCount {
                    category: "external".to_string(),
                    count: 3,
                },
                CategoryCount {
                    category: "internal".to_string(),
                    count: 2,
                },
            ],
            by_direction: vec![
                CategoryCount {
                    category: "outbound".to_string(),
                    count: 4,
                },
                CategoryCount {
                    category: "inbound".to_string(),
                    count: 1,
                },
            ],
            by_protocol_family: vec![CategoryCount {
                category: "REST".to_string(),
                count: 3,
            }],
            by_basis: vec![
                CategoryCount {
                    category: "pattern".to_string(),
                    count: 4,
                },
                CategoryCount {
                    category: "import".to_string(),
                    count: 1,
                },
            ],
            files_with_boundaries: vec![
                "src/api/client.ts".to_string(),
                "src/db/pool.ts".to_string(),
            ],
        }),
        http_providers: Some(3),
        http_consumers: Some(2),
        http_degraded: None,
        test_only: partition::Additive::Absent,
        unknown: partition::Additive::Absent,
        surface_coverage: sample_coverage(),
    }
}

fn sample_empty_summary_response() -> BoundariesSummaryResponse {
    BoundariesSummaryResponse {
        command: "boundaries summary".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        http_providers: None,
        http_consumers: None,
        http_degraded: None,
        test_only: partition::Additive::Absent,
        unknown: partition::Additive::Absent,
        surface_coverage: sample_coverage(),
        summary: Some(BoundarySummary {
            total_surfaces: 0,
            total_channels: 0,
            by_channel_kind: vec![],
            by_boundary_scope: vec![],
            by_direction: vec![],
            by_protocol_family: vec![],
            by_basis: vec![],
            files_with_boundaries: vec![],
        }),
    }
}

/// Parse a daemon-shaped payload with an additive `test_only_summary`.
fn response_with_test_only() -> BoundariesSummaryResponse {
    let payload = serde_json::json!({
        "command": "boundaries summary",
        "repo": "r",
        "snapshot": "s",
        "summary": {
            "totalSurfaces": 5,
            "totalChannels": 5,
            "byChannelKind": [
                {"channelKind": "amqp_producer", "count": 3},
                {"channelKind": "database", "count": 2}
            ],
            "byBoundaryScope": [{"boundaryScope": "internal", "count": 5}],
            "byDirection": [{"direction": "provider", "count": 5}],
            "byProtocolFamily": [{"protocolFamily": "amqp", "count": 3},{"protocolFamily":"sql","count":2}],
            "byBasis": [{"basis": "import", "count": 5}],
            "filesWithBoundaries": ["src/db/pool.rs", "tests/fixtures/amqp.rs"]
        },
        "http_surface_providers": 0,
        "http_surface_consumers": 0,
        "test_only_summary": {
            "totalSurfaces": 3,
            "totalChannels": 0,
            "byChannelKind": [{"channelKind": "amqp_producer", "count": 3}],
            "byBoundaryScope": [{"boundaryScope": "internal", "count": 3}],
            "byDirection": [{"direction": "provider", "count": 3}],
            "byProtocolFamily": [{"protocolFamily": "amqp", "count": 3}],
            "byBasis": [{"basis": "import", "count": 3}],
            "filesWithBoundaries": ["tests/fixtures/amqp.rs"],
            "http_surface_providers": 0,
            "http_surface_consumers": 0
        }
    });
    BoundariesSummaryResponse::from_json(payload).expect("parse")
}

#[test]
fn summary_render_shows_header() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(output.contains("Boundaries Summary"));
}

#[test]
fn summary_render_shows_totals() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(output.contains("3 surfaces"));
    assert!(output.contains("5 channels"));
}

#[test]
fn summary_headline_excludes_test_only_and_discloses_it_trailing() {
    // review-1 #2b: the headline totals/breakdowns are production+unknown (full − test-
    // only); the test-only content is a clearly-labeled trailing section.
    let resp = response_with_test_only();
    let out = resp.render_human();
    // Headline surfaces = 5 − 3 = 2 (NOT 5). The demoted amqp fixtures are gone from the
    // headline channel-kind breakdown.
    assert!(out.contains("2 surfaces"), "{out}");
    let headline = &out[..out.find("test-only surfaces (excluded").unwrap()];
    assert!(
        !headline.contains("amqp_producer"),
        "test-only amqp not in the headline breakdown:\n{headline}"
    );
    assert!(
        headline.contains("database"),
        "production kind stays:\n{headline}"
    );
    // The headline file list drops the wholly-test-only file, keeps the production one.
    assert!(headline.contains("src/db/pool.rs"), "{headline}");
    assert!(
        !headline.contains("tests/fixtures/amqp.rs"),
        "test-only file demoted out of the headline:\n{headline}"
    );
    // The trailing disclosure names the test-only surfaces + points at `boundaries list`.
    let trailing = &out[out.find("test-only surfaces (excluded").unwrap()..];
    assert!(trailing.contains("3 surfaces"), "{trailing}");
    assert!(trailing.contains("amqp_producer"), "{trailing}");
    assert!(trailing.contains("tests/fixtures/amqp.rs"), "{trailing}");
    assert!(trailing.contains("boundaries list"), "{trailing}");
}

#[test]
fn summary_discloses_unknown_composition_without_demoting() {
    // review-2 #1 + binding direction rule: unknown-composition surfaces STAY in the headline
    // counts (not subtracted) but are disclosed with their reasons — never silent production.
    let payload = serde_json::json!({
        "command": "boundaries summary",
        "repo": "r",
        "snapshot": "s",
        "summary": {
            "totalSurfaces": 4,
            "totalChannels": 4,
            "byChannelKind": [{"channelKind": "database", "count": 4}],
            "byBoundaryScope": [{"boundaryScope": "internal", "count": 4}],
            "byDirection": [{"direction": "consumer", "count": 4}],
            "byProtocolFamily": [{"protocolFamily": "sql", "count": 4}],
            "byBasis": [{"basis": "import", "count": 4}],
            "filesWithBoundaries": ["src/a.rs", "vendor/x.rs"]
        },
        "http_surface_providers": 0,
        "http_surface_consumers": 0,
        "unknown_composition": {
            "surfaces": 2,
            "reasons": ["no stored is_test fact for vendor/x.rs"]
        }
    });
    let resp = BoundariesSummaryResponse::from_json(payload).expect("parse");
    let out = resp.render_human();
    // Headline is UNCHANGED by unknown (still 4 — unknown is never subtracted).
    assert!(out.contains("4 surfaces"), "{out}");
    // The disclosure names the count + reason.
    assert!(
        out.contains("2 headline surfaces of unknown test-composition"),
        "{out}"
    );
    assert!(out.contains("not confirmed production"), "{out}");
    assert!(
        out.contains("no stored is_test fact for vendor/x.rs"),
        "{out}"
    );
}

#[test]
fn summary_degraded_test_only_shows_full_headline_and_says_so() {
    // review-2 #2: a present-but-malformed `test_only_summary` must NOT silently subtract a
    // zero-filled partial. It degrades — headline is the FULL summary, with an explicit note.
    let payload = serde_json::json!({
        "command": "boundaries summary",
        "repo": "r",
        "snapshot": "s",
        "summary": {
            "totalSurfaces": 5,
            "totalChannels": 5,
            "byChannelKind": [{"channelKind": "amqp_producer", "count": 5}],
            "byBoundaryScope": [{"boundaryScope": "internal", "count": 5}],
            "byDirection": [{"direction": "provider", "count": 5}],
            "byProtocolFamily": [{"protocolFamily": "amqp", "count": 5}],
            "byBasis": [{"basis": "import", "count": 5}],
            "filesWithBoundaries": ["tests/fixtures/amqp.rs"]
        },
        "http_surface_providers": 0,
        "http_surface_consumers": 0,
        // Missing required `totalSurfaces` ⇒ strict parse fails ⇒ Degraded.
        "test_only_summary": {
            "totalChannels": 0,
            "byChannelKind": [],
            "byBoundaryScope": [],
            "byDirection": [],
            "byProtocolFamily": [],
            "byBasis": [],
            "filesWithBoundaries": [],
            "http_surface_providers": 0,
            "http_surface_consumers": 0
        }
    });
    let resp = BoundariesSummaryResponse::from_json(payload).expect("outer parse still ok");
    let out = resp.render_human();
    // Nothing subtracted — the full 5 surfaces remain (never hide possibly-real architecture).
    assert!(out.contains("5 surfaces"), "{out}");
    assert!(out.contains("test-only partition unavailable"), "{out}");
    // No trailing "test-only surfaces (excluded ...)" section, because we could not compute it.
    assert!(!out.contains("test-only surfaces (excluded"), "{out}");
}

#[test]
fn summary_no_test_only_is_unchanged() {
    // Absent test_only_summary → no trailing section, headline == full (the pre-slice
    // render, byte-identical for a fixture-free repo).
    let resp = sample_summary_response();
    let out = resp.render_human();
    assert!(!out.contains("test-only surfaces"), "{out}");
    assert!(out.contains("3 surfaces"));
}

#[test]
fn summary_render_shows_by_kind() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(output.contains("By channel kind:"));
    assert!(output.contains("http_client"));
    assert!(output.contains("database"));
}

#[test]
fn summary_render_shows_unified_http_line() {
    // §2.3: the unified HTTP provider/consumer line (same aggregation the surfaces footer
    // prints).
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(
        output.contains("HTTP/REST surfaces: 3 providers, 2 consumers"),
        "{output}"
    );
}

#[test]
fn summary_render_http_degraded_is_unknown() {
    // §2.3 + honesty: a degraded union read is UNKNOWN, never a silent zero.
    let mut resp = sample_summary_response();
    resp.http_providers = None;
    resp.http_consumers = None;
    resp.http_degraded = Some("db locked".to_string());
    let output = resp.render_human();
    assert!(output.contains("HTTP/REST surfaces: unknown"), "{output}");
    assert!(output.contains("db locked"), "{output}");
}

#[test]
fn summary_render_shows_by_scope() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(output.contains("By scope:"));
    assert!(output.contains("external"));
    assert!(output.contains("internal"));
}

#[test]
fn summary_render_shows_by_direction() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(output.contains("By direction:"));
    assert!(output.contains("outbound"));
    assert!(output.contains("inbound"));
}

#[test]
fn summary_render_shows_files() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    assert!(output.contains("Files with boundaries:"));
    assert!(output.contains("src/api/client.ts"));
    assert!(output.contains("src/db/pool.ts"));
}

#[test]
fn summary_render_empty_shows_hint() {
    let resp = sample_empty_summary_response();
    let output = resp.render_human();
    // ZEROSTATE-SCOPE-1 §2.2: the coverage form replaces the old "No architectural
    // boundaries detected" headline; the hint stays.
    assert!(output.contains("No boundary patterns detected."));
    assert!(output.contains("hint:"));
}

/// ZEROSTATE-SCOPE-1 §2.2: the summary zero-state adopts the coverage form (states the
/// tool's coverage + this repo's per-repo gap) and NO LONGER blames the codebase.
#[test]
fn summary_render_empty_states_coverage_not_codebase_blame() {
    let resp = sample_empty_summary_response();
    let output = resp.render_human();
    assert!(
        output.contains("Boundary detection on this build covers Java Spring (@RestController/@Controller), Next.js App Router."),
        "{output}"
    );
    assert!(
        output.contains("No detector for C, C++ on this build"),
        "{output}"
    );
    assert!(
        !output.contains("in this codebase"),
        "must not blame the codebase:\n{output}"
    );
}

#[test]
fn summary_render_sorts_by_count_desc() {
    let resp = sample_summary_response();
    let output = resp.render_human();
    // http_client (3) should come before database (2)
    let http_pos = output.find("http_client").unwrap();
    let db_pos = output.find("database").unwrap();
    assert!(
        http_pos < db_pos,
        "Categories should be sorted by count descending"
    );
}
