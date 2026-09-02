//! ZEROSTATE-SCOPE-1 (§2.3): the TS/JS `import type` caveat gated on cycle MEMBERSHIP.
//!
//! # Why this module
//!
//! The `import type` (type-only) caveat warns that some rendered cycle edges may be
//! TypeScript type-only imports, which do not create a runtime cycle. Before this slice the
//! caveat gated on REPO-LEVEL materiality (`reader_context::repo_has_material_ts_js`, the
//! ≥10%-of-code-files gate): repo-graph — dominant Rust, with its ONE production cycle in a
//! TypeScript sub-tool (`tools/rgistr`) below the 10% repo share — got NO caveat, even
//! though the cycle IS TypeScript. The fact that matters is not "is the REPO materially
//! TS/JS" but "is any MEMBER of a rendered cycle TS/JS".
//!
//! On the SQLite MODULE-cycle routes the member languages ARE reachable (the tracked-files
//! table carries per-file `language`, and `module_qualified_names` carries every module's
//! directory identity), so those routes gate on membership via
//! [`any_cycle_member_is_ts_js`]. On routes where member languages are NOT carried (the
//! LiveGraph fastpath / file-import / module-import serves, the compare diagnostic) the
//! repo-level gate stays — the slice states nothing new there.
//!
//! # Membership definition (deepest-module ownership, not loose subtree)
//!
//! A cycle member is a MODULE (a repo-relative directory path, e.g. `tools/rgistr/src`). A
//! file "belongs to" a cycle member iff its DEEPEST containing module (the longest module
//! directory that is a path-prefix of the file) is that member. Deepest-ownership — over the
//! full module set, not just the cycle members — is what keeps a nested non-cycle module's
//! TS file from being wrongly attributed to a cycle-member ancestor (VISION: precision
//! matters for cycles). A file whose deepest owner is a cycle member AND whose language is in
//! the TS/JS family flips the caveat.
//!
//! # Abstraction one-liner
//!
//! `rendered_cycle_member_dirs` + `any_cycle_member_is_ts_js` — pure helpers deriving the
//! per-membership caveat; two callers (`serve_cycles_sqlite`, the forced `--engine sqlite`
//! `handle_cycles`); axis = none (composes the existing TS/JS token vocabulary
//! [`crate::reader_context::is_ts_js_language_token`] × path-ownership); rejected simpler =
//! inline the membership walk at both SQLite call sites — rejected because it would duplicate
//! the ownership logic and the JSON member extraction across two files.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::reader_context::is_ts_js_language_token;

/// The qualified-name (module directory) set of the rendered cycles — the `qualified_name` of
/// every node across all cycles in the canonical output. This is the cycle-MEMBER universe the
/// membership caveat tests file ownership against.
pub(crate) fn rendered_cycle_member_dirs(canonical_cycles: &[Value]) -> BTreeSet<String> {
    let mut members = BTreeSet::new();
    for cycle in canonical_cycles {
        let Some(nodes) = cycle.get("nodes").and_then(|n| n.as_array()) else {
            continue;
        };
        for node in nodes {
            if let Some(q) = node.get("qualified_name").and_then(|q| q.as_str()) {
                members.insert(q.to_string());
            }
        }
    }
    members
}

/// Is `path` inside module directory `dir` (equal to it, or under it on a path-segment
/// boundary)? The empty string is the repo-root module and contains every path.
fn path_in_module(path: &str, dir: &str) -> bool {
    dir.is_empty() || path == dir || path.starts_with(&format!("{dir}/"))
}

/// ZEROSTATE-SCOPE-1 §2.3: true iff ≥1 TS/JS file's DEEPEST-owning module is a member of any
/// rendered cycle. `member_dirs` is [`rendered_cycle_member_dirs`]; `all_module_dirs` is every
/// module's qualified directory (for deepest-ownership resolution — MUST be a superset of
/// `member_dirs`); `files` is `(path, language_token)` from the tracked-files table (a file
/// with `language == None` is simply never TS/JS).
///
/// An empty `member_dirs` (no rendered cycles) is `false` by construction — the caller also
/// guards on `count > 0`, so this is the "no cycles, no caveat" floor either way.
pub(crate) fn any_cycle_member_is_ts_js(
    member_dirs: &BTreeSet<String>,
    all_module_dirs: &[String],
    files: &[(&str, Option<&str>)],
) -> bool {
    if member_dirs.is_empty() {
        return false;
    }
    for (path, lang) in files {
        // Only TS/JS files can flip the caveat; skip everything else (incl. `language == None`).
        if !lang.is_some_and(is_ts_js_language_token) {
            continue;
        }
        // Deepest-owning module = the longest module dir that contains this file.
        let owner = all_module_dirs
            .iter()
            .filter(|d| path_in_module(path, d))
            .max_by_key(|d| d.len());
        if let Some(owner) = owner {
            if member_dirs.contains(owner.as_str()) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cyc(members: &[&str]) -> Value {
        json!({
            "nodes": members
                .iter()
                .map(|m| json!({ "qualified_name": m }))
                .collect::<Vec<_>>(),
        })
    }

    #[test]
    fn extracts_member_dirs_from_canonical_cycles() {
        let cycles = vec![cyc(&["a/x", "a/y"]), cyc(&["b/z"])];
        let members = rendered_cycle_member_dirs(&cycles);
        assert_eq!(
            members,
            ["a/x", "a/y", "b/z"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        );
    }

    /// The repo-graph shape §4 demands: dominant Rust (below-the-repo-gate TS share) with ONE
    /// TypeScript cycle. Repo-level materiality would say "not material TS" → no caveat; the
    /// membership gate flips it TRUE because the cycle's member module is TypeScript.
    #[test]
    fn dominant_rust_with_one_ts_cycle_flips_the_caveat() {
        let members = rendered_cycle_member_dirs(&[cyc(&["tools/rgistr/src", "tools/rgistr/lib"])]);
        let all_modules = vec![
            "src".to_string(),
            "tools/rgistr/src".to_string(),
            "tools/rgistr/lib".to_string(),
        ];
        // A big Rust core + a small TS cycle sub-tool.
        let files = vec![
            ("src/main.rs", Some("rust")),
            ("src/lib.rs", Some("rust")),
            ("tools/rgistr/src/index.ts", Some("typescript")),
            ("tools/rgistr/lib/util.ts", Some("typescript")),
        ];
        assert!(
            any_cycle_member_is_ts_js(&members, &all_modules, &files),
            "a TS cycle member must flip the caveat even on a dominant-Rust repo"
        );
    }

    #[test]
    fn pure_rust_cycle_does_not_flip() {
        let members = rendered_cycle_member_dirs(&[cyc(&["crates/a", "crates/b"])]);
        let all_modules = vec!["crates/a".to_string(), "crates/b".to_string()];
        let files = vec![
            ("crates/a/lib.rs", Some("rust")),
            ("crates/b/lib.rs", Some("rust")),
        ];
        assert!(!any_cycle_member_is_ts_js(&members, &all_modules, &files));
    }

    /// A TS file in a NESTED module that is NOT part of the cycle must not flip the caveat for
    /// its cycle-member ANCESTOR — deepest-ownership, not loose subtree matching.
    #[test]
    fn nested_non_cycle_ts_module_does_not_leak_to_ancestor() {
        let members = rendered_cycle_member_dirs(&[cyc(&["src", "src/core"])]);
        // `src/web` is a real module, NOT in the cycle. Its TS file belongs to it, not `src`.
        let all_modules = vec![
            "src".to_string(),
            "src/core".to_string(),
            "src/web".to_string(),
        ];
        let files = vec![
            ("src/main.rs", Some("rust")),
            ("src/core/mod.rs", Some("rust")),
            ("src/web/app.ts", Some("typescript")),
        ];
        assert!(
            !any_cycle_member_is_ts_js(&members, &all_modules, &files),
            "the TS file's deepest owner is the non-cycle `src/web`, so no caveat"
        );
    }

    #[test]
    fn js_family_tokens_all_count() {
        let members = rendered_cycle_member_dirs(&[cyc(&["pkg"])]);
        let all_modules = vec!["pkg".to_string()];
        for tok in ["typescript", "tsx", "javascript", "jsx"] {
            assert!(any_cycle_member_is_ts_js(
                &members,
                &all_modules,
                &[("pkg/f", Some(tok))]
            ));
        }
        // A non-TS/JS language and a missing language never flip it.
        assert!(!any_cycle_member_is_ts_js(
            &members,
            &all_modules,
            &[("pkg/f", Some("python")), ("pkg/g", None)]
        ));
    }

    #[test]
    fn no_cycles_means_no_caveat() {
        let members = rendered_cycle_member_dirs(&[]);
        assert!(!any_cycle_member_is_ts_js(
            &members,
            &["pkg".to_string()],
            &[("pkg/f.ts", Some("typescript"))]
        ));
    }
}
