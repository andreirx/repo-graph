//! Explain use-case tests: symbol target.

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{
    run_explain, AgentFocusCandidate, AgentFocusKind, AgentSymbolContext, Budget, SignalCode,
    EXPLAIN_COMMAND,
};

fn seed_symbol_repo(fake: &mut FakeAgentStorage) {
    fake.seed_minimal_repo("r1", "my-repo", "snap1");

    // Symbol resolution by name.
    fake.symbol_name_results.insert(
        ("snap1".into(), "MyService".into()),
        vec![AgentFocusCandidate {
            stable_key: "r1:src/service.ts:MyService:SYMBOL".into(),
            kind: AgentFocusKind::Symbol,
            file: Some("src/service.ts".into()),
            line: None,
        }],
    );

    // Symbol context.
    fake.symbol_contexts.insert(
        ("snap1".into(), "r1:src/service.ts:MyService:SYMBOL".into()),
        AgentSymbolContext {
            file_path: Some("src/service.ts".into()),
            module_path: Some("src/core".into()),
            module_stable_key: Some("r1:src/core:MODULE".into()),
            name: "MyService".into(),
            qualified_name: Some("src/service.ts:MyService".into()),
            subtype: Some("CLASS".into()),
            line_start: Some(10),
        },
    );
}

#[test]
fn explain_symbol_has_identity_section() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();

    assert_eq!(result.command, EXPLAIN_COMMAND);
    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainIdentity),
        "must have EXPLAIN_IDENTITY, got: {:?}",
        codes
    );
}

#[test]
fn explain_symbol_has_trust_section() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();

    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainTrust),
        "must have EXPLAIN_TRUST, got: {:?}",
        codes
    );
}

#[test]
fn explain_symbol_no_file_only_sections() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();

    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    // Symbol should NOT have EXPLAIN_IMPORTS, EXPLAIN_SYMBOLS,
    // EXPLAIN_FILES (those are file/path only).
    assert!(
        !codes.contains(&SignalCode::ExplainImports),
        "symbol must not have EXPLAIN_IMPORTS"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainSymbols),
        "symbol must not have EXPLAIN_SYMBOLS"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainFiles),
        "symbol must not have EXPLAIN_FILES"
    );
}

#[test]
fn explain_symbol_no_match_returns_empty() {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");

    let result = run_explain(&fake, "r1", "nonexistent", Budget::Medium, TEST_NOW).unwrap();

    assert_eq!(result.command, EXPLAIN_COMMAND);
    assert!(!result.focus.resolved);
    assert!(result.signals.is_empty());
}

#[test]
fn explain_symbol_module_context_on_inherited_signals() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);

    // Seed a cycle involving the owning module.
    fake.cycles_involving_module.insert(
        ("snap1".into(), "src/core".into()),
        vec![repo_graph_agent::AgentCycle {
            length: 2,
            modules: vec!["src/core".into(), "src/adapters".into()],
            test_composition: None,
            type_only: None,
            walk: None,
        }],
    );

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();

    let cycle_signal = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainCycles)
        .expect("must have EXPLAIN_CYCLES");

    assert_eq!(
        cycle_signal.scope(),
        repo_graph_agent::SignalScope::ModuleContext,
        "inherited cycle signal must have ModuleContext scope"
    );
}

#[test]
fn explain_symbol_callers_truncated_when_exceeding_cap() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);

    // Seed 20 callers (cap for medium = 15).
    let callers: Vec<repo_graph_agent::AgentCallerRow> = (0..20)
        .map(|i| repo_graph_agent::AgentCallerRow {
            stable_key: format!("r1:src/c{}.ts:fn{}:SYMBOL", i, i),
            name: format!("fn{}", i),
            file: Some(format!("src/c{}.ts", i)),
            line: None,
            module_path: Some("src/callers".into()),
            module_stable_key: None,
        })
        .collect();
    fake.symbol_callers.insert(
        ("snap1".into(), "r1:src/service.ts:MyService:SYMBOL".into()),
        callers,
    );

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();

    let callers_signal = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainCallers)
        .expect("must have EXPLAIN_CALLERS");

    let json = serde_json::to_value(callers_signal.evidence()).unwrap();
    assert_eq!(json["count"], 20);
    assert_eq!(json["items"].as_array().unwrap().len(), 15);
    assert_eq!(json["items_truncated"], true);
    assert_eq!(json["items_omitted_count"], 5);
}

// ── CPP-DECLARATORS-1 §2.3 (2026-09-07): type-vs-constructor bare-name collapse ──

/// Seed a `{one CLASS + one CONSTRUCTOR of that class}` candidate set under one bare name.
fn seed_type_and_constructor(fake: &mut FakeAgentStorage) {
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    let class_key = "r1:src/w.cpp:Widget:SYMBOL";
    let ctor_key = "r1:src/w.cpp:Widget::Widget:SYMBOL";
    fake.symbol_name_results.insert(
        ("snap1".into(), "Widget".into()),
        vec![
            AgentFocusCandidate {
                stable_key: class_key.into(),
                kind: AgentFocusKind::Symbol,
                file: Some("src/w.cpp".into()),
                line: Some(5),
            },
            AgentFocusCandidate {
                stable_key: ctor_key.into(),
                kind: AgentFocusKind::Symbol,
                file: Some("src/w.cpp".into()),
                line: Some(9),
            },
        ],
    );
    fake.symbol_contexts.insert(
        ("snap1".into(), class_key.into()),
        AgentSymbolContext {
            file_path: Some("src/w.cpp".into()),
            module_path: Some("src".into()),
            module_stable_key: None,
            name: "Widget".into(),
            qualified_name: Some("Widget".into()),
            subtype: Some("CLASS".into()),
            line_start: Some(5),
        },
    );
    fake.symbol_contexts.insert(
        ("snap1".into(), ctor_key.into()),
        AgentSymbolContext {
            file_path: Some("src/w.cpp".into()),
            module_path: Some("src".into()),
            module_stable_key: None,
            name: "Widget".into(),
            qualified_name: Some("Widget::Widget".into()),
            subtype: Some("CONSTRUCTOR".into()),
            line_start: Some(9),
        },
    );
    // review-3 #4: the collapse only fires when the classified candidates are the WHOLE
    // definition universe. Here the two candidates ARE all of it (count == 2).
    fake.symbol_definition_counts
        .insert(("snap1".into(), "Widget".into()), 2);
}

#[test]
fn explain_bare_name_resolves_to_type_over_its_constructor() {
    // Operator ruling `cpp_decl_explain_constructor_collision`: a bare name whose candidates
    // are exactly one TYPE + constructor(s) of that type resolves to the TYPE, and the
    // constructor cursor is surfaced in `next` (never hidden).
    let mut fake = FakeAgentStorage::new();
    seed_type_and_constructor(&mut fake);

    let result = run_explain(&fake, "r1", "Widget", Budget::Medium, TEST_NOW).unwrap();

    // Resolved to the class (explain_symbol ran) — NOT an ambiguous focus.
    assert!(result.focus.resolved, "bare name must resolve to the type");
    let identity = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainIdentity)
        .expect("resolved type must have EXPLAIN_IDENTITY");
    let ev = serde_json::to_value(identity.evidence()).unwrap();
    assert_eq!(ev["stable_key"], "r1:src/w.cpp:Widget:SYMBOL");

    // The constructor cursor is surfaced in `next`, never hidden.
    assert_eq!(result.next.len(), 1, "one constructor cursor surfaced");
    let action = &result.next[0];
    assert_eq!(action.target.as_deref(), Some("Widget::Widget"));
    assert!(
        action.reason.contains("constructor"),
        "reason names the constructor, got {:?}",
        action.reason
    );
}

#[test]
fn explain_two_types_same_name_stay_ambiguous() {
    // Any set OTHER than {one type + its constructor(s)} stays honestly ambiguous — here two
    // distinct types share a name (no constructor-collapse heuristic invented).
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    let a = "r1:src/a.cpp:Thing:SYMBOL";
    let b = "r1:src/b.cpp:Thing:SYMBOL";
    fake.symbol_name_results.insert(
        ("snap1".into(), "Thing".into()),
        vec![
            AgentFocusCandidate {
                stable_key: a.into(),
                kind: AgentFocusKind::Symbol,
                file: Some("src/a.cpp".into()),
                line: Some(1),
            },
            AgentFocusCandidate {
                stable_key: b.into(),
                kind: AgentFocusKind::Symbol,
                file: Some("src/b.cpp".into()),
                line: Some(1),
            },
        ],
    );
    for (key, file) in [(a, "src/a.cpp"), (b, "src/b.cpp")] {
        fake.symbol_contexts.insert(
            ("snap1".into(), key.into()),
            AgentSymbolContext {
                file_path: Some(file.into()),
                module_path: Some("src".into()),
                module_stable_key: None,
                name: "Thing".into(),
                qualified_name: Some(format!("{file}:Thing")),
                subtype: Some("STRUCT".into()),
                line_start: Some(1),
            },
        );
    }

    let result = run_explain(&fake, "r1", "Thing", Budget::Medium, TEST_NOW).unwrap();
    assert!(
        !result.focus.resolved,
        "two same-named types must stay ambiguous"
    );
    assert!(
        result.next.is_empty(),
        "ambiguous result surfaces no constructor cursor"
    );
}

#[test]
fn explain_collapse_bails_when_lookup_window_truncated() {
    // review-3 #4: `resolve_symbol_name` caps at LIMIT 5. If an exact-name definition was hidden
    // by that cap, a `{one type + constructor}` WINDOW must NOT resolve to the type — the real set
    // could be ambiguous. The classified candidates (2) are fewer than the uncapped definition
    // count (3) ⇒ completeness unproven ⇒ stay ambiguous.
    let mut fake = FakeAgentStorage::new();
    seed_type_and_constructor(&mut fake);
    // An unseen 3rd exact-name definition exists beyond the window.
    fake.symbol_definition_counts
        .insert(("snap1".into(), "Widget".into()), 3);

    let result = run_explain(&fake, "r1", "Widget", Budget::Medium, TEST_NOW).unwrap();
    assert!(
        !result.focus.resolved,
        "a truncated lookup window must NOT collapse to the type"
    );
    assert!(
        result.next.is_empty(),
        "no constructor cursor surfaced when completeness is unproven"
    );
}

#[test]
fn explain_collapse_bails_without_qualified_name_proof() {
    // review-3 #4: membership must be PROVEN, not inferred. A constructor with no qualified name
    // cannot be shown to belong to the type by container match, so the set stays ambiguous rather
    // than collapsing on the shared-simple-name assumption.
    let mut fake = FakeAgentStorage::new();
    seed_type_and_constructor(&mut fake);
    // Strip the constructor's qualified name — membership can no longer be proven.
    let ctor_key = "r1:src/w.cpp:Widget::Widget:SYMBOL";
    fake.symbol_contexts.insert(
        ("snap1".into(), ctor_key.into()),
        AgentSymbolContext {
            file_path: Some("src/w.cpp".into()),
            module_path: Some("src".into()),
            module_stable_key: None,
            name: "Widget".into(),
            qualified_name: None,
            subtype: Some("CONSTRUCTOR".into()),
            line_start: Some(9),
        },
    );

    let result = run_explain(&fake, "r1", "Widget", Budget::Medium, TEST_NOW).unwrap();
    assert!(
        !result.focus.resolved,
        "unproven membership must stay ambiguous"
    );
    assert!(
        result.next.is_empty(),
        "no constructor cursor surfaced without qualified-name proof"
    );
}
