//! TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04 workspace-member clause; RG-REQ-002-L11): the pure
//! match between a bare TS/JS import specifier and the repository's own npm workspace members.
//!
//! A specifier that names exactly one workspace member, whose manifest declares a root entry
//! explicitly and none of whose declared targets is indexed (build output such as `dist/` is not),
//! and which has exactly one indexed source entry (`<root>/src/index.{ts,…}`), is BOUND to that
//! source entry — as an INFERRED fact: the index cannot show that the package's build maps the
//! source entry to the declared one. Several source entries are AMBIGUOUS (recorded, never
//! picked). Every other case is NOT BOUND and the import stays as it was.
//!
//! - what: the decision `(specifier, workspace members, is_indexed) → outcome`, and the lookup the
//!   orchestrator builds once per index from the declared-module catalog.
//! - concrete current users: built by `orchestrator` (beside `rust_crate_roots`); read by the
//!   resolver's trailing TS-only import stage (`resolver::resolve_workspace_package_import`).
//! - force: `resolver.rs` is far over the 500-line guardrail; the rule is one cohesive decision.
//! - rejected simpler: inlining into `resolver.rs` (the guardrail) or a trait (one producer, one
//!   consumer — a plain map and a function suffice).

use std::collections::HashMap;

use crate::types::{DeclaredModule, NpmDeclaredEntry};

/// The `basis` an IMPORTS edge bound to a workspace member's source entry carries — the reason it
/// is `inferred`, not `static`.
pub const WORKSPACE_SOURCE_ENTRY_BASIS: &str = "workspace_source_entry";

/// The `basis` an unresolved IMPORTS row carries when the member has several indexed source
/// entries (every one recorded, none picked).
pub const AMBIGUOUS_WORKSPACE_SOURCE_ENTRY_BASIS: &str = "ambiguous_workspace_source_entry";

/// The extension variants of `src/index.ts` that count as a member's source entry.
const SOURCE_ENTRY_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The completions Node tries for a `main` target (`<target>.js`, `.json`, `.node`,
/// `<target>/index.js`). A [`NpmDeclaredEntry::Main`] target counts as indexed when it, or one of
/// these, is indexed; an [`NpmDeclaredEntry::Export`] target only when it is indexed as written
/// (Node resolves an `exports` target literally) — D-TWR-ENTRY-ORIGIN.
const MAIN_COMPLETIONS: [&str; 4] = [".js", ".json", ".node", "/index.js"];

/// One npm workspace member, as the import stage needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmWorkspacePackage {
    /// The member's root directory, repo-relative (`.` = the repository root).
    pub root: String,
    /// Every root target the member's manifest declares explicitly, repo-relative, in manifest
    /// order, with its origin (`package_json::declared_entries`); empty when none is declared or
    /// determinable.
    pub declared_entries: Vec<NpmDeclaredEntry>,
}

/// Package name → every workspace member that declares that name (more than one = no binding).
pub type NpmWorkspacePackages = HashMap<String, Vec<NpmWorkspacePackage>>;

/// Build the member lookup from the declared-module catalog: `"npm"` entries only; a member listed
/// twice under the same root (two workspace patterns matching it) counts once.
pub fn build_npm_workspace_packages(declared_modules: &[DeclaredModule]) -> NpmWorkspacePackages {
    let mut packages: NpmWorkspacePackages = HashMap::new();
    for m in declared_modules.iter().filter(|m| m.ecosystem == "npm") {
        let members = packages.entry(m.name.clone()).or_default();
        if members.iter().any(|p| p.root == m.canonical_root) {
            continue;
        }
        members.push(NpmWorkspacePackage {
            root: m.canonical_root.clone(),
            declared_entries: m.npm_declared_entries.clone(),
        });
    }
    packages
}

/// The outcome of matching one import specifier against the workspace members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceImportMatch {
    /// Bind INFERRED to `source_entry`; `declared_entries` (paths, manifest order) are the other
    /// candidates.
    Bound {
        source_entry: String,
        declared_entries: Vec<String>,
    },
    /// Several indexed source entries (sorted), none picked; the row stays unresolved.
    Ambiguous {
        source_entries: Vec<String>,
        declared_entries: Vec<String>,
    },
    /// The rule does not apply; the import stays exactly as it was.
    NotBound,
}

/// Decide what the workspace rule says about `specifier`. `is_indexed(path)` answers whether a
/// repo-relative path is an indexed file. PURE.
pub fn match_workspace_package_import(
    specifier: &str,
    packages: &NpmWorkspacePackages,
    is_indexed: impl Fn(&str) -> bool,
) -> WorkspaceImportMatch {
    // A relative, absolute or scheme-qualified specifier never names a package; a subpath
    // (`@scope/pkg/sub`, `pkg/sub`) never equals a member name, so the exact lookup declines it.
    if specifier.is_empty()
        || specifier.starts_with('.')
        || specifier.starts_with('/')
        || specifier.contains(':')
    {
        return WorkspaceImportMatch::NotBound;
    }
    let member = match packages.get(specifier).map(Vec::as_slice) {
        Some([one]) => one,
        // No member of that name, or a name two members declare.
        _ => return WorkspaceImportMatch::NotBound,
    };
    if member.declared_entries.is_empty() {
        return WorkspaceImportMatch::NotBound;
    }
    let declared_indexed = member.declared_entries.iter().any(|entry| match entry {
        NpmDeclaredEntry::Main(target) => {
            is_indexed(target)
                || MAIN_COMPLETIONS
                    .iter()
                    .any(|completion| is_indexed(&format!("{target}{completion}")))
        }
        NpmDeclaredEntry::Export(target) => is_indexed(target),
    });
    if declared_indexed {
        return WorkspaceImportMatch::NotBound;
    }
    let prefix = if member.root == "." {
        "src/index.".to_string()
    } else {
        format!("{}/src/index.", member.root)
    };
    let mut source_entries: Vec<String> = SOURCE_ENTRY_EXTENSIONS
        .iter()
        .map(|ext| format!("{prefix}{ext}"))
        .filter(|path| is_indexed(path))
        .collect();
    source_entries.sort();
    let declared_paths = || -> Vec<String> {
        member
            .declared_entries
            .iter()
            .map(|entry| entry.path().to_string())
            .collect()
    };
    match source_entries.len() {
        0 => WorkspaceImportMatch::NotBound,
        1 => WorkspaceImportMatch::Bound {
            source_entry: source_entries.remove(0),
            declared_entries: declared_paths(),
        },
        _ => WorkspaceImportMatch::Ambiguous {
            source_entries,
            declared_entries: declared_paths(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A member whose declared entries come from `main` (Node completes them).
    fn npm(name: &str, root: &str, entries: &[&str]) -> DeclaredModule {
        member(name, root, entries, |p| {
            NpmDeclaredEntry::Main(p.to_string())
        })
    }

    /// A member whose declared entries are root `exports` targets (matched as written).
    fn npm_exports(name: &str, root: &str, entries: &[&str]) -> DeclaredModule {
        member(name, root, entries, |p| {
            NpmDeclaredEntry::Export(p.to_string())
        })
    }

    fn member(
        name: &str,
        root: &str,
        entries: &[&str],
        origin: impl Fn(&str) -> NpmDeclaredEntry,
    ) -> DeclaredModule {
        DeclaredModule {
            ecosystem: "npm".to_string(),
            name: name.to_string(),
            canonical_root: root.to_string(),
            npm_declared_entries: entries.iter().map(|e| origin(e)).collect(),
        }
    }

    fn indexed(paths: &[&str]) -> impl Fn(&str) -> bool {
        let set: HashSet<String> = paths.iter().map(|p| p.to_string()).collect();
        move |p: &str| set.contains(p)
    }

    /// FRAKTAG's shape: `@fraktag/engine` at `packages/engine`, `main: dist/index.js`.
    fn fraktag() -> NpmWorkspacePackages {
        build_npm_workspace_packages(&[npm(
            "@fraktag/engine",
            "packages/engine",
            &["packages/engine/dist/index.js"],
        )])
    }

    #[test]
    fn bound_with_every_declared_entry_when_one_source_entry_is_indexed_and_no_declared_entry_is() {
        let files = indexed(&["packages/engine/src/index.ts", "packages/api/src/server.ts"]);
        assert_eq!(
            match_workspace_package_import("@fraktag/engine", &fraktag(), &files),
            WorkspaceImportMatch::Bound {
                source_entry: "packages/engine/src/index.ts".to_string(),
                declared_entries: vec!["packages/engine/dist/index.js".to_string()],
            }
        );
        // Several declared targets are all kept, in order; an extension variant is a source entry.
        let packages = build_npm_workspace_packages(&[npm_exports(
            "@amodx/effects",
            "packages/effects",
            &[
                "packages/effects/dist/index.d.ts",
                "packages/effects/dist/index.js",
            ],
        )]);
        assert_eq!(
            match_workspace_package_import(
                "@amodx/effects",
                &packages,
                indexed(&["packages/effects/src/index.tsx"])
            ),
            WorkspaceImportMatch::Bound {
                source_entry: "packages/effects/src/index.tsx".to_string(),
                declared_entries: vec![
                    "packages/effects/dist/index.d.ts".to_string(),
                    "packages/effects/dist/index.js".to_string(),
                ],
            }
        );
        // A member at the repository root.
        let packages = build_npm_workspace_packages(&[npm("root-pkg", ".", &["dist/main.js"])]);
        assert_eq!(
            match_workspace_package_import("root-pkg", &packages, indexed(&["src/index.mjs"])),
            WorkspaceImportMatch::Bound {
                source_entry: "src/index.mjs".to_string(),
                declared_entries: vec!["dist/main.js".to_string()],
            }
        );
    }

    #[test]
    fn declined_when_any_declared_entry_is_indexed() {
        let src = "packages/engine/src/index.ts";
        // The exact declared target.
        assert_eq!(
            match_workspace_package_import(
                "@fraktag/engine",
                &fraktag(),
                indexed(&[src, "packages/engine/dist/index.js"])
            ),
            WorkspaceImportMatch::NotBound
        );
        // A Node completion of a declared target.
        for completion in [
            "packages/engine/dist/index.js.js",
            "packages/engine/dist/index.js.json",
            "packages/engine/dist/index.js.node",
            "packages/engine/dist/index.js/index.js",
        ] {
            assert_eq!(
                match_workspace_package_import(
                    "@fraktag/engine",
                    &fraktag(),
                    indexed(&[src, completion])
                ),
                WorkspaceImportMatch::NotBound,
                "{completion}"
            );
        }
        // Any one of several declared targets (storybook's `code: ./src/index.ts` condition).
        let packages = build_npm_workspace_packages(&[npm_exports(
            "@storybook/addon-links",
            "code/addons/links",
            &[
                "code/addons/links/dist/index.d.ts",
                "code/addons/links/src/index.ts",
                "code/addons/links/dist/index.js",
            ],
        )]);
        assert_eq!(
            match_workspace_package_import(
                "@storybook/addon-links",
                &packages,
                indexed(&["code/addons/links/src/index.ts"])
            ),
            WorkspaceImportMatch::NotBound
        );
    }

    #[test]
    fn declined_when_the_member_declares_no_entry() {
        let packages = build_npm_workspace_packages(&[npm("renderer", "renderer", &[])]);
        assert_eq!(
            match_workspace_package_import(
                "renderer",
                &packages,
                indexed(&["renderer/src/index.ts"])
            ),
            WorkspaceImportMatch::NotBound
        );
    }

    #[test]
    fn declined_without_an_indexed_source_entry() {
        // FRAKTAG `@fraktag/ui`: no `src/index.*`. A source file elsewhere is not a source entry.
        let packages = build_npm_workspace_packages(&[npm(
            "@fraktag/ui",
            "packages/ui",
            &["packages/ui/dist/index.js"],
        )]);
        assert_eq!(
            match_workspace_package_import(
                "@fraktag/ui",
                &packages,
                indexed(&["packages/ui/src/App.tsx", "packages/ui/index.ts"])
            ),
            WorkspaceImportMatch::NotBound
        );
    }

    #[test]
    fn ambiguous_when_several_source_entries_are_indexed() {
        assert_eq!(
            match_workspace_package_import(
                "@fraktag/engine",
                &fraktag(),
                indexed(&[
                    "packages/engine/src/index.ts",
                    "packages/engine/src/index.js"
                ])
            ),
            WorkspaceImportMatch::Ambiguous {
                source_entries: vec![
                    "packages/engine/src/index.js".to_string(),
                    "packages/engine/src/index.ts".to_string(),
                ],
                declared_entries: vec!["packages/engine/dist/index.js".to_string()],
            }
        );
    }

    #[test]
    fn declined_for_a_name_two_members_declare() {
        let packages = build_npm_workspace_packages(&[
            npm("shared", "a/shared", &["a/shared/dist/index.js"]),
            npm("shared", "b/shared", &["b/shared/dist/index.js"]),
        ]);
        assert_eq!(packages["shared"].len(), 2);
        assert_eq!(
            match_workspace_package_import(
                "shared",
                &packages,
                indexed(&["a/shared/src/index.ts", "b/shared/src/index.ts"])
            ),
            WorkspaceImportMatch::NotBound
        );
        // The same member listed twice (two workspace patterns) is one member.
        let packages = build_npm_workspace_packages(&[
            npm("shared", "a/shared", &["a/shared/dist/index.js"]),
            npm("shared", "a/shared", &["a/shared/dist/index.js"]),
        ]);
        assert_eq!(packages["shared"].len(), 1);
        assert!(matches!(
            match_workspace_package_import(
                "shared",
                &packages,
                indexed(&["a/shared/src/index.ts"])
            ),
            WorkspaceImportMatch::Bound { .. }
        ));
    }

    #[test]
    fn declined_for_a_subpath_or_relative_specifier() {
        let files = indexed(&["packages/engine/src/index.ts"]);
        for specifier in [
            "@fraktag/engine/sub",
            "@fraktag/engine/",
            "./@fraktag/engine",
            "../engine",
            "/@fraktag/engine",
            "npm:@fraktag/engine",
            "",
            "react",
        ] {
            assert_eq!(
                match_workspace_package_import(specifier, &fraktag(), &files),
                WorkspaceImportMatch::NotBound,
                "{specifier}"
            );
        }
    }

    #[test]
    fn index_holds_npm_workspace_members_only() {
        let packages = build_npm_workspace_packages(&[
            DeclaredModule {
                ecosystem: "cargo".to_string(),
                name: "engine".to_string(),
                canonical_root: "rust/engine".to_string(),
                npm_declared_entries: vec![],
            },
            npm(
                "@fraktag/engine",
                "packages/engine",
                &["packages/engine/dist/index.js"],
            ),
        ]);
        assert_eq!(packages.len(), 1);
        assert!(
            !packages.contains_key("engine"),
            "a Cargo crate is not an npm member"
        );
        assert_eq!(
            packages["@fraktag/engine"],
            vec![NpmWorkspacePackage {
                root: "packages/engine".to_string(),
                declared_entries: vec![NpmDeclaredEntry::Main(
                    "packages/engine/dist/index.js".to_string()
                )],
            }]
        );
    }

    /// D-TWR-ENTRY-ORIGIN (the reviewer's counterexample): an `exports` root target written
    /// without an extension is resolved by Node exactly as written, so an indexed `lib/index.js`
    /// beside the declared `./lib/index` is NOT the declared target, and the import binds to the
    /// one indexed source entry with the declared path as the other candidate.
    #[test]
    fn export_target_is_matched_literally_so_its_completed_path_never_declines_the_binding() {
        let packages = build_npm_workspace_packages(&[npm_exports(
            "@acme/lib",
            "packages/lib",
            &["packages/lib/lib/index"],
        )]);
        let files = indexed(&["packages/lib/lib/index.js", "packages/lib/src/index.ts"]);
        assert_eq!(
            match_workspace_package_import("@acme/lib", &packages, &files),
            WorkspaceImportMatch::Bound {
                source_entry: "packages/lib/src/index.ts".to_string(),
                declared_entries: vec!["packages/lib/lib/index".to_string()],
            }
        );
        // Every other completion form is likewise not the export target.
        for completed in [
            "packages/lib/lib/index.json",
            "packages/lib/lib/index.node",
            "packages/lib/lib/index/index.js",
        ] {
            assert!(
                matches!(
                    match_workspace_package_import(
                        "@acme/lib",
                        &packages,
                        indexed(&[completed, "packages/lib/src/index.ts"])
                    ),
                    WorkspaceImportMatch::Bound { .. }
                ),
                "{completed}"
            );
        }
        // The target itself, indexed as written, declines the binding.
        assert_eq!(
            match_workspace_package_import(
                "@acme/lib",
                &packages,
                indexed(&["packages/lib/lib/index", "packages/lib/src/index.ts"])
            ),
            WorkspaceImportMatch::NotBound
        );
    }

    /// D-TWR-ENTRY-ORIGIN: a `main` target written without an extension is completed by Node, so
    /// an indexed `lib/index.js` IS the declared entry and the rule does not apply.
    #[test]
    fn main_target_is_completed_so_an_indexed_completion_declines_the_binding() {
        let packages = build_npm_workspace_packages(&[npm(
            "@acme/lib",
            "packages/lib",
            &["packages/lib/lib/index"],
        )]);
        for completed in [
            "packages/lib/lib/index.js",
            "packages/lib/lib/index.json",
            "packages/lib/lib/index.node",
            "packages/lib/lib/index/index.js",
        ] {
            assert_eq!(
                match_workspace_package_import(
                    "@acme/lib",
                    &packages,
                    indexed(&[completed, "packages/lib/src/index.ts"])
                ),
                WorkspaceImportMatch::NotBound,
                "{completed}"
            );
        }
        // Without an indexed completion the same member binds.
        assert_eq!(
            match_workspace_package_import(
                "@acme/lib",
                &packages,
                indexed(&["packages/lib/src/index.ts"])
            ),
            WorkspaceImportMatch::Bound {
                source_entry: "packages/lib/src/index.ts".to_string(),
                declared_entries: vec!["packages/lib/lib/index".to_string()],
            }
        );
    }
}
