//! The seams the pure seed logic depends on — defined here (policy side) so the
//! outer adapters implement them (adapter → policy, inward dependency).
//!
//! SEED-CHUNK-1: the corpus is now per-SYMBOL **chunks** (not files), and vectors
//! live per-snapshot in SQLite (not a `.vec` sidecar). The DTOs that cross the
//! storage boundary stay raw owned structs — never a `rusqlite::Row`.

use thiserror::Error;

/// The model-runtime seam (spec §10). Implementations issue the already-assembled
/// chunk/query text to the model and return raw float vectors — **only
/// `Vec<Vec<f32>>` crosses this boundary, never an HTTP/framework type**.
///
/// Dispatch axis (concrete): the D-ES-4 distribution choice. Implementations
/// grow — endpoint (a) shipped in IMPL-1; the in-process static engine (b,
/// `LocalEmbedder`) is the SEED-CHUNK-1 ratified impl — so this is interface +
/// polymorphism.
pub trait Embedder {
    /// The model id that becomes the store's `model_id` stamp (spec §3).
    fn model_id(&self) -> &str;
    /// The embedding dimension that becomes the store's `dim` stamp.
    fn dim(&self) -> usize;
    /// Embed a batch of already-assembled documents. Returns one vector per input,
    /// in input order.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
}

/// Every failure the model seam can report. Each variant maps to exactly one
/// honest degraded state (spec §8.3) — the candidate generator *declines*, it
/// never guesses.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// The model runtime was unavailable (endpoint down / model not cached and not
    /// fetchable) → "model unavailable".
    #[error("embedding model unavailable: {detail}")]
    Unavailable { detail: String },
    /// Response was not the expected bounded shape (bad cardinality, non-finite).
    #[error("embedding response malformed: {detail}")]
    Malformed { detail: String },
    /// A returned vector's length ≠ the pinned `dim` → whole-store "pins mismatch".
    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    DimMismatch { expected: usize, got: usize },
}

/// One corpus **chunk** — a SYMBOL node's identity + the raw material for its
/// document (spec §2.1). A raw owned boundary DTO (no storage type). The span
/// SOURCE TEXT is NOT here: the background pass reads the working-tree file and
/// slices `line_start..line_end` itself (closing the source/snapshot race), so
/// only the line bounds cross this boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedCorpusEntry {
    /// The SYMBOL node's snapshot-scoped uid (the seed_vectors PK part).
    pub node_uid: String,
    /// The node's cross-snapshot identity (the copy-forward + `explain <key>` key).
    pub stable_key: String,
    /// The owning file's uid (for the module-ownership lookup).
    pub file_uid: String,
    /// Repo-relative file path (the `path:line` anchor + working-tree read key).
    pub path: String,
    /// The symbol's qualified name (document header + anchor); `None` when unstored.
    pub qualified_name: Option<String>,
    /// The node's `subtype` (`FUNCTION`/`METHOD`/`CONSTANT`/…, the stored uppercase
    /// value); `None` when unstored. SEED-CHUNK-2 uses it to gate the decl/impl label to
    /// CALLABLES only — a const/variable is never a "declaration without a body".
    pub subtype: Option<String>,
    /// The symbol's doc comment (document body prefix); `None` when unstored.
    pub doc_comment: Option<String>,
    /// 1-indexed span start line; the `line` anchor. `None` when the node had no span.
    pub line_start: Option<i64>,
    /// 1-indexed span end line (inclusive). `None` when the node had no span.
    pub line_end: Option<i64>,
    /// The owning file's `is_test` classification at this snapshot (the partition input).
    pub is_test: bool,
    /// The owning file's `file_versions.content_hash` pin (the source/snapshot-race
    /// admission check + ONE part of the copy-forward key — the reuse identity is
    /// `(stable_key, content_hash, document_hash)`, review-3; see [`SeedVectorEntry::document_hash`]).
    pub content_hash: String,
    /// CPP-DECLARATORS-1 (§2.3): the 3-state classification of the node's stored
    /// `metadata_json.forward_decl`. A genuine [`ForwardDecl`](SeedForwardDecl::ForwardDecl)
    /// lets the seed classifier label a TYPE chunk `(decl)` and demote it under its definition
    /// (the same mechanism SEED-CHUNK-2 applies to span-structural callables). A CORRUPT carrier
    /// ([`Unreadable`](SeedForwardDecl::Unreadable)) NEVER forces `(decl)` — it is excluded from
    /// the decl tier and counted (`BuildReport.forward_decl_unreadable`), so corrupt metadata is
    /// never presented as a positive declaration fact (STANDING HONESTY RULE 1). `Definition` for
    /// every non-C++ node (their extractors set no such key).
    pub forward_decl: SeedForwardDecl,
}

/// CPP-DECLARATORS-1 (§2.3): the 3-state read of a chunk's stored `metadata_json.forward_decl`
/// as it crosses the seed corpus boundary. Mirrors the indexer's `ForwardDeclRead` (which the
/// `storage` adapter maps FROM — the pure seed crate never depends on the indexer), so a CORRUPT
/// carrier is a DISTINCT truth from an absent one and is never collapsed into `ForwardDecl`.
///
/// Dispatch axis: the 3 mutually-exclusive truth-states of the stored flag — fixed variants,
/// exhaustive match at the one consumer (`build_store`). A sum type, NOT a `bool` (which could not
/// represent `Unreadable` and made corrupt metadata masquerade as a `(decl)` — the review-5 defect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedForwardDecl {
    /// No carrier, valid JSON without the key, or `forward_decl: false` — a definition (a KNOWN,
    /// readable fact: this node is not a forward declaration). The span heuristic still governs
    /// callables downstream.
    Definition,
    /// `metadata_json` carries `forward_decl: true` — a bodiless declaration. Forces `(decl)`.
    ForwardDecl,
    /// The carrier was PRESENT but UNREADABLE (did not parse, or a non-boolean `forward_decl`
    /// value). NEVER forces `(decl)` — excluded from the decl tier and COUNTED, so corrupt
    /// metadata is never rendered as a positive declaration fact (STANDING HONESTY RULE 1).
    Unreadable,
}

/// The corpus for a repo's current READY snapshot. `snapshot_uid` is `None` when
/// the repo is not indexed / has no READY snapshot — that is "no corpus", never an
/// error; the pass then has nothing to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedCorpus {
    pub snapshot_uid: Option<String>,
    pub entries: Vec<SeedCorpusEntry>,
}

/// One stored seed vector — a chunk's embedding plus the denormalized fields the
/// serving path renders WITHOUT re-joining `nodes` (spec §3/§4). A raw owned
/// boundary DTO; `Vec<f32>` crosses the boundary, never a BLOB handle.
#[derive(Debug, Clone, PartialEq)]
pub struct SeedVectorEntry {
    pub node_uid: String,
    pub stable_key: String,
    pub file_uid: String,
    pub path: String,
    /// The `path:line` anchor line; `None` renders WITHOUT a line (never a 0).
    pub line: Option<i64>,
    pub qualified_name: Option<String>,
    /// Production (`false`) vs test (`true`) — the moat partition (spec §5).
    /// SEED-CHUNK-2: this is now the PER-CHUNK value (the file fact OR structural
    /// per-symbol evidence), not the bare file fact.
    pub is_test: bool,
    /// SEED-CHUNK-2 (spec §2.2): the chunk's span is a DECLARATION without a body
    /// (prototype / trait-method decl / interface member / `declare`). A decl ranks
    /// below any body-bearing chunk of the same qualified name and renders `(decl)`.
    pub is_decl: bool,
    /// SEED-CHUNK-3 (spec §2.1): the chunk is in the FIELD tier — a data member
    /// (subtype PROPERTY/FIELD, any language) or an undocumented one-line chunk whose
    /// embedded document is ~90% its own qualified name. A field-tier chunk ranks BELOW
    /// every body-bearing chunk of its PARTITION (keyed on the partition only, never on
    /// the qualified name — a property has no impl counterpart for decl-demotion to pair
    /// with) and renders `[field]`. Properties stay IN the corpus; the tier applies
    /// within the ranked list, not at admission.
    pub is_field: bool,
    pub content_hash: String,
    /// SEED-CHUNK-3 (review-3): the copy-forward REUSE discriminator — the 16-hex digest
    /// ([`crate::hash::content_hash`]) of the ASSEMBLED chunk document
    /// ([`crate::document::build_chunk_document`]), i.e. the exact bytes fed to the embedder.
    ///
    /// `content_hash` (the FILE hash) is NOT a sufficient reuse key: the document also carries
    /// the EXTRACTOR-DERIVED `doc_comment`, which SC3 made variable INDEPENDENTLY of the file
    /// bytes (a property whose leading `//` run was previously discarded now becomes its doc on
    /// re-index, with the file byte-identical). Keying reuse on `(stable_key, content_hash,
    /// document_hash)` invalidates a copy-forward whenever the embedded document changes — from
    /// file bytes OR extractor logic — so a stale vector is never re-stamped as current
    /// (STANDING HONESTY RULE 1). `None` on a row that predates migration 037 (a legacy parent):
    /// its document identity is unknown, so it is EXCLUDED from copy-forward and re-embedded (the
    /// one-time self-heal transition), never reused blind.
    pub document_hash: Option<String>,
    /// The `dim`-length L2-normalized vector.
    pub vector: Vec<f32>,
}

/// A snapshot's stored vectors plus the homogeneous model stamp they carry. A
/// snapshot's rows are all written in one pass under one model, so the set has one
/// `model_id`/`model_checksum`/`dim` (all `None` when the set is empty). The reader
/// REJECTS a heterogeneous set (mixed stamps / a vector whose decoded length differs
/// from `dim`) as unreadable — it never hands the serving path a corrupt/partial
/// store to score (STANDING HONESTY RULE 1). The serving path compares this stamp to
/// the runtime model to decide fresh-serve vs pins-mismatch (spec §3).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSeedVectors {
    pub model_id: Option<String>,
    /// sha256 of the model files the writing pass loaded (spec §2 "checksum
    /// recorded") — recorded provenance of the embedding regime; `None` when the set
    /// is empty. Surfaced by the doctor; not part of the serve-time pin (that is
    /// `model_id` + `dim`, the ratified invalidation identity, spec §3).
    pub model_checksum: Option<String>,
    pub dim: Option<u32>,
    pub entries: Vec<SeedVectorEntry>,
}

/// Failure reading the corpus / vectors (a wrapped storage error string — the pure
/// core never sees the concrete storage error type).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SeedCorpusError {
    #[error("seed corpus read failed: {0}")]
    Read(String),
    /// SEED-CHUNK-2 (§2.4): the stored vectors predate per-chunk test/decl
    /// classification (migration 034) — they carry the stale per-FILE `is_test` and no
    /// `is_decl`, so serving them would present a false per-chunk fact. A DISTINCT
    /// variant from [`Read`](Self::Read) (genuine corruption / I-O) because the two
    /// outcomes are treated differently: this is a self-healing UPGRADE state — the
    /// daemon SCHEDULES a background re-seed and renders "re-embedding (pending)", never
    /// the terminal "unreadable; rebuild on next index". Kept a typed variant, NOT a
    /// message-substring test at the consumer (a name/message is not its semantics —
    /// STANDING HONESTY RULE: never classify from a string).
    #[error("seed vectors predate per-chunk classification (migration 034): {0}")]
    StaleClassification(String),
}

/// The read seam that hands the pure seed logic its corpus + stored vectors without
/// the logic importing the SQLite adapter (spec §10; the earned dependency
/// inversion — identical to the ratified `AgentStorageRead` pattern). Implemented on
/// `StorageConnection` by `repo-graph-storage`.
pub trait SeedCorpusRead {
    /// Enumerate the current READY snapshot's SYMBOL chunks for `repo_uid`, ordered
    /// by `(path, line_start)` (deterministic). `snapshot_uid = None` ⇒ not indexed.
    fn seed_corpus(&self, repo_uid: &str) -> Result<SeedCorpus, SeedCorpusError>;

    /// Read all stored seed vectors for `snapshot_uid`. Empty ⇒ no vectors yet
    /// (pre-migration snapshot, or the async pass has not written them) — the caller
    /// renders "no seeds yet", never a stale fallback. The homogeneous model stamp is
    /// carried on the returned [`StoredSeedVectors`] so the caller can hard-fail a
    /// model/dim pin mismatch (I3).
    fn read_seed_vectors(&self, snapshot_uid: &str) -> Result<StoredSeedVectors, SeedCorpusError>;

    /// Resolve the OWNING MODULE display path for each of `file_uids` in
    /// `snapshot_uid` from the genuine ownership mapping (operator ruling
    /// 2026-08-25). Presence = a genuine owning module; absence = no ownership
    /// recorded (the caller renders that as unavailable-with-reason, never a
    /// fallback value). A read failure is `Err` — never an empty map.
    fn module_owners(
        &self,
        snapshot_uid: &str,
        file_uids: &[String],
    ) -> Result<std::collections::HashMap<String, String>, SeedCorpusError>;
}
