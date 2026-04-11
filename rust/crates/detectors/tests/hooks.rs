//! Hook implementation tests — Rust mirror of TS hook semantics.
//!
//! Each hook is unit-tested in isolation against synthetic inputs
//! that exercise its specific contract dimension. The tests are
//! organized by hook, and each hook has tests for:
//!
//!   - happy path emission shape
//!   - tri-state field correctness (Emitted vs Option)
//!   - dispatch logic where applicable
//!   - skip behavior on malformed input
//!
//! These tests do NOT use the loader or walker. They construct
//! synthetic `EnvHookContext` / `FsHookContext` values directly
//! and call the hook function via the registry. The walker-level
//! integration is verified by R1-J parity tests later.
//!
//! Helper: each hook is invoked through the production registry
//! lookup so the test exercises the same dispatch path the walker
//! will use.

use std::collections::HashSet;

use regex::Regex;
use repo_graph_detectors::comment_masker::mask_comments_for_file;
use repo_graph_detectors::hooks::create_default_hook_registry;
use repo_graph_detectors::types::{
	DetectorFamily, DetectorLanguage, Emitted, EnvAccessKind, EnvAccessPattern,
	EnvDetectorRecord, EnvHookContext, EnvHookResult, FsDetectorRecord,
	FsHookContext, FsHookResult, MutationKind, MutationPattern,
};

// ── Test fixtures ──────────────────────────────────────────────────

/// Build a synthetic env record with the given regex and
/// access_kind/access_pattern statics. Used to construct hook
/// contexts in tests.
fn make_env_record(pattern_id: &str, regex: &str) -> EnvDetectorRecord {
	let compiled = Regex::new(regex).unwrap();
	EnvDetectorRecord {
		language: DetectorLanguage::Ts,
		family: DetectorFamily::Env,
		pattern_id: pattern_id.to_string(),
		regex: regex.to_string(),
		flags: "g".to_string(),
		compiled_regex: compiled,
		confidence: 0.95,
		hook: None,
		single_match_per_line: false,
		access_kind: Some(EnvAccessKind::Required),
		access_pattern: Some(EnvAccessPattern::ProcessEnvDot),
	}
}

fn make_fs_record(pattern_id: &str, regex: &str) -> FsDetectorRecord {
	let compiled = Regex::new(regex).unwrap();
	FsDetectorRecord {
		language: DetectorLanguage::Ts,
		family: DetectorFamily::Fs,
		pattern_id: pattern_id.to_string(),
		regex: regex.to_string(),
		flags: "g".to_string(),
		compiled_regex: compiled,
		confidence: 0.90,
		hook: None,
		single_match_per_line: false,
		base_kind: Some(MutationKind::WriteFile),
		base_pattern: Some(MutationPattern::FsWriteFile),
		two_ended: false,
		position_dedup_group: None,
	}
}

/// Run an env hook against a line of source. The hook is looked up
/// by name in the production registry, then invoked with a
/// synthetic context built from the regex's first match against
/// the line.
fn run_env_hook(
	hook_name: &'static str,
	regex_pattern: &str,
	line: &str,
) -> EnvHookResult {
	let registry = create_default_hook_registry();
	let entry = registry
		.env
		.get(hook_name)
		.unwrap_or_else(|| panic!("env hook {hook_name} not registered"));
	let record = make_env_record("test", regex_pattern);
	let captures = record
		.compiled_regex
		.captures(line)
		.expect("expected regex to match");
	let ctx = EnvHookContext {
		captures,
		line,
		line_number: 1,
		file_path: "test.ts",
		base_record: &record,
	};
	(entry.func)(ctx)
}

/// Run an fs hook against a line of source.
fn run_fs_hook(
	hook_name: &'static str,
	regex_pattern: &str,
	line: &str,
) -> FsHookResult {
	let registry = create_default_hook_registry();
	let entry = registry
		.fs
		.get(hook_name)
		.unwrap_or_else(|| panic!("fs hook {hook_name} not registered"));
	let record = make_fs_record("test", regex_pattern);
	let captures = record
		.compiled_regex
		.captures(line)
		.expect("expected regex to match");
	let ctx = FsHookContext {
		captures,
		line,
		line_number: 1,
		file_path: "test.ts",
		base_record: &record,
	};
	(entry.func)(ctx)
}

fn unwrap_env_records(result: EnvHookResult) -> Vec<repo_graph_detectors::types::EnvHookEmission> {
	match result {
		EnvHookResult::Records(rs) => rs,
		EnvHookResult::Skip => panic!("expected Records, got Skip"),
	}
}

fn unwrap_fs_records(result: FsHookResult) -> Vec<repo_graph_detectors::types::FsHookEmission> {
	match result {
		FsHookResult::Records(rs) => rs,
		FsHookResult::Skip => panic!("expected Records, got Skip"),
	}
}

// ──────────────────────────────────────────────────────────────────
// infer_js_env_access_with_default
// ──────────────────────────────────────────────────────────────────

#[test]
fn infer_js_bare_access_is_required_with_no_default() {
	let result = run_env_hook(
		"infer_js_env_access_with_default",
		r#"process\.env\.([A-Z_][A-Z0-9_]*)"#,
		"const port = process.env.PORT;",
	);
	let records = unwrap_env_records(result);
	assert_eq!(records.len(), 1);
	let r = &records[0];
	assert_eq!(r.var_name.as_deref(), Some("PORT"));
	assert_eq!(r.access_kind, Some(EnvAccessKind::Required));
	assert!(matches!(r.default_value, Emitted::Value(None)));
}

#[test]
fn infer_js_or_fallback_is_optional_with_default() {
	let result = run_env_hook(
		"infer_js_env_access_with_default",
		r#"process\.env\.([A-Z_][A-Z0-9_]*)"#,
		r#"const port = process.env.PORT || "3000";"#,
	);
	let records = unwrap_env_records(result);
	assert_eq!(records.len(), 1);
	let r = &records[0];
	assert_eq!(r.access_kind, Some(EnvAccessKind::Optional));
	assert!(matches!(r.default_value, Emitted::Value(Some(ref s)) if s == "3000"));
}

#[test]
fn infer_js_nullish_fallback_is_optional_with_default() {
	let result = run_env_hook(
		"infer_js_env_access_with_default",
		r#"process\.env\.([A-Z_][A-Z0-9_]*)"#,
		r#"const port = process.env.PORT ?? "3000";"#,
	);
	let records = unwrap_env_records(result);
	let r = &records[0];
	assert_eq!(r.access_kind, Some(EnvAccessKind::Optional));
	assert!(matches!(r.default_value, Emitted::Value(Some(ref s)) if s == "3000"));
}

#[test]
fn infer_js_inside_function_call_is_unknown() {
	let result = run_env_hook(
		"infer_js_env_access_with_default",
		r#"process\.env\.([A-Z_][A-Z0-9_]*)"#,
		"someFn(process.env.NODE_ENV)",
	);
	let records = unwrap_env_records(result);
	let r = &records[0];
	assert_eq!(r.access_kind, Some(EnvAccessKind::Unknown));
	assert!(matches!(r.default_value, Emitted::Value(None)));
}

// ──────────────────────────────────────────────────────────────────
// expand_js_env_destructure
// ──────────────────────────────────────────────────────────────────

#[test]
fn destructure_emits_one_record_per_var() {
	let result = run_env_hook(
		"expand_js_env_destructure",
		r#"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*process\.env"#,
		"const { PORT, HOST, DB_URL } = process.env;",
	);
	let records = unwrap_env_records(result);
	let names: Vec<&str> = records
		.iter()
		.map(|r| r.var_name.as_deref().unwrap())
		.collect();
	assert_eq!(names, vec!["PORT", "HOST", "DB_URL"]);
	for r in &records {
		assert_eq!(r.access_kind, Some(EnvAccessKind::Unknown));
	}
}

#[test]
fn destructure_filters_out_lowercase_names() {
	let result = run_env_hook(
		"expand_js_env_destructure",
		r#"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*process\.env"#,
		"const { PORT, lowercase, HOST } = process.env;",
	);
	let records = unwrap_env_records(result);
	let names: Vec<&str> = records
		.iter()
		.map(|r| r.var_name.as_deref().unwrap())
		.collect();
	assert_eq!(names, vec!["PORT", "HOST"]);
}

#[test]
fn destructure_handles_renaming() {
	let result = run_env_hook(
		"expand_js_env_destructure",
		r#"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*process\.env"#,
		"const { PORT: localPort, HOST: localHost } = process.env;",
	);
	let records = unwrap_env_records(result);
	let names: Vec<&str> = records
		.iter()
		.map(|r| r.var_name.as_deref().unwrap())
		.collect();
	// Original names (before rename), filtered to upper-snake.
	assert_eq!(names, vec!["PORT", "HOST"]);
}

#[test]
fn destructure_with_no_valid_vars_skips() {
	let result = run_env_hook(
		"expand_js_env_destructure",
		r#"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*process\.env"#,
		"const { lowercase, alsoLower } = process.env;",
	);
	assert!(matches!(result, EnvHookResult::Skip));
}

// ──────────────────────────────────────────────────────────────────
// python_get_default_from_group2
// ──────────────────────────────────────────────────────────────────

#[test]
fn python_get_with_default_is_optional() {
	let result = run_env_hook(
		"python_get_default_from_group2",
		r#"os\.environ\.get\(["']([A-Z_][A-Z0-9_]*)["'](?:\s*,\s*["']([^"']*)["'])?\)"#,
		r#"port = os.environ.get("PORT", "3000")"#,
	);
	let records = unwrap_env_records(result);
	let r = &records[0];
	assert_eq!(r.var_name.as_deref(), Some("PORT"));
	assert_eq!(r.access_kind, Some(EnvAccessKind::Optional));
	assert!(matches!(r.default_value, Emitted::Value(Some(ref s)) if s == "3000"));
}

#[test]
fn python_get_without_default_is_unknown() {
	let result = run_env_hook(
		"python_get_default_from_group2",
		r#"os\.environ\.get\(["']([A-Z_][A-Z0-9_]*)["'](?:\s*,\s*["']([^"']*)["'])?\)"#,
		r#"port = os.environ.get("PORT")"#,
	);
	let records = unwrap_env_records(result);
	let r = &records[0];
	assert_eq!(r.access_kind, Some(EnvAccessKind::Unknown));
	assert!(matches!(r.default_value, Emitted::Value(None)));
}

// ──────────────────────────────────────────────────────────────────
// extract_first_string_arg
// ──────────────────────────────────────────────────────────────────

#[test]
fn extract_first_string_arg_literal_path() {
	let result = run_fs_hook(
		"extract_first_string_arg",
		r#"\bfs\.writeFile(?:Sync)?\s*\("#,
		r#"fs.writeFile("output.txt", data, cb)"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(Some(ref s)) if s == "output.txt"));
	assert!(matches!(r.dynamic_path, Emitted::Value(false)));
}

#[test]
fn extract_first_string_arg_dynamic_path() {
	let result = run_fs_hook(
		"extract_first_string_arg",
		r#"\bfs\.writeFile(?:Sync)?\s*\("#,
		"fs.writeFile(somePath, data, cb)",
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(None)));
	assert!(matches!(r.dynamic_path, Emitted::Value(true)));
}

#[test]
fn extract_first_string_arg_template_literal_static() {
	// Plain template literal with no interpolation is treated as
	// a literal.
	let result = run_fs_hook(
		"extract_first_string_arg",
		r#"\bfs\.writeFile(?:Sync)?\s*\("#,
		r#"fs.writeFile(`output.txt`, data, cb)"#,
	);
	let records = unwrap_fs_records(result);
	assert!(matches!(records[0].target_path, Emitted::Value(Some(ref s)) if s == "output.txt"));
}

#[test]
fn extract_first_string_arg_template_literal_with_interpolation_is_dynamic() {
	let result = run_fs_hook(
		"extract_first_string_arg",
		r#"\bfs\.writeFile(?:Sync)?\s*\("#,
		r#"fs.writeFile(`logs/${name}.log`, data, cb)"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(None)));
	assert!(matches!(r.dynamic_path, Emitted::Value(true)));
}

// ──────────────────────────────────────────────────────────────────
// extract_two_ended_args
// ──────────────────────────────────────────────────────────────────

#[test]
fn extract_two_ended_args_both_literal() {
	let result = run_fs_hook(
		"extract_two_ended_args",
		r#"\bfs\.rename(?:Sync)?\s*\("#,
		r#"fs.rename("a.txt", "b.txt", cb)"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(Some(ref s)) if s == "a.txt"));
	assert!(matches!(r.destination_path, Emitted::Value(Some(ref s)) if s == "b.txt"));
}

#[test]
fn extract_two_ended_args_first_literal_second_dynamic() {
	let result = run_fs_hook(
		"extract_two_ended_args",
		r#"\bfs\.rename(?:Sync)?\s*\("#,
		r#"fs.rename("a.txt", dest, cb)"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(Some(ref s)) if s == "a.txt"));
	assert!(matches!(r.destination_path, Emitted::Value(None)));
}

#[test]
fn extract_two_ended_args_first_dynamic_emits_both_null() {
	let result = run_fs_hook(
		"extract_two_ended_args",
		r#"\bfs\.rename(?:Sync)?\s*\("#,
		"fs.rename(src, dst, cb)",
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(None)));
	assert!(matches!(r.destination_path, Emitted::Value(None)));
}

// ──────────────────────────────────────────────────────────────────
// unwrap_java_files_write_literal (and the three sibling Java hooks)
// ──────────────────────────────────────────────────────────────────

#[test]
fn unwrap_java_files_write_literal_extracts_path() {
	let result = run_fs_hook(
		"unwrap_java_files_write_literal",
		r#"\bFiles\.write(?:String)?\s*\("#,
		r#"Files.write(Paths.get("output.txt"), bytes);"#,
	);
	let records = unwrap_fs_records(result);
	assert!(matches!(records[0].target_path, Emitted::Value(Some(ref s)) if s == "output.txt"));
}

#[test]
fn unwrap_java_files_write_literal_dynamic_when_no_paths_get() {
	let result = run_fs_hook(
		"unwrap_java_files_write_literal",
		r#"\bFiles\.write(?:String)?\s*\("#,
		"Files.write(somePath, bytes);",
	);
	let records = unwrap_fs_records(result);
	assert!(matches!(records[0].target_path, Emitted::Value(None)));
	assert!(matches!(records[0].dynamic_path, Emitted::Value(true)));
}

#[test]
fn unwrap_java_files_delete_literal_extracts_path() {
	let result = run_fs_hook(
		"unwrap_java_files_delete_literal",
		r#"\bFiles\.delete\s*\("#,
		r#"Files.delete(Paths.get("tmp.dat"));"#,
	);
	let records = unwrap_fs_records(result);
	assert!(matches!(records[0].target_path, Emitted::Value(Some(ref s)) if s == "tmp.dat"));
}

#[test]
fn unwrap_java_files_create_directory_literal_extracts_path() {
	let result = run_fs_hook(
		"unwrap_java_files_create_directory_literal",
		r#"\bFiles\.createDirector(?:y|ies)\s*\("#,
		r#"Files.createDirectory(Paths.get("uploads"));"#,
	);
	let records = unwrap_fs_records(result);
	assert!(matches!(records[0].target_path, Emitted::Value(Some(ref s)) if s == "uploads"));
}

#[test]
fn unwrap_java_file_output_stream_literal_static_confidence() {
	// Literal case → confidence stays at the static (no override).
	let result = run_fs_hook(
		"unwrap_java_file_output_stream_literal",
		r#"\bnew\s+FileOutputStream\s*\("#,
		r#"OutputStream out = new FileOutputStream("data.bin");"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(Some(ref s)) if s == "data.bin"));
	assert!(matches!(r.dynamic_path, Emitted::Value(false)));
	assert_eq!(r.confidence, None); // walker uses record.confidence
}

#[test]
fn unwrap_java_file_output_stream_literal_dynamic_confidence_override() {
	// Dynamic case → hook overrides confidence to 0.80.
	let result = run_fs_hook(
		"unwrap_java_file_output_stream_literal",
		r#"\bnew\s+FileOutputStream\s*\("#,
		"OutputStream out = new FileOutputStream(somePath);",
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert!(matches!(r.target_path, Emitted::Value(None)));
	assert!(matches!(r.dynamic_path, Emitted::Value(true)));
	assert_eq!(r.confidence, Some(0.80));
}

// ──────────────────────────────────────────────────────────────────
// dispatch_c_fopen_mode
// ──────────────────────────────────────────────────────────────────

#[test]
fn dispatch_c_fopen_write_mode() {
	let result = run_fs_hook(
		"dispatch_c_fopen_mode",
		r#"\bfopen\s*\(\s*(?:"([^"]+)"|[^,]+)\s*,\s*"([wa][b+]?)""#,
		r#"FILE *f = fopen("data.bin", "wb");"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert_eq!(r.mutation_kind, Some(MutationKind::WriteFile));
	assert_eq!(r.mutation_pattern, Some(MutationPattern::CFopenWrite));
	assert!(matches!(r.target_path, Emitted::Value(Some(ref s)) if s == "data.bin"));
}

#[test]
fn dispatch_c_fopen_append_mode() {
	let result = run_fs_hook(
		"dispatch_c_fopen_mode",
		r#"\bfopen\s*\(\s*(?:"([^"]+)"|[^,]+)\s*,\s*"([wa][b+]?)""#,
		r#"FILE *f = fopen("audit.log", "a");"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert_eq!(r.mutation_kind, Some(MutationKind::AppendFile));
	assert_eq!(r.mutation_pattern, Some(MutationPattern::CFopenAppend));
}

// ──────────────────────────────────────────────────────────────────
// dispatch_python_open_mode
// ──────────────────────────────────────────────────────────────────

#[test]
fn dispatch_python_open_write_mode() {
	let result = run_fs_hook(
		"dispatch_python_open_mode",
		r#"\bopen\s*\(\s*(?:["']([^"']+)["']|[^,)]+)\s*,\s*["']([wa])[^"']*["']"#,
		r#"with open("output.txt", "w") as f: f.write(data)"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert_eq!(r.mutation_kind, Some(MutationKind::WriteFile));
	assert_eq!(r.mutation_pattern, Some(MutationPattern::PyOpenWrite));
	assert!(matches!(r.target_path, Emitted::Value(Some(ref s)) if s == "output.txt"));
}

#[test]
fn dispatch_python_open_append_mode() {
	let result = run_fs_hook(
		"dispatch_python_open_mode",
		r#"\bopen\s*\(\s*(?:["']([^"']+)["']|[^,)]+)\s*,\s*["']([wa])[^"']*["']"#,
		r#"open("audit.log", "a")"#,
	);
	let records = unwrap_fs_records(result);
	let r = &records[0];
	assert_eq!(r.mutation_kind, Some(MutationKind::AppendFile));
	assert_eq!(r.mutation_pattern, Some(MutationPattern::PyOpenAppend));
}

// ──────────────────────────────────────────────────────────────────
// dispatch_python_remove_call
// ──────────────────────────────────────────────────────────────────

#[test]
fn dispatch_python_remove_dispatches_os_remove() {
	let result = run_fs_hook(
		"dispatch_python_remove_call",
		r#"\b(os\.(?:remove|unlink)|shutil\.rmtree)\s*\(\s*(?:["']([^"']+)["']|[^)]+)\s*\)"#,
		r#"os.remove("tmp/cache.json")"#,
	);
	let records = unwrap_fs_records(result);
	assert_eq!(records[0].mutation_pattern, Some(MutationPattern::PyOsRemove));
}

#[test]
fn dispatch_python_remove_dispatches_os_unlink() {
	let result = run_fs_hook(
		"dispatch_python_remove_call",
		r#"\b(os\.(?:remove|unlink)|shutil\.rmtree)\s*\(\s*(?:["']([^"']+)["']|[^)]+)\s*\)"#,
		r#"os.unlink("tmp/cache.json")"#,
	);
	let records = unwrap_fs_records(result);
	assert_eq!(records[0].mutation_pattern, Some(MutationPattern::PyOsUnlink));
}

#[test]
fn dispatch_python_remove_dispatches_shutil_rmtree() {
	let result = run_fs_hook(
		"dispatch_python_remove_call",
		r#"\b(os\.(?:remove|unlink)|shutil\.rmtree)\s*\(\s*(?:["']([^"']+)["']|[^)]+)\s*\)"#,
		r#"shutil.rmtree("build")"#,
	);
	let records = unwrap_fs_records(result);
	assert_eq!(records[0].mutation_pattern, Some(MutationPattern::PyShutilRmtree));
}

// ──────────────────────────────────────────────────────────────────
// dispatch_python_mkdir_call
// ──────────────────────────────────────────────────────────────────

#[test]
fn dispatch_python_mkdir_dispatches_mkdir() {
	let result = run_fs_hook(
		"dispatch_python_mkdir_call",
		r#"\bos\.(mkdir|makedirs)\s*\(\s*(?:["']([^"']+)["']|[^)]+)"#,
		r#"os.mkdir("uploads")"#,
	);
	let records = unwrap_fs_records(result);
	assert_eq!(records[0].mutation_pattern, Some(MutationPattern::PyOsMkdir));
}

#[test]
fn dispatch_python_mkdir_dispatches_makedirs() {
	let result = run_fs_hook(
		"dispatch_python_mkdir_call",
		r#"\bos\.(mkdir|makedirs)\s*\(\s*(?:["']([^"']+)["']|[^)]+)"#,
		r#"os.makedirs("data/uploads")"#,
	);
	let records = unwrap_fs_records(result);
	assert_eq!(records[0].mutation_pattern, Some(MutationPattern::PyOsMakedirs));
}

// ──────────────────────────────────────────────────────────────────
// Registry shape sanity
// ──────────────────────────────────────────────────────────────────

#[test]
fn registry_contains_all_thirteen_hooks() {
	let registry = create_default_hook_registry();
	let env_names: HashSet<&'static str> = registry.env.keys().copied().collect();
	let fs_names: HashSet<&'static str> = registry.fs.keys().copied().collect();

	assert!(env_names.contains("infer_js_env_access_with_default"));
	assert!(env_names.contains("expand_js_env_destructure"));
	assert!(env_names.contains("python_get_default_from_group2"));
	assert_eq!(env_names.len(), 3);

	assert!(fs_names.contains("extract_first_string_arg"));
	assert!(fs_names.contains("extract_two_ended_args"));
	assert!(fs_names.contains("unwrap_java_files_write_literal"));
	assert!(fs_names.contains("unwrap_java_files_delete_literal"));
	assert!(fs_names.contains("unwrap_java_files_create_directory_literal"));
	assert!(fs_names.contains("unwrap_java_file_output_stream_literal"));
	assert!(fs_names.contains("dispatch_c_fopen_mode"));
	assert!(fs_names.contains("dispatch_python_open_mode"));
	assert!(fs_names.contains("dispatch_python_remove_call"));
	assert!(fs_names.contains("dispatch_python_mkdir_call"));
	assert_eq!(fs_names.len(), 10);
}

// Suppress the unused-import warning if `mask_comments_for_file` is
// not directly referenced in any test (it's there for future use).
#[allow(dead_code)]
fn _unused_import_keepalive() {
	let _ = mask_comments_for_file("x.ts", "");
}
