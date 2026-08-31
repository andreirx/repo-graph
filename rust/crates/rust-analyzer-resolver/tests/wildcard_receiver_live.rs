//! Live end-to-end proof of the ENRICH-YIELD-3 receiver locator (EY1-C).
//!
//! Spawns a REAL rust-analyzer against a tiny Cargo fixture where `self`'s type (`Outer`) and
//! `self.field`'s type (`Inner`) DIFFER and BOTH define a `run` method — the exact false-edge
//! scenario. It proves the resolver, routed through the tree-sitter receiver locator, resolves the
//! FIELD's type (`Inner`), not `self`'s (`Outer`). Without the locator the resolver would hover the
//! stored call-expression start (the `self` token) and return `Outer`, minting a false Layer-0 edge.
//!
//! `#[ignore]` because it spawns rust-analyzer (slow warm-up, environment-dependent). Run it
//! explicitly:
//!
//! ```text
//! cargo test -p rust-analyzer-resolver --test wildcard_receiver_live -- --ignored --nocapture
//! ```
//!
//! It skips (passes) cleanly if rust-analyzer cannot start, so it is portable.

use std::fs;

use enrichment::{EligibleEdge, EnrichmentLanguage, ReceiverTypeResolver, UnresolvedCategory};
use rust_analyzer_resolver::RustAnalyzerResolver;

const LIB_RS: &str = "\
pub struct Inner;
impl Inner {
    pub fn run(&self) {}
}

pub struct Outer {
    field: Inner,
}

impl Outer {
    // Outer ALSO defines `run` — the false-edge trap. The call below must resolve to Inner::run, so
    // the receiver type must be Inner (the field's type), NOT Outer (the enclosing type). (Comment
    // deliberately avoids the literal call text so the position search below lands on real code.)
    pub fn run(&self) {}

    pub fn caller(&self) {
        self.field.run();
    }
}
";

/// 1-based line + 0-based column of the first occurrence of `needle` in `source` — the coordinate
/// convention the extractor emits (`line_start` 1-based, `col_start` 0-based).
fn line_col_of(source: &str, needle: &str) -> (u32, u32) {
    let byte = source.find(needle).expect("needle present");
    let mut line = 1u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i == byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[test]
#[ignore = "spawns rust-analyzer; run with --ignored"]
fn self_field_method_resolves_the_field_type_not_self_live() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"ey3_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), LIB_RS).unwrap();

    // The call edge is stamped at the call-expression START — the `self` token of `self.field.run()`.
    let (line, col) = line_col_of(LIB_RS, "self.field.run");
    let edge = EligibleEdge {
        edge_uid: "ey3-live".to_string(),
        snapshot_uid: "snap".to_string(),
        repo_uid: "repo".to_string(),
        source_node_uid: "caller".to_string(),
        target_key: "self.field.run".to_string(),
        source_file_path: "src/lib.rs".to_string(),
        line_start: line,
        col_start: col,
        // The REAL category Rust assigns to `self.field.method`: the indexer categorizer only maps
        // TS `this.`-keys to the wildcard category, so Rust `self.field` is OBJ. This proves the
        // resolver routes it to the locator by SHAPE (EY3-ROUTING), not by category.
        category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        language: EnrichmentLanguage::Rust,
    };

    let resolver = RustAnalyzerResolver::new();
    let results = resolver.resolve_batch(root, &[edge], None, None).results;

    assert_eq!(results.len(), 1, "one result per edge");
    let r = &results[0];

    // Portability: if rust-analyzer could not start/respond in this environment, skip rather than
    // fail (the failure reason names the tooling problem, not a resolution mismatch).
    if !r.is_success() {
        let reason = r.failure_reason.as_deref().unwrap_or("");
        if reason.contains("failed to start")
            || reason.contains("did not respond")
            || reason.contains("hover_no_response")
            || reason.contains("hover_timeout")
        {
            eprintln!("SKIP: rust-analyzer unavailable/slow in this environment: {reason}");
            return;
        }
        panic!("unexpected resolution failure: {reason:?}");
    }

    // THE PROOF: the resolved receiver type is the FIELD's type (Inner), not self's type (Outer).
    assert_eq!(
        r.receiver_type.as_deref(),
        Some("Inner"),
        "receiver locator must resolve self.field's type (Inner), not self's type (Outer) — got {:?}",
        r.receiver_type
    );
    assert_ne!(
        r.receiver_type.as_deref(),
        Some("Outer"),
        "resolving Outer here would be the EY1-C false edge"
    );
}
