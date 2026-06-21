//! PERF-INSTRUMENTATION-1: runtime perf-trace gate.
//!
//! Perf tracing is enabled at RUNTIME via the `RMAP_PERF` environment variable,
//! read once into a process-global. This lets an ALREADY-INSTALLED daemon binary
//! emit `[PERF]` markers WITHOUT a `--features perf-trace` rebuild — the operator
//! launches (or relaunches) the daemon with `RMAP_PERF=1` set in its environment.
//!
//! # Levels
//!
//! | `RMAP_PERF` | level | effect                                                   |
//! |-------------|-------|----------------------------------------------------------|
//! | unset / `0` | 0     | off — every marker check is a single relaxed atomic load |
//! | `1`         | 1     | per-request duration + index/refresh phase timing        |
//! | `2` (or +)  | 2     | additionally per-file extraction progress                |
//!
//! Unparseable or empty values are treated as `0` (off); values above the
//! highest defined level are clamped to the most-verbose level.
//!
//! # Home crate rationale
//!
//! This gate is read by two macros in two crates: `perf_log!` here in
//! `repo-index` (index/refresh phase markers) and `perf_trace!` in
//! `daemon-runtime` (per-request + handler markers). `daemon-runtime` already
//! depends on `repo-index`, and `repo-index` cannot depend back on it, so this
//! crate is the lowest common home that gives ONE process-global without a new
//! crate or dependency edge.
//!
//! # Compile-time force-on
//!
//! The legacy `--features perf-trace` build flag is retained as a force-on: the
//! macros emit when `cfg!(feature = "perf-trace")` OR the runtime gate is on.
//! That keeps existing perf-trace builds behaving exactly as before (the
//! `cfg!` constant short-circuits, so a force-on build pays no atomic load).

use repo_graph_indexer::types::{IndexPhase, IndexProgressEvent};
use repo_graph_indexer::PhaseTimings;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

/// Highest defined perf level. Higher `RMAP_PERF` values clamp to this.
const MAX_LEVEL: u8 = 2;

/// Sentinel meaning "the env var has not been read yet". `u8::MAX` can never be
/// a real level (levels are clamped to `MAX_LEVEL`), so it is an unambiguous
/// "unread" marker for the lazy one-time read.
const UNREAD: u8 = u8::MAX;

/// Process-global perf level. Initialized lazily on the first `perf_level()`
/// call (or eagerly at daemon startup via [`init`]). After the first read,
/// every check is a single relaxed atomic load.
static PERF_LEVEL: AtomicU8 = AtomicU8::new(UNREAD);

/// Pure mapping from a raw `RMAP_PERF` value to an effective perf level.
///
/// Kept separate from the process-global so the gate logic is deterministically
/// unit-testable without touching environment or global state.
fn level_from_env(raw: Option<&str>) -> u8 {
    let parsed = match raw {
        Some(v) => v.trim().parse::<u8>().unwrap_or(0),
        None => 0,
    };
    parsed.min(MAX_LEVEL)
}

/// Whether any perf tracing is active at `level` (request + phase markers).
#[inline]
fn is_enabled(level: u8) -> bool {
    level >= 1
}

/// Whether per-file (verbose) perf tracing is active at `level`.
#[inline]
fn is_file_enabled(level: u8) -> bool {
    level >= MAX_LEVEL
}

/// The effective perf level (0/1/2), reading `RMAP_PERF` exactly once.
///
/// The first call reads the environment and caches the result in the
/// process-global; all subsequent calls are a single relaxed atomic load. The
/// read is idempotent (the environment is stable for a daemon's lifetime), so
/// the unsynchronized first-read race between threads is benign: every racer
/// computes the same value.
pub fn perf_level() -> u8 {
    let cached = PERF_LEVEL.load(Ordering::Relaxed);
    if cached != UNREAD {
        return cached;
    }
    let level = level_from_env(std::env::var("RMAP_PERF").ok().as_deref());
    PERF_LEVEL.store(level, Ordering::Relaxed);
    level
}

/// True when request/phase perf markers should emit (level >= 1).
#[inline]
pub fn perf_enabled() -> bool {
    is_enabled(perf_level())
}

/// True when per-file extraction markers should emit (level >= 2).
#[inline]
pub fn perf_file_enabled() -> bool {
    is_file_enabled(perf_level())
}

/// Force the one-time `RMAP_PERF` read at startup and return the active level.
///
/// The daemon runtime calls this once at boot so the process-global is
/// populated before the first request and the active level can be logged.
pub fn init() -> u8 {
    perf_level()
}

// ── PERF-INSTRUMENTATION-1: index phase summary + progress markers ───────────
//
// The orchestrator measures the REAL phase boundaries (see
// `repo_graph_indexer::index_timing::PhaseTimings`); this module owns the
// formatting + emission. Keeping that here (not in the 4500-line compose.rs)
// is the structural-guardrail fix from the build-0 review.

/// Item counts for the index perf summary, read from the `IndexResult`.
///
/// A small display bundle so [`format_index_summary`]'s signature and its unit
/// test stay readable. Sole user: the compose index path → `format_index_summary`.
pub struct IndexCounts {
    pub nodes: u64,
    pub edges: u64,
    pub unresolved: u64,
}

/// Format the one-line per-phase index summary (deterministic; unit-tested).
///
/// `discover` / `init` / `postpass` are compose-level windows the caller times;
/// `extract` / `resolve` / `store` / `finalize` are the orchestrator's REAL
/// measurements (`store` = the actual storage write calls, NOT the finalization
/// tail). `other` is the unattributed remainder (e.g. the contract/gRPC
/// subpipelines that run inside `index_repo` after the core pipeline) so the
/// line self-accounts to `total` — unattributed time is made visible, not hidden.
#[allow(clippy::too_many_arguments)]
pub fn format_index_summary(
    repo: &str,
    discover_ms: u128,
    discover_files: usize,
    init_ms: u128,
    timings: &PhaseTimings,
    postpass_ms: u128,
    total_ms: u128,
    counts: &IndexCounts,
) -> String {
    let known = discover_ms
        .saturating_add(init_ms)
        .saturating_add(timings.extract_ms as u128)
        .saturating_add(timings.resolve_ms as u128)
        .saturating_add(timings.store_ms as u128)
        .saturating_add(timings.finalize_ms as u128)
        .saturating_add(postpass_ms);
    let other_ms = total_ms.saturating_sub(known);
    format!(
        "[PERF] index {}: discover={}ms({} files) init={}ms extract={}ms resolve={}ms \
         store={}ms finalize={}ms postpass={}ms other={}ms total={}ms | nodes={} edges={} unresolved={}",
        repo,
        discover_ms,
        discover_files,
        init_ms,
        timings.extract_ms,
        timings.resolve_ms,
        timings.store_ms,
        timings.finalize_ms,
        postpass_ms,
        other_ms,
        total_ms,
        counts.nodes,
        counts.edges,
        counts.unresolved,
    )
}

/// Emits the phase-ENTRY markers and (level-2) per-file extraction markers
/// derived from the orchestrator's `IndexProgressEvent` stream.
///
/// Phase-entry labels are honest: `> extract` / `> resolve` / `> finalize` map
/// to the orchestrator's Extracting / Resolving / Persisting events (the
/// Persisting event fires AFTER the writes, so it is the finalization boundary —
/// build-0 mislabeled it `> store`). There is deliberately NO `> store` entry
/// marker: the storage writes interleave the extract/resolve phases and the
/// orchestrator is pure-policy (it must not format log lines), so store time is
/// reported in the summary's `store=` field, not as an entry marker.
///
/// Only constructed + observed when perf is on (level >= 1); per-file markers
/// additionally require `file_level` (level >= 2). Emits to stderr (the daemon
/// log) directly — the per-file `<ms>` is the gap since the previous file entry
/// (≈ that file's processing time), so a hang names the last file reached.
pub struct IndexProgressMarkers<'a> {
    repo: &'a str,
    expected_files: u64,
    file_level: bool,
    seen_phase: Option<IndexPhase>,
    last_file_at: Option<Instant>,
}

impl<'a> IndexProgressMarkers<'a> {
    /// `expected_files` is the discovered source-file count (the first
    /// Extracting event carries a 0 total, which would read as a false "0
    /// files"), `file_level` is true at `RMAP_PERF` level >= 2.
    pub fn new(repo: &'a str, expected_files: u64, file_level: bool) -> Self {
        Self {
            repo,
            expected_files,
            file_level,
            seen_phase: None,
            last_file_at: None,
        }
    }

    /// Observe one orchestrator progress event. Caller guarantees perf is on.
    pub fn observe(&mut self, event: &IndexProgressEvent) {
        // Phase-entry marker on transition → a hang names the last phase entered.
        if self.seen_phase != Some(event.phase) {
            match event.phase {
                IndexPhase::Scanning => {}
                IndexPhase::Extracting => {
                    eprintln!(
                        "[PERF] index {}: > extract ({} files)",
                        self.repo, self.expected_files
                    );
                }
                IndexPhase::Resolving => eprintln!("[PERF] index {}: > resolve", self.repo),
                IndexPhase::Persisting => eprintln!("[PERF] index {}: > finalize", self.repo),
            }
            self.seen_phase = Some(event.phase);
        }

        // Level 2: per-file extraction progress. One Extracting event per file
        // names the file just entered; `<ms>` is the gap since the previous
        // entry (≈ that file's processing time). The last path printed is the
        // file in flight — the hang suspect.
        if self.file_level {
            if let (IndexPhase::Extracting, Some(file)) = (event.phase, event.file.as_deref()) {
                let now = Instant::now();
                let gap_ms = self
                    .last_file_at
                    .map(|at| now.saturating_duration_since(at).as_millis())
                    .unwrap_or(0);
                eprintln!(
                    "[PERF] index file {} ({}/{}): {}ms",
                    file, event.current, event.total, gap_ms
                );
                self.last_file_at = Some(now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_from_env_maps_values() {
        assert_eq!(level_from_env(None), 0, "unset -> off");
        assert_eq!(level_from_env(Some("")), 0, "empty -> off");
        assert_eq!(level_from_env(Some("0")), 0);
        assert_eq!(level_from_env(Some("1")), 1);
        assert_eq!(level_from_env(Some("2")), 2);
        assert_eq!(
            level_from_env(Some(" 1 ")),
            1,
            "surrounding whitespace trimmed"
        );
        assert_eq!(
            level_from_env(Some("9")),
            MAX_LEVEL,
            "above max clamps to most-verbose"
        );
        assert_eq!(level_from_env(Some("garbage")), 0, "non-numeric -> off");
        assert_eq!(
            level_from_env(Some("true")),
            0,
            "non-numeric -> off (RMAP_PERF is numeric)"
        );
    }

    #[test]
    fn predicates_track_level() {
        // level 0: every marker is a no-op.
        assert!(!is_enabled(0));
        assert!(!is_file_enabled(0));
        // level 1: request/phase markers on, per-file off.
        assert!(is_enabled(1));
        assert!(!is_file_enabled(1));
        // level 2: both on.
        assert!(is_enabled(2));
        assert!(is_file_enabled(2));
    }

    #[test]
    fn env_to_emit_decision_end_to_end() {
        // The exact wiring a marker uses: env value -> level -> emit decision.
        // off: RMAP_PERF unset/0 -> neither phase nor per-file markers emit.
        assert!(!is_enabled(level_from_env(None)));
        assert!(!is_enabled(level_from_env(Some("0"))));
        // RMAP_PERF=1 -> phase markers emit, per-file does not.
        assert!(is_enabled(level_from_env(Some("1"))));
        assert!(!is_file_enabled(level_from_env(Some("1"))));
        // RMAP_PERF=2 -> phase AND per-file markers emit.
        assert!(is_enabled(level_from_env(Some("2"))));
        assert!(is_file_enabled(level_from_env(Some("2"))));
    }

    #[test]
    fn index_summary_reports_real_store_and_self_accounts() {
        // The summary's `store` field is the orchestrator's REAL write time,
        // reported separately from the finalization tail (`finalize`) — the
        // build-0 review's core complaint. And the line self-accounts: the
        // `other` remainder = total − sum(known phases).
        let timings = PhaseTimings {
            extract_ms: 30,
            resolve_ms: 10,
            store_ms: 40,   // real storage-write time
            finalize_ms: 5, // distinct from store
        };
        let counts = IndexCounts {
            nodes: 100,
            edges: 80,
            unresolved: 7,
        };
        // known = 3+2+30+10+40+5+8 = 98; total = 120 → other = 22.
        let line = format_index_summary("repoX", 3, 12, 2, &timings, 8, 120, &counts);

        assert!(
            line.contains("store=40ms"),
            "store must be the real write time, not finalization: {line}"
        );
        assert!(
            line.contains("finalize=5ms"),
            "finalize is reported distinctly from store: {line}"
        );
        assert!(line.contains("extract=30ms"), "{line}");
        assert!(line.contains("resolve=10ms"), "{line}");
        assert!(
            line.contains("other=22ms"),
            "other must be total minus the known phases (self-accounting): {line}"
        );
        assert!(line.contains("total=120ms"), "{line}");
        assert!(line.contains("discover=3ms(12 files)"), "{line}");
        assert!(line.contains("nodes=100 edges=80 unresolved=7"), "{line}");
    }

    #[test]
    fn index_summary_other_saturates_when_phases_exceed_total() {
        // Defensive: if measured phases slightly exceed the coarse total
        // (ms truncation / Instant-capture overlap), `other` saturates to 0
        // rather than underflowing.
        let timings = PhaseTimings {
            extract_ms: 50,
            resolve_ms: 50,
            store_ms: 50,
            finalize_ms: 50,
        };
        let counts = IndexCounts {
            nodes: 0,
            edges: 0,
            unresolved: 0,
        };
        let line = format_index_summary("r", 0, 0, 0, &timings, 0, 10, &counts);
        assert!(
            line.contains("other=0ms"),
            "other must saturate to 0, not underflow: {line}"
        );
    }
}
