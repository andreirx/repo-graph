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
/// How the labeled cycle's `modules` field is filled — the ONE axis on which `orient`'s repo
/// headline and `explain`'s focus-scoped cycle reads differ. The walk, type-only verdict and
/// test-composition are computed IDENTICALLY for both (the shared kernels below); only the
/// member-name vector differs, so both entry points share this one function.
///
/// - `Short`: the SCC members' short display names (`orient`'s repo cycles — `find_module_cycles`;
///   `orient` renders the ring from the qualified `walk`, so its `modules` field stays the
///   historical short-name form and this is byte-stable for that surface).
/// - `Qualified`: the SCC members' QUALIFIED module paths (`explain`'s focus/path cycle reads).
///   `explain` renders the unordered member listing from `modules`, so it MUST carry the same
///   qualified display names `cycles`' `CycleNode::display()` renders — otherwise the two
///   surfaces would list different member strings for the same cycle (EXPLAIN-CYCLES-HONEST-1).
#[derive(Clone, Copy)]
enum ModuleNaming {
    Short,
    Qualified,
}

/// EXPLAIN-CYCLES-HONEST-1 (§2.1): label a FOCUS/PATH-scoped set of module cycles the SAME way
/// `orient`'s repo cycles are labeled — attaching the REAL directed `walk` (via the shared
/// `cycle_walk` kernel) and the `type_only` verdict — so `explain`'s Import-cycles block draws a
/// verified ring (or the honest unordered form) identical to what `cycles`/`orient` draw, instead
/// of a ring fabricated from the sorted member set (RC-4). The ONLY difference from
/// [`label_module_cycles`] is that `modules` carries the QUALIFIED display names (see
/// [`ModuleNaming::Qualified`]) — the identity `explain`'s focus reads have always rendered and the
/// one `cycles`' member listing uses. Same computation, same reads, same honesty rules.
pub(crate) fn label_focus_cycles(
    conn: &StorageConnection,
    snapshot_uid: &str,
    cycles: Vec<crate::queries::CycleResult>,
) -> Result<Vec<AgentCycle>, AgentStorageError> {
    label_cycles(conn, snapshot_uid, cycles, ModuleNaming::Qualified)
}

pub(crate) fn label_module_cycles(
    conn: &StorageConnection,
    snapshot_uid: &str,
    cycles: Vec<crate::queries::CycleResult>,
) -> Result<Vec<AgentCycle>, AgentStorageError> {
    label_cycles(conn, snapshot_uid, cycles, ModuleNaming::Short)
}

fn label_cycles(
    conn: &StorageConnection,
    snapshot_uid: &str,
    cycles: Vec<crate::queries::CycleResult>,
    naming: ModuleNaming,
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

    // COHERENCE-3 (§2.1): precompute each cycle's REAL directed walk via the SHARED
    // `cycle_walk` kernel — the SAME intra-SCC edge selection + walk finder the `cycles`
    // command uses — over the SAME `module_import_edges` set already read above. `cycle_members`
    // is `(node_id, qualified_display)` (the SAME identities the edges key on and the SAME display
    // `cycles` renders), so `orient`'s walk and `cycles`' walk cannot differ. A truncated edge set
    // (over `CYCLE_EDGE_CAP`) draws NO walk — an incomplete subset could imply a chain the full set
    // does not — exactly as the `cycles` renderer falls back to `members (unordered)` there.
    let all_pairs: Vec<(&str, &str)> = edges_mapped.iter().map(|(f, t, _)| (*f, *t)).collect();
    let walks: Vec<Option<Vec<String>>> = cycle_members
        .iter()
        .map(|members| {
            let member_ids: Vec<&str> = members.iter().map(|(id, _)| *id).collect();
            let (intra, truncated) = repo_graph_agent::intra_cycle_edges(&member_ids, &all_pairs);
            if truncated {
                None
            } else {
                let intra_refs: Vec<(&str, &str)> = intra
                    .iter()
                    .map(|(f, t)| (f.as_str(), t.as_str()))
                    .collect();
                repo_graph_agent::find_cycle_walk(members, &intra_refs)
            }
        })
        .collect();

    Ok(cycles
        .into_iter()
        .zip(comps)
        .zip(type_onlys)
        .zip(walks)
        .map(|(((c, comp), type_only), walk)| {
            // EXPLAIN-CYCLES-HONEST-1: the member-name vector is the ONLY axis on which the repo
            // and focus entry points differ (see `ModuleNaming`). `Qualified` mirrors the
            // qualified display `cycles` renders and the `walk` uses; `Short` is `orient`'s
            // historical short-name form (byte-stable, since `orient` renders the ring from `walk`).
            let modules = match naming {
                ModuleNaming::Short => c.nodes.iter().map(|n| n.name.clone()).collect(),
                ModuleNaming::Qualified => c
                    .nodes
                    .iter()
                    .map(|n| {
                        qualified
                            .get(&n.node_id)
                            .cloned()
                            .unwrap_or_else(|| n.name.clone())
                    })
                    .collect(),
            };
            AgentCycle {
                length: c.length,
                modules,
                test_composition: Some(comp),
                type_only,
                walk,
            }
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
    /// `import type` cycle labels `TypeOnly`; a 2-cycle with one type-only + one runtime edge labels
    /// `BreaksAtRuntime` (COHERENCE-2 §2.2 Option A — erasing the type-only edge leaves no runtime
    /// cycle).
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
            Some(CycleTypeOnly::BreaksAtRuntime {
                type_only: 1,
                of: 2
            }),
            "a 2-cycle with one type-only + one runtime edge breaks at runtime (Option A): \
             erasing the type-only edge leaves no runtime cycle"
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

    /// Build a snapshot with a real 3-module import ring `src/a -> src/b -> src/c -> src/a` and
    /// return `(storage, snapshot_uid)` — the fixture for the EXPLAIN-CYCLES-HONEST-1 walk-agreement
    /// tests. The MODULE→MODULE `IMPORTS` edges are the SAME set `find_cycles` runs Tarjan over AND
    /// `module_import_edges` feeds the walk finder, so the walk is a real ring.
    fn three_module_ring() -> (StorageConnection, String) {
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
        let s = snap.snapshot_uid.clone();
        storage
            .insert_nodes(&[
                module_node("m_a", &s, "src/a"),
                module_node("m_b", &s, "src/b"),
                module_node("m_c", &s, "src/c"),
            ])
            .unwrap();
        storage
            .upsert_files(&[
                make_file("r1", "src/a/lib.rs"),
                make_file("r1", "src/b/lib.rs"),
                make_file("r1", "src/c/lib.rs"),
            ])
            .unwrap();
        let imports = |uid: &str, from: &str, to: &str| {
            let mut e = make_edge(uid, &s, "r1", from, to);
            e.edge_type = "IMPORTS".to_string();
            e
        };
        storage
            .insert_edges(&[
                imports("e_ab", "m_a", "m_b"),
                imports("e_bc", "m_b", "m_c"),
                imports("e_ca", "m_c", "m_a"),
            ])
            .unwrap();
        (storage, s)
    }

    /// EXPLAIN-CYCLES-HONEST-1 (§2.1): the MODULE-focus cycle read carries the SAME verified `walk`
    /// the repo read (`find_module_cycles`) carries for the SAME cycle — one derivation via the
    /// shared `cycle_walk` kernel, so `explain` draws the ring `cycles`/`orient` draw (RC-4). Real
    /// storage reads on both paths.
    #[test]
    fn focus_module_cycle_read_carries_the_same_walk_as_the_repo_read() {
        use repo_graph_agent::AgentStorageRead;
        let (storage, s) = three_module_ring();

        let repo = AgentStorageRead::find_module_cycles(&storage, &s).unwrap();
        assert_eq!(repo.len(), 1, "one 3-module SCC");
        let repo_walk = repo[0]
            .walk
            .clone()
            .expect("the repo read carries a verified walk over real edges");
        assert_eq!(
            repo_walk,
            vec![
                "src/a".to_string(),
                "src/b".to_string(),
                "src/c".to_string()
            ],
            "the real directed ring in qualified-display order"
        );

        let focus = AgentStorageRead::find_cycles_involving_module(&storage, &s, "src/a").unwrap();
        assert_eq!(focus.len(), 1, "the ring involves src/a");
        assert_eq!(
            focus[0].walk, repo[0].walk,
            "the focus read carries the SAME walk as the repo read (one derivation)"
        );
        // The focus read renders QUALIFIED member names (explain's unordered form reads these).
        assert!(
            focus[0].modules.contains(&"src/a".to_string()),
            "focus modules carry qualified paths: {:?}",
            focus[0].modules
        );
    }

    /// EXPLAIN-CYCLES-HONEST-1 (§2.1): the PATH-focus cycle read likewise carries the repo read's
    /// walk for the same cycle.
    #[test]
    fn focus_path_cycle_read_carries_the_same_walk_as_the_repo_read() {
        use repo_graph_agent::AgentStorageRead;
        let (storage, s) = three_module_ring();

        let repo = AgentStorageRead::find_module_cycles(&storage, &s).unwrap();
        let focus = AgentStorageRead::find_cycles_involving_path(&storage, &s, "src").unwrap();
        assert_eq!(focus.len(), 1, "the ring is under src/");
        assert_eq!(
            focus[0].walk, repo[0].walk,
            "the path read carries the SAME walk as the repo read"
        );
    }

    /// EXPLAIN-CYCLES-HONEST-1 A-2 (ECH-IR-002, P-ECH-06): both cancellable focus reads honour ONE
    /// cooperative checkpoint immediately BEFORE `label_focus_cycles` — the read's LAST checkpoint — so a
    /// client that disconnected during the filter loop abandons the read before the (un-checkpointed)
    /// labeling. Proven without hardcoding the checkpoint count: (1) an always-Continue run records N =
    /// the total number of checkpoint calls and the labelled reference; (2) Break at exactly call N (the
    /// pre-labeling checkpoint) cancels with the `before cycle labeling` reason and returns NO cycles;
    /// (3) Break one past the last call (never reached) returns the SAME labelled result as (1). The
    /// labeling that follows a Continue stays un-checkpointed exactly as the repo-level
    /// `find_module_cycles_cancellable` already is (COHERENCE-3); per-cycle checkpoints inside labeling
    /// are CANCEL-LABELING-1, out of scope.
    #[test]
    fn focus_cycle_reads_cancellable_return_cancelled_at_the_checkpoint_before_labeling() {
        use repo_graph_agent::{AgentCycle, AgentStorageError, AgentStorageRead};
        use std::ops::ControlFlow;

        // Dispatch either cancellable focus read through one shared cancel closure.
        fn run(
            storage: &StorageConnection,
            s: &str,
            is_module: bool,
            cancel: &mut dyn FnMut() -> ControlFlow<()>,
        ) -> Result<Vec<AgentCycle>, AgentStorageError> {
            if is_module {
                AgentStorageRead::find_cycles_involving_module_cancellable(
                    storage, s, "src/a", cancel,
                )
            } else {
                AgentStorageRead::find_cycles_involving_path_cancellable(storage, s, "src", cancel)
            }
        }

        for is_module in [true, false] {
            let (storage, s) = three_module_ring();

            // (1) never-breaking run: record N and keep the labelled reference.
            let mut count = 0usize;
            let reference = {
                let mut cont = || {
                    count += 1;
                    ControlFlow::Continue(())
                };
                run(&storage, &s, is_module, &mut cont)
                    .expect("a never-breaking checkpoint must not cancel")
            };
            let n = count;
            assert!(
                n >= 2,
                "at least the per-cycle filter checkpoint and the pre-labeling checkpoint fire (is_module={is_module}, n={n})"
            );
            assert_eq!(reference.len(), 1, "the ring is a single labelled cycle");
            assert!(
                reference[0].walk.is_some(),
                "the labelled reference carries a verified walk"
            );

            // (2) Break at exactly call N — the read's LAST checkpoint, immediately before labeling.
            let mut k = 0usize;
            let at_last = {
                let mut brk = || {
                    k += 1;
                    if k == n {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                };
                run(&storage, &s, is_module, &mut brk)
            };
            let err =
                at_last.expect_err("breaking at the pre-labeling checkpoint must cancel the read");
            assert!(
                err.to_string().contains("before cycle labeling"),
                "the cancel abandons the read at the checkpoint before labeling (is_module={is_module}), got: {err}"
            );

            // (3) Break one past the last call — never reached — the read completes as in (1).
            let mut j = 0usize;
            let past_last = {
                let mut brk = || {
                    j += 1;
                    if j == n + 1 {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                };
                run(&storage, &s, is_module, &mut brk)
                    .expect("a checkpoint that never fires must not cancel")
            };
            assert_eq!(
                past_last.len(),
                reference.len(),
                "a never-reached break yields the full labelled result"
            );
            assert_eq!(
                past_last[0].walk, reference[0].walk,
                "the walk is identical to the never-breaking run"
            );
        }
    }
}
