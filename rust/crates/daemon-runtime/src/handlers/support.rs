//! Shared support for daemon handlers.
//!
//! Contains common utilities used across handler families.
//! Handler-family-specific utilities live in their own support modules.

use std::path::Path;
use std::sync::Arc;

use repo_graph_daemon_transport::{ErrorCode, ErrorDetail};
use serde_json::Value;

use crate::state::{DaemonState, RepoState};

/// Get an optional string parameter from request params.
pub fn get_optional_string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

/// Resolve repo from params and load it.
///
/// Returns (RepoState, repo_uid) on success, ErrorDetail on failure.
pub fn resolve_and_load_repo(
    state: &DaemonState,
    params: &Value,
) -> Result<(Arc<RepoState>, String), ErrorDetail> {
    let repo_ref = params
        .get("repo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorDetail::invalid_request("missing or invalid 'repo' parameter"))?;

    // Resolve via registry (alias or path)
    let entry = state.resolve_alias_or_path(repo_ref).ok_or_else(|| {
        ErrorDetail::new(
            ErrorCode::RepoNotFound,
            format!(
                "repo not indexed: {} (run: rmap index {})",
                repo_ref, repo_ref
            ),
        )
    })?;

    let db_path = Path::new(&entry.db_path);
    let repo_uid = &entry.repo_uid;

    // Auto-load if not already loaded
    let repo_state = state
        .load_repo(db_path, repo_uid)
        .map_err(|e| ErrorDetail::new(ErrorCode::InternalError, e))?;

    Ok((repo_state, repo_uid.clone()))
}

/// Sibling of [`resolve_and_load_repo`] for the handlers that read the working tree (churn,
/// hotspots, risk, coverage, map): the same resolution and errors, plus the registry entry's
/// `canonical_path`, checked (STATE-ROOT-RELATIVE-REPO-ROOT-1). [`resolve_and_load_repo`] and its
/// callers that never touch a checkout are unchanged.
pub(crate) fn resolve_and_load_repo_with_root(
    state: &DaemonState,
    params: &Value,
) -> Result<(Arc<RepoState>, String, crate::repo_root::CheckedRepoRoot), ErrorDetail> {
    let resolved = crate::repo_root::resolve_and_load_repo_with_root(state, params)?;
    Ok((resolved.repo_state, resolved.repo_uid, resolved.root))
}
