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

    // The constructor cursor is surfaced in `next`, never hidden. JAVA-SYMBOL-AMBIGUITY-HINT-1
    // (D-JSAH-CURSOR-1): the cursor is the constructor's STABLE KEY, which names exactly one
    // constructor (the qualified name `Widget::Widget` is shared by every constructor).
    assert_eq!(result.next.len(), 1, "one constructor cursor surfaced");
    let action = &result.next[0];
    assert_eq!(
        action.target.as_deref(),
        Some("r1:src/w.cpp:Widget::Widget:SYMBOL")
    );
    assert!(
        action.reason.contains("constructor"),
        "reason names the constructor, got {:?}",
        action.reason
    );
}

#[test]
fn explain_bare_java_type_resolves_to_type_over_dotted_init_constructors() {
    // JAVA-SYMBOL-AMBIGUITY-HINT-1 (RG-REQ-005-L04 Java clause): the Java extractor stores a
    // constructor's qualified name as `<pkg>.<Type>.<init>`. Its container (the qualified name
    // minus the segment after the LAST `::` or `.`) is the type's qualified name, so a bare type
    // name over the type and its constructors resolves to the TYPE, as on C++, and each
    // constructor is surfaced by its stable key.
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    let class_key = "r1:src/org/example/Widget.java#Widget:SYMBOL:CLASS";
    let ctor1 = "r1:src/org/example/Widget.java#Widget:SYMBOL:CONSTRUCTOR";
    let ctor2 = "r1:src/org/example/Widget.java#Widget:SYMBOL:CONSTRUCTOR:dup2";
    let cand = |key: &str, line: u64| AgentFocusCandidate {
        stable_key: key.into(),
        kind: AgentFocusKind::Symbol,
        file: Some("src/org/example/Widget.java".into()),
        line: Some(line),
    };
    fake.symbol_name_results.insert(
        ("snap1".into(), "Widget".into()),
        vec![cand(class_key, 5), cand(ctor1, 9), cand(ctor2, 14)],
    );
    for (key, qn, subtype, line) in [
        (class_key, "org.example.Widget", "CLASS", 5),
        (ctor1, "org.example.Widget.<init>", "CONSTRUCTOR", 9),
        (ctor2, "org.example.Widget.<init>", "CONSTRUCTOR", 14),
    ] {
        fake.symbol_contexts.insert(
            ("snap1".into(), key.into()),
            AgentSymbolContext {
                file_path: Some("src/org/example/Widget.java".into()),
                module_path: Some("src/org/example".into()),
                module_stable_key: None,
                name: "Widget".into(),
                qualified_name: Some(qn.into()),
                subtype: Some(subtype.into()),
                line_start: Some(line),
            },
        );
    }
    fake.symbol_definition_counts
        .insert(("snap1".into(), "Widget".into()), 3);

    let result = run_explain(&fake, "r1", "Widget", Budget::Medium, TEST_NOW).unwrap();

    assert!(
        result.focus.resolved,
        "a bare Java type name over its `.<init>` constructors resolves to the type"
    );
    let identity = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainIdentity)
        .expect("resolved type must have EXPLAIN_IDENTITY");
    let ev = serde_json::to_value(identity.evidence()).unwrap();
    assert_eq!(ev["stable_key"], class_key);
    let targets: Vec<Option<&str>> = result.next.iter().map(|a| a.target.as_deref()).collect();
    assert_eq!(targets, vec![Some(ctor1), Some(ctor2)]);
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
                undetermined_identity: None,
            },
            AgentMemberEntry {
                name: "stop".into(),
                qualified_name: "src/service.ts:MyService::stop".into(),
                subtype: Some("METHOD".into()),
                file: "src/service.ts".into(),
                line_start: Some(20),
                forward_decl: false,
                undetermined_identity: None,
            },
        ],
    );
    fake.file_importers.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![AgentFileImporter {
            file: "src/main.ts".into(),
            module_path: Some("src".into()),
            resolution: "static".into(),
            basis: None,
            alternate_path: None,
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

// ── PYTHON-SUBMODULE-IMPORT-1 (RG-REQ-002-L11): Referenced-by partitions inferred importers ──

fn importer(
    file: &str,
    module: &str,
    resolution: &str,
    alternate: Option<&str>,
) -> AgentFileImporter {
    let inferred = resolution == "inferred";
    AgentFileImporter {
        file: file.into(),
        module_path: Some(module.into()),
        resolution: resolution.into(),
        basis: inferred.then(|| "python_submodule".to_string()),
        alternate_path: alternate.map(|a| a.to_string()),
    }
}

#[test]
fn explain_referenced_by_partitions_inferred_importers_with_their_alternate() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    // The django `BaseHandler` shape: one certain importer, two inferred ones naming the init.
    fake.file_importers.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![
            importer("src/asgi.py", "src", "inferred", Some("src/__init__.py")),
            importer("src/client.py", "test", "static", None),
            importer("src/wsgi.py", "src", "inferred", Some("src/__init__.py")),
        ],
    );

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let refs = referenced_by_evidence(&result).expect("type focus emits EXPLAIN_REFERENCED_BY");

    // Certain facts only in count / items / top_modules.
    assert_eq!(refs.count, 1);
    assert_eq!(refs.items.len(), 1);
    assert_eq!(refs.items[0].file, "src/client.py");
    assert_eq!(refs.top_modules.len(), 1);
    assert_eq!(refs.top_modules[0].module, "test");
    assert_eq!(refs.top_modules[0].count, 1);

    // The inferred remainder, with the reason and the other candidate.
    assert_eq!(refs.inferred_count, 2);
    let inferred: Vec<(&str, &str, Option<&str>)> = refs
        .inferred_items
        .iter()
        .map(|i| (i.file.as_str(), i.basis.as_str(), i.alternate.as_deref()))
        .collect();
    assert_eq!(
        inferred,
        vec![
            ("src/asgi.py", "python_submodule", Some("src/__init__.py")),
            ("src/wsgi.py", "python_submodule", Some("src/__init__.py")),
        ]
    );
    assert_eq!(refs.inferred_items_omitted_count, None);

    // JSON consumers see the partition.
    let json = serde_json::to_value(&refs).unwrap();
    assert_eq!(json["inferred_count"], 2);
    assert_eq!(json["inferred_items"][0]["alternate"], "src/__init__.py");
    assert_eq!(json["inferred_items"][0]["basis"], "python_submodule");
}

#[test]
fn explain_referenced_by_counts_a_file_with_any_certain_edge_as_certain_only() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    // mixed.py imports the file through one inferred and one static edge (and one dynamic);
    // plain.py through a static edge only.
    fake.file_importers.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![
            importer("src/mixed.py", "src", "dynamic", None),
            importer("src/mixed.py", "src", "inferred", Some("src/__init__.py")),
            importer("src/mixed.py", "src", "static", None),
            importer("src/plain.py", "src", "static", None),
        ],
    );

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let refs = referenced_by_evidence(&result).expect("type focus emits EXPLAIN_REFERENCED_BY");
    assert_eq!(refs.count, 2, "each certain file counted once");
    let files: Vec<&str> = refs.items.iter().map(|i| i.file.as_str()).collect();
    assert_eq!(files, vec!["src/mixed.py", "src/plain.py"]);
    assert_eq!(refs.top_modules[0].count, 2);
    assert_eq!(
        refs.inferred_count, 0,
        "a file with any certain edge is never inferred"
    );
    assert!(refs.inferred_items.is_empty());

    // With no inferred importer the JSON carries none of the inferred keys (byte-identical shape).
    let json = serde_json::to_value(&refs).unwrap();
    let keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert!(
        !keys.iter().any(|k| k.starts_with("inferred")),
        "no inferred key when there is no inferred importer: {keys:?}"
    );
}

#[test]
fn explain_referenced_by_fails_on_an_unknown_resolution() {
    // F-PSI-02: an importer row whose resolution is outside static | dynamic | inferred is a broken
    // read — the explain fails loudly, never counting the file as a certain importer.
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    fake.file_importers.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![
            importer("src/a.py", "src", "static", None),
            importer("src/b.py", "src", "resolved", None),
        ],
    );
    let err = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW)
        .expect_err("an unknown resolution is not a certain importer");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("src/b.py") && msg.contains("resolved"),
        "{msg}"
    );
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
            undetermined_identity: None,
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
            undetermined_identity: None,
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

// ── CPP-ATTRIBUTE-MACRO-1A (RG-REQ-002-L11): the undetermined-identity marker reaches explain ──

#[test]
fn explain_members_carry_the_undetermined_identity_marker_verbatim() {
    use repo_graph_agent::storage_port::AgentUndeterminedIdentity;

    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    fake.members_of_type.insert(
        ("snap1".into(), "src/service.ts:MyService".into()),
        vec![
            AgentMemberEntry {
                name: "IncrementBy".into(),
                qualified_name: "src/service.ts:MyService::IncrementBy".into(),
                subtype: Some("METHOD".into()),
                file: "db/db_test.cc".into(),
                line_start: Some(47),
                forward_decl: true,
                undetermined_identity: None,
            },
            AgentMemberEntry {
                name: "LOCKS_EXCLUDED".into(),
                qualified_name: "src/service.ts:MyService::LOCKS_EXCLUDED".into(),
                subtype: Some("METHOD".into()),
                file: "db/db_test.cc".into(),
                line_start: Some(47),
                forward_decl: false,
                undetermined_identity: Some(AgentUndeterminedIdentity {
                    candidates: ["IncrementBy".into(), "LOCKS_EXCLUDED".into()],
                    basis: "macro_recovery_ambiguous_identity".into(),
                }),
            },
        ],
    );

    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let members = members_evidence(&result).expect("type focus emits EXPLAIN_MEMBERS");
    assert_eq!(members.count, 2);

    // The wire shape: the marker's three flat keys on the marked member only; a determined
    // member's JSON carries none of them (byte-identical to before this slice).
    let json = serde_json::to_value(&members).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(
        items[0],
        serde_json::json!({
            "name": "IncrementBy", "subtype": "METHOD", "file": "db/db_test.cc",
            "line": 47, "forward_decl": true
        }),
        "a determined member serializes exactly as before"
    );
    assert_eq!(
        items[1],
        serde_json::json!({
            "name": "LOCKS_EXCLUDED", "subtype": "METHOD", "file": "db/db_test.cc",
            "line": 47, "forward_decl": false,
            "identity": "undetermined",
            "identity_candidates": ["IncrementBy", "LOCKS_EXCLUDED"],
            "identity_basis": "macro_recovery_ambiguous_identity"
        }),
        "the marker is carried verbatim, candidates in stored order"
    );
    let as_text = serde_json::to_string(&members.items[0]).unwrap();
    assert_eq!(
        as_text,
        r#"{"name":"IncrementBy","subtype":"METHOD","file":"db/db_test.cc","line":47,"forward_decl":true}"#,
        "key order and bytes of a determined member unchanged"
    );
}

// ── PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11, RG-REQ-005-L09): the call remainders ──

const SERVICE_KEY: &str = "r1:src/service.ts:MyService:SYMBOL";

fn inferred_row(key: &str, name: &str, line: u64) -> repo_graph_agent::AgentInferredCallRow {
    repo_graph_agent::AgentInferredCallRow {
        stable_key: key.into(),
        name: name.into(),
        qualified_name: Some(name.into()),
        file: Some("src/other.py".into()),
        line: Some(line),
        module_path: Some("src".into()),
        module_stable_key: None,
        basis: "receiver_untyped_name_only".into(),
        receiver: Some("dependencies".into()),
        extractor: "python-core:0.2.0".into(),
    }
}

fn basis(code: &str, count: u64) -> repo_graph_agent::AgentBasisCount {
    repo_graph_agent::AgentBasisCount {
        basis_code: code.into(),
        count,
    }
}

fn signal_json(result: &repo_graph_agent::OrientResult, code: SignalCode) -> serde_json::Value {
    let sig = result
        .signals
        .iter()
        .find(|s| s.code() == code)
        .unwrap_or_else(|| panic!("must have {code:?}"));
    serde_json::to_value(sig.evidence()).unwrap()
}

/// One certain caller, one inferred caller and one unresolved same-name call.
fn seed_remainders(fake: &mut FakeAgentStorage) {
    fake.symbol_callers.insert(
        ("snap1".into(), SERVICE_KEY.into()),
        vec![repo_graph_agent::AgentCallerRow {
            stable_key: "r1:src/a.ts:certain:SYMBOL".into(),
            name: "certain".into(),
            file: Some("src/a.ts".into()),
            line: Some(4),
            module_path: Some("src".into()),
            module_stable_key: None,
        }],
    );
    fake.symbol_call_remainders.insert(
        ("snap1".into(), SERVICE_KEY.into()),
        repo_graph_agent::AgentCallRemainders {
            inferred_callers: vec![inferred_row("r1:src/other.py#guess:SYMBOL", "guess", 1737)],
            inferred_callees: vec![inferred_row("r1:src/other.py#ext:SYMBOL", "ext", 12)],
            unresolved_naming: vec![basis("self_call_hierarchy_miss", 1)],
            unresolved_naming_sites: Vec::new(),
            unresolved_from: vec![
                basis("no_supporting_signal", 2),
                basis("self_call_hierarchy_miss", 1),
            ],
            unresolved_from_sites: Vec::new(),
        },
    );
}

#[test]
fn explain_callers_state_the_inferred_remainder_and_the_unresolved_calls_naming_the_symbol() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    seed_remainders(&mut fake);
    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let json = signal_json(&result, SignalCode::ExplainCallers);
    assert_eq!(json["count"], 1, "certain rows only");
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    assert_eq!(json["inferred_count"], 1);
    let inf = &json["inferred_items"][0];
    assert_eq!(inf["name"], "guess");
    assert_eq!(inf["line"], 1737, "the call site");
    assert_eq!(inf["basis"], "receiver_untyped_name_only");
    assert_eq!(
        json["unresolved_naming"],
        serde_json::json!({"name": "MyService", "count": 1, "by_basis": {"self_call_hierarchy_miss": 1}})
    );
}

#[test]
fn explain_callees_state_the_inferred_remainder_and_the_unresolved_calls_from_the_symbol() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    seed_remainders(&mut fake);
    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let json = signal_json(&result, SignalCode::ExplainCallees);
    assert_eq!(json["count"], 0);
    assert_eq!(json["inferred_count"], 1);
    assert_eq!(json["inferred_items"][0]["name"], "ext");
    assert_eq!(
        json["unresolved_from"],
        serde_json::json!({"count": 3, "by_basis": {"no_supporting_signal": 2, "self_call_hierarchy_miss": 1}})
    );
}

#[test]
fn explain_zero_certain_callers_state_the_unresolved_calls_naming_the_symbol() {
    // RG-REQ-005-L09: zero certain callers, two unresolved calls naming the symbol — the zero
    // never reads as absence.
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    fake.symbol_call_remainders.insert(
        ("snap1".into(), SERVICE_KEY.into()),
        repo_graph_agent::AgentCallRemainders {
            unresolved_naming: vec![
                basis("no_supporting_signal", 1),
                basis("self_call_ambiguous_mro", 1),
            ],
            ..Default::default()
        },
    );
    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let json = signal_json(&result, SignalCode::ExplainCallers);
    assert_eq!(json["count"], 0);
    assert!(
        json.get("inferred_count").is_none(),
        "no inferred rows → no field"
    );
    assert_eq!(json["unresolved_naming"]["count"], 2);
    assert_eq!(
        json["unresolved_naming"]["by_basis"],
        serde_json::json!({"no_supporting_signal": 1, "self_call_ambiguous_mro": 1})
    );
    // An explain with no remainder stays byte-identical: no remainder fields at all.
    let mut plain = FakeAgentStorage::new();
    seed_symbol_repo(&mut plain);
    let r = run_explain(&plain, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let j = signal_json(&r, SignalCode::ExplainCallers);
    assert!(j.get("unresolved_naming").is_none() && j.get("inferred_items").is_none());
}

/// TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): explain's Import-cycles block renders when an excluded
/// cycle involves the focus module even though the default view has none, and names only the
/// excluded cycles involving it.
#[test]
fn explain_import_cycles_block_names_excluded_cycles_involving_the_focus() {
    let mut fake = FakeAgentStorage::new();
    seed_symbol_repo(&mut fake);
    let cyc = |members: &[&str]| repo_graph_agent::AgentExcludedCycle {
        members: members.iter().map(|s| s.to_string()).collect(),
        flags: vec!["include_tests".to_string()],
        contains_shown: Vec::new(),
        partitions: Default::default(),
    };
    fake.import_cycle_partitions.insert(
        "snap1".into(),
        repo_graph_agent::AgentImportCyclePartition {
            view: repo_graph_classification::import_partition::ImportView::DEFAULT,
            remainder: Default::default(),
            excluded_cycles: vec![cyc(&["src/core", "src/db"]), cyc(&["lib/a", "lib/b"])],
            cycle_partitions: Vec::new(),
            importers: vec![repo_graph_agent::AgentImporterFile {
                path: "src/core/c_test.c".into(),
                is_test: Some(false),
            }],
        },
    );
    let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW).unwrap();
    let sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainCycles)
        .expect("EXPLAIN_CYCLES renders for an excluded cycle involving the focus");
    let ev = serde_json::to_value(sig).unwrap()["evidence"].clone();
    assert_eq!(ev["count"], 0);
    assert_eq!(ev["excluded_cycles"].as_array().unwrap().len(), 1);
    assert_eq!(
        ev["excluded_cycles"][0]["members"],
        serde_json::json!(["src/core", "src/db"])
    );
    assert_eq!(ev["importer_test_status_undetermined"]["count"], 1);
    assert_eq!(
        ev["importer_test_status_undetermined"]["universe"],
        "cross_directory_importers"
    );
}
