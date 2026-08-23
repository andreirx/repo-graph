//! Storage and repository context helpers.

use std::path::{Path, PathBuf};

/// Compute the path to store in `repos.root_path` for a given repo and DB location.
///
/// The stored path is relative to the DB file's directory. This ensures that:
/// - DB remains portable (relative paths, not absolute)
/// - Resolution is cwd-independent (always relative to DB location)
/// - DB and repo can be moved together and paths still work
///
/// # Arguments
/// * `repo_path` - The repo path as provided by the user (may be relative or absolute)
/// * `db_path` - Path to the database file
///
/// # Returns
/// A path string suitable for storing in `repos.root_path`, relative to the DB directory.
///
/// # Errors
/// Returns error if paths cannot be canonicalized or if relative path computation fails.
pub fn compute_storage_root_path(repo_path: &Path, db_path: &Path) -> Result<String, String> {
    // Canonicalize both paths to absolute
    let repo_abs = repo_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve repo path '{}': {}", repo_path.display(), e))?;

    // Get DB directory (parent of db file, or current dir if db_path has no parent)
    let db_dir = if let Some(parent) = db_path.parent() {
        if parent.as_os_str().is_empty() {
            // db_path is just a filename like "repo.db", use current directory
            std::env::current_dir().map_err(|e| format!("cannot get current directory: {}", e))?
        } else {
            parent
                .canonicalize()
                .map_err(|e| format!("cannot resolve DB directory '{}': {}", parent.display(), e))?
        }
    } else {
        std::env::current_dir().map_err(|e| format!("cannot get current directory: {}", e))?
    };

    // Compute relative path from db_dir to repo_abs
    let relative = pathdiff::diff_paths(&repo_abs, &db_dir).ok_or_else(|| {
        format!(
            "cannot compute relative path from '{}' to '{}'",
            db_dir.display(),
            repo_abs.display()
        )
    })?;

    // pathdiff returns empty string when paths are identical; use "." instead
    let relative_str = relative.to_string_lossy();
    if relative_str.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(relative_str.into_owned())
    }
}

/// Resolve the repo root path relative to the database file location.
///
/// The `repos.root_path` is stored relative to the DB location (e.g., "."
/// or "../other-repo"). This function resolves it to an absolute path
/// using the DB file's parent directory as the base.
///
/// This ensures filesystem-backed surfaces (docs, churn, coverage, etc.)
/// work correctly regardless of the caller's working directory.
///
/// # Arguments
/// * `db_path` - Path to the database file
/// * `root_path` - The `root_path` value from the `repos` table
///
/// # Returns
/// Canonicalized absolute path to the repo root, or error if resolution fails.
pub fn resolve_repo_root(db_path: &Path, root_path: &str) -> Result<PathBuf, String> {
    // Get the directory containing the DB file
    let db_dir = db_path
        .parent()
        .ok_or_else(|| format!("cannot determine parent directory of {}", db_path.display()))?;

    // Resolve root_path relative to db_dir
    let resolved = db_dir.join(root_path);

    // Canonicalize to get absolute path and verify it exists
    resolved.canonicalize().map_err(|e| {
        format!(
            "failed to resolve repo root '{}' relative to '{}': {}",
            root_path,
            db_dir.display(),
            e
        )
    })
}

/// Open a storage connection from a database file path.
///
/// Returns an error if the file does not exist or cannot be opened.
pub fn open_storage(db_path: &Path) -> Result<repo_graph_storage::StorageConnection, String> {
    // NO-CREATE (FORGET-REPO-1): a client-side read must never create the DB. Only a true
    // NotFound gets the friendly missing-db message; any other metadata fault (EACCES,
    // ENOTDIR, ...) must NOT be collapsed into "does not exist" — fall through to
    // `open_existing`, which reports the real I/O/SQLite fault and closes the
    // check→open TOCTOU so even a race cannot resurrect a just-removed DB.
    match std::fs::metadata(db_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "database file does not exist: {}",
                db_path.display()
            ));
        }
        Ok(_) | Err(_) => {}
    }
    repo_graph_storage::StorageConnection::open_existing(db_path)
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

#[cfg(test)]
mod tests {
    use super::*;

    // review-11 #1: a NON-NotFound metadata fault (here ENOTDIR — a db path routed THROUGH a
    // regular file) must NOT be collapsed into the friendly "database file does not exist" claim;
    // it falls through to `open_existing`, which reports the real fault.
    #[test]
    fn open_storage_non_notfound_fault_is_not_reported_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plain-file");
        std::fs::write(&file, b"x").unwrap();
        let bogus = file.join("some.db"); // path THROUGH a regular file → ENOTDIR, not NotFound
        let err = open_storage(&bogus).expect_err("must fail");
        assert!(
            !err.contains("does not exist"),
            "ENOTDIR must not be rendered as a missing database: {err}"
        );
        assert!(
            err.contains("failed to open database"),
            "real fault surfaces: {err}"
        );
    }

    // The honest common case is preserved: a genuinely-missing file gets the friendly message.
    #[test]
    fn open_storage_missing_file_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let err = open_storage(&dir.path().join("nope.db")).expect_err("must fail");
        assert!(err.contains("does not exist"), "{err}");
    }
}
