//! # repo-graph-seed — pure support crate for local-embedding seed vectors
//!
//! EMBED-SEED-IMPL-1 (Semantic Seeding track). This crate is the **pure,
//! headless-testable core** of semantic seeding. It knows nothing about SQLite,
//! the daemon, HTTP, or a model runtime — it defines the two seams those live
//! behind ([`Embedder`], [`SeedCorpusRead`]) and implements the deterministic
//! logic the VISION bounds require:
//!
//! - [`document`] — the exact serialized chunk document (`qualified_name` +
//!   `doc_comment` + capped span source) and the verbatim query the code model
//!   embeds (spec §2.1; SEED-CHUNK-1 drops nomic's role prefixes).
//! - [`hash`] — the content pin ([`content_hash`], `SHA-256(bytes).hex[..16]`),
//!   byte-identical to the scanner's `hash_content`. The background pass re-runs
//!   it on the working tree to close the source/snapshot race (spec §3.5).
//! - [`rank`] — cosine (dot of L2-normalized vectors) with the production-above-test
//!   partition (spec §5) + the `(-score, path)` tie-break (spec §7.2).
//! - [`pass`] — the pure pipeline (`build_store`) that ties the ports together
//!   with a caller-supplied file reader + cancel token, so the whole embed pass
//!   is testable with fakes (no model, no daemon, no DB). It returns the
//!   [`SeedVectorEntry`] rows the daemon writes to the per-snapshot `seed_vectors`
//!   table (SEED-CHUNK-1 retired the `.vec` sidecar for SQLite).
//!
//! ## Why a crate, not a module (D-ES-8, recorded)
//! *`repo-graph-seed` — pure corpus-build + `.vec` envelope + cosine ranking.
//! Concrete current users: the daemon-runtime background embed pass (writer) and
//! the daemon/agent query path (reader). Dispatch axis: operations-fixed → plain
//! functions. It defines the `Embedder` port (impl-growth axis = D-ES-4 runtime
//! choice) and the `SeedCorpusRead` port (earned dependency inversion so the pure
//! core never imports the SQLite adapter). Rejected simpler: inline in
//! daemon-runtime — would put pure domain logic in an adapter (architecture
//! Rule 3).*
//!
//! Every candidate this crate produces is a **Layer-3 hint** (I2): it never
//! enters a resolved fact, and it is discarded on any pin mismatch rather than
//! served stale (I3/I4).

pub mod classify;
pub mod document;
pub mod hash;
pub mod pass;
pub mod ports;
pub mod rank;

pub use hash::content_hash;
pub use ports::{
    EmbedError, Embedder, SeedCorpus, SeedCorpusEntry, SeedCorpusError, SeedCorpusRead,
    SeedVectorEntry, StoredSeedVectors,
};
pub use rank::{best_score, rank, RankedCandidate, NEAR_TIE_EPSILON};
