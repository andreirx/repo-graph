//! FORGET-REPO-1: forget a repo, and detect / reclaim orphaned on-disk storage.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the storage-reclaim domain for the daemon — (1) [`forget_repo`], the honest
//!   "forget everything repo-graph created for X" mechanism (registry entry + in-memory state +
//!   `db_runtimes` slot + `.db`/`-wal`/`-shm` + `<repo>/.rgr/`), each artifact reported
//!   `removed | absent | failed`; (2) [`scan_orphans`], the cheap directory-listing reconciliation
//!   of `databases/` against the registry into the three orphan classes; and (3) [`run_gc`], which
//!   reclaims the reclaimable classes and lists the conservative one.
//! - **Concrete current users:** `dispatch::handle_repo_remove` ([`forget_repo`]);
//!   `dispatch::handle_daemon_info` + `dispatch::handle_maintenance_gc` ([`scan_orphans`] /
//!   [`run_gc`]); `reconcile::reconcile_all_repos` (the boot orphan log, [`log_orphans_at_boot`]).
//! - **Named axis of variation (operator ruling 4, 2026-08-24):** the storage-reclaim lifecycle,
//!   kept out of the 8.8k-line dispatcher — the repo's own >500-line structural guardrail. This is a
//!   crate-private (`pub(crate)`) cohesion/size boundary, NOT a new architecture boundary: no new
//!   crate, no new dependency edge, no data crossing a documented boundary; a private module inside
//!   `daemon-runtime`. (Dispatch grows operations by adding sum-type arms; this module is not a
//!   polymorphism seam — it exists so those arms delegate reclaim mechanism instead of inlining it.)
//! - **Rejected simpler alternative:** inline the mechanism in `dispatch.rs` (8783 lines — the
//!   500-line structural guardrail forbids appending new responsibilities there) or bolt it onto
//!   `reconcile.rs` (whose responsibility is crash-orphaned *snapshot* reconciliation inside a DB —
//!   a different axis; folding DB-file reconciliation in would mix responsibilities and blow it past
//!   500 lines). Rejected both.
//!
//! ## Safety — no partial deletion while a write is in flight (review-2 atomicity, operator-ratified 2026-08-23)
//!
//! [`forget_repo`] must not race a concurrent index/refresh: review-2 OBSERVED that a *snapshot*
//! check (read the activity registry, then decide) is non-atomic — an index acquires the DB write
//! lock BEFORE it stamps its activity op, so a snapshot taken in that window sees nothing, forget
//! deletes, and the already-admitted index then writes an unregistered DB. The operator ratified the
//! fix: **forget JOINS the existing writer discipline** rather than inventing a new quiescence
//! barrier (slice §3 stop-condition: no new lock). It TRY-acquires the SAME two locks
//! `handle_refresh` / `livegraph_preload` take, in the SAME order, and **HOLDS BOTH across the whole
//! eviction + deletion** — so no writer can slip in after the check:
//!
//! 1. **DB write lock** (`db_runtime.try_acquire_write()` on the [`crate::state::DatabaseState`]) —
//!    the *universal* write barrier: index, refresh, prune, enrich, and retention ALL take this exact
//!    lock (`try`-variant or blocking). Held by any of them → forget REFUSES (deletes nothing). Forget
//!    fetches the slot with the SAME call `handle_index` uses — `get_or_create_db_runtime_for_new_db`
//!    — which keys on `canonicalize(parent)/filename` and so resolves to the ONE slot a concurrent
//!    (re-)index contends on whether or not the `.db` file exists yet (review-4: a lookup that needed
//!    the file to exist returned `None` on an absent-file repo → no guard → orphan-write race). A
//!    writer that arrives AFTER forget fetches that SAME slot and BLOCKS on the held guard until forget
//!    finishes, then re-registers fresh against the now-empty registry. This is the lock that catches a
//!    *first*/re-index too (it coordinates on the DB mutex, never on the `RepoCoordinator`).
//! 2. **Repo coordinator writer** (`coordinator.try_acquire_write()`, `WriteKind::Write`) — excludes
//!    active READERS, which take `coordinator.acquire_read` and NOT the DB write lock. `Write` (not
//!    `Refresh`): forget is destructive, so — unlike a refresh, which under W-B keeps the repo
//!    readable — it must also block NEW readers for the deletion window. Only meaningful for a LOADED
//!    repo; an unloaded repo has no coordinator and no reader can be mid-flight (reads load first).
//!
//! `try`-acquisition means forget NEVER blocks and NEVER partially deletes: if either lock is held it
//! returns an honest refusal (no artifacts touched). The two locks are a strict superset of the old
//! snapshot's signals (activity + coordinator state), now enforced ATOMICALLY.
//!
//! ## The `db_runtimes` slot must outlive deletion (review-8, operator-ratified 2026-08-24)
//!
//! Holding the DB write guard is not enough on its own: the guard only serializes writers that fetch
//! the SAME `db_runtimes` slot. Review-8 OBSERVED that the old code dropped that slot from the map
//! (inside the combined `evict_repo_and_runtime`) BEFORE registry/file deletion — while forget still
//! held the guard. A late `handle_index` calling `get_or_create_db_runtime_for_new_db` then found NO
//! slot, minted a FRESH `DatabaseState` with a FRESH lock, and acquired it without ever contending on
//! forget's guard — writing an unregistered DB while forget was mid-deletion. The fix orders the
//! teardown so the slot is the LAST thing to go, still under the held guard:
//!
//! 1. `state.evict_repo_memory(..)` — drop the in-memory `RepoState`, RETURN the slot keys, KEEP the
//!    slot in the map (still discoverable).
//! 2. delete the registry entry, then `.db`/`-wal`/`-shm`, then `<repo>/.rgr/`.
//! 3. `state.drop_db_runtime_slots(keys)` — drop the slot LAST, still under the held guard.
//!
//! So any late (re-)index that fetches the slot during steps 1–2 gets forget's slot and BLOCKS on the
//! guard until forget returns; only after every artifact is processed does the slot disappear, at
//! which point the parked index resolves a now-empty registry and re-registers a FRESH identity.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::registry::RegistryEntry;
use crate::state::DaemonState;

// ── Artifact outcomes ───────────────────────────────────────────────────

/// The fate of one artifact a forget touched. `Failed` carries the reader-frame I/O reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArtifactStatus {
    /// The artifact existed and was removed.
    Removed,
    /// The artifact did not exist (nothing to do) — not a failure.
    Absent,
    /// The artifact existed and was DELIBERATELY kept (`--keep-db`) — not removed, not absent
    /// (review-1 #3: reporting a retained-and-present DB as `absent` was a false report).
    Retained,
    /// Removal was attempted and failed; the string is the I/O reason.
    Failed(String),
}

impl ArtifactStatus {
    fn as_str(&self) -> &str {
        match self {
            ArtifactStatus::Removed => "removed",
            ArtifactStatus::Absent => "absent",
            ArtifactStatus::Retained => "retained",
            ArtifactStatus::Failed(_) => "failed",
        }
    }
}

/// One artifact in a [`ForgetReport`] — a file, a directory, or a non-file bookkeeping slot
/// (registry entry, in-memory state).
#[derive(Debug, Clone)]
pub(crate) struct ArtifactOutcome {
    /// Reader-frame artifact kind: "registry", "memory", "database", "wal", "shm", "warm-cache".
    pub(crate) kind: &'static str,
    /// The path (for file/dir artifacts) or a label (for non-file slots).
    pub(crate) label: String,
    pub(crate) status: ArtifactStatus,
    /// Bytes reclaimed/held: `Some(n)` when measured (0 when nothing was removed/held);
    /// `None` when the size could NOT be measured (stat/traversal fault) — rendered as
    /// unknown, never as 0 (VISION: "unknown is never zero"). The fault is in `size_error`.
    pub(crate) bytes: Option<u64>,
    /// The reason `bytes` is unknown (`None` when `bytes` is measured).
    pub(crate) size_error: Option<String>,
}

impl ArtifactOutcome {
    fn to_json(&self) -> Value {
        let reason = match &self.status {
            ArtifactStatus::Failed(r) => Some(r.as_str()),
            _ => None,
        };
        json!({
            "kind": self.kind,
            "artifact": self.label,
            "status": self.status.as_str(),
            // `null` = size unknown (see the field docs) — a machine consumer must not read 0.
            "bytes": self.bytes,
            "size_error": self.size_error,
            "reason": reason,
        })
    }
}

// ── Forget ──────────────────────────────────────────────────────────────

/// The result of forgetting (or refusing to forget) one repo.
#[derive(Debug, Clone)]
pub(crate) struct ForgetReport {
    pub(crate) repo_display: String,
    pub(crate) canonical_path: PathBuf,
    pub(crate) db_path: PathBuf,
    /// `true` when `--keep-db` was requested: the `.db`/`-wal`/`-shm` were left in place.
    pub(crate) kept_db: bool,
    /// `Some(reason)` when forgetting was refused (an in-flight write) — nothing was deleted.
    pub(crate) refused: Option<String>,
    pub(crate) artifacts: Vec<ArtifactOutcome>,
}

impl ForgetReport {
    /// Any artifact removal failed → the caller must exit non-zero.
    pub(crate) fn any_failed(&self) -> bool {
        self.artifacts
            .iter()
            .any(|a| matches!(a.status, ArtifactStatus::Failed(_)))
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "repo": self.repo_display,
            "canonical_path": self.canonical_path.to_string_lossy(),
            "db_path": self.db_path.to_string_lossy(),
            "kept_db": self.kept_db,
            "refused": self.refused,
            "ok": self.refused.is_none() && !self.any_failed(),
            "artifacts": self.artifacts.iter().map(|a| a.to_json()).collect::<Vec<_>>(),
        })
    }
}

/// A refusal `ForgetReport` — nothing was touched (`artifacts` empty), carrying the honest reason.
/// Two concrete callers (the DB-write-lock and coordinator-writer arms of [`forget_repo`]); factored
/// out to keep the two refusal sites from drifting in shape. Not a variation seam.
fn refused_report(
    repo_display: &str,
    entry: &RegistryEntry,
    keep_db: bool,
    reason: &str,
) -> ForgetReport {
    ForgetReport {
        repo_display: repo_display.to_string(),
        canonical_path: entry.canonical_path.clone(),
        db_path: entry.db_path.clone(),
        kept_db: keep_db,
        refused: Some(reason.to_string()),
        artifacts: Vec::new(),
    }
}

/// FORGET-REPO-1 §2.1: forget everything repo-graph created for `entry`, or REFUSE (deleting
/// nothing) while a write op is in flight.
///
/// `keep_db == true` (`--keep-db`) leaves the `.db`/`-wal`/`-shm` on disk (and reports where);
/// the registry entry, in-memory state and `db_runtimes` slot are still dropped, and `<repo>/.rgr/`
/// is still removed (it is warm cache keyed to the now-gone registry entry).
pub(crate) fn forget_repo(
    state: &DaemonState,
    entry: &RegistryEntry,
    keep_db: bool,
) -> ForgetReport {
    // Production entry: no barrier. The barriered inner is the SAME body — the closure is a test-only
    // seam (see [`forget_repo_barriered`]).
    forget_repo_barriered(state, entry, keep_db, || {})
}

/// The [`forget_repo`] body, with a test-only `after_evict` seam fired ONCE inside the deletion
/// window — after the guards are held and the in-memory state is evicted, but with the `db_runtimes`
/// slot STILL in the map and NOTHING deleted yet. Its only concrete users are [`forget_repo`] (a
/// no-op closure) and the `forget_parks_a_late_index_*` regression, which parks a late index at that
/// exact instant to prove the slot stays discoverable across deletion (review-8). Not a variation
/// seam; the closure is monomorphized away for the production no-op. Rejected simpler alternative: a
/// `#[cfg(test)]` global barrier static (shared mutable state, worse than a local closure param).
fn forget_repo_barriered<F: FnMut()>(
    state: &DaemonState,
    entry: &RegistryEntry,
    keep_db: bool,
    mut after_evict: F,
) -> ForgetReport {
    let repo_display = entry
        .alias
        .clone()
        .unwrap_or_else(|| entry.canonical_path.to_string_lossy().to_string());

    // review-2 atomicity fix (operator-ratified): TRY-acquire the writer-discipline locks and HOLD
    // them across the whole eviction+deletion (see the module header). Lock ORDER mirrors
    // `handle_refresh`: (1) DB write lock, then (2) repo coordinator writer. `try`-acquisition never
    // blocks; a held lock → honest refusal with NOTHING deleted.
    //
    // (1) DB write lock — the universal write barrier (index/refresh/prune/enrich/retention). Fetch
    //     the SAME coordination slot `handle_index` fetches — `get_or_create_db_runtime_for_new_db` —
    //     NOT a lookup that depends on the DB file existing. That call keys the slot on
    //     `canonicalize(parent)/filename`, so it resolves to the ONE slot a concurrent (re-)index will
    //     block on whether or not the `.db` file is present yet. This is the review-4 fix: the former
    //     `existing_or_new_db_runtime` returned `None` for a registered-but-absent-file repo with no
    //     live slot (its full-path canonicalize failed on the missing file and its raw-path fallback
    //     missed the index's canonical-keyed slot), so forget held no guard and a late index could
    //     write an unregistered/orphan DB — the exact failure forget exists to prevent. The Arc is
    //     bound to a local so the guard's borrow stays valid even after `drop_db_runtime_slots` drops
    //     the db_runtimes map entry at the END of forget (our clone keeps the DatabaseState alive).
    //
    //     `Err` here means only that the `databases/` PARENT dir cannot be canonicalized (the dir
    //     itself is gone) — then no `.db` file can exist under it AND no index can target it, because
    //     `handle_index` takes this identical call and fails identically. With no possible coordinated
    //     writer, forget proceeds guardless (there is nothing on disk to protect).
    let db_runtime = state
        .get_or_create_db_runtime_for_new_db(&entry.db_path)
        .ok();
    let _db_write_guard = match &db_runtime {
        Some(rt) => match rt.try_acquire_write() {
            Some(g) => Some(g),
            None => {
                return refused_report(
                    &repo_display,
                    entry,
                    keep_db,
                    "an index, refresh, or maintenance write is in progress on this repo; cancel it first, then re-run",
                )
            }
        },
        // databases/ parent dir gone → no DB file possible, no index can coordinate: proceed guardless.
        None => None,
    };

    // (2) Repo coordinator writer — excludes active readers (only meaningful for a loaded repo).
    let repo_state = state.loaded_repo_by_uid(&entry.repo_uid, &entry.db_path);
    let _coord_write_guard = match &repo_state {
        Some(rs) => match rs.coordinator.try_acquire_write() {
            Some(g) => Some(g),
            None => {
                return refused_report(
                    &repo_display,
                    entry,
                    keep_db,
                    "this repo is being read right now; retry in a moment",
                )
            }
        },
        None => None,
    };

    let mut artifacts = Vec::new();

    // In-memory state ONLY. Keyed on the registry repo_uid, NOT on the DB file existing (the field
    // bug: eviction was gated on the DB file canonicalizing). The `db_runtimes` slot is DELIBERATELY
    // left in the map here (its keys captured in `mem.runtime_keys`) and dropped LAST, after all
    // deletion — so a late (re-)index that fetches it mid-deletion contends on our held guard instead
    // of minting a fresh slot (review-8; see the module header). Memory and slot are still reported as
    // SEPARATE artifacts (review-1 #3), the slot's line pushed at the end.
    let mem = state.evict_repo_memory(&entry.repo_uid, &entry.db_path);
    artifacts.push(ArtifactOutcome {
        kind: "memory",
        label: "in-memory repo state".to_string(),
        status: if mem.memory_evicted {
            ArtifactStatus::Removed
        } else {
            ArtifactStatus::Absent
        },
        bytes: Some(0),
        size_error: None,
    });

    // Test-only seam: guards held, memory evicted, slot STILL discoverable, nothing deleted yet. A
    // no-op in production (see [`forget_repo`]); the regression fires a late index here.
    after_evict();

    // Registry entry (+ persist). Removal failure here is a real failure (registry stays dirty).
    let registry_status = {
        let mut registry = state.registry_mut();
        match registry.remove(&entry.canonical_path) {
            Ok(_) => match registry.save() {
                Ok(()) => ArtifactStatus::Removed,
                Err(e) => ArtifactStatus::Failed(format!("registry saved failed: {e}")),
            },
            Err(_) => ArtifactStatus::Absent,
        }
    };
    artifacts.push(ArtifactOutcome {
        kind: "registry",
        label: "registry entry".to_string(),
        status: registry_status,
        bytes: Some(0),
        size_error: None,
    });

    // The database file + WAL/SHM sidecars.
    if keep_db {
        // `--keep-db`: the DB AND its `-wal`/`-shm` sidecars are DELIBERATELY left in place. Report
        // EACH present file as `retained` with its real byte size — NOT `absent` (review-1 #3: an
        // existing, retained DB reported as absent was a false report contradicting the CLI's
        // "retained" line). A genuinely-missing file is still `absent`. review-3 #2: the sidecars are
        // reported on their own lines too — the earlier keep-db branch reported only the base `.db` and
        // left `-wal`/`-shm` retained-but-unreported, an incomplete per-artifact report.
        artifacts.push(kept_file_artifact("database", &entry.db_path));
        artifacts.push(kept_file_artifact("wal", &sidecar(&entry.db_path, "-wal")));
        artifacts.push(kept_file_artifact("shm", &sidecar(&entry.db_path, "-shm")));
    } else {
        artifacts.push(remove_file_artifact("database", &entry.db_path));
        artifacts.push(remove_file_artifact(
            "wal",
            &sidecar(&entry.db_path, "-wal"),
        ));
        artifacts.push(remove_file_artifact(
            "shm",
            &sidecar(&entry.db_path, "-shm"),
        ));
    }

    // `<repo>/.rgr/` warm cache + livegraph-compare sidecars. review-10: gate on `fs::metadata`, NOT
    // `exists()`. `exists()` collapses every metadata fault to `false`, so an INACCESSIBLE repo root
    // (permission denied, ENOTDIR on an ancestor) was silently omitted from the report — a hole in
    // the per-artifact honest-report contract. Now: only a genuine NotFound repo root means "no
    // directory to clean" (omit); any other stat fault is reported as an honest `failed(reason)`.
    match fs::metadata(&entry.canonical_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Repo dir genuinely gone → `.rgr/` cannot exist; nothing to clean.
        }
        Err(e) => artifacts.push(ArtifactOutcome {
            kind: "warm-cache",
            label: entry
                .canonical_path
                .join(".rgr")
                .to_string_lossy()
                .to_string(),
            status: ArtifactStatus::Failed(format!(
                "cannot stat repo root {}: {e}",
                entry.canonical_path.display()
            )),
            bytes: Some(0),
            size_error: None,
        }),
        Ok(_) => artifacts.push(remove_dir_artifact(
            "warm-cache",
            &entry.canonical_path.join(".rgr"),
        )),
    }

    // db_runtimes coordination slot — dropped LAST, after every registry + file artifact, still under
    // the held DB write guard (review-8). Until this line the slot stayed discoverable, so a late
    // (re-)index blocked on our guard rather than minting a fresh slot and writing past it.
    let runtime_dropped = state.drop_db_runtime_slots(&mem.runtime_keys);
    artifacts.push(ArtifactOutcome {
        kind: "runtime-slot",
        label: "db_runtimes coordination slot".to_string(),
        status: if runtime_dropped {
            ArtifactStatus::Removed
        } else {
            ArtifactStatus::Absent
        },
        bytes: Some(0),
        size_error: None,
    });

    ForgetReport {
        repo_display,
        canonical_path: entry.canonical_path.clone(),
        db_path: entry.db_path.clone(),
        kept_db: keep_db,
        refused: None,
        artifacts,
    }
}

// ── Orphan scan ─────────────────────────────────────────────────────────

/// A file with its on-disk size.
#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
}

/// Class A: a `.db` file in `databases/` no registry entry references, plus its own sidecars.
#[derive(Debug, Clone)]
pub(crate) struct OrphanDb {
    pub(crate) db_file: FileEntry,
    pub(crate) sidecars: Vec<FileEntry>,
}

impl OrphanDb {
    fn bytes(&self) -> u64 {
        self.db_file.bytes + self.sidecars.iter().map(|s| s.bytes).sum::<u64>()
    }
}

/// Class B: a registry entry whose repo path no longer exists on disk.
///
/// Only the fields the renderers actually read are carried: `canonical_path` (the dead path + the
/// argument to the `rmap repo remove` next action) and `display` (the reader-frame label). The DB
/// file is NOT reclaimed for a dead-path entry (it is still registry-referenced, so not an orphan),
/// so no `db_path`/`repo_uid` is needed here — the crate-private visibility surfaced them as dead.
#[derive(Debug, Clone)]
pub(crate) struct DeadPathEntry {
    pub(crate) canonical_path: PathBuf,
    pub(crate) display: String,
}

/// The three orphan classes computed from a `databases/` listing + the registry.
#[derive(Debug, Clone, Default)]
pub(crate) struct OrphanReport {
    /// Class A — orphan DB files (+ their sidecars).
    pub(crate) orphan_dbs: Vec<OrphanDb>,
    /// Class C — sidecars with no base `.db` file at all.
    pub(crate) stray_sidecars: Vec<FileEntry>,
    /// Class B — registry entries whose repo path is gone.
    pub(crate) dead_path_entries: Vec<DeadPathEntry>,
    /// `Some` when the `databases/` listing itself failed (unknown, never rendered as zero).
    pub(crate) scan_error: Option<String>,
}

impl OrphanReport {
    pub(crate) fn orphan_db_bytes(&self) -> u64 {
        self.orphan_dbs.iter().map(|o| o.bytes()).sum()
    }
    pub(crate) fn stray_bytes(&self) -> u64 {
        self.stray_sidecars.iter().map(|s| s.bytes).sum()
    }
    /// Bytes `rmap maintenance gc` would reclaim (classes A + C — NOT the dead-path entries).
    pub(crate) fn reclaimable_bytes(&self) -> u64 {
        self.orphan_db_bytes() + self.stray_bytes()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.orphan_dbs.is_empty()
            && self.stray_sidecars.is_empty()
            && self.dead_path_entries.is_empty()
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "orphan_db_count": self.orphan_dbs.len(),
            "orphan_db_bytes": self.orphan_db_bytes(),
            "stray_sidecar_count": self.stray_sidecars.len(),
            "stray_sidecar_bytes": self.stray_bytes(),
            "reclaimable_bytes": self.reclaimable_bytes(),
            "dead_path_entries": self.dead_path_entries.iter().map(|d| json!({
                "path": d.canonical_path.to_string_lossy(),
                "repo": d.display,
                // review-1 #4: a copy-pasteable, shell-quoted command (a path may contain spaces).
                "next_action": dead_path_next_action(&d.canonical_path),
            })).collect::<Vec<_>>(),
            "scan_error": self.scan_error,
        })
    }
}

/// FORGET-REPO-1 §2.2: reconcile the `databases/` directory against the registry into the three
/// orphan classes. Cheap — one directory listing + `stat` per file; no DB opens.
pub(crate) fn scan_orphans(db_dir: &Path, entries: &[RegistryEntry]) -> OrphanReport {
    // Accumulates EVERY "cannot determine" fault across the whole scan (class-B stat faults below,
    // per-entry directory read errors, and reclaim-candidate stat failures) so `scan_error` becomes
    // `Some` and no caller renders a false clean zero ("unknown is never zero", review-9/review-10).
    let mut scan_errors: Vec<String> = Vec::new();

    // Class B is independent of the listing: a registry entry whose repo path is gone. review-10:
    // classify with `fs::metadata`, NOT `exists()`. ONLY a genuine NotFound is a dead path — a
    // non-NotFound stat fault (permission denied, ENOTDIR on an ancestor) is UNKNOWN, never class B,
    // or gc/doctor would recommend a DESTRUCTIVE `rmap repo remove` on a path that may still be live.
    // The fault is recorded so the scan renders UNKNOWN instead of silently dropping the entry.
    let dead_path_entries: Vec<DeadPathEntry> = entries
        .iter()
        .filter_map(|e| match fs::metadata(&e.canonical_path) {
            Ok(_) => None, // path present → not a dead path
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(DeadPathEntry {
                canonical_path: e.canonical_path.clone(),
                display: e
                    .alias
                    .clone()
                    .unwrap_or_else(|| e.canonical_path.to_string_lossy().to_string()),
            }),
            Err(err) => {
                scan_errors.push(format!(
                    "cannot stat registered path {}: {err}",
                    e.canonical_path.display()
                ));
                None
            }
        })
        .collect();

    // The set of db file NAMES the registry references (matched by file name — the registry db_path
    // and the on-disk file share `databases/`).
    let referenced: std::collections::BTreeSet<OsString> = entries
        .iter()
        .filter_map(|e| e.db_path.file_name().map(|n| n.to_os_string()))
        .collect();

    let read = match fs::read_dir(db_dir) {
        Ok(r) => r,
        Err(e) => {
            // Listing failure is itself a fault; fold it in alongside any class-B stat faults so the
            // report carries every reason, not just this one.
            scan_errors.push(format!("cannot list {}: {e}", db_dir.display()));
            return OrphanReport {
                dead_path_entries,
                scan_error: Some(join_scan_faults(&scan_errors, db_dir)),
                ..Default::default()
            };
        }
    };

    // First pass: partition the directory into present .db files and sidecars. Per-entry iteration
    // errors are PRESERVED (review-1 #2: `read.flatten()` silently dropped them, so a partly-unreadable
    // directory could report as fully clean — "unknown is never zero").
    let mut present_db_names: std::collections::BTreeSet<OsString> = Default::default();
    let mut orphan_by_name: std::collections::BTreeMap<OsString, OrphanDb> = Default::default();
    let mut sidecar_paths: Vec<PathBuf> = Vec::new();
    for entry in read {
        let dirent = match entry {
            Ok(d) => d,
            Err(e) => {
                scan_errors.push(e.to_string());
                continue;
            }
        };
        let path = dirent.path();
        let name = match path.file_name() {
            Some(n) => n.to_os_string(),
            None => continue,
        };
        let name_str = name.to_string_lossy();
        if name_str.ends_with("-wal") || name_str.ends_with("-shm") {
            sidecar_paths.push(path);
        } else if name_str.ends_with(".db") {
            present_db_names.insert(name.clone());
            if !referenced.contains(&name) {
                // review-9 #1: a stat failure on a reclaim candidate is UNKNOWN, not 0 — record it
                // so `scan_error` becomes `Some` and no caller renders a false clean zero.
                let bytes = stat_len_or_record(&path, &mut scan_errors);
                orphan_by_name.insert(
                    name,
                    OrphanDb {
                        db_file: FileEntry { path, bytes },
                        sidecars: Vec::new(),
                    },
                );
            }
        }
        // Any other file (e.g. registry.json.tmp) is not our concern here.
    }

    // Second pass: attribute each sidecar to its base `.db`.
    let mut stray_sidecars = Vec::new();
    for path in sidecar_paths {
        // review-9 #1: stat only the sidecars we actually classify as reclaimable (class A / C), and
        // record any stat failure into `scan_errors` — a live sidecar we leave alone is never
        // reclaimed, so its stat is irrelevant and must not pollute the error set.
        match sidecar_base_name(&path) {
            Some(base) if orphan_by_name.contains_key(&base) => {
                // Belongs to an orphan DB → counted with class A.
                let bytes = stat_len_or_record(&path, &mut scan_errors);
                orphan_by_name
                    .get_mut(&base)
                    .unwrap()
                    .sidecars
                    .push(FileEntry { path, bytes });
            }
            Some(base) if present_db_names.contains(&base) => {
                // Live sidecar of a referenced/present DB → leave it alone.
            }
            _ => {
                // No base `.db` file at all → class C stray sidecar.
                let bytes = stat_len_or_record(&path, &mut scan_errors);
                stray_sidecars.push(FileEntry { path, bytes });
            }
        }
    }

    // A partially-unreadable directory (or an unstattable registered path) is UNKNOWN, not clean:
    // surface the count + first cause so callers (gc / doctor / boot log) render it as unknown
    // rather than as zero orphans.
    let scan_error = if scan_errors.is_empty() {
        None
    } else {
        Some(join_scan_faults(&scan_errors, db_dir))
    };

    OrphanReport {
        orphan_dbs: orphan_by_name.into_values().collect(),
        stray_sidecars,
        dead_path_entries,
        scan_error,
    }
}

/// Render a non-empty set of scan faults (class-B stat failures, per-entry read errors, or
/// reclaim-candidate stat failures) into one honest `scan_error` string: the total count plus the
/// first cause as an example. Two callers (the `read_dir`-failure early return and the normal tail),
/// non-trivial format → one helper, no duplication.
fn join_scan_faults(faults: &[String], db_dir: &Path) -> String {
    format!(
        "{} scan fault(s) affecting {} (e.g. {})",
        faults.len(),
        db_dir.display(),
        faults[0]
    )
}

/// FORGET-REPO-1: log the orphan-class counts at boot (or the listing failure — unknown, never
/// zero). Called from the boot reconciliation sweep.
pub(crate) fn log_orphans_at_boot(state: &DaemonState) {
    let (db_dir, entries) = {
        let reg = state.registry();
        (
            reg.db_dir().to_path_buf(),
            reg.list().into_iter().cloned().collect::<Vec<_>>(),
        )
    };
    let report = scan_orphans(&db_dir, &entries);
    if let Some(err) = &report.scan_error {
        eprintln!("warn: startup orphan scan could not list databases/: {err}");
        return;
    }
    // review-2 (§2.2): log the counts on EVERY boot, including all-zero. "Unknown is never zero" — and
    // zero IS a measurement: an operator watching the log must be able to distinguish "scanned, clean"
    // from "never scanned". A clean scan says so and omits the reclaim hint; a dirty one appends it.
    let hint = if report.is_empty() {
        ""
    } else {
        " — run `rmap maintenance gc` to reclaim, `rmap doctor` to inspect"
    };
    eprintln!(
        "info: startup orphan scan: {} orphan DB file(s) ({} bytes), {} stray sidecar(s) ({} bytes), {} dead-path registry entr(y/ies){hint}",
        report.orphan_dbs.len(),
        report.orphan_db_bytes(),
        report.stray_sidecars.len(),
        report.stray_bytes(),
        report.dead_path_entries.len(),
    );
}

// ── Garbage collection ──────────────────────────────────────────────────

/// One file `gc` removed (or, in `--dry-run`, would remove; or SKIPPED because a concurrent
/// registration re-claimed it — operator ruling 2).
#[derive(Debug, Clone)]
pub(crate) struct GcItem {
    pub(crate) kind: &'static str, // "orphan-db" | "orphan-sidecar" | "stray-sidecar"
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
    /// `true` when actually unlinked (always `false` in `--dry-run` and when `skipped`).
    pub(crate) removed: bool,
    /// `Some` on a real removal that failed.
    pub(crate) error: Option<String>,
    /// `Some(reason)` when the candidate was DELIBERATELY not touched because it stopped being an
    /// orphan between the scan and the unlink — a concurrent (re-)index either holds the DB write
    /// slot or has re-registered the path. Skipped is a SAFE outcome (the file is intact), NOT a
    /// failure: it is the operator-ratified guard against GC deleting a now-live DB.
    pub(crate) skipped: Option<String>,
}

impl GcItem {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "path": self.path.to_string_lossy(),
            "bytes": self.bytes,
            "removed": self.removed,
            "error": self.error,
            "skipped": self.skipped,
        })
    }
}

/// The outcome of a `gc` run.
#[derive(Debug, Clone)]
pub(crate) struct GcOutcome {
    pub(crate) dry_run: bool,
    pub(crate) items: Vec<GcItem>,
    /// Bytes actually freed (0 in `--dry-run`).
    pub(crate) reclaimed_bytes: u64,
    /// Bytes that would be freed (all candidate bytes, both modes).
    pub(crate) would_reclaim_bytes: u64,
    /// Class B — LISTED with the `rmap repo remove` next action, never auto-removed.
    pub(crate) dead_path_entries: Vec<DeadPathEntry>,
    pub(crate) scan_error: Option<String>,
}

impl GcOutcome {
    pub(crate) fn any_failed(&self) -> bool {
        self.items.iter().any(|i| i.error.is_some())
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "dry_run": self.dry_run,
            "reclaimed_bytes": self.reclaimed_bytes,
            "would_reclaim_bytes": self.would_reclaim_bytes,
            "removed_count": self.items.iter().filter(|i| i.removed).count(),
            "skipped_count": self.items.iter().filter(|i| i.skipped.is_some()).count(),
            "candidate_count": self.items.len(),
            "items": self.items.iter().map(|i| i.to_json()).collect::<Vec<_>>(),
            "dead_path_entries": self.dead_path_entries.iter().map(|d| json!({
                "path": d.canonical_path.to_string_lossy(),
                "repo": d.display,
                // review-1 #4: a copy-pasteable, shell-quoted command (a path may contain spaces).
                "next_action": dead_path_next_action(&d.canonical_path),
            })).collect::<Vec<_>>(),
            "scan_error": self.scan_error,
            // review-1 #2: an incomplete scan is NOT a clean success. A `scan_error` (whole listing
            // failed, or some entries unreadable) is an unknown result → the CLI exits non-zero and
            // renders "unknown", never "No orphan files to reclaim".
            "ok": !self.any_failed() && self.scan_error.is_none(),
        })
    }
}

/// One base `.db` and every file `gc` would reclaim under that base's DB write slot. All files that
/// share a base (`foo.db` + `foo.db-wal` + `foo.db-shm`) are unlinked under ONE hold of the slot the
/// index of that path contends on, so GC and a concurrent (re-)index can never both touch them.
struct ReclaimUnit {
    /// The `.db` path whose `get_or_create_db_runtime_for_new_db` slot guards this unit.
    base_db: PathBuf,
    /// `file_name` of `base_db` — the key the registry recheck matches against.
    base_name: OsString,
    /// (kind, path, bytes) for each file in the unit.
    files: Vec<(&'static str, PathBuf, u64)>,
}

/// Group a scan's reclaim candidates into per-base-`.db` [`ReclaimUnit`]s (deterministic order:
/// orphan DBs — each with its own sidecars — then stray sidecars grouped by their base name).
fn reclaim_units(db_dir: &Path, scan: &OrphanReport) -> Vec<ReclaimUnit> {
    let mut units: Vec<ReclaimUnit> = Vec::new();
    for orphan in &scan.orphan_dbs {
        let mut files = vec![(
            "orphan-db",
            orphan.db_file.path.clone(),
            orphan.db_file.bytes,
        )];
        for s in &orphan.sidecars {
            files.push(("orphan-sidecar", s.path.clone(), s.bytes));
        }
        units.push(ReclaimUnit {
            base_name: orphan
                .db_file
                .path
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_default(),
            base_db: orphan.db_file.path.clone(),
            files,
        });
    }
    // Stray sidecars: group by their derived base `.db` name so all strays of one base share a guard.
    let mut stray_by_base: std::collections::BTreeMap<OsString, ReclaimUnit> = Default::default();
    for s in &scan.stray_sidecars {
        // Every stray ends in -wal/-shm (scan_orphans guarantees it), so a base name always exists.
        let base_name = sidecar_base_name(&s.path).unwrap_or_default();
        let base_db = db_dir.join(&base_name);
        stray_by_base
            .entry(base_name.clone())
            .or_insert_with(|| ReclaimUnit {
                base_db,
                base_name,
                files: Vec::new(),
            })
            .files
            .push(("stray-sidecar", s.path.clone(), s.bytes));
    }
    units.extend(stray_by_base.into_values());
    units
}

/// FORGET-REPO-1 §2.3 + operator ruling 2: reclaim classes A + C (orphan DB files + stray sidecars),
/// reporting bytes; LIST class B (dead-path entries) with their next action — never auto-remove them
/// (a path may be a temporarily-unmounted volume). `dry_run` lists candidates without deleting.
///
/// ## The scan→unlink race, and how each unlink is guarded (operator ruling 2, 2026-08-24)
///
/// A single directory scan then a bare `remove_file` has a window: between the scan classifying
/// `foo.db` as an orphan and GC unlinking it, an index of a path that hashes to `foo.db` can register
/// and start writing — GC would then delete a now-live DB (data loss / a fresh orphan). GC closes the
/// window by treating each base `.db` as a [`ReclaimUnit`] and, IMMEDIATELY BEFORE unlinking it:
///
/// 1. **acquires the unit's DB write slot** — `get_or_create_db_runtime_for_new_db(base_db)`, the
///    SAME slot a concurrent (re-)index of that path takes (iteration 5: the index registers UNDER
///    this held guard). `try`-acquire: held ⇒ an index is mid-write ⇒ SKIP the unit (do not block,
///    do not delete); and
/// 2. **rechecks the live registry** while holding that guard — if any entry now references
///    `base_name`, the path was (re-)registered since the scan ⇒ SKIP.
///
/// Only when BOTH pass does GC unlink, under the held guard, so no index can slip in between the
/// recheck and the `remove_file`. A skipped unit is reported (`GcItem::skipped`) — a safe, honest
/// outcome, not a failure. `dry_run` touches nothing and needs no guard (it only lists candidates).
pub(crate) fn run_gc(state: &DaemonState, dry_run: bool) -> GcOutcome {
    // Production entry: no barrier. The barriered inner is the SAME body — the closure is a test-only
    // seam (see [`run_gc_barriered`]).
    run_gc_barriered(state, dry_run, || {})
}

/// The [`run_gc`] body, with a test-only `after_scan` seam fired ONCE after the directory scan has
/// fixed the candidate set but BEFORE the per-unit slot-guard + registry recheck runs — the exact
/// window operator ruling 2's recheck exists to close. Its only concrete users are [`run_gc`] (a
/// no-op closure) and the `gc_skips_a_candidate_registered_after_the_scan_*` regression, which
/// re-registers a candidate at that instant to prove the recheck (not the scan) is what protects a
/// DB claimed after the scan. Not a variation seam; the closure is monomorphized away for the
/// production no-op. Rejected simpler alternative: a `#[cfg(test)]` global barrier static (shared
/// mutable state, worse than a local closure param).
fn run_gc_barriered<F: FnMut()>(
    state: &DaemonState,
    dry_run: bool,
    mut after_scan: F,
) -> GcOutcome {
    let (db_dir, entries) = {
        let reg = state.registry();
        (
            reg.db_dir().to_path_buf(),
            reg.list().into_iter().cloned().collect::<Vec<_>>(),
        )
    };
    let scan = scan_orphans(&db_dir, &entries);
    let units = reclaim_units(&db_dir, &scan);

    // Test-only seam: candidate set fixed, nothing acquired or deleted yet. A no-op in production
    // (see [`run_gc`]); the regression re-registers a candidate here to race the per-unit recheck.
    after_scan();

    let would_reclaim_bytes: u64 = units
        .iter()
        .flat_map(|u| u.files.iter().map(|(_, _, b)| *b))
        .sum();
    let mut reclaimed_bytes = 0u64;
    let mut items = Vec::new();

    for unit in units {
        // ── dry-run: list every file, touch nothing, no guard needed. ──
        if dry_run {
            for (kind, path, bytes) in unit.files {
                items.push(GcItem {
                    kind,
                    path,
                    bytes,
                    removed: false,
                    error: None,
                    skipped: None,
                });
            }
            continue;
        }

        // ── guard: acquire the unit's DB write slot (the index's slot); held ⇒ skip. ──
        let slot = state
            .get_or_create_db_runtime_for_new_db(&unit.base_db)
            .ok();
        let _guard = match slot.as_ref().map(|rt| rt.try_acquire_write()) {
            Some(Some(g)) => g,
            // slot exists but a writer holds it (index in progress), OR the parent dir could not be
            // canonicalized (gone since the scan) — either way, do NOT delete this unit.
            Some(None) | None => {
                push_skipped(
                    &mut items,
                    &unit,
                    "a write is in progress on this database (an index may be (re-)creating it); left intact",
                );
                continue;
            }
        };

        // ── recheck: under the held guard, has the path been (re-)registered since the scan? ──
        if base_name_is_registered(state, &unit.base_name) {
            push_skipped(
                &mut items,
                &unit,
                "re-registered since the scan (a live index now owns this database); left intact",
            );
            continue;
        }

        // ── both clear: unlink every file in the unit under the held guard. ──
        for (kind, path, bytes) in unit.files {
            match fs::remove_file(&path) {
                Ok(()) => {
                    reclaimed_bytes += bytes;
                    items.push(GcItem {
                        kind,
                        path,
                        bytes,
                        removed: true,
                        error: None,
                        skipped: None,
                    });
                }
                Err(e) => items.push(GcItem {
                    kind,
                    path,
                    bytes,
                    removed: false,
                    error: Some(e.to_string()),
                    skipped: None,
                }),
            }
        }
    }

    GcOutcome {
        dry_run,
        items,
        reclaimed_bytes,
        would_reclaim_bytes,
        dead_path_entries: scan.dead_path_entries,
        scan_error: scan.scan_error,
    }
}

/// Is `base_name` referenced by ANY current registry entry's `db_path`? Read fresh under the caller's
/// held DB write guard so it observes a registration that raced the scan (operator ruling 2 recheck).
fn base_name_is_registered(state: &DaemonState, base_name: &OsString) -> bool {
    let reg = state.registry();
    reg.list()
        .iter()
        .filter_map(|e| e.db_path.file_name())
        .any(|n| n == base_name.as_os_str())
}

/// Push every file of a skipped unit as a `skipped` [`GcItem`] carrying `reason`.
fn push_skipped(items: &mut Vec<GcItem>, unit: &ReclaimUnit, reason: &str) {
    for (kind, path, bytes) in &unit.files {
        items.push(GcItem {
            kind,
            path: path.clone(),
            bytes: *bytes,
            removed: false,
            error: None,
            skipped: Some(reason.to_string()),
        });
    }
}

// ── Next-action rendering ─────────────────────────────────────────────────

/// review-1 #4: the copy-pasteable `rmap repo remove <path>` next action for a dead-path entry, with
/// the path shell-quoted so it pastes as ONE argument even with spaces (valid on the operator's
/// macOS system). This is the SINGLE authority for the command string — both product renderers
/// (`rmap doctor`, `rmap maintenance gc`) consume this `next_action` from the daemon rather than
/// re-building (and re-quoting) it client-side.
fn dead_path_next_action(path: &Path) -> String {
    format!("rmap repo remove {}", shell_quote(&path.to_string_lossy()))
}

/// POSIX-quote a string for a copy-pasteable shell command. A path made only of unambiguously-safe
/// characters is emitted bare (the clean common case); anything else — a space, a quote, a glob char —
/// is wrapped in single quotes with any embedded `'` escaped as `'\''`, so the whole path is one
/// argument. Not a general shell-escaper; scoped to rendering a filesystem path into a `remove` hint.
fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'_' | b'-' | b'.' | b'/' | b'@' | b'%' | b'+' | b':' | b',' | b'='
                )
        });
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

// ── File helpers ────────────────────────────────────────────────────────

/// `<db_path><suffix>` — e.g. `/…/hash.db` + `-wal` → `/…/hash.db-wal`.
fn sidecar(db_path: &Path, suffix: &str) -> PathBuf {
    let mut s = db_path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// The base `.db` name of a sidecar, i.e. strip a trailing `-wal` / `-shm`. `foo.db-wal` → `foo.db`.
fn sidecar_base_name(path: &Path) -> Option<OsString> {
    let name = path.file_name()?.to_string_lossy();
    let base = name
        .strip_suffix("-wal")
        .or_else(|| name.strip_suffix("-shm"))?;
    Some(OsString::from(base))
}

/// Stat a file that is about to be classified as a reclaim candidate (an orphan DB or a stray
/// sidecar), returning its byte length. A stat failure is NOT silently
/// coerced to `0` (VISION: "unknown is never zero"): the reason is appended to `errors` so
/// [`scan_orphans`] folds it into `scan_error`, and every caller (doctor / gc / boot log) then
/// renders the scan as UNKNOWN rather than as a clean zero-byte success (review-9 #1). The returned
/// `0` is an arithmetic fallback only — it never stands alone as a truthful size, because the
/// accompanying `scan_error` marks the total as untrustworthy.
fn stat_len_or_record(path: &Path, errors: &mut Vec<String>) -> u64 {
    match fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            errors.push(format!("cannot stat {}: {e}", path.display()));
            0
        }
    }
}

/// Remove a single file artifact, reporting `absent` / `removed(bytes)` / `failed(reason)`.
fn remove_file_artifact(kind: &'static str, path: &Path) -> ArtifactOutcome {
    let label = path.to_string_lossy().to_string();
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Absent,
            bytes: Some(0),
            size_error: None,
        },
        Err(e) => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Failed(e.to_string()),
            bytes: Some(0),
            size_error: None,
        },
        Ok(meta) => {
            let bytes = meta.len();
            match fs::remove_file(path) {
                Ok(()) => ArtifactOutcome {
                    kind,
                    label,
                    status: ArtifactStatus::Removed,
                    bytes: Some(bytes),
                    size_error: None,
                },
                Err(e) => ArtifactOutcome {
                    kind,
                    label,
                    status: ArtifactStatus::Failed(e.to_string()),
                    bytes: Some(0),
                    size_error: None,
                },
            }
        }
    }
}

/// Report a file DELIBERATELY kept under `--keep-db`: `retained(bytes)` when present, `absent` when
/// missing (review-3 #2 — every kept artifact, base `.db` AND `-wal`/`-shm` sidecars, gets an honest
/// line). Never deletes. An unstattable-but-not-not-found file is still `retained` (we did not delete
/// it); its size is UNKNOWN (`bytes: None` + `size_error`) — never rendered as 0 (review-11 class).
fn kept_file_artifact(kind: &'static str, path: &Path) -> ArtifactOutcome {
    let label = path.to_string_lossy().to_string();
    match fs::metadata(path) {
        Ok(m) => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Retained,
            bytes: Some(m.len()),
            size_error: None,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Absent,
            bytes: Some(0),
            size_error: None,
        },
        Err(_) => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Retained,
            bytes: Some(0),
            size_error: None,
        },
    }
}

/// Remove a directory artifact (recursively), reporting `absent` / `removed(bytes)` / `failed`.
fn remove_dir_artifact(kind: &'static str, path: &Path) -> ArtifactOutcome {
    let label = path.to_string_lossy().to_string();
    // review-9 #2: `path.exists()` collapses EVERY metadata error (permission denied, ENOTDIR on a
    // bad ancestor, …) into `false`, so an inaccessible `.rgr/` was falsely reported `absent`
    // instead of `failed(reason)`. Mirror `remove_file_artifact`: only a genuine NotFound is
    // `absent`; any other stat error is an honest `failed`.
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Absent,
            bytes: Some(0),
            size_error: None,
        },
        Err(e) => ArtifactOutcome {
            kind,
            label,
            status: ArtifactStatus::Failed(e.to_string()),
            bytes: Some(0),
            size_error: None,
        },
        Ok(_) => {
            // review-11 #2: a traversal/stat fault while sizing must NOT collapse to a numeric 0 —
            // the removal still proceeds, but the reclaimed size is reported UNKNOWN with its reason.
            let (measured, size_faults) = dir_size_bytes(path);
            let (bytes, size_error) = if size_faults.is_empty() {
                (Some(measured), None)
            } else {
                (None, Some(size_faults.join("; ")))
            };
            match fs::remove_dir_all(path) {
                Ok(()) => ArtifactOutcome {
                    kind,
                    label,
                    status: ArtifactStatus::Removed,
                    bytes,
                    size_error,
                },
                Err(e) => ArtifactOutcome {
                    kind,
                    label,
                    status: ArtifactStatus::Failed(e.to_string()),
                    bytes: Some(0),
                    size_error: None,
                },
            }
        }
    }
}

/// Recursive byte size of a directory tree. Returns `(measured_total, faults)`; every
/// `read_dir` / dirent / `file_type` / stat fault is RECORDED in `faults` instead of being
/// silently dropped (review-11 #2 — a non-empty fault list means the total is a lower bound,
/// and callers must render the size as unknown, never as the bare number).
fn dir_size_bytes(path: &Path) -> (u64, Vec<String>) {
    let mut total = 0u64;
    let mut faults = Vec::new();
    let read = match fs::read_dir(path) {
        Ok(r) => r,
        Err(e) => {
            faults.push(format!("read_dir {}: {e}", path.display()));
            return (0, faults);
        }
    };
    for dirent in read {
        let dirent = match dirent {
            Ok(d) => d,
            Err(e) => {
                faults.push(format!("dirent in {}: {e}", path.display()));
                continue;
            }
        };
        let p = dirent.path();
        match dirent.file_type() {
            Ok(ft) if ft.is_dir() => {
                let (sub, sub_faults) = dir_size_bytes(&p);
                total += sub;
                faults.extend(sub_faults);
            }
            Ok(_) => match fs::metadata(&p) {
                Ok(m) => total += m.len(),
                Err(e) => faults.push(format!("stat {}: {e}", p.display())),
            },
            Err(e) => faults.push(format!("file_type {}: {e}", p.display())),
        }
    }
    (total, faults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::types::Repo;
    use repo_graph_storage::StorageConnection;
    use tempfile::tempdir;

    // ── scan_orphans ────────────────────────────────────────────────────

    fn touch(path: &Path, bytes: usize) {
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn entry_referencing(db_dir: &Path, repo_path: &Path) -> RegistryEntry {
        // A registry entry whose db_path lives in db_dir (the shape the registry produces).
        RegistryEntry::new(repo_path.to_path_buf(), db_dir)
    }

    #[test]
    fn scan_classifies_all_three_orphan_classes() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().join("databases");
        fs::create_dir_all(&db_dir).unwrap();

        // A live, referenced repo: real repo dir + its db file + a live -wal sidecar.
        let repo_dir = dir.path().join("live-repo");
        fs::create_dir_all(&repo_dir).unwrap();
        let live = entry_referencing(&db_dir, &repo_dir);
        touch(&live.db_path, 100);
        touch(&sidecar(&live.db_path, "-wal"), 10); // live sidecar → must NOT be flagged stray

        // A dead-path entry (class B): repo dir removed, db file still present + referenced.
        let dead_repo = dir.path().join("gone-repo");
        fs::create_dir_all(&dead_repo).unwrap();
        let dead = entry_referencing(&db_dir, &dead_repo);
        touch(&dead.db_path, 50);
        fs::remove_dir_all(&dead_repo).unwrap();

        // An orphan db (class A) with its own -shm sidecar, referenced by nobody.
        let orphan_db = db_dir.join("deadbeefdeadbeef.db");
        touch(&orphan_db, 4000);
        touch(&db_dir.join("deadbeefdeadbeef.db-shm"), 40);

        // A stray sidecar (class C): base .db does not exist at all.
        touch(&db_dir.join("cafecafecafecafe.db-wal"), 7);

        let report = scan_orphans(&db_dir, &[live.clone(), dead.clone()]);

        assert!(report.scan_error.is_none());
        // Class A: exactly the one orphan db, its bytes include the -shm sidecar.
        assert_eq!(report.orphan_dbs.len(), 1, "one orphan db");
        assert_eq!(report.orphan_db_bytes(), 4040, "db + its shm sidecar");
        // Class C: exactly the base-less -wal.
        assert_eq!(report.stray_sidecars.len(), 1, "one stray sidecar");
        assert_eq!(report.stray_bytes(), 7);
        // The live repo's -wal is NOT stray (its base .db is present + referenced).
        assert!(report.stray_sidecars.iter().all(|s| !s
            .path
            .to_string_lossy()
            .contains(&*live.db_path.file_name().unwrap().to_string_lossy())));
        // Class B: exactly the dead-path entry.
        assert_eq!(report.dead_path_entries.len(), 1, "one dead-path entry");
        assert_eq!(report.dead_path_entries[0].canonical_path, dead_repo);
        // Reclaimable = A + C (NOT the dead-path db, which is still referenced).
        assert_eq!(report.reclaimable_bytes(), 4047);
    }

    #[test]
    fn scan_error_when_db_dir_missing_is_unknown_not_zero() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("no-such-databases");
        let report = scan_orphans(&missing, &[]);
        assert!(report.scan_error.is_some(), "listing failure is surfaced");
        assert!(report.is_empty());
    }

    // review-9 #1: a reclaim candidate whose `stat` FAILS (not merely NotFound) makes the whole scan
    // UNKNOWN — `scan_error` is `Some` — never a false clean zero-byte success. Simulated with a
    // self-referential symlink named like a DB file: `read_dir` lists it, but `fs::metadata` follows
    // it and fails with ELOOP. On the pre-fix code `file_bytes` swallowed that error to `0` and left
    // `scan_error: None`, so doctor/gc/boot could claim a successful scan reporting 0 bytes.
    #[test]
    fn scan_stat_failure_on_an_orphan_is_unknown_not_zero() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        // `seed_state` keys its db_dir at `<state_root>/databases`, so place the candidate there and
        // both the direct scan below and `run_gc`'s internal scan list the same directory.
        let state = seed_state(dir.path());
        let db_dir = state.registry().db_dir().to_path_buf();
        // A `.db` symlink pointing at itself → unreferenced (scanned as an orphan) but un-stattable
        // (`fs::metadata` follows it and fails with ELOOP).
        let looping = db_dir.join("deadbeefdeadbeef.db");
        symlink(&looping, &looping).unwrap();

        let report = scan_orphans(&db_dir, &[]);
        assert!(
            report.scan_error.is_some(),
            "an un-stattable orphan makes the scan UNKNOWN, never a clean zero-byte success"
        );
        // The JSON `ok` a caller keys on must therefore be false for a gc over this dir.
        let out = run_gc(&state, false);
        assert_eq!(
            out.to_json()["ok"],
            serde_json::json!(false),
            "a stat failure during the scan is not a clean gc success"
        );
    }

    // ── run_gc ──────────────────────────────────────────────────────────

    #[test]
    fn gc_dry_run_lists_without_deleting() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let db_dir = state.registry().db_dir().to_path_buf();
        let orphan = db_dir.join("aaaa.db");
        touch(&orphan, 1234);
        touch(&db_dir.join("bbbb.db-wal"), 5); // stray

        let out = run_gc(&state, true);
        assert!(out.dry_run);
        assert_eq!(out.reclaimed_bytes, 0, "dry-run frees nothing");
        assert_eq!(
            out.would_reclaim_bytes, 1239,
            "but reports what it would free"
        );
        assert!(orphan.exists(), "file still present after dry-run");
        assert_eq!(out.items.len(), 2);
        assert!(out.items.iter().all(|i| !i.removed));
    }

    #[test]
    fn gc_reclaims_orphans_and_strays_reports_bytes_and_lists_dead_paths() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let db_dir = state.registry().db_dir().to_path_buf();

        // Orphan db + its sidecar (both reclaimed).
        let orphan = db_dir.join("aaaa.db");
        touch(&orphan, 1000);
        let orphan_wal = db_dir.join("aaaa.db-wal");
        touch(&orphan_wal, 20);
        // Stray sidecar (reclaimed).
        let stray = db_dir.join("cccc.db-shm");
        touch(&stray, 3);

        // A dead-path entry — LISTED, not removed. Register it (referenced db in db_dir), then
        // remove its repo dir so its path is dead but its db is still registry-referenced.
        let dead_repo = root.path().join("gone");
        let dead = register_and_create_db(&state, &dead_repo);
        fs::remove_dir_all(&dead_repo).unwrap();

        let out = run_gc(&state, false);
        assert!(!out.dry_run);
        assert_eq!(out.reclaimed_bytes, 1023, "orphan db + its wal + the stray");
        assert!(!orphan.exists() && !orphan_wal.exists() && !stray.exists());
        assert!(
            dead.db_path.exists(),
            "a referenced (dead-path) db is NOT reclaimed"
        );
        assert_eq!(out.dead_path_entries.len(), 1, "dead-path entry listed");
        assert!(!out.any_failed());
        assert!(
            out.items.iter().all(|i| i.skipped.is_none()),
            "nothing was re-registered, so no candidate is skipped"
        );
    }

    // review-1 #2: a gc over an unscannable databases/ dir is UNKNOWN, not a clean success — `ok` is
    // false (→ non-zero CLI exit) and it must not read as zero orphans.
    #[test]
    fn gc_scan_error_is_not_a_clean_success() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        // Remove the databases/ dir so the listing itself fails → unknown, not a clean success.
        fs::remove_dir_all(state.registry().db_dir()).unwrap();
        let out = run_gc(&state, false);
        assert!(out.scan_error.is_some(), "the listing failure is surfaced");
        let json = out.to_json();
        assert_eq!(
            json["ok"],
            serde_json::json!(false),
            "an unscannable dir is not a clean gc success"
        );
        assert_eq!(json["candidate_count"], serde_json::json!(0));
    }

    // review-1 #4: the dead-path next action shell-quotes a path with spaces so it pastes as ONE
    // argument; a plain path stays unquoted (clean common case). Both the doctor JSON (OrphanReport)
    // and the gc JSON (GcOutcome) carry this same quoted command.
    #[test]
    fn shell_quote_escapes_spaces_and_quotes() {
        assert_eq!(shell_quote("/a/b_c.db"), "/a/b_c.db");
        assert_eq!(shell_quote("/a/b c"), "'/a/b c'");
        assert_eq!(shell_quote("/a/o'brien"), "'/a/o'\\''brien'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn dead_path_next_action_is_shell_quoted_in_both_json_shapes() {
        let spaced = DeadPathEntry {
            canonical_path: PathBuf::from("/Users/me/My Repo/proj"),
            display: "proj".to_string(),
        };
        let plain = DeadPathEntry {
            canonical_path: PathBuf::from("/private/tmp/test_repo"),
            display: "test_repo".to_string(),
        };

        // OrphanReport (the `rmap doctor` source).
        let orphan = OrphanReport {
            dead_path_entries: vec![spaced.clone(), plain.clone()],
            ..Default::default()
        };
        let oj = orphan.to_json();
        assert_eq!(
            oj["dead_path_entries"][0]["next_action"].as_str().unwrap(),
            "rmap repo remove '/Users/me/My Repo/proj'"
        );
        assert_eq!(
            oj["dead_path_entries"][1]["next_action"].as_str().unwrap(),
            "rmap repo remove /private/tmp/test_repo"
        );

        // GcOutcome (the `rmap maintenance gc` source) carries the same quoted command.
        let gc = GcOutcome {
            dry_run: true,
            items: Vec::new(),
            reclaimed_bytes: 0,
            would_reclaim_bytes: 0,
            dead_path_entries: vec![spaced],
            scan_error: None,
        };
        assert_eq!(
            gc.to_json()["dead_path_entries"][0]["next_action"]
                .as_str()
                .unwrap(),
            "rmap repo remove '/Users/me/My Repo/proj'"
        );
    }

    // ── forget_repo ─────────────────────────────────────────────────────

    fn seed_state(state_root: &Path) -> DaemonState {
        let registry = crate::registry::RepoRegistry::with_state_root(state_root).unwrap();
        DaemonState::with_registry(registry)
    }

    /// Register a repo AND create its db file with a valid repo row (so it can be loaded).
    fn register_and_create_db(state: &DaemonState, repo_dir: &Path) -> RegistryEntry {
        fs::create_dir_all(repo_dir).unwrap();
        let entry = {
            let mut reg = state.registry_mut();
            let e = reg.register(repo_dir).unwrap().clone();
            reg.save().unwrap();
            e
        };
        let storage = StorageConnection::open(&entry.db_path).unwrap();
        storage
            .add_repo(&Repo {
                repo_uid: entry.repo_uid.clone(),
                name: "t".to_string(),
                root_path: ".".to_string(),
                default_branch: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
        entry
    }

    #[test]
    fn forget_removes_registry_memory_db_and_sidecars() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        // WAL/SHM sidecars + a .rgr warm cache dir.
        touch(&sidecar(&entry.db_path, "-wal"), 10);
        touch(&sidecar(&entry.db_path, "-shm"), 5);
        let rgr = repo_dir.join(".rgr").join("warm-cache");
        fs::create_dir_all(&rgr).unwrap();
        touch(&rgr.join("default.cache"), 100);
        // Load it into memory so eviction has something to remove.
        state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();

        let report = forget_repo(&state, &entry, false);
        assert!(report.refused.is_none());
        assert!(!report.any_failed(), "{:?}", report.artifacts);

        // Registry gone.
        assert!(state
            .resolve_alias_or_path(&repo_dir.to_string_lossy())
            .is_none());
        // Memory evicted.
        assert!(state.list_repos().is_empty());
        // Files gone.
        assert!(!entry.db_path.exists());
        assert!(!sidecar(&entry.db_path, "-wal").exists());
        assert!(!sidecar(&entry.db_path, "-shm").exists());
        assert!(!repo_dir.join(".rgr").exists());
        // The memory + db artifacts report removed.
        let kinds: Vec<_> = report
            .artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Removed)
            .map(|a| a.kind)
            .collect();
        assert!(
            kinds.contains(&"memory")
                && kinds.contains(&"database")
                && kinds.contains(&"warm-cache")
        );
    }

    #[test]
    fn forget_keep_db_retains_the_file() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        // A present -wal and an ABSENT -shm, to prove BOTH sidecar fates are reported (review-3 #2).
        touch(&sidecar(&entry.db_path, "-wal"), 12);

        let report = forget_repo(&state, &entry, true);
        assert!(report.kept_db);
        assert!(entry.db_path.exists(), "--keep-db leaves the DB file");
        assert!(
            sidecar(&entry.db_path, "-wal").exists(),
            "--keep-db leaves the -wal sidecar too"
        );
        // review-1 #3: the retained, still-present DB reports `retained` (with its real byte size),
        // NOT `absent` — the false report the CLI's "Database retained" line contradicted.
        let db = report
            .artifacts
            .iter()
            .find(|a| a.kind == "database")
            .expect("database artifact present");
        assert_eq!(db.status, ArtifactStatus::Retained, "{:?}", db);
        assert!(
            db.bytes > Some(0),
            "retained DB reports its byte size: {db:?}"
        );
        // review-3 #2: the sidecars are reported on their OWN lines — the present -wal as `retained`
        // (with bytes), the missing -shm as `absent`. Neither is silently left unreported.
        let wal = report
            .artifacts
            .iter()
            .find(|a| a.kind == "wal")
            .expect("wal sidecar artifact present in the keep-db report");
        assert_eq!(wal.status, ArtifactStatus::Retained, "{:?}", wal);
        assert_eq!(wal.bytes, Some(12), "retained -wal reports its size");
        let shm = report
            .artifacts
            .iter()
            .find(|a| a.kind == "shm")
            .expect("shm sidecar artifact present in the keep-db report");
        assert_eq!(
            shm.status,
            ArtifactStatus::Absent,
            "a missing -shm reports absent, not retained: {shm:?}"
        );
        assert!(!report.any_failed());
        // But the registry entry is still gone (forget still forgets the tracking).
        assert!(state
            .resolve_alias_or_path(&repo_dir.to_string_lossy())
            .is_none());
    }

    // review-1 #3: the in-memory state and the db_runtimes slot are reported as SEPARATE artifacts,
    // each with its own removed/absent fate — not folded into one `memory` line.
    #[test]
    fn forget_reports_memory_and_runtime_slot_separately() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        // Load it AND materialize a db_runtimes coordination slot.
        state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();
        state.get_or_create_db_runtime(&entry.db_path).unwrap();

        let report = forget_repo(&state, &entry, false);
        let mem = report
            .artifacts
            .iter()
            .find(|a| a.kind == "memory")
            .expect("memory artifact present");
        let slot = report
            .artifacts
            .iter()
            .find(|a| a.kind == "runtime-slot")
            .expect("runtime-slot artifact present (reported separately)");
        assert_eq!(mem.status, ArtifactStatus::Removed);
        assert_eq!(
            slot.status,
            ArtifactStatus::Removed,
            "the db_runtimes slot fate is reported on its own line"
        );
    }

    // review-2 atomicity (operator-ratified): forget must refuse while a writer HOLDS the DB write
    // lock — even with NO activity op stamped. This is the exact admission-window race review-2
    // raised: an index acquires the DB write lock BEFORE it stamps its activity op, so the OLD
    // snapshot check (`activity().active_for_db`) could miss it and delete active storage. Proving
    // refusal here with ONLY the lock held (no activity) is the proof the barrier is the LOCK, not the
    // snapshot. ("admits a writer first (holds the guard) and proves remove refuses".)
    #[test]
    fn forget_refuses_while_a_writer_holds_the_db_write_lock_and_deletes_nothing() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);

        // A concurrent index holds the DB write lock (the SAME slot forget will fetch via
        // get_or_create_db_runtime) but has NOT yet stamped its activity op.
        let rt = state.get_or_create_db_runtime(&entry.db_path).unwrap();
        let _held = rt.acquire_write();
        assert!(
            state
                .activity()
                .active_for_db(&entry.db_path.canonicalize().unwrap())
                .is_none(),
            "precondition: no activity stamped — only the write lock is held"
        );

        let report = forget_repo(&state, &entry, false);
        assert!(
            report.refused.is_some(),
            "must refuse while the DB write lock is held (admission-window race)"
        );
        assert!(report.artifacts.is_empty(), "no partial deletion");
        assert!(entry.db_path.exists(), "the DB file is untouched");
        assert!(
            state
                .resolve_alias_or_path(&repo_dir.to_string_lossy())
                .is_some(),
            "the registry entry is untouched"
        );
    }

    // review-2 atomicity: the coordinator arm — forget refuses while a query is actively READING the
    // repo (readers take `coordinator.acquire_read`, NOT the DB write lock), deleting nothing. Proves
    // the second lock catches a signal the DB write lock cannot.
    #[test]
    fn forget_refuses_while_a_reader_is_active_and_deletes_nothing() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        let repo_state = state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();

        // An active reader holds the coordinator read permit (the DB write lock stays FREE).
        let _read = repo_state.coordinator.acquire_read();

        let report = forget_repo(&state, &entry, false);
        assert!(
            report.refused.is_some(),
            "must refuse while a query is reading the repo"
        );
        assert!(report.artifacts.is_empty(), "no partial deletion");
        assert!(entry.db_path.exists(), "the DB file is untouched");
        assert!(
            state
                .resolve_alias_or_path(&repo_dir.to_string_lossy())
                .is_some(),
            "the registry entry is untouched"
        );
    }

    // review-2 atomicity ("runs forget under the held guards and proves a late writer waits/
    // re-registers fresh"). Two deterministic halves:
    //   Half 1 — while the DB write lock is HELD (as forget holds it across deletion), a late writer
    //     taking that same lock the blocking way WAITS, and proceeds only once it is released.
    //   Half 2 — after a real forget deletes everything, re-registering the same path mints a FRESH
    //     repo_uid (a new index re-registers fresh against the now-empty registry).
    #[test]
    fn forget_serializes_a_late_writer_then_reregisters_fresh() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc as StdArc;

        let root = tempdir().unwrap();
        let state = StdArc::new(seed_state(root.path()));
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);

        // ── Half 1: a late writer WAITS on the DB write lock while it is held. ──
        let rt = state.get_or_create_db_runtime(&entry.db_path).unwrap();
        let forget_window = rt.acquire_write(); // stands in for forget's held guard during deletion

        let acquired = StdArc::new(AtomicBool::new(false));
        let writer = {
            let rt2 = state.get_or_create_db_runtime(&entry.db_path).unwrap(); // SAME slot
            let acquired2 = StdArc::clone(&acquired);
            std::thread::spawn(move || {
                let _g = rt2.acquire_write(); // blocks until the window is released
                acquired2.store(true, Ordering::SeqCst);
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !acquired.load(Ordering::SeqCst),
            "a late writer must WAIT while forget holds the DB write lock"
        );
        drop(forget_window); // forget finishes deletion + releases
        writer.join().unwrap();
        assert!(
            acquired.load(Ordering::SeqCst),
            "the late writer proceeds once forget releases the lock"
        );

        // ── Half 2: after a real forget, re-registration is FRESH. ──
        state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();
        let report = forget_repo(&state, &entry, false);
        assert!(
            report.refused.is_none() && !report.any_failed(),
            "{:?}",
            report.artifacts
        );
        assert!(!entry.db_path.exists(), "the DB file is gone after forget");

        let fresh = {
            let mut reg = state.registry_mut();
            let e = reg.register(&repo_dir).unwrap().clone();
            reg.save().unwrap();
            e
        };
        assert_ne!(
            fresh.repo_uid, entry.repo_uid,
            "re-registering the same path mints a fresh repo_uid"
        );
    }

    // review-8 (operator-ratified): the regression through the REAL forget path. The prior
    // `forget_serializes_*` test manually held a slot and manually deleted — it never exercised
    // forget's OWN slot lifecycle, so it could not catch the defect that forget dropped the
    // `db_runtimes` slot from the map BEFORE deletion (letting a late index mint a fresh slot + lock
    // and write past the held guard). Here a late index is PARKED at the exact deletion-window instant
    // (fired from forget's `after_evict` seam: guards held, memory evicted, slot still discoverable,
    // nothing deleted). It fetches the SAME slot forget coordinates on and blocks on the held guard; it
    // MUST NOT write while forget's registry/file artifacts are unprocessed; then it resumes and
    // re-registers a FRESH identity. On the pre-fix code (slot dropped early) the late index fetches a
    // FRESH slot, acquires immediately, and the mid-deletion `assert!(!acquired)` fails.
    #[test]
    fn forget_parks_a_late_index_until_all_artifacts_are_processed_then_it_reregisters_fresh() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc as StdArc, Barrier};

        let root = tempdir().unwrap();
        let state = StdArc::new(seed_state(root.path()));
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        // Sidecars + a warm cache so the deletion window spans several real artifacts.
        touch(&sidecar(&entry.db_path, "-wal"), 10);
        touch(&sidecar(&entry.db_path, "-shm"), 5);
        let rgr = repo_dir.join(".rgr").join("warm-cache");
        fs::create_dir_all(&rgr).unwrap();
        touch(&rgr.join("default.cache"), 100);
        state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();

        // The late index: once released, fetch the SAME slot `handle_index` fetches and take its write
        // lock the blocking way. If the slot is still discoverable (fixed) it BLOCKS on forget's guard;
        // if forget dropped it early (buggy) it gets a fresh slot and acquires immediately.
        let go = StdArc::new(Barrier::new(2));
        let acquired = StdArc::new(AtomicBool::new(false));
        let late = {
            let state2 = StdArc::clone(&state);
            let db_path = entry.db_path.clone();
            let go2 = StdArc::clone(&go);
            let acquired2 = StdArc::clone(&acquired);
            std::thread::spawn(move || {
                go2.wait(); // released from inside forget's deletion window
                let rt = state2
                    .get_or_create_db_runtime_for_new_db(&db_path)
                    .unwrap();
                let _g = rt.acquire_write(); // blocks on forget's held guard (fixed) — else races (buggy)
                acquired2.store(true, Ordering::SeqCst);
            })
        };

        // Run the REAL forget body with a barrier fired once, right after memory eviction.
        let report = {
            let go = StdArc::clone(&go);
            let acquired = StdArc::clone(&acquired);
            let mut fired = false;
            forget_repo_barriered(&state, &entry, false, move || {
                if fired {
                    return;
                }
                fired = true;
                go.wait(); // let the late index start fetching the slot
                           // While forget still holds the guard mid-deletion, the late index — having
                           // found forget's still-discoverable slot — is BLOCKED and has written
                           // nothing. (Sleep-then-assert-absence is the house pattern, cf.
                           // `forget_serializes_a_late_writer_then_reregisters_fresh`.)
                std::thread::sleep(std::time::Duration::from_millis(100));
                assert!(
                    !acquired.load(Ordering::SeqCst),
                    "a late index must NOT acquire the write slot while forget is mid-deletion"
                );
            })
        };
        assert!(
            report.refused.is_none() && !report.any_failed(),
            "{:?}",
            report.artifacts
        );
        assert!(!entry.db_path.exists(), "forget deleted the DB file");

        // forget released the guard → the parked index resumes.
        late.join().unwrap();
        assert!(
            acquired.load(Ordering::SeqCst),
            "the parked index resumes once forget completes"
        );

        // It resolves a now-empty registry and re-registers a FRESH identity.
        assert!(
            state
                .resolve_alias_or_path(&repo_dir.to_string_lossy())
                .is_none(),
            "forget emptied the registry"
        );
        let fresh = {
            let mut reg = state.registry_mut();
            let e = reg.register(&repo_dir).unwrap().clone();
            reg.save().unwrap();
            e
        };
        assert_ne!(
            fresh.repo_uid, entry.repo_uid,
            "the resumed index mints a fresh repo_uid"
        );
    }

    #[test]
    fn forget_evicts_memory_even_when_db_deleted_out_of_band() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();
        assert_eq!(state.list_repos().len(), 1);

        // The DB file vanishes out-of-band BEFORE forget (the field bug: eviction was gated on the
        // file canonicalizing, so this used to leave the in-memory state stuck).
        fs::remove_file(&entry.db_path).unwrap();

        let report = forget_repo(&state, &entry, false);
        assert!(report.refused.is_none());
        assert!(
            state.list_repos().is_empty(),
            "memory evicted despite the deleted DB"
        );
        // The db artifact reports `absent` (already gone), not `failed`.
        let db = report
            .artifacts
            .iter()
            .find(|a| a.kind == "database")
            .unwrap();
        assert_eq!(db.status, ArtifactStatus::Absent);
        assert!(!report.any_failed());
    }

    #[test]
    fn forget_reports_failed_when_unlink_fails() {
        // Simulate an unlink failure: the "db file" is actually a NON-EMPTY directory, so
        // `fs::remove_file` fails with an OS error → reported `failed(reason)`, non-zero exit.
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        let entry = {
            let mut reg = state.registry_mut();
            let e = reg.register(&repo_dir).unwrap().clone();
            reg.save().unwrap();
            e
        };
        // Make db_path a directory containing a file (remove_file on a dir fails on macOS/Linux).
        fs::create_dir_all(&entry.db_path).unwrap();
        touch(&entry.db_path.join("blocker"), 1);

        let report = forget_repo(&state, &entry, false);
        assert!(report.refused.is_none());
        assert!(report.any_failed(), "unlink failure surfaces as failed");
        let db = report
            .artifacts
            .iter()
            .find(|a| a.kind == "database")
            .unwrap();
        assert!(matches!(db.status, ArtifactStatus::Failed(_)));
    }

    // review-10: an INACCESSIBLE repo root during forget (here ENOTDIR — a canonical_path that
    // traverses a regular file) must REPORT the `.rgr/` warm-cache artifact as `failed(reason)`,
    // never silently omit it. The old `canonical_path.exists()` gate collapsed the stat fault to
    // `false` and dropped the artifact entirely — a hole in the per-artifact honest-report contract.
    #[test]
    fn forget_reports_failed_warm_cache_on_unstattable_repo_root_not_omitted() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let db_dir = state.registry().db_dir().to_path_buf();
        let not_a_dir = root.path().join("regular-file");
        touch(&not_a_dir, 1);
        let unstattable_repo = not_a_dir.join("repo"); // repo root stat → ENOTDIR (not NotFound)
        let entry = RegistryEntry::new(unstattable_repo, &db_dir);

        let report = forget_repo(&state, &entry, false);
        let warm = report
            .artifacts
            .iter()
            .find(|a| a.kind == "warm-cache")
            .expect("warm-cache artifact must be reported, not omitted");
        assert!(
            matches!(warm.status, ArtifactStatus::Failed(_)),
            "an unstattable repo root must be `failed(reason)`, got {:?}",
            warm.status
        );
    }

    // review-9 #2: an inaccessible `.rgr/` must report `failed(reason)`, not `absent`. The old
    // `path.exists()` collapsed every stat error to `false`. Simulated with a path that traverses a
    // regular file (`<file>/.rgr`) → `fs::metadata` fails with ENOTDIR (not NotFound), which the old
    // code reported `absent`.
    #[test]
    fn remove_dir_artifact_reports_failed_on_stat_error_not_absent() {
        let dir = tempdir().unwrap();
        let not_a_dir = dir.path().join("not-a-dir");
        touch(&not_a_dir, 1);
        // A path THROUGH a regular file: statting it errors with ENOTDIR, and `exists()` → false.
        let through_file = not_a_dir.join(".rgr");
        let out = remove_dir_artifact("warm-cache", &through_file);
        assert!(
            matches!(out.status, ArtifactStatus::Failed(_)),
            "a non-NotFound stat error must be `failed`, not `absent`: {:?}",
            out.status
        );

        // A genuinely-missing `.rgr/` still reports `absent` (the honest common case is preserved).
        let missing = dir.path().join("real-repo").join(".rgr");
        fs::create_dir_all(dir.path().join("real-repo")).unwrap();
        let out = remove_dir_artifact("warm-cache", &missing);
        assert_eq!(out.status, ArtifactStatus::Absent);
    }

    // review-11 #2: a sizing fault inside a removable `.rgr/` must render the reclaimed size as
    // UNKNOWN (`bytes: None` + `size_error`), never as a numeric 0/partial total. The dir holds a
    // real file plus a self-referential symlink: `remove_dir_all` succeeds (symlinks are unlinked,
    // not followed) but statting the loop for sizing fails with ELOOP.
    #[test]
    fn removed_dir_with_unmeasurable_contents_reports_size_unknown_not_zero() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let rgr = dir.path().join(".rgr");
        fs::create_dir_all(&rgr).unwrap();
        touch(&rgr.join("warm.cache"), 10);
        let looping = rgr.join("loop");
        symlink(&looping, &looping).unwrap();

        let out = remove_dir_artifact("warm-cache", &rgr);
        assert_eq!(
            out.status,
            ArtifactStatus::Removed,
            "removal itself succeeds"
        );
        assert_eq!(
            out.bytes, None,
            "unmeasurable size must be None, not a number"
        );
        let why = out.size_error.expect("the sizing fault is named");
        assert!(why.contains("stat"), "reason names the failed stat: {why}");
        assert!(!rgr.exists(), "the directory is actually gone");

        // The measurable case still reports the real number (no regression).
        let rgr2 = dir.path().join(".rgr2");
        fs::create_dir_all(&rgr2).unwrap();
        touch(&rgr2.join("warm.cache"), 10);
        let out = remove_dir_artifact("warm-cache", &rgr2);
        assert_eq!(out.status, ArtifactStatus::Removed);
        assert_eq!(out.bytes, Some(10));
        assert_eq!(out.size_error, None);
    }

    // review-10: a registered repo path that cannot be STATTED (here ENOTDIR — a path that
    // traverses a regular file) is UNKNOWN, never a dead path. Classifying it class-B would make
    // gc/doctor recommend a DESTRUCTIVE `rmap repo remove` on a path that may still be live. Instead
    // the fault feeds `scan_error` (the scan renders UNKNOWN) and the entry is NOT reported dead.
    #[test]
    fn scan_unstattable_registered_path_is_unknown_not_dead_path() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().join("databases");
        fs::create_dir_all(&db_dir).unwrap();

        let not_a_dir = dir.path().join("regular-file");
        touch(&not_a_dir, 1);
        let unstattable = not_a_dir.join("repo"); // stat → ENOTDIR (not NotFound)
        let entry = entry_referencing(&db_dir, &unstattable);

        let report = scan_orphans(&db_dir, std::slice::from_ref(&entry));
        assert!(
            report.dead_path_entries.is_empty(),
            "an unstattable path must NOT be classified dead: {:?}",
            report.dead_path_entries
        );
        assert!(
            report.scan_error.is_some(),
            "the stat fault must render the scan UNKNOWN, never a clean zero"
        );
    }

    // ── operator ruling 2 (2026-08-24): the stale-handle + GC-vs-registration races ──

    // A request holding a stale `Arc<RepoState>` from before a forget must fail HONESTLY when it
    // finally opens storage — and must NOT recreate the deleted DB. This is the choke-point proof:
    // `storage()` uses the NO-CREATE `open_existing`, so the resurrection the reviewer traced
    // (stale read → `open` → recreated unregistered orphan) cannot happen.
    #[test]
    fn stale_repo_handle_after_forget_fails_honestly_and_creates_no_file() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        // Resolve+hold the handle BEFORE forget (models a request admitted before the forget).
        let stale = state.load_repo(&entry.db_path, &entry.repo_uid).unwrap();

        let report = forget_repo(&state, &entry, false);
        assert!(
            report.refused.is_none() && !report.any_failed(),
            "{:?}",
            report.artifacts
        );
        assert!(!entry.db_path.exists(), "forget deleted the DB file");

        // The stale handle opens storage AFTER the deletion → honest missing-db error, no recreate.
        let err = stale
            .storage()
            .expect_err("a stale read after forget must fail, not resurrect the DB");
        assert!(
            err.contains("database not found") || err.contains("not indexed"),
            "the error names the missing/unindexed DB honestly: {err}"
        );
        assert!(
            !entry.db_path.exists(),
            "the stale read must NOT recreate the deleted DB (no read-that-writes / orphan)"
        );
    }

    // GC vs a concurrent (re-)index: a registration takes the target DB's write slot (iteration 5:
    // it registers UNDER that held guard). GC must SKIP any candidate whose slot is held — never
    // delete a DB an index is mid-writing — and report it skipped, not removed/failed.
    #[test]
    fn gc_skips_a_candidate_whose_write_slot_a_registration_holds() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let db_dir = state.registry().db_dir().to_path_buf();
        // A file the scan will class as an orphan, but which a concurrent index is (re-)claiming.
        let contended = db_dir.join("deadbeefdeadbeef.db");
        touch(&contended, 4096);

        // The index holds the SAME slot GC fetches (`get_or_create_db_runtime_for_new_db`).
        let rt = state
            .get_or_create_db_runtime_for_new_db(&contended)
            .unwrap();
        let _held = rt.acquire_write();

        let out = run_gc(&state, false);
        assert_eq!(out.reclaimed_bytes, 0, "a slot-held candidate is not freed");
        assert!(contended.exists(), "the contended DB is left intact");
        let item = out
            .items
            .iter()
            .find(|i| i.path == contended)
            .expect("the contended candidate is reported");
        assert!(
            item.skipped.is_some() && !item.removed && item.error.is_none(),
            "reported skipped (safe), not removed or failed: {item:?}"
        );
    }

    // GC's registry recheck (guard FREE, but the base is still registered): a sidecar of a
    // registered repo whose base `.db` FILE is momentarily absent scans as a "stray", but the base
    // is still registry-referenced — the recheck must SKIP it, proving the recheck is load-bearing
    // beyond the scan's file-only view.
    #[test]
    fn gc_recheck_skips_a_stray_whose_base_is_still_registered() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        let entry = register_and_create_db(&state, &repo_dir);
        // Base .db file gone, registry entry retained, a -wal sidecar left behind.
        fs::remove_file(&entry.db_path).unwrap();
        let wal = sidecar(&entry.db_path, "-wal");
        touch(&wal, 64);

        let out = run_gc(&state, false);
        let item = out
            .items
            .iter()
            .find(|i| i.path == wal)
            .expect("the stray sidecar is considered a candidate by the scan");
        assert!(
            item.skipped.is_some(),
            "the recheck skips a stray whose base .db is still registered: {item:?}"
        );
        assert!(
            wal.exists() && out.reclaimed_bytes == 0,
            "a still-registered repo's sidecar is NOT deleted"
        );
    }

    // review-9 #3: the REQUIRED interleaving through `run_gc`'s real recheck path — a candidate that
    // is a genuine orphan AT SCAN TIME, then re-registered by a concurrent index AFTER the scan but
    // BEFORE the per-unit unlink, must be SKIPPED (not deleted) and the registered DB left intact.
    //
    // This is distinct from the two prior tests: `gc_skips_a_candidate_whose_write_slot_a_registration_holds`
    // exercises the SLOT-HELD branch (nobody re-registers), and `gc_recheck_skips_a_stray_whose_base_is_still_registered`
    // registers BEFORE the scan (so the base is registered at scan time). Here the base is
    // UNregistered at scan time — the scan legitimately classes the file as an orphan — and the
    // registration races in during the post-scan window via the `run_gc_barriered` seam. Only the
    // guard-held registry recheck (operator ruling 2) can catch this; the slot itself is FREE.
    #[test]
    fn gc_skips_a_candidate_registered_after_the_scan_through_the_recheck() {
        let root = tempdir().unwrap();
        let state = seed_state(root.path());
        let repo_dir = root.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();

        // Discover the deterministic db_path this repo maps to, materialize the `.db` file, then
        // REMOVE the registry entry — so at scan time nothing references the file (a true orphan),
        // yet re-registering the same path later reclaims the SAME base name (hash of the path).
        let (db_path, canonical) = {
            let mut reg = state.registry_mut();
            let e = reg.register(&repo_dir).unwrap().clone();
            let (db_path, canonical) = (e.db_path.clone(), e.canonical_path.clone());
            reg.remove(&canonical).unwrap();
            reg.save().unwrap();
            (db_path, canonical)
        };
        touch(&db_path, 4096);

        // Fire a concurrent re-registration in the window AFTER the scan, BEFORE the per-unit recheck.
        let out = {
            let state = &state;
            let repo_dir = &repo_dir;
            run_gc_barriered(state, false, move || {
                let mut reg = state.registry_mut();
                reg.register(repo_dir).unwrap();
                reg.save().unwrap();
            })
        };

        // The candidate was a scan-time orphan but is now registered → SKIPPED via the recheck.
        let item = out
            .items
            .iter()
            .find(|i| i.path == db_path)
            .expect("the scan-time orphan is reported as a candidate");
        assert!(
            item.skipped.is_some() && !item.removed && item.error.is_none(),
            "a candidate re-registered after the scan is skipped (safe), not deleted: {item:?}"
        );
        assert_eq!(out.reclaimed_bytes, 0, "nothing was reclaimed");
        assert!(
            db_path.exists(),
            "the re-registered DB is left intact — GC did not delete a now-live database"
        );
        // Sanity: the recheck (not a held slot) is what saved it — the slot was FREE the whole time.
        // Bind the Arc to a local so the write guard it yields does not outlive its owner.
        let slot = state.get_or_create_db_runtime_for_new_db(&db_path).ok();
        assert!(
            slot.as_ref()
                .and_then(|rt| rt.try_acquire_write())
                .is_some(),
            "the DB write slot was free; only the registry recheck skipped the candidate"
        );
        // And the re-registration really did land in the registry.
        assert!(
            state
                .resolve_alias_or_path(&canonical.to_string_lossy())
                .is_some(),
            "the concurrent registration persisted"
        );
    }
}
