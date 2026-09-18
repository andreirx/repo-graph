//! CALL-BINDING-RECEIVER-1 — resolver-seam regression for the receiver-typed call-binding
//! invariant (RG-REQ-005-L01 / RG-REQ-002-L01).
//!
//! WHAT THIS TEST IS — and is NOT (F-CBR-003 honesty):
//!
//! This is a RESOLVER-SEAM test. It exercises the real shared resolver entry point
//! (`resolve_edges`) with the exact `metadata_json` shapes the C++ extractor emits for a
//! `field_expression` call — `{"calleeName", "receiver", "receiverType"}`. Those shapes are NOT
//! invented here: they are the ones the extractor's own tests pin (`receiver_type_*` in
//! `repo-graph-cpp-extractor`, check CBR-C05). This test proves the resolver CONSUMES them
//! correctly; CBR-C05 proves the extractor PRODUCES them; together they bracket the seam.
//!
//! It is NOT an end-to-end index. It does not run tree-sitter, write source, build a SQLite
//! store, or run `callers`/`callees` queries — the `repo-graph-indexer` crate deliberately takes
//! NO dependency on any extractor adapter or on `repo-graph-storage` (see its `Cargo.toml` scope
//! note: "no tree-sitter grammars", "no extractor adapter implementations", "Does NOT depend on
//! repo-graph-storage concrete types"), and `tests/parity.rs` — the harness this crate ships —
//! indexes NOTHING (it runs pure routing/resolution/invalidation policy over JSON fixtures). A
//! true source→store→`callers`/`callees` C++ fixture test therefore cannot live in this crate; it
//! belongs in `repo-graph-repo-index`'s integration harness (which wires cpp-extractor + indexer +
//! storage). That end-to-end proof EXISTS and is CBR-C06:
//! `rust/crates/repo-index/tests/call_binding_receiver.rs` writes the A/B/X C++ fixture into a
//! temporary directory, indexes it through the full stack, and asserts the store-level invariant
//! (decision D-CBR-C06-HOME-1) — it is neither a follow-up nor a blocker. Alongside it, the real
//! corpus evidence for this slice is the live leveldb index proof (CBR-C10..C15: receiver-bearing
//! CALLS self-loops 155→0 on the full stack).
//!
//! Scenario (the `A`/`B`/`X` shape CBR-C06 names, at the resolver boundary):
//!
//! ```cpp
//! class A { void run(); };
//! class B {
//!   A* a_;   // a data member typed A
//!   X* x_;   // a data member typed X — X is NOT indexed
//!   void run() {
//!     a_->run();    // indirect receiver, typed A  → binds to A::run
//!     this->run();  // explicit self             → binds to B::run (the ONE legit self-loop)
//!     x_->run();    // indirect receiver, type X unknown → stays unresolved, counted
//!   }
//! };
//! ```

use std::collections::HashMap;

use repo_graph_indexer::resolver::{resolve_edges, ResolverIndex, ResolverNode};
use repo_graph_indexer::types::{EdgeType, ExtractedEdge, Resolution};

const CPP: &str = "cpp-core:0.1.0";

fn method_node(uid: &str, qualified_name: &str) -> ResolverNode {
    ResolverNode {
        node_uid: uid.into(),
        stable_key: uid.into(),
        name: qualified_name
            .rsplit("::")
            .next()
            .unwrap_or(qualified_name)
            .into(),
        qualified_name: Some(qualified_name.into()),
        kind: "SYMBOL".into(),
        subtype: Some("METHOD".into()),
        file_uid: Some("repo:src/f.cpp".into()),
        forward_decl: false,
    }
}

/// A C++ `field_expression` CALLS edge as the extractor emits it: `target_key` is the bare
/// method name; `metadata_json` carries `calleeName`, the `receiver` text and (when the
/// extractor could type it in-file) `receiverType`.
fn call_edge(uid: &str, source_uid: &str, callee: &str, metadata: &str) -> ExtractedEdge {
    ExtractedEdge {
        edge_uid: uid.into(),
        snapshot_uid: "snap1".into(),
        repo_uid: "repo".into(),
        source_node_uid: source_uid.into(),
        target_key: callee.into(),
        edge_type: EdgeType::Calls,
        resolution: Resolution::Static,
        extractor: CPP.into(),
        location: None,
        metadata_json: Some(metadata.into()),
    }
}

fn build_index(
    nodes_by_name: HashMap<String, Vec<ResolverNode>>,
    callers: Vec<ResolverNode>,
) -> ResolverIndex {
    let mut nodes_by_uid = HashMap::new();
    for n in callers {
        nodes_by_uid.insert(n.node_uid.clone(), n);
    }
    ResolverIndex {
        nodes_by_stable_key: HashMap::new(),
        nodes_by_name,
        nodes_by_uid,
        node_uid_to_file_uid: HashMap::new(),
        file_resolution: HashMap::new(),
        per_file_include_resolution: HashMap::new(),
        stable_key_to_uid: HashMap::new(),
        file_to_module: HashMap::new(),
        include_resolver: None,
        rust_crate_roots: HashMap::new(),
        java_suffix_index: HashMap::new(),
    }
}

#[test]
fn receiver_typed_calls_bind_by_type_never_to_the_caller_by_default() {
    let a_run = method_node("a_run", "A::run");
    let b_run = method_node("b_run", "B::run");

    let mut nodes_by_name = HashMap::new();
    nodes_by_name.insert("run".to_string(), vec![a_run, b_run.clone()]);

    // The caller of all three calls is `B::run`.
    let index = build_index(nodes_by_name, vec![b_run]);

    let edges = vec![
        // a_->run(): indirect receiver typed A → A::run.
        call_edge(
            "e_a",
            "b_run",
            "run",
            r#"{"calleeName":"run","receiver":"a_","receiverType":"A"}"#,
        ),
        // this->run(): explicit self → B::run (the one legitimate self-loop).
        call_edge(
            "e_this",
            "b_run",
            "run",
            r#"{"calleeName":"run","receiver":"this"}"#,
        ),
        // x_->run(): X is not indexed → unresolved, counted.
        call_edge(
            "e_x",
            "b_run",
            "run",
            r#"{"calleeName":"run","receiver":"x_","receiverType":"X"}"#,
        ),
    ];

    let result = resolve_edges(&edges, &index, None);

    // ── callers A::run == [B::run] ──────────────────────────────────────────
    let callers_of_a: Vec<&str> = result
        .resolved
        .iter()
        .filter(|e| e.target_node_uid == "a_run")
        .map(|e| e.source_node_uid.as_str())
        .collect();
    assert_eq!(
        callers_of_a,
        vec!["b_run"],
        "A::run is called only by B::run"
    );

    // ── callees B::run == {A::run, B::run} and NOTHING else ─────────────────
    let mut callees_of_b: Vec<&str> = result
        .resolved
        .iter()
        .filter(|e| e.source_node_uid == "b_run")
        .map(|e| e.target_node_uid.as_str())
        .collect();
    callees_of_b.sort_unstable();
    assert_eq!(
        callees_of_b,
        vec!["a_run", "b_run"],
        "callees are A::run (typed a_) and B::run (this->run self-call), nothing else"
    );

    // ── x_->run() stays unresolved with a calls_* category ──────────────────
    assert_eq!(
        result.still_unresolved.len(),
        1,
        "exactly the x_->run() call is unresolved"
    );
    let cat = format!("{:?}", result.still_unresolved[0].category).to_lowercase();
    assert!(
        cat.starts_with("calls"),
        "the unresolved call carries a calls_* category: {cat}"
    );

    // ── corpus assertion: NO CALLS self-loop with a receiver other than "this" ──
    let bad_self_loops = result
        .resolved
        .iter()
        .filter(|e| e.source_node_uid == e.target_node_uid)
        .filter(|e| {
            let meta = e.metadata_json.as_deref().unwrap_or("");
            let has_receiver = meta.contains("\"receiver\"");
            let is_this = meta.contains("\"receiver\":\"this\"");
            has_receiver && !is_this
        })
        .count();
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class (the RC-1 invariant)"
    );
}
