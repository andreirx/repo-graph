//! FOCUS-RESOLUTION-LIVEGRAPH-IMPL: resolver unit tests (split from the `impl` per the 500-line
//! guardrail, review-1 pt5). `use super::*` pulls in the resolver methods, the re-exported native
//! types, and the parent module's private key-parse helpers.

use super::*;
use repo_graph_ir::{
    CanonicalKey, IrVisibility, Partition, PartitionId, PartitionIr, PartitionKind, Provenance,
    SourceRange, SymbolAttributes,
};
use repo_graph_trust_model::{AnswerClass, LanguageSupport};

const REPO: &str = "repo_focus";

fn prov() -> Provenance {
    Provenance {
        indexer: "scip-typescript".into(),
        indexer_version: "0.4.0".into(),
        scip_symbol_id: None,
        build_inputs_hash: "h".into(),
    }
}

fn partition() -> Partition {
    Partition {
        id: PartitionId::new("p"),
        kind: PartitionKind::TsPackage,
        root: ".".into(),
        indexer: "scip-typescript".into(),
        indexer_version: "0.4.0".into(),
        build_inputs_hash: "h".into(),
        package_name: None,
        declared_dependencies: std::collections::BTreeSet::new(),
        tsconfig_aliases: None,
    }
}

fn file_node(path: &str) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(format!("{REPO}:{path}:FILE")),
        subtype: "File".into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        range: None,
        partition_id: PartitionId::new("p"),
        identity_source: IdentitySource::AstFileScope,
        provenance: prov(),
        attributes: None,
    }
}

/// An AST-adopted symbol. `name_segment` is the key's `#…` segment (= qualified_name); `name`
/// is the bare symbol name (they differ for a method).
fn symbol_node(path: &str, name_segment: &str, name: &str, kind: &str, line: u32) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(format!("{REPO}:{path}#{name_segment}:SYMBOL:{kind}")),
        subtype: "Term".into(), // coarse SCIP descriptor (distinct from the granular kind)
        name: name.into(),
        range: Some(SourceRange {
            file: path.into(),
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 0,
        }),
        partition_id: PartitionId::new("p"),
        identity_source: IdentitySource::AstAdopted,
        provenance: prov(),
        attributes: Some(SymbolAttributes {
            visibility: Some(IrVisibility::Export),
            is_top_level: true,
            symbol_kind: Some(kind.into()),
        }),
    }
}

/// A LiveGraph with two nested files + a repo-root file, several symbols (incl. a duplicate
/// name and a method), one resident TS partition.
fn fixture() -> LiveGraph {
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node("src/a.ts"));
    ir.nodes.push(file_node("src/util/b.ts"));
    ir.nodes.push(file_node("main.ts"));
    ir.nodes
        .push(symbol_node("src/a.ts", "foo", "foo", "FUNCTION", 3));
    ir.nodes
        .push(symbol_node("src/a.ts", "Widget", "Widget", "CLASS", 10));
    ir.nodes.push(symbol_node(
        "src/a.ts",
        "Widget.render",
        "render",
        "METHOD",
        12,
    ));
    ir.nodes
        .push(symbol_node("src/util/b.ts", "foo", "foo", "FUNCTION", 1));
    ir.nodes
        .push(symbol_node("main.ts", "boot", "boot", "FUNCTION", 1));
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);
    lg
}

#[test]
fn key_parse_helpers_are_colon_and_hash_safe() {
    let sym = "repo_x:src/a.ts#Widget.render:SYMBOL:METHOD";
    assert_eq!(symbol_key_path(sym), Some("src/a.ts"));
    assert_eq!(
        symbol_key_qualified_name(sym),
        Some("Widget.render".to_string())
    );
    assert_eq!(key_repo_prefix(sym), Some("repo_x"));
    assert_eq!(module_key_dir("repo_x:src/util:MODULE"), Some("src/util"));
    // A dup-suffixed key still parses the qualified_name from the `#…:SYMBOL:` segment.
    assert_eq!(
        symbol_key_qualified_name("repo_x:f.ts#dupName:SYMBOL:FUNCTION:dup2"),
        Some("dupName".to_string())
    );
    // Non-symbol keys yield None for the symbol parses.
    assert_eq!(symbol_key_qualified_name("repo_x:src/a.ts:FILE"), None);
    assert_eq!(module_key_dir("repo_x:src/a.ts:FILE"), None);
}

#[test]
fn resolve_path_exact_file() {
    let env = fixture().resolve_path("src/a.ts");
    assert_eq!(env.class(), AnswerClass::Exact);
    let d = env.data().unwrap();
    assert!(d.has_exact_file);
    assert_eq!(d.file_key.as_deref(), Some("repo_focus:src/a.ts:FILE"));
    assert!(!d.has_content_under_prefix);
    assert_eq!(d.module_key, None);
}

#[test]
fn resolve_path_directory_is_module_and_prefix() {
    let lg = fixture();
    let top = lg.resolve_path("src");
    let d = top.data().unwrap();
    assert!(!d.has_exact_file);
    assert!(d.has_content_under_prefix);
    assert_eq!(d.module_key.as_deref(), Some("repo_focus:src:MODULE"));

    let nested = lg.resolve_path("src/util");
    let d = nested.data().unwrap();
    assert!(d.has_content_under_prefix);
    assert_eq!(d.module_key.as_deref(), Some("repo_focus:src/util:MODULE"));
}

#[test]
fn resolve_path_miss_is_all_negative() {
    let env = fixture().resolve_path("does/not/exist");
    let d = env.data().unwrap();
    assert!(!d.has_exact_file);
    assert!(!d.has_content_under_prefix);
    assert_eq!(d.module_key, None);
    assert_eq!(d.file_key, None);
}

#[test]
fn resolve_stable_key_file_module_symbol_and_miss() {
    let lg = fixture();
    let f = lg.resolve_stable_key("repo_focus:src/a.ts:FILE");
    let c = f.data().unwrap().clone().unwrap();
    assert_eq!(c.kind, FocusKind::File);
    assert_eq!(c.file.as_deref(), Some("src/a.ts"));

    let m = lg.resolve_stable_key("repo_focus:src:MODULE");
    let c = m.data().unwrap().clone().unwrap();
    assert_eq!(c.kind, FocusKind::Module);
    assert_eq!(c.file, None); // MODULE has null file_uid in SQLite

    let s = lg.resolve_stable_key("repo_focus:src/a.ts#Widget:SYMBOL:CLASS");
    let c = s.data().unwrap().clone().unwrap();
    assert_eq!(c.kind, FocusKind::Symbol);
    assert_eq!(c.file.as_deref(), Some("src/a.ts"));

    // A :MODULE key for a non-existent directory -> None.
    assert!(lg
        .resolve_stable_key("repo_focus:nope:MODULE")
        .data()
        .unwrap()
        .is_none());
    // A garbage key -> None.
    assert!(lg.resolve_stable_key("garbage").data().unwrap().is_none());
}

/// review-1 pt2 (the module repo-prefix false positive): a foreign-repo module stable key MUST miss
/// even when its directory segment names a resident directory of THIS repo. SQLite
/// `resolve_stable_key_focus` matches `stable_key` EXACTLY, so `other_repo:src:MODULE` resolves to
/// nothing here. (`src` IS a resident directory — proven by the positive case just above — so the
/// only reason to miss is the repo-prefix guard.)
#[test]
fn resolve_stable_key_foreign_repo_module_misses() {
    let lg = fixture();
    // Sanity: the SAME directory under the RESIDENT repo prefix DOES resolve.
    assert!(lg
        .resolve_stable_key("repo_focus:src:MODULE")
        .data()
        .unwrap()
        .is_some());
    // A foreign repo prefix over the same resident directory MUST miss.
    assert!(
        lg.resolve_stable_key("other_repo:src:MODULE")
            .data()
            .unwrap()
            .is_none(),
        "a foreign-repo module key must not match a resident directory"
    );
    // A nested resident directory under a foreign prefix likewise misses.
    assert!(lg
        .resolve_stable_key("other_repo:src/util:MODULE")
        .data()
        .unwrap()
        .is_none());
}

#[test]
fn resolve_symbol_name_orders_and_caps() {
    let lg = fixture();
    let env = lg.resolve_symbol_name("foo");
    let cands = env.data().unwrap();
    assert_eq!(cands.len(), 2, "two symbols named foo");
    // Sorted by key ascending: src/a.ts < src/util/b.ts.
    assert_eq!(cands[0].key, "repo_focus:src/a.ts#foo:SYMBOL:FUNCTION");
    assert_eq!(cands[1].key, "repo_focus:src/util/b.ts#foo:SYMBOL:FUNCTION");
    assert!(cands.iter().all(|c| c.kind == FocusKind::Symbol));
    assert_eq!(cands[1].file.as_deref(), Some("src/util/b.ts"));

    assert_eq!(lg.resolve_symbol_name("Widget").data().unwrap().len(), 1);
    assert!(lg.resolve_symbol_name("missing").data().unwrap().is_empty());
}

/// review-1 pt4 (ambiguity at the cap): >5 same-name symbols -> exactly the first 5 by key ascending,
/// proving the `LIMIT 5` + `ORDER BY stable_key ASC` parity shape at the producer level (the
/// against-SQLite parity of this cap lives in the daemon cert tests).
#[test]
fn resolve_symbol_name_caps_at_five_by_key_order() {
    let mut ir = PartitionIr::new(partition());
    // Six files, each declaring a symbol named "dup"; keys sort by the file segment.
    for i in 0..6 {
        let path = format!("src/d{i}.ts");
        ir.nodes.push(file_node(&path));
        ir.nodes
            .push(symbol_node(&path, "dup", "dup", "FUNCTION", 1));
    }
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);

    let cands = lg.resolve_symbol_name("dup").data().unwrap().clone();
    assert_eq!(cands.len(), 5, "six matches capped to five");
    // The first five by canonical-key ascending (d0..d4); d5 is dropped by the cap.
    let keys: Vec<&str> = cands.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "repo_focus:src/d0.ts#dup:SYMBOL:FUNCTION",
            "repo_focus:src/d1.ts#dup:SYMBOL:FUNCTION",
            "repo_focus:src/d2.ts#dup:SYMBOL:FUNCTION",
            "repo_focus:src/d3.ts#dup:SYMBOL:FUNCTION",
            "repo_focus:src/d4.ts#dup:SYMBOL:FUNCTION",
        ]
    );
}

#[test]
fn symbol_context_full_fields_and_method_qualified_name() {
    let lg = fixture();
    let env = lg.symbol_context("repo_focus:src/a.ts#Widget.render:SYMBOL:METHOD");
    let ctx = env.data().unwrap().clone().unwrap();
    assert_eq!(ctx.file_path.as_deref(), Some("src/a.ts"));
    assert_eq!(ctx.module_path.as_deref(), Some("src"));
    assert_eq!(ctx.module_key.as_deref(), Some("repo_focus:src:MODULE"));
    assert_eq!(ctx.name, "render");
    assert_eq!(ctx.qualified_name.as_deref(), Some("Widget.render"));
    assert_eq!(ctx.subtype.as_deref(), Some("METHOD"));
    assert_eq!(ctx.line_start, Some(12));
}

#[test]
fn symbol_context_root_file_has_no_module() {
    // A symbol in a repo-root file: no ancestor directory -> no module (matches SQLite's null
    // OWNS-edge join for a root file).
    let lg = fixture();
    let env = lg.symbol_context("repo_focus:main.ts#boot:SYMBOL:FUNCTION");
    let ctx = env.data().unwrap().clone().unwrap();
    assert_eq!(ctx.file_path.as_deref(), Some("main.ts"));
    assert_eq!(ctx.module_path, None);
    assert_eq!(ctx.module_key, None);
}

#[test]
fn symbol_context_miss_is_none() {
    let lg = fixture();
    assert!(lg
        .symbol_context("repo_focus:src/a.ts#ghost:SYMBOL:FUNCTION")
        .data()
        .unwrap()
        .is_none());
}

#[test]
fn focus_corpus_enumerates_the_resident_identity_set() {
    let c = fixture().focus_corpus();
    assert_eq!(c.file_paths, vec!["main.ts", "src/a.ts", "src/util/b.ts"]);
    assert_eq!(c.module_dirs, vec!["src", "src/util"]);
    assert_eq!(
        c.symbol_keys,
        vec![
            "repo_focus:src/a.ts#Widget.render:SYMBOL:METHOD",
            "repo_focus:src/a.ts#Widget:SYMBOL:CLASS",
            "repo_focus:src/a.ts#foo:SYMBOL:FUNCTION",
            "repo_focus:src/util/b.ts#foo:SYMBOL:FUNCTION",
            "repo_focus:main.ts#boot:SYMBOL:FUNCTION",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );
    assert_eq!(c.symbol_names, vec!["Widget", "boot", "foo", "render"]);
}

#[test]
fn non_resident_partition_forces_partial_never_exact_empty() {
    // The "null = unknown, never empty" rule: after unload the IR is gone, so a focus that WAS
    // resolvable now returns empty UNDER a Partial envelope (UNKNOWN), never a confident miss.
    let mut lg = fixture();
    lg.unload_partition("p");
    let env = lg.resolve_symbol_name("foo");
    assert_eq!(
        env.class(),
        AnswerClass::Partial,
        "a non-resident partition must degrade to Partial, not Exact-empty"
    );
    assert!(env.data().map(|v| v.is_empty()).unwrap_or(true));
    // resolve_path likewise degrades.
    assert_eq!(lg.resolve_path("src/a.ts").class(), AnswerClass::Partial);
}

#[test]
fn non_ts_partition_forces_partial_never_exact() {
    // review-1 pt4 (non-TS fallback at the producer level): a non-TS partition is in the
    // `whole_graph_completeness` `missing` set, so every resolver envelope is Partial — the
    // completeness gate the daemon cert relies on to force RED -> SQLite fallback for non-TS repos.
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node("src/a.ts"));
    ir.nodes
        .push(symbol_node("src/a.ts", "foo", "foo", "FUNCTION", 1));
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::RustPartialBeta);
    assert_eq!(
        lg.resolve_path("src/a.ts").class(),
        AnswerClass::Partial,
        "a non-TS partition must never resolve Exact"
    );
    assert_eq!(lg.resolve_symbol_name("foo").class(), AnswerClass::Partial);
}

#[test]
fn resolution_reads_zero_storage_structurally() {
    // The producer's no-`nodes`-read proof at the PRODUCER level: the resolver is a method on
    // an in-memory LiveGraph built purely from IR — repo-graph-livegraph has NO storage / SQLite
    // dependency, so resolution CANNOT read `nodes`. This test exercises every kind against an
    // in-memory graph with no storage in the picture; correctness here IS the zero-read proof.
    // (The daemon's GREEN-serve panicking-closure proof — that the green DECISION skips SQLite —
    // lives in `focus_resolution_cert::tests`; the consumer-fastpath storage-spy proof is V2.)
    let lg = fixture();
    assert!(lg.resolve_path("src/a.ts").data().unwrap().has_exact_file);
    assert!(lg
        .resolve_stable_key("repo_focus:src/a.ts:FILE")
        .data()
        .unwrap()
        .is_some());
    assert_eq!(lg.resolve_symbol_name("foo").data().unwrap().len(), 2);
    assert!(lg
        .symbol_context("repo_focus:src/a.ts#foo:SYMBOL:FUNCTION")
        .data()
        .unwrap()
        .is_some());
}
