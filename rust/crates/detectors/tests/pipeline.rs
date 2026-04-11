//! Public detector pipeline integration tests.
//!
//! These tests exercise the top-level `detect_env_accesses` and
//! `detect_fs_mutations` functions as drop-in replacements for
//! the TS public API. They verify:
//!
//!   1. Language dispatch — the right walker bucket is selected
//!      from the file path extension, unknown extensions return
//!      empty
//!   2. Comment masking integration — patterns inside comments
//!      are NOT detected (the full pipeline runs the masker
//!      before the walker)
//!   3. End-to-end smoke on real production regexes — basic
//!      `process.env.PORT` and `fs.writeFile("out.txt")` detections
//!      produce the expected fact shapes
//!   4. Production graph singleton loads cleanly (accessing
//!      `production_pipeline()` triggers the lazy init and should
//!      never panic on a valid `detectors.toml`)
//!
//! The curated contract parity corpus comes later in R1-H/R1-I/R1-J.
//! These tests are smoke-level and do not assert byte-for-byte
//! equivalence against the TS pipeline — they assert that the
//! wrapper composes the three layers (masker + walker + registry)
//! correctly and produces the shapes R1-B / R1-F defined.

use repo_graph_detectors::types::{
	EnvAccessKind, EnvAccessPattern, MutationKind, MutationPattern,
};
use repo_graph_detectors::{detect_env_accesses, detect_fs_mutations, production_pipeline};

// ── Production singleton loads cleanly ────────────────────────────

#[test]
fn production_pipeline_loads_without_panic() {
	let pipeline = production_pipeline();
	// The graph must contain records from every language bucket
	// the production detectors.toml declares.
	assert!(!pipeline.graph.all_records.is_empty());
	// Registry must contain the 13 production hooks (3 env + 10 fs).
	assert_eq!(pipeline.registry.env.len(), 3);
	assert_eq!(pipeline.registry.fs.len(), 10);
}

// ── Language dispatch smoke tests ─────────────────────────────────

#[test]
fn unknown_extension_returns_empty_for_env() {
	let out = detect_env_accesses("process.env.PORT", "src/foo.rb");
	assert!(out.is_empty());
}

#[test]
fn unknown_extension_returns_empty_for_fs() {
	let out = detect_fs_mutations("fs.writeFile(\"out.txt\")", "src/foo.rb");
	assert!(out.is_empty());
}

#[test]
fn no_extension_returns_empty() {
	let out = detect_env_accesses("process.env.PORT", "Makefile");
	assert!(out.is_empty());
}

// ── Env end-to-end smoke ──────────────────────────────────────────

#[test]
fn env_ts_process_env_dot_detection_produces_expected_shape() {
	let out = detect_env_accesses("const port = process.env.PORT;", "src/foo.ts");
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.var_name, "PORT");
	assert_eq!(r.access_kind, EnvAccessKind::Required);
	assert_eq!(r.access_pattern, EnvAccessPattern::ProcessEnvDot);
	assert_eq!(r.file_path, "src/foo.ts");
	assert_eq!(r.line_number, 1);
	assert_eq!(r.default_value, None);
}

#[test]
fn env_ts_process_env_dot_with_fallback_is_optional_with_default() {
	let out = detect_env_accesses(
		r#"const port = process.env.PORT || "3000";"#,
		"src/foo.ts",
	);
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.access_kind, EnvAccessKind::Optional);
	assert_eq!(r.default_value.as_deref(), Some("3000"));
}

#[test]
fn env_ts_bracket_access_detected() {
	let out = detect_env_accesses(
		r#"const port = process.env["PORT"];"#,
		"src/foo.ts",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "PORT");
	assert_eq!(
		out[0].access_pattern,
		EnvAccessPattern::ProcessEnvBracket
	);
}

#[test]
fn env_python_os_environ_detected() {
	let out = detect_env_accesses(
		r#"port = os.environ["PORT"]"#,
		"src/app.py",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "PORT");
	assert_eq!(out[0].access_pattern, EnvAccessPattern::OsEnviron);
	assert_eq!(out[0].access_kind, EnvAccessKind::Required);
}

#[test]
fn env_python_os_getenv_with_default_is_optional() {
	let out = detect_env_accesses(
		r#"port = os.getenv("PORT", "3000")"#,
		"src/app.py",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].access_kind, EnvAccessKind::Optional);
	assert_eq!(out[0].default_value.as_deref(), Some("3000"));
}

#[test]
fn env_rust_std_env_var_detected() {
	let out = detect_env_accesses(
		r#"let port = std::env::var("PORT").unwrap();"#,
		"src/main.rs",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "PORT");
	assert_eq!(out[0].access_pattern, EnvAccessPattern::StdEnvVar);
}

#[test]
fn env_java_system_getenv_detected() {
	let out = detect_env_accesses(
		r#"String port = System.getenv("PORT");"#,
		"src/Main.java",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "PORT");
	assert_eq!(out[0].access_pattern, EnvAccessPattern::SystemGetenv);
}

#[test]
fn env_c_getenv_detected() {
	let out = detect_env_accesses(
		r#"char *port = getenv("PORT");"#,
		"src/main.c",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "PORT");
	assert_eq!(out[0].access_pattern, EnvAccessPattern::CGetenv);
}

// ── Fs end-to-end smoke ───────────────────────────────────────────

#[test]
fn fs_ts_writefile_literal_detection_produces_expected_shape() {
	let out = detect_fs_mutations(
		r#"fs.writeFile("out.txt", data, cb);"#,
		"src/foo.ts",
	);
	assert_eq!(out.len(), 1);
	let r = &out[0];
	assert_eq!(r.mutation_kind, MutationKind::WriteFile);
	assert_eq!(r.mutation_pattern, MutationPattern::FsWriteFile);
	assert_eq!(r.target_path.as_deref(), Some("out.txt"));
	assert_eq!(r.destination_path, None);
	assert!(!r.dynamic_path);
	assert_eq!(r.file_path, "src/foo.ts");
	assert_eq!(r.line_number, 1);
}

#[test]
fn fs_ts_writefile_dynamic_path() {
	let out = detect_fs_mutations("fs.writeFile(somePath, data, cb);", "src/foo.ts");
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path, None);
	assert!(out[0].dynamic_path);
}

#[test]
fn fs_ts_rename_captures_destination() {
	let out = detect_fs_mutations(
		r#"fs.rename("a.txt", "b.txt", cb);"#,
		"src/foo.ts",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].mutation_pattern, MutationPattern::FsRename);
	assert_eq!(out[0].target_path.as_deref(), Some("a.txt"));
	assert_eq!(out[0].destination_path.as_deref(), Some("b.txt"));
}

#[test]
fn fs_rust_fs_write_literal_wins_over_catchall_via_position_dedup() {
	let out = detect_fs_mutations(
		r#"std::fs::write("output.txt", data)?;"#,
		"src/main.rs",
	);
	// The literal variant and catchall variant of rust_fs_write
	// share the same position_dedup_group, so only the literal
	// emission should survive.
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].target_path.as_deref(), Some("output.txt"));
}

#[test]
fn fs_c_fopen_dispatches_append_vs_write() {
	let write_out = detect_fs_mutations(
		r#"FILE *f = fopen("data.bin", "wb");"#,
		"src/main.c",
	);
	assert_eq!(write_out.len(), 1);
	assert_eq!(write_out[0].mutation_kind, MutationKind::WriteFile);
	assert_eq!(write_out[0].mutation_pattern, MutationPattern::CFopenWrite);

	let append_out = detect_fs_mutations(
		r#"FILE *f = fopen("audit.log", "a");"#,
		"src/main.c",
	);
	assert_eq!(append_out.len(), 1);
	assert_eq!(append_out[0].mutation_kind, MutationKind::AppendFile);
	assert_eq!(append_out[0].mutation_pattern, MutationPattern::CFopenAppend);
}

#[test]
fn fs_python_open_dispatches_write_vs_append() {
	let write_out = detect_fs_mutations(
		r#"with open("output.txt", "w") as f: f.write(data)"#,
		"src/app.py",
	);
	assert_eq!(write_out.len(), 1);
	assert_eq!(write_out[0].mutation_kind, MutationKind::WriteFile);
	assert_eq!(write_out[0].mutation_pattern, MutationPattern::PyOpenWrite);

	let append_out = detect_fs_mutations(
		r#"open("audit.log", "a")"#,
		"src/app.py",
	);
	assert_eq!(append_out.len(), 1);
	assert_eq!(append_out[0].mutation_kind, MutationKind::AppendFile);
	assert_eq!(append_out[0].mutation_pattern, MutationPattern::PyOpenAppend);
}

#[test]
fn fs_java_files_write_with_paths_get_literal() {
	let out = detect_fs_mutations(
		r#"Files.write(Paths.get("output.txt"), bytes);"#,
		"src/Main.java",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].mutation_pattern, MutationPattern::JavaFilesWrite);
	assert_eq!(out[0].target_path.as_deref(), Some("output.txt"));
}

#[test]
fn fs_java_file_output_stream_dynamic_has_lower_confidence() {
	let literal_out = detect_fs_mutations(
		r#"OutputStream out = new FileOutputStream("data.bin");"#,
		"src/Writer.java",
	);
	assert_eq!(literal_out.len(), 1);
	assert_eq!(literal_out[0].confidence, 0.85);

	let dynamic_out = detect_fs_mutations(
		"OutputStream out = new FileOutputStream(somePath);",
		"src/Writer.java",
	);
	assert_eq!(dynamic_out.len(), 1);
	assert_eq!(dynamic_out[0].confidence, 0.80);
}

// ── Comment masking integration ──────────────────────────────────

#[test]
fn env_inside_line_comment_is_not_detected() {
	// The full pipeline must run the comment masker before the
	// walker, so patterns inside line comments should be blanked
	// and NOT emitted.
	let out = detect_env_accesses(
		"// process.env.PHANTOM is not a real access\n",
		"src/foo.ts",
	);
	assert!(out.is_empty());
}

#[test]
fn env_inside_block_comment_is_not_detected() {
	let out = detect_env_accesses(
		"/* process.env.PHANTOM */",
		"src/foo.ts",
	);
	assert!(out.is_empty());
}

#[test]
fn env_inside_jsdoc_is_not_detected() {
	let out = detect_env_accesses(
		"/**\n * @example process.env.PHANTOM\n */\n",
		"src/foo.ts",
	);
	assert!(out.is_empty());
}

#[test]
fn real_env_access_detected_alongside_commented_phantom() {
	let out = detect_env_accesses(
		"// process.env.PHANTOM documented\nconst real = process.env.REAL;",
		"src/foo.ts",
	);
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "REAL");
	// Line number should reflect the REAL access's line (2), not
	// the commented phantom's line (1). This verifies the masker
	// preserved line numbers.
	assert_eq!(out[0].line_number, 2);
}

#[test]
fn fs_inside_comment_is_not_detected() {
	let out = detect_fs_mutations(
		"// fs.writeFile(\"phantom.txt\") is in a comment\n",
		"src/foo.ts",
	);
	assert!(out.is_empty());
}

#[test]
fn python_env_inside_hash_comment_is_not_detected() {
	let out = detect_env_accesses(
		"# os.environ[\"PHANTOM\"] is commented\n",
		"src/app.py",
	);
	assert!(out.is_empty());
}

#[test]
fn python_env_inside_triple_quoted_docstring_is_preserved_in_string_literal() {
	// Python triple-quoted strings are NOT masked by the masker
	// (they are real string literals, not comments). But
	// `os.environ["X"]` inside a docstring is still a string
	// literal, not a code-level access, so the env detector's
	// regex would need to NOT match it. Let me trace: the regex
	// is `os\.environ\[["']([A-Z_][A-Z0-9_]*)["']\]`. This matches
	// the bytes `os.environ["X"]` regardless of whether they are
	// inside a code expression or inside a docstring.
	//
	// The TS side has the same behavior: detectors regex-match
	// against the masked content, and masking preserves string
	// literal contents (including triple-quoted). Patterns inside
	// string literals are matched as if they were code.
	//
	// This is an acknowledged limitation of the regex-based
	// detectors and is recorded as TECH-DEBT in the TS slice
	// ("string-literal-embedded env access false positives"). It
	// is NOT a bug in this slice — the Rust pipeline must produce
	// the same behavior as the TS pipeline.
	let out = detect_env_accesses(
		"doc = \"\"\"Example: os.environ[\"EXAMPLE\"]\"\"\"",
		"src/app.py",
	);
	// Both runtimes detect this as a real access (known limitation
	// shared between TS and Rust). The test pins the current
	// behavior; fixing this is out of scope for R1-G.
	assert_eq!(out.len(), 1);
	assert_eq!(out[0].var_name, "EXAMPLE");
}

// ── Destructure via hook end-to-end ───────────────────────────────

#[test]
fn env_ts_destructure_produces_one_record_per_var() {
	let out = detect_env_accesses(
		"const { PORT, HOST, DB_URL } = process.env;",
		"src/config.ts",
	);
	// The destructure hook emits one record per var name. All
	// three should be detected with access_kind = Unknown.
	let names: Vec<&str> = out.iter().map(|r| r.var_name.as_str()).collect();
	assert!(names.contains(&"PORT"));
	assert!(names.contains(&"HOST"));
	assert!(names.contains(&"DB_URL"));
	for r in &out {
		assert_eq!(r.access_pattern, EnvAccessPattern::ProcessEnvDestructure);
	}
}

// ── Multi-line line number preservation ──────────────────────────

#[test]
fn line_numbers_preserved_across_comment_masked_content() {
	let content = r#"// line 1 comment
/* line 2
   line 3
   line 4 */
const port = process.env.PORT;
// line 6
const host = process.env.HOST;"#;
	let out = detect_env_accesses(content, "src/foo.ts");
	let port = out.iter().find(|r| r.var_name == "PORT").unwrap();
	let host = out.iter().find(|r| r.var_name == "HOST").unwrap();
	assert_eq!(port.line_number, 5);
	assert_eq!(host.line_number, 7);
}
