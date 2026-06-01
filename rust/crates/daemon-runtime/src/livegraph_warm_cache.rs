//! WARM-CACHE-DAEMON-WIRING-1: warm-cache seam for the daemon SCIP refresh path.
//!
//! Owns ONLY the cache boundary — the on-disk path, [`CacheKey`] construction, a non-fatal validated
//! read, and a best-effort atomic write. It deliberately does NOT run the producer or swap the
//! `LiveGraph`; those stay in [`crate::livegraph_refresh`] (separation of concerns + the 500-line
//! guardrail).
//!
//! Authority (PARTITIONED-WARM-CACHE-ARCH-1): the warm cache is a NON-authoritative accelerator. Every
//! read is validated (key + schema + checksum) before use; ANY read miss/mismatch/corruption returns
//! `None` (→ producer path), and ANY write failure is logged and swallowed (→ the fresh in-memory
//! graph is already serving). The cache never blocks correctness and is always safe to delete.

use std::path::{Path, PathBuf};

use repo_graph_ir::PartitionIr;
use repo_graph_livegraph::ValueFact;
use repo_graph_warm_cache::{
    atomic_write, decode_partition, encode_partition, read_validated, CacheKey, CacheManifest,
    ProducerFingerprint,
};
use repo_graph_warm_cache_feed::{encode_value_facts_sidecar, try_decode_value_facts_sidecar};

/// Producer name stamped into the cache key. MUST match the value passed to `ingest_partition` in
/// [`crate::livegraph_refresh::run_refresh`] — `run_refresh` references this same constant so the cache
/// identity and the ingested provenance cannot drift.
pub const INDEXER_NAME: &str = "scip-typescript";
/// Producer version (see [`INDEXER_NAME`]).
pub const INDEXER_VERSION: &str = "0.4.0";
/// Runtime version stamped into the cache key (the producer/runtime identity axis — D3). A
/// daemon-runtime version change invalidates every entry as `KeyMismatch`. Uses the daemon-runtime
/// crate version consistently (this module lives in daemon-runtime).
pub const REPO_GRAPH_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The cache file path: `<project_dir>/.rgr/warm-cache/<partition_id>.cache` (D1: repo-local +
/// disposable; `.rgr/` is gitignored).
pub fn partition_cache_path(project_dir: &str, partition_id: &str) -> PathBuf {
    Path::new(project_dir)
        .join(".rgr")
        .join("warm-cache")
        .join(format!("{partition_id}.cache"))
}

/// The producer's logical fingerprint (PRODUCER-ABSENT-1 D3): name + version, stable across reinstalls
/// (NOT binary path/mtime). A producer-absent load compares this against the manifest's stored value.
pub fn logical_fingerprint() -> ProducerFingerprint {
    ProducerFingerprint {
        name: INDEXER_NAME.to_string(),
        version: INDEXER_VERSION.to_string(),
    }
}

/// Construct the expected cache key (D2/D3 identity). `source_inputs_hash` is the producer-free source
/// digest; `producer_fingerprint` is the logical producer identity. `schema_version` is NOT a key field
/// (crate-owned format gate → `SchemaMismatch`); `repo_graph_version` IS a key field (`KeyMismatch`).
pub fn build_cache_key(repo_uid: &str, partition_id: &str, source_inputs_hash: &str) -> CacheKey {
    CacheKey {
        repo_uid: repo_uid.to_string(),
        partition_id: partition_id.to_string(),
        source_inputs_hash: source_inputs_hash.to_string(),
        producer_fingerprint: logical_fingerprint(),
        repo_graph_version: REPO_GRAPH_VERSION.to_string(),
    }
}

/// Try to load a VALID partition cache (non-fatal). Returns `Some(ir)` only on a fully-validated hit
/// (`read_validated` gates magic/schema/key/checksum, then `decode_partition` re-validates + converts);
/// absence, mismatch, or corruption returns `None`, so the caller falls through to the producer. Never
/// panics, never fails the refresh.
pub fn try_read_partition_cache(
    project_dir: &str,
    partition_id: &str,
    expected_key: &CacheKey,
) -> Option<PartitionIr> {
    let path = partition_cache_path(project_dir, partition_id);
    if !path.is_file() {
        return None;
    }
    match read_validated(&path, expected_key)
        .and_then(|bytes| decode_partition(&bytes, expected_key))
    {
        Ok(ir) => Some(ir),
        Err(e) => {
            // Non-fatal: a stale / corrupt / mismatched entry is treated as a cache miss.
            eprintln!(
                "warm-cache: ignoring invalid partition cache {}: {e}",
                path.display()
            );
            None
        }
    }
}

/// Best-effort write of a partition cache after a successful producer refresh (D5 atomic write; D6
/// after-feed). A failure is logged and swallowed — the cache is an accelerator, never a correctness
/// dependency, so it must not fail the refresh. `created_at` is supplied by the caller (this module
/// does not read the clock).
pub fn best_effort_write_partition_cache(
    project_dir: &str,
    partition_id: &str,
    ir: &PartitionIr,
    key: &CacheKey,
    created_at: u64,
) {
    let manifest = CacheManifest {
        magic: 0,          // filled by encode_partition
        schema_version: 0, // filled by encode_partition
        key: key.clone(),
        created_at,
        content_length: 0,       // filled by encode_partition
        checksum: String::new(), // filled by encode_partition
    };
    let bytes = encode_partition(ir, manifest);
    let path = partition_cache_path(project_dir, partition_id);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("warm-cache: cannot create {}: {e}", parent.display());
            return;
        }
    }
    if let Err(e) = atomic_write(&path, &bytes) {
        eprintln!("warm-cache: cache write failed for {}: {e}", path.display());
    }
}

/// The value-facts sidecar path: `<project_dir>/.rgr/warm-cache/<partition_id>.vf` (sibling of the
/// partition `.cache`; WARM-CACHE-VALUEFACTS-1 D3).
pub fn value_facts_sidecar_path(project_dir: &str, partition_id: &str) -> PathBuf {
    Path::new(project_dir)
        .join(".rgr")
        .join("warm-cache")
        .join(format!("{partition_id}.vf"))
}

/// Try to load a VALID value-facts sidecar (non-fatal, D7 independence). Returns `Some(facts)` only on
/// a fully-validated hit under `expected_key` (the SAME key as the partition); absence / mismatch /
/// corruption returns `None` so the caller warm-loads the graph WITHOUT value facts. Never panics.
pub fn read_value_facts_sidecar(
    project_dir: &str,
    partition_id: &str,
    expected_key: &CacheKey,
) -> Option<Vec<ValueFact>> {
    let path = value_facts_sidecar_path(project_dir, partition_id);
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    try_decode_value_facts_sidecar(&bytes, expected_key)
}

/// Best-effort write of the value-facts sidecar after a successful producer refresh (D5 atomic; D6
/// after-feed). INDEPENDENT of the partition cache write: a failure here is logged and swallowed and
/// NEVER fails the refresh (D7 — the sidecar is optional for serving graph queries). `created_at` is
/// supplied by the caller (this module does not read the clock).
pub fn best_effort_write_value_facts_sidecar(
    project_dir: &str,
    partition_id: &str,
    facts: &[ValueFact],
    key: &CacheKey,
    created_at: u64,
) {
    let manifest = CacheManifest {
        magic: 0,
        schema_version: 0,
        key: key.clone(),
        created_at,
        content_length: 0,
        checksum: String::new(),
    };
    let bytes = encode_value_facts_sidecar(facts, manifest);
    let path = value_facts_sidecar_path(project_dir, partition_id);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("warm-cache: cannot create {}: {e}", parent.display());
            return;
        }
    }
    if let Err(e) = atomic_write(&path, &bytes) {
        eprintln!(
            "warm-cache: value-facts sidecar write failed for {}: {e}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_is_repo_local_under_rgr() {
        let p = partition_cache_path("/repo/x", "default");
        assert!(
            p.ends_with(".rgr/warm-cache/default.cache"),
            "{}",
            p.display()
        );
    }

    #[test]
    fn cache_key_carries_runtime_and_producer_identity() {
        let k = build_cache_key("repo_1", "default", "deadbeef");
        assert_eq!(k.repo_uid, "repo_1");
        assert_eq!(k.partition_id, "default");
        assert_eq!(k.source_inputs_hash, "deadbeef");
        assert_eq!(k.producer_fingerprint.name, INDEXER_NAME);
        assert_eq!(k.producer_fingerprint.version, INDEXER_VERSION);
        assert_eq!(k.repo_graph_version, REPO_GRAPH_VERSION);
    }

    #[test]
    fn missing_cache_file_is_a_miss_not_an_error() {
        let key = build_cache_key("repo_1", "default", "deadbeef");
        assert!(try_read_partition_cache("/nonexistent/repo/path", "default", &key).is_none());
    }
}
