//! repo-graph-storage — Rust port of the SQLite storage substrate.
//!
//! This crate is the Rust-side mirror of the TypeScript storage
//! adapter at `src/adapters/storage/sqlite/` and the storage port
//! at `src/core/ports/storage.ts`. It is built incrementally as
//! the Rust-2 storage substrate parity slice progresses.
//!
//! Slice substep state (Rust-2):
//!   - R2-A workspace skeleton ............ done
//!   - R2-B schema DTOs and row mappers ... done
//!   - R2-C migration runner .............. done
//!   - R2-D connection lifecycle API ...... done
//!   - R2-E read/write parity (D4-Core) ... done
//!   - R2-F parity harness ................ done
//!   - R2-G script integration ............ done
//!   - R2-H final acceptance gate ......... done
//!
//! ── Locked slice contract ─────────────────────────────────────
//!
//! Migration boundary (R2-C scope): all 18 TypeScript migrations
//! (`001-initial.ts` through `016-fs-mutations.ts`) are hand-ported
//! to Rust as the schema boundary. The Rust crate produces a
//! fully-migrated database on `open()`, regardless of which
//! entities R2-E exposes CRUD methods for. Migration source-of-
//! truth is hand-ported per substep R2 lock decision D3c; drift
//! between TS and Rust migration logic is detected by the parity
//! harness (R2-F), not by shared source-of-truth gymnastics.
//! Migration 001 is the lone exception: its SQL is shared via
//! `include_str!` of `src/adapters/storage/sqlite/migrations/001-initial.sql`
//! per D12, providing a small structural anchor.
//!
//! Entity scope (R2-E behavioral boundary, D4-Core, 6 entities):
//!   - repos
//!   - snapshots
//!   - files
//!   - file_versions  ← parse-state and invalidation mechanism
//!   - nodes
//!   - edges
//!
//! file_versions behavioral surface (locked at R2-E):
//!   - upsert_file_versions     ↔ TS upsertFileVersions
//!   - get_stale_files          ↔ TS getStaleFiles
//!   - query_file_version_hashes ↔ TS queryFileVersionHashes
//!
//! Explicitly excluded from R2-E (deferred future work):
//!   - copy_forward_unchanged_files (TS copyForwardUnchangedFiles)
//!   - any behavior dragging in extraction_edges or file_signals
//!   - any post-008-adjacent artifact-copy behavior
//!
//! All other entities present in the schema (declarations,
//! modules, measurements, annotations, obligations, surfaces,
//! env_dependencies, fs_mutations, etc.) are explicitly out of
//! scope for Rust-2 and deferred to Rust-3+ slices.
//!
//! ── Tech stack (locked at R2 lock phase) ──────────────────────
//!
//!   - SQLite library: rusqlite with `bundled` feature (D1, D1a)
//!   - API model: synchronous (D2)
//!   - Migration source-of-truth: hand-ported in Rust (D3c)
//!   - Type mapping: hand-rolled FromRow-style impls (D5)
//!   - Error handling: thiserror enum (D6)
//!   - Crate structure: single crate `repo-graph-storage` (D7)
//!   - schema_migrations table: shared between runtimes (D10)
//!
//! ── Parity acceptance model (R2-F / R2-H) ─────────────────────
//!
//!   - Schema parity: canonical logical-schema dump via
//!     `PRAGMA table_info` per table plus a sorted list of index
//!     names, NOT a raw `sqlite_master.sql` text comparison (D8a
//!     as corrected during R2-F implementation). The PRAGMA-based
//!     representation is necessary because Rust's outdated-001
//!     plus migration-002 ALTER-TABLE creation path produces
//!     different `sqlite_master.sql` text than TS's inline-
//!     CREATE-TABLE path, even when the logical schema is
//!     identical. See `diagnostic.rs` and
//!     `storage-parity-fixtures/README.md` for the full format
//!     specification.
//!   - Per-table CRUD parity: execute identical operation
//!     sequences against both runtimes, compare resulting row
//!     sets structurally (D8b)
//!   - Fixture format: JSON operation scripts in a new top-level
//!     `storage-parity-fixtures/` directory at the repo root, with
//!     its own README documenting the fixture shape. Separate from
//!     the existing `parity-fixtures/` (which is detector-specific)
//!     to maintain the directory-as-surface convention (D9)
//!   - Dynamic value normalization: `schema_migrations.applied_at`
//!     is blanket-normalized to `"<TIMESTAMP>"`, and
//!     `createSnapshot`-generated `snapshotUid` / `createdAt`
//!     values are replaced with symbolic `<SNAP:binding>` and
//!     `<TS:binding:createdAt>` placeholders (corrected from the
//!     initial D-F3 v1 scope of migration-timestamp-only
//!     normalization during the R2-F lock override when read-return
//!     assertions and `createSnapshot` fixture coverage were
//!     added).
//!   - Diagnostic surface: the R2-F parity harness consumes the
//!     narrow `StorageConnection::diagnostic_dump()` method
//!     (returns `serde_json::Value`) rather than a raw rusqlite
//!     Connection handle. The `connection()` / `connection_mut()`
//!     accessors remain `pub(crate)` per the R2-D lock; the
//!     rusqlite Connection is never exposed outside the crate.
//!     See `diagnostic.rs` module docs for the architectural
//!     rationale.
//!
//! ── Explicit deferrals (NOT part of R2 contract) ──────────────
//!
//! These properties are NOT claimed, NOT tested, and NOT promised
//! by Rust-2. They are recorded here so they are visible as known
//! future work, not as silent gaps:
//!
//!   - Cross-runtime DB-file interoperability (was decision D11,
//!     removed from contract at the lock phase to avoid the
//!     overclaim pattern). The same .db file being usable by both
//!     runtimes alternately, without corruption, is desirable but
//!     unverified by Rust-2. WAL mode is portable across libsqlite3
//!     versions in principle, so this should work in practice, but
//!     Rust-2 makes no contract about it. A future slice may add
//!     a parity test that opens a TS-written database with Rust
//!     and vice versa.
//!
//!   - CRUD methods for entities outside D4-Core.
//!
//!   - TS-side migration source-of-truth refactor (the D3b
//!     alternative). Rust-2 hand-ports migration logic; the TS
//!     side is untouched.
//!
//!   - Replacing better-sqlite3 in the TS adapter.
//!
//!   - Postgres backend (would be a future
//!     repo-graph-storage-postgres crate or sqlx adoption).
//!
//!   - Indexer, daemon, classification, trust, extractors port to
//!     Rust. All deferred to Rust-3+ slices.
//!
//! ── Public API surface (filled in incrementally by substeps) ──
//!
//! R2-A: empty public API (workspace skeleton only).
//! R2-B: `types` module exposing the 6 D4-Core DTOs and the
//!       `SourceLocation` helper, all with hand-rolled
//!       `from_row(&Row)` constructors and serde camelCase
//!       parity-boundary derives.
//! R2-C: `error` module exposing the crate-level `StorageError`
//!       enum; `migrations` module exposing `run_migrations()`
//!       which hand-ports all 16 TS migrations to the embedded
//!       rusqlite Connection.
//! R2-D: `connection` module exposing `open(path)`,
//!       `StorageConnection`, lifecycle.
//! R2-E: CRUD methods on `StorageConnection` for the 6 entities.
//! R10:  `queries` module — `resolve_symbol` (SYMBOL-only, 3-step
//!       exact resolution) + `find_direct_callers` (one-hop CALLS).
//! R11:  `find_direct_callees` (symmetric reverse of callers).
//! R12:  `find_dead_nodes` (unreferenced nodes, 3 exclusion layers).
//! R13:  `find_cycles` (simple cycle enumeration via recursive CTE).
//! R14:  `compute_module_stats` (fan-in/out, instability, abstractness).
//! R18:  `node_exists` + `find_imports`.
//! R19:  `find_shortest_path` (BFS via recursive CTE, CALLS+IMPORTS).
//! R22:  `get_active_boundary_declarations` + `find_imports_between_paths`.
//! R24:  `get_active_requirement_declarations`.

pub mod connection;
pub mod crud;
pub(crate) mod diagnostic;
pub mod error;
pub mod migrations;
mod agent_impl; // AgentStorageRead impl for StorageConnection (Rust-42)
mod gate_impl; // GateStorageRead impl for StorageConnection (Rust-43A)
mod indexer_impl; // SnapshotLifecyclePort + FileCatalogPort impl (R5-C)
pub mod quality_policy_port; // Quality policy storage port (QP-Step-5)
mod quality_policy_impl; // QualityPolicyStoragePort impl for StorageConnection
pub mod queries; // Read-side graph queries (R10+)
mod trust_impl; // TrustStorageRead impl for StorageConnection (R4-E/F)
pub mod types;

// Convenience re-exports for the public connection lifecycle API.
pub use connection::StorageConnection;
