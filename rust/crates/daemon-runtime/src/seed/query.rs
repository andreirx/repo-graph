//! The query-time semantic tier (spec §8/§8B): `run_semantic_query` — shared by
//! the `orient`/`explain` no-match fallback and the `find` verb. It loads the
//! validated sidecar, partitions it by freshness against the current corpus,
//! embeds the query with the pinned model, ranks by cosine, resolves each
//! candidate's stable key, and attaches the GENUINE owning module.
//!
//! Every read that is rendered or classified reports **unknown-with-reason**
//! (STANDING HONESTY RULE): only `io::NotFound` on the sidecar is "no store"; a
//! non-NotFound read is `StoreUnreadable`; a corpus-read failure is
//! `FreshnessUnknown` (we cannot verify which vectors are current) — none
//! collapses into another or into "not built yet".

use std::collections::HashMap;
use std::path::Path;

use repo_graph_agent::dto::envelope::ModuleHint;
use repo_graph_agent::storage_port::AgentStorageRead;
use repo_graph_seed::document::build_query;
use repo_graph_seed::ports::EmbedError;
use repo_graph_seed::rank::{l2_normalize, partition_fresh, rank};
use repo_graph_seed::store::SeedStoreError;
use repo_graph_seed::{Embedder, SeedCorpusRead};

use super::transport::EndpointEmbedder;
use super::{sidecar_path, SeedEndpointConfig};

/// Why the semantic tier could not produce candidates (spec §8.3). Each maps to a
/// distinct honest reason string — none collapses into another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradeReason {
    /// No `.vec` store on disk yet (never indexed / just built — the pass is async).
    /// ONLY `io::NotFound` reaches this — a genuine absence, not a failure.
    NoStore,
    /// The sidecar file IS present but a non-NotFound read failed (permissions,
    /// I/O), OR the loaded store was corrupt/truncated. Distinct from "not built
    /// yet": it exists but cannot be used; it rebuilds on next index.
    StoreUnreadable,
    /// The current corpus could not be read, so freshness (which vectors are
    /// current, I3) cannot be verified — we decline rather than rank possibly-stale
    /// vectors. NOT "no store" (the store loaded fine).
    FreshnessUnknown,
    /// The local embedding model was unreachable / declined.
    ModelUnavailable,
    /// Store pins (model/dim/schema) differ from the runtime config (I3 hard-fail).
    PinsMismatch,
    /// The store exceeds the seed budget (spec §4.3).
    StoreTooLarge,
    /// A ranked candidate's path could not be resolved against the snapshot
    /// because the resolution READ itself failed (a DB/storage error, NOT a
    /// path that legitimately names no file). The store + corpus loaded fine, so
    /// this is distinct from every reason above; we decline rather than silently
    /// drop a ranked candidate (STANDING HONESTY RULE — review-2 #3).
    ResolveUnavailable,
    /// The seed configuration is invalid (e.g. `RMAP_SEED_DIM` set to a non-integer
    /// / zero). We decline rather than silently act on a misconfiguration as if it
    /// were valid (STANDING HONESTY RULE — review-4 #1). The specific bad value is
    /// surfaced in full on the doctor diagnostic surface; the query line stays terse.
    InvalidConfig,
}

impl DegradeReason {
    /// The reader-facing reason (spec §8.3 `reasons[0]` text).
    pub fn reason(self) -> &'static str {
        match self {
            DegradeReason::NoStore => {
                "no seed vectors yet; they build in the background after indexing"
            }
            DegradeReason::StoreUnreadable => {
                "seed vector store present but unreadable; it rebuilds on next index"
            }
            DegradeReason::FreshnessUnknown => {
                "cannot verify which seed vectors are current (corpus read failed)"
            }
            DegradeReason::ModelUnavailable => {
                "no local embedding model reachable; seeding is optional, resolution is unaffected"
            }
            DegradeReason::PinsMismatch => {
                "seed vectors were built with a different model; rebuild on next index"
            }
            DegradeReason::StoreTooLarge => {
                "vector store exceeds the seed budget — seeding declined"
            }
            DegradeReason::ResolveUnavailable => {
                "could not resolve a semantic candidate (snapshot read failed); hints withheld this run"
            }
            DegradeReason::InvalidConfig => {
                "seed configuration is invalid (RMAP_SEED_DIM); set a valid positive integer"
            }
        }
    }
}

/// A resolved semantic candidate (the daemon maps this to the seam's DTO).
#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub stable_key: String,
    pub path: String,
    /// Owning-module hint — a GENUINE module from `module_file_ownership`, or an
    /// explicit unavailable-with-reason (operator ruling 2026-08-25). NEVER a
    /// directory guess.
    pub module: ModuleHint,
    pub score: f64,
    pub model_id: String,
}

/// The outcome of a semantic query — either candidates, a genuine known-zero, or a
/// labeled degraded reason (spec §8.3). The handler renders each distinctly.
#[derive(Debug)]
pub enum SemanticResult {
    Fired {
        candidates: Vec<SemanticCandidate>,
        stale_count: usize,
        total: usize,
    },
    /// Query embedded but nothing scored above zero (genuine known-zero, §8.3).
    NothingScored,
    /// The tier could not run; `reason` is the labeled cause.
    Unavailable(DegradeReason),
}

fn classify_store_error(err: &SeedStoreError) -> DegradeReason {
    match err {
        // A missing file is the only genuine absence — normally handled by the
        // caller before this, but classified here too so the taxonomy is total.
        SeedStoreError::NotFound => DegradeReason::NoStore,
        SeedStoreError::TooLarge { .. } | SeedStoreError::FileTooLarge { .. } => {
            DegradeReason::StoreTooLarge
        }
        // A model/dim/schema PIN differs — the store is for a different embedding
        // regime; rebuild on next index.
        SeedStoreError::KeyMismatch | SeedStoreError::SchemaMismatch { .. } => {
            DegradeReason::PinsMismatch
        }
        // Wrong magic is "not our file" (spec §4.3) — corrupt/foreign, NOT a pin
        // mismatch (review-9 #4). Grouped with the corrupt/truncated/decode cases:
        // the store exists but is unusable and rebuilds on next index. Honest
        // "unreadable", NOT "not built yet" and NOT "wrong model".
        _ => DegradeReason::StoreUnreadable,
    }
}

/// Resolve the owning-module hint for a file from a genuine ownership lookup
/// result (operator ruling 2026-08-25): a real module display path, an explicit
/// "no ownership recorded", or (on a lookup failure) an explicit lookup-failed
/// reason. Never a directory guess, never a silent absence.
fn module_hint(file_uid: &str, owners: &Result<HashMap<String, String>, String>) -> ModuleHint {
    match owners {
        Ok(map) => match map.get(file_uid) {
            Some(module) => ModuleHint::Owning(module.clone()),
            None => ModuleHint::Unavailable("no module ownership recorded".to_string()),
        },
        Err(e) => ModuleHint::Unavailable(format!("module ownership lookup failed: {e}")),
    }
}

/// Run the semantic fallback for a seam's own resolution input. `storage` is the
/// live connection (implements both read ports); `snapshot_uid` scopes freshness
/// resolution + module ownership; `db_path` locates the sidecar; `top_n` is the
/// seam cap (≤5 for the fallback tier, ≤10 for `find`).
#[allow(clippy::too_many_arguments)]
pub fn run_semantic_query<S>(
    storage: &S,
    snapshot_uid: &str,
    repo_uid: &str,
    db_path: &Path,
    query: &str,
    top_n: usize,
    cfg: &SeedEndpointConfig,
) -> SemanticResult
where
    S: SeedCorpusRead + AgentStorageRead + ?Sized,
{
    // An invalid seed config (e.g. a non-integer `RMAP_SEED_DIM`) is declined up
    // front — we never rank at a silently-defaulted dim as if the operator's value
    // were valid (STANDING HONESTY RULE — review-4 #1).
    if cfg.dim_config_error.is_some() {
        return SemanticResult::Unavailable(DegradeReason::InvalidConfig);
    }
    let sidecar = match sidecar_path(db_path) {
        Some(p) => p,
        None => return SemanticResult::Unavailable(DegradeReason::StoreUnreadable),
    };
    let key = cfg.store_key();

    // Load the store through the metadata-guarded reader (an over-budget sidecar is
    // rejected without loading, review-9 #1). `SeedStoreError::NotFound` is the ONLY
    // "absent" case (honesty rule): a missing file is "no store yet"; every other
    // error is a PRESENT but unusable store, classified — never silently absent.
    let body = match repo_graph_seed::store::read_validated(&sidecar, &key) {
        Ok(b) => b,
        Err(SeedStoreError::NotFound) => {
            return SemanticResult::Unavailable(DegradeReason::NoStore)
        }
        Err(e) => return SemanticResult::Unavailable(classify_store_error(&e)),
    };

    // Current corpus → freshness map (I3). A corpus read error means we cannot
    // verify freshness, so we decline — labeled `FreshnessUnknown`, NOT "no store".
    let current: HashMap<String, String> = match storage.seed_corpus(repo_uid) {
        Ok(c) => c
            .into_iter()
            .map(|e| (e.file_uid, e.content_hash))
            .collect(),
        Err(_) => return SemanticResult::Unavailable(DegradeReason::FreshnessUnknown),
    };
    let part = partition_fresh(&body, &current);

    // Embed the query with the same configured model (query-time identity).
    let embedder = match EndpointEmbedder::from_config(cfg) {
        Ok(e) => e,
        Err(_) => return SemanticResult::Unavailable(DegradeReason::ModelUnavailable),
    };
    let mut qv = match embedder.embed(&[build_query(query)]) {
        Ok(mut v) => match v.pop() {
            Some(v) => v,
            None => return SemanticResult::Unavailable(DegradeReason::ModelUnavailable),
        },
        Err(EmbedError::DimMismatch { .. }) | Err(EmbedError::ModelMismatch { .. }) => {
            return SemanticResult::Unavailable(DegradeReason::PinsMismatch)
        }
        Err(_) => return SemanticResult::Unavailable(DegradeReason::ModelUnavailable),
    };
    l2_normalize(&mut qv);

    let ranked = rank(&qv, &part.fresh, top_n);
    if ranked.is_empty() {
        return SemanticResult::NothingScored;
    }

    // Resolve each candidate's stable_key (drop a candidate whose path no longer
    // resolves against the current snapshot — the same admission the follow-up
    // would apply, spec §8.2). Keep the file_uid for the module lookup.
    struct Resolved {
        stable_key: String,
        file_uid: String,
        path: String,
        score: f32,
    }
    let mut resolved: Vec<Resolved> = Vec::new();
    for r in ranked {
        // Distinguish a genuine non-resolution (the path names no file node → drop
        // THIS candidate, legitimate) from a storage FAILURE (the read errored → we
        // cannot trust ANY resolution, so decline the whole tier with a labeled
        // reason rather than silently drop candidates — review-2 #3).
        let resolution = match storage.resolve_path_focus(snapshot_uid, &r.path) {
            Ok(res) => res,
            Err(_) => return SemanticResult::Unavailable(DegradeReason::ResolveUnavailable),
        };
        let stable_key = match resolution.file_stable_key {
            Some(k) => k,
            None => continue,
        };
        resolved.push(Resolved {
            stable_key,
            file_uid: r.file_uid,
            path: r.path,
            score: r.score,
        });
    }
    if resolved.is_empty() {
        return SemanticResult::NothingScored;
    }

    // Genuine owning-module resolution (operator ruling): ONE batch lookup over the
    // resolved file_uids. A lookup FAILURE is carried as an explicit
    // unavailable-with-reason on every candidate — the candidates (path/score/key)
    // are still valid, so we do not drop them; we only label the module honestly.
    let file_uids: Vec<String> = resolved.iter().map(|r| r.file_uid.clone()).collect();
    let owners = storage
        .module_owners(snapshot_uid, &file_uids)
        .map_err(|e| e.to_string());

    let out: Vec<SemanticCandidate> = resolved
        .into_iter()
        .map(|r| SemanticCandidate {
            module: module_hint(&r.file_uid, &owners),
            stable_key: r.stable_key,
            path: r.path,
            score: r.score as f64,
            model_id: cfg.model_id.clone(),
        })
        .collect();

    SemanticResult::Fired {
        candidates: out,
        stale_count: part.stale_count,
        total: part.total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_hint_maps_known_absent_and_failure_distinctly() {
        let mut map = HashMap::new();
        map.insert("f1".to_string(), "backend/services".to_string());
        let ok: Result<HashMap<String, String>, String> = Ok(map);
        assert_eq!(
            module_hint("f1", &ok),
            ModuleHint::Owning("backend/services".to_string())
        );
        assert_eq!(
            module_hint("f2", &ok),
            ModuleHint::Unavailable("no module ownership recorded".to_string())
        );
        let err: Result<HashMap<String, String>, String> = Err("db locked".to_string());
        assert_eq!(
            module_hint("f1", &err),
            ModuleHint::Unavailable("module ownership lookup failed: db locked".to_string())
        );
    }

    #[test]
    fn degrade_reasons_are_all_distinct_strings() {
        use DegradeReason::*;
        let all = [
            NoStore,
            StoreUnreadable,
            FreshnessUnknown,
            ModelUnavailable,
            PinsMismatch,
            StoreTooLarge,
            ResolveUnavailable,
            InvalidConfig,
        ];
        let strings: Vec<&str> = all.iter().map(|r| r.reason()).collect();
        for (i, a) in strings.iter().enumerate() {
            for b in strings.iter().skip(i + 1) {
                assert_ne!(a, b, "degrade reasons must be distinct");
            }
        }
    }

    #[test]
    fn corrupt_store_is_unreadable_not_absent() {
        // A decode/truncation error must NOT masquerade as "not built yet".
        assert_eq!(
            classify_store_error(&SeedStoreError::Truncated),
            DegradeReason::StoreUnreadable
        );
        assert_eq!(
            classify_store_error(&SeedStoreError::KeyMismatch),
            DegradeReason::PinsMismatch
        );
    }

    #[test]
    fn magic_mismatch_is_unreadable_not_pin_mismatch() {
        // review-9 #4: wrong magic = "not our file" (corrupt/foreign), NOT a
        // model/dim pin mismatch. Rendering it as PinsMismatch would tell the
        // operator to "rebuild with the same model" when the file is simply not
        // ours; StoreUnreadable is the honest cause.
        assert_eq!(
            classify_store_error(&SeedStoreError::MagicMismatch {
                expected: 0x5247_5356,
                found: 0xDEAD_BEEF,
            }),
            DegradeReason::StoreUnreadable
        );
    }

    #[test]
    fn schema_mismatch_is_pin_mismatch() {
        // A schema bump IS a pin mismatch (the format regime changed) — kept
        // distinct from the magic case above.
        assert_eq!(
            classify_store_error(&SeedStoreError::SchemaMismatch {
                expected: 1,
                found: 2,
            }),
            DegradeReason::PinsMismatch
        );
    }

    #[test]
    fn oversized_file_classifies_as_too_large() {
        assert_eq!(
            classify_store_error(&SeedStoreError::FileTooLarge {
                file_bytes: u64::MAX
            }),
            DegradeReason::StoreTooLarge
        );
    }
}
