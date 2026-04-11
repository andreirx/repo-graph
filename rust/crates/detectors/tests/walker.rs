//! Walker integration tests — Rust mirror of TS `walker.test.ts`.
//!
//! Tests are organized by behavior category (not by verbatim TS
//! translation). Each test proves one invariant the walker must
//! preserve. Categories:
//!
//!   1. Hookless env records
//!   2. Hooked env records
//!   3. Hookless fs records
//!   4. Hooked fs records
//!   5. Position dedup via named overlap groups
//!   6. single_match_per_line semantics
//!   7. Declaration order preservation
//!
//! The tests use synthetic in-memory detector graphs built via the
//! loader (not the production detectors.toml). This keeps walker
//! tests independent of the data file; the data file gets its own
//! parity tests in R1-H..R1-J.

use std::collections::HashSet;

use repo_graph_detectors::hooks::create_default_hook_registry;
use repo_graph_detectors::loader::load_detector_graph;
use repo_graph_detectors::types::{
	DetectorLanguage, Emitted, EnvAccessKind, EnvAccessPattern, EnvHook,
	EnvHookContext, EnvHookEmission, EnvHookEntry, EnvHookResult, EnvSuppliedField,
	FsHook, FsHookContext, FsHookEmission, FsHookEntry, FsHookResult,
	FsSuppliedField, HookRegistry, MutationKind, MutationPattern,
};
use repo_graph_detectors::walker::{walk_env_detectors, walk_fs_detectors};

// ── Test fixtures / helpers ────────────────────────────────────────

fn empty_registry() -> HookRegistry {
	HookRegistry::default()
}

/// Build a registry with a single env hook under the given name.
fn env_registry_with(
	name: &'static str,
	func: EnvHook,
	supplies: &[EnvSuppliedField],
) -> HookRegistry {
	let mut r = HookRegistry::default();
	let supplies_set: HashSet<EnvSuppliedField> = supplies.iter().copied().collect();
	r.env.insert(
		name,
		EnvHookEntry {
			func,
			supplies: supplies_set,
		},
	);
	r
}

fn fs_registry_with(
	name: &'static str,
	func: FsHook,
	supplies: &[FsSuppliedField],
) -> HookRegistry {
	let mut r = HookRegistry::default();
	let supplies_set: HashSet<FsSuppliedField> = supplies.iter().copied().collect();
	r.fs.insert(
		name,
		FsHookEntry {
			func,
			supplies: supplies_set,
		},
	);
	r
}

// ──────────────────────────────────────────────────────────────────
// 1. Hookless env records
// ──────────────────────────────────────────────────────────────────

#[test]
fn hookless_env_emits_one_record_with_walker_default_varname() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\.env\.([A-Z_][A-Z0-9_]*)'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		"const port = process.env.PORT;",
		"src/foo.ts",
	);
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.var_name, "PORT");
	assert_eq!(r.access_kind, EnvAccessKind::Required);
	assert_eq!(r.access_pattern, EnvAccessPattern::ProcessEnvDot);
	assert_eq!(r.file_path, "src/foo.ts");
	assert_eq!(r.line_number, 1);
	assert_eq!(r.default_value, None);
	assert_eq!(r.confidence, 0.95);
}

#[test]
fn hookless_env_dedupes_by_file_and_varname_across_matches() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\.env\.([A-Z_][A-Z0-9_]*)'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let content = "\nconst a = process.env.PORT;\nconst b = process.env.PORT;\nconst c = process.env.HOST;\n";
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, content, "f.ts");
	let names: Vec<&str> = out.iter().map(|r| r.var_name.as_str()).collect();
	assert_eq!(names, vec!["PORT", "HOST"]);
}

#[test]
fn hookless_env_filters_non_upper_snake_walker_default_names() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "loose_match"
regex = 'X(\w+)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, "Xfoo XBAR Xbaz", "f.ts");
	let names: Vec<&str> = out.iter().map(|r| r.var_name.as_str()).collect();
	assert_eq!(names, vec!["BAR"]);
}

#[test]
fn walker_returns_empty_when_no_records_for_language() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Py, "anything", "f.py");
	assert!(out.is_empty());
}

#[test]
fn hookless_env_preserves_line_numbers_across_multi_line_content() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\.env\.([A-Z_][A-Z0-9_]*)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let content =
		"// line 1\n// line 2\nconst a = process.env.PORT;\n// line 4\nconst b = process.env.HOST;";
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, content, "f.ts");
	let port = out.iter().find(|r| r.var_name == "PORT").unwrap();
	let host = out.iter().find(|r| r.var_name == "HOST").unwrap();
	assert_eq!(port.line_number, 3);
	assert_eq!(host.line_number, 5);
}

// ──────────────────────────────────────────────────────────────────
// 2. Hooked env records
// ──────────────────────────────────────────────────────────────────

#[test]
fn hooked_env_dispatches_and_merges_output_with_walker_defaults() {
	fn hook(ctx: EnvHookContext<'_>) -> EnvHookResult {
		let var_name = ctx.captures.get(1).map(|m| m.as_str().to_string());
		EnvHookResult::Records(vec![EnvHookEmission {
			var_name,
			access_kind: Some(EnvAccessKind::Optional),
			default_value: Emitted::Value(Some("from-hook".to_string())),
			..Default::default()
		}])
	}

	let registry = env_registry_with(
		"set_optional_with_default",
		hook,
		&[EnvSuppliedField::AccessKind, EnvSuppliedField::DefaultValue],
	);

	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\.env\.([A-Z_][A-Z0-9_]*)'
confidence = 0.9
access_pattern = "process_env_dot"
hook = "set_optional_with_default"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, "x = process.env.PORT", "f.ts");
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.var_name, "PORT");
	assert_eq!(r.access_kind, EnvAccessKind::Optional);
	assert_eq!(r.access_pattern, EnvAccessPattern::ProcessEnvDot); // from static
	assert_eq!(r.default_value, Some("from-hook".to_string()));
	assert_eq!(r.confidence, 0.9);
}

#[test]
fn hooked_env_skip_suppresses_emission() {
	fn skip_hook(_ctx: EnvHookContext<'_>) -> EnvHookResult {
		EnvHookResult::Skip
	}
	let registry = env_registry_with(
		"always_skip",
		skip_hook,
		&[EnvSuppliedField::AccessKind, EnvSuppliedField::AccessPattern],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'process\.env\.([A-Z_]+)'
confidence = 0.9
hook = "always_skip"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, "x = process.env.PORT", "f.ts");
	assert!(out.is_empty());
}

#[test]
fn hooked_env_multi_emit_produces_multiple_records_per_match() {
	fn multi_hook(ctx: EnvHookContext<'_>) -> EnvHookResult {
		let body = match ctx.captures.get(1) {
			Some(m) => m.as_str(),
			None => return EnvHookResult::Skip,
		};
		let records = body
			.split(',')
			.map(|v| EnvHookEmission {
				var_name: Some(v.trim().to_string()),
				access_kind: Some(EnvAccessKind::Unknown),
				..Default::default()
			})
			.collect();
		EnvHookResult::Records(records)
	}
	let registry = env_registry_with(
		"split_csv",
		multi_hook,
		&[EnvSuppliedField::VarName, EnvSuppliedField::AccessKind],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "destructure"
regex = '\{([^}]+)\}'
confidence = 0.9
access_pattern = "process_env_destructure"
hook = "split_csv"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		"const { PORT, HOST, DB } = x",
		"f.ts",
	);
	let names: Vec<&str> = out.iter().map(|r| r.var_name.as_str()).collect();
	assert_eq!(names, vec!["PORT", "HOST", "DB"]);
}

#[test]
fn hooked_env_varname_overrides_walker_default_group1() {
	fn rename_hook(_ctx: EnvHookContext<'_>) -> EnvHookResult {
		EnvHookResult::Records(vec![EnvHookEmission {
			var_name: Some("RENAMED".to_string()),
			access_kind: Some(EnvAccessKind::Required),
			..Default::default()
		}])
	}
	let registry = env_registry_with(
		"rename",
		rename_hook,
		&[EnvSuppliedField::VarName, EnvSuppliedField::AccessKind],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'process\.env\.([A-Z_]+)'
confidence = 0.9
access_pattern = "process_env_dot"
hook = "rename"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, "x = process.env.ORIGINAL", "f.ts");
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "RENAMED");
}

// ──────────────────────────────────────────────────────────────────
// 3. Hookless fs records
// ──────────────────────────────────────────────────────────────────

#[test]
fn hookless_fs_emits_one_record_with_walker_default_target_path() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_literal"
regex = '(?:std::)?fs::write\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Rs,
		r#"std::fs::write("output.txt", data)?;"#,
		"src/main.rs",
	);
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.mutation_kind, MutationKind::WriteFile);
	assert_eq!(r.mutation_pattern, MutationPattern::RustFsWrite);
	assert_eq!(r.target_path.as_deref(), Some("output.txt"));
	assert_eq!(r.destination_path, None);
	assert!(!r.dynamic_path);
	assert_eq!(r.confidence, 0.9);
}

#[test]
fn hookless_fs_derives_dynamic_path_from_none_target() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_dynamic"
regex = '(?:std::)?fs::write\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Rs,
		"std::fs::write(some_var, data)?;",
		"src/main.rs",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path, None);
	assert!(out[0].dynamic_path);
}

#[test]
fn hookless_fs_captures_destination_from_group_2_when_two_ended() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_rename_literal"
regex = '(?:std::)?fs::rename\s*\(\s*"([^"]+)"\s*,\s*"([^"]+)"'
confidence = 0.9
base_kind = "rename_path"
base_pattern = "rust_fs_rename"
two_ended = true
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Rs,
		r#"fs::rename("a.txt", "b.txt")?;"#,
		"src/r.rs",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path.as_deref(), Some("a.txt"));
	assert_eq!(out[0].destination_path.as_deref(), Some("b.txt"));
}

#[test]
fn hookless_fs_ignores_group_2_when_not_two_ended() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "captures_two_groups"
regex = '\b(\w+)\s+(\w+)'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(&graph, &registry, DetectorLanguage::Rs, "foo bar", "f.rs");
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].destination_path, None);
}

// ──────────────────────────────────────────────────────────────────
// 4. Hooked fs records
// ──────────────────────────────────────────────────────────────────

#[test]
fn hooked_fs_dispatches_and_merges_output_with_walker_defaults() {
	fn dispatch_hook(_ctx: FsHookContext<'_>) -> FsHookResult {
		FsHookResult::Records(vec![FsHookEmission {
			target_path: Emitted::Value(Some("from-hook".to_string())),
			mutation_kind: Some(MutationKind::DeletePath),
			mutation_pattern: Some(MutationPattern::PyOsRemove),
			dynamic_path: Emitted::Value(false),
			..Default::default()
		}])
	}
	let registry = fs_registry_with(
		"dispatch_remove",
		dispatch_hook,
		&[
			FsSuppliedField::TargetPath,
			FsSuppliedField::DynamicPath,
			FsSuppliedField::MutationKind,
			FsSuppliedField::MutationPattern,
		],
	);
	let toml = r#"
[[detector]]
language = "py"
family = "fs"
pattern_id = "x"
regex = '\bremove\s*\('
confidence = 0.9
hook = "dispatch_remove"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(&graph, &registry, DetectorLanguage::Py, "os.remove(somevar)", "f.py");
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.mutation_kind, MutationKind::DeletePath);
	assert_eq!(r.mutation_pattern, MutationPattern::PyOsRemove);
	assert_eq!(r.target_path.as_deref(), Some("from-hook"));
	assert!(!r.dynamic_path);
}

#[test]
fn hooked_fs_emitted_target_path_overrides_walker_default() {
	fn override_hook(_ctx: FsHookContext<'_>) -> FsHookResult {
		FsHookResult::Records(vec![FsHookEmission {
			target_path: Emitted::Value(Some("overridden".to_string())),
			dynamic_path: Emitted::Value(false),
			..Default::default()
		}])
	}
	let registry = fs_registry_with(
		"override_path",
		override_hook,
		&[FsSuppliedField::TargetPath, FsSuppliedField::DynamicPath],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\bcall\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "override_path"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		r#"call("from-group-1")"#,
		"f.ts",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path.as_deref(), Some("overridden"));
}

#[test]
fn hooked_fs_unset_target_path_falls_back_to_walker_default() {
	// Hook emits an emission with target_path = Emitted::Unset.
	// Walker should use the walker default (group 1).
	fn unset_hook(_ctx: FsHookContext<'_>) -> FsHookResult {
		FsHookResult::Records(vec![FsHookEmission {
			// target_path left as Emitted::Unset (the default)
			..Default::default()
		}])
	}
	let registry = fs_registry_with("unset", unset_hook, &[]);
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\bcall\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "unset"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		r#"call("from-group-1")"#,
		"f.ts",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path.as_deref(), Some("from-group-1"));
}

#[test]
fn hooked_fs_explicit_null_overrides_walker_default() {
	// Hook emits target_path = Emitted::Value(None) — this is the
	// "explicit null override" case. Walker must NOT fall back to
	// group 1; it must honor the None.
	fn null_override_hook(_ctx: FsHookContext<'_>) -> FsHookResult {
		FsHookResult::Records(vec![FsHookEmission {
			target_path: Emitted::Value(None),
			dynamic_path: Emitted::Value(true),
			..Default::default()
		}])
	}
	let registry = fs_registry_with(
		"null_override",
		null_override_hook,
		&[FsSuppliedField::TargetPath, FsSuppliedField::DynamicPath],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\bcall\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "null_override"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		r#"call("would-have-been-default")"#,
		"f.ts",
	);
	assert_eq!(out.len(), 1);
	// Walker must NOT use group 1; hook explicitly overrode to null.
	assert_eq!(out[0].target_path, None);
	assert!(out[0].dynamic_path);
}

#[test]
fn hooked_fs_skip_suppresses_emission() {
	fn skip_hook(_ctx: FsHookContext<'_>) -> FsHookResult {
		FsHookResult::Skip
	}
	let registry = fs_registry_with(
		"always_skip",
		skip_hook,
		&[
			FsSuppliedField::TargetPath,
			FsSuppliedField::DynamicPath,
			FsSuppliedField::MutationKind,
			FsSuppliedField::MutationPattern,
		],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\ba\b'
confidence = 0.9
hook = "always_skip"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(&graph, &registry, DetectorLanguage::Ts, "a a a", "f.ts");
	assert!(out.is_empty());
}

// ──────────────────────────────────────────────────────────────────
// 5. Position dedup via named overlap groups
// ──────────────────────────────────────────────────────────────────

#[test]
fn position_dedup_same_group_dedupes_at_same_position() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_literal"
regex = '(?:std::)?fs::write\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"

[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_catchall"
regex = '(?:std::)?fs::write\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Rs,
		r#"std::fs::write("output.txt", data)?;"#,
		"f.rs",
	);
	// First record (literal) wins; catchall is suppressed by group dedup.
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path.as_deref(), Some("output.txt"));
}

#[test]
fn position_dedup_different_groups_do_not_interfere() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "first"
regex = '\bfs::write\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "group_a"

[[detector]]
language = "rs"
family = "fs"
pattern_id = "second"
regex = '\bfs::write\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "group_b"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(&graph, &registry, DetectorLanguage::Rs, "fs::write(x);", "f.rs");
	// Different groups → both emit.
	assert_eq!(out.len(), 2);
}

#[test]
fn position_dedup_ungrouped_records_always_emit_at_same_position() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "first"
regex = '\bfs\.writeFile\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"

[[detector]]
language = "ts"
family = "fs"
pattern_id = "second"
regex = '\bfs\.writeFile\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(&graph, &registry, DetectorLanguage::Ts, "fs.writeFile(x);", "f.ts");
	// No group → no dedup → both emit.
	assert_eq!(out.len(), 2);
}

#[test]
fn position_dedup_is_per_line_not_per_file() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "first"
regex = '\bfs::write\b'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Rs,
		"fs::write(a);\nfs::write(b);\nfs::write(c);",
		"f.rs",
	);
	// Each line is an independent dedup universe.
	assert_eq!(out.len(), 3);
	let lines: Vec<usize> = out.iter().map(|r| r.line_number).collect();
	assert_eq!(lines, vec![1, 2, 3]);
}

#[test]
fn position_dedup_distinct_positions_on_same_line_both_emit() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write"
regex = '(?:std::)?fs::write\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Rs,
		r#"std::fs::write("a.txt", x); std::fs::write("b.txt", y);"#,
		"f.rs",
	);
	assert_eq!(out.len(), 2);
	let paths: Vec<&str> = out
		.iter()
		.map(|r| r.target_path.as_deref().unwrap())
		.collect();
	assert_eq!(paths, vec!["a.txt", "b.txt"]);
}

// ──────────────────────────────────────────────────────────────────
// 6. single_match_per_line semantics
// ──────────────────────────────────────────────────────────────────

#[test]
fn env_single_match_per_line_stops_after_first_hit() {
	fn split_hook(ctx: EnvHookContext<'_>) -> EnvHookResult {
		let body = match ctx.captures.get(1) {
			Some(m) => m.as_str(),
			None => return EnvHookResult::Skip,
		};
		let records = body
			.split(',')
			.map(|v| EnvHookEmission {
				var_name: Some(v.trim().to_string()),
				access_kind: Some(EnvAccessKind::Unknown),
				..Default::default()
			})
			.collect();
		EnvHookResult::Records(records)
	}
	let registry = env_registry_with(
		"split_csv",
		split_hook,
		&[EnvSuppliedField::VarName, EnvSuppliedField::AccessKind],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "destructure"
regex = '\{([^}]+)\}\s*=\s*process\.env'
confidence = 0.9
access_kind = "unknown"
access_pattern = "process_env_destructure"
hook = "split_csv"
single_match_per_line = true
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		"const {A, B} = process.env; const {C, D} = process.env",
		"f.ts",
	);
	// Only first destructure processed → A and B emitted.
	let names: Vec<&str> = out.iter().map(|r| r.var_name.as_str()).collect();
	assert_eq!(names, vec!["A", "B"]);
}

#[test]
fn env_default_iterates_all_matches_on_line() {
	fn split_hook(ctx: EnvHookContext<'_>) -> EnvHookResult {
		let body = match ctx.captures.get(1) {
			Some(m) => m.as_str(),
			None => return EnvHookResult::Skip,
		};
		let records = body
			.split(',')
			.map(|v| EnvHookEmission {
				var_name: Some(v.trim().to_string()),
				access_kind: Some(EnvAccessKind::Unknown),
				..Default::default()
			})
			.collect();
		EnvHookResult::Records(records)
	}
	let registry = env_registry_with(
		"split_csv",
		split_hook,
		&[EnvSuppliedField::VarName, EnvSuppliedField::AccessKind],
	);
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "destructure"
regex = '\{([^}]+)\}\s*=\s*process\.env'
confidence = 0.9
access_kind = "unknown"
access_pattern = "process_env_destructure"
hook = "split_csv"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		"const {A, B} = process.env; const {C, D} = process.env",
		"f.ts",
	);
	let mut names: Vec<&str> = out.iter().map(|r| r.var_name.as_str()).collect();
	names.sort();
	assert_eq!(names, vec!["A", "B", "C", "D"]);
}

#[test]
fn fs_single_match_per_line_stops_after_first_hit() {
	let toml = r#"
[[detector]]
language = "java"
family = "fs"
pattern_id = "files_write"
regex = '\bFiles\.write\s*\('
confidence = 0.85
base_kind = "write_file"
base_pattern = "java_files_write"
single_match_per_line = true
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Java,
		"Files.write(a); Files.write(b);",
		"F.java",
	);
	assert_eq!(out.len(), 1);
}

#[test]
fn fs_single_match_per_line_still_applies_on_subsequent_lines() {
	let toml = r#"
[[detector]]
language = "java"
family = "fs"
pattern_id = "files_write"
regex = '\bFiles\.write\s*\('
confidence = 0.85
base_kind = "write_file"
base_pattern = "java_files_write"
single_match_per_line = true
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_fs_detectors(
		&graph,
		&registry,
		DetectorLanguage::Java,
		"Files.write(a); Files.write(b);\nFiles.write(c);",
		"F.java",
	);
	assert_eq!(out.len(), 2);
	let lines: Vec<usize> = out.iter().map(|r| r.line_number).collect();
	assert_eq!(lines, vec![1, 2]);
}

// ──────────────────────────────────────────────────────────────────
// 7. Declaration order preservation
// ──────────────────────────────────────────────────────────────────

#[test]
fn env_walker_iterates_records_in_toml_declaration_order() {
	// Two records both match `PORT`, but only the first claims the
	// (file, varName) key due to dedup. First record wins.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "first"
regex = '\b([A-Z]+)\b'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "second"
regex = '\b([A-Z]+)\b'
confidence = 0.5
access_kind = "optional"
access_pattern = "process_env_bracket"
"#;
	let registry = empty_registry();
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(&graph, &registry, DetectorLanguage::Ts, "PORT", "f.ts");
	assert_eq!(out.len(), 1);
	let r = &out[0];
	// First record's static fields win.
	assert_eq!(r.access_kind, EnvAccessKind::Required);
	assert_eq!(r.access_pattern, EnvAccessPattern::ProcessEnvDot);
	assert_eq!(r.confidence, 0.95);
}

// ──────────────────────────────────────────────────────────────────
// Production registry smoke (sanity — the walker can use the real
// hook registry on a real detector graph)
// ──────────────────────────────────────────────────────────────────

#[test]
fn walker_works_with_production_hook_registry_on_synthetic_graph() {
	// Build a tiny synthetic graph that references one of the
	// production hooks. This verifies that the walker + loader +
	// production hooks integrate cleanly before we wire the full
	// detectors.toml in R1-G.
	let registry = create_default_hook_registry();
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\.env\.([A-Z_][A-Z0-9_]*)'
confidence = 0.95
access_pattern = "process_env_dot"
hook = "infer_js_env_access_with_default"
"#;
	let graph = load_detector_graph(toml, &registry).unwrap();
	let out = walk_env_detectors(
		&graph,
		&registry,
		DetectorLanguage::Ts,
		r#"const port = process.env.PORT || "3000";"#,
		"f.ts",
	);
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.var_name, "PORT");
	assert_eq!(r.access_kind, EnvAccessKind::Optional);
	assert_eq!(r.default_value.as_deref(), Some("3000"));
}
