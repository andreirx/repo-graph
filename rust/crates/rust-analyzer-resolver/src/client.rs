//! rust-analyzer LSP client and ReceiverTypeResolver implementation.
//!
//! Spawns rust-analyzer as subprocess, communicates via JSON-RPC.
//! Uses a dedicated reader thread with channel timeout for true
//! timeout enforcement on blocking reads.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

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

use crate::cargo::group_by_cargo_root;
use crate::receiver_locator::{ReceiverLocation, ReceiverLocator};
use crate::transport::{
    write_notification, write_request, IdGenerator, ReaderHandle, TransportError,
};
use crate::types::{extract_type_from_hover, is_external_type, is_valid_rust_type_name};

/// Configuration for the rust-analyzer resolver.
#[derive(Debug, Clone)]
pub struct RustAnalyzerConfig {
    /// Timeout for initialization (seconds).
    pub init_timeout_secs: u64,

    /// Maximum warm-up retries before giving up.
    pub warmup_retries: u32,

    /// Delay between warm-up retries (milliseconds).
    pub warmup_delay_ms: u64,

    /// Timeout for individual hover requests (seconds).
    pub hover_timeout_secs: u64,
}

impl Default for RustAnalyzerConfig {
    fn default() -> Self {
        Self {
            init_timeout_secs: 120,
            warmup_retries: 60,
            warmup_delay_ms: 3000,
            hover_timeout_secs: 30,
        }
    }
}

/// Outcome of a hover request.
///
/// Distinguishes between server not responding vs. server responding
/// but no type found. This is critical for warm-up detection.
enum HoverOutcome {
    /// Server did not respond (transport error, process exited).
    NoResponse,
    /// Request timed out (enforced via reader thread).
    Timeout,
    /// Server responded. `type_name` is Some if a type was extracted.
    ServerResponded { type_name: Option<String> },
}

/// Rust receiver type resolver using rust-analyzer LSP.
///
/// Sessions are created per-batch, per-Cargo-context. The resolver
/// itself is stateless between batches.
pub struct RustAnalyzerResolver {
    config: RustAnalyzerConfig,
}

impl RustAnalyzerResolver {
    pub fn new() -> Self {
        Self {
            config: RustAnalyzerConfig::default(),
        }
    }

    pub fn with_config(config: RustAnalyzerConfig) -> Self {
        Self { config }
    }
}

impl Default for RustAnalyzerResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiverTypeResolver for RustAnalyzerResolver {
    fn language(&self) -> EnrichmentLanguage {
        EnrichmentLanguage::Rust
    }

    fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
        // Initialization is deferred to resolve_batch, per-Cargo-context.
        // This is because we need to start one session per Cargo.toml root.
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

        // Group edges by nearest Cargo.toml
        let groups = group_by_cargo_root(repo_root, edges);

        let mut all_results: Vec<ReceiverTypeResult> = Vec::new();
        let total = edges.len();
        let mut processed = 0;

        'groups: for (cargo_root, group_edges) in groups {
            // ENRICH-LIFECYCLE-1 batch boundary: an explicit index/refresh can ask a running
            // background pass to yield. Check BEFORE starting a new rust-analyzer session, so a
            // cancel never pays a fresh (expensive) warm-up; the already-resolved groups are
            // returned as a partial batch (the session Drop stops the current process).
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

            // Start rust-analyzer session for this Cargo context
            let session_result = RustAnalyzerSession::start(&cargo_root, &self.config);

            let mut session = match session_result {
                Ok(s) => s,
                Err(e) => {
                    // Failed to start — mark all edges in this group as failed
                    warn!(
                        cargo_root = %cargo_root.display(),
                        error = %e,
                        "rust-analyzer failed to start"
                    );
                    for edge in &group_edges {
                        all_results.push(ReceiverTypeResult::failed(
                            edge.edge_uid.clone(),
                            format!("rust-analyzer failed to start: {}", e),
                        ));
                    }
                    processed += group_edges.len();
                    continue;
                }
            };

            // Warm up: wait for rust-analyzer to be ready
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
                    cargo_root = %cargo_root.display(),
                    "rust-analyzer did not respond after warm-up timeout"
                );
                session.stop();
                for edge in &group_edges {
                    all_results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "rust-analyzer did not respond after loading timeout",
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

                let abs_path = repo_root.join(&edge.source_file_path);

                // COMPOUND receiver (`self.field.method()`): the stored position is the
                // call-expression start (the `self` token), so a direct hover would resolve `self`'s
                // type, not `self.field`'s — a false Layer-0 edge (EY1-C). Use the tree-sitter locator
                // to move the hover onto the intermediate receiver's type-bearing token. Simple
                // receivers hover `col_start` directly, unchanged (their receiver already sits there).
                //
                // NOTE: routed on SHAPE, not the wildcard CATEGORY. The indexer's categorizer
                // (`indexer::resolver::categorize_unresolved_call`, out of scope) only maps TS's
                // `this.`-prefixed keys to `CallsThisWildcardMethodNeedsTypeInfo`; Rust's `self.field`
                // keys fall through to `CallsObjMethodNeedsTypeInfo` (verified: every one of the ~140
                // self-index `self.field.method` candidates carries the obj category). Routing on the
                // wildcard category alone would therefore NEVER fire for Rust, and the gate-8 admission
                // of `self.field.method` would then promote on `self`'s type — the exact EY1-C false
                // edge. The shape predicate subsumes the wildcard category (a wildcard key is always a
                // `this.`-compound), so both paths are covered. See DECISION_REQUIRED EY3-ROUTING.
                let result = if needs_receiver_locator(edge) {
                    resolve_compound_receiver(&mut session, &abs_path, edge)
                } else {
                    session.resolve_type(&abs_path, edge.line_start, edge.col_start, &edge.edge_uid)
                };
                all_results.push(result);
                processed += 1;
            }

            // Stop session for this Cargo context
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

        // Rust resolution never skips a context (a session-start failure marks its edges FAILED,
        // not not-attempted), so there are no per-context skips to report.
        BatchResolution::from_results(all_results)
    }

    fn shutdown(&mut self) {
        // Sessions are created and destroyed per-batch, so nothing to do here.
        // Each session's Drop impl handles graceful shutdown.
    }
}

/// A single rust-analyzer session (one per Cargo context).
struct RustAnalyzerSession {
    process: Child,
    stdin: ChildStdin,
    reader: ReaderHandle,
    ids: IdGenerator,
    opened_files: HashSet<String>,
    config: RustAnalyzerConfig,
}

impl RustAnalyzerSession {
    /// Start a rust-analyzer session for the given Cargo root.
    fn start(cargo_root: &Path, config: &RustAnalyzerConfig) -> Result<Self, ResolverError> {
        // Spawn rust-analyzer
        let mut process = Command::new("rust-analyzer")
            .current_dir(cargo_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // Discard stderr
            .spawn()
            .map_err(|e| ResolverError::ToolNotAvailable {
                tool: format!("rust-analyzer: {}", e),
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
            ids: IdGenerator::new(),
            opened_files: HashSet::new(),
            config: config.clone(),
        };

        // Send initialize request
        session.initialize(cargo_root)?;

        Ok(session)
    }

    /// Send LSP initialize/initialized handshake.
    fn initialize(&mut self, cargo_root: &Path) -> Result<(), ResolverError> {
        let root_uri = path_to_uri(cargo_root);

        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            // Use workspace_folders (root_uri is deprecated)
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: cargo_root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "workspace".to_string()),
            }]),
            capabilities: ClientCapabilities::default(),
            initialization_options: Some(serde_json::json!({
                "checkOnSave": { "enable": false },
                "diagnostics": { "enable": false },
                "inlayHints": { "enable": false },
                "lens": { "enable": false },
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
        let response: crate::transport::LspResponse<serde_json::Value> = self
            .reader
            .recv_response(init_id, timeout)
            .map_err(|e| match e {
                TransportError::Timeout(d) => ResolverError::Timeout {
                    operation: format!("initialize ({}s)", d.as_secs()),
                },
                TransportError::ProcessExited => ResolverError::StartupFailed {
                    reason: "rust-analyzer process exited during initialization".to_string(),
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

        debug!(cargo_root = %cargo_root.display(), "rust-analyzer initialized");

        Ok(())
    }

    /// Warm up by retrying hover until rust-analyzer is ready.
    ///
    /// Readiness criteria (in order of preference):
    /// 1. `ServerResponded { type_name: Some(_) }` — ideal, project loaded
    /// 2. Multiple consecutive `ServerResponded { type_name: None }` after
    ///    minimum delay — RA is responding, position just has no hover data
    /// 3. Never accept NoResponse or Timeout as ready
    ///
    /// The distinction matters because during loading, RA may respond with
    /// null results before it has indexed the file. We need to wait until
    /// it's actually ready to give accurate type info.
    fn warm_up(&mut self, file_path: &Path, line: u32, col: u32) -> bool {
        // Minimum delay before accepting null responses as "ready"
        const MIN_DELAY_BEFORE_NULL_ACCEPT_MS: u64 = 15_000;
        // Consecutive null responses required to accept as ready
        const CONSECUTIVE_NULLS_FOR_READY: u32 = 3;

        let start = std::time::Instant::now();
        let mut consecutive_nulls = 0u32;

        for attempt in 0..self.config.warmup_retries {
            debug!(attempt, "warming up rust-analyzer");

            match self.hover_raw(file_path, line, col) {
                HoverOutcome::ServerResponded {
                    type_name: Some(ref t),
                } => {
                    // Got actual type info — definitely ready
                    debug!(attempt, type_name = %t, "rust-analyzer ready (got type)");
                    return true;
                }
                HoverOutcome::ServerResponded { type_name: None } => {
                    consecutive_nulls += 1;
                    let elapsed_ms = start.elapsed().as_millis() as u64;

                    // Accept null responses as ready only after:
                    // 1. Minimum delay has passed (RA had time to load)
                    // 2. We've seen several consecutive nulls (consistent behavior)
                    if elapsed_ms >= MIN_DELAY_BEFORE_NULL_ACCEPT_MS
                        && consecutive_nulls >= CONSECUTIVE_NULLS_FOR_READY
                    {
                        debug!(
                            attempt,
                            consecutive_nulls,
                            elapsed_ms,
                            "rust-analyzer ready (consistent null responses, assuming position has no hover)"
                        );
                        return true;
                    }

                    debug!(
                        attempt,
                        consecutive_nulls,
                        elapsed_ms,
                        "rust-analyzer responded with null, waiting for project to load"
                    );
                }
                HoverOutcome::NoResponse => {
                    // Reset consecutive nulls — server not even responding
                    consecutive_nulls = 0;
                    debug!(attempt, "rust-analyzer not ready (no response), retrying");
                }
                HoverOutcome::Timeout => {
                    // Reset consecutive nulls — timeout means something's off
                    consecutive_nulls = 0;
                    debug!(attempt, "rust-analyzer hover timed out, retrying");
                }
            }

            std::thread::sleep(Duration::from_millis(self.config.warmup_delay_ms));
        }

        false
    }

    /// Send textDocument/hover request and return raw outcome.
    ///
    /// Distinguishes between:
    /// - Server not responding (transport error)
    /// - Timeout (enforced via reader thread)
    /// - Server responded but no type found
    /// - Server responded with type
    fn hover_raw(&mut self, file_path: &Path, line: u32, col: u32) -> HoverOutcome {
        // Ensure absolute path for LSP URI
        let abs_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(file_path))
                .unwrap_or_else(|_| file_path.to_path_buf())
        };
        let uri = path_to_uri(&abs_path);
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
                    language_id: "rust".to_string(),
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
        let response: Result<crate::transport::LspResponse<Hover>, _> =
            self.reader.recv_response(hover_id, timeout);

        match response {
            Ok(resp) => {
                // Server responded — extract type if present
                let hover_text = resp
                    .result
                    .as_ref()
                    .and_then(|hover| extract_hover_text(&hover.contents));
                let type_name = hover_text.and_then(|t| extract_type_from_hover(&t));
                HoverOutcome::ServerResponded { type_name }
            }
            Err(TransportError::Timeout(_)) => HoverOutcome::Timeout,
            Err(TransportError::ProcessExited) => HoverOutcome::NoResponse,
            Err(_) => HoverOutcome::NoResponse,
        }
    }

    /// Resolve type at position and return ReceiverTypeResult.
    ///
    /// Reports distinct failure reasons for:
    /// - Timeout (rust-analyzer did not respond in time)
    /// - NoResponse (transport error, process crash)
    /// - ServerResponded but no type extracted
    /// - ServerResponded with invalid type name
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
                if is_valid_rust_type_name(&name) {
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

    /// Stop the rust-analyzer process gracefully.
    fn stop(&mut self) {
        // Send shutdown request
        let shutdown_id = self.ids.next_id();
        let _ = write_request(
            &mut self.stdin,
            shutdown_id,
            "shutdown",
            serde_json::Value::Null,
        );

        // Wait briefly for shutdown response (5 second timeout)
        let _ = self
            .reader
            .recv_response::<serde_json::Value>(shutdown_id, Duration::from_secs(5));

        // Send exit notification
        let _ = write_notification(&mut self.stdin, "exit", serde_json::Value::Null);

        // Kill process if still running
        let _ = self.process.kill();
    }
}

impl Drop for RustAnalyzerSession {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compound Receiver Resolution (ENRICH-YIELD-3)
// ─────────────────────────────────────────────────────────────────────────────

/// Whether an edge's receiver is COMPOUND (`self.field` / `this.field`) and therefore needs the
/// tree-sitter locator to move the hover off the stored call-expression start.
///
/// Routed on SHAPE, not the wildcard category, because the indexer categorizer only assigns the
/// wildcard category to TS `this.`-keys — Rust `self.field.method` is `CallsObjMethodNeedsTypeInfo`
/// (verified against the self-index corpus). The predicate is: the target key starts with `self.` or
/// `this.` AND has ≥2 dots (the receiver itself contains a `.`), i.e. exactly the `self.field.method`
/// / `this.field.method` shapes gate 8 admits. Simple `obj.method` / `self.method` (one dot) keep the
/// direct `col_start` hover, unchanged. The explicit wildcard-category check is belt-and-suspenders
/// (a wildcard key always satisfies the shape predicate too).
fn needs_receiver_locator(edge: &EligibleEdge) -> bool {
    edge.category == UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo
        || is_self_or_this_compound(&edge.target_key)
}

/// `self.field.method` / `this.field.method` — a `self.`/`this.` prefix with a compound (dotted)
/// receiver (≥2 dots total). NOT `self.method` (1 dot) and NOT a bare `obj.field.method`.
fn is_self_or_this_compound(target_key: &str) -> bool {
    let head = target_key.split('.').next();
    matches!(head, Some("self") | Some("this"))
        && target_key.bytes().filter(|&b| b == b'.').count() >= 2
}

/// Resolve the receiver type for a COMPOUND-receiver edge (`self.field.method()`).
///
/// The edge's stored position is the call-expression start (`self`); hovering there resolves
/// `self`'s type, not `self.field`'s — a false Layer-0 CALLS edge (EY1-C). This:
/// 1. reads the source file,
/// 2. uses the tree-sitter [`ReceiverLocator`] to find the intermediate receiver's type-bearing
///    token (the `field` of `self.field`),
/// 3. queries rust-analyzer for the type at THAT position.
///
/// Mirrors the `tsserver-resolver` seam (syntax localization + semantic typing). Failure reasons use
/// the shared `receiver_locator_*` vocabulary so the enrichment report reads the same across
/// languages. A located-but-unresolvable receiver simply yields a `failed` result (no promotion) —
/// never a guessed type.
fn resolve_compound_receiver(
    session: &mut RustAnalyzerSession,
    abs_path: &Path,
    edge: &EligibleEdge,
) -> ReceiverTypeResult {
    let source = match std::fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(e) => {
            return ReceiverTypeResult::failed(
                edge.edge_uid.clone(),
                format!("receiver_locator_read_error:{}", e),
            );
        }
    };

    let mut locator = match ReceiverLocator::new() {
        Ok(l) => l,
        Err(e) => {
            return ReceiverTypeResult::failed(
                edge.edge_uid.clone(),
                format!("receiver_locator_init_error:{}", e),
            );
        }
    };

    // Coordinates flow through unchanged: EligibleEdge is 1-based line / 0-based col, the locator
    // takes and returns the same, and resolve_type expects the same — correct-by-construction.
    match locator.locate_receiver(&source, edge.line_start, edge.col_start) {
        ReceiverLocation::Found { line, column, text } => {
            debug!(
                edge_uid = %edge.edge_uid,
                stored_pos = %format!("{}:{}", edge.line_start, edge.col_start),
                receiver_pos = %format!("{}:{}", line, column),
                receiver_text = %text,
                "located Rust intermediate receiver; querying its type"
            );
            session.resolve_type(abs_path, line, column, &edge.edge_uid)
        }
        ReceiverLocation::NoReceiver => {
            ReceiverTypeResult::failed(edge.edge_uid.clone(), "receiver_locator_no_receiver")
        }
        ReceiverLocation::UnsupportedPattern { reason } => ReceiverTypeResult::failed(
            edge.edge_uid.clone(),
            format!("receiver_locator_unsupported:{}", reason),
        ),
        ReceiverLocation::ParseError { reason } => ReceiverTypeResult::failed(
            edge.edge_uid.clone(),
            format!("receiver_locator_parse_error:{}", reason),
        ),
    }
}

/// Convert a filesystem path to an LSP URI.
fn path_to_uri(path: &Path) -> Url {
    Url::from_file_path(path).unwrap_or_else(|_| {
        // Fallback for paths that can't be converted
        Url::parse(&format!("file://{}", path.display())).expect("invalid path")
    })
}

/// Extract text from HoverContents.
fn extract_hover_text(contents: &HoverContents) -> Option<String> {
    match contents {
        HoverContents::Scalar(MarkedString::String(s)) => Some(s.clone()),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => Some(ls.value.clone()),
        HoverContents::Markup(MarkupContent { value, .. }) => Some(value.clone()),
        HoverContents::Array(arr) => {
            // Concatenate all strings
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_uri() {
        let path = Path::new("/home/user/project/src/main.rs");
        let uri = path_to_uri(path);
        assert!(uri.scheme() == "file");
        assert!(uri.path().ends_with("main.rs"));
    }

    #[test]
    fn test_extract_hover_text_markup() {
        let contents = HoverContents::Markup(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: "```rust\nfn foo() -> i32\n```".to_string(),
        });
        let text = extract_hover_text(&contents);
        assert!(text.is_some());
        assert!(text.unwrap().contains("fn foo"));
    }

    // ── ENRICH-YIELD-3 routing (EY3-ROUTING) ─────────────────────────────────────────────────────

    fn edge(target_key: &str, category: UnresolvedCategory) -> EligibleEdge {
        EligibleEdge {
            edge_uid: "e".to_string(),
            snapshot_uid: "s".to_string(),
            repo_uid: "r".to_string(),
            source_node_uid: "n".to_string(),
            target_key: target_key.to_string(),
            source_file_path: "src/lib.rs".to_string(),
            line_start: 1,
            col_start: 0,
            category,
            language: EnrichmentLanguage::Rust,
        }
    }

    // THE critical routing fact: Rust `self.field.method` carries the OBJ category (the indexer
    // categorizer only recognizes TS `this.`), yet it MUST go through the receiver locator — otherwise
    // the gate-8 admission promotes on `self`'s type (the EY1-C false edge). Shape routing guarantees
    // it does.
    #[test]
    fn self_field_method_obj_category_still_routes_to_locator() {
        assert!(
            needs_receiver_locator(&edge(
                "self.field.method",
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo
            )),
            "Rust self.field.method (obj category) must route to the locator via its SHAPE"
        );
    }

    #[test]
    fn simple_receivers_keep_the_direct_hover() {
        // One dot → the receiver IS at col_start; no locator, no behavior change.
        assert!(!needs_receiver_locator(&edge(
            "obj.method",
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo
        )));
        assert!(!needs_receiver_locator(&edge(
            "self.method",
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo
        )));
    }

    #[test]
    fn wildcard_category_routes_to_locator_belt_and_suspenders() {
        // A wildcard-category edge (TS parity path) routes to the locator even though its shape also
        // satisfies the predicate.
        assert!(needs_receiver_locator(&edge(
            "this.field.method",
            UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo
        )));
    }

    #[test]
    fn is_self_or_this_compound_shape_matrix() {
        assert!(is_self_or_this_compound("self.field.method"));
        assert!(is_self_or_this_compound("this.field.method"));
        assert!(is_self_or_this_compound("self.a.b.method")); // deep self chain (gate 8 rejects later)
        assert!(!is_self_or_this_compound("self.method")); // simple self receiver
        assert!(!is_self_or_this_compound("obj.method")); // simple obj receiver
        assert!(!is_self_or_this_compound("a.b.c.method")); // compound but not self/this
        assert!(!is_self_or_this_compound("method")); // no receiver
    }
}
