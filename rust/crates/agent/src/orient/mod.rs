//! Orient use case — public entry point.
//!
//! `orient()` is the only function callers should ever use. It
//! dispatches on the `focus` argument:
//!
//!   - `None` → repo-level pipeline (`orient_repo`).
//!   - `Some(path)` that resolves to an exact FILE node →
//!     file pipeline (`orient_file`).
//!   - `Some(path)` that resolves to a path area (directory
//!     with content, optionally a MODULE node) → path pipeline
//!     (`orient_path`).
//!   - `Some(stable_key)` that resolves to a MODULE node →
//!     path pipeline.
//!   - `Some(stable_key)` that resolves to a FILE node →
//!     file pipeline.
//!   - `Some(stable_key)` that resolves to a SYMBOL node →
//!     symbol pipeline (`orient_symbol`).
//!   - `Some(name)` that resolves to a single SYMBOL via name
//!     lookup → symbol pipeline.
//!   - `Some(name)` that resolves to multiple SYMBOLs →
//!     ambiguous result with candidates.
//!   - `Some(string)` that resolves to nothing → valid
//!     `OrientResult` with `Focus::no_match`, zero signals,
//!     zero limits.

pub mod file;
pub mod path;
pub mod repo;
pub mod symbol;

use repo_graph_gate::GateStorageRead;

use crate::dto::budget::Budget;
use crate::dto::envelope::{
	Focus, FocusCandidate, OrientResult, ResolvedKind,
	ORIENT_COMMAND, ORIENT_SCHEMA,
};
use crate::dto::envelope::Confidence;
use crate::errors::OrientError;
use crate::storage_port::{
	AgentFocusCandidate, AgentFocusKind, AgentStorageRead,
};

/// Entry point for the orient use case.
///
/// Generic over a single storage handle that satisfies both
/// `AgentStorageRead` (the agent-orient port) and
/// `GateStorageRead` (the gate policy port). One concrete
/// adapter — `repo_graph_storage::StorageConnection` in
/// production, or a test fake that implements both traits —
/// fulfills both bounds, so the call site passes one value.
///
/// `now` is an ISO 8601 timestamp used for waiver expiry
/// evaluation. The agent crate is deliberately clock-free:
/// callers must supply `now` explicitly. Production callers
/// (CLI, daemon) read the system clock at their own boundary;
/// tests pass a fixed string so their outcomes are
/// deterministic. Passing a far-future or far-past sentinel
/// silently distorts waiver semantics — do not.
pub fn orient<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	focus: Option<&str>,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, OrientError> {
	match focus {
		None => repo::orient_repo(storage, repo_uid, budget, now),
		Some(focus_str) => orient_focused(storage, repo_uid, focus_str, budget, now),
	}
}

/// Focus resolution and dispatch.
///
/// Resolves the focus string against the latest READY snapshot
/// and routes to the appropriate pipeline. The repo and snapshot
/// are resolved here (shared with the pipelines) so that each
/// pipeline does not repeat the lookup.
fn orient_focused<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	focus_str: &str,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, OrientError> {
	// ── 1. Resolve repo identity. ────────────────────────────
	let repo = storage
		.get_repo(repo_uid)?
		.ok_or_else(|| OrientError::NoRepo { repo_uid: repo_uid.to_string() })?;

	// ── 2. Resolve snapshot. ─────────────────────────────────
	let snapshot = storage
		.get_latest_snapshot(repo_uid)?
		.ok_or_else(|| OrientError::NoSnapshot {
			repo_uid: repo_uid.to_string(),
		})?;

	let snapshot_uid = &snapshot.snapshot_uid;

	// ── 3. Try path-based resolution first. ──────────────────
	let resolution = storage.resolve_path_focus(snapshot_uid, focus_str)?;

	if resolution.has_exact_file {
		return file::orient_file(
			storage,
			&repo.name,
			&snapshot,
			focus_str,
			resolution.file_stable_key.as_deref(),
			budget,
			now,
		);
	}

	// Path-area focus fires when EITHER:
	//   - files exist under the prefix (subtree has content), OR
	//   - an exact MODULE node exists at this path (even if the
	//     module currently has no descendant FILE nodes — the
	//     module is a declared structural unit and the contract
	//     says "exact path match -> FILE or MODULE").
	//
	// Without the `module_stable_key` check, a MODULE node with
	// zero descendant files would fall through to stable-key
	// resolution and then to no_match, which violates the
	// documented precedence rule.
	if resolution.has_content_under_prefix
		|| resolution.module_stable_key.is_some()
	{
		return path::orient_path(
			storage,
			&repo.name,
			&snapshot,
			focus_str,
			resolution.module_stable_key.as_deref(),
			budget,
			now,
		);
	}

	// ── 4. Try stable-key resolution. ────────────────────────
	match storage.resolve_stable_key_focus(snapshot_uid, focus_str)? {
		Some(candidate) if candidate.kind == AgentFocusKind::Symbol => {
			// SYMBOL by stable key — route to symbol pipeline.
			let context =
				storage.get_symbol_context(snapshot_uid, &candidate.stable_key)?;
			match context {
				Some(ctx) => symbol::orient_symbol(
					storage,
					&repo.name,
					&snapshot,
					&candidate.stable_key,
					&ctx,
					focus_str,
					budget,
					now,
				),
				None => {
					// Stable key resolved but no context found —
					// defensive: treat as no match.
					Ok(build_no_match_result(
						&repo.name,
						&snapshot,
						focus_str,
						budget,
					))
				}
			}
		}
		Some(candidate) if candidate.kind == AgentFocusKind::File => {
			let file_path = candidate.file.as_deref().unwrap_or(focus_str);
			file::orient_file(
				storage,
				&repo.name,
				&snapshot,
				file_path,
				Some(&candidate.stable_key),
				budget,
				now,
			)
		}
		Some(candidate) => {
			// MODULE by stable key — extract the path from the
			// candidate or from the stable key itself.
			let path = extract_path_from_candidate(&candidate, focus_str);
			path::orient_path(
				storage,
				&repo.name,
				&snapshot,
				&path,
				Some(&candidate.stable_key),
				budget,
				now,
			)
		}
		None => {
			// ── 5. Try symbol name resolution. ──────────────
			let symbol_candidates =
				storage.resolve_symbol_name(snapshot_uid, focus_str)?;
			match symbol_candidates.len() {
				0 => {
					// No match — valid response with no data.
					Ok(build_no_match_result(
						&repo.name,
						&snapshot,
						focus_str,
						budget,
					))
				}
				1 => {
					let candidate = &symbol_candidates[0];
					let context = storage.get_symbol_context(
						snapshot_uid,
						&candidate.stable_key,
					)?;
					match context {
						Some(ctx) => symbol::orient_symbol(
							storage,
							&repo.name,
							&snapshot,
							&candidate.stable_key,
							&ctx,
							focus_str,
							budget,
							now,
						),
						None => Ok(build_no_match_result(
							&repo.name,
							&snapshot,
							focus_str,
							budget,
						)),
					}
				}
				_ => {
					// Ambiguous — return candidates, no signals.
					Ok(build_ambiguous_result(
						&repo.name,
						&snapshot,
						focus_str,
						symbol_candidates,
						budget,
					))
				}
			}
		}
	}
}

/// Extract a path from a MODULE focus candidate.
///
/// Tries the candidate's file field first (when a MODULE node
/// has a file association, the file path is the module root).
/// Falls back to parsing the stable key pattern
/// `{repo}:{path}:MODULE` to extract the path portion.
/// Last resort: uses the raw focus string.
fn extract_path_from_candidate(
	candidate: &crate::storage_port::AgentFocusCandidate,
	focus_str: &str,
) -> String {
	if let Some(ref f) = candidate.file {
		return f.clone();
	}
	// Try stable key pattern: {repo}:{path}:MODULE
	let key = &candidate.stable_key;
	if let Some(stripped) = key.strip_suffix(":MODULE") {
		if let Some(colon) = stripped.find(':') {
			return stripped[colon + 1..].to_string();
		}
	}
	focus_str.to_string()
}

/// Build an `OrientResult` for the ambiguous case.
///
/// Multiple SYMBOL candidates matched the focus string. Return
/// the candidates so the caller can disambiguate.
fn build_ambiguous_result(
	repo_name: &str,
	snapshot: &crate::storage_port::AgentSnapshot,
	focus_str: &str,
	candidates: Vec<AgentFocusCandidate>,
	_budget: Budget,
) -> OrientResult {
	let focus_candidates: Vec<FocusCandidate> = candidates
		.into_iter()
		.map(|c| FocusCandidate {
			stable_key: c.stable_key,
			file: c.file,
			kind: ResolvedKind::Symbol,
		})
		.collect();

	OrientResult {
		schema: ORIENT_SCHEMA,
		command: ORIENT_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot.snapshot_uid.clone(),
		focus: Focus::ambiguous(focus_str, focus_candidates),
		confidence: Confidence::High,

		documentation: None,

		signals: Vec::new(),
		signals_truncated: None,
		signals_omitted_count: None,

		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,

		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,

		truncated: false,
	}
}

/// Build an `OrientResult` for the no-match case.
///
/// This is NOT an error — it is a valid response with
/// `Focus::no_match`, zero signals, zero limits, and
/// `truncated: false`.
fn build_no_match_result(
	repo_name: &str,
	snapshot: &crate::storage_port::AgentSnapshot,
	focus_str: &str,
	_budget: Budget,
) -> OrientResult {
	OrientResult {
		schema: ORIENT_SCHEMA,
		command: ORIENT_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot.snapshot_uid.clone(),
		focus: Focus::no_match(focus_str),
		confidence: Confidence::High,

		documentation: None,

		signals: Vec::new(),
		signals_truncated: None,
		signals_omitted_count: None,

		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,

		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,

		truncated: false,
	}
}
