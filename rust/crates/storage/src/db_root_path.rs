//! Resolve a stored `repos.root_path` against the DB file's parent directory.
//!
//! `root_path` is stored RELATIVE to the DB file's directory (the daemon's
//! `compute_storage_root_path` / pathdiff convention). Any consumer that turns
//! it back into a filesystem path MUST resolve it against the DB file's parent
//! — NOT the process's current working directory. Resolving against cwd is the
//! ENRICH-ROOT-1 defect class: a launchd-served daemon runs at `/`, so a
//! cwd-relative resolve silently pointed at the wrong (or a nonexistent)
//! directory and enrichment attempted 0 edges without saying so.
//!
//! Abstraction ledger —
//! - **What:** the DB-parent join for a stored (DB-relative) repo `root_path`.
//! - **Concrete current users:** [`crate::agent_orient_reads::doc_inventory`]
//!   (the live doc inventory that backs dense `orient`) and
//!   `crate::enrichment_impl`'s `EnrichmentStoragePort::get_repo_root` (the
//!   enrichment pipeline's repo root). Two concrete callers, in two modules.
//! - **Axis of variation:** none — a single shared MECHANISM (the pathdiff
//!   storage convention). This is not a polymorphism seam; it is deduplication
//!   of load-bearing path logic that had already been written twice.
//! - **Rejected simpler alternative:** inline the join at each site (the status
//!   quo). Rejected because ENRICH-ROOT-1 is a RECURRENCE of exactly this bug —
//!   `agent_orient_reads.rs` already fixed it once for doc inventory; a third
//!   hand-rolled copy is what the slice forbids.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Join `raw_root` onto the DB file's parent directory.
///
/// - A RELATIVE `raw_root` (the stored convention) resolves against the DB
///   file's parent, yielding an absolute path independent of the process cwd.
/// - An ABSOLUTE `raw_root` replaces (standard [`Path::join`] semantics), so
///   this is correct whether the stored value is relative or absolute.
/// - An in-memory DB has no file path (`Connection::path()` is `None`) → fall
///   back to `raw_root` as-is. This preserves the behavior the in-memory test
///   fakes rely on; a real (on-disk) DB always has a parent.
///
/// This performs NO filesystem access and cannot fail — it is a pure path join.
/// Whether the resolved path must EXIST is the caller's decision (doc inventory
/// degrades to an empty inventory; enrichment treats a non-resolvable root as a
/// hard error carrying the attempted path).
pub(crate) fn resolve_root_against_db_parent(conn: &Connection, raw_root: &str) -> PathBuf {
    match conn.path().and_then(|db| Path::new(db).parent()) {
        Some(parent) => parent.join(raw_root),
        None => PathBuf::from(raw_root),
    }
}
