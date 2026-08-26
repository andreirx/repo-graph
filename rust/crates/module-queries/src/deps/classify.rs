//! Observed-import classification gate (DEPS-LIST-REWRITE-1 §2.1).
//!
//! The `observed_external_imports` fed to `reconcile` originate in `unresolved_edges`
//! (CALL targets), after identifier→specifier binding resolution in `compose`. Bound
//! identifiers arrive as real import specifiers (`"react"`, `"@scope/pkg"`); UNBOUND call
//! targets arrive as raw call-expression text (`"Object.values(allNodes).filter(...)"`,
//! `"Math.sqrt"`, `"StringBuilder"`). This module is the single gate that keeps
//! call-expression text OUT of the package namespace:
//!
//! - import-specifier-shaped values become candidate packages;
//! - language builtins / globals / stdlib prefixes classify as builtins (never packages);
//! - everything else is a non-specifier — dropped from attribution and counted so the
//!   caller can report it honestly instead of hoisting it into `observed_but_undeclared`.
//!
//! Crate-private. Current users (both direct): `reconcile.rs` (classifies each observed import
//! into builtin/package/local/non-specifier) and `compose.rs` (`admit_observed` gates the same
//! classification into the module's import vs. rejected buckets). Extracted (rather than inlined)
//! because the shape rules carry their own rejection matrix that must be unit-tested in isolation,
//! and because `reconcile.rs` is near the 500-line guardrail. Axis of variation: none imagined —
//! this is a plain factoring for test-seam + file-size, not an abstraction.

use std::collections::HashSet;

use super::normalize::{
    is_local_specifier, normalize_cargo_specifier, normalize_java_specifier,
    normalize_npm_specifier, normalize_python_specifier,
};

/// How a single observed external reference classifies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservedKind {
    /// A language builtin / global / stdlib prefix (ES `Map`/`Math`, Python `os`/`asyncio`,
    /// Java `java.util`). `name` is the matched builtin token (reader-facing).
    Builtin { name: String },
    /// A genuine import specifier that normalized to this package name.
    Package { package: String },
    /// Local / relative import — not external, ignored by the caller.
    Local,
    /// Call-expression text or other non-import token — never a package, never a builtin.
    NonSpecifier,
}

/// True when `s` is shaped like an import specifier: no interior whitespace and no
/// call/expression punctuation. Dots, slashes, `@`, `-`, and `::` ARE allowed — scoped
/// packages (`@scope/pkg`), subpaths (`lodash/get`), Rust paths (`tokio::spawn`), and
/// dotted names (`socket.io`, `os.path`, `java.util.List`). Parens, brackets, braces,
/// operators, quotes, and whitespace mark call-expression or other code text, which an
/// import specifier never contains.
fn is_specifier_shaped(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    !s.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '(' | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '<'
                    | '>'
                    | '='
                    | ';'
                    | ','
                    | '!'
                    | '?'
                    | '&'
                    | '|'
                    | '+'
                    | '*'
                    | '%'
                    | '^'
                    | '~'
                    | '"'
                    | '\''
                    | '`'
                    | '\\'
            )
    })
}

/// If `raw`/`package` names a builtin, return the matched builtin token.
///
/// Matches (against the injected flat set of identifiers ∪ module specifiers ∪ stdlib
/// prefixes) in this order: the full raw string, the normalized package, then each leading
/// dot-prefix of `raw` (`java.util.List` → `java`, `java.util`, `java.util.List`; `Math.sqrt`
/// → `Math`; `os.path` → `os`). Progressive dot-prefixing lets one flat set cover bare
/// identifiers, dotted stdlib modules, and Java package prefixes without per-ecosystem branches.
fn matches_builtin(raw: &str, package: &str, builtins: &HashSet<String>) -> Option<String> {
    if builtins.contains(raw) {
        return Some(raw.to_string());
    }
    if builtins.contains(package) {
        return Some(package.to_string());
    }
    let mut acc = String::new();
    for seg in raw.split('.') {
        if !acc.is_empty() {
            acc.push('.');
        }
        acc.push_str(seg);
        if builtins.contains(&acc) {
            return Some(acc);
        }
    }
    None
}

/// Classify one observed external reference. `builtins` is the injected, ecosystem-appropriate
/// set (identifiers ∪ module specifiers ∪ stdlib prefixes). `ecosystem` selects the normalizer.
pub(crate) fn classify_observed(
    raw: &str,
    ecosystem: &str,
    builtins: &HashSet<String>,
) -> ObservedKind {
    if is_local_specifier(raw) {
        return ObservedKind::Local;
    }

    // Call-expression text / code fragments never reach the package namespace.
    if !is_specifier_shaped(raw) {
        return ObservedKind::NonSpecifier;
    }

    let package = match ecosystem {
        "cargo" => normalize_cargo_specifier(raw),
        "python" => normalize_python_specifier(raw),
        "java" => normalize_java_specifier(raw),
        _ => normalize_npm_specifier(raw),
    };

    // Rust crate-relative paths that survived normalization are local, not external.
    if package.starts_with("crate::")
        || package.starts_with("self::")
        || package.starts_with("super::")
    {
        return ObservedKind::Local;
    }

    // Rust prelude crates are always builtins regardless of the injected set (the
    // language guarantees them; `is_runtime_builtin` enforced the same intrinsic).
    if ecosystem == "cargo" && matches!(package.as_str(), "std" | "core" | "alloc") {
        return ObservedKind::Builtin { name: package };
    }

    if let Some(name) = matches_builtin(raw, &package, builtins) {
        return ObservedKind::Builtin { name };
    }

    ObservedKind::Package { package }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ── the rejection matrix: fragment / chain / whitespace, per language ──

    #[test]
    fn rejects_call_expression_text() {
        // FRAKTAG's real regression: a whole call expression hoisted into the bucket.
        let raw = "Object.values(allNodes)\n .filter(n => n.kind).sort";
        assert_eq!(
            classify_observed(raw, "npm", &builtins(&[])),
            ObservedKind::NonSpecifier
        );
    }

    #[test]
    fn rejects_parens_and_whitespace_and_operators() {
        for raw in [
            "express()",
            "foo(bar)",
            "a + b",
            "new Foo",
            "x = y",
            "arr[0]",
            "obj.method()",
            "a && b",
        ] {
            assert_eq!(
                classify_observed(raw, "npm", &builtins(&[])),
                ObservedKind::NonSpecifier,
                "should reject: {raw}"
            );
        }
    }

    #[test]
    fn ts_globals_classify_as_builtins_not_packages() {
        let b = builtins(&["Map", "Set", "Promise", "Math", "Object"]);
        for (raw, name) in [
            ("Map", "Map"),
            ("Set", "Set"),
            ("Promise", "Promise"),
            ("Math.sqrt", "Math"), // dotted, no parens → head prefix matches
        ] {
            assert_eq!(
                classify_observed(raw, "npm", &b),
                ObservedKind::Builtin {
                    name: name.to_string()
                },
                "expected builtin for {raw}"
            );
        }
    }

    #[test]
    fn java_stdlib_prefix_classifies_as_builtin() {
        let b = builtins(&[
            "java.util",
            "java.io",
            "StringBuilder",
            "IllegalArgumentException",
        ]);
        assert_eq!(
            classify_observed("java.util.List", "java", &b),
            ObservedKind::Builtin {
                name: "java.util".to_string()
            }
        );
        assert_eq!(
            classify_observed("StringBuilder", "java", &b),
            ObservedKind::Builtin {
                name: "StringBuilder".to_string()
            }
        );
    }

    #[test]
    fn python_stdlib_classifies_as_builtin() {
        let b = builtins(&["asyncio", "os", "AssertionError"]);
        assert_eq!(
            classify_observed("asyncio", "python", &b),
            ObservedKind::Builtin {
                name: "asyncio".to_string()
            }
        );
        assert_eq!(
            classify_observed("os.path", "python", &b),
            ObservedKind::Builtin {
                name: "os".to_string()
            }
        );
    }

    #[test]
    fn real_packages_survive_including_dotted_and_scoped() {
        let b = builtins(&["Map", "Math"]);
        assert_eq!(
            classify_observed("react", "npm", &b),
            ObservedKind::Package {
                package: "react".to_string()
            }
        );
        assert_eq!(
            classify_observed("@fraktag/engine", "npm", &b),
            ObservedKind::Package {
                package: "@fraktag/engine".to_string()
            }
        );
        // socket.io is a real package with a dot; must NOT be rejected as a chain.
        assert_eq!(
            classify_observed("socket.io", "npm", &b),
            ObservedKind::Package {
                package: "socket.io".to_string()
            }
        );
        assert_eq!(
            classify_observed("lodash/get", "npm", &b),
            ObservedKind::Package {
                package: "lodash".to_string()
            }
        );
    }

    #[test]
    fn cargo_paths_normalize_and_local_dropped() {
        let b = builtins(&["std"]);
        assert_eq!(
            classify_observed("tokio::spawn", "cargo", &b),
            ObservedKind::Package {
                package: "tokio".to_string()
            }
        );
        assert_eq!(
            classify_observed("std::collections::HashMap", "cargo", &b),
            ObservedKind::Builtin {
                name: "std".to_string()
            }
        );
        assert_eq!(
            classify_observed("crate::utils", "cargo", &b),
            ObservedKind::Local
        );
    }

    #[test]
    fn local_specifiers_are_local() {
        let b = builtins(&[]);
        for raw in ["./utils", "../shared", "/abs/path"] {
            assert_eq!(classify_observed(raw, "npm", &b), ObservedKind::Local);
        }
    }
}
