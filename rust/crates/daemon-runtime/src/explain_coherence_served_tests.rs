//! EXPLAIN-LIVEGRAPH-IMPL: daemon-half LG-SERVED VALUE proofs (review-7 items 2+3).
//!
//! Through `build_explain_envelope`, prove the IDENTITY / IMPORTS / CYCLES leaf VALUES are genuinely REBUILT
//! from the LiveGraph (`node_display` / `live_import_view` / `module_import_cycles`) — NOT merely labelled and
//! NOT a re-labelled SQLite result. Each test feeds a DELIBERATELY-WRONG SQLite primary and asserts the served
//! value matches what the LiveGraph surface yields (and differs from the bogus SQLite value).
//!
//! Child of `explain_coherence_tests` (reuses its helpers — `build_db_with_calls`, `explain_symbol_result`,
//! `REPO` — via `use super::*`), split out to respect the >500-line structural guardrail. It HAND-BUILDS a
//! single-partition LiveGraph with two cross-dir files importing each other (a module cycle) + a symbol,
//! mirroring the livegraph crate's `module_cycle_from_cross_dir_file_imports`, so the import/cycle surfaces
//! are deterministic without depending on a SCIP fixture's import shape. The import/cycle no-loss certs are
//! seeded GREEN at the live fingerprint (mirroring orient's `seed_certs_green`): the genuine compare is
//! unit-tested elsewhere — here we isolate the VALUE-serving wiring.

use super::*;
use crate::livegraph_feed::{import_cert_fingerprint, CycleNoLossCert, ImportNoLossCert};
use repo_graph_agent::{
    CoherentOrientResult, CycleEvidence, ExplainCyclesEvidence, ExplainImportItem,
    ExplainImportsEvidence,
};
use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType, IdentitySource, ImportEdgeMeta, ImportResolution, IrEdge,
    IrNode, Partition, PartitionId, PartitionIr, PartitionKind, Provenance,
};

const PART: &str = "p";
const FILE_A_KEY: &str = "repo:alpha/a.ts:FILE";
const FILE_B_KEY: &str = "repo:beta/b.ts:FILE";
const SYM_A_KEY: &str = "repo:alpha/a.ts:bar:SYMBOL";
const SYM_A_LIVE_NAME: &str = "liveBar";

// ── Hand-built IR construction (public repo_graph_ir types) ──────────

fn prov() -> Provenance {
    Provenance {
        indexer: "scip-typescript".to_string(),
        indexer_version: "0.4.0".to_string(),
        scip_symbol_id: None,
        build_inputs_hash: "h".to_string(),
    }
}

fn partition() -> Partition {
    Partition {
        id: PartitionId::new(PART),
        kind: PartitionKind::TsPackage,
        root: "/x".to_string(),
        indexer: "scip-typescript".to_string(),
        indexer_version: "0.4.0".to_string(),
        build_inputs_hash: "h".to_string(),
        package_name: None,
        declared_dependencies: std::collections::BTreeSet::new(),
        tsconfig_aliases: None,
    }
}

fn file_node(key: &str) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(key),
        subtype: "FILE".to_string(),
        name: key.to_string(),
        range: None,
        partition_id: PartitionId::new(PART),
        identity_source: IdentitySource::AstFileScope,
        provenance: prov(),
        attributes: None,
    }
}

fn symbol_node(key: &str, name: &str, subtype: &str) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(key),
        subtype: subtype.to_string(),
        name: name.to_string(),
        range: None,
        partition_id: PartitionId::new(PART),
        identity_source: IdentitySource::AstAdopted,
        provenance: prov(),
        attributes: None,
    }
}

fn import_edge(src: &str, dst: &str) -> IrEdge {
    IrEdge {
        src: CanonicalKey::from_existing(src),
        dst: CanonicalKey::from_existing(dst),
        edge_type: EdgeType::Imports,
        basis: EdgeBasis::AstImport,
        provenance: prov(),
        import: Some(ImportEdgeMeta {
            raw_specifier: "./x".to_string(),
            resolved_path: "x".to_string(),
            resolution: ImportResolution::StaticResolved,
        }),
    }
}

/// A single resident TS partition: `alpha/a.ts` <-> `beta/b.ts` (a MODULE cycle) + a symbol in `alpha/a.ts`.
/// FILE keys use the `repo:{dir}/{file}.ts:FILE` shape so module aggregation (dirname) has real directory
/// identities, exactly like the livegraph crate's cycle test.
fn cyclic_lg() -> LiveGraph {
    let mut lg = LiveGraph::new();
    lg.load_partition(
        PART,
        PartitionIr {
            partition: partition(),
            nodes: vec![
                file_node(FILE_A_KEY),
                file_node(FILE_B_KEY),
                symbol_node(SYM_A_KEY, SYM_A_LIVE_NAME, "method"),
            ],
            edges: vec![
                import_edge(FILE_A_KEY, FILE_B_KEY),
                import_edge(FILE_B_KEY, FILE_A_KEY),
            ],
            import_observations: Vec::new(),
        },
        LanguageSupport::TypeScriptPrimary,
    );
    lg
}

// ── Cert seeding (mirrors orient `seed_certs_green`) ─────────────────

fn seed_import_cert_green(state: &RepoState, snapshot_uid: &str) {
    let fp = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph set");
        import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
    };
    *state.import_cert.write() = Some(ImportNoLossCert {
        verdict: "GREEN".to_string(),
        fingerprint: fp,
    });
}

fn seed_cycles_cert_green(state: &RepoState, snapshot_uid: &str) {
    let fp = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph set");
        import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
    };
    *state.cycles_cert.write() = Some(CycleNoLossCert {
        verdict: "GREEN".to_string(),
        values_verdict: "GREEN".to_string(),
        fingerprint: fp,
    });
}

// ── Focus-specific result builders (the symbol one is reused from the parent) ──

fn explain_file_result(snapshot_uid: &str, file: &str, signals: Vec<Signal>) -> OrientResult {
    OrientResult {
        focus: Focus::file(file, None, file),
        ..explain_symbol_result(snapshot_uid, "unused", signals)
    }
}

fn explain_path_result(snapshot_uid: &str, module: &str, signals: Vec<Signal>) -> OrientResult {
    OrientResult {
        focus: Focus::path_area(module, None, module),
        ..explain_symbol_result(snapshot_uid, "unused", signals)
    }
}

/// The served evidence `items` array of a leaf, read from the serialized envelope (no typed accessor exists
/// for imports/cycles evidence — the daemon BUILDS those values, never reads them back).
fn served_items(
    env: &repo_graph_coherence::CoherenceEnvelope<CoherentOrientResult>,
    code: &str,
) -> serde_json::Value {
    let json = serde_json::to_value(env).expect("serialize envelope");
    json["value"]["signals"]
        .as_array()
        .expect("signals array")
        .iter()
        .find(|l| l["value"]["code"] == code)
        .unwrap_or_else(|| panic!("{code} leaf present"))["value"]["evidence"]["items"]
        .clone()
}

// ── Identity: served from the live IR anchor (node_display) → {livegraph, sqlite} ──

#[test]
fn explain_identity_serves_live_anchor_from_livegraph() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(cyclic_lg());

    // The current-state IR display name the daemon must serve (NOT the drifted SQLite name), + a resident
    // file path for the identity coordinate so the residency precondition passes (derived from the LiveGraph
    // -> robust to the key->path format).
    let (expected_name, resident_file) = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().unwrap();
        let name = lg
            .node_display(&CanonicalKey::from_existing(SYM_A_KEY))
            .map(|(n, _)| n)
            .expect("resident symbol has a live IR name");
        let file = lg
            .resident_file_statuses()
            .into_keys()
            .next()
            .expect("a resident file path");
        (name, file)
    };
    assert_eq!(expected_name, SYM_A_LIVE_NAME);

    let result = explain_symbol_result(
        &snapshot_uid,
        SYM_A_KEY,
        vec![Signal::explain_identity(ExplainIdentityEvidence {
            target_kind: "symbol".to_string(),
            path: Some(resident_file.clone()),
            stable_key: Some(SYM_A_KEY.to_string()),
            name: Some("DRIFTED_SQLITE_NAME".to_string()),
            subtype: Some("function".to_string()),
            line_start: Some(10),
            language: None,
            is_test: None,
            module_path: Some("alpha".to_string()),
            file_count: None,
            symbol_count: None,
        })],
    );
    let env = build_explain_envelope(&state, REPO, result, false, false);
    let id = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainIdentity)
        .expect("identity leaf present");
    let ev = id
        .value
        .explain_identity_evidence()
        .expect("served identity evidence");
    assert_eq!(
        ev.name.as_deref(),
        Some(SYM_A_LIVE_NAME),
        "served identity NAME is the current-state LiveGraph IR name (LG-served, not the drifted SQLite name)"
    );
    assert_ne!(ev.name.as_deref(), Some("DRIFTED_SQLITE_NAME"));
    // The snapshot-scoped coordinate fields stay SQLite (the multi-source split).
    assert_eq!(ev.line_start, Some(10), "coordinate fields stay SQLite");
    assert_eq!(ev.path.as_deref(), Some(resident_file.as_str()));
    assert_eq!(
        id.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "served identity is the D8 {{livegraph, sqlite}} leaf (anchor LG + coordinates SQLite)"
    );
    assert!(id.provenance.fallback_reason.is_none());
    assert!(env.provenance.source.contains(&Source::Livegraph));
}

// ── Imports: served from live_import_view → single-source {livegraph} ──

#[test]
fn explain_imports_serves_live_view_from_livegraph() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(cyclic_lg());

    // Pick a file that HAS an outgoing import + the targets the live view yields for it (derived from the
    // LiveGraph -> the daemon must serve THESE, not the SQLite primary).
    let (importing_file, expected_targets) = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().unwrap();
        let any = lg.live_import_view(None);
        let src = any
            .edges
            .first()
            .map(|e| e.src_file.clone())
            .expect("the cyclic fixture has an import edge");
        let view = lg.live_import_view(Some(&src));
        let targets: Vec<String> = view
            .edges
            .into_iter()
            .filter(|e| e.src_file == src)
            .map(|e| e.dst_file)
            .collect();
        (src, targets)
    };
    assert!(
        !expected_targets.is_empty(),
        "the importing file has at least one live import target"
    );

    seed_import_cert_green(&state, &snapshot_uid);

    let result = explain_file_result(
        &snapshot_uid,
        &importing_file,
        vec![Signal::explain_imports(ExplainImportsEvidence {
            count: 1,
            items: vec![ExplainImportItem {
                target_file: "BOGUS_SQLITE_ONLY.ts".to_string(),
            }],
            items_truncated: None,
            items_omitted_count: None,
        })],
    );
    let env = build_explain_envelope(&state, REPO, result, false, false);
    let imports = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainImports)
        .expect("imports leaf present");
    // The VALUE is rebuilt from live_import_view, NOT the bogus SQLite primary.
    let items = served_items(&env, "EXPLAIN_IMPORTS");
    let served: Vec<String> = items
        .as_array()
        .expect("items array")
        .iter()
        .map(|i| i["target_file"].as_str().expect("target_file").to_string())
        .collect();
    assert_eq!(
        served, expected_targets,
        "served imports are REBUILT from live_import_view (not the SQLite primary)"
    );
    assert!(!served.iter().any(|t| t == "BOGUS_SQLITE_ONLY.ts"));
    assert_eq!(
        imports.provenance.source,
        BTreeSet::from([Source::Livegraph]),
        "imports served from the field-exact import cert is single-source {{livegraph}}"
    );
    assert!(imports.provenance.fallback_reason.is_none());
}

// ── Cycles: served from module_import_cycles → single-source {livegraph} ──

#[test]
fn explain_cycles_serves_live_cycles_from_livegraph() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(cyclic_lg());

    // The live module cycle + a focus module that is one of its members (derived from the LiveGraph -> the
    // daemon must serve THESE members, not the SQLite primary).
    let (focus_module, expected_members) = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().unwrap();
        let answer = lg.module_import_cycles();
        let members = answer
            .data()
            .and_then(|d| d.cycles.first().map(|c| c.members.clone()))
            .expect("the cyclic fixture has a module cycle");
        let focus = members.first().cloned().expect("cycle has a member");
        (focus, members.into_iter().collect::<BTreeSet<String>>())
    };
    assert!(
        expected_members.len() >= 2,
        "a module cycle has at least two members"
    );

    seed_cycles_cert_green(&state, &snapshot_uid);

    let result = explain_path_result(
        &snapshot_uid,
        &focus_module,
        vec![Signal::explain_cycles(ExplainCyclesEvidence {
            count: 1,
            items: vec![CycleEvidence {
                length: 1,
                modules: vec!["BOGUS_SQLITE_MODULE".to_string()],
                type_only: None,
            }],
            items_truncated: None,
            items_omitted_count: None,
        })],
    );
    let env = build_explain_envelope(&state, REPO, result, false, false);
    let cycles = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainCycles)
        .expect("cycles leaf present");
    // The VALUE is rebuilt from module_import_cycles, NOT the bogus SQLite primary.
    let items = served_items(&env, "EXPLAIN_CYCLES");
    let served_members: BTreeSet<String> = items
        .as_array()
        .expect("items array")
        .iter()
        .flat_map(|c| {
            c["modules"]
                .as_array()
                .expect("modules array")
                .iter()
                .map(|m| m.as_str().expect("module").to_string())
        })
        .collect();
    assert_eq!(
        served_members, expected_members,
        "served cycle members are REBUILT from module_import_cycles (not the SQLite primary)"
    );
    assert!(!served_members.contains("BOGUS_SQLITE_MODULE"));
    assert_eq!(
        cycles.provenance.source,
        BTreeSet::from([Source::Livegraph]),
        "cycles served from the field-exact module-cycle cert is single-source {{livegraph}}"
    );
    assert!(cycles.provenance.fallback_reason.is_none());
}

// ── EC-M2-LEAF-SERVE-1: FILE/PATH identity structural counts — the label follows the ACTUAL serve ──

/// `summary_served == true` (dispatch: module-summary cert GREEN at the witness fingerprint —
/// review-0 #1: independent of the bounded fold — ∧ epoch still resident) → the FILE-focus
/// EXPLAIN_IDENTITY leaf (whose `symbol_count`/`file_count` came from the decorator-served
/// `compute_file_summary`) is the multi-source `{livegraph, sqlite}` identity treatment.
/// `summary_served == false` → NO decision: the pre-M-2 unlabelled `{sqlite}` leaf,
/// byte-identical (the DR-E3 listing half keeps its `nodes` reads and sqlite label always).
#[test]
fn explain_identity_file_focus_counts_label_follows_actual_serve() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(cyclic_lg());

    let identity_signal = || {
        Signal::explain_identity(ExplainIdentityEvidence {
            target_kind: "file".to_string(),
            path: Some("alpha/a.ts".to_string()),
            stable_key: None,
            name: None,
            subtype: None,
            line_start: None,
            language: Some("typescript".to_string()),
            is_test: Some(false),
            module_path: None,
            file_count: None,
            symbol_count: Some(1),
        })
    };

    // SERVED: the counts were decorator-served from the LiveGraph inventory.
    let result = explain_file_result(&snapshot_uid, "alpha/a.ts", vec![identity_signal()]);
    let env = build_explain_envelope(&state, REPO, result, false, true);
    let id = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == repo_graph_agent::SignalCode::ExplainIdentity)
        .expect("identity leaf present");
    assert_eq!(
        id.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "served FILE-focus counts -> multi-source {{livegraph, sqlite}} identity"
    );
    assert!(id.provenance.fallback_reason.is_none());

    // NOT SERVED: the pre-M-2 unlabelled sqlite leaf (no decision, no fallback reason).
    let result = explain_file_result(&snapshot_uid, "alpha/a.ts", vec![identity_signal()]);
    let env = build_explain_envelope(&state, REPO, result, false, false);
    let id = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == repo_graph_agent::SignalCode::ExplainIdentity)
        .expect("identity leaf present");
    assert_eq!(
        id.provenance.source,
        BTreeSet::from([Source::Sqlite]),
        "unserved FILE-focus identity stays the plain sqlite leaf (byte-identical pre-M-2 path)"
    );
    assert!(id.provenance.fallback_reason.is_none());
}
