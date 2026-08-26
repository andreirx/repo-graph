//! Tests for `docs.rs` (moved out per the 500-line guardrail; SELF-POLLUTION-1 review-6 #3).

use super::*;

// ── docs list tests ──────────────────────────────────────────────────────

fn sample_list_response() -> DocsListResponse {
    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("readme".to_string(), 2);
    counts_by_kind.insert("changelog".to_string(), 1);

    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![
            DocEntry {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "abc123".to_string(),
            },
            DocEntry {
                path: "docs/README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "def456".to_string(),
            },
            DocEntry {
                path: "CHANGELOG.md".to_string(),
                kind: "changelog".to_string(),
                generated: true,
                content_hash: "ghi789".to_string(),
            },
        ],
        count: 3,
        counts_by_kind,
        generated_count: 1,
        unreadable: 0,
    }
}

/// A response with NO generated entries (the byte-parity case: nothing for the
/// slice to filter).
fn sample_list_response_no_generated() -> DocsListResponse {
    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("readme".to_string(), 1);
    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![DocEntry {
            path: "README.md".to_string(),
            kind: "readme".to_string(),
            generated: false,
            content_hash: "abc".to_string(),
        }],
        count: 1,
        counts_by_kind,
        generated_count: 0,
        unreadable: 0,
    }
}

#[test]
fn list_render_surfaces_unreadable_count() {
    // operator RULING 3: unreadable sidecars are said out loud in the human listing
    // ("+N unreadable, counted"), never silently folded as authored.
    let mut resp = sample_list_response_no_generated();
    resp.unreadable = 3;
    let out = resp.render_human(false);
    assert!(
        out.contains("+3 unreadable, counted"),
        "unreadable surfaced in human render: {out}"
    );
    // Zero unreadable → no such line (no "+0 unreadable" noise).
    let clean = sample_list_response_no_generated();
    assert!(
        !clean.render_human(false).contains("unreadable"),
        "no unreadable line when zero"
    );
}

#[test]
fn list_render_shows_header() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    assert!(out.starts_with("Documentation\n"));
}

#[test]
fn list_render_shows_count() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    assert!(out.contains("3 documents"));
}

#[test]
fn list_render_shows_by_kind() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    assert!(out.contains("By kind:"));
    assert!(out.contains("readme  2"));
    assert!(out.contains("changelog  1"));
}

#[test]
fn list_render_shows_generated_count() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    assert!(out.contains("1 generated"));
}

#[test]
fn list_render_shows_entries_sorted_by_path() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    // Entries should be sorted: CHANGELOG.md, README.md, docs/README.md
    let changelog_pos = out.find("CHANGELOG.md").unwrap();
    let readme_pos = out.find("README.md").unwrap();
    let docs_readme_pos = out.find("docs/README.md").unwrap();
    assert!(changelog_pos < readme_pos);
    assert!(readme_pos < docs_readme_pos);
}

#[test]
fn list_render_shows_generated_marker() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    assert!(out.contains("CHANGELOG.md  changelog  [generated]"));
}

#[test]
fn list_render_shows_hint() {
    let resp = sample_list_response();
    let out = resp.render_human(true);
    assert!(out.contains("hint: run 'rmap docs extract' to scan for explicit rg: markers"));
}

#[test]
fn list_render_empty_shows_hint() {
    let resp = DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![],
        count: 0,
        counts_by_kind: BTreeMap::new(),
        generated_count: 0,
        unreadable: 0,
    };
    let out = resp.render_human(true);
    assert!(out.contains("0 documents"));
    assert!(out.contains("hint: no documentation files detected"));
}

#[test]
fn list_render_singular_document() {
    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("readme".to_string(), 1);

    let resp = DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![DocEntry {
            path: "README.md".to_string(),
            kind: "readme".to_string(),
            generated: false,
            content_hash: "abc".to_string(),
        }],
        count: 1,
        counts_by_kind,
        generated_count: 0,
        unreadable: 0,
    };
    let out = resp.render_human(true);
    assert!(out.contains("1 document\n")); // singular
}

// ── SELF-POLLUTION-1 §3: default generated-map exclusion ──────────────────

/// A response mixing the reader's docs with rmap's own generated maps.
fn response_with_generated_maps() -> DocsListResponse {
    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![
            DocEntry {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "a".to_string(),
            },
            DocEntry {
                path: "src/MAP.md".to_string(),
                kind: "map".to_string(),
                generated: true,
                content_hash: "b".to_string(),
            },
            DocEntry {
                path: "src/core/MAP.md".to_string(),
                kind: "map".to_string(),
                generated: true,
                content_hash: "c".to_string(),
            },
        ],
        count: 3,
        counts_by_kind: BTreeMap::new(),
        generated_count: 2,
        unreadable: 0,
    }
}

#[test]
fn list_default_excludes_generated_maps_and_states_the_count() {
    // Default (include_generated = false): rmap's own maps are hidden, only the
    // reader's doc is shown, and the excluded count is stated (never silently hidden).
    let resp = response_with_generated_maps();
    let out = resp.render_human(false);
    assert!(
        out.contains("1 document\n"),
        "only the reader's doc counts: {out}"
    );
    assert!(
        out.contains("2 generated maps excluded (rmap's own; use --include-generated to show)"),
        "excluded count stated: {out}"
    );
    assert!(out.contains("README.md"), "reader's doc listed: {out}");
    assert!(
        !out.contains("src/MAP.md"),
        "generated map hidden by default: {out}"
    );
    // Singular wording when exactly one map is excluded.
    assert!(
        !out.contains("1 generated maps"),
        "plural only when >1: {out}"
    );
}

#[test]
fn list_include_generated_shows_maps_and_no_exclusion_line() {
    // Opt-in: all entries render, and NO "excluded" line is printed.
    let resp = response_with_generated_maps();
    let out = resp.render_human(true);
    assert!(out.contains("3 documents"), "{out}");
    assert!(
        out.contains("src/MAP.md"),
        "generated map shown on opt-in: {out}"
    );
    assert!(out.contains("src/core/MAP.md"), "{out}");
    assert!(
        !out.contains("excluded (rmap's own"),
        "no exclusion line when opted in: {out}"
    );
    assert!(
        out.contains("2 generated"),
        "visible generated count on opt-in: {out}"
    );
}

#[test]
fn list_all_generated_does_not_claim_no_docs() {
    // When EVERY doc is an rmap map, the default listing must not misrepresent the
    // repo as having no documentation — it points at --include-generated instead.
    let resp = DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![DocEntry {
            path: "MAP.md".to_string(),
            kind: "map".to_string(),
            generated: true,
            content_hash: "a".to_string(),
        }],
        count: 1,
        counts_by_kind: BTreeMap::new(),
        generated_count: 1,
        unreadable: 0,
    };
    let out = resp.render_human(false);
    assert!(out.contains("0 documents"), "{out}");
    assert!(
        out.contains("1 generated map excluded"),
        "singular map excluded wording: {out}"
    );
    assert!(
        out.contains("all documentation here is rmap-generated"),
        "honest hint, never a false 'no documentation': {out}"
    );
    assert!(
        !out.contains("no documentation files detected"),
        "must not claim the repo has no docs when it has generated maps: {out}"
    );
}

// ── SELF-POLLUTION-1 §2.3 + review-5 finding 1: `--json` filter + byte-parity ─

#[test]
fn json_default_excludes_generated_and_reports_excluded_count() {
    // Default (include_generated = false) WITH generated maps present: the machine
    // view drops rmap's own maps, and `excluded_generated` states how many — a
    // consumer never has to infer what was hidden. This is the AFFECTED case, so
    // `filtered_json_view` returns Some.
    let resp = response_with_generated_maps();
    let v = resp
        .filtered_json_view(false)
        .expect("generated maps present → filtered view");

    assert_eq!(v["count"], 1, "only the reader's doc counts: {v}");
    assert_eq!(v["excluded_generated"], 2, "excluded count reported: {v}");
    assert_eq!(v["generated_count"], 0, "no generated in visible set: {v}");

    let entries = v["entries"].as_array().unwrap();
    let paths: Vec<&str> = entries
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["README.md"], "generated maps dropped: {v}");
}

#[test]
fn json_include_generated_is_raw_passthrough_no_filtered_view() {
    // Opt-in: nothing is excluded → `filtered_json_view` returns None, so the
    // command prints the RAW daemon value UNCHANGED (byte-parity with pre-slice —
    // review-5 finding 1). The CLI never rebuilds/sorts/annotates in this case.
    let resp = response_with_generated_maps();
    assert!(
        resp.filtered_json_view(true).is_none(),
        "--include-generated excludes nothing → raw passthrough, not a rebuilt view"
    );
}

#[test]
fn json_no_generated_maps_is_raw_passthrough() {
    // A repo with NO generated maps: default `--json` filters nothing → None →
    // raw passthrough. This is the byte-parity guarantee review-5 finding 1
    // requires (no gratuitous `excluded_generated: 0`, no re-sorting).
    let resp = sample_list_response_no_generated();
    assert!(
        resp.filtered_json_view(false).is_none(),
        "nothing generated → nothing filtered → raw passthrough"
    );
}

#[test]
fn json_filtered_view_preserves_entry_fields_and_recomputes_counts_by_kind() {
    // In the AFFECTED case the visible entries keep their full field shape (incl.
    // content_hash), and counts_by_kind reflects the VISIBLE set.
    let resp = response_with_generated_maps();
    let v = resp.filtered_json_view(false).expect("filtered view");

    let entry = &v["entries"].as_array().unwrap()[0];
    assert_eq!(entry["path"], "README.md");
    assert_eq!(entry["kind"], "readme");
    assert_eq!(entry["generated"], false);
    assert_eq!(entry["content_hash"], "a");

    // Only the reader's readme is visible → counts_by_kind has just that.
    assert_eq!(v["counts_by_kind"]["readme"], 1);
    assert!(
        v["counts_by_kind"].get("map").is_none(),
        "generated 'map' kind excluded by default: {v}"
    );
    // No unreadable in this fixture → the key is absent (not `0`).
    assert!(
        v.get("unreadable").is_none(),
        "unreadable key omitted when zero: {v}"
    );
}

#[test]
fn json_filtered_view_surfaces_unreadable_when_present() {
    // operator RULING 3: when the daemon reported unreadable sidecars, the filtered
    // machine view carries the count so a consumer sees the UNKNOWN, never a silent
    // omission. Emitted only when > 0.
    let mut resp = response_with_generated_maps();
    resp.unreadable = 2;
    let v = resp.filtered_json_view(false).expect("filtered view");
    assert_eq!(
        v["unreadable"], 2,
        "unreadable count surfaced in machine view: {v}"
    );
}

// ── docs extract tests ───────────────────────────────────────────────────

fn sample_extract_response() -> DocsExtractResponse {
    let mut files_by_kind = BTreeMap::new();
    files_by_kind.insert("readme".to_string(), 2);

    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("api_endpoint".to_string(), 5);
    counts_by_kind.insert("config_key".to_string(), 3);

    DocsExtractResponse {
        command: "docs extract".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        files_scanned: 2,
        files_by_kind,
        facts_extracted: 8,
        facts_inserted: 6,
        facts_deleted: 2,
        counts_by_kind,
        generated_docs_count: 1,
        warnings: vec![],
    }
}

#[test]
fn extract_render_shows_header() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.starts_with("Documentation Extraction\n"));
}

#[test]
fn extract_render_shows_files_scanned() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("2 files scanned"));
}

#[test]
fn extract_render_shows_files_by_kind() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("By kind:"));
    assert!(out.contains("readme  2"));
}

#[test]
fn extract_render_shows_extraction_results() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("Extraction results:"));
    assert!(out.contains("8 facts extracted"));
    assert!(out.contains("6 facts inserted"));
    assert!(out.contains("2 facts deleted"));
    assert!(out.contains("1 generated docs"));
}

#[test]
fn extract_render_shows_facts_by_kind() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("Facts by kind:"));
    assert!(out.contains("api_endpoint  5"));
    assert!(out.contains("config_key  3"));
}

#[test]
fn extract_render_shows_no_warnings() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("No warnings."));
}

#[test]
fn extract_render_shows_warnings() {
    let mut resp = sample_extract_response();
    resp.warnings = vec![
        "Failed to parse docs/api.md".to_string(),
        "Unknown format in CHANGELOG.md".to_string(),
    ];
    let out = resp.render_human();
    assert!(out.contains("2 warnings:"));
    assert!(out.contains("- Failed to parse docs/api.md"));
    assert!(out.contains("- Unknown format in CHANGELOG.md"));
}

#[test]
fn extract_render_singular_file() {
    let mut files_by_kind = BTreeMap::new();
    files_by_kind.insert("readme".to_string(), 1);

    let resp = DocsExtractResponse {
        command: "docs extract".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        files_scanned: 1,
        files_by_kind,
        facts_extracted: 0,
        facts_inserted: 0,
        facts_deleted: 0,
        counts_by_kind: BTreeMap::new(),
        generated_docs_count: 0,
        warnings: vec![],
    };
    let out = resp.render_human();
    assert!(out.contains("1 file scanned")); // singular
}
