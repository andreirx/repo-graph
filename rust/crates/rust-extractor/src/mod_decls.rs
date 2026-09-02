//! Rust `mod` inclusion-fact extraction (IS-TEST-RUST-1).
//!
//! The `is_test` classification for in-crate Rust test modules
//! (`src/**/tests.rs`, `#[cfg(test)] mod tests;`, …) cannot be derived from a
//! filename — the basis is the STRUCTURAL `#[cfg(test)]` inclusion chain
//! (slice §2.1). This module is the EXTRACTION half of that mechanism: for one
//! source file it emits, via the same tree-sitter pass the extractor already
//! runs, one [`RustModDecl`] per EXTERNAL `mod <name>;` declaration (the ones
//! that pull in a sibling FILE), tagged with whether that inclusion is
//! `#[cfg(test)]`-gated (directly OR via an enclosing `#[cfg(test)] mod { … }`)
//! and any `#[path = "…"]` override.
//!
//! These per-file facts are serialized onto the FILE node's `metadata_json`
//! (an existing column — no storage-schema change; copied forward on refresh
//! with the node). The COMPOSE-side resolver
//! (`repo-index::rust_test_classifier`) later walks the whole crate's chain
//! over these facts and reclassifies `is_test`. This split keeps extraction
//! per-file/local and resolution cross-file — the extractor never guesses a
//! classification.
//!
//! Scope (deliberately narrow, fail-closed like the ratified witness resolver
//! in `daemon-runtime/tests/consolidation_witness.rs`):
//!   - Only EXTERNAL `mod <name>;` (a body-less declaration) is a fact; inline
//!     `mod name { … }` blocks pull no file, so they are recorded ONLY as a
//!     `#[cfg(test)]` context that propagates to their nested declarations.
//!   - Only the exact attribute `#[cfg(test)]` (whitespace-insensitive) gates.
//!     `#[cfg(all(test, …))]`, `#[cfg_attr(...)]`, feature cfgs, etc. are NOT
//!     treated as test-gating — an unrecognized gate stays production, never a
//!     false test label.

use tree_sitter::Node;

/// A single EXTERNAL `mod <name>;` declaration fact: the module `name` it
/// pulls in, whether that inclusion is `#[cfg(test)]`-gated (own attribute or
/// an enclosing `#[cfg(test)] mod`), any `#[path = "…"]` file override, and the
/// enclosing INLINE-module directory segments (`inline_path`) needed to locate
/// the target on disk.
///
/// ## Boundary: raw JSON, two DTOs (NOT one shared type)
///
/// This is the PRODUCER DTO. It is serialized to raw JSON under the
/// `rust_mod_decls` key of the FILE node's `metadata_json`, which crosses the
/// extractor→compose boundary. The consumer (`repo-index::rust_test_classifier`)
/// owns a SEPARATE, structurally-independent DTO that deserializes this JSON.
/// The two sides share only the wire contract — the JSON field names
/// (`name` / `cfg_test` / `path` / `inline_path`) and the object key — never a
/// Rust type. A shared type would be a cross-crate public API the slice packet
/// forbids; keeping the type crate-private on each side is the whole point of
/// the raw-DTO boundary rule.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RustModDecl {
    /// Module identifier in `mod <name>;`.
    pub name: String,
    /// True iff this inclusion is compiled only under `#[cfg(test)]`.
    pub cfg_test: bool,
    /// `#[path = "…"]` override, verbatim (relative to the declaring file's
    /// directory, or — when nested in inline modules — to the inline context
    /// directory). `None` when the module uses the conventional file layout.
    #[serde(rename = "path", default, skip_serializing_if = "Option::is_none")]
    pub path_override: Option<String>,
    /// Directory segments contributed by enclosing INLINE `mod <seg> { … }`
    /// blocks, OUTERMOST first. Empty for a declaration at the file's top level.
    /// `mod scope { mod child; }` in `src/lib.rs` gives `["scope"]`, so the
    /// resolver seeks `src/scope/child.rs` (Rust nests inline-module names as
    /// directories), not `src/child.rs`. An inline block that itself carries a
    /// `#[path]` is NOT represented here — its subtree is skipped at extraction
    /// (see [`walk_container`]), fail-closed, so no wrong directory is guessed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inline_path: Vec<String>,
}

/// The `metadata_json` object key under which [`collect_mod_decls`] facts ride
/// on the FILE node. Kept as one constant so producer and consumer never drift.
pub(crate) const MOD_DECLS_METADATA_KEY: &str = "rust_mod_decls";

/// Extract every EXTERNAL `mod <name>;` declaration in `source` (already parsed
/// into `root`), tagging each with its `#[cfg(test)]` gating and `#[path]`
/// override. Deterministic source order.
pub(crate) fn collect_mod_decls(root: &Node, source: &str) -> Vec<RustModDecl> {
    let mut out = Vec::new();
    // The crate-root context is non-test; a declaration becomes test only by
    // crossing a `#[cfg(test)]` gate. `inline_path` starts empty (top level).
    let mut inline_path: Vec<String> = Vec::new();
    walk_container(root, source, false, &mut inline_path, &mut out);
    out
}

/// Serialize `decls` into the FILE-node `metadata_json` string, or `None` when
/// there are no declarations (files with no `mod` keep `metadata_json = None`,
/// so existing FILE-node output is byte-stable).
///
/// The FILE node carries no other metadata at this call site (the extractor
/// creates it with `metadata_json = None`), so this writes a fresh single-key
/// object — no merge. If a future producer adds other FILE metadata, this
/// becomes its concrete second caller and the merge is added then (earned, not
/// imagined).
pub(crate) fn mod_decls_metadata_json(decls: &[RustModDecl]) -> Option<String> {
    if decls.is_empty() {
        return None;
    }
    Some(serde_json::json!({ MOD_DECLS_METADATA_KEY: decls }).to_string())
}

/// Walk the direct children of a container node (the `source_file` root, or a
/// `mod` body's `declaration_list`), recording external `mod` facts and
/// descending into inline `mod` bodies with the propagated cfg(test) context and
/// the inline-module directory path. Only these two container kinds are
/// descended — a `mod` declaration anywhere else cannot relocate a sibling FILE
/// for the crate.
///
/// `inline_path` is the stack of enclosing inline-module NAMES (the directory
/// segments Rust nests them as); it is pushed/popped around each inline
/// descent so every external-`mod` fact captures the exact directory context it
/// resolves in.
fn walk_container(
    container: &Node,
    source: &str,
    enclosing_cfg_test: bool,
    inline_path: &mut Vec<String>,
    out: &mut Vec<RustModDecl>,
) {
    let mut cursor = container.walk();
    for child in container.children(&mut cursor) {
        if child.kind() != "mod_item" {
            continue;
        }
        let Some(name) = child
            .child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
        else {
            continue;
        };
        let (own_cfg_test, path_override) = read_outer_attrs(&child, source);
        let cfg_test = enclosing_cfg_test || own_cfg_test;

        match child.child_by_field_name("body") {
            // Inline `mod name { … }`: pulls no file. Propagate its cfg(test)
            // context AND its name as a directory segment into nested
            // declarations. EXCEPTION (fail-closed): an inline block carrying a
            // `#[path]` relocates its child directory to a value we do not model
            // here — appending its NAME would guess a wrong directory and risk a
            // false test label. We skip its subtree entirely, leaving any nested
            // external mods undeclared (they keep their existing classification,
            // slice §2.2), never guessed. `#[path]` on inline modules does not
            // occur in the corpus (verified: no `#[path]`-on-`mod`-block hits).
            Some(body) => {
                if path_override.is_some() {
                    continue;
                }
                inline_path.push(name);
                walk_container(&body, source, cfg_test, inline_path, out);
                inline_path.pop();
            }
            // External `mod name;`: a file-inclusion fact.
            None => out.push(RustModDecl {
                name,
                cfg_test,
                path_override,
                inline_path: inline_path.clone(),
            }),
        }
    }
}

/// Read the `#[cfg(test)]`-gating and `#[path = "…"]` override from the outer
/// attributes of `item`. In tree-sitter-rust an item's outer attributes are
/// PRECEDING `attribute_item` siblings (the same convention `extract_doc_comment`
/// relies on), so we scan backwards over the contiguous attribute/comment run.
fn read_outer_attrs(item: &Node, source: &str) -> (bool, Option<String>) {
    let mut cfg_test = false;
    let mut path_override = None;
    let mut prev = item.prev_sibling();
    while let Some(sib) = prev {
        match sib.kind() {
            "attribute_item" => {
                let text = node_text(&sib, source);
                if is_cfg_test_attr(text) {
                    cfg_test = true;
                } else if let Some(p) = parse_path_attr(text) {
                    path_override = Some(p);
                }
                prev = sib.prev_sibling();
            }
            // Doc comments legally sit between an item and its attributes.
            "line_comment" | "block_comment" => prev = sib.prev_sibling(),
            _ => break,
        }
    }
    (cfg_test, path_override)
}

/// True iff `attr` is exactly `#[cfg(test)]` (whitespace-insensitive). Strict by
/// design: a compound gate (`cfg(all(test, …))`, `cfg_attr`, feature cfg) is not
/// recognized and therefore never mislabels a module as test.
fn is_cfg_test_attr(attr: &str) -> bool {
    let compact: String = attr.chars().filter(|c| !c.is_whitespace()).collect();
    compact == "#[cfg(test)]"
}

/// Extract the string from a `#[path = "…"]` attribute, if `attr` is one.
fn parse_path_attr(attr: &str) -> Option<String> {
    let inner = attr.trim().strip_prefix("#[")?.strip_suffix(']')?.trim();
    let val = inner
        .strip_prefix("path")?
        .trim_start()
        .strip_prefix('=')?
        .trim()
        .strip_prefix('"')?;
    val.find('"').map(|end| val[..end].to_string())
}

fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn decls(src: &str) -> Vec<RustModDecl> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        collect_mod_decls(&tree.root_node(), src)
    }

    #[test]
    fn plain_external_mod_is_non_test() {
        let d = decls("mod foo;\n");
        assert_eq!(
            d,
            vec![RustModDecl {
                name: "foo".into(),
                cfg_test: false,
                path_override: None,
                inline_path: vec![],
            }]
        );
    }

    #[test]
    fn cfg_test_gated_mod_is_test() {
        let d = decls("#[cfg(test)]\nmod tests;\n");
        assert_eq!(
            d,
            vec![RustModDecl {
                name: "tests".into(),
                cfg_test: true,
                path_override: None,
                inline_path: vec![],
            }]
        );
    }

    #[test]
    fn path_override_captured() {
        let d = decls("#[cfg(test)]\n#[path = \"foo_tests.rs\"]\nmod tests;\n");
        assert_eq!(
            d,
            vec![RustModDecl {
                name: "tests".into(),
                cfg_test: true,
                path_override: Some("foo_tests.rs".into()),
                inline_path: vec![],
            }]
        );
    }

    #[test]
    fn inline_cfg_test_propagates_to_nested_external_mod() {
        // `#[cfg(test)] mod tests { mod helper; }` — the nested external
        // `mod helper;` inherits the enclosing cfg(test) gate AND records
        // `tests` as its directory segment (target is `tests/helper.rs`).
        let d = decls("#[cfg(test)]\nmod tests {\n    mod helper;\n}\n");
        assert_eq!(
            d,
            vec![RustModDecl {
                name: "helper".into(),
                cfg_test: true,
                path_override: None,
                inline_path: vec!["tests".into()],
            }]
        );
    }

    #[test]
    fn deeply_nested_inline_mods_stack_directory_segments() {
        // `mod a { mod b { mod child; } }` → inline_path OUTERMOST-first `[a, b]`.
        let d = decls("mod a {\n    mod b {\n        mod child;\n    }\n}\n");
        assert_eq!(
            d,
            vec![RustModDecl {
                name: "child".into(),
                cfg_test: false,
                path_override: None,
                inline_path: vec!["a".into(), "b".into()],
            }]
        );
    }

    #[test]
    fn inline_mod_with_path_attr_skips_its_subtree() {
        // Fail-closed: an inline block carrying `#[path]` relocates its child
        // directory to a value we do not model → we skip the subtree, so the
        // nested `mod child;` yields NO fact (never a wrong-directory guess).
        let d = decls("#[path = \"custom\"]\nmod scope {\n    mod child;\n}\n");
        assert!(d.is_empty());
    }

    #[test]
    fn inline_non_test_mod_body_is_not_a_fact_but_descends() {
        let d = decls("mod real {\n    mod inner;\n}\n");
        assert_eq!(
            d,
            vec![RustModDecl {
                name: "inner".into(),
                cfg_test: false,
                path_override: None,
                inline_path: vec!["real".into()],
            }]
        );
    }

    #[test]
    fn compound_cfg_gate_is_not_test() {
        // Fail-closed: `cfg(all(test, ...))` is not the recognized gate.
        let d = decls("#[cfg(all(test, feature = \"x\"))]\nmod tests;\n");
        assert_eq!(d.len(), 1);
        assert!(!d[0].cfg_test);
    }

    #[test]
    fn pub_mod_and_visibility_scoped_mod() {
        let d = decls("pub mod a;\npub(crate) mod b;\n");
        let names: Vec<_> = d.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
        assert!(d.iter().all(|x| !x.cfg_test));
    }

    #[test]
    fn no_mod_decls_yields_empty() {
        assert!(decls("fn main() {}\n").is_empty());
    }

    #[test]
    fn metadata_json_none_when_empty() {
        assert_eq!(mod_decls_metadata_json(&[]), None);
    }

    #[test]
    fn metadata_json_roundtrips() {
        let decls = vec![RustModDecl {
            name: "tests".into(),
            cfg_test: true,
            path_override: Some("x_tests.rs".into()),
            inline_path: vec!["outer".into()],
        }];
        let json = mod_decls_metadata_json(&decls).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let arr = v.get(MOD_DECLS_METADATA_KEY).unwrap();
        let back: Vec<RustModDecl> = serde_json::from_value(arr.clone()).unwrap();
        assert_eq!(back, decls);
    }
}
