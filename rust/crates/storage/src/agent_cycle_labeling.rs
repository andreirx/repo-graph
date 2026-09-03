//! ORIENT-CYCLES-DISAGREE-1: the SQLite serving-computation test-only labeling for `orient`'s
//! module cycles.
//!
//! Extracted from the 1700-line `agent_impl` adapter under the 500-line structural guardrail
//! (the same split ORIENT-DENSITY-1 made for `agent_orient_reads`): the guardrail forbids
//! appending a NEW responsibility to a file already far over 500 lines, so the cycle test-only
//! labeling lives here instead of growing `agent_impl`.
//!
//! Abstraction record — module: `agent_cycle_labeling`; concrete current user:
//! `agent_impl::AgentStorageRead::find_module_cycles{,_cancellable}` (the SQLite serving
//! computation for `orient`'s repo cycle leaf); axis: keeping the FIXTURE-POLLUTION-1 test-only
//! labeling OFF the over-guardrail `agent_impl` file; rejected simpler alternative: inlining it
//! in `agent_impl` (appends a new responsibility to a 1742-line file — the review-1 finding).
//!
//! The classification itself is NOT here — it is the SHARED `repo_graph_agent::classify_cycles`,
//! the SAME function the `cycles` command's serving computation (`daemon-runtime::cycle_output`)
//! calls (operator ruling cycle-count-derivation-placement, 2026-09-02: "one partition function
//! at one site"). This module is the thin SQLite adapter: read the classification inputs, call
//! the shared classifier, attach the per-cycle result. So `orient` and `cycles` cannot disagree.

use repo_graph_agent::{AgentCycle, AgentStorageError};

use crate::agent_impl::map_err;
use crate::connection::StorageConnection;

/// ORIENT-CYCLES-DISAGREE-1: attach the FIXTURE-POLLUTION-1 test-only classification to each
/// module cycle at the SQLite SERVING computation for `orient`. The basis is the shared
/// `repo_graph_agent::classify_cycles` over the stored `is_test` fact PLUS the qualified module
/// paths — NEVER a path/name heuristic (STANDING HONESTY RULE #2).
///
/// Both extra reads (`module_qualified_names`, the tracked `is_test` files) are CLASSIFIED
/// inputs — they determine a RENDERED figure (the production/test-only split) — so a genuine
/// read failure PROPAGATES; it is NEVER collapsed to a silent "no split" / production default
/// (STANDING HONESTY RULE #1). An empty result (a legitimately file-less snapshot) yields
/// `Unknown` per-cycle from the classifier, never a false test-only or production label.
pub(crate) fn label_module_cycles(
    conn: &StorageConnection,
    snapshot_uid: &str,
    cycles: Vec<crate::queries::CycleResult>,
) -> Result<Vec<AgentCycle>, AgentStorageError> {
    let qualified = conn
        .module_qualified_names(snapshot_uid)
        .map_err(map_err("find_module_cycles"))?;
    let files = repo_tracked_files(conn, snapshot_uid)?;
    let files_ref: Vec<(&str, bool)> = files.iter().map(|(p, t, _)| (p.as_str(), *t)).collect();
    // Per-cycle member qualified paths; a member uid with no MODULE qualified-name mapping is
    // `None` (the classifier treats it as unclassifiable → contributes to `Unknown`, never a
    // silent production default).
    let member_lists: Vec<Vec<Option<&str>>> = cycles
        .iter()
        .map(|c| {
            c.nodes
                .iter()
                .map(|n| qualified.get(&n.node_id).map(String::as_str))
                .collect()
        })
        .collect();
    let comps = repo_graph_agent::classify_cycles(&member_lists, &files_ref);

    // TYPE-ONLY-IMPORTS-1: the per-cycle runtime-vs-type-only verdict for `orient`'s cycle leaf,
    // computed by the SAME shared kernel (`classify_cycles_type_only`) the `cycles` command's
    // serving computation calls — so the two surfaces cannot disagree (route-agreement DoD;
    // ORIENT-CYCLES-DISAGREE-1 "one derivation"). Inputs are assembled from the SAME reads the
    // `cycles` route uses: the stored per-module-edge `is_type_only` fact (`module_import_edges`),
    // per-file language (`get_files_by_repo`), and the qualified module directories. Both reads
    // are CLASSIFIED (they determine a RENDERED verdict) so a genuine failure PROPAGATES — never a
    // silent default (STANDING HONESTY RULE #1).
    let module_edges = conn
        .module_import_edges(snapshot_uid)
        .map_err(map_err("find_module_cycles"))?;
    let edges_mapped: Vec<(&str, &str, Option<repo_graph_agent::EdgeTypeOnly>)> = module_edges
        .iter()
        .map(|(from, to, disp)| {
            (
                from.as_str(),
                to.as_str(),
                disp.map(crate::queries::edge_type_only_of),
            )
        })
        .collect();
    let files_lang: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|(p, _, lang)| (p.as_str(), lang.as_deref()))
        .collect();
    let all_module_dirs: Vec<String> = qualified.values().cloned().collect();
    // Per-cycle members as (node_id, qualified_dir). `qualified_dir` MIRRORS the daemon's canonical
    // `qualified_name` (the qualified path when mapped, else the short name — exactly what
    // `canonical_module_cycles_json` emits), so the §5 TS/JS membership gate resolves identically on
    // both routes. `node_id` (the node_uid) maps the edges — the SAME identity space
    // `module_import_edges` returns.
    let member_pairs_owned: Vec<Vec<(String, String)>> = cycles
        .iter()
        .map(|c| {
            c.nodes
                .iter()
                .map(|n| {
                    let qual = qualified
                        .get(&n.node_id)
                        .cloned()
                        .unwrap_or_else(|| n.name.clone());
                    (n.node_id.clone(), qual)
                })
                .collect()
        })
        .collect();
    let cycle_members: Vec<Vec<(&str, &str)>> = member_pairs_owned
        .iter()
        .map(|c| c.iter().map(|(id, q)| (id.as_str(), q.as_str())).collect())
        .collect();
    let type_onlys = repo_graph_agent::classify_cycles_type_only(
        &cycle_members,
        &edges_mapped,
        &files_lang,
        &all_module_dirs,
    );

    Ok(cycles
        .into_iter()
        .zip(comps)
        .zip(type_onlys)
        .map(|((c, comp), type_only)| AgentCycle {
            length: c.length,
            modules: c.nodes.into_iter().map(|n| n.name).collect(),
            test_composition: Some(comp),
            type_only,
        })
        .collect())
}

/// ORIENT-CYCLES-DISAGREE-1: the tracked `(path, is_test)` rows — the classification input for
/// [`label_module_cycles`]. This is the EXACT same source the `cycles` command's serving
/// computation uses for ITS test-only split: `get_files_by_repo(repo_uid)` (both SQLite cycles
/// paths — `dispatch::handle_cycles` and `livegraph_feed::serve_cycles_sqlite` — call it). That
/// method filters `is_excluded = 0` and reads `is_test` via `TrackedFile::from_row`'s strict
/// `== 1` mapping. Consuming it verbatim (rather than a bespoke snapshot-keyed join) is what
/// makes `orient` and `cycles` classify from ONE file set: a repo with an excluded file, or a
/// file not versioned in the target snapshot, now yields identical splits on both surfaces
/// (review-2 finding — the prior `file_versions` join included excluded rows and used `!= 0`).
///
/// `repo_uid` is derived from the snapshot the caller is serving (`get_snapshot`). Both reads are
/// CLASSIFIED (they determine the RENDERED production/test-only split), so a genuine failure
/// PROPAGATES (STANDING HONESTY RULE #1). A snapshot we are actively serving cycles for that has
/// no row is an internal inconsistency — reported as a loud error, NEVER a silent empty file set
/// (which would misclassify every cycle as `Unknown`); only io NotFound means absent, and a
/// resolved snapshot_uid resolving to no snapshot is not that.
fn repo_tracked_files(
    conn: &StorageConnection,
    snapshot_uid: &str,
) -> Result<Vec<(String, bool, Option<String>)>, AgentStorageError> {
    let repo_uid = conn
        .get_snapshot(snapshot_uid)
        .map_err(map_err("find_module_cycles"))?
        .ok_or_else(|| {
            AgentStorageError::new(
                "find_module_cycles",
                format!("snapshot {snapshot_uid} not found while labeling module cycles"),
            )
        })?
        .repo_uid;
    let files = conn
        .get_files_by_repo(&repo_uid)
        .map_err(map_err("find_module_cycles"))?;
    Ok(files
        .into_iter()
        .map(|f| (f.path, f.is_test, f.language))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::StorageConnection;
    use crate::crud::test_helpers::{
        fresh_storage, make_edge, make_file, make_file_version, make_repo,
    };
    use crate::queries::{CycleNode, CycleResult};
    use crate::types::{CreateSnapshotInput, GraphNode, TrackedFile};
    use repo_graph_agent::{CycleTestComposition, CycleTypeOnly};
    use repo_graph_indexer::storage_port::{EdgeStorePort, TypeOnlyDisposition};

    /// A MODULE node whose canonical directory is `qualified` (the classifier's ownership key).
    fn module_node(uid: &str, snapshot_uid: &str, qualified: &str) -> GraphNode {
        GraphNode {
            node_uid: uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: "r1".to_string(),
            stable_key: format!("r1:{qualified}:MODULE"),
            kind: "MODULE".to_string(),
            subtype: None,
            name: qualified
                .rsplit('/')
                .next()
                .unwrap_or(qualified)
                .to_string(),
            qualified_name: Some(qualified.to_string()),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: None,
        }
    }

    /// TYPE-ONLY-IMPORTS-1 (review-0 item 1): `orient`'s SQLite serving computation
    /// (`label_module_cycles`) MUST derive each cycle's runtime-vs-type-only verdict from the STORED
    /// per-module-edge `is_type_only` fact via the SHARED kernel — the SAME derivation the `cycles`
    /// command calls — so the two surfaces cannot disagree (the route-agreement DoD). This exercises
    /// REAL storage reads (nodes + files + IMPORTS edges + the stamped `is_type_only` column): a pure
    /// `import type` cycle labels `TypeOnly`; a cycle with one runtime edge labels `HasRuntimeEdges`.
    #[test]
    fn orient_derives_per_cycle_type_only_from_the_stored_fact() {
        let mut storage: StorageConnection = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: Some("abc123".to_string()),
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap();
        let s = snap.snapshot_uid.as_str();

        // Two TS module cycles: A<->B pure `import type`; C<->D one runtime edge.
        storage
            .insert_nodes(&[
                module_node("m_a", s, "src/a"),
                module_node("m_b", s, "src/b"),
                module_node("m_c", s, "src/c"),
                module_node("m_d", s, "src/d"),
            ])
            .unwrap();
        // TS files so §5 membership fires (make_file defaults language = typescript).
        storage
            .upsert_files(&[
                make_file("r1", "src/a/index.ts"),
                make_file("r1", "src/b/index.ts"),
                make_file("r1", "src/c/index.ts"),
                make_file("r1", "src/d/index.ts"),
            ])
            .unwrap();

        // MODULE->MODULE IMPORTS edges (the set `module_import_edges` reads).
        let imports = |uid: &str, from: &str, to: &str| {
            let mut e = make_edge(uid, s, "r1", from, to);
            e.edge_type = "IMPORTS".to_string();
            e
        };
        storage
            .insert_edges(&[
                imports("e_ab", "m_a", "m_b"),
                imports("e_ba", "m_b", "m_a"),
                imports("e_cd", "m_c", "m_d"),
                imports("e_dc", "m_d", "m_c"),
            ])
            .unwrap();
        // Stamp the disposition column exactly as the orchestrator's write path does.
        storage
            .set_edge_type_only(&[
                ("e_ab".to_string(), TypeOnlyDisposition::TypeOnly),
                ("e_ba".to_string(), TypeOnlyDisposition::TypeOnly),
                ("e_cd".to_string(), TypeOnlyDisposition::TypeOnly),
                ("e_dc".to_string(), TypeOnlyDisposition::Runtime),
            ])
            .unwrap();

        let node = |uid: &str, name: &str| CycleNode {
            node_id: uid.to_string(),
            name: name.to_string(),
            file: None,
        };
        let cycles = vec![
            CycleResult {
                cycle_id: "c-ab".to_string(),
                length: 2,
                nodes: vec![node("m_a", "a"), node("m_b", "b")],
            },
            CycleResult {
                cycle_id: "c-cd".to_string(),
                length: 2,
                nodes: vec![node("m_c", "c"), node("m_d", "d")],
            },
        ];

        let labeled = label_module_cycles(&storage, s, cycles).unwrap();
        assert_eq!(
            labeled[0].type_only,
            Some(CycleTypeOnly::TypeOnly),
            "a pure `import type` cycle vanishes at runtime"
        );
        assert_eq!(
            labeled[1].type_only,
            Some(CycleTypeOnly::HasRuntimeEdges),
            "a cycle with one runtime edge is a real runtime cycle"
        );
    }

    fn file(path: &str, is_test: bool, is_excluded: bool) -> TrackedFile {
        TrackedFile {
            is_test,
            is_excluded,
            ..make_file("r1", path)
        }
    }

    /// ORIENT-CYCLES-DISAGREE-1 (review-2 regression): `orient`'s SQLite cycle labeling MUST
    /// consume the EXACT file set the `cycles` command's serving computation uses —
    /// `get_files_by_repo(repo_uid)` (repo-scoped, `is_excluded = 0`, strict `== 1` `is_test`).
    ///
    /// This exercises REAL storage reads on BOTH surfaces (no hand-built `AgentCycle`s): it seeds
    /// two module cycles whose classification FLIPS depending on whether two rows are counted —
    /// (1) a VERSIONED-but-EXCLUDED production file, (2) a production file with NO `file_versions`
    /// row — the two cases the prior `file_versions`-join `snapshot_tracked_files` mishandled
    /// (it kept the excluded row and dropped the unversioned one). `orient`'s labeling (via
    /// `label_module_cycles`) must produce the SAME per-cycle `CycleTestComposition` as the
    /// `cycles` derivation (`get_files_by_repo` + the shared `classify_cycles`). Under the old
    /// code these disagreed; under the fix they cannot.
    #[test]
    fn orient_labeling_agrees_with_cycles_source_under_exclusion_and_nonversioning() {
        let mut storage: StorageConnection = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: Some("abc123".to_string()),
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap();
        let s = snap.snapshot_uid.as_str();

        // Module `src/excl` owns a versioned TEST file (⇒ looks test-only) and a versioned
        // EXCLUDED production file. `get_files_by_repo` drops the excluded row ⇒ test-only; the
        // old join kept it ⇒ production.
        // Module `src/nonver` owns a versioned TEST file and an UNVERSIONED production file.
        // `get_files_by_repo` (repo-scoped) includes the unversioned row ⇒ production; the old
        // join dropped it ⇒ test-only.
        let excl_test = file("src/excl/lib.rs", true, false);
        let excl_prod_excluded = file("src/excl/gen.rs", false, true);
        let nonver_test = file("src/nonver/lib.rs", true, false);
        let nonver_prod_unversioned = file("src/nonver/extra.rs", false, false);
        let test_file = file("tests/fix/a.rs", true, false);
        storage
            .upsert_files(&[
                excl_test.clone(),
                excl_prod_excluded.clone(),
                nonver_test.clone(),
                nonver_prod_unversioned.clone(),
                test_file.clone(),
            ])
            .unwrap();
        // Version everything EXCEPT `src/nonver/extra.rs` (the unversioned-file case).
        storage
            .upsert_file_versions(&[
                make_file_version(s, &excl_test.file_uid),
                make_file_version(s, &excl_prod_excluded.file_uid),
                make_file_version(s, &nonver_test.file_uid),
                make_file_version(s, &test_file.file_uid),
            ])
            .unwrap();

        storage
            .insert_nodes(&[
                module_node("m_excl", s, "src/excl"),
                module_node("m_nonver", s, "src/nonver"),
                module_node("m_test", s, "tests/fix"),
            ])
            .unwrap();

        // Two cycles, referencing the MODULE node uids the way `find_cycles` would.
        let node = |uid: &str, name: &str| CycleNode {
            node_id: uid.to_string(),
            name: name.to_string(),
            file: None,
        };
        let cycles = vec![
            CycleResult {
                cycle_id: "c-excl".to_string(),
                length: 2,
                nodes: vec![node("m_excl", "excl"), node("m_test", "fix")],
            },
            CycleResult {
                cycle_id: "c-nonver".to_string(),
                length: 2,
                nodes: vec![node("m_nonver", "nonver"), node("m_test", "fix")],
            },
        ];

        // ── orient's serving computation (the code under test) ──
        let orient = label_module_cycles(&storage, s, cycles.clone()).unwrap();
        let orient_comps: Vec<CycleTestComposition> = orient
            .iter()
            .map(|c| c.test_composition.clone().expect("SQLite path labels"))
            .collect();

        // ── the `cycles` command's derivation, from the SAME real reads it performs ──
        let tracked = storage.get_files_by_repo("r1").unwrap();
        let files: Vec<(&str, bool)> = tracked
            .iter()
            .map(|f| (f.path.as_str(), f.is_test))
            .collect();
        let qualified = storage.module_qualified_names(s).unwrap();
        let member_lists: Vec<Vec<Option<&str>>> = cycles
            .iter()
            .map(|c| {
                c.nodes
                    .iter()
                    .map(|n| qualified.get(&n.node_id).map(String::as_str))
                    .collect()
            })
            .collect();
        let cycles_comps = repo_graph_agent::classify_cycles(&member_lists, &files);

        // The two surfaces agree per-cycle — the DoD.
        assert_eq!(
            orient_comps, cycles_comps,
            "orient and cycles must classify each cycle identically from one file source"
        );
        // And the shared source's verdict is the honest one (the value the OLD orient code
        // contradicted): excluded production file NOT counted ⇒ test-only; unversioned
        // production file IS counted ⇒ production.
        assert_eq!(orient_comps[0], CycleTestComposition::TestOnly);
        assert_eq!(orient_comps[1], CycleTestComposition::Production);
    }
}
