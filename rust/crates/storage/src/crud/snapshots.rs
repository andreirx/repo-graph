//! CRUD methods for the `snapshots` table.
//!
//! Mirrors `createSnapshot`, `getSnapshot`, `getLatestSnapshot`,
//! `updateSnapshotStatus`, `updateSnapshotCounts` from
//! `src/adapters/storage/sqlite/sqlite-storage.ts:248-319`.
//!
//! All five methods are single-statement operations and are NOT
//! transaction-wrapped.

use rusqlite::Connection;
use uuid::Uuid;

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::{CreateSnapshotInput, Snapshot, UpdateSnapshotStatusInput};

/// Initial status for new snapshots, matching the TS
/// `SnapshotStatus.BUILDING` constant from
/// `src/core/model/types.ts:121`.
const SNAPSHOT_STATUS_BUILDING: &str = "building";

/// "Latest" filter status for `get_latest_snapshot`, matching the
/// TS `SnapshotStatus.READY` constant. Per the R2-E parity
/// lock: `get_latest_snapshot` returns the latest READY snapshot
/// only, not the latest by timestamp regardless of status.
const SNAPSHOT_STATUS_READY: &str = "ready";

impl StorageConnection {
	/// Create a new snapshot row in `BUILDING` status.
	///
	/// Mirrors TS `createSnapshot` (sqlite-storage.ts:248).
	/// Behavior:
	///
	///   1. Generate a unique `snapshot_uid` of the form
	///      `<repo_uid>/<ISO-timestamp>/<UUID-v4-prefix>`. Same
	///      format as TS:
	///      ```text
	///      `${input.repoUid}/${new Date().toISOString()}/${uuidv4().slice(0, 8)}`
	///      ```
	///   2. Generate a `created_at` ISO timestamp.
	///   3. INSERT the new row with status = `BUILDING` and the
	///      counter columns left at their schema defaults (0).
	///   4. Read the row back via `get_snapshot(uid)` and return
	///      the DTO.
	///
	/// The read-back is a defensive check inherited from TS: if
	/// the insert succeeded but the read returns None, something
	/// is wrong with the database state. Returns
	/// `StorageError::Sqlite` (with rusqlite::Error::QueryReturnedNoRows
	/// wrapped) in that case.
	///
	/// **Not transaction-wrapped.** The TS adapter does not wrap
	/// either; the read-back depends on autocommit isolation. If
	/// the schema does not implicitly serialize the INSERT and
	/// the subsequent SELECT, the read-back could miss the row
	/// — but in practice both run on the same connection in
	/// SQLite's default journal mode, which serializes single-
	/// connection operations. Mirrors TS exactly.
	pub fn create_snapshot(
		&self,
		input: &CreateSnapshotInput,
	) -> Result<Snapshot, StorageError> {
		let now = current_iso_timestamp(self.connection())?;
		let uid = format!(
			"{}/{}/{}",
			input.repo_uid,
			now,
			&Uuid::new_v4().to_string()[..8]
		);

		self.connection().execute(
			"INSERT INTO snapshots \
			 (snapshot_uid, repo_uid, parent_snapshot_uid, kind, basis_ref, basis_commit, status, created_at, label, toolchain_json) \
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			rusqlite::params![
				uid,
				input.repo_uid,
				input.parent_snapshot_uid,
				input.kind,
				input.basis_ref,
				input.basis_commit,
				SNAPSHOT_STATUS_BUILDING,
				now,
				input.label,
				input.toolchain_json,
			],
		)?;

		// Read back. The TS code throws a generic Error if the
		// read-back fails; the Rust port surfaces it as a
		// rusqlite::Error::QueryReturnedNoRows wrapped in
		// StorageError::Sqlite.
		match self.get_snapshot(&uid)? {
			Some(s) => Ok(s),
			None => Err(StorageError::Sqlite(
				rusqlite::Error::QueryReturnedNoRows,
			)),
		}
	}

	/// Look up a snapshot by uid. Returns `Ok(None)` if not
	/// found, `Ok(Some(Snapshot))` on hit. Mirrors TS
	/// `getSnapshot` (sqlite-storage.ts:276).
	pub fn get_snapshot(
		&self,
		snapshot_uid: &str,
	) -> Result<Option<Snapshot>, StorageError> {
		let result = self.connection().query_row(
			"SELECT * FROM snapshots WHERE snapshot_uid = ?",
			rusqlite::params![snapshot_uid],
			Snapshot::from_row,
		);
		match result {
			Ok(s) => Ok(Some(s)),
			Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
			Err(e) => Err(StorageError::Sqlite(e)),
		}
	}

	/// Look up the latest READY snapshot for a repo, ordered by
	/// `created_at DESC`. Mirrors TS `getLatestSnapshot`
	/// (sqlite-storage.ts:283).
	///
	/// **Parity-critical:** the WHERE clause includes
	/// `status = 'ready'`. Snapshots in `BUILDING`, `STALE`, or
	/// `FAILED` status are excluded. A repo with only a BUILDING
	/// snapshot returns `Ok(None)` from this method even though
	/// the snapshot exists.
	pub fn get_latest_snapshot(
		&self,
		repo_uid: &str,
	) -> Result<Option<Snapshot>, StorageError> {
		let result = self.connection().query_row(
			"SELECT * FROM snapshots \
			 WHERE repo_uid = ? AND status = ? \
			 ORDER BY created_at DESC LIMIT 1",
			rusqlite::params![repo_uid, SNAPSHOT_STATUS_READY],
			Snapshot::from_row,
		);
		match result {
			Ok(s) => Ok(Some(s)),
			Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
			Err(e) => Err(StorageError::Sqlite(e)),
		}
	}

	/// Update a snapshot's status and completed_at timestamp.
	/// Mirrors TS `updateSnapshotStatus` (sqlite-storage.ts:296).
	///
	/// `completed_at` defaults to the current ISO timestamp when
	/// `input.completed_at` is `None`. Matches TS
	/// `input.completedAt ?? new Date().toISOString()`.
	pub fn update_snapshot_status(
		&self,
		input: &UpdateSnapshotStatusInput,
	) -> Result<(), StorageError> {
		let completed_at = match &input.completed_at {
			Some(ts) => ts.clone(),
			None => current_iso_timestamp(self.connection())?,
		};
		self.connection().execute(
			"UPDATE snapshots SET status = ?, completed_at = ? WHERE snapshot_uid = ?",
			rusqlite::params![input.status, completed_at, input.snapshot_uid],
		)?;
		Ok(())
	}

	/// Recompute and update the three counter columns
	/// (`files_total`, `nodes_total`, `edges_total`) for a
	/// snapshot from actual `COUNT(*)` queries against
	/// `file_versions`, `nodes`, `edges`.
	///
	/// Mirrors TS `updateSnapshotCounts` (sqlite-storage.ts:309).
	/// The TS code uses a single UPDATE with three correlated
	/// subqueries; the Rust port mirrors that exact SQL shape.
	///
	/// Single statement (one UPDATE with three SELECT subqueries),
	/// not transaction-wrapped.
	pub fn update_snapshot_counts(
		&self,
		snapshot_uid: &str,
	) -> Result<(), StorageError> {
		self.connection().execute(
			"UPDATE snapshots SET \
			   files_total = (SELECT COUNT(*) FROM file_versions WHERE snapshot_uid = ?), \
			   nodes_total = (SELECT COUNT(*) FROM nodes WHERE snapshot_uid = ?), \
			   edges_total = (SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?) \
			 WHERE snapshot_uid = ?",
			rusqlite::params![snapshot_uid, snapshot_uid, snapshot_uid, snapshot_uid],
		)?;
		Ok(())
	}
}

/// Generate an ISO 8601 / RFC 3339 timestamp for the current
/// instant in the **exact format** that JavaScript's
/// `new Date().toISOString()` produces:
///
/// ```text
/// YYYY-MM-DDTHH:MM:SS.sssZ
/// ```
///
/// Example: `2025-01-01T12:34:56.789Z`. 24 characters, ASCII
/// only, UTC ('Z' suffix), millisecond precision.
///
/// Implemented via SQLite's `strftime` function with the
/// `%Y-%m-%dT%H:%M:%fZ` format string. SQLite's `%f` substitution
/// produces `SS.SSS` (seconds with three fractional digits), and
/// SQLite's `'now'` modifier always returns UTC, so the literal
/// `Z` suffix is correct without any timezone conversion.
///
/// **Why this matters (R2-E parity correction):**
///
/// The TypeScript adapter persists snapshot timestamps via
/// `new Date().toISOString()` in BOTH the `created_at` /
/// `completed_at` columns AND the `snapshot_uid` itself
/// (`<repo_uid>/<iso-timestamp>/<uuid-prefix>` per
/// `docs/architecture/schema.txt:23`). An earlier version of this
/// helper used SQLite's `datetime('now')` which produces
/// `YYYY-MM-DD HH:MM:SS` (no T separator, no fractional seconds,
/// no Z). That was a real contract violation, not a cosmetic
/// difference:
///
///   1. `snapshot_uid` is part of the schema's portable identity
///      strategy. The format is locked in `schema.txt`.
///   2. `'T'` (ASCII 0x54) sorts AFTER `' '` (ASCII 0x20). A
///      database with mixed TS-written and Rust-written rows
///      would order incorrectly under `ORDER BY created_at DESC`,
///      and `get_latest_snapshot` could return the wrong row.
///   3. Any tooling that parses these timestamps with strict ISO
///      8601 expectations would reject the SQLite-format strings.
///
/// The corrected helper produces byte-equivalent output to TS for
/// every timestamp it generates. The pinning test
/// `current_iso_timestamp_matches_js_to_iso_string_format`
/// catches regressions.
///
/// **Why not add a `chrono` or `time` dep:** the SQLite
/// `strftime` approach is zero-dep, runs on the connection we
/// already have, and produces the exact byte format we need. A
/// time crate would add a dep, a parsing surface, and timezone-
/// handling complexity for no functional gain at this layer.
///
/// **Asymmetry with `record_migration`:** the migration runner's
/// `record_migration` helper (in `migrations/mod.rs`) uses
/// SQLite's `datetime('now')` to set `schema_migrations.applied_at`.
/// That is intentional and matches the TS adapter exactly: every
/// TS migration file inserts its row with `datetime('now')`, not
/// with `toISOString()`. The TS database therefore has TWO
/// timestamp formats coexisting: SQLite-format for
/// `schema_migrations.applied_at` and ISO 8601 for everything
/// snapshot-related. The Rust port mirrors this asymmetry exactly.
fn current_iso_timestamp(conn: &Connection) -> Result<String, StorageError> {
	let ts = conn.query_row(
		"SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
		[],
		|row| row.get::<_, String>(0),
	)?;
	Ok(ts)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{
		fresh_storage, make_edge, make_file, make_file_version, make_node, make_repo,
	};

	fn create_test_snapshot(storage: &StorageConnection) -> Snapshot {
		storage
			.create_snapshot(&CreateSnapshotInput {
				repo_uid: "r1".to_string(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: Some("abc123".to_string()),
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.unwrap()
	}

	#[test]
	fn create_snapshot_returns_dto_with_building_status() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		let snap = create_test_snapshot(&storage);

		assert_eq!(snap.repo_uid, "r1");
		assert_eq!(snap.status, SNAPSHOT_STATUS_BUILDING);
		assert_eq!(snap.kind, "full");
	}

	#[test]
	fn create_snapshot_generates_unique_uid_with_repo_prefix() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		let snap1 = create_test_snapshot(&storage);
		// Sleep is not needed; UUID prefix randomization makes
		// collision astronomically unlikely.
		let snap2 = create_test_snapshot(&storage);

		assert_ne!(snap1.snapshot_uid, snap2.snapshot_uid);
		// UID format: <repo_uid>/<timestamp>/<uuid_prefix>
		assert!(snap1.snapshot_uid.starts_with("r1/"));
		assert!(snap2.snapshot_uid.starts_with("r1/"));
	}

	#[test]
	fn get_snapshot_returns_none_for_nonexistent() {
		let storage = fresh_storage();
		let result = storage.get_snapshot("nope").unwrap();
		assert!(result.is_none());
	}

	#[test]
	fn update_snapshot_status_changes_status() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();
		let snap = create_test_snapshot(&storage);

		storage
			.update_snapshot_status(&UpdateSnapshotStatusInput {
				snapshot_uid: snap.snapshot_uid.clone(),
				status: SNAPSHOT_STATUS_READY.to_string(),
				completed_at: None,
			})
			.unwrap();

		let updated = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
		assert_eq!(updated.status, SNAPSHOT_STATUS_READY);
		assert!(updated.completed_at.is_some());
	}

	#[test]
	fn get_latest_snapshot_excludes_building_snapshots() {
		// PARITY-CRITICAL: getLatestSnapshot must filter by status=READY,
		// not just by ORDER BY created_at. A repo with only a BUILDING
		// snapshot returns None.
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();
		let _snap = create_test_snapshot(&storage);

		let latest = storage.get_latest_snapshot("r1").unwrap();
		assert!(
			latest.is_none(),
			"BUILDING snapshot must NOT be returned by get_latest_snapshot"
		);
	}

	#[test]
	fn get_latest_snapshot_returns_ready_snapshot_after_status_update() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();
		let snap = create_test_snapshot(&storage);

		storage
			.update_snapshot_status(&UpdateSnapshotStatusInput {
				snapshot_uid: snap.snapshot_uid.clone(),
				status: SNAPSHOT_STATUS_READY.to_string(),
				completed_at: None,
			})
			.unwrap();

		let latest = storage.get_latest_snapshot("r1").unwrap();
		assert!(latest.is_some());
		assert_eq!(latest.unwrap().snapshot_uid, snap.snapshot_uid);
	}

	// ── Timestamp format parity (regression pin) ──────────────

	#[test]
	fn current_iso_timestamp_matches_js_to_iso_string_format() {
		// Pins the R2-E parity correction.
		//
		// JavaScript `new Date().toISOString()` produces strings
		// of the exact form `YYYY-MM-DDTHH:MM:SS.sssZ`:
		//
		//   - 24 ASCII characters
		//   - '-' at positions 4 and 7
		//   - 'T' at position 10
		//   - ':' at positions 13 and 16
		//   - '.' at position 19
		//   - 'Z' at position 23
		//
		// An earlier version of `current_iso_timestamp` used
		// SQLite's `datetime('now')` which produces a different
		// format (`YYYY-MM-DD HH:MM:SS`, 19 chars, space
		// separator). That was a real contract violation because
		// snapshot_uid embeds this timestamp and ORDER BY
		// created_at would sort incorrectly across mixed
		// TS-written and Rust-written rows.
		//
		// This test pins the corrected format. If a future
		// maintainer reverts to `datetime('now')` or any other
		// format that does not match toISOString(), this test
		// fails immediately.
		let storage = fresh_storage();
		let ts = current_iso_timestamp(storage.connection())
			.expect("current_iso_timestamp must succeed");

		assert_eq!(
			ts.len(),
			24,
			"toISOString() format is exactly 24 chars, got {} chars: {:?}",
			ts.len(),
			ts
		);
		let bytes = ts.as_bytes();
		assert_eq!(bytes[4], b'-', "expected '-' at position 4 in {:?}", ts);
		assert_eq!(bytes[7], b'-', "expected '-' at position 7 in {:?}", ts);
		assert_eq!(
			bytes[10], b'T',
			"expected 'T' (NOT space) at position 10 in {:?}; if this fails the format reverted to SQLite datetime('now')",
			ts
		);
		assert_eq!(bytes[13], b':', "expected ':' at position 13 in {:?}", ts);
		assert_eq!(bytes[16], b':', "expected ':' at position 16 in {:?}", ts);
		assert_eq!(
			bytes[19], b'.',
			"expected '.' (fractional seconds separator) at position 19 in {:?}",
			ts
		);
		assert_eq!(
			bytes[23], b'Z',
			"expected 'Z' (Zulu/UTC suffix) at position 23 in {:?}",
			ts
		);
		// Year, month, day digit positions.
		for pos in [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22] {
			assert!(
				(bytes[pos] as char).is_ascii_digit(),
				"expected ASCII digit at position {} in {:?}",
				pos,
				ts
			);
		}
	}

	#[test]
	fn create_snapshot_uid_includes_iso_timestamp_with_t_separator() {
		// Cross-check: the snapshot UID format
		// `<repo_uid>/<iso-timestamp>/<uuid-prefix>` must contain
		// the ISO 8601 'T' separator in the timestamp segment.
		// Pins the schema.txt:23 contract that snapshot_uid uses
		// an ISO timestamp.
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();
		let snap = create_test_snapshot(&storage);

		// Format: r1/<iso-timestamp>/<uuid-prefix>
		// Splitting on '/' should yield exactly 3 parts.
		let parts: Vec<&str> = snap.snapshot_uid.split('/').collect();
		assert_eq!(parts.len(), 3, "snapshot_uid format is r1/<ts>/<uuid>");
		assert_eq!(parts[0], "r1");

		// The middle segment must contain a 'T' separator
		// (proves it's the ISO 8601 format, not SQLite format
		// which uses a space — and a space would not appear in
		// the segment because we split on '/').
		assert!(
			parts[1].contains('T'),
			"snapshot_uid timestamp segment must contain 'T' separator, got: {:?}",
			parts[1]
		);
		assert!(
			parts[1].ends_with('Z'),
			"snapshot_uid timestamp segment must end with 'Z' (UTC), got: {:?}",
			parts[1]
		);

		// The created_at column must also contain the same format.
		assert!(
			snap.created_at.contains('T'),
			"created_at must be ISO 8601 with T separator, got: {:?}",
			snap.created_at
		);
		assert!(
			snap.created_at.ends_with('Z'),
			"created_at must end with Z (UTC), got: {:?}",
			snap.created_at
		);
	}

	#[test]
	fn update_snapshot_counts_recomputes_from_actual_data() {
		let mut storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();
		let snap = create_test_snapshot(&storage);

		// Insert a file, file_version, two nodes, and one edge
		// so we have non-zero counts to verify against.
		let file = make_file("r1", "src/a.ts");
		storage.upsert_files(&[file.clone()]).unwrap();
		storage
			.upsert_file_versions(&[make_file_version(
				&snap.snapshot_uid,
				&file.file_uid,
			)])
			.unwrap();

		let node_a = make_node(
			"node-a",
			&snap.snapshot_uid,
			"r1",
			"r1:src/a.ts#fnA:SYMBOL:FUNCTION",
			&file.file_uid,
			"fnA",
		);
		let node_b = make_node(
			"node-b",
			&snap.snapshot_uid,
			"r1",
			"r1:src/a.ts#fnB:SYMBOL:FUNCTION",
			&file.file_uid,
			"fnB",
		);
		storage
			.insert_nodes(&[node_a.clone(), node_b.clone()])
			.unwrap();

		let edge = make_edge(
			"edge-1",
			&snap.snapshot_uid,
			"r1",
			&node_a.node_uid,
			&node_b.node_uid,
		);
		storage.insert_edges(&[edge]).unwrap();

		// Pre-recompute: counts should still be 0 (default).
		let pre = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
		assert_eq!(pre.files_total, 0);
		assert_eq!(pre.nodes_total, 0);
		assert_eq!(pre.edges_total, 0);

		// Recompute.
		storage.update_snapshot_counts(&snap.snapshot_uid).unwrap();

		// Post-recompute: counts match actual data.
		let post = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
		assert_eq!(post.files_total, 1);
		assert_eq!(post.nodes_total, 2);
		assert_eq!(post.edges_total, 1);
	}
}
