//! Detector substrate type definitions.
//!
//! This module is the Rust mirror of `src/core/seams/detectors/types.ts`
//! and the public detector output shapes from
//! `src/core/seams/env-dependency.ts` and `src/core/seams/fs-mutation.ts`.
//!
//! Pure types only. No behavior. No I/O. No regex compilation. No
//! smart constructors. The loader (R1-C) consumes the raw record
//! types and produces the runtime record types via explicit
//! validation. The walker (R1-F) consumes the runtime types and
//! the hook registry to produce detected fact outputs.
//!
//! Parity contract:
//!
//!   - Output fact structs (`DetectedEnvDependency`,
//!     `DetectedFsMutation`) are serialized via serde with
//!     `rename_all = "camelCase"`. Field order in the struct
//!     declaration matches the TS interface order exactly. JSON
//!     output is byte-identical to the TS serialization.
//!
//!   - `Option<T>` serializes as `null` when `None`, matching the
//!     TS nullable convention. Fields that the locked expected.json
//!     shape requires to be always-present (e.g. `destinationPath`
//!     on fs facts) are typed as `Option<T>` not `#[serde(skip)]`.
//!
//!   - String enum variants are serialized via `rename_all`
//!     attributes that match the TS string literal values exactly.
//!     `DetectorLanguage::Ts` ↔ `"ts"`, `MutationKind::WriteFile`
//!     ↔ `"write_file"`, etc.

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

// ── Tri-state emission wrapper ─────────────────────────────────────

/// Distinguishes "the hook did not supply this field" from "the
/// hook explicitly supplied a value (which may be null)".
///
/// Walker merge semantics for hook emissions match the TypeScript
/// walker exactly. The TS walker uses two different operators:
///
///   - `??` (nullish coalescing): collapses both `null` and
///     `undefined` into "fall back". Rust uses plain `Option<T>`
///     for these fields — `None` covers both states because they
///     produce the same merged output.
///
///   - `!== undefined`: distinguishes `null` (explicit override
///     to null) from `undefined` (use walker/static fallback).
///     Rust uses `Emitted<T>` for these fields — three states:
///     `Unset`, `Value(None)`, `Value(Some(x))`.
///
/// `Emitted<T>` is walker-internal. It is NOT serialized to the
/// parity boundary; the walker merges it into the final
/// `DetectedEnvDependency` / `DetectedFsMutation` shape, after
/// which all fields are concrete (Option<T> or T) and the
/// tri-state distinction is gone.
///
/// Default is `Unset` — hooks construct emissions by setting only
/// the fields they want to override.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Emitted<T> {
	#[default]
	Unset,
	Value(T),
}

impl<T> Emitted<T> {
	/// Resolve this emission against a fallback closure. If the
	/// emission is `Unset`, the fallback is invoked. Otherwise the
	/// inner value is returned.
	///
	/// Used by the walker merge path:
	///
	/// ```ignore
	/// let target_path = emission.target_path.or_else_with(|| walker_target_path);
	/// ```
	pub fn or_else_with<F: FnOnce() -> T>(self, f: F) -> T {
		match self {
			Emitted::Unset => f(),
			Emitted::Value(v) => v,
		}
	}

	/// True iff the hook supplied a value for this field.
	pub fn is_set(&self) -> bool {
		matches!(self, Emitted::Value(_))
	}
}

// ── Family / language enums ────────────────────────────────────────

/// Source language a detector record applies to.
///
/// Mirrors `DetectorLanguage` in `types.ts`. The legacy file
/// extension dispatch maps multiple JS-family extensions
/// (`.ts`, `.tsx`, `.js`, `.jsx`) all to the `Ts` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectorLanguage {
	Ts,
	Py,
	Rs,
	Java,
	C,
}

/// Detector family — what kind of seam this detector reports.
///
/// Mirrors `DetectorFamily` in `types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectorFamily {
	Env,
	Fs,
}

// ── Env access enums ───────────────────────────────────────────────

/// How an env var is accessed at the call site.
///
/// Mirrors `EnvAccessKind` in `env-dependency.ts`. Used as both a
/// static field on detector records and as part of the detected
/// output fact serialization shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvAccessKind {
	Required,
	Optional,
	Unknown,
}

/// Which language pattern matched an env access.
///
/// Mirrors `EnvAccessPattern` in `env-dependency.ts`. The string
/// literal values are stable identifiers used both inside the
/// detector graph and in serialized fact output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvAccessPattern {
	ProcessEnvDot,
	ProcessEnvBracket,
	ProcessEnvDestructure,
	OsEnviron,
	OsGetenv,
	StdEnvVar,
	SystemGetenv,
	CGetenv,
	DotenvReference,
}

// ── Fs mutation enums ──────────────────────────────────────────────

/// Coarse fs mutation taxonomy.
///
/// Mirrors `MutationKind` in `fs-mutation.ts`. Stable string forms
/// like `write_file`, `delete_path`, `create_dir`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
	WriteFile,
	AppendFile,
	DeletePath,
	CreateDir,
	RenamePath,
	CopyPath,
	ChmodPath,
	CreateTemp,
}

/// Per-language fs mutation pattern identifier.
///
/// Mirrors `MutationPattern` in `fs-mutation.ts`. Stable string
/// forms with language prefixes like `fs_write_file`, `py_open_write`,
/// `rust_fs_rename`, `java_files_write`, `c_fopen_write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationPattern {
	// JS/TS (node:fs)
	FsWriteFile,
	FsAppendFile,
	FsUnlink,
	FsRm,
	FsMkdir,
	FsCreateWriteStream,
	FsRename,
	FsCopyFile,
	FsChmod,
	// Python
	PyOpenWrite,
	PyOpenAppend,
	PyOsRemove,
	PyOsUnlink,
	PyShutilRmtree,
	PyOsMkdir,
	PyOsMakedirs,
	PyPathlibWrite,
	PyPathlibMkdir,
	PyTempfile,
	// Rust
	RustFsWrite,
	RustFsRemoveFile,
	RustFsRemoveDirAll,
	RustFsCreateDir,
	RustFsRename,
	RustFsCopy,
	// Java
	JavaFilesWrite,
	JavaFilesDelete,
	JavaFilesCreateDirectory,
	JavaFileOutputStream,
	// C/C++
	CFopenWrite,
	CFopenAppend,
	CUnlink,
	CRemove,
	CRmdir,
	CMkdir,
}

// ── Public output facts (parity boundary) ──────────────────────────
//
// These two structs are the parity boundary. Their JSON
// serialization MUST be byte-identical to the TS
// `DetectedEnvDependency` and `DetectedFsMutation` shapes when
// produced from the same input. Field order, field names (after
// camelCase rename), and null handling all participate in the
// contract.

/// Detected env var access. Mirrors `DetectedEnvDependency` in
/// `env-dependency.ts`.
///
/// Field order matches the TS interface declaration order exactly.
/// Serde derives serialization in struct field order; do not
/// reorder these fields without coordinating with the TS side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedEnvDependency {
	pub var_name: String,
	pub access_kind: EnvAccessKind,
	pub access_pattern: EnvAccessPattern,
	pub file_path: String,
	pub line_number: usize,
	pub default_value: Option<String>,
	pub confidence: f64,
}

/// Detected fs mutation. Mirrors `DetectedFsMutation` in
/// `fs-mutation.ts`.
///
/// Field order matches the TS interface declaration order exactly.
///
/// Note on `destination_path`: in the TS interface this field is
/// declared as optional (`destinationPath?: string | null`), so it
/// may be absent from the TS JSON when the detector did not set
/// it. The locked parity expected.json shape requires the field to
/// be ALWAYS PRESENT (with value `null` when absent). The Rust
/// struct uses `Option<String>` which serde serializes as `null`
/// when `None` — always present. The TS parity harness must
/// normalize the TS output to match this convention; that
/// normalization is a parity-harness concern (R1-I), not a Rust
/// type concern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedFsMutation {
	pub file_path: String,
	pub line_number: usize,
	pub mutation_kind: MutationKind,
	pub mutation_pattern: MutationPattern,
	pub target_path: Option<String>,
	pub destination_path: Option<String>,
	pub dynamic_path: bool,
	pub confidence: f64,
}

// ── Detector record runtime types ──────────────────────────────────
//
// These are the post-load record shapes produced by the loader and
// consumed by the walker. The loader takes raw TOML records (defined
// below) and produces these by validating, compiling regex, and
// resolving hook references against the registry.

/// Env-family detector record after loading.
///
/// Mirrors `EnvDetectorRecord` in `types.ts`. The `compiled_regex`
/// field is set by the loader and used by the walker hot path.
/// `access_kind` and `access_pattern` are nullable because they are
/// optional when a hook is declared and supplies them via the
/// hook's supplies set.
#[derive(Debug, Clone)]
pub struct EnvDetectorRecord {
	pub language: DetectorLanguage,
	pub family: DetectorFamily,
	pub pattern_id: String,
	pub regex: String,
	pub flags: String,
	pub compiled_regex: Regex,
	pub confidence: f64,
	pub hook: Option<String>,
	pub single_match_per_line: bool,
	pub access_kind: Option<EnvAccessKind>,
	pub access_pattern: Option<EnvAccessPattern>,
}

/// Fs-family detector record after loading.
///
/// Mirrors `FsDetectorRecord` in `types.ts`. `base_kind` and
/// `base_pattern` are nullable for the same hook-supplies reason as
/// the env equivalents. `position_dedup_group` declares this record
/// as a participant in a named overlap group; records sharing a
/// non-null group name dedupe by `(line, match.start())` within a
/// single walker call.
#[derive(Debug, Clone)]
pub struct FsDetectorRecord {
	pub language: DetectorLanguage,
	pub family: DetectorFamily,
	pub pattern_id: String,
	pub regex: String,
	pub flags: String,
	pub compiled_regex: Regex,
	pub confidence: f64,
	pub hook: Option<String>,
	pub single_match_per_line: bool,
	pub base_kind: Option<MutationKind>,
	pub base_pattern: Option<MutationPattern>,
	pub two_ended: bool,
	pub position_dedup_group: Option<String>,
}

/// Discriminated union of env and fs detector records.
///
/// Mirrors the TS `DetectorRecord` union type. The walker matches
/// on the variant to dispatch to the appropriate emission path.
#[derive(Debug, Clone)]
pub enum DetectorRecord {
	Env(EnvDetectorRecord),
	Fs(FsDetectorRecord),
}

impl DetectorRecord {
	/// The (family, language) bucket key for the loaded graph index.
	pub fn family_language(&self) -> (DetectorFamily, DetectorLanguage) {
		match self {
			DetectorRecord::Env(r) => (DetectorFamily::Env, r.language),
			DetectorRecord::Fs(r) => (DetectorFamily::Fs, r.language),
		}
	}
}

/// The full loaded detector graph.
///
/// Mirrors `LoadedDetectorGraph` in `types.ts`. Records are stored
/// once in `all_records` and indexed by `(family, language)` in
/// `by_family_language` for O(1) walker dispatch. Both views
/// preserve declaration order from the TOML source — required for
/// deterministic walker output (per slice constraint: walker must
/// preserve table order).
#[derive(Debug, Clone)]
pub struct LoadedDetectorGraph {
	pub all_records: Vec<DetectorRecord>,
	pub by_family_language:
		HashMap<(DetectorFamily, DetectorLanguage), Vec<DetectorRecord>>,
}

// ── Raw TOML deserialization shapes ────────────────────────────────
//
// These structs are the lowest-level shape that mirrors the TOML
// file's `[[detector]]` array layout. The loader (R1-C) consumes a
// `Vec<RawDetectorRecord>` from `toml::from_str` and validates each
// raw record into an `EnvDetectorRecord` or `FsDetectorRecord`. The
// raw structs use `Option<T>` for fields that are optional in the
// TOML schema; the loader's validation logic enforces the
// must-be-supplied rules.
//
// `serde(deny_unknown_fields)` is the load-time defense against
// typos in the TOML file. Any unknown field fails parse with a
// precise error pointing at the offending key.

/// Top-level TOML document shape.
///
/// `dead_code` is allowed at the type level for the entire raw
/// deserialization layer because R1-B introduces these types but
/// R1-C (the loader) is the first consumer. Once R1-C lands, every
/// field below is read at least once and the allow becomes a no-op.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawDetectorDocument {
	#[serde(default)]
	pub detector: Vec<RawDetectorRecord>,
}

/// One raw `[[detector]]` table from the TOML source.
///
/// All fields are optional at the deserialization layer because the
/// schema's required-vs-optional split depends on the family and on
/// hook references — both of which the loader validates after
/// parsing. The loader is the single point of truth for "is this
/// declaration valid".
///
/// `deny_unknown_fields` defends against typos.
///
/// `dead_code` is allowed for the same reason as
/// `RawDetectorDocument` above — R1-B introduces these fields but
/// R1-C consumes them.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawDetectorRecord {
	pub language: Option<String>,
	pub family: Option<String>,
	pub pattern_id: Option<String>,
	pub regex: Option<String>,
	pub flags: Option<String>,
	pub confidence: Option<f64>,
	pub hook: Option<String>,
	pub single_match_per_line: Option<bool>,
	// Env-only fields
	pub access_kind: Option<String>,
	pub access_pattern: Option<String>,
	// Fs-only fields
	pub base_kind: Option<String>,
	pub base_pattern: Option<String>,
	pub two_ended: Option<bool>,
	pub position_dedup_group: Option<String>,
}

// ── Hook context, result, emission ────────────────────────────────

/// Context passed to an env hook on each regex match.
///
/// DTO-shaped per the slice's Clean Architecture constraint: no
/// storage, no fs, no side-effect surfaces. The hook receives raw
/// match context plus the base record from the loaded graph and
/// returns a hook result.
///
/// Lifetimes: `captures` borrows from the line text, `line` and
/// `file_path` borrow from caller-owned strings. The walker holds
/// the input text alive for the duration of the hook call.
#[derive(Debug)]
pub struct EnvHookContext<'a> {
	pub captures: regex::Captures<'a>,
	pub line: &'a str,
	pub line_number: usize,
	pub file_path: &'a str,
	pub base_record: &'a EnvDetectorRecord,
}

/// One env record an env hook wants to emit before walker merge.
///
/// All fields are optional. The walker fills omitted fields from
/// either walker defaults (varName from regex group 1) or static
/// record fields (accessKind / accessPattern). This matches the TS
/// `EnvHookEmission` shape exactly.
///
/// Field merge semantics (must match the TS walker):
///
///   - `var_name`: `Option<String>`. `??` semantics — `None` falls
///     back to walker default (regex capture group 1).
///   - `access_kind`: `Option<EnvAccessKind>`. `??` semantics —
///     `None` falls back to record's static access_kind.
///   - `access_pattern`: `Option<EnvAccessPattern>`. `??` semantics.
///   - `default_value`: `Emitted<Option<String>>`. **Tri-state** —
///     TS uses `e.defaultValue !== undefined` so the walker
///     distinguishes "hook didn't say" (Unset → fallback null) from
///     "hook explicitly said null" (Value(None) → null) from
///     "hook explicitly said string" (Value(Some(x)) → x).
///     Functionally the Unset and Value(None) cases produce the
///     same output today, but the contract is preserved exactly so
///     a future hook that depends on the distinction will not break.
///   - `confidence`: `Option<f64>`. `??` semantics.
#[derive(Debug, Clone, Default)]
pub struct EnvHookEmission {
	pub var_name: Option<String>,
	pub access_kind: Option<EnvAccessKind>,
	pub access_pattern: Option<EnvAccessPattern>,
	pub default_value: Emitted<Option<String>>,
	pub confidence: Option<f64>,
}

/// Result returned by an env hook.
///
/// `Skip` suppresses the match entirely. `Records(...)` emits zero
/// or more partials that the walker merges with defaults.
#[derive(Debug, Clone)]
pub enum EnvHookResult {
	Skip,
	Records(Vec<EnvHookEmission>),
}

/// Context passed to an fs hook on each regex match.
#[derive(Debug)]
pub struct FsHookContext<'a> {
	pub captures: regex::Captures<'a>,
	pub line: &'a str,
	pub line_number: usize,
	pub file_path: &'a str,
	pub base_record: &'a FsDetectorRecord,
}

/// One fs record an fs hook wants to emit before walker merge.
///
/// Field merge semantics (must match the TS walker):
///
///   - `target_path`: `Emitted<Option<String>>`. **Tri-state** — TS
///     uses `e.targetPath !== undefined`. The walker distinguishes
///     "hook didn't say" (Unset → walker default = regex group 1)
///     from "hook explicitly said null" (Value(None) → null) from
///     "hook explicitly said string" (Value(Some(x)) → x). The
///     dynamic-path hooks rely on the explicit-null override for
///     correctness.
///
///   - `destination_path`: `Emitted<Option<String>>`. **Tri-state** —
///     same as target_path. TS uses `e.destinationPath !== undefined`.
///     The walker default for destinationPath is regex group 2 when
///     `two_ended` is set, otherwise null.
///
///   - `dynamic_path`: `Emitted<bool>`. **Tri-state in semantics, but
///     the inner type is `bool` (no nested Option)** because
///     dynamicPath cannot be null. TS uses
///     `e.dynamicPath !== undefined ? e.dynamicPath : (targetPath === null)`.
///     Walker default is derived from the merged targetPath. The
///     three states are Unset (derive), Value(true), Value(false).
///
///   - `mutation_kind`: `Option<MutationKind>`. `??` semantics —
///     `None` falls back to record's static base_kind.
///   - `mutation_pattern`: `Option<MutationPattern>`. `??` semantics.
///   - `confidence`: `Option<f64>`. `??` semantics.
///
/// Default is all-Unset / all-None — hooks construct emissions by
/// setting only the fields they want to override.
#[derive(Debug, Clone, Default)]
pub struct FsHookEmission {
	pub target_path: Emitted<Option<String>>,
	pub destination_path: Emitted<Option<String>>,
	pub mutation_kind: Option<MutationKind>,
	pub mutation_pattern: Option<MutationPattern>,
	pub dynamic_path: Emitted<bool>,
	pub confidence: Option<f64>,
}

/// Result returned by an fs hook.
#[derive(Debug, Clone)]
pub enum FsHookResult {
	Skip,
	Records(Vec<FsHookEmission>),
}

// ── Hook function pointer types ────────────────────────────────────
//
// Hooks are stateless pure functions. They are registered as
// function pointers in a HashMap keyed by name. Function pointers
// are zero-overhead at the call site (no virtual dispatch, no
// boxing) and match the H-C lock from R1 architecture decisions.
//
// The `for<'a>` higher-rank trait bound is required because the
// hook signature is generic over the lifetime of the input text.

/// Function pointer type for env hooks.
pub type EnvHook = for<'a> fn(ctx: EnvHookContext<'a>) -> EnvHookResult;

/// Function pointer type for fs hooks.
pub type FsHook = for<'a> fn(ctx: FsHookContext<'a>) -> FsHookResult;

// ── Supplied-field enums ───────────────────────────────────────────
//
// Each hook entry declares which output fields it is responsible
// for supplying. The loader validates that every required output
// field is reachable from either a static schema field or the
// hook's supplies set, per the Y2 must-be-supplied semantics
// established in the TS detector slice.

/// Env output fields a hook may supply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnvSuppliedField {
	VarName,
	AccessKind,
	AccessPattern,
	DefaultValue,
}

/// Fs output fields a hook may supply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FsSuppliedField {
	TargetPath,
	DestinationPath,
	MutationKind,
	MutationPattern,
	DynamicPath,
}

/// Required env output fields the loader must verify are reachable
/// for every record. A field is "reachable" if either the record
/// declares a static value for it OR the hook (if any) lists it in
/// `supplies`.
///
/// Walker default behavior fills `var_name` from regex capture
/// group 1 when the hook does not supply it; that field is NOT in
/// the must-be-supplied set because the walker default makes it
/// reachable for any record with at least one capture group.
pub fn env_must_be_supplied() -> HashSet<EnvSuppliedField> {
	let mut s = HashSet::new();
	s.insert(EnvSuppliedField::AccessKind);
	s.insert(EnvSuppliedField::AccessPattern);
	s
}

/// Required fs output fields the loader must verify are reachable.
///
/// Walker default behavior fills `target_path` from group 1,
/// `destination_path` from group 2 (when `two_ended`), and derives
/// `dynamic_path` from `target_path == None`. None of those are in
/// the must-be-supplied set.
pub fn fs_must_be_supplied() -> HashSet<FsSuppliedField> {
	let mut s = HashSet::new();
	s.insert(FsSuppliedField::MutationKind);
	s.insert(FsSuppliedField::MutationPattern);
	s
}

// ── Hook registry ──────────────────────────────────────────────────

/// One env hook entry: function pointer plus declared supplies set.
///
/// The supplies set is the contract surface between the detector
/// graph and the procedural hook layer. The loader checks every
/// record's must-be-supplied requirements against this set when the
/// record references the hook.
#[derive(Debug, Clone)]
pub struct EnvHookEntry {
	pub func: EnvHook,
	pub supplies: HashSet<EnvSuppliedField>,
}

/// One fs hook entry: function pointer plus declared supplies set.
#[derive(Debug, Clone)]
pub struct FsHookEntry {
	pub func: FsHook,
	pub supplies: HashSet<FsSuppliedField>,
}

/// Named registry of env and fs hook entries.
///
/// Mirrors `HookRegistry` in `types.ts`. The loader validates that
/// any `hook = "..."` reference in the TOML resolves to a
/// registered entry for the matching family.
///
/// Hook name keys are `&'static str` because they are compile-time
/// literal hook names registered at module-load time. The Rust
/// type system enforces that the keys come from the literal set;
/// detector authors register a new hook by adding a string literal
/// + a function pointer + a supplies set to the registry builder.
#[derive(Debug, Clone, Default)]
pub struct HookRegistry {
	pub env: HashMap<&'static str, EnvHookEntry>,
	pub fs: HashMap<&'static str, FsHookEntry>,
}
