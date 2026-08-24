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

use crate::locate::locate_tsserver;
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
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Vec<ReceiverTypeResult> {
        if edges.is_empty() {
            return Vec::new();
        }

        // tsserver associates files with a project by ABSOLUTE path — an `open` with a relative path
        // lands the file in no project, so a later `quickinfo` throws "Cannot read properties of
        // undefined (reading 'getSourceFile')". The stored repo root is RELATIVE (pathdiff from the DB
        // dir), which is fine for a configured project (its files load from the tsconfig up front) but
        // breaks tsserver's INFERRED project (loose JS/JSX with no tsconfig), which only knows the files
        // it is explicitly `open`ed with. Canonicalize the repo root once here so every derived path
        // (warm-up file, per-edge file, and the session `current_dir`) is absolute. Falls back to the
        // raw path if it cannot be resolved (then the downstream ops degrade honestly, not panic).
        // This is a genuine bug fix, not a JS-only change: the relative-root defect ALSO cost
        // configured TS projects real resolutions when tsserver needed the absolute path (measured:
        // a configured-TS receiver goes 0→1 after this — a compiler fact the bug was dropping, now
        // recovered — NOT fabricated). So it improves TS too; it is load-bearing for the JS
        // inferred-project path (JS-ENRICHMENT-1) and correctness-restoring for TS. (The earlier
        // "byte-neutral for TS" claim here was FALSE and is corrected — the measured 0→1 disproves it.)
        let repo_root_abs = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let repo_root = repo_root_abs.as_path();

        // Build ownership resolver to determine which tsconfig owns each file.
        // This replaces the naive "nearest config by directory" grouping.
        let ownership_resolver = match crate::ownership::TsProjectOwnershipResolver::build(
            repo_root,
        ) {
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

        let total = edges.len();
        let processed = ownership_failures.len();
        let mut all_results: Vec<ReceiverTypeResult> = ownership_failures;

        // The ONE shared tsserver locator (TSSERVER-LOCATE-1 §2.1): the resolver and the enrich-pass
        // probe resolve tsserver identically, so the "skipped" verdict and the session start can never
        // disagree about where it is. Per-context — walks UP from each project context to `repo_root`
        // (nearest `node_modules/.bin/tsserver` wins), then config path, then `$PATH`.
        let cfg_path = self.config.tsserver_path.clone();
        let locate = |ctx: &Path| locate_tsserver(ctx, repo_root, cfg_path.as_deref());

        let group_results = self.resolve_groups(
            repo_root, groups, &locate, total, processed, progress, cancel,
        );
        all_results.extend(group_results);

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

impl TsServerResolver {
    /// Resolve per-project-context edge groups, one tsserver session per context.
    ///
    /// `locate` is the injected tsserver locator: production passes the ONE shared [`locate_tsserver`]
    /// (partially applied over `repo_root` + the config path); the mixed-availability unit test injects a
    /// deterministic per-context locator. A context whose `locate` returns `None` is SKIPPED — no session
    /// starts and its edges get NO result (TSSERVER-LOCATE-1 §2.2: honest "not attempted", byte-identical
    /// to the historical whole-pass skip, never the fabricated `tsserver failed to start` failure it used
    /// to record). This is the ENFORCEMENT of the per-context availability the enrich-pass probe NAMES
    /// with the same locator; the two can never disagree because they share the locator.
    ///
    /// Abstraction ledger — **What:** the per-context session loop with the tsserver locator injected as
    /// a closure. **Concrete current users:** [`resolve_batch`](TsServerResolver::resolve_batch)
    /// (production; injects the real `locate_tsserver`) + the `mixed_availability_*` unit test (injects a
    /// deterministic per-context locator). **Axis:** per-context tsserver presence — real filesystem/`$PATH`
    /// in production, controlled in the operator-mandated mixed-availability proof. **Rejected simpler
    /// alternative:** call `locate_tsserver` inline in `resolve_batch`'s loop — then the mandated mixed
    /// test cannot be hermetic (a host with tsserver on `$PATH` flips the "skipped" context to available)
    /// and cannot distinguish enter-vs-skip without a live LSP. (Same seam pattern as `locate_tsserver_with`.)
    #[allow(clippy::too_many_arguments)]
    fn resolve_groups(
        &self,
        repo_root: &Path,
        groups: HashMap<PathBuf, Vec<EligibleEdge>>,
        locate: &dyn Fn(&Path) -> Option<String>,
        total: usize,
        mut processed: usize,
        progress: Option<&dyn ResolverProgress>,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Vec<ReceiverTypeResult> {
        let mut results: Vec<ReceiverTypeResult> = Vec::new();

        'groups: for (project_root, group_edges) in groups {
            // ENRICH-LIFECYCLE-1 batch boundary: yield to an explicit index/refresh BEFORE
            // starting a new tsserver session (never pay a fresh warm-up on cancel); the
            // groups resolved so far are returned as a partial batch (session Drop stops it).
            if cancel.is_some_and(|c| c()) {
                break 'groups;
            }

            // TSSERVER-LOCATE-1 §2.2 — per-context ENFORCEMENT. A context with no tsserver is SKIPPED
            // here (the enrich-pass probe already NAMED it via this same locator): no session starts and
            // the group's edges get NO result — honest "not attempted", never the `tsserver failed to
            // start` failure this path used to record. Placed before the progress report so a skipped
            // context emits no spurious "Initializing".
            let Some(tsserver_cmd) = locate(&project_root) else {
                debug!(
                    project_root = %project_root.display(),
                    "no tsserver for this project context — skipping (not failing)"
                );
                processed += group_edges.len();
                continue;
            };

            // Report progress: starting session
            if let Some(p) = progress {
                p.report(enrichment::Progress {
                    phase: enrichment::ProgressPhase::Initializing,
                    current: processed,
                    total,
                });
            }

            // Start tsserver session for this project context (with the located command).
            let session_result = TsServerSession::start(&project_root, &self.config, &tsserver_cmd);

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
                        results.push(ReceiverTypeResult::failed(
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
            let warmed_up =
                session.warm_up(&first_path, first_edge.line_start, first_edge.col_start);

            if !warmed_up {
                warn!(
                    project_root = %project_root.display(),
                    "tsserver did not respond after warm-up timeout"
                );
                session.stop();
                for edge in &group_edges {
                    results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "tsserver did not respond after loading timeout",
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
                    results.push(result);
                    processed += 1;
                    continue;
                }

                let result = session.resolve_type(
                    &abs_path,
                    edge.line_start,
                    edge.col_start,
                    &edge.edge_uid,
                );
                results.push(result);
                processed += 1;
            }

            // Stop session for this project context
            session.stop();
        }

        results
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
    /// Start a tsserver session for the given project context.
    ///
    /// `tsserver_cmd` was resolved by the caller via the ONE shared [`locate_tsserver`] (TSSERVER-LOCATE-1
    /// §2.1) — the resolver and the enrich-pass probe agree on where tsserver is. A context with no
    /// tsserver is SKIPPED by the caller (`resolve_groups`, §2.2) BEFORE this is reached, so `start` is
    /// only ever called with a located command.
    fn start(
        project_root: &Path,
        config: &TsServerConfig,
        tsserver_cmd: &str,
    ) -> Result<Self, ResolverError> {
        // Spawn tsserver
        // Note: tsserver wants to be run from the project root to find tsconfig.json
        let mut command = Command::new(tsserver_cmd);
        command
            .current_dir(project_root)
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
            QuickInfoOutcome::ServerResponded {
                type_name: Some(name),
            } => {
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
        "string"
            | "number"
            | "boolean"
            | "null"
            | "undefined"
            | "void"
            | "any"
            | "unknown"
            | "never"
            | "object"
            | "symbol"
            | "bigint"
            | "String"
            | "Number"
            | "Boolean"
            | "Object"
            | "Symbol"
            | "BigInt"
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
        "Buffer",
        "Stream",
        "Readable",
        "Writable",
        "Transform",
        "EventEmitter",
        "ChildProcess",
        "Server",
        "Socket",
        "IncomingMessage",
        "ServerResponse",
        "Agent",
    ];

    // Common library types
    const LIBRARY_TYPES: &[&str] = &[
        // RxJS
        "Observable",
        "Subject",
        "BehaviorSubject",
        "ReplaySubject",
        "Subscription",
        "Subscriber",
        "Observer",
        // Express
        "Express",
        "Router",
        "NextFunction",
        // React
        "Component",
        "PureComponent",
        "ReactNode",
        "ReactElement",
        "RefObject",
        "MutableRefObject",
        // Angular
        "NgModule",
        "Injector",
        "ComponentRef",
        // Database
        "Connection",
        "Repository",
        "QueryBuilder",
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
            ReceiverTypeResult::failed(edge_uid.to_string(), "receiver_locator_no_receiver")
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
/// - Vec of failures for edges with Unowned (TS) or Ambiguous ownership
///
/// # JS-ENRICHMENT-1: the package.json Node-context fallback for Unowned JavaScript
///
/// A JavaScript-family file (`.js`/`.jsx`/`.mjs`/`.cjs`) is legitimately part of a Node project that
/// declares itself with `package.json` and carries NO `tsconfig.json`/`jsconfig.json` (e.g. a Vite +
/// React app). tsconfig ownership therefore reports such a file `Unowned` — but tsserver CAN resolve
/// it: an opened loose `.js`/`.jsx` file lands in tsserver's inferred project, which enables `allowJs`
/// and resolves imported library types from `node_modules` exactly as an editor does. That is a real
/// TypeScript language-service fact, not a fabricated or synthesized context (we write no config).
///
/// So a JS-family `Unowned` edge is routed to its nearest `package.json`/repo-root context via the
/// existing directory-based [`group_by_project_root`] (which already treats `package.json` as a project
/// boundary) instead of being failed. tsserver's inferred project then resolves it or honestly fails —
/// never fabricates. TypeScript `Unowned` edges keep the explicit failure: a `.ts` file with no owning
/// tsconfig is a config gap the reader should close, and TS enrichment output stays byte-unchanged
/// (byte-parity — the JS branch never runs for a TS-only repo). `Owned`/`Ambiguous` are unchanged; the
/// merged groups still start exactly one tsserver session per project root (no session thrash).
fn group_by_ownership(
    resolver: &crate::ownership::TsProjectOwnershipResolver,
    repo_root: &Path,
    edges: &[EligibleEdge],
) -> (HashMap<PathBuf, Vec<EligibleEdge>>, Vec<ReceiverTypeResult>) {
    let mut groups: HashMap<PathBuf, Vec<EligibleEdge>> = HashMap::new();
    let mut failures: Vec<ReceiverTypeResult> = Vec::new();
    // JS-family files that no tsconfig owns → resolved via their package.json Node context below.
    let mut js_unowned: Vec<EligibleEdge> = Vec::new();

    for edge in edges {
        let file_path = Path::new(&edge.source_file_path);

        match resolver.resolve(file_path) {
            crate::ownership::ProjectOwnership::Owned { project_root, .. } => {
                groups.entry(project_root).or_default().push(edge.clone());
            }
            crate::ownership::ProjectOwnership::Unowned => {
                if is_js_family(&edge.source_file_path) {
                    // No tsconfig owns this JS/JSX file → fall back to its package.json Node context
                    // (tsserver inferred project, allowJs). Batched + grouped after the loop.
                    js_unowned.push(edge.clone());
                } else {
                    // A TypeScript file with no owning tsconfig — explicit failure (byte-parity).
                    failures.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "ts_project_ownership_not_found",
                    ));
                }
            }
            crate::ownership::ProjectOwnership::Ambiguous { candidates } => {
                // Multiple tsconfigs claim ownership — explicit failure
                let candidate_list: Vec<String> =
                    candidates.iter().map(|p| p.display().to_string()).collect();
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

    // Route the JS-family Unowned edges to their nearest package.json/repo-root context and merge into
    // the ownership groups (an Owned tsconfig dir that coincides with a package.json dir shares the one
    // session for that root — no thrash).
    if !js_unowned.is_empty() {
        for (project_root, edges) in group_by_project_root(repo_root, &js_unowned) {
            groups.entry(project_root).or_default().extend(edges);
        }
    }

    (groups, failures)
}

/// Whether a repo-relative path is a JavaScript-family source file (`.js`/`.jsx`/`.mjs`/`.cjs`).
///
/// Used only by [`group_by_ownership`] to scope the package.json Node-context fallback to JavaScript
/// (JS-ENRICHMENT-1): TypeScript files that no tsconfig owns keep their explicit failure. Extension
/// match on the source path — the same extensions the extractor classifies as `javascript`/`jsx` and
/// that `EnrichmentLanguage::from_extension` folds into the TypeScript resolver.
fn is_js_family(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

// tsserver location lives in `locate.rs` (`locate_tsserver`), the ONE locator shared with the
// enrich-pass probe (TSSERVER-LOCATE-1). `resolve_batch` applies it per project context and SKIPS a
// context with none (§2.2) — so the resolver no longer has a `find_tsserver` that erroring-out marked a
// whole group failed; a missing tsserver is a skip, decided by the caller before `TsServerSession::start`.

#[cfg(test)]
mod tests {
    use super::*;

    /// TSSERVER-LOCATE-1 §2.2 (operator ruling 2026-08-24, review-0 #2/#3) — per-context availability is
    /// ENFORCED, not only rendered. Two project contexts: one whose locator resolves a tsserver and one
    /// whose locator returns `None`. The available context is ENTERED (a session start is attempted); the
    /// missing context is SKIPPED — it produces NO result at all, NOT a `tsserver failed to start`
    /// failure. This is the enforcement the resolver previously lacked (it started a session for every
    /// group and failed the ones with no tsserver).
    ///
    /// Hermetic: the locator is injected (no host `$PATH` dependence, no live LSP). The "available"
    /// context's command is a binary that does not exist, so entering it fails at spawn — that FAILED
    /// result is precisely the proof the context WAS entered. (The live "an available context actually
    /// enriches" half is the glamCRM live-lift in the slice's §4, which needs a real tsserver.)
    #[test]
    fn mixed_availability_enters_the_available_context_and_skips_the_missing_one() {
        use std::collections::HashMap;
        use tempfile::TempDir;

        fn edge(uid: &str, file: &str) -> EligibleEdge {
            EligibleEdge {
                edge_uid: uid.to_string(),
                snapshot_uid: "snap".to_string(),
                repo_uid: "repo".to_string(),
                source_node_uid: "node".to_string(),
                target_key: "obj.method".to_string(),
                source_file_path: file.to_string(),
                line_start: 1,
                col_start: 1,
                category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
                language: EnrichmentLanguage::TypeScript,
            }
        }

        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        let pkg_avail = repo_root.join("pkg_avail");
        let pkg_missing = repo_root.join("pkg_missing");
        std::fs::create_dir_all(&pkg_avail).unwrap();
        std::fs::create_dir_all(&pkg_missing).unwrap();

        let mut groups: HashMap<PathBuf, Vec<EligibleEdge>> = HashMap::new();
        groups.insert(
            pkg_avail.clone(),
            vec![edge("edge-avail", "pkg_avail/a.ts")],
        );
        groups.insert(
            pkg_missing.clone(),
            vec![edge("edge-missing", "pkg_missing/b.ts")],
        );

        // Injected locator: the missing context → None (→ skip); the available one → a command that
        // does not exist (→ entered, then spawn fails, which is what proves it was entered).
        let bogus = "rmap-tsserver-does-not-exist-xyz";
        let locate = |ctx: &Path| {
            if ctx == pkg_missing.as_path() {
                None
            } else {
                Some(bogus.to_string())
            }
        };

        let resolver = TsServerResolver::new();
        let results =
            resolver.resolve_groups(repo_root, groups, &locate, 2, 0, None, Some(&|| false));

        // The missing context is SKIPPED — no result of any kind for its edge.
        assert!(
            !results.iter().any(|r| r.edge_uid == "edge-missing"),
            "the context whose locator returned None must be SKIPPED — no result, not a failure: {:?}",
            results.iter().map(|r| &r.edge_uid).collect::<Vec<_>>()
        );
        // The available context WAS entered — its edge has a result (a failed one here, because the
        // injected command does not exist; the presence of the result is the proof of entry).
        let avail = results.iter().find(|r| r.edge_uid == "edge-avail");
        assert!(
            avail.is_some(),
            "the context whose locator resolved a tsserver must be ENTERED (a session start attempted)"
        );
        assert!(
            !avail.unwrap().is_success(),
            "with a non-existent injected tsserver the entered context fails at spawn (hermetic proxy for entry)"
        );
    }

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
        assert_eq!(extract_type_from_display_string("(local var) x: any"), None);
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

    // ── JS-ENRICHMENT-1: the package.json Node-context fallback for Unowned JavaScript ──────────────

    #[test]
    fn is_js_family_matches_js_extensions_only() {
        for p in ["a.js", "a.jsx", "a.mjs", "a.cjs", "dir/App.JSX", "x/y/z.Js"] {
            assert!(is_js_family(p), "{p} must be JS-family");
        }
        for p in [
            "a.ts", "a.tsx", "a.mts", "a.cts", "a.rs", "a.py", "noext", "a.json",
        ] {
            assert!(!is_js_family(p), "{p} must NOT be JS-family");
        }
    }

    /// The load-bearing behavior change: in a Node project with a `package.json` but NO
    /// tsconfig/jsconfig (a Vite/React app — the glam frontend shape), an Unowned `.jsx`/`.js` file is
    /// routed to its package.json context for tsserver (inferred project, allowJs) instead of being
    /// failed, while an Unowned `.ts` file keeps its explicit `ts_project_ownership_not_found` failure
    /// (TS byte-parity). Drives the REAL `group_by_ownership` + ownership resolver over a temp dir.
    #[test]
    fn js_unowned_routes_to_package_json_context_ts_stays_failed() {
        use enrichment::UnresolvedCategory;

        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        // Node project: package.json only, NO tsconfig/jsconfig → every file is Unowned by tsconfig.
        std::fs::write(root.join("package.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/App.jsx"), "export const a = 1;\n").unwrap();
        std::fs::write(root.join("src/util.js"), "export const b = 2;\n").unwrap();
        std::fs::write(root.join("src/main.ts"), "export const c = 3;\n").unwrap();

        let resolver = crate::ownership::TsProjectOwnershipResolver::build(root).unwrap();

        let mk = |uid: &str, path: &str| EligibleEdge {
            edge_uid: uid.to_string(),
            snapshot_uid: "s".to_string(),
            repo_uid: "r".to_string(),
            source_node_uid: "n".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: path.to_string(),
            line_start: 1,
            col_start: 1,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: EnrichmentLanguage::TypeScript, // JS folds to TypeScript for the resolver
        };
        let edges = vec![
            mk("jsx", "src/App.jsx"),
            mk("js", "src/util.js"),
            mk("ts", "src/main.ts"),
        ];

        let (groups, failures) = group_by_ownership(&resolver, root, &edges);

        let grouped: Vec<&str> = groups
            .values()
            .flatten()
            .map(|e| e.edge_uid.as_str())
            .collect();
        // JS-family Unowned edges reach tsserver (grouped under a project context), not failed.
        assert!(
            grouped.contains(&"jsx") && grouped.contains(&"js"),
            "JS-family Unowned edges are routed to a Node context for tsserver: {grouped:?}"
        );
        // The one project root is the package.json dir (the repo root here).
        assert!(
            groups.contains_key(root),
            "JS edges grouped under the package.json/repo-root context"
        );
        // The TS Unowned edge keeps its explicit failure (byte-parity: TS behavior unchanged).
        assert!(
            failures.iter().any(|f| f.edge_uid == "ts"
                && f.failure_reason.as_deref() == Some("ts_project_ownership_not_found")),
            "TS Unowned edge keeps ts_project_ownership_not_found"
        );
        assert!(
            !grouped.contains(&"ts"),
            "TS Unowned edge is NOT sent to tsserver"
        );
        assert!(
            !failures
                .iter()
                .any(|f| f.edge_uid == "jsx" || f.edge_uid == "js"),
            "no JS-family edge is failed by ownership anymore"
        );
    }

    /// Byte-parity guard: when a tsconfig DOES own the files, the JS fallback never triggers — every
    /// edge is grouped by ownership exactly as before, and there are no ownership failures.
    #[test]
    fn owned_files_are_unaffected_by_the_js_fallback() {
        use enrichment::UnresolvedCategory;

        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("tsconfig.json"), r#"{"include":["src/**/*"]}"#).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/App.jsx"), "export const a = 1;\n").unwrap();
        std::fs::write(root.join("src/main.ts"), "export const c = 3;\n").unwrap();

        let resolver = crate::ownership::TsProjectOwnershipResolver::build(root).unwrap();
        let mk = |uid: &str, path: &str| EligibleEdge {
            edge_uid: uid.to_string(),
            snapshot_uid: "s".to_string(),
            repo_uid: "r".to_string(),
            source_node_uid: "n".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: path.to_string(),
            line_start: 1,
            col_start: 1,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: EnrichmentLanguage::TypeScript,
        };
        let edges = vec![mk("jsx", "src/App.jsx"), mk("ts", "src/main.ts")];
        let (groups, failures) = group_by_ownership(&resolver, root, &edges);

        let grouped: Vec<&str> = groups
            .values()
            .flatten()
            .map(|e| e.edge_uid.as_str())
            .collect();
        assert!(
            grouped.contains(&"jsx") && grouped.contains(&"ts"),
            "tsconfig-owned files are grouped by ownership as before: {grouped:?}"
        );
        assert!(
            failures.is_empty(),
            "no ownership failures when a tsconfig owns the files"
        );
    }
}
