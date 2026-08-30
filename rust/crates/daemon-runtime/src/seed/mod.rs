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
//! - [`transport`] — the option-(a) `Embedder`: the a2 std-library loopback HTTP
//!   *socket/connection* concern (D-ES-9). ~260 L.
//! - [`http`] — the a2 accepted-response *parsing* concern: header/body framing
//!   (exact `Content-Length`), the OpenAI body shape, and the
//!   non-finite/zero-norm/dim/echoed-model checks (split from `transport` under the
//!   500-line guardrail, operator ruling 2). ~300 L.
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

use repo_graph_seed::store::SeedStoreKey;

use crate::livegraph_warm_cache::REPO_GRAPH_VERSION;

mod doctor;
mod dto;
mod http;
mod query;
mod transport;

// Flat public API preserved (the callers keep using `crate::seed::X`).
pub use doctor::{seed_doctor_facts, SeedDoctorFacts};
pub(crate) use dto::build_find_response;
pub use dto::{build_group_b_data, FindCandidate, FindNext, FindResponse};
pub use query::{run_semantic_query, DegradeReason, SemanticCandidate, SemanticResult};
pub use transport::{EndpointEmbedder, ModelIdentity};

/// Default loopback endpoint — the literal-IP form of the spike's LM Studio URL
/// so the out-of-box default passes the literal-IP allowlist (spec §6.1).
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:1234/v1/embeddings";
/// Default model id — the id the spike requested (operator-asserted, spec §6.1).
pub const DEFAULT_MODEL_ID: &str = "text-embedding-nomic-embed-text-v1.5";
/// Default embedding dimension (nomic-embed-text v1.5).
pub const DEFAULT_DIM: usize = 768;

/// The three endpoint env inputs (spec §6.1), read via `std::env::var` — the
/// house pattern, NOT a config subsystem.
#[derive(Debug, Clone)]
pub struct SeedEndpointConfig {
    pub endpoint: String,
    pub model_id: String,
    pub dim: usize,
    /// `Some(reason)` when `RMAP_SEED_DIM` was SET to a value that is not a positive
    /// integer. Absence → the default dim silently (honest: no operator intent to
    /// honor). A present-but-invalid value is NEVER silently coerced to the default
    /// (that would act on a misconfiguration as if it were valid); it is surfaced as
    /// an honest degraded/unavailable cause on every seed surface (STANDING HONESTY
    /// RULE — review-4 #1). `dim` still carries the default so downstream types stay
    /// total, but the query/doctor paths decline while this is `Some`.
    pub dim_config_error: Option<String>,
}

impl SeedEndpointConfig {
    pub fn from_env() -> Self {
        let endpoint =
            std::env::var("RMAP_SEED_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let model_id =
            std::env::var("RMAP_SEED_MODEL_ID").unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string());
        let (dim, dim_config_error) = parse_dim(std::env::var("RMAP_SEED_DIM").ok().as_deref());
        Self {
            endpoint,
            model_id,
            dim,
            dim_config_error,
        }
    }

    /// The store pin for this config (spec §4.3/§7.1). `repo_graph_version` is the
    /// same runtime version the warm cache keys on.
    pub fn store_key(&self) -> SeedStoreKey {
        SeedStoreKey {
            model_id: self.model_id.clone(),
            dim: self.dim as u32,
            repo_graph_version: REPO_GRAPH_VERSION.to_string(),
        }
    }
}

/// Parse `RMAP_SEED_DIM` (spec §6.1). Distinguishes three honest cases:
/// - absent / empty / whitespace ⇒ `(DEFAULT_DIM, None)` — no operator intent, default.
/// - a positive integer ⇒ `(that, None)`.
/// - a present-but-invalid value (non-numeric, zero, negative) ⇒ `(DEFAULT_DIM,
///   Some(reason))` — the default keeps `dim` total, but the `Some` reason forces
///   every seed surface to decline rather than silently act on a misconfiguration.
fn parse_dim(raw: Option<&str>) -> (usize, Option<String>) {
    match raw.map(str::trim) {
        None | Some("") => (DEFAULT_DIM, None),
        Some(s) => match s.parse::<usize>() {
            Ok(d) if d > 0 => (d, None),
            _ => (
                DEFAULT_DIM,
                Some(format!(
                    "RMAP_SEED_DIM is not a valid embedding dimension: '{s}' (expected a positive integer)"
                )),
            ),
        },
    }
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

/// The per-repo `.vec` sidecar path (spec §4.1): `<state_root>/seed-vectors/<hash16>.vec`,
/// where `<hash16>` is the DB filename stem (the same `allocate_db_path` hash).
/// A dedicated `seed-vectors/` subdir keeps the DB-orphan classifier unambiguous.
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
    fn parse_dim_distinguishes_absent_valid_and_invalid() {
        // Absence / empty ⇒ default, NO error (no operator intent to honor).
        assert_eq!(parse_dim(None), (DEFAULT_DIM, None));
        assert_eq!(parse_dim(Some("")), (DEFAULT_DIM, None));
        assert_eq!(parse_dim(Some("  ")), (DEFAULT_DIM, None));
        // A valid positive integer ⇒ that value, no error.
        assert_eq!(parse_dim(Some("1024")), (1024, None));
        assert_eq!(parse_dim(Some(" 512 ")), (512, None));
        // Present-but-invalid ⇒ default dim (kept total) + an honest error string;
        // NEVER a silent coercion to the default (review-4 #1).
        for bad in ["abc", "0", "-5", "3.5", "768x"] {
            let (dim, err) = parse_dim(Some(bad));
            assert_eq!(
                dim, DEFAULT_DIM,
                "invalid dim keeps the default for totality"
            );
            assert!(
                err.is_some(),
                "'{bad}' must surface a config error, not default silently"
            );
        }
    }

    #[test]
    fn sidecar_path_derives_under_state_root() {
        let db = Path::new("/root/databases/deadbeef12345678.db");
        let p = sidecar_path(db).unwrap();
        assert_eq!(p, PathBuf::from("/root/seed-vectors/deadbeef12345678.vec"));
    }
}
