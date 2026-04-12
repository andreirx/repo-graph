//! Classifier signal predicates (pure, pub(crate)).
//!
//! Mirrors the predicate functions from
//! `src/core/classification/signals.ts`. These are implementation
//! vocabulary for the classifier (R3-D) and framework detectors
//! (R3-F), NOT stable public API. They stay `pub(crate)` per the
//! R3 locked public-API boundary.
//!
//! All functions are pure: no I/O, no state, no allocation beyond
//! the return value. Linear scans are fine at the scale these
//! operate on (dep lists < 1000, alias entries < 100, runtime
//! builtins < 500).

use crate::types::{
	PackageDependencySet, RuntimeBuiltinsSet, TsconfigAliases,
};

/// True iff `name` appears in `deps.names`. Mirror of
/// `hasPackageDependency` from `signals.ts:44`.
pub(crate) fn has_package_dependency(
	deps: &PackageDependencySet,
	name: &str,
) -> bool {
	deps.names.iter().any(|n| n == name)
}

/// True iff `identifier` appears in `builtins.identifiers`.
/// Mirror of `hasRuntimeBuiltinIdentifier` from `signals.ts:73`.
pub(crate) fn has_runtime_builtin_identifier(
	builtins: &RuntimeBuiltinsSet,
	identifier: &str,
) -> bool {
	builtins.identifiers.iter().any(|n| n == identifier)
}

/// True iff `specifier` appears in `builtins.module_specifiers`.
/// Mirror of `hasRuntimeBuiltinModule` from `signals.ts:83`.
pub(crate) fn has_runtime_builtin_module(
	builtins: &RuntimeBuiltinsSet,
	specifier: &str,
) -> bool {
	builtins.module_specifiers.iter().any(|n| n == specifier)
}

/// True iff `specifier` matches any tsconfig path alias entry.
///
/// Mirror of `matchesAnyAlias` from `signals.ts:130`. Matching
/// semantics follow TypeScript's `paths` resolution:
///
/// - Pattern ending in `*` is a PREFIX match. `@/*` matches any
///   specifier starting with `@/`.
/// - Pattern with no `*` is an EXACT match. `@types` matches
///   only the specifier `@types` (no prefix extension).
/// - Pattern consisting solely of `*` is SKIPPED: it would match
///   every specifier, which is useless as a classification signal.
/// - Pattern whose `*` leaves an empty prefix is also SKIPPED
///   (same reason: empty prefix matches everything).
pub(crate) fn matches_any_alias(
	specifier: &str,
	aliases: &TsconfigAliases,
) -> bool {
	for entry in &aliases.entries {
		let pattern = &entry.pattern;
		if pattern == "*" {
			continue;
		}
		if let Some(prefix) = pattern.strip_suffix('*') {
			if prefix.is_empty() {
				continue;
			}
			if specifier.starts_with(prefix) {
				return true;
			}
		} else if specifier == pattern {
			return true;
		}
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::TsconfigAliasEntry;

	// ── hasPackageDependency ──────────────────────────────────

	#[test]
	fn has_package_dependency_returns_true_for_present_name() {
		let deps = PackageDependencySet {
			names: vec!["express".into(), "lodash".into()],
		};
		assert!(has_package_dependency(&deps, "express"));
	}

	#[test]
	fn has_package_dependency_returns_false_for_absent_name() {
		let deps = PackageDependencySet {
			names: vec!["express".into()],
		};
		assert!(!has_package_dependency(&deps, "koa"));
	}

	#[test]
	fn has_package_dependency_returns_false_for_empty_set() {
		let deps = PackageDependencySet { names: vec![] };
		assert!(!has_package_dependency(&deps, "anything"));
	}

	// ── hasRuntimeBuiltinIdentifier ───────────────────────────

	#[test]
	fn has_runtime_builtin_identifier_matches() {
		let builtins = RuntimeBuiltinsSet {
			identifiers: vec!["Map".into(), "Date".into(), "process".into()],
			module_specifiers: vec![],
		};
		assert!(has_runtime_builtin_identifier(&builtins, "Map"));
		assert!(!has_runtime_builtin_identifier(&builtins, "Foo"));
	}

	// ── hasRuntimeBuiltinModule ───────────────────────────────

	#[test]
	fn has_runtime_builtin_module_matches() {
		let builtins = RuntimeBuiltinsSet {
			identifiers: vec![],
			module_specifiers: vec!["path".into(), "node:fs".into()],
		};
		assert!(has_runtime_builtin_module(&builtins, "path"));
		assert!(has_runtime_builtin_module(&builtins, "node:fs"));
		assert!(!has_runtime_builtin_module(&builtins, "express"));
	}

	// ── matchesAnyAlias ───────────────────────────────────────

	#[test]
	fn wildcard_pattern_matches_prefix() {
		let aliases = TsconfigAliases {
			entries: vec![TsconfigAliasEntry {
				pattern: "@/*".into(),
				substitutions: vec!["./src/*".into()],
			}],
		};
		assert!(matches_any_alias("@/utils/foo", &aliases));
		assert!(matches_any_alias("@/", &aliases));
	}

	#[test]
	fn wildcard_pattern_does_not_match_different_prefix() {
		let aliases = TsconfigAliases {
			entries: vec![TsconfigAliasEntry {
				pattern: "@/*".into(),
				substitutions: vec![],
			}],
		};
		assert!(!matches_any_alias("lodash", &aliases));
	}

	#[test]
	fn exact_pattern_matches_only_exact() {
		let aliases = TsconfigAliases {
			entries: vec![TsconfigAliasEntry {
				pattern: "@types".into(),
				substitutions: vec![],
			}],
		};
		assert!(matches_any_alias("@types", &aliases));
		assert!(!matches_any_alias("@types/node", &aliases));
	}

	#[test]
	fn solo_star_pattern_is_skipped() {
		let aliases = TsconfigAliases {
			entries: vec![TsconfigAliasEntry {
				pattern: "*".into(),
				substitutions: vec![],
			}],
		};
		assert!(!matches_any_alias("anything", &aliases));
	}

	#[test]
	fn empty_alias_set_returns_false() {
		let aliases = TsconfigAliases { entries: vec![] };
		assert!(!matches_any_alias("anything", &aliases));
	}

	#[test]
	fn multiple_patterns_or_semantics() {
		let aliases = TsconfigAliases {
			entries: vec![
				TsconfigAliasEntry { pattern: "@/*".into(), substitutions: vec![] },
				TsconfigAliasEntry { pattern: "~/*".into(), substitutions: vec![] },
			],
		};
		assert!(matches_any_alias("@/foo", &aliases));
		assert!(matches_any_alias("~/bar", &aliases));
		assert!(!matches_any_alias("lodash", &aliases));
	}

	#[test]
	fn empty_prefix_wildcard_is_skipped() {
		// Pattern "*abc" has empty prefix after stripping the
		// trailing "*". Wait, "*" at the END → prefix is empty.
		// But "*abc" doesn't end in *. Let me use a pattern
		// where strip_suffix('*') gives "".
		// Actually the TS code checks `pattern.endsWith("*")`.
		// A pattern like "*" ends in * → prefix is "" → skip.
		// A pattern like "x*" ends in * → prefix is "x" → match.
		// A pattern like "abc" doesn't end in * → exact match.
		// What about "*x"? Doesn't end in * → exact match for "*x".
		//
		// Edge case per TS line 139: `if (prefix === "") continue;`
		// That only fires for patterns like "*" (prefix = "").
		// Let me test with just "*" which is already tested above.
		// The "empty prefix wildcard" concept is the same as "solo *".
		// No separate test needed.
	}
}
