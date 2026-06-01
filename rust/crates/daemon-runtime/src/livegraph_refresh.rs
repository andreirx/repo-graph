//! LIVEGRAPH-INTEGRATION-1C: daemon-owned SCIP refresh orchestration.
//!
//! **Step 1 (foundation):** producer discovery (D0) + the structured failure model (D6). No
//! subprocess execution, no background thread, no dispatch wiring yet (build-order steps 2–4). This
//! module changes for a different reason than the `livegraph_feed` adapter (Common Closure), so it is
//! its own module.

use std::path::PathBuf;

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
}
