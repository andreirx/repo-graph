//! Background embed pass (EMBED-SEED-IMPL-1, spec §5), mirroring the
//! ENRICH-LIFECYCLE-1 shape: a detached, cancellable pass spawned after every
//! index/refresh, with its OWN coordinator (generation-supersede + a daemon-wide
//! run slot) ordered into the maintenance chain **enrich → seed → retention**.
//!
//! The pass READS the READY snapshot's corpus + the working tree, embeds via the
//! option-(a) `Embedder`, and publishes the `.vec` sidecar by atomic rename. It
//! writes NO SQL. On a missing/unreachable model it skips **honestly** (never an
//! error), exactly as auto-enrich skips with no resolver toolchain.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, MutexGuard};

use repo_graph_seed::pass::{build_store, BuildConfig, BuildOutcome};

use crate::cancel::CancelFlag;
use crate::seed::{seed_enabled, sidecar_path, EndpointEmbedder, SeedEndpointConfig};
use crate::state::DaemonState;

const REQUEUE_MAX_ATTEMPTS: u32 = 60;
const REQUEUE_BACKOFF: Duration = Duration::from_millis(1000);

/// The most-recent completed pass, for oplog/doctor honesty.
#[derive(Debug, Clone)]
pub struct SeedReport {
    pub repo_display: String,
    pub outcome: String,
    pub admitted: usize,
    /// Of `admitted`, vectors copied forward from the prior sidecar (spec §5
    /// incremental refresh) — these made no embed call this pass.
    pub reused: usize,
    pub drifted: usize,
    pub corpus_omitted: usize,
}

/// Daemon-wide seed lifecycle coordination — per-repo trigger generation
/// (supersede rule), a single "one seed pass at a time" run slot, and the running
/// pass's cancel flag (so a newer index cancels an in-flight older pass).
#[derive(Debug, Default)]
pub struct SeedCoordinator {
    generations: Mutex<BTreeMap<String, u64>>,
    run_slot: Mutex<()>,
    running: Mutex<BTreeMap<PathBuf, CancelFlag>>,
    last_report: Mutex<Option<SeedReport>>,
}

impl SeedCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bump_generation(&self, repo_uid: &str) -> u64 {
        let mut g = self.generations.lock();
        let c = g.entry(repo_uid.to_string()).or_insert(0);
        *c += 1;
        *c
    }

    pub fn current_generation(&self, repo_uid: &str) -> u64 {
        self.generations.lock().get(repo_uid).copied().unwrap_or(0)
    }

    fn try_acquire_run_slot(&self) -> Option<MutexGuard<'_, ()>> {
        self.run_slot.try_lock()
    }

    /// Cancel an in-flight pass for `db_path` (a newer index superseding it).
    pub fn request_cancel_for_db(&self, db_path: &Path) {
        if let Some(flag) = self.running.lock().get(db_path) {
            flag.cancel();
        }
    }

    /// Is an embed pass currently in flight for `db_path`? The doctor's `building`
    /// signal (spec §9) — a running pass registers here for its duration.
    pub fn is_running_for_db(&self, db_path: &Path) -> bool {
        self.running.lock().contains_key(db_path)
    }

    fn register_running(&self, db_path: &Path) -> (RunningGuard<'_>, CancelFlag) {
        let flag = CancelFlag::new();
        self.running
            .lock()
            .insert(db_path.to_path_buf(), flag.clone());
        (
            RunningGuard {
                coord: self,
                key: db_path.to_path_buf(),
            },
            flag,
        )
    }

    pub fn record_report(&self, report: SeedReport) {
        *self.last_report.lock() = Some(report);
    }

    pub fn last_report(&self) -> Option<SeedReport> {
        self.last_report.lock().clone()
    }
}

struct RunningGuard<'a> {
    coord: &'a SeedCoordinator,
    key: PathBuf,
}
impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.coord.running.lock().remove(&self.key);
    }
}

enum SeedAttempt {
    Ran(SeedReport),
    Yielded,
    Superseded,
    Skipped(String),
}

/// Is `repo_uid` still referenced by ANY current registry entry? Read fresh under
/// the caller's held DB write guard so it observes a `forget` that completed while
/// this pass was embedding (review-5 #2 publish-time registry recheck — the same
/// discipline `reclaim::base_name_is_registered` uses for gc).
fn repo_uid_is_registered(state: &DaemonState, repo_uid: &str) -> bool {
    state
        .registry()
        .list()
        .iter()
        .any(|e| e.repo_uid == repo_uid)
}

/// The decision the publish gate reaches. Factored out of [`try_seed_attempt`]'s
/// `Built` arm so the forget-vs-seed race fix (review-5 #2) is deterministically
/// testable without a live embedder: a unit test drives exactly this gate with a
/// registered then forgotten repo and asserts the sidecar is published then NOT
/// resurrected. Not a variation seam — one concrete caller (`try_seed_attempt`) plus
/// the regression; the alternative (inline + a full detached-pass integration race)
/// is non-deterministic, which is what the reviewer rejected.
#[derive(Debug)]
enum PublishOutcome {
    Published,
    Yielded,
    Superseded,
    Skipped(String),
}

/// Publish the built sidecar under the SAME writer discipline `forget_repo` / `run_gc`
/// use (review-5 #2: close the forget-vs-seed publication race). The embed that
/// produced `bytes` held NO lock (a 72s cold embed must never block forget/index), so
/// a `forget` can run to completion — deleting the `.vec` AND the registry entry —
/// WHILE this pass embeds. Without coordination the pass would then `atomic_write` the
/// sidecar back for a repo that no longer exists: a resurrected orphan.
///
/// The gate: acquire the base DB's write slot — the ONE slot `forget_repo` / `run_gc`
/// / an index contend on — for the FAST publish, then, under that held guard, re-check
/// BOTH (a) our generation (a newer index superseded) and (b) that the repo is STILL
/// registered (a `forget` completed). Only then atomic_write. This serializes
/// publication against forget's `.vec` deletion and its registry-entry removal:
///   • forget completed first   ⇒ registry recheck fails ⇒ Skipped (no orphan);
///   • this pass publishes first ⇒ forget then deletes the `.vec` cleanly;
///   • concurrent                ⇒ the write-slot mutex serializes them.
/// The slot is held only for the sub-millisecond publish, never the embed.
fn publish_guarded(
    state: &Arc<DaemonState>,
    db_path: &Path,
    repo_uid: &str,
    my_generation: u64,
    sidecar: &Path,
    bytes: &[u8],
) -> PublishOutcome {
    let slot = state.get_or_create_db_runtime_for_new_db(db_path).ok();
    let _publish_guard = match slot.as_ref().map(|rt| rt.try_acquire_write()) {
        Some(Some(g)) => g,
        // Slot held (a forget/index/maintenance write is in flight) or the databases/
        // parent is gone: do NOT publish now. Re-queue — a newer generation, or the
        // settled registry, decides the next attempt.
        Some(None) | None => return PublishOutcome::Yielded,
    };
    // Pre-publish supersede guard: never overwrite a newer generation's store.
    if state.seed_coord().current_generation(repo_uid) != my_generation {
        return PublishOutcome::Superseded;
    }
    // Registry-validity gate (review-5 #2): a `forget` between our corpus read and here
    // removed this repo's registry entry. Read fresh UNDER the held write guard so it
    // observes a forget that raced us (the same recheck discipline as `run_gc`).
    if !repo_uid_is_registered(state, repo_uid) {
        return PublishOutcome::Skipped(
            "repo forgotten during embed pass — sidecar not published".to_string(),
        );
    }
    if let Err(e) = repo_graph_seed::store::atomic_write(sidecar, bytes) {
        return PublishOutcome::Skipped(format!("publish failed: {e}"));
    }
    PublishOutcome::Published
}

/// The maintenance-chain tail after enrich (spec §5): run the seed pass, which
/// itself chains retention on completion. When seeding is disabled, retention is
/// chained directly so the tail is preserved in every configuration.
pub fn chain_seed_then_retention(
    state: &Arc<DaemonState>,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
) {
    if seed_enabled() {
        spawn_auto_seed(
            Arc::clone(state),
            db_path.to_path_buf(),
            repo_uid.to_string(),
            repo_display.to_string(),
        );
    } else {
        crate::retention_pass::spawn_auto_retention(
            Arc::clone(state),
            db_path.to_path_buf(),
            repo_uid.to_string(),
            repo_display.to_string(),
        );
    }
}

/// Spawn the detached background embed pass (no-op + retention passthrough when
/// disabled). Cancels any in-flight older pass for this DB first (supersede).
pub fn spawn_auto_seed(
    state: Arc<DaemonState>,
    db_path: PathBuf,
    repo_uid: String,
    repo_display: String,
) {
    if !seed_enabled() {
        // Preserve the maintenance tail even when seeding is off.
        crate::retention_pass::spawn_auto_retention(state, db_path, repo_uid, repo_display);
        return;
    }
    let my_generation = state.seed_coord().bump_generation(&repo_uid);
    state.seed_coord().request_cancel_for_db(&db_path);
    std::thread::spawn(move || {
        run_auto_seed(&state, &db_path, &repo_uid, &repo_display, my_generation);
    });
}

fn chain_retention(state: &Arc<DaemonState>, db_path: &Path, repo_uid: &str, repo_display: &str) {
    crate::retention_pass::spawn_auto_retention(
        Arc::clone(state),
        db_path.to_path_buf(),
        repo_uid.to_string(),
        repo_display.to_string(),
    );
}

fn run_auto_seed(
    state: &Arc<DaemonState>,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
    my_generation: u64,
) {
    for _ in 0..REQUEUE_MAX_ATTEMPTS {
        match try_seed_attempt(state, db_path, repo_uid, repo_display, my_generation) {
            SeedAttempt::Ran(report) => {
                crate::oplog::log_op_outcome(
                    "seed",
                    repo_uid,
                    None,
                    &format!(
                        "{} (admitted {}, reused {}, drifted {}, omitted {})",
                        report.outcome,
                        report.admitted,
                        report.reused,
                        report.drifted,
                        report.corpus_omitted
                    ),
                );
                state.seed_coord().record_report(report);
                chain_retention(state, db_path, repo_uid, repo_display);
                return;
            }
            SeedAttempt::Skipped(reason) => {
                crate::oplog::log_op_outcome("seed", repo_uid, None, &format!("skipped: {reason}"));
                chain_retention(state, db_path, repo_uid, repo_display);
                return;
            }
            SeedAttempt::Superseded => {
                // A newer index's pass will chain retention.
                return;
            }
            SeedAttempt::Yielded => {
                std::thread::sleep(REQUEUE_BACKOFF);
            }
        }
    }
    // Exhausted requeues — still preserve the retention tail.
    chain_retention(state, db_path, repo_uid, repo_display);
}

/// The config precondition for running an embed pass (review-9 #3). Returns the
/// honest skip reason when `RMAP_SEED_DIM` was SET to an invalid value
/// (`dim_config_error`), so the pass declines rather than build a store at the
/// silently-defaulted dimension. Factored out of [`try_seed_attempt`] so the skip
/// is deterministically testable without process-global env-var races (the same
/// reason `PublishOutcome` was extracted). One concrete caller; no variation axis —
/// a test seam. Rejected simpler: an inline `if cfg.dim_config_error.is_some()`
/// (only testable by mutating the process environment, which races parallel tests).
fn config_skip_reason(cfg: &SeedEndpointConfig) -> Option<String> {
    cfg.dim_config_error
        .as_ref()
        .map(|reason| format!("seed config invalid: {reason}"))
}

fn try_seed_attempt(
    state: &Arc<DaemonState>,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
    my_generation: u64,
) -> SeedAttempt {
    // Supersede: a newer trigger for this repo makes this queued pass stale.
    if state.seed_coord().current_generation(repo_uid) != my_generation {
        return SeedAttempt::Superseded;
    }
    // Daemon-global "one seed at a time" slot.
    let _slot = match state.seed_coord().try_acquire_run_slot() {
        Some(s) => s,
        None => return SeedAttempt::Yielded,
    };
    if state.seed_coord().current_generation(repo_uid) != my_generation {
        return SeedAttempt::Superseded;
    }

    let repo_state = match state.load_repo(db_path, repo_uid) {
        Ok(rs) => rs,
        Err(e) => return SeedAttempt::Skipped(format!("could not load repo: {e}")),
    };

    let cfg = SeedEndpointConfig::from_env();
    // An invalid seed config (e.g. a non-integer / zero `RMAP_SEED_DIM`) must NOT
    // build a store at the silently-defaulted dimension (review-9 #3): every seed
    // surface — query, doctor, AND this pass — declines while `dim_config_error` is
    // set. Skipping honestly (recorded in the oplog + doctor via the report) beats
    // publishing a store pinned to a dim the operator did not choose.
    if let Some(reason) = config_skip_reason(&cfg) {
        return SeedAttempt::Skipped(reason);
    }
    let embedder = match EndpointEmbedder::from_config(&cfg) {
        Ok(e) => e,
        // No reachable / non-loopback model → honest skip (never an error).
        Err(e) => return SeedAttempt::Skipped(format!("{e}")),
    };

    let sidecar = match sidecar_path(db_path) {
        Some(p) => p,
        None => return SeedAttempt::Skipped("cannot derive sidecar path".to_string()),
    };

    // Read the corpus under a BRIEF read lock, then RELEASE the lock (and close the
    // storage connection) BEFORE the slow embed phase. The embed only touches
    // working-tree files + the model, so it must never hold a DB lock (else a
    // 72s cold embed would block forget/index). This scope drops both.
    let entries = {
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match repo_state.storage() {
            Ok(s) => s,
            Err(e) => return SeedAttempt::Skipped(format!("could not open storage: {e}")),
        };
        match repo_graph_seed::SeedCorpusRead::seed_corpus(&storage, repo_uid) {
            Ok(e) => e,
            Err(e) => return SeedAttempt::Skipped(format!("corpus read failed: {e}")),
        }
    };

    let (_running_guard, cancel_flag) = state.seed_coord().register_running(db_path);
    // review-10 #3 — close the check→register supersession window: a
    // `bump_generation` + `request_cancel_for_db` landing after the generation
    // check above but before `register_running` found NO registered flag, so its
    // cancel was lost and this stale pass would embed a full job (rejected only at
    // publication). Now that we are registered (any later bump reaches our flag),
    // one final generation re-check catches every bump from that window.
    if state.seed_coord().current_generation(repo_uid) != my_generation {
        return SeedAttempt::Superseded;
    }
    let cancel = || cancel_flag.is_cancelled();

    // Read working-tree files by repo-relative path (repo_display = canonical root).
    let repo_root = Path::new(repo_display).to_path_buf();
    let read_file = |rel: &str| std::fs::read_to_string(repo_root.join(rel));

    // SELF-POLLUTION-1 §2.4: never EMBED rmap's OWN `map` exhaust or secrets-adjacent
    // `.env*`. Same shared classifier as drift + docs inventory (one truth); the
    // `rmap map` marker read is gated to sidecar-NAMED candidates (honesty rule —
    // a bare name is not evidence). Already-built stores drop these on the next
    // content-hash refresh pass — no migration (DEC-2: filtered at the composition
    // root so `repo-graph-seed` stays a clean pure crate).
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| !seed_path_is_exhaust(&repo_root, &e.path))
        .collect();

    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = cfg.store_key();

    // Spec §5 incremental refresh: load the prior sidecar (validated under the SAME
    // pin) so unchanged files copy their vector forward instead of re-embedding;
    // only changed/new files hit the model. A missing OR pin-incompatible OR corrupt
    // prior ⇒ `None` ⇒ a full (re)build. This is a pure build-time OPTIMIZATION read
    // — its result is neither rendered nor classified to a user, so a best-effort
    // `.ok()` (degrade to full embed) is correct here, distinct from the rendered
    // query/doctor reads that must report unknown-with-reason.
    // Read through the metadata-guarded reader (review-9 #1): an over-budget prior
    // sidecar is rejected without loading it, then degrades to a full re-embed.
    let prior_body = repo_graph_seed::store::read_validated(&sidecar, &key).ok();

    let outcome = build_store(
        entries,
        &embedder,
        read_file,
        cancel,
        &key,
        created_at,
        BuildConfig::default(),
        prior_body.as_ref(),
    );

    match outcome {
        BuildOutcome::Built { bytes, report } => {
            match publish_guarded(state, db_path, repo_uid, my_generation, &sidecar, &bytes) {
                PublishOutcome::Published => SeedAttempt::Ran(SeedReport {
                    repo_display: repo_display.to_string(),
                    outcome: "built".to_string(),
                    admitted: report.admitted,
                    reused: report.reused,
                    drifted: report.drifted,
                    corpus_omitted: report.corpus_omitted,
                }),
                PublishOutcome::Yielded => SeedAttempt::Yielded,
                PublishOutcome::Superseded => SeedAttempt::Superseded,
                PublishOutcome::Skipped(r) => SeedAttempt::Skipped(r),
            }
        }
        BuildOutcome::Cancelled => SeedAttempt::Superseded,
        BuildOutcome::NoCorpus => SeedAttempt::Skipped("no seedable corpus".to_string()),
        BuildOutcome::Embed(e) => SeedAttempt::Skipped(format!("model unavailable: {e}")),
        BuildOutcome::Store(e) => SeedAttempt::Skipped(format!("store encode failed: {e}")),
    }
}

/// Is a corpus path rmap's OWN exhaust (`map` sidecar / `.rgr/`) or a secrets-adjacent
/// `.env*` — i.e. must NOT be embedded (SELF-POLLUTION-1 §2.4)? `.env*` is a pure name
/// rule; the `rmap map` marker is read ONLY for sidecar-NAMED candidates (honesty
/// rule — a bare name is not evidence). `repo_root` is the canonical working-tree root.
///
/// A read failure on a sidecar candidate keeps the file for embedding (the
/// conservative direction: an unprovable candidate is never dropped as exhaust). The
/// `NotFound`-vs-unreadable outcomes are kept DISTINCT (not collapsed to a bare `.ok()`
/// / `Err => None`), so an unreadable file is treated as UNKNOWN — kept — never a
/// silent "not exhaust" assertion from a failed read (operator RULING 3, honesty rule
/// #1, review-5 finding 3).
fn seed_path_is_exhaust(repo_root: &Path, rel_path: &str) -> bool {
    if repo_graph_doc_facts::is_env_path(rel_path) {
        return true;
    }
    // Non-sidecar paths: name-definitional only (`.rgr/` tool-state) — no read.
    if !repo_graph_doc_facts::has_map_sidecar_name(rel_path) {
        return repo_graph_doc_facts::is_self_generated(rel_path, None);
    }
    // Sidecar-NAMED: the first-line marker is the evidence.
    match std::fs::read_to_string(repo_root.join(rel_path)) {
        // Read OK → the marker decides exhaust-or-not.
        Ok(c) => {
            repo_graph_doc_facts::is_self_generated(rel_path, Some(c.lines().next().unwrap_or("")))
        }
        // `NotFound`: the file is gone — no marker, not provable exhaust → KEPT.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        // Unreadable (permission/IO): UNKNOWN — we cannot prove exhaust, so KEEP the
        // file for embedding rather than silently drop it as exhaust from a failed read.
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "seed_pass_tests.rs"]
mod tests;
