//! TS-ALIAS-RESOLUTION-1 (RG-REQ-006-L04 first clause; RG-REQ-002-L11; RG-REQ-001-L09) — a
//! TypeScript `paths` alias import, indexed from source through the FULL stack (compose →
//! ts-extractor → indexer resolver → storage). Check TSA-C02.
//!
//! The defect (RC-2 resolver side, RC-6 references layout, `docs/audits/2026-10-03-root-causes-v0.20.0.md`):
//! amodx `admin/src/components/editor/Toolbar.tsx:3` `import { Button } from "@/components/ui/button";`
//! with `admin/tsconfig.app.json` mapping `"@/*": ["./src/*"]` stayed an unresolved row labelled
//! `specifier_matches_project_alias` — the index knew it was an alias and could not follow it.
//! Now the alias stage binds it STATIC to the file TypeScript's selection reaches, when exactly one
//! inspected tsconfig project covers the importing file (D-TSA-BOUNDED-SCOPE-1).
//!
//! Every edge is read back through storage's `find_imports` and every unresolved row through
//! `find_unresolved_file_imports` — the two readers of the per-file listing.

use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_repo_index::compose::{
    index_into_storage, index_path, refresh_into_storage, ComposeOptions,
};
use repo_graph_storage::queries::ImportReason;
use repo_graph_storage::StorageConnection;

fn write(repo: &Path, rel: &str, content: &str) {
    let path = repo.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Index `repo` into a fresh store; returns (the store's temp dir, the db path, the snapshot uid).
fn index(repo: &Path, repo_uid: &str) -> (tempfile::TempDir, PathBuf, String) {
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, repo_uid, &ComposeOptions::default()).unwrap();
    (db_dir, db, result.snapshot_uid)
}

/// One `find_imports` row reduced to (target file, resolution, reason).
type ImportRow = (String, Option<String>, Option<ImportReason>);

fn imports_of(db: &Path, snap: &str, repo_uid: &str, from: &str) -> Vec<ImportRow> {
    let storage = StorageConnection::open(db).unwrap();
    storage
        .find_imports(snap, &format!("{repo_uid}:{from}:FILE"))
        .unwrap()
        .into_iter()
        .map(|r| (r.file, r.resolution, r.reason))
        .collect()
}

/// One `find_unresolved_file_imports` row reduced to (target key, category, basis code, recorded
/// basis, the candidates as the reader decodes them — `Paths([..])`, `NotRecorded`, `Unreadable(..)`).
type UnresolvedRow = (String, String, String, Option<String>, String);

fn unresolved_of(db: &Path, snap: &str, from: &str) -> Vec<UnresolvedRow> {
    let storage = StorageConnection::open(db).unwrap();
    storage
        .find_unresolved_file_imports(snap, from)
        .unwrap()
        .into_iter()
        .map(|u| {
            (
                u.target_key,
                u.category,
                u.basis_code,
                u.basis,
                format!("{:?}", u.candidates),
            )
        })
        .collect()
}

/// The stored alias signal of one file (`file_signals.tsconfig_aliases_json`), parsed.
fn alias_signal(db: &Path, snap: &str, path: &str) -> Option<serde_json::Value> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT g.tsconfig_aliases_json FROM file_signals g \
             JOIN files f ON f.file_uid = g.file_uid \
             WHERE g.snapshot_uid = ? AND f.path = ?",
            rusqlite::params![snap, path],
            |r| r.get(0),
        )
        .ok();
    raw.flatten()
        .map(|j| serde_json::from_str(&j).expect("a JSON alias signal"))
}

/// The persisted MODULE → MODULE import pairs, as (source module key, target module key, resolution).
fn module_import_pairs(db: &Path, snap: &str) -> Vec<(String, String, String)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sn.stable_key, tn.stable_key, e.resolution
               FROM edges e
               JOIN nodes sn ON sn.node_uid = e.source_node_uid
               JOIN nodes tn ON tn.node_uid = e.target_node_uid
              WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS'
                AND sn.kind = 'MODULE' AND tn.kind = 'MODULE'
              ORDER BY 1, 2",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn tsconfig_paths_reason(target: &str) -> Option<ImportReason> {
    Some(ImportReason::Recorded {
        basis: "tsconfig_paths".into(),
        candidates: vec![target.to_string()],
    })
}

/// The amodx `admin` layout: a root `tsconfig.json` with `files: []`, two references and its own
/// `paths`; `tsconfig.app.json` (`include: ["src"]`) covers `src/**`; `tsconfig.node.json` covers
/// only `vite.config.ts`.
fn amodx_admin_repo(repo: &Path) {
    write(
        repo,
        "admin/tsconfig.json",
        r#"{
  "files": [],
  "references": [
    { "path": "./tsconfig.app.json" },
    { "path": "./tsconfig.node.json" }
  ],
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    }
  }
}"#,
    );
    write(
        repo,
        "admin/tsconfig.app.json",
        r#"{
  "compilerOptions": {
    /* Bundler mode */
    "moduleResolution": "bundler",
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src"],
  "exclude": ["src/**/*.test.ts"]
}"#,
    );
    write(
        repo,
        "admin/tsconfig.node.json",
        r#"{ "compilerOptions": { "module": "ESNext" }, "include": ["vite.config.ts"] }"#,
    );
    write(
        repo,
        "admin/vite.config.ts",
        "export default { plugins: [] };\n",
    );
    write(
        repo,
        "admin/src/components/editor/Toolbar.tsx",
        "import { Button } from \"@/components/ui/button\";\n\
         export function Toolbar() { return Button(); }\n",
    );
    write(
        repo,
        "admin/src/components/ui/button.tsx",
        "export function Button() { return 1; }\n",
    );
}

#[test]
fn alias_import_resolves_static_end_to_end_under_a_nested_package_tsconfig() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    amodx_admin_repo(repo);
    let (_d, db, snap) = index(repo, "am");

    assert_eq!(
        imports_of(&db, &snap, "am", "admin/src/components/editor/Toolbar.tsx"),
        vec![(
            "admin/src/components/ui/button.tsx".to_string(),
            Some("static".to_string()),
            tsconfig_paths_reason("admin/src/components/ui/button.tsx"),
        )],
        "ONE static edge to the file the compiler binds, its basis and the target as its one \
         candidate"
    );
    assert!(
        unresolved_of(&db, &snap, "admin/src/components/editor/Toolbar.tsx").is_empty(),
        "no unresolved row remains for the bound import"
    );
    // A static alias edge is a certain import: it feeds the persisted module graph.
    assert!(
        module_import_pairs(&db, &snap).contains(&(
            "am:admin/src/components/editor:MODULE".to_string(),
            "am:admin/src/components/ui:MODULE".to_string(),
            "static".to_string()
        )),
        "{:?}",
        module_import_pairs(&db, &snap)
    );
    // The stored signal: `entries` are the nearest tsconfig.json's own (as at HEAD); the mapping
    // is the sole inspected covering project's (tsconfig.app.json), anchored at its directory.
    let signal = alias_signal(&db, &snap, "admin/src/components/editor/Toolbar.tsx").unwrap();
    assert_eq!(
        signal,
        serde_json::json!({
            "entries": [{"pattern": "@/*", "substitutions": ["./src/*"]}],
            "soleInspectedCoveringProjectMapping": {
                "anchorDir": "admin",
                "baseUrl": ".",
                "entries": [{"pattern": "@/*", "substitutions": ["./src/*"]}],
            },
        })
    );
}

#[test]
fn alias_import_under_a_references_layout_resolves_static_end_to_end() {
    // The hexmanos layout: the nearest tsconfig.json holds only `references`; the `paths` live in
    // the referenced tsconfig.app.json.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "frontend/tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" }, { "path": "./tsconfig.node.json" } ] }"#,
    );
    write(
        repo,
        "frontend/tsconfig.app.json",
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    },
    "jsx": "react-jsx"
  },
  "include": ["src"]
}"#,
    );
    write(
        repo,
        "frontend/tsconfig.node.json",
        r#"{ "include": ["vite.config.ts"] }"#,
    );
    write(
        repo,
        "frontend/src/App.tsx",
        "import { EditorPage } from \"@/features/editor\";\nexport const App = EditorPage;\n",
    );
    write(
        repo,
        "frontend/src/features/editor/index.ts",
        "export const EditorPage = 1;\n",
    );
    let (_d, db, snap) = index(repo, "hx");

    assert_eq!(
        imports_of(&db, &snap, "hx", "frontend/src/App.tsx"),
        vec![(
            "frontend/src/features/editor/index.ts".to_string(),
            Some("static".to_string()),
            tsconfig_paths_reason("frontend/src/features/editor/index.ts"),
        )]
    );
    assert!(unresolved_of(&db, &snap, "frontend/src/App.tsx").is_empty());
    let signal = alias_signal(&db, &snap, "frontend/src/App.tsx").unwrap();
    assert_eq!(
        signal["entries"],
        serde_json::json!([]),
        "the nearest tsconfig.json declares no paths: entries stay HEAD's"
    );
    assert_eq!(
        signal["soleInspectedCoveringProjectMapping"]["anchorDir"],
        "frontend"
    );
}

#[test]
fn alias_import_without_an_indexed_target_stays_an_unresolved_project_alias_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "tsconfig.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#,
    );
    write(
        repo,
        "src/main.ts",
        "import { gone } from \"@/missing\";\nexport const y = gone;\n",
    );
    let (_d, db, snap) = index(repo, "nm");

    assert!(imports_of(&db, &snap, "nm", "src/main.ts").is_empty());
    assert_eq!(
        unresolved_of(&db, &snap, "src/main.ts"),
        vec![(
            "@/missing".to_string(),
            "imports_file_not_found".to_string(),
            "specifier_matches_project_alias".to_string(),
            None,
            "NotRecorded".to_string(),
        )],
        "no workspace package of that name: the row stays exactly HEAD's project-alias row"
    );
}

#[test]
fn alias_import_whose_one_inspected_covering_project_reaches_two_indexed_files_stays_unresolved_with_both_candidates_end_to_end(
) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "tsconfig.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./lib/*"] } } }"#,
    );
    write(
        repo,
        "src/main.ts",
        "import { x } from \"@/x\";\nexport const y = x;\n",
    );
    write(repo, "lib/x.ts", "export const x = 1;\n");
    write(repo, "lib/x/index.ts", "export const x = 2;\n");
    let (_d, db, snap) = index(repo, "sv");

    assert!(
        imports_of(&db, &snap, "sv", "src/main.ts").is_empty(),
        "never a pick"
    );
    assert_eq!(
        unresolved_of(&db, &snap, "src/main.ts"),
        vec![(
            "@/x".to_string(),
            "imports_file_not_found".to_string(),
            "specifier_matches_project_alias".to_string(),
            Some("ambiguous_tsconfig_paths".to_string()),
            r#"Paths(["lib/x.ts", "lib/x/index.ts"])"#.to_string(),
        )]
    );
}

#[test]
fn a_file_covered_by_two_inspected_projects_takes_no_aliases_from_the_stage() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.a.json" }, { "path": "./tsconfig.b.json" } ] }"#,
    );
    for name in ["tsconfig.a.json", "tsconfig.b.json"] {
        write(
            repo,
            name,
            r#"{ "compilerOptions": { "paths": { "@/*": ["./one/*"] } }, "include": ["src"] }"#,
        );
    }
    write(
        repo,
        "src/main.ts",
        "import { x } from \"@/x\";\nexport const y = x;\n",
    );
    write(repo, "one/x.ts", "export const x = 1;\n");
    let (_d, db, snap) = index(repo, "tp");

    assert!(
        imports_of(&db, &snap, "tp", "src/main.ts").is_empty(),
        "two inspected projects cover the file: nothing bound, even when they agree"
    );
    let rows = unresolved_of(&db, &snap, "src/main.ts");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (target, category, basis_code, basis, candidates) = &rows[0];
    assert_eq!(target, "@/x");
    assert_eq!(category, "imports_file_not_found");
    assert_eq!(basis, &None, "the stage adds no reason");
    assert_eq!(candidates, "NotRecorded", "the stage adds no candidate");
    assert_ne!(
        basis_code, "specifier_matches_project_alias",
        "the nearest tsconfig.json declares no paths, so the classifier's input is HEAD's"
    );
    let signal = alias_signal(&db, &snap, "src/main.ts");
    assert!(
        signal
            .as_ref()
            .is_none_or(|s| s.get("soleInspectedCoveringProjectMapping").is_none()),
        "no mapping is stored for a file two inspected projects cover: {signal:?}"
    );
}

#[test]
fn alias_import_under_a_root_tsconfig_with_empty_files_binds_through_the_covering_reference_end_to_end(
) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" } ], "compilerOptions": { "paths": { "@/*": ["./root/*"] } } }"#,
    );
    write(
        repo,
        "tsconfig.app.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./child/*"] } }, "include": ["src"] }"#,
    );
    write(
        repo,
        "src/main.ts",
        "import { x } from \"@/x\";\nexport const y = x;\n",
    );
    write(repo, "root/x.ts", "export const x = 1;\n");
    write(repo, "child/x.ts", "export const x = 2;\n");
    let (_d, db, snap) = index(repo, "rf");

    assert_eq!(
        imports_of(&db, &snap, "rf", "src/main.ts"),
        vec![(
            "child/x.ts".to_string(),
            Some("static".to_string()),
            tsconfig_paths_reason("child/x.ts"),
        )],
        "the root covers no file; its reference is the sole inspected covering project — never the root's \
         mapping"
    );
    assert!(unresolved_of(&db, &snap, "src/main.ts").is_empty());
}

#[test]
fn refresh_re_resolves_an_alias_import_when_its_target_file_is_added() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "tsconfig.json",
        r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./src/*"] } } }"#,
    );
    write(
        repo,
        "src/main.ts",
        "import { util } from \"@/lib/util\";\nexport const y = util;\n",
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let mut storage = StorageConnection::open(&db).unwrap();
    let r1 = index_into_storage(repo, &mut storage, "rr", &ComposeOptions::default()).unwrap();
    assert!(imports_of(&db, &r1.snapshot_uid, "rr", "src/main.ts").is_empty());
    assert_eq!(unresolved_of(&db, &r1.snapshot_uid, "src/main.ts").len(), 1);

    write(repo, "src/lib/util.ts", "export const util = 1;\n");
    let r2 = refresh_into_storage(repo, &mut storage, "rr", &ComposeOptions::default()).unwrap();
    assert_eq!(
        imports_of(&db, &r2.snapshot_uid, "rr", "src/main.ts"),
        vec![(
            "src/lib/util.ts".to_string(),
            Some("static".to_string()),
            tsconfig_paths_reason("src/lib/util.ts"),
        )],
        "the copied-forward import binds once its target is indexed"
    );
    assert!(unresolved_of(&db, &r2.snapshot_uid, "src/main.ts").is_empty());
}

#[test]
fn refresh_rebinds_an_alias_import_when_only_its_referenced_project_paths_change() {
    // D-TSA-REFRESH-SCOPE-1, the hexmanos layout: the nearest `tsconfig.json` holds only
    // `references`; the referenced `tsconfig.app.json` supplies `@/*`. Only that file changes.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "frontend/tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" } ] }"#,
    );
    write(
        repo,
        "frontend/tsconfig.app.json",
        r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./src/a/*"] } }, "include": ["src"] }"#,
    );
    write(
        repo,
        "frontend/src/App.tsx",
        "import { x } from \"@/x\";\nexport const y = x;\n",
    );
    write(repo, "frontend/src/a/x.ts", "export const x = 1;\n");
    write(repo, "frontend/src/b/x.ts", "export const x = 2;\n");
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let mut storage = StorageConnection::open(&db).unwrap();
    let r1 = index_into_storage(repo, &mut storage, "rr", &ComposeOptions::default()).unwrap();
    assert_eq!(
        imports_of(&db, &r1.snapshot_uid, "rr", "frontend/src/App.tsx"),
        vec![(
            "frontend/src/a/x.ts".to_string(),
            Some("static".to_string()),
            tsconfig_paths_reason("frontend/src/a/x.ts"),
        )]
    );

    write(
        repo,
        "frontend/tsconfig.app.json",
        r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./src/b/*"] } }, "include": ["src"] }"#,
    );
    let r2 = refresh_into_storage(repo, &mut storage, "rr", &ComposeOptions::default()).unwrap();
    assert_eq!(
        imports_of(&db, &r2.snapshot_uid, "rr", "frontend/src/App.tsx"),
        vec![(
            "frontend/src/b/x.ts".to_string(),
            Some("static".to_string()),
            tsconfig_paths_reason("frontend/src/b/x.ts"),
        )],
        "the unchanged importer is re-extracted under the referenced project's new mapping; no \
         edge reaches the old target"
    );
    assert!(unresolved_of(&db, &r2.snapshot_uid, "frontend/src/App.tsx").is_empty());
}

// ── RG-REQ-001-L09: the same tree indexed twice yields the same alias facts ──

/// IMPORTS edges as (source stable key, target stable key, resolution, carrier), sorted.
fn all_import_edges(db: &Path, snap: &str) -> Vec<(String, String, String, Option<String>)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sn.stable_key, tn.stable_key, e.resolution, e.metadata_json
               FROM edges e
               JOIN nodes sn ON sn.node_uid = e.source_node_uid
               JOIN nodes tn ON tn.node_uid = e.target_node_uid
              WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS'
              ORDER BY 1, 2, 3, 4",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// Unresolved IMPORTS rows as (source stable key, target key, category, classification, basis
/// code, carrier), sorted.
type StoredUnresolved = (String, String, String, String, String, Option<String>);

fn all_unresolved_imports(db: &Path, snap: &str) -> Vec<StoredUnresolved> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sn.stable_key, u.target_key, u.category, u.classification, u.basis_code,
                    u.metadata_json
               FROM unresolved_edges u
               JOIN nodes sn ON sn.node_uid = u.source_node_uid
              WHERE u.snapshot_uid = ? AND u.type = 'IMPORTS'
              ORDER BY 1, 2, 3, 4, 5, 6",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// Every file's stored alias signal, keyed by path.
fn all_alias_signals(db: &Path, snap: &str) -> Vec<(String, Option<String>)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT f.path, g.tsconfig_aliases_json FROM file_signals g
               JOIN files f ON f.file_uid = g.file_uid
              WHERE g.snapshot_uid = ? ORDER BY 1",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn indexing_the_same_alias_fixture_twice_yields_identical_edges_rows_and_alias_signals() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    // A nested-package layout.
    amodx_admin_repo(repo);
    // A references layout whose two directories hold files covered by different referenced projects.
    write(
        repo,
        "web/tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" }, { "path": "./tsconfig.tools.json" } ] }"#,
    );
    write(
        repo,
        "web/tsconfig.app.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./src/*"] } }, "include": ["src"] }"#,
    );
    write(
        repo,
        "web/tsconfig.tools.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./tools/lib/*"] } }, "include": ["tools"] }"#,
    );
    write(
        repo,
        "web/src/main.ts",
        "import { a } from \"@/a\";\nexport const m = a;\n",
    );
    write(repo, "web/src/a.ts", "export const a = 1;\n");
    write(
        repo,
        "web/tools/run.ts",
        "import { t } from \"@/t\";\nexport const r = t;\n",
    );
    write(repo, "web/tools/lib/t.ts", "export const t = 1;\n");
    // Two referenced projects covering one file.
    write(
        repo,
        "dual/tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.a.json" }, { "path": "./tsconfig.b.json" } ] }"#,
    );
    write(
        repo,
        "dual/tsconfig.a.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./one/*"] } }, "include": ["src"] }"#,
    );
    write(
        repo,
        "dual/tsconfig.b.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./two/*"] } }, "include": ["src"] }"#,
    );
    write(
        repo,
        "dual/src/main.ts",
        "import { x } from \"@/x\";\nexport const y = x;\n",
    );
    write(repo, "dual/one/x.ts", "export const x = 1;\n");
    write(repo, "dual/two/x.ts", "export const x = 2;\n");
    // A root `files: []` layout.
    write(
        repo,
        "rootfiles/tsconfig.json",
        r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" } ], "compilerOptions": { "paths": { "@/*": ["./root/*"] } } }"#,
    );
    write(
        repo,
        "rootfiles/tsconfig.app.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./child/*"] } }, "include": ["src"] }"#,
    );
    write(
        repo,
        "rootfiles/src/main.ts",
        "import { x } from \"@/x\";\nexport const y = x;\n",
    );
    write(repo, "rootfiles/root/x.ts", "export const x = 1;\n");
    write(repo, "rootfiles/child/x.ts", "export const x = 2;\n");

    let (_d1, db1, snap1) = index(repo, "fx");
    let (_d2, db2, snap2) = index(repo, "fx");

    let edges1 = all_import_edges(&db1, &snap1);
    assert_eq!(edges1, all_import_edges(&db2, &snap2));
    assert_eq!(
        all_unresolved_imports(&db1, &snap1),
        all_unresolved_imports(&db2, &snap2)
    );
    let signals1 = all_alias_signals(&db1, &snap1);
    assert_eq!(signals1, all_alias_signals(&db2, &snap2));

    // The fixture exercises the stage: each layout binds where the one-project rule says.
    for (src, tgt) in [
        (
            "admin/src/components/editor/Toolbar.tsx",
            "admin/src/components/ui/button.tsx",
        ),
        ("web/src/main.ts", "web/src/a.ts"),
        ("web/tools/run.ts", "web/tools/lib/t.ts"),
        ("rootfiles/src/main.ts", "rootfiles/child/x.ts"),
    ] {
        assert!(
            edges1
                .iter()
                .any(|(s, t, res, _)| s == &format!("fx:{src}:FILE")
                    && t == &format!("fx:{tgt}:FILE")
                    && res == "static"),
            "{src} -> {tgt}: {edges1:?}"
        );
    }
    assert!(
        !edges1
            .iter()
            .any(|(s, _, _, _)| s == "fx:dual/src/main.ts:FILE"),
        "two inspected projects: nothing bound"
    );
    assert!(
        signals1.iter().any(|(_, j)| j
            .as_deref()
            .is_some_and(|j| j.contains("soleInspectedCoveringProjectMapping"))),
        "at least one stored signal carries a mapping"
    );
}
