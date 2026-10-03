//! TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04 workspace-member clause; RG-REQ-002-L11) — an import
//! of the repository's own npm workspace package, indexed from source through the FULL stack
//! (compose → ts-extractor → indexer resolver → storage). Check TWR-C02.
//!
//! The defect (RC-6 of the v0.19.0 root causes): FRAKTAG `packages/api/src/server.ts:6`
//! `import { Fraktag } from '@fraktag/engine';` names a workspace member whose declared entry
//! (`main: dist/index.js`) is build output the index never holds, while its source entry
//! `packages/engine/src/index.ts` is indexed. Before this slice the import was an unresolved
//! external candidate. Now it is ONE inferred IMPORTS edge to the source entry with every candidate
//! and the reason recorded, never fed to the persisted module graph.

use std::fs;
use std::path::Path;

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

fn carrier_of(raw: Option<String>) -> serde_json::Value {
    serde_json::from_str(raw.as_deref().expect("a stored carrier")).expect("a JSON carrier")
}

/// An IMPORTS edge as stored: (source file path, target stable key, resolution, carrier).
type StoredImport = (String, String, String, serde_json::Value);
/// An unresolved IMPORTS row as stored: (source file path, target key, carrier).
type StoredUnresolvedImport = (String, String, serde_json::Value);

// Storage diagnostics of what the pipeline wrote, read through a second raw SQLite connection.

fn stored_imports_from(db: &Path, snap: &str, from: &str) -> Vec<StoredImport> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sf.path, tn.stable_key, e.resolution, e.metadata_json
               FROM edges e
               JOIN nodes sn ON sn.node_uid = e.source_node_uid
               JOIN nodes tn ON tn.node_uid = e.target_node_uid
               JOIN files sf ON sf.file_uid = sn.file_uid
              WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS' AND sn.kind = 'FILE'
                AND sf.path = ?
              ORDER BY tn.stable_key",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![snap, from], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap();
    rows.map(|r| {
        let (src, tgt, res, md) = r.unwrap();
        (src, tgt, res, carrier_of(md))
    })
    .collect()
}

fn stored_unresolved_from(db: &Path, snap: &str, from: &str) -> Vec<StoredUnresolvedImport> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sf.path, u.target_key, u.metadata_json
               FROM unresolved_edges u
               JOIN nodes sn ON sn.node_uid = u.source_node_uid
               JOIN files sf ON sf.file_uid = sn.file_uid
              WHERE u.snapshot_uid = ? AND u.type = 'IMPORTS' AND sf.path = ?
              ORDER BY u.target_key",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![snap, from], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?))
        })
        .unwrap();
    rows.map(|r| {
        let (src, tk, md) = r.unwrap();
        (src, tk, carrier_of(md))
    })
    .collect()
}

/// The persisted MODULE → MODULE import pairs, as (source module key, target module key).
fn module_import_pairs(db: &Path, snap: &str) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sn.stable_key, tn.stable_key
               FROM edges e
               JOIN nodes sn ON sn.node_uid = e.source_node_uid
               JOIN nodes tn ON tn.node_uid = e.target_node_uid
              WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS'
                AND sn.kind = 'MODULE' AND tn.kind = 'MODULE'
              ORDER BY 1, 2",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// The npm module candidates and their evidence payloads, as (module key, root, payload).
fn npm_candidates(db: &Path, snap: &str) -> Vec<(String, String, String)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT m.module_key, m.canonical_root_path, e.payload_json
               FROM module_candidates m
               JOIN module_candidate_evidence e ON e.module_candidate_uid = m.module_candidate_uid
              WHERE m.snapshot_uid = ? AND m.module_key LIKE 'npm:%'
              ORDER BY 1",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

const SERVER_TS: &str = "import { Fraktag } from '@fraktag/engine';\n\
export function start() { return new Fraktag(); }\n";
const ENGINE_INDEX_TS: &str = "export class Fraktag { run() { return 1; } }\n";

/// FRAKTAG's shape: array workspaces, the engine declares `main: dist/index.js`.
fn fraktag_repo(repo: &Path, engine_manifest: &str) {
    write(
        repo,
        "package.json",
        r#"{"name":"fraktag","private":true,"workspaces":["packages/*"]}"#,
    );
    write(repo, "packages/engine/package.json", engine_manifest);
    write(repo, "packages/engine/src/index.ts", ENGINE_INDEX_TS);
    write(
        repo,
        "packages/api/package.json",
        r#"{"name":"@fraktag/api","dependencies":{"@fraktag/engine":"*","express":"^4"}}"#,
    );
    write(repo, "packages/api/src/server.ts", SERVER_TS);
}

#[test]
fn workspace_member_import_resolves_inferred_to_its_source_entry_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fraktag_repo(
        repo,
        r#"{"name":"@fraktag/engine","main":"dist/index.js","types":"dist/index.d.ts"}"#,
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "fk", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;

    assert_eq!(
        stored_imports_from(&db, snap, "packages/api/src/server.ts"),
        vec![(
            "packages/api/src/server.ts".to_string(),
            "fk:packages/engine/src/index.ts:FILE".to_string(),
            "inferred".to_string(),
            serde_json::json!({
                "rawPath": "@fraktag/engine",
                "isTypeOnly": false,
                "basis": "workspace_source_entry",
                "candidates": [
                    "fk:packages/engine/src/index.ts:FILE",
                    "fk:packages/engine/dist/index.js:FILE"
                ],
            }),
        )],
        "ONE inferred edge to the source entry, both candidates and the reason recorded, the \
         extractor's keys kept"
    );
    assert!(
        stored_unresolved_from(&db, snap, "packages/api/src/server.ts").is_empty(),
        "no unresolved row remains for the bound import"
    );
    assert!(
        !module_import_pairs(&db, snap).contains(&(
            "fk:packages/api:MODULE".to_string(),
            "fk:packages/engine:MODULE".to_string()
        )),
        "an inferred import never feeds the persisted module graph: {:?}",
        module_import_pairs(&db, snap)
    );

    // The storage read carries the reason with every candidate path.
    let storage = StorageConnection::open(&db).unwrap();
    let rows = storage
        .find_imports(snap, "fk:packages/api/src/server.ts:FILE")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].resolution.as_deref(), Some("inferred"));
    assert_eq!(
        rows[0].reason,
        Some(ImportReason::Recorded {
            basis: "workspace_source_entry".into(),
            candidates: vec![
                "packages/engine/src/index.ts".into(),
                "packages/engine/dist/index.js".into()
            ],
        })
    );
}

#[test]
fn object_form_workspaces_with_conditional_exports_resolve_inferred() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "package.json",
        r#"{"name":"amodx","private":true,"workspaces":{"packages":["packages/*"]}}"#,
    );
    write(
        repo,
        "packages/effects/package.json",
        r#"{"name":"@amodx/effects","main":"dist/index.js",
            "exports":{".":{"types":"./dist/index.d.ts","default":"./dist/index.js"}}}"#,
    );
    write(
        repo,
        "packages/effects/src/index.ts",
        "export const EFFECT_LIST = [1, 2];\n",
    );
    write(
        repo,
        "packages/plugins/package.json",
        r#"{"name":"@amodx/plugins"}"#,
    );
    write(
        repo,
        "packages/plugins/src/common/EffectControls.tsx",
        "import { EFFECT_LIST } from \"@amodx/effects\";\nexport const n = EFFECT_LIST.length;\n",
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "am", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;

    let imports = stored_imports_from(&db, snap, "packages/plugins/src/common/EffectControls.tsx");
    assert_eq!(imports.len(), 1, "{imports:?}");
    assert_eq!(imports[0].1, "am:packages/effects/src/index.ts:FILE");
    assert_eq!(imports[0].2, "inferred");
    assert_eq!(imports[0].3["basis"], "workspace_source_entry");
    assert_eq!(
        imports[0].3["candidates"],
        serde_json::json!([
            "am:packages/effects/src/index.ts:FILE",
            "am:packages/effects/dist/index.d.ts:FILE",
            "am:packages/effects/dist/index.js:FILE"
        ]),
        "both root-export targets recorded in manifest order, neither selected"
    );
}

#[test]
fn workspace_subpath_and_unexported_root_imports_stay_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "package.json",
        r#"{"name":"mono","private":true,"workspaces":["packages/*"]}"#,
    );
    // A member exporting subpaths only (no root export).
    write(
        repo,
        "packages/plugins/package.json",
        r#"{"name":"@x/plugins","main":"dist/index.js","exports":{"./admin":"./dist/admin.js"}}"#,
    );
    write(
        repo,
        "packages/plugins/src/index.ts",
        "export const a = 1;\n",
    );
    // A member that declares no entry (Node's implicit index.js is not a declared entry).
    write(
        repo,
        "packages/renderer/package.json",
        r#"{"name":"@x/renderer"}"#,
    );
    write(
        repo,
        "packages/renderer/src/index.ts",
        "export const r = 1;\n",
    );
    // A member whose declared target is indexed (storybook's `code` condition).
    write(
        repo,
        "packages/links/package.json",
        r#"{"name":"@x/links","exports":{".":{"types":"./dist/index.d.ts","code":"./src/index.ts","default":"./dist/index.js"}}}"#,
    );
    write(repo, "packages/links/src/index.ts", "export const l = 1;\n");
    write(repo, "packages/app/package.json", r#"{"name":"@x/app"}"#);
    write(
        repo,
        "packages/app/src/main.ts",
        "import { getPluginList } from \"@x/plugins/admin\";\n\
         import { a } from \"@x/plugins\";\n\
         import { r } from \"@x/renderer\";\n\
         import { l } from \"@x/links\";\n\
         export const all = [getPluginList, a, r, l];\n",
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "mx", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;

    assert!(
        stored_imports_from(&db, snap, "packages/app/src/main.ts").is_empty(),
        "no edge for a subpath, an unexported root, an undeclared entry or an indexed declared \
         target"
    );
    let unresolved = stored_unresolved_from(&db, snap, "packages/app/src/main.ts");
    let keys: Vec<&str> = unresolved.iter().map(|u| u.1.as_str()).collect();
    assert_eq!(
        keys,
        vec!["@x/links", "@x/plugins", "@x/plugins/admin", "@x/renderer"]
    );
    for (_, key, carrier) in &unresolved {
        assert_eq!(
            carrier,
            &serde_json::json!({"rawPath": key, "isTypeOnly": false}),
            "the carrier is the extractor's, untouched"
        );
    }
}

#[test]
fn repo_without_workspaces_leaves_a_bare_package_import_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    // A root package without `workspaces`: `packages/engine` is not a workspace member.
    write(
        repo,
        "package.json",
        r#"{"name":"app","dependencies":{"react":"^18"}}"#,
    );
    write(
        repo,
        "packages/engine/package.json",
        r#"{"name":"@fraktag/engine","main":"dist/index.js"}"#,
    );
    write(repo, "packages/engine/src/index.ts", ENGINE_INDEX_TS);
    write(
        repo,
        "src/app.ts",
        "import React from 'react';\nimport { Fraktag } from '@fraktag/engine';\n\
         export const x = [React, Fraktag];\n",
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "nw", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;

    assert!(stored_imports_from(&db, snap, "src/app.ts").is_empty());
    let keys: Vec<String> = stored_unresolved_from(&db, snap, "src/app.ts")
        .into_iter()
        .map(|u| u.1)
        .collect();
    assert_eq!(keys, vec!["@fraktag/engine", "react"]);
    // The npm module candidates are written as before: the root package only, its payload
    // carrying no declared entry.
    let candidates = npm_candidates(&db, snap);
    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert_eq!(candidates[0].1, ".");
    assert!(
        !candidates[0].2.contains("declared") && !candidates[0].2.contains("dist"),
        "the evidence payload is unchanged: {}",
        candidates[0].2
    );
}

#[test]
fn refresh_unbinds_the_inferred_import_when_the_declared_entry_becomes_indexed() {
    // `dist/` is never indexed (an always-excluded directory), so the declared entry here is
    // `lib/index.js`: a file that can appear in the index.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fraktag_repo(repo, r#"{"name":"@fraktag/engine","main":"lib/index.js"}"#);
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let mut storage = StorageConnection::open(&db).unwrap();
    let r1 = index_into_storage(repo, &mut storage, "rf", &ComposeOptions::default()).unwrap();
    let before = stored_imports_from(&db, &r1.snapshot_uid, "packages/api/src/server.ts");
    assert_eq!(before.len(), 1, "{before:?}");
    assert_eq!(before[0].1, "rf:packages/engine/src/index.ts:FILE");
    assert_eq!(before[0].2, "inferred");
    assert_eq!(
        before[0].3["candidates"],
        serde_json::json!([
            "rf:packages/engine/src/index.ts:FILE",
            "rf:packages/engine/lib/index.js:FILE"
        ])
    );

    // The declared entry appears; server.ts is unchanged.
    write(
        repo,
        "packages/engine/lib/index.js",
        "exports.Fraktag = class {};\n",
    );
    let r2 = refresh_into_storage(repo, &mut storage, "rf", &ComposeOptions::default()).unwrap();

    assert!(
        stored_imports_from(&db, &r2.snapshot_uid, "packages/api/src/server.ts").is_empty(),
        "the refresh re-resolved the import against the current files: no edge remains"
    );
    assert_eq!(
        stored_unresolved_from(&db, &r2.snapshot_uid, "packages/api/src/server.ts"),
        vec![(
            "packages/api/src/server.ts".to_string(),
            "@fraktag/engine".to_string(),
            serde_json::json!({"rawPath": "@fraktag/engine", "isTypeOnly": false}),
        )],
        "one unresolved row again, its carrier the extractor's"
    );
}

/// D-TWR-ENTRY-ORIGIN (OC-3), through compose: a root `exports` target written without an
/// extension (`./lib/index`) is resolved by Node as written, so an indexed `lib/index.js` is not
/// the declared target and the import binds INFERRED to the one indexed source entry, with the
/// declared path as its other candidate. The same file declared by `main` is completed by Node, so
/// there the import stays unresolved — the origin crosses the compose→indexer boundary.
#[test]
fn export_target_is_literal_end_to_end_so_a_completed_path_never_declines_the_binding() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fraktag_repo(
        repo,
        r#"{"name":"@fraktag/engine","exports":{".":"./lib/index"}}"#,
    );
    write(
        repo,
        "packages/engine/lib/index.js",
        "exports.Fraktag = class {};\n",
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "ex", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;
    assert_eq!(
        stored_imports_from(&db, snap, "packages/api/src/server.ts"),
        vec![(
            "packages/api/src/server.ts".to_string(),
            "ex:packages/engine/src/index.ts:FILE".to_string(),
            "inferred".to_string(),
            serde_json::json!({
                "rawPath": "@fraktag/engine",
                "isTypeOnly": false,
                "basis": "workspace_source_entry",
                "candidates": [
                    "ex:packages/engine/src/index.ts:FILE",
                    "ex:packages/engine/lib/index:FILE"
                ],
            }),
        )],
        "an `exports` target is matched as written: the indexed lib/index.js does not decline"
    );
    assert!(stored_unresolved_from(&db, snap, "packages/api/src/server.ts").is_empty());

    // The same path declared by `main`: Node completes it to the indexed lib/index.js, so the
    // declared entry IS indexed and the import stays an unresolved row.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fraktag_repo(repo, r#"{"name":"@fraktag/engine","main":"lib/index"}"#);
    write(
        repo,
        "packages/engine/lib/index.js",
        "exports.Fraktag = class {};\n",
    );
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "mn", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;
    assert!(
        stored_imports_from(&db, snap, "packages/api/src/server.ts").is_empty(),
        "a `main` target is completed: lib/index.js is the declared entry"
    );
    assert_eq!(
        stored_unresolved_from(&db, snap, "packages/api/src/server.ts")
            .into_iter()
            .map(|u| u.1)
            .collect::<Vec<_>>(),
        vec!["@fraktag/engine"]
    );
}
