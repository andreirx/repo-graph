//! Classification substrate type definitions.
//!
//! This module is the Rust mirror of the TypeScript type
//! vocabulary consumed and produced by the classification
//! substrate at `src/core/classification/`, plus the transitive
//! type deps from `src/core/diagnostics/`, the narrow
//! `ImportBinding` mirror from `src/core/ports/extractor.ts`,
//! and the classification-local `ClassifierEdgeInput` (a 2-field
//! input type replacing the full extractor-shaped `UnresolvedEdge`
//! per the locked R3 boundary — see the `ClassifierEdgeInput`
//! docstring for the rationale).
//!
//! Pure types only. No behavior, no I/O, no regex compilation.
//! The signals predicates (R3-C), blast-radius function (R3-C),
//! classifier (R3-D), boundary matcher (R3-E), and framework
//! detection (R3-F) all consume these types as inputs and
//! produce subsets of them as outputs.
//!
//! ── Locked design (R3 lock + R3-B narrow override) ────────────
//!
//! - **D-A2: typed Rust enums with serde rename.** Every string-
//!   literal union in TS becomes a typed Rust enum with
//!   `#[serde(rename_all = "...")]` matching the exact TS string
//!   values. Downstream Rust consumers get compile-time
//!   exhaustiveness checking; parity-boundary JSON serialization
//!   emits the same strings TS does.
//!
//! - **D-A2 additional DTO collection rule: no HashSet/HashMap in
//!   public DTOs.** TS uses `ReadonlySet<string>` on
//!   `FileSignals.sameFileValueSymbols` etc. for convenience. Rust
//!   uses `Vec<String>` instead. The classification functions
//!   build local `HashSet<&str>` for O(1) lookup and discard them
//!   before returning; the public DTO boundary stays
//!   deterministic-order.
//!
//! - **R3-B narrow D-A1 timing correction:** `serde_json::Map` is
//!   used for the two `metadata: Record<string, unknown>` fields
//!   on `BoundaryProviderFact` and `BoundaryConsumerFact`. Scope
//!   is limited to those two fields; no other field uses
//!   `serde_json::Value` or `Map`. The `preserve_order` feature
//!   on serde_json is required for byte-equal parity output
//!   (mirrors R2-E's rationale for the same feature).
//!
//! - **Serde field-name convention:** all DTO struct fields are
//!   Rust `snake_case`; struct-level
//!   `#[serde(rename_all = "camelCase")]` converts to the TS
//!   camelCase wire format for parity JSON. Enum variants are
//!   Rust PascalCase; enum-level `#[serde(rename_all = "...")]`
//!   matches the TS string-literal values. Per-enum rename policy
//!   varies (snake_case, lowercase, SCREAMING_SNAKE_CASE) because
//!   the TS string values are not uniform.
//!
//! ── What is NOT in this module ────────────────────────────────
//!
//! - `classifyUnresolvedEdge` function (R3-D)
//! - `deriveBlastRadius` function (R3-C)
//! - `computeMatcherKey` / `matchBoundaryFacts` (R3-E)
//! - `detectFrameworkBoundary` / `detectLambdaEntrypoints` (R3-F)
//! - `humanLabelForCategory` — display-only helper from
//!   `unresolved-edge-categories.ts`, not part of the machine
//!   vocabulary this crate exposes. Rendering is a CLI/display
//!   concern and stays in the TS adapter layer.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ── Source location + extractor-port mirror types ─────────────────
//
// SourceLocation is mirrored from `src/core/model/types.ts`.
// ImportBinding is mirrored from `src/core/ports/extractor.ts`.
// Only these two types are mirrored — the full extractor port
// (including `UnresolvedEdge`, `EdgeType`, `Resolution`, and the
// `ExtractorPort` interface itself) is NOT ported per the R3 lock.
//
// The classifier's input is a classification-local
// `ClassifierEdgeInput` (defined below), NOT the full
// `UnresolvedEdge` struct. See its docstring for the rationale.

/// Position bundle for a source-code span. Mirror of
/// `SourceLocation` from `src/core/model/types.ts`.
///
/// All four fields are non-nullable at the type level; nullability
/// lives on the containing field (e.g., `ImportBinding.location:
/// Option<SourceLocation>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
	pub line_start: i64,
	pub col_start: i64,
	pub line_end: i64,
	pub col_end: i64,
}

/// One import-statement binding observed in a source file.
///
/// Mirror of `ImportBinding` from
/// `src/core/ports/extractor.ts:72`.
///
/// ImportBinding is a SIDE-CHANNEL extractor fact: it records what
/// identifiers were introduced by import statements, for use by
/// the classifier. It is NOT an edge and does NOT affect the
/// graph's trust posture. The classification substrate treats
/// these as read-only input signals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBinding {
	/// Local identifier introduced by this import.
	pub identifier: String,
	/// Module specifier as written in source (quotes stripped).
	pub specifier: String,
	/// True iff the specifier starts with "." (path-relative
	/// import) or, for Rust, uses crate::/super::/self:: prefixes.
	pub is_relative: bool,
	/// Source location of the import statement. `None` if the
	/// extractor did not record it.
	pub location: Option<SourceLocation>,
	/// True iff the enclosing statement is `import type ...`.
	/// First-slice scope: statement-level only. Specifier-level
	/// `{ type X }` is NOT detected and yields `is_type_only:
	/// false`.
	pub is_type_only: bool,
}

/// Classification-local input type for the unresolved-edge
/// classifier.
///
/// **This is NOT a full mirror of `UnresolvedEdge` from
/// `src/core/ports/extractor.ts`.** The locked R3 boundary
/// explicitly excluded mirroring the extractor port beyond
/// `ImportBinding`. The classifier reads only two fields from the
/// full unresolved edge (`targetKey` and `metadataJson`); the
/// other 9 fields (`edgeUid`, `snapshotUid`, `repoUid`,
/// `sourceNodeUid`, `type`, `resolution`, `extractor`,
/// `location`) are storage/indexer plumbing that the classifier
/// never touches.
///
/// By defining a narrow classification-local input type instead
/// of publishing the full extractor-shaped DTO, the classification
/// crate avoids coupling downstream consumers to extractor
/// vocabulary earlier than planned. The indexer (which does own
/// the full `UnresolvedEdge` type) can construct
/// `ClassifierEdgeInput` from its own edge DTO at the call site.
///
/// Field sources in the TS classifier
/// (`unresolved-classifier.ts:191-470`):
///   - `edge.targetKey` → `target_key`
///   - `edge.metadataJson` → `metadata_json` (parsed locally by
///     the classifier when it needs the import specifier or raw
///     callee name)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierEdgeInput {
	/// Symbolic target reference. NOT a node UID. Examples:
	/// - IMPORTS: `"repo:src/types:FILE"` (extensionless file
	///   stable key)
	/// - CALLS: `"this.repo.findById"` (callee expression)
	/// - INSTANTIATES: `"UserRepository"` (class name)
	/// - IMPLEMENTS: `"ILLMAdapter"` (interface name)
	pub target_key: String,
	/// Arbitrary extractor-attached metadata as a JSON string.
	/// The classifier parses this locally when needed (e.g., to
	/// extract the import specifier or raw callee name from the
	/// extractor's metadata). `None` when the extractor did not
	/// attach metadata to this edge.
	pub metadata_json: Option<String>,
}

// NOTE: `EdgeType` and `Resolution` are NOT ported here.
//
// The locked R3 boundary excluded mirroring the extractor port
// beyond `ImportBinding`. `EdgeType` (18 canonical edge types)
// and `Resolution` (static/dynamic/inferred) are extractor-port
// vocabulary that the classification functions never read. The
// `UnresolvedEdgeCategory` enum already captures the extraction
// failure mode dimension; the `EdgeType` dimension is orthogonal
// and lives in the extractor's domain. If a future Rust slice
// (indexer, trust) needs these types, they should be defined in
// a shared types crate or mirrored in the consuming crate — not
// published from the classification crate.

// ── UnresolvedEdgeCategory ────────────────────────────────────────

/// Machine-stable diagnostic category for an unresolved edge.
/// Mirror of `UnresolvedEdgeCategory` from
/// `src/core/diagnostics/unresolved-edge-categories.ts`.
///
/// Keys are versioned as part of the snapshot's
/// `extraction_diagnostics` payload. Adding a new key is non-
/// breaking. Removing or renaming is breaking and must bump
/// `diagnostics_version`. Human-readable labels are rendered from
/// these machine keys at display time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedEdgeCategory {
	ImportsFileNotFound,
	InstantiatesClassNotFound,
	ImplementsInterfaceNotFound,
	CallsThisWildcardMethodNeedsTypeInfo,
	CallsThisMethodNeedsClassContext,
	CallsObjMethodNeedsTypeInfo,
	CallsFunctionAmbiguousOrMissing,
	Other,
}

impl UnresolvedEdgeCategory {
	/// True iff this category is in the CALLS family.
	///
	/// Mirrors the TS `CALLS_CATEGORIES` constant
	/// (`unresolved-edge-categories.ts:61`). Used by reliability
	/// formulas computing the call-graph resolution rate and by
	/// the blast-radius function to scope itself to CALLS-family
	/// edges only.
	pub fn is_calls_category(self) -> bool {
		matches!(
			self,
			Self::CallsThisWildcardMethodNeedsTypeInfo
				| Self::CallsThisMethodNeedsClassContext
				| Self::CallsObjMethodNeedsTypeInfo
				| Self::CallsFunctionAmbiguousOrMissing
		)
	}

	/// True iff this category is in the IMPORTS family.
	///
	/// Mirrors the TS `IMPORTS_CATEGORIES` constant. Currently
	/// singleton; future import-family categories would be added
	/// here and in the TS source in sync.
	pub fn is_imports_category(self) -> bool {
		matches!(self, Self::ImportsFileNotFound)
	}
}

// ── UnresolvedEdgeClassification ──────────────────────────────────

/// Semantic classification bucket for an unresolved edge. Mirror
/// of `UnresolvedEdgeClassification` from
/// `src/core/diagnostics/unresolved-edge-classification.ts:48`.
///
/// Orthogonal axis from `UnresolvedEdgeCategory`: category is the
/// extraction failure mode, classification is the semantic
/// meaning of the gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedEdgeClassification {
	ExternalLibraryCandidate,
	InternalCandidate,
	/// The unresolved call matches a known framework runtime-
	/// wiring or registration pattern. Narrow contract: only for
	/// true runtime wiring / framework-defined execution
	/// surfaces, NOT for ordinary framework API usage.
	FrameworkBoundaryCandidate,
	Unknown,
}

// ── UnresolvedEdgeBasisCode ───────────────────────────────────────

/// The specific classifier rule that produced a classification.
/// Mirror of `UnresolvedEdgeBasisCode` from
/// `src/core/diagnostics/unresolved-edge-classification.ts:70`.
///
/// Every classifier verdict carries a basis code so the
/// classification is auditable without re-running the classifier.
/// Backfill tooling can detect and rewrite stale basis codes when
/// the rule set version bumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedEdgeBasisCode {
	/// Import specifier is a bare name matching a package.json
	/// dependency.
	SpecifierMatchesPackageDependency,
	/// Import binding specifier matches a project-level path
	/// alias (e.g., tsconfig `paths` entry).
	SpecifierMatchesProjectAlias,
	/// Import binding specifier matches a known runtime/stdlib
	/// module (e.g., `"path"`, `"fs"`, `"node:crypto"`).
	SpecifierMatchesRuntimeModule,
	/// `obj.method()` receiver came in via external import in
	/// this file.
	ReceiverMatchesExternalImport,
	/// `obj.method()` receiver came in via internal (relative or
	/// alias) import.
	ReceiverMatchesInternalImport,
	/// `obj.method()` receiver is a symbol declared in the same
	/// source file.
	ReceiverMatchesSameFileSymbol,
	/// `obj.method()` receiver is a known runtime global (Map,
	/// Date, etc.).
	ReceiverMatchesRuntimeGlobal,
	/// `fn()` callee is declared in the same source file.
	CalleeMatchesSameFileSymbol,
	/// `fn()` callee came in via external import in this file.
	CalleeMatchesExternalImport,
	/// `fn()` callee came in via internal import in this file.
	CalleeMatchesInternalImport,
	/// `fn()` / `new Foo()` callee is a known runtime global.
	CalleeMatchesRuntimeGlobal,
	/// `this.m()` or `this.x.m()` — receiver is on the current
	/// class.
	ThisReceiverImpliesInternal,
	/// An `imports_file_not_found` observation whose specifier is
	/// path-relative (starts with "." for TS, or crate::/super::
	/// for Rust). Definite internal import.
	RelativeImportTargetUnresolved,
	/// Rust crate-internal module import heuristic. Specifier is
	/// lowercase, not in Cargo deps, not in stdlib, and came from
	/// the Rust extractor.
	RustCrateInternalModuleHeuristic,
	/// Express route registration: `app.get()`, `app.post()`,
	/// `router.use()`, etc.
	ExpressRouteRegistration,
	/// Express middleware registration: `app.use()`, `router.use()`.
	ExpressMiddlewareRegistration,
	/// No classification signal matched.
	NoSupportingSignal,
}

/// Current classifier rule-set version. Mirror of
/// `CURRENT_CLASSIFIER_VERSION` from
/// `src/core/diagnostics/unresolved-edge-classification.ts:185`.
///
/// Bump when any classification value or basis code is renamed /
/// removed, or when a rule's semantic changes. Adding values does
/// NOT require a bump.
pub const CURRENT_CLASSIFIER_VERSION: u32 = 6;

// ── Boundary interaction model ────────────────────────────────────

/// The transport/mechanism through which a boundary interaction
/// occurs. Mirror of `BoundaryMechanism` from
/// `src/core/classification/api-boundary.ts:29`.
///
/// Extensible: new mechanisms are added as adapters are built.
/// All TS values are lowercase snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryMechanism {
	// Interaction boundaries (request/response, command/exit)
	Http,
	Grpc,
	Rpc,
	CliCommand,
	// State boundaries (read/write to persisted data)
	Filesystem,
	DatabaseSql,
	DatabaseNosql,
	Cache,
	ObjectStore,
	// System-level boundaries
	IpcSharedMemory,
	Ioctl,
	Queue,
	Event,
	Socket,
	DeviceProtocol,
	RegisterMap,
	Other,
}

/// Role of a boundary participant. Mirror of `BoundaryRole` from
/// `api-boundary.ts:54`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryRole {
	Provider,
	Consumer,
}

/// How a provider fact was determined. Mirror of the inline
/// `basis` union on `BoundaryProviderFact` from
/// `api-boundary.ts:92`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryProviderBasis {
	Annotation,
	Registration,
	Convention,
	Contract,
	Declaration,
}

/// How a consumer fact was determined. Mirror of the inline
/// `basis` union on `BoundaryConsumerFact` from
/// `api-boundary.ts:122`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryConsumerBasis {
	Literal,
	Template,
	Wrapper,
	Contract,
	Declaration,
}

/// How a boundary link match was determined. Mirror of the inline
/// `matchBasis` union on `BoundaryLinkCandidate` from
/// `api-boundary.ts:150`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryMatchBasis {
	ExactContract,
	AddressMatch,
	OperationMatch,
	Heuristic,
}

/// A boundary provider fact: something exposes an entry surface.
///
/// Mirror of `BoundaryProviderFact` from `api-boundary.ts:69`.
///
/// Examples:
/// - Spring `@GetMapping("/api/orders/{id}")`
/// - Express `app.get("/api/orders/:id")`
/// - gRPC service method `OrderService.GetOrder`
/// - IOCTL handler for `CMD_SET_MODE`
/// - Message topic subscriber
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryProviderFact {
	/// Transport mechanism.
	pub mechanism: BoundaryMechanism,
	/// Semantic operation identifier.
	pub operation: String,
	/// Protocol-specific address/path/command.
	pub address: String,
	/// Stable key of the handler symbol.
	pub handler_stable_key: String,
	/// Source file path.
	pub source_file: String,
	/// Source line (1-based, as declared by the TS model).
	pub line_start: i64,
	/// Framework or subsystem that produced this fact.
	pub framework: String,
	/// How the fact was determined.
	pub basis: BoundaryProviderBasis,
	/// Optional: schema/contract/DTO identity if known.
	pub schema_ref: Option<String>,
	/// Mechanism-specific metadata (transport-dependent fields).
	///
	/// Mirrors the TS `Record<string, unknown>` — arbitrary JSON
	/// object, preserved across the parity boundary. The
	/// `preserve_order` feature on serde_json (enabled at crate
	/// level) keeps key insertion order stable so JSON output is
	/// byte-equal between runtimes.
	pub metadata: Map<String, Value>,
}

/// A boundary consumer fact: something reaches across a boundary.
///
/// Mirror of `BoundaryConsumerFact` from `api-boundary.ts:109`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryConsumerFact {
	/// Transport mechanism.
	pub mechanism: BoundaryMechanism,
	/// Semantic operation identifier (same format as provider).
	pub operation: String,
	/// Protocol-specific address/path/command.
	pub address: String,
	/// Stable key of the calling symbol.
	pub caller_stable_key: String,
	/// Source file path.
	pub source_file: String,
	/// Source line (1-based).
	pub line_start: i64,
	/// How the fact was determined.
	pub basis: BoundaryConsumerBasis,
	/// Confidence in the extraction (0-1).
	pub confidence: f64,
	/// Optional: schema/contract identity if known.
	pub schema_ref: Option<String>,
	/// Mechanism-specific metadata. See note on
	/// `BoundaryProviderFact.metadata` for serde_json rationale.
	pub metadata: Map<String, Value>,
}

/// A matched boundary link candidate: a provider and consumer
/// were paired by a match strategy.
///
/// Mirror of `BoundaryLinkCandidate` from `api-boundary.ts:144`.
/// Stored in the `boundary_links` derived table, NOT in the core
/// `edges` table. Carries stable persisted fact UIDs, not object
/// references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryLinkCandidate {
	/// Persisted UID of the provider fact that was matched.
	pub provider_fact_uid: String,
	/// Persisted UID of the consumer fact that was matched.
	pub consumer_fact_uid: String,
	/// How the match was determined.
	pub match_basis: BoundaryMatchBasis,
	/// Combined confidence.
	pub confidence: f64,
}

// ── Matchable fact wrappers (for boundary matcher) ────────────────

/// A boundary provider fact with a stable persisted UID, ready
/// for the boundary matcher.
///
/// Mirror of the TS intersection type
/// `MatchableProviderFact = BoundaryProviderFact & { factUid: string }`
/// from `boundary-matcher.ts:43`.
///
/// The `#[serde(flatten)]` attribute on the inner `fact` field
/// serializes the fact's fields at the top level (alongside
/// `fact_uid`), matching the TS intersection-type behavior where
/// both `factUid` and all `BoundaryProviderFact` fields appear
/// as sibling JSON keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchableProviderFact {
	pub fact_uid: String,
	#[serde(flatten)]
	pub fact: BoundaryProviderFact,
}

/// A boundary consumer fact with a stable persisted UID, ready
/// for the boundary matcher.
///
/// Mirror of `MatchableConsumerFact = BoundaryConsumerFact & { factUid: string }`
/// from `boundary-matcher.ts:50`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchableConsumerFact {
	pub fact_uid: String,
	#[serde(flatten)]
	pub fact: BoundaryConsumerFact,
}

// ── Classifier signals ────────────────────────────────────────────

/// The set of package names declared as dependencies. Mirror of
/// `PackageDependencySet` from `signals.ts:40`.
///
/// Union of dependencies + devDependencies + peerDependencies +
/// optionalDependencies, de-duplicated and sorted for stability.
///
/// Uses `Vec<String>` per the D-A2 additional DTO collection rule:
/// no HashSet/HashMap in public parity-boundary types. Callers
/// that need O(1) membership construct a local HashSet inside
/// their classification logic and discard it before returning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDependencySet {
	pub names: Vec<String>,
}

/// Language-agnostic DTO representing identifiers and module
/// specifiers that exist at runtime without a declaration in
/// source. Mirror of `RuntimeBuiltinsSet` from `signals.ts:66`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBuiltinsSet {
	/// Identifiers globally available at runtime (Map, Date,
	/// process, etc.).
	pub identifiers: Vec<String>,
	/// Module specifiers for stdlib/runtime modules ("path",
	/// "node:fs", etc.).
	pub module_specifiers: Vec<String>,
}

/// A single entry from `compilerOptions.paths` in tsconfig.json.
/// Mirror of `TsconfigAliasEntry` from `signals.ts:106`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsconfigAliasEntry {
	pub pattern: String,
	pub substitutions: Vec<String>,
}

/// The full set of `compilerOptions.paths` entries. Mirror of
/// `TsconfigAliases` from `signals.ts:111`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsconfigAliases {
	pub entries: Vec<TsconfigAliasEntry>,
}

/// Snapshot-scoped classifier signals. Built once per indexing
/// pass; shared across every file's classification in that
/// snapshot.
///
/// Mirror of `SnapshotSignals` from `signals.ts:163`. Note that
/// `packageDependencies` and `tsconfigAliases` are NOT on this
/// struct — they live on `FileSignals` because monorepo sub-
/// packages declare their own deps and aliases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSignals {
	pub runtime_builtins: RuntimeBuiltinsSet,
}

impl SnapshotSignals {
	/// Construct an empty `SnapshotSignals`. Used when the repo
	/// has no recognized runtime vocabulary. The classifier
	/// degrades gracefully: rules that depend on these signals
	/// simply do not fire.
	///
	/// Mirror of `emptySnapshotSignals` from `signals.ts:220`.
	pub fn empty() -> Self {
		Self {
			runtime_builtins: RuntimeBuiltinsSet {
				identifiers: Vec::new(),
				module_specifiers: Vec::new(),
			},
		}
	}
}

/// Per-source-file classifier signals. Built once per source file
/// from the extractor's output.
///
/// Mirror of `FileSignals` from `signals.ts:193`, with the
/// DTO collection rule applied: same-file symbol collections are
/// `Vec<String>` instead of `ReadonlySet<string>`. The classifier
/// builds local `HashSet<&str>` for lookup inside its hot path.
///
/// Same-file symbol matching is SUBTYPE-AWARE. A flat name-set
/// confuses roles: a type-only `Foo` must NOT cause a runtime
/// `Foo()` call to classify as same-file. Each classifier rule
/// picks the appropriate set based on the category being
/// classified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSignals {
	/// Every identifier-bearing import binding in this file.
	pub import_bindings: Vec<ImportBinding>,
	/// Runtime-bindable identifiers declared in this file
	/// (FUNCTION, CLASS, METHOD, VARIABLE, CONSTANT, ENUM,
	/// ENUM_MEMBER, CONSTRUCTOR, GETTER, SETTER, PROPERTY).
	/// Used for CALLS and CALLS_OBJ_METHOD receiver matching.
	pub same_file_value_symbols: Vec<String>,
	/// Only NodeSubtype::Class names. Used for INSTANTIATES
	/// (`new Foo()`) matching.
	pub same_file_class_symbols: Vec<String>,
	/// Only NodeSubtype::Interface names. Used for IMPLEMENTS
	/// matching. Does NOT include CLASS names even though TS
	/// permits `implements ClassName` — conservative.
	pub same_file_interface_symbols: Vec<String>,
	/// Dependencies declared in the NEAREST package.json ancestor
	/// of this source file. In a monorepo, different files may
	/// have different dep sets.
	pub package_dependencies: PackageDependencySet,
	/// Path aliases from the NEAREST tsconfig.json ancestor of
	/// this source file. Follows `extends` chains (relative paths
	/// only in first slice).
	pub tsconfig_aliases: TsconfigAliases,
}

impl FileSignals {
	/// Construct an empty `FileSignals`. Used for files the
	/// extractor did not emit bindings for (non-TypeScript
	/// files, files with zero imports and zero declarations).
	/// The classifier falls through to `Unknown` for every edge
	/// in such files.
	///
	/// Mirror of `emptyFileSignals` from `signals.ts:232`.
	pub fn empty() -> Self {
		Self {
			import_bindings: Vec::new(),
			same_file_value_symbols: Vec::new(),
			same_file_class_symbols: Vec::new(),
			same_file_interface_symbols: Vec::new(),
			package_dependencies: PackageDependencySet { names: Vec::new() },
			tsconfig_aliases: TsconfigAliases { entries: Vec::new() },
		}
	}
}

// ── Blast-radius vocabulary ───────────────────────────────────────

/// The origin of an unresolved edge's receiver. Mirror of
/// `ReceiverOrigin` from `blast-radius.ts:32`.
///
/// `LocalLike` is the honest catch-all for receivers that are NOT
/// import-bound, NOT same-file-declared, NOT runtime globals, NOT
/// `this`-bound. They are likely local variables, parameters,
/// destructured locals, closure captures, or intermediate
/// expression roots. The label does NOT overclaim exact
/// provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiverOrigin {
	ThisBound,
	ImportBoundExternal,
	ImportBoundInternal,
	SameFileDeclared,
	RuntimeGlobal,
	RuntimeModule,
	LocalLike,
	NotApplicable,
}

/// Significance of the enclosing scope for an unresolved edge.
/// Mirror of `EnclosingScopeSignificance` from `blast-radius.ts:42`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnclosingScopeSignificance {
	ExportedPublic,
	InternalPrivate,
	UnknownScope,
}

/// Blast-radius level for an unresolved edge. Mirror of
/// `BlastRadiusLevel` from `blast-radius.ts:47`.
///
/// Entrypoint-path detection (which would drive HIGH) is deferred;
/// first slice never emits HIGH.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlastRadiusLevel {
	Low,
	Medium,
	High,
	NotApplicable,
}

/// Full blast-radius assessment for one unresolved edge. Mirror of
/// `BlastRadiusAssessment` from `blast-radius.ts:53`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlastRadiusAssessment {
	pub receiver_origin: ReceiverOrigin,
	pub enclosing_scope_significance: EnclosingScopeSignificance,
	pub blast_radius: BlastRadiusLevel,
}

// ── Classifier verdict ────────────────────────────────────────────

/// The output of `classify_unresolved_edge`. Mirror of
/// `ClassifierVerdict` from
/// `src/core/classification/unresolved-classifier.ts:54`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierVerdict {
	pub classification: UnresolvedEdgeClassification,
	pub basis_code: UnresolvedEdgeBasisCode,
}

// ── Framework entrypoint detection ─────────────────────────────────

/// An exported symbol eligible for framework entrypoint detection.
///
/// Narrowed input type for `detect_lambda_entrypoints`. Only
/// the fields the detector reads are included. Not a full graph
/// node — the caller constructs this from whatever node-shaped
/// data it has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedSymbol {
	pub stable_key: String,
	pub name: String,
	pub visibility: Option<String>,
	pub subtype: Option<String>,
}

/// A detected framework entrypoint. Mirror of `DetectedEntrypoint`
/// from `framework-entrypoints.ts:63`.
///
/// Returned by `detect_lambda_entrypoints`. Consumed by dead-code
/// analysis (symbol is live despite no internal callers) and trust
/// reporting (detected entrypoints vs declared entrypoints).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedEntrypoint {
	/// Stable key of the symbol identified as a framework entrypoint.
	pub target_stable_key: String,
	/// Machine-stable convention that matched.
	pub convention: String,
	/// Confidence in the detection (0–1).
	pub confidence: f64,
	/// Human-readable explanation.
	pub reason: String,
}

// ── Tests ─────────────────────────────────────────────────────────
//
// R3-B is types-only, so the tests here focus on serde
// serialization parity and enum-value assertions — the same
// discipline R2-B applied. Behavioral tests for the functions
// that consume these types land in R3-C / R3-D / R3-E / R3-F.

#[cfg(test)]
mod tests {
	use super::*;

	// ── Enum string-value serialization ───────────────────────

	#[test]
	fn unresolved_edge_category_serializes_snake_case() {
		let v = UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo;
		let s = serde_json::to_string(&v).unwrap();
		assert_eq!(s, "\"calls_obj_method_needs_type_info\"");
	}

	#[test]
	fn unresolved_edge_classification_serializes_snake_case() {
		let v = UnresolvedEdgeClassification::FrameworkBoundaryCandidate;
		let s = serde_json::to_string(&v).unwrap();
		assert_eq!(s, "\"framework_boundary_candidate\"");
	}

	#[test]
	fn unresolved_edge_basis_code_serializes_snake_case() {
		let v = UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal;
		let s = serde_json::to_string(&v).unwrap();
		assert_eq!(s, "\"this_receiver_implies_internal\"");
	}

	// EdgeType and Resolution tests removed: those types are
	// not in the classification crate's public surface per the
	// locked R3 boundary (extractor-port vocabulary excluded
	// beyond ImportBinding).

	#[test]
	fn boundary_mechanism_serializes_snake_case() {
		assert_eq!(
			serde_json::to_string(&BoundaryMechanism::Http).unwrap(),
			"\"http\""
		);
		assert_eq!(
			serde_json::to_string(&BoundaryMechanism::DatabaseSql).unwrap(),
			"\"database_sql\""
		);
		assert_eq!(
			serde_json::to_string(&BoundaryMechanism::IpcSharedMemory).unwrap(),
			"\"ipc_shared_memory\""
		);
	}

	#[test]
	fn boundary_role_serializes_lowercase() {
		assert_eq!(serde_json::to_string(&BoundaryRole::Provider).unwrap(), "\"provider\"");
		assert_eq!(serde_json::to_string(&BoundaryRole::Consumer).unwrap(), "\"consumer\"");
	}

	#[test]
	fn receiver_origin_serializes_snake_case() {
		let v = ReceiverOrigin::ImportBoundExternal;
		assert_eq!(serde_json::to_string(&v).unwrap(), "\"import_bound_external\"");
	}

	#[test]
	fn blast_radius_level_serializes_snake_case() {
		assert_eq!(serde_json::to_string(&BlastRadiusLevel::Low).unwrap(), "\"low\"");
		assert_eq!(
			serde_json::to_string(&BlastRadiusLevel::NotApplicable).unwrap(),
			"\"not_applicable\""
		);
	}

	// ── Struct field-name serialization (camelCase boundary) ──

	#[test]
	fn import_binding_serializes_camel_case_fields() {
		let ib = ImportBinding {
			identifier: "foo".into(),
			specifier: "./bar".into(),
			is_relative: true,
			location: None,
			is_type_only: false,
		};
		let s = serde_json::to_string(&ib).unwrap();
		assert!(s.contains("\"isRelative\":true"));
		assert!(s.contains("\"isTypeOnly\":false"));
		assert!(!s.contains("\"is_relative\""));
		assert!(!s.contains("\"is_type_only\""));
	}

	#[test]
	fn classifier_verdict_serializes_camel_case() {
		let v = ClassifierVerdict {
			classification: UnresolvedEdgeClassification::InternalCandidate,
			basis_code: UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal,
		};
		let s = serde_json::to_string(&v).unwrap();
		assert!(s.contains("\"classification\":\"internal_candidate\""));
		assert!(s.contains("\"basisCode\":\"this_receiver_implies_internal\""));
		assert!(!s.contains("\"basis_code\""));
	}

	#[test]
	fn boundary_provider_fact_metadata_preserves_key_order() {
		// Pins the preserve_order feature on serde_json:
		// key insertion order must be stable across round-trips.
		let json_in = r#"{
			"mechanism": "http",
			"operation": "GET /x",
			"address": "/x",
			"handlerStableKey": "r1:src/a.ts#fn:SYMBOL:FUNCTION",
			"sourceFile": "src/a.ts",
			"lineStart": 10,
			"framework": "express",
			"basis": "registration",
			"schemaRef": null,
			"metadata": {
				"zzz_last": true,
				"middle": 2,
				"aaa_first": "value"
			}
		}"#;
		let fact: BoundaryProviderFact = serde_json::from_str(json_in).unwrap();
		let json_out = serde_json::to_string(&fact.metadata).unwrap();
		// Insertion order (zzz_last, middle, aaa_first) must survive
		// the round-trip. Without preserve_order, serde_json would
		// alphabetize (aaa_first, middle, zzz_last), which would
		// break parity with TS JSON.stringify.
		let zzz_pos = json_out.find("zzz_last").unwrap();
		let middle_pos = json_out.find("middle").unwrap();
		let aaa_pos = json_out.find("aaa_first").unwrap();
		assert!(
			zzz_pos < middle_pos,
			"insertion order: zzz_last must precede middle, got {}",
			json_out
		);
		assert!(
			middle_pos < aaa_pos,
			"insertion order: middle must precede aaa_first, got {}",
			json_out
		);
	}

	// ── Helper methods ────────────────────────────────────────

	#[test]
	fn is_calls_category_covers_all_four_calls_variants() {
		assert!(UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo
			.is_calls_category());
		assert!(UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext
			.is_calls_category());
		assert!(UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo.is_calls_category());
		assert!(
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing.is_calls_category()
		);
	}

	#[test]
	fn is_calls_category_excludes_imports_and_other() {
		assert!(!UnresolvedEdgeCategory::ImportsFileNotFound.is_calls_category());
		assert!(!UnresolvedEdgeCategory::InstantiatesClassNotFound.is_calls_category());
		assert!(!UnresolvedEdgeCategory::ImplementsInterfaceNotFound.is_calls_category());
		assert!(!UnresolvedEdgeCategory::Other.is_calls_category());
	}

	#[test]
	fn is_imports_category_only_matches_imports_file_not_found() {
		assert!(UnresolvedEdgeCategory::ImportsFileNotFound.is_imports_category());
		for other in [
			UnresolvedEdgeCategory::InstantiatesClassNotFound,
			UnresolvedEdgeCategory::ImplementsInterfaceNotFound,
			UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo,
			UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
			UnresolvedEdgeCategory::Other,
		] {
			assert!(!other.is_imports_category());
		}
	}

	// ── Empty-signal constructors ─────────────────────────────

	#[test]
	fn snapshot_signals_empty_is_deterministic_empty_shape() {
		let s = SnapshotSignals::empty();
		assert!(s.runtime_builtins.identifiers.is_empty());
		assert!(s.runtime_builtins.module_specifiers.is_empty());
	}

	#[test]
	fn file_signals_empty_is_deterministic_empty_shape() {
		let s = FileSignals::empty();
		assert!(s.import_bindings.is_empty());
		assert!(s.same_file_value_symbols.is_empty());
		assert!(s.same_file_class_symbols.is_empty());
		assert!(s.same_file_interface_symbols.is_empty());
		assert!(s.package_dependencies.names.is_empty());
		assert!(s.tsconfig_aliases.entries.is_empty());
	}

	#[test]
	fn current_classifier_version_matches_ts() {
		// Mirror of
		// src/core/diagnostics/unresolved-edge-classification.ts:185.
		// Bumping this constant requires a corresponding TS bump
		// and a backfill strategy — see the TS source docstring.
		assert_eq!(CURRENT_CLASSIFIER_VERSION, 6);
	}

	// ── Framework entrypoint DTO serde shape ──────────────────

	#[test]
	fn exported_symbol_serializes_camel_case() {
		let sym = ExportedSymbol {
			stable_key: "r1:src/index.ts#handler:SYMBOL:FUNCTION".into(),
			name: "handler".into(),
			visibility: Some("export".into()),
			subtype: Some("FUNCTION".into()),
		};
		let s = serde_json::to_string(&sym).unwrap();
		assert!(s.contains("\"stableKey\":"));
		assert!(s.contains("\"visibility\":\"export\""));
		assert!(!s.contains("\"stable_key\""));
	}

	#[test]
	fn detected_entrypoint_serializes_camel_case() {
		let ep = DetectedEntrypoint {
			target_stable_key: "r1:src/index.ts#handler:SYMBOL:FUNCTION".into(),
			convention: "lambda_exported_handler".into(),
			confidence: 0.9,
			reason: "test".into(),
		};
		let s = serde_json::to_string(&ep).unwrap();
		assert!(s.contains("\"targetStableKey\":"));
		assert!(!s.contains("\"target_stable_key\""));
	}

	// ── Matchable fact wrapper serde shape ─────────────────────

	#[test]
	fn matchable_provider_fact_serializes_fact_uid_as_top_level_sibling() {
		// Pins the #[serde(flatten)] behavior. The TS intersection
		// type `BoundaryProviderFact & { factUid: string }` produces
		// a flat JSON object where factUid sits alongside all of the
		// BoundaryProviderFact's fields. If the flatten attribute is
		// removed or the wrapper is restructured, this test fails
		// because "factUid" would no longer appear at the top level
		// (it would be nested inside a "fact" key instead).
		let fact = MatchableProviderFact {
			fact_uid: "pf-1".to_string(),
			fact: BoundaryProviderFact {
				mechanism: BoundaryMechanism::Http,
				operation: "GET /api".to_string(),
				address: "/api".to_string(),
				handler_stable_key: "handler".to_string(),
				source_file: "src/app.ts".to_string(),
				line_start: 10,
				framework: "express".to_string(),
				basis: BoundaryProviderBasis::Registration,
				schema_ref: None,
				metadata: serde_json::Map::new(),
			},
		};
		let json = serde_json::to_value(&fact).unwrap();
		// factUid must be a top-level key, NOT nested under "fact".
		assert!(
			json.get("factUid").is_some(),
			"factUid must be a top-level key (serde flatten)"
		);
		assert!(
			json.get("fact").is_none(),
			"the inner 'fact' field must NOT appear as a key (serde flatten)"
		);
		// And the underlying fact fields must also be top-level.
		assert!(json.get("mechanism").is_some(), "mechanism must be top-level");
		assert!(json.get("address").is_some(), "address must be top-level");
		assert!(json.get("handlerStableKey").is_some(), "handlerStableKey must be top-level (camelCase)");
	}

	#[test]
	fn matchable_consumer_fact_serializes_fact_uid_as_top_level_sibling() {
		let fact = MatchableConsumerFact {
			fact_uid: "cf-1".to_string(),
			fact: BoundaryConsumerFact {
				mechanism: BoundaryMechanism::Http,
				operation: "GET /api".to_string(),
				address: "/api".to_string(),
				caller_stable_key: "caller".to_string(),
				source_file: "src/client.ts".to_string(),
				line_start: 20,
				basis: BoundaryConsumerBasis::Literal,
				confidence: 0.9,
				schema_ref: None,
				metadata: serde_json::Map::new(),
			},
		};
		let json = serde_json::to_value(&fact).unwrap();
		assert!(json.get("factUid").is_some());
		assert!(json.get("fact").is_none());
		assert!(json.get("callerStableKey").is_some());
	}
}
