//! METRIC-LANG-COVERAGE-1 (part B) — Rust complexity emission, proven at the
//! `ExtractorPort` boundary.
//!
//! Relocated out of `src/extractor.rs` (a >500-line file) per the repo's
//! Structural Guardrail ("do not append new responsibilities to files over 500
//! lines"). These tests touch ONLY the crate's public surface — `RustExtractor`
//! driven through the `ExtractorPort` trait, reading the public
//! `ExtractionResult` DTO — so they belong in an integration crate, not inline
//! with the extractor's private walk. No private access is required.
//!
//! What they prove: metrics reach `ExtractionResult.metrics`, keyed by each
//! symbol's `stable_key`, with hand-computed cyclomatic values that use the SAME
//! decision-point counting rules as the C/TS extractors (base 1; +1 per
//! `if`/`while`/`loop`/`for`; +1 per non-wildcard `match` arm — a bare `_` is
//! C's `default:`, +0; +1 per `&&`/`||`; +1 per `?`), so cross-language
//! complexity rankings stay comparable (the slice's comparability contract).

use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::types::{ExtractedMetrics, ExtractionResult, NodeKind};
use repo_graph_rust_extractor::RustExtractor;

/// Drive the public port over a source string, mirroring the crate's own
/// `extract_test` helper but through the exported API only.
fn extract_test(source: &str) -> ExtractionResult {
    let mut extractor = RustExtractor::new();
    extractor.initialize().unwrap();
    extractor
        .extract(source, "src/lib.rs", "test:src/lib.rs", "test", "snap-1")
        .unwrap()
}

/// Look up the metric emitted for a symbol by its stable_key, given the
/// symbol's `name` (works for free functions keyed `#name:SYMBOL:FUNCTION`).
fn metric_for_named(result: &ExtractionResult, name: &str) -> ExtractedMetrics {
    let node = result
        .nodes
        .iter()
        .find(|n| n.name == name && n.kind == NodeKind::Symbol)
        .unwrap_or_else(|| panic!("no symbol node named {name}"));
    *result
        .metrics
        .get(&node.stable_key)
        .unwrap_or_else(|| panic!("no metric for {name} (key {})", node.stable_key))
}

#[test]
fn emits_cyclomatic_for_free_function() {
    // base(1) + if(1) + &&(1) = 3
    let result = extract_test("fn f(a: bool, b: bool) { if a && b { g(); } }");
    let m = metric_for_named(&result, "f");
    assert_eq!(m.cyclomatic_complexity, 3);
    assert_eq!(m.parameter_count, 2);
    assert_eq!(m.max_nesting_depth, 1);
}

#[test]
fn emits_cyclomatic_for_match_heavy_function() {
    // The dispatch-handler shape this slice targets: base(1) + 3 real arms
    // (+3) + bare `_` (+0) = 4.
    let src = r#"
        fn dispatch(cmd: Cmd) -> u8 {
            match cmd {
                Cmd::A => 1,
                Cmd::B => 2,
                Cmd::C => 3,
                _ => 0,
            }
        }
    "#;
    let result = extract_test(src);
    let m = metric_for_named(&result, "dispatch");
    assert_eq!(m.cyclomatic_complexity, 4);
    assert_eq!(m.parameter_count, 1);
}

#[test]
fn emits_cyclomatic_for_impl_method_with_self() {
    // base(1) + for(1) + if(1) + ?(1) = 4; params: self + items = 2.
    let src = r#"
        struct S;
        impl S {
            fn run(&self, items: Vec<i32>) -> Result<i32, E> {
                let mut n = 0;
                for it in items {
                    if it > 0 { n += do_thing(it)?; }
                }
                Ok(n)
            }
        }
    "#;
    let result = extract_test(src);
    let m = metric_for_named(&result, "run");
    assert_eq!(m.cyclomatic_complexity, 4);
    assert_eq!(m.parameter_count, 2);
}

#[test]
fn emits_metric_for_default_trait_method_not_bare_signature() {
    // A default method (has a body) is measured; a bare signature is not.
    let src = r#"
        trait T {
            fn required(&self);
            fn defaulted(&self, x: i32) -> i32 {
                if x > 0 { 1 } else { 0 }
            }
        }
    "#;
    let result = extract_test(src);
    // defaulted: base(1) + if(1) = 2
    let m = metric_for_named(&result, "defaulted");
    assert_eq!(m.cyclomatic_complexity, 2);
    // The bare signature emits a node but NO metric.
    let req = result.nodes.iter().find(|n| n.name == "required").unwrap();
    assert!(!result.metrics.contains_key(&req.stable_key));
}

#[test]
fn non_function_symbols_carry_no_metric() {
    // Structs/enums are not measured; only bodies are.
    let result = extract_test("struct Foo { a: i32 }\nfn bar() {}");
    let foo = result.nodes.iter().find(|n| n.name == "Foo").unwrap();
    assert!(!result.metrics.contains_key(&foo.stable_key));
    assert!(result.metrics.contains_key(
        &result
            .nodes
            .iter()
            .find(|n| n.name == "bar")
            .unwrap()
            .stable_key
    ));
}
