//! Daemon self-measurement: process resident memory + on-disk storage size.
//!
//! DOCTOR-RESOURCE-REPORT. Pure platform MECHANISM behind the `daemon_info`
//! diagnostic. Single current user: `dispatch::ServiceDispatcher::handle_daemon_info`,
//! which surfaces these to `rmap doctor` so an operator can see whether the
//! in-memory LiveGraph substrate ballooned the daemon footprint, and how much disk
//! the warm SCIP/SQLite state costs across all repos.
//!
//! Abstraction note (per repo guardrails): this is a new module because (a) the read
//! is per-OS FFI mechanism (macOS mach / Linux `/proc` / `getrusage`) plus a
//! filesystem walk, and (b) `dispatch.rs` is already ~6.8k lines — the structural
//! guardrail forbids appending new responsibilities there. Axis of variation: the
//! per-platform RSS read. Rejected alternative: inlining the `#[cfg]` FFI + dir walk
//! inside the handler (mixes platform mechanism into the oversized orchestration file).
//!
//! Degradation contract: every reader returns `Option` and NEVER panics or
//! hard-errors. `None` means the metric is genuinely unreadable on this
//! platform/sandbox — an UNKNOWN. The daemon serializes `None` as JSON `null`;
//! `rmap doctor` renders it "unavailable" and keeps the health verdict green. An
//! empty-but-readable `databases/` dir is `Some(0)` (known-zero), never `None`.

use std::path::Path;

/// Current resident set size (RSS) of THIS process, in bytes.
///
/// The LIVE footprint — what the daemon (including the resident LiveGraph
/// partitions) is holding right now. This is the headline answer to "did the
/// daemon balloon?". Decision (decide-and-record): we report CURRENT RSS as the
/// primary figure (the slice prefers a live footprint over a high-water mark) and
/// pair it with [`peak_rss_bytes`] for transient spikes.
///
/// - macOS: `task_info(MACH_TASK_BASIC_INFO).resident_size` (already bytes).
/// - Linux: field 2 (resident pages) of `/proc/self/statm` × page size.
/// - other: `None`.
pub fn current_rss_bytes() -> Option<u64> {
    current_rss_bytes_impl()
}

// DELIBERATE, CONTAINED DEBT (decide-and-record): `libc`'s entire Mach module is
// deprecated in favor of the `mach2` crate, but the symbols are present and correct in
// the pinned `libc 0.2.184` (deprecation != removal). We use them under a localized
// `#[allow(deprecated)]` rather than add a `mach2` dependency edge for a single syscall.
// MIGRATION TRIGGER: a future `libc` bump that removes these symbols fails the build —
// switch this fn to `mach2` then.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn current_rss_bytes_impl() -> Option<u64> {
    use std::mem;

    // SAFETY: `task_info` with `MACH_TASK_BASIC_INFO` fills a `mach_task_basic_info`
    // into `info`. `count` is initialized to the struct size measured in `natural_t`
    // units, exactly as the Mach ABI requires (the MACH_TASK_BASIC_INFO_COUNT idiom).
    // `resident_size` is read only on `KERN_SUCCESS`.
    unsafe {
        let mut info: libc::mach_task_basic_info = mem::zeroed();
        let mut count = (mem::size_of::<libc::mach_task_basic_info>()
            / mem::size_of::<libc::natural_t>())
            as libc::mach_msg_type_number_t;
        let kr = libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO as libc::task_flavor_t,
            &mut info as *mut _ as libc::task_info_t,
            &mut count,
        );
        if kr == libc::KERN_SUCCESS {
            Some(info.resident_size as u64)
        } else {
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn current_rss_bytes_impl() -> Option<u64> {
    // /proc/self/statm columns (in pages): size resident shared text lib data dt.
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: sysconf(_SC_PAGESIZE) is a pure query with no preconditions.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return None;
    }
    Some(resident_pages.saturating_mul(page_size as u64))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn current_rss_bytes_impl() -> Option<u64> {
    None
}

/// Peak resident set size (high-water mark) of THIS process, in bytes.
///
/// Cross-platform via `getrusage(RUSAGE_SELF).ru_maxrss`, NORMALIZED to bytes:
/// macOS reports `ru_maxrss` in bytes, Linux in kilobytes. One syscall, no
/// allocation — cheap enough to always pair with [`current_rss_bytes`]. Catches a
/// transient balloon (e.g. a large index pass) that the live figure would miss.
/// `None` if the call fails or reports a non-positive value.
#[cfg(unix)]
pub fn peak_rss_bytes() -> Option<u64> {
    use std::mem;

    // SAFETY: `getrusage` fully initializes the `rusage` out-param on success
    // (return 0). We only read it on success.
    let maxrss = unsafe {
        let mut usage: libc::rusage = mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return None;
        }
        usage.ru_maxrss
    };

    if maxrss <= 0 {
        return None;
    }
    let maxrss = maxrss as u64;

    // macOS: ru_maxrss is already bytes. Linux (and other unix): kilobytes.
    #[cfg(target_os = "macos")]
    let bytes = maxrss;
    #[cfg(not(target_os = "macos"))]
    let bytes = maxrss.saturating_mul(1024);

    Some(bytes)
}

#[cfg(not(unix))]
pub fn peak_rss_bytes() -> Option<u64> {
    None
}

/// Total on-disk size, in bytes, of all regular files under `dir` (recursive).
///
/// Used to sum the daemon's `databases/` state root — every repo's `.db`
/// (plus `-wal`/`-shm`) — for the `rmap doctor` "total storage" line. Recursive so
/// the figure stays correct if the layout ever nests; today `databases/` is flat.
/// Symlinks are NOT followed (no escape out of the state root, no loops).
///
/// Returns:
/// - `Some(n)` — sum of regular-file lengths. `Some(0)` for an empty but readable
///   dir (known-zero).
/// - `None` — `dir` cannot be enumerated (missing / unreadable): UNKNOWN, not zero.
///
/// Per-entry `stat` failures are skipped (best-effort sum) rather than collapsing
/// the whole read to `None`; only a failed top-level enumeration is `None`.
pub fn directory_size_bytes(dir: &Path) -> Option<u64> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            // Recurse into real subdirectories; a `None` (unreadable subdir) is
            // skipped so one bad subtree does not erase the rest of the sum.
            if let Some(sub) = directory_size_bytes(&entry.path()) {
                total = total.saturating_add(sub);
            }
        } else if file_type.is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
        // Symlinks and other entry kinds are intentionally skipped.
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn current_rss_is_real_and_nonzero() {
        // REAL metric, not a placeholder: a running process always has resident pages.
        let rss = current_rss_bytes().expect("current RSS readable on macOS/Linux");
        assert!(rss > 0, "RSS must be a real non-zero footprint, got {rss}");
    }

    #[cfg(unix)]
    #[test]
    fn peak_rss_is_real_and_nonzero() {
        let peak = peak_rss_bytes().expect("peak RSS readable on unix");
        assert!(peak > 0, "peak RSS must be non-zero, got {peak}");
    }

    #[test]
    fn directory_size_sums_flat_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.db"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.path().join("b.db"), vec![0u8; 2345]).unwrap();
        assert_eq!(directory_size_bytes(dir.path()), Some(3345));
    }

    #[test]
    fn directory_size_recurses_into_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("nested");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(dir.path().join("top.db"), vec![0u8; 100]).unwrap();
        std::fs::write(sub.join("deep.db"), vec![0u8; 200]).unwrap();
        assert_eq!(directory_size_bytes(dir.path()), Some(300));
    }

    #[test]
    fn empty_dir_is_known_zero_not_unknown() {
        let dir = tempfile::tempdir().unwrap();
        // Existing-but-empty is a real zero, NOT `None` (explicit-degradation rule).
        assert_eq!(directory_size_bytes(dir.path()), Some(0));
    }

    #[test]
    fn missing_dir_is_unknown() {
        let missing = Path::new("/nonexistent/repo-graph/databases/zzz");
        assert_eq!(directory_size_bytes(missing), None);
    }
}
