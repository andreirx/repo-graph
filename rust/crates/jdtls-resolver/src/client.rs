//! Eclipse JDT Language Server client and ReceiverTypeResolver implementation.
//!
//! Spawns jdtls as subprocess, communicates via LSP JSON-RPC.
//! Uses shared lsp-subprocess crate for transport machinery.
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────┐
//! │                      JdtlsResolver                            │
//! │  (stateless, creates sessions per workspace context)         │
//! └───────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌───────────────────────────────────────────────────────────────┐
//! │                      JdtlsSession                             │
//! │  (one per Java workspace / settings.gradle scope)            │
//! │                                                               │
//! │  Uses lsp-subprocess for:                                     │
//! │  - Content-Length framing                                     │
//! │  - Reader thread + timeout                                    │
//! │  - Request/response correlation                               │
//! └───────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Configuration
//!
//! jdtls requires an explicit path — no automatic discovery.
//! This is intentional: jdtls packaging is inconsistent across
//! platforms, and bad discovery logic creates false confidence.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

use lsp_subprocess::{
    write_notification, write_request, IdGenerator, LspResponse, ReaderHandle, TransportError,
};
use lsp_types::{
    ClientCapabilities, Hover, HoverContents, HoverParams, InitializeParams, InitializedParams,
    MarkedString, MarkupContent, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
    WorkspaceFolder,
};
use tracing::{debug, warn};

use enrichment::{
    BatchResolution, EligibleEdge, EnrichmentLanguage, ReceiverTypeOrigin, ReceiverTypeResolver,
    ReceiverTypeResult, ResolverError, ResolverProgress, UnresolvedCategory,
};

use crate::project::{group_by_workspace_root, BuildSystem, JavaProjectContext};

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the jdtls resolver.
///
/// PROVISIONAL: These defaults are more generous than rust-analyzer due to
/// JVM startup + dependency resolution overhead. They are configurable and
/// should be tuned based on real-world performance data.
#[derive(Debug, Clone)]
pub struct JdtlsConfig {
    /// Path to the jdtls executable or launcher script.
    /// REQUIRED: No automatic discovery. Fail clearly without this.
    pub jdtls_path: Option<String>,

    /// Path to jdtls workspace data directory.
    /// jdtls requires a writable directory for workspace metadata.
    /// If None, uses a temp directory.
    pub workspace_data_path: Option<String>,

    /// Timeout for initialization (seconds).
    /// Provisional: 120s for JVM startup + project import.
    pub init_timeout_secs: u64,

    /// Maximum warm-up retries before giving up.
    /// Provisional: 45 retries (more than rust-analyzer's 30).
    pub warmup_retries: u32,

    /// Delay between warm-up retries (milliseconds).
    /// Provisional: 3000ms (more than rust-analyzer's 2000ms).
    pub warmup_delay_ms: u64,

    /// Timeout for individual hover requests (seconds).
    /// Provisional: 15s (same as tsserver).
    pub hover_timeout_secs: u64,

    /// Whether to log jdtls stderr (for debugging).
    pub log_stderr: bool,
}

impl Default for JdtlsConfig {
    fn default() -> Self {
        Self {
            jdtls_path: None,
            workspace_data_path: None,
            init_timeout_secs: 120,
            warmup_retries: 45,
            warmup_delay_ms: 3000,
            hover_timeout_secs: 15,
            log_stderr: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hover Outcome
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome of a hover request.
///
/// Distinguishes between server not responding vs. server responding
/// but no type found. Critical for warm-up detection.
enum HoverOutcome {
    /// Server did not respond (transport error, process exited).
    NoResponse,
    /// Request timed out (enforced via reader thread).
    Timeout,
    /// Server responded. `type_name` is Some if a type was extracted.
    ServerResponded { type_name: Option<String> },
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver
// ─────────────────────────────────────────────────────────────────────────────

/// Java receiver type resolver using Eclipse JDT Language Server.
///
/// Sessions are created per-batch, per-workspace-context. The resolver
/// itself is stateless between batches.
pub struct JdtlsResolver {
    config: JdtlsConfig,
}

impl JdtlsResolver {
    pub fn new() -> Self {
        Self {
            config: JdtlsConfig::default(),
        }
    }

    pub fn with_config(config: JdtlsConfig) -> Self {
        Self { config }
    }
}

impl Default for JdtlsResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiverTypeResolver for JdtlsResolver {
    fn language(&self) -> EnrichmentLanguage {
        EnrichmentLanguage::Java
    }

    fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
        // Validate jdtls path is configured
        if self.config.jdtls_path.is_none() {
            return Err(ResolverError::ToolNotAvailable {
                tool: "jdtls: no jdtls_path configured".to_string(),
            });
        }
        Ok(())
    }

    fn resolve_batch(
        &self,
        repo_root: &Path,
        edges: &[EligibleEdge],
        progress: Option<&dyn ResolverProgress>,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> BatchResolution {
        if edges.is_empty() {
            return BatchResolution::default();
        }

        // Validate jdtls path
        let jdtls_path = match &self.config.jdtls_path {
            Some(p) => p.clone(),
            None => {
                // No jdtls configured — fail all edges
                return BatchResolution::from_results(
                    edges
                        .iter()
                        .map(|e| {
                            ReceiverTypeResult::failed(
                                e.edge_uid.clone(),
                                "jdtls not configured: set jdtls_path in config",
                            )
                        })
                        .collect(),
                );
            }
        };

        // Group edges by Java workspace context
        let groups = group_by_workspace_root(repo_root, edges);

        let mut all_results: Vec<ReceiverTypeResult> = Vec::new();
        let total = edges.len();
        let mut processed = 0;

        'groups: for (workspace_root, (context, group_edges)) in groups {
            // ENRICH-LIFECYCLE-1 batch boundary: yield to an explicit index/refresh BEFORE
            // starting a new jdtls session (never pay a fresh warm-up on cancel); the groups
            // resolved so far are returned as a partial batch (session Drop stops it).
            if cancel.is_some_and(|c| c()) {
                break 'groups;
            }
            // Report progress: starting session
            if let Some(p) = progress {
                p.report(enrichment::Progress {
                    phase: enrichment::ProgressPhase::Initializing,
                    current: processed,
                    total,
                });
            }

            // Start jdtls session for this workspace context
            let session_result =
                JdtlsSession::start(&workspace_root, &context, &jdtls_path, &self.config);

            let mut session = match session_result {
                Ok(s) => s,
                Err(e) => {
                    // Failed to start — mark all edges in this group as failed
                    warn!(
                        workspace_root = %workspace_root.display(),
                        error = %e,
                        "jdtls failed to start"
                    );
                    for edge in &group_edges {
                        all_results.push(ReceiverTypeResult::failed(
                            edge.edge_uid.clone(),
                            format!("jdtls failed to start: {}", e),
                        ));
                    }
                    processed += group_edges.len();
                    continue;
                }
            };

            // Warm up: wait for jdtls to be ready
            if let Some(p) = progress {
                p.report(enrichment::Progress {
                    phase: enrichment::ProgressPhase::LoadingProject,
                    current: processed,
                    total,
                });
            }

            let first_edge = &group_edges[0];
            let first_path = repo_root.join(&first_edge.source_file_path);
            let warmed_up =
                session.warm_up(&first_path, first_edge.line_start, first_edge.col_start);

            if !warmed_up {
                warn!(
                    workspace_root = %workspace_root.display(),
                    "jdtls did not respond after warm-up timeout"
                );
                session.stop();
                for edge in &group_edges {
                    all_results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "jdtls did not respond after loading timeout",
                    ));
                }
                processed += group_edges.len();
                continue;
            }

            // Resolve types for this group
            for edge in &group_edges {
                // Batch boundary within a warmed session: yield within one LSP request.
                if cancel.is_some_and(|c| c()) {
                    break 'groups;
                }
                if let Some(p) = progress {
                    p.report(enrichment::Progress {
                        phase: enrichment::ProgressPhase::ResolvingTypes,
                        current: processed,
                        total,
                    });
                }

                // LIMITATION: calls_this_wildcard_method_needs_type_info requires AST
                // traversal. Same limitation as TypeScript resolver.
                if edge.category == UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo {
                    all_results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "unsupported_category:calls_this_wildcard_method_needs_type_info",
                    ));
                    processed += 1;
                    continue;
                }

                let abs_path = repo_root.join(&edge.source_file_path);
                let result = session.resolve_type(
                    &abs_path,
                    edge.line_start,
                    edge.col_start,
                    &edge.edge_uid,
                );
                all_results.push(result);
                processed += 1;
            }

            // Stop session for this workspace context
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

        // Java resolution has no per-context skip concept (an unconfigured/failed jdtls marks edges
        // FAILED, above), so there are no not-attempted skips to report.
        BatchResolution::from_results(all_results)
    }

    fn shutdown(&mut self) {
        // Sessions are created and destroyed per-batch, so nothing to do here.
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session
// ─────────────────────────────────────────────────────────────────────────────

/// A single jdtls session (one per Java workspace context).
struct JdtlsSession {
    process: Child,
    stdin: ChildStdin,
    reader: ReaderHandle,
    ids: IdGenerator,
    opened_files: HashSet<String>,
    config: JdtlsConfig,
}

impl JdtlsSession {
    /// Start a jdtls session for the given workspace.
    fn start(
        workspace_root: &Path,
        context: &JavaProjectContext,
        jdtls_path: &str,
        config: &JdtlsConfig,
    ) -> Result<Self, ResolverError> {
        // Create workspace data directory if needed
        let data_dir = match &config.workspace_data_path {
            Some(p) => std::path::PathBuf::from(p),
            None => {
                // Use temp directory with unique name based on workspace
                let hash = simple_hash(workspace_root.to_string_lossy().as_ref());
                std::env::temp_dir().join(format!("jdtls-{}", hash))
            }
        };

        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir).map_err(|e| ResolverError::StartupFailed {
                reason: format!("failed to create jdtls data directory: {}", e),
            })?;
        }

        // Spawn jdtls
        // jdtls command format varies by installation. Common patterns:
        // - Direct: java -jar jdtls-launcher.jar -data <data_dir>
        // - Script: jdtls -data <data_dir>
        let mut command = Command::new(jdtls_path);
        command
            .arg("-data")
            .arg(&data_dir)
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());

        if config.log_stderr {
            command.stderr(Stdio::inherit());
        } else {
            command.stderr(Stdio::null());
        }

        let mut process = command
            .spawn()
            .map_err(|e| ResolverError::ToolNotAvailable {
                tool: format!("jdtls ({}): {}", jdtls_path, e),
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

        // Spawn reader thread with timeout support (from lsp-subprocess)
        let reader = ReaderHandle::spawn(stdout);

        let mut session = Self {
            process,
            stdin,
            reader,
            ids: IdGenerator::new(),
            opened_files: HashSet::new(),
            config: config.clone(),
        };

        // Send LSP initialize request
        session.initialize(workspace_root, context)?;

        debug!(
            workspace_root = %workspace_root.display(),
            build_system = ?context.build_system,
            "jdtls session started"
        );

        Ok(session)
    }

    /// Send LSP initialize/initialized handshake.
    fn initialize(
        &mut self,
        workspace_root: &Path,
        context: &JavaProjectContext,
    ) -> Result<(), ResolverError> {
        let root_uri = path_to_uri(workspace_root);

        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: workspace_root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "workspace".to_string()),
            }]),
            capabilities: ClientCapabilities::default(),
            initialization_options: Some(serde_json::json!({
                "bundles": [],
                "settings": {
                    "java": {
                        "import": {
                            "gradle": { "enabled": context.build_system == BuildSystem::Gradle },
                            "maven": { "enabled": context.build_system == BuildSystem::Maven }
                        }
                    }
                }
            })),
            ..Default::default()
        };

        let init_id = self.ids.next_id();
        write_request(&mut self.stdin, init_id, "initialize", init_params).map_err(|e| {
            ResolverError::StartupFailed {
                reason: format!("failed to send initialize: {}", e),
            }
        })?;

        // Wait for initialize response with real timeout enforcement
        let timeout = Duration::from_secs(self.config.init_timeout_secs);
        let response: LspResponse<serde_json::Value> = self
            .reader
            .recv_response(init_id, timeout)
            .map_err(|e| match e {
                TransportError::Timeout(d) => ResolverError::Timeout {
                    operation: format!("initialize ({}s)", d.as_secs()),
                },
                TransportError::ProcessExited => ResolverError::StartupFailed {
                    reason: "jdtls process exited during initialization".to_string(),
                },
                _ => ResolverError::StartupFailed {
                    reason: format!("failed to read initialize response: {}", e),
                },
            })?;

        if let Some(err) = response.error {
            return Err(ResolverError::StartupFailed {
                reason: format!("initialize failed: {} ({})", err.message, err.code),
            });
        }

        if response.result.is_none() {
            return Err(ResolverError::StartupFailed {
                reason: "initialize returned no result".to_string(),
            });
        }

        // Send initialized notification
        write_notification(&mut self.stdin, "initialized", InitializedParams {}).map_err(|e| {
            ResolverError::StartupFailed {
                reason: format!("failed to send initialized: {}", e),
            }
        })?;

        debug!(workspace_root = %workspace_root.display(), "jdtls initialized");

        Ok(())
    }

    /// Warm up by retrying hover until jdtls responds.
    fn warm_up(&mut self, file_path: &Path, line: u32, col: u32) -> bool {
        for attempt in 0..self.config.warmup_retries {
            debug!(attempt, "warming up jdtls");

            match self.hover_raw(file_path, line, col) {
                HoverOutcome::ServerResponded { .. } => {
                    debug!(attempt, "jdtls ready (server responded)");
                    return true;
                }
                HoverOutcome::NoResponse => {
                    debug!(attempt, "jdtls not ready, retrying");
                }
                HoverOutcome::Timeout => {
                    debug!(attempt, "jdtls hover timed out, retrying");
                }
            }

            std::thread::sleep(Duration::from_millis(self.config.warmup_delay_ms));
        }

        false
    }

    /// Send textDocument/hover request and return raw outcome.
    fn hover_raw(&mut self, file_path: &Path, line: u32, col: u32) -> HoverOutcome {
        let uri = path_to_uri(file_path);
        let uri_str = uri.to_string();

        // Open document if not already open
        if !self.opened_files.contains(&uri_str) {
            let text = match std::fs::read_to_string(file_path) {
                Ok(t) => t,
                Err(_) => return HoverOutcome::NoResponse,
            };
            let did_open_params = lsp_types::DidOpenTextDocumentParams {
                text_document: lsp_types::TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "java".to_string(),
                    version: 1,
                    text,
                },
            };
            if write_notification(&mut self.stdin, "textDocument/didOpen", did_open_params).is_err()
            {
                return HoverOutcome::NoResponse;
            }
            self.opened_files.insert(uri_str);
        }

        // Send hover request
        let hover_params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: line.saturating_sub(1), // LSP is 0-indexed
                    character: col,
                },
            },
            work_done_progress_params: Default::default(),
        };

        let hover_id = self.ids.next_id();
        if write_request(
            &mut self.stdin,
            hover_id,
            "textDocument/hover",
            hover_params,
        )
        .is_err()
        {
            return HoverOutcome::NoResponse;
        }

        // Read response with real timeout enforcement
        let timeout = Duration::from_secs(self.config.hover_timeout_secs);
        let response: Result<LspResponse<Hover>, _> = self.reader.recv_response(hover_id, timeout);

        match response {
            Ok(resp) => {
                let type_name = resp
                    .result
                    .and_then(|hover| extract_hover_text(&hover.contents))
                    .and_then(|t| extract_type_from_hover(&t));

                HoverOutcome::ServerResponded { type_name }
            }
            Err(TransportError::Timeout(_)) => HoverOutcome::Timeout,
            Err(TransportError::ProcessExited) => HoverOutcome::NoResponse,
            Err(_) => HoverOutcome::NoResponse,
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
        match self.hover_raw(file_path, line, col) {
            HoverOutcome::ServerResponded {
                type_name: Some(name),
            } => {
                if is_valid_java_type_name(&name) {
                    ReceiverTypeResult {
                        edge_uid: edge_uid.to_string(),
                        receiver_type: Some(name.clone()),
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
            HoverOutcome::ServerResponded { type_name: None } => {
                ReceiverTypeResult::failed(edge_uid.to_string(), "hover_no_type")
            }
            HoverOutcome::Timeout => {
                ReceiverTypeResult::failed(edge_uid.to_string(), "hover_timeout")
            }
            HoverOutcome::NoResponse => {
                ReceiverTypeResult::failed(edge_uid.to_string(), "hover_no_response")
            }
        }
    }

    /// Stop the jdtls process gracefully.
    fn stop(&mut self) {
        // Send shutdown request
        let shutdown_id = self.ids.next_id();
        let _ = write_request(
            &mut self.stdin,
            shutdown_id,
            "shutdown",
            serde_json::Value::Null,
        );

        // Wait briefly for shutdown response
        let _ = self
            .reader
            .recv_response::<serde_json::Value>(shutdown_id, Duration::from_secs(5));

        // Send exit notification
        let _ = write_notification(&mut self.stdin, "exit", serde_json::Value::Null);

        // Kill process if still running
        let _ = self.process.kill();
    }
}

impl Drop for JdtlsSession {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type Extraction
// ─────────────────────────────────────────────────────────────────────────────

/// Extract text from HoverContents.
fn extract_hover_text(contents: &HoverContents) -> Option<String> {
    match contents {
        HoverContents::Scalar(MarkedString::String(s)) => Some(s.clone()),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => Some(ls.value.clone()),
        HoverContents::Markup(MarkupContent { value, .. }) => Some(value.clone()),
        HoverContents::Array(arr) => {
            let text: String = arr
                .iter()
                .map(|ms| match ms {
                    MarkedString::String(s) => s.as_str(),
                    MarkedString::LanguageString(ls) => ls.value.as_str(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
    }
}

/// Extract type name from jdtls hover text.
///
/// jdtls returns Java-formatted type information in hover. Examples:
/// - "MyClass obj" → "MyClass"
/// - "String name" → "String"
/// - "List<String> items" → "List"
/// - "java.util.Map<K, V> map" → "Map"
fn extract_type_from_hover(text: &str) -> Option<String> {
    let text = text.trim();

    // Skip empty
    if text.is_empty() {
        return None;
    }

    // Try to extract type from "Type varName" pattern
    // or "package.Type varName" pattern
    let first_line = text.lines().next()?;

    // Look for type declaration pattern
    // Skip modifiers like public, private, final, etc.
    let words: Vec<&str> = first_line.split_whitespace().collect();

    for (i, word) in words.iter().enumerate() {
        // Skip Java keywords/modifiers
        if matches!(
            *word,
            "public"
                | "private"
                | "protected"
                | "static"
                | "final"
                | "abstract"
                | "synchronized"
                | "volatile"
                | "transient"
                | "native"
                | "strictfp"
        ) {
            continue;
        }

        // This might be the type
        let type_str = *word;

        // Strip generics: List<String> -> List
        let type_name = if let Some(angle_pos) = type_str.find('<') {
            &type_str[..angle_pos]
        } else {
            type_str
        };

        // Strip package prefix: java.util.List -> List
        let simple_name = type_name.rsplit('.').next().unwrap_or(type_name);

        // Skip primitives
        if matches!(
            simple_name,
            "void" | "int" | "long" | "short" | "byte" | "float" | "double" | "char" | "boolean"
        ) {
            // If there's a next word, this might be return type, keep looking
            if i + 1 < words.len() {
                continue;
            }
            return None;
        }

        // Check if it looks like a class name (starts with uppercase)
        if simple_name
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            return Some(simple_name.to_string());
        }
    }

    None
}

/// Check if a type name is valid for promotion.
fn is_valid_java_type_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Skip Java primitives and wrapper ambiguities
    if matches!(
        name,
        "void"
            | "int"
            | "long"
            | "short"
            | "byte"
            | "float"
            | "double"
            | "char"
            | "boolean"
            | "Object" // Too generic
    ) {
        return false;
    }

    // Must start with uppercase letter (Java class naming convention)
    let first = name.chars().next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }

    // Rest must be alphanumeric or underscore
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check if a type appears to be from an external package.
fn is_external_type(type_name: &str) -> bool {
    // Common Java standard library types
    const JAVA_STD_TYPES: &[&str] = &[
        "String",
        "Integer",
        "Long",
        "Double",
        "Float",
        "Boolean",
        "Byte",
        "Short",
        "Character",
        "List",
        "ArrayList",
        "LinkedList",
        "Set",
        "HashSet",
        "TreeSet",
        "Map",
        "HashMap",
        "TreeMap",
        "Optional",
        "Stream",
        "Collector",
        "Predicate",
        "Function",
        "Consumer",
        "Supplier",
        "Date",
        "Calendar",
        "LocalDate",
        "LocalTime",
        "LocalDateTime",
        "Instant",
        "Duration",
        "File",
        "Path",
        "Files",
        "InputStream",
        "OutputStream",
        "Reader",
        "Writer",
        "Thread",
        "Runnable",
        "Callable",
        "Future",
        "CompletableFuture",
        "Executor",
        "Exception",
        "RuntimeException",
        "Error",
        "Throwable",
        "Class",
        "Method",
        "Field",
        "Constructor",
        "Annotation",
    ];

    // Common framework types
    const FRAMEWORK_TYPES: &[&str] = &[
        // Spring
        "ApplicationContext",
        "BeanFactory",
        "RestTemplate",
        "WebClient",
        "JdbcTemplate",
        "TransactionTemplate",
        "MockMvc",
        // Servlet
        "HttpServletRequest",
        "HttpServletResponse",
        "ServletContext",
        // JPA/Hibernate
        "EntityManager",
        "Session",
        "SessionFactory",
        "Query",
        "Criteria",
        // Testing
        "MockitoAnnotations",
        "ArgumentCaptor",
    ];

    JAVA_STD_TYPES.contains(&type_name) || FRAMEWORK_TYPES.contains(&type_name)
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a filesystem path to an LSP URI.
fn path_to_uri(path: &Path) -> Url {
    Url::from_file_path(path).unwrap_or_else(|_| {
        Url::parse(&format!("file://{}", path.display())).expect("invalid path")
    })
}

/// Simple hash for workspace directory naming.
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_from_hover_simple() {
        assert_eq!(
            extract_type_from_hover("MyClass obj"),
            Some("MyClass".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_hover_qualified() {
        assert_eq!(
            extract_type_from_hover("java.util.List items"),
            Some("List".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_hover_generic() {
        assert_eq!(
            extract_type_from_hover("List<String> names"),
            Some("List".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_hover_with_modifiers() {
        assert_eq!(
            extract_type_from_hover("private final UserService service"),
            Some("UserService".to_string())
        );
    }

    #[test]
    fn test_extract_type_from_hover_skips_primitives() {
        assert_eq!(extract_type_from_hover("int count"), None);
        assert_eq!(extract_type_from_hover("void"), None);
    }

    #[test]
    fn test_is_valid_java_type_name() {
        assert!(is_valid_java_type_name("MyClass"));
        assert!(is_valid_java_type_name("UserService"));
        assert!(is_valid_java_type_name("MyClass_V2"));

        // Invalid
        assert!(!is_valid_java_type_name("int"));
        assert!(!is_valid_java_type_name("Object"));
        assert!(!is_valid_java_type_name(""));
        assert!(!is_valid_java_type_name("myClass")); // lowercase start
    }

    #[test]
    fn test_is_external_type() {
        assert!(is_external_type("String"));
        assert!(is_external_type("List"));
        assert!(is_external_type("ApplicationContext"));

        assert!(!is_external_type("MyClass"));
        assert!(!is_external_type("UserService"));
    }

    #[test]
    fn test_simple_hash() {
        let h1 = simple_hash("/path/to/project");
        let h2 = simple_hash("/path/to/project");
        let h3 = simple_hash("/path/to/other");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
}
