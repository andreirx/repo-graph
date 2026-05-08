//! TSServer client and ReceiverTypeResolver implementation.
//!
//! Spawns tsserver as subprocess, communicates via newline-delimited JSON.
//! Uses a dedicated reader thread with channel timeout for real timeout
//! enforcement on blocking reads.
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────┐
//! │                    TsServerResolver                           │
//! │  (stateless, creates sessions per project context)           │
//! └───────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌───────────────────────────────────────────────────────────────┐
//! │                    TsServerSession                            │
//! │  (one per tsconfig/jsconfig/package.json context)            │
//! │                                                               │
//! │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐          │
//! │  │   stdin     │   │  stdout     │   │ reader thr  │          │
//! │  │  (write)    │   │  (read)     │   │  (timeout)  │          │
//! │  └─────────────┘   └─────────────┘   └─────────────┘          │
//! └───────────────────────────────────────────────────────────────┘
//! ```
//!
//! # TSServer Protocol
//!
//! Unlike LSP, TSServer uses:
//! - Newline-delimited JSON (no Content-Length headers)
//! - `seq` / `request_seq` for request/response correlation
//! - Events emitted asynchronously (must be filtered)
//! - `quickinfo` command for type information at position

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

use tracing::{debug, warn};

use enrichment::{
    EligibleEdge, EnrichmentLanguage, ReceiverTypeOrigin, ReceiverTypeResolver, ReceiverTypeResult,
    ResolverError, ResolverProgress, UnresolvedCategory,
};

use crate::project::group_by_project_root;
use crate::protocol::{
    commands, CloseArgs, ConfigureArgs, OpenArgs, QuickInfoArgs, QuickInfoBody, Request,
};
use crate::transport::{write_request, ReaderHandle, SeqGenerator, TransportError};

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the tsserver resolver.
#[derive(Debug, Clone)]
pub struct TsServerConfig {
    /// Path to the tsserver executable.
    /// If None, searches PATH for "tsserver" or falls back to npx.
    pub tsserver_path: Option<String>,

    /// Timeout for quickinfo requests (seconds).
    pub quickinfo_timeout_secs: u64,

    /// Maximum warm-up retries before giving up.
    pub warmup_retries: u32,

    /// Delay between warm-up retries (milliseconds).
    pub warmup_delay_ms: u64,

    /// Whether to log tsserver stderr (for debugging).
    pub log_stderr: bool,
}

impl Default for TsServerConfig {
    fn default() -> Self {
        Self {
            tsserver_path: None,
            quickinfo_timeout_secs: 15,
            warmup_retries: 20,
            warmup_delay_ms: 1500,
            log_stderr: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// QuickInfo Outcome
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome of a quickinfo request.
///
/// Distinguishes between server not responding vs. server responding
/// but no type found. Critical for warm-up detection.
enum QuickInfoOutcome {
    /// Server did not respond (transport error, process exited).
    NoResponse,
    /// Request timed out (enforced via reader thread).
    Timeout,
    /// Server responded with error.
    Error(String),
    /// Server responded successfully. `type_name` is Some if extracted.
    ServerResponded { type_name: Option<String> },
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver
// ─────────────────────────────────────────────────────────────────────────────

/// TypeScript/JavaScript receiver type resolver using tsserver.
///
/// Sessions are created per-batch, per-project-context. The resolver
/// itself is stateless between batches.
pub struct TsServerResolver {
    config: TsServerConfig,
}

impl TsServerResolver {
    pub fn new() -> Self {
        Self {
            config: TsServerConfig::default(),
        }
    }

    pub fn with_config(config: TsServerConfig) -> Self {
        Self { config }
    }
}

impl Default for TsServerResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiverTypeResolver for TsServerResolver {
    fn language(&self) -> EnrichmentLanguage {
        EnrichmentLanguage::TypeScript
    }

    fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
        // Initialization is deferred to resolve_batch, per-project-context.
        // Each tsconfig/jsconfig/package.json gets its own tsserver session.
        Ok(())
    }

    fn resolve_batch(
        &self,
        repo_root: &Path,
        edges: &[EligibleEdge],
        progress: Option<&dyn ResolverProgress>,
    ) -> Vec<ReceiverTypeResult> {
        if edges.is_empty() {
            return Vec::new();
        }

        // Build ownership resolver to determine which tsconfig owns each file.
        // This replaces the naive "nearest config by directory" grouping.
        let ownership_resolver = match crate::ownership::TsProjectOwnershipResolver::build(repo_root) {
            Ok(r) => Some(r),
            Err(e) => {
                warn!(error = %e, "failed to build ownership resolver, falling back to directory grouping");
                None
            }
        };

        // Group edges by owning project config.
        // Edges with Unowned or Ambiguous ownership fail explicitly.
        let (groups, ownership_failures) = if let Some(ref resolver) = ownership_resolver {
            group_by_ownership(resolver, repo_root, edges)
        } else {
            // Fallback: use legacy directory-based grouping
            let legacy_groups = group_by_project_root(repo_root, edges);
            (legacy_groups, Vec::new())
        };

        let mut all_results: Vec<ReceiverTypeResult> = ownership_failures;
        let total = edges.len();
        let mut processed = all_results.len();

        for (project_root, group_edges) in groups {
            // Report progress: starting session
            if let Some(p) = progress {
                p.report(enrichment::Progress {
                    phase: enrichment::ProgressPhase::Initializing,
                    current: processed,
                    total,
                });
            }

            // Start tsserver session for this project context
            let session_result = TsServerSession::start(&project_root, repo_root, &self.config);

            let mut session = match session_result {
                Ok(s) => s,
                Err(e) => {
                    // Failed to start — mark all edges in this group as failed
                    warn!(
                        project_root = %project_root.display(),
                        error = %e,
                        "tsserver failed to start"
                    );
                    for edge in &group_edges {
                        all_results.push(ReceiverTypeResult::failed(
                            edge.edge_uid.clone(),
                            format!("tsserver failed to start: {}", e),
                        ));
                    }
                    processed += group_edges.len();
                    continue;
                }
            };

            // Warm up: wait for tsserver to be ready
            if let Some(p) = progress {
                p.report(enrichment::Progress {
                    phase: enrichment::ProgressPhase::LoadingProject,
                    current: processed,
                    total,
                });
            }

            let first_edge = &group_edges[0];
            let first_path = repo_root.join(&first_edge.source_file_path);
            let warmed_up = session.warm_up(
                &first_path,
                first_edge.line_start,
                first_edge.col_start,
            );

            if !warmed_up {
                warn!(
                    project_root = %project_root.display(),
                    "tsserver did not respond after warm-up timeout"
                );
                session.stop();
                for edge in &group_edges {
                    all_results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "tsserver did not respond after loading timeout",
                    ));
                }
                processed += group_edges.len();
                continue;
            }

            // Resolve types for this group
            for edge in &group_edges {
                if let Some(p) = progress {
                    p.report(enrichment::Progress {
                        phase: enrichment::ProgressPhase::ResolvingTypes,
                        current: processed,
                        total,
                    });
                }

                let abs_path = repo_root.join(&edge.source_file_path);

                // For wildcard receiver category, use tree-sitter to locate
                // the actual receiver expression before querying tsserver.
                if edge.category == UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo {
                    let result = resolve_wildcard_receiver(
                        &mut session,
                        repo_root,
                        &abs_path,
                        edge.line_start,
                        edge.col_start,
                        &edge.edge_uid,
                        &edge.source_file_path,
                    );
                    all_results.push(result);
                    processed += 1;
                    continue;
                }

                let result = session.resolve_type(&abs_path, edge.line_start, edge.col_start, &edge.edge_uid);
                all_results.push(result);
                processed += 1;
            }

            // Stop session for this project context
            session.stop();
        }

        // Report completion
        if let Some(p) = progress {
            p.report(enrichment::Progress {
                phase: enrichment::ProgressPhase::Done,
                current: total,
                total,
            });
        }

        all_results
    }

    fn shutdown(&mut self) {
        // Sessions are created and destroyed per-batch, so nothing to do here.
        // Each session's Drop impl handles graceful shutdown.
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session
// ─────────────────────────────────────────────────────────────────────────────

/// A single tsserver session (one per project context).
struct TsServerSession {
    process: Child,
    stdin: ChildStdin,
    reader: ReaderHandle,
    seq: SeqGenerator,
    opened_files: HashSet<String>,
    config: TsServerConfig,
}

impl TsServerSession {
    /// Start a tsserver session for the given project root.
    fn start(
        project_root: &Path,
        repo_root: &Path,
        config: &TsServerConfig,
    ) -> Result<Self, ResolverError> {
        // Find tsserver executable
        let tsserver_cmd = find_tsserver(config, repo_root)?;

        // Spawn tsserver
        // Note: tsserver wants to be run from the project root to find tsconfig.json
        let mut command = Command::new(&tsserver_cmd);
        command
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());

        if config.log_stderr {
            command.stderr(Stdio::inherit());
        } else {
            command.stderr(Stdio::null());
        }

        let mut process = command.spawn().map_err(|e| ResolverError::ToolNotAvailable {
            tool: format!("tsserver ({}): {}", tsserver_cmd, e),
        })?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| ResolverError::StartupFailed {
                reason: "failed to capture stdin".to_string(),
            })?;

        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| ResolverError::StartupFailed {
                reason: "failed to capture stdout".to_string(),
            })?;

        // Spawn reader thread with timeout support
        let reader = ReaderHandle::spawn(stdout);

        let mut session = Self {
            process,
            stdin,
            reader,
            seq: SeqGenerator::new(),
            opened_files: HashSet::new(),
            config: config.clone(),
        };

        // Send configure request (optional but good practice)
        session.configure()?;

        debug!(project_root = %project_root.display(), "tsserver session started");

        Ok(session)
    }

    /// Send configure request to tsserver.
    fn configure(&mut self) -> Result<(), ResolverError> {
        let seq = self.seq.next();
        let request = Request::new(
            seq,
            commands::CONFIGURE,
            ConfigureArgs {
                host_info: Some("repo-graph enrichment".to_string()),
                preferences: None,
            },
        );

        write_request(&mut self.stdin, &request).map_err(|e| ResolverError::StartupFailed {
            reason: format!("failed to send configure: {}", e),
        })?;

        // Wait for configure response (short timeout, tsserver is usually ready immediately)
        let timeout = Duration::from_secs(5);
        match self.reader.recv_response(seq, timeout) {
            Ok(resp) => {
                if !resp.success {
                    warn!(message = ?resp.message, "tsserver configure returned failure");
                }
                Ok(())
            }
            Err(TransportError::Timeout(_)) => {
                // Configure timeout is not fatal — tsserver may still work
                warn!("tsserver configure timed out, continuing");
                Ok(())
            }
            Err(e) => Err(ResolverError::StartupFailed {
                reason: format!("configure failed: {}", e),
            }),
        }
    }

    /// Warm up by retrying quickinfo until tsserver responds.
    ///
    /// IMPORTANT: This checks for server readiness (any response), NOT
    /// for successful type extraction. The first edge may be unresolvable,
    /// but if the server responds at all, it's ready.
    fn warm_up(&mut self, file_path: &Path, line: u32, col: u32) -> bool {
        for attempt in 0..self.config.warmup_retries {
            debug!(attempt, "warming up tsserver");

            match self.quickinfo_raw(file_path, line, col) {
                QuickInfoOutcome::ServerResponded { .. } => {
                    // Server responded (even if no type found) — ready
                    debug!(attempt, "tsserver ready (server responded)");
                    return true;
                }
                QuickInfoOutcome::Error(_) => {
                    // Server responded with error — still means it's ready
                    debug!(attempt, "tsserver ready (responded with error)");
                    return true;
                }
                QuickInfoOutcome::NoResponse => {
                    // Server not responding yet — retry
                    debug!(attempt, "tsserver not ready, retrying");
                }
                QuickInfoOutcome::Timeout => {
                    // Timeout on this attempt — retry
                    debug!(attempt, "tsserver quickinfo timed out, retrying");
                }
            }

            std::thread::sleep(Duration::from_millis(self.config.warmup_delay_ms));
        }

        false
    }

    /// Open a file in tsserver.
    fn open_file(&mut self, file_path: &Path) -> Result<(), TransportError> {
        let file_str = file_path.to_string_lossy().to_string();

        if self.opened_files.contains(&file_str) {
            return Ok(());
        }

        let seq = self.seq.next();
        let request = Request::new(
            seq,
            commands::OPEN,
            OpenArgs {
                file: file_str.clone(),
                file_content: None,
                script_kind_name: None,
                project_root_path: None,
            },
        );

        write_request(&mut self.stdin, &request)?;

        // Open is a fire-and-forget command in tsserver (no response expected)
        // but we'll give it a moment to register
        self.opened_files.insert(file_str);

        Ok(())
    }

    /// Send quickinfo request and return raw outcome.
    fn quickinfo_raw(&mut self, file_path: &Path, line: u32, col: u32) -> QuickInfoOutcome {
        // Open document if not already open
        if let Err(e) = self.open_file(file_path) {
            debug!(error = %e, "failed to open file for quickinfo");
            return QuickInfoOutcome::NoResponse;
        }

        let file_str = file_path.to_string_lossy().to_string();

        // Send quickinfo request
        let seq = self.seq.next();
        let request = Request::new(
            seq,
            commands::QUICKINFO,
            QuickInfoArgs {
                file: file_str,
                line,
                offset: col + 1, // TSServer uses 1-based columns
            },
        );

        if write_request(&mut self.stdin, &request).is_err() {
            return QuickInfoOutcome::NoResponse;
        }

        // Read response with real timeout enforcement
        let timeout = Duration::from_secs(self.config.quickinfo_timeout_secs);
        match self.reader.recv_response(seq, timeout) {
            Ok(resp) => {
                if !resp.success {
                    let msg = resp.message.unwrap_or_else(|| "unknown error".to_string());
                    return QuickInfoOutcome::Error(msg);
                }

                // Parse body to extract type information
                match resp.body {
                    Some(body) => {
                        match serde_json::from_value::<QuickInfoBody>(body) {
                            Ok(info) => {
                                let type_name = extract_type_from_quickinfo(&info);
                                QuickInfoOutcome::ServerResponded { type_name }
                            }
                            Err(_) => {
                                // Body parse failed, but server responded
                                QuickInfoOutcome::ServerResponded { type_name: None }
                            }
                        }
                    }
                    None => {
                        // No body (position has no symbol)
                        QuickInfoOutcome::ServerResponded { type_name: None }
                    }
                }
            }
            Err(TransportError::Timeout(_)) => QuickInfoOutcome::Timeout,
            Err(TransportError::ProcessExited) => QuickInfoOutcome::NoResponse,
            Err(_) => QuickInfoOutcome::NoResponse,
        }
    }

    /// Resolve type at position and return ReceiverTypeResult.
    fn resolve_type(
        &mut self,
        file_path: &Path,
        line: u32,
        col: u32,
        edge_uid: &str,
    ) -> ReceiverTypeResult {
        match self.quickinfo_raw(file_path, line, col) {
            QuickInfoOutcome::ServerResponded { type_name: Some(name) } => {
                if is_valid_ts_type_name(&name) {
                    ReceiverTypeResult {
                        edge_uid: edge_uid.to_string(),
                        receiver_type: Some(name.clone()),
                        // type_display_name is the normalized type name, not the full signature
                        type_display_name: Some(name.clone()),
                        is_external_type: is_external_type(&name),
                        origin: ReceiverTypeOrigin::Compiler,
                        failure_reason: None,
                    }
                } else {
                    ReceiverTypeResult::failed(
                        edge_uid.to_string(),
                        format!("invalid_type_name:{}", name),
                    )
                }
            }
            QuickInfoOutcome::ServerResponded { type_name: None } => {
                ReceiverTypeResult::failed(edge_uid.to_string(), "quickinfo_no_type")
            }
            QuickInfoOutcome::Error(msg) => {
                ReceiverTypeResult::failed(edge_uid.to_string(), format!("quickinfo_error:{}", msg))
            }
            QuickInfoOutcome::Timeout => {
                ReceiverTypeResult::failed(edge_uid.to_string(), "quickinfo_timeout")
            }
            QuickInfoOutcome::NoResponse => {
                ReceiverTypeResult::failed(edge_uid.to_string(), "quickinfo_no_response")
            }
        }
    }

    /// Stop the tsserver process gracefully.
    fn stop(&mut self) {
        // Close all opened files
        for file in self.opened_files.drain() {
            let seq = self.seq.next();
            let request = Request::new(seq, commands::CLOSE, CloseArgs { file });
            let _ = write_request(&mut self.stdin, &request);
        }

        // Send exit command
        let seq = self.seq.next();
        let request = Request::new(seq, commands::EXIT, serde_json::Value::Null);
        let _ = write_request(&mut self.stdin, &request);

        // Kill process if still running
        let _ = self.process.kill();
    }
}

impl Drop for TsServerSession {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type Extraction
// ─────────────────────────────────────────────────────────────────────────────

/// Extract type name from QuickInfo body.
///
/// TSServer returns type information in the `kind` and `displayString` fields.
/// The displayString contains the full signature, e.g.:
///   "(property) MyClass.myProp: string"
///   "(method) Engine.start(): void"
///   "(local var) obj: MyClass"
///
/// For method calls like `obj.method()`, we want the receiver type, which
/// is the type of `obj`. The displayString for the identifier `obj` would be:
///   "(local var) obj: MyClass"
/// And we extract "MyClass".
fn extract_type_from_quickinfo(info: &QuickInfoBody) -> Option<String> {
    // Primary: extract from displayString
    if !info.display_string.is_empty() {
        return extract_type_from_display_string(&info.display_string);
    }

    // Fallback: use kind if it's a class/interface name
    if info.kind == "class" || info.kind == "interface" || info.kind == "type" {
        // For class/interface definitions, we don't have a receiver type
        // This is the definition itself, not a usage
        return None;
    }

    None
}

/// Extract type from tsserver displayString.
///
/// Patterns:
///   "(local var) name: Type"           -> "Type"
///   "(parameter) name: Type"           -> "Type"
///   "(property) Class.name: Type"      -> "Type"
///   "(const) name: Type"               -> "Type"
///   "(let) name: Type"                 -> "Type"
///   "(method) Class.method(): Type"    -> NOT a receiver type
///   "class ClassName"                  -> NOT a receiver type
///
/// For complex types:
///   "(local var) arr: string[]"        -> "Array"
///   "(local var) obj: { x: number }"   -> skip (anonymous)
///   "(local var) fn: () => void"       -> skip (function)
///   "(local var) union: A | B"         -> skip (union)
///   "(local var) gen: Promise<T>"      -> "Promise"
fn extract_type_from_display_string(s: &str) -> Option<String> {
    // Skip method signatures — we're looking for receiver types
    if s.starts_with("(method)") || s.starts_with("(function)") {
        return None;
    }

    // Skip class/interface definitions
    if s.starts_with("class ") || s.starts_with("interface ") || s.starts_with("type ") {
        return None;
    }

    // Look for ": Type" pattern
    let colon_pos = s.rfind(':')?;
    let type_part = s[colon_pos + 1..].trim();

    // Skip empty
    if type_part.is_empty() {
        return None;
    }

    // Skip "any", "unknown", "never" — not useful
    if matches!(type_part, "any" | "unknown" | "never" | "void") {
        return None;
    }

    // Skip union types (contain |)
    if type_part.contains('|') {
        return None;
    }

    // Skip intersection types (contain &)
    if type_part.contains('&') {
        return None;
    }

    // Skip anonymous object types (start with {)
    if type_part.starts_with('{') {
        return None;
    }

    // Skip function types (contain =>)
    if type_part.contains("=>") {
        return None;
    }

    // Handle array types: T[] -> Array
    if type_part.ends_with("[]") {
        return Some("Array".to_string());
    }

    // Handle generic types: Promise<T> -> Promise
    if let Some(angle_pos) = type_part.find('<') {
        let base = type_part[..angle_pos].trim();
        if is_valid_ts_type_name(base) {
            return Some(base.to_string());
        }
        return None;
    }

    // Handle parenthesized types (typeof X), (X)
    if type_part.starts_with('(') || type_part.starts_with("typeof ") {
        return None;
    }

    // Plain type name
    if is_valid_ts_type_name(type_part) {
        return Some(type_part.to_string());
    }

    None
}

/// Check if a type name is valid for promotion.
///
/// Valid: "MyClass", "Promise", "Array", "Map", "Set", "SomeService"
/// Invalid: primitives, "any", empty, special chars
fn is_valid_ts_type_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Skip TypeScript primitives and special types
    if matches!(
        name,
        "string" | "number" | "boolean" | "null" | "undefined" | "void" |
        "any" | "unknown" | "never" | "object" | "symbol" | "bigint" |
        "String" | "Number" | "Boolean" | "Object" | "Symbol" | "BigInt"
    ) {
        return false;
    }

    // Must start with letter or underscore
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }

    // Rest must be alphanumeric or underscore
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check if a type appears to be from an external package.
///
/// Heuristics:
/// - Well-known Node.js types (Buffer, Stream, etc.)
/// - Well-known library types (Observable, Express, etc.)
fn is_external_type(type_name: &str) -> bool {
    // Node.js built-in types
    const NODE_TYPES: &[&str] = &[
        "Buffer", "Stream", "Readable", "Writable", "Transform",
        "EventEmitter", "ChildProcess", "Server", "Socket",
        "IncomingMessage", "ServerResponse", "Agent",
    ];

    // Common library types
    const LIBRARY_TYPES: &[&str] = &[
        // RxJS
        "Observable", "Subject", "BehaviorSubject", "ReplaySubject",
        "Subscription", "Subscriber", "Observer",
        // Express
        "Express", "Router", "NextFunction",
        // React
        "Component", "PureComponent", "ReactNode", "ReactElement",
        "RefObject", "MutableRefObject",
        // Angular
        "NgModule", "Injector", "ComponentRef",
        // Database
        "Connection", "Repository", "QueryBuilder",
    ];

    NODE_TYPES.contains(&type_name) || LIBRARY_TYPES.contains(&type_name)
}

// ─────────────────────────────────────────────────────────────────────────────
// Wildcard Receiver Resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve type for wildcard receiver category using tree-sitter localization.
///
/// For `this.field.method()`, the edge position points at `method`, but we
/// need the type of `this.field`. This function:
/// 1. Reads the source file
/// 2. Uses tree-sitter to locate the receiver expression
/// 3. Queries tsserver for the type at the receiver position
fn resolve_wildcard_receiver(
    session: &mut TsServerSession,
    _repo_root: &Path,
    abs_path: &Path,
    line: u32,
    col: u32,
    edge_uid: &str,
    rel_path: &str,
) -> ReceiverTypeResult {
    // Read source file
    let source = match std::fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(e) => {
            return ReceiverTypeResult::failed(
                edge_uid.to_string(),
                format!("receiver_locator_read_error:{}", e),
            );
        }
    };

    // Create locator based on file extension
    let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");
    let mut locator = if is_tsx {
        match crate::receiver_locator::ReceiverLocator::new_tsx() {
            Ok(l) => l,
            Err(e) => {
                return ReceiverTypeResult::failed(
                    edge_uid.to_string(),
                    format!("receiver_locator_init_error:{}", e),
                );
            }
        }
    } else {
        match crate::receiver_locator::ReceiverLocator::new_typescript() {
            Ok(l) => l,
            Err(e) => {
                return ReceiverTypeResult::failed(
                    edge_uid.to_string(),
                    format!("receiver_locator_init_error:{}", e),
                );
            }
        }
    };

    // Locate receiver expression
    let location = locator.locate_receiver(&source, line, col);

    match location {
        crate::receiver_locator::ReceiverLocation::Found {
            line: recv_line,
            column: recv_col,
            text,
        } => {
            debug!(
                edge_uid = edge_uid,
                original_pos = format!("{}:{}", line, col),
                receiver_pos = format!("{}:{}", recv_line, recv_col),
                receiver_text = text.as_str(),
                "located receiver expression"
            );

            // Query tsserver at the receiver position
            session.resolve_type(abs_path, recv_line, recv_col, edge_uid)
        }

        crate::receiver_locator::ReceiverLocation::NoReceiver => {
            ReceiverTypeResult::failed(
                edge_uid.to_string(),
                "receiver_locator_no_receiver",
            )
        }

        crate::receiver_locator::ReceiverLocation::UnsupportedPattern { reason } => {
            ReceiverTypeResult::failed(
                edge_uid.to_string(),
                format!("receiver_locator_unsupported:{}", reason),
            )
        }

        crate::receiver_locator::ReceiverLocation::ParseError { reason } => {
            ReceiverTypeResult::failed(
                edge_uid.to_string(),
                format!("receiver_locator_parse_error:{}", reason),
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ownership-Based Grouping
// ─────────────────────────────────────────────────────────────────────────────

/// Group edges by their owning tsconfig, using the ownership resolver.
///
/// Returns:
/// - Map of project_root → edges owned by that project
/// - Vec of failures for edges with Unowned or Ambiguous ownership
fn group_by_ownership(
    resolver: &crate::ownership::TsProjectOwnershipResolver,
    _repo_root: &Path,
    edges: &[EligibleEdge],
) -> (HashMap<PathBuf, Vec<EligibleEdge>>, Vec<ReceiverTypeResult>) {
    let mut groups: HashMap<PathBuf, Vec<EligibleEdge>> = HashMap::new();
    let mut failures: Vec<ReceiverTypeResult> = Vec::new();

    for edge in edges {
        let file_path = Path::new(&edge.source_file_path);

        match resolver.resolve(file_path) {
            crate::ownership::ProjectOwnership::Owned { project_root, .. } => {
                groups
                    .entry(project_root)
                    .or_default()
                    .push(edge.clone());
            }
            crate::ownership::ProjectOwnership::Unowned => {
                // File not covered by any tsconfig — explicit failure
                failures.push(ReceiverTypeResult::failed(
                    edge.edge_uid.clone(),
                    "ts_project_ownership_not_found",
                ));
            }
            crate::ownership::ProjectOwnership::Ambiguous { candidates } => {
                // Multiple tsconfigs claim ownership — explicit failure
                let candidate_list: Vec<String> = candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                failures.push(ReceiverTypeResult::failed(
                    edge.edge_uid.clone(),
                    format!(
                        "ts_project_ownership_ambiguous:{}",
                        candidate_list.join(",")
                    ),
                ));
            }
        }
    }

    (groups, failures)
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Find the tsserver executable.
///
/// Search order:
/// 1. Config-specified path
/// 2. node_modules/.bin/tsserver (project-local)
/// 3. `tsserver` in PATH
/// 4. `npx tsserver` as fallback
fn find_tsserver(config: &TsServerConfig, repo_root: &Path) -> Result<String, ResolverError> {
    // 1. Config-specified
    if let Some(path) = &config.tsserver_path {
        if Path::new(path).exists() {
            return Ok(path.clone());
        }
        // Try as command name
        return Ok(path.clone());
    }

    // 2. Project-local node_modules
    let local_tsserver = repo_root.join("node_modules/.bin/tsserver");
    if local_tsserver.exists() {
        return Ok(local_tsserver.to_string_lossy().to_string());
    }

    // 3. Check PATH
    if which_tsserver() {
        return Ok("tsserver".to_string());
    }

    // 4. Fallback to npx (assumes npm/npx is installed)
    // npx will download typescript if needed
    Err(ResolverError::ToolNotAvailable {
        tool: "tsserver not found in node_modules or PATH".to_string(),
    })
}

/// Check if tsserver is available in PATH.
fn which_tsserver() -> bool {
    Command::new("which")
        .arg("tsserver")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_from_display_string_local_var() {
        assert_eq!(
            extract_type_from_display_string("(local var) obj: MyClass"),
            Some("MyClass".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_display_string_parameter() {
        assert_eq!(
            extract_type_from_display_string("(parameter) service: UserService"),
            Some("UserService".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_display_string_property() {
        assert_eq!(
            extract_type_from_display_string("(property) Engine.state: EngineState"),
            Some("EngineState".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_display_string_const() {
        assert_eq!(
            extract_type_from_display_string("(const) logger: Logger"),
            Some("Logger".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_display_string_generic() {
        assert_eq!(
            extract_type_from_display_string("(local var) promise: Promise<string>"),
            Some("Promise".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_display_string_array() {
        assert_eq!(
            extract_type_from_display_string("(local var) items: Item[]"),
            Some("Array".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_display_string_skips_any() {
        assert_eq!(
            extract_type_from_display_string("(local var) x: any"),
            None
        );
    }

    #[test]
    fn test_extract_type_from_display_string_skips_union() {
        assert_eq!(
            extract_type_from_display_string("(local var) x: A | B"),
            None
        );
    }

    #[test]
    fn test_extract_type_from_display_string_skips_method() {
        assert_eq!(
            extract_type_from_display_string("(method) Engine.start(): void"),
            None
        );
    }

    #[test]
    fn test_extract_type_from_display_string_skips_anonymous() {
        assert_eq!(
            extract_type_from_display_string("(local var) obj: { x: number }"),
            None
        );
    }

    #[test]
    fn test_is_valid_ts_type_name() {
        assert!(is_valid_ts_type_name("MyClass"));
        assert!(is_valid_ts_type_name("UserService"));
        assert!(is_valid_ts_type_name("_InternalClass"));
        assert!(is_valid_ts_type_name("Promise"));

        // Invalid
        assert!(!is_valid_ts_type_name("string"));
        assert!(!is_valid_ts_type_name("number"));
        assert!(!is_valid_ts_type_name("any"));
        assert!(!is_valid_ts_type_name(""));
        assert!(!is_valid_ts_type_name("123Class"));
    }

    #[test]
    fn test_is_external_type() {
        assert!(is_external_type("Buffer"));
        assert!(is_external_type("Observable"));
        assert!(is_external_type("EventEmitter"));

        assert!(!is_external_type("MyClass"));
        assert!(!is_external_type("UserService"));
    }
}
