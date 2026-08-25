//! The `.vec` sidecar store (spec §4.3), modeled field-for-field on the ratified
//! warm-cache envelope (`repo-graph-warm-cache`): a validated [`SeedManifest`]
//! header + an opaque `bincode` body whose bytes the header's `content_length`
//! and `checksum` cover. Validation is a hard gate (I3): any header/integrity
//! mismatch discards the **whole** store → "no hints", never a partial read.
//!
//! The two-layer `bincode` discipline (payload serialized once, checksummed,
//! then wrapped) is copied from warm-cache so independent implementations
//! produce byte-compatible `.vec` files.

use bincode::Options as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// File magic for a repo-graph seed-vector sidecar ("RGSV"). A wrong value ⇒
/// not our file ⇒ discard.
pub const MAGIC: u32 = 0x5247_5356;
/// Sidecar format version. Any body field-shape change bumps this; older stores
/// are **discarded, never migrated** (a Layer-3 cache is rebuildable).
pub const SCHEMA_VERSION: u32 = 1;

/// Store limits (rejection, not truncation) — spec §4.3. A 160k-file monorepo at
/// 768-dim ≈ 0.5 GiB, so the 1 GiB body cap is one order above the target; past
/// it the load is rejected and seeding declines rather than reading partially.
pub const MAX_HEADER_BYTES: u64 = 64 * 1024; // 64 KiB
pub const MAX_BODY_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB

/// Framing slack for the bincode envelope (`{ manifest, payload }`): the payload's
/// 8-byte length prefix plus a comfortable margin. Kept tiny so it cannot mask a
/// real over-budget store.
const ENVELOPE_SLACK: u64 = 4 * 1024; // 4 KiB

/// The absolute on-disk / in-memory byte ceiling for a whole `.vec` file: header +
/// body budgets plus envelope framing. Two guards enforce it (review-9 #1):
/// 1. a **file-metadata pre-guard** (`read_validated`) rejects an over-budget file
///    WITHOUT reading it — a multi-GB sidecar never enters memory; and
/// 2. a **bincode allocation ceiling** ([`decode_opts`]) bounds the deserializer so
///    a length prefix inside a *small* file cannot drive a pre-allocation past the
///    budget (the plain `bincode::deserialize` warm-cache uses does NOT bound this).
pub const MAX_FILE_BYTES: u64 = MAX_HEADER_BYTES + MAX_BODY_BYTES + ENVELOPE_SLACK;

/// Bincode config that reproduces `bincode::serialize`/`deserialize` defaults
/// (fixint, little-endian, reject-trailing) but adds an allocation ceiling. Used
/// ONLY for decode; [`encode`] keeps the plain `bincode::serialize` so the on-disk
/// bytes stay byte-identical to a warm-cache-style store. The two are byte-compatible
/// because the only non-default axis here (`with_limit`) affects allocation, not the
/// wire format.
fn decode_opts() -> impl bincode::Options {
    bincode::options()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(MAX_FILE_BYTES)
}

/// The pin tuple (analogue of warm-cache `CacheKey`). Because the **model itself
/// is the producer**, `model_id` occupies the fingerprint role. Any field
/// differing from the current runtime/config ⇒ [`SeedStoreError::KeyMismatch`]
/// ⇒ discard (I3 hard-fail).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedStoreKey {
    pub model_id: String,
    pub dim: u32,
    pub repo_graph_version: String,
}

/// The validation header (the seven-field warm-cache manifest shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedManifest {
    /// File magic ([`MAGIC`]).
    pub magic: u32,
    /// Sidecar format version ([`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// The pin tuple (model_id, dim, repo_graph_version).
    pub key: SeedStoreKey,
    /// Caller-supplied creation timestamp (unix seconds). Metadata only — this
    /// crate never reads the clock.
    pub created_at: u64,
    /// Byte length of the opaque payload.
    pub content_length: u64,
    /// Hex SHA-256 of the opaque payload bytes.
    pub checksum: String,
}

/// One admitted corpus file — its identity, its content pin, and its
/// `dim`-length L2-normalized vector. Field order is fixed (spec §4.3); changing
/// it requires a `SCHEMA_VERSION` bump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedVectorEntryV1 {
    pub file_uid: String,
    pub path: String,
    pub content_hash: String,
    pub vector: Vec<f32>,
}

/// The opaque payload the manifest's `content_length`/`checksum` cover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedVectorBodyV1 {
    pub entries: Vec<SeedVectorEntryV1>,
}

/// The on-disk file: validation header + opaque bincode payload (mirrors
/// warm-cache `CacheFileEnvelope`). The whole file is one bincode blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SeedFileEnvelope {
    manifest: SeedManifest,
    payload: Vec<u8>,
}

/// Everything that can go wrong loading/validating a store. Each maps to an
/// honest degraded state (spec §8.3) — never a stale-served hint.
#[derive(Debug, Error)]
pub enum SeedStoreError {
    /// The `.vec` file does not exist — the ONLY genuine "absent" case (I4:
    /// unknown-with-reason, not a masked failure). Mapped from `io::NotFound`; the
    /// caller renders "no store yet", never a degraded reason.
    #[error("seed store not found")]
    NotFound,
    #[error("seed store I/O error: {0}")]
    Io(String),
    #[error("seed store payload decode error: {0}")]
    Decode(String),
    #[error("seed store magic mismatch: found {found:#010x}, expected {expected:#010x}")]
    MagicMismatch { expected: u32, found: u32 },
    #[error("seed store schema_version mismatch: found {found}, expected {expected}")]
    SchemaMismatch { expected: u32, found: u32 },
    #[error("seed store key mismatch (model/dim/version pin differs)")]
    KeyMismatch,
    #[error("seed store content checksum mismatch (corrupt or tampered payload)")]
    ChecksumMismatch,
    #[error("seed store truncated or malformed")]
    Truncated,
    #[error("seed store exceeds the seed budget (header {header_bytes} B, body {body_bytes} B)")]
    TooLarge { header_bytes: u64, body_bytes: u64 },
    /// The `.vec` file on disk exceeds [`MAX_FILE_BYTES`] — rejected by the
    /// metadata pre-guard WITHOUT reading it (review-9 #1). Distinct from
    /// [`SeedStoreError::TooLarge`], which is measured after decode from the
    /// header/body split; this one is measured from the file length alone.
    #[error("seed store file exceeds the seed budget ({file_bytes} B on disk)")]
    FileTooLarge { file_bytes: u64 },
    /// A body entry violated the pinned invariants (wrong `dim`, non-finite, or
    /// not L2-normalized). The WHOLE store is rejected (spec §4.3, review-2 #6) —
    /// the tier never serves a partial/degenerate subset.
    #[error("seed store body entry invalid: {detail}")]
    BodyEntryInvalid { detail: String },
}

/// L2-norm tolerance for the per-entry normalized-form check. Stored vectors are
/// `v / (‖v‖ + 1e-9)` (spec §4.3 / `rank::l2_normalize`), so a legitimate entry's
/// norm is ≈ 1 to within f32 accumulation over `dim`; anything outside this band
/// is un-normalized or corrupt.
const NORM_TOLERANCE: f32 = 1e-2;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Serialize a body + pins into the on-disk `.vec` bytes. Rejects (never
/// truncates) an over-budget store.
pub fn encode(
    body: &SeedVectorBodyV1,
    key: &SeedStoreKey,
    created_at: u64,
) -> Result<Vec<u8>, SeedStoreError> {
    let payload = bincode::serialize(body).map_err(|e| SeedStoreError::Decode(e.to_string()))?;
    let manifest = SeedManifest {
        magic: MAGIC,
        schema_version: SCHEMA_VERSION,
        key: key.clone(),
        created_at,
        content_length: payload.len() as u64,
        checksum: sha256_hex(&payload),
    };
    // A manifest-serialization failure is a REAL error, never swallowed to 0 (which
    // would bypass the header-size cap and publish a store we could not size —
    // STANDING HONESTY RULE, review-4 #1).
    let header_bytes = bincode::serialize(&manifest)
        .map_err(|e| SeedStoreError::Decode(e.to_string()))?
        .len() as u64;
    if header_bytes > MAX_HEADER_BYTES || payload.len() as u64 > MAX_BODY_BYTES {
        return Err(SeedStoreError::TooLarge {
            header_bytes,
            body_bytes: payload.len() as u64,
        });
    }
    let envelope = SeedFileEnvelope { manifest, payload };
    bincode::serialize(&envelope).map_err(|e| SeedStoreError::Io(e.to_string()))
}

/// Validate and decode the body. Order (spec §4.3, review-2 #6):
/// 1. deserialize the envelope,
/// 2. **budget guard** — a DISTINCT pre-gate (not part of the integrity order),
///    so an over-budget store is rejected before any integrity work,
/// 3. **manifest integrity** — the ratified `magic → schema → key →
///    content_length → checksum` order, with NO size check interleaved,
/// 4. deserialize the body,
/// 5. **per-entry invariants** — pinned `dim`, finite, L2-normalized; ANY
///    invalid entry rejects the WHOLE store (never a partial subset).
///
/// Any mismatch at any step ⇒ discard the whole store.
pub fn decode(
    file_bytes: &[u8],
    expected_key: &SeedStoreKey,
) -> Result<SeedVectorBodyV1, SeedStoreError> {
    // Bounded deserialize (review-9 #1): a length prefix inside a small file cannot
    // drive an allocation past `MAX_FILE_BYTES` — the deserializer refuses first.
    let envelope: SeedFileEnvelope = decode_opts()
        .deserialize(file_bytes)
        .map_err(|_| SeedStoreError::Truncated)?;
    check_budget(&envelope.manifest, &envelope.payload)?;
    validate_manifest(&envelope.manifest, &envelope.payload, expected_key)?;
    let body: SeedVectorBodyV1 = decode_opts()
        .deserialize(&envelope.payload)
        .map_err(|e| SeedStoreError::Decode(e.to_string()))?;
    validate_entries(&body, expected_key.dim)?;
    Ok(body)
}

/// Store-limit pre-gate (spec §4.3 "Store limits — rejection, not truncation").
/// Kept OUT of [`validate_manifest`] so the ratified integrity order is pristine.
fn check_budget(manifest: &SeedManifest, payload: &[u8]) -> Result<(), SeedStoreError> {
    // Re-serialization failure is a REAL decode error, never swallowed to 0 (which
    // would let an unmeasurable header slip past the size cap — STANDING HONESTY
    // RULE, review-4 #1).
    let header_bytes = bincode::serialize(manifest)
        .map_err(|e| SeedStoreError::Decode(e.to_string()))?
        .len() as u64;
    if header_bytes > MAX_HEADER_BYTES || payload.len() as u64 > MAX_BODY_BYTES {
        return Err(SeedStoreError::TooLarge {
            header_bytes,
            body_bytes: payload.len() as u64,
        });
    }
    Ok(())
}

/// Header integrity in the EXACT ratified order (spec §4.3): `magic →
/// schema_version → key → content_length → checksum`. No size cap interleaved.
fn validate_manifest(
    manifest: &SeedManifest,
    payload: &[u8],
    expected_key: &SeedStoreKey,
) -> Result<(), SeedStoreError> {
    if manifest.magic != MAGIC {
        return Err(SeedStoreError::MagicMismatch {
            expected: MAGIC,
            found: manifest.magic,
        });
    }
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(SeedStoreError::SchemaMismatch {
            expected: SCHEMA_VERSION,
            found: manifest.schema_version,
        });
    }
    if &manifest.key != expected_key {
        return Err(SeedStoreError::KeyMismatch);
    }
    if manifest.content_length != payload.len() as u64 {
        return Err(SeedStoreError::Truncated);
    }
    if manifest.checksum != sha256_hex(payload) {
        return Err(SeedStoreError::ChecksumMismatch);
    }
    Ok(())
}

/// Per-entry body invariants (spec §4.3, review-2 #6). Every stored vector MUST
/// match the pinned `dim`, be all-finite, and be in L2-normalized form. A single
/// violation rejects the WHOLE store — so ranking (`rank::rank`) can rely on
/// uniform, finite, unit vectors and never silently filters a wrong-dimension
/// entry (which would serve a partial subset under the guise of a full store).
fn validate_entries(body: &SeedVectorBodyV1, dim: u32) -> Result<(), SeedStoreError> {
    let dim = dim as usize;
    for (i, e) in body.entries.iter().enumerate() {
        if e.vector.len() != dim {
            return Err(SeedStoreError::BodyEntryInvalid {
                detail: format!(
                    "entry {i} ({}) has {} dims, pinned dim is {dim}",
                    e.path,
                    e.vector.len()
                ),
            });
        }
        if e.vector.iter().any(|x| !x.is_finite()) {
            return Err(SeedStoreError::BodyEntryInvalid {
                detail: format!("entry {i} ({}) has a non-finite component", e.path),
            });
        }
        let norm = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if (norm - 1.0).abs() > NORM_TOLERANCE {
            return Err(SeedStoreError::BodyEntryInvalid {
                detail: format!("entry {i} ({}) is not L2-normalized (norm {norm})", e.path),
            });
        }
    }
    Ok(())
}

/// Read + validate + decode a `.vec` file, with a **metadata pre-guard** so an
/// over-budget sidecar is rejected WITHOUT loading it into memory (review-9 #1,
/// spec §4.3 rejection budget). `io::ErrorKind::NotFound` is the ONLY "absent" case
/// ([`SeedStoreError::NotFound`], I4: unknown-with-reason, not known-zero) — the
/// caller maps a missing file to "no vector store yet", everything else to a
/// degraded reason.
///
/// The guard reads the file length from metadata FIRST (a stat, no read) and
/// rejects `> MAX_FILE_BYTES` before allocating. The subsequent read is then
/// `take`-capped at the same ceiling as belt-and-suspenders against a file that
/// grows between the stat and the read.
pub fn read_validated(
    path: &Path,
    expected_key: &SeedStoreKey,
) -> Result<SeedVectorBodyV1, SeedStoreError> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(SeedStoreError::NotFound),
        Err(e) => return Err(SeedStoreError::Io(e.to_string())),
    };
    let len = file
        .metadata()
        .map_err(|e| SeedStoreError::Io(e.to_string()))?
        .len();
    if len > MAX_FILE_BYTES {
        return Err(SeedStoreError::FileTooLarge { file_bytes: len });
    }
    // Cap the read at the ceiling + 1 so a file that grew past the budget after the
    // stat is still caught (below) rather than read unbounded.
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SeedStoreError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(SeedStoreError::FileTooLarge {
            file_bytes: bytes.len() as u64,
        });
    }
    decode(&bytes, expected_key)
}

/// Publish the store by atomic rename (spec §4.3), copied from warm-cache's
/// `atomic_write`: write a temp sibling, fsync it, rename over the target. A
/// cancelled/superseded pass simply never calls this, so a valid existing store
/// can never be replaced by a partial one.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SeedStoreError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| SeedStoreError::Io("target path has no parent directory".to_string()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| SeedStoreError::Io(format!("create dir {}: {e}", parent.display())))?;

    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = PathBuf::from(tmp_os);

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| SeedStoreError::Io(format!("open temp {}: {e}", tmp_path.display())))?;
        file.write_all(bytes)
            .map_err(|e| SeedStoreError::Io(format!("write temp: {e}")))?;
        file.sync_all()
            .map_err(|e| SeedStoreError::Io(format!("fsync temp: {e}")))?;
    }

    std::fs::rename(&tmp_path, path)
        .map_err(|e| SeedStoreError::Io(format!("rename temp over target: {e}")))?;

    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Unit tests live in `store_tests.rs` (split via `#[path]` to keep this file under
/// the 500-line structural guardrail — the repo `pass_tests.rs` idiom).
#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
