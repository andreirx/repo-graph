//! Symbol-scoped signal emission tests.
//!
//! Tests that the symbol pipeline emits the correct signals with
//! the correct evidence and scope annotations.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
    orient, AgentBoundaryDeclaration, AgentCalleeRow, AgentCallerRow, AgentCycle,
    AgentFocusCandidate, AgentFocusKind, AgentImportEdge, AgentSymbolContext, Budget, LimitCode,
    SignalCode, SignalScope,
};

fn seeded_symbol() -> FakeAgentStorage {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap-1");

    // Default: path resolution returns nothing.
    // Default: stable-key lookup returns nothing.
    // Symbol name resolution returns 1 result.
    let sk = "r1:src/core/service.ts:SYMBOL:doWork";
    fake.symbol_name_results.insert(
        ("snap-1".into(), "doWork".into()),
        vec![AgentFocusCandidate {
            stable_key: sk.into(),
            kind: AgentFocusKind::Symbol,
            file: Some("src/core/service.ts".into()),
            line: None,
        }],
    );
    fake.symbol_contexts.insert(
        ("snap-1".into(), sk.into()),
        AgentSymbolContext {
            file_path: Some("src/core/service.ts".into()),
            module_path: Some("src/core".into()),
            module_stable_key: Some("r1:src/core:MODULE".into()),
            name: "doWork".into(),
            qualified_name: Some("doWork".into()),
            subtype: Some("function".into()),
            line_start: Some(10),
        },
    );
    fake
}

fn seeded_symbol_no_module() -> FakeAgentStorage {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap-1");

    let sk = "r1:src/standalone.ts:SYMBOL:lonely";
    fake.symbol_name_results.insert(
        ("snap-1".into(), "lonely".into()),
        vec![AgentFocusCandidate {
            stable_key: sk.into(),
            kind: AgentFocusKind::Symbol,
            file: Some("src/standalone.ts".into()),
            line: None,
        }],
    );
    fake.symbol_contexts.insert(
        ("snap-1".into(), sk.into()),
        AgentSymbolContext {
            file_path: Some("src/standalone.ts".into()),
            module_path: None,
            module_stable_key: None,
            name: "lonely".into(),
            qualified_name: Some("lonely".into()),
            subtype: None,
            line_start: Some(1),
        },
    );
    fake
}

// ── 5. CALLERS_SUMMARY groups by module ────────────────────────

#[test]
fn callers_summary_groups_by_module() {
    let mut fake = seeded_symbol();
    let sk = "r1:src/core/service.ts:SYMBOL:doWork";

    fake.symbol_callers.insert(
        ("snap-1".into(), sk.into()),
        vec![
            AgentCallerRow {
                stable_key: "r1:src/cli/run.ts:SYMBOL:main".into(),
                name: "main".into(),
                file: Some("src/cli/run.ts".into()),
                line: None,
                module_path: Some("src/cli".into()),
                module_stable_key: Some("r1:src/cli:MODULE".into()),
            },
            AgentCallerRow {
                stable_key: "r1:src/cli/setup.ts:SYMBOL:setup".into(),
                name: "setup".into(),
                file: Some("src/cli/setup.ts".into()),
                line: None,
                module_path: Some("src/cli".into()),
                module_stable_key: Some("r1:src/cli:MODULE".into()),
            },
            AgentCallerRow {
                stable_key: "r1:src/core/handler.ts:SYMBOL:handle".into(),
                name: "handle".into(),
                file: Some("src/core/handler.ts".into()),
                line: None,
                module_path: Some("src/core".into()),
                module_stable_key: Some("r1:src/core:MODULE".into()),
            },
        ],
    );

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let callers_sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::CallersSummary)
        .expect("CALLERS_SUMMARY signal must be emitted");

    match callers_sig.evidence() {
        repo_graph_agent::SignalEvidence::CallersSummary(ev) => {
            assert_eq!(ev.count, 3);
            assert_eq!(ev.top_modules.len(), 2);
            // src/cli has 2 callers, should be first.
            assert_eq!(ev.top_modules[0].module, "src/cli");
            assert_eq!(ev.top_modules[0].count, 2);
            assert_eq!(ev.top_modules[1].module, "src/core");
            assert_eq!(ev.top_modules[1].count, 1);
        }
        other => panic!("expected CallersSummary evidence, got: {:?}", other),
    }
}

// ── 6. CALLEES_SUMMARY groups by module ────────────────────────

#[test]
fn callees_summary_groups_by_module() {
    let mut fake = seeded_symbol();
    let sk = "r1:src/core/service.ts:SYMBOL:doWork";

    fake.symbol_callees.insert(
        ("snap-1".into(), sk.into()),
        vec![
            AgentCalleeRow {
                stable_key: "r1:src/adapters/db.ts:SYMBOL:query".into(),
                name: "query".into(),
                file: Some("src/adapters/db.ts".into()),
                line: None,
                module_path: Some("src/adapters".into()),
                module_stable_key: Some("r1:src/adapters:MODULE".into()),
            },
            AgentCalleeRow {
                stable_key: "r1:src/adapters/cache.ts:SYMBOL:get".into(),
                name: "get".into(),
                file: Some("src/adapters/cache.ts".into()),
                line: None,
                module_path: Some("src/adapters".into()),
                module_stable_key: Some("r1:src/adapters:MODULE".into()),
            },
            AgentCalleeRow {
                stable_key: "r1:src/core/utils.ts:SYMBOL:validate".into(),
                name: "validate".into(),
                file: Some("src/core/utils.ts".into()),
                line: None,
                module_path: Some("src/core".into()),
                module_stable_key: Some("r1:src/core:MODULE".into()),
            },
        ],
    );

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let callees_sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::CalleesSummary)
        .expect("CALLEES_SUMMARY signal must be emitted");

    match callees_sig.evidence() {
        repo_graph_agent::SignalEvidence::CalleesSummary(ev) => {
            assert_eq!(ev.count, 3);
            assert_eq!(ev.top_modules.len(), 2);
            assert_eq!(ev.top_modules[0].module, "src/adapters");
            assert_eq!(ev.top_modules[0].count, 2);
            assert_eq!(ev.top_modules[1].module, "src/core");
            assert_eq!(ev.top_modules[1].count, 1);
        }
        other => panic!("expected CalleesSummary evidence, got: {:?}", other),
    }
}

// ── 7. Callers with unknown module grouped as "(unknown)" ──────

#[test]
fn callers_with_unknown_module_grouped_as_unknown() {
    let mut fake = seeded_symbol();
    let sk = "r1:src/core/service.ts:SYMBOL:doWork";

    fake.symbol_callers.insert(
        ("snap-1".into(), sk.into()),
        vec![
            AgentCallerRow {
                stable_key: "r1:src/orphan.ts:SYMBOL:orphanFn".into(),
                name: "orphanFn".into(),
                file: Some("src/orphan.ts".into()),
                line: None,
                module_path: None,
                module_stable_key: None,
            },
            AgentCallerRow {
                stable_key: "r1:src/another.ts:SYMBOL:anotherFn".into(),
                name: "anotherFn".into(),
                file: Some("src/another.ts".into()),
                line: None,
                module_path: None,
                module_stable_key: None,
            },
        ],
    );

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let callers_sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::CallersSummary)
        .expect("CALLERS_SUMMARY must be emitted");

    match callers_sig.evidence() {
        repo_graph_agent::SignalEvidence::CallersSummary(ev) => {
            assert_eq!(ev.count, 2);
            assert_eq!(ev.top_modules.len(), 1);
            assert_eq!(
                ev.top_modules[0].module, "(unknown)",
                "callers without module must be grouped as (unknown)"
            );
            assert_eq!(ev.top_modules[0].count, 2);
        }
        other => panic!("expected CallersSummary, got: {:?}", other),
    }
}

// ── 8. Inherited boundary violations have module_context scope ──

#[test]
fn inherited_boundary_violations_have_module_context_scope() {
    let mut fake = seeded_symbol();

    // Seed boundary declaration for the owning module.
    fake.boundary_declarations.insert(
        "r1".into(),
        vec![AgentBoundaryDeclaration {
            source_module: "src/core".into(),
            forbidden_target: "src/adapters".into(),
            reason: Some("clean arch".into()),
        }],
    );
    // Seed violating edges.
    fake.imports_between_paths.insert(
        ("snap-1".into(), "src/core".into(), "src/adapters".into()),
        vec![AgentImportEdge {
            source_file: "src/core/service.ts".into(),
            target_file: "src/adapters/db.ts".into(),
        }],
    );

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let boundary_sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::BoundaryViolations)
        .expect("BOUNDARY_VIOLATIONS must be emitted");

    assert_eq!(
        boundary_sig.scope(),
        SignalScope::ModuleContext,
        "boundary violations at symbol scope must have ModuleContext scope"
    );
}

// ── 11. Inherited import cycles have module_context scope ──────

#[test]
fn inherited_import_cycles_have_module_context_scope() {
    let mut fake = seeded_symbol();

    // Seed cycle involving the owning module (exact match).
    fake.cycles_involving_module.insert(
        ("snap-1".into(), "src/core".into()),
        vec![AgentCycle {
            length: 2,
            modules: vec!["src/core".into(), "src/adapters".into()],
            test_composition: None,
            type_only: None,
            walk: None,
        }],
    );

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let cycle_sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ImportCycles)
        .expect("IMPORT_CYCLES must be emitted");

    assert_eq!(
        cycle_sig.scope(),
        SignalScope::ModuleContext,
        "import cycles at symbol scope must have ModuleContext scope"
    );
}

// ── 12. Direct callers_summary has no scope field in JSON ──────

#[test]
fn direct_callers_summary_has_no_scope_field_in_json() {
    let mut fake = seeded_symbol();
    let sk = "r1:src/core/service.ts:SYMBOL:doWork";

    fake.symbol_callers.insert(
        ("snap-1".into(), sk.into()),
        vec![AgentCallerRow {
            stable_key: "r1:src/cli/main.ts:SYMBOL:run".into(),
            name: "run".into(),
            file: Some("src/cli/main.ts".into()),
            line: None,
            module_path: Some("src/cli".into()),
            module_stable_key: Some("r1:src/cli:MODULE".into()),
        }],
    );

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let callers_sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::CallersSummary)
        .expect("CALLERS_SUMMARY must exist");

    // Serialize to JSON and verify "scope" is absent.
    let json = serde_json::to_value(callers_sig).unwrap();
    assert!(
        json.get("scope").is_none(),
        "Direct (default) scope must NOT appear in JSON: {:?}",
        json
    );
}

// ── 13. No inherited signals when module context is missing ────

#[test]
fn no_inherited_signals_when_module_context_missing() {
    let mut fake = seeded_symbol_no_module();

    // Seed boundary declarations and cycles that WOULD fire if
    // the symbol had a module context.
    fake.boundary_declarations.insert(
        "r1".into(),
        vec![AgentBoundaryDeclaration {
            source_module: "src/standalone".into(),
            forbidden_target: "src/adapters".into(),
            reason: None,
        }],
    );
    fake.cycles_involving_module.insert(
        ("snap-1".into(), "src/standalone".into()),
        vec![AgentCycle {
            length: 2,
            modules: vec!["src/standalone".into(), "src/other".into()],
            test_composition: None,
            type_only: None,
            walk: None,
        }],
    );

    let result = orient(&fake, "r1", Some("lonely"), Budget::Large, common::TEST_NOW).unwrap();

    let boundary = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::BoundaryViolations);
    let cycles = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ImportCycles);
    let gate = result.signals.iter().find(|s| {
        matches!(
            s.code(),
            SignalCode::GatePass | SignalCode::GateFail | SignalCode::GateIncomplete
        )
    });

    assert!(
        boundary.is_none(),
        "boundary violations must not fire without module context"
    );
    assert!(
        cycles.is_none(),
        "import cycles must not fire without module context"
    );
    assert!(
        gate.is_none(),
        "gate signals must not fire without module context"
    );

    // Also no GATE_NOT_APPLICABLE_TO_FOCUS limit.
    let gate_limit = result
        .limits
        .iter()
        .find(|l| l.code == LimitCode::GateNotApplicableToFocus);
    assert!(
        gate_limit.is_none(),
        "GATE_NOT_APPLICABLE_TO_FOCUS must not fire without module context"
    );
}

// ── 14. MODULE_SUMMARY not emitted at symbol scope ─────────────

#[test]
fn module_summary_not_emitted_at_symbol_scope() {
    let fake = seeded_symbol();

    let result = orient(&fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();

    let mod_summary = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ModuleSummary);
    assert!(
        mod_summary.is_none(),
        "MODULE_SUMMARY must not be emitted at symbol scope"
    );
}

// ── PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11): the inferred remainder on the symbol focus ──

const DO_WORK: &str = "r1:src/core/service.ts:SYMBOL:doWork";

fn inferred(key: &str, basis: &str) -> repo_graph_agent::AgentInferredCallRow {
    repo_graph_agent::AgentInferredCallRow {
        stable_key: key.into(),
        name: key.into(),
        qualified_name: None,
        file: Some("src/x.py".into()),
        line: Some(7),
        module_path: Some("src".into()),
        module_stable_key: None,
        basis: basis.into(),
        receiver: Some("obj".into()),
        extractor: "python-core:0.2.0".into(),
    }
}

fn caller(name: &str) -> AgentCallerRow {
    AgentCallerRow {
        stable_key: format!("r1:src/cli/{name}.ts:SYMBOL:{name}"),
        name: name.into(),
        file: Some(format!("src/cli/{name}.ts")),
        line: None,
        module_path: Some("src/cli".into()),
        module_stable_key: None,
    }
}

fn summary_of(fake: &FakeAgentStorage, code: SignalCode) -> Option<(String, serde_json::Value)> {
    let result = orient(fake, "r1", Some("doWork"), Budget::Large, common::TEST_NOW).unwrap();
    result.signals.iter().find(|s| s.code() == code).map(|s| {
        (
            s.summary().to_string(),
            serde_json::to_value(s.evidence()).unwrap(),
        )
    })
}

#[test]
fn orient_symbol_callers_summary_counts_certain_callers_and_states_the_inferred_remainder() {
    let mut fake = seeded_symbol();
    fake.symbol_callers.insert(
        ("snap-1".into(), DO_WORK.into()),
        vec![caller("main"), caller("setup")],
    );
    fake.symbol_call_remainders.insert(
        ("snap-1".into(), DO_WORK.into()),
        repo_graph_agent::AgentCallRemainders {
            inferred_callers: vec![
                inferred("a", "receiver_untyped_name_only"),
                inferred("b", "receiver_untyped_name_only"),
                inferred("c", "receiver_untyped_name_only"),
            ],
            ..Default::default()
        },
    );
    let (summary, json) = summary_of(&fake, SignalCode::CallersSummary).expect("emitted");
    assert_eq!(json["count"], 2, "certain callers only");
    assert_eq!(json["inferred_count"], 3);
    assert_eq!(
        summary,
        "2 direct callers across 1 module; 3 inferred (name-only) — investigate with rmap callers --include-inferred"
    );
}

#[test]
fn orient_symbol_emits_the_callers_summary_when_only_inferred_callers_exist() {
    let mut fake = seeded_symbol();
    fake.symbol_call_remainders.insert(
        ("snap-1".into(), DO_WORK.into()),
        repo_graph_agent::AgentCallRemainders {
            // A legacy row (basis unrecorded) → the label drops "(name-only)".
            inferred_callers: vec![inferred("a", "unrecorded")],
            ..Default::default()
        },
    );
    let (summary, json) = summary_of(&fake, SignalCode::CallersSummary).expect("emitted");
    assert_eq!(json["count"], 0);
    assert_eq!(json["inferred_count"], 1);
    assert_eq!(
        summary,
        "0 direct callers across 0 modules; 1 inferred — investigate with rmap callers --include-inferred"
    );
    // Neither certain nor inferred → no summary (unchanged).
    let plain = seeded_symbol();
    assert!(summary_of(&plain, SignalCode::CallersSummary).is_none());
}

#[test]
fn orient_symbol_callees_summary_states_the_inferred_remainder() {
    let mut fake = seeded_symbol();
    fake.symbol_callees.insert(
        ("snap-1".into(), DO_WORK.into()),
        vec![AgentCalleeRow {
            stable_key: "r1:src/core/db.ts:SYMBOL:query".into(),
            name: "query".into(),
            file: Some("src/core/db.ts".into()),
            line: None,
            module_path: Some("src/core".into()),
            module_stable_key: None,
        }],
    );
    fake.symbol_call_remainders.insert(
        ("snap-1".into(), DO_WORK.into()),
        repo_graph_agent::AgentCallRemainders {
            inferred_callees: vec![inferred("x", "receiver_untyped_name_only")],
            ..Default::default()
        },
    );
    let (summary, json) = summary_of(&fake, SignalCode::CalleesSummary).expect("emitted");
    assert_eq!(json["count"], 1);
    assert_eq!(json["inferred_count"], 1);
    assert_eq!(
        summary,
        "1 direct callee across 1 module; 1 inferred (name-only) — investigate with rmap callees --include-inferred"
    );
    // A summary without inferred rows serializes without the field (byte-stable).
    let mut plain = seeded_symbol();
    plain
        .symbol_callers
        .insert(("snap-1".into(), DO_WORK.into()), vec![caller("main")]);
    let (s, j) = summary_of(&plain, SignalCode::CallersSummary).unwrap();
    assert_eq!(s, "1 direct caller across 1 module.");
    assert!(j.get("inferred_count").is_none());
}
