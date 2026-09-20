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

// ── SYMBOL-IDENTITY-1 §2.1 / §2.4 ─────────────────────────────────────────

#[test]
fn explain_no_match_is_not_high_confidence() {
    // SYMBOL-IDENTITY-1 §2.4 / STANDING HONESTY RULE 3: a MISS must never render "Confidence: high".
    // The root-cause audit §H-A flagged `explain DBImpl::Recover` printing "Confidence: high" beside
    // "unresolved: no_match" — a static literal nothing computed. It is now `Low`.
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");

    let result = run_explain(&fake, "r1", "nonexistent", Budget::Medium, TEST_NOW).unwrap();

    assert!(!result.focus.resolved, "the target does not resolve");
    assert_ne!(
        result.confidence,
        repo_graph_agent::Confidence::High,
        "a no_match must not claim high confidence"
    );
    assert_eq!(
        result.confidence,
        repo_graph_agent::Confidence::Low,
        "a miss states the honest floor `low`"
    );
}

#[test]
fn explain_ambiguous_symbol_is_not_high_confidence() {
    // SYMBOL-IDENTITY-1 §2.4: the ambiguous arm must not claim high confidence either — the
    // candidate list is a fact, but "which one you meant" is unresolved.
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    // Two same-name candidates → ambiguous. Seeded via the name results (the default shared
    // resolver maps len>1 → Ambiguous).
    fake.symbol_name_results.insert(
        ("snap1".into(), "dispatch".into()),
        vec![
            AgentFocusCandidate {
                stable_key: "r1:a.rs#A::dispatch:SYMBOL:METHOD".into(),
                kind: AgentFocusKind::Symbol,
                file: Some("a.rs".into()),
                line: Some(3),
            },
            AgentFocusCandidate {
                stable_key: "r1:b.rs#B::dispatch:SYMBOL:METHOD".into(),
                kind: AgentFocusKind::Symbol,
                file: Some("b.rs".into()),
                line: Some(7),
            },
        ],
    );

    let result = run_explain(&fake, "r1", "dispatch", Budget::Medium, TEST_NOW).unwrap();

    assert!(
        !result.focus.resolved,
        "two candidates → ambiguous, not resolved"
    );
    assert!(
        !result.focus.candidates.is_empty(),
        "ambiguity is LISTED with candidates, never rendered as not-found"
    );
    assert_ne!(
        result.confidence,
        repo_graph_agent::Confidence::High,
        "an ambiguous outcome must not claim high confidence"
    );
}

#[test]
fn explain_resolves_qualified_suffix_through_shared_resolver() {
    // SYMBOL-IDENTITY-1 §2.1 (ruling B): explain routes through `storage.resolve_symbol` (the shared
    // resolver), NOT `resolve_symbol_name`. Proof: the qualified suffix `DBImpl::Recover` is seeded
    // ONLY in the shared-resolver map, and `resolve_symbol_name` is force-failed. Explain still
    // resolves the symbol → it must have consulted `resolve_symbol`, and a `find`-printed qualified
    // name now resolves in `explain`.
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    let sk = "r1:db/db_impl.cc#DBImpl::Recover:SYMBOL:METHOD";
    fake.symbol_resolutions.insert(
        ("snap1".into(), "DBImpl::Recover".into()),
        repo_graph_agent::AgentSymbolResolution::Resolved(AgentFocusCandidate {
            stable_key: sk.into(),
            kind: AgentFocusKind::Symbol,
            file: Some("db/db_impl.cc".into()),
            line: Some(292),
        }),
    );
    fake.symbol_contexts.insert(
        ("snap1".into(), sk.into()),
        AgentSymbolContext {
            file_path: Some("db/db_impl.cc".into()),
            module_path: Some("db".into()),
            module_stable_key: Some("r1:db:MODULE".into()),
            name: "Recover".into(),
            qualified_name: Some("leveldb::DBImpl::Recover".into()),
            subtype: Some("METHOD".into()),
            line_start: Some(292),
        },
    );
    // Force the OLD resolver to fail: if explain still resolves, it did NOT use resolve_symbol_name.
    *fake.force_error_on.borrow_mut() = Some("resolve_symbol_name");

    let result = run_explain(&fake, "r1", "DBImpl::Recover", Budget::Medium, TEST_NOW).unwrap();

    assert!(
        result.focus.resolved,
        "the qualified suffix resolves through the shared resolver (not resolve_symbol_name)"
    );
    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainIdentity),
        "the resolved symbol emits EXPLAIN_IDENTITY: {codes:?}"
    );
}

// ── EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): type-focus members + referenced-by (ETS-C02) ──

use repo_graph_agent::{
    AgentFileImporter, AgentMemberEntry, ExplainMembersEvidence, ExplainReferencedByEvidence,
    SignalEvidence,
};

fn members_evidence(result: &repo_graph_agent::OrientResult) -> Option<ExplainMembersEvidence> {
    result.signals.iter().find_map(|s| match s.evidence() {
        SignalEvidence::ExplainMembers(e) => Some(e.clone()),
        _ => None,
    })
}

fn referenced_by_evidence(
    result: &repo_graph_agent::OrientResult,
) -> Option<ExplainReferencedByEvidence> {
    result.signals.iter().find_map(|s| match s.evidence() {
        SignalEvidence::ExplainReferencedBy(e) => Some(e.clone()),
        _ => None,
    })
}

#[test]
fn explain_type_focus_emits_members_and_referenced_by() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake); // MyService is a CLASS, qualified_name src/service.ts:MyService.

    fake.members_of_type.insert(
        ("snap1".into(), "src/service.ts:MyService".into()),
        vec![
            AgentMemberEntry {
                name: "start".into(),
                qualified_name: "src/service.ts:MyService::start".into(),
                subtype: Some("METHOD".into()),
                file: "src/service.ts".into(),
                line_start: Some(12),
                forward_decl: false,
            },
            AgentMemberEntry {
                name: "stop".into(),
                qualified_name: "src/service.ts:MyService::stop".into(),
                subtype: Some("METHOD".into()),
                file: "src/service.ts".into(),
                line_start: Some(20),
                forward_decl: false,
            },
        ],
    );
    fake.file_importers.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![AgentFileImporter {
            file: "src/main.ts".into(),
            module_path: Some("src".into()),
        }],
    );

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();

    let members = members_evidence(&result).expect("type focus emits EXPLAIN_MEMBERS");
    assert_eq!(members.count, 2, "the fake's two members");
    let names: Vec<&str> = members.items.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["start", "stop"]);

    let refs = referenced_by_evidence(&result).expect("type focus emits EXPLAIN_REFERENCED_BY");
    assert_eq!(refs.count, 1, "the one importing file");
    assert_eq!(refs.items[0].file, "src/main.ts");
    // top_modules grouped exactly like callers' group_by_module.
    assert_eq!(refs.top_modules.len(), 1);
    assert_eq!(refs.top_modules[0].module, "src");
    assert_eq!(refs.top_modules[0].count, 1);
}

#[test]
fn explain_function_focus_emits_no_members_or_referenced_by() {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    // A FUNCTION focus (not a type).
    fake.symbol_name_results.insert(
        ("snap1".into(), "doWork".into()),
        vec![AgentFocusCandidate {
            stable_key: "r1:src/w.ts:doWork:SYMBOL".into(),
            kind: AgentFocusKind::Symbol,
            file: Some("src/w.ts".into()),
            line: None,
        }],
    );
    fake.symbol_contexts.insert(
        ("snap1".into(), "r1:src/w.ts:doWork:SYMBOL".into()),
        AgentSymbolContext {
            file_path: Some("src/w.ts".into()),
            module_path: Some("src".into()),
            module_stable_key: Some("r1:src:MODULE".into()),
            name: "doWork".into(),
            qualified_name: Some("src/w.ts:doWork".into()),
            subtype: Some("FUNCTION".into()),
            line_start: Some(5),
        },
    );
    // Even if members/importers were (erroneously) seeded, a FUNCTION focus must not read them.
    fake.members_of_type.insert(
        ("snap1".into(), "src/w.ts:doWork".into()),
        vec![AgentMemberEntry {
            name: "ghost".into(),
            qualified_name: "src/w.ts:doWork::ghost".into(),
            subtype: Some("METHOD".into()),
            file: "src/w.ts".into(),
            line_start: Some(6),
            forward_decl: false,
        }],
    );

    let result = run_explain(&fake, "r1", "doWork", Budget::Medium, TEST_NOW).unwrap();
    assert!(
        members_evidence(&result).is_none(),
        "a FUNCTION focus emits NO EXPLAIN_MEMBERS"
    );
    assert!(
        referenced_by_evidence(&result).is_none(),
        "a FUNCTION focus emits NO EXPLAIN_REFERENCED_BY"
    );
    // The signal set is exactly today's function-focus set (identity, callers, callees, trust).
    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(codes.contains(&SignalCode::ExplainIdentity));
    assert!(codes.contains(&SignalCode::ExplainCallers));
    assert!(codes.contains(&SignalCode::ExplainCallees));
    assert!(!codes.contains(&SignalCode::ExplainMembers));
    assert!(!codes.contains(&SignalCode::ExplainReferencedBy));
}

#[test]
fn explain_type_focus_members_count_is_the_pre_truncation_total() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);

    let members: Vec<AgentMemberEntry> = (0..20)
        .map(|i| AgentMemberEntry {
            name: format!("m{i:02}"),
            qualified_name: format!("src/service.ts:MyService::m{i:02}"),
            subtype: Some("METHOD".into()),
            file: "src/service.ts".into(),
            line_start: Some(10 + i as u64),
            forward_decl: false,
        })
        .collect();
    fake.members_of_type
        .insert(("snap1".into(), "src/service.ts:MyService".into()), members);

    // Budget::Small floors to Medium (cap 15); 20 members → count 20, items 15, truncation set.
    let result = run_explain(&fake, "r1", "MyService", Budget::Small, TEST_NOW).unwrap();
    let ev = members_evidence(&result).expect("EXPLAIN_MEMBERS present");
    assert_eq!(ev.count, 20, "count is the PRE-truncation total");
    assert_eq!(ev.items.len(), 15, "items capped by items_cap(Medium)");
    assert_eq!(ev.items_truncated, Some(true));
    assert_eq!(ev.items_omitted_count, Some(5));
}
