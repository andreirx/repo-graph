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
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use enrichment::{
    EligibilityQuery, EnrichmentConfig, EnrichmentLanguage, EnrichmentPipeline,
    EnrichmentStoragePort, ResolverRegistry,
};
use jdtls_resolver::{JdtlsConfig, JdtlsResolver};
use parking_lot::{Mutex, MutexGuard};
use repo_graph_storage::StorageConnection;
use rust_analyzer_resolver::RustAnalyzerResolver;
use tsserver_resolver::TsServerResolver;

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

/// Is the resolver toolchain for `lang` present, given the repo root and the resolved `jdtls_path`?
///
/// Mirrors how each resolver actually locates its LSP binary (verified against the resolver crates):
/// - **Rust:** `rust-analyzer` on `$PATH` (`RustAnalyzerSession::start` hardcodes the binary name).
/// - **TypeScript:** `tsserver` on `$PATH` OR the repo's `node_modules/.bin/tsserver` (the two places
///   `TsServerResolver`'s `find_tsserver` looks, minus the explicit config path the pass never sets).
/// - **Java:** a `jdtls_path` is configured (jdtls has NO PATH discovery — env/flag only).
///
/// This is the SINGLE source the pass uses for BOTH which resolvers to register AND which languages
/// to honestly skip, so the doctor skip line can never promise a remedy the pass did not attempt.
pub fn resolver_toolchain_available(
    lang: EnrichmentLanguage,
    repo_root: &Path,
    jdtls_path: Option<&str>,
) -> bool {
    match lang {
        EnrichmentLanguage::Rust => binary_on_path("rust-analyzer"),
        EnrichmentLanguage::TypeScript => {
            binary_on_path("tsserver") || repo_root.join("node_modules/.bin/tsserver").is_file()
        }
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

/// The stable lowercase language token used in the doctor JSON + skip lines.
fn language_token(lang: EnrichmentLanguage) -> &'static str {
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
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
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
            "finished_secs_ago": self.at.elapsed().as_secs(),
        })
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
    /// Count of spawned-but-not-finished auto passes (`enter_flight` .. pass-thread exit). `Arc` so
    /// [`FlightGuard`] can hold a clone across the detached pass thread; read by `activity_state`.
    in_flight: Arc<AtomicUsize>,
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
        } else if self.in_flight.load(Ordering::Relaxed) > 0 {
            "queued"
        } else {
            "idle"
        }
    }

    /// Mark an auto pass as in-flight (spawned) and return a guard that clears the mark on drop.
    /// [`spawn_auto_enrich`] calls this and moves the guard into the pass thread, so the "queued"
    /// signal is live from spawn until the pass thread exits on ANY terminal state (ran / failed /
    /// superseded / deferred). See [`activity_state`](Self::activity_state).
    pub fn enter_flight(&self) -> FlightGuard {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        FlightGuard {
            counter: Arc::clone(&self.in_flight),
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

/// RAII guard decrementing the in-flight auto-pass count on drop (mirrors [`RunningPassGuard`]). Held
/// by the spawned pass thread for its whole life, so [`EnrichCoordinator::activity_state`] reports
/// "queued"/"running" for exactly as long as a pass is in flight. `Send + 'static` (holds an owned
/// `Arc`, not a coordinator borrow), so it moves into the detached pass thread.
pub struct FlightGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
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
    jdtls_path: Option<&str>,
    available: &dyn Fn(EnrichmentLanguage) -> bool,
    cancel: &dyn Fn() -> bool,
) -> Result<EnrichPassOutcome, String> {
    // Resolve the latest READY snapshot + the repo root (the pipeline uses the same root). A fresh
    // connection scoped to this probe; the pipeline opens its own (it takes ownership).
    let (snapshot_uid, present) = {
        let storage = StorageConnection::open(db_path).map_err(|e| e.to_string())?;
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
        (snapshot.snapshot_uid, present)
    };

    // Toolchain plan (which languages RUN vs are honestly skipped) + the resolver registry. A test
    // backend (`set_test_registry_builder`) short-circuits BOTH the real toolchain probe and the real
    // resolver construction, so the daemon-level running-yield proof can drive this REAL pass with a
    // fake cancellable resolver (no live LSP). Production never installs it → real `plan_languages` +
    // real resolvers, unchanged.
    let test_registry = test_registry_builder();
    let (to_run, skipped) = match &test_registry {
        // The test provides resolvers for every eligible-present language → run them all (availability
        // is exercised separately by `plan_languages`' unit tests + the toolchain-skip proof).
        Some(_) => (present.clone(), Vec::new()),
        None => plan_languages(&present, available),
    };

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

    let pipeline_storage = StorageConnection::open(db_path).map_err(|e| e.to_string())?;
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

    let available = |lang: EnrichmentLanguage| {
        resolver_toolchain_available(
            lang,
            Path::new(repo_display),
            jdtls_path_from_env().as_deref(),
        )
    };
    let run_result = run_enrich_pass(
        db_path,
        repo_uid,
        jdtls_path_from_env().as_deref(),
        &available,
        &cancel,
    );
    classify_completed_attempt(run_result, cancel_flag.is_cancelled())
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
    let flight = state.enrich_coord().enter_flight();
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
                eprintln!(
                    "enrichment: {} (repo {repo_uid})",
                    summarize_outcome(&outcome)
                );
                state.record_enrichment_report(EnrichmentReport::new(
                    repo_display.to_string(),
                    outcome,
                ));
                chain_retention(state, db_path, repo_uid, repo_display);
                return;
            }
            EnrichAttempt::Failed(e) => {
                eprintln!("enrichment: pass failed for repo {repo_uid}: {e}");
                // Still chain retention — the index succeeded; cleanup should not hinge on enrich.
                chain_retention(state, db_path, repo_uid, repo_display);
                return;
            }
            EnrichAttempt::Superseded => {
                // A newer index's pass owns this repo now; it will chain retention. Drop silently.
                return;
            }
            EnrichAttempt::Yielded(_reason) => {
                std::thread::sleep(REQUEUE_BACKOFF);
            }
        }
    }
    eprintln!(
        "enrichment: deferred for repo {repo_uid} — the repo stayed busy for {}s; the next successful index/refresh will retry",
        (REQUEUE_MAX_ATTEMPTS as u64) * REQUEUE_BACKOFF.as_secs()
    );
    // Deferred (never ran) — still hand off to retention so cleanup runs this cycle.
    chain_retention(state, db_path, repo_uid, repo_display);
}

/// Chain the background retention pass after enrichment (the retention slice's "after enrichment
/// promotion once ENRICH-LIFECYCLE-1 lands" hook). Sequencing enrichment BEFORE retention — rather
/// than spawning both from the completion path — is what keeps them from contending: retention's
/// bounded requeue never competes with (and so is never starved by) a long enrichment holding the
/// write lock. No-op when retention is opted out (`spawn_auto_retention` checks it).
fn chain_retention(state: &Arc<DaemonState>, db_path: &Path, repo_uid: &str, repo_display: &str) {
    crate::retention_pass::spawn_auto_retention(
        Arc::clone(state),
        db_path.to_path_buf(),
        repo_uid.to_string(),
        repo_display.to_string(),
    );
}

/// Reader-frame one-liner for the daemon log: "enriched N/M edges, promoted P (skipped: rust)" |
/// "nothing to enrich" | "skipped: rust-analyzer not found — …".
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
            "enriched {}/{} edges, promoted {}{skip_note}",
            o.enriched_count, o.eligible_count, o.promoted_count
        )
    }
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
            resolver_toolchain_available(EnrichmentLanguage::Java, root, Some("/opt/jdtls")),
            "Java available iff a jdtls path is configured"
        );
        assert!(
            !resolver_toolchain_available(EnrichmentLanguage::Java, root, None),
            "no jdtls path → Java skipped"
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
        assert_eq!(coord.activity_state(), "idle", "nothing in flight → idle");

        // A spawned-but-not-yet-running pass is QUEUED (in flight, not holding the write lock) — the
        // exact state that must NOT render as "none yet — runs after the next index".
        let flight = coord.enter_flight();
        assert_eq!(
            coord.activity_state(),
            "queued",
            "an in-flight pass not yet holding the lock is queued, not idle/none-yet"
        );

        // Once it registers as running (holds the DB write lock) it is RUNNING (running wins over the
        // in-flight count, since a running pass is also counted in flight).
        let db = Path::new("/nonexistent/activity-state-test.db");
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
        };
        assert_eq!(all_skipped.lifecycle_state(), "skipped");
        assert!(summarize_outcome(&all_skipped).contains("skipped: rust"));

        let empty = EnrichPassOutcome {
            eligible_count: 0,
            enriched_count: 0,
            promoted_count: 0,
            enrichment_rate: 0.0,
            skipped: vec![],
        };
        assert_eq!(summarize_outcome(&empty), "nothing to enrich");
        assert_eq!(empty.lifecycle_state(), "completed");
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
}
