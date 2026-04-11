//! Generic detector graph walker.
//!
//! Rust mirror of `src/core/seams/detectors/walker.ts`. Consumes a
//! `LoadedDetectorGraph` plus a hook registry and produces
//! `DetectedEnvDependency` / `DetectedFsMutation` for one source
//! file.
//!
//! The walker is the procedural counterpart to the declarative
//! detector graph: it iterates lines, runs each record's compiled
//! regex, dispatches to hooks when present, and merges hook output
//! with the static record's base fields.
//!
//! Walker default behavior (must match types.rs documentation and
//! the TS walker exactly):
//!  - env varName ← regex capture group 1 (when hook does not supply)
//!  - fs targetPath ← regex capture group 1 (when hook does not supply)
//!  - fs destinationPath ← regex capture group 2 (when two_ended)
//!  - fs dynamicPath ← derived from targetPath == None
//!  - confidence ← record.confidence
//!  - filePath / lineNumber ← walker context
//!
//! Env-family deduplication: the walker tracks `(file_path, var_name)`
//! across all matches in a single call and skips records whose key
//! has already been emitted. First-occurrence-wins.
//!
//! Fs-family position dedup: per-line, per-group seen-position sets.
//! Records with `position_dedup_group == None` always emit. Records
//! sharing a non-null group name share a per-line seen-position set.
//!
//! `single_match_per_line`: after the first regex hit on a line for a
//! given record, the walker breaks out of the inner regex iteration
//! for that record on that line. Preserves legacy single-match-per
//! line semantics.
//!
//! Line iteration uses `content.split('\n')` rather than `lines()`
//! because Rust's `str::lines()` strips trailing `\r` characters
//! while TS `split("\n")` does not. For byte-identical behavior
//! under CRLF inputs, `split` is the correct choice.
//!
//! Zero-width match policy: the walker uses `regex::Regex::captures_iter`
//! which handles empty-match advancement internally. This is safe
//! because the loader rejects empty-match-capable regexes at load
//! time (see `loader::compile_regex`) — the walker is guaranteed
//! to only see regexes that require at least one mandatory
//! character, so `captures_iter`'s empty-match advancement rule
//! cannot produce cross-runtime divergence with the TS walker. If
//! a future detector is authored with an empty-match-capable
//! regex, the loader catches it before the walker ever runs.
//!
//! Pure function. No I/O. No external dependencies beyond the loaded
//! graph and registry passed in.

use std::collections::{HashMap, HashSet};

use regex::Captures;

use crate::types::{
	DetectedEnvDependency, DetectedFsMutation, DetectorFamily, DetectorLanguage,
	DetectorRecord, Emitted, EnvDetectorRecord, EnvHookContext, EnvHookResult,
	FsDetectorRecord, FsHookContext, FsHookResult, HookRegistry, LoadedDetectorGraph,
};

// ── Public entry points ───────────────────────────────────────────

/// Walk env-family detectors for one source file.
///
/// Returns `DetectedEnvDependency` records in walker iteration
/// order with env-family deduplication already applied. Caller is
/// responsible for passing content through the comment masker
/// before calling this function.
pub fn walk_env_detectors(
	graph: &LoadedDetectorGraph,
	registry: &HookRegistry,
	language: DetectorLanguage,
	content: &str,
	file_path: &str,
) -> Vec<DetectedEnvDependency> {
	let records = match graph
		.by_family_language
		.get(&(DetectorFamily::Env, language))
	{
		Some(rs) => rs,
		None => return Vec::new(),
	};

	let mut results: Vec<DetectedEnvDependency> = Vec::new();
	// Dedup key: (file_path, var_name). We use String for the
	// file_path portion even though it's already an &str parameter,
	// because the HashSet must own its keys across line iterations.
	let mut seen: HashSet<(String, String)> = HashSet::new();

	for (line_idx, line) in content.split('\n').enumerate() {
		let line_number = line_idx + 1;

		for record in records {
			let env_record = match record {
				DetectorRecord::Env(r) => r,
				DetectorRecord::Fs(_) => continue,
			};

			let regex = &env_record.compiled_regex;
			for captures in regex.captures_iter(line) {
				let emissions = emit_env_from_match(
					env_record,
					captures,
					line,
					line_number,
					file_path,
					registry,
				);
				for e in emissions {
					let key = (file_path.to_string(), e.var_name.clone());
					if seen.contains(&key) {
						continue;
					}
					seen.insert(key);
					results.push(e);
				}
				if env_record.single_match_per_line {
					break;
				}
			}
		}
	}

	results
}

/// Walk fs-family detectors for one source file.
pub fn walk_fs_detectors(
	graph: &LoadedDetectorGraph,
	registry: &HookRegistry,
	language: DetectorLanguage,
	content: &str,
	file_path: &str,
) -> Vec<DetectedFsMutation> {
	let records = match graph
		.by_family_language
		.get(&(DetectorFamily::Fs, language))
	{
		Some(rs) => rs,
		None => return Vec::new(),
	};

	let mut results: Vec<DetectedFsMutation> = Vec::new();

	for (line_idx, line) in content.split('\n').enumerate() {
		let line_number = line_idx + 1;

		// Per-line, per-group seen-position sets. Records with no
		// positionDedupGroup do NOT participate in dedup. Records
		// sharing the same non-null group name share a single
		// per-line seen-position set.
		let mut seen_by_group: HashMap<&str, HashSet<usize>> = HashMap::new();

		for record in records {
			let fs_record = match record {
				DetectorRecord::Fs(r) => r,
				DetectorRecord::Env(_) => continue,
			};

			let regex = &fs_record.compiled_regex;
			let group: Option<&str> = fs_record.position_dedup_group.as_deref();

			for captures in regex.captures_iter(line) {
				let start = captures.get(0).map(|m| m.start()).unwrap_or(0);

				if let Some(g) = group {
					let set = seen_by_group.entry(g).or_default();
					if set.contains(&start) {
						continue;
					}
					set.insert(start);
				}

				let emissions = emit_fs_from_match(
					fs_record,
					captures,
					line,
					line_number,
					file_path,
					registry,
				);
				for e in emissions {
					results.push(e);
				}
				if fs_record.single_match_per_line {
					break;
				}
			}
		}
	}

	results
}

// ── Per-match emission (env) ──────────────────────────────────────

fn emit_env_from_match(
	record: &EnvDetectorRecord,
	captures: Captures<'_>,
	line: &str,
	line_number: usize,
	file_path: &str,
	registry: &HookRegistry,
) -> Vec<DetectedEnvDependency> {
	// Walker default: varName from regex capture group 1.
	let walker_var_name = captures.get(1).map(|m| m.as_str().to_string());

	if record.hook.is_none() {
		// Hookless path: walker default fills var_name, static fields
		// fill the rest. The loader has already verified that
		// access_kind and access_pattern are statically declared for
		// hookless records (must-be-supplied check).
		let var_name = match walker_var_name {
			Some(n) => n,
			None => return Vec::new(),
		};
		// Reject names that don't match the upper-snake convention.
		// Preserves the legacy detectors' implicit filtering via
		// `[A-Z_][A-Z0-9_]*` regex.
		if !is_likely_var_name(&var_name) {
			return Vec::new();
		}
		return vec![DetectedEnvDependency {
			var_name,
			access_kind: record.access_kind.expect(
				"walker: hookless env record missing access_kind — loader should have rejected",
			),
			access_pattern: record.access_pattern.expect(
				"walker: hookless env record missing access_pattern — loader should have rejected",
			),
			file_path: file_path.to_string(),
			line_number,
			default_value: None,
			confidence: record.confidence,
		}];
	}

	// Hooked path: dispatch to the registered hook.
	let hook_name = record.hook.as_deref().expect("hook is Some");
	let entry = registry.env.get(hook_name).unwrap_or_else(|| {
		panic!(
			"walker: env hook {:?} not registered (record {}). Loader should have rejected this configuration.",
			hook_name, record.pattern_id
		)
	});

	let ctx = EnvHookContext {
		captures,
		line,
		line_number,
		file_path,
		base_record: record,
	};
	let emissions = match (entry.func)(ctx) {
		EnvHookResult::Skip => return Vec::new(),
		EnvHookResult::Records(rs) => rs,
	};

	let mut out: Vec<DetectedEnvDependency> = Vec::with_capacity(emissions.len());
	for e in emissions {
		// Merge: hook fields (if set) override walker defaults /
		// static fields. The merge operators mirror the TS walker:
		// `??` semantics for most fields, `Emitted` tri-state for
		// default_value.
		let var_name = match e.var_name.or_else(|| walker_var_name.clone()) {
			Some(n) => n,
			None => continue,
		};
		let access_kind = match e.access_kind.or(record.access_kind) {
			Some(k) => k,
			None => panic!(
				"walker: env record {} via hook {} produced an emission with missing accessKind. Loader should have rejected this configuration.",
				record.pattern_id, hook_name
			),
		};
		let access_pattern = match e.access_pattern.or(record.access_pattern) {
			Some(p) => p,
			None => panic!(
				"walker: env record {} via hook {} produced an emission with missing accessPattern. Loader should have rejected this configuration.",
				record.pattern_id, hook_name
			),
		};
		// default_value tri-state resolution:
		//   - Unset      → None (walker fallback for env is null)
		//   - Value(None) → None (explicit null)
		//   - Value(Some(x)) → Some(x)
		let default_value = match e.default_value {
			Emitted::Unset => None,
			Emitted::Value(v) => v,
		};
		let confidence = e.confidence.unwrap_or(record.confidence);
		out.push(DetectedEnvDependency {
			var_name,
			access_kind,
			access_pattern,
			file_path: file_path.to_string(),
			line_number,
			default_value,
			confidence,
		});
	}
	out
}

// ── Per-match emission (fs) ──────────────────────────────────────

fn emit_fs_from_match(
	record: &FsDetectorRecord,
	captures: Captures<'_>,
	line: &str,
	line_number: usize,
	file_path: &str,
	registry: &HookRegistry,
) -> Vec<DetectedFsMutation> {
	// Walker defaults: target_path from regex group 1, destination
	// from group 2 if two_ended.
	let walker_target_path = captures.get(1).map(|m| m.as_str().to_string());
	let walker_destination_path = if record.two_ended {
		captures.get(2).map(|m| m.as_str().to_string())
	} else {
		None
	};

	if record.hook.is_none() {
		// Hookless path: walker defaults fill paths; static base
		// fields fill kind and pattern. The loader has verified
		// base_kind and base_pattern are present.
		let target_path = walker_target_path;
		let dynamic_path = target_path.is_none();
		return vec![DetectedFsMutation {
			file_path: file_path.to_string(),
			line_number,
			mutation_kind: record.base_kind.expect(
				"walker: hookless fs record missing base_kind — loader should have rejected",
			),
			mutation_pattern: record.base_pattern.expect(
				"walker: hookless fs record missing base_pattern — loader should have rejected",
			),
			target_path,
			destination_path: walker_destination_path,
			dynamic_path,
			confidence: record.confidence,
		}];
	}

	// Hooked path: dispatch to the registered hook.
	let hook_name = record.hook.as_deref().expect("hook is Some");
	let entry = registry.fs.get(hook_name).unwrap_or_else(|| {
		panic!(
			"walker: fs hook {:?} not registered (record {}). Loader should have rejected this configuration.",
			hook_name, record.pattern_id
		)
	});

	let ctx = FsHookContext {
		captures,
		line,
		line_number,
		file_path,
		base_record: record,
	};
	let emissions = match (entry.func)(ctx) {
		FsHookResult::Skip => return Vec::new(),
		FsHookResult::Records(rs) => rs,
	};

	let mut out: Vec<DetectedFsMutation> = Vec::with_capacity(emissions.len());
	for e in emissions {
		// target_path: Emitted tri-state merge.
		let target_path = match e.target_path {
			Emitted::Unset => walker_target_path.clone(),
			Emitted::Value(v) => v,
		};
		// destination_path: Emitted tri-state merge.
		let destination_path = match e.destination_path {
			Emitted::Unset => walker_destination_path.clone(),
			Emitted::Value(v) => v,
		};
		// dynamic_path: Emitted with inner bool. Unset → derive
		// from merged target_path. Value(b) → use b.
		//
		// IMPORTANT: the derivation uses the MERGED target_path,
		// not the walker default. This matches the TS walker's line
		// order: `const dynamicPath = e.dynamicPath !== undefined ? e.dynamicPath : targetPath === null`.
		let dynamic_path = match e.dynamic_path {
			Emitted::Unset => target_path.is_none(),
			Emitted::Value(b) => b,
		};
		// mutation_kind: `??` semantics — None falls back to static.
		let mutation_kind = match e.mutation_kind.or(record.base_kind) {
			Some(k) => k,
			None => panic!(
				"walker: fs record {} via hook {} produced an emission with missing mutationKind. Loader should have rejected this configuration.",
				record.pattern_id, hook_name
			),
		};
		let mutation_pattern = match e.mutation_pattern.or(record.base_pattern) {
			Some(p) => p,
			None => panic!(
				"walker: fs record {} via hook {} produced an emission with missing mutationPattern. Loader should have rejected this configuration.",
				record.pattern_id, hook_name
			),
		};
		let confidence = e.confidence.unwrap_or(record.confidence);
		out.push(DetectedFsMutation {
			file_path: file_path.to_string(),
			line_number,
			mutation_kind,
			mutation_pattern,
			target_path,
			destination_path,
			dynamic_path,
			confidence,
		});
	}
	out
}

// ── Helpers ───────────────────────────────────────────────────────

/// Match the upper-snake env var name convention used by the
/// legacy detectors. Names that don't match are filtered out at
/// emission time, preserving the implicit filtering done by the
/// original regexes which all use `[A-Z_][A-Z0-9_]*`.
///
/// The walker applies this only to walker-default var_name
/// extraction (regex group 1). Hook-supplied var_names pass through
/// unchanged because hooks may have their own filtering rules
/// (e.g., `expand_js_env_destructure`).
fn is_likely_var_name(name: &str) -> bool {
	let mut chars = name.chars();
	let Some(first) = chars.next() else {
		return false;
	};
	if !(first.is_ascii_uppercase() || first == '_') {
		return false;
	}
	for c in chars {
		if !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
			return false;
		}
	}
	true
}
