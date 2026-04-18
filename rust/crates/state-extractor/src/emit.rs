//! `StateBoundaryEmitter` — snapshot-scoped emitter that consumes
//! per-callsite facts and produces `ExtractedNode` /
//! `ExtractedEdge` batches for the indexer pipeline.
//!
//! Design locks:
//!
//! - SB-2.1 (stateful struct): the emitter owns the resource-node
//!   dedup map. Dedup is intrinsically snapshot-scoped; forcing
//!   callers to thread the map would hide the same state in
//!   different plumbing.
//! - SB-2.2 (indexer types for outputs only): `ExtractedNode` and
//!   `ExtractedEdge` are the output shape. Inputs are crate-
//!   owned via `StateBoundaryCallsite`, which embeds
//!   `state-bindings` view types (`ImportView`, `CalleePath`) and
//!   the shared `SourceLocation` from the classification crate.
//! - SB-2.3 (in-memory dedup): `HashMap<stable_key, node>`.
//!   O(1) lookup. No storage round-trips.
//!
//! Slice-1 emission semantics (contract §3 / §7):
//!
//! - Binding direction `read`  → one `READS` edge.
//! - Binding direction `write` → one `WRITES` edge.
//! - Binding direction `read_write` → TWO edges (one `READS`, one
//!   `WRITES`). Each carries its own evidence blob whose
//!   `direction` field matches that edge's actual direction.
//!
//! - `Resolution::Static`   when the logical name is literal
//!   (`literal_identifier`, `normalized_path`, `normalized_url`).
//! - `Resolution::Inferred` when the logical name is derived from
//!   an env key (the stable key is shape-correct but the
//!   real-world endpoint depends on deploy-time config).
//!
//! Subtype inference:
//!
//! - DB → `Connection` (single subtype per contract §4.2).
//! - Cache → `Cache` (single subtype).
//! - Blob → `Bucket` (slice-1 blob coverage is AWS S3 only; the
//!   `Container` and `Namespace` subtypes are reserved for Azure
//!   and GCP expansions in later slices).
//! - FS → depends on `logical_name_source`:
//!   - `NormalizedPath` or `NormalizedUrl` → `FilePath` (literal
//!     paths reference concrete files; distinguishing
//!     DirectoryPath from FilePath without filesystem access is
//!     deferred).
//!   - `EnvKey` or `LiteralIdentifier` → `Logical`.

use std::collections::HashMap;

use repo_graph_classification::types::SourceLocation;
use repo_graph_indexer::types::{
	EdgeType, ExtractedEdge, ExtractedNode, NodeKind, NodeSubtype, Resolution,
};
use repo_graph_state_bindings::{
	build_blob, build_cache_state, build_db_resource, build_fs_path, match_form_a,
	table::{Direction, ResourceKind},
	BindingEntry, BindingTable, CalleePath, Driver, FsPathOrLogical, ImportView,
	Language, LogicalName, Provider, RepoUid, StableKey, ValidationError,
};

// ── Callsite logical-name variant ────────────────────────────────

/// A call site's logical-name payload, carrying the kind of
/// payload grammar the caller is offering.
///
/// FS payloads have a genuinely different grammar from the other
/// three resource families: contract §5.1's FS parsing-semantics
/// note explicitly permits `:` inside FS stable-key payloads
/// (Windows drive letters `C:\...`, URI-style references
/// `file:///...`), while DB / Cache / Blob stable-key segments
/// must remain naive-split-safe. This enum is the API-boundary
/// expression of that distinction.
///
/// Compatibility between the variant the caller passes and the
/// matched binding's resource kind is resolved by the emitter:
///
/// - FS binding:
///   - `Fs(..)` → used directly.
///   - `Generic(..)` → upgraded to `FsPathOrLogical`. Always
///     succeeds: `LogicalName`'s invariants are a strict subset
///     of `FsPathOrLogical`'s.
/// - DB / Cache / Blob binding:
///   - `Generic(..)` → used directly.
///   - `Fs(..)` → attempted downgrade to `LogicalName`. Succeeds
///     iff the FS payload contains no `:` (generic-segment rule).
///     Fails with `EmitError::FsPayloadInNonFsSlot` otherwise.
#[derive(Debug, Clone)]
pub enum CallsiteLogicalName {
	/// DB / Cache / Blob payload. No `:` permitted.
	Generic(LogicalName),
	/// FS payload. Permits `:` in the payload per contract §5.1.
	Fs(FsPathOrLogical),
}

impl CallsiteLogicalName {
	/// Borrow the underlying payload string. Unambiguous across
	/// both variants because the string value is the
	/// authoritative surface of each.
	pub fn as_str(&self) -> &str {
		match self {
			CallsiteLogicalName::Generic(ln) => ln.as_str(),
			CallsiteLogicalName::Fs(path) => path.as_str(),
		}
	}
}

use crate::evidence::{
	LogicalNameSource, StateBoundaryEvidence, STATE_BOUNDARY_EVIDENCE_VERSION,
};

// ── Input DTOs ────────────────────────────────────────────────────

/// One call-site fact offered to the emitter.
///
/// Inputs are crate-owned and narrow (SB-2.2 narrowing). The
/// `imports_in_file` and `callee` fields come from
/// `state-bindings` view types; the `source_location` field comes
/// from the shared `classification::SourceLocation` used
/// throughout the extractor layer.
#[derive(Debug, Clone)]
pub struct StateBoundaryCallsite {
	/// `node_uid` of the enclosing symbol (the caller of the
	/// matched API). The edge's `source_node_uid` will be this
	/// value.
	pub source_node_uid: String,
	/// `file_uid` of the file containing the call site. Carried
	/// for edge provenance; state-extractor does not persist it
	/// separately in slice 1 (edges inherit file context from
	/// their source symbol).
	pub file_uid: String,
	/// Source location of the call site.
	pub source_location: SourceLocation,
	/// Every `ImportView` present in the file. Order is
	/// irrelevant; the matcher scans for any module match.
	pub imports_in_file: Vec<ImportView>,
	/// The resolved callee at the call site.
	pub callee: CalleePath,
	/// The logical-name segment for the target resource, selected
	/// by the caller per contract §5.3 precedence.
	///
	/// The enum variant carries the caller's payload grammar:
	/// `Generic` for DB / Cache / Blob (no `:`), `Fs` for
	/// filesystem (permits `:` per contract §5.1). The emitter
	/// resolves variant-vs-binding-kind compatibility at match
	/// time (see `CallsiteLogicalName` docs).
	pub logical_name: CallsiteLogicalName,
	/// Provenance tag for the logical name, carried into
	/// evidence and used to infer `Resolution` and the FS
	/// subtype.
	pub logical_name_source: LogicalNameSource,
}

/// Snapshot-level parameters supplied once at emitter
/// construction.
#[derive(Debug, Clone)]
pub struct EmitterContext {
	/// `repo_uid` stamped on every emitted node and edge.
	pub repo_uid: RepoUid,
	/// Snapshot UID stamped on every emitted node and edge.
	pub snapshot_uid: String,
	/// Source language of the files this emitter serves.
	pub language: Language,
	/// Extractor name stamped on every emitted edge
	/// (e.g. `"state-extractor:0.1.0"`).
	pub extractor_name: String,
}

// ── Output aggregate ──────────────────────────────────────────────

/// The emitter's accumulated output for a snapshot. Consumed by
/// the caller once per-file emission is complete. Node order is
/// first-seen stable-key insertion order (deterministic across
/// repeated builds for a given input ordering).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmittedFacts {
	/// Resource nodes (deduped by `stable_key`).
	pub nodes: Vec<ExtractedNode>,
	/// READS / WRITES edges referencing the nodes above via
	/// `target_key` = resource stable key.
	pub edges: Vec<ExtractedEdge>,
}

// ── Emitter errors ────────────────────────────────────────────────

/// Emission-time failure.
#[derive(Debug)]
pub enum EmitError {
	/// A stable-key-segment validation failed. Unreachable when
	/// the binding table was loaded through
	/// `BindingTable::load_str` (which rejects the offending
	/// inputs upstream); carried here rather than panicked so
	/// downstream call sites can surface the failure if a
	/// pathological table ever bypasses the loader.
	InvalidBindingSegment(ValidationError),

	/// The caller supplied `CallsiteLogicalName::Fs(..)` but the
	/// matched binding targets a non-FS resource kind AND the FS
	/// payload contains characters (specifically `:`) that the
	/// target kind's stable-key segment grammar forbids. This is
	/// an architectural mismatch surface — the caller's variant
	/// choice is incompatible with the matched binding's kind,
	/// and the downgrade path is blocked. Contract §5.1's
	/// parsing-semantics note permits `:` only in FS payloads.
	FsPayloadInNonFsSlot {
		/// The resource kind of the matched binding.
		resource_kind: ResourceKind,
		/// The FS payload string that could not be downgraded.
		payload: String,
	},
}

impl std::fmt::Display for EmitError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			EmitError::InvalidBindingSegment(err) => {
				write!(f, "state-boundary emit failed: {}", err)
			}
			EmitError::FsPayloadInNonFsSlot {
				resource_kind,
				payload,
			} => {
				write!(
					f,
					"state-boundary emit failed: FS-capable payload {:?} cannot be downgraded to a {:?}-kind stable-key segment (segment grammar forbids `:`)",
					payload, resource_kind
				)
			}
		}
	}
}

impl std::error::Error for EmitError {}

impl From<ValidationError> for EmitError {
	fn from(value: ValidationError) -> Self {
		EmitError::InvalidBindingSegment(value)
	}
}

// ── Emitter ──────────────────────────────────────────────────────

/// Snapshot-scoped state-boundary emitter.
///
/// Holds the binding-table reference, snapshot-level context, and
/// the resource-node dedup map. Callers invoke
/// `emit_for_callsite` once per call site and `drain` once at
/// snapshot close.
///
/// The lifetime parameter `'t` binds the emitter to the loaded
/// binding table. Callers typically pass
/// `BindingTable::load_embedded()` (which returns `&'static
/// BindingTable`), giving `'t = 'static`; tests can pass shorter-
/// lived borrows.
pub struct StateBoundaryEmitter<'t> {
	table: &'t BindingTable,
	context: EmitterContext,
	/// Dedup map keyed on resource stable-key string.
	/// Insertion order is preserved via an auxiliary Vec of
	/// insertion-order keys; `HashMap` alone is not order-stable.
	resource_nodes: HashMap<String, ExtractedNode>,
	/// Order of first-seen stable keys.
	resource_insertion_order: Vec<String>,
	edges: Vec<ExtractedEdge>,
}

impl<'t> StateBoundaryEmitter<'t> {
	/// Construct a new emitter for a snapshot.
	pub fn new(table: &'t BindingTable, context: EmitterContext) -> Self {
		Self {
			table,
			context,
			resource_nodes: HashMap::new(),
			resource_insertion_order: Vec::new(),
			edges: Vec::new(),
		}
	}

	/// Match one call site against the binding table. If Form-A
	/// matches, build or reuse the target resource node and emit
	/// the appropriate READS / WRITES edge(s). Returns the number
	/// of edges emitted (0 if no match, 1 for read or write, 2
	/// for read_write).
	pub fn emit_for_callsite(
		&mut self,
		callsite: &StateBoundaryCallsite,
	) -> Result<usize, EmitError> {
		// Form-A match. No match → no emission.
		let Some(match_result) = match_form_a(
			&callsite.imports_in_file,
			&callsite.callee,
			self.table,
			self.context.language,
		) else {
			return Ok(0);
		};

		// Build the resource's stable key from the matched
		// binding's resource_kind + driver and the caller-supplied
		// logical name.
		let stable_key = build_resource_stable_key(
			&self.context.repo_uid,
			match_result.binding,
			&callsite.logical_name,
		)?;

		// Ensure the resource node exists (dedup or insert) and
		// get its stable-key string back for edge targeting.
		let target_key = self.ensure_resource_node(
			stable_key,
			match_result.binding,
			&callsite.logical_name,
			callsite.logical_name_source,
		);

		// Build the edges per direction (contract §3.2).
		let resolution = resolution_for_logical_source(callsite.logical_name_source);
		let direction = match_result.binding.direction;
		let binding_key = match_result.binding_key.clone();
		let basis = match_result.binding.basis;
		let binding_notes = match_result.binding.notes.clone();

		let emitted = match direction {
			Direction::Read => {
				self.emit_edge(
					callsite,
					&target_key,
					EdgeType::Reads,
					resolution,
					Direction::Read,
					&binding_key,
					basis,
					binding_notes,
				);
				1
			}
			Direction::Write => {
				self.emit_edge(
					callsite,
					&target_key,
					EdgeType::Writes,
					resolution,
					Direction::Write,
					&binding_key,
					basis,
					binding_notes,
				);
				1
			}
			Direction::ReadWrite => {
				// Two edges. Each carries its own evidence with
				// the edge's actual direction (not `read_write`).
				self.emit_edge(
					callsite,
					&target_key,
					EdgeType::Reads,
					resolution,
					Direction::Read,
					&binding_key,
					basis,
					binding_notes.clone(),
				);
				self.emit_edge(
					callsite,
					&target_key,
					EdgeType::Writes,
					resolution,
					Direction::Write,
					&binding_key,
					basis,
					binding_notes,
				);
				2
			}
		};
		Ok(emitted)
	}

	/// Consume the emitter and return the accumulated nodes and
	/// edges. Node order reflects first-seen stable-key order;
	/// edge order reflects emission order.
	pub fn drain(self) -> EmittedFacts {
		let StateBoundaryEmitter {
			mut resource_nodes,
			resource_insertion_order,
			edges,
			..
		} = self;
		let mut nodes = Vec::with_capacity(resource_insertion_order.len());
		for key in resource_insertion_order {
			if let Some(n) = resource_nodes.remove(&key) {
				nodes.push(n);
			}
		}
		EmittedFacts { nodes, edges }
	}

	// ── Private helpers ──────────────────────────────────────

	fn ensure_resource_node(
		&mut self,
		stable_key: StableKey,
		binding: &BindingEntry,
		logical_name: &CallsiteLogicalName,
		logical_name_source: LogicalNameSource,
	) -> String {
		let stable_key_str = stable_key.into_string();
		if !self.resource_nodes.contains_key(&stable_key_str) {
			let node = build_resource_node(
				&stable_key_str,
				binding,
				logical_name,
				logical_name_source,
				&self.context,
			);
			self.resource_insertion_order
				.push(stable_key_str.clone());
			self.resource_nodes.insert(stable_key_str.clone(), node);
		}
		stable_key_str
	}

	#[allow(clippy::too_many_arguments)]
	fn emit_edge(
		&mut self,
		callsite: &StateBoundaryCallsite,
		target_stable_key: &str,
		edge_type: EdgeType,
		resolution: Resolution,
		direction: Direction,
		binding_key: &str,
		basis: repo_graph_state_bindings::Basis,
		binding_notes: Option<String>,
	) {
		let evidence = StateBoundaryEvidence {
			state_boundary_version: STATE_BOUNDARY_EVIDENCE_VERSION,
			basis,
			binding_key: binding_key.to_string(),
			direction,
			logical_name_source: callsite.logical_name_source,
			binding_notes,
		};
		let metadata_json = serde_json::to_string(&evidence)
			.expect("StateBoundaryEvidence must always serialize — the struct is schema-stable");

		self.edges.push(ExtractedEdge {
			edge_uid: uuid::Uuid::new_v4().to_string(),
			snapshot_uid: self.context.snapshot_uid.clone(),
			repo_uid: self.context.repo_uid.as_str().to_string(),
			source_node_uid: callsite.source_node_uid.clone(),
			target_key: target_stable_key.to_string(),
			edge_type,
			resolution,
			extractor: self.context.extractor_name.clone(),
			location: Some(callsite.source_location),
			metadata_json: Some(metadata_json),
		});
	}
}

// ── Free helpers ──────────────────────────────────────────────────

/// Map a logical-name source to the slice-1 resolution tag per
/// contract §7.
fn resolution_for_logical_source(source: LogicalNameSource) -> Resolution {
	match source {
		// Env-derived logical names are shape-correct but the
		// real endpoint depends on runtime config.
		LogicalNameSource::EnvKey => Resolution::Inferred,
		// Literal-backed names are deterministic.
		LogicalNameSource::LiteralIdentifier
		| LogicalNameSource::NormalizedPath
		| LogicalNameSource::NormalizedUrl => Resolution::Static,
	}
}

/// Resolve the caller's logical-name variant into a
/// `LogicalName` for non-FS resource kinds. Enforces the
/// generic-segment `:`-rejection rule at the downgrade path.
fn resolve_as_generic(
	payload: &CallsiteLogicalName,
	kind: ResourceKind,
) -> Result<LogicalName, EmitError> {
	match payload {
		CallsiteLogicalName::Generic(ln) => Ok(ln.clone()),
		CallsiteLogicalName::Fs(path) => LogicalName::new(path.as_str().to_string()).map_err(
			|_| EmitError::FsPayloadInNonFsSlot {
				resource_kind: kind,
				payload: path.as_str().to_string(),
			},
		),
	}
}

/// Resolve the caller's logical-name variant into an
/// `FsPathOrLogical` for FS resource kinds. Upgrading a
/// `Generic(LogicalName)` always succeeds because
/// `FsPathOrLogical`'s invariants are a strict superset of
/// `LogicalName`'s.
fn resolve_as_fs(payload: &CallsiteLogicalName) -> Result<FsPathOrLogical, ValidationError> {
	match payload {
		CallsiteLogicalName::Fs(path) => Ok(path.clone()),
		CallsiteLogicalName::Generic(ln) => FsPathOrLogical::new(ln.as_str().to_string()),
	}
}

/// Build the stable key for the target resource by dispatching on
/// the binding's `resource_kind`.
fn build_resource_stable_key(
	repo_uid: &RepoUid,
	binding: &BindingEntry,
	logical_name: &CallsiteLogicalName,
) -> Result<StableKey, EmitError> {
	match binding.resource_kind {
		ResourceKind::Db => {
			let ln = resolve_as_generic(logical_name, ResourceKind::Db)?;
			let driver = Driver::new(binding.driver.clone())?;
			Ok(build_db_resource(repo_uid, &driver, &ln))
		}
		ResourceKind::Fs => {
			let path = resolve_as_fs(logical_name)?;
			Ok(build_fs_path(repo_uid, &path))
		}
		ResourceKind::Cache => {
			let ln = resolve_as_generic(logical_name, ResourceKind::Cache)?;
			let driver = Driver::new(binding.driver.clone())?;
			Ok(build_cache_state(repo_uid, &driver, &ln))
		}
		ResourceKind::Blob => {
			let ln = resolve_as_generic(logical_name, ResourceKind::Blob)?;
			let provider = Provider::new(binding.driver.clone())?;
			Ok(build_blob(repo_uid, &provider, &ln))
		}
	}
}

/// Map a binding's resource_kind + logical-name source to the
/// slice-1 (NodeKind, NodeSubtype) pair. See module docs for the
/// full policy.
fn node_kind_and_subtype_for_binding(
	binding: &BindingEntry,
	logical_name_source: LogicalNameSource,
) -> (NodeKind, NodeSubtype) {
	match binding.resource_kind {
		ResourceKind::Db => (NodeKind::DbResource, NodeSubtype::Connection),
		ResourceKind::Cache => (NodeKind::State, NodeSubtype::Cache),
		ResourceKind::Blob => (NodeKind::Blob, NodeSubtype::Bucket),
		ResourceKind::Fs => {
			let subtype = match logical_name_source {
				// Literal filesystem paths / URLs reference
				// concrete files in slice 1. DirectoryPath is
				// reserved but NOT selected automatically;
				// distinguishing it from FilePath without
				// filesystem access is deferred.
				LogicalNameSource::NormalizedPath | LogicalNameSource::NormalizedUrl => {
					NodeSubtype::FilePath
				}
				// Env/config-derived or stable-identifier names
				// describe a logical filesystem resource rather
				// than a concrete path.
				LogicalNameSource::EnvKey | LogicalNameSource::LiteralIdentifier => {
					NodeSubtype::Logical
				}
			};
			(NodeKind::FsPath, subtype)
		}
	}
}

/// Construct the `ExtractedNode` for a newly-discovered resource.
fn build_resource_node(
	stable_key: &str,
	binding: &BindingEntry,
	logical_name: &CallsiteLogicalName,
	logical_name_source: LogicalNameSource,
	context: &EmitterContext,
) -> ExtractedNode {
	let (kind, subtype) = node_kind_and_subtype_for_binding(binding, logical_name_source);
	ExtractedNode {
		node_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: context.snapshot_uid.clone(),
		repo_uid: context.repo_uid.as_str().to_string(),
		stable_key: stable_key.to_string(),
		kind,
		subtype: Some(subtype),
		name: logical_name.as_str().to_string(),
		qualified_name: None,
		file_uid: None,
		parent_node_uid: None,
		location: None,
		signature: None,
		visibility: None,
		doc_comment: None,
		metadata_json: None,
	}
}
