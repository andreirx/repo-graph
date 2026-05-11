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
//! SB-7A: Each language module now exports an adapter struct that
//! implements `LanguageStateAdapter`. The free functions are
//! preserved for backward compatibility.
//!
//! Populated:
//! - `typescript` (SB-3): TypeScript/JavaScript adapter
//! - `python` (SB-7C): Python adapter
//! - Java, C++ adapters arrive in follow-on slices (SB-7B)

pub mod python;
pub mod typescript;

// Re-export adapter structs for convenience.
pub use python::PythonAdapter;
pub use typescript::TypeScriptAdapter;
