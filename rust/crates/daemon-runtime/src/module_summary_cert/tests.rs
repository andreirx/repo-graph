//! EC-M2-LEAF-SERVE-1: unit tests for the MODULE_SUMMARY identity-reconciliation cert's PURE
//! halves — the faithful SQL-LIKE prefix filter (incl. its ground-truth pin against the REAL
//! SQLite engine through the exact shipped `compute_path_summary` query — review-0 #2), the
//! replicated scan-language mapping (pinned to the indexer's through the existing
//! dev-dependency), and the summary serve helpers. The stateful compare/build/serve integration
//! lives in `orient_serve::tests` (the decorator's GREEN/RED end-to-end proofs, beside its
//! sibling leaf proofs).

use super::*;
use repo_graph_livegraph::{StructuralFileRow, StructuralInventoryAnswer};

fn row(path: &str, has_file_node: bool, symbols: u64) -> StructuralFileRow {
    StructuralFileRow {
        path: path.to_string(),
        has_file_node,
        ast_symbol_count: symbols,
    }
}

fn inv(files: Vec<StructuralFileRow>, unattributed: u64) -> StructuralInventoryAnswer {
    StructuralInventoryAnswer {
        files,
        unattributed_symbols: unattributed,
        contributing_epochs: std::collections::BTreeMap::new(),
    }
}

// ── sql_like_prefix_match: the FAITHFUL replication of `path LIKE '{p}/%' OR path = {p}` ────────

#[test]
fn like_prefix_plain_segment_semantics() {
    assert!(sql_like_prefix_match("src", "src/a.ts"));
    assert!(sql_like_prefix_match("src", "src/deep/b.ts"));
    assert!(sql_like_prefix_match("src", "src"), "the `=` arm");
    assert!(
        !sql_like_prefix_match("src", "srcx/a.ts"),
        "segment boundary"
    );
    assert!(
        !sql_like_prefix_match("src/co", "src/core/x.ts"),
        "partial segment"
    );
}

#[test]
fn like_prefix_is_ascii_case_insensitive_on_the_like_arm_only() {
    // SQLite default LIKE is ASCII-case-insensitive: prefix 'src' matches 'SRC/…'.
    assert!(sql_like_prefix_match("src", "SRC/a.ts"));
    assert!(sql_like_prefix_match("SRC", "src/a.ts"));
    // The `=` arm is BINARY: 'SRC' alone does NOT equal prefix 'src' — but the LIKE arm needs a
    // trailing '/', so a bare case-mismatched dir path does not match.
    assert!(!sql_like_prefix_match("src", "SRC"));
}

#[test]
fn like_prefix_underscore_and_percent_are_wildcards_as_shipped() {
    // The shipped SQLite predicate treats prefix metacharacters as wildcards — replicated
    // deliberately (byte-identity with the SQLite serve; the latent defect is surfaced, not fixed).
    assert!(sql_like_prefix_match("my_mod", "my_mod/a.ts"));
    assert!(
        sql_like_prefix_match("my_mod", "my-mod/a.ts"),
        "`_` = any one character"
    );
    assert!(!sql_like_prefix_match("my_mod", "my--mod/a.ts"));
    assert!(
        sql_like_prefix_match("a%", "anything/deep/x.ts"),
        "`%` = any sequence"
    );
}

#[test]
fn like_underscore_matches_one_unicode_character_not_one_byte() {
    // review-0 #2: SQLite's `LIKE` reads UTF-8 by CODE POINT (`Utf8Read`), so `_` consumes exactly
    // one CHARACTER. Reviewer-executed ground truth: `SELECT 'aéb/x.ts' LIKE 'a_b/%';` → 1
    // ('é' = 2 bytes, 1 code point). A byte-wise `_` returns false here — the regression this pins.
    assert!(
        sql_like_prefix_match("a_b", "aéb/x.ts"),
        "`_` matches the single code point 'é' (SQLite semantics), not a single byte"
    );
    // Exactly ONE character: a two-char run must not match.
    assert!(!sql_like_prefix_match("a_b", "axyb/x.ts"));
    // Non-ASCII characters never case-fold (SQLite folds only when BOTH code points are ASCII).
    assert!(sql_like_prefix_match("sré", "sré/x.ts"));
    assert!(
        !sql_like_prefix_match("sré", "srÉ/x.ts"),
        "no Unicode case folding — SQLite LIKE is ASCII-case-insensitive only"
    );
    // ASCII case folding still applies around a `_`.
    assert!(sql_like_prefix_match("a_b", "A_B/x.ts"));
}

/// The GROUND-TRUTH pin (review-0 #2): [`sql_like_prefix_match`] vs the REAL SQLite engine through
/// the EXACT shipped `compute_path_summary` query (`f.path LIKE '{prefix}/%' OR f.path = ?`, no
/// ESCAPE) — the same pin-against-the-real-thing pattern `scan_language_matches_indexer_detect_language`
/// uses for the language mapping. For every prefix the full `AgentRepoSummary` from real SQLite must
/// equal [`path_summary_from_inventory`] over an inventory carrying the SAME paths, AND match a
/// hand-derived expected count (so an identical bug on both sides cannot hide behind parity).
#[test]
fn like_prefix_matches_real_sqlite_like() {
    use repo_graph_agent::AgentStorageRead;
    use repo_graph_storage::types::{CreateSnapshotInput, Repo, TrackedFile};
    use repo_graph_storage::StorageConnection;

    const PATHS: [&str; 9] = [
        "a_b/x.ts",  // literal underscore directory
        "aéb/y.ts",  // the reviewer's Unicode case: `_` matches the one code point 'é'
        "axb/z.ts",  // ASCII single-char wildcard match
        "axyb/w.ts", // `_` must NOT span two characters
        "A_B/u.ts",  // ASCII case-insensitive + underscore
        "src/e.ts",  // plain segment
        "SRC/f.ts",  // ASCII case-insensitive segment
        "srcx/g.ts", // segment boundary (no match for 'src')
        "sré/h.ts",  // non-ASCII exact (never case-folded)
    ];
    // (prefix, expected file_count under SQLite-default LIKE semantics)
    const CASES: [(&str, u64); 5] = [
        ("a_b", 4), // a_b, aéb, axb, A_B — NOT axyb
        ("src", 2), // src, SRC — NOT srcx
        ("sré", 1), // exact only; srÉ would not match (none present)
        ("a%", 5),  // a_b, aéb, axb, axyb, A_B ('%' any sequence, ASCII-CI 'a')
        ("srcx", 1),
    ];

    let dir = tempfile::tempdir().unwrap();
    let mut conn = StorageConnection::open(dir.path().join("like.db")).expect("open storage");
    conn.add_repo(&Repo {
        repo_uid: "repo_like".into(),
        name: "repo_like".into(),
        root_path: ".".into(),
        default_branch: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        metadata_json: None,
    })
    .expect("add repo");
    let snapshot_uid = conn
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: "repo_like".into(),
            kind: "full".into(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: None,
            label: None,
            toolchain_json: None,
        })
        .expect("create snapshot")
        .snapshot_uid;
    let tracked: Vec<TrackedFile> = PATHS
        .iter()
        .map(|path| TrackedFile {
            file_uid: format!("fuid::{path}"),
            repo_uid: "repo_like".into(),
            path: (*path).into(),
            language: Some("typescript".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        })
        .collect();
    conn.upsert_files(&tracked).expect("upsert files");
    let versions: Vec<repo_graph_storage::types::FileVersion> = PATHS
        .iter()
        .map(|path| repo_graph_storage::types::FileVersion {
            snapshot_uid: snapshot_uid.clone(),
            file_uid: format!("fuid::{path}"),
            content_hash: "h".into(),
            ast_hash: None,
            extractor: Some("test".into()),
            parse_status: "parsed".into(),
            size_bytes: Some(1),
            line_count: Some(1),
            indexed_at: "2026-01-01T00:00:00Z".into(),
        })
        .collect();
    conn.upsert_file_versions(&versions)
        .expect("upsert file versions");

    let inventory = inv(PATHS.iter().map(|p| row(p, true, 0)).collect(), 0);
    for (prefix, expected_files) in CASES {
        let sqlite = AgentStorageRead::compute_path_summary(&conn, &snapshot_uid, prefix)
            .expect("compute_path_summary ok");
        let livegraph = path_summary_from_inventory(&inventory, prefix);
        assert_eq!(
            (sqlite.file_count, &sqlite.symbol_count, &sqlite.languages),
            (
                livegraph.file_count,
                &livegraph.symbol_count,
                &livegraph.languages
            ),
            "prefix {prefix:?}: the LiveGraph LIKE replication diverges from REAL SQLite LIKE"
        );
        assert_eq!(
            sqlite.file_count, expected_files,
            "prefix {prefix:?}: real SQLite disagrees with the hand-derived LIKE semantics"
        );
    }
}

// ── scan_language: pinned byte-for-byte to the indexer's mapping (the drift guard) ──────────────

#[test]
fn scan_language_matches_indexer_detect_language() {
    // The replicated mapping must equal `repo_graph_indexer::routing::detect_language` for every
    // extension either side knows, plus non-code/edge shapes. The indexer crate is reachable here
    // through the EXISTING dev-dependency — no production edge.
    let cases = [
        "a.ts",
        "a.mts",
        "a.cts",
        "a.tsx",
        "a.js",
        "a.mjs",
        "a.cjs",
        "a.jsx",
        "a.java",
        "a.py",
        "a.rs",
        "a.c",
        "a.h",
        "a.cpp",
        "a.cc",
        "a.cxx",
        "a.hpp",
        "a.hxx",
        "README.md",
        "package.json",
        "noext",
        "dir.with.dots/file.ts",
        "x.proto",
        "a.TS",
    ];
    for path in cases {
        assert_eq!(
            scan_language(path),
            repo_graph_indexer::routing::detect_language(path),
            "mapping drift for {path}"
        );
    }
}

// ── summary serve helpers ────────────────────────────────────────────────────────────────────────

#[test]
fn repo_summary_counts_files_symbols_languages() {
    let inventory = inv(
        vec![
            row("main.ts", true, 2),
            row("src/a.ts", true, 3),
            row("src/b.tsx", true, 0),
            // A symbols-only anomaly path: counted in the symbol TOTAL, never as a file.
            row("src/ghost.ts", false, 1),
        ],
        4, // unattributed symbols count into the repo total (the compute_repo_summary mirror)
    );
    let s = repo_summary_from_inventory(&inventory);
    assert_eq!(s.file_count, 3);
    assert_eq!(s.symbol_count, 2 + 3 + 1 + 4);
    assert_eq!(
        s.languages,
        vec!["tsx".to_string(), "typescript".to_string()]
    );
}

#[test]
fn path_summary_filters_by_prefix_and_excludes_unattributed() {
    let inventory = inv(
        vec![
            row("main.ts", true, 2),
            row("src/a.ts", true, 3),
            row("src/deep/b.ts", true, 5),
        ],
        7,
    );
    let s = path_summary_from_inventory(&inventory, "src");
    assert_eq!(s.file_count, 2);
    assert_eq!(
        s.symbol_count, 8,
        "unattributed NEVER counts in a path scope"
    );
    assert_eq!(s.languages, vec!["typescript".to_string()]);
}

#[test]
fn file_summary_is_exact_binary_match() {
    let inventory = inv(vec![row("src/a.ts", true, 3), row("SRC/a.ts", true, 9)], 0);
    let s = file_summary_from_inventory(&inventory, "src/a.ts");
    assert_eq!(
        (s.file_count, s.symbol_count),
        (1, 3),
        "SQL `=` is case-sensitive"
    );
    assert_eq!(s.languages, vec!["typescript".to_string()]);
}

#[test]
fn dirname_module_buckets_root_as_empty() {
    assert_eq!(dirname_module("main.ts"), "");
    assert_eq!(dirname_module("src/a.ts"), "src");
    assert_eq!(dirname_module("src/deep/b.ts"), "src/deep");
}
