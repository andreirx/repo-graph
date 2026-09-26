//! TOOLCHAIN-STALENESS-1 (RG-REQ-001-L06, LD-01): the one extractor set an index composes, and the
//! running toolchain stamp derived from it.
//!
//! Abstraction record — what: the one owned list of the six language extractors in composition
//! order (ts, c, cpp, java, python, rust); concrete users: `compose.rs`'s index path and refresh path
//! (construct → initialize → hand `ports()` to the orchestrator) and the daemon's running stamp
//! ([`running_toolchain_json`]); force: the stamp an index WRITES and the stamp the daemon COMPARES
//! must come from one list, or the first slice that bumps a version constant makes them drift;
//! simpler alternative rejected: a names-only list beside the composition plus a test binding the
//! two — still a second hand-maintained list.

use repo_graph_c_extractor::CExtractor;
use repo_graph_cpp_extractor::CppExtractor;
use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::orchestrator::build_toolchain_json;
use repo_graph_java_extractor::JavaExtractor;
use repo_graph_python_extractor::PythonExtractor;
use repo_graph_rust_extractor::RustExtractor;
use repo_graph_ts_extractor::TsExtractor;

use crate::compose::ComposeError;

/// The six language extractors, in the composition order every index and refresh uses.
pub struct ExtractorSet {
    ts: TsExtractor,
    c: CExtractor,
    cpp: CppExtractor,
    java: JavaExtractor,
    python: PythonExtractor,
    rust: RustExtractor,
}

impl ExtractorSet {
    /// Construct the six extractors. Construction only — no grammar is loaded until
    /// [`Self::initialize`].
    pub fn new() -> Self {
        Self {
            ts: TsExtractor::new(),
            c: CExtractor::new(),
            cpp: CppExtractor::new(),
            java: JavaExtractor::new(),
            python: PythonExtractor::new(),
            rust: RustExtractor::new(),
        }
    }

    /// Initialize the six extractors in composition order. The first failure stops and is
    /// reported as `ComposeError::ExtractorInit("<lang>: <error>")` — the same text the index and
    /// refresh paths produced when each built its own list.
    pub fn initialize(&mut self) -> Result<(), ComposeError> {
        self.ts
            .initialize()
            .map_err(|e| ComposeError::ExtractorInit(format!("ts: {}", e)))?;
        self.c
            .initialize()
            .map_err(|e| ComposeError::ExtractorInit(format!("c: {}", e)))?;
        self.cpp
            .initialize()
            .map_err(|e| ComposeError::ExtractorInit(format!("cpp: {}", e)))?;
        self.java
            .initialize()
            .map_err(|e| ComposeError::ExtractorInit(format!("java: {}", e)))?;
        self.python
            .initialize()
            .map_err(|e| ComposeError::ExtractorInit(format!("python: {}", e)))?;
        self.rust
            .initialize()
            .map_err(|e| ComposeError::ExtractorInit(format!("rust: {}", e)))?;
        Ok(())
    }

    /// The extractor ports the orchestrator takes, in composition order.
    pub fn ports(&mut self) -> Vec<&mut dyn ExtractorPort> {
        vec![
            &mut self.ts,
            &mut self.c,
            &mut self.cpp,
            &mut self.java,
            &mut self.python,
            &mut self.rust,
        ]
    }
}

impl Default for ExtractorSet {
    fn default() -> Self {
        Self::new()
    }
}

/// The toolchain stamp this build writes on every full index and refresh
/// (`{"extractors":[six names in composition order],"indexer":INDEXER_VERSION}`), built from a
/// constructed — not initialized — [`ExtractorSet`] through the orchestrator's own stamp builder.
pub fn running_toolchain_json() -> String {
    let mut set = ExtractorSet::new();
    build_toolchain_json(&set.ports())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_toolchain_lists_the_six_extractors_in_composition_order_and_the_indexer() {
        let v: serde_json::Value = serde_json::from_str(&running_toolchain_json()).unwrap();
        let names: Vec<&str> = v["extractors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap().split(':').next().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "ts-core",
                "c-core",
                "cpp-core",
                "java-core",
                "python-core",
                "rust-core"
            ]
        );
        assert_eq!(
            v["indexer"],
            repo_graph_indexer::orchestrator::INDEXER_VERSION
        );
        // Construction only: the stamp needs no grammar, and `ports()` hands the orchestrator the
        // same six in the same order.
        let mut set = ExtractorSet::new();
        let ports = set.ports();
        assert_eq!(running_toolchain_json(), build_toolchain_json(&ports));
        assert_eq!(
            v,
            serde_json::json!({
                "extractors": [
                    TsExtractor::new().name(),
                    CExtractor::new().name(),
                    CppExtractor::new().name(),
                    JavaExtractor::new().name(),
                    PythonExtractor::new().name(),
                    RustExtractor::new().name(),
                ],
                "indexer": repo_graph_indexer::orchestrator::INDEXER_VERSION,
            })
        );
    }
}
