//! ENRICH-LIFECYCLE-1: the automatic background enrichment pass.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the daemon's automatic, background enrichment pass — the LIFECYCLE the shipped
//!   enrichment pipeline (`enrichment::EnrichmentPipeline`) never had. After a successful
//!   index/refresh the daemon spawns this pass; it detects which resolver toolchains are present
//!   (rust-analyzer / tsserver / jdtls — the shipped resolvers only), runs the enrichment pipeline
//!   (WITH promotion, so the call-graph actually upgrades) for the languages whose toolchain is
//!   present, and records an honest per-language SKIP (with a reader-frame install next-action) for
//!   the languages whose toolchain is absent.
//! - **Concrete current users:** `dispatch::ServiceDispatcher::finish_write_with_maintenance` (called
//!   on every index/refresh success), via [`spawn_auto_enrich`]. The gate + pass core
//!   ([`try_enrich_attempt`] / [`run_enrich_pass`]) and the pure planner ([`plan_languages`]) are
//!   also driven directly by the named auto-trigger / supersede / toolchain-skip / opt-out /
//!   contention tests — the headless Test API seam.
//! - **Named axis of variation:** none beyond the op that triggers it. This is a cohesion split from
//!   `dispatch.rs` (far over the 500-line structural guardrail), mirroring the `retention_pass`
//!   precedent — NOT a variation seam. There is exactly one enrichment pass; the per-language
//!   resolver choice is data (the `EnrichmentLanguage` match), not a plugin axis.
//! - **Rejected simpler alternative:** reuse `dispatch::handle_enrich`. Rejected: that handler is
//!   request-shaped (takes a `Request` + `ProgressEmitter`, returns a `DispatchResult`, and treats
//!   "no resolver available" as a hard ERROR) — the background pass has no client, must treat a
//!   missing toolchain as an honest SKIP (never an error), and needs the two-gate contention +
//!   generation-supersede discipline. Sharing the pipeline INVOCATION (open storage → build registry
//!   → `EnrichmentPipeline::run`) is the reuse; the request framing is not.
//!
//! # Contention safety — reused verbatim from the retention pass
//!
//! Before touching the DB the pass checks BOTH gates the ratified retention pass uses
//! ([`crate::retention_pass::try_retention_attempt`]):
//!
//! 1. **Activity registry clear for OTHER ops** — `state.activity().active_for_db(db)` finds any
//!    in-flight index/refresh/enrich/retention. Checked BEFORE the pass stamps its own `Enrich` op,
//!    so it sees only OTHER ops.
//! 2. **Non-blocking DB write lock** — `try_acquire_write()` on the same `DatabaseState` lock an
//!    index takes. Held for the whole pass, so an index that starts mid-pass blocks on
//!    `acquire_write()` until the pass ends.
//!
//! Because the triggering index still holds its own write lock + activity stamp when the pass is
//! spawned, the pass's first attempt naturally YIELDS (bounded sleep-and-retry) until that index
//! finishes, then runs on a later attempt — explicit user ops always win the START of the pass.
//!
//! # Running-yield: batch-boundary cancellation (slice §3.4, ratified 2026-07-06)
//!
//! Explicit user writes always win — INCLUDING against a RUNNING enrichment. The pass, once it holds
//! the write lock, no longer runs unconditionally to completion: it polls a [`CancelFlag`]
//! ([`EnrichCoordinator::register_running`]) at BATCH BOUNDARIES and yields when an explicit
//! index/refresh latches it. The signal wiring, and why it must be an explicit flag rather than the
//! activity registry:
//!
//! - **The latch:** `dispatch::handle_index`/`handle_refresh` call
//!   [`EnrichCoordinator::request_yield_for_db`] BEFORE they take the DB write lock. This is
//!   required because [OBSERVED] the index handler `acquire_write()`s FIRST and stamps its activity
//!   op only after — so a lock-holding pass polling the activity registry would never see the
//!   blocked index. The flag closes that chicken-and-egg.
//! - **The acquire→register window (review-1 fix):** [`try_enrich_attempt`] takes the DB write lock
//!   BEFORE it calls [`EnrichCoordinator::register_running`] (there is real work between — run slot,
//!   generation re-check, repo load, refresh lock, activity stamp). An explicit write whose
//!   `request_yield_for_db` lands in THAT window finds no flag to latch — and the pass is already
//!   holding the write lock the explicit write is about to block on. A lost no-op there would let the
//!   pass run to completion while the explicit write blocked, violating "explicit writes always win".
//!   So `request_yield_for_db` records a PENDING marker when it finds no flag; `register_running`
//!   ADOPTS the marker (the pass starts already-cancelled and yields at its first batch boundary); and
//!   the explicit write drops the marker via [`EnrichCoordinator::clear_pending_yield`] once it owns
//!   the lock, so a marker left by a write that had nothing to cancel never makes the next pass yield
//!   spuriously. The signal is persistent because the index sends it exactly ONCE, before blocking —
//!   a non-persistent latch is unavoidably lost if it precedes registration.
//! - **The batch boundaries** (`pipeline::run_cancellable` + the `resolve_batch` cancel param): the
//!   pipeline checks the flag BETWEEN languages; each resolver checks it BEFORE starting a new
//!   per-project LSP session (so a cancel never pays a fresh warm-up) and BEFORE each per-edge
//!   resolve within a warmed session. No mid-LSP-request abort is needed or attempted.
//! - **On cancel:** the current batch's already-resolved edges are persisted (a complete additive
//!   fact — `persist_enrichments` is an additive `metadata_json` UPDATE; readers never see torn
//!   state), promotion is SKIPPED, and [`try_enrich_attempt`] returns [`EnrichAttempt::Yielded`] so
//!   `run_auto_enrich` requeues. The incoming index proceeds (its blocked `acquire_write()` unblocks
//!   the instant the pass drops the guard) and its own completion supersedes this repo's requeued
//!   pass (the generation bump), re-enriching the fresh snapshot.
//! - **Yield latency** is bounded by the in-flight LSP interaction + graceful session teardown: a
//!   handful of seconds when mid per-edge resolution; worst case one crate's warm-up (tens of
//!   seconds) if the flag latches during a fresh session's warm-up — the un-abortable-LSP-request
//!   floor the ratified "no mid-LSP-request abort" accepts. Each requeue is one bounded batch.
//!
//! # One-at-a-time + supersede (slice §3.1)
//!
//! "One background enrichment at a time per daemon" is the [`EnrichCoordinator::run_slot`] mutex
//! (rust-analyzer/tsserver spin up heavyweight LSP sessions; two at once would thrash). "A newer
//! trigger for the same repo supersedes a queued (not-yet-started) older one" is the per-repo
//! [`EnrichCoordinator::generations`] counter: each trigger bumps it; a spawned pass captures its
//! generation and exits [`EnrichAttempt::Superseded`] once a newer trigger has bumped past it.
//!
//! # Detached completion (INDEX-DISCONNECT-1 principle)
//!
//! The pass has no client (spawned AFTER the index response was sent), so it is inherently detached:
//! it runs to completion regardless of any client, records its outcome for `rmap doctor`
//! ([`crate::state::DaemonState::record_enrichment_report`]), logs one reader-frame line, and then
//! chains the retention pass (the retention slice's "after enrichment promotion" hook — see
//! [`run_auto_enrich`]).
//!
//! # References
//! - `docs/slices/enrich-lifecycle-1.md`
//! - `docs/slices/snapshot-retention-1.md` (the actor pattern + the retention chain-point)
//! - `docs/slices/daemon-visibility-1.md` (§2 — the two-gate contention this reuses)

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use enrichment::{
    EligibilityQuery, EligibleEdge, EnrichmentConfig, EnrichmentLanguage, EnrichmentPipeline,
    EnrichmentStoragePort, PromotionFunnel, ResolverRegistry,
};
use jdtls_resolver::{JdtlsConfig, JdtlsResolver};
use parking_lot::{Mutex, MutexGuard};
use repo_graph_storage::StorageConnection;
use rust_analyzer_resolver::RustAnalyzerResolver;
use tsserver_resolver::{group_by_project_root, locate_tsserver, TsServerResolver};

use crate::state::DaemonState;

/// Bounded sleep-and-retry loop parameters (identical to the retention pass): steady-state passes
/// wait out a concurrent index/refresh, then run. ~60 attempts × ~1s ≈ up to a minute of waiting for
/// a busy DB before deferring — the next successful index/refresh requeues a fresh pass anyway.
const REQUEUE_MAX_ATTEMPTS: u32 = 60;
const REQUEUE_BACKOFF: Duration = Duration::from_millis(1000);

// ─────────────────────────────────────────────────────────────────────────────
// Opt-out switch (slice §3.3) — RMAP_AUTO_ENRICH, default ON
// ─────────────────────────────────────────────────────────────────────────────

/// Opt-out switch for the automatic background enrichment pass (default ON — the ratified posture is
/// enrich-by-default). Consistent with the daemon's established env-var config precedent
/// (`RMAP_AUTO_RETENTION`, `RMAP_STATE_ROOT`, `RMAP_PERF`): set `RMAP_AUTO_ENRICH` to
/// `0`/`false`/`off`/`no`/`disabled` (case-insensitive) to disable. Any other value — or unset —
/// leaves it ON.
pub fn auto_enrich_enabled() -> bool {
    match AUTO_ENRICH_OVERRIDE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => auto_enrich_enabled_from(std::env::var("RMAP_AUTO_ENRICH").ok().as_deref()),
    }
}

/// Test override for [`auto_enrich_enabled`]: 0 = no override (use env), 1 = force ON, 2 = force OFF.
static AUTO_ENRICH_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// TEST SEAM — force the auto-enrich pass ON/OFF for the current test binary, race-free (an atomic,
/// NOT the process-global `RMAP_AUTO_ENRICH` env var, which is UB to mutate while the daemon's
/// threads read it). Integration tests that exercise index/refresh VISIBILITY or snapshot counts
/// (NOT enrichment itself) disable the pass so its background LSP work cannot perturb their
/// deterministic assertions. `#[doc(hidden)]`, `_for_test`-named: no production caller.
#[doc(hidden)]
pub fn set_auto_enrich_for_test(enabled: bool) {
    AUTO_ENRICH_OVERRIDE.store(if enabled { 1 } else { 2 }, Ordering::Relaxed);
}

/// Pure core of [`auto_enrich_enabled`] (env value in, decision out) — unit-tested without mutating
/// the process-global environment (which would race parallel tests).
fn auto_enrich_enabled_from(val: Option<&str>) -> bool {
    match val {
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no" | "disabled"
        ),
        None => true,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver-backend test seam (running-yield daemon proof) — production always None
// ─────────────────────────────────────────────────────────────────────────────

/// A builder that constructs the resolver registry for the toolchain-present languages of a pass.
type TestRegistryBuilder = Arc<dyn Fn(&[EnrichmentLanguage]) -> ResolverRegistry + Send + Sync>;

/// TEST SEAM — an injected resolver-registry builder for [`run_enrich_pass`], so the daemon-level
/// running-yield proof can drive the REAL pass (`try_enrich_attempt` → `run_enrich_pass` → pipeline →
/// resolver batch loop → yield → `run_auto_enrich` requeue) into a cancellable RUNNING state without a
/// live LSP toolchain. `None` in production (no caller sets it) → `run_enrich_pass` runs the real
/// `plan_languages` toolchain probe + builds the real resolvers, unchanged.
///
/// (Abstraction ledger — **What:** a process-global override of the pass's resolver backend. **Concrete
/// current user:** the `enrich_lifecycle` cancel-of-running proof (the ratified 2026-07-06 running-yield
/// test). **Axis of variation:** none in production — it is a hermetic stand-in for the three real LSP
/// resolvers, which cannot run as subprocesses in a unit/integration test. **Rejected simpler
/// alternative:** thread a registry-builder PARAM through `run_enrich_pass`/`try_enrich_attempt`/
/// `run_auto_enrich`/`spawn_auto_enrich` — four production signatures changed for a test; the seam
/// leaves every production signature and code path untouched. Mirrors the existing
/// [`set_auto_enrich_for_test`] atomic-override precedent, `#[doc(hidden)]` + `_test`-named.)
static TEST_REGISTRY_BUILDER: Mutex<Option<TestRegistryBuilder>> = parking_lot::const_mutex(None);

/// TEST SEAM (see [`TEST_REGISTRY_BUILDER`]) — install a fake resolver backend for the current test
/// binary. When set, [`run_enrich_pass`] runs EVERY eligible-present language through `builder`'s
/// registry (bypassing the real toolchain probe — the test provides the resolvers), so a real pass can
/// be driven to a batch-boundary yield with no live LSP. Clear it with
/// [`clear_test_registry_builder`] before releasing the test's serial lock. No production caller.
#[doc(hidden)]
pub fn set_test_registry_builder<F>(builder: F)
where
    F: Fn(&[EnrichmentLanguage]) -> ResolverRegistry + Send + Sync + 'static,
{
    *TEST_REGISTRY_BUILDER.lock() = Some(Arc::new(builder));
}

/// TEST SEAM — remove any installed fake resolver backend (see [`set_test_registry_builder`]).
#[doc(hidden)]
pub fn clear_test_registry_builder() {
    *TEST_REGISTRY_BUILDER.lock() = None;
}

/// The installed fake resolver backend, if any (clones the `Arc` out under the lock). Always `None` in
/// production.
fn test_registry_builder() -> Option<TestRegistryBuilder> {
    TEST_REGISTRY_BUILDER.lock().clone()
}

/// ORIENT-FACT-COHERENCE-1 (operator ruling review-3(b)) — bridge the existing [`TEST_REGISTRY_BUILDER`]
/// seam to the EXPLICIT-enrich handler (`handle_enrich`). If a fake backend is installed, build its
/// registry for `requested`; `None` in production (no caller sets the builder), leaving `handle_enrich`'s
/// real configured-resolver construction untouched. This is what lets the explicit-enrich real-handler
/// coherence test park a REAL `handle_enrich` in flight (holding its `OpKind::Enrich` activity stamp)
/// without a live LSP toolchain — the operator required testing the canon-stamp fix THROUGH the real
/// handler, and `handle_enrich`'s real resolvers (rust-analyzer/tsserver/jdtls) cannot run hermetically.
///
/// (Abstraction ledger — **What:** a crate-private accessor that reuses the auto pass's resolver seam for
/// the explicit handler. **Concrete current user:** `handle_enrich` (guarded) + the
/// `enrich_in_flight_coherence` explicit-enrich test. **Axis of variation:** none in production — inert
/// hermetic stand-in. **Rejected simpler alternative:** a second, separate seam for the explicit handler —
/// rejected: one seam already models "inject a fake resolver backend"; a parallel one duplicates it.)
pub(crate) fn test_enrich_registry(requested: &[EnrichmentLanguage]) -> Option<ResolverRegistry> {
    test_registry_builder().map(|builder| builder(requested))
}

// ─────────────────────────────────────────────────────────────────────────────
// Toolchain detection (slice §3.2) — per-language resolver presence
// ─────────────────────────────────────────────────────────────────────────────

/// Is an executable named `bin` present on `$PATH`? A lightweight presence probe (no subprocess) for
/// the toolchain-aware honest skip — orientation, not a guarantee the binary will run (VISION:
/// discovery over perfection). macOS/Linux only (the VISION's platform priority); `is_file` on each
/// `$PATH` entry is sufficient (rust-analyzer / tsserver ship as real files on PATH).
fn binary_on_path(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(bin).is_file())
}

/// Is the resolver toolchain for `lang` present, given the repo root, the discovered TypeScript
/// project contexts, and the resolved `jdtls_path`?
///
/// Mirrors how each resolver actually locates its LSP binary (verified against the resolver crates):
/// - **Rust:** `rust-analyzer` on `$PATH` (`RustAnalyzerSession::start` hardcodes the binary name).
/// - **TypeScript:** tsserver is located for ANY discovered project context via the SAME shared
///   [`tsserver_resolver::locate_tsserver`] the resolver's own per-context session loop uses — walk UP from each
///   context to the repo root (nearest `node_modules/.bin/tsserver` wins), then config path, then
///   `$PATH` (TSSERVER-LOCATE-1). A nested-package repo (tsserver in `frontend/web/node_modules`, not
///   at the root) is now correctly seen as available — no second parallel heuristic.
/// - **Java:** a `jdtls_path` is configured (jdtls has NO PATH discovery — env/flag only).
///
/// This is the SINGLE source the pass uses for BOTH which resolvers to register AND which languages
/// to honestly skip, so the doctor skip line can never promise a remedy the pass did not attempt.
pub fn resolver_toolchain_available(
    lang: EnrichmentLanguage,
    repo_root: &Path,
    ts_contexts: &[PathBuf],
    jdtls_path: Option<&str>,
) -> bool {
    match lang {
        EnrichmentLanguage::Rust => binary_on_path("rust-analyzer"),
        EnrichmentLanguage::TypeScript => ts_contexts
            .iter()
            .any(|ctx| locate_tsserver(ctx, repo_root, None).is_some()),
        EnrichmentLanguage::Java => jdtls_path.is_some(),
    }
}

/// Reader-frame install next-action for a language whose toolchain is absent (VISION: "labels speak
/// the reader's language"). Describes the reader's world (their missing tool), not our pipeline.
fn install_next_action(lang: EnrichmentLanguage) -> &'static str {
    match lang {
        EnrichmentLanguage::Rust => "install rust-analyzer (rustup component add rust-analyzer)",
        EnrichmentLanguage::TypeScript => {
            "install typescript so tsserver is on PATH (npm i -g typescript)"
        }
        EnrichmentLanguage::Java => "set JDTLS_PATH to your jdtls launcher",
    }
}

/// Reader-frame skip reason naming the TS project contexts that have no tsserver (TSSERVER-LOCATE-1
/// §2.2). Speaks the reader's world — *their* directories, *their* local install — not our pipeline:
/// "no typescript in `frontend/legacy`, `serverless` — npm i -D typescript there". The per-package
/// `npm i -D typescript` is the correct remedy for a nested layout (a global install is NOT what these
/// packages need). Directories render repo-relative; the repo-root context renders as `.`.
fn ts_missing_context_reason(repo_root: &Path, missing: &[&PathBuf]) -> String {
    let dirs: Vec<String> = missing
        .iter()
        .map(|dir| {
            let rel = dir.strip_prefix(repo_root).unwrap_or(dir);
            let shown = if rel.as_os_str().is_empty() {
                ".".to_string()
            } else {
                rel.display().to_string()
            };
            format!("`{shown}`")
        })
        .collect();
    format!(
        "no typescript in {} — npm i -D typescript there",
        dirs.join(", ")
    )
}

/// The stable lowercase language token used in the doctor JSON + skip lines.
///
/// `pub(crate)` so `enrichment_skip_gate::gated_enrichment_skips` (DOCS-LIST-2 §2.4) maps its material
/// enrichment languages to the SAME token `SkippedLanguage.language` carries — one token vocabulary,
/// so the doctor materiality gate and the skip lines cannot disagree on a language's spelling.
pub(crate) fn language_token(lang: EnrichmentLanguage) -> &'static str {
    match lang {
        EnrichmentLanguage::Rust => "rust",
        EnrichmentLanguage::TypeScript => "typescript",
        EnrichmentLanguage::Java => "java",
    }
}

/// A language that had eligible edges but whose resolver toolchain is absent — the honest skip
/// (slice §3.2). Never an error; carries a reader-frame reason with the install next-action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedLanguage {
    /// Lowercase language token (`rust` / `typescript` / `java`).
    pub language: String,
    /// Reader-frame reason, e.g. "rust-analyzer not found — install rust-analyzer (…)".
    pub reason: String,
}

/// Split the languages that HAVE eligible edges into (run these) + (skip these, toolchain absent).
///
/// Pure over the language slice and an injected availability predicate — unit-tested without touching
/// `$PATH` (production passes a closure over [`resolver_toolchain_available`]). Order is the caller's
/// (`run_enrich_pass` builds `present` in the fixed Rust→TypeScript→Java order for determinism —
/// `EnrichmentLanguage` is not `Ord`, so a fixed probe order replaces a `BTreeSet`).
pub fn plan_languages(
    present: &[EnrichmentLanguage],
    available: &dyn Fn(EnrichmentLanguage) -> bool,
) -> (Vec<EnrichmentLanguage>, Vec<SkippedLanguage>) {
    let mut to_run = Vec::new();
    let mut skipped = Vec::new();
    for &lang in present {
        if available(lang) {
            to_run.push(lang);
        } else {
            let bin = match lang {
                EnrichmentLanguage::Rust => "rust-analyzer",
                EnrichmentLanguage::TypeScript => "tsserver",
                EnrichmentLanguage::Java => "jdtls",
            };
            skipped.push(SkippedLanguage {
                language: language_token(lang).to_string(),
                reason: format!("{bin} not found — {}", install_next_action(lang)),
            });
        }
    }
    (to_run, skipped)
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass outcome + doctor report
// ─────────────────────────────────────────────────────────────────────────────

/// What one enrichment pass did — the data the daemon records + reports.
#[derive(Debug, Clone)]
pub struct EnrichPassOutcome {
    /// Eligible unresolved edges the pass attempted (for the languages it ran).
    pub eligible_count: usize,
    /// Edges whose receiver type was resolved.
    pub enriched_count: usize,
    /// Enriched edges promoted to resolved graph edges (what upgrades the call graph).
    pub promoted_count: usize,
    /// Resolution rate (percent) over the eligible set the pass ran.
    pub enrichment_rate: f64,
    /// Languages present with eligible edges but no toolchain — the honest skips (slice §3.2).
    pub skipped: Vec<SkippedLanguage>,
    /// ENRICH-ROOT-1 §2: eligible edges NOT ATTEMPTED because their project context lacked a
    /// toolchain (e.g. no tsserver under a tsconfig). Distinct from `skipped` (a whole LANGUAGE with
    /// no toolchain): these are per-context misses WITHIN a language that DID run. Counted so
    /// `eligible = enriched + failed + not_attempted` holds.
    pub not_attempted_count: usize,
    /// The per-context breakdown (context path + reason + edge count) behind `not_attempted_count`.
    pub skipped_contexts: Vec<enrichment::SkippedContext>,
    /// ENRICH-YIELD-1: the promotion funnel — resolved candidates → promoted, with the reader-frame
    /// per-gate first-rejection breakdown of the rest. `None` when the pass ran no promotion (nothing
    /// eligible / all languages skipped), so a zero-work pass renders as "no data" rather than a
    /// misleading measured-zero. Some(funnel with candidates=0) is a promotion that found no
    /// candidates (honest, distinct from None).
    pub funnel: Option<PromotionFunnel>,
}

impl EnrichPassOutcome {
    /// The lifecycle state token surfaced on the doctor line (slice §3.7):
    /// - `skipped` — nothing ran because every eligible language lacked a toolchain;
    /// - `completed` — at least one language ran (even if some others were skipped).
    pub fn lifecycle_state(&self) -> &'static str {
        if self.enriched_count == 0 && self.eligible_count == 0 && !self.skipped.is_empty() {
            "skipped"
        } else {
            "completed"
        }
    }
}

/// A completed enrichment pass, kept on the [`EnrichCoordinator`] so `rmap doctor` can report the
/// full lifecycle (completed / skipped-with-reason). Async (spawned after the index response is
/// sent), so the synchronous index reply cannot carry its result — doctor reads this. Mirrors the
/// `RetentionReport` precedent.
#[derive(Debug, Clone)]
pub struct EnrichmentReport {
    pub repo_display: String,
    pub outcome: EnrichPassOutcome,
    at: Instant,
}

impl EnrichmentReport {
    pub fn new(repo_display: String, outcome: EnrichPassOutcome) -> Self {
        Self {
            repo_display,
            outcome,
            at: Instant::now(),
        }
    }

    /// The JSON shape `daemon_info.last_enrichment` carries (read by the `rmap doctor` enrichment
    /// probe). `state` is the lifecycle token; `skipped` is the reader-frame per-language honest skip.
    /// `promotion_funnel` (ENRICH-YIELD-1) is present only when the pass ran promotion — its absence
    /// is honest "no funnel data", never a measured-zero.
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "repo": self.repo_display,
            "state": self.outcome.lifecycle_state(),
            "eligible_count": self.outcome.eligible_count,
            "enriched_count": self.outcome.enriched_count,
            "promoted_count": self.outcome.promoted_count,
            "enrichment_rate": self.outcome.enrichment_rate,
            "skipped": self.outcome.skipped.iter().map(|s| serde_json::json!({
                "language": s.language,
                "reason": s.reason,
            })).collect::<Vec<_>>(),
            // ENRICH-ROOT-1 §2: per-context not-attempted accounting. Always present (0 / [] when
            // every context had its toolchain) — additive keys, all pre-existing keys byte-identical.
            "not_attempted_count": self.outcome.not_attempted_count,
            "skipped_contexts": self.outcome.skipped_contexts.iter().map(|c| serde_json::json!({
                "context_path": c.context_path,
                "reason": c.reason,
                "edge_count": c.edge_count,
            })).collect::<Vec<_>>(),
            "finished_secs_ago": self.at.elapsed().as_secs(),
        });
        // Additive: attach the funnel breakdown ({candidates, promoted, rejected, rejections:[{reason,
        // gate, label, count}]}) only when it exists, keeping every pre-existing key byte-identical.
        if let Some(funnel) = &self.outcome.funnel {
            if let (Some(obj), Ok(funnel_json)) =
                (value.as_object_mut(), serde_json::to_value(funnel))
            {
                obj.insert("promotion_funnel".to_string(), funnel_json);
            }
        }
        value
    }
}

/// The outcome of one gated attempt at the pass.
pub enum EnrichAttempt {
    /// The pass ran to completion; carries its outcome (may include per-language skips).
    Ran(EnrichPassOutcome),
    /// A contention gate was closed (another op writes this DB, or another enrichment holds the
    /// daemon-global run slot) — the caller should requeue.
    Yielded(&'static str),
    /// A newer trigger for the same repo has superseded this (queued) pass — drop it (slice §3.1).
    Superseded,
    /// The pass could not start / errored (storage open, no ready snapshot, or a storage error).
    Failed(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Daemon-global coordination: one-at-a-time slot + per-repo generation + last report
// ─────────────────────────────────────────────────────────────────────────────

/// The running-pass registry: the cancel flag of the pass currently holding the DB write lock (if
/// any), PLUS the pending yield markers for the acquire→register window. BOTH live under ONE mutex so
/// that a `request_yield_for_db` either latches a registered flag OR records a pending marker
/// ATOMICALLY against a concurrent `register_running` — the lost-yield race (review-1) closes only if
/// "check for a flag, else mark pending" and "adopt pending, then insert the flag" are each indivisible.
///
/// (Abstraction ledger — **What:** a two-field struct grouping the per-DB running-pass cancel flags
/// and the per-DB pending-yield markers under one lock. **Concrete current users:** [`EnrichCoordinator`]'s
/// `register_running` (adopt pending + insert flag), `request_yield_for_db` (latch flag OR mark
/// pending), `clear_pending_yield` (drop a stale marker), `RunningPassGuard::drop` (remove flag), and
/// `activity_state` (read flags). **Axis of variation:** none — a cohesion wrapper, not a plugin seam.
/// **Rejected simpler alternative:** two separate `Mutex`es (flags, pending) — rejected because the
/// correctness argument requires the flag-check and the pending-mark to be ONE atomic step; two locks
/// reintroduce a race between them.)
#[derive(Debug, Default)]
struct RunningRegistry {
    /// The cancel flag of the pass currently holding the DB write lock, keyed by canonical db_path.
    /// Bounded to ≤1 entry by the run slot, but keyed by DB so an index on DB-A never cancels DB-B.
    flags: BTreeMap<PathBuf, crate::cancel::CancelFlag>,
    /// DBs for which an explicit index/refresh requested a yield while NO pass was registered — a
    /// persistent signal a pass in the acquire→register window adopts on registration. Removed by
    /// `clear_pending_yield` once the requesting write owns the lock (so it cannot cancel a later pass).
    pending: BTreeSet<PathBuf>,
}

/// Daemon-global enrichment coordination, held as ONE cohesive field on [`DaemonState`] (mirroring
/// the single `activity` field). Bundles the enrichment-lifecycle state:
/// - `generations` — per-repo trigger counter for the supersede rule (slice §3.1);
/// - `run_slot` — the "one background enrichment at a time per daemon" mutex (slice §3.1);
/// - `last_report` — the most-recent completed pass, for the `rmap doctor` lifecycle line (slice §3.7);
/// - `running` — a [`RunningRegistry`]: the RUNNING pass's cancel flag (so an explicit index/refresh
///   can make a running background pass yield — slice §3.4, the ENRICH-RUNNING-YIELD refinement) PLUS
///   the pending yield markers that close the acquire→register window (review-1). Both keyed by
///   canonical db_path, so an index on DB-A never cancels an enrichment running on DB-B.
/// - `in_flight` — count of spawned-but-not-finished auto passes, so [`activity_state`](Self::activity_state)
///   can tell `rmap doctor` a pass is QUEUED (spawned, not yet holding the write lock) rather than
///   letting the enrichment line falsely render "none yet — runs after the next index" (slice §3.7).
#[derive(Debug, Default)]
pub struct EnrichCoordinator {
    generations: Mutex<BTreeMap<String, u64>>,
    run_slot: Mutex<()>,
    last_report: Mutex<Option<EnrichmentReport>>,
    running: Mutex<RunningRegistry>,
    /// Per-DB count of spawned-but-not-finished auto passes (`enter_flight` .. pass-thread exit), keyed
    /// by canonical db_path. `Arc<Mutex<..>>` so [`FlightGuard`] can hold a clone across the detached
    /// pass thread; read by `activity_state` (daemon-wide) and `auto_enrichment_in_flight_for_db` (per-repo).
    ///
    /// ORIENT-FACT-COHERENCE-1: this was a daemon-wide `AtomicUsize`. It is now PER-DB so the daemon can
    /// answer "is a pass queued for THIS repo" without cross-labelling a second repo as enriching while
    /// a first repo's pass runs (a STANDING HONESTY-RULE violation — a false in-flight claim). The
    /// daemon-wide `activity_state()` (doctor) is derived from "any db in flight", byte-identical.
    in_flight: Arc<Mutex<BTreeMap<PathBuf, usize>>>,
}

impl EnrichCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bump the trigger generation for `repo_uid` and return the NEW value. Each index/refresh
    /// completion calls this before spawning; the spawned pass captures the returned generation.
    pub fn bump_generation(&self, repo_uid: &str) -> u64 {
        let mut g = self.generations.lock();
        let counter = g.entry(repo_uid.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    /// The current (latest) trigger generation for `repo_uid` (0 if never triggered).
    pub fn current_generation(&self, repo_uid: &str) -> u64 {
        self.generations.lock().get(repo_uid).copied().unwrap_or(0)
    }

    /// Try to take the daemon-global "one enrichment at a time" slot without blocking. `None` means
    /// another enrichment is already running — the caller yields and requeues.
    pub fn try_acquire_run_slot(&self) -> Option<MutexGuard<'_, ()>> {
        self.run_slot.try_lock()
    }

    /// Record the most-recent completed pass (most-recent-wins across repos, mirroring the single
    /// `last_retention` line). Called by the detached pass, so it never blocks a request path.
    pub fn record(&self, report: EnrichmentReport) {
        *self.last_report.lock() = Some(report);
    }

    /// The most-recent pass as `daemon_info.last_enrichment` JSON (`None` if none has completed since
    /// the daemon started).
    pub fn last_json(&self) -> Option<serde_json::Value> {
        self.last_report.lock().as_ref().map(|r| r.to_json())
    }

    /// The current auto-enrich lifecycle activity, for the `rmap doctor` enrichment line (slice §3.7):
    /// - `"running"` — a pass currently holds the DB write lock (registered in `running`);
    /// - `"queued"` — a pass is spawned but not yet holding the lock (in the gate/requeue wait);
    /// - `"idle"` — none in flight.
    ///
    /// This is the signal that lets doctor tell "queued" from "none yet — runs after the next index":
    /// a pass that is queued (or running its first, not-yet-recorded pass) must NOT render as "none
    /// yet", which is false. `running` is checked first because a running pass is also counted in
    /// `in_flight` (in-flight spans spawn → thread exit, which includes the running window).
    pub fn activity_state(&self) -> &'static str {
        if !self.running.lock().flags.is_empty() {
            "running"
        } else if !self.in_flight.lock().is_empty() {
            "queued"
        } else {
            "idle"
        }
    }

    /// ORIENT-FACT-COHERENCE-1: is the AUTO background enrichment pass QUEUED or RUNNING **for this db**
    /// right now? RUNNING is the per-DB cancel-flag registry (`running.flags`); QUEUED is a spawned-but-
    /// not-yet-lock-holding pass (`in_flight[db] > 0`). Both keys are canonicalized the SAME way the pass
    /// registers them (via [`canon_db`]), so the repo scope is exact — never a second repo. `false` when
    /// no auto pass is in flight (the common case).
    ///
    /// SCOPE (honest name — review-1 F1): this covers ONLY the auto pass, which is all the coordinator
    /// tracks (`enter_flight` / `register_running` have no explicit-enrich caller). An explicit `rmap
    /// enrich` is in flight in the [`ActivityRegistry`](crate::activity) (`OpKind::Enrich`), NOT here, so
    /// the reader-facing "is any enrichment in flight" question is answered by
    /// [`DaemonState::enrichment_in_flight_for_db`](crate::state::DaemonState::enrichment_in_flight_for_db),
    /// which unions this with the activity registry. Handlers call THAT, not this.
    pub(crate) fn auto_enrichment_in_flight_for_db(&self, db_path: &Path) -> bool {
        let key = canon_db(db_path);
        if self.running.lock().flags.contains_key(&key) {
            return true;
        }
        self.in_flight.lock().get(&key).copied().unwrap_or(0) > 0
    }

    /// Mark an auto pass as in-flight (spawned) for `db_path` and return a guard that clears the mark on
    /// drop. [`spawn_auto_enrich`] calls this and moves the guard into the pass thread, so the "queued"
    /// signal is live from spawn until the pass thread exits on ANY terminal state (ran / failed /
    /// superseded / deferred). See [`activity_state`](Self::activity_state) and
    /// [`auto_enrichment_in_flight_for_db`](Self::auto_enrichment_in_flight_for_db).
    pub fn enter_flight(&self, db_path: &Path) -> FlightGuard {
        let key = canon_db(db_path);
        *self.in_flight.lock().entry(key.clone()).or_insert(0) += 1;
        FlightGuard {
            counts: Arc::clone(&self.in_flight),
            key,
        }
    }

    /// Register a [`CancelFlag`] for the pass now running on `db_path`, returning a guard that
    /// deregisters it on drop plus the flag itself (which the pass polls between batches). Called
    /// under the run slot, so at most one flag is ever registered. An explicit index/refresh finds it
    /// via [`request_yield_for_db`](Self::request_yield_for_db).
    ///
    /// If an explicit write requested a yield for `db_path` during the acquire→register window (found
    /// no flag, left a PENDING marker), this ADOPTS it: the returned flag starts already-cancelled, so
    /// the pass yields at its first batch boundary instead of running to completion while that explicit
    /// write blocks (the review-1 lost-yield fix). The check-and-adopt is one critical section with
    /// `request_yield_for_db`'s mark, so the window cannot leak a signal between them.
    ///
    /// (Abstraction ledger — the ENRICH-RUNNING-YIELD signal, per the operating rule for a new
    /// cross-cutting element: **What** — a per-DB cancel-flag registry so an explicit write can ask a
    /// running background enrichment to yield the DB write lock. **Concrete current users** —
    /// [`try_enrich_attempt`] registers; `dispatch::handle_index`/`handle_refresh` latch via
    /// `request_yield_for_db`. **Axis of variation** — none; it reuses the shipped
    /// `cancel::CancelFlag` (`Arc<AtomicBool>` latch), not a new cancel framework. **Rejected simpler
    /// alternative** — poll the activity registry from inside the pass: rejected because an explicit
    /// index BLOCKS on `acquire_write` BEFORE it stamps its activity op [OBSERVED: `handle_index`
    /// acquires the write lock, then stamps], so a lock-holding pass could never see it there.)
    pub fn register_running(
        &self,
        db_path: &Path,
    ) -> (RunningPassGuard<'_>, crate::cancel::CancelFlag) {
        let key = canon_db(db_path);
        let flag = crate::cancel::CancelFlag::new();
        let mut reg = self.running.lock();
        // Adopt a yield that landed in the acquire→register window (recorded as pending because no
        // flag was registered to latch). Removing + adopting in the same critical section as the
        // insert closes the lost-yield race with a concurrent `request_yield_for_db`.
        if reg.pending.remove(&key) {
            flag.cancel();
        }
        reg.flags.insert(key.clone(), flag.clone());
        (RunningPassGuard { coord: self, key }, flag)
    }

    /// Ask a background enrichment on `db_path` to yield at its next batch boundary (slice §3.4).
    /// Called by explicit index/refresh handlers BEFORE they take the DB write lock — so a lock-holding
    /// pass sees the signal and releases the lock (the index was blocked on `acquire_write` the whole
    /// time and then proceeds).
    ///
    /// Two cases, handled in ONE critical section so the acquire→register window cannot swallow the
    /// signal (review-1):
    /// - **A pass is registered** → latch its flag directly (it polls this between batches).
    /// - **No pass is registered** → the pass may be in the acquire→register window, already holding
    ///   the write lock this call is about to block on. Record a PENDING marker so that pass adopts the
    ///   yield when it calls [`register_running`](Self::register_running), instead of losing it. The
    ///   requesting write drops the marker via [`clear_pending_yield`](Self::clear_pending_yield) once
    ///   it owns the lock, so a marker set with nothing running never cancels a later pass.
    pub fn request_yield_for_db(&self, db_path: &Path) {
        let key = canon_db(db_path);
        let mut reg = self.running.lock();
        match reg.flags.get(&key) {
            Some(flag) => flag.cancel(),
            None => {
                reg.pending.insert(key);
            }
        }
    }

    /// Drop any PENDING yield marker for `db_path` (see [`request_yield_for_db`](Self::request_yield_for_db)).
    /// Called by the explicit index/refresh handler right AFTER it acquires the DB write lock and
    /// BEFORE it releases it. At that point the handler holds the write lock, so — by mutual exclusion
    /// on that lock — no enrichment pass can be in the acquire→register window; any pending marker is
    /// therefore stale (already adopted by a pass that yielded, or set when nothing was running).
    /// Clearing it here keeps a stale marker from making the NEXT enrichment pass yield spuriously
    /// (e.g. the very pass this index is about to queue). Safe because a later pass can only register
    /// after this handler releases the lock, which happens-after this clear.
    pub fn clear_pending_yield(&self, db_path: &Path) {
        let key = canon_db(db_path);
        self.running.lock().pending.remove(&key);
    }
}

/// Canonicalize a DB path for the running-pass registry key, falling back to the raw path if the DB
/// file does not exist (which can only happen for a first index — where no pass is running anyway).
/// Both the registering pass and the yielding index resolve the SAME registry-entry db_path for a
/// repo, so the keys match; canonicalizing makes the match robust to relative-vs-absolute forms.
fn canon_db(db_path: &Path) -> PathBuf {
    db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf())
}

/// RAII guard deregistering a running pass's cancel flag on drop (mirrors the `ActivityGuard`
/// pattern). Held by [`try_enrich_attempt`] for the pass's duration.
pub struct RunningPassGuard<'a> {
    coord: &'a EnrichCoordinator,
    key: PathBuf,
}

impl Drop for RunningPassGuard<'_> {
    fn drop(&mut self) {
        self.coord.running.lock().flags.remove(&self.key);
    }
}

/// RAII guard decrementing the per-DB in-flight auto-pass count on drop (mirrors [`RunningPassGuard`]).
/// Held by the spawned pass thread for its whole life, so [`EnrichCoordinator::activity_state`] and
/// [`EnrichCoordinator::auto_enrichment_in_flight_for_db`] report "queued"/"running" for exactly as long as a
/// pass is in flight. `Send + 'static` (holds an owned `Arc` + `PathBuf`, not a coordinator borrow), so
/// it moves into the detached pass thread.
pub struct FlightGuard {
    counts: Arc<Mutex<BTreeMap<PathBuf, usize>>>,
    key: PathBuf,
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        let mut counts = self.counts.lock();
        if let Some(n) = counts.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                counts.remove(&self.key);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The pass core
// ─────────────────────────────────────────────────────────────────────────────

/// Run the enrichment pipeline (WITH promotion) for the toolchain-present languages of `repo_uid`'s
/// latest READY snapshot, returning the outcome + honest skips.
///
/// **The caller MUST already hold the DB write lock + the run slot** (see [`try_enrich_attempt`]);
/// this is the raw mechanism, exposed so the named tests can drive it with an injected availability
/// predicate. `available` decides per-language toolchain presence (production: a closure over
/// [`resolver_toolchain_available`]); tests inject a fake so the real `$PATH` is irrelevant.
///
/// Sequence: resolve latest READY snapshot → probe eligible edges to learn which languages are
/// PRESENT → [`plan_languages`] splits present into run / skip → if nothing to run, return an outcome
/// carrying only the skips (honest; never an error) → else register exactly the runnable resolvers,
/// run the pipeline with promotion, and return the real counts + the skips.
///
/// `cancel` is the running-yield check (slice §3.4): passed into `run_cancellable`, it is polled at
/// the pipeline's between-language boundary and threaded into each resolver's per-session/per-edge
/// boundaries, so an explicit index/refresh makes the pass yield within one LSP request. A completed
/// (never-cancelled) run promotes; a cancelled run skips promotion (the caller maps it to a requeue).
pub fn run_enrich_pass(
    db_path: &Path,
    repo_uid: &str,
    repo_root: &Path,
    jdtls_path: Option<&str>,
    available: &dyn Fn(EnrichmentLanguage, &[PathBuf]) -> bool,
    cancel: &dyn Fn() -> bool,
) -> Result<EnrichPassOutcome, String> {
    // Resolve the latest READY snapshot + the repo root (the pipeline uses the same root). A fresh
    // connection scoped to this probe; the pipeline opens its own (it takes ownership).
    let (snapshot_uid, present, ts_contexts, language_counts) = {
        // NO-CREATE (FORGET-REPO-1): auto-enrich probes an EXISTING indexed DB; never create.
        let storage = StorageConnection::open_existing(db_path).map_err(|e| e.to_string())?;
        let snapshot = storage
            .get_latest_snapshot(repo_uid)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no ready snapshot to enrich".to_string())?;
        if snapshot.status != "ready" {
            return Err(format!(
                "latest snapshot for '{repo_uid}' is not ready (status: {})",
                snapshot.status
            ));
        }
        // Which languages have any eligible unresolved edge? (No limit — we only need the language
        // set; the pipeline applies the real limit.) `query_eligible_edges` is on EnrichmentStoragePort.
        // Fixed Rust→TypeScript→Java probe order gives a deterministic, deduped `present` without
        // requiring `Ord` on `EnrichmentLanguage`.
        let eligible = storage
            .query_eligible_edges(&EligibilityQuery::new(&snapshot.snapshot_uid))
            .map_err(|e| e.to_string())?;
        let present: Vec<EnrichmentLanguage> = [
            EnrichmentLanguage::Rust,
            EnrichmentLanguage::TypeScript,
            EnrichmentLanguage::Java,
        ]
        .into_iter()
        .filter(|lang| eligible.iter().any(|e| e.language == *lang))
        .collect();
        // TS project contexts (TSSERVER-LOCATE-1 §2.1): the SAME `project.rs` directory grouping the
        // resolver falls back on — one context per nearest tsconfig/jsconfig/package.json ancestor. The
        // shared locator walks UP from each of these to the repo root, so the probe and the resolver
        // agree on where tsserver is. Absolute dirs (keys of `group_by_project_root`).
        let ts_edges: Vec<EligibleEdge> = eligible
            .iter()
            .filter(|e| e.language == EnrichmentLanguage::TypeScript)
            .cloned()
            .collect();
        let ts_contexts: Vec<PathBuf> = if ts_edges.is_empty() {
            Vec::new()
        } else {
            group_by_project_root(repo_root, &ts_edges)
                .into_keys()
                .collect()
        };
        // DOCS-LIST-2 §2.4 (review-0 F3): the repo's per-language file counts, for the doctor skip
        // materiality gate (see below). Kept as a `Result`, NOT `.ok()`-collapsed: this read CLASSIFIES
        // the remediation lines, so a failure must render unknown-with-reason (never silently keep the
        // ungated remedies, which would leak `npm i -D typescript` into a Python repo). The honest
        // Ok/Err handling lives in `enrichment_skip_gate::gated_enrichment_skips`.
        let language_counts = repo_graph_agent::AgentStorageRead::query_file_count_by_language(
            &storage,
            &snapshot.snapshot_uid,
        )
        .map_err(|e| e.to_string());
        (snapshot.snapshot_uid, present, ts_contexts, language_counts)
    };

    // Toolchain plan (which languages RUN vs are honestly skipped) + the resolver registry. A test
    // backend (`set_test_registry_builder`) short-circuits BOTH the real toolchain probe and the real
    // resolver construction, so the daemon-level running-yield proof can drive this REAL pass with a
    // fake cancellable resolver (no live LSP). Production never installs it → real `plan_languages` +
    // real resolvers, unchanged.
    // The injected availability predicate is context-aware for TypeScript; partially apply the
    // discovered contexts so `plan_languages` keeps its simple `Fn(lang) -> bool` shape.
    let avail = |lang: EnrichmentLanguage| available(lang, &ts_contexts);
    let test_registry = test_registry_builder();
    let (to_run, mut skipped) = match &test_registry {
        // The test provides resolvers for every eligible-present language → run them all (availability
        // is exercised separately by `plan_languages`' unit tests + the toolchain-skip proof).
        Some(_) => (present.clone(), Vec::new()),
        None => plan_languages(&present, &avail),
    };

    // TSSERVER-LOCATE-1 §2.2 — partial availability is PER CONTEXT, not all-or-nothing. Name the TS
    // contexts that lack a tsserver in the reader's language, whether TypeScript ran (some contexts had
    // one, some did not) or was fully skipped (none did). `ts_gate` gates the per-context locate through
    // the SAME injected predicate that decided run/skip, so a test that forces TypeScript absent
    // (`|_, _| false`) names every context host-independently (never depends on the host `$PATH`).
    if present.contains(&EnrichmentLanguage::TypeScript) {
        let ts_gate = avail(EnrichmentLanguage::TypeScript);
        let missing: Vec<&PathBuf> = ts_contexts
            .iter()
            .filter(|ctx| !(ts_gate && locate_tsserver(ctx, repo_root, None).is_some()))
            .collect();
        if !missing.is_empty() {
            let reason = ts_missing_context_reason(repo_root, &missing);
            match skipped
                .iter_mut()
                .find(|s| s.language == language_token(EnrichmentLanguage::TypeScript))
            {
                // Fully skipped (no context had one): replace the generic global reason with the
                // per-context one naming every directory.
                Some(ts_skip) => ts_skip.reason = reason,
                // Partially available (TypeScript is in `to_run`): add a skip naming ONLY the contexts
                // without a tsserver — the ones that will not enrich.
                None => skipped.push(SkippedLanguage {
                    language: language_token(EnrichmentLanguage::TypeScript).to_string(),
                    reason,
                }),
            }
        }
    }

    // DOCS-LIST-2 §2.4: route the doctor skip remediation through the SAME per-language capability
    // gate the CTA uses, so doctor never prescribes another ecosystem's remedy. On a materially-Python
    // repo an incidental (<10%) TypeScript skip is DROPPED (no `npm i -D typescript`); when no material
    // code language is enrichable on any build, the no-semantic-path sentence for the dominant language
    // replaces it. On a language-breakdown READ FAILURE the raw remedies are replaced by a single
    // unknown-with-reason skip (review-0 F3) — never silently kept (which would leak npm advice).
    skipped = crate::enrichment_skip_gate::gated_enrichment_skips(&skipped, language_counts);

    // Nothing runnable — either no eligible edges at all (present empty → skipped empty → a clean
    // "nothing to enrich") or every eligible language lacks a toolchain (skipped populated → the
    // honest skip). Never an error.
    if to_run.is_empty() {
        return Ok(EnrichPassOutcome {
            eligible_count: 0,
            enriched_count: 0,
            promoted_count: 0,
            enrichment_rate: 0.0,
            skipped,
            not_attempted_count: 0,
            skipped_contexts: Vec::new(),
            // No language ran → no promotion → no funnel (honest "no data", not a measured-zero).
            funnel: None,
        });
    }

    // Register exactly the runnable resolvers (the composition root for the background pass) — from the
    // test backend when installed, else the real per-language resolvers.
    let registry = match &test_registry {
        Some(build) => build(&to_run),
        None => {
            let mut registry = ResolverRegistry::new();
            for &lang in &to_run {
                match lang {
                    EnrichmentLanguage::Rust => {
                        registry.register(Box::new(RustAnalyzerResolver::new()))
                    }
                    EnrichmentLanguage::TypeScript => {
                        registry.register(Box::new(TsServerResolver::new()))
                    }
                    EnrichmentLanguage::Java => {
                        // `plan_languages` only keeps Java when `available(Java)` was true, which in
                        // production means `jdtls_path.is_some()`. Guard defensively rather than panic.
                        let Some(path) = jdtls_path else {
                            continue;
                        };
                        let config = JdtlsConfig {
                            jdtls_path: Some(path.to_string()),
                            ..Default::default()
                        };
                        registry.register(Box::new(JdtlsResolver::with_config(config)));
                    }
                }
            }
            registry
        }
    };

    // Auto-enrich PROMOTES: promotion is what turns resolved receiver types into resolved call-graph
    // edges, i.e. what makes the DoD's "a snapshot whose call-graph resolution reflects enrichment"
    // true without a further command. Restrict the pipeline to the runnable languages so it does not
    // re-tally the skipped ones as pipeline failures.
    let config = EnrichmentConfig::new()
        .with_promotion()
        .with_languages(to_run.clone());

    // NO-CREATE (FORGET-REPO-1): the enrich pipeline writes an EXISTING indexed DB; never create.
    let pipeline_storage = StorageConnection::open_existing(db_path).map_err(|e| e.to_string())?;
    let mut pipeline = EnrichmentPipeline::with_registry(pipeline_storage, registry);
    let report = pipeline
        .run_cancellable(repo_uid, &snapshot_uid, &config, cancel)
        .map_err(|e| e.to_string())?;

    Ok(EnrichPassOutcome {
        eligible_count: report.eligible_count,
        enriched_count: report.enriched_count,
        promoted_count: report.promotion.as_ref().map(|p| p.promoted).unwrap_or(0),
        enrichment_rate: report.enrichment_rate,
        skipped,
        // ENRICH-ROOT-1 §2: carry the per-context not-attempted accounting from the pipeline report
        // to the doctor surface. Zero/empty when every eligible context had its toolchain.
        not_attempted_count: report.not_attempted_count,
        skipped_contexts: report.skipped_contexts.clone(),
        // ENRICH-YIELD-1: decompose the promotion funnel from the same report the promoted count
        // comes from. `Some` iff promotion ran (auto-enrich always promotes), so the funnel is
        // present here whenever a language ran; the reader-frame per-gate breakdown flows to doctor.
        funnel: report.promotion.as_ref().map(|p| p.funnel()),
    })
}

/// Try to run the enrichment pass once, honoring the generation-supersede rule + the two contention
/// gates + the daemon-global run slot.
///
/// Stamps an `Enrich` activity op (so `rmap doctor` renders "enriching <repo>") ONLY after the gates
/// pass — so gate 1 (`active_for_db`) sees only OTHER ops, never this pass itself.
pub fn try_enrich_attempt(
    state: &DaemonState,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
    my_generation: u64,
) -> EnrichAttempt {
    // Supersede check (slice §3.1) — a newer trigger for this repo makes this queued pass stale.
    if state.enrich_coord().current_generation(repo_uid) != my_generation {
        return EnrichAttempt::Superseded;
    }

    // Gate 1 — never touch the DB while a live op (index/refresh/enrich/retention) writes it.
    // Checked before stamping our own op, so we see only OTHERS.
    if state.activity().active_for_db(db_path).is_some() {
        return EnrichAttempt::Yielded("another operation is writing this repo");
    }

    // Gate 2 — take the DB write lock non-blockingly. Held for the whole pass, so a later index
    // waits it out (the interim contention behavior — see the module header).
    let db_runtime = match state.get_or_create_db_runtime(db_path) {
        Ok(r) => r,
        Err(e) => return EnrichAttempt::Failed(format!("could not resolve db runtime: {e}")),
    };
    let _db_guard = match db_runtime.try_acquire_write() {
        Some(g) => g,
        None => return EnrichAttempt::Yielded("another operation is writing this repo"),
    };

    // Daemon-global "one enrichment at a time" slot (slice §3.1).
    let _slot = match state.enrich_coord().try_acquire_run_slot() {
        Some(s) => s,
        None => return EnrichAttempt::Yielded("another enrichment is running"),
    };

    // Re-check the generation after acquiring the gates (a newer trigger may have landed while we
    // waited) — do not run a stale pass under the locks.
    if state.enrich_coord().current_generation(repo_uid) != my_generation {
        return EnrichAttempt::Superseded;
    }

    // Load the repo and take the coordinator's REFRESH lock — the SAME lock discipline the manual
    // `dispatch::handle_enrich` uses ("enrich is a write op on an existing snapshot"). This is what
    // "integrate under existing locks" (slice §3.5) means: readers concurrent with the pass are
    // coordinated through the W-B epoch machinery exactly as they are for a manual enrich, so the
    // auto pass introduces NO new reader-visibility behavior beyond the shipped manual path. (The
    // in-place promotion's own non-transactional delete+insert is a PRE-EXISTING property of the
    // shipped manual enrich — see the build report's atomicity finding; closing it is "beyond
    // existing write coordination" and out of this additive slice.)
    let repo_state = match state.load_repo(db_path, repo_uid) {
        Ok(rs) => rs,
        Err(e) => return EnrichAttempt::Failed(format!("could not load repo to enrich: {e}")),
    };
    let _refresh_guard = repo_state.coordinator.acquire_refresh();

    // Both gates + slot clear → make the pass visible on `rmap doctor` for its duration (the
    // DAEMON-VISIBILITY-1 activity stamp; `OpKind::Enrich` renders "enriching <repo>").
    let _activity = state.activity().begin(
        crate::activity::OpKind::Enrich,
        repo_display.to_string(),
        Some(repo_uid.to_string()),
        db_path.to_path_buf(),
    );

    // Register our cancel flag so an explicit index/refresh for this DB can make us yield (slice
    // §3.4). The guard deregisters on drop; `cancel_flag` is the latch the pipeline/resolvers poll.
    let (_running_guard, cancel_flag) = state.enrich_coord().register_running(db_path);
    let cancel = || cancel_flag.is_cancelled();

    let available = |lang: EnrichmentLanguage, ts_contexts: &[PathBuf]| {
        resolver_toolchain_available(
            lang,
            Path::new(repo_display),
            ts_contexts,
            jdtls_path_from_env().as_deref(),
        )
    };
    // DAEMON-CRASH-RECOVERY-1 (F8): op-START line for enrichment. Its terminal OUTCOME is logged by
    // `run_auto_enrich` for the Ran (completed) and Failed dispositions; the THIRD terminal of a
    // STARTED run — a cancel at a batch boundary (`Yielded` AFTER this start) — is closed just below,
    // because `run_auto_enrich` cannot tell that case from a gate-yield (both are
    // `EnrichAttempt::Yielded`, but a gate-yield returns ABOVE, before this start line, so it logged no
    // start). Observability only — enrich semantics (the classify → requeue mapping, the return value)
    // are unchanged.
    crate::oplog::log_op_start("enrich", repo_uid, None);
    let run_result = run_enrich_pass(
        db_path,
        repo_uid,
        Path::new(repo_display),
        jdtls_path_from_env().as_deref(),
        &available,
        &cancel,
    );
    let attempt = classify_completed_attempt(run_result, cancel_flag.is_cancelled());
    // review-2 (F8, item 1): every logged `op enrich started` must get a terminal outcome. A `Yielded`
    // reached HERE (after the start line) can ONLY be a cancel-at-batch-boundary — a started, then
    // interrupted, run (every contention gate-yield returned above, before the start). Close it with an
    // `interrupted` outcome so the start/outcome pairing is a local invariant of this function.
    // `run_auto_enrich` still requeues on `Yielded`; this only ADDS the previously-missing log line.
    if let EnrichAttempt::Yielded(reason) = &attempt {
        crate::oplog::log_op_outcome("enrich", repo_uid, None, &format!("interrupted ({reason})"));
    }
    attempt
    // _running_guard + _activity + _slot + _db_guard drop here.
}

/// Classify a completed pass into a requeue-or-record outcome (extracted so the yield decision is
/// unit-testable without a live pass).
///
/// If our cancel flag was latched by an explicit index/refresh mid-pass, we YIELDED at a batch
/// boundary (the pass stopped and skipped promotion) → [`EnrichAttempt::Yielded`], which
/// [`run_auto_enrich`] requeues. The incoming index's own completion then supersedes this repo's
/// requeued pass and re-enriches the fresh snapshot, so the partial outcome is intentionally NOT
/// recorded. (Benign edge: if the flag latches in the window AFTER the last batch completes, a
/// fully-completed pass is treated as yielded — a rare, self-correcting re-enrich of a snapshot the
/// index is superseding anyway, never wrong data.) A non-cancelled pass RANs its real outcome.
fn classify_completed_attempt(
    run_result: Result<EnrichPassOutcome, String>,
    cancelled: bool,
) -> EnrichAttempt {
    match run_result {
        Ok(_) if cancelled => EnrichAttempt::Yielded("yielded to an explicit index/refresh"),
        Ok(outcome) => EnrichAttempt::Ran(outcome),
        Err(e) => EnrichAttempt::Failed(e),
    }
}

/// The one place the pass reads the jdtls launcher (slice §3.2 — env only, matching the resolver's
/// own discipline and `handle_enrich`'s `JDTLS_PATH` fallback).
fn jdtls_path_from_env() -> Option<String> {
    std::env::var("JDTLS_PATH").ok()
}

/// Spawn the automatic background enrichment pass for a DB after a successful index/refresh.
///
/// No-op when opted out (`RMAP_AUTO_ENRICH`) — the caller handles the disabled case (it spawns
/// retention directly). Otherwise bumps this repo's trigger generation (so an older queued pass
/// supersedes) and detaches a thread running the two-gate pass with bounded requeue. NEVER runs on
/// the caller's (foreground) thread.
pub fn spawn_auto_enrich(
    state: Arc<DaemonState>,
    db_path: PathBuf,
    repo_uid: String,
    repo_display: String,
) {
    if !auto_enrich_enabled() {
        return;
    }
    let my_generation = state.enrich_coord().bump_generation(&repo_uid);
    // Mark the pass in-flight NOW (before the thread starts) so `rmap doctor` immediately reads
    // "queued" rather than the false "none yet — runs after the next index" (slice §3.7). The guard
    // rides the thread and clears the mark when `run_auto_enrich` returns on ANY terminal state.
    let flight = state.enrich_coord().enter_flight(&db_path);
    std::thread::spawn(move || {
        let _flight = flight;
        run_auto_enrich(&state, &db_path, &repo_uid, &repo_display, my_generation);
    });
}

/// The detached pass body: requeue until a gate opens (or we are superseded), then run + record +
/// log, and CHAIN the retention pass (the retention slice's "after enrichment promotion" hook).
/// Separated from the thread spawn so the gate/run logic is testable without threads. Takes
/// `&Arc<DaemonState>` (not `&DaemonState`) only so it can hand an owned `Arc` to the chained
/// retention spawn; `try_enrich_attempt` receives it via deref coercion.
///
/// `#[doc(hidden)] pub` (visibility only — no signature/behavior change): the `enrich_lifecycle`
/// cancel-of-running proof drives THIS real requeue loop (Yielded → sleep+retry → Superseded) against
/// a real pass, which the reviewer required over the prior hand-rolled lock-and-flag simulation. The
/// rejected simpler alternative — call `try_enrich_attempt` twice from the test — proves the mapping
/// but not the loop the reviewer flagged as untested.
#[doc(hidden)]
pub fn run_auto_enrich(
    state: &Arc<DaemonState>,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
    my_generation: u64,
) {
    for _ in 0..REQUEUE_MAX_ATTEMPTS {
        match try_enrich_attempt(state, db_path, repo_uid, repo_display, my_generation) {
            EnrichAttempt::Ran(outcome) => {
                // DAEMON-CRASH-RECOVERY-1 (F8, review-0 item a): the op-lifecycle OUTCOME line —
                // same shape as index/refresh (op, repo, outcome), paired with the op-START line
                // `try_enrich_attempt` logged. Replaces the ad-hoc "enrichment: …" summary so the
                // daemon log reads as ONE uniform op lifecycle; `summarize_outcome` is the reader-frame
                // detail (enriched N/M, promoted P / skipped langs / nothing to enrich).
                crate::oplog::log_op_outcome(
                    "enrich",
                    repo_uid,
                    None,
                    &format!("completed ({})", summarize_outcome(&outcome)),
                );
                state.record_enrichment_report(EnrichmentReport::new(
                    repo_display.to_string(),
                    outcome,
                ));
                chain_maintenance_tail(state, db_path, repo_uid, repo_display);
                return;
            }
            EnrichAttempt::Failed(e) => {
                crate::oplog::log_op_outcome("enrich", repo_uid, None, &format!("failed: {e}"));
                // Still chain retention — the index succeeded; cleanup should not hinge on enrich.
                chain_maintenance_tail(state, db_path, repo_uid, repo_display);
                return;
            }
            EnrichAttempt::Superseded => {
                // A newer index's pass owns this repo now; it will chain retention. Drop silently
                // (Superseded returns BEFORE the op-START line, so there is no dangling start to close).
                return;
            }
            EnrichAttempt::Yielded(_reason) => {
                // Yielded to an explicit index/refresh (or a closed gate) — a requeue, not a terminal
                // outcome; sleep and retry.
                std::thread::sleep(REQUEUE_BACKOFF);
            }
        }
    }
    // Deferred (never completed a run) — the terminal disposition of a spawned pass that kept yielding.
    crate::oplog::log_op_outcome(
        "enrich",
        repo_uid,
        None,
        &format!(
            "deferred (repo stayed busy for {}s; the next index/refresh retries)",
            (REQUEUE_MAX_ATTEMPTS as u64) * REQUEUE_BACKOFF.as_secs()
        ),
    );
    // Deferred (never ran) — still hand off to retention so cleanup runs this cycle.
    chain_maintenance_tail(state, db_path, repo_uid, repo_display);
}

/// Chain the maintenance tail after enrichment: **enrich → seed → retention**
/// (EMBED-SEED-IMPL-1, spec §5). The seed pass runs after enrichment and itself
/// chains retention on completion; when seeding is opted out, retention is
/// chained directly. Sequencing the passes — rather than spawning them all from
/// the completion path — is what keeps them from contending for the write lock.
/// Each hop is a no-op when its pass is opted out.
fn chain_maintenance_tail(
    state: &Arc<DaemonState>,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
) {
    crate::seed_pass::chain_seed_then_retention(state, db_path, repo_uid, repo_display);
}

/// Reader-frame one-liner for the daemon log: "enriched N/M edges, promoted P (top rejections: …)
/// (skipped: rust)" | "nothing to enrich" | "skipped: rust-analyzer not found — …".
///
/// ENRICH-YIELD-1: when promotion ran and rejected candidates, the top rejecting classes (reader
/// frame) are named in the headline so the oplog outcome line says WHY promotion banked so few — not
/// just the bare "promoted P". Absent/empty funnel → no note (a zero-work pass stays honest).
pub fn summarize_outcome(o: &EnrichPassOutcome) -> String {
    let skip_note = if o.skipped.is_empty() {
        String::new()
    } else {
        let langs: Vec<&str> = o.skipped.iter().map(|s| s.language.as_str()).collect();
        format!(" (skipped: {})", langs.join(", "))
    };
    if o.eligible_count == 0 {
        if o.skipped.is_empty() {
            "nothing to enrich".to_string()
        } else {
            format!("no toolchain for eligible languages{skip_note}")
        }
    } else {
        format!(
            "enriched {}/{} edges, {}{}{skip_note}",
            o.enriched_count,
            o.eligible_count,
            promoted_clause(o),
            funnel_headline_note(o.funnel.as_ref())
        )
    }
}

/// The promotion clause of the completion headline. ENRICH-YIELD-1 review-2 item 3: state the
/// funnel's OWN denominator — the count of **resolved candidates** that reached the promotion filter
/// (`funnel.candidates`) — so "promoted P" is read against that population, NOT against the larger
/// `enriched`/`eligible` counts (a different, upstream population). Numerator and denominator both
/// come from the funnel so the ratio is internally consistent. Falls back to the bare "promoted P"
/// when no funnel carries a denominator (older report, or a zero-candidate pass) — honest "no
/// denominator to show", not a fabricated one.
fn promoted_clause(o: &EnrichPassOutcome) -> String {
    match o.funnel.as_ref() {
        Some(f) if f.candidates > 0 => {
            format!(
                "promoted {}/{} resolved candidates",
                f.promoted, f.candidates
            )
        }
        _ => format!("promoted {}", o.promoted_count),
    }
}

/// The " (top rejections: <label> N, <label> N)" clause for the completion headline, or empty when
/// there is no funnel or nothing was rejected. Names the top TWO reader-frame classes — enough to
/// point the reader at the dominant cause without reproducing the whole breakdown (that lives on the
/// doctor detail surface).
fn funnel_headline_note(funnel: Option<&PromotionFunnel>) -> String {
    let Some(funnel) = funnel else {
        return String::new();
    };
    let top = funnel.top(2);
    if top.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = top
        .iter()
        .map(|r| format!("{} {}", r.label, r.count))
        .collect();
    format!(" (top rejections: {})", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── auto_enrich_enabled: the opt-out switch (default ON) ───────────────────────────────────

    #[test]
    fn auto_enrich_default_on_and_opt_out_values() {
        assert!(
            auto_enrich_enabled_from(None),
            "unset → ON (ratified default)"
        );
        for off in [
            "0", "false", "off", "no", "disabled", "FALSE", " Off ", "No",
        ] {
            assert!(!auto_enrich_enabled_from(Some(off)), "{off:?} must disable");
        }
        for on in ["1", "true", "on", "yes", "enabled", ""] {
            assert!(auto_enrich_enabled_from(Some(on)), "{on:?} must stay ON");
        }
    }

    // ── plan_languages: the toolchain split (run vs honest-skip) ───────────────────────────────

    fn set(langs: &[EnrichmentLanguage]) -> Vec<EnrichmentLanguage> {
        langs.to_vec()
    }

    #[test]
    fn plan_runs_available_and_skips_absent_with_reason() {
        let present = set(&[EnrichmentLanguage::Rust, EnrichmentLanguage::TypeScript]);
        // Only TypeScript's toolchain is available.
        let available = |l: EnrichmentLanguage| l == EnrichmentLanguage::TypeScript;
        let (to_run, skipped) = plan_languages(&present, &available);
        assert_eq!(to_run, vec![EnrichmentLanguage::TypeScript]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].language, "rust");
        assert!(
            skipped[0].reason.contains("rust-analyzer not found"),
            "reader-frame reason names the missing tool: {}",
            skipped[0].reason
        );
        assert!(
            skipped[0]
                .reason
                .contains("rustup component add rust-analyzer"),
            "reason carries the install next-action: {}",
            skipped[0].reason
        );
    }

    #[test]
    fn plan_all_absent_skips_all_runs_none() {
        let present = set(&[EnrichmentLanguage::Rust]);
        let available = |_l: EnrichmentLanguage| false;
        let (to_run, skipped) = plan_languages(&present, &available);
        assert!(to_run.is_empty(), "no toolchain → nothing runs");
        assert_eq!(
            skipped.len(),
            1,
            "the eligible language is honestly skipped"
        );
    }

    #[test]
    fn plan_no_eligible_languages_is_clean() {
        let present = set(&[]);
        let available = |_l: EnrichmentLanguage| true;
        let (to_run, skipped) = plan_languages(&present, &available);
        assert!(to_run.is_empty());
        assert!(
            skipped.is_empty(),
            "no eligible edges → no skip, no run (clean)"
        );
    }

    // ── resolver_toolchain_available: the Java (env-only) branch is deterministic ──────────────

    #[test]
    fn java_toolchain_gated_on_jdtls_path() {
        let root = Path::new("/nonexistent/repo");
        assert!(
            resolver_toolchain_available(EnrichmentLanguage::Java, root, &[], Some("/opt/jdtls")),
            "Java available iff a jdtls path is configured"
        );
        assert!(
            !resolver_toolchain_available(EnrichmentLanguage::Java, root, &[], None),
            "no jdtls path → Java skipped"
        );
    }

    // ── resolver_toolchain_available: TypeScript uses the shared per-context locator ────────────────
    #[test]
    fn typescript_available_when_a_nested_context_has_tsserver() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo_root = tmp.path();
        // tsserver ONLY under a nested package (the shape the repo-root-only probe used to miss).
        let ctx = repo_root.join("frontend/web");
        let bin = ctx.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tsserver"), "#!/bin/sh\n").unwrap();

        assert!(
            resolver_toolchain_available(EnrichmentLanguage::TypeScript, repo_root, &[ctx], None,),
            "a nested-package tsserver makes TypeScript available (shared locator walks up)"
        );
        // No contexts at all → not available (nothing to walk up from; $PATH is the only other source,
        // deliberately not exercised here so the assertion is host-independent).
        let empty: [PathBuf; 0] = [];
        assert!(
            !resolver_toolchain_available(EnrichmentLanguage::TypeScript, repo_root, &empty, None,)
                || binary_on_path("tsserver"),
            "no contexts → available only if the host happens to have tsserver on $PATH"
        );
    }

    // ── EnrichCoordinator: the supersede generation counter + the run slot ─────────────────────

    #[test]
    fn generation_bumps_and_supersede_is_detectable() {
        let coord = EnrichCoordinator::new();
        assert_eq!(coord.current_generation("r1"), 0, "never triggered → 0");
        let g1 = coord.bump_generation("r1");
        assert_eq!(g1, 1);
        // A queued pass captured g1; a newer trigger bumps to g2 → the queued pass is superseded.
        let g2 = coord.bump_generation("r1");
        assert_eq!(g2, 2);
        assert_ne!(
            coord.current_generation("r1"),
            g1,
            "the g1 pass sees a newer generation → superseded"
        );
        // A different repo has its own independent counter.
        assert_eq!(coord.bump_generation("r2"), 1);
    }

    #[test]
    fn run_slot_is_exclusive() {
        let coord = EnrichCoordinator::new();
        let held = coord.try_acquire_run_slot();
        assert!(held.is_some(), "first take succeeds");
        assert!(
            coord.try_acquire_run_slot().is_none(),
            "second take fails while held (one enrichment at a time per daemon)"
        );
        drop(held);
        assert!(coord.try_acquire_run_slot().is_some(), "slot frees on drop");
    }

    // ── EnrichCoordinator::activity_state — the queued/running/idle doctor lifecycle signal ─────────

    // The signal that lets `rmap doctor` tell "queued" from the false "none yet — runs after the next
    // index" (review-0 item 1): a spawned pass is queued; a lock-holding pass is running; a finished
    // pass is idle.
    #[test]
    fn activity_state_tracks_queued_running_and_idle() {
        let coord = EnrichCoordinator::new();
        let db = Path::new("/nonexistent/activity-state-test.db");
        assert_eq!(coord.activity_state(), "idle", "nothing in flight → idle");
        assert!(
            !coord.auto_enrichment_in_flight_for_db(db),
            "no pass for this db → not in flight"
        );

        // A spawned-but-not-yet-running pass is QUEUED (in flight, not holding the write lock) — the
        // exact state that must NOT render as "none yet — runs after the next index".
        let flight = coord.enter_flight(db);
        assert_eq!(
            coord.activity_state(),
            "queued",
            "an in-flight pass not yet holding the lock is queued, not idle/none-yet"
        );
        assert!(
            coord.auto_enrichment_in_flight_for_db(db),
            "a queued pass registers as in-flight for its db"
        );

        // Once it registers as running (holds the DB write lock) it is RUNNING (running wins over the
        // in-flight count, since a running pass is also counted in flight).
        let (running_guard, _flag) = coord.register_running(db);
        assert_eq!(coord.activity_state(), "running");

        // Releasing the lock but still in flight (e.g. between requeue attempts) drops back to queued.
        drop(running_guard);
        assert_eq!(coord.activity_state(), "queued");

        // The pass thread exiting clears the in-flight mark → idle.
        drop(flight);
        assert_eq!(coord.activity_state(), "idle", "pass finished → idle");
    }

    // ── summarize_outcome: the daemon-log wording is honest about all fates ────────────────────

    #[test]
    fn summarize_covers_ran_skipped_and_empty() {
        let ran = EnrichPassOutcome {
            eligible_count: 100,
            enriched_count: 81,
            promoted_count: 40,
            enrichment_rate: 81.0,
            skipped: vec![],
            not_attempted_count: 0,
            skipped_contexts: vec![],
            // No funnel → no rejection note; the headline stays exactly as before (backcompat).
            funnel: None,
        };
        assert_eq!(
            summarize_outcome(&ran),
            "enriched 81/100 edges, promoted 40"
        );
        assert_eq!(ran.lifecycle_state(), "completed");

        let all_skipped = EnrichPassOutcome {
            eligible_count: 0,
            enriched_count: 0,
            promoted_count: 0,
            enrichment_rate: 0.0,
            skipped: vec![SkippedLanguage {
                language: "rust".to_string(),
                reason: "rust-analyzer not found — install rust-analyzer".to_string(),
            }],
            not_attempted_count: 0,
            skipped_contexts: vec![],
            funnel: None,
        };
        assert_eq!(all_skipped.lifecycle_state(), "skipped");
        assert!(summarize_outcome(&all_skipped).contains("skipped: rust"));

        let empty = EnrichPassOutcome {
            eligible_count: 0,
            enriched_count: 0,
            promoted_count: 0,
            enrichment_rate: 0.0,
            skipped: vec![],
            not_attempted_count: 0,
            skipped_contexts: vec![],
            funnel: None,
        };
        assert_eq!(summarize_outcome(&empty), "nothing to enrich");
        assert_eq!(empty.lifecycle_state(), "completed");
    }

    // ── ENRICH-YIELD-1: the funnel on the completion report (oplog headline + doctor JSON) ──────────

    fn outcome_with_funnel(funnel: Option<PromotionFunnel>) -> EnrichPassOutcome {
        EnrichPassOutcome {
            eligible_count: 100,
            enriched_count: 81,
            promoted_count: 40,
            enrichment_rate: 81.0,
            skipped: vec![],
            not_attempted_count: 0,
            skipped_contexts: vec![],
            funnel,
        }
    }

    // The oplog headline names the top rejecting classes (reader frame) so the completion line says
    // WHY only 40 of 78 promoted — not just the bare "promoted 40".
    #[test]
    fn summarize_headline_names_top_rejections_when_funnel_present() {
        let funnel = PromotionFunnel::from_counts(
            78,
            40,
            &[
                ("external_type".to_string(), 20),
                ("method_not_found_on_class".to_string(), 18),
                ("ambiguous_class_multiple_definitions".to_string(), 0), // measured-absent → hidden
            ]
            .into_iter()
            .collect(),
            &BTreeMap::new(),
        );
        let line = summarize_outcome(&outcome_with_funnel(Some(funnel)));
        // review-2 item 3: the headline states the funnel's OWN denominator (78 resolved candidates),
        // then names the top rejecting classes. The fixture is the reviewer's 100/81/78/40 case.
        assert!(
            line.starts_with(
                "enriched 81/100 edges, promoted 40/78 resolved candidates (top rejections:"
            ),
            "the candidates→promoted funnel headline is rendered and the rejection note appended: {line}"
        );
        // The promotion denominator is the funnel's candidates (78) — NOT the enriched (81) or eligible
        // (100) counts. This is the no-conflation assertion review-2 item 3 requires (differing values).
        assert!(
            line.contains("promoted 40/78"),
            "denominator is candidates: {line}"
        );
        assert!(
            !line.contains("40/81") && !line.contains("40/100"),
            "promotion must NOT be denominated against enriched/eligible: {line}"
        );
        assert!(
            line.contains(
                "receiver type is external to this repo (a std/library type or language primitive) 20"
            ),
            "dominant class named in reader frame with its count: {line}"
        );
        assert!(
            line.contains("method isn't defined on the resolved class or enum 18"),
            "second class named too: {line}"
        );
        assert!(
            !line.contains("gate"),
            "no pipeline-internal 'gate N' in the reader line: {line}"
        );
    }

    // Honest zero-work: a completed pass whose promotion found no candidates (Some funnel, but
    // candidates=0) appends NO rejection note — nothing was rejected, so nothing is claimed.
    #[test]
    fn summarize_headline_has_no_note_when_nothing_rejected() {
        let funnel =
            PromotionFunnel::from_counts(0, 0, &std::collections::HashMap::new(), &BTreeMap::new());
        let line = summarize_outcome(&outcome_with_funnel(Some(funnel)));
        assert_eq!(line, "enriched 81/100 edges, promoted 40");
    }

    // The doctor JSON (`last_enrichment`) carries the full breakdown under `promotion_funnel` when a
    // funnel exists, and OMITS the key entirely when it does not (honest "no data", not measured-zero).
    #[test]
    fn to_json_carries_promotion_funnel_only_when_present() {
        let funnel = PromotionFunnel::from_counts(
            78,
            40,
            &[("external_type".to_string(), 38)].into_iter().collect(),
            // Ground-truth entries: 78 reach gates 1–4 (gate 2 is the no-op config-opt-in placeholder);
            // gate 4 rejects 38 external → 40 survive and pass through gates 7/8/5/6 to promotion.
            &[
                (1u8, 78usize),
                (2, 78),
                (3, 78),
                (4, 78),
                (7, 40),
                (8, 40),
                (5, 40),
                (6, 40),
            ]
            .into_iter()
            .collect(),
        );
        let with =
            EnrichmentReport::new("r".to_string(), outcome_with_funnel(Some(funnel))).to_json();
        let pf = &with["promotion_funnel"];
        assert_eq!(pf["candidates"], 78);
        assert_eq!(pf["promoted"], 40);
        assert_eq!(pf["rejected"], 38);
        assert_eq!(pf["rejections"][0]["reason"], "external_type");
        assert_eq!(pf["rejections"][0]["gate"], 4);
        assert_eq!(
            pf["rejections"][0]["label"],
            "receiver type is external to this repo (a std/library type or language primitive)"
        );
        // The per-gate waterfall reaches the doctor JSON too (eval order; gate 4 entered 78, rejected
        // 38 external). This is the §2.1 per-gate accounting on the product surface.
        let gates = pf["gates"]
            .as_array()
            .expect("gates waterfall present in doctor JSON");
        let gate4 = gates
            .iter()
            .find(|g| g["gate"] == 4)
            .expect("gate 4 present");
        assert_eq!(gate4["entered"], 78);
        assert_eq!(gate4["rejected"], 38);
        // Gate order in the JSON is evaluation order, not numeric — all eight documented gates.
        let order: Vec<u64> = gates.iter().filter_map(|g| g["gate"].as_u64()).collect();
        assert_eq!(order, vec![1, 2, 3, 4, 7, 8, 5, 6]);
        // Gate 2 (config-opt-in placeholder) reaches the doctor JSON as a no-op stage: entered, 0
        // rejected.
        let gate2 = gates
            .iter()
            .find(|g| g["gate"] == 2)
            .expect("gate 2 present");
        assert_eq!(gate2["entered"], 78);
        assert_eq!(gate2["rejected"], 0);

        let without = EnrichmentReport::new("r".to_string(), outcome_with_funnel(None)).to_json();
        assert!(
            without.get("promotion_funnel").is_none(),
            "no funnel → the key is absent, not null-or-zero: {without}"
        );
        // The pre-existing keys are unchanged whether or not a funnel is attached.
        assert_eq!(without["promoted_count"], 40);
        assert_eq!(without["state"], "completed");
    }

    // ── ENRICH-LIFECYCLE-1 running-yield: the yield signal + the requeue mapping ───────────────────

    // The signal an explicit index/refresh sends: register a running pass's cancel flag, then
    // `request_yield_for_db` latches it — but only for the matching DB, and only while registered.
    #[test]
    fn request_yield_for_db_latches_the_running_pass_flag() {
        let coord = EnrichCoordinator::new();
        // Nonexistent paths → canon_db falls back to the raw path, so the keys compare deterministically.
        let db = Path::new("/nonexistent/enrich-yield-test.db");
        let other = Path::new("/nonexistent/other.db");

        let (guard, flag) = coord.register_running(db);
        assert!(
            !flag.is_cancelled(),
            "a fresh running pass is not cancelled"
        );

        // A yield for a DIFFERENT db must not touch this pass (an index on DB-A never cancels DB-B).
        coord.request_yield_for_db(other);
        assert!(
            !flag.is_cancelled(),
            "an explicit write on another DB must not cancel this pass"
        );

        // A yield for THIS db latches the flag — the pass polls this at its batch boundaries.
        coord.request_yield_for_db(db);
        assert!(
            flag.is_cancelled(),
            "an explicit index/refresh on this DB makes the running pass yield"
        );

        // The guard deregisters on drop → a later yield is a no-op (no running pass to signal).
        drop(guard);
        let (_g2, flag2) = coord.register_running(db);
        coord.request_yield_for_db(db);
        // (flag2 is a fresh registration; the prior request is not replayed onto it — proven above by
        // latching, here by the deregister removing the old entry so only flag2 exists.)
        assert!(
            flag2.is_cancelled(),
            "a re-registered pass takes fresh yield signals"
        );
    }

    // ── review-1: the acquire→register lost-yield window ───────────────────────────────────────────

    // THE regression test for review-1's blocking defect. An explicit index/refresh calls
    // `request_yield_for_db` AFTER a pass took the DB write lock but BEFORE it registered its cancel
    // flag (the acquire→register window). With no flag to latch, the request must NOT be a lost no-op:
    // it records a PENDING marker that the registering pass ADOPTS, so the pass starts already-cancelled
    // and yields instead of running to completion while the explicit write blocks. Before the fix this
    // asserted-false (register returned a fresh un-cancelled flag) — that WAS the defect.
    #[test]
    fn yield_requested_before_registration_is_adopted_not_lost() {
        let coord = EnrichCoordinator::new();
        let db = Path::new("/nonexistent/window-race.db");

        // Explicit write requests a yield while the pass is still in the acquire→register window (no
        // flag registered) → recorded as pending, not lost.
        coord.request_yield_for_db(db);

        // The pass registers (as it does right after acquiring the write lock) → it MUST adopt the
        // pending yield and start already-cancelled.
        let (_guard, flag) = coord.register_running(db);
        assert!(
            flag.is_cancelled(),
            "a yield requested in the acquire→register window must be adopted by the registering pass, not lost"
        );
    }

    // The common case must NOT regress into a spurious yield: every explicit write calls
    // `request_yield_for_db`, usually with nothing running, leaving a pending marker. Once that write
    // owns the lock it calls `clear_pending_yield`; the NEXT pass must then register un-cancelled.
    #[test]
    fn clear_pending_yield_stops_a_stale_marker_from_cancelling_the_next_pass() {
        let coord = EnrichCoordinator::new();
        let db = Path::new("/nonexistent/stale-pending.db");

        coord.request_yield_for_db(db); // nothing running → pending marker
        coord.clear_pending_yield(db); // the write acquired the lock and proceeded → marker cleared

        let (_guard, flag) = coord.register_running(db);
        assert!(
            !flag.is_cancelled(),
            "a pending marker cleared by the proceeding write must not spuriously cancel a later pass"
        );
    }

    // The pending marker is per-DB: a window-yield for DB-A must never be adopted by a pass on DB-B.
    #[test]
    fn pending_yield_is_scoped_to_its_db() {
        let coord = EnrichCoordinator::new();
        let db_a = Path::new("/nonexistent/pending-a.db");
        let db_b = Path::new("/nonexistent/pending-b.db");

        coord.request_yield_for_db(db_a); // window-yield for A only

        let (_gb, flag_b) = coord.register_running(db_b);
        assert!(
            !flag_b.is_cancelled(),
            "a pending yield for DB-A must not cancel a pass registering on DB-B"
        );
        let (_ga, flag_a) = coord.register_running(db_a);
        assert!(
            flag_a.is_cancelled(),
            "the pass on DB-A adopts its own pending window-yield"
        );
    }

    // The requeue mapping: a pass whose flag was latched mid-run YIELDS (→ requeue), not records; a
    // clean pass RANs; a pass error stays Failed regardless of the flag.
    #[test]
    fn classify_completed_attempt_maps_cancelled_to_yielded() {
        let outcome = EnrichPassOutcome {
            eligible_count: 50,
            enriched_count: 30,
            promoted_count: 10,
            enrichment_rate: 60.0,
            skipped: vec![],
            not_attempted_count: 0,
            skipped_contexts: vec![],
            funnel: None,
        };

        match classify_completed_attempt(Ok(outcome.clone()), false) {
            EnrichAttempt::Ran(o) => {
                assert_eq!(
                    o.promoted_count, 10,
                    "a completed, un-yielded pass records its real outcome"
                )
            }
            _ => panic!("a completed, un-yielded pass must Ran"),
        }

        assert!(
            matches!(
                classify_completed_attempt(Ok(outcome), true),
                EnrichAttempt::Yielded(_)
            ),
            "a pass whose flag latched mid-run yields (requeue), never records the partial"
        );

        assert!(
            matches!(
                classify_completed_attempt(Err("boom".to_string()), true),
                EnrichAttempt::Failed(_)
            ),
            "a pass error stays Failed regardless of the yield flag"
        );
    }

    // DAEMON-CRASH-RECOVERY-1 (F8, review-0 item a): a real background enrichment pass writes an
    // explicit op-lifecycle START + OUTCOME pair to the log sink — the same `op <op> <outcome> (repo
    // <repo>)` shape as index/refresh — NOT the old ad-hoc "enrichment: …" summary. Drives the real
    // `run_auto_enrich` on a repo with a READY snapshot but no eligible edges ("nothing to enrich"),
    // so no live LSP toolchain is needed.
    #[test]
    fn run_auto_enrich_logs_the_op_lifecycle_outcome_line() {
        use crate::registry::RepoRegistry;
        use repo_graph_storage::types::{CreateSnapshotInput, Repo, UpdateSnapshotStatusInput};

        crate::oplog::enable_oplog_capture_for_test();
        // The chained retention pass would spawn its own thread + line; disable it so this test stays
        // focused on the enrich outcome line and leaves no detached thread touching the tempdir.
        crate::retention_pass::set_auto_retention_for_test(false);

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("enrich_outcome.db");
        // Unique repo → the process-global (non-draining) capture buffer is filtered to THIS test.
        let repo = "enrich-outcome-repo";
        {
            let storage = StorageConnection::open(&db_path).unwrap();
            storage
                .add_repo(&Repo {
                    repo_uid: repo.to_string(),
                    name: repo.to_string(),
                    root_path: ".".to_string(),
                    default_branch: None,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    metadata_json: None,
                })
                .unwrap();
            // A READY snapshot with no nodes/edges → no eligible unresolved edges → "nothing to enrich".
            let snap = storage
                .create_snapshot(&CreateSnapshotInput {
                    repo_uid: repo.to_string(),
                    kind: "full".to_string(),
                    basis_ref: None,
                    basis_commit: None,
                    parent_snapshot_uid: None,
                    label: None,
                    toolchain_json: None,
                })
                .unwrap();
            storage
                .update_snapshot_status(&UpdateSnapshotStatusInput {
                    snapshot_uid: snap.snapshot_uid,
                    status: "ready".to_string(),
                    completed_at: None,
                })
                .unwrap();
        }
        let db_path = db_path.canonicalize().unwrap();
        let state = Arc::new(DaemonState::with_registry(
            RepoRegistry::empty_non_persistent(),
        ));
        let my_generation = state.enrich_coord().bump_generation(repo);
        run_auto_enrich(&state, &db_path, repo, repo, my_generation);

        let lines: Vec<String> = crate::oplog::oplog_lines_for_test()
            .into_iter()
            .filter(|l| l.contains(repo))
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("op enrich started")),
            "the pass logs an op-START line: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("op enrich completed")),
            "the pass logs an op-lifecycle OUTCOME line (not the old ad-hoc 'enrichment: …' summary): {lines:?}"
        );
    }

    // ── DAEMON-CRASH-RECOVERY-1 (F8, review-2 item 1): every STARTED enrich op is CLOSED ───────────

    /// Seed a repo with one snapshot in `status` ("ready" or "building"), returning the canonical db
    /// path. Two callers (the interrupted + failed F8 proofs below) with the same ~20-line seed —
    /// earned over duplication; the pre-existing "completed" proof predates it and stays inlined
    /// (minimal-change discipline).
    fn seed_repo_with_snapshot(dir: &Path, repo: &str, status: &str) -> PathBuf {
        use repo_graph_storage::types::{CreateSnapshotInput, Repo, UpdateSnapshotStatusInput};
        let db_path = dir.join(format!("{repo}.db"));
        let storage = StorageConnection::open(&db_path).unwrap();
        storage
            .add_repo(&Repo {
                repo_uid: repo.to_string(),
                name: repo.to_string(),
                root_path: ".".to_string(),
                default_branch: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap();
        if status == "ready" {
            storage
                .update_snapshot_status(&UpdateSnapshotStatusInput {
                    snapshot_uid: snap.snapshot_uid,
                    status: "ready".to_string(),
                    completed_at: None,
                })
                .unwrap();
        }
        db_path.canonicalize().unwrap()
    }

    // THE review-2 finding: an enrich pass that logged an op-START and is then CANCELLED at a batch
    // boundary (an explicit index/refresh latched its yield flag) must close that start with a terminal
    // `interrupted` outcome. Before the fix it requeued SILENTLY, leaving a started op with no
    // completed/interrupted/failed line. Deterministic + hermetic (no LSP): a pending yield adopted in
    // the acquire→register window makes the real `try_enrich_attempt` run already-cancelled and map to
    // Yielded-after-start; the zero-eligible snapshot means the pipeline body is never even reached.
    #[test]
    fn a_cancelled_started_enrich_closes_its_start_with_an_interrupted_outcome() {
        use crate::registry::RepoRegistry;
        crate::oplog::enable_oplog_capture_for_test();
        let repo = "enrich-f8-interrupted-repo"; // unique → parallel-safe capture filter
        let dir = tempfile::tempdir().unwrap();
        let db_path = seed_repo_with_snapshot(dir.path(), repo, "ready");

        let state = Arc::new(DaemonState::with_registry(
            RepoRegistry::empty_non_persistent(),
        ));
        let gen = state.enrich_coord().bump_generation(repo);
        // Latch a yield in the acquire→register window → `register_running` adopts it, so the pass
        // starts already-cancelled and yields at its first batch boundary (a started, interrupted run).
        state.enrich_coord().request_yield_for_db(&db_path);

        let attempt = try_enrich_attempt(&state, &db_path, repo, repo, gen);
        assert!(
            matches!(attempt, EnrichAttempt::Yielded(_)),
            "a cancel-after-start maps to Yielded"
        );

        let lines: Vec<String> = crate::oplog::oplog_lines_for_test()
            .into_iter()
            .filter(|l| l.contains(repo))
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("op enrich started")),
            "the started run logged an op-START: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("op enrich interrupted")),
            "the cancelled run's start is CLOSED by an interrupted outcome (review-2 fix): {lines:?}"
        );
    }

    // The failure disposition of the same invariant: an enrich pass that FAILS (its snapshot is not
    // enrichable) closes its op-START with a terminal `failed` outcome. Deterministic + hermetic: a
    // repo whose latest snapshot is `building` (never ready) → `run_enrich_pass` errors →
    // `EnrichAttempt::Failed` → `run_auto_enrich` logs the failed outcome. Retention forced OFF so the
    // chained pass is a cheap no-op that spawns no thread racing tempdir teardown.
    #[test]
    fn a_failed_enrich_closes_its_start_with_a_failed_outcome() {
        use crate::registry::RepoRegistry;
        crate::oplog::enable_oplog_capture_for_test();
        crate::retention_pass::set_auto_retention_for_test(false);
        let repo = "enrich-f8-failed-repo";
        let dir = tempfile::tempdir().unwrap();
        let db_path = seed_repo_with_snapshot(dir.path(), repo, "building");

        let state = Arc::new(DaemonState::with_registry(
            RepoRegistry::empty_non_persistent(),
        ));
        let gen = state.enrich_coord().bump_generation(repo);
        run_auto_enrich(&state, &db_path, repo, repo, gen);

        let lines: Vec<String> = crate::oplog::oplog_lines_for_test()
            .into_iter()
            .filter(|l| l.contains(repo))
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("op enrich started")),
            "the attempt logged an op-START: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("op enrich failed")),
            "a failed run's start is CLOSED by a failed outcome: {lines:?}"
        );
    }
}
