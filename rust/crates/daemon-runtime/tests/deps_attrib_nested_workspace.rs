//! DEPS-ATTRIB-2 — SURFACE reproduction of the glamCRM deps bug through the real
//! `handle_deps_list` dispatch (drives `compose_dependency_summaries` end-to-end against a freshly
//! indexed on-disk fixture in an ISOLATED temp state root; the operator's registry/daemon are never
//! touched). This is the reproducing fixture the slice mandates (§4): it reproduces the
//! zero-attribution shape and proves each fix.
//!
//! The fixture is glamCRM's shape in miniature — a NESTED-WORKSPACE npm repo with NO root
//! `package.json` (so repo-index discovers zero `npm:` modules; the manifest-governed source is
//! owned by coarse `inferred:` directory modules), PLUS a materially-present Java half with a real
//! `build.gradle`. Pre-fix, the seven-of-seven false "govern no indexed source" excuse rendered and
//! Java was silently absent. This asserts:
//!   1. the nested npm manifests attribute to modules (non-empty `results`, real `manifest_path`),
//!      and the false "no indexed source" excuse cannot render (`manifests_no_indexed_source == 0`);
//!   2. the DEFAULT view states the Java half's truth (`other_ecosystems` names Java's attributed
//!      Gradle deps) — never a silent absence, never a no-reader sentence for a reader that exists;
//!   3. `deps list --ecosystem java` renders the real Gradle attribution directly.

use std::path::Path;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Harness (mirrors tests/honest_degradation_impl2.rs) ───────────────────────

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

fn isolated() -> (ServiceDispatcher, TempDir) {
    // Disable the background maintenance passes (enrich/seed/retention) so they never hold the DB
    // while the test reads it (the `database is locked` flake class the sibling suites document).
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(state);
    (dispatcher, state_root)
}

fn run(dispatcher: &ServiceDispatcher, id: &str, method: &str, params: Value) -> DispatchResult {
    let mut emitter = Quiet;
    dispatcher.dispatch(
        &Request {
            id: id.to_string(),
            method: method.to_string(),
            params,
        },
        &mut emitter,
    )
}

#[track_caller]
fn expect_success(result: DispatchResult) -> Value {
    match result {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => {
            panic!(
                "expected success, got error {}: {}",
                e.error.code, e.error.message
            )
        }
    }
}

fn index_repo(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> String {
    let indexed = expect_success(run(
        dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    indexed["canonical_path"]
        .as_str()
        .expect("index returns canonical_path")
        .to_string()
}

/// Write the nested-workspace fixture: NO root manifest; two nested npm packages that own indexed
/// TS source; a Java half with a real `build.gradle`. TS/JS is dominant (5 files) and Java is a
/// material ~28% minority (2 files) — above the ≥10% materiality gate, so it is a secondary
/// ecosystem the default view must surface.
fn write_nested_workspace(dir: &Path) {
    let w = |rel: &str, body: &str| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };

    // ── serverless nested package (npm, no root manifest above it) ──
    w(
        "serverless/packages/backend/package.json",
        r#"{"name":"@app/backend","dependencies":{"express":"^4.0.0"}}"#,
    );
    w(
        "serverless/packages/backend/src/handler.ts",
        "import express from 'express';\nexport const app = express();\n",
    );
    w(
        "serverless/packages/backend/src/util.ts",
        "export function id<T>(x: T): T { return x; }\n",
    );

    // ── frontend nested package (npm) ──
    w(
        "frontend/web/package.json",
        r#"{"name":"@app/web","dependencies":{"react":"^18.0.0"}}"#,
    );
    w(
        "frontend/web/App.tsx",
        "import React from 'react';\nexport const App = () => React.createElement('div');\n",
    );
    w("frontend/web/index.ts", "export { App } from './App';\n");
    w(
        "frontend/web/store.ts",
        "export const store = { count: 0 };\n",
    );

    // ── Java half with a real Gradle build script ──
    w(
        "backend/build.gradle",
        "dependencies {\n  implementation 'com.google.guava:guava:31.0'\n  implementation 'org.apache.commons:commons-lang3:3.12'\n}\n",
    );
    w(
        "backend/src/main/java/com/app/Main.java",
        "import com.google.common.base.Strings;\n\npublic class Main {\n  public static void main(String[] args) {\n    System.out.println(Strings.isNullOrEmpty(\"\"));\n  }\n}\n",
    );
    w(
        "backend/src/main/java/com/app/Util.java",
        "public class Util {\n  int one() { return 1; }\n}\n",
    );
}

fn field_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

// ── The reproduction ──────────────────────────────────────────────────────────

#[test]
fn nested_workspace_npm_attributes_and_default_view_states_java_truth() {
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("glamlike");
    write_nested_workspace(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    // ── DEFAULT view (no ecosystem / module filter) ──
    let out = expect_success(run(&dispatcher, "d", "deps_list", json!({ "repo": repo })));

    // The dominant ecosystem is npm (TS/JS plurality).
    assert_eq!(
        out["ecosystem"].as_str(),
        Some("npm"),
        "TS/JS-dominant repo must render the npm ecosystem: {out}"
    );

    // FIX 1 — the nested manifests attribute to modules (pre-fix this was `count: 0`). At least one
    // reconciled module row exists, and at least one carries a real nested `package.json` path.
    let results = out["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "nested npm manifests attributed to ZERO modules — the DEPS-ATTRIB-2 bug is unfixed: {out}"
    );
    // BOTH nested manifests must be attributed — not just one (review-1 item 4: the old `any(...)`
    // check passed even if one workspace stayed un-attributed). A coarse `inferred:` module that
    // SPANS several nested manifests honestly renders `manifest_path: null` with a nested-manifest
    // CONTEXT (it cannot pin one file across many); a module sitting AT one manifest pins its exact
    // path. Either is real, containment-based attribution — collect BOTH surfaces and require each
    // fixture manifest to appear.
    let manifest_refs: Vec<String> = results
        .iter()
        .flat_map(|m| {
            [
                m.get("manifest_path").and_then(Value::as_str),
                m.get("manifest_context").and_then(Value::as_str),
            ]
        })
        .flatten()
        .map(str::to_string)
        .collect();
    for manifest in [
        "serverless/packages/backend/package.json",
        "frontend/web/package.json",
    ] {
        assert!(
            manifest_refs.iter().any(|r| r.contains(manifest)),
            "nested manifest {manifest} not attributed to any module row/context \
             (attribution join covers only part of the workspace): refs={manifest_refs:?} out={out}"
        );
    }
    // And the attribution carries REAL declared deps (react + express), not empty rows.
    let attributed_deps: u64 = results
        .iter()
        .filter_map(|m| field_u64(m, "declared_and_used"))
        .sum();
    assert!(
        attributed_deps >= 2,
        "nested npm packages must attribute their declared deps (react + express): {out}"
    );

    // FIX 3 — EXACT coverage facts (review-1 item 4): the fixture has exactly TWO parsed npm
    // manifests, BOTH governing indexed source → present=2, attributed=2, no_indexed_source=0. The
    // false "govern no indexed source" excuse is arithmetically impossible. These are REQUIRED (not
    // conditional): a missing field would itself be the silent-omit regression.
    assert_eq!(
        field_u64(&out, "manifests_present"),
        Some(2),
        "expected exactly 2 present npm manifests: {out}"
    );
    assert_eq!(
        field_u64(&out, "manifests_attributed"),
        Some(2),
        "both nested npm manifests must be attributed (govern indexed source): {out}"
    );
    assert_eq!(
        field_u64(&out, "manifests_no_indexed_source"),
        Some(0),
        "no manifest governs zero indexed source — the false excuse must be 0: {out}"
    );
    assert!(
        out.get("manifests_coverage_unavailable").is_none(),
        "owned-files read should have succeeded on this fixture: {out}"
    );
    // review-4 blocker 2: every nested manifest's indexed source is module-owned here, so the
    // indexed-but-unattributed count is absent (omitted when zero) — the false excuse and the
    // unattributed clause are BOTH arithmetically impossible on this all-attributed fixture.
    assert!(
        out.get("manifests_indexed_unattributed").is_none(),
        "all nested source is owned → no indexed-but-unattributed manifests: {out}"
    );

    // FIX 2 (Option 2) — the DEFAULT view states the Java half's truth; Java is NOT silently absent.
    let others = out["other_ecosystems"]
        .as_array()
        .unwrap_or_else(|| panic!("default view must carry other_ecosystems: {out}"));
    let java = others
        .iter()
        .find(|e| e.get("ecosystem").and_then(Value::as_str) == Some("java"))
        .unwrap_or_else(|| panic!("Java half silently absent from the default view: {out}"));
    assert_eq!(
        java.get("state").and_then(Value::as_str),
        Some("attributed"),
        "Java's Gradle deps were read + attributed → state must be 'attributed': {java}"
    );
    assert!(
        field_u64(java, "declared_dependencies").is_some_and(|n| n >= 1),
        "the two build.gradle deps must be attributed in the default view: {java}"
    );

    // The human render names Java and never emits a no-reader sentence for a reader that exists.
    // (Additive JSON is the daemon contract; the CLI render is exercised in rgr unit tests. Here we
    // assert the daemon payload — the source of truth — carries Java's attributed state.)

    // ── TARGETED java view — the same Gradle deps render as a first-class ecosystem ──
    let jout = expect_success(run(
        &dispatcher,
        "j",
        "deps_list",
        json!({ "repo": repo, "ecosystem": "java" }),
    ));
    assert_eq!(jout["ecosystem"].as_str(), Some("java"), "{jout}");
    let jresults = jout["results"].as_array().expect("java results array");
    let declared_total: u64 = jresults
        .iter()
        .flat_map(|m| {
            [
                field_u64(m, "declared_and_used"),
                field_u64(m, "declared_but_unobserved"),
            ]
        })
        .flatten()
        .sum();
    assert!(
        declared_total >= 1,
        "`deps list --ecosystem java` must render the real Gradle attribution: {jout}"
    );
    // A targeted view carries no secondary-ecosystem block (it is a property of the DEFAULT view).
    assert!(
        jout.get("other_ecosystems").is_none(),
        "targeted --ecosystem view must not carry other_ecosystems: {jout}"
    );
}
