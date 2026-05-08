//! TypeScript project ownership resolution.
//!
//! Determines which tsconfig.json (or jsconfig.json) owns a given file
//! based on the actual config semantics: `files`, `include`, `exclude`,
//! `extends`, and `references`.
//!
//! # Problem
//!
//! The naive approach of "find nearest config by directory ancestry" fails
//! in monorepos where:
//! - Files are grouped by `references` across project boundaries
//! - A file's physical location differs from its config ownership
//! - Multiple configs have overlapping `include` patterns
//!
//! # Solution
//!
//! Parse all configs in the repo, evaluate their include/exclude patterns,
//! and determine ownership deterministically:
//! - Exactly one match → `Owned`
//! - Multiple matches → `Ambiguous` (explicit failure, not silent guess)
//! - No matches → `Unowned` (explicit failure)
//!
//! # Maturity
//!
//! This module is PROTOTYPE maturity. It supports:
//! - Config discovery (tsconfig*.json, jsconfig.json)
//! - `extends` chain resolution
//! - `files` / `include` / `exclude` evaluation
//! - `references` awareness
//! - Deterministic ownership with explicit failure modes
//!
//! Known limitations:
//! - Glob matching is simplified (uses Rust `glob` crate, not TS semantics)
//! - Does not handle all edge cases in `extends` (e.g., node_modules resolution)
//! - Does not evaluate `compilerOptions.rootDir` / `outDir` interactions

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Public Types
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome of project ownership resolution for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectOwnership {
    /// File is owned by exactly one project.
    Owned {
        /// Absolute path to the owning tsconfig/jsconfig.
        config_path: PathBuf,
        /// Project root (directory containing the config).
        project_root: PathBuf,
    },
    /// File matches multiple projects with no clear winner.
    Ambiguous {
        /// All configs that claim ownership (sorted for determinism).
        candidates: Vec<PathBuf>,
    },
    /// File is not covered by any discovered config.
    Unowned,
}

/// Error during ownership resolution.
#[derive(Debug, Clone)]
pub enum OwnershipError {
    /// Failed to read or parse a config file.
    ConfigParseError { path: PathBuf, reason: String },
    /// Circular `extends` chain detected.
    CircularExtends { chain: Vec<PathBuf> },
    /// IO error during discovery.
    IoError { reason: String },
}

impl std::fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigParseError { path, reason } => {
                write!(f, "failed to parse {}: {}", path.display(), reason)
            }
            Self::CircularExtends { chain } => {
                let paths: Vec<_> = chain.iter().map(|p| p.display().to_string()).collect();
                write!(f, "circular extends chain: {}", paths.join(" -> "))
            }
            Self::IoError { reason } => write!(f, "IO error: {}", reason),
        }
    }
}

impl std::error::Error for OwnershipError {}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver
// ─────────────────────────────────────────────────────────────────────────────

/// Resolver that determines file ownership based on tsconfig semantics.
///
/// Build once per repo, then query multiple files.
pub struct TsProjectOwnershipResolver {
    /// All discovered and parsed configs, keyed by absolute path.
    configs: HashMap<PathBuf, ParsedConfig>,
    /// Repo root (for relative path resolution).
    repo_root: PathBuf,
}

impl TsProjectOwnershipResolver {
    /// Build resolver by discovering and parsing all configs in the repo.
    ///
    /// Errors are collected but do not fail the build — configs that fail
    /// to parse are skipped with a warning. This allows partial resolution
    /// even if some configs are malformed.
    pub fn build(repo_root: &Path) -> Result<Self, OwnershipError> {
        let repo_root = repo_root
            .canonicalize()
            .map_err(|e| OwnershipError::IoError {
                reason: format!("failed to canonicalize repo root: {}", e),
            })?;

        // Discover all config files
        let config_paths = discover_configs(&repo_root)?;

        debug!(
            repo_root = %repo_root.display(),
            config_count = config_paths.len(),
            "discovered TS config files"
        );

        // Parse each config (with extends resolution)
        let mut configs = HashMap::new();
        for path in config_paths {
            match parse_config_with_extends(&path, &repo_root) {
                Ok(config) => {
                    configs.insert(path, config);
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "skipping malformed config");
                }
            }
        }

        Ok(Self { configs, repo_root })
    }

    /// Resolve ownership for a file.
    ///
    /// `file_path` should be relative to repo root or absolute.
    pub fn resolve(&self, file_path: &Path) -> ProjectOwnership {
        // Normalize to absolute path
        let abs_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            self.repo_root.join(file_path)
        };

        // Canonicalize for consistent matching
        let abs_path = match abs_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // File doesn't exist — can't resolve ownership
                debug!(path = %file_path.display(), "file does not exist");
                return ProjectOwnership::Unowned;
            }
        };

        // Find all configs that own this file
        let mut owners: Vec<&PathBuf> = Vec::new();

        for (config_path, config) in &self.configs {
            if config.owns_file(&abs_path, &self.repo_root) {
                owners.push(config_path);
            }
        }

        match owners.len() {
            0 => ProjectOwnership::Unowned,
            1 => {
                let config_path = owners[0].clone();
                let project_root = config_path.parent().unwrap_or(&self.repo_root).to_path_buf();
                ProjectOwnership::Owned {
                    config_path,
                    project_root,
                }
            }
            _ => {
                // Multiple owners — try tiebreaker (most specific directory)
                if let Some(winner) = tiebreak_by_specificity(&abs_path, &owners) {
                    let project_root = winner.parent().unwrap_or(&self.repo_root).to_path_buf();
                    ProjectOwnership::Owned {
                        config_path: winner.clone(),
                        project_root,
                    }
                } else {
                    // Genuine ambiguity
                    let mut candidates: Vec<PathBuf> =
                        owners.into_iter().cloned().collect();
                    candidates.sort(); // Deterministic order
                    ProjectOwnership::Ambiguous { candidates }
                }
            }
        }
    }

    /// Get all discovered config paths (for diagnostics).
    pub fn config_paths(&self) -> Vec<&PathBuf> {
        self.configs.keys().collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Config Discovery
// ─────────────────────────────────────────────────────────────────────────────

/// Discover all tsconfig*.json and jsconfig.json files in the repo.
///
/// Excludes node_modules and common build output directories.
fn discover_configs(repo_root: &Path) -> Result<Vec<PathBuf>, OwnershipError> {
    let mut configs = Vec::new();

    // Walk directory tree
    discover_configs_recursive(repo_root, repo_root, &mut configs)?;

    // Sort for deterministic order
    configs.sort();

    Ok(configs)
}

fn discover_configs_recursive(
    dir: &Path,
    repo_root: &Path,
    configs: &mut Vec<PathBuf>,
) -> Result<(), OwnershipError> {
    // Skip excluded directories
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if should_skip_directory(dir_name) {
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| OwnershipError::IoError {
        reason: format!("failed to read {}: {}", dir.display(), e),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            discover_configs_recursive(&path, repo_root, configs)?;
        } else if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_ts_config_file(name) {
                    configs.push(path);
                }
            }
        }
    }

    Ok(())
}

fn should_skip_directory(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "bower_components"
            | "jspm_packages"
            | ".git"
            | "dist"
            | "build"
            | "out"
            | "target"
            | ".next"
            | ".nuxt"
    )
}

fn is_ts_config_file(name: &str) -> bool {
    // tsconfig.json, tsconfig.*.json, jsconfig.json, jsconfig.*.json
    name == "tsconfig.json"
        || name == "jsconfig.json"
        || (name.starts_with("tsconfig.") && name.ends_with(".json"))
        || (name.starts_with("jsconfig.") && name.ends_with(".json"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Config Parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Raw tsconfig.json structure (for serde).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawTsConfig {
    extends: Option<String>,
    files: Option<Vec<String>>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    references: Option<Vec<RawReference>>,
    compiler_options: Option<RawCompilerOptions>,
}

#[derive(Debug, Deserialize)]
struct RawReference {
    path: String,
}

/// Compiler options we care about for ownership resolution.
///
/// `out_dir` and `root_dir` are parsed for potential future use in
/// exclude default computation but not currently used.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct RawCompilerOptions {
    out_dir: Option<String>,
    root_dir: Option<String>,
    composite: Option<bool>,
}

/// Parsed and resolved config.
#[derive(Debug)]
struct ParsedConfig {
    /// Absolute paths to explicit files (if `files` is specified).
    files: Option<Vec<PathBuf>>,
    /// Include patterns (as specified, for glob matching).
    include: Vec<String>,
    /// Exclude patterns (as specified, for glob matching).
    exclude: Vec<String>,
    /// Referenced project configs (resolved absolute paths).
    references: Vec<PathBuf>,
    /// Whether this is a composite/solution config.
    is_composite: bool,
    /// Config directory (for pattern base).
    config_dir: PathBuf,
    /// Whether this config explicitly defines its file scope.
    ///
    /// True if `files` or `include` was explicitly set in this config
    /// (not just inherited defaults). Base configs that only set
    /// `exclude` or `compilerOptions` have this false and should not
    /// be considered as file owners.
    has_explicit_file_scope: bool,
}

impl ParsedConfig {
    /// Check if this config owns the given file.
    fn owns_file(&self, abs_file_path: &Path, repo_root: &Path) -> bool {
        // Configs without explicit file scope (base configs) don't own files
        if !self.has_explicit_file_scope {
            return false;
        }

        // Solution-style configs (references but no own code) don't own files directly
        if self.is_solution_style() {
            return false;
        }

        // If `files` is specified, only those files are included
        if let Some(ref files) = self.files {
            return files.iter().any(|f| f == abs_file_path);
        }

        // Otherwise, evaluate include/exclude patterns
        let relative_path = match abs_file_path.strip_prefix(repo_root) {
            Ok(rel) => rel,
            Err(_) => return false,
        };

        // File must match at least one include pattern
        let matches_include = self.include.is_empty()
            || self
                .include
                .iter()
                .any(|pattern| matches_glob_pattern(pattern, relative_path, &self.config_dir, repo_root));

        if !matches_include {
            return false;
        }

        // File must not match any exclude pattern
        let matches_exclude = self.exclude.iter().any(|pattern| {
            matches_glob_pattern(pattern, relative_path, &self.config_dir, repo_root)
        });

        !matches_exclude
    }

    /// Check if this is a solution-style config (has references, no own code).
    fn is_solution_style(&self) -> bool {
        !self.references.is_empty()
            && self.files.is_none()
            && self.include.is_empty()
    }
}

/// Parse a config file with extends chain resolution.
fn parse_config_with_extends(
    config_path: &Path,
    repo_root: &Path,
) -> Result<ParsedConfig, OwnershipError> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    parse_config_recursive(config_path, repo_root, &mut visited)
}

fn parse_config_recursive(
    config_path: &Path,
    repo_root: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<ParsedConfig, OwnershipError> {
    let abs_path = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        repo_root.join(config_path)
    };

    let abs_path = abs_path.canonicalize().map_err(|e| OwnershipError::ConfigParseError {
        path: config_path.to_path_buf(),
        reason: format!("failed to canonicalize: {}", e),
    })?;

    // Cycle detection
    if visited.contains(&abs_path) {
        return Err(OwnershipError::CircularExtends {
            chain: visited.iter().cloned().collect(),
        });
    }
    visited.insert(abs_path.clone());

    // Read and parse JSON
    let content = fs::read_to_string(&abs_path).map_err(|e| OwnershipError::ConfigParseError {
        path: abs_path.clone(),
        reason: format!("failed to read: {}", e),
    })?;

    // Strip comments (tsconfig allows them)
    let content = strip_json_comments(&content);

    let raw: RawTsConfig =
        serde_json::from_str(&content).map_err(|e| OwnershipError::ConfigParseError {
            path: abs_path.clone(),
            reason: format!("invalid JSON: {}", e),
        })?;

    let config_dir = abs_path.parent().unwrap_or(repo_root).to_path_buf();

    // Handle extends (parent config)
    let mut base = if let Some(ref extends_path) = raw.extends {
        let base_path = resolve_extends_path(extends_path, &config_dir)?;
        parse_config_recursive(&base_path, repo_root, visited)?
    } else {
        // Default config
        ParsedConfig {
            files: None,
            include: default_include(),
            exclude: default_exclude(),
            references: Vec::new(),
            is_composite: false,
            config_dir: config_dir.clone(),
            has_explicit_file_scope: false,
        }
    };

    // Track whether this config explicitly defines file scope
    let has_explicit_scope = raw.files.is_some() || raw.include.is_some();

    // Merge this config over base (child overrides parent)
    if raw.files.is_some() {
        base.files = raw.files.map(|files| {
            files
                .into_iter()
                .map(|f| config_dir.join(&f))
                .filter_map(|p| p.canonicalize().ok())
                .collect()
        });
    }

    if raw.include.is_some() {
        base.include = raw.include.unwrap_or_default();
    }

    if raw.exclude.is_some() {
        base.exclude = raw.exclude.unwrap_or_default();
    }

    // Mark as having explicit scope if this config or base has it
    if has_explicit_scope {
        base.has_explicit_file_scope = true;
    }

    if let Some(refs) = raw.references {
        base.references = refs
            .into_iter()
            .filter_map(|r| {
                let ref_path = config_dir.join(&r.path);
                // Reference path points to directory or config file
                let config_path = if ref_path.is_dir() {
                    ref_path.join("tsconfig.json")
                } else {
                    ref_path
                };
                config_path.canonicalize().ok()
            })
            .collect();
    }

    if let Some(ref opts) = raw.compiler_options {
        if opts.composite == Some(true) {
            base.is_composite = true;
        }
    }

    base.config_dir = config_dir;

    Ok(base)
}

/// Resolve an `extends` path to an absolute config path.
fn resolve_extends_path(extends: &str, config_dir: &Path) -> Result<PathBuf, OwnershipError> {
    // Relative path
    if extends.starts_with('.') {
        let path = config_dir.join(extends);
        // Add .json if not present
        let path = if path.extension().is_none() {
            path.with_extension("json")
        } else {
            path
        };
        return Ok(path);
    }

    // TODO: Handle node_modules resolution for package extends
    // For now, treat as relative if not starting with '.'
    let path = config_dir.join(extends);
    let path = if path.extension().is_none() {
        path.with_extension("json")
    } else {
        path
    };
    Ok(path)
}

fn default_include() -> Vec<String> {
    // Default when no include specified: all TS/JS files
    vec!["**/*".to_string()]
}

fn default_exclude() -> Vec<String> {
    // Default excludes
    vec![
        "node_modules".to_string(),
        "bower_components".to_string(),
        "jspm_packages".to_string(),
    ]
}

/// Strip single-line and multi-line comments from JSON.
///
/// tsconfig.json allows comments, but serde_json doesn't parse them.
fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(c) = chars.next() {
        if escape_next {
            result.push(c);
            escape_next = false;
            continue;
        }

        if c == '\\' && in_string {
            result.push(c);
            escape_next = true;
            continue;
        }

        if c == '"' {
            in_string = !in_string;
            result.push(c);
            continue;
        }

        if in_string {
            result.push(c);
            continue;
        }

        if c == '/' {
            if let Some(&next) = chars.peek() {
                if next == '/' {
                    // Single-line comment: skip until newline
                    chars.next(); // consume second '/'
                    while let Some(&nc) = chars.peek() {
                        if nc == '\n' {
                            break;
                        }
                        chars.next();
                    }
                    continue;
                } else if next == '*' {
                    // Multi-line comment: skip until */
                    chars.next(); // consume '*'
                    while let Some(nc) = chars.next() {
                        if nc == '*' {
                            if let Some(&'/') = chars.peek() {
                                chars.next(); // consume '/'
                                break;
                            }
                        }
                    }
                    continue;
                }
            }
        }

        result.push(c);
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Glob Matching
// ─────────────────────────────────────────────────────────────────────────────

/// Check if a file matches a glob pattern.
///
/// Patterns are relative to config_dir. TypeScript glob semantics:
/// - `*` matches any characters except `/`
/// - `**` matches any characters including `/` (directory recursion)
/// - `?` matches single character except `/`
/// - Directory patterns (no wildcards) match the directory and all contents
///
/// This implementation uses simplified matching. Full TS semantics would
/// require more complex handling of pattern normalization.
fn matches_glob_pattern(
    pattern: &str,
    file_rel_path: &Path,
    config_dir: &Path,
    repo_root: &Path,
) -> bool {
    // Get config dir relative to repo root
    let config_rel = config_dir
        .strip_prefix(repo_root)
        .unwrap_or(Path::new(""));

    // Pattern is relative to config dir
    let full_pattern = if config_rel.as_os_str().is_empty() {
        pattern.to_string()
    } else {
        format!("{}/{}", config_rel.display(), pattern)
    };

    // Convert to glob pattern string
    let file_str = file_rel_path.to_string_lossy();

    // TypeScript semantics: a pattern without wildcards matches the directory
    // and all its contents. E.g., "dist" matches "dist/main.ts".
    if !full_pattern.contains('*') && !full_pattern.contains('?') {
        // Directory pattern: check if file is under this directory
        if file_str.starts_with(&full_pattern) {
            let rest = &file_str[full_pattern.len()..];
            // Must be exact match or followed by /
            return rest.is_empty() || rest.starts_with('/');
        }
        return false;
    }

    // Simple glob matching
    glob_match(&full_pattern, &file_str)
}

/// Simple glob matching.
///
/// Supports: `*`, `**`, `?`
fn glob_match(pattern: &str, path: &str) -> bool {
    // Handle ** specially
    if pattern.contains("**") {
        return glob_match_recursive(pattern, path);
    }

    // Simple pattern without **
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    if pattern_parts.len() != path_parts.len() {
        return false;
    }

    pattern_parts
        .iter()
        .zip(path_parts.iter())
        .all(|(p, s)| segment_match(p, s))
}

fn glob_match_recursive(pattern: &str, path: &str) -> bool {
    // Split pattern at first **
    if let Some(pos) = pattern.find("**") {
        let prefix = &pattern[..pos];
        let suffix = &pattern[pos + 2..];

        // Remove leading/trailing slashes from suffix
        let suffix = suffix.trim_start_matches('/');

        // Prefix must match start of path
        let prefix = prefix.trim_end_matches('/');
        if !prefix.is_empty() && !path.starts_with(prefix) {
            return false;
        }

        // If no suffix, ** matches everything
        if suffix.is_empty() {
            return true;
        }

        // Try matching suffix at every position
        let start = if prefix.is_empty() {
            0
        } else {
            prefix.len() + 1
        };
        let remaining = &path[start.min(path.len())..];

        // ** can match zero or more path segments
        for i in 0..=remaining.len() {
            let check_path = &remaining[i..];
            if glob_match(suffix, check_path) {
                return true;
            }
            // Only advance to next '/' boundary or end
            if i < remaining.len() && !remaining[i..].starts_with('/') {
                if remaining[i..].find('/').is_some() {
                    continue;
                }
            }
        }

        // Also check if suffix matches from start
        glob_match(suffix, remaining)
    } else {
        glob_match(pattern, path)
    }
}

/// Match a single path segment against a pattern segment.
fn segment_match(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let mut pattern_chars = pattern.chars().peekable();
    let mut segment_chars = segment.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // * matches any remaining characters in segment
                if pattern_chars.peek().is_none() {
                    return true;
                }
                // Try matching rest of pattern at each position
                let rest_pattern: String = pattern_chars.collect();
                loop {
                    let rest_segment: String = segment_chars.clone().collect();
                    if segment_match(&rest_pattern, &rest_segment) {
                        return true;
                    }
                    if segment_chars.next().is_none() {
                        break;
                    }
                }
                return false;
            }
            '?' => {
                // ? matches exactly one character
                if segment_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Literal match
                if segment_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    // Both must be exhausted
    segment_chars.next().is_none()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tiebreaking
// ─────────────────────────────────────────────────────────────────────────────

/// Tiebreak multiple owners by specificity (deepest directory wins).
///
/// Returns Some if there's a clear winner, None if still ambiguous.
fn tiebreak_by_specificity<'a>(file_path: &Path, owners: &[&'a PathBuf]) -> Option<&'a PathBuf> {
    if owners.is_empty() {
        return None;
    }

    // Find the owner whose config_dir is closest ancestor of the file
    let file_dir = file_path.parent()?;

    let mut best: Option<&PathBuf> = None;
    let mut best_depth = 0;

    for owner in owners {
        if let Some(config_dir) = owner.parent() {
            if file_dir.starts_with(config_dir) {
                let depth = config_dir.components().count();
                if depth > best_depth {
                    best = Some(owner);
                    best_depth = depth;
                } else if depth == best_depth && best.is_some() {
                    // Tie at same depth — ambiguous
                    return None;
                }
            }
        }
    }

    best
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(root: &Path, rel_path: &str, content: &str) {
        let path = root.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_strip_json_comments_single_line() {
        let input = r#"{
            // This is a comment
            "name": "test"
        }"#;
        let result = strip_json_comments(input);
        assert!(!result.contains("//"));
        assert!(result.contains("\"name\""));
    }

    #[test]
    fn test_strip_json_comments_multi_line() {
        let input = r#"{
            /* This is a
               multi-line comment */
            "name": "test"
        }"#;
        let result = strip_json_comments(input);
        assert!(!result.contains("/*"));
        assert!(!result.contains("*/"));
        assert!(result.contains("\"name\""));
    }

    #[test]
    fn test_strip_json_comments_in_string() {
        let input = r#"{"url": "http://example.com"}"#;
        let result = strip_json_comments(input);
        assert_eq!(input, result);
    }

    #[test]
    fn test_segment_match_literal() {
        assert!(segment_match("foo", "foo"));
        assert!(!segment_match("foo", "bar"));
    }

    #[test]
    fn test_segment_match_star() {
        assert!(segment_match("*", "anything"));
        assert!(segment_match("foo*", "foobar"));
        assert!(segment_match("*bar", "foobar"));
        assert!(segment_match("*.ts", "main.ts"));
        assert!(!segment_match("*.ts", "main.js"));
    }

    #[test]
    fn test_segment_match_question() {
        assert!(segment_match("f?o", "foo"));
        assert!(!segment_match("f?o", "fooo"));
    }

    #[test]
    fn test_glob_match_simple() {
        assert!(glob_match("src/main.ts", "src/main.ts"));
        assert!(!glob_match("src/main.ts", "src/other.ts"));
    }

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("src/*.ts", "src/main.ts"));
        assert!(!glob_match("src/*.ts", "src/sub/main.ts"));
    }

    #[test]
    fn test_glob_match_doublestar() {
        assert!(glob_match("**/*.ts", "main.ts"));
        assert!(glob_match("**/*.ts", "src/main.ts"));
        assert!(glob_match("**/*.ts", "src/deep/main.ts"));
        assert!(glob_match("src/**/*.ts", "src/main.ts"));
        assert!(glob_match("src/**/*.ts", "src/deep/main.ts"));
    }

    #[test]
    fn test_discover_skips_node_modules() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        create_file(root, "tsconfig.json", "{}");
        create_file(root, "node_modules/pkg/tsconfig.json", "{}");
        create_file(root, "src/sub/tsconfig.json", "{}");

        let configs = discover_configs(root).unwrap();
        assert_eq!(configs.len(), 2);
        assert!(configs.iter().any(|p| p.ends_with("tsconfig.json")));
        assert!(configs.iter().any(|p| p.ends_with("src/sub/tsconfig.json")));
        assert!(!configs.iter().any(|p| p.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn test_single_config_owns_all_ts_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        create_file(root, "tsconfig.json", r#"{"include": ["src/**/*.ts"]}"#);
        create_file(root, "src/main.ts", "");
        create_file(root, "src/lib/util.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        let ownership = resolver.resolve(Path::new("src/main.ts"));
        assert!(matches!(ownership, ProjectOwnership::Owned { .. }));

        let ownership = resolver.resolve(Path::new("src/lib/util.ts"));
        assert!(matches!(ownership, ProjectOwnership::Owned { .. }));
    }

    #[test]
    fn test_file_outside_include_is_unowned() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        create_file(root, "tsconfig.json", r#"{"include": ["src/**/*.ts"]}"#);
        create_file(root, "test/main.test.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        let ownership = resolver.resolve(Path::new("test/main.test.ts"));
        assert!(matches!(ownership, ProjectOwnership::Unowned));
    }

    #[test]
    fn test_excluded_file_is_unowned() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        create_file(
            root,
            "tsconfig.json",
            r#"{"include": ["**/*.ts"], "exclude": ["dist/**"]}"#,
        );
        create_file(root, "src/main.ts", "");
        create_file(root, "dist/main.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        let ownership = resolver.resolve(Path::new("src/main.ts"));
        assert!(matches!(ownership, ProjectOwnership::Owned { .. }));

        let ownership = resolver.resolve(Path::new("dist/main.ts"));
        assert!(matches!(ownership, ProjectOwnership::Unowned));
    }

    #[test]
    fn test_multiple_projects_deterministic() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        create_file(
            root,
            "packages/app/tsconfig.json",
            r#"{"include": ["src/**/*.ts"]}"#,
        );
        create_file(
            root,
            "packages/lib/tsconfig.json",
            r#"{"include": ["src/**/*.ts"]}"#,
        );
        create_file(root, "packages/app/src/main.ts", "");
        create_file(root, "packages/lib/src/util.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        let app_ownership = resolver.resolve(Path::new("packages/app/src/main.ts"));
        if let ProjectOwnership::Owned { config_path, .. } = app_ownership {
            assert!(config_path.to_string_lossy().contains("packages/app"));
        } else {
            panic!("expected Owned for app file");
        }

        let lib_ownership = resolver.resolve(Path::new("packages/lib/src/util.ts"));
        if let ProjectOwnership::Owned { config_path, .. } = lib_ownership {
            assert!(config_path.to_string_lossy().contains("packages/lib"));
        } else {
            panic!("expected Owned for lib file");
        }
    }

    #[test]
    fn test_extends_merges_config() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        create_file(
            root,
            "tsconfig.base.json",
            r#"{"exclude": ["node_modules", "dist"]}"#,
        );
        create_file(
            root,
            "tsconfig.json",
            r#"{"extends": "./tsconfig.base.json", "include": ["src/**/*.ts"]}"#,
        );
        create_file(root, "src/main.ts", "");
        create_file(root, "dist/main.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        let ownership = resolver.resolve(Path::new("src/main.ts"));
        assert!(matches!(ownership, ProjectOwnership::Owned { .. }));

        // dist should be excluded (inherited from base)
        let ownership = resolver.resolve(Path::new("dist/main.ts"));
        assert!(matches!(ownership, ProjectOwnership::Unowned));
    }

    #[test]
    fn test_ambiguous_when_overlapping_include() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Two configs at same level with overlapping include
        create_file(root, "tsconfig.app.json", r#"{"include": ["src/**/*.ts"]}"#);
        create_file(root, "tsconfig.lib.json", r#"{"include": ["src/**/*.ts"]}"#);
        create_file(root, "src/main.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        let ownership = resolver.resolve(Path::new("src/main.ts"));
        // Both configs claim the file, and they're at the same directory level
        assert!(matches!(ownership, ProjectOwnership::Ambiguous { .. }));
    }

    #[test]
    fn test_tiebreak_by_specificity() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Root config with broad include
        create_file(root, "tsconfig.json", r#"{"include": ["**/*.ts"]}"#);
        // Nested config with same include
        create_file(
            root,
            "packages/app/tsconfig.json",
            r#"{"include": ["**/*.ts"]}"#,
        );
        create_file(root, "packages/app/src/main.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        // Nested config should win (more specific)
        let ownership = resolver.resolve(Path::new("packages/app/src/main.ts"));
        if let ProjectOwnership::Owned { config_path, .. } = ownership {
            assert!(config_path.to_string_lossy().contains("packages/app"));
        } else {
            panic!("expected Owned with nested config winning");
        }
    }

    #[test]
    fn test_solution_style_config_does_not_own_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Solution-style config (references only, no include/files)
        create_file(
            root,
            "tsconfig.json",
            r#"{"references": [{"path": "./packages/app"}]}"#,
        );
        create_file(
            root,
            "packages/app/tsconfig.json",
            r#"{"include": ["src/**/*.ts"]}"#,
        );
        create_file(root, "packages/app/src/main.ts", "");

        let resolver = TsProjectOwnershipResolver::build(root).unwrap();

        // File should be owned by the app config, not the solution config
        let ownership = resolver.resolve(Path::new("packages/app/src/main.ts"));
        if let ProjectOwnership::Owned { config_path, .. } = ownership {
            assert!(config_path.to_string_lossy().contains("packages/app"));
        } else {
            panic!("expected Owned by app config");
        }
    }
}
