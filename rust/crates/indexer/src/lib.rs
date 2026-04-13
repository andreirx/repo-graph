//! repo-graph-indexer — Rust port of the indexer orchestration layer.
//!
//! This crate is the Rust-side mirror of the TypeScript indexer at
//! `src/adapters/indexer/repo-indexer.ts` and the port interfaces
//! at `src/core/ports/indexer.ts` and `src/core/ports/extractor.ts`.
//!
//! Slice substep state (Rust-5):
//!   - R5-A workspace skeleton ........... done
//!   - R5-B first storage subtraits ...... done
//!   - R5-C storage impl batch 1 ........ done
//!   - R5-D routing policy .............. done
//!   - R5-E edge resolution policy ...... done
//!   - R5-F storage subtraits batch 2 ... done
//!   - R5-G core indexing orchestration .. done
//!   - R5-H delta indexing .............. done
//!   - R5-I parity harness .............. done
//!   - R5-J script integration .......... done
//!   - R5-K final acceptance gate ....... done
//!
//! ── Architecture ─────────────────────────────────────────────
//!
//! This crate is POLICY code. It defines the orchestration
//! workflow and the ports it needs:
//!
//!   - `ExtractorPort`: the interface extractors implement.
//!     Defined here (policy owns the interface). Concrete
//!     extractor adapters (tree-sitter) are NOT in this crate.
//!
//!   - `IndexerStoragePort`: a composed facade over narrow
//!     sub-traits (`SnapshotLifecyclePort`, `FileCatalogPort`,
//!     `NodeStorePort`, `EdgeStorePort`, `UnresolvedEdgePort`,
//!     `FileSignalPort`, `DeltaCopyPort`). Each sub-trait covers
//!     one phase's storage needs. The facade is a blanket impl
//!     for any type implementing all sub-traits. The storage
//!     crate (adapter) implements the sub-traits on
//!     `StorageConnection`.
//!
//!   - Classification integration: uses `classify_unresolved_edge`
//!     and `derive_blast_radius` from `repo-graph-classification`
//!     for unresolved edge classification during resolution.
//!
//!   - Trust integration: NOT in the indexer. The indexer persists
//!     extraction diagnostics; the trust report is computed
//!     on-demand by the CLI via `repo-graph-trust`.
//!
//! This crate does NOT depend on `repo-graph-storage`. The
//! dependency direction is adapter → policy (storage implements
//! the indexer-owned traits).
//!
//! ── Locked contract divergence: sync ExtractorPort ───────────
//!
//! The TS `ExtractorPort` uses async (`Promise<ExtractionResult>`).
//! The Rust trait is synchronous. This is a deliberate divergence
//! locked at R5-A: tree-sitter parsing is CPU-bound, the Rust
//! extractor receives source text (no I/O), and `async fn` in
//! traits is not object-safe (conflicts with `&dyn ExtractorPort`
//! for heterogeneous collections). Future async needs (LSP
//! enrichment) belong on a separate enrichment port, not the base
//! extraction port. See `extractor_port.rs` module doc for the
//! full rationale.
//!
//! ── Scope (Rust-5 lock) ──────────────────────────────────────
//!
//! Core indexing orchestration only:
//!   - snapshot creation and finalization
//!   - file routing (extension → extractor)
//!   - extraction invocation through `ExtractorPort`
//!   - edge resolution (resolver index, resolve by type)
//!   - unresolved edge classification
//!   - module node/edge creation
//!   - delta/refresh indexing (invalidation + copy-forward)
//!
//! NOT in scope (deferred enrichment):
//!   - postpasses (module discovery, boundary extraction,
//!     framework detectors, seam detection)
//!   - annotations port
//!   - measurements/metrics persistence
//!   - concrete extractor implementations (Rust-6)
//!   - CLI entry point
//!   - daemon process work
//!   - filesystem I/O (caller provides file content)
//!
//! ── Storage sub-trait growth ──────────────────────────────────
//!
//! Sub-traits are added progressively per substep:
//!   - R5-B: SnapshotLifecyclePort, FileCatalogPort
//!   - R5-F: NodeStorePort, EdgeStorePort, UnresolvedEdgePort,
//!           FileSignalPort
//!   - R5-H: DeltaCopyPort
//!
//! The IndexerStoragePort facade grows to include each new
//! sub-trait as it is defined.

pub mod extractor_port;
pub mod invalidation;
pub mod orchestrator;
pub mod resolver;
pub mod routing;
pub mod storage_port;
pub mod types;

// ── Public re-exports ────────────────────────────────────────

pub use extractor_port::{ExtractorError, ExtractorPort};
pub use storage_port::{
	DeltaCopyPort, EdgeStorePort, FileCatalogPort, FileSignalPort,
	IndexerStoragePort, NodeStorePort, SnapshotLifecyclePort,
	UnresolvedEdgePort,
};
