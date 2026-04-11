//! Detector graph loader with strict validation.
//!
//! Loads a TOML detector graph and produces a `LoadedDetectorGraph`
//! ready for walker consumption. The loader is the gate for every
//! malformed declaration: any error is a hard return at load time,
//! NOT silent coercion at runtime.
//!
//! Hard validation rules (mirror of `loader.ts`):
//!  - unknown family / language values reject
//!  - missing required fields reject (no defaults for required)
//!  - invalid regex (parse error) rejects
//!  - unknown hook name (not in the supplied registry) rejects
//!  - duplicate (language, family, pattern_id) rejects
//!  - confidence outside [0, 1] rejects
//!  - regex flags outside the allowed set reject
//!  - unknown top-level keys reject (defense against typos, via
//!    serde `deny_unknown_fields` on `RawDetectorDocument`)
//!  - per-family field allowlist: env records cannot declare
//!    fs-only fields and vice versa
//!  - position_dedup_group is fs-only and rejects empty strings
//!  - must-be-supplied check: every required output field must be
//!    reachable from either a static schema field on the record or
//!    the referenced hook's `supplies` set
//!
//! Pure given a registry. Caller is responsible for reading the
//! file. The loader is called once at module-import time by the
//! production graph singleton.
//!
//! Validation discipline: the loader proves REACHABILITY of
//! required fields, not walker fallback simulation. The walker is
//! the source of truth for merge semantics; the loader only
//! verifies that the merged result will be well-formed.

use std::collections::{HashMap, HashSet};

use regex::{Regex, RegexBuilder};

use crate::types::{
	env_must_be_supplied, fs_must_be_supplied, DetectorFamily,
	DetectorLanguage, DetectorRecord, EnvAccessKind, EnvAccessPattern,
	EnvDetectorRecord, EnvSuppliedField, FsDetectorRecord, FsSuppliedField,
	HookRegistry, LoadedDetectorGraph, MutationKind, MutationPattern,
	RawDetectorDocument, RawDetectorRecord,
};

// ── Public error type ─────────────────────────────────────────────

/// Thrown by `load_detector_graph` on any validation failure.
///
/// Mirror of TS `DetectorLoadError`. The message identifies the
/// failing record by index and pattern_id (when available) so
/// callers and authors can locate the offending declaration in the
/// source TOML quickly.
#[derive(Debug, thiserror::Error)]
pub enum DetectorLoadError {
	/// TOML parse failed (invalid syntax, type mismatch in raw
	/// deserialization, unknown top-level key, etc.).
	#[error("TOML parse error: {0}")]
	TomlParse(String),

	/// Per-record validation failure. The message includes the
	/// record index and pattern_id (when available) plus a
	/// human-readable description of the violated rule.
	#[error("{0}")]
	Validation(String),
}

impl From<toml::de::Error> for DetectorLoadError {
	fn from(e: toml::de::Error) -> Self {
		DetectorLoadError::TomlParse(e.to_string())
	}
}

// ── Public entry point ────────────────────────────────────────────

/// Parse and validate a TOML detector graph string.
///
/// `toml_source` is the raw TOML text. `registry` is the hook
/// registry whose entries will be referenced by `hook = "..."`
/// declarations in the source. Returns the loaded, validated,
/// indexed detector graph or a `DetectorLoadError`.
pub fn load_detector_graph(
	toml_source: &str,
	registry: &HookRegistry,
) -> Result<LoadedDetectorGraph, DetectorLoadError> {
	let document: RawDetectorDocument = toml::from_str(toml_source)?;

	let mut records: Vec<DetectorRecord> = Vec::with_capacity(document.detector.len());
	let mut seen_keys: HashSet<(DetectorLanguage, DetectorFamily, String)> = HashSet::new();

	for (i, raw) in document.detector.into_iter().enumerate() {
		let record = validate_record(raw, i, registry)?;

		// Duplicate check: (language, family, pattern_id) must be unique.
		let key = match &record {
			DetectorRecord::Env(r) => {
				(r.language, DetectorFamily::Env, r.pattern_id.clone())
			}
			DetectorRecord::Fs(r) => {
				(r.language, DetectorFamily::Fs, r.pattern_id.clone())
			}
		};
		if !seen_keys.insert(key.clone()) {
			let pattern_id = match &record {
				DetectorRecord::Env(r) => &r.pattern_id,
				DetectorRecord::Fs(r) => &r.pattern_id,
			};
			return Err(DetectorLoadError::Validation(format!(
				"detector[{i}] ({pattern_id}): duplicate (language={lang}, family={fam}, pattern_id={pattern_id}). Each combination must be unique.",
				lang = format_language(key.0),
				fam = format_family(key.1),
			)));
		}

		records.push(record);
	}

	// Build the (family, language) index, preserving original
	// declaration order within each bucket. Walker dispatch is one
	// HashMap lookup per call; iteration order within a bucket is
	// the declaration order, which the walker relies on for
	// deterministic env dedup behavior.
	let mut by_family_language: HashMap<
		(DetectorFamily, DetectorLanguage),
		Vec<DetectorRecord>,
	> = HashMap::new();
	for r in &records {
		let key = r.family_language();
		by_family_language
			.entry(key)
			.or_default()
			.push(r.clone());
	}

	Ok(LoadedDetectorGraph {
		all_records: records,
		by_family_language,
	})
}

// ── Per-record validation ─────────────────────────────────────────

fn validate_record(
	raw: RawDetectorRecord,
	index: usize,
	registry: &HookRegistry,
) -> Result<DetectorRecord, DetectorLoadError> {
	let ctx = format!("detector[{index}]");

	// language (required)
	let language_str = require_string(&raw.language, "language", &ctx)?;
	let language = parse_language(&language_str, &ctx)?;

	// family (required)
	let family_str = require_string(&raw.family, "family", &ctx)?;
	let family = parse_family(&family_str, &ctx)?;

	// pattern_id (required, non-empty)
	let pattern_id = require_string(&raw.pattern_id, "pattern_id", &ctx)?;
	let pattern_ctx = format!("{ctx} ({pattern_id})");

	// regex (required, non-empty)
	let regex_source = require_string(&raw.regex, "regex", &ctx)?;

	// flags (optional, default "g")
	let flags = raw.flags.clone().unwrap_or_else(|| "g".to_string());
	if flags.is_empty() {
		// Empty string is treated as "no flags" — equivalent to "".
		// The TS loader does not allow empty flags via optional_string,
		// so an explicit empty string in the TOML rejects.
		if raw.flags.is_some() {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"flags\" must not be empty"
			)));
		}
	}
	for ch in flags.chars() {
		if !is_allowed_flag(ch) {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: unknown regex flag \"{ch}\". Allowed: gimsuy"
			)));
		}
	}

	// confidence (required, finite, [0, 1])
	let confidence = raw.confidence.ok_or_else(|| {
		DetectorLoadError::Validation(format!(
			"{ctx}: missing required field \"confidence\""
		))
	})?;
	if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: confidence must be a finite number in [0, 1], got {confidence}"
		)));
	}

	// hook (optional, non-empty if present)
	let hook = match raw.hook.as_deref() {
		None => None,
		Some("") => {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"hook\" must not be empty"
			)));
		}
		Some(h) => Some(h.to_string()),
	};

	// single_match_per_line (optional, default false)
	let single_match_per_line = raw.single_match_per_line.unwrap_or(false);

	// Compile regex with flags. Fail fast on syntax errors.
	let compiled_regex = compile_regex(&regex_source, &flags, &pattern_ctx)?;

	// Per-family validation diverges here.
	match family {
		DetectorFamily::Env => validate_env_record(
			raw,
			language,
			pattern_id,
			regex_source,
			flags,
			compiled_regex,
			confidence,
			hook,
			single_match_per_line,
			registry,
			&pattern_ctx,
		)
		.map(DetectorRecord::Env),
		DetectorFamily::Fs => validate_fs_record(
			raw,
			language,
			pattern_id,
			regex_source,
			flags,
			compiled_regex,
			confidence,
			hook,
			single_match_per_line,
			registry,
			&pattern_ctx,
		)
		.map(DetectorRecord::Fs),
	}
}

// ── Env-family validation ─────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn validate_env_record(
	raw: RawDetectorRecord,
	language: DetectorLanguage,
	pattern_id: String,
	regex_source: String,
	flags: String,
	compiled_regex: Regex,
	confidence: f64,
	hook: Option<String>,
	single_match_per_line: bool,
	registry: &HookRegistry,
	pattern_ctx: &str,
) -> Result<EnvDetectorRecord, DetectorLoadError> {
	// Reject fs-only fields on env records.
	if raw.base_kind.is_some() {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: unknown field \"base_kind\" for env family. Allowed: language, family, pattern_id, regex, flags, confidence, hook, single_match_per_line, access_kind, access_pattern"
		)));
	}
	if raw.base_pattern.is_some() {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: unknown field \"base_pattern\" for env family. Allowed: language, family, pattern_id, regex, flags, confidence, hook, single_match_per_line, access_kind, access_pattern"
		)));
	}
	if raw.two_ended.is_some() {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: unknown field \"two_ended\" for env family. Allowed: language, family, pattern_id, regex, flags, confidence, hook, single_match_per_line, access_kind, access_pattern"
		)));
	}
	if raw.position_dedup_group.is_some() {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: unknown field \"position_dedup_group\" for env family. Allowed: language, family, pattern_id, regex, flags, confidence, hook, single_match_per_line, access_kind, access_pattern"
		)));
	}

	// access_kind (optional)
	let access_kind = match raw.access_kind.as_deref() {
		None => None,
		Some("") => {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"access_kind\" must not be empty"
			)));
		}
		Some(s) => Some(parse_access_kind(s, pattern_ctx)?),
	};

	// access_pattern (optional)
	let access_pattern = match raw.access_pattern.as_deref() {
		None => None,
		Some("") => {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"access_pattern\" must not be empty"
			)));
		}
		Some(s) => Some(parse_access_pattern(s, pattern_ctx)?),
	};

	// Hook reference resolution and supplies-set lookup.
	let hook_supplies: HashSet<EnvSuppliedField> = if let Some(name) = &hook {
		match registry.env.get(name.as_str()) {
			Some(entry) => entry.supplies.clone(),
			None => {
				let registered = if registry.env.is_empty() {
					"(none)".to_string()
				} else {
					let mut keys: Vec<&&str> = registry.env.keys().collect();
					keys.sort();
					keys.iter().map(|k| **k).collect::<Vec<_>>().join(", ")
				};
				return Err(DetectorLoadError::Validation(format!(
					"{pattern_ctx}: unknown env hook \"{name}\". Registered: {registered}"
				)));
			}
		}
	} else {
		HashSet::new()
	};

	// Must-be-supplied check: every required env output field must
	// be reachable from either a static field or the hook's
	// supplies set. The loader proves reachability; the walker is
	// the source of truth for merge behavior.
	let mut static_supplied: HashSet<EnvSuppliedField> = HashSet::new();
	if access_kind.is_some() {
		static_supplied.insert(EnvSuppliedField::AccessKind);
	}
	if access_pattern.is_some() {
		static_supplied.insert(EnvSuppliedField::AccessPattern);
	}
	for required in env_must_be_supplied() {
		if !static_supplied.contains(&required)
			&& !hook_supplies.contains(&required)
		{
			let toml_field = match required {
				EnvSuppliedField::AccessKind => "access_kind",
				EnvSuppliedField::AccessPattern => "access_pattern",
				EnvSuppliedField::VarName => "var_name",
				EnvSuppliedField::DefaultValue => "default_value",
			};
			let field_name = format_env_field(required);
			let hook_label = hook.as_deref().unwrap_or("(none)");
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: required env field \"{field_name}\" is neither declared statically nor supplied by hook \"{hook_label}\". Either set \"{toml_field}\" in the record or register a hook whose supplies set includes \"{field_name}\"."
			)));
		}
	}

	Ok(EnvDetectorRecord {
		language,
		family: DetectorFamily::Env,
		pattern_id,
		regex: regex_source,
		flags,
		compiled_regex,
		confidence,
		hook,
		single_match_per_line,
		access_kind,
		access_pattern,
	})
}

// ── Fs-family validation ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn validate_fs_record(
	raw: RawDetectorRecord,
	language: DetectorLanguage,
	pattern_id: String,
	regex_source: String,
	flags: String,
	compiled_regex: Regex,
	confidence: f64,
	hook: Option<String>,
	single_match_per_line: bool,
	registry: &HookRegistry,
	pattern_ctx: &str,
) -> Result<FsDetectorRecord, DetectorLoadError> {
	// Reject env-only fields on fs records.
	if raw.access_kind.is_some() {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: unknown field \"access_kind\" for fs family. Allowed: language, family, pattern_id, regex, flags, confidence, hook, single_match_per_line, base_kind, base_pattern, two_ended, position_dedup_group"
		)));
	}
	if raw.access_pattern.is_some() {
		return Err(DetectorLoadError::Validation(format!(
			"{pattern_ctx}: unknown field \"access_pattern\" for fs family. Allowed: language, family, pattern_id, regex, flags, confidence, hook, single_match_per_line, base_kind, base_pattern, two_ended, position_dedup_group"
		)));
	}

	// base_kind (optional)
	let base_kind = match raw.base_kind.as_deref() {
		None => None,
		Some("") => {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"base_kind\" must not be empty"
			)));
		}
		Some(s) => Some(parse_mutation_kind(s, pattern_ctx)?),
	};

	// base_pattern (optional)
	let base_pattern = match raw.base_pattern.as_deref() {
		None => None,
		Some("") => {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"base_pattern\" must not be empty"
			)));
		}
		Some(s) => Some(parse_mutation_pattern(s, pattern_ctx)?),
	};

	// two_ended (optional, default false)
	let two_ended = raw.two_ended.unwrap_or(false);

	// position_dedup_group (optional, fs-only, non-empty if present)
	let position_dedup_group = match raw.position_dedup_group.as_deref() {
		None => None,
		Some("") => {
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: field \"position_dedup_group\" must not be empty"
			)));
		}
		Some(s) => Some(s.to_string()),
	};

	// Hook reference resolution.
	let hook_supplies: HashSet<FsSuppliedField> = if let Some(name) = &hook {
		match registry.fs.get(name.as_str()) {
			Some(entry) => entry.supplies.clone(),
			None => {
				let registered = if registry.fs.is_empty() {
					"(none)".to_string()
				} else {
					let mut keys: Vec<&&str> = registry.fs.keys().collect();
					keys.sort();
					keys.iter().map(|k| **k).collect::<Vec<_>>().join(", ")
				};
				return Err(DetectorLoadError::Validation(format!(
					"{pattern_ctx}: unknown fs hook \"{name}\". Registered: {registered}"
				)));
			}
		}
	} else {
		HashSet::new()
	};

	// Must-be-supplied check.
	let mut static_supplied: HashSet<FsSuppliedField> = HashSet::new();
	if base_kind.is_some() {
		static_supplied.insert(FsSuppliedField::MutationKind);
	}
	if base_pattern.is_some() {
		static_supplied.insert(FsSuppliedField::MutationPattern);
	}
	for required in fs_must_be_supplied() {
		if !static_supplied.contains(&required)
			&& !hook_supplies.contains(&required)
		{
			let toml_field = match required {
				FsSuppliedField::MutationKind => "base_kind",
				FsSuppliedField::MutationPattern => "base_pattern",
				FsSuppliedField::TargetPath => "target_path",
				FsSuppliedField::DestinationPath => "destination_path",
				FsSuppliedField::DynamicPath => "dynamic_path",
			};
			let field_name = format_fs_field(required);
			let hook_label = hook.as_deref().unwrap_or("(none)");
			return Err(DetectorLoadError::Validation(format!(
				"{pattern_ctx}: required fs field \"{field_name}\" is neither declared statically nor supplied by hook \"{hook_label}\". Either set \"{toml_field}\" in the record or register a hook whose supplies set includes \"{field_name}\"."
			)));
		}
	}

	Ok(FsDetectorRecord {
		language,
		family: DetectorFamily::Fs,
		pattern_id,
		regex: regex_source,
		flags,
		compiled_regex,
		confidence,
		hook,
		single_match_per_line,
		base_kind,
		base_pattern,
		two_ended,
		position_dedup_group,
	})
}

// ── Field extraction helpers ─────────────────────────────────────

fn require_string(
	field: &Option<String>,
	field_name: &str,
	ctx: &str,
) -> Result<String, DetectorLoadError> {
	match field.as_deref() {
		None => Err(DetectorLoadError::Validation(format!(
			"{ctx}: missing required field \"{field_name}\""
		))),
		Some("") => Err(DetectorLoadError::Validation(format!(
			"{ctx}: field \"{field_name}\" must not be empty"
		))),
		Some(s) => Ok(s.to_string()),
	}
}

// ── Regex compilation ────────────────────────────────────────────

/// Compile a regex source string with TS-style flag characters into
/// a Rust `regex::Regex`. Translates the cross-runtime flag set
/// into Rust regex builder calls or hard-rejects flags that the
/// Rust runtime cannot honor.
///
/// Flag handling:
///
///  - `g` (global): implicitly enabled by Rust's match-iteration
///    model — all matches are returned by `find_iter` /
///    `captures_iter`. Accepted as a no-op so the same TOML works
///    for both runtimes without modification.
///
///  - `i` (case-insensitive): translated to
///    `RegexBuilder::case_insensitive(true)`.
///
///  - `m` (multi-line): translated to `RegexBuilder::multi_line(true)`.
///
///  - `s` (dotall): translated to
///    `RegexBuilder::dot_matches_new_line(true)`.
///
///  - `u` (unicode): enabled by default in Rust regex; accepted as
///    a no-op.
///
///  - `y` (sticky): **HARD REJECTED.** The TS sticky flag forces
///    matches to occur exactly at `lastIndex`, not anywhere from
///    that index onward. Rust's `regex` crate has no equivalent
///    primitive. Silently treating `y` as a no-op (the previous
///    behavior) would degrade sticky semantics to global, which is
///    a material divergence from the TS contract. Detector authors
///    who declare `y` get an explicit error naming the cause.
///
/// If a future detector genuinely needs sticky semantics, the
/// resolution is either to escalate to a schema extension that
/// declares the requirement explicitly, or to implement sticky
/// matching by calling `regex::Regex::find_at` with bounded
/// position checking. Either path requires a deliberate decision,
/// not silent acceptance.
fn compile_regex(
	source: &str,
	flags: &str,
	ctx: &str,
) -> Result<Regex, DetectorLoadError> {
	let mut builder = RegexBuilder::new(source);
	for ch in flags.chars() {
		match ch {
			'g' | 'u' => {
				// no-op (see doc comment)
			}
			'i' => {
				builder.case_insensitive(true);
			}
			'm' => {
				builder.multi_line(true);
			}
			's' => {
				builder.dot_matches_new_line(true);
			}
			'y' => {
				// Sticky semantics have no Rust regex equivalent.
				// Reject explicitly rather than silently degrade.
				return Err(DetectorLoadError::Validation(format!(
					"{ctx}: regex flag \"y\" (sticky) has no Rust regex equivalent and is not supported by this runtime. Sticky matching forces matches to occur exactly at the iteration index, which Rust's regex crate does not provide as a primitive. Remove the flag or escalate to a schema extension that declares sticky semantics explicitly."
				)));
			}
			_ => {
				// Already validated by is_allowed_flag, but defense
				// in depth in case validation is bypassed.
				return Err(DetectorLoadError::Validation(format!(
					"{ctx}: unknown regex flag \"{ch}\""
				)));
			}
		}
	}
	let compiled = builder.build().map_err(|e| {
		DetectorLoadError::Validation(format!(
			"{ctx}: regex compile error: {e}"
		))
	})?;

	// Reject empty-match-capable regexes explicitly.
	//
	// The TS walker guards zero-width matches by manually
	// incrementing `regex.lastIndex` after an empty match. The
	// Rust walker uses `regex::Regex::captures_iter` which handles
	// empty-match advancement internally — but the advancement
	// rule differs from JS RegExp.exec on non-ASCII input (Rust
	// advances by one char boundary; JS advances by one UTF-16
	// code unit). On non-BMP input, a future empty-match-capable
	// detector regex could produce different output between the
	// two runtimes.
	//
	// Preventing the problem at load time is cleaner than trying
	// to re-implement JS iteration semantics in Rust: Rust's `str`
	// type forbids sub-character indexing, so we cannot step
	// through a UTF-16 code unit inside a multi-byte UTF-8
	// sequence in safe Rust.
	//
	// Detection: a regex is empty-match-capable iff it matches the
	// empty string. This check catches `a*`, `a?`, `(?:)`, `a*|b*`,
	// and every other shape where the regex can produce a
	// zero-width match. Production detector regexes all require at
	// least one literal character and are not empty-match-capable.
	if compiled.is_match("") {
		return Err(DetectorLoadError::Validation(format!(
			"{ctx}: regex is empty-match-capable (matches the empty string). Zero-width matches have divergent iteration semantics between the TypeScript walker (advances by one UTF-16 code unit) and the Rust walker (advances by one char boundary), producing different output on non-BMP input. Rewrite the regex to require at least one mandatory character — e.g., replace `a*` with `a+` or anchor the pattern with a literal prefix."
		)));
	}

	Ok(compiled)
}

/// Recognized TS regex flag characters. The Rust runtime accepts
/// these at the validation layer (so a TOML declaring `flags = "y"`
/// passes the field-shape check) and then either translates them,
/// no-ops them, or hard-rejects them in `compile_regex`. The
/// per-flag dispatch in `compile_regex` is the source of truth for
/// runtime behavior; this set only enumerates which characters are
/// part of the cross-runtime flag vocabulary.
fn is_allowed_flag(ch: char) -> bool {
	matches!(ch, 'g' | 'i' | 'm' | 's' | 'u' | 'y')
}

// ── Enum parsers (string -> enum variant) ────────────────────────

fn parse_language(s: &str, ctx: &str) -> Result<DetectorLanguage, DetectorLoadError> {
	match s {
		"ts" => Ok(DetectorLanguage::Ts),
		"py" => Ok(DetectorLanguage::Py),
		"rs" => Ok(DetectorLanguage::Rs),
		"java" => Ok(DetectorLanguage::Java),
		"c" => Ok(DetectorLanguage::C),
		_ => Err(DetectorLoadError::Validation(format!(
			"{ctx}: unknown language \"{s}\". Allowed: ts, py, rs, java, c"
		))),
	}
}

fn parse_family(s: &str, ctx: &str) -> Result<DetectorFamily, DetectorLoadError> {
	match s {
		"env" => Ok(DetectorFamily::Env),
		"fs" => Ok(DetectorFamily::Fs),
		_ => Err(DetectorLoadError::Validation(format!(
			"{ctx}: unknown family \"{s}\". Allowed: env, fs"
		))),
	}
}

fn parse_access_kind(
	s: &str,
	ctx: &str,
) -> Result<EnvAccessKind, DetectorLoadError> {
	match s {
		"required" => Ok(EnvAccessKind::Required),
		"optional" => Ok(EnvAccessKind::Optional),
		"unknown" => Ok(EnvAccessKind::Unknown),
		_ => Err(DetectorLoadError::Validation(format!(
			"{ctx}: unknown access_kind \"{s}\". Allowed: required, optional, unknown"
		))),
	}
}

fn parse_access_pattern(
	s: &str,
	ctx: &str,
) -> Result<EnvAccessPattern, DetectorLoadError> {
	match s {
		"process_env_dot" => Ok(EnvAccessPattern::ProcessEnvDot),
		"process_env_bracket" => Ok(EnvAccessPattern::ProcessEnvBracket),
		"process_env_destructure" => Ok(EnvAccessPattern::ProcessEnvDestructure),
		"os_environ" => Ok(EnvAccessPattern::OsEnviron),
		"os_getenv" => Ok(EnvAccessPattern::OsGetenv),
		"std_env_var" => Ok(EnvAccessPattern::StdEnvVar),
		"system_getenv" => Ok(EnvAccessPattern::SystemGetenv),
		"c_getenv" => Ok(EnvAccessPattern::CGetenv),
		"dotenv_reference" => Ok(EnvAccessPattern::DotenvReference),
		_ => Err(DetectorLoadError::Validation(format!(
			"{ctx}: unknown access_pattern \"{s}\""
		))),
	}
}

fn parse_mutation_kind(
	s: &str,
	ctx: &str,
) -> Result<MutationKind, DetectorLoadError> {
	match s {
		"write_file" => Ok(MutationKind::WriteFile),
		"append_file" => Ok(MutationKind::AppendFile),
		"delete_path" => Ok(MutationKind::DeletePath),
		"create_dir" => Ok(MutationKind::CreateDir),
		"rename_path" => Ok(MutationKind::RenamePath),
		"copy_path" => Ok(MutationKind::CopyPath),
		"chmod_path" => Ok(MutationKind::ChmodPath),
		"create_temp" => Ok(MutationKind::CreateTemp),
		_ => Err(DetectorLoadError::Validation(format!(
			"{ctx}: unknown base_kind \"{s}\""
		))),
	}
}

fn parse_mutation_pattern(
	s: &str,
	ctx: &str,
) -> Result<MutationPattern, DetectorLoadError> {
	match s {
		"fs_write_file" => Ok(MutationPattern::FsWriteFile),
		"fs_append_file" => Ok(MutationPattern::FsAppendFile),
		"fs_unlink" => Ok(MutationPattern::FsUnlink),
		"fs_rm" => Ok(MutationPattern::FsRm),
		"fs_mkdir" => Ok(MutationPattern::FsMkdir),
		"fs_create_write_stream" => Ok(MutationPattern::FsCreateWriteStream),
		"fs_rename" => Ok(MutationPattern::FsRename),
		"fs_copy_file" => Ok(MutationPattern::FsCopyFile),
		"fs_chmod" => Ok(MutationPattern::FsChmod),
		"py_open_write" => Ok(MutationPattern::PyOpenWrite),
		"py_open_append" => Ok(MutationPattern::PyOpenAppend),
		"py_os_remove" => Ok(MutationPattern::PyOsRemove),
		"py_os_unlink" => Ok(MutationPattern::PyOsUnlink),
		"py_shutil_rmtree" => Ok(MutationPattern::PyShutilRmtree),
		"py_os_mkdir" => Ok(MutationPattern::PyOsMkdir),
		"py_os_makedirs" => Ok(MutationPattern::PyOsMakedirs),
		"py_pathlib_write" => Ok(MutationPattern::PyPathlibWrite),
		"py_pathlib_mkdir" => Ok(MutationPattern::PyPathlibMkdir),
		"py_tempfile" => Ok(MutationPattern::PyTempfile),
		"rust_fs_write" => Ok(MutationPattern::RustFsWrite),
		"rust_fs_remove_file" => Ok(MutationPattern::RustFsRemoveFile),
		"rust_fs_remove_dir_all" => Ok(MutationPattern::RustFsRemoveDirAll),
		"rust_fs_create_dir" => Ok(MutationPattern::RustFsCreateDir),
		"rust_fs_rename" => Ok(MutationPattern::RustFsRename),
		"rust_fs_copy" => Ok(MutationPattern::RustFsCopy),
		"java_files_write" => Ok(MutationPattern::JavaFilesWrite),
		"java_files_delete" => Ok(MutationPattern::JavaFilesDelete),
		"java_files_create_directory" => Ok(MutationPattern::JavaFilesCreateDirectory),
		"java_file_output_stream" => Ok(MutationPattern::JavaFileOutputStream),
		"c_fopen_write" => Ok(MutationPattern::CFopenWrite),
		"c_fopen_append" => Ok(MutationPattern::CFopenAppend),
		"c_unlink" => Ok(MutationPattern::CUnlink),
		"c_remove" => Ok(MutationPattern::CRemove),
		"c_rmdir" => Ok(MutationPattern::CRmdir),
		"c_mkdir" => Ok(MutationPattern::CMkdir),
		_ => Err(DetectorLoadError::Validation(format!(
			"{ctx}: unknown base_pattern \"{s}\""
		))),
	}
}

// ── Display helpers (for error messages) ─────────────────────────

fn format_language(l: DetectorLanguage) -> &'static str {
	match l {
		DetectorLanguage::Ts => "ts",
		DetectorLanguage::Py => "py",
		DetectorLanguage::Rs => "rs",
		DetectorLanguage::Java => "java",
		DetectorLanguage::C => "c",
	}
}

fn format_family(f: DetectorFamily) -> &'static str {
	match f {
		DetectorFamily::Env => "env",
		DetectorFamily::Fs => "fs",
	}
}

fn format_env_field(f: EnvSuppliedField) -> &'static str {
	match f {
		EnvSuppliedField::VarName => "varName",
		EnvSuppliedField::AccessKind => "accessKind",
		EnvSuppliedField::AccessPattern => "accessPattern",
		EnvSuppliedField::DefaultValue => "defaultValue",
	}
}

fn format_fs_field(f: FsSuppliedField) -> &'static str {
	match f {
		FsSuppliedField::TargetPath => "targetPath",
		FsSuppliedField::DestinationPath => "destinationPath",
		FsSuppliedField::MutationKind => "mutationKind",
		FsSuppliedField::MutationPattern => "mutationPattern",
		FsSuppliedField::DynamicPath => "dynamicPath",
	}
}
