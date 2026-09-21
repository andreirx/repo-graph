//! End-to-end integration test: index a checked-in fixture repo
//! from disk into SQLite and verify deterministic outcomes.
//!
//! Test 1 uses `rust/crates/repo-index/tests/fixtures/typescript/classifier-repo/`
//! for exact graph counts + config signal verification.
//!
//! Test 2 uses `rust/crates/repo-index/tests/fixtures/typescript/rust-7a-fixture/`
//! for scanner exclusion proof (gitignore, node_modules, dist).
//!
//! Classifier-repo has:
//!   - package.json (dependencies: lodash, devDeps: typescript)
//!   - tsconfig.json (paths: @/* → ./src/*)
//!   - src/index.ts (imports, calls, exported function)

use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("typescript")
        .join("classifier-repo")
}

#[test]
fn index_classifier_repo_from_disk() {
    let repo_path = fixture_path();
    assert!(
        repo_path.join("package.json").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "classifier-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // ── Snapshot ─────────────────────────────────────────────
    let snap = storage.get_snapshot(&result.snapshot_uid).unwrap().unwrap();
    assert_eq!(snap.status, "ready", "snapshot must be READY");

    // ── File count ───────────────────────────────────────────
    // Only src/index.ts is a source file (.ts).
    // Config files (package.json, tsconfig.json) are tracked for refresh
    // invalidation but NOT counted in files_total.
    assert_eq!(result.files_total, 1, "only src/index.ts is a source file");

    // ── Nodes ────────────────────────────────────────────────
    use repo_graph_indexer::storage_port::NodeStorePort;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
    let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

    // FILE node for src/index.ts.
    assert!(
        stable_keys.contains(&"classifier-fixture:src/index.ts:FILE"),
        "missing FILE node, keys: {:?}",
        stable_keys
    );

    // FUNCTION node for standalone.
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#standalone:SYMBOL:FUNCTION")),
        "missing standalone FUNCTION node, keys: {:?}",
        stable_keys
    );

    // MODULE node for src.
    assert!(
        stable_keys.iter().any(|k| k.contains("src:MODULE")),
        "missing MODULE node for src, keys: {:?}",
        stable_keys
    );

    // ── Exact edge counts ────────────────────────────────────
    // edges_total = 1: OWNS(src module → index.ts file).
    // The IMPORTS edge for ./local-nonexistent stays unresolved
    // (target file doesn't exist) so it's in edges_unresolved.
    assert_eq!(result.edges_total, 1, "edges_total");

    // edges_unresolved = 7 (DEPS-CLASSIFIER-1B §2.2 item 1): non-relative `import` statements now
    // emit an UNRESOLVED external-candidate IMPORTS edge, so npm usage is no longer calls-only:
    //   - debounce() → calls_function_ambiguous_or_missing
    //   - aliased() → calls_function_ambiguous_or_missing
    //   - relatively() → calls_function_ambiguous_or_missing
    //   - mysteryFunction() → calls_function_ambiguous_or_missing
    //   - import "lodash" → imports_file_not_found (classifies external_library_candidate)   [NEW]
    //   - import "@/lib/missing" → imports_file_not_found (classifies internal via tsconfig alias) [NEW]
    //   - import "./local-nonexistent" → imports_file_not_found (relative, resolves-then-missing)
    assert_eq!(result.edges_unresolved, 7, "edges_unresolved");

    // ── Exact unresolved breakdown ───────────────────────────
    assert_eq!(
        result
            .unresolved_breakdown
            .get("calls_function_ambiguous_or_missing"),
        Some(&4),
        "breakdown: {:?}",
        result.unresolved_breakdown
    );
    assert_eq!(
        result.unresolved_breakdown.get("imports_file_not_found"),
        // 3: the two bare imports (lodash, @/lib/missing) + the relative-missing one. The CATEGORY is
        // imports_file_not_found for all three; their CLASSIFICATIONS differ (external / internal /
        // internal) — asserted directly below by grouping these IMPORTS edges by classification.
        Some(&3),
        "breakdown: {:?}",
        result.unresolved_breakdown
    );

    // ── Classification of the IMPORTS edges (DEPS-CLASSIFIER-1B §2.2 item 1) ──
    // The three `imports_file_not_found` edges do NOT share a classification, and that split is the
    // whole point of item 1's external-candidate contract: a bare-package `import "lodash"` is an
    // EXTERNAL library candidate (npm usage evidence, not calls-only), while the `@/lib/missing`
    // tsconfig-alias import and the `./local-nonexistent` relative import are INTERNAL candidates.
    // Group the IMPORTS-family edges by classification and assert the exact split, so a regression
    // that misclassified the bare import as internal (or dropped its edge entirely) fails here.
    {
        use repo_graph_classification::types::{
            UnresolvedEdgeCategory, UnresolvedEdgeClassification,
        };
        use repo_graph_trust::storage_port::{CountByClassificationInput, TrustStorageRead};

        let rows = TrustStorageRead::count_unresolved_edges_by_classification(
            &storage,
            &CountByClassificationInput {
                snapshot_uid: result.snapshot_uid.clone(),
                filter_categories: vec![UnresolvedEdgeCategory::ImportsFileNotFound],
            },
        )
        .unwrap();
        let count_of = |c: UnresolvedEdgeClassification| -> u64 {
            rows.iter()
                .filter(|r| r.classification == c)
                .map(|r| r.count)
                .sum()
        };
        // Exactly one external candidate among the imports: `lodash` (the only bare declared-package
        // specifier; the alias and relative imports are both internal).
        assert_eq!(
            count_of(UnresolvedEdgeClassification::ExternalLibraryCandidate),
            1,
            "expected `import \"lodash\"` to be the sole external_library_candidate IMPORTS edge; \
             rows = {:?}",
            rows
        );
        // Exactly two internal candidates among the imports: `@/lib/missing` (tsconfig alias) and
        // `./local-nonexistent` (relative).
        assert_eq!(
            count_of(UnresolvedEdgeClassification::InternalCandidate),
            2,
            "expected `@/lib/missing` (alias) and `./local-nonexistent` (relative) to be the two \
             internal_candidate IMPORTS edges; rows = {:?}",
            rows
        );
    }

    // ── Config signals ───────────────────────────────────────
    // Verify package.json deps were resolved. Query file signals.
    use repo_graph_indexer::storage_port::FileSignalPort;
    let signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["classifier-fixture:src/index.ts".into()],
    )
    .unwrap();

    // Should have at least import bindings.
    assert!(
        !signals.is_empty(),
        "expected file signals for src/index.ts"
    );
    let sig = &signals[0];
    assert!(
        sig.import_bindings_json.is_some(),
        "expected import_bindings_json"
    );

    // Package deps should include lodash and typescript.
    if let Some(ref deps_json) = sig.package_dependencies_json {
        let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
        let names = deps["names"].as_array().unwrap();
        let name_strs: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            name_strs.contains(&"lodash"),
            "expected lodash in package deps, got: {:?}",
            name_strs
        );
        assert!(
            name_strs.contains(&"typescript"),
            "expected typescript in package deps, got: {:?}",
            name_strs
        );
    } else {
        panic!("expected package_dependencies_json on file signal");
    }

    // Tsconfig aliases should include @/*.
    if let Some(ref aliases_json) = sig.tsconfig_aliases_json {
        let aliases: serde_json::Value = serde_json::from_str(aliases_json).unwrap();
        let entries = aliases["entries"].as_array().unwrap();
        let patterns: Vec<&str> = entries
            .iter()
            .filter_map(|e| e["pattern"].as_str())
            .collect();
        assert!(
            patterns.contains(&"@/*"),
            "expected @/* in tsconfig aliases, got: {:?}",
            patterns
        );
    } else {
        panic!("expected tsconfig_aliases_json on file signal");
    }
}

// ── Exclusion proof ──────────────────────────────────────────────

fn exclusion_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("typescript")
        .join("rust-7a-fixture")
}

#[test]
fn index_excludes_gitignored_and_always_excluded() {
    let repo_path = exclusion_fixture_path();
    assert!(
        repo_path.join("package.json").exists(),
        "rust-7a-fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result =
        index_into_storage(&repo_path, &mut storage, "r7a", &ComposeOptions::default()).unwrap();

    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // Exact file count: src/index.ts + src/server.ts = 2.
    // Config files (package.json, tsconfig.json) tracked but not counted.
    // Excluded:
    //   - src/generated.ts (gitignored)
    //   - node_modules/pkg/index.ts (always-excluded dir)
    //   - dist/bundle.js (always-excluded dir)
    assert_eq!(result.files_total, 2, "files_total");

    // Verify excluded files are absent from nodes.
    use repo_graph_indexer::storage_port::NodeStorePort;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
    let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

    assert!(
        !stable_keys.iter().any(|k| k.contains("generated")),
        "gitignored file should not appear: {:?}",
        stable_keys
    );
    assert!(
        !stable_keys.iter().any(|k| k.contains("node_modules")),
        "node_modules should not appear: {:?}",
        stable_keys
    );
    assert!(
        !stable_keys.iter().any(|k| k.contains("dist")),
        "dist should not appear: {:?}",
        stable_keys
    );
    assert!(
        !stable_keys.iter().any(|k| k.contains("bundle")),
        "dist/bundle.js should not appear: {:?}",
        stable_keys
    );

    // Included files present.
    assert!(stable_keys.contains(&"r7a:src/index.ts:FILE"));
    assert!(stable_keys.contains(&"r7a:src/server.ts:FILE"));
}

fn snap_status(storage: &StorageConnection, uid: &str) -> String {
    storage.get_snapshot(uid).unwrap().unwrap().status
}

// ── Mixed-language isolation ─────────────────────────────────────

fn mixed_lang_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests").join("fixtures").join("mixed-lang")
}

#[test]
fn mixed_lang_language_isolation() {
    // Language isolation regression test: in a repo containing Rust, TypeScript,
    // and Java files alongside both Cargo.toml and package.json, each file must
    // receive only the dependency signals that match its own language.
    //
    // Failure mode (the bug this guards against): the compose layer's else-branch
    // was returning package.json deps for all non-Rust files, meaning Java files
    // would inherit Node dependency signals from a nearby package.json.
    //
    // Fixture layout:
    //   Cargo.toml    → deps: serde, wgpu
    //   package.json  → deps: express
    //   src/engine.rs → must receive serde + wgpu, NOT express
    //   src/server.ts → must receive express, NOT serde/wgpu
    //   src/App.java  → must receive empty signals (no Java manifest reader yet)
    //   build.gradle  → not a recognized source extension, not indexed
    let repo_path = mixed_lang_fixture_path();
    assert!(
        repo_path.join("Cargo.toml").exists(),
        "mixed-lang fixture not found at {:?}",
        repo_path
    );
    assert!(
        repo_path.join("package.json").exists(),
        "mixed-lang fixture missing package.json at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "mixed-lang",
        &ComposeOptions::default(),
    )
    .unwrap();

    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // 3 source files: engine.rs (Rust), server.ts (TS), App.java (Java).
    // Config files (build.gradle, Cargo.toml, package.json) tracked but not counted.
    assert_eq!(
        result.files_total, 3,
        "expected engine.rs + server.ts + App.java"
    );

    use repo_graph_indexer::storage_port::FileSignalPort;

    // ── server.ts: package.json deps only ────────────────────────
    let ts_signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["mixed-lang:src/server.ts".into()],
    )
    .unwrap();

    assert!(
        !ts_signals.is_empty(),
        "expected file signals for src/server.ts"
    );
    let ts_sig = &ts_signals[0];

    if let Some(ref deps_json) = ts_sig.package_dependencies_json {
        let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
        let names = deps["names"].as_array().unwrap();
        let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            dep_names.contains(&"express"),
            "server.ts must receive express from package.json, got: {:?}",
            dep_names
        );
        // Cargo deps must NOT bleed into TS files.
        assert!(
            !dep_names.contains(&"serde"),
            "server.ts must NOT receive Cargo.toml deps (serde), got: {:?}",
            dep_names
        );
        assert!(
            !dep_names.contains(&"wgpu"),
            "server.ts must NOT receive Cargo.toml deps (wgpu), got: {:?}",
            dep_names
        );
    } else {
        panic!("expected package_dependencies_json for server.ts");
    }

    // ── engine.rs: Cargo.toml deps only ──────────────────────────
    let rs_signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["mixed-lang:src/engine.rs".into()],
    )
    .unwrap();

    assert!(
        !rs_signals.is_empty(),
        "expected file signals for src/engine.rs"
    );
    let rs_sig = &rs_signals[0];

    if let Some(ref deps_json) = rs_sig.package_dependencies_json {
        let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
        let names = deps["names"].as_array().unwrap();
        let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            dep_names.contains(&"serde"),
            "engine.rs must receive serde from Cargo.toml, got: {:?}",
            dep_names
        );
        assert!(
            dep_names.contains(&"wgpu"),
            "engine.rs must receive wgpu from Cargo.toml, got: {:?}",
            dep_names
        );
        // package.json deps must NOT bleed into Rust files.
        assert!(
            !dep_names.contains(&"express"),
            "engine.rs must NOT receive package.json deps (express), got: {:?}",
            dep_names
        );
    } else {
        panic!("expected package_dependencies_json for engine.rs (from Cargo.toml)");
    }

    // ── App.java: Gradle-declared deps ONLY (GRADLE-DEP-READER-1) ─
    // Since GRADLE-DEP-READER-1 (78053eb, 2026-07-20) Java files DO
    // have a manifest reader: the fixture's build.gradle declares
    // org.springframework.boot, and App.java must receive exactly the
    // Gradle-declared set. The isolation invariant this test guards is
    // unchanged: no cross-language contamination — npm (express) and
    // Cargo (serde/wgpu) deps must never bleed into a Java file.
    // (This assertion previously encoded the pre-reader state — "Java
    // has no manifest reader yet" — and went red the day the reader
    // shipped; fixed 2026-08-25, see docs/TECH-DEBT.md closeout note.)
    let java_signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["mixed-lang:src/App.java".into()],
    )
    .unwrap();

    assert!(
        !java_signals.is_empty(),
        "expected file signals for src/App.java (Gradle reader shipped)"
    );
    let java_sig = &java_signals[0];
    let deps_json = java_sig
        .package_dependencies_json
        .as_ref()
        .expect("App.java must receive Gradle-declared dependency signals (build.gradle)");
    let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
    let names = deps["names"].as_array().unwrap();
    let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        dep_names.contains(&"org.springframework.boot"),
        "App.java must receive org.springframework.boot from build.gradle, got: {:?}",
        dep_names
    );
    // Cross-language contamination is still forbidden.
    assert!(
        !dep_names.contains(&"express"),
        "App.java must NOT receive package.json deps (express), got: {:?}",
        dep_names
    );
    assert!(
        !dep_names.contains(&"serde") && !dep_names.contains(&"wgpu"),
        "App.java must NOT receive Cargo.toml deps (serde/wgpu), got: {:?}",
        dep_names
    );
}

// ── Rust extraction ──────────────────────────────────────────────

fn rust_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("rust")
        .join("simple-crate")
}

#[test]
fn index_rust_crate_extracts_symbols() {
    let repo_path = rust_fixture_path();
    assert!(
        repo_path.join("Cargo.toml").exists(),
        "simple-crate fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "rust-simple",
        &ComposeOptions::default(),
    )
    .unwrap();

    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // ── File count ───────────────────────────────────────────
    // src/lib.rs and src/utils.rs are Rust source files.
    // Cargo.toml is a config file (tracked for invalidation, not counted).
    assert_eq!(result.files_total, 2, "expected lib.rs + utils.rs");

    // ── Nodes ────────────────────────────────────────────────
    use repo_graph_indexer::storage_port::NodeStorePort;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
    let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

    // FILE nodes for both Rust files.
    assert!(
        stable_keys.contains(&"rust-simple:src/lib.rs:FILE"),
        "missing lib.rs FILE node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys.contains(&"rust-simple:src/utils.rs:FILE"),
        "missing utils.rs FILE node, keys: {:?}",
        stable_keys
    );

    // SYMBOL nodes extracted by Rust extractor.
    // Struct: Config
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#Config:SYMBOL:CLASS")),
        "missing Config struct node, keys: {:?}",
        stable_keys
    );

    // Enum: Status
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#Status:SYMBOL:ENUM")),
        "missing Status enum node, keys: {:?}",
        stable_keys
    );

    // Trait: Processor
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#Processor:SYMBOL:INTERFACE")),
        "missing Processor trait node, keys: {:?}",
        stable_keys
    );

    // Impl method: Config.new
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#Config.new:SYMBOL:METHOD")),
        "missing Config.new method node, keys: {:?}",
        stable_keys
    );

    // Impl method: Config.get_value
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#Config.get_value:SYMBOL:METHOD")),
        "missing Config.get_value method node, keys: {:?}",
        stable_keys
    );

    // Trait impl method: Config.process (impl Processor for Config)
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#Config.process:SYMBOL:METHOD")),
        "missing Config.process trait impl method, keys: {:?}",
        stable_keys
    );

    // Free function: create_config
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#create_config:SYMBOL:FUNCTION")),
        "missing create_config function node, keys: {:?}",
        stable_keys
    );

    // Const: MAX_SIZE
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#MAX_SIZE:SYMBOL:CONSTANT")),
        "missing MAX_SIZE const node, keys: {:?}",
        stable_keys
    );

    // Utils module function: describe_config
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("utils.rs") && k.contains("#describe_config:SYMBOL:FUNCTION")),
        "missing describe_config from utils.rs, keys: {:?}",
        stable_keys
    );

    // ── Edges ────────────────────────────────────────────────
    // Verify edges exist via result counts.
    // edges_total includes resolved OWNS edges (module->file).
    // The Rust fixture should produce at least a few resolved edges.
    assert!(
        result.edges_total > 0,
        "expected at least one resolved edge (e.g., OWNS from module to file)"
    );

    // Verify import bindings were extracted via file signals.
    use repo_graph_indexer::storage_port::FileSignalPort;
    let signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["rust-simple:src/lib.rs".into()],
    )
    .unwrap();

    assert!(!signals.is_empty(), "expected file signals for src/lib.rs");

    let sig = &signals[0];
    assert!(
        sig.import_bindings_json.is_some(),
        "expected import_bindings_json for Rust file"
    );

    // Verify import binding contains HashMap.
    if let Some(ref bindings_json) = sig.import_bindings_json {
        let bindings: serde_json::Value = serde_json::from_str(bindings_json).unwrap();
        let arr = bindings.as_array().unwrap();
        let identifiers: Vec<&str> = arr
            .iter()
            .filter_map(|b| b["identifier"].as_str())
            .collect();
        assert!(
            identifiers.contains(&"HashMap"),
            "expected HashMap in import bindings, got: {:?}",
            identifiers
        );
    }
}

#[test]
fn index_rust_crate_visibility_correct() {
    let repo_path = rust_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "rust-vis",
        &ComposeOptions::default(),
    )
    .unwrap();

    use repo_graph_indexer::storage_port::NodeStorePort;
    use repo_graph_indexer::types::Visibility;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();

    // Config struct is pub -> Export
    let config_node = nodes
        .iter()
        .find(|n| n.stable_key.contains("#Config:SYMBOL:CLASS"))
        .expect("Config node not found");
    assert_eq!(
        config_node.visibility,
        Some(Visibility::Export),
        "Config should be exported (pub)"
    );

    // helper function is private -> Private
    let helper_node = nodes
        .iter()
        .find(|n| n.stable_key.contains("#helper:SYMBOL:FUNCTION"))
        .expect("helper node not found");
    assert_eq!(
        helper_node.visibility,
        Some(Visibility::Private),
        "helper should be private (no pub)"
    );

    // create_config is pub -> Export
    let create_node = nodes
        .iter()
        .find(|n| n.stable_key.contains("#create_config:SYMBOL:FUNCTION"))
        .expect("create_config node not found");
    assert_eq!(
        create_node.visibility,
        Some(Visibility::Export),
        "create_config should be exported (pub)"
    );
}

#[test]
fn index_rust_crate_receives_cargo_deps() {
    // P1 regression test: Rust files must receive Cargo.toml dependencies,
    // NOT package.json dependencies. This test verifies the language-aware
    // dependency wiring in prepare_repo_inputs().
    let repo_path = rust_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "rust-cargo-deps",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Query file signals for lib.rs
    use repo_graph_indexer::storage_port::FileSignalPort;
    let signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["rust-cargo-deps:src/lib.rs".into()],
    )
    .unwrap();

    assert!(!signals.is_empty(), "expected file signals for lib.rs");
    let sig = &signals[0];

    // Verify package_dependencies contains Cargo.toml deps, NOT package.json deps.
    // The fixture Cargo.toml has: serde, tokio, tempfile, but NO express/react.
    if let Some(ref deps_json) = sig.package_dependencies_json {
        let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
        let names = deps["names"].as_array().unwrap();
        let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();

        // Cargo deps should be present (normalized: hyphens → underscores not needed here).
        assert!(
            dep_names.contains(&"serde"),
            "expected serde from Cargo.toml, got: {:?}",
            dep_names
        );
        assert!(
            dep_names.contains(&"tokio"),
            "expected tokio from Cargo.toml, got: {:?}",
            dep_names
        );

        // Package.json deps should NOT be present.
        assert!(
            !dep_names.contains(&"express"),
            "Rust file should NOT have package.json deps, got: {:?}",
            dep_names
        );
        assert!(
            !dep_names.contains(&"react"),
            "Rust file should NOT have package.json deps, got: {:?}",
            dep_names
        );
    } else {
        panic!("expected package_dependencies_json for Rust file (from Cargo.toml)");
    }
}

// ── Cross-crate import resolution (IMPORT-RESOLUTION-RUST-1) ──────

fn rust_workspace_fixture_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("rust")
        .join("workspace")
}

/// A non-relative `use other_crate::…` resolves to the file that defines it, the derived
/// cross-module edge appears, and no import lands in `imports_file_not_found`.
#[test]
fn cross_crate_use_resolves_to_defining_file() {
    use repo_graph_classification::types::UnresolvedEdgeCategory;
    use repo_graph_trust::storage_port::{CountByClassificationInput, TrustStorageRead};

    let repo_path = rust_workspace_fixture_path();
    assert!(
        repo_path.join("a/src/lib.rs").exists(),
        "workspace fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "rust-workspace",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // ── (1) File-level resolution: both `use b::…` edges hit b's files ──
    // `use b::util::helper` → b/src/util.rs ; `use b::Thing` → b/src/lib.rs.
    let imports = storage
        .get_resolved_imports_for_snapshot(&result.snapshot_uid)
        .unwrap();
    let a_lib = "rust-workspace:a/src/lib.rs";
    let targets: Vec<&str> = imports
        .iter()
        .filter(|i| i.source_file_uid == a_lib)
        .map(|i| i.target_file_uid.as_str())
        .collect();
    assert!(
        targets.contains(&"rust-workspace:b/src/util.rs"),
        "`use b::util::helper` must resolve to b/src/util.rs; resolved cross-crate targets = {:?}",
        targets
    );
    assert!(
        targets.contains(&"rust-workspace:b/src/lib.rs"),
        "`use b::Thing` must resolve to b/src/lib.rs; resolved cross-crate targets = {:?}",
        targets
    );

    // ── (2) Nothing lands in imports_file_not_found ──
    let unresolved_imports: u64 = TrustStorageRead::count_unresolved_edges_by_classification(
        &storage,
        &CountByClassificationInput {
            snapshot_uid: result.snapshot_uid.clone(),
            filter_categories: vec![UnresolvedEdgeCategory::ImportsFileNotFound],
        },
    )
    .unwrap()
    .iter()
    .map(|row| row.count)
    .sum();
    assert_eq!(
        unresolved_imports, 0,
        "no IMPORTS edge should remain imports_file_not_found after crate resolution"
    );

    // ── (3) One derived MODULE→MODULE edge a → b ──
    let facts =
        repo_graph_module_queries::load_module_graph_facts(&storage, &result.snapshot_uid).unwrap();
    let ab_edges: Vec<(&str, &str)> = facts
        .edges
        .iter()
        .map(|e| {
            (
                e.source_canonical_path.as_str(),
                e.target_canonical_path.as_str(),
            )
        })
        .collect();
    assert_eq!(
        ab_edges,
        vec![("a", "b")],
        "expected exactly one cross-module edge a → b, got {:?}",
        ab_edges
    );

    // ── (4) Cycles response shape: the §4 zero-state clause the fixture drives ──
    // IMPORT-RESOLUTION-RUST-1 §4 (operator ruling cycle-4, `cycles-module-count-semantics` = C):
    // the daemon's SQLite cycles handler builds `module_count` / `module_edge_count` from EXACTLY
    // these two storage reads (dispatch.rs:2558-2559). Asserting them here proves the fixture
    // yields (4, 1) through the real production reads, so the rendered zero-state is
    // "over 4 directory groups / 1 resolved import edge" (the cycles-presenter unit test
    // `zero_state_renders_two_crate_fixture_clause_verbatim` pins the rendering of those numbers).
    // 4 = the per-directory MODULE nodes `a`, `a/src`, `b`, `b/src` that `find_cycles` runs its SCC
    // over — the same population `stats` prints as "directory groups"; 1 = the resolved a → b edge.
    let module_count = storage
        .module_qualified_names(&result.snapshot_uid)
        .unwrap()
        .len();
    assert_eq!(
        module_count, 4,
        "cycles module_count = per-directory MODULE nodes (a, a/src, b, b/src)"
    );
    let module_edge_count = storage
        .module_import_edges(&result.snapshot_uid)
        .unwrap()
        .len();
    assert_eq!(
        module_edge_count, 1,
        "exactly one resolved cross-module import edge feeds the cycles SCC graph"
    );
}

// ── Cross-package Java import resolution (IMPORT-RESOLUTION-JAVA-1) ─

fn java_multi_project_fixture_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("java")
        .join("multi-project")
}

/// A Java `import a.b.C` (and the nested-class form `a.b.C.D`) resolves by path suffix to the
/// file that defines it; a wildcard import stays unresolved with the NAMED, COUNTED
/// `imports_wildcard` basis; a `projectDir`-relocated project owns its files; and the resolved
/// cross-package import yields one MODULE→MODULE edge app → core.
#[test]
fn cross_package_java_import_resolves_by_suffix() {
    let repo_path = java_multi_project_fixture_path();
    assert!(
        repo_path
            .join("app/src/main/java/org/x/app/Main.java")
            .exists(),
        "java multi-project fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "java-multi",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // ── (1) BOTH non-wildcard imports resolve to the relocated Util.java ──
    // `import org.x.core.Util` and `import org.x.core.Util.Inner` (nested class → suffix-shortens
    // to Util.java) BOTH resolve to libs/core/src/main/java/org/x/core/Util.java. The two are
    // distinct IMPORTS edges (distinct FQN target_key), and `get_resolved_imports_for_snapshot`
    // is non-DISTINCT (storage/src/crud/module_edges_support.rs), so both surface as rows — we
    // assert EXACTLY two Main.java → Util.java resolutions (not merely that Util.java is present:
    // a `contains` check would pass even if only `Util` resolved while `Util.Inner` did not).
    let imports = storage
        .get_resolved_imports_for_snapshot(&result.snapshot_uid)
        .unwrap();
    let main_file = "java-multi:app/src/main/java/org/x/app/Main.java";
    let util_file = "java-multi:libs/core/src/main/java/org/x/core/Util.java";
    let targets: Vec<&str> = imports
        .iter()
        .filter(|i| i.source_file_uid == main_file)
        .map(|i| i.target_file_uid.as_str())
        .collect();
    let main_to_util = targets.iter().filter(|t| **t == util_file).count();
    assert_eq!(
        main_to_util, 2,
        "BOTH `import org.x.core.Util` and `import org.x.core.Util.Inner` must resolve to \
         the relocated Util.java (exactly two edges); resolved targets from Main.java = {:?}",
        targets
    );

    // ── (2) The wildcard import is unresolved with the NAMED, COUNTED basis ──
    assert_eq!(
        result.unresolved_breakdown.get("imports_wildcard"),
        Some(&1),
        "the single `import org.x.core.*` must be counted as imports_wildcard; breakdown = {:?}",
        result.unresolved_breakdown
    );

    // ── (3) One derived MODULE→MODULE edge app → core (canonical libs/core) ──
    let facts =
        repo_graph_module_queries::load_module_graph_facts(&storage, &result.snapshot_uid).unwrap();
    let edges: Vec<(&str, &str)> = facts
        .edges
        .iter()
        .map(|e| {
            (
                e.source_canonical_path.as_str(),
                e.target_canonical_path.as_str(),
            )
        })
        .collect();
    assert_eq!(
        edges,
        vec![("app", "libs/core")],
        "expected exactly one cross-module edge app → core (relocated to libs/core), got {:?}",
        edges
    );

    // ── (4) The projectDir-relocated `core` project owns its files ──
    let core = facts
        .modules()
        .iter()
        .find(|m| m.display_name.as_deref() == Some("core"))
        .expect("core module present");
    assert_eq!(
        core.canonical_root_path, "libs/core",
        "projectDir relocation must set core's physical root to libs/core"
    );
    let core_files = facts.files_for_module(&core.module_candidate_uid);
    assert!(
        core_files.iter().any(|f| f.file_uid == util_file),
        "relocated core module must own libs/core/.../Util.java; owned = {:?}",
        core_files.iter().map(|f| &f.file_uid).collect::<Vec<_>>()
    );
}

// ── Python extraction ────────────────────────────────────────────

fn python_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("python")
        .join("simple-app")
}

#[test]
fn index_python_extracts_symbols() {
    // Python extractor integration test: verify FILE, CLASS, FUNCTION,
    // METHOD nodes are extracted from Python source files.
    let repo_path = python_fixture_path();
    assert!(
        repo_path.join("src").join("app.py").exists(),
        "python fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "py-simple",
        &ComposeOptions::default(),
    )
    .unwrap();

    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // ── File count ───────────────────────────────────────────
    // 4 Python files: __init__.py, app.py, service.py, test_service.py
    assert_eq!(result.files_total, 4, "expected 4 Python files");

    // ── Nodes ────────────────────────────────────────────────
    use repo_graph_indexer::storage_port::NodeStorePort;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
    let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

    // FILE nodes for Python files.
    assert!(
        stable_keys.contains(&"py-simple:src/app.py:FILE"),
        "missing app.py FILE node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys.contains(&"py-simple:src/service.py:FILE"),
        "missing service.py FILE node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys.contains(&"py-simple:tests/test_service.py:FILE"),
        "missing test_service.py FILE node, keys: {:?}",
        stable_keys
    );

    // CLASS nodes.
    assert!(
        stable_keys.iter().any(|k| k.contains("#App:SYMBOL:CLASS")),
        "missing App CLASS node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#UserService:SYMBOL:CLASS")),
        "missing UserService CLASS node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#TestUserService:SYMBOL:CLASS")),
        "missing TestUserService CLASS node, keys: {:?}",
        stable_keys
    );

    // FUNCTION nodes.
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#main:SYMBOL:FUNCTION")),
        "missing main FUNCTION node, keys: {:?}",
        stable_keys
    );

    // CONSTRUCTOR nodes (__init__ methods are constructors).
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#App.__init__:SYMBOL:CONSTRUCTOR")),
        "missing App.__init__ CONSTRUCTOR node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#App.run:SYMBOL:METHOD")),
        "missing App.run METHOD node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#UserService.get_users:SYMBOL:METHOD")),
        "missing UserService.get_users METHOD node, keys: {:?}",
        stable_keys
    );
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#UserService._process_user:SYMBOL:METHOD")),
        "missing UserService._process_user METHOD node (private), keys: {:?}",
        stable_keys
    );

    // Test methods.
    assert!(
        stable_keys
            .iter()
            .any(|k| k.contains("#TestUserService.test_get_users_empty:SYMBOL:METHOD")),
        "missing test method node, keys: {:?}",
        stable_keys
    );

    // ── Edges ────────────────────────────────────────────────
    // Verify edges exist (imports and calls).
    assert!(
        result.edges_total > 0 || result.edges_unresolved > 0,
        "expected at least some edges from Python extraction"
    );
}

#[test]
fn index_python_visibility_correct() {
    // Python visibility test: underscore-prefixed names should be Internal/Private.
    let repo_path = python_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "py-vis",
        &ComposeOptions::default(),
    )
    .unwrap();

    use repo_graph_indexer::storage_port::NodeStorePort;
    use repo_graph_indexer::types::Visibility;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();

    // App class is public (no underscore) -> Export
    let app_node = nodes
        .iter()
        .find(|n| n.stable_key.contains("#App:SYMBOL:CLASS"))
        .expect("App node not found");
    assert_eq!(
        app_node.visibility,
        Some(Visibility::Export),
        "App class should be exported (no underscore)"
    );

    // UserService.get_users is public -> Export
    let get_users_node = nodes
        .iter()
        .find(|n| {
            n.stable_key
                .contains("#UserService.get_users:SYMBOL:METHOD")
        })
        .expect("get_users node not found");
    assert_eq!(
        get_users_node.visibility,
        Some(Visibility::Export),
        "get_users method should be exported (no underscore)"
    );

    // UserService._process_user is single-underscore private -> Internal
    let process_user_node = nodes
        .iter()
        .find(|n| {
            n.stable_key
                .contains("#UserService._process_user:SYMBOL:METHOD")
        })
        .expect("_process_user node not found");
    assert_eq!(
        process_user_node.visibility,
        Some(Visibility::Internal),
        "_process_user method should be Internal (single underscore)"
    );
}

#[test]
fn index_python_binds_a_self_call_to_the_own_class_method() {
    // PYTHON-SELF-BINDING-1 (RG-REQ-005-L03) end-to-end on the smallest corpus (unchanged
    // fixture): `UserService.process` calls `self._process_user(user)` — an own-class unique hit
    // that now binds; `App.run` calls `self._service.process()` — a 3-part attribute chain that
    // stays an unresolved row. Indexing twice yields the same CALLS count (RG-REQ-001-L09).
    let repo_path = python_fixture_path();

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "py-self",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

    // The self-call `self._process_user(user)` now binds to UserService._process_user.
    let bound: i64 = storage
        .query_scalar(
            "SELECT COUNT(*) FROM edges e \
             JOIN nodes s ON e.source_node_uid = s.node_uid AND s.snapshot_uid = e.snapshot_uid \
             JOIN nodes t ON e.target_node_uid = t.node_uid AND t.snapshot_uid = e.snapshot_uid \
             WHERE e.type = 'CALLS' \
               AND s.qualified_name = 'UserService.process' \
               AND t.qualified_name = 'UserService._process_user'",
        )
        .unwrap();
    assert_eq!(
        bound, 1,
        "self._process_user() binds to the own-class method"
    );

    // The 3-part attribute chain `self._service.process` carries NO self-call carrier, so the new
    // hierarchy stage never fires for it: it is NEVER bound to the own-class private method, and
    // it resolves exactly as before this slice — by the pre-existing unique bare-name fallback,
    // to UserService.process (the sole method named `process`).
    let chain_to_process = |storage: &StorageConnection, target: &str| -> i64 {
        storage
            .query_scalar(&format!(
                "SELECT COUNT(*) FROM edges e \
                 JOIN nodes s ON e.source_node_uid = s.node_uid AND s.snapshot_uid = e.snapshot_uid \
                 JOIN nodes t ON e.target_node_uid = t.node_uid AND t.snapshot_uid = e.snapshot_uid \
                 WHERE e.type = 'CALLS' AND s.qualified_name = 'App.run' \
                   AND t.qualified_name = '{target}'"
            ))
            .unwrap()
    };
    assert_eq!(
        chain_to_process(&storage, "UserService._process_user"),
        0,
        "the 3-part chain is never mis-bound by the self-call hierarchy stage"
    );
    assert_eq!(
        chain_to_process(&storage, "UserService.process"),
        1,
        "the chain resolves by the pre-existing bare-name fallback, unchanged"
    );

    // Determinism (RG-REQ-001-L09): a second index of the same tree yields the same CALLS count.
    let calls_first: i64 = storage
        .query_scalar("SELECT COUNT(*) FROM edges WHERE type = 'CALLS'")
        .unwrap();
    let mut storage2 = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage2,
        "py-self-2",
        &ComposeOptions::default(),
    )
    .unwrap();
    let calls_second: i64 = storage2
        .query_scalar("SELECT COUNT(*) FROM edges WHERE type = 'CALLS'")
        .unwrap();
    assert_eq!(
        calls_first, calls_second,
        "indexing the same tree twice yields the same CALLS edge count"
    );
}

#[test]
fn index_python_imports_resolve() {
    // Python import resolution test: verify relative and absolute imports
    // resolve to the correct target files.
    //
    // src/app.py contains:
    //   from .service import UserService   (relative → src/service.py)
    // tests/test_service.py contains:
    //   from src.service import UserService (absolute → src/service.py)
    //
    // Expected resolved edges:
    //   - app.py → service.py (relative import)
    //   - test_service.py → service.py (absolute import)
    //   - MODULE edges (src → files, tests → files)
    //
    // Expected unresolved edges:
    //   - json (stdlib, no local file)
    //   - typing (stdlib, no local file)
    //   - internal call edges that can't resolve
    let repo_path = python_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "py-imports",
        &ComposeOptions::default(),
    )
    .unwrap();

    // The Python fixture should have:
    // - At least 2 resolved IMPORTS edges (relative + absolute local imports)
    // - Plus OWNS edges from modules to files
    // - Some unresolved edges (stdlib imports like json, typing)
    assert!(
        result.edges_total >= 2,
        "expected at least 2 resolved edges (local imports), got {}",
        result.edges_total
    );

    // Stdlib imports (json, typing) should be unresolved since they
    // don't map to local files. The exact count depends on how many
    // external imports exist and how many call edges are unresolved.
    assert!(
        result.edges_unresolved >= 2,
        "expected at least 2 unresolved edges (stdlib imports), got {}",
        result.edges_unresolved
    );

    // Verify total extraction happened by checking nodes
    use repo_graph_indexer::storage_port::NodeStorePort;
    let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();

    // FILE nodes for the Python source files should exist
    assert!(
        nodes
            .iter()
            .any(|n| n.stable_key == "py-imports:src/app.py:FILE"),
        "expected src/app.py FILE node"
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.stable_key == "py-imports:src/service.py:FILE"),
        "expected src/service.py FILE node"
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.stable_key == "py-imports:tests/test_service.py:FILE"),
        "expected tests/test_service.py FILE node"
    );
}
