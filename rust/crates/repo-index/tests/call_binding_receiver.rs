//! CALL-BINDING-RECEIVER-1 — end-to-end receiver-typed call-binding invariant, indexed from
//! source through the FULL stack (RG-REQ-005-L01 / RG-REQ-005-L02 / RG-REQ-002-L01 /
//! RG-REQ-001-L03). This is check CBR-C06 as re-homed by decision D-CBR-C06-HOME-1.
//!
//! WHY THIS LIVES HERE (D-CBR-C06-HOME-1):
//!
//! CBR-C06 requires an end-to-end test that WRITES a C++ `A`/`B`/`X` fixture, INDEXES it, and
//! asserts callers/callees + an unresolved_edges row + the SQL self-loop invariant on the RESULTING
//! store. The `repo-graph-indexer` crate cannot host it — it takes NO dependency on any extractor
//! adapter or on `repo-graph-storage` concrete types, and its `tests/parity.rs` indexes nothing
//! (pure routing/resolution policy over JSON fixtures). The indexer crate's
//! `tests/call_binding_receiver.rs` is therefore an honest RESOLVER-SEAM test (it exercises
//! `resolve_edges` with the exact metadata shapes the extractor emits). THIS test is the real
//! source→store→callers/callees proof: `repo-graph-repo-index` wires cpp-extractor + indexer +
//! storage and already indexes C++ fixtures end-to-end (see `cpp_sb_1_integration.rs`). Together
//! they bracket the seam; nothing is mocked.
//!
//! Scenario (the exact `A`/`B`/`X` shape CBR-C06 names):
//!
//! ```cpp
//! class A { public: void run() { } };          // A::run is a real definition
//! class X;                                      // forward-declared, X::run is NOT indexed
//! class B {
//! public:
//!   A* a_;   // data member typed A
//!   X* x_;   // data member typed X — X::run is unknown
//!   void run() {                               // INLINE, so B's member types are in scope
//!     a_->run();    // indirect receiver typed A  → binds to A::run
//!     this->run();  // explicit self             → binds to B::run (the ONE legit self-loop)
//!     x_->run();    // indirect receiver, type X unknown → stays unresolved, counted
//!   }
//! };
//! ```
//!
//! Asserted on the indexed store:
//!   1. callers(A::run) == [B::run]                          (a_->run bound by receiver type A)
//!   2. callees(B::run) == {A::run, B::run} and nothing else (a_->run + this->run self-call)
//!   3. x_->run() is an `unresolved_edges` row with a `calls_*` category (counted)
//!   4. corpus assertion: CALLS self-loops whose metadata carries a receiver other than "this" == 0

use std::fs;
use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_path, ComposeOptions};
use repo_graph_storage::StorageConnection;

/// The one-file C++ translation unit that carries all three call shapes. Kept in ONE file so `B`'s
/// data-member types are visible to its INLINE `run()` (the per-file receiver-typing the extractor
/// performs; an out-of-line method in another file is the documented cross-file limit, not tested
/// here).
const FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class X;

class B {
public:
    A* a_;
    X* x_;
    void run() {
        a_->run();
        this->run();
        x_->run();
    }
};
"#;

fn temp_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("ab.cpp"), FIXTURE).unwrap();
    (dir, repo)
}

/// The stable_key of the unique METHOD node with this qualified name, or a panic that dumps every
/// method's qualified name so a shape drift is diagnosable rather than a bare unwrap.
fn method_stable_key(storage: &StorageConnection, snapshot_uid: &str, qualified: &str) -> String {
    let nodes = storage.query_all_nodes(snapshot_uid).unwrap();
    let matches: Vec<&repo_graph_storage::types::GraphNode> = nodes
        .iter()
        .filter(|n| n.qualified_name.as_deref() == Some(qualified))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one node with qualified_name {qualified:?}; method qualified_names in \
         the store: {:?}",
        nodes
            .iter()
            .filter(|n| n.subtype.as_deref() == Some("METHOD"))
            .map(|n| (n.qualified_name.clone(), n.stable_key.clone()))
            .collect::<Vec<_>>()
    );
    matches[0].stable_key.clone()
}

#[test]
fn indexed_cpp_binds_indirect_receiver_by_type_never_to_the_caller() {
    let (_dir, repo) = temp_repo();
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cbr-test", &ComposeOptions::default())
        .expect("indexing the C++ fixture must succeed");
    let snap = &result.snapshot_uid;

    let storage = StorageConnection::open(&db_path).unwrap();

    let a_run = method_stable_key(&storage, snap, "A::run");
    let b_run = method_stable_key(&storage, snap, "B::run");

    // ── (1) callers(A::run) == [B::run] ─────────────────────────────────────
    let callers = storage
        .find_direct_callers(snap, &a_run, &["CALLS"])
        .expect("callers query");
    let caller_qns: Vec<String> = callers
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert_eq!(
        caller_qns,
        vec!["B::run".to_string()],
        "A::run is called only by B::run (a_->run bound by receiver type A); \
         raw caller rows: {callers:?}"
    );

    // ── (2) callees(B::run) == {A::run, B::run} and nothing else ────────────
    let callees = storage
        .find_direct_callees(snap, &b_run, &["CALLS"])
        .expect("callees query");
    let mut callee_qns: Vec<String> = callees
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    callee_qns.sort();
    assert_eq!(
        callee_qns,
        vec!["A::run".to_string(), "B::run".to_string()],
        "callees of B::run are A::run (typed a_) and B::run (this->run self-call), nothing else; \
         raw callee rows: {callees:?}"
    );

    // ── (3) x_->run() is a counted unresolved_edges row with a calls_* category ──
    let x_unresolved: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM unresolved_edges \
             WHERE snapshot_uid = '{snap}' AND target_key = 'run' \
               AND category LIKE 'calls_%' \
               AND metadata_json LIKE '%\"receiver\":\"x_\"%'"
        ))
        .expect("unresolved x_ query");
    assert_eq!(
        x_unresolved, 1,
        "the x_->run() call (receiver type X, not indexed) must be exactly one unresolved_edges \
         row with a calls_* category"
    );

    // ── (4) corpus assertion: NO CALLS self-loop with a receiver other than "this" ──
    let bad_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\"%' \
               AND metadata_json NOT LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("self-loop corpus query");
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class (the RC-1 invariant)"
    );

    // Guard against a vacuous pass: the one legitimate self-loop (this->run → B::run) must exist,
    // proving the pipeline really produced and bound the CALLS edges.
    let legit_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("legit self-loop query");
    assert_eq!(
        legit_self_loops, 1,
        "the explicit this->run() self-call must be the one legitimate CALLS self-loop"
    );
}

/// F-CBR-009 / F-CBR-010 (source→store): the CONSERVATIVE shadowing rule. A local declared MORE
/// THAN ONCE in a function body is NEVER typed. In
///   `A* p = mkA(); { B* p = mkB(); p->run(); } p->run();`
/// `p` is declared twice (outer `A* p`, block-local `B* p`), so BOTH `p->run()` calls stay
/// untyped — indirect receivers with no `receiverType`, left unresolved and counted. The store must
/// therefore carry NO `g -> A::run` edge and NO `g -> B::run` edge from these calls, no
/// receiver-bearing self-loop, and two counted `unresolved_edges` rows for `p`.
///
/// This supersedes the cycle-7 scope-stack attempt (in-block=B, post-block=A). The reviewer proved
/// (F-CBR-010) that a lexical stack missed scoping nodes and could mistype; the conservative rule
/// refuses to type a re-declared name at all, which never fabricates a wrong edge.
const SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class B {
public:
    void run() { }
};

A* mkA();
B* mkB();

void g() {
    A* p = mkA();
    {
        B* p = mkB();
        p->run();
    }
    p->run();
}
"#;

#[test]
fn indexed_cpp_block_scope_shadowed_local_is_never_typed() {
    assert_shadowed_local_untyped(SHADOW_FIXTURE, "shadow.cpp", "cbr-shadow-block");
}

/// F-CBR-010 (source→store): the exact for-initializer shape the reviewer named. A `for`
/// initializer is a `declaration` node in tree-sitter-cpp, so
///   `A* p = mkA(); for (B* p = mkB(); cond(); ) { p->run(); } p->run();`
/// declares `p` twice. Under the conservative rule `p` is never typed: neither the in-loop nor the
/// post-loop `p->run()` binds, so no false `B::run` edge, no `A::run` edge from these calls, and no
/// receiver-bearing self-loop is stored — the class the cycle-7 `compound_statement`-only
/// save/restore missed is closed at the source→store level.
const FOR_SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class B {
public:
    void run() { }
};

A* mkA();
B* mkB();
bool cond();

void g() {
    A* p = mkA();
    for (B* p = mkB(); cond(); ) {
        p->run();
    }
    p->run();
}
"#;

#[test]
fn indexed_cpp_for_initializer_shadowed_local_is_never_typed() {
    assert_shadowed_local_untyped(FOR_SHADOW_FIXTURE, "for_shadow.cpp", "cbr-shadow-for");
}

/// Shared assertion for the conservative shadowing rule: index `fixture`, then prove that the
/// twice-declared `p` produced NO typed binding — g calls neither `A::run` nor `B::run` through
/// `p`, both `p->run()` calls are counted `unresolved_edges` rows, and there is no
/// receiver-bearing self-loop.
fn assert_shadowed_local_untyped(fixture: &str, file: &str, repo_uid: &str) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join(file), fixture).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, repo_uid, &ComposeOptions::default())
        .expect("indexing the shadowing C++ fixture must succeed");
    let snap = &result.snapshot_uid;

    let storage = StorageConnection::open(&db_path).unwrap();

    let a_run = method_stable_key(&storage, snap, "A::run");
    let b_run = method_stable_key(&storage, snap, "B::run");
    let g = method_stable_key(&storage, snap, "g");

    // ── (1) NO typed binding: g calls neither A::run nor B::run through the shadowed `p` ──
    let a_callers: Vec<String> = storage
        .find_direct_callers(snap, &a_run, &["CALLS"])
        .expect("callers(A::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !a_callers.contains(&"g".to_string()),
        "a shadowed local is never typed: g must NOT bind to A::run; callers(A::run) = {a_callers:?}"
    );
    let b_callers: Vec<String> = storage
        .find_direct_callers(snap, &b_run, &["CALLS"])
        .expect("callers(B::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !b_callers.contains(&"g".to_string()),
        "a shadowed local is never typed: g must NOT bind to B::run; callers(B::run) = {b_callers:?}"
    );

    // g's CALLS callees must include neither A::run nor B::run (only the free mkA/mkB may resolve).
    let g_callees: Vec<String> = storage
        .find_direct_callees(snap, &g, &["CALLS"])
        .expect("callees(g) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !g_callees.contains(&"A::run".to_string()) && !g_callees.contains(&"B::run".to_string()),
        "neither run() method is a resolved callee of g (both p->run() stay unresolved); \
         callees(g) = {g_callees:?}"
    );

    // ── (2) both p->run() calls are counted unresolved_edges rows ──
    let p_unresolved: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM unresolved_edges \
             WHERE snapshot_uid = '{snap}' AND target_key = 'run' \
               AND category LIKE 'calls_%' \
               AND metadata_json LIKE '%\"receiver\":\"p\"%'"
        ))
        .expect("unresolved p query");
    assert_eq!(
        p_unresolved, 2,
        "both p->run() calls (shadowed receiver, untyped) are counted unresolved_edges rows"
    );

    // ── (3) no receiver-bearing self-loop anywhere (free function; no explicit this here) ──
    let bad_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\"%' \
               AND metadata_json NOT LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("self-loop corpus query");
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class"
    );
}

/// F-CBR-011 (source→store): a data member `A* p` shadowed by a PARAMETER `C* p`. C++ lexical
/// lookup selects the parameter, so `p->run()` must bind to `C::run` (the parameter's own declared
/// type) and NEVER to `A::run` (the same-named member). Proves both halves the reviewer named: no
/// false `B -> A::run` edge, AND the correct typed edge `B::run -> C::run` when the evidence (the
/// parameter's declared type) exists.
const PARAM_SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class C {
public:
    void run() { }
};

class B {
public:
    A* p;
    void run(C* p) {
        p->run();
    }
};
"#;

#[test]
fn indexed_cpp_member_shadowed_by_parameter_binds_the_parameter_type() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("param_shadow.cpp"), PARAM_SHADOW_FIXTURE).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let result = index_path(
        &repo,
        &db_path,
        "cbr-param-shadow",
        &ComposeOptions::default(),
    )
    .expect("indexing the parameter-shadow C++ fixture must succeed");
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db_path).unwrap();

    let a_run = method_stable_key(&storage, snap, "A::run");
    let c_run = method_stable_key(&storage, snap, "C::run");
    let b_run = method_stable_key(&storage, snap, "B::run");

    // ── the correct typed edge: B::run -> C::run (p typed by the parameter's own type C) ──
    let c_callers: Vec<String> = storage
        .find_direct_callers(snap, &c_run, &["CALLS"])
        .expect("callers(C::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert_eq!(
        c_callers,
        vec!["B::run".to_string()],
        "the parameter `C* p` types the receiver: B::run binds C::run; callers(C::run) = {c_callers:?}"
    );

    // ── no false B -> A::run edge (the shadowed member `A* p` never types the parameter receiver) ──
    let a_callers: Vec<String> = storage
        .find_direct_callers(snap, &a_run, &["CALLS"])
        .expect("callers(A::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !a_callers.contains(&"B::run".to_string()),
        "the shadowed member `A* p` never types the parameter receiver (F-CBR-011); \
         callers(A::run) = {a_callers:?}"
    );

    // ── callees(B::run): C::run bound, A::run never ──
    let b_callees: Vec<String> = storage
        .find_direct_callees(snap, &b_run, &["CALLS"])
        .expect("callees(B::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        b_callees.contains(&"C::run".to_string()) && !b_callees.contains(&"A::run".to_string()),
        "B::run binds C::run (the parameter type), never A::run (the member); \
         callees(B::run) = {b_callees:?}"
    );

    // ── no receiver-bearing self-loop ──
    let bad_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\"%' \
               AND metadata_json NOT LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("self-loop corpus query");
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class"
    );
}

/// F-CBR-011 (source→store): a data member `A* p` shadowed by an UNTYPED local `auto p = make_c();`.
/// `auto` yields no extractable declared type, so `p` is locally bound but has no usable type — the
/// member-field fallback is forbidden, `p->run()` stays unresolved and counted, and no false
/// `B -> A::run` (nor `B -> C::run`) edge is stored, even though both `A::run` and `C::run` are
/// indexed.
const AUTO_SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class C {
public:
    void run() { }
};

C* make_c();

class B {
public:
    A* p;
    void run() {
        auto p = make_c();
        p->run();
    }
};
"#;

#[test]
fn indexed_cpp_member_shadowed_by_untyped_local_stays_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("auto_shadow.cpp"), AUTO_SHADOW_FIXTURE).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let result = index_path(
        &repo,
        &db_path,
        "cbr-auto-shadow",
        &ComposeOptions::default(),
    )
    .expect("indexing the untyped-local-shadow C++ fixture must succeed");
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db_path).unwrap();

    let a_run = method_stable_key(&storage, snap, "A::run");
    let c_run = method_stable_key(&storage, snap, "C::run");
    let b_run = method_stable_key(&storage, snap, "B::run");

    // ── no false edge from the shadowed member (A) nor from the initializer's type (C) ──
    for (label, key) in [("A::run", &a_run), ("C::run", &c_run)] {
        let callers: Vec<String> = storage
            .find_direct_callers(snap, key, &["CALLS"])
            .expect("callers query")
            .iter()
            .filter_map(|c| c.qualified_name.clone())
            .collect();
        assert!(
            !callers.contains(&"B::run".to_string()),
            "an untyped `auto p` is never typed — B::run must not bind {label}; callers({label}) = {callers:?}"
        );
    }
    let b_callees: Vec<String> = storage
        .find_direct_callees(snap, &b_run, &["CALLS"])
        .expect("callees(B::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !b_callees.contains(&"A::run".to_string()) && !b_callees.contains(&"C::run".to_string()),
        "neither run() method is a resolved callee of B::run (p->run() stays unresolved); \
         callees(B::run) = {b_callees:?}"
    );

    // ── p->run() is exactly one counted unresolved_edges row with a calls_* category ──
    let p_unresolved: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM unresolved_edges \
             WHERE snapshot_uid = '{snap}' AND target_key = 'run' \
               AND category LIKE 'calls_%' \
               AND metadata_json LIKE '%\"receiver\":\"p\"%'"
        ))
        .expect("unresolved p query");
    assert_eq!(
        p_unresolved, 1,
        "the untyped-local receiver `p` stays an indirect receiver with no type — one counted \
         unresolved_edges row"
    );

    // ── no receiver-bearing self-loop ──
    let bad_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\"%' \
               AND metadata_json NOT LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("self-loop corpus query");
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class"
    );
}

/// F-CBR-012 (source→store): a data member `A* p` shadowed by a STRUCTURED-BINDING name
/// (`auto [x, p] = make_pair_();`). tree-sitter models the binding's names as multiple `identifier`
/// children; the earlier enumerating collector read only the first and missed `p`, letting the
/// member-field fallback type `p->run()` as `A`. The grammar-independent rule forbids the fallback
/// for any locally-written name, so `p->run()` stays unresolved and counted — no false
/// `B -> A::run` edge, no receiver-bearing self-loop.
const STRUCTURED_BINDING_SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

struct Pair { A* first; A* second; };
Pair make_pair_();

class B {
public:
    A* p;
    void run() {
        auto [x, p] = make_pair_();
        p->run();
    }
};
"#;

#[test]
fn indexed_cpp_member_shadowed_by_structured_binding_stays_unresolved() {
    assert_member_shadow_untyped(
        STRUCTURED_BINDING_SHADOW_FIXTURE,
        "structured_binding_shadow.cpp",
        "cbr-shadow-structured",
    );
}

/// F-CBR-012 (source→store): a data member `A* p` shadowed by a `catch (C* p)` parameter. A
/// `catch_clause` owns a `parameter_list`, not a `declaration`; the earlier enumerating collector
/// missed it and let the member-field fallback type `p->run()` as the member's `A`. The
/// grammar-independent rule forbids the fallback, so `p->run()` stays unresolved and counted — no
/// false `B -> A::run` edge (and no `C::run` either: the extractor does not type catch parameters —
/// the honest per-file limit), and no receiver-bearing self-loop.
const CATCH_PARAM_SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class C {
public:
    void run() { }
};

void risky();

class B {
public:
    A* p;
    void run() {
        try {
            risky();
        } catch (C* p) {
            p->run();
        }
    }
};
"#;

#[test]
fn indexed_cpp_member_shadowed_by_catch_parameter_stays_unresolved() {
    assert_member_shadow_untyped(
        CATCH_PARAM_SHADOW_FIXTURE,
        "catch_param_shadow.cpp",
        "cbr-shadow-catch",
    );
}

/// F-CBR-013 (source→store): a data member `A* p` whose name is also bound by a LAMBDA in the same
/// function — here an init-capture `[p = make_c()]`. The forbidden-set scan (`scan_written`) used to
/// early-return at `lambda_expression`, so the capture's `p` was invisible and the member-field
/// fallback typed the OUTER `p->run()` as the member's `A` — a guessed edge the receiver's own
/// binding does not support. The corrected scan descends into lambdas: the capture `p` marks `p`
/// written-locally, the fallback is barred, and `p->run()` stays unresolved and counted — no false
/// `B -> A::run` edge, no receiver-bearing self-loop. The call walk still does NOT enter the lambda,
/// so the lambda body itself produces no CALLS edge (nothing inside it is emitted).
const LAMBDA_CAPTURE_SHADOW_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

int make_c();

class B {
public:
    A* p;
    void run() {
        auto f = [p = make_c()]() { return p; };
        p->run();
    }
};
"#;

#[test]
fn indexed_cpp_member_shadowed_by_lambda_capture_stays_unresolved() {
    assert_member_shadow_untyped(
        LAMBDA_CAPTURE_SHADOW_FIXTURE,
        "lambda_capture_shadow.cpp",
        "cbr-shadow-lambda",
    );
}

/// Shared assertion for the F-CBR-012 member-shadow cases: index `fixture`, then prove that the
/// member-named receiver `p` — shadowed by a locally-written binding the earlier collector missed —
/// produced NO typed binding. `B::run` must not bind `A::run` (the member's type) through `p`; the
/// single `p->run()` call is a counted `unresolved_edges` row; and there is no receiver-bearing
/// self-loop.
fn assert_member_shadow_untyped(fixture: &str, file: &str, repo_uid: &str) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join(file), fixture).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let result = index_path(&repo, &db_path, repo_uid, &ComposeOptions::default())
        .expect("indexing the member-shadow C++ fixture must succeed");
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db_path).unwrap();

    let a_run = method_stable_key(&storage, snap, "A::run");
    let b_run = method_stable_key(&storage, snap, "B::run");

    // ── the member-field fallback must NOT fabricate `B::run -> A::run` through the shadowed `p` ──
    let a_callers: Vec<String> = storage
        .find_direct_callers(snap, &a_run, &["CALLS"])
        .expect("callers(A::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !a_callers.contains(&"B::run".to_string()),
        "the shadowed member `A* p` never types a locally-written receiver (F-CBR-012); \
         callers(A::run) = {a_callers:?}"
    );

    let b_callees: Vec<String> = storage
        .find_direct_callees(snap, &b_run, &["CALLS"])
        .expect("callees(B::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !b_callees.contains(&"A::run".to_string()),
        "B::run must not bind A::run through the shadowed `p`; callees(B::run) = {b_callees:?}"
    );

    // ── p->run() is exactly one counted unresolved_edges row with a calls_* category ──
    let p_unresolved: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM unresolved_edges \
             WHERE snapshot_uid = '{snap}' AND target_key = 'run' \
               AND category LIKE 'calls_%' \
               AND metadata_json LIKE '%\"receiver\":\"p\"%'"
        ))
        .expect("unresolved p query");
    assert_eq!(
        p_unresolved, 1,
        "the locally-written receiver `p` stays an indirect receiver with no type — one counted \
         unresolved_edges row"
    );

    // ── no receiver-bearing self-loop ──
    let bad_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\"%' \
               AND metadata_json NOT LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("self-loop corpus query");
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class"
    );
}

/// F-CBR-014 (source→store): a data member `A* p` of class `B`, shadowed by a UNIQUE inner-block
/// local `C* p` (a THIRD class). Before the lexical-scope fix, the flat per-function
/// `local_var_types` map kept the inner `C` binding after its `{ }` block closed, so the POST-BLOCK
/// `p->run()` — which denotes the member `A* p` — was mistyped `C` and evidence-bound to `C::run`
/// (a typed edge from evidence out of scope). With the `LocalType { scope_id, decl_end }` scope
/// test, the two `p->run()` calls diverge:
///   in-block  `p->run()` → typed `C` (the inner local IS in lexical scope): binds `C::run`.
///   post-block `p->run()`→ NO receiverType (the inner local's scope ended; `p` is still locally
///                          bound, so the member fallback is barred too): an indirect receiver with
///                          no type, unresolved and counted — never `C::run` (no second edge from
///                          the post-block site), never `A::run` (the member), never a self-loop.
const OUT_OF_SCOPE_LOCAL_FIXTURE: &str = r#"
class A {
public:
    void run() { }
};

class C {
public:
    void run() { }
};

C* mkC();

class B {
public:
    A* p;
    void run() {
        {
            C* p = mkC();
            p->run();
        }
        p->run();
    }
};
"#;

#[test]
fn indexed_cpp_member_shadowed_by_out_of_scope_local_stays_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("out_of_scope_local.cpp"),
        OUT_OF_SCOPE_LOCAL_FIXTURE,
    )
    .unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let result = index_path(
        &repo,
        &db_path,
        "cbr-out-of-scope",
        &ComposeOptions::default(),
    )
    .expect("indexing the out-of-scope-local C++ fixture must succeed");
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db_path).unwrap();

    let a_run = method_stable_key(&storage, snap, "A::run");
    let c_run = method_stable_key(&storage, snap, "C::run");
    let b_run = method_stable_key(&storage, snap, "B::run");

    // ── (1) the IN-BLOCK p->run() binds C::run (the inner local is in lexical scope) ──
    let c_callers: Vec<String> = storage
        .find_direct_callers(snap, &c_run, &["CALLS"])
        .expect("callers(C::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        c_callers.contains(&"B::run".to_string()),
        "the in-block p->run() is typed C and binds C::run; callers(C::run) = {c_callers:?}"
    );
    let b_callees: Vec<String> = storage
        .find_direct_callees(snap, &b_run, &["CALLS"])
        .expect("callees(B::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        b_callees.contains(&"C::run".to_string()),
        "B::run's in-block call resolves to C::run; callees(B::run) = {b_callees:?}"
    );

    // ── (2) the member `A* p` is NEVER typed: no B::run -> A::run edge ──
    let a_callers: Vec<String> = storage
        .find_direct_callers(snap, &a_run, &["CALLS"])
        .expect("callers(A::run) query")
        .iter()
        .filter_map(|c| c.qualified_name.clone())
        .collect();
    assert!(
        !a_callers.contains(&"B::run".to_string()),
        "the out-of-scope member `A* p` never types the post-block receiver; \
         callers(A::run) = {a_callers:?}"
    );
    assert!(
        !b_callees.contains(&"A::run".to_string()),
        "B::run must not bind A::run through the post-block `p`; callees(B::run) = {b_callees:?}"
    );

    // ── (3) exactly ONE resolved B::run -> C::run CALLS edge: the post-block site added none ──
    let b_to_c: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges e \
             JOIN nodes s ON s.node_uid = e.source_node_uid AND s.snapshot_uid = e.snapshot_uid \
             JOIN nodes t ON t.node_uid = e.target_node_uid AND t.snapshot_uid = e.snapshot_uid \
             WHERE e.snapshot_uid = '{snap}' AND e.type = 'CALLS' \
               AND s.qualified_name = 'B::run' AND t.qualified_name = 'C::run'"
        ))
        .expect("B::run->C::run edge count query");
    assert_eq!(
        b_to_c, 1,
        "only the IN-BLOCK p->run() binds C::run; the out-of-scope post-block call adds no edge"
    );

    // ── (4) exactly ONE counted unresolved p->run() row: the post-block call ──
    let p_unresolved: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM unresolved_edges \
             WHERE snapshot_uid = '{snap}' AND target_key = 'run' \
               AND category LIKE 'calls_%' \
               AND metadata_json LIKE '%\"receiver\":\"p\"%'"
        ))
        .expect("unresolved p query");
    assert_eq!(
        p_unresolved, 1,
        "the post-block p->run() (member receiver, no type in scope) is one counted unresolved row"
    );

    // ── (5) no receiver-bearing self-loop ──
    let bad_self_loops: i64 = storage
        .query_scalar(&format!(
            "SELECT count(*) FROM edges \
             WHERE snapshot_uid = '{snap}' AND type = 'CALLS' \
               AND source_node_uid = target_node_uid \
               AND metadata_json LIKE '%\"receiver\"%' \
               AND metadata_json NOT LIKE '%\"receiver\":\"this\"%'"
        ))
        .expect("self-loop corpus query");
    assert_eq!(
        bad_self_loops, 0,
        "no indirect-receiver call binds to the caller's own class"
    );
}
