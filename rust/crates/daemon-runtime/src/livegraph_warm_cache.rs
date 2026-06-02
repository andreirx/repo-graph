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
    atomic_write, decode_partition, encode_partition, peek_manifest, read_validated, CacheKey,
    CacheManifest, ProducerFingerprint,
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

/// A filesystem-safe rendering of a `partition_id` for use as a FILENAME component
/// (IMPORTS-XPART-ENUMERATION-1): a multi-partition `partition_id` is the repo-relative root (e.g.
/// `"packages/a"`), whose `/` would otherwise create a spurious nested directory (the bug live
/// validation surfaced). Path separators -> `_`. This does NOT change the partition's identity (slot
/// key / keys); only the filename. The per-partition `project_dir` already disambiguates partitions, so
/// a sanitization collision cannot cross-contaminate two partitions' caches.
pub fn filename_safe_partition_id(partition_id: &str) -> String {
    partition_id.replace(['/', '\\'], "_")
}

/// The cache file path: `<project_dir>/.rgr/warm-cache/<partition_id>.cache` (D1: repo-local +
/// disposable; `.rgr/` is gitignored). `partition_id` is filename-sanitized (multi-partition ids carry
/// `/`).
pub fn partition_cache_path(project_dir: &str, partition_id: &str) -> PathBuf {
    Path::new(project_dir)
        .join(".rgr")
        .join("warm-cache")
        .join(format!(
            "{}.cache",
            filename_safe_partition_id(partition_id)
        ))
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
        .join(format!("{}.vf", filename_safe_partition_id(partition_id)))
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

/// The result of selecting a producer-absent cache candidate for a partition (PRODUCER-ABSENT-1 D5).
pub enum ProducerAbsentCandidate {
    /// No cache file whose manifest `source_inputs_hash` matches the current one.
    None,
    /// Exactly one producer fingerprint among matching candidates: the cache bytes + the expected key
    /// reconstructed from the manifest's `producer_fingerprint` + the CURRENT repo/runtime fields.
    One {
        /// The on-disk cache bytes (still subject to final checksum/key validation via `decode_validated`).
        bytes: Vec<u8>,
        /// The expected key for final validation (current fields + the manifest's producer fingerprint).
        expected_key: CacheKey,
    },
    /// Matching candidates disagree on `producer_fingerprint` — refuse to pick arbitrarily (D5).
    Ambiguous,
}

/// Find a producer-absent cache candidate for `partition_id` (PRODUCER-ABSENT-1 D5). Reads the
/// partition's cache file(s), peeks each manifest (magic/schema only — not acceptance), keeps those
/// whose `manifest.key.source_inputs_hash` matches the CURRENT `source_inputs_hash`, and delegates to
/// [`select_producer_absent_candidate`]. The single-file layout yields ≤1 candidate; the
/// distinct-fingerprint guard exists for any future multi-file layout.
pub fn find_producer_absent_candidate(
    project_dir: &str,
    repo_uid: &str,
    partition_id: &str,
    source_inputs_hash: &str,
) -> ProducerAbsentCandidate {
    let paths = [partition_cache_path(project_dir, partition_id)];
    let mut candidates: Vec<(Vec<u8>, ProducerFingerprint)> = Vec::new();
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(manifest) = peek_manifest(&bytes) else {
            continue; // not a compatible cache file (magic/schema) -> not a candidate
        };
        if manifest.key.source_inputs_hash == source_inputs_hash {
            candidates.push((bytes, manifest.key.producer_fingerprint));
        }
    }
    select_producer_absent_candidate(candidates, repo_uid, partition_id, source_inputs_hash)
}

/// Pure selection over a candidate list (testable; PRODUCER-ABSENT-1 D5): 0 → `None`; all candidates
/// share one fingerprint → `One` (the expected key is built from that fingerprint + the CURRENT
/// repo/partition/source/runtime fields, so a runtime-version mismatch is still caught by the final
/// `decode_validated`); ≥2 DISTINCT fingerprints → `Ambiguous` (never pick arbitrarily).
fn select_producer_absent_candidate(
    candidates: Vec<(Vec<u8>, ProducerFingerprint)>,
    repo_uid: &str,
    partition_id: &str,
    source_inputs_hash: &str,
) -> ProducerAbsentCandidate {
    let Some(first) = candidates.first() else {
        return ProducerAbsentCandidate::None;
    };
    let first_fp = first.1.clone();
    if candidates.iter().any(|(_, fp)| *fp != first_fp) {
        return ProducerAbsentCandidate::Ambiguous;
    }
    let (bytes, fingerprint) = candidates.into_iter().next().expect("non-empty");
    let expected_key = CacheKey {
        repo_uid: repo_uid.to_string(),
        partition_id: partition_id.to_string(),
        source_inputs_hash: source_inputs_hash.to_string(),
        producer_fingerprint: fingerprint,
        repo_graph_version: REPO_GRAPH_VERSION.to_string(),
    };
    ProducerAbsentCandidate::One {
        bytes,
        expected_key,
    }
}

/// Final acceptance for a producer-absent candidate: decode + FULLY validate (checksum + key) the
/// bytes into a `PartitionIr`; `None` on any rejection (corrupt/mismatch → no usable cache).
pub fn decode_validated(bytes: &[u8], expected_key: &CacheKey) -> Option<PartitionIr> {
    decode_partition(bytes, expected_key).ok()
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
    fn multi_partition_id_is_filename_safe() {
        // IMPORTS-XPART-ENUMERATION-1: a repo-relative partition id ("packages/a") must NOT create a
        // nested path in any filename (the live-validation ENOENT bug).
        assert_eq!(filename_safe_partition_id("packages/a"), "packages_a");
        assert_eq!(filename_safe_partition_id("default"), "default"); // single-partition unchanged
        let p = partition_cache_path("/repo/packages/a", "packages/a");
        assert!(
            p.ends_with(".rgr/warm-cache/packages_a.cache"),
            "no nested dir from the id slash: {}",
            p.display()
        );
        let vf = value_facts_sidecar_path("/repo/packages/a", "packages/a");
        assert!(
            vf.ends_with(".rgr/warm-cache/packages_a.vf"),
            "{}",
            vf.display()
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

    #[test]
    fn producer_absent_distinct_fingerprints_is_ambiguous_else_one_or_none() {
        let fp = |v: &str| ProducerFingerprint {
            name: "scip-typescript".to_string(),
            version: v.to_string(),
        };
        // >=2 DISTINCT fingerprints -> Ambiguous (never pick arbitrarily; D5).
        let cands = vec![(vec![1u8], fp("0.4.0")), (vec![2u8], fp("0.5.0"))];
        assert!(matches!(
            select_producer_absent_candidate(cands, "r", "p", "h"),
            ProducerAbsentCandidate::Ambiguous
        ));
        // one fingerprint -> One.
        let cands = vec![(vec![1u8], fp("0.4.0"))];
        assert!(matches!(
            select_producer_absent_candidate(cands, "r", "p", "h"),
            ProducerAbsentCandidate::One { .. }
        ));
        // empty -> None.
        assert!(matches!(
            select_producer_absent_candidate(vec![], "r", "p", "h"),
            ProducerAbsentCandidate::None
        ));
    }
}
