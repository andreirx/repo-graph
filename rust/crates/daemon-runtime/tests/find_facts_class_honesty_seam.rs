//! FIND-FACTS-1 — the FACTS-tier PER-CLASS honesty proofs, driven through the REAL
//! `ServiceDispatcher::dispatch` `find` surface. Sibling of `find_facts_seam.rs` (the
//! TIER-BEHAVIOR half); split so each integration binary stays under the 500-line
//! structural guardrail (review-8; the `index_basis` split precedent). The shared
//! dispatcher/real-git/fake-embed harness lives in `tests/seed_harness/mod.rs`.
//!
//! This file owns the CLASS-SPECIFIC certainty/honesty guarantees: a `boundary`
//! governance-declaration hit's emitted `next` (`violations`) actually RENDERS the
//! declaration (review-6 re-home); an observed-but-undeclared import is NEVER laundered
//! into the `dependency · extracted` class; and a `dependency` class whose manifest
//! provenance is unreadable renders `unavailable (<reason>)`, never a false empty.

mod seed_harness;
use seed_harness::*;

use repo_graph_seed::SeedCorpusRead;
use repo_graph_storage::StorageConnection;
use serde_json::json;

/// FIND-FACTS-1 review-6 (operator-ratified 2026-08-30, item 2): the declarations-backed
/// replacement for the dropped orphan-`surface_entrypoints` seam — proving the emitted
/// boundary `next` command RENDERS the hit's declaration, not merely that it exits 0.
///
/// A boundary declaration exists in the store BEFORE any query. `find` matches it in the
/// `boundary` group and emits `next: "violations"`; dispatching `violations` then reads
/// the SAME declarations store and renders it in its ARMED state (`armed: true`,
/// `declarations_checked: 1` — the exact one declaration find pointed at). This is the
/// property `surface_entrypoints` lacked: there, NO command rendered the table, so the
/// emitted next dead-ended the reader. Here the emitted command's output is a rendering
/// of the hit's declaration.
///
/// Deterministic: it asserts on the armed-state configuration facts `violations`
/// computes directly from the declarations store, so it needs no IMPORTS-edge extraction
/// (which the isolated stdio index does not reliably populate — the concurrency tests
/// seed edges by hand for that reason).
#[test]
fn find_boundary_hit_next_command_renders_the_declaration() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // untouched under --exact
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);

    // ONE active boundary declaration, repo-scoped (NULL snapshot), target key carrying
    // the needle `corebnd`. FKs OFF on the raw seed connection; the `repos` parent exists
    // from `index`. WAL + busy_timeout so the write commits past the daemon's idle handle.
    let raw = rusqlite::Connection::open(&db_path).expect("raw open for declaration seed");
    raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    raw.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    raw.execute_batch(&format!(
        "INSERT INTO declarations \
           (declaration_uid, repo_uid, snapshot_uid, target_stable_key, kind, value_json, created_at, is_active) \
         VALUES ('decl-corebnd', '{repo_uid}', NULL, '{repo_uid}:corebnd:MODULE', 'boundary', '{{\"forbids\":\"adapters\"}}', '2026-01-01T00:00:00Z', 1);"
    ))
    .expect("seed one boundary declaration");
    drop(raw);

    // `find corebnd --exact`: the boundary group carries the hit, emitting `violations`.
    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "corebnd", "exact": true }),
    );
    let boundary = resp["facts"]
        .as_array()
        .and_then(|f| f.iter().find(|g| g["fact_class"] == "boundary"))
        .expect("boundary group present");
    let hit = boundary["hits"]
        .as_array()
        .and_then(|h| {
            h.iter()
                .find(|h| h["display"].as_str().unwrap_or("").contains("corebnd"))
        })
        .unwrap_or_else(|| panic!("boundary hit for the seeded declaration: {boundary}"));
    let next = hit["next"].as_str().expect("hit carries a next command");
    assert_eq!(
        next, "violations",
        "a boundary-kind declaration points at `rmap violations`: {hit}"
    );

    // Dispatch the EMITTED next command. Its output must RENDER the hit's declaration —
    // here as the armed state: exactly the one declaration find matched is checked. Before
    // the seed this surface is `armed: false` (GOV-ARMED-1); it is armed BECAUSE of it.
    let viol = dispatch_ok(&d, next, json!({ "repo": repo.path().to_string_lossy() }));
    assert_eq!(
        viol["armed"].as_bool(),
        Some(true),
        "the emitted `violations` renders the declaration (armed): {viol}"
    );
    assert_eq!(
        viol["declarations_checked"].as_u64(),
        Some(1),
        "exactly the one declaration find pointed at is checked by `violations`: {viol}"
    );
}

/// review-5 item 1 + item 3: an OBSERVED-BUT-UNDECLARED dependency (imported in
/// source, NOT present in any manifest — a Layer-2 reconciler inference at 0.8
/// confidence) must NEVER surface in the `dependency · extracted` fact class, while a
/// genuine DECLARED-but-unobserved manifest name in the SAME module still does. Both
/// package names carry the shared needle `obsx`, so a single `find obsx --exact`
/// exercises the category filter end-to-end through the REAL compose read path.
///
/// The full ObservedButUndeclared classification requires the whole compose chain: an
/// admitted external import (an `unresolved_edges` external-library candidate BACKED by
/// an `import_bindings_json` specifier — §2.1's specifier-only gate) whose package is
/// absent from the module's declared set, WITH manifest scope available (a declared dep
/// present) so the reconciler picks ObservedButUndeclared over UnknownExternalLike.
#[test]
fn find_dependency_excludes_observed_but_undeclared() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // untouched under --exact
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    let snapshot_uid = idx["snapshot_uid"]
        .as_str()
        .expect("index returns snapshot_uid")
        .to_string();

    // helper.ts's real file_uid (owns the declared dep + is the import source file).
    let corpus = StorageConnection::open(&db_path).unwrap();
    let entries = corpus.seed_corpus(&repo_uid).unwrap();
    let helper_uid = entries
        .iter()
        .find(|e| e.path.ends_with("helper.ts"))
        .expect("helper.ts in corpus")
        .file_uid
        .clone();
    drop(corpus);

    let raw = rusqlite::Connection::open(&db_path).expect("raw open for observed-undeclared seed");
    raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    raw.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    // A real node in helper.ts is the FK-less JOIN target `get_external_imports_for_snapshot`
    // requires (it reads `n.file_uid` off this node); helper.ts's `helperFunction` guarantees one.
    let node_uid: String = raw
        .query_row(
            "SELECT node_uid FROM nodes WHERE file_uid = ?1 LIMIT 1",
            [&helper_uid],
            |r| r.get(0),
        )
        .expect("a node in helper.ts");
    // Statement order: (1) owning module (npm key ⇒ TS-dominant ecosystem admits it);
    // (2) ownership helper.ts → module; (3) file_signals carrying BOTH the DECLARED dep
    // `obsx-declared` (⇒ manifest scope available) AND the import BINDING that backs the
    // observed specifier `obsx-observed` (§2.1 specifier gate ⇒ the import is admitted,
    // not rejected); (4) the external-library-candidate edge for that specifier.
    raw.execute_batch(&format!(
        "INSERT INTO module_candidates \
           (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence, display_name) \
         VALUES ('mc-obs', '{snapshot_uid}', '{repo_uid}', 'npm:obs:mod', 'inferred', 'obs-mod', 1.0, 'obs-module'); \
         INSERT INTO module_file_ownership \
           (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) \
         VALUES ('{snapshot_uid}', '{repo_uid}', '{helper_uid}', 'mc-obs', 'exact', 1.0); \
         INSERT INTO file_signals (snapshot_uid, file_uid, package_dependencies_json, import_bindings_json) \
         VALUES ('{snapshot_uid}', '{helper_uid}', '{{\"names\":[\"obsx-declared\"]}}', \
            '[{{\"identifier\":\"obsxObserved\",\"specifier\":\"obsx-observed\",\"is_relative\":false}}]'); \
         INSERT INTO unresolved_edges \
           (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, resolution, extractor, category, classification, classifier_version, basis_code, observed_at) \
         VALUES ('ue-obs', '{snapshot_uid}', '{repo_uid}', '{node_uid}', 'obsx-observed', 'call', 'unresolved', 'test', 'external', 'external_library_candidate', 1, 'test', '2026-01-01T00:00:00Z');"
    ))
    .expect("seed the observed-but-undeclared chain");
    drop(raw);

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "obsx", "exact": true }),
    );
    let dep_group = resp["facts"]
        .as_array()
        .expect("facts array")
        .iter()
        .find(|g| g["fact_class"] == "dependency")
        .unwrap_or_else(|| panic!("dependency group present: {resp}"));
    assert!(
        dep_group.get("error").and_then(|e| e.as_str()).is_none(),
        "dependency class read did not fail (provenance is Tracked): {dep_group}"
    );
    let displays: Vec<&str> = dep_group["hits"]
        .as_array()
        .expect("hits array")
        .iter()
        .filter_map(|h| h["display"].as_str())
        .collect();
    // The DECLARED manifest name IS an extracted fact.
    assert!(
        displays.contains(&"obsx-declared"),
        "declared-manifest dep surfaces as extracted: {dep_group}"
    );
    // The OBSERVED-BUT-UNDECLARED import (Layer-2 inference) is NOT laundered into it.
    assert!(
        !displays.contains(&"obsx-observed"),
        "observed-but-undeclared import must NOT render as an extracted dependency fact: {dep_group}"
    );
}

/// review-5 item 2 + item 3: when the parsed-manifest provenance is UNREADABLE
/// (a malformed `deps_manifests` diagnostics value), the `dependency` class cannot
/// attest which names came from a manifest, so it renders `unavailable (<reason>)` —
/// NOT an honest-looking empty and NOT the (genuinely present) declared dep laundered
/// into an extracted fact. A real declared dep `prov-dep` is seeded precisely so that
/// WITHOUT the provenance guard a hit WOULD render; the corrupt provenance must suppress
/// it in favor of the labeled degraded form.
#[test]
fn find_dependency_unavailable_when_manifest_provenance_malformed() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // untouched under --exact
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    let snapshot_uid = idx["snapshot_uid"]
        .as_str()
        .expect("index returns snapshot_uid")
        .to_string();

    let corpus = StorageConnection::open(&db_path).unwrap();
    let entries = corpus.seed_corpus(&repo_uid).unwrap();
    let helper_uid = entries
        .iter()
        .find(|e| e.path.ends_with("helper.ts"))
        .expect("helper.ts in corpus")
        .file_uid
        .clone();
    drop(corpus);

    let raw = rusqlite::Connection::open(&db_path).expect("raw open for malformed-provenance seed");
    raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    raw.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    // A genuine declared dep `prov-dep` (so a hit WOULD render absent the guard), then
    // OVERWRITE the snapshot's extraction-diagnostics blob so `deps_manifests` is a
    // non-array — `read_manifest_provenance` decodes it as `ProvenanceRead::Unavailable`
    // (the "provenance record malformed" cause), distinct from a predates-tracking absent.
    raw.execute_batch(&format!(
        "INSERT INTO module_candidates \
           (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence, display_name) \
         VALUES ('mc-prov', '{snapshot_uid}', '{repo_uid}', 'npm:prov:mod', 'inferred', 'prov-mod', 1.0, 'prov-module'); \
         INSERT INTO module_file_ownership \
           (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) \
         VALUES ('{snapshot_uid}', '{repo_uid}', '{helper_uid}', 'mc-prov', 'exact', 1.0); \
         INSERT INTO file_signals (snapshot_uid, file_uid, package_dependencies_json) \
         VALUES ('{snapshot_uid}', '{helper_uid}', '{{\"names\":[\"prov-dep\"]}}'); \
         UPDATE snapshots SET extraction_diagnostics_json = '{{\"deps_manifests\": \"not-an-array\"}}' \
           WHERE snapshot_uid = '{snapshot_uid}';"
    ))
    .expect("seed declared dep + corrupt provenance");
    drop(raw);

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "prov", "exact": true }),
    );
    let dep_group = resp["facts"]
        .as_array()
        .expect("facts array")
        .iter()
        .find(|g| g["fact_class"] == "dependency")
        .unwrap_or_else(|| panic!("dependency group present: {resp}"));
    let error = dep_group
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap_or_else(|| panic!("dependency class renders unavailable-with-reason: {dep_group}"));
    assert!(
        error.contains("manifest provenance unavailable") && error.contains("malformed"),
        "class error names the specific unreadable-provenance cause: {error}"
    );
    // The genuine declared dep is SUPPRESSED — never laundered into an extracted hit.
    let displays: Vec<&str> = dep_group["hits"]
        .as_array()
        .map(|h| h.iter().filter_map(|x| x["display"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        !displays.contains(&"prov-dep"),
        "no declared dep is rendered while provenance is unreadable: {dep_group}"
    );
}
