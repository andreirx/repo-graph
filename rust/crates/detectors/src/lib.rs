//! repo-graph-detectors — the operational dependency seam detector
//! substrate.
//!
//! Provenance: ported from the TypeScript detector substrate that
//! formerly lived at `src/core/seams/detectors/` (with the comment
//! masker at `src/core/seams/comment-masker.ts`). That TypeScript
//! prototype — and the cross-runtime parity harness that guarded the
//! port — were retired and deleted by TS-PROTOTYPE-RETIREMENT-1 (last
//! release containing them: v0.7.0). This crate is now the sole
//! implementation; the TOML detector graph it embeds
//! (`detectors.toml`) was relocated byte-for-byte from that tree into
//! this crate.
//!
//! The public surface is `detect_env_accesses` and
//! `detect_fs_mutations`, re-exported from `pipeline`. The substep
//! modules remain publicly exposed so tests and advanced callers can
//! reference the contract shapes directly.

pub mod comment_masker;
pub mod hooks;
pub mod loader;
pub mod pipeline;
pub mod types;
pub mod walker;

// Convenience re-exports for the public detector API.
pub use pipeline::{detect_env_accesses, detect_fs_mutations, production_pipeline};
