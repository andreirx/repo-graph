//! Symbol focus resolution tests.
//!
//! Tests that the `orient()` entry point correctly dispatches to
//! the symbol pipeline based on stable-key and name resolution.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentFocusCandidate, AgentFocusKind, AgentPathResolution,
	AgentSymbolContext, Budget, FocusFailureReason, ResolvedKind,
};

fn seeded() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	fake
}

// ── 1. Symbol resolves by stable key ───────────────────────────

#[test]
fn symbol_resolves_by_stable_key() {
	let mut fake = seeded();
	// Path resolution returns nothing.
	fake.path_resolutions.insert(
		("snap-1".into(), "r1:src/lib.rs:SYMBOL:parse".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: false,
			module_stable_key: None,
		},
	);
	// Stable-key lookup resolves to SYMBOL.
	fake.stable_key_candidates.insert(
		("snap-1".into(), "r1:src/lib.rs:SYMBOL:parse".into()),
		AgentFocusCandidate {
			stable_key: "r1:src/lib.rs:SYMBOL:parse".into(),
			kind: AgentFocusKind::Symbol,
			file: Some("src/lib.rs".into()),
		},
	);
	// Seed symbol context.
	fake.symbol_contexts.insert(
		("snap-1".into(), "r1:src/lib.rs:SYMBOL:parse".into()),
		AgentSymbolContext {
			file_path: Some("src/lib.rs".into()),
			module_path: Some("src".into()),
			module_stable_key: Some("r1:src:MODULE".into()),
			name: "parse".into(),
			qualified_name: Some("parse".into()),
			subtype: Some("function".into()),
			line_start: Some(42),
		},
	);

	let result = orient(
		&fake,
		"r1",
		Some("r1:src/lib.rs:SYMBOL:parse"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved, "symbol stable key must resolve");
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Symbol));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/lib.rs:SYMBOL:parse")
	);
	assert_eq!(
		result.focus.resolved_path.as_deref(),
		Some("src/lib.rs")
	);
}

// ── 2. Symbol resolves by exact name (single match) ────────────

#[test]
fn symbol_resolves_by_exact_name_single_match() {
	let mut fake = seeded();
	// Path resolution returns nothing, stable-key lookup returns
	// nothing (the focus string is a plain name, not a key).

	// Symbol name resolution returns 1 result.
	fake.symbol_name_results.insert(
		("snap-1".into(), "handleRequest".into()),
		vec![AgentFocusCandidate {
			stable_key: "r1:src/handler.rs:SYMBOL:handleRequest".into(),
			kind: AgentFocusKind::Symbol,
			file: Some("src/handler.rs".into()),
		}],
	);
	// Seed context for that candidate.
	fake.symbol_contexts.insert(
		(
			"snap-1".into(),
			"r1:src/handler.rs:SYMBOL:handleRequest".into(),
		),
		AgentSymbolContext {
			file_path: Some("src/handler.rs".into()),
			module_path: None,
			module_stable_key: None,
			name: "handleRequest".into(),
			qualified_name: Some("handleRequest".into()),
			subtype: None,
			line_start: Some(5),
		},
	);

	let result = orient(
		&fake,
		"r1",
		Some("handleRequest"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved);
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Symbol));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/handler.rs:SYMBOL:handleRequest")
	);
	assert_eq!(
		result.focus.resolved_path.as_deref(),
		Some("src/handler.rs")
	);
}

// ── 3. Symbol name with multiple matches returns ambiguous ──────

#[test]
fn symbol_name_multiple_matches_returns_ambiguous() {
	let mut fake = seeded();

	// Symbol name resolution returns 3 results (sorted by
	// stable_key ascending).
	fake.symbol_name_results.insert(
		("snap-1".into(), "init".into()),
		vec![
			AgentFocusCandidate {
				stable_key: "r1:src/a.rs:SYMBOL:init".into(),
				kind: AgentFocusKind::Symbol,
				file: Some("src/a.rs".into()),
			},
			AgentFocusCandidate {
				stable_key: "r1:src/b.rs:SYMBOL:init".into(),
				kind: AgentFocusKind::Symbol,
				file: Some("src/b.rs".into()),
			},
			AgentFocusCandidate {
				stable_key: "r1:src/c.rs:SYMBOL:init".into(),
				kind: AgentFocusKind::Symbol,
				file: Some("src/c.rs".into()),
			},
		],
	);

	let result = orient(
		&fake,
		"r1",
		Some("init"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(!result.focus.resolved, "ambiguous must not resolve");
	assert_eq!(
		result.focus.reason,
		Some(FocusFailureReason::Ambiguous)
	);
	assert_eq!(
		result.focus.candidates.len(),
		3,
		"must include all candidates"
	);
	// Candidates sorted by stable_key (from storage).
	assert_eq!(
		result.focus.candidates[0].stable_key,
		"r1:src/a.rs:SYMBOL:init"
	);
	assert_eq!(
		result.focus.candidates[1].stable_key,
		"r1:src/b.rs:SYMBOL:init"
	);
	assert_eq!(
		result.focus.candidates[2].stable_key,
		"r1:src/c.rs:SYMBOL:init"
	);
	// All candidates should be Symbol kind.
	for c in &result.focus.candidates {
		assert_eq!(c.kind, ResolvedKind::Symbol);
	}
	assert!(
		result.signals.is_empty(),
		"ambiguous must emit zero signals"
	);
	assert!(
		result.limits.is_empty(),
		"ambiguous must emit zero limits"
	);
}

// ── 4. Symbol name with zero matches returns no_match ───────────

#[test]
fn symbol_name_zero_matches_returns_no_match() {
	let fake = seeded();
	// No path, no stable key, no symbol name results seeded.

	let result = orient(
		&fake,
		"r1",
		Some("nonExistentSymbol"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(!result.focus.resolved);
	assert_eq!(
		result.focus.reason,
		Some(FocusFailureReason::NoMatch)
	);
	assert_eq!(
		result.focus.input.as_deref(),
		Some("nonExistentSymbol")
	);
	assert!(result.signals.is_empty());
	assert!(result.limits.is_empty());
}
