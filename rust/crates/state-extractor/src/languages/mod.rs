//! Language-specific adapters for state-boundary emission.
//!
//! Each submodule here owns the translation from a language
//! extractor's output (e.g. `ts-extractor`'s `ResolvedCallsite`)
//! into `StateBoundaryCallsite` inputs that drive the
//! language-agnostic `StateBoundaryEmitter`.
//!
//! Adapters live here rather than in the emitter itself to keep
//! the emitter free of per-language concerns (SB-2.2 narrowing:
//! inputs stay crate-owned; output DTOs are indexer types).
//!
//! Slice-1 populated: `typescript` (Rust-side ts-extractor
//! integration). Other languages arrive when their respective
//! Rust-workspace extractors ship.

pub mod typescript;
