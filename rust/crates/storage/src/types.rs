//! Storage substrate type definitions.
//!
//! This module is the Rust mirror of the TypeScript storage DTOs
//! defined under `src/core/model/` (`repo.ts`, `snapshot.ts`,
//! `file.ts`, `node.ts`, `edge.ts`, `types.ts`) and the read-side
//! row mappers in `src/adapters/storage/sqlite/sqlite-storage.ts`.
//!
//! Pure types only at this substep (R2-B). No I/O, no behavior
//! beyond row mapping. The migration runner (R2-C), connection
//! lifecycle (R2-D), and CRUD methods (R2-E) all consume the
//! types defined here.
//!
//! ── Parity contract (locked at R2-B sub-decisions) ────────────
//!
//! - **Field naming (D-B7a):** Rust struct fields are snake_case
//!   (idiomatic Rust). The `#[serde(rename_all = "camelCase")]`
//!   attribute on each DTO converts field names to camelCase at
//!   the JSON parity boundary, matching the TypeScript model
//!   interfaces (`Repo`, `Snapshot`, `TrackedFile`, `FileVersion`,
//!   `GraphNode`, `GraphEdge`) which all use camelCase.
//!
//! - **Row lookup:** `from_row` mappers look up columns by **SQL
//!   column name** (snake_case, e.g. `row.get("repo_uid")`), not
//!   by Rust field name. The Rust field name and the JSON-rendered
//!   field name happen to differ from the SQL column name; the
//!   row lookup uses the SQL name directly.
//!
//! - **SQLite booleans → Rust `bool` (D-B2):** SQLite has no
//!   native boolean. The convention is `INTEGER NOT NULL DEFAULT 0`
//!   with values 0 / 1. The TS adapter converts via strict
//!   `=== 1` in `mapFile` (sqlite-storage.ts:3284). The Rust
//!   mapper mirrors exactly using strict equality:
//!   `row.get::<_, i64>("col")? == 1`. This produces byte-equivalent
//!   output to TS for every possible integer column value, including
//!   the out-of-range values (e.g., 2, -1) the schema does not
//!   prevent because there is no `CHECK (col IN (0, 1))` constraint.
//!   An earlier version of this mapper used the more permissive
//!   `!= 0` form; that introduced a real parity drift and was
//!   corrected in R2-B. The pinning regression test is
//!   `tracked_file_boolean_uses_strict_eq_one_parity_with_ts`.
//!
//! - **JSON columns → `Option<String>` (D-B3):** TEXT columns
//!   ending in `_json` (`metadata_json`, `toolchain_json`) hold
//!   JSON-serialized application data. Storage substrate is
//!   opaque to JSON content. Matches the TS shape exactly (TS
//!   declares these as `string | null`).
//!
//! - **`edges.type` → `edge_type` (D-B4):** SQL column `type` is
//!   a Rust reserved keyword. The Rust struct field is named
//!   `edge_type`; `#[serde(rename = "type")]` ensures the JSON
//!   parity boundary still emits `"type"` matching the TS
//!   `GraphEdge.type` field. The row mapper uses `row.get("type")`
//!   with the SQL column name, not the Rust field name.
//!
//! - **`SourceLocation` nesting (D-B6α):** SQL has four flat
//!   columns (`line_start`, `col_start`, `line_end`, `col_end`)
//!   on both `nodes` and `edges`. The TS DTOs (`GraphNode`,
//!   `GraphEdge`) collapse those four columns into one nested
//!   `location: SourceLocation | null` field. The Rust DTOs
//!   mirror this nesting exactly. The collapse is a known
//!   structural transform applied at the row mapper boundary;
//!   the schema-column-order lock from R2-B is interpreted at
//!   field-position granularity (the `location` field occupies
//!   the position where `line_start` would have been). See
//!   `SourceLocation::from_partial_columns` for the asymmetric
//!   null-handling rule.
//!
//! - **`Snapshot.toolchain_json` (D-B5):** Added by migration 002,
//!   not present in `001-initial.sql`. R2-B's `Snapshot` DTO
//!   includes it because DTOs reflect the **final post-migration
//!   schema**, not migration 001 only.
//!
//! - **Counter columns defensive `?? 0` (Snapshot):** The TS
//!   `mapSnapshot` (sqlite-storage.ts:3258) defaults the three
//!   counter columns (`files_total`, `nodes_total`, `edges_total`)
//!   to 0 when the row column is null, even though the SQL
//!   schema declares them `INTEGER NOT NULL DEFAULT 0`. Rust
//!   mirrors via `unwrap_or(0)` on `Option<i64>` lookups for
//!   these specific columns.
//!
//! ── Read-side mapper coverage in TS (informational) ────────────
//!
//! The TS adapter has dedicated row mappers for:
//!   - `Repo` (mapRepo at line 3247)
//!   - `Snapshot` (mapSnapshot at line 3258)
//!   - `TrackedFile` (mapFile at line 3278)
//!   - `GraphNode` (mapNode at line 3290)
//!
//! It does NOT have dedicated row mappers for:
//!   - `FileVersion` — only used as write input in
//!     `upsertFileVersions` (line 371). Read methods on
//!     file_versions return projection shapes (file_uid lists,
//!     hash maps), not full `FileVersion[]`.
//!   - `GraphEdge` — only used as write input in `insertEdges`
//!     (line 566). No read method reconstructs a full GraphEdge.
//!
//! The Rust crate still implements `from_row` for `FileVersion`
//! and `GraphEdge` because the R2-B substep contract requires
//! row mappers for all six D4-Core entities. These two mappers
//! exist for symmetry and for any future use; they have no TS
//! reference implementation to mirror exactly. The `GraphEdge`
//! mapper borrows the asymmetric location null-handling rule from
//! `mapNode` by analogy (principle of least surprise).

use rusqlite::{Result as SqlResult, Row};
use serde::{Deserialize, Serialize};

// ── Source location helper ─────────────────────────────────────────

/// Position bundle for a node or edge in source code.
///
/// Mirrors the TypeScript `SourceLocation` interface from
/// `src/core/model/types.ts`:
///
/// ```ts
/// export interface SourceLocation {
///   lineStart: number;
///   colStart: number;
///   lineEnd: number;
///   colEnd: number;
/// }
/// ```
///
/// All four inner fields are non-nullable in the TS interface
/// definition. The "may be null" status applies to the whole
/// `SourceLocation | null` slot, not to the individual fields
/// inside. In Rust this manifests as `Option<SourceLocation>` on
/// the parent DTO (`GraphNode.location`, `GraphEdge.location`),
/// with `SourceLocation` itself having four required `i64` fields.
///
/// `i64` is the canonical Rust mapping for SQLite `INTEGER`
/// (64-bit signed). Line numbers and column numbers are positive
/// in practice, but the schema does not enforce non-negative
/// values; `i64` matches the SQL type literally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
	pub line_start: i64,
	pub col_start: i64,
	pub line_end: i64,
	pub col_end: i64,
}

impl SourceLocation {
	/// Construct a `SourceLocation` from the four nullable SQL
	/// position columns, applying the asymmetric null-handling
	/// rule from the TypeScript `mapNode` row mapper at
	/// `src/adapters/storage/sqlite/sqlite-storage.ts:3302-3310`:
	///
	/// ```ts
	/// location:
	///   row.line_start != null
	///     ? {
	///         lineStart: row.line_start as number,
	///         colStart: (row.col_start as number) ?? 0,
	///         lineEnd: (row.line_end as number) ?? (row.line_start as number),
	///         colEnd: (row.col_end as number) ?? 0,
	///       }
	///     : null,
	/// ```
	///
	/// Rules captured here:
	///   - `line_start` is the **discriminator**: if it is `None`,
	///     the entire location collapses to `None` regardless of
	///     the other three columns.
	///   - When `line_start` is `Some(ls)`:
	///       * `line_start = ls`
	///       * `col_start = col_start.unwrap_or(0)`
	///       * `line_end = line_end.unwrap_or(ls)` (defaults to
	///         `line_start`, NOT to 0)
	///       * `col_end = col_end.unwrap_or(0)`
	///
	/// The defaults are asymmetric and surprising. They cannot be
	/// reduced to "fall back to 0 for everything" or "all four
	/// must be present". The exact rule is what the TS adapter
	/// emits, and the parity boundary requires byte-equivalence
	/// at the JSON layer.
	///
	/// This helper is reused by both `GraphNode::from_row` and
	/// `GraphEdge::from_row`. The reuse is deliberate: there is
	/// no `mapEdge` in the TS adapter (edges are write-only), so
	/// the Rust `GraphEdge::from_row` borrows the rule from
	/// `mapNode` by analogy. Both Rust mappers therefore apply
	/// the same rule, which is the only documented rule in the
	/// codebase.
	pub fn from_partial_columns(
		line_start: Option<i64>,
		col_start: Option<i64>,
		line_end: Option<i64>,
		col_end: Option<i64>,
	) -> Option<SourceLocation> {
		line_start.map(|ls| SourceLocation {
			line_start: ls,
			col_start: col_start.unwrap_or(0),
			line_end: line_end.unwrap_or(ls),
			col_end: col_end.unwrap_or(0),
		})
	}
}

// ── Repos ──────────────────────────────────────────────────────────

/// Repository registration entity. Mirrors `Repo` from
/// `src/core/model/repo.ts`.
///
/// SQL table (from `001-initial.sql`):
/// ```sql
/// CREATE TABLE IF NOT EXISTS repos (
///   repo_uid        TEXT PRIMARY KEY,
///   name            TEXT NOT NULL,
///   root_path       TEXT NOT NULL,
///   default_branch  TEXT,
///   created_at      TEXT NOT NULL,
///   metadata_json   TEXT
/// );
/// ```
///
/// Field declaration order matches the SQL column order exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
	pub repo_uid: String,
	pub name: String,
	pub root_path: String,
	pub default_branch: Option<String>,
	pub created_at: String,
	pub metadata_json: Option<String>,
}

impl Repo {
	/// Construct a `Repo` from a rusqlite row.
	///
	/// Mirrors the TS `mapRepo` row mapper at
	/// `sqlite-storage.ts:3247`. Looks up columns by SQL column
	/// name (snake_case) regardless of Rust field name.
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			repo_uid: row.get("repo_uid")?,
			name: row.get("name")?,
			root_path: row.get("root_path")?,
			default_branch: row.get("default_branch")?,
			created_at: row.get("created_at")?,
			metadata_json: row.get("metadata_json")?,
		})
	}
}

// ── Snapshots ──────────────────────────────────────────────────────

/// Snapshot of a repo's extracted knowledge at a point in time.
/// Mirrors `Snapshot` from `src/core/model/snapshot.ts`.
///
/// SQL table (composed of `001-initial.sql` plus `toolchain_json`
/// added by migration 002):
/// ```sql
/// CREATE TABLE IF NOT EXISTS snapshots (
///   snapshot_uid        TEXT PRIMARY KEY,
///   repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
///   parent_snapshot_uid TEXT REFERENCES snapshots(snapshot_uid),
///   kind                TEXT NOT NULL,
///   basis_ref           TEXT,
///   basis_commit        TEXT,
///   dirty_hash          TEXT,
///   status              TEXT NOT NULL,
///   files_total         INTEGER NOT NULL DEFAULT 0,
///   nodes_total         INTEGER NOT NULL DEFAULT 0,
///   edges_total         INTEGER NOT NULL DEFAULT 0,
///   created_at          TEXT NOT NULL,
///   completed_at        TEXT,
///   label               TEXT,
///   toolchain_json      TEXT  -- added by migration 002
/// );
/// ```
///
/// `kind` and `status` are TEXT columns holding string-literal
/// enum values (`SnapshotKind`, `SnapshotStatus`). Per D-B7a they
/// are typed as `String` in Rust rather than as typed enums; the
/// JSON parity boundary emits the same string values either way.
///
/// The three counter columns are `i64` because the SQL type is
/// `INTEGER`. They mirror the TS `number` type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub parent_snapshot_uid: Option<String>,
	pub kind: String,
	pub basis_ref: Option<String>,
	pub basis_commit: Option<String>,
	pub dirty_hash: Option<String>,
	pub status: String,
	pub files_total: i64,
	pub nodes_total: i64,
	pub edges_total: i64,
	pub created_at: String,
	pub completed_at: Option<String>,
	pub label: Option<String>,
	pub toolchain_json: Option<String>,
}

impl Snapshot {
	/// Construct a `Snapshot` from a rusqlite row.
	///
	/// Mirrors the TS `mapSnapshot` row mapper at
	/// `sqlite-storage.ts:3258`. Note the defensive `unwrap_or(0)`
	/// on the three counter columns: even though the SQL schema
	/// declares them `INTEGER NOT NULL DEFAULT 0`, the TS mapper
	/// applies a defensive nullish-coalesce default. Rust mirrors
	/// exactly to preserve parity in any edge case where a NULL
	/// counter value reaches the mapper.
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			snapshot_uid: row.get("snapshot_uid")?,
			repo_uid: row.get("repo_uid")?,
			parent_snapshot_uid: row.get("parent_snapshot_uid")?,
			kind: row.get("kind")?,
			basis_ref: row.get("basis_ref")?,
			basis_commit: row.get("basis_commit")?,
			dirty_hash: row.get("dirty_hash")?,
			status: row.get("status")?,
			files_total: row.get::<_, Option<i64>>("files_total")?.unwrap_or(0),
			nodes_total: row.get::<_, Option<i64>>("nodes_total")?.unwrap_or(0),
			edges_total: row.get::<_, Option<i64>>("edges_total")?.unwrap_or(0),
			created_at: row.get("created_at")?,
			completed_at: row.get("completed_at")?,
			label: row.get("label")?,
			toolchain_json: row.get("toolchain_json")?,
		})
	}
}

// ── Tracked files ──────────────────────────────────────────────────

/// A tracked file in a repository. Mirrors `TrackedFile` from
/// `src/core/model/file.ts`.
///
/// SQL table (from `001-initial.sql`):
/// ```sql
/// CREATE TABLE IF NOT EXISTS files (
///   file_uid        TEXT PRIMARY KEY,
///   repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid),
///   path            TEXT NOT NULL,
///   language        TEXT,
///   is_test         INTEGER NOT NULL DEFAULT 0,
///   is_generated    INTEGER NOT NULL DEFAULT 0,
///   is_excluded     INTEGER NOT NULL DEFAULT 0
/// );
/// ```
///
/// The three boolean columns (`is_test`, `is_generated`,
/// `is_excluded`) are stored as SQLite INTEGER 0/1 and mapped to
/// Rust `bool` per D-B2. The TS adapter does the same conversion
/// via `=== 1` (sqlite-storage.ts:3284-3286).
///
/// The Rust type is named `TrackedFile` to match the TS interface
/// name exactly. The SQL table is `files`, not `tracked_files`;
/// the type-name disambiguation comes from the TS side, where
/// `File` would conflict with the DOM File API and several Node
/// type names. Matching `TrackedFile` preserves cross-runtime
/// mental-model alignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedFile {
	pub file_uid: String,
	pub repo_uid: String,
	pub path: String,
	pub language: Option<String>,
	pub is_test: bool,
	pub is_generated: bool,
	pub is_excluded: bool,
}

impl TrackedFile {
	/// Construct a `TrackedFile` from a rusqlite row.
	///
	/// Mirrors the TS `mapFile` row mapper at
	/// `sqlite-storage.ts:3278`. The boolean conversion uses
	/// **strict `i64 == 1`** to exactly match the TS adapter's
	/// `=== 1` semantics.
	///
	/// **Why strict, not `!= 0`:** the schema declares these
	/// columns as `INTEGER NOT NULL DEFAULT 0` but does NOT
	/// enforce a `CHECK (col IN (0, 1))` constraint, so
	/// out-of-range integer values (e.g., 2, -1) are structurally
	/// possible if a contributor inserts data via raw SQL or via
	/// a migration that doesn't validate. With strict `== 1`:
	///
	///   - `0` → `false` (the schema default)
	///   - `1` → `true`  (the schema "set" value)
	///   - any other value (`2`, `-1`, `999`, ...) → `false`,
	///     matching the TS `=== 1` strict equality
	///
	/// An earlier version of this mapper used `!= 0` (the SQL
	/// truthiness convention). That form is more permissive than
	/// the TS adapter and introduces a real parity drift on
	/// out-of-range values: TS would emit `false` for 2/-1, the
	/// `!= 0` Rust form would emit `true`. The corrected `== 1`
	/// form is byte-equivalent to TS for every possible integer
	/// column value, including the out-of-range ones the schema
	/// does not prevent. The test
	/// `tracked_file_boolean_uses_strict_eq_one_parity_with_ts`
	/// pins this correction against future regression.
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			file_uid: row.get("file_uid")?,
			repo_uid: row.get("repo_uid")?,
			path: row.get("path")?,
			language: row.get("language")?,
			is_test: row.get::<_, i64>("is_test")? == 1,
			is_generated: row.get::<_, i64>("is_generated")? == 1,
			is_excluded: row.get::<_, i64>("is_excluded")? == 1,
		})
	}
}

// ── File versions ──────────────────────────────────────────────────

/// Parse state of a file at a specific snapshot. Mirrors
/// `FileVersion` from `src/core/model/file.ts`.
///
/// This is the parse-state and invalidation mechanism: if
/// `content_hash` changes between snapshots, the file is stale
/// and must be re-extracted. The R2-E behavioral surface for
/// `file_versions` (locked at R2 lock phase) covers
/// `upsertFileVersions`, `getStaleFiles`, and
/// `queryFileVersionHashes`.
///
/// SQL table (from `001-initial.sql`):
/// ```sql
/// CREATE TABLE IF NOT EXISTS file_versions (
///   snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
///   file_uid        TEXT NOT NULL REFERENCES files(file_uid),
///   content_hash    TEXT NOT NULL,
///   ast_hash        TEXT,
///   extractor       TEXT,
///   parse_status    TEXT NOT NULL,
///   size_bytes      INTEGER,
///   line_count      INTEGER,
///   indexed_at      TEXT NOT NULL,
///   PRIMARY KEY (snapshot_uid, file_uid)
/// );
/// ```
///
/// **Composite primary key:** `(snapshot_uid, file_uid)`. There
/// is no surrogate UID. Per the R2-B lock, the composite primary
/// key is part of the type contract — both fields are required
/// and together identify a `FileVersion` uniquely. Rust does not
/// have a way to express "composite primary key" in the type
/// system itself; the constraint is enforced at the SQL layer
/// (R2-C migration runner) and at the CRUD layer (R2-E).
///
/// **No TS read mapper:** The TS adapter never reads
/// `FileVersion` back as a full DTO. Read methods on file_versions
/// return projection shapes (file_uid lists for `getStaleFiles`,
/// hash maps for `queryFileVersionHashes`), not full
/// `FileVersion[]`. This Rust `from_row` exists for symmetry
/// (per the R2-B contract requiring row mappers for all six
/// D4-Core entities) and is straightforward field-by-field
/// because no nesting or complex transforms apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersion {
	pub snapshot_uid: String,
	pub file_uid: String,
	pub content_hash: String,
	pub ast_hash: Option<String>,
	pub extractor: Option<String>,
	pub parse_status: String,
	pub size_bytes: Option<i64>,
	pub line_count: Option<i64>,
	pub indexed_at: String,
}

impl FileVersion {
	/// Construct a `FileVersion` from a rusqlite row.
	///
	/// No TS reference implementation. Field-by-field mapping
	/// using SQL column names. `parse_status` is `String` per
	/// D-B7a (string-literal enum passthrough).
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			snapshot_uid: row.get("snapshot_uid")?,
			file_uid: row.get("file_uid")?,
			content_hash: row.get("content_hash")?,
			ast_hash: row.get("ast_hash")?,
			extractor: row.get("extractor")?,
			parse_status: row.get("parse_status")?,
			size_bytes: row.get("size_bytes")?,
			line_count: row.get("line_count")?,
			indexed_at: row.get("indexed_at")?,
		})
	}
}

// ── Graph nodes ────────────────────────────────────────────────────

/// A graph node. Everything is a node: symbols, files, modules,
/// endpoints, events, tables, configs, tests, states. Mirrors
/// `GraphNode` from `src/core/model/node.ts`.
///
/// SQL table (from `001-initial.sql`):
/// ```sql
/// CREATE TABLE IF NOT EXISTS nodes (
///   node_uid        TEXT PRIMARY KEY,
///   snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
///   repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid),
///   stable_key      TEXT NOT NULL,
///   kind            TEXT NOT NULL,
///   subtype         TEXT,
///   name            TEXT NOT NULL,
///   qualified_name  TEXT,
///   file_uid        TEXT REFERENCES files(file_uid),
///   parent_node_uid TEXT REFERENCES nodes(node_uid),
///   line_start      INTEGER,
///   col_start       INTEGER,
///   line_end        INTEGER,
///   col_end         INTEGER,
///   signature       TEXT,
///   visibility      TEXT,
///   doc_comment     TEXT,
///   metadata_json   TEXT
/// );
/// ```
///
/// **Field count:** 18 SQL columns → 15 Rust fields. The four
/// position columns (`line_start`, `col_start`, `line_end`,
/// `col_end`) collapse into one nested `location:
/// Option<SourceLocation>` field per D-B6α. The `location` field
/// occupies the position where `line_start` would have been in
/// the schema column order.
///
/// **Field naming:** all snake_case in Rust, camelCase in JSON
/// via `#[serde(rename_all = "camelCase")]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
	pub node_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub stable_key: String,
	pub kind: String,
	pub subtype: Option<String>,
	pub name: String,
	pub qualified_name: Option<String>,
	pub file_uid: Option<String>,
	pub parent_node_uid: Option<String>,
	pub location: Option<SourceLocation>,
	pub signature: Option<String>,
	pub visibility: Option<String>,
	pub doc_comment: Option<String>,
	pub metadata_json: Option<String>,
}

impl GraphNode {
	/// Construct a `GraphNode` from a rusqlite row.
	///
	/// Mirrors the TS `mapNode` row mapper at
	/// `sqlite-storage.ts:3290`. The location field is built via
	/// `SourceLocation::from_partial_columns` which captures the
	/// asymmetric null-handling rule. See that function's
	/// docstring for the exact rule.
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			node_uid: row.get("node_uid")?,
			snapshot_uid: row.get("snapshot_uid")?,
			repo_uid: row.get("repo_uid")?,
			stable_key: row.get("stable_key")?,
			kind: row.get("kind")?,
			subtype: row.get("subtype")?,
			name: row.get("name")?,
			qualified_name: row.get("qualified_name")?,
			file_uid: row.get("file_uid")?,
			parent_node_uid: row.get("parent_node_uid")?,
			location: SourceLocation::from_partial_columns(
				row.get("line_start")?,
				row.get("col_start")?,
				row.get("line_end")?,
				row.get("col_end")?,
			),
			signature: row.get("signature")?,
			visibility: row.get("visibility")?,
			doc_comment: row.get("doc_comment")?,
			metadata_json: row.get("metadata_json")?,
		})
	}
}

// ── Graph edges ────────────────────────────────────────────────────

/// A graph edge. Connects any node to any node. Typed, with
/// resolution and extractor provenance. Mirrors `GraphEdge` from
/// `src/core/model/edge.ts`.
///
/// SQL table (from `001-initial.sql`):
/// ```sql
/// CREATE TABLE IF NOT EXISTS edges (
///   edge_uid        TEXT PRIMARY KEY,
///   snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
///   repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid),
///   source_node_uid TEXT NOT NULL REFERENCES nodes(node_uid),
///   target_node_uid TEXT NOT NULL REFERENCES nodes(node_uid),
///   type            TEXT NOT NULL,
///   resolution      TEXT NOT NULL,
///   extractor       TEXT NOT NULL,
///   line_start      INTEGER,
///   col_start       INTEGER,
///   line_end        INTEGER,
///   col_end         INTEGER,
///   metadata_json   TEXT
/// );
/// ```
///
/// **Field count:** 13 SQL columns → 10 Rust fields. Same
/// position-collapse pattern as `GraphNode`.
///
/// **Reserved keyword:** the SQL column `type` is a Rust reserved
/// keyword. The Rust struct field is `edge_type` per D-B4. The
/// `#[serde(rename = "type")]` attribute overrides the
/// struct-level `rename_all = "camelCase"` rule and emits the
/// JSON field as `"type"`, matching the TS `GraphEdge.type`
/// field. The row mapper uses `row.get("type")` with the SQL
/// column name.
///
/// **No TS read mapper:** see the module-level note. The Rust
/// `from_row` borrows the location null-handling rule from
/// `mapNode` by analogy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
	pub edge_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub source_node_uid: String,
	pub target_node_uid: String,
	#[serde(rename = "type")]
	pub edge_type: String,
	pub resolution: String,
	pub extractor: String,
	pub location: Option<SourceLocation>,
	pub metadata_json: Option<String>,
}

impl GraphEdge {
	/// Construct a `GraphEdge` from a rusqlite row.
	///
	/// **No TS reference implementation.** The TS adapter never
	/// reads `GraphEdge` back as a full DTO. This mapper exists
	/// for symmetry per the R2-B substep contract. The location
	/// null-handling rule is borrowed from `mapNode` by analogy
	/// (principle of least surprise: same rule applied
	/// consistently across both entities).
	///
	/// The row lookup for the renamed `edge_type` field uses the
	/// SQL column name `"type"`, not the Rust field name.
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			edge_uid: row.get("edge_uid")?,
			snapshot_uid: row.get("snapshot_uid")?,
			repo_uid: row.get("repo_uid")?,
			source_node_uid: row.get("source_node_uid")?,
			target_node_uid: row.get("target_node_uid")?,
			edge_type: row.get("type")?,
			resolution: row.get("resolution")?,
			extractor: row.get("extractor")?,
			location: SourceLocation::from_partial_columns(
				row.get("line_start")?,
				row.get("col_start")?,
				row.get("line_end")?,
				row.get("col_end")?,
			),
			metadata_json: row.get("metadata_json")?,
		})
	}
}

// ── Input shapes (R2-E) ────────────────────────────────────────────
//
// Per R2-E decision D-E3: mirror only the TS input shapes that
// already exist in `src/core/ports/storage.ts`. Three input
// types: `RepoRef`, `CreateSnapshotInput`, `UpdateSnapshotStatusInput`.
// No Rust-only command structs for repos, files, nodes, or edges
// — those entities take the DTO directly as the function argument
// or use plain primitive parameters.

/// Identify a repo by uid, name, or root path.
///
/// Mirrors the TypeScript discriminated union from
/// `src/core/ports/storage.ts:656`:
///
/// ```ts
/// export type RepoRef = { uid: string } | { name: string } | { rootPath: string };
/// ```
///
/// The Rust enum carries the same three discriminants. The
/// `get_repo` method dispatches on the variant to choose which
/// SQL column to query.
///
/// `#[serde(untagged)]` is NOT used here because RepoRef is an
/// internal API type, not a parity-boundary DTO. It is never
/// serialized to JSON. The serde derives are present only for
/// future-proofing if a configuration loader ever needs to
/// deserialize this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepoRef {
	/// Look up by `repos.repo_uid`.
	Uid(String),
	/// Look up by `repos.name`.
	Name(String),
	/// Look up by `repos.root_path`.
	RootPath(String),
}

/// Input for `create_snapshot`. Mirrors `CreateSnapshotInput`
/// from `src/core/ports/storage.ts:658`.
///
/// The TS interface has these fields all optional except
/// `repoUid` and `kind`. Rust uses `Option<String>` for the
/// optional ones. Note: the TS interface does NOT include
/// `dirtyHash`, `status`, `filesTotal`, `nodesTotal`,
/// `edgesTotal`, `createdAt`, or `completedAt` — those are
/// generated or set by the adapter at insert time.
///
/// Also note: `kind` is typed as `String` per D-B7a (string-
/// passthrough for enum-like columns), not as a typed Rust enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSnapshotInput {
	pub repo_uid: String,
	pub kind: String,
	pub basis_ref: Option<String>,
	pub basis_commit: Option<String>,
	pub parent_snapshot_uid: Option<String>,
	pub label: Option<String>,
	/// Snapshot-level toolchain provenance JSON.
	pub toolchain_json: Option<String>,
}

/// Input for `update_snapshot_status`. Mirrors
/// `UpdateSnapshotStatusInput` from
/// `src/core/ports/storage.ts:669`.
///
/// `completed_at` is optional; the TS adapter defaults to the
/// current ISO timestamp when not provided. The Rust adapter
/// mirrors that behavior in `crud::snapshots::update_snapshot_status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSnapshotStatusInput {
	pub snapshot_uid: String,
	pub status: String,
	pub completed_at: Option<String>,
}

// ── Module Candidates (RS-MG-1) ────────────────────────────────────

/// Discovered module candidate. Mirrors the `module_candidates` table
/// from migration 011-module-candidates.
///
/// SQL table:
/// ```sql
/// CREATE TABLE IF NOT EXISTS module_candidates (
///   module_candidate_uid  TEXT PRIMARY KEY,
///   snapshot_uid          TEXT NOT NULL,
///   repo_uid              TEXT NOT NULL,
///   module_key            TEXT NOT NULL,
///   module_kind           TEXT NOT NULL,
///   canonical_root_path   TEXT NOT NULL,
///   confidence            REAL NOT NULL,
///   display_name          TEXT,
///   metadata_json         TEXT,
///   UNIQUE (snapshot_uid, module_key)
/// );
/// ```
///
/// This is a read-only DTO for Rust. The TS indexer is the producer;
/// Rust `rmap` commands are consumers. The `canonical_root_path` field
/// is the stable identity anchor for boundary evaluation and module
/// edge derivation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleCandidate {
	pub module_candidate_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub module_key: String,
	pub module_kind: String,
	pub canonical_root_path: String,
	pub confidence: f64,
	pub display_name: Option<String>,
	pub metadata_json: Option<String>,
}

impl ModuleCandidate {
	/// Construct a `ModuleCandidate` from a rusqlite row.
	///
	/// Looks up columns by SQL column name (snake_case).
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			module_candidate_uid: row.get("module_candidate_uid")?,
			snapshot_uid: row.get("snapshot_uid")?,
			repo_uid: row.get("repo_uid")?,
			module_key: row.get("module_key")?,
			module_kind: row.get("module_kind")?,
			canonical_root_path: row.get("canonical_root_path")?,
			confidence: row.get("confidence")?,
			display_name: row.get("display_name")?,
			metadata_json: row.get("metadata_json")?,
		})
	}
}

// ── Measurement input ───────────────────────────────────────────────

/// Input for batch measurement insertion.
///
/// RS-MS-3c-prereq: Used by the compose layer to persist metrics
/// computed during extraction. Mirrors the TS `Measurement` write
/// shape from `src/core/ports/storage.ts`.
///
/// Unlike `MeasurementRow` (read-side, subset of columns),
/// `MeasurementInput` includes all columns needed for INSERT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasurementInput {
	pub measurement_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub target_stable_key: String,
	pub kind: String,
	pub value_json: String,
	pub source: String,
	pub created_at: String,
}

// ── Project Surfaces ───────────────────────────────────────────────

/// Discovered project surface. Mirrors the `project_surfaces` table
/// from migration 013-project-surfaces + 018-surface-identity.
///
/// Surfaces describe how modules are operationalized: CLI tools,
/// libraries, backend services, etc. One module can have multiple
/// surfaces (e.g., a library surface plus a CLI surface).
///
/// SQL table (post-migration 018):
/// ```sql
/// CREATE TABLE project_surfaces (
///   project_surface_uid    TEXT PRIMARY KEY,
///   snapshot_uid           TEXT NOT NULL,
///   repo_uid               TEXT NOT NULL,
///   module_candidate_uid   TEXT NOT NULL,
///   surface_kind           TEXT NOT NULL,
///   display_name           TEXT,
///   root_path              TEXT NOT NULL,
///   entrypoint_path        TEXT,
///   build_system           TEXT NOT NULL,
///   runtime_kind           TEXT NOT NULL,
///   confidence             REAL NOT NULL,
///   metadata_json          TEXT,
///   source_type            TEXT,           -- migration 018
///   source_specific_id     TEXT,           -- migration 018
///   stable_surface_key     TEXT,           -- migration 018
///   UNIQUE (snapshot_uid, stable_surface_key)
/// );
/// ```
///
/// This is a read-only DTO for Rust. The TS indexer is the producer;
/// Rust `rmap` commands are consumers. The `stable_surface_key` is
/// the snapshot-independent identity (nullable for legacy rows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSurface {
	pub project_surface_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub module_candidate_uid: String,
	pub surface_kind: String,
	pub display_name: Option<String>,
	pub root_path: String,
	pub entrypoint_path: Option<String>,
	pub build_system: String,
	pub runtime_kind: String,
	pub confidence: f64,
	pub metadata_json: Option<String>,
	// Identity columns (migration 018) — nullable for legacy rows
	pub source_type: Option<String>,
	pub source_specific_id: Option<String>,
	pub stable_surface_key: Option<String>,
}

impl ProjectSurface {
	/// Construct a `ProjectSurface` from a rusqlite row.
	///
	/// Looks up columns by SQL column name (snake_case).
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			project_surface_uid: row.get("project_surface_uid")?,
			snapshot_uid: row.get("snapshot_uid")?,
			repo_uid: row.get("repo_uid")?,
			module_candidate_uid: row.get("module_candidate_uid")?,
			surface_kind: row.get("surface_kind")?,
			display_name: row.get("display_name")?,
			root_path: row.get("root_path")?,
			entrypoint_path: row.get("entrypoint_path")?,
			build_system: row.get("build_system")?,
			runtime_kind: row.get("runtime_kind")?,
			confidence: row.get("confidence")?,
			metadata_json: row.get("metadata_json")?,
			source_type: row.get("source_type")?,
			source_specific_id: row.get("source_specific_id")?,
			stable_surface_key: row.get("stable_surface_key")?,
		})
	}
}

/// Evidence item for a project surface. Mirrors the
/// `project_surface_evidence` table from migration 013.
///
/// SQL table:
/// ```sql
/// CREATE TABLE project_surface_evidence (
///   project_surface_evidence_uid  TEXT PRIMARY KEY,
///   project_surface_uid           TEXT NOT NULL,
///   snapshot_uid                  TEXT NOT NULL,
///   repo_uid                      TEXT NOT NULL,
///   source_type                   TEXT NOT NULL,
///   source_path                   TEXT NOT NULL,
///   evidence_kind                 TEXT NOT NULL,
///   confidence                    REAL NOT NULL,
///   payload_json                  TEXT
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSurfaceEvidence {
	pub project_surface_evidence_uid: String,
	pub project_surface_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub source_type: String,
	pub source_path: String,
	pub evidence_kind: String,
	pub confidence: f64,
	pub payload_json: Option<String>,
}

impl ProjectSurfaceEvidence {
	/// Construct a `ProjectSurfaceEvidence` from a rusqlite row.
	pub fn from_row(row: &Row<'_>) -> SqlResult<Self> {
		Ok(Self {
			project_surface_evidence_uid: row.get("project_surface_evidence_uid")?,
			project_surface_uid: row.get("project_surface_uid")?,
			snapshot_uid: row.get("snapshot_uid")?,
			repo_uid: row.get("repo_uid")?,
			source_type: row.get("source_type")?,
			source_path: row.get("source_path")?,
			evidence_kind: row.get("evidence_kind")?,
			confidence: row.get("confidence")?,
			payload_json: row.get("payload_json")?,
		})
	}
}

// ── Tests ──────────────────────────────────────────────────────────
//
// R2-B unit tests cover the pure-logic helper
// `SourceLocation::from_partial_columns`. The asymmetric
// null-handling rule it captures is the only piece of logic in
// this module that can be tested in isolation without a real
// rusqlite database.
//
// Full `from_row` tests for `Repo`, `Snapshot`, `TrackedFile`,
// `FileVersion`, `GraphNode`, `GraphEdge` require a real database
// (in-memory rusqlite::Connection plus the schema migrations).
// Those tests land in R2-D (after the connection lifecycle and
// migration runner exist) or R2-E (alongside the CRUD methods
// that exercise the same row shapes). This boundary is
// intentional: R2-B is "types and pure mapping logic"; the
// integration test surface that needs a real database belongs
// to a later substep.

#[cfg(test)]
mod tests {
	use super::*;

	// ── SourceLocation::from_partial_columns ──────────────────

	#[test]
	fn location_is_none_when_line_start_is_none() {
		assert_eq!(
			SourceLocation::from_partial_columns(None, None, None, None),
			None
		);
	}

	#[test]
	fn location_is_none_when_line_start_is_none_even_if_others_are_set() {
		// The asymmetric rule: line_start is the discriminator.
		// Other columns being non-null does not produce a location.
		assert_eq!(
			SourceLocation::from_partial_columns(None, Some(5), Some(10), Some(15)),
			None
		);
	}

	#[test]
	fn location_is_some_when_line_start_is_set_with_all_others_null() {
		// All defaults fire: col_start=0, line_end=line_start, col_end=0.
		let loc = SourceLocation::from_partial_columns(Some(7), None, None, None);
		assert_eq!(
			loc,
			Some(SourceLocation {
				line_start: 7,
				col_start: 0,
				line_end: 7, // defaults to line_start, NOT to 0
				col_end: 0,
			})
		);
	}

	#[test]
	fn location_uses_explicit_col_start_when_set() {
		let loc = SourceLocation::from_partial_columns(Some(7), Some(3), None, None);
		assert_eq!(
			loc,
			Some(SourceLocation {
				line_start: 7,
				col_start: 3,
				line_end: 7,
				col_end: 0,
			})
		);
	}

	#[test]
	fn location_uses_explicit_line_end_when_set() {
		let loc = SourceLocation::from_partial_columns(Some(7), None, Some(12), None);
		assert_eq!(
			loc,
			Some(SourceLocation {
				line_start: 7,
				col_start: 0,
				line_end: 12, // explicit value, not the default
				col_end: 0,
			})
		);
	}

	#[test]
	fn location_uses_explicit_col_end_when_set() {
		let loc = SourceLocation::from_partial_columns(Some(7), None, None, Some(20));
		assert_eq!(
			loc,
			Some(SourceLocation {
				line_start: 7,
				col_start: 0,
				line_end: 7,
				col_end: 20,
			})
		);
	}

	#[test]
	fn location_uses_all_explicit_values_when_all_set() {
		let loc =
			SourceLocation::from_partial_columns(Some(7), Some(3), Some(12), Some(20));
		assert_eq!(
			loc,
			Some(SourceLocation {
				line_start: 7,
				col_start: 3,
				line_end: 12,
				col_end: 20,
			})
		);
	}

	#[test]
	fn location_default_line_end_to_line_start_is_critical() {
		// This test pins the most surprising default: line_end
		// falls back to line_start, NOT to 0. If a future change
		// inverts this, parity with the TS mapNode rule breaks.
		let loc = SourceLocation::from_partial_columns(Some(42), None, None, None);
		assert_eq!(loc.unwrap().line_end, 42);
	}

	#[test]
	fn location_zero_line_start_is_still_some() {
		// line_start = 0 is a valid non-null value (Some(0)).
		// The discriminator is presence, not truthiness.
		let loc = SourceLocation::from_partial_columns(Some(0), None, None, None);
		assert_eq!(
			loc,
			Some(SourceLocation {
				line_start: 0,
				col_start: 0,
				line_end: 0, // defaults to line_start which is 0
				col_end: 0,
			})
		);
	}

	// ── Serde rename_all and field rename ─────────────────────

	#[test]
	fn repo_serializes_field_names_as_camel_case() {
		let repo = Repo {
			repo_uid: "r1".into(),
			name: "test".into(),
			root_path: "/abs".into(),
			default_branch: Some("main".into()),
			created_at: "2025-01-01T00:00:00Z".into(),
			metadata_json: None,
		};
		let json = serde_json::to_string(&repo).unwrap();
		// camelCase field names per D-B7a
		assert!(json.contains("\"repoUid\":\"r1\""));
		assert!(json.contains("\"rootPath\":\"/abs\""));
		assert!(json.contains("\"defaultBranch\":\"main\""));
		assert!(json.contains("\"createdAt\":"));
		assert!(json.contains("\"metadataJson\":null"));
		// snake_case must NOT appear in the serialized JSON
		assert!(!json.contains("\"repo_uid\""));
		assert!(!json.contains("\"root_path\""));
		assert!(!json.contains("\"default_branch\""));
		assert!(!json.contains("\"metadata_json\""));
	}

	#[test]
	fn graph_edge_type_field_serializes_as_type() {
		// D-B4: the Rust field is `edge_type`, JSON field is `type`
		// to match the TS `GraphEdge.type`.
		let edge = GraphEdge {
			edge_uid: "e1".into(),
			snapshot_uid: "s1".into(),
			repo_uid: "r1".into(),
			source_node_uid: "n1".into(),
			target_node_uid: "n2".into(),
			edge_type: "CALLS".into(),
			resolution: "static".into(),
			extractor: "ts-core:0.1.0".into(),
			location: None,
			metadata_json: None,
		};
		let json = serde_json::to_string(&edge).unwrap();
		assert!(json.contains("\"type\":\"CALLS\""));
		// Rust field name must NOT leak to JSON
		assert!(!json.contains("\"edgeType\""));
		assert!(!json.contains("\"edge_type\""));
	}

	#[test]
	fn graph_node_location_serializes_as_nested_object_when_some() {
		let node = GraphNode {
			node_uid: "n1".into(),
			snapshot_uid: "s1".into(),
			repo_uid: "r1".into(),
			stable_key: "r1:src/foo.ts#bar:SYMBOL".into(),
			kind: "SYMBOL".into(),
			subtype: Some("FUNCTION".into()),
			name: "bar".into(),
			qualified_name: None,
			file_uid: None,
			parent_node_uid: None,
			location: Some(SourceLocation {
				line_start: 1,
				col_start: 2,
				line_end: 3,
				col_end: 4,
			}),
			signature: None,
			visibility: None,
			doc_comment: None,
			metadata_json: None,
		};
		let json = serde_json::to_string(&node).unwrap();
		// location is a nested camelCase object
		assert!(
			json.contains("\"location\":{\"lineStart\":1,\"colStart\":2,\"lineEnd\":3,\"colEnd\":4}")
		);
	}

	// ── TrackedFile boolean parity (regression pin) ───────────

	#[test]
	fn tracked_file_boolean_uses_strict_eq_one_parity_with_ts() {
		// Pins the D-B2 strict-parity correction.
		//
		// The TS `mapFile` row mapper at sqlite-storage.ts:3284
		// uses `(row.is_test as number) === 1`. The Rust mapper
		// must use exactly the same semantics — strict `== 1`,
		// not the more permissive `!= 0` SQL-truthiness form.
		//
		// The schema declares these columns as INTEGER NOT NULL
		// DEFAULT 0 but does NOT enforce CHECK (col IN (0, 1)).
		// Out-of-range values (2, -1, etc.) are structurally
		// possible. Both runtimes must emit `false` for any
		// value other than literal 1, otherwise parity drifts.
		//
		// This test uses an inline rusqlite::Connection plus a
		// minimal CREATE TABLE matching the `files` table schema
		// from 001-initial.sql. It does NOT depend on R2-C
		// (migration runner) or R2-D (connection lifecycle) —
		// the test is self-contained and exercises only the
		// `TrackedFile::from_row` mapper directly.
		use rusqlite::Connection;

		let conn = Connection::open_in_memory().expect("open in-memory db");
		conn.execute_batch(
			"CREATE TABLE files (
				file_uid     TEXT PRIMARY KEY,
				repo_uid     TEXT NOT NULL,
				path         TEXT NOT NULL,
				language     TEXT,
				is_test      INTEGER NOT NULL DEFAULT 0,
				is_generated INTEGER NOT NULL DEFAULT 0,
				is_excluded  INTEGER NOT NULL DEFAULT 0
			)",
		)
		.expect("create files table");

		// Four rows covering the boolean integer cases:
		//   - "f_zero": all 0  → all false
		//   - "f_one":  all 1  → all true
		//   - "f_two":  all 2  → all FALSE (matches TS === 1)
		//   - "f_neg":  all -1 → all FALSE (matches TS === 1)
		conn.execute(
			"INSERT INTO files (file_uid, repo_uid, path, language, is_test, is_generated, is_excluded) VALUES
				('f_zero', 'r1', 'a.ts', 'typescript', 0, 0, 0),
				('f_one',  'r1', 'b.ts', 'typescript', 1, 1, 1),
				('f_two',  'r1', 'c.ts', 'typescript', 2, 2, 2),
				('f_neg',  'r1', 'd.ts', 'typescript', -1, -1, -1)",
			[],
		)
		.expect("insert test rows");

		let mut stmt = conn
			.prepare("SELECT * FROM files")
			.expect("prepare select");
		let rows: Vec<TrackedFile> = stmt
			.query_map([], |row| TrackedFile::from_row(row))
			.expect("query_map")
			.collect::<Result<Vec<_>, _>>()
			.expect("collect rows");

		// Look up by file_uid (order-independent).
		let by_uid = |uid: &str| -> &TrackedFile {
			rows.iter()
				.find(|f| f.file_uid == uid)
				.unwrap_or_else(|| panic!("no row with file_uid={uid}"))
		};

		// f_zero: 0 → false. Schema default case.
		let f_zero = by_uid("f_zero");
		assert!(!f_zero.is_test, "is_test=0 must be false");
		assert!(!f_zero.is_generated, "is_generated=0 must be false");
		assert!(!f_zero.is_excluded, "is_excluded=0 must be false");

		// f_one: 1 → true. Schema "set" case.
		let f_one = by_uid("f_one");
		assert!(f_one.is_test, "is_test=1 must be true");
		assert!(f_one.is_generated, "is_generated=1 must be true");
		assert!(f_one.is_excluded, "is_excluded=1 must be true");

		// f_two: 2 → FALSE (the parity-critical case).
		// Permissive `!= 0` would have produced true here, drifting
		// from TS `=== 1`. Strict `== 1` produces false.
		let f_two = by_uid("f_two");
		assert!(
			!f_two.is_test,
			"is_test=2 must be false (TS === 1 parity); permissive != 0 would drift"
		);
		assert!(!f_two.is_generated, "is_generated=2 must be false (TS === 1 parity)");
		assert!(!f_two.is_excluded, "is_excluded=2 must be false (TS === 1 parity)");

		// f_neg: -1 → FALSE (the other parity-critical case).
		let f_neg = by_uid("f_neg");
		assert!(
			!f_neg.is_test,
			"is_test=-1 must be false (TS === 1 parity); permissive != 0 would drift"
		);
		assert!(!f_neg.is_generated, "is_generated=-1 must be false (TS === 1 parity)");
		assert!(!f_neg.is_excluded, "is_excluded=-1 must be false (TS === 1 parity)");
	}

	#[test]
	fn graph_node_location_serializes_as_null_when_none() {
		let node = GraphNode {
			node_uid: "n1".into(),
			snapshot_uid: "s1".into(),
			repo_uid: "r1".into(),
			stable_key: "k".into(),
			kind: "SYMBOL".into(),
			subtype: None,
			name: "n".into(),
			qualified_name: None,
			file_uid: None,
			parent_node_uid: None,
			location: None,
			signature: None,
			visibility: None,
			doc_comment: None,
			metadata_json: None,
		};
		let json = serde_json::to_string(&node).unwrap();
		assert!(json.contains("\"location\":null"));
	}
}
