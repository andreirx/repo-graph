//! TRUST-MODULE-EDGES-1 — cross-surface seam: `trust`'s per-module fan_in/fan_out
//! must equal the per-module fans of the module-dependency edge set `modules list` /
//! `modules deps` render, on every rendered module.
//!
//! RG-REQ-002-L02 (surfaces reading one snapshot agree), RG-REQ-004-L01 (trust and
//! modules read one module-edge computation), RG-REQ-009-L02 (trust's module
//! connectivity reads the edge set `modules list` renders).
//!
//! The seam reads BOTH computations from the SAME store — trust via
//! `repo_graph_trust::TrustStorageRead::compute_module_stats`, and modules deps via
//! `repo_graph_module_queries::load_module_graph_facts` (the exact derivation `modules
//! deps`/`modules list` consume, with fans re-aggregated per module from the rendered
//! edge list) — and asserts they agree per module. It runs on two fixtures: a two-module
//! Cargo workspace (real crates) and a twin-names npm-workspace repo (two packages that
//! share the leaf basename). So a module with a rendered edge can never be flagged
//! zero-connectivity, and identifier collisions cannot break the mapping. A divergence
//! on either fixture fails the test.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use repo_graph_module_queries::load_module_graph_facts;
use repo_graph_repo_index::compose::{index_path, ComposeOptions};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::TrustStorageRead;

/// Per-module fan counts re-aggregated from the derived module-dependency edge set —
/// exactly what `modules deps` renders. `fan_out(M)` = distinct target modules of M's
/// outgoing edges; `fan_in(M)` = distinct source modules of M's incoming edges. Keyed
/// by canonical_root_path (the `path` trust's stats also key on).
fn modules_deps_fans(storage: &StorageConnection, snapshot_uid: &str) -> DepsFans {
    let facts = load_module_graph_facts(storage, snapshot_uid).expect("load module graph facts");
    let mut fan_out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut fan_in: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &facts.edges {
        fan_out
            .entry(e.source_canonical_path.clone())
            .or_default()
            .insert(e.target_canonical_path.clone());
        fan_in
            .entry(e.target_canonical_path.clone())
            .or_default()
            .insert(e.source_canonical_path.clone());
    }
    DepsFans { fan_out, fan_in }
}

struct DepsFans {
    fan_out: BTreeMap<String, BTreeSet<String>>,
    fan_in: BTreeMap<String, BTreeSet<String>>,
}

impl DepsFans {
    fn fan_out(&self, path: &str) -> u64 {
        self.fan_out.get(path).map(|s| s.len() as u64).unwrap_or(0)
    }
    fn fan_in(&self, path: &str) -> u64 {
        self.fan_in.get(path).map(|s| s.len() as u64).unwrap_or(0)
    }
}

/// Index `repo_path` into a throwaway db, then assert trust's per-module fans equal the
/// modules-deps per-module fans for every rendered module. Returns the total
/// connectivity observed (sum of fan_in + fan_out across trust's modules) so the caller
/// can require the fixture actually exercises edges.
fn assert_fans_agree(repo_path: &Path, label: &str) -> u64 {
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("seam.db");

    let result = index_path(repo_path, &db_path, "r1", &ComposeOptions::default())
        .unwrap_or_else(|e| panic!("[{label}] index failed: {e:?}"));
    let snapshot_uid = result.snapshot_uid;

    let storage = StorageConnection::open(&db_path).expect("open store");

    let stats = TrustStorageRead::compute_module_stats(&storage, &snapshot_uid)
        .unwrap_or_else(|e| panic!("[{label}] compute_module_stats failed: {e:?}"));
    let deps = modules_deps_fans(&storage, &snapshot_uid);

    let mut total_connectivity = 0u64;
    for m in &stats {
        let expected_out = deps.fan_out(&m.path);
        let expected_in = deps.fan_in(&m.path);
        assert_eq!(
            m.fan_out, expected_out,
            "[{label}] fan_out mismatch for module {}: trust={} modules-deps={}",
            m.path, m.fan_out, expected_out
        );
        assert_eq!(
            m.fan_in, expected_in,
            "[{label}] fan_in mismatch for module {}: trust={} modules-deps={}",
            m.path, m.fan_in, expected_in
        );
        total_connectivity += m.fan_in + m.fan_out;
    }

    // A module with a rendered edge is never flagged zero-connectivity: any module that
    // appears as an endpoint in the deps edge set must have fan > 0 in trust's stats.
    for path in deps.fan_out.keys().chain(deps.fan_in.keys()) {
        if let Some(m) = stats.iter().find(|m| &m.path == path) {
            assert!(
                m.fan_in > 0 || m.fan_out > 0,
                "[{label}] module {} has rendered edges but trust reports zero connectivity",
                path
            );
        }
    }

    total_connectivity
}

#[test]
fn trust_module_fans_equal_modules_deps() {
    // Fixture 1 — a two-crate Cargo workspace (crate `a` imports across the boundary
    // into crate `b`). The committed IMPORT-RESOLUTION-RUST-1 fixture; indexing reads it
    // in place (no writes to the source tree).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let two_crate = Path::new(manifest_dir)
        .join("../repo-index/tests/fixtures/rust/workspace")
        .canonicalize()
        .expect("two-crate fixture path");
    let two_crate_conn = assert_fans_agree(&two_crate, "two-crate");
    assert!(
        two_crate_conn > 0,
        "two-crate fixture must exercise at least one cross-module edge (regression guard)"
    );

    // Fixture 2 — a twin-names npm-workspace repo: two packages that share the leaf
    // basename `auth` under different parents, one importing the other. Guards that
    // identifier collisions cannot break the per-module mapping.
    let twin_dir = tempfile::tempdir().unwrap();
    build_twin_names_fixture(twin_dir.path());
    let twin_conn = assert_fans_agree(twin_dir.path(), "twin-names");
    assert!(
        twin_conn > 0,
        "twin-names fixture must exercise at least one cross-module edge (regression guard)"
    );
}

/// Build a twin-names npm-workspace repo: `services/auth` imports `libs/auth`; both
/// packages share the leaf basename `auth`.
fn build_twin_names_fixture(root: &Path) {
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"root","private":true,"workspaces":["services/*","libs/*"]}"#,
    )
    .unwrap();

    std::fs::create_dir_all(root.join("services/auth/src")).unwrap();
    std::fs::write(
        root.join("services/auth/package.json"),
        r#"{"name":"@svc/auth","version":"0.0.0","dependencies":{"@lib/auth":"*"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("services/auth/src/index.ts"),
        "import { token } from \"../../../libs/auth/src/index\";\nexport function login() { return token(); }\n",
    )
    .unwrap();

    std::fs::create_dir_all(root.join("libs/auth/src")).unwrap();
    std::fs::write(
        root.join("libs/auth/package.json"),
        r#"{"name":"@lib/auth","version":"0.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("libs/auth/src/index.ts"),
        "export function token() { return \"t\"; }\n",
    )
    .unwrap();
}
