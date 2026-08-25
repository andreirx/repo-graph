//! The two seams the pure seed logic depends on — defined here (policy side) so
//! the outer adapters implement them (adapter → policy, inward dependency).

use thiserror::Error;

/// The model-runtime seam (spec §10). `texts` are already role-prefixed
/// (`search_document: …` / `search_query: …`) by the caller (spec §3.2); the
/// impl issues them to the model and returns raw float vectors — **only
/// `Vec<Vec<f32>>` crosses this boundary, never an HTTP/framework type**.
///
/// Dispatch axis (concrete): the D-ES-4 distribution choice. Implementations
/// grow — endpoint (a) ships in IMPL-1; embedded-ONNX (b) is the named,
/// ratification-pending second impl — so this is interface + polymorphism.
pub trait Embedder {
    /// The operator-asserted model id that becomes the store pin (spec §6.1/§7.1).
    fn model_id(&self) -> &str;
    /// The embedding dimension that becomes the store pin (`dim`).
    fn dim(&self) -> usize;
    /// Embed a batch of already-role-prefixed documents. Returns one vector per
    /// input, in input order (the impl is responsible for correlating the
    /// endpoint's response by `index`, not array position — a2 contract, D-ES-9).
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
}

/// Every failure the model seam can report. Each variant maps to exactly one
/// honest degraded state (spec §8.3) — the candidate generator *declines*, it
/// never guesses.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// Endpoint not reachable (server down / no local model) → "model unavailable".
    #[error("embedding endpoint unreachable ({endpoint}): {detail}")]
    Unreachable { endpoint: String, detail: String },
    /// Configured endpoint is not a loopback IP literal → refused before connect
    /// (I4 structural enforcement, D-ES-4/§6.1). Never egress.
    #[error("embedding endpoint is not loopback and was refused: {endpoint}")]
    NonLoopbackRejected { endpoint: String },
    /// Response body was not the expected bounded shape (non-200, chunked, TLS,
    /// bad JSON, bad index permutation, non-finite/zero-norm vector — the a2
    /// accepted-response contract, D-ES-9).
    #[error("embedding response malformed: {detail}")]
    Malformed { detail: String },
    /// A returned vector's length ≠ the pinned `dim` → whole-store "pins mismatch".
    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    DimMismatch { expected: usize, got: usize },
    /// Model identity ≠ the pinned `model_id`. Either the endpoint's echoed
    /// response `model` field (when present) ≠ pin (wire-time check), or the
    /// query-time configured model ≠ the store pin (spec §7.1). A *missing* echo
    /// is NOT this error (the pin stays operator-asserted, §6.1/§9).
    #[error("embedding model mismatch: expected {expected}, got {got}")]
    ModelMismatch { expected: String, got: String },
}

/// One corpus file, as a raw owned boundary DTO — three `String`s, **no**
/// `rusqlite::Row`, no storage type, no framework object (architecture boundary
/// DTO rule). Filled by the storage adapter's [`SeedCorpusRead`] impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedCorpusEntry {
    pub file_uid: String,
    pub path: String,
    /// The READY-snapshot `file_versions.content_hash` pin for this file.
    pub content_hash: String,
}

/// Failure reading the corpus catalog (a wrapped storage error string — the pure
/// core never sees the concrete storage error type).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SeedCorpusError {
    #[error("seed corpus read failed: {0}")]
    Read(String),
}

/// The read seam that hands the pure seed logic its corpus catalog without the
/// logic importing the SQLite adapter (spec §10 ledger; the earned dependency
/// inversion — identical to the ratified `AgentStorageRead` pattern).
///
/// `repo-graph-storage` adds a dependency on this crate and implements this port
/// on `StorageConnection`: the READY-snapshot
/// `SELECT file_uid, path FROM files WHERE repo_uid=? AND is_test=0 AND
/// is_generated=0 AND is_excluded=0` joined to `file_versions.content_hash`
/// (spec §3.1/§3.3), converting rows to [`SeedCorpusEntry`] — direction
/// adapter → policy (outer → inner).
pub trait SeedCorpusRead {
    /// Enumerate the current READY-snapshot corpus for `repo_uid`, ordered by
    /// `path` ascending (deterministic; the corpus-cap truncation and the tie
    /// break both rely on a stable order). Returns an empty vec when the repo is
    /// not indexed / has no READY snapshot — that is "no corpus", never an error.
    fn seed_corpus(&self, repo_uid: &str) -> Result<Vec<SeedCorpusEntry>, SeedCorpusError>;

    /// Resolve the OWNING MODULE display path (`module_candidates.canonical_root_path`)
    /// for each of `file_uids` in `snapshot_uid`, from the genuine ownership mapping
    /// (`module_file_ownership`, operator ruling 2026-08-25) — NOT a directory guess.
    /// A file with several ownership rows resolves to its most-specific module
    /// (longest `canonical_root_path`), the same longest-prefix winner used
    /// everywhere else a file is displayed under a module.
    ///
    /// The returned map contains ONLY the file_uids that have an ownership row:
    /// **presence = a genuine owning module; absence = no ownership recorded** (the
    /// caller renders that as an explicit unavailable-with-reason, never a fallback
    /// value). A read failure is `Err` (unknown-with-reason) — never an empty map.
    fn module_owners(
        &self,
        snapshot_uid: &str,
        file_uids: &[String],
    ) -> Result<std::collections::HashMap<String, String>, SeedCorpusError>;
}
