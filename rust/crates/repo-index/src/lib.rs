//! repo-graph-repo-index — outer composition crate for disk-based
//! repo indexing.
//!
//! Owns the "index a real repo from disk into SQLite" use case.
//! All code here is outer-layer mechanism: filesystem scanning,
//! config reading, content hashing, FileInput assembly, and
//! composition wiring.
//!
//! Slice substep state (Rust-7A):
//!   - R7-A crate skeleton + locks ........ done
//!   - R7-B scanner adapter ............... done
//!   - R7-C config readers ................ done
//!   - R7-D composition entry points ...... done
//!   - R7-E integration test .............. done
//!   - R7-F final acceptance gate ......... done
//!
//! ── Locked decisions ─────────────────────────────────────────
//!
//! **Hash algorithm:** SHA-256 of UTF-8 content bytes, hex-encoded,
//! truncated to first 16 characters. Byte-matches TS `hashContent`:
//! `createHash("sha256").update(content).digest("hex").slice(0, 16)`.
//!
//! **Package.json lookup:** Walk from file's parent directory
//! upward to repo root. First `package.json` found wins. Only
//! for JS/TS files. Reads `dependencies`, `devDependencies`,
//! `peerDependencies`, `optionalDependencies` — names as sorted
//! unique `Vec<String>`.
//!
//! **Tsconfig.json lookup:** Walk from file's parent directory
//! upward to repo root. First `tsconfig.json` found wins.
//! Follows `extends` chains (relative paths only, max depth 10).
//! First `compilerOptions.paths` in the chain is the effective
//! result (child replaces parent entirely per TypeScript merge
//! rule).
//!
//! **API shape:** Four entry points:
//!   - `index_path` / `index_into_storage` — full index from disk
//!   - `refresh_path` / `refresh_into_storage` — incremental refresh
//!     Both share `prepare_repo_inputs` for scanning/config/assembly
//!     and `persist_read_failures` for read-failure repair.
//!
//! **Scope:** Full-index + refresh from disk. No CLI, no trust
//! computation. Policy crates unchanged.
//!
//! ── Intermediate types ───────────────────────────────────────
//!
//! `ScannedFile` — filesystem facts (path, bytes, hash, language)
//! `RepoConfigContext` — config facts per directory (deps, aliases)
//! Assembly phase converts these + config into typed `FileInput`
//! for the indexer orchestrator.

pub mod compose;
pub mod config;
pub mod express_detector;
pub(crate) mod http_boundary;
pub mod impact_propagation;
// DEPS-LIST-REWRITE-1 §2.2: manifest provenance + the pyproject/Gradle readers. `pub(crate)` — no
// cross-crate consumer (verified); the query side reads provenance off the diagnostics blob by the
// `deps_manifests` wire key, not by importing these types.
pub(crate) mod manifest_deps;
// PERF-INSTRUMENTATION-1: runtime perf-trace gate (RMAP_PERF). Shared by this
// crate's `perf_log!` and daemon-runtime's `perf_trace!` (which reaches it via
// the pre-existing daemon-runtime -> repo-index dependency).
pub mod perf;
pub mod react_detector;
pub mod refresh_policy;
pub mod scanner;
pub mod state_boundary_hook;

/// Iterative AST-walk helper shared by the in-crate re-parse detectors
/// (PERSIST-RECURSION-1).
mod walk;
