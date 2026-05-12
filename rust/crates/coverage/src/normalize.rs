//! Path normalization for coverage reports.
//!
//! RS-MS-4-prereq-a: Repo-root-relative path normalization.
//!
//! Contract (locked):
//! 1. Normalize separators to `/`
//! 2. Normalize the repo root path to `/` as well
//! 3. If a coverage entry path is absolute and is under the repo root, strip the repo root prefix
//! 4. Trim any leading `/` after stripping
//! 5. Match the resulting repo-relative path exactly against indexed file paths
//! 6. No suffix matching
//! 7. No basename matching
//! 8. No "closest path" heuristics
//!
//! This is canonicalization, not fuzzy matching.

/// Normalize a path from a coverage report to repo-relative form.
///
/// # Arguments
/// * `report_path` - The path as it appears in the coverage report (may be absolute)
/// * `repo_root` - The absolute path to the repository root
///
/// # Returns
/// * `Some(normalized)` - The repo-relative path if normalization succeeded
/// * `None` - If the path could not be normalized (e.g., absolute path not under repo root)
///
/// # Normalization rules
/// 1. Normalize all separators to `/`
/// 2. If `report_path` is absolute and starts with `repo_root`, strip the prefix
/// 3. Trim leading `/` after stripping
/// 4. If the path is already relative, return it as-is (with normalized separators)
/// 5. If the path is absolute but NOT under repo_root, return `None`
pub fn normalize_to_repo_relative(report_path: &str, repo_root: &str) -> Option<String> {
    // Step 1: Normalize separators to `/`
    let normalized_report = report_path.replace('\\', "/");
    let normalized_root = repo_root.replace('\\', "/");

    // Step 2: Determine if the path is absolute
    let is_absolute = normalized_report.starts_with('/')
        || (normalized_report.len() >= 2
            && normalized_report.chars().nth(1) == Some(':')
            && normalized_report
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false));

    if !is_absolute {
        // Already relative - return with normalized separators, trimming leading ./
        let trimmed = normalized_report.trim_start_matches("./");

        // Reject paths that escape the repo (start with ../)
        if trimmed.starts_with("../") || trimmed == ".." {
            return None;
        }

        return Some(trimmed.to_string());
    }

    // Step 3: Check if the absolute path is under the repo root
    // Normalize the repo root to not have a trailing slash for consistent comparison
    let root_prefix = normalized_root.trim_end_matches('/');

    if let Some(suffix) = normalized_report.strip_prefix(root_prefix) {
        // The path starts with the repo root prefix, but we need to verify it's
        // actually under the repo root (not just a prefix match like /repo vs /repo-other)

        // The suffix must either be empty (path IS the repo root) or start with /
        if suffix.is_empty() {
            // The path WAS the repo root itself - unusual but handle it
            return None;
        }

        if !suffix.starts_with('/') {
            // Partial prefix match: /repo matched but path is /repo-other
            return None;
        }

        // Step 4: Trim leading `/` after stripping
        let relative = suffix.trim_start_matches('/');

        if relative.is_empty() {
            // Path was repo root with trailing slash
            None
        } else {
            Some(relative.to_string())
        }
    } else {
        // Absolute path not under repo root - cannot normalize
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_relative_path() {
        let result = normalize_to_repo_relative("src/main.ts", "/home/user/repo");
        assert_eq!(result, Some("src/main.ts".to_string()));
    }

    #[test]
    fn relative_with_dot_prefix() {
        let result = normalize_to_repo_relative("./src/main.ts", "/home/user/repo");
        assert_eq!(result, Some("src/main.ts".to_string()));
    }

    #[test]
    fn absolute_under_repo_root() {
        let result = normalize_to_repo_relative("/home/user/repo/src/main.ts", "/home/user/repo");
        assert_eq!(result, Some("src/main.ts".to_string()));
    }

    #[test]
    fn absolute_under_repo_root_with_trailing_slash() {
        let result = normalize_to_repo_relative("/home/user/repo/src/main.ts", "/home/user/repo/");
        assert_eq!(result, Some("src/main.ts".to_string()));
    }

    #[test]
    fn absolute_not_under_repo_root() {
        let result = normalize_to_repo_relative("/other/path/src/main.ts", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn absolute_partial_prefix_match_rejected() {
        // /home/user/repo-other should NOT match /home/user/repo
        let result =
            normalize_to_repo_relative("/home/user/repo-other/src/main.ts", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn windows_path_normalized() {
        let result = normalize_to_repo_relative(
            "C:\\Users\\dev\\repo\\src\\main.ts",
            "C:\\Users\\dev\\repo",
        );
        assert_eq!(result, Some("src/main.ts".to_string()));
    }

    #[test]
    fn windows_relative_path() {
        let result = normalize_to_repo_relative("src\\lib\\utils.ts", "C:\\repo");
        assert_eq!(result, Some("src/lib/utils.ts".to_string()));
    }

    #[test]
    fn mixed_separators() {
        let result =
            normalize_to_repo_relative("/home/user/repo/src\\lib/main.ts", "/home/user/repo");
        assert_eq!(result, Some("src/lib/main.ts".to_string()));
    }

    #[test]
    fn repo_root_path_itself_returns_none() {
        let result = normalize_to_repo_relative("/home/user/repo", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn repo_root_with_trailing_slash_returns_none() {
        let result = normalize_to_repo_relative("/home/user/repo/", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn deeply_nested_path() {
        let result = normalize_to_repo_relative(
            "/home/user/repo/src/adapters/storage/sqlite/sqlite-storage.ts",
            "/home/user/repo",
        );
        assert_eq!(
            result,
            Some("src/adapters/storage/sqlite/sqlite-storage.ts".to_string())
        );
    }

    #[test]
    fn path_with_spaces() {
        let result = normalize_to_repo_relative(
            "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/src/main.ts",
            "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph",
        );
        assert_eq!(result, Some("src/main.ts".to_string()));
    }

    #[test]
    fn relative_path_escaping_repo_rejected() {
        // ../other/file.ts escapes the repo - should be rejected
        let result = normalize_to_repo_relative("../other/file.ts", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn relative_path_double_escape_rejected() {
        let result = normalize_to_repo_relative("../../outside.ts", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn relative_path_dot_dot_only_rejected() {
        let result = normalize_to_repo_relative("..", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn relative_path_with_dot_prefix_escaping_rejected() {
        // ./../other/file.ts after trimming ./ becomes ../other/file.ts
        let result = normalize_to_repo_relative("./../other/file.ts", "/home/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn relative_path_with_embedded_dotdot_allowed() {
        // src/../lib/main.ts is still within repo (resolves to lib/main.ts)
        // We allow this because it's still repo-relative, just with internal traversal
        let result = normalize_to_repo_relative("src/../lib/main.ts", "/home/user/repo");
        assert_eq!(result, Some("src/../lib/main.ts".to_string()));
    }
}
