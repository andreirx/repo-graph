//! Focus resolution tests.
//!
//! Tests that the `orient()` entry point correctly dispatches to
//! the file pipeline, path pipeline, or returns an error/no-match
//! based on the focus string and seeded path/stable-key resolution
//! data.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentFocusCandidate, AgentFocusKind, AgentPathResolution,
	AgentSymbolContext, Budget, ResolvedKind,
};

fn seeded() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	fake
}

// ── File resolution via path ────────────────────────────────────

#[test]
fn focus_resolves_file_when_exact_file_exists() {
	let mut fake = seeded();
	fake.path_resolutions.insert(
		("snap-1".into(), "src/core/service.ts".into()),
		AgentPathResolution {
			has_exact_file: true,
			file_stable_key: Some("r1:src/core/service.ts:FILE".into()),
			has_content_under_prefix: false,
			module_stable_key: None,
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core/service.ts"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved, "file focus must resolve");
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::File));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/core/service.ts:FILE")
	);
	assert_eq!(
		result.focus.resolved_path.as_deref(),
		Some("src/core/service.ts")
	);
}

// ── Module resolution via path prefix ───────────────────────────

#[test]
fn focus_resolves_module_when_content_under_prefix() {
	let mut fake = seeded();
	fake.path_resolutions.insert(
		("snap-1".into(), "src/core".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: true,
			module_stable_key: Some("r1:src/core:MODULE".into()),
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved);
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Module));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/core:MODULE")
	);
	assert_eq!(result.focus.resolved_path.as_deref(), Some("src/core"));
}

#[test]
fn focus_resolves_path_area_without_module_node() {
	let mut fake = seeded();
	fake.path_resolutions.insert(
		("snap-1".into(), "src/core".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: true,
			module_stable_key: None,
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved);
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Module));
	assert!(
		result.focus.resolved_key.is_none(),
		"path-area without MODULE node must not emit resolved_key"
	);
	assert_eq!(result.focus.resolved_path.as_deref(), Some("src/core"));
}

#[test]
fn focus_resolves_exact_module_node_with_no_descendant_files() {
	// Regression: an exact MODULE node exists for the path but
	// the module currently has no descendant FILE nodes (empty
	// module, or files not yet indexed). The contract says
	// "exact path match -> FILE or MODULE" — the MODULE node
	// must be found even without content under the prefix.
	// Pre-fix, this fell through to stable-key resolution and
	// then to no_match because only `has_content_under_prefix`
	// was checked.
	let mut fake = seeded();
	fake.path_resolutions.insert(
		("snap-1".into(), "src/empty_module".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: false, // no files under prefix
			module_stable_key: Some("r1:src/empty_module:MODULE".into()),
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/empty_module"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(
		result.focus.resolved,
		"MODULE node with no descendants must still resolve"
	);
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Module));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/empty_module:MODULE")
	);
	assert_eq!(
		result.focus.resolved_path.as_deref(),
		Some("src/empty_module")
	);
}

// ── Stable-key resolution ───────────────────────────────────────

#[test]
fn focus_resolves_via_stable_key_for_module() {
	let mut fake = seeded();
	// Path resolution returns nothing — no file, no content.
	fake.path_resolutions.insert(
		("snap-1".into(), "r1:src/core:MODULE".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: false,
			module_stable_key: None,
		},
	);
	// Stable key lookup succeeds.
	fake.stable_key_candidates.insert(
		("snap-1".into(), "r1:src/core:MODULE".into()),
		AgentFocusCandidate {
			stable_key: "r1:src/core:MODULE".into(),
			kind: AgentFocusKind::Module,
			file: None,
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("r1:src/core:MODULE"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved);
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Module));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/core:MODULE")
	);
	assert_eq!(result.focus.resolved_path.as_deref(), Some("src/core"));
}

// ── Symbol resolution via stable key (Rust-45) ──────────────────

#[test]
fn focus_resolves_symbol_via_stable_key() {
	let mut fake = seeded();
	fake.path_resolutions.insert(
		("snap-1".into(), "r1:src/core/service.ts:SYMBOL:myFunc".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: false,
			module_stable_key: None,
		},
	);
	fake.stable_key_candidates.insert(
		(
			"snap-1".into(),
			"r1:src/core/service.ts:SYMBOL:myFunc".into(),
		),
		AgentFocusCandidate {
			stable_key: "r1:src/core/service.ts:SYMBOL:myFunc".into(),
			kind: AgentFocusKind::Symbol,
			file: Some("src/core/service.ts".into()),
		},
	);
	// Seed symbol context so the pipeline can run.
	fake.symbol_contexts.insert(
		(
			"snap-1".into(),
			"r1:src/core/service.ts:SYMBOL:myFunc".into(),
		),
		AgentSymbolContext {
			file_path: Some("src/core/service.ts".into()),
			module_path: None,
			module_stable_key: None,
			name: "myFunc".into(),
			qualified_name: Some("myFunc".into()),
			subtype: None,
			line_start: Some(10),
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("r1:src/core/service.ts:SYMBOL:myFunc"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(result.focus.resolved, "symbol focus must resolve");
	assert_eq!(result.focus.resolved_kind, Some(ResolvedKind::Symbol));
	assert_eq!(
		result.focus.resolved_key.as_deref(),
		Some("r1:src/core/service.ts:SYMBOL:myFunc")
	);
	assert_eq!(
		result.focus.resolved_path.as_deref(),
		Some("src/core/service.ts")
	);
}

// ── No match ────────────────────────────────────────────────────

#[test]
fn focus_returns_no_match_for_unknown_path() {
	let fake = seeded();
	// No seeds — nothing resolves.
	let result = orient(
		&fake,
		"r1",
		Some("nonexistent/path"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(!result.focus.resolved, "unknown path must not resolve");
	assert!(result.focus.resolved_kind.is_none());
	assert!(result.focus.resolved_key.is_none());
	assert!(result.focus.resolved_path.is_none());
	assert_eq!(
		result.focus.reason,
		Some(repo_graph_agent::FocusFailureReason::NoMatch)
	);
	assert_eq!(result.focus.input.as_deref(), Some("nonexistent/path"));
	assert!(result.signals.is_empty(), "no-match must emit zero signals");
	assert!(result.limits.is_empty(), "no-match must emit zero limits");
}
