//! Unit tests for the pure `#[cfg(test)]` inclusion-chain resolver
//! (`super`). Split out of `rust_test_classifier.rs` (IS-TEST-RUST-1 review-2
//! item 3) so the production module stays under the 500-line module guardrail;
//! as a child module it still reaches `super`'s crate-private items.

use super::*;

fn decl(name: &str, cfg_test: bool, path: Option<&str>) -> RustModDecl {
    RustModDecl {
        name: name.into(),
        cfg_test,
        path_override: path.map(|s| s.to_string()),
        inline_path: vec![],
    }
}

/// A declaration nested inside inline module blocks (outermost-first segments).
fn decl_inline(name: &str, cfg_test: bool, path: Option<&str>, inline: &[&str]) -> RustModDecl {
    RustModDecl {
        name: name.into(),
        cfg_test,
        path_override: path.map(|s| s.to_string()),
        inline_path: inline.iter().map(|s| s.to_string()).collect(),
    }
}

fn facts(rel: &str, decls: Vec<RustModDecl>) -> RustFileFacts {
    RustFileFacts {
        rel_path: rel.into(),
        mod_decls: decls,
    }
}

#[test]
fn cfg_test_mod_promotes_target() {
    // lib.rs: `#[cfg(test)] mod tests;` -> src/tests.rs
    let files = vec![
        facts("src/lib.rs", vec![decl("tests", true, None)]),
        facts("src/tests.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(out.test_files.contains("src/tests.rs"));
    assert!(!out.test_files.contains("src/lib.rs"));
    assert!(out.unresolved.is_empty());
}

#[test]
fn nested_include_under_test_is_transitively_test() {
    // lib.rs: #[cfg(test)] mod tests; -> tests.rs
    // tests.rs: mod helper; (no cfg) -> tests/helper.rs
    let files = vec![
        facts("src/lib.rs", vec![decl("tests", true, None)]),
        facts("src/tests.rs", vec![decl("helper", false, None)]),
        facts("src/tests/helper.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(out.test_files.contains("src/tests.rs"));
    assert!(out.test_files.contains("src/tests/helper.rs"));
}

#[test]
fn inline_module_path_resolves_nested_child() {
    // lib.rs: `#[cfg(test)] mod scope { mod child; }` — the inline block `scope`
    // is cfg(test)-gated and contributes a directory segment, so `child` is
    // src/scope/child.rs and is transitively test. (review-2 item 1)
    let files = vec![
        facts(
            "src/lib.rs",
            vec![decl_inline("child", true, None, &["scope"])],
        ),
        facts("src/scope/child.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(
        out.test_files.contains("src/scope/child.rs"),
        "inline module `scope` must place `child` under src/scope/ and promote it",
    );
    // Without the inline segment the resolver would have sought src/child.rs.
    assert!(!out.test_files.contains("src/child.rs"));
    assert!(out.unresolved.is_empty());
}

#[test]
fn inline_path_override_relative_to_inline_context() {
    // `mod scope { #[cfg(test)] #[path = "c.rs"] mod child; }` in src/lib.rs:
    // the #[path] is nested in inline `scope`, so it resolves to src/scope/c.rs.
    let files = vec![
        facts(
            "src/lib.rs",
            vec![decl_inline("child", true, Some("c.rs"), &["scope"])],
        ),
        facts("src/scope/c.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(out.test_files.contains("src/scope/c.rs"));
}

#[test]
fn path_override_resolves_name_trap() {
    // The corpus idiom: #[cfg(test)] #[path = "foo_tests.rs"] mod tests;
    let files = vec![
        facts(
            "src/foo.rs",
            vec![decl("tests", true, Some("foo_tests.rs"))],
        ),
        facts("src/foo_tests.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(out.test_files.contains("src/foo_tests.rs"));
}

#[test]
fn production_module_with_test_in_name_is_not_promoted() {
    // Name-trap: `mod tests_util;` WITHOUT cfg(test) -> production.
    let files = vec![
        facts("src/lib.rs", vec![decl("tests_util", false, None)]),
        facts("src/tests_util.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(!out.test_files.contains("src/tests_util.rs"));
}

#[test]
fn directory_module_via_mod_rs() {
    // #[cfg(test)] mod suite; -> src/suite/mod.rs
    let files = vec![
        facts("src/lib.rs", vec![decl("suite", true, None)]),
        facts("src/suite/mod.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(out.test_files.contains("src/suite/mod.rs"));
}

#[test]
fn undeclared_stray_file_untouched() {
    let files = vec![facts("src/lib.rs", vec![]), facts("src/stray.rs", vec![])];
    let out = classify(&files);
    assert!(out.test_files.is_empty());
    assert!(out.unresolved.is_empty());
}

#[test]
fn missing_target_is_unresolved() {
    let files = vec![facts("src/lib.rs", vec![decl("ghost", true, None)])];
    let out = classify(&files);
    assert!(out.test_files.is_empty());
    assert_eq!(out.unresolved.len(), 1);
    assert_eq!(out.unresolved[0].reason, "no_candidate_file");
    assert_eq!(out.unresolved[0].mod_name, "ghost");
}

#[test]
fn multiple_parents_diagnosed_and_not_promoted() {
    // Two files declare `mod shared;` resolving to the same target, and ONE
    // of them is #[cfg(test)]-gated. Fail-closed: the ambiguity poisons the
    // target — it is NOT promoted despite the cfg(test) declaration.
    let files = vec![
        facts("src/a.rs", vec![decl("shared", true, Some("shared.rs"))]),
        facts("src/b.rs", vec![decl("shared", false, Some("shared.rs"))]),
        facts("src/shared.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(
        out.unresolved
            .iter()
            .any(|u| u.reason == "multiple_parents"),
        "two parents compiling one file must be diagnosed"
    );
    assert!(
        !out.test_files.contains("src/shared.rs"),
        "ambiguous target must keep its existing classification, never promote on a guess"
    );
}

#[test]
fn ambiguous_conventional_target_not_promoted() {
    // Both src/foo.rs AND src/foo/mod.rs exist for `#[cfg(test)] mod foo;`.
    // Real Rust rejects this; we refuse to pick — neither is promoted.
    let files = vec![
        facts("src/lib.rs", vec![decl("foo", true, None)]),
        facts("src/foo.rs", vec![]),
        facts("src/foo/mod.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(
        out.test_files.is_empty(),
        "ambiguous target must not promote"
    );
    assert!(out
        .unresolved
        .iter()
        .any(|u| u.reason == "ambiguous_target"));
}

#[test]
fn duplicate_declaration_same_parent_not_promoted() {
    // The SAME parent declares `mod dup;` twice (a cfg-variant shape): once
    // #[cfg(test)]-gated, once not. Its cfg(test) status is ambiguous →
    // poisoned, not promoted, diagnosed as a duplicate declaration.
    let files = vec![
        facts(
            "src/lib.rs",
            vec![decl("dup", true, None), decl("dup", false, None)],
        ),
        facts("src/dup.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(
        !out.test_files.contains("src/dup.rs"),
        "a duplicate-declared target must not promote on the cfg(test) variant"
    );
    assert!(out
        .unresolved
        .iter()
        .any(|u| u.reason == "duplicate_declaration"));
}

#[test]
fn child_of_poisoned_parent_is_not_promoted() {
    // A #[cfg(test)] target that is ITSELF poisoned (two parents) must not
    // leak a test label to the module it includes.
    let files = vec![
        facts("src/a.rs", vec![decl("suite", true, Some("suite.rs"))]),
        facts("src/b.rs", vec![decl("suite", true, Some("suite.rs"))]),
        facts("src/suite.rs", vec![decl("child", false, None)]),
        facts("src/suite/child.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(!out.test_files.contains("src/suite.rs"));
    assert!(
        !out.test_files.contains("src/suite/child.rs"),
        "a child pulled in by a poisoned (unpromoted) parent must not be promoted"
    );
}

#[test]
fn escaping_path_override_does_not_promote_in_repo_same_tail() {
    // review-3: `#[cfg(test)] #[path = "../../src/production.rs"] mod x;` in
    // src/lib.rs escapes ABOVE the repo root (src -> repo root -> repo parent),
    // then descends back to a `src/production.rs` TAIL. The prior `join_rel`
    // silently discarded the un-poppable `..` and clamped the target onto the
    // real in-repo `src/production.rs`, promoting a PRODUCTION file on a phantom
    // inclusion. Fail-closed: the escape yields no in-repo candidate, so the
    // file is NOT promoted and the inclusion is diagnosed `no_candidate_file`.
    let files = vec![
        facts(
            "src/lib.rs",
            vec![decl("x", true, Some("../../src/production.rs"))],
        ),
        facts("src/production.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(
        !out.test_files.contains("src/production.rs"),
        "an escaping #[path] must never promote an in-repo same-tail file"
    );
    assert!(out.test_files.is_empty());
    assert_eq!(out.unresolved.len(), 1);
    assert_eq!(out.unresolved[0].reason, "no_candidate_file");
    assert_eq!(out.unresolved[0].declaring_file, "src/lib.rs");
    assert_eq!(out.unresolved[0].mod_name, "x");
}

#[test]
fn in_repo_relative_path_override_still_resolves() {
    // The fail-closed guard must NOT reject a LEGITIMATE `..` that stays inside
    // the repo: `#[cfg(test)] #[path = "../shared.rs"] mod s;` in src/a/b.rs is
    // relative to the declaring file's dir (src/a), so it resolves UP one level
    // to src/shared.rs and is promoted.
    let files = vec![
        facts("src/a/b.rs", vec![decl("s", true, Some("../shared.rs"))]),
        facts("src/shared.rs", vec![]),
    ];
    let out = classify(&files);
    assert!(
        out.test_files.contains("src/shared.rs"),
        "an in-repo `..` #[path] must still resolve and promote"
    );
    assert!(out.unresolved.is_empty());
}

#[test]
fn parse_file_mod_decls_none_and_empty() {
    assert_eq!(parse_file_mod_decls(None), (Vec::new(), false));
    assert_eq!(parse_file_mod_decls(Some("{}")), (Vec::new(), false));
}

#[test]
fn parse_file_mod_decls_valid_blob() {
    let raw = r#"{"rust_mod_decls":[{"name":"tests","cfg_test":true}]}"#;
    let (decls, failed) = parse_file_mod_decls(Some(raw));
    assert!(!failed);
    assert_eq!(decls, vec![decl("tests", true, None)]);
}

#[test]
fn parse_file_mod_decls_inline_path_blob() {
    // The wire contract carries `inline_path`; the consumer DTO deserializes it.
    let raw = r#"{"rust_mod_decls":[{"name":"child","cfg_test":true,"inline_path":["scope"]}]}"#;
    let (decls, failed) = parse_file_mod_decls(Some(raw));
    assert!(!failed);
    assert_eq!(decls, vec![decl_inline("child", true, None, &["scope"])]);
}

#[test]
fn parse_file_mod_decls_malformed_blob_flags_failure() {
    // A non-null blob that is not valid JSON → failure flagged, no decls
    // (honest degradation toward the prior value, never a fabricated label).
    let (decls, failed) = parse_file_mod_decls(Some("not-json"));
    assert!(failed);
    assert!(decls.is_empty());
    // A well-formed blob whose `rust_mod_decls` is the wrong SHAPE also fails.
    let (decls2, failed2) = parse_file_mod_decls(Some(r#"{"rust_mod_decls":"nope"}"#));
    assert!(failed2);
    assert!(decls2.is_empty());
}
