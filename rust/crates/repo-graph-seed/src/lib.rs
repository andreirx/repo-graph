//! # repo-graph-seed — pure support crate for local-embedding seed vectors
//!
//! EMBED-SEED-IMPL-1 (Semantic Seeding track). This crate is the **pure,
//! headless-testable core** of semantic seeding. It knows nothing about SQLite,
//! the daemon, HTTP, or a model runtime — it defines the two seams those live
//! behind ([`Embedder`], [`SeedCorpusRead`]) and implements the deterministic
//! logic the VISION bounds require:
//!
//! - [`document`] — the exact serialized corpus document (`search_document: …`)
//!   and query (`search_query: …`) the nomic model expects (spec §3.2).
//! - [`hash`] — the content pin ([`content_hash`], `SHA-256(bytes).hex[..16]`),
//!   byte-identical to the scanner's `hash_content`. The background pass re-runs
//!   it on the working tree to close the source/snapshot race (spec §3.5).
//! - [`store`] — the `.vec` sidecar envelope: a warm-cache-style validated
//!   header ([`store::SeedManifest`]) + a `bincode` body, atomic publication,
//!   and the pin hard-fail (I3).
//! - [`rank`] — cosine (dot of L2-normalized vectors) with the `(-score, path)`
//!   tie-break (spec §7.2), plus per-item freshness partitioning against the
//!   current corpus (I3 staleness).
//! - [`pass`] — the pure pipeline (`build_store`) that ties the ports together
//!   with a caller-supplied file reader + cancel token, so the whole embed pass
//!   is testable with fakes (no model, no daemon, no DB).
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

pub mod document;
pub mod hash;
pub mod pass;
pub mod ports;
pub mod rank;
pub mod store;

pub use hash::content_hash;
pub use ports::{EmbedError, Embedder, SeedCorpusEntry, SeedCorpusError, SeedCorpusRead};
pub use rank::{partition_fresh, rank, FreshnessPartition, RankedCandidate, NEAR_TIE_EPSILON};
pub use store::{
    SeedManifest, SeedStoreError, SeedStoreKey, SeedVectorBodyV1, SeedVectorEntryV1, MAGIC,
    MAX_BODY_BYTES, MAX_HEADER_BYTES, SCHEMA_VERSION,
};
