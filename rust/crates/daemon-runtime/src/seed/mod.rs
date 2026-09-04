//! Semantic-seeding daemon glue (EMBED-SEED-IMPL-1) — the outer adapter around the
//! pure `repo-graph-seed` crate: it supplies the real model runtime, the real
//! files, and the real state-root sidecar path. The pure ranking/store/corpus
//! logic lives in `repo-graph-seed`; this module is the impure edge.
//!
//! ## Why the split (operator ruling 2026-08-25 / review-1 #5)
//! The single `seed.rs` reached 935 lines mixing four concerns; the 500-line
//! guardrail forbids growing it. It is split into crate-private submodules, one
//! cohesive concern each, with the flat public API (`seed::X`) preserved by
//! re-export so no caller changes:
//!
//! - [`local_engine`] — the in-process static-embedding engine (`LocalEmbedder` over
//!   `model2vec-rs` + potion-code-16M-v2), the ratified SEED-CHUNK-1 replacement for
//!   the deleted lmstudio HTTP embedder (`transport`/`http` are GONE from this crate).
//! - [`query`] — the query-time semantic tier (`run_semantic_query`), its degrade
//!   taxonomy, and genuine owning-module resolution (§8/§8.2a). ~200 L.
//! - [`dto`] — the `rmap find` response DTO + rendering (§8B.2/§8B.3). ~120 L.
//! - [`doctor`] — the doctor "Semantic seeding" facts (§9). ~90 L.
//!
//! Abstraction one-liner: crate-private cohesion split of ONE adapter under the
//! 500-line guardrail; concrete users = this crate's `seed_pass`/`dispatch`/
//! `handlers::metrics`/`reclaim`; axis = the file-size guardrail, NOT a new public
//! boundary (no new public API — the pre-ratified module allowance). Rejected
//! simpler: one 935-line file (breaches the guardrail).

use std::path::{Path, PathBuf};

mod doctor;
mod dto;
mod local_engine;
mod query;

// Flat public API preserved (the callers keep using `crate::seed::X`).
pub use doctor::{seed_doctor_facts, SeedDoctorFacts};
pub(crate) use dto::build_find_response;
pub use dto::{build_group_b_data, FindCandidate, FindNext, FindResponse};
// SEED-CHUNK-1: the in-process static-embedding engine is the ONLY seed embedder
// now — the lmstudio HTTP path (`transport`/`http`) is retired from the seed tier
// (spec §6). The model id/dim are FIXED pins from the model (no endpoint config).
// Crate-scoped (review-0 #5): every caller (`seed_pass`, `query`, `doctor`,
// `dispatch_seed`) is inside this crate; the loader does I/O, so no unearned
// cross-crate surface. Widen to `pub` only when a concrete external caller appears.
pub(crate) use local_engine::{LocalEmbedder, MODEL_DIM, MODEL_ID};
pub use query::{run_semantic_query, DegradeReason, SemanticCandidate, SemanticResult};

/// The state-root directory the in-process model is cached under (operator ruling
/// DEP-HFHUB-EDGE): `<state_root>/seed-model-cache`. Derived from the DB path the
/// same way [`sidecar_path`] is, so proofs and the product share the state-isolation
/// invariant and never touch the operator's HF home. `None` when the DB path has no
/// resolvable state root.
pub fn model_cache_dir(db_path: &Path) -> Option<PathBuf> {
    let databases_dir = db_path.parent()?; // <state_root>/databases
    let state_root = databases_dir.parent()?; // <state_root>
    Some(state_root.join("seed-model-cache"))
}

/// Test override for [`seed_enabled`]: 0 = no override (use env), 1 = force ON,
/// 2 = force OFF. Mirrors `enrich_pass::AUTO_ENRICH_OVERRIDE` so daemon tests can
/// keep the background embed pass from racing their assertions.
static AUTO_SEED_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The refresh opt-out (spec §5.2), mirroring `auto_enrich_enabled`: default ON,
/// disabled by `RMAP_SEED_VECTORS` in {0,false,off,no,disabled}.
pub fn seed_enabled() -> bool {
    match AUTO_SEED_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => seed_enabled_from_env(std::env::var("RMAP_SEED_VECTORS").ok().as_deref()),
    }
}

fn seed_enabled_from_env(val: Option<&str>) -> bool {
    match val {
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no" | "disabled"
        ),
        None => true,
    }
}

/// Test seam: force the background embed pass ON (`true`) or OFF (`false`),
/// overriding `RMAP_SEED_VECTORS`. `#[doc(hidden)]` — no production caller.
#[doc(hidden)]
pub fn set_auto_seed_for_test(enabled: bool) {
    AUTO_SEED_OVERRIDE.store(
        if enabled { 1 } else { 2 },
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// The per-repo LEGACY `.vec` sidecar path: `<state_root>/seed-vectors/<hash16>.vec`.
/// SEED-CHUNK-1 moved seed vectors into the per-snapshot `seed_vectors` SQLite table,
/// so nothing WRITES a `.vec` anymore — but a pre-migration install may still have
/// one on disk. `reclaim` keeps calling this to garbage-collect those legacy files
/// (forget deletes the DB's `.vec`; the orphan scan reclaims dangling ones). It is
/// migration hygiene, not a live store path.
pub fn sidecar_path(db_path: &Path) -> Option<PathBuf> {
    let hash16 = db_path.file_stem()?; // "<hash16>" from "<hash16>.db"
    let databases_dir = db_path.parent()?; // <state_root>/databases
    let state_root = databases_dir.parent()?; // <state_root>
    Some(
        state_root
            .join("seed-vectors")
            .join(format!("{}.vec", hash16.to_string_lossy())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_enabled_env_matrix() {
        assert!(seed_enabled_from_env(None));
        assert!(seed_enabled_from_env(Some("1")));
        assert!(seed_enabled_from_env(Some("yes")));
        for off in ["0", "false", "off", "no", "disabled", "OFF", " Off "] {
            assert!(!seed_enabled_from_env(Some(off)), "{off} should disable");
        }
    }

    #[test]
    fn sidecar_path_derives_under_state_root() {
        let db = Path::new("/root/databases/deadbeef12345678.db");
        let p = sidecar_path(db).unwrap();
        assert_eq!(p, PathBuf::from("/root/seed-vectors/deadbeef12345678.vec"));
    }

    #[test]
    fn model_cache_dir_derives_under_state_root() {
        let db = Path::new("/root/databases/deadbeef12345678.db");
        let p = model_cache_dir(db).unwrap();
        assert_eq!(p, PathBuf::from("/root/seed-model-cache"));
    }
}
