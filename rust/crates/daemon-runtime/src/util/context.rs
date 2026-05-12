//! Storage and repository context helpers.

use std::path::Path;

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
