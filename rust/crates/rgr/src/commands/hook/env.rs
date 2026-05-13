//! Host environment variable detection and translation.
//!
//! Implements HOST-1 environment variable contract:
//! - Host-provided variables are host-specific (CLAUDE_*, CODEX_*)
//! - rmap internal variables (RMAP_*) are derived by hook commands
//!
//! Extended by HOOK-1A for stdin JSON transport:
//! - Hosts like Claude Code pass context as JSON on stdin
//! - StdinPayload is normalized to the same HookContext
//!
//! The translation layer belongs here, not in host shims.

use std::env;
use std::path::PathBuf;

use super::transport::StdinPayload;

/// Detected agent host type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostType {
    ClaudeCode,
    Codex,
    Unknown,
}

impl std::fmt::Display for HostType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostType::ClaudeCode => write!(f, "Claude Code"),
            HostType::Codex => write!(f, "Codex"),
            HostType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Context extracted from host environment variables.
#[derive(Debug, Clone)]
pub struct HostContext {
    /// Detected host type.
    pub host_type: HostType,
    /// Project/repository path (translated to RMAP_REPO_PATH).
    pub repo_path: Option<PathBuf>,
    /// Session identifier (translated to RMAP_SESSION_ID).
    pub session_id: Option<String>,
    /// Tool name (PostToolUse events).
    pub tool_name: Option<String>,
    /// Tool output (PostToolUse events, may contain file paths).
    pub tool_output: Option<String>,
    /// Prompt text (UserPromptSubmit events).
    pub prompt_text: Option<String>,
}

impl HostContext {
    /// Detect host type and extract context from environment variables.
    ///
    /// Detection order:
    /// 1. Claude Code: CLAUDE_PROJECT_PATH present
    /// 2. Codex: CODEX_PROJECT_PATH present
    /// 3. Unknown: neither present
    pub fn from_env() -> Self {
        let host_type = detect_host_type();

        match host_type {
            HostType::ClaudeCode => Self::from_claude_code_env(),
            HostType::Codex => Self::from_codex_env(),
            HostType::Unknown => Self::unknown(),
        }
    }

    /// Extract context from Claude Code environment variables.
    fn from_claude_code_env() -> Self {
        Self {
            host_type: HostType::ClaudeCode,
            repo_path: env::var("CLAUDE_PROJECT_PATH").ok().map(PathBuf::from),
            session_id: env::var("CLAUDE_SESSION_ID").ok(),
            tool_name: env::var("TOOL_NAME").ok(),
            tool_output: env::var("TOOL_OUTPUT").ok(),
            prompt_text: env::var("PROMPT_TEXT").ok(),
        }
    }

    /// Extract context from Codex environment variables.
    ///
    /// Note: Codex variable names are assumed based on HOST-1 contract.
    /// Actual names should be verified against Codex CLI documentation.
    fn from_codex_env() -> Self {
        Self {
            host_type: HostType::Codex,
            repo_path: env::var("CODEX_PROJECT_PATH").ok().map(PathBuf::from),
            session_id: env::var("CODEX_SESSION_ID").ok(),
            tool_name: env::var("TOOL_NAME").ok(),
            tool_output: env::var("CHANGED_FILES").ok(),
            prompt_text: env::var("PROMPT").ok(),
        }
    }

    /// Create context when host cannot be detected.
    fn unknown() -> Self {
        Self {
            host_type: HostType::Unknown,
            repo_path: None,
            session_id: None,
            tool_name: None,
            tool_output: None,
            prompt_text: None,
        }
    }

    /// Returns true if this context has enough information for hook execution.
    ///
    /// Minimally requires repo_path. session_id is recommended but not required
    /// (hooks can generate a session ID if needed).
    #[allow(dead_code)] // Future use
    pub fn is_usable(&self) -> bool {
        self.repo_path.is_some()
    }

    /// Generate a session ID if one was not provided.
    pub fn session_id_or_generate(&self) -> String {
        self.session_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
    }
}

/// Detect host type from environment variables.
fn detect_host_type() -> HostType {
    if env::var("CLAUDE_PROJECT_PATH").is_ok() || env::var("CLAUDE_SESSION_ID").is_ok() {
        HostType::ClaudeCode
    } else if env::var("CODEX_PROJECT_PATH").is_ok() || env::var("CODEX_SESSION_ID").is_ok() {
        HostType::Codex
    } else {
        HostType::Unknown
    }
}

/// Hook invocation context resolved from arguments or environment.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Database path.
    pub db_path: Option<PathBuf>,
    /// Repository path.
    pub repo_path: Option<PathBuf>,
    /// Session identifier.
    pub session_id: String,
    /// Host type (if detected).
    #[allow(dead_code)] // Future use: host-specific behavior
    pub host_type: HostType,
    /// Whether JSON output is requested.
    pub json_output: bool,
    /// Additional files (for post-edit).
    pub files: Vec<PathBuf>,
    /// Tool name (for post-edit).
    #[allow(dead_code)] // Future use: tool-specific refresh logic
    pub tool_name: Option<String>,
    /// Prompt text (for prompt-submit).
    pub prompt_text: Option<String>,
    /// Whether to require validation (for stop).
    pub require_validation: bool,
    /// Transcript output path (for stop).
    pub transcript_path: Option<PathBuf>,
    /// Whether to classify prompt (for prompt-submit).
    pub classify: bool,
}

impl HookContext {
    /// Parse hook context from command-line arguments.
    ///
    /// Supports three transport modes:
    /// - `--from-stdin`: Read JSON payload from stdin (Claude Code)
    /// - `--from-env`: Read from host environment variables (Codex)
    /// - Explicit: `--db <path> --repo <path>`
    ///
    /// Resolution order (per HOOK-1/HOOK-1A slices):
    /// 1. Explicit --db/--repo arguments (highest priority)
    /// 2. RMAP_DB_PATH, RMAP_REPO_PATH environment variables
    /// 3. --from-stdin: JSON payload provides cwd/session_id/event data
    /// 4. --from-env: Host environment variables (CLAUDE_PROJECT_PATH, etc.)
    /// 5. Discovery: find .rmap.db or repo.db in current directory or parents
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut from_stdin = false;
        let mut from_env = false;
        let mut db_path: Option<PathBuf> = None;
        let mut repo_path: Option<PathBuf> = None;
        let mut json_output = false;
        let mut files: Vec<PathBuf> = Vec::new();
        let mut require_validation = false;
        let mut transcript_path: Option<PathBuf> = None;
        let mut classify = false;
        let mut prompt_text_arg: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--from-stdin" => {
                    from_stdin = true;
                    i += 1;
                }
                "--from-env" => {
                    from_env = true;
                    i += 1;
                }
                "--db" => {
                    if i + 1 >= args.len() {
                        return Err("--db requires a path argument".to_string());
                    }
                    db_path = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                }
                "--repo" => {
                    if i + 1 >= args.len() {
                        return Err("--repo requires a path argument".to_string());
                    }
                    repo_path = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                }
                "--files" => {
                    if i + 1 >= args.len() {
                        return Err("--files requires a path argument".to_string());
                    }
                    // Parse comma-separated or JSON array
                    let files_arg = &args[i + 1];
                    files = parse_files_arg(files_arg)?;
                    i += 2;
                }
                "--json" => {
                    json_output = true;
                    i += 1;
                }
                "--require-validation" => {
                    require_validation = true;
                    i += 1;
                }
                "--transcript" => {
                    if i + 1 >= args.len() {
                        return Err("--transcript requires a path argument".to_string());
                    }
                    transcript_path = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                }
                "--classify" => {
                    classify = true;
                    i += 1;
                }
                "--prompt" => {
                    if i + 1 >= args.len() {
                        return Err("--prompt requires a text argument".to_string());
                    }
                    prompt_text_arg = Some(args[i + 1].clone());
                    i += 2;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown option: {}", other));
                }
                _ => {
                    // Positional arguments not supported in hook commands
                    return Err(format!("unexpected argument: {}", args[i]));
                }
            }
        }

        // Validate transport mode exclusivity
        if from_stdin && from_env {
            return Err("--from-stdin and --from-env are mutually exclusive".to_string());
        }

        // Parse stdin payload if requested
        let stdin_payload = if from_stdin {
            Some(StdinPayload::from_stdin()?)
        } else {
            None
        };

        // Resolution chain for DB path:
        // 1. Explicit --db argument
        // 2. RMAP_DB_PATH environment variable
        // 3. Discovery (find .rmap.db or repo.db)
        let resolved_db = db_path
            .or_else(|| env::var("RMAP_DB_PATH").ok().map(PathBuf::from))
            .or_else(discover_db_path);

        // Resolution chain for repo path:
        // 1. Explicit --repo argument
        // 2. RMAP_REPO_PATH environment variable
        // 3. --from-stdin: cwd from JSON payload
        // 4. --from-env: Host environment variables (CLAUDE_PROJECT_PATH, CODEX_PROJECT_PATH)
        // 5. Discovery from DB location or current directory
        let host_ctx = if from_env {
            HostContext::from_env()
        } else {
            HostContext::unknown()
        };

        let resolved_repo = repo_path
            .or_else(|| env::var("RMAP_REPO_PATH").ok().map(PathBuf::from))
            .or_else(|| stdin_payload.as_ref().map(|p| p.cwd.clone()))
            .or(host_ctx.repo_path.clone())
            .or_else(|| discover_repo_path(resolved_db.as_ref()));

        // Session ID resolution:
        // 1. --from-stdin: from JSON payload
        // 2. --from-env: from host environment
        // 3. Generate UUID
        let session_id = if let Some(ref payload) = stdin_payload {
            payload
                .session_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        } else if from_env {
            host_ctx.session_id_or_generate()
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        // Host type detection
        let host_type = if stdin_payload.is_some() {
            // Stdin transport is currently Claude Code specific
            HostType::ClaudeCode
        } else {
            host_ctx.host_type
        };

        // Tool name: stdin payload > env transport
        let tool_name = stdin_payload
            .as_ref()
            .and_then(|p| p.tool_name.clone())
            .or(host_ctx.tool_name.clone());

        // Tool output: stdin payload > env transport
        let tool_output = stdin_payload
            .as_ref()
            .and_then(|p| p.tool_output.clone())
            .or(host_ctx.tool_output);

        // Files: explicit arg > stdin payload > env transport
        let resolved_files = if !files.is_empty() {
            files
        } else if let Some(ref payload) = stdin_payload {
            payload.extract_file_paths()
        } else {
            tool_output
                .as_ref()
                .map(|s| parse_files_arg(s).unwrap_or_default())
                .unwrap_or_default()
        };

        // Prompt text: explicit arg > stdin payload > env transport
        let resolved_prompt = prompt_text_arg
            .or_else(|| stdin_payload.as_ref().and_then(|p| p.prompt.clone()))
            .or(host_ctx.prompt_text);

        // Transcript path: explicit arg > stdin payload
        let resolved_transcript = transcript_path.or_else(|| {
            stdin_payload
                .as_ref()
                .and_then(|p| p.transcript_path.clone())
        });

        Ok(Self {
            db_path: resolved_db,
            repo_path: resolved_repo,
            session_id,
            host_type,
            json_output,
            files: resolved_files,
            tool_name,
            prompt_text: resolved_prompt,
            require_validation,
            transcript_path: resolved_transcript,
            classify,
        })
    }
}

/// Discover database file by searching current directory and parents.
///
/// Looks for:
/// - .rmap.db
/// - repo.db
/// - *.db (single match only)
fn discover_db_path() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let mut dir = cwd.as_path();

    loop {
        // Check for .rmap.db
        let rmap_db = dir.join(".rmap.db");
        if rmap_db.exists() {
            return Some(rmap_db);
        }

        // Check for repo.db
        let repo_db = dir.join("repo.db");
        if repo_db.exists() {
            return Some(repo_db);
        }

        // Move to parent
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    None
}

/// Discover repository path from DB location or current directory.
fn discover_repo_path(db_path: Option<&PathBuf>) -> Option<PathBuf> {
    // If we have a DB path, the repo is likely in the same directory or parent
    if let Some(db) = db_path {
        if let Some(parent) = db.parent() {
            // Check if parent looks like a repo (has .git or common markers)
            if parent.join(".git").exists() {
                return Some(parent.to_path_buf());
            }
            // Otherwise just use the DB's directory
            return Some(parent.to_path_buf());
        }
    }

    // Fall back to current directory if it looks like a repo
    let cwd = env::current_dir().ok()?;
    if cwd.join(".git").exists() {
        return Some(cwd);
    }

    // Search upward for .git
    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    None
}

/// Parse files argument (comma-separated or JSON array).
fn parse_files_arg(arg: &str) -> Result<Vec<PathBuf>, String> {
    let trimmed = arg.trim();

    // Try JSON array first
    if trimmed.starts_with('[') {
        let parsed: Result<Vec<String>, _> = serde_json::from_str(trimmed);
        match parsed {
            Ok(paths) => return Ok(paths.into_iter().map(PathBuf::from).collect()),
            Err(_) => {
                // Fall through to comma-separated parsing
            }
        }
    }

    // Comma-separated
    Ok(trimmed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_files_comma_separated() {
        let result = parse_files_arg("src/a.rs, src/b.rs").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], PathBuf::from("src/a.rs"));
        assert_eq!(result[1], PathBuf::from("src/b.rs"));
    }

    #[test]
    fn parse_files_json_array() {
        let result = parse_files_arg(r#"["src/a.rs", "src/b.rs"]"#).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], PathBuf::from("src/a.rs"));
        assert_eq!(result[1], PathBuf::from("src/b.rs"));
    }

    #[test]
    fn parse_files_empty() {
        let result = parse_files_arg("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn hook_context_from_explicit_args() {
        let args = vec![
            "--db".to_string(),
            "test.db".to_string(),
            "--repo".to_string(),
            "/path/to/repo".to_string(),
        ];
        let ctx = HookContext::from_args(&args).unwrap();
        assert_eq!(ctx.db_path, Some(PathBuf::from("test.db")));
        assert_eq!(ctx.repo_path, Some(PathBuf::from("/path/to/repo")));
        assert_eq!(ctx.host_type, HostType::Unknown);
    }

    #[test]
    fn hook_context_json_flag() {
        let args = vec!["--json".to_string()];
        let ctx = HookContext::from_args(&args).unwrap();
        assert!(ctx.json_output);
    }

    #[test]
    fn from_stdin_and_from_env_mutually_exclusive() {
        let args = vec!["--from-stdin".to_string(), "--from-env".to_string()];
        let result = HookContext::from_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mutually exclusive"));
    }

    #[test]
    fn explicit_args_override_transport() {
        // Even with transport flags, explicit args take priority
        // (can't test --from-stdin without mocking stdin, but can test the precedence principle)
        let args = vec![
            "--db".to_string(),
            "explicit.db".to_string(),
            "--repo".to_string(),
            "/explicit/repo".to_string(),
            "--from-env".to_string(),
        ];
        let ctx = HookContext::from_args(&args).unwrap();
        // Explicit args should take precedence
        assert_eq!(ctx.db_path, Some(PathBuf::from("explicit.db")));
        assert_eq!(ctx.repo_path, Some(PathBuf::from("/explicit/repo")));
    }
}
