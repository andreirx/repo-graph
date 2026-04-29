//! Storage and repository context helpers.

use std::path::Path;

/// Open a storage connection from a database file path.
///
/// Returns an error if the file does not exist or cannot be opened.
pub fn open_storage(
    db_path: &Path,
) -> Result<repo_graph_storage::StorageConnection, String> {
    if !db_path.exists() {
        return Err(format!(
            "database file does not exist: {}",
            db_path.display()
        ));
    }
    repo_graph_storage::StorageConnection::open(db_path)
        .map_err(|e| format!("failed to open database: {}", e))
}

/// Resolve a repo reference to a `repo_uid`.
///
/// Resolution order: exact UID -> exact name -> exact root_path.
/// Returns `Ok(repo_uid)` on success, `Err(message)` if not found or
/// storage error.
pub fn resolve_repo_ref(
    storage: &repo_graph_storage::StorageConnection,
    repo_ref: &str,
) -> Result<String, String> {
    use repo_graph_storage::types::RepoRef;

    // Try UID first.
    if let Ok(Some(repo)) = storage.get_repo(&RepoRef::Uid(repo_ref.to_string())) {
        return Ok(repo.repo_uid);
    }

    // Try name.
    if let Ok(Some(repo)) = storage.get_repo(&RepoRef::Name(repo_ref.to_string())) {
        return Ok(repo.repo_uid);
    }

    // Try root_path.
    if let Ok(Some(repo)) = storage.get_repo(&RepoRef::RootPath(repo_ref.to_string())) {
        return Ok(repo.repo_uid);
    }

    Err(format!("repo not found: {}", repo_ref))
}
