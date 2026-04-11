//! Loader integration tests — Rust mirror of TS `loader.test.ts`.
//!
//! Each TS describe block / it case has a corresponding Rust test
//! function. Test names are camelCase-mapped to snake_case but
//! preserve the same input + expected outcome shape.
//!
//! Assertion mechanism: instead of regex matching against the JS
//! Error message string, Rust tests use `matches!` against the
//! error variant plus a `contains` check on the message text. The
//! message format is preserved closely to the TS format so the
//! contains-checks are robust to minor wording changes.

use std::collections::{HashMap, HashSet};

use repo_graph_detectors::loader::{load_detector_graph, DetectorLoadError};
use repo_graph_detectors::types::{
	DetectorFamily, DetectorRecord, EnvHook, EnvHookEntry, EnvHookResult,
	EnvSuppliedField, FsHook, FsHookEntry, FsHookResult, FsSuppliedField,
	HookRegistry,
};

// ── Test helpers ───────────────────────────────────────────────────

const fn dummy_env_hook() -> EnvHook {
	|_ctx| EnvHookResult::Skip
}

const fn dummy_fs_hook() -> FsHook {
	|_ctx| FsHookResult::Skip
}

#[derive(Clone)]
struct EnvHookSpec {
	name: &'static str,
	supplies: &'static [EnvSuppliedField],
}

#[derive(Clone)]
struct FsHookSpec {
	name: &'static str,
	supplies: &'static [FsSuppliedField],
}

fn make_registry(env_hooks: &[EnvHookSpec], fs_hooks: &[FsHookSpec]) -> HookRegistry {
	let mut env: HashMap<&'static str, EnvHookEntry> = HashMap::new();
	let mut fs: HashMap<&'static str, FsHookEntry> = HashMap::new();
	for spec in env_hooks {
		let mut supplies = HashSet::new();
		for f in spec.supplies {
			supplies.insert(*f);
		}
		env.insert(
			spec.name,
			EnvHookEntry {
				func: dummy_env_hook(),
				supplies,
			},
		);
	}
	for spec in fs_hooks {
		let mut supplies = HashSet::new();
		for f in spec.supplies {
			supplies.insert(*f);
		}
		fs.insert(
			spec.name,
			FsHookEntry {
				func: dummy_fs_hook(),
				supplies,
			},
		);
	}
	HookRegistry { env, fs }
}

fn empty_registry() -> HookRegistry {
	make_registry(&[], &[])
}

fn assert_validation_err_contains(
	res: Result<impl std::fmt::Debug, DetectorLoadError>,
	expected_substring: &str,
) {
	match res {
		Ok(v) => panic!("expected validation error containing {expected_substring:?}, got Ok({v:?})"),
		Err(DetectorLoadError::Validation(msg)) => {
			assert!(
				msg.contains(expected_substring),
				"expected validation error to contain {expected_substring:?}, got: {msg}"
			);
		}
		Err(DetectorLoadError::TomlParse(msg)) => {
			assert!(
				msg.contains(expected_substring),
				"expected error to contain {expected_substring:?}, got TomlParse: {msg}"
			);
		}
	}
}

fn assert_toml_parse_err_contains(
	res: Result<impl std::fmt::Debug, DetectorLoadError>,
	expected_substring: &str,
) {
	match res {
		Ok(v) => panic!("expected TOML parse error containing {expected_substring:?}, got Ok({v:?})"),
		Err(DetectorLoadError::TomlParse(msg)) => {
			assert!(
				msg.contains(expected_substring),
				"expected TOML parse error to contain {expected_substring:?}, got: {msg}"
			);
		}
		Err(DetectorLoadError::Validation(msg)) => {
			panic!("expected TOML parse error, got Validation: {msg}")
		}
	}
}

// ──────────────────────────────────────────────────────────────────
// happy path
// ──────────────────────────────────────────────────────────────────

#[test]
fn loads_a_single_env_record_with_all_required_fields() {
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
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 1);
	let r = &graph.all_records[0];
	match r {
		DetectorRecord::Env(e) => {
			assert_eq!(e.pattern_id, "process_env_dot");
			assert!((e.confidence - 0.95).abs() < f64::EPSILON);
			assert_eq!(e.flags, "g");
			assert!(e.hook.is_none());
		}
		_ => panic!("expected env record"),
	}
}

#[test]
fn loads_a_single_fs_record_with_all_required_fields() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "fs_write_file"
regex = '\bfs\.writeFile(?:Sync)?\s*\('
confidence = 0.90
base_kind = "write_file"
base_pattern = "fs_write_file"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 1);
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => {
			assert_eq!(f.pattern_id, "fs_write_file");
			assert!(!f.two_ended);
			assert!(f.position_dedup_group.is_none());
		}
		_ => panic!("expected fs record"),
	}
}

#[test]
fn accepts_an_env_record_that_references_a_registered_hook() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\.env\.([A-Z_][A-Z0-9_]*)'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"
hook = "infer_js_env_access_with_default"
"#;
	let registry = make_registry(
		&[EnvHookSpec {
			name: "infer_js_env_access_with_default",
			supplies: &[],
		}],
		&[],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => {
			assert_eq!(e.hook.as_deref(), Some("infer_js_env_access_with_default"));
		}
		_ => panic!("expected env record"),
	}
}

#[test]
fn accepts_an_fs_record_that_references_a_registered_hook() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "fs_rename"
regex = '\bfs\.rename(?:Sync)?\s*\('
confidence = 0.90
base_kind = "rename_path"
base_pattern = "fs_rename"
two_ended = true
hook = "extract_two_ended_args"
"#;
	let registry = make_registry(
		&[],
		&[FsHookSpec {
			name: "extract_two_ended_args",
			supplies: &[],
		}],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => {
			assert!(f.two_ended);
			assert_eq!(f.hook.as_deref(), Some("extract_two_ended_args"));
		}
		_ => panic!("expected fs record"),
	}
}

#[test]
fn preserves_declaration_order_in_all_records() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "first"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "second"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_bracket"

[[detector]]
language = "py"
family = "env"
pattern_id = "third"
regex = 'c'
confidence = 0.9
access_kind = "required"
access_pattern = "os_environ"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	let ids: Vec<&str> = graph
		.all_records
		.iter()
		.map(|r| match r {
			DetectorRecord::Env(e) => e.pattern_id.as_str(),
			DetectorRecord::Fs(f) => f.pattern_id.as_str(),
		})
		.collect();
	assert_eq!(ids, vec!["first", "second", "third"]);
}

#[test]
fn indexes_records_under_every_expected_family_language_bucket() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "ts_env_a"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "ts_env_b"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_bracket"

[[detector]]
language = "py"
family = "env"
pattern_id = "py_env"
regex = 'c'
confidence = 0.9
access_kind = "required"
access_pattern = "os_environ"

[[detector]]
language = "ts"
family = "fs"
pattern_id = "ts_fs"
regex = 'd'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	use repo_graph_detectors::types::DetectorLanguage::*;
	use DetectorFamily::*;
	assert_eq!(graph.by_family_language.get(&(Env, Ts)).unwrap().len(), 2);
	assert_eq!(graph.by_family_language.get(&(Env, Py)).unwrap().len(), 1);
	assert_eq!(graph.by_family_language.get(&(Fs, Ts)).unwrap().len(), 1);
	assert!(!graph.by_family_language.contains_key(&(Fs, Py)));
}

#[test]
fn preserves_order_within_each_family_language_bucket() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "first"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "second"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_bracket"

[[detector]]
language = "ts"
family = "env"
pattern_id = "third"
regex = 'c'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_destructure"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	use repo_graph_detectors::types::DetectorLanguage::*;
	use DetectorFamily::*;
	let bucket = graph.by_family_language.get(&(Env, Ts)).unwrap();
	let ids: Vec<&str> = bucket
		.iter()
		.map(|r| match r {
			DetectorRecord::Env(e) => e.pattern_id.as_str(),
			DetectorRecord::Fs(_) => unreachable!(),
		})
		.collect();
	assert_eq!(ids, vec!["first", "second", "third"]);
}

#[test]
fn compiles_regex_with_custom_flags() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "case_insensitive"
regex = 'foo'
flags = "gi"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => {
			assert_eq!(e.flags, "gi");
			// Smoke test: case-insensitive regex matches both cases.
			assert!(e.compiled_regex.is_match("FOO"));
			assert!(e.compiled_regex.is_match("foo"));
		}
		_ => panic!("expected env record"),
	}
}

// ──────────────────────────────────────────────────────────────────
// malformed TOML
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_invalid_toml_syntax() {
	let res = load_detector_graph("[[detector\nbroken", &empty_registry());
	assert_toml_parse_err_contains(res, "");
}

#[test]
fn rejects_unknown_top_level_keys() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "ok"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[settings]
unknown = true
"#;
	let res = load_detector_graph(toml, &empty_registry());
	// serde's deny_unknown_fields error message includes the
	// unknown field name.
	assert_toml_parse_err_contains(res, "settings");
}

#[test]
fn rejects_when_detector_is_not_an_array_of_tables() {
	let toml = r#"detector = "not an array""#;
	let res = load_detector_graph(toml, &empty_registry());
	// serde error: expected sequence, got string
	match res {
		Err(DetectorLoadError::TomlParse(_)) => {}
		other => panic!("expected TomlParse error, got {other:?}"),
	}
}

// ──────────────────────────────────────────────────────────────────
// required field validation
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_missing_language() {
	let toml = r#"
[[detector]]
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "missing required field \"language\"");
}

#[test]
fn rejects_missing_family() {
	let toml = r#"
[[detector]]
language = "ts"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "missing required field \"family\"");
}

#[test]
fn rejects_missing_pattern_id() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "missing required field \"pattern_id\"");
}

#[test]
fn rejects_missing_regex() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "missing required field \"regex\"");
}

#[test]
fn rejects_missing_confidence() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "missing required field \"confidence\"");
}

#[test]
fn rejects_env_record_missing_access_kind_when_no_hook_supplies_it() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res,
		"required env field \"accessKind\" is neither declared statically nor supplied by hook",
	);
}

#[test]
fn rejects_env_record_missing_access_pattern_when_no_hook_supplies_it() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res,
		"required env field \"accessPattern\" is neither declared statically nor supplied by hook",
	);
}

#[test]
fn rejects_fs_record_missing_base_kind_when_no_hook_supplies_it() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_pattern = "fs_write_file"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res,
		"required fs field \"mutationKind\" is neither declared statically nor supplied by hook",
	);
}

#[test]
fn rejects_fs_record_missing_base_pattern_when_no_hook_supplies_it() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res,
		"required fs field \"mutationPattern\" is neither declared statically nor supplied by hook",
	);
}

// ──────────────────────────────────────────────────────────────────
// enum validation
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_unknown_language() {
	let toml = r#"
[[detector]]
language = "go"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown language \"go\"");
}

#[test]
fn rejects_unknown_family() {
	let toml = r#"
[[detector]]
language = "ts"
family = "network"
pattern_id = "x"
regex = 'a'
confidence = 0.9
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown family \"network\"");
}

#[test]
fn rejects_unknown_env_access_kind() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "maybe"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown access_kind \"maybe\"");
}

#[test]
fn rejects_unknown_env_access_pattern() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_void"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown access_pattern");
}

#[test]
fn rejects_unknown_fs_base_kind() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "vaporize"
base_pattern = "fs_write_file"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown base_kind \"vaporize\"");
}

#[test]
fn rejects_unknown_fs_base_pattern() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_void"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown base_pattern");
}

// ──────────────────────────────────────────────────────────────────
// type validation
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_non_string_language() {
	let toml = r#"
[[detector]]
language = 1
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	// serde rejects type mismatch at deserialization
	assert_toml_parse_err_contains(res, "language");
}

#[test]
fn rejects_non_number_confidence() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = "high"
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_toml_parse_err_contains(res, "confidence");
}

#[test]
fn rejects_confidence_outside_zero_one() {
	let toml_high = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 1.5
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml_high, &empty_registry());
	assert_validation_err_contains(res, "confidence must be a finite number in [0, 1]");

	let toml_low = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = -0.1
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml_low, &empty_registry());
	assert_validation_err_contains(res, "confidence must be a finite number in [0, 1]");
}

#[test]
fn rejects_empty_string_field() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = ""
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "\"pattern_id\" must not be empty");
}

#[test]
fn rejects_non_boolean_two_ended() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "rename_path"
base_pattern = "fs_rename"
two_ended = "yes"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_toml_parse_err_contains(res, "two_ended");
}

// ──────────────────────────────────────────────────────────────────
// regex validation
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_invalid_regex_syntax() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "broken"
regex = '[unclosed'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "regex compile error");
}

#[test]
fn rejects_unknown_regex_flag() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
flags = "gz"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown regex flag \"z\"");
}

#[test]
fn rejects_sticky_flag_unsupported() {
	// The TS sticky flag has no Rust regex equivalent. Silently
	// treating it as a no-op (the previous behavior) would degrade
	// sticky semantics to global, which is a material divergence
	// from the TS contract. The loader hard-rejects `y` with a
	// precise error naming the cause.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
flags = "gy"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "regex flag \"y\" (sticky)");
	// Also pin the actionable language so the error tells authors
	// what to do, not just what failed.
	let res2 = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res2,
		"no Rust regex equivalent and is not supported by this runtime",
	);
}

#[test]
fn rejects_sticky_flag_alone() {
	// Sticky alone (no `g`) also rejects.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
flags = "y"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "regex flag \"y\" (sticky)");
}

#[test]
fn rejects_empty_match_capable_star_regex() {
	// `a*` matches the empty string. Reject at load time because
	// the walker's iteration advancement differs from JS on
	// non-BMP input.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a*'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "empty-match-capable");
}

#[test]
fn rejects_empty_match_capable_optional_regex() {
	// `a?` matches the empty string.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a?'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "empty-match-capable");
}

#[test]
fn rejects_empty_match_capable_alternation() {
	// `a*|b*` matches empty via either alternative.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a*|b*'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "empty-match-capable");
}

#[test]
fn rejects_empty_match_capable_non_capture_group() {
	// `(?:)` is literally the empty group — always matches empty.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = '(?:)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "empty-match-capable");
}

#[test]
fn accepts_plus_quantifier_that_requires_one_or_more_chars() {
	// `a+` requires at least one char and does NOT match empty.
	// This test pins the negative case: non-empty-matchable
	// regexes are not accidentally rejected.
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a+'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 1);
}

#[test]
fn error_message_suggests_actionable_fix() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a*'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	// The error message should include actionable guidance.
	assert_validation_err_contains(res, "Rewrite the regex");
}

// ──────────────────────────────────────────────────────────────────
// hook validation
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_unknown_env_hook_name() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
hook = "nonexistent_hook"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown env hook \"nonexistent_hook\"");
}

#[test]
fn rejects_unknown_fs_hook_name() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "nonexistent_hook"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown fs hook \"nonexistent_hook\"");
}

#[test]
fn env_hook_from_fs_registry_is_rejected_cross_family() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
hook = "extract_first_string_arg"
"#;
	let registry = make_registry(
		&[],
		&[FsHookSpec {
			name: "extract_first_string_arg",
			supplies: &[],
		}],
	);
	let res = load_detector_graph(toml, &registry);
	assert_validation_err_contains(res, "unknown env hook \"extract_first_string_arg\"");
}

// ──────────────────────────────────────────────────────────────────
// duplicate detection
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_duplicate_language_family_pattern_id() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res,
		"duplicate (language=ts, family=env, pattern_id=process_env_dot)",
	);
}

#[test]
fn allows_same_pattern_id_across_different_languages() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "shared_id"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "py"
family = "env"
pattern_id = "shared_id"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "os_environ"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 2);
}

#[test]
fn allows_same_pattern_id_across_different_families() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "shared_id"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "fs"
pattern_id = "shared_id"
regex = 'b'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 2);
}

// ──────────────────────────────────────────────────────────────────
// field allowlist (typo defense + per-family)
// ──────────────────────────────────────────────────────────────────

#[test]
fn rejects_unknown_field_on_env_record() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
typo_field = true
"#;
	let res = load_detector_graph(toml, &empty_registry());
	// serde deny_unknown_fields catches this
	assert_toml_parse_err_contains(res, "typo_field");
}

#[test]
fn rejects_fs_only_field_on_env_record() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
two_ended = false
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown field \"two_ended\" for env family");
}

#[test]
fn rejects_env_only_field_on_fs_record() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
access_kind = "required"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "unknown field \"access_kind\" for fs family");
}

// ──────────────────────────────────────────────────────────────────
// must-be-supplied (env)
// ──────────────────────────────────────────────────────────────────

#[test]
fn accepts_env_record_with_no_static_fields_when_hook_supplies_both() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
hook = "supplies_kind_and_pattern"
"#;
	let registry = make_registry(
		&[EnvHookSpec {
			name: "supplies_kind_and_pattern",
			supplies: &[EnvSuppliedField::AccessKind, EnvSuppliedField::AccessPattern],
		}],
		&[],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	assert_eq!(graph.all_records.len(), 1);
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => {
			assert!(e.access_kind.is_none());
			assert!(e.access_pattern.is_none());
			assert_eq!(e.hook.as_deref(), Some("supplies_kind_and_pattern"));
		}
		_ => panic!("expected env record"),
	}
}

#[test]
fn accepts_env_record_with_hook_supplying_only_access_kind_static_pattern() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_pattern = "process_env_dot"
hook = "supplies_kind_only"
"#;
	let registry = make_registry(
		&[EnvHookSpec {
			name: "supplies_kind_only",
			supplies: &[EnvSuppliedField::AccessKind],
		}],
		&[],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => {
			assert!(e.access_kind.is_none());
			assert!(e.access_pattern.is_some());
		}
		_ => panic!("expected env record"),
	}
}

#[test]
fn accepts_env_record_with_both_static_and_hook_override_semantics() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
hook = "infer_kind"
"#;
	let registry = make_registry(
		&[EnvHookSpec {
			name: "infer_kind",
			supplies: &[EnvSuppliedField::AccessKind],
		}],
		&[],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => {
			assert!(e.access_kind.is_some());
			assert!(e.access_pattern.is_some());
			assert_eq!(e.hook.as_deref(), Some("infer_kind"));
		}
		_ => panic!("expected env record"),
	}
}

#[test]
fn rejects_env_record_where_hook_supplies_only_access_kind_and_pattern_missing() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
hook = "supplies_kind_only"
"#;
	let registry = make_registry(
		&[EnvHookSpec {
			name: "supplies_kind_only",
			supplies: &[EnvSuppliedField::AccessKind],
		}],
		&[],
	);
	let res = load_detector_graph(toml, &registry);
	assert_validation_err_contains(
		res,
		"required env field \"accessPattern\" is neither declared statically nor supplied by hook \"supplies_kind_only\"",
	);
}

#[test]
fn rejects_env_record_where_hook_supplies_no_required_field() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
hook = "supplies_nothing_required"
"#;
	let registry = make_registry(
		&[EnvHookSpec {
			name: "supplies_nothing_required",
			supplies: &[EnvSuppliedField::VarName, EnvSuppliedField::DefaultValue],
		}],
		&[],
	);
	let res = load_detector_graph(toml, &registry);
	assert_validation_err_contains(
		res,
		"is neither declared statically nor supplied by hook \"supplies_nothing_required\"",
	);
}

// ──────────────────────────────────────────────────────────────────
// must-be-supplied (fs)
// ──────────────────────────────────────────────────────────────────

#[test]
fn accepts_fs_record_with_no_static_fields_when_hook_supplies_both() {
	let toml = r#"
[[detector]]
language = "c"
family = "fs"
pattern_id = "c_fopen"
regex = '\bfopen\s*\('
confidence = 0.85
hook = "dispatch_c_fopen_mode"
"#;
	let registry = make_registry(
		&[],
		&[FsHookSpec {
			name: "dispatch_c_fopen_mode",
			supplies: &[
				FsSuppliedField::MutationKind,
				FsSuppliedField::MutationPattern,
				FsSuppliedField::TargetPath,
			],
		}],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => {
			assert!(f.base_kind.is_none());
			assert!(f.base_pattern.is_none());
		}
		_ => panic!("expected fs record"),
	}
}

#[test]
fn accepts_fs_record_with_hook_supplying_only_mutation_pattern_static_kind() {
	let toml = r#"
[[detector]]
language = "py"
family = "fs"
pattern_id = "py_remove"
regex = '\b(os\.(?:remove|unlink)|shutil\.rmtree)\s*\('
confidence = 0.90
base_kind = "delete_path"
hook = "dispatch_python_remove_call"
"#;
	let registry = make_registry(
		&[],
		&[FsHookSpec {
			name: "dispatch_python_remove_call",
			supplies: &[FsSuppliedField::MutationPattern],
		}],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => {
			assert!(f.base_kind.is_some());
			assert!(f.base_pattern.is_none());
		}
		_ => panic!("expected fs record"),
	}
}

#[test]
fn rejects_fs_record_where_hook_does_not_supply_mutation_kind_and_base_kind_missing() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\bfs\.writeFile\s*\('
confidence = 0.9
base_pattern = "fs_write_file"
hook = "extract_first_string_arg"
"#;
	let registry = make_registry(
		&[],
		&[FsHookSpec {
			name: "extract_first_string_arg",
			supplies: &[FsSuppliedField::TargetPath, FsSuppliedField::DynamicPath],
		}],
	);
	let res = load_detector_graph(toml, &registry);
	assert_validation_err_contains(
		res,
		"required fs field \"mutationKind\" is neither declared statically nor supplied by hook \"extract_first_string_arg\"",
	);
}

#[test]
fn accepts_fs_record_with_full_static_and_hook_supplies_path_only() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "fs_write_file"
regex = '\bfs\.writeFile(?:Sync)?\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "extract_first_string_arg"
"#;
	let registry = make_registry(
		&[],
		&[FsHookSpec {
			name: "extract_first_string_arg",
			supplies: &[FsSuppliedField::TargetPath, FsSuppliedField::DynamicPath],
		}],
	);
	let graph = load_detector_graph(toml, &registry).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => {
			assert!(f.base_kind.is_some());
			assert!(f.base_pattern.is_some());
			assert_eq!(f.hook.as_deref(), Some("extract_first_string_arg"));
		}
		_ => panic!("expected fs record"),
	}
}

// ──────────────────────────────────────────────────────────────────
// single_match_per_line
// ──────────────────────────────────────────────────────────────────

#[test]
fn defaults_single_match_per_line_to_false_when_absent() {
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
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => assert!(!e.single_match_per_line),
		_ => panic!("expected env record"),
	}
}

#[test]
fn accepts_single_match_per_line_true_on_env_record() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
single_match_per_line = true
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Env(e) => assert!(e.single_match_per_line),
		_ => panic!("expected env record"),
	}
}

#[test]
fn accepts_single_match_per_line_true_on_fs_record() {
	let toml = r#"
[[detector]]
language = "java"
family = "fs"
pattern_id = "x"
regex = '\bFiles\.write\s*\('
confidence = 0.85
base_kind = "write_file"
base_pattern = "java_files_write"
single_match_per_line = true
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => assert!(f.single_match_per_line),
		_ => panic!("expected fs record"),
	}
}

#[test]
fn rejects_non_boolean_single_match_per_line() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
single_match_per_line = "yes"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_toml_parse_err_contains(res, "single_match_per_line");
}

// ──────────────────────────────────────────────────────────────────
// position_dedup_group (fs only)
// ──────────────────────────────────────────────────────────────────

#[test]
fn accepts_fs_record_with_named_overlap_group() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_literal"
regex = '\bfs::write\s*\(\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => {
			assert_eq!(f.position_dedup_group.as_deref(), Some("rust_fs_write_callsite"));
		}
		_ => panic!("expected fs record"),
	}
}

#[test]
fn defaults_position_dedup_group_to_none_when_absent() {
	let toml = r#"
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\bfs\.writeFile\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
"#;
	let graph = load_detector_graph(toml, &empty_registry()).unwrap();
	match &graph.all_records[0] {
		DetectorRecord::Fs(f) => assert!(f.position_dedup_group.is_none()),
		_ => panic!("expected fs record"),
	}
}

#[test]
fn rejects_empty_string_position_dedup_group() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "x"
regex = '\bfs::write\s*\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = ""
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(res, "\"position_dedup_group\" must not be empty");
}

#[test]
fn rejects_position_dedup_group_on_env_records_fs_only() {
	let toml = r#"
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'process\.env\.([A-Z_]+)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
position_dedup_group = "some_group"
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_validation_err_contains(
		res,
		"unknown field \"position_dedup_group\" for env family",
	);
}

#[test]
fn rejects_non_string_position_dedup_group() {
	let toml = r#"
[[detector]]
language = "rs"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = 42
"#;
	let res = load_detector_graph(toml, &empty_registry());
	assert_toml_parse_err_contains(res, "position_dedup_group");
}

// ──────────────────────────────────────────────────────────────────
// edge cases
// ──────────────────────────────────────────────────────────────────

#[test]
fn loads_an_empty_graph_from_empty_toml() {
	let graph = load_detector_graph("", &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 0);
	assert_eq!(graph.by_family_language.len(), 0);
}

#[test]
fn loads_an_empty_graph_from_toml_with_no_detectors() {
	let graph = load_detector_graph("# comment only\n", &empty_registry()).unwrap();
	assert_eq!(graph.all_records.len(), 0);
}
