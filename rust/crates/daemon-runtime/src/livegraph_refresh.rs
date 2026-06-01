//! LIVEGRAPH-INTEGRATION-1C: daemon-owned SCIP refresh orchestration.
//!
//! **Step 1 (foundation):** producer discovery (D0) + the structured failure model (D6). No
//! subprocess execution, no background thread, no dispatch wiring yet (build-order steps 2–4). This
//! module changes for a different reason than the `livegraph_feed` adapter (Common Closure), so it is
//! its own module.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use repo_graph_livegraph::LiveGraph;
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use repo_graph_trust_model::LanguageSupport;

use crate::livegraph_warm_cache;
use crate::state::RepoState;

/// Structured refresh failure classes (D6). Surfaced in the refresh command's structured response;
/// `ProducerUnavailable` is the D0 graceful-absent path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshFailure {
    /// The `scip-typescript` producer was not found (config + PATH). Graceful: partition unchanged.
    ProducerUnavailable,
    /// The producer ran but exited non-zero.
    ProducerFailed(String),
    /// The producer exceeded its timeout.
    Timeout,
    /// Decoding / ingesting the producer output failed.
    IngestFailed(String),
    /// Computing `build_inputs_hash` failed.
    HashFailed(String),
    /// The target is not a supported TS partition (D1).
    UnsupportedPartition(String),
}

impl RefreshFailure {
    /// Stable machine code for the structured daemon response (D6).
    pub fn code(&self) -> &'static str {
        match self {
            RefreshFailure::ProducerUnavailable => "ProducerUnavailable",
            RefreshFailure::ProducerFailed(_) => "ProducerFailed",
            RefreshFailure::Timeout => "Timeout",
            RefreshFailure::IngestFailed(_) => "IngestFailed",
            RefreshFailure::HashFailed(_) => "HashFailed",
            RefreshFailure::UnsupportedPartition(_) => "UnsupportedPartition",
        }
    }

    /// Human-readable detail for the structured response.
    pub fn detail(&self) -> String {
        match self {
            RefreshFailure::ProducerUnavailable => {
                "scip-typescript not found (set RMAP_SCIP_TYPESCRIPT or add it to PATH)".to_string()
            }
            RefreshFailure::Timeout => "producer timed out".to_string(),
            RefreshFailure::ProducerFailed(d)
            | RefreshFailure::IngestFailed(d)
            | RefreshFailure::HashFailed(d)
            | RefreshFailure::UnsupportedPartition(d) => d.clone(),
        }
    }
}

/// Discover the `scip-typescript` producer binary (D0): configured path first, PATH second.
/// `RMAP_SCIP_TYPESCRIPT` (an absolute path to the binary) wins when it points at a real file; else
/// `scip-typescript` is looked up on `$PATH`. Returns [`RefreshFailure::ProducerUnavailable`] when
/// absent — the daemon degrades gracefully and NEVER crashes / installs / hits the network.
pub fn discover_scip_typescript() -> Result<PathBuf, RefreshFailure> {
    let configured = std::env::var_os("RMAP_SCIP_TYPESCRIPT").map(PathBuf::from);
    discover_from(configured, which_on_path("scip-typescript"))
}

/// Pure discovery policy (testable without env mutation): a configured path that IS a file wins (D0);
/// else the PATH-found binary; else [`RefreshFailure::ProducerUnavailable`]. A configured-but-missing
/// path falls through to PATH ("configured first, PATH second").
fn discover_from(
    configured: Option<PathBuf>,
    path_found: Option<PathBuf>,
) -> Result<PathBuf, RefreshFailure> {
    if let Some(p) = configured {
        if p.is_file() {
            return Ok(p);
        }
    }
    if let Some(p) = path_found {
        return Ok(p);
    }
    Err(RefreshFailure::ProducerUnavailable)
}

/// Minimal `$PATH` executable lookup (no external `which` crate): the first `name` that is a file on
/// `$PATH`.
fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

// ── Step 4: synchronous daemon-owned SCIP refresh ──
// The daemon is single-threaded + !Send (DaemonState uses RefCell), so the producer runs INLINE on
// the request thread (DAEMON-ASYNC-REFRESH-1 tracks the non-blocking follow-up). The producer runs
// LOCK-FREE; the LiveGraph write lock is acquired ONLY for the swap; on any failure the last-good
// epoch is untouched.

/// Compute a real `build_inputs_hash` (D4): a fast SHA-256 digest over the partition's config +
/// `.ts` sources + the producer identity (path + size + mtime — `scip-typescript@0.4.0 --version`
/// is unavailable, so the binary metadata stands in). Coherence, not security.
fn compute_build_inputs_hash(project_dir: &str, producer: &Path) -> Result<String, RefreshFailure> {
    use sha2::{Digest, Sha256};
    let root = Path::new(project_dir);
    let mut hasher = Sha256::new();
    for rel in [
        "tsconfig.json",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
    ] {
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            hasher.update(rel.as_bytes());
            hasher.update(&bytes);
        }
    }
    let mut sources = Vec::new();
    collect_ts_sources(root, &mut sources).map_err(RefreshFailure::HashFailed)?;
    sources.sort();
    for p in &sources {
        let bytes = std::fs::read(p)
            .map_err(|e| RefreshFailure::HashFailed(format!("read {}: {e}", p.display())))?;
        hasher.update(p.to_string_lossy().as_bytes());
        hasher.update(&bytes);
    }
    hasher.update(producer.to_string_lossy().as_bytes());
    if let Ok(meta) = std::fs::metadata(producer) {
        hasher.update(meta.len().to_le_bytes());
        if let Ok(mtime) = meta.modified() {
            if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(d.as_secs().to_le_bytes());
            }
        }
    }
    hasher.update(b"scip-typescript@0.4.0");
    Ok(hex::encode(hasher.finalize()))
}

/// Collect `.ts` source files under `dir`, skipping `node_modules` / `.git`.
fn collect_ts_sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == "node_modules" || name == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_ts_sources(&path, out)?;
        } else if path.extension().map(|e| e == "ts").unwrap_or(false) {
            out.push(path);
        }
    }
    Ok(())
}

/// Run the producer binary-direct (D3): `scip-typescript index --cwd <dir> --output <out>
/// --no-progress-bar` via `std::process::Command` (no shell), with a timeout + captured stderr.
fn run_producer(
    producer: &Path,
    project_dir: &str,
    output: &Path,
    timeout: Duration,
) -> Result<(), RefreshFailure> {
    let mut child = Command::new(producer)
        .arg("index")
        .arg("--cwd")
        .arg(project_dir)
        .arg("--output")
        .arg(output)
        .arg("--no-progress-bar")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RefreshFailure::ProducerFailed(format!("spawn: {e}")))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut err);
                }
                return Err(RefreshFailure::ProducerFailed(format!(
                    "exit {:?}: {}",
                    status.code(),
                    err.trim()
                )));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(RefreshFailure::Timeout);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(RefreshFailure::ProducerFailed(format!("wait: {e}"))),
        }
    }
}

/// Synchronous daemon-owned refresh (1C step 4): discover the producer (D0), run it on the
/// partition's project dir, decode + ingest, and SWAP the result into the repo's LiveGraph (the write
/// lock is acquired ONLY for the swap). On any failure the last-good epoch is untouched. A
/// single-package partition = the repo root (D1; multi-package enumeration is a natural extension).
pub fn run_refresh(
    repo_state: &RepoState,
    repo_uid: &str,
    partition_id: &str,
    project_dir: &str,
) -> Result<serde_json::Value, RefreshFailure> {
    if !Path::new(project_dir).join("tsconfig.json").is_file() {
        return Err(RefreshFailure::UnsupportedPartition(format!(
            "no tsconfig.json under {project_dir}"
        )));
    }
    let producer = discover_scip_typescript()?; // Err(ProducerUnavailable) propagates
    let hash = compute_build_inputs_hash(project_dir, &producer)?;

    // WARM-CACHE-DAEMON-WIRING-1 (D3): try a VALID warm cache BEFORE running the producer. A hit feeds
    // the cached PartitionIr graph-only (value facts Unavailable until a producer refresh) and SKIPS
    // the multi-second producer; any miss / mismatch / corruption falls through to the producer below
    // (try_read_partition_cache is non-fatal). NOTE: producer DISCOVERY still runs (the hash embeds
    // producer identity); only producer EXECUTION is skipped. Serving purely from cache with an absent
    // producer is out of scope here (would need hash/producer decoupling).
    let cache_key = livegraph_warm_cache::build_cache_key(repo_uid, partition_id, &hash);
    if let Some(ir) =
        livegraph_warm_cache::try_read_partition_cache(project_dir, partition_id, &cache_key)
    {
        let nodes = ir.nodes.len();
        let edges = ir.edges.len();
        let epoch = {
            let mut guard = repo_state.livegraph.write();
            let lg = guard.get_or_insert_with(LiveGraph::new);
            repo_graph_livegraph_feed::feed_partition_ir(
                lg,
                partition_id,
                ir,
                LanguageSupport::TypeScriptPrimary,
            );
            lg.partition_epoch(partition_id).map(|e| e.0)
        };
        return Ok(serde_json::json!({
            "status": "WarmedFromCache",
            "refreshed": true,
            "warmed_from_cache": true,
            "partition": partition_id,
            "nodes": nodes,
            "edges": edges,
            "value_facts": 0, // graph-only warm load: value facts Unavailable (not faked)
            "epoch": epoch,
            "build_inputs_hash": hash,
        }));
    }

    let output = std::env::temp_dir().join(format!("rmap-scip-refresh-{partition_id}.scip"));
    run_producer(&producer, project_dir, &output, Duration::from_secs(120))?;
    let bytes = std::fs::read(&output)
        .map_err(|e| RefreshFailure::IngestFailed(format!("read producer output: {e}")))?;
    let index =
        decode_index(&bytes).map_err(|e| RefreshFailure::IngestFailed(format!("decode: {e}")))?;
    let outcome = ingest_partition(
        &index,
        project_dir,
        repo_uid,
        partition_id,
        livegraph_warm_cache::INDEXER_NAME,
        livegraph_warm_cache::INDEXER_VERSION,
        &hash,
    );
    let nodes = outcome.ir.nodes.len();
    let edges = outcome.ir.edges.len();
    let value_facts = outcome.complexity.len();
    // Clone the IR for the cache write BEFORE the feed consumes `outcome`, so the write happens
    // strictly AFTER the swap (D6 order). The clone is on the cold producer path (which just spent
    // seconds indexing), never the hot query path.
    let ir_for_cache = outcome.ir.clone();
    // SWAP: hold the LiveGraph write lock ONLY here (D5). The producer ran lock-free above.
    let epoch = {
        let mut guard = repo_state.livegraph.write();
        let lg = guard.get_or_insert_with(LiveGraph::new);
        repo_graph_livegraph_feed::feed_partition(
            lg,
            partition_id,
            outcome,
            LanguageSupport::TypeScriptPrimary,
        );
        lg.partition_epoch(partition_id).map(|e| e.0)
    };
    // WARM-CACHE write (D5 atomic, D6 after-feed): best-effort + non-fatal — a write failure never
    // blocks serving the fresh in-memory graph. `created_at` from the daemon clock (the warm-cache
    // crate stays clock-free).
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    livegraph_warm_cache::best_effort_write_partition_cache(
        project_dir,
        partition_id,
        &ir_for_cache,
        &cache_key,
        created_at,
    );
    let _ = std::fs::remove_file(&output);
    Ok(serde_json::json!({
        "status": "Refreshed",
        "refreshed": true,
        "warmed_from_cache": false,
        "partition": partition_id,
        "nodes": nodes,
        "edges": edges,
        "value_facts": value_facts,
        "epoch": epoch,
        "build_inputs_hash": hash,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_codes_are_stable() {
        assert_eq!(
            RefreshFailure::ProducerUnavailable.code(),
            "ProducerUnavailable"
        );
        assert_eq!(RefreshFailure::Timeout.code(), "Timeout");
        assert_eq!(
            RefreshFailure::ProducerFailed("x".into()).code(),
            "ProducerFailed"
        );
        assert_eq!(
            RefreshFailure::IngestFailed("x".into()).code(),
            "IngestFailed"
        );
        assert_eq!(RefreshFailure::HashFailed("x".into()).code(), "HashFailed");
        assert_eq!(
            RefreshFailure::UnsupportedPartition("x".into()).code(),
            "UnsupportedPartition"
        );
    }

    #[test]
    fn discovery_config_then_path_then_unavailable() {
        // Pure policy test (no env mutation). `current_exe` is a guaranteed-existing file.
        let exe = std::env::current_exe().expect("test exe path");
        // configured file wins
        assert_eq!(discover_from(Some(exe.clone()), None).unwrap(), exe);
        // configured-but-missing falls through to the PATH-found binary
        assert_eq!(
            discover_from(
                Some(PathBuf::from("/nonexistent/scip-typescript")),
                Some(exe.clone())
            )
            .unwrap(),
            exe
        );
        // nothing found → ProducerUnavailable (the D0 graceful-absent path)
        assert_eq!(
            discover_from(None, None).unwrap_err(),
            RefreshFailure::ProducerUnavailable
        );
    }

    #[test]
    fn build_inputs_hash_is_deterministic_and_nonempty() {
        // D4: hash over the synthetic fixture's real configs + .ts sources (node_modules excluded).
        let synth = format!(
            "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
            env!("CARGO_MANIFEST_DIR")
        );
        let producer = PathBuf::from("/nonexistent/scip-typescript"); // metadata skipped; path hashed
        let h1 = compute_build_inputs_hash(&synth, &producer).expect("hash");
        let h2 = compute_build_inputs_hash(&synth, &producer).expect("hash");
        assert!(!h1.is_empty());
        assert_eq!(h1, h2, "build_inputs_hash must be deterministic");
        assert_ne!(h1, "preload", "real hash, not the placeholder");
    }
}
