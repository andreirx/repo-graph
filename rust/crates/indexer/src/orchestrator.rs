//! Core indexing orchestration — the base indexing workflow.
//!
//! Mirror of the core pipeline in
//! `src/adapters/indexer/repo-indexer.ts` (the `runIndex` path).
//!
//! This module implements the structural graph pipeline:
//!   1. Snapshot creation (BUILDING)
//!   2. File tracking + extraction
//!   3. Module node creation
//!   4. Edge resolution (batched)
//!   5. Unresolved edge classification
//!   6. Module edge derivation (OWNS + MODULE→MODULE IMPORTS)
//!   7. Snapshot finalization (counts, diagnostics, READY)
//!
//! NOT in scope (deferred enrichment):
//!   - Postpasses (boundary extraction, framework detectors, etc.)
//!   - Annotations, measurements, manifest versions
//!   - Delta/refresh indexing (R5-H)
//!
//! The orchestrator receives file content from the caller — it
//! does no filesystem I/O. All persistence goes through
//! `IndexerStoragePort`. All extraction goes through
//! `ExtractorPort`.

use std::collections::{BTreeMap, HashMap};

use repo_graph_classification::classify_unresolved_edge;
use repo_graph_classification::types::{
	FileSignals, PackageDependencySet, RuntimeBuiltinsSet, SnapshotSignals,
	SourceLocation, TsconfigAliases,
};

use crate::extractor_port::{ExtractorError, ExtractorPort};
use crate::resolver::{
	build_file_resolution_map, get_module_path, resolve_edges, ResolverIndex,
};
use crate::routing::{self, detect_language, is_test_file, MAX_FILE_SIZE_BYTES};
use crate::storage_port::{
	CreateSnapshotInput, ExtractionEdgeRow, FileSignalRow, FileVersion,
	IndexerStoragePort, PersistedUnresolvedEdge, TrackedFile,
	UpdateSnapshotStatusInput,
};
use crate::types::{
	EdgeType, IndexOptions, IndexResult, NodeKind, NodeSubtype,
	ParseStatus, Resolution, SnapshotKind, SnapshotStatus, ExtractedNode,
};

// ── Constants ────────────────────────────────────────────────────

const DEFAULT_EDGE_BATCH_SIZE: usize = 10_000;
const CLASSIFIER_VERSION: u32 = 1;

/// Indexer version string stamped on module-derived edges.
const INDEXER_VERSION: &str = "indexer:1.0.0";

// ── Error type ───────────────────────────────────────────────────

/// Error from the indexing orchestration pipeline.
///
/// Carries both storage failures and extractor failures through
/// the same typed channel, so callers get a single `Result` path.
#[derive(Debug)]
pub enum IndexError<E> {
	/// A storage operation failed.
	Storage(E),
	/// An extractor's `initialize()` call failed.
	ExtractorInit {
		extractor_name: String,
		source: ExtractorError,
	},
}

impl<E: std::fmt::Display> std::fmt::Display for IndexError<E> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Storage(e) => write!(f, "storage error: {}", e),
			Self::ExtractorInit { extractor_name, source } => {
				write!(f, "extractor {} init failed: {}", extractor_name, source)
			}
		}
	}
}

// ── Input types ──────────────────────────────────────────────────

/// A file provided by the caller for indexing. The orchestrator
/// does not read files from disk — the caller provides the content.
///
/// The caller is also responsible for providing pre-computed
/// package dependencies and tsconfig aliases for each file (from
/// config readers that walk `package.json` / `tsconfig.json`).
/// These feed into the unresolved-edge classifier. If not
/// provided (`None`), classification proceeds with empty signals
/// (which weakens alias-based and registry-based resolution).
pub struct FileInput {
	/// Repo-relative path (forward slashes).
	pub rel_path: String,
	/// UTF-8 source text.
	pub content: String,
	/// Pre-computed content hash (e.g., SHA-256 hex).
	pub content_hash: String,
	/// File size in bytes.
	pub size_bytes: usize,
	/// Line count.
	pub line_count: usize,
	/// Pre-computed package dependencies for this file's nearest
	/// owning manifest (e.g., package.json deps). Typed, not
	/// raw JSON — the caller parses at the adapter boundary.
	pub package_dependencies: Option<PackageDependencySet>,
	/// Pre-computed tsconfig path aliases for this file's nearest
	/// owning tsconfig.json. Typed, not raw JSON.
	pub tsconfig_aliases: Option<TsconfigAliases>,
}

// ── Main orchestration ───────────────────────────────────────────

/// Run the core indexing pipeline on a set of files.
///
/// Creates a snapshot in BUILDING status, extracts all files,
/// resolves edges, classifies unresolved edges, creates module
/// structure, finalizes the snapshot to READY, and returns the
/// result.
///
/// The caller is responsible for:
///   - Scanning the filesystem
///   - Reading file content + computing hashes
///   - Filtering by include/exclude patterns
///   - Providing only source-extension files
///   - Computing per-file package deps and tsconfig aliases
///
/// Fatal errors (storage or extractor-init failures) abort the
/// pipeline. On any fatal error after snapshot creation, the
/// snapshot is transitioned to FAILED before returning.
/// Non-fatal errors (per-file extraction failures, oversized
/// files) are recorded in the snapshot diagnostics.
pub fn index_repo<S: IndexerStoragePort>(
	storage: &mut S,
	extractors: &mut [&mut dyn ExtractorPort],
	repo_uid: &str,
	files: &[FileInput],
	options: &mut IndexOptions,
	hook: Option<&mut dyn crate::hook::ExtractionResultHook>,
) -> Result<IndexResult, IndexError<S::StorageError>> {
	let start = std::time::Instant::now();

	// ── Initialize extractors ────────────────────────────────
	for ext in extractors.iter_mut() {
		if let Err(e) = ext.initialize() {
			return Err(IndexError::ExtractorInit {
				extractor_name: ext.name().to_string(),
				source: e,
			});
		}
	}

	// ── Build routing table ──────────────────────────────────
	let ext_refs: Vec<&dyn ExtractorPort> = extractors.iter().map(|e| &**e).collect();
	let routing_table = routing::build_extension_routing_table(&ext_refs);

	// ── Build snapshot signals (runtime builtins from all extractors) ──
	let mut all_identifiers = Vec::new();
	let mut all_module_specifiers = Vec::new();
	for ext in extractors.iter() {
		let builtins = ext.runtime_builtins();
		all_identifiers.extend(builtins.identifiers.iter().cloned());
		all_module_specifiers.extend(builtins.module_specifiers.iter().cloned());
	}
	let snapshot_signals = SnapshotSignals {
		runtime_builtins: RuntimeBuiltinsSet {
			identifiers: all_identifiers,
			module_specifiers: all_module_specifiers,
		},
	};

	// ── Create snapshot ──────────────────────────────────────
	let toolchain_json = build_toolchain_json(extractors);
	let snapshot = storage.create_snapshot(&CreateSnapshotInput {
		repo_uid: repo_uid.into(),
		kind: SnapshotKind::Full,
		basis_ref: None,
		basis_commit: options.basis_commit.clone(),
		parent_snapshot_uid: None,
		label: None,
		toolchain_json: Some(toolchain_json),
	}).map_err(IndexError::Storage)?;
	let snap_uid = snapshot.snapshot_uid.clone();

	// Run the pipeline. On any failure, transition the snapshot
	// to FAILED before returning the error.
	let created_at = snapshot.created_at.clone();
	let progress = &mut options.on_progress;
	let all_file_paths: Vec<String> = files.iter().map(|f| f.rel_path.clone()).collect();
	// Full index: no copy-forward, so no copied resource keys.
	let empty_resource_keys: HashMap<String, crate::storage_port::CopiedResourceNodeKey> = HashMap::new();
	match run_pipeline(storage, extractors, repo_uid, &snap_uid, files, &all_file_paths, &snapshot_signals, &routing_table, &created_at, options.edge_batch_size, progress, start, hook, &empty_resource_keys) {
		Ok(result) => Ok(result),
		Err(storage_err) => {
			// Best-effort: transition to FAILED. If this also fails,
			// we still return the original error.
			let _ = storage.update_snapshot_status(&UpdateSnapshotStatusInput {
				snapshot_uid: snap_uid,
				status: SnapshotStatus::Failed,
				completed_at: None,
			});
			Err(IndexError::Storage(storage_err))
		}
	}
}

/// The actual pipeline, extracted so `index_repo` can catch errors
/// and transition the snapshot to FAILED.
///
/// `files` — files to extract (may be a subset during refresh).
/// `all_file_paths` — ALL file paths in the snapshot (copied +
///   extracted) for module-node creation and resolution context.
///   For full index, this is the same as the extracted file paths.
fn run_pipeline<S: IndexerStoragePort>(
	storage: &mut S,
	extractors: &mut [&mut dyn ExtractorPort],
	repo_uid: &str,
	snap_uid: &str,
	files: &[FileInput],
	all_file_paths: &[String],
	snapshot_signals: &SnapshotSignals,
	routing_table: &BTreeMap<String, usize>,
	created_at: &str,
	edge_batch_size: Option<usize>,
	progress: &mut Option<crate::types::ProgressCallback>,
	start: std::time::Instant,
	mut hook: Option<&mut dyn crate::hook::ExtractionResultHook>,
	copied_resource_keys: &HashMap<String, crate::storage_port::CopiedResourceNodeKey>,
) -> Result<IndexResult, S::StorageError> {
	let now_iso = created_at.to_string();
	let total_files = files.len() as u64;

	// Helper: emit progress if callback is set.
	let mut emit = |phase: crate::types::IndexPhase, current: u64, total: u64, file: Option<String>| {
		if let Some(ref mut cb) = progress {
			cb(&crate::types::IndexProgressEvent { phase, current, total, file });
		}
	};

	emit(crate::types::IndexPhase::Extracting, 0, total_files, None);

	// ── Phase 1: Extract files ───────────────────────────────
	let mut tracked_files: Vec<TrackedFile> = Vec::new();
	let mut file_versions: Vec<FileVersion> = Vec::new();
	let mut all_nodes: Vec<ExtractedNode> = Vec::new();
	let mut all_extraction_edges: Vec<ExtractionEdgeRow> = Vec::new();
	let mut all_signals: Vec<FileSignalRow> = Vec::new();
	let mut skipped_oversized: u64 = 0;
	let mut files_read_failed: u64 = 0;
	let mut nodes_total: u64 = 0;

	for (file_idx, file) in files.iter().enumerate() {
		emit(crate::types::IndexPhase::Extracting, file_idx as u64 + 1, total_files, Some(file.rel_path.clone()));
		let file_uid = format!("{}:{}", repo_uid, file.rel_path);
		let language = detect_language(&file.rel_path);
		let is_test = is_test_file(&file.rel_path);

		tracked_files.push(TrackedFile {
			file_uid: file_uid.clone(),
			repo_uid: repo_uid.into(),
			path: file.rel_path.clone(),
			language: language.map(|s| s.to_string()),
			is_test,
			is_generated: false,
			is_excluded: false,
		});

		// Skip oversized files.
		if file.size_bytes > MAX_FILE_SIZE_BYTES {
			file_versions.push(FileVersion {
				snapshot_uid: snap_uid.to_string(),
				file_uid: file_uid.clone(),
				content_hash: file.content_hash.clone(),
				ast_hash: None,
				extractor: Some("skipped:oversized".into()),
				parse_status: ParseStatus::Skipped,
				size_bytes: Some(file.size_bytes as u64),
				line_count: Some(file.line_count as u64),
				indexed_at: now_iso.clone(),
			});
			skipped_oversized += 1;
			continue;
		}

		// Route to extractor.
		let extractor_idx = routing::route_file(&file.rel_path, &routing_table);
		let extractor = extractor_idx.map(|idx| &*extractors[idx]);

		let extractor_name = extractor.map(|e| e.name().to_string());

		file_versions.push(FileVersion {
			snapshot_uid: snap_uid.to_string(),
			file_uid: file_uid.clone(),
			content_hash: file.content_hash.clone(),
			ast_hash: None,
			extractor: extractor_name.clone(),
			parse_status: if extractor.is_some() {
				ParseStatus::Parsed
			} else {
				ParseStatus::Skipped
			},
			size_bytes: Some(file.size_bytes as u64),
			line_count: Some(file.line_count as u64),
			indexed_at: now_iso.clone(),
		});

		let extractor = match extractor {
			Some(e) => e,
			None => continue,
		};

		// Extract.
		let result = match extractor.extract(
			&file.content,
			&file.rel_path,
			&file_uid,
			repo_uid,
			snap_uid,
		) {
			Ok(r) => r,
			Err(_e) => {
				// Non-fatal: record as failed, continue.
				if let Some(fv) = file_versions.last_mut() {
					fv.parse_status = ParseStatus::Failed;
				}
				files_read_failed += 1;
				continue;
			}
		};

		// Hook: hand off the extraction result for any hook
		// processing (e.g. state-boundary emission) BEFORE moving
		// nodes/edges into accumulators. The hook borrows `result`
		// immutably; it accumulates internally and drains at
		// snapshot close (below).
		if let Some(ref mut h) = hook {
			h.on_extraction_result(repo_uid, snap_uid, &file_uid, &file.rel_path, &result);
		}

		nodes_total += result.nodes.len() as u64;
		all_nodes.extend(result.nodes);

		for edge in &result.edges {
			all_extraction_edges.push(ExtractionEdgeRow {
				edge_uid: edge.edge_uid.clone(),
				snapshot_uid: edge.snapshot_uid.clone(),
				repo_uid: edge.repo_uid.clone(),
				source_node_uid: edge.source_node_uid.clone(),
				target_key: edge.target_key.clone(),
				edge_type: edge.edge_type,
				resolution: edge.resolution,
				extractor: edge.extractor.clone(),
				line_start: edge.location.map(|l| l.line_start),
				col_start: edge.location.map(|l| l.col_start),
				line_end: edge.location.map(|l| l.line_end),
				col_end: edge.location.map(|l| l.col_end),
				metadata_json: edge.metadata_json.clone(),
				source_file_uid: Some(file_uid.clone()),
			});
		}

		// Persist file signals if any signal data exists (import
		// bindings from extraction, or package deps / tsconfig
		// aliases from caller-provided typed config).
		let has_bindings = !result.import_bindings.is_empty();
		let has_pkg_deps = file.package_dependencies.is_some();
		let has_aliases = file.tsconfig_aliases.is_some();
		if has_bindings || has_pkg_deps || has_aliases {
			all_signals.push(FileSignalRow {
				snapshot_uid: snap_uid.to_string(),
				file_uid: file_uid.clone(),
				import_bindings_json: if has_bindings {
					Some(serde_json::to_string(&result.import_bindings).unwrap_or_default())
				} else {
					None
				},
				// Serialize typed DTOs to JSON at the storage boundary.
				package_dependencies_json: file
					.package_dependencies
					.as_ref()
					.map(|p| serde_json::to_string(p).unwrap_or_default()),
				tsconfig_aliases_json: file
					.tsconfig_aliases
					.as_ref()
					.map(|t| serde_json::to_string(t).unwrap_or_default()),
			});
		}
	}

	// Drain hook extras before phase-1 persistence so
	// hook-produced nodes + edges are included in the same
	// persistence batch. Diagnostics are rendered to stderr.
	if let Some(h) = hook {
		let extras = h.drain_snapshot_extras();

		// ── Edge attribution (Fix B.1) ──────────────────────
		// Derive source_file_uid for hook-drained edges from the
		// source symbol's file_uid. This ensures copy-forward
		// preserves state-boundary edges for unchanged files.
		let node_to_file: std::collections::HashMap<&str, &str> = all_nodes
			.iter()
			.filter_map(|n| {
				n.file_uid
					.as_deref()
					.map(|f| (n.node_uid.as_str(), f))
			})
			.collect();

		for extra_edge in extras.edges {
			let source_file = node_to_file
				.get(extra_edge.source_node_uid.as_str())
				.map(|f| f.to_string());
			all_extraction_edges.push(ExtractionEdgeRow {
				edge_uid: extra_edge.edge_uid,
				snapshot_uid: extra_edge.snapshot_uid,
				repo_uid: extra_edge.repo_uid,
				source_node_uid: extra_edge.source_node_uid,
				target_key: extra_edge.target_key,
				edge_type: extra_edge.edge_type,
				resolution: extra_edge.resolution,
				extractor: extra_edge.extractor,
				line_start: extra_edge.location.map(|l| l.line_start),
				col_start: extra_edge.location.map(|l| l.col_start),
				line_end: extra_edge.location.map(|l| l.line_end),
				col_end: extra_edge.location.map(|l| l.col_end),
				metadata_json: extra_edge.metadata_json,
				source_file_uid: source_file,
			});
		}

		// ── Resource-node dedup against copy-forward (Fix B.3) ──
		// If refresh copied null-file resource nodes from the
		// parent snapshot, the hook may emit the same stable_key
		// for changed files that reference the same resource.
		// Dedup: first-wins (copy-forward wins), fail loudly on
		// identity mismatch (kind/subtype/name differ). The
		// `copied_resource_keys` set is empty for full index
		// (no copy-forward) so this loop is a no-op there.
		let mut deduped_nodes: Vec<ExtractedNode> = Vec::with_capacity(extras.nodes.len());
		for node in extras.nodes {
			if let Some(existing) = copied_resource_keys.get(node.stable_key.as_str()) {
				// Check identity match.
				let node_kind_str = serde_json::to_value(&node.kind)
					.ok()
					.and_then(|v| v.as_str().map(|s| s.to_string()))
					.unwrap_or_default();
				let node_subtype_str = node.subtype.as_ref().and_then(|st| {
					serde_json::to_value(st)
						.ok()
						.and_then(|v| v.as_str().map(|s| s.to_string()))
				});
				if node_kind_str != existing.kind
					|| node_subtype_str.as_deref() != existing.subtype.as_deref()
					|| node.name != existing.name
				{
					eprintln!(
						"[state-boundary] IDENTITY MISMATCH: stable_key {:?} \
						 carried forward as ({}, {:?}, {:?}) but hook emitted \
						 ({}, {:?}, {:?}). Skipping hook node; this is a bug.",
						node.stable_key,
						existing.kind, existing.subtype, existing.name,
						node_kind_str, node_subtype_str, node.name,
					);
				}
				// Skip: copy-forward wins.
			} else {
				deduped_nodes.push(node);
			}
		}
		if !deduped_nodes.is_empty() {
			nodes_total += deduped_nodes.len() as u64;
			all_nodes.extend(deduped_nodes);
		}

		for diag in &extras.diagnostics {
			eprintln!(
				"[state-boundary] {}: {} (file: {})",
				diag.code,
				diag.message,
				diag.file_path.as_deref().unwrap_or("-"),
			);
		}
	}

	// Persist phase 1 results.
	storage.upsert_files(&tracked_files)?;
	storage.upsert_file_versions(&file_versions)?;
	if !all_nodes.is_empty() {
		storage.insert_nodes(&all_nodes)?;
	}
	if !all_extraction_edges.is_empty() {
		storage.insert_extraction_edges(&all_extraction_edges)?;
	}
	if !all_signals.is_empty() {
		storage.insert_file_signals(&all_signals)?;
	}

	// ── Phase 2: Module nodes ────────────────────────────────
	// Use the FULL file set (copied + extracted) for module derivation.
	let all_tracked_for_modules: Vec<TrackedFile> = all_file_paths
		.iter()
		.map(|path| TrackedFile {
			file_uid: format!("{}:{}", repo_uid, path),
			repo_uid: repo_uid.into(),
			path: path.clone(),
			language: detect_language(path).map(|s| s.to_string()),
			is_test: is_test_file(path),
			is_generated: false,
			is_excluded: false,
		})
		.collect();
	let module_nodes = create_module_nodes(
		&all_tracked_for_modules,
		repo_uid,
		snap_uid,
	);
	let module_node_count = module_nodes.len() as u64;
	if !module_nodes.is_empty() {
		storage.insert_nodes(&module_nodes)?;
	}
	nodes_total += module_node_count;

	// ── Phase 3: Edge resolution ─────────────────────────────
	emit(crate::types::IndexPhase::Resolving, 0, 0, None);
	let resolver_nodes = storage.query_resolver_nodes(snap_uid)?;
	// Use the FULL file set for resolution context.
	let file_resolution_map = build_file_resolution_map(all_file_paths, repo_uid);

	let mut index = ResolverIndex {
		nodes_by_stable_key: HashMap::new(),
		nodes_by_name: HashMap::new(),
		node_uid_to_file_uid: HashMap::new(),
		file_resolution: file_resolution_map,
		per_file_include_resolution: HashMap::new(),
		stable_key_to_uid: HashMap::new(),
		file_to_module: HashMap::new(),
	};

	for node in &resolver_nodes {
		index
			.nodes_by_stable_key
			.insert(node.stable_key.clone(), node.clone());
		index
			.stable_key_to_uid
			.insert(node.stable_key.clone(), node.node_uid.clone());
		index
			.nodes_by_name
			.entry(node.name.clone())
			.or_default()
			.push(node.clone());
		if let Some(ref fuid) = node.file_uid {
			index
				.node_uid_to_file_uid
				.insert(node.node_uid.clone(), fuid.clone());
		}
		// Build file→module mapping for module-edge creation.
		if node.kind == "FILE" {
			if let Some(ref qn) = node.qualified_name {
				if let Some(mod_path) = get_module_path(qn) {
					let module_key = format!("{}:{}:MODULE", repo_uid, mod_path);
					index
						.file_to_module
						.insert(node.node_uid.clone(), module_key);
				}
			}
		}
	}

	// Batch resolution.
	let batch_size = edge_batch_size.unwrap_or(DEFAULT_EDGE_BATCH_SIZE);
	let mut resolved_total: u64 = 0;
	let mut unresolved_count: u64 = 0;
	let mut unresolved_breakdown: BTreeMap<String, u64> = BTreeMap::new();
	let mut all_resolved_import_pairs: Vec<(String, String)> = Vec::new();
	let mut cursor: Option<String> = None;
	let classification_observed_at = now_iso.clone();

	// Build import bindings by file for call resolution.
	let import_bindings_by_file = build_import_bindings_map(&all_signals);

	loop {
		let batch = storage.query_extraction_edges_batch(
			snap_uid,
			batch_size,
			cursor.as_deref(),
		)?;
		if batch.is_empty() {
			break;
		}
		cursor = Some(batch.last().unwrap().edge_uid.clone());

		// Convert ExtractionEdgeRows to ExtractedEdges for the resolver.
		let extracted_edges: Vec<crate::types::ExtractedEdge> = batch
			.iter()
			.map(|e| crate::types::ExtractedEdge {
				edge_uid: e.edge_uid.clone(),
				snapshot_uid: e.snapshot_uid.clone(),
				repo_uid: e.repo_uid.clone(),
				source_node_uid: e.source_node_uid.clone(),
				target_key: e.target_key.clone(),
				edge_type: e.edge_type,
				resolution: e.resolution,
				extractor: e.extractor.clone(),
				location: e.line_start.map(|ls| SourceLocation {
					line_start: ls,
					col_start: e.col_start.unwrap_or(0),
					line_end: e.line_end.unwrap_or(ls),
					col_end: e.col_end.unwrap_or(0),
				}),
				metadata_json: e.metadata_json.clone(),
			})
			.collect();

		let result = resolve_edges(
			&extracted_edges,
			&index,
			Some(&import_bindings_by_file),
		);

		// Persist resolved edges.
		if !result.resolved.is_empty() {
			storage.insert_resolved_edges(&result.resolved)?;
		}
		resolved_total += result.resolved.len() as u64;
		all_resolved_import_pairs.extend(result.resolved_import_pairs);

		// Classify and persist unresolved edges.
		if !result.still_unresolved.is_empty() {
			let mut persisted: Vec<PersistedUnresolvedEdge> = Vec::new();
			for ue in &result.still_unresolved {
				let category = ue.category;
				let cat_key = serde_json::to_value(&category)
					.ok()
					.and_then(|v| v.as_str().map(|s| s.to_string()))
					.unwrap_or_default();
				*unresolved_breakdown.entry(cat_key).or_insert(0) += 1;

				// Build file signals for classification.
				let file_signals = build_file_signals_for_edge(
					&ue.source_file_uid,
					&all_signals,
				);

				let verdict = classify_unresolved_edge(
					&repo_graph_classification::types::ClassifierEdgeInput {
						target_key: ue.edge.target_key.clone(),
						metadata_json: ue.edge.metadata_json.clone(),
					},
					category,
					&snapshot_signals,
					&file_signals,
				);

				persisted.push(PersistedUnresolvedEdge {
					edge_uid: ue.edge.edge_uid.clone(),
					snapshot_uid: ue.edge.snapshot_uid.clone(),
					repo_uid: ue.edge.repo_uid.clone(),
					source_node_uid: ue.edge.source_node_uid.clone(),
					target_key: ue.edge.target_key.clone(),
					edge_type: ue.edge.edge_type,
					resolution: ue.edge.resolution,
					extractor: ue.edge.extractor.clone(),
					line_start: ue.edge.location.map(|l| l.line_start),
					col_start: ue.edge.location.map(|l| l.col_start),
					line_end: ue.edge.location.map(|l| l.line_end),
					col_end: ue.edge.location.map(|l| l.col_end),
					metadata_json: ue.edge.metadata_json.clone(),
					category,
					classification: verdict.classification,
					classifier_version: CLASSIFIER_VERSION,
					basis_code: verdict.basis_code,
					observed_at: classification_observed_at.clone(),
				});
			}
			storage.insert_unresolved_edges(&persisted)?;
			unresolved_count += persisted.len() as u64;
		}
	}

	// ── Phase 4: Module edges ────────────────────────────────
	let module_edges = create_module_edges(
		&index,
		&all_resolved_import_pairs,
		snap_uid,
		repo_uid,
	);
	let module_edges_count = module_edges.len() as u64;
	if !module_edges.is_empty() {
		storage.insert_resolved_edges(&module_edges)?;
	}

	// ── Phase 5: Finalization ────────────────────────────────
	emit(crate::types::IndexPhase::Persisting, 0, 0, None);
	storage.update_snapshot_counts(snap_uid)?;

	let diagnostics = build_extraction_diagnostics(
		resolved_total + module_edges_count,
		unresolved_count,
		&unresolved_breakdown,
		skipped_oversized,
		files_read_failed,
	);
	storage.update_snapshot_extraction_diagnostics(
		snap_uid,
		&serde_json::to_string(&diagnostics).unwrap_or_default(),
	)?;

	storage.update_snapshot_status(&UpdateSnapshotStatusInput {
		snapshot_uid: snap_uid.to_string(),
		status: SnapshotStatus::Ready,
		completed_at: None,
	})?;

	let duration_ms = start.elapsed().as_millis() as u64;

	Ok(IndexResult {
		snapshot_uid: snap_uid.to_string(),
		files_total: all_file_paths.len() as u64,
		nodes_total,
		edges_total: resolved_total + module_edges_count,
		edges_unresolved: unresolved_count,
		unresolved_breakdown,
		duration_ms,
		orphaned_declarations: 0,
	})
}

// ── Module node creation ─────────────────────────────────────────

fn create_module_nodes(
	files: &[TrackedFile],
	repo_uid: &str,
	snapshot_uid: &str,
) -> Vec<ExtractedNode> {
	let mut dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
	for f in files {
		let mut path = f.path.as_str();
		while let Some(pos) = path.rfind('/') {
			let dir = &path[..pos];
			if !dirs.insert(dir.to_string()) {
				break; // Already seen this and all parents.
			}
			path = dir;
		}
	}

	dirs.into_iter()
		.map(|dir| {
			let name = dir.rsplit('/').next().unwrap_or(&dir);
			ExtractedNode {
				node_uid: uuid::Uuid::new_v4().to_string(),
				snapshot_uid: snapshot_uid.into(),
				repo_uid: repo_uid.into(),
				stable_key: format!("{}:{}:MODULE", repo_uid, dir),
				kind: NodeKind::Module,
				subtype: Some(NodeSubtype::Directory),
				name: name.into(),
				qualified_name: Some(dir.clone()),
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}
		})
		.collect()
}

// ── Module edge creation ─────────────────────────────────────────

fn create_module_edges(
	index: &ResolverIndex,
	resolved_import_pairs: &[(String, String)],
	snapshot_uid: &str,
	repo_uid: &str,
) -> Vec<crate::resolver::ResolvedEdge> {
	use crate::resolver::ResolvedEdge;

	let mut edges = Vec::new();

	// OWNS edges: MODULE → FILE.
	for (file_node_uid, module_key) in &index.file_to_module {
		if let Some(module_uid) = index.stable_key_to_uid.get(module_key) {
			edges.push(ResolvedEdge {
				edge_uid: uuid::Uuid::new_v4().to_string(),
				snapshot_uid: snapshot_uid.into(),
				repo_uid: repo_uid.into(),
				source_node_uid: module_uid.clone(),
				target_node_uid: file_node_uid.clone(),
				edge_type: EdgeType::Owns,
				resolution: Resolution::Static,
				extractor: INDEXER_VERSION.into(),
				location: None,
				metadata_json: None,
			});
		}
	}

	// MODULE→MODULE IMPORTS: derived from file-level IMPORTS.
	let mut seen_pairs: std::collections::HashSet<(String, String)> =
		std::collections::HashSet::new();
	for (src_uid, tgt_uid) in resolved_import_pairs {
		let src_mod = index.file_to_module.get(src_uid);
		let tgt_mod = index.file_to_module.get(tgt_uid);
		if let (Some(src_key), Some(tgt_key)) = (src_mod, tgt_mod) {
			if src_key == tgt_key {
				continue;
			}
			let pair = (src_key.clone(), tgt_key.clone());
			if !seen_pairs.insert(pair) {
				continue;
			}
			let src_mod_uid = index.stable_key_to_uid.get(src_key);
			let tgt_mod_uid = index.stable_key_to_uid.get(tgt_key);
			if let (Some(su), Some(tu)) = (src_mod_uid, tgt_mod_uid) {
				edges.push(ResolvedEdge {
					edge_uid: uuid::Uuid::new_v4().to_string(),
					snapshot_uid: snapshot_uid.into(),
					repo_uid: repo_uid.into(),
					source_node_uid: su.clone(),
					target_node_uid: tu.clone(),
					edge_type: EdgeType::Imports,
					resolution: Resolution::Static,
					extractor: INDEXER_VERSION.into(),
					location: None,
					metadata_json: None,
				});
			}
		}
	}

	edges
}

// ── Helper functions ─────────────────────────────────────────────

fn build_toolchain_json(extractors: &[&mut dyn ExtractorPort]) -> String {
	let names: Vec<String> = extractors.iter().map(|e| e.name().to_string()).collect();
	serde_json::json!({
		"extractors": names,
		"indexer": INDEXER_VERSION,
	})
	.to_string()
}

fn build_import_bindings_map(
	signals: &[FileSignalRow],
) -> HashMap<String, Vec<repo_graph_classification::types::ImportBinding>> {
	let mut map = HashMap::new();
	for s in signals {
		if let Some(ref json_str) = s.import_bindings_json {
			if let Ok(bindings) = serde_json::from_str(json_str) {
				map.insert(s.file_uid.clone(), bindings);
			}
		}
	}
	map
}

fn build_file_signals_for_edge(
	source_file_uid: &Option<String>,
	all_signals: &[FileSignalRow],
) -> FileSignals {
	let empty = FileSignals {
		import_bindings: vec![],
		same_file_value_symbols: vec![],
		same_file_class_symbols: vec![],
		same_file_interface_symbols: vec![],
		package_dependencies: PackageDependencySet { names: vec![] },
		tsconfig_aliases: TsconfigAliases { entries: vec![] },
	};

	let fuid = match source_file_uid {
		Some(f) => f,
		None => return empty,
	};

	let signal_row = all_signals.iter().find(|s| &s.file_uid == fuid);
	let signal_row = match signal_row {
		Some(s) => s,
		None => return empty,
	};

	let import_bindings = signal_row
		.import_bindings_json
		.as_ref()
		.and_then(|json| serde_json::from_str(json).ok())
		.unwrap_or_default();

	let package_dependencies = signal_row
		.package_dependencies_json
		.as_ref()
		.and_then(|json| serde_json::from_str(json).ok())
		.unwrap_or(PackageDependencySet { names: vec![] });

	let tsconfig_aliases = signal_row
		.tsconfig_aliases_json
		.as_ref()
		.and_then(|json| serde_json::from_str(json).ok())
		.unwrap_or(TsconfigAliases { entries: vec![] });

	FileSignals {
		import_bindings,
		same_file_value_symbols: vec![],
		same_file_class_symbols: vec![],
		same_file_interface_symbols: vec![],
		package_dependencies,
		tsconfig_aliases,
	}
}

fn build_extraction_diagnostics(
	edges_total: u64,
	unresolved_total: u64,
	unresolved_breakdown: &BTreeMap<String, u64>,
	skipped_oversized: u64,
	files_read_failed: u64,
) -> serde_json::Value {
	serde_json::json!({
		"diagnostics_version": 1,
		"edges_total": edges_total,
		"unresolved_total": unresolved_total,
		"unresolved_breakdown": unresolved_breakdown,
		"files_skipped_oversized": skipped_oversized,
		"files_read_failed": files_read_failed,
	})
}

// ── Refresh/delta indexing ────────────────────────────────────────

/// Run a delta/refresh index. Compares current files against
/// the parent snapshot's hashes, copies forward unchanged
/// artifacts, and re-extracts only changed/new/config-widened files.
///
/// Falls back to `index_repo` (full index) if:
///   - No parent snapshot exists
///   - Nothing would be copied forward (all files changed/new)
///
/// Mirror of `refreshRepo` from `repo-indexer.ts`.
pub fn refresh_repo<S: IndexerStoragePort>(
	storage: &mut S,
	extractors: &mut [&mut dyn ExtractorPort],
	repo_uid: &str,
	current_files: &[FileInput],
	options: &mut IndexOptions,
	hook: Option<&mut dyn crate::hook::ExtractionResultHook>,
) -> Result<IndexResult, IndexError<S::StorageError>> {
	// Check for a parent snapshot.
	let parent = storage
		.get_latest_snapshot(repo_uid)
		.map_err(IndexError::Storage)?;
	let parent = match parent {
		Some(p) if p.status == SnapshotStatus::Ready => p,
		_ => {
			// No ready parent → fall back to full index.
			return index_repo(storage, extractors, repo_uid, current_files, options, hook);
		}
	};

	// Build invalidation plan.
	let parent_hashes = storage
		.query_file_version_hashes(&parent.snapshot_uid)
		.map_err(IndexError::Storage)?;

	let current_states: Vec<crate::invalidation::CurrentFileState> = current_files
		.iter()
		.map(|f| crate::invalidation::CurrentFileState {
			file_uid: format!("{}:{}", repo_uid, f.rel_path),
			path: f.rel_path.clone(),
			content_hash: f.content_hash.clone(),
		})
		.collect();

	let plan = crate::invalidation::build_invalidation_plan(
		&parent.snapshot_uid,
		&parent_hashes,
		&current_states,
		repo_uid,
	);

	// If nothing to copy, fall back to full index.
	if plan.files_to_copy.is_empty() {
		return index_repo(storage, extractors, repo_uid, current_files, options, hook);
	}

	// Initialize extractors.
	for ext in extractors.iter_mut() {
		if let Err(e) = ext.initialize() {
			return Err(IndexError::ExtractorInit {
				extractor_name: ext.name().to_string(),
				source: e,
			});
		}
	}

	// Create REFRESH snapshot with parent link.
	let ext_refs: Vec<&dyn ExtractorPort> = extractors.iter().map(|e| &**e).collect();
	let routing_table = routing::build_extension_routing_table(&ext_refs);

	let mut all_identifiers = Vec::new();
	let mut all_module_specifiers = Vec::new();
	for ext in extractors.iter() {
		let builtins = ext.runtime_builtins();
		all_identifiers.extend(builtins.identifiers.iter().cloned());
		all_module_specifiers.extend(builtins.module_specifiers.iter().cloned());
	}
	let snapshot_signals = SnapshotSignals {
		runtime_builtins: RuntimeBuiltinsSet {
			identifiers: all_identifiers,
			module_specifiers: all_module_specifiers,
		},
	};

	let toolchain_json = build_toolchain_json(extractors);
	let snapshot = storage
		.create_snapshot(&CreateSnapshotInput {
			repo_uid: repo_uid.into(),
			kind: SnapshotKind::Refresh,
			basis_ref: None,
			basis_commit: options.basis_commit.clone(),
			parent_snapshot_uid: Some(parent.snapshot_uid.clone()),
			label: None,
			toolchain_json: Some(toolchain_json),
		})
		.map_err(IndexError::Storage)?;
	let snap_uid = snapshot.snapshot_uid.clone();
	let created_at = snapshot.created_at.clone();

	// Copy forward unchanged files.
	let copy_file_uids: Vec<String> = plan
		.files_to_copy
		.iter()
		.map(|p| format!("{}:{}", repo_uid, p))
		.collect();
	let copy_result = storage
		.copy_forward_unchanged_files(&crate::storage_port::CopyForwardInput {
			from_snapshot_uid: parent.snapshot_uid.clone(),
			to_snapshot_uid: snap_uid.clone(),
			repo_uid: repo_uid.into(),
			file_uids: copy_file_uids,
		})
		.map_err(IndexError::Storage)?;

	// Register copied files as tracked.
	let copied_tracked: Vec<TrackedFile> = plan
		.files_to_copy
		.iter()
		.map(|path| TrackedFile {
			file_uid: format!("{}:{}", repo_uid, path),
			repo_uid: repo_uid.into(),
			path: path.clone(),
			language: detect_language(path).map(|s| s.to_string()),
			is_test: is_test_file(path),
			is_generated: false,
			is_excluded: false,
		})
		.collect();
	storage.upsert_files(&copied_tracked).map_err(IndexError::Storage)?;

	// Filter current_files to only those needing extraction.
	let extract_set: std::collections::HashSet<&str> = plan
		.files_to_extract
		.iter()
		.map(|s| s.as_str())
		.collect();
	// Collect references into a temporary owned Vec<FileInput> is
	// not possible without cloning. Instead, pass the full set and
	// let run_pipeline handle all files — the routing table already
	// skips unsupported extensions, and the only difference is that
	// copied files get re-tracked (idempotent via upsert).
	// For correctness, filter the input slice to only extract-needed files.
	let files_to_extract: Vec<FileInput> = current_files
		.iter()
		.filter(|f| extract_set.contains(f.rel_path.as_str()))
		.map(|f| FileInput {
			rel_path: f.rel_path.clone(),
			content: f.content.clone(),
			content_hash: f.content_hash.clone(),
			size_bytes: f.size_bytes,
			line_count: f.line_count,
			package_dependencies: f.package_dependencies.clone(),
			tsconfig_aliases: f.tsconfig_aliases.clone(),
		})
		.collect();

	// Build the FULL file path set: copied + to-extract.
	// This is used for module-node creation and resolution context.
	let mut all_file_paths: Vec<String> = plan.files_to_copy.clone();
	all_file_paths.extend(plan.files_to_extract.iter().cloned());

	// Build resource-node dedup keys from copy-forward result
	// (SB-4-pre Fix B). Used by `run_pipeline` to prevent
	// inserting duplicate resource nodes that the hook may
	// re-emit for resources also referenced by unchanged files.
	let copied_resource_keys: HashMap<String, crate::storage_port::CopiedResourceNodeKey> =
		copy_result
			.copied_resource_node_keys
			.iter()
			.map(|k| (k.stable_key.clone(), k.clone()))
			.collect();

	let progress = &mut options.on_progress;
	let start = std::time::Instant::now();
	match run_pipeline(
		storage,
		extractors,
		repo_uid,
		&snap_uid,
		&files_to_extract,
		&all_file_paths,
		&snapshot_signals,
		&routing_table,
		&created_at,
		options.edge_batch_size,
		progress,
		start,
		hook,
		&copied_resource_keys,
	) {
		Ok(mut result) => {
			// Adjust node count to include copied nodes (which
			// run_pipeline didn't extract but are in the snapshot).
			result.nodes_total += copy_result.nodes_copied;
			Ok(result)
		}
		Err(storage_err) => {
			let _ = storage.update_snapshot_status(&UpdateSnapshotStatusInput {
				snapshot_uid: snap_uid,
				status: SnapshotStatus::Failed,
				completed_at: None,
			});
			Err(IndexError::Storage(storage_err))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::extractor_port::{ExtractorError, ExtractorPort};
	use crate::storage_port::*;
	use crate::types::*;

	// ── Mock storage ─────────────────────────────────────────

	/// Minimal mock storage that stores data in memory.
	#[derive(Default)]
	struct MockStorage {
		snapshots: Vec<Snapshot>,
		files: Vec<TrackedFile>,
		file_versions: Vec<FileVersion>,
		nodes: Vec<ExtractedNode>,
		extraction_edges: Vec<ExtractionEdgeRow>,
		resolved_edges: Vec<crate::resolver::ResolvedEdge>,
		unresolved_edges: Vec<PersistedUnresolvedEdge>,
		file_signals: Vec<FileSignalRow>,
		snap_counter: u32,
	}

	impl SnapshotLifecyclePort for MockStorage {
		type Error = String;
		fn create_snapshot(&mut self, input: &CreateSnapshotInput) -> Result<Snapshot, String> {
			self.snap_counter += 1;
			let snap = Snapshot {
				snapshot_uid: format!("snap_{}", self.snap_counter),
				repo_uid: input.repo_uid.clone(),
				parent_snapshot_uid: input.parent_snapshot_uid.clone(),
				kind: input.kind,
				basis_ref: input.basis_ref.clone(),
				basis_commit: input.basis_commit.clone(),
				dirty_hash: None,
				status: SnapshotStatus::Building,
				files_total: 0,
				nodes_total: 0,
				edges_total: 0,
				created_at: "2025-01-01T00:00:00.000Z".into(),
				completed_at: None,
				label: input.label.clone(),
				toolchain_json: input.toolchain_json.clone(),
			};
			self.snapshots.push(snap.clone());
			Ok(snap)
		}
		fn get_snapshot(&self, uid: &str) -> Result<Option<Snapshot>, String> {
			Ok(self.snapshots.iter().find(|s| s.snapshot_uid == uid).cloned())
		}
		fn get_latest_snapshot(&self, _: &str) -> Result<Option<Snapshot>, String> {
			Ok(self.snapshots.last().cloned())
		}
		fn update_snapshot_status(&mut self, input: &UpdateSnapshotStatusInput) -> Result<(), String> {
			if let Some(s) = self.snapshots.iter_mut().find(|s| s.snapshot_uid == input.snapshot_uid) {
				s.status = input.status;
			}
			Ok(())
		}
		fn update_snapshot_counts(&mut self, _: &str) -> Result<(), String> { Ok(()) }
		fn update_snapshot_extraction_diagnostics(&mut self, _: &str, _: &str) -> Result<(), String> { Ok(()) }
	}

	impl FileCatalogPort for MockStorage {
		type Error = String;
		fn upsert_files(&mut self, files: &[TrackedFile]) -> Result<(), String> {
			self.files.extend(files.iter().cloned());
			Ok(())
		}
		fn upsert_file_versions(&mut self, versions: &[FileVersion]) -> Result<(), String> {
			self.file_versions.extend(versions.iter().cloned());
			Ok(())
		}
		fn get_files_by_repo(&self, _: &str) -> Result<Vec<TrackedFile>, String> {
			Ok(self.files.clone())
		}
		fn get_stale_files(&self, _: &str) -> Result<Vec<TrackedFile>, String> { Ok(vec![]) }
		fn query_file_version_hashes(&self, snapshot_uid: &str) -> Result<BTreeMap<String, String>, String> {
			let map: BTreeMap<String, String> = self
				.file_versions
				.iter()
				.filter(|v| v.snapshot_uid == snapshot_uid)
				.map(|v| (v.file_uid.clone(), v.content_hash.clone()))
				.collect();
			Ok(map)
		}
	}

	impl NodeStorePort for MockStorage {
		type Error = String;
		fn insert_nodes(&mut self, nodes: &[ExtractedNode]) -> Result<(), String> {
			self.nodes.extend(nodes.iter().cloned());
			Ok(())
		}
		fn query_all_nodes(&self, _: &str) -> Result<Vec<ExtractedNode>, String> {
			Ok(self.nodes.clone())
		}
		fn query_resolver_nodes(&self, _: &str) -> Result<Vec<ResolverNode>, String> {
			Ok(self.nodes.iter().map(|n| ResolverNode {
				node_uid: n.node_uid.clone(),
				stable_key: n.stable_key.clone(),
				name: n.name.clone(),
				qualified_name: n.qualified_name.clone(),
				kind: serde_json::to_value(&n.kind)
					.ok()
					.and_then(|v| v.as_str().map(|s| s.to_string()))
					.unwrap_or_default(),
				subtype: n.subtype.as_ref().and_then(|st| {
					serde_json::to_value(st)
						.ok()
						.and_then(|v| v.as_str().map(|s| s.to_string()))
				}),
				file_uid: n.file_uid.clone(),
			}).collect())
		}
		fn delete_nodes_by_file(&mut self, _: &str, _: &str) -> Result<(), String> { Ok(()) }
	}

	impl EdgeStorePort for MockStorage {
		type Error = String;
		fn insert_resolved_edges(&mut self, edges: &[crate::resolver::ResolvedEdge]) -> Result<(), String> {
			self.resolved_edges.extend(edges.iter().cloned());
			Ok(())
		}
		fn insert_extraction_edges(&mut self, edges: &[ExtractionEdgeRow]) -> Result<(), String> {
			self.extraction_edges.extend(edges.iter().cloned());
			Ok(())
		}
		fn query_extraction_edges_batch(&self, _: &str, limit: usize, after: Option<&str>) -> Result<Vec<ExtractionEdgeRow>, String> {
			let mut filtered: Vec<&ExtractionEdgeRow> = match after {
				Some(cursor) => self.extraction_edges.iter().filter(|e| e.edge_uid.as_str() > cursor).collect(),
				None => self.extraction_edges.iter().collect(),
			};
			filtered.sort_by(|a, b| a.edge_uid.cmp(&b.edge_uid));
			Ok(filtered.into_iter().take(limit).cloned().collect())
		}
		fn delete_edges_by_uids(&mut self, _: &[String]) -> Result<(), String> { Ok(()) }
	}

	impl UnresolvedEdgePort for MockStorage {
		type Error = String;
		fn insert_unresolved_edges(&mut self, edges: &[PersistedUnresolvedEdge]) -> Result<(), String> {
			self.unresolved_edges.extend(edges.iter().cloned());
			Ok(())
		}
	}

	impl FileSignalPort for MockStorage {
		type Error = String;
		fn insert_file_signals(&mut self, signals: &[FileSignalRow]) -> Result<(), String> {
			self.file_signals.extend(signals.iter().cloned());
			Ok(())
		}
		fn query_file_signals_batch(&self, _: &str, _: &[String]) -> Result<Vec<FileSignalRow>, String> {
			Ok(self.file_signals.clone())
		}
	}

	impl crate::storage_port::DeltaCopyPort for MockStorage {
		type Error = String;
		fn copy_forward_unchanged_files(
			&mut self,
			_input: &crate::storage_port::CopyForwardInput,
		) -> Result<crate::storage_port::CopyForwardResult, String> {
			Ok(crate::storage_port::CopyForwardResult::default())
		}
	}

	// ── Mock extractor ───────────────────────────────────────

	struct MockExtractor {
		langs: Vec<String>,
		builtins: RuntimeBuiltinsSet,
	}

	impl MockExtractor {
		fn new(langs: Vec<String>) -> Self {
			Self {
				langs,
				builtins: RuntimeBuiltinsSet {
					identifiers: vec![],
					module_specifiers: vec![],
				},
			}
		}
	}

	impl ExtractorPort for MockExtractor {
		fn name(&self) -> &str { "mock:1.0.0" }
		fn languages(&self) -> &[String] { &self.langs }
		fn runtime_builtins(&self) -> &RuntimeBuiltinsSet { &self.builtins }
		fn initialize(&mut self) -> Result<(), ExtractorError> { Ok(()) }
		fn extract(
			&self,
			_source: &str,
			file_path: &str,
			file_uid: &str,
			repo_uid: &str,
			snapshot_uid: &str,
		) -> Result<ExtractionResult, ExtractorError> {
			// Emit a FILE node for every file.
			let file_node = ExtractedNode {
				node_uid: format!("{}_node", file_uid),
				snapshot_uid: snapshot_uid.into(),
				repo_uid: repo_uid.into(),
				stable_key: format!("{}:FILE", file_uid),
				kind: NodeKind::File,
				subtype: None,
				name: file_path.rsplit('/').next().unwrap_or(file_path).into(),
				qualified_name: Some(file_path.into()),
				file_uid: Some(file_uid.into()),
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			};
			Ok(ExtractionResult {
				nodes: vec![file_node],
				edges: vec![],
				metrics: BTreeMap::new(),
				import_bindings: vec![],
				resolved_callsites: vec![],
			})
		}
	}

	// ── Tests ────────────────────────────────────────────────

	#[test]
	fn index_repo_creates_snapshot_and_finalizes_to_ready() {
		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		let files = vec![FileInput {
			rel_path: "src/index.ts".into(),
			content: "export const x = 1;".into(),
			content_hash: "hash1".into(),
			size_bytes: 20,
			line_count: 1,
			package_dependencies: None,
			tsconfig_aliases: None,
		}];

		let result = index_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		assert_eq!(result.files_total, 1);
		assert!(result.nodes_total >= 1); // At least the FILE node
		assert_eq!(result.edges_unresolved, 0);
		// Snapshot should be READY.
		let snap = storage.snapshots.last().unwrap();
		assert_eq!(snap.status, SnapshotStatus::Ready);
	}

	#[test]
	fn index_repo_creates_module_nodes_for_directories() {
		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		let files = vec![
			FileInput {
				rel_path: "src/core/service.ts".into(),
				content: "".into(),
				content_hash: "h1".into(),
				size_bytes: 0,
				line_count: 0,
				package_dependencies: None,
				tsconfig_aliases: None,
			},
			FileInput {
				rel_path: "src/api/handler.ts".into(),
				content: "".into(),
				content_hash: "h2".into(),
				size_bytes: 0,
				line_count: 0,
				package_dependencies: None,
				tsconfig_aliases: None,
			},
		];

		let _result = index_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		let module_nodes: Vec<_> = storage
			.nodes
			.iter()
			.filter(|n| n.kind == NodeKind::Module)
			.collect();
		// Should have: src, src/core, src/api
		assert_eq!(module_nodes.len(), 3);
		let module_paths: Vec<&str> = module_nodes
			.iter()
			.filter_map(|n| n.qualified_name.as_deref())
			.collect();
		assert!(module_paths.contains(&"src"));
		assert!(module_paths.contains(&"src/core"));
		assert!(module_paths.contains(&"src/api"));
	}

	#[test]
	fn index_repo_skips_oversized_files() {
		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		let files = vec![FileInput {
			rel_path: "src/huge.ts".into(),
			content: "x".repeat(MAX_FILE_SIZE_BYTES + 1),
			content_hash: "big".into(),
			size_bytes: MAX_FILE_SIZE_BYTES + 1,
			line_count: 1,
			package_dependencies: None,
			tsconfig_aliases: None,
		}];

		let result = index_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		assert_eq!(result.files_total, 1);
		// No extraction nodes (file was skipped).
		let non_module_nodes: Vec<_> = storage
			.nodes
			.iter()
			.filter(|n| n.kind != NodeKind::Module)
			.collect();
		assert_eq!(non_module_nodes.len(), 0);
	}

	#[test]
	fn index_repo_skips_unsupported_extensions() {
		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		let files = vec![FileInput {
			rel_path: "README.md".into(),
			content: "# Hello".into(),
			content_hash: "md".into(),
			size_bytes: 7,
			line_count: 1,
			package_dependencies: None,
			tsconfig_aliases: None,
		}];

		let result = index_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		// File tracked but not extracted.
		assert_eq!(result.files_total, 1);
		let non_module_nodes: Vec<_> = storage
			.nodes
			.iter()
			.filter(|n| n.kind != NodeKind::Module)
			.collect();
		assert_eq!(non_module_nodes.len(), 0);
	}

	// ── Error handling ───────────────────────────────────────

	#[test]
	fn extractor_init_failure_returns_typed_error() {
		struct FailingExtractor {
			builtins: RuntimeBuiltinsSet,
		}
		impl FailingExtractor {
			fn new() -> Self {
				Self { builtins: RuntimeBuiltinsSet { identifiers: vec![], module_specifiers: vec![] } }
			}
		}
		impl ExtractorPort for FailingExtractor {
			fn name(&self) -> &str { "failing:1" }
			fn languages(&self) -> &[String] { &[] }
			fn runtime_builtins(&self) -> &RuntimeBuiltinsSet { &self.builtins }
			fn initialize(&mut self) -> Result<(), ExtractorError> {
				Err(ExtractorError { message: "grammar not found".into() })
			}
			fn extract(&self, _: &str, _: &str, _: &str, _: &str, _: &str) -> Result<ExtractionResult, ExtractorError> {
				unimplemented!()
			}
		}

		let mut storage = MockStorage::default();
		let mut ext = FailingExtractor::new();
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];
		let result = index_repo(&mut storage, &mut extractors, "r1", &[], &mut IndexOptions::default(), None);
		match result {
			Err(IndexError::ExtractorInit { extractor_name, .. }) => {
				assert_eq!(extractor_name, "failing:1");
			}
			other => panic!("expected ExtractorInit error, got {:?}", other),
		}
	}

	#[test]
	fn storage_failure_transitions_snapshot_to_failed() {
		/// Mock that fails on insert_nodes (simulating a mid-pipeline storage error).
		#[derive(Default)]
		struct FailOnInsertNodes {
			inner: MockStorage,
		}
		impl SnapshotLifecyclePort for FailOnInsertNodes {
			type Error = String;
			fn create_snapshot(&mut self, i: &CreateSnapshotInput) -> Result<Snapshot, String> { self.inner.create_snapshot(i) }
			fn get_snapshot(&self, uid: &str) -> Result<Option<Snapshot>, String> { self.inner.get_snapshot(uid) }
			fn get_latest_snapshot(&self, r: &str) -> Result<Option<Snapshot>, String> { self.inner.get_latest_snapshot(r) }
			fn update_snapshot_status(&mut self, i: &UpdateSnapshotStatusInput) -> Result<(), String> { self.inner.update_snapshot_status(i) }
			fn update_snapshot_counts(&mut self, s: &str) -> Result<(), String> { self.inner.update_snapshot_counts(s) }
			fn update_snapshot_extraction_diagnostics(&mut self, s: &str, d: &str) -> Result<(), String> { self.inner.update_snapshot_extraction_diagnostics(s, d) }
		}
		impl FileCatalogPort for FailOnInsertNodes {
			type Error = String;
			fn upsert_files(&mut self, f: &[TrackedFile]) -> Result<(), String> { self.inner.upsert_files(f) }
			fn upsert_file_versions(&mut self, v: &[FileVersion]) -> Result<(), String> { self.inner.upsert_file_versions(v) }
			fn get_files_by_repo(&self, r: &str) -> Result<Vec<TrackedFile>, String> { self.inner.get_files_by_repo(r) }
			fn get_stale_files(&self, s: &str) -> Result<Vec<TrackedFile>, String> { self.inner.get_stale_files(s) }
			fn query_file_version_hashes(&self, s: &str) -> Result<BTreeMap<String, String>, String> { self.inner.query_file_version_hashes(s) }
		}
		impl NodeStorePort for FailOnInsertNodes {
			type Error = String;
			fn insert_nodes(&mut self, _: &[ExtractedNode]) -> Result<(), String> {
				Err("simulated node insert failure".into())
			}
			fn query_all_nodes(&self, s: &str) -> Result<Vec<ExtractedNode>, String> { self.inner.query_all_nodes(s) }
			fn query_resolver_nodes(&self, s: &str) -> Result<Vec<crate::resolver::ResolverNode>, String> { self.inner.query_resolver_nodes(s) }
			fn delete_nodes_by_file(&mut self, s: &str, f: &str) -> Result<(), String> { self.inner.delete_nodes_by_file(s, f) }
		}
		impl EdgeStorePort for FailOnInsertNodes {
			type Error = String;
			fn insert_resolved_edges(&mut self, e: &[crate::resolver::ResolvedEdge]) -> Result<(), String> { self.inner.insert_resolved_edges(e) }
			fn insert_extraction_edges(&mut self, e: &[ExtractionEdgeRow]) -> Result<(), String> { self.inner.insert_extraction_edges(e) }
			fn query_extraction_edges_batch(&self, s: &str, l: usize, a: Option<&str>) -> Result<Vec<ExtractionEdgeRow>, String> { self.inner.query_extraction_edges_batch(s, l, a) }
			fn delete_edges_by_uids(&mut self, u: &[String]) -> Result<(), String> { self.inner.delete_edges_by_uids(u) }
		}
		impl UnresolvedEdgePort for FailOnInsertNodes {
			type Error = String;
			fn insert_unresolved_edges(&mut self, e: &[PersistedUnresolvedEdge]) -> Result<(), String> { self.inner.insert_unresolved_edges(e) }
		}
		impl FileSignalPort for FailOnInsertNodes {
			type Error = String;
			fn insert_file_signals(&mut self, s: &[FileSignalRow]) -> Result<(), String> { self.inner.insert_file_signals(s) }
			fn query_file_signals_batch(&self, s: &str, f: &[String]) -> Result<Vec<FileSignalRow>, String> { self.inner.query_file_signals_batch(s, f) }
		}
		impl crate::storage_port::DeltaCopyPort for FailOnInsertNodes {
			type Error = String;
			fn copy_forward_unchanged_files(&mut self, _: &crate::storage_port::CopyForwardInput) -> Result<crate::storage_port::CopyForwardResult, String> {
				Ok(crate::storage_port::CopyForwardResult::default())
			}
		}

		let mut storage = FailOnInsertNodes::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];
		let files = vec![FileInput {
			rel_path: "src/a.ts".into(),
			content: "export const x = 1;".into(),
			content_hash: "h".into(),
			size_bytes: 20,
			line_count: 1,
			package_dependencies: None,
			tsconfig_aliases: None,
		}];

		let result = index_repo(&mut storage, &mut extractors, "r1", &files, &mut IndexOptions::default(), None);
		assert!(matches!(result, Err(IndexError::Storage(_))));

		// Snapshot must have been transitioned to FAILED.
		let snap = storage.inner.snapshots.last().unwrap();
		assert_eq!(snap.status, SnapshotStatus::Failed);
	}

	// ── Progress events ──────────────────────────────────────

	#[test]
	fn progress_callback_receives_phase_events() {
		use std::sync::{Arc, Mutex};

		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		let events: Arc<Mutex<Vec<crate::types::IndexPhase>>> = Arc::new(Mutex::new(vec![]));
		let events_clone = events.clone();

		let files = vec![FileInput {
			rel_path: "src/index.ts".into(),
			content: "export const x = 1;".into(),
			content_hash: "hash1".into(),
			size_bytes: 20,
			line_count: 1,
			package_dependencies: None,
			tsconfig_aliases: None,
		}];

		let mut opts = IndexOptions {
			on_progress: Some(Box::new(move |evt| {
				events_clone.lock().unwrap().push(evt.phase);
			})),
			..IndexOptions::default()
		};

		let _result = index_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files,
			&mut opts,
			None,
		)
		.unwrap();

		let phases = events.lock().unwrap();
		// Must see at least: Extracting (initial + per-file), Resolving, Persisting.
		assert!(
			phases.contains(&crate::types::IndexPhase::Extracting),
			"expected Extracting phase, got {:?}",
			*phases
		);
		assert!(
			phases.contains(&crate::types::IndexPhase::Resolving),
			"expected Resolving phase, got {:?}",
			*phases
		);
		assert!(
			phases.contains(&crate::types::IndexPhase::Persisting),
			"expected Persisting phase, got {:?}",
			*phases
		);
	}

	// ── refresh_repo tests ───────────────────────────────────

	#[test]
	fn refresh_falls_back_to_full_index_when_no_parent() {
		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		let files = vec![FileInput {
			rel_path: "src/a.ts".into(),
			content: "const a = 1;".into(),
			content_hash: "h1".into(),
			size_bytes: 12,
			line_count: 1,
			package_dependencies: None,
			tsconfig_aliases: None,
		}];

		// No prior snapshot → refresh should fall back to full.
		let result = refresh_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		assert_eq!(result.files_total, 1);
		// Should have created a FULL snapshot (fallback).
		let snap = storage.snapshots.last().unwrap();
		assert_eq!(snap.kind, SnapshotKind::Full);
		assert_eq!(snap.status, SnapshotStatus::Ready);
	}

	#[test]
	fn refresh_creates_refresh_snapshot_with_copy_forward() {
		let mut storage = MockStorage::default();
		let mut ext = MockExtractor::new(vec!["typescript".into()]);
		let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ext];

		// First: full index to create a parent.
		let files_v1 = vec![
			FileInput {
				rel_path: "src/a.ts".into(),
				content: "const a = 1;".into(),
				content_hash: "h_a".into(),
				size_bytes: 12,
				line_count: 1,
				package_dependencies: None,
				tsconfig_aliases: None,
			},
			FileInput {
				rel_path: "src/b.ts".into(),
				content: "const b = 2;".into(),
				content_hash: "h_b".into(),
				size_bytes: 12,
				line_count: 1,
				package_dependencies: None,
				tsconfig_aliases: None,
			},
		];
		let full_result = index_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files_v1,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		let parent_snap_uid = full_result.snapshot_uid.clone();
		assert_eq!(
			storage.snapshots.last().unwrap().status,
			SnapshotStatus::Ready
		);

		// Now: refresh. a.ts unchanged (same hash), b.ts changed.
		let files_v2 = vec![
			FileInput {
				rel_path: "src/a.ts".into(),
				content: "const a = 1;".into(),
				content_hash: "h_a".into(), // Same → unchanged → copy
				size_bytes: 12,
				line_count: 1,
				package_dependencies: None,
				tsconfig_aliases: None,
			},
			FileInput {
				rel_path: "src/b.ts".into(),
				content: "const b = 999;".into(),
				content_hash: "h_b_new".into(), // Changed → re-extract
				size_bytes: 14,
				line_count: 1,
				package_dependencies: None,
				tsconfig_aliases: None,
			},
		];

		let refresh_result = refresh_repo(
			&mut storage,
			&mut extractors,
			"r1",
			&files_v2,
			&mut IndexOptions::default(),
			None,
		)
		.unwrap();

		// Should have 2 snapshots total.
		assert_eq!(storage.snapshots.len(), 2);

		// Second snapshot should be REFRESH linked to parent.
		let refresh_snap = storage.snapshots.last().unwrap();
		assert_eq!(refresh_snap.kind, SnapshotKind::Refresh);
		assert_eq!(refresh_snap.status, SnapshotStatus::Ready);
		assert_eq!(
			refresh_snap.parent_snapshot_uid,
			Some(parent_snap_uid)
		);

		// files_total should include both (copied a.ts + extracted b.ts).
		assert_eq!(refresh_result.files_total, 2);
	}
}
