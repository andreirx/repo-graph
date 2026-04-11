//! Detector hook registry implementations.
//!
//! Rust mirror of `src/core/seams/detectors/hooks.ts`. Each hook is
//! the procedural counterpart of one or more declared detector
//! records in `detectors.toml`. Hooks own the parts of detector
//! behavior that are NOT cleanly representable as data:
//! line-context inspection, multi-record emission, mode-based
//! dispatch, and state-machine string-literal extraction.
//!
//! Every hook is lifted byte-identical from the TS implementation
//! to preserve detector output exactly across the externalization
//! boundary. The parity tests (R1-J) verify byte-identical output
//! against the TS pipeline on a curated fixture corpus.
//!
//! Hook contract (per types.rs):
//!  - DTO-shaped I/O — no storage, no fs, no side-effect surface
//!  - Returns either `FsHookResult::Skip` / `EnvHookResult::Skip`
//!    or `Records(Vec<...>)`
//!  - Each emission is a partial; the walker merges with walker
//!    defaults and the static record's base fields
//!
//! Hook merge semantics use the tri-state `Emitted<T>` wrapper for
//! fields where the TS walker uses `!== undefined`. See `types.rs`
//! `Emitted` documentation for the rationale.
//!
//! Pure functions only. No I/O. No external state.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::types::{
	Emitted, EnvAccessKind, EnvHookContext, EnvHookEmission, EnvHookEntry,
	EnvHookResult, EnvSuppliedField, FsHookContext, FsHookEmission, FsHookEntry,
	FsHookResult, FsSuppliedField, HookRegistry, MutationKind, MutationPattern,
};

// ── Lazy-compiled inner regexes ────────────────────────────────────
//
// Several hooks run a more specific regex against `ctx.line` to
// extract a literal that the outer detector regex did not capture.
// These are compiled once per process via `LazyLock`.

/// Inner regex for `infer_js_env_access_with_default`: captures the
/// default value from `process.env.X || "default"` style fallbacks.
static JS_DEFAULT_VALUE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"^\s*(?:\|\||&&|\?\?)\s*["']([^"']+)["']"#).unwrap()
});

/// Inner regex for `infer_js_env_access_with_default`: matches a
/// fallback operator (`||`, `&&`, `??`) right after the var.
static JS_HAS_FALLBACK_OP: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"^\s*(\|\||&&|\?\?)"#).unwrap());

/// Inner regex for `infer_js_env_access_with_default`: matches a
/// closing paren right after the var (indicates the access is
/// inside a function call argument list — kind = unknown).
static JS_NEXT_IS_CLOSE_PAREN: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"^\s*\)"#).unwrap());

/// Inner regex for `unwrap_java_files_write_literal`.
static JAVA_FILES_WRITE_LITERAL: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"Files\.write(?:String)?\s*\(\s*Paths\.get\s*\(\s*"([^"]+)""#).unwrap()
});

/// Inner regex for `unwrap_java_files_delete_literal`.
static JAVA_FILES_DELETE_LITERAL: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"Files\.delete\s*\(\s*Paths\.get\s*\(\s*"([^"]+)""#).unwrap()
});

/// Inner regex for `unwrap_java_files_create_directory_literal`.
static JAVA_FILES_CREATE_DIRECTORY_LITERAL: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"createDirector(?:y|ies)\s*\(\s*Paths\.get\s*\(\s*"([^"]+)""#)
		.unwrap()
});

/// Inner regex for `unwrap_java_file_output_stream_literal`.
static JAVA_FILE_OUTPUT_STREAM_LITERAL: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"new\s+FileOutputStream\s*\(\s*"([^"]+)""#).unwrap());

// ── Env hooks ──────────────────────────────────────────────────────

/// Infer required vs optional access kind for JS env access by
/// reading line context AFTER the matched var name. Lifted from
/// TS `inferJsAccessKind` + `extractJsDefault`.
///
/// Behavior preserved exactly:
///  - `process.env.X || "fallback"` → optional, default "fallback"
///  - `process.env.X ?? "fallback"` → optional, default "fallback"
///  - `process.env.X)` (inside a function call) → unknown
///  - bare `process.env.X` → required, no default
///
/// Uses `line.find(varName)` (which mirrors TS `line.indexOf`)
/// rather than the regex match position, to preserve any
/// pre-existing edge case behavior of the original.
fn infer_js_env_access_with_default(ctx: EnvHookContext<'_>) -> EnvHookResult {
	let var_name = match ctx.captures.get(1) {
		Some(m) => m.as_str().to_string(),
		None => return EnvHookResult::Skip,
	};

	let var_pos = match ctx.line.find(&var_name) {
		Some(p) => p,
		None => return EnvHookResult::Skip,
	};
	let after_var = &ctx.line[var_pos + var_name.len()..];

	let access_kind = if JS_HAS_FALLBACK_OP.is_match(after_var) {
		EnvAccessKind::Optional
	} else if JS_NEXT_IS_CLOSE_PAREN.is_match(after_var) {
		EnvAccessKind::Unknown
	} else {
		EnvAccessKind::Required
	};

	let default_value = JS_DEFAULT_VALUE
		.captures(after_var)
		.and_then(|c| c.get(1).map(|m| m.as_str().to_string()));

	EnvHookResult::Records(vec![EnvHookEmission {
		var_name: Some(var_name),
		access_kind: Some(access_kind),
		default_value: Emitted::Value(default_value),
		..Default::default()
	}])
}

/// Expand a JS destructure pattern into one record per declared
/// var. Lifted from the destructure branch in TS detectJsEnvAccesses.
///
/// Walker default for varName does NOT apply because regex group 1
/// is the entire destructure body, not an individual var.
fn expand_js_env_destructure(ctx: EnvHookContext<'_>) -> EnvHookResult {
	let body = match ctx.captures.get(1) {
		Some(m) => m.as_str(),
		None => return EnvHookResult::Skip,
	};

	let mut records: Vec<EnvHookEmission> = Vec::new();
	let upper_snake = Regex::new(r#"^[A-Z_][A-Z0-9_]*$"#).unwrap();

	for raw in body.split(',') {
		let v = raw.trim();
		// Handle renaming: `VAR_NAME: localName`
		let var_name = if let Some(colon) = v.find(':') {
			v[..colon].trim()
		} else {
			v
		};
		if upper_snake.is_match(var_name) {
			records.push(EnvHookEmission {
				var_name: Some(var_name.to_string()),
				access_kind: Some(EnvAccessKind::Unknown),
				default_value: Emitted::Value(None),
				..Default::default()
			});
		}
	}

	if records.is_empty() {
		return EnvHookResult::Skip;
	}
	EnvHookResult::Records(records)
}

/// Read default value from regex capture group 2 for Python
/// `os.environ.get("X", "default")` and `os.getenv("X", "default")`.
///
/// Behavior preserved exactly:
///  - if group 2 is present, accessKind = "optional", default = group 2
///  - if group 2 is absent, accessKind = "unknown", default = null
fn python_get_default_from_group2(ctx: EnvHookContext<'_>) -> EnvHookResult {
	let var_name = match ctx.captures.get(1) {
		Some(m) => m.as_str().to_string(),
		None => return EnvHookResult::Skip,
	};
	let default_value = ctx.captures.get(2).map(|m| m.as_str().to_string());
	let access_kind = if default_value.is_some() {
		EnvAccessKind::Optional
	} else {
		EnvAccessKind::Unknown
	};
	EnvHookResult::Records(vec![EnvHookEmission {
		var_name: Some(var_name),
		access_kind: Some(access_kind),
		default_value: Emitted::Value(default_value),
		..Default::default()
	}])
}

// ── Fs hooks ───────────────────────────────────────────────────────

/// Extract the literal first string argument from a function call's
/// argument list using a state machine that respects quotes,
/// escapes, and template literal interpolation.
///
/// Used by JS fs detectors where the regex matches the call site
/// (e.g., `\bfs.writeFile\s*\(`) and the path follows immediately.
/// Returns null if the first arg is not a literal string (dynamic).
fn extract_first_string_arg(ctx: FsHookContext<'_>) -> FsHookResult {
	let after_offset = ctx.captures.get(0).map(|m| m.end()).unwrap_or(0);
	let after = &ctx.line[after_offset..];
	let literal = extract_first_string_arg_impl(after);
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(literal.clone()),
		dynamic_path: Emitted::Value(literal.is_none()),
		..Default::default()
	}])
}

/// Extract first AND second string args for two-ended ops (rename,
/// copy). If the first arg is dynamic, both target and destination
/// are null. If the first is literal but the second is dynamic,
/// target = literal, destination = null.
fn extract_two_ended_args(ctx: FsHookContext<'_>) -> FsHookResult {
	let after_offset = ctx.captures.get(0).map(|m| m.end()).unwrap_or(0);
	let after = &ctx.line[after_offset..];
	let first = extract_first_string_arg_impl(after);
	let second = if first.is_some() {
		extract_second_string_arg_impl(after)
	} else {
		None
	};
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(first.clone()),
		destination_path: Emitted::Value(second),
		dynamic_path: Emitted::Value(first.is_none()),
		..Default::default()
	}])
}

/// Java `Files.write(Paths.get("..."))` literal unwrap.
fn unwrap_java_files_write_literal(ctx: FsHookContext<'_>) -> FsHookResult {
	let literal = JAVA_FILES_WRITE_LITERAL
		.captures(ctx.line)
		.and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(literal.clone()),
		dynamic_path: Emitted::Value(literal.is_none()),
		..Default::default()
	}])
}

fn unwrap_java_files_delete_literal(ctx: FsHookContext<'_>) -> FsHookResult {
	let literal = JAVA_FILES_DELETE_LITERAL
		.captures(ctx.line)
		.and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(literal.clone()),
		dynamic_path: Emitted::Value(literal.is_none()),
		..Default::default()
	}])
}

fn unwrap_java_files_create_directory_literal(ctx: FsHookContext<'_>) -> FsHookResult {
	let literal = JAVA_FILES_CREATE_DIRECTORY_LITERAL
		.captures(ctx.line)
		.and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(literal.clone()),
		dynamic_path: Emitted::Value(literal.is_none()),
		..Default::default()
	}])
}

/// Java `new FileOutputStream("...")` literal-or-dynamic dispatch
/// with confidence override.
///
/// Legacy semantics:
///  - if a literal FOS exists anywhere on the line: targetPath =
///    that literal, confidence = 0.85 (the static)
///  - else if a non-literal FOS call exists: targetPath = null,
///    confidence = 0.80 (overridden)
///
/// The walker default confidence (the record's static `confidence`)
/// covers the literal case (0.85). The hook overrides confidence
/// to 0.80 in the dynamic case.
fn unwrap_java_file_output_stream_literal(ctx: FsHookContext<'_>) -> FsHookResult {
	if let Some(c) = JAVA_FILE_OUTPUT_STREAM_LITERAL.captures(ctx.line) {
		let literal = c.get(1).unwrap().as_str().to_string();
		FsHookResult::Records(vec![FsHookEmission {
			target_path: Emitted::Value(Some(literal)),
			dynamic_path: Emitted::Value(false),
			..Default::default()
		}])
	} else {
		FsHookResult::Records(vec![FsHookEmission {
			target_path: Emitted::Value(None),
			dynamic_path: Emitted::Value(true),
			confidence: Some(0.80),
			..Default::default()
		}])
	}
}

/// C `fopen(path, "wa+")` mode-based dispatch.
///
/// Group 1 captures the path literal (or undefined for dynamic),
/// group 2 captures the mode string. Mode starting with "a" →
/// append_file / c_fopen_append. Otherwise → write_file /
/// c_fopen_write.
fn dispatch_c_fopen_mode(ctx: FsHookContext<'_>) -> FsHookResult {
	let path = ctx.captures.get(1).map(|m| m.as_str().to_string());
	let mode = match ctx.captures.get(2) {
		Some(m) => m.as_str(),
		None => return FsHookResult::Skip,
	};
	let is_append = mode.starts_with('a');
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(path.clone()),
		dynamic_path: Emitted::Value(path.is_none()),
		mutation_kind: Some(if is_append {
			MutationKind::AppendFile
		} else {
			MutationKind::WriteFile
		}),
		mutation_pattern: Some(if is_append {
			MutationPattern::CFopenAppend
		} else {
			MutationPattern::CFopenWrite
		}),
		..Default::default()
	}])
}

/// Python `open(path, "w"|"a")` mode-based dispatch.
fn dispatch_python_open_mode(ctx: FsHookContext<'_>) -> FsHookResult {
	let path = ctx.captures.get(1).map(|m| m.as_str().to_string());
	let mode = match ctx.captures.get(2) {
		Some(m) => m.as_str(),
		None => return FsHookResult::Skip,
	};
	let is_append = mode == "a";
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(path.clone()),
		dynamic_path: Emitted::Value(path.is_none()),
		mutation_kind: Some(if is_append {
			MutationKind::AppendFile
		} else {
			MutationKind::WriteFile
		}),
		mutation_pattern: Some(if is_append {
			MutationPattern::PyOpenAppend
		} else {
			MutationPattern::PyOpenWrite
		}),
		..Default::default()
	}])
}

/// Python `os.remove`/`os.unlink`/`shutil.rmtree` pattern dispatch.
///
/// Group 1 = call name, group 2 = literal path or undefined.
/// `mutation_kind` is always `delete_path` (supplied statically by
/// the record); only `mutation_pattern` is dispatched here.
fn dispatch_python_remove_call(ctx: FsHookContext<'_>) -> FsHookResult {
	let call_name = match ctx.captures.get(1) {
		Some(m) => m.as_str(),
		None => return FsHookResult::Skip,
	};
	let path = ctx.captures.get(2).map(|m| m.as_str().to_string());
	let pattern = if call_name == "os.unlink" {
		MutationPattern::PyOsUnlink
	} else if call_name == "shutil.rmtree" {
		MutationPattern::PyShutilRmtree
	} else {
		MutationPattern::PyOsRemove
	};
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(path.clone()),
		dynamic_path: Emitted::Value(path.is_none()),
		mutation_pattern: Some(pattern),
		..Default::default()
	}])
}

/// Python `os.mkdir`/`os.makedirs` pattern dispatch.
fn dispatch_python_mkdir_call(ctx: FsHookContext<'_>) -> FsHookResult {
	let call_name = match ctx.captures.get(1) {
		Some(m) => m.as_str(),
		None => return FsHookResult::Skip,
	};
	let path = ctx.captures.get(2).map(|m| m.as_str().to_string());
	let pattern = if call_name == "makedirs" {
		MutationPattern::PyOsMakedirs
	} else {
		MutationPattern::PyOsMkdir
	};
	FsHookResult::Records(vec![FsHookEmission {
		target_path: Emitted::Value(path.clone()),
		dynamic_path: Emitted::Value(path.is_none()),
		mutation_pattern: Some(pattern),
		..Default::default()
	}])
}

// ── State-machine string-literal extractors ───────────────────────
//
// Lifted byte-identical from `extractFirstStringArg` and
// `extractSecondStringArg` in TS fs-mutation-detectors.ts. Both
// walk bytes (not chars) for the same UTF-8 stability reason as
// the comment masker: state-transition triggers (`"`, `'`, `` ` ``,
// `\`, `$`, `{`) are all ASCII, so multi-byte continuation bytes
// flow through correctly. Result strings are sliced at byte
// boundaries that correspond to character boundaries because the
// state machine only ever transitions on ASCII bytes — the result
// is always valid UTF-8.

/// Extract the literal first string argument from a function call's
/// argument list. Returns the literal contents (with escape
/// sequences resolved to the escaped char) if the first non-
/// whitespace character is a quote; otherwise None.
///
/// `after` is the substring starting at the position right after
/// the matched call's opening `(`.
fn extract_first_string_arg_impl(after: &str) -> Option<String> {
	let bytes = after.as_bytes();
	let len = bytes.len();
	let mut i = 0;
	// Skip leading whitespace.
	while i < len && bytes[i].is_ascii_whitespace() {
		i += 1;
	}
	if i >= len {
		return None;
	}
	let quote = bytes[i];
	if quote != b'"' && quote != b'\'' && quote != b'`' {
		return None;
	}
	let mut result: Vec<u8> = Vec::new();
	i += 1;
	while i < len && bytes[i] != quote {
		// Reject template literal interpolations as dynamic.
		if quote == b'`' && bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
			return None;
		}
		// Backslash escape: append the next byte verbatim.
		if bytes[i] == b'\\' && i + 1 < len {
			result.push(bytes[i + 1]);
			i += 2;
			continue;
		}
		result.push(bytes[i]);
		i += 1;
	}
	if i >= len {
		return None;
	}
	// String::from_utf8 is safe here: every pushed byte is either
	// a verbatim copy from the valid-UTF-8 input or an escaped
	// byte from the same input. The result respects character
	// boundaries because the only state transitions are on ASCII
	// bytes (quotes, backslash, dollar/brace) which cannot appear
	// inside a multi-byte UTF-8 sequence.
	String::from_utf8(result).ok()
}

/// Extract the SECOND string literal from an argument list. Used
/// by two-ended ops (rename, copy).
fn extract_second_string_arg_impl(after: &str) -> Option<String> {
	let bytes = after.as_bytes();
	let len = bytes.len();
	let mut i = 0;
	// Skip leading whitespace.
	while i < len && bytes[i].is_ascii_whitespace() {
		i += 1;
	}
	if i >= len {
		return None;
	}
	let first_quote = bytes[i];
	if first_quote != b'"' && first_quote != b'\'' && first_quote != b'`' {
		return None;
	}
	i += 1;
	// Skip past the first arg's closing quote.
	while i < len && bytes[i] != first_quote {
		if bytes[i] == b'\\' {
			i += 2;
			continue;
		}
		i += 1;
	}
	if i >= len {
		return None;
	}
	i += 1; // past closing quote
	// Skip whitespace and comma.
	while i < len && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
		i += 1;
	}
	if i >= len {
		return None;
	}
	// Now extract the second string literal.
	let second_quote = bytes[i];
	if second_quote != b'"' && second_quote != b'\'' && second_quote != b'`' {
		return None;
	}
	let mut result: Vec<u8> = Vec::new();
	i += 1;
	while i < len && bytes[i] != second_quote {
		if second_quote == b'`' && bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
			return None;
		}
		if bytes[i] == b'\\' && i + 1 < len {
			result.push(bytes[i + 1]);
			i += 2;
			continue;
		}
		result.push(bytes[i]);
		i += 1;
	}
	if i >= len {
		return None;
	}
	String::from_utf8(result).ok()
}

// ── Registry construction ──────────────────────────────────────────

/// Build the canonical hook registry for the production detector
/// graph.
///
/// The set of registered hooks must match the set of hook names
/// referenced in `detectors.toml`. The loader rejects any record
/// that references an unregistered hook. Each entry's `supplies`
/// set is the contract surface for the loader's must-be-supplied
/// check.
///
/// Adding a new hook requires:
///   1. defining the hook function above
///   2. registering it here with the correct supplies set
///   3. (if applicable) referencing it from `detectors.toml`
///
/// The Rust type system enforces that every supplies field name is
/// a member of `EnvSuppliedField` / `FsSuppliedField`, so typos in
/// the supplies set become compile errors.
pub fn create_default_hook_registry() -> HookRegistry {
	let mut env: HashMap<&'static str, EnvHookEntry> = HashMap::new();
	let mut fs: HashMap<&'static str, FsHookEntry> = HashMap::new();

	// ── env ────────────────────────────────────────────────────────
	env.insert(
		"infer_js_env_access_with_default",
		EnvHookEntry {
			func: infer_js_env_access_with_default,
			supplies: env_supplies(&[
				EnvSuppliedField::VarName,
				EnvSuppliedField::AccessKind,
				EnvSuppliedField::DefaultValue,
			]),
		},
	);
	env.insert(
		"expand_js_env_destructure",
		EnvHookEntry {
			func: expand_js_env_destructure,
			supplies: env_supplies(&[
				EnvSuppliedField::VarName,
				EnvSuppliedField::AccessKind,
				EnvSuppliedField::DefaultValue,
			]),
		},
	);
	env.insert(
		"python_get_default_from_group2",
		EnvHookEntry {
			func: python_get_default_from_group2,
			supplies: env_supplies(&[
				EnvSuppliedField::VarName,
				EnvSuppliedField::AccessKind,
				EnvSuppliedField::DefaultValue,
			]),
		},
	);

	// ── fs ─────────────────────────────────────────────────────────
	fs.insert(
		"extract_first_string_arg",
		FsHookEntry {
			func: extract_first_string_arg,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
			]),
		},
	);
	fs.insert(
		"extract_two_ended_args",
		FsHookEntry {
			func: extract_two_ended_args,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DestinationPath,
				FsSuppliedField::DynamicPath,
			]),
		},
	);
	fs.insert(
		"unwrap_java_files_write_literal",
		FsHookEntry {
			func: unwrap_java_files_write_literal,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
			]),
		},
	);
	fs.insert(
		"unwrap_java_files_delete_literal",
		FsHookEntry {
			func: unwrap_java_files_delete_literal,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
			]),
		},
	);
	fs.insert(
		"unwrap_java_files_create_directory_literal",
		FsHookEntry {
			func: unwrap_java_files_create_directory_literal,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
			]),
		},
	);
	fs.insert(
		"unwrap_java_file_output_stream_literal",
		FsHookEntry {
			func: unwrap_java_file_output_stream_literal,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
			]),
		},
	);
	fs.insert(
		"dispatch_c_fopen_mode",
		FsHookEntry {
			func: dispatch_c_fopen_mode,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
				FsSuppliedField::MutationKind,
				FsSuppliedField::MutationPattern,
			]),
		},
	);
	fs.insert(
		"dispatch_python_open_mode",
		FsHookEntry {
			func: dispatch_python_open_mode,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
				FsSuppliedField::MutationKind,
				FsSuppliedField::MutationPattern,
			]),
		},
	);
	fs.insert(
		"dispatch_python_remove_call",
		FsHookEntry {
			func: dispatch_python_remove_call,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
				FsSuppliedField::MutationPattern,
			]),
		},
	);
	fs.insert(
		"dispatch_python_mkdir_call",
		FsHookEntry {
			func: dispatch_python_mkdir_call,
			supplies: fs_supplies(&[
				FsSuppliedField::TargetPath,
				FsSuppliedField::DynamicPath,
				FsSuppliedField::MutationPattern,
			]),
		},
	);

	HookRegistry { env, fs }
}

// ── Helpers ───────────────────────────────────────────────────────

fn env_supplies(fields: &[EnvSuppliedField]) -> std::collections::HashSet<EnvSuppliedField> {
	fields.iter().copied().collect()
}

fn fs_supplies(fields: &[FsSuppliedField]) -> std::collections::HashSet<FsSuppliedField> {
	fields.iter().copied().collect()
}
