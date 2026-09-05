//! The query-time semantic tier (spec §8/§8B): `run_semantic_query` — shared by
//! the `orient`/`explain` no-match fallback and the `find` verb. SEED-CHUNK-1: it
//! reads the current snapshot's stored chunk vectors from SQLite, hard-fails a model
//! pin mismatch, embeds the query with the in-process static model, ranks by cosine
//! with the production-above-test partition, and attaches each candidate's GENUINE
//! owning module.
//!
//! Every read that is rendered or classified reports **unknown-with-reason**
//! (STANDING HONESTY RULE): empty vectors = "no store yet"; a read error =
//! "unreadable"; a model/dim/checksum stamp differing from the runtime = "pins
//! mismatch"; an unreachable model = "model unavailable" — none collapses into
//! another. The checksum arm (review-1) is the SERVE-time guard that a model whose
//! bytes changed under the SAME id + dim cannot embed a query against vectors stored
//! by the OLD bytes.

use std::collections::HashMap;
use std::path::Path;

use repo_graph_agent::dto::envelope::ModuleHint;
use repo_graph_seed::document::build_query;
use repo_graph_seed::rank::{best_score, l2_normalize, rank};
use repo_graph_seed::{Embedder, SeedCorpusError, SeedCorpusRead, SeedVectorEntry};

use super::local_engine::{LocalEmbedder, ModelResolveError};
use super::{model_cache_dir, MODEL_DIM, MODEL_ID};

/// Why the semantic tier could not produce candidates (spec §8.3). Each maps to a
/// distinct honest reason string — none collapses into another. Not `Copy`:
/// [`ModelUnreadable`](Self::ModelUnreadable) carries the loader's true cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradeReason {
    /// No seed vectors for this snapshot yet — the async pass has not written them,
    /// or the snapshot predates the seed-vectors migration. A genuine absence.
    NoStore,
    /// The vector read failed (I/O / DB error), OR a stored vector blob was corrupt /
    /// the stored set was not homogeneous. Distinct from "not built yet": rows may
    /// exist but cannot be used.
    StoreUnreadable,
    /// The in-process embedding model was not cached and not fetchable (offline), or
    /// declined at embed time — seeding is optional, resolution is unaffected.
    ModelUnavailable,
    /// The model files ARE cached but the runtime failed to load/parse them (corrupt
    /// cache, unexpected layout) — a DIFFERENT cause from not-cached-not-fetchable
    /// (review-1 gap c; same rule as TYPE-ONLY's Unreadable-vs-absent). Carries the
    /// loader's true cause so the reader sees WHY, never a fabricated absence.
    ModelUnreadable { detail: String },
    /// The stored vectors' model/dim stamp differs from the runtime model (I3
    /// hard-fail) — they were built by a different embedding regime; rebuild on next
    /// index.
    PinsMismatch,
    /// SEED-CHUNK-2 (§2.4): the stored vectors predate per-chunk test/decl
    /// classification (migration 034). Refused rather than served as stale per-file
    /// fact — and the daemon SCHEDULES a background re-seed (self-heal), so this is a
    /// transient UPGRADE state that fixes itself, DISTINCT from
    /// [`StoreUnreadable`](Self::StoreUnreadable) (terminal corruption).
    SeedsReembedding,
    /// SEED-CHUNK-2 (review-2 item 2): the stored vectors predate per-chunk
    /// classification (as [`SeedsReembedding`](Self::SeedsReembedding)) BUT seeding is
    /// disabled (`RMAP_SEED_VECTORS` off), so NO re-seed is scheduled — nothing is
    /// "pending." A distinct, truthful terminal-until-re-enabled state: refusing to serve
    /// stale classification is still right, but it must not falsely claim a re-embed is in
    /// progress. Never collapsed into [`StoreUnreadable`](Self::StoreUnreadable) (that is
    /// corruption, not an opt-out) nor into [`SeedsReembedding`](Self::SeedsReembedding).
    SeedsStaleSeedingDisabled,
}

/// The degrade reason for a pre-034 (StaleClassification) store, chosen by whether
/// seeding is enabled. PURE so both branches are unit-tested without mutating the global
/// `seed_enabled()` override (which would race parallel unit tests) — the same test-seam
/// convention as [`degrade_for_resolve_error`] / [`serve_identity`] in this file.
///
/// Abstraction one-liner: pure mapper `stale_degrade_reason`; caller
/// [`run_semantic_query`]; axis: a config-free test seam for the enabled-vs-disabled
/// branch (a concrete honesty requirement, review-2 item 2); rejected simpler: inline
/// `if seed_enabled()` tested through the global override — racy across parallel unit
/// tests and untestable without process-global mutation.
fn stale_degrade_reason(seeding_enabled: bool) -> DegradeReason {
    if seeding_enabled {
        DegradeReason::SeedsReembedding
    } else {
        DegradeReason::SeedsStaleSeedingDisabled
    }
}

impl DegradeReason {
    /// The reader-facing reason (spec §8.3 `reasons[0]` text).
    pub fn reason(&self) -> String {
        match self {
            DegradeReason::NoStore => {
                "no seed vectors yet; they build in the background after indexing".to_string()
            }
            DegradeReason::StoreUnreadable => {
                "seed vectors present but unreadable; they rebuild on next index".to_string()
            }
            DegradeReason::ModelUnavailable => {
                "embedding model not cached and not fetchable; seeding is optional, resolution is unaffected".to_string()
            }
            DegradeReason::ModelUnreadable { detail } => {
                format!("cached embedding model unreadable: {detail}; it rebuilds on next index")
            }
            DegradeReason::PinsMismatch => {
                "seed vectors were built with a different model; rebuild on next index".to_string()
            }
            DegradeReason::SeedsReembedding => {
                "seeds re-embedding for per-chunk facts (pending)".to_string()
            }
            DegradeReason::SeedsStaleSeedingDisabled => {
                "seeds predate per-chunk facts and seeding is disabled; enable seeding (unset RMAP_SEED_VECTORS) to rebuild them".to_string()
            }
        }
    }
}

/// A resolved semantic candidate (the daemon maps this to the seam's DTO). Carries
/// the FIND-EVIDENCE-1 anchor material (`path:line` + qualified name) and the
/// is_test partition label.
#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub stable_key: String,
    pub path: String,
    /// The `path:line` anchor line; `None` renders WITHOUT a line (never a 0).
    pub line: Option<i64>,
    pub qualified_name: Option<String>,
    /// `true` ⇒ a DEMOTED test-classified chunk (labeled in rendering, spec §5).
    pub is_test: bool,
    /// SEED-CHUNK-2 (spec §2.2): `true` ⇒ a declaration without a body — ranked below
    /// its own implementation and labeled `(decl)` in rendering.
    pub is_decl: bool,
    /// Owning-module hint — a GENUINE module or explicit unavailable-with-reason.
    pub module: ModuleHint,
    pub score: f64,
    pub model_id: String,
}

/// The outcome of a semantic query — either candidates, a genuine known-zero (with
/// the honest best sub-floor score), or a labeled degraded reason (spec §8.3).
#[derive(Debug)]
pub enum SemanticResult {
    Fired {
        candidates: Vec<SemanticCandidate>,
        /// Always 0 in the per-snapshot model (a served snapshot's vectors are current
        /// by construction); kept for DTO stability with the fallback renderer.
        stale_count: usize,
        total: usize,
    },
    /// Query embedded but nothing scored above the zero floor. `best` is the highest
    /// sub-floor cosine (for the honest "(best: X)" line), `None` when there were no
    /// vectors to score.
    NothingScored { best: Option<f32> },
    /// The tier could not run; `reason` is the labeled cause.
    Unavailable(DegradeReason),
}

/// Resolve the owning-module hint for a file from a genuine ownership lookup result
/// (operator ruling 2026-08-25): a real module display path, an explicit "no
/// ownership recorded", or (on a lookup failure) an explicit lookup-failed reason.
fn module_hint(file_uid: &str, owners: &Result<HashMap<String, String>, String>) -> ModuleHint {
    match owners {
        Ok(map) => match map.get(file_uid) {
            Some(module) => ModuleHint::Owning(module.clone()),
            None => ModuleHint::Unavailable("no module ownership recorded".to_string()),
        },
        Err(e) => ModuleHint::Unavailable(format!("module ownership lookup failed: {e}")),
    }
}

/// The serve-time model-provenance decision (review-1). The `(model_id, dim)` pin is
/// necessary but NOT sufficient: a model whose BYTES changed under the SAME repo id
/// and dim would embed the query with the NEW regime and rank it against vectors
/// stored from the OLD bytes — a mixed regime and a false provenance claim. This pure
/// function decides, from the STORED stamp and the LIVE model's checksum, whether
/// serving is authorized — extracted (like [`module_hint`]) so its three honesty
/// outcomes are unit-tested WITHOUT a network model fetch.
///
/// Abstraction one-liner: pure decision helper / caller: [`run_semantic_query`] / axis:
/// a network-free test seam for the checksum gate / rejected simpler: assert inline and
/// test the whole function — impossible without fetching the real model (non-deterministic).
#[derive(Debug, PartialEq, Eq)]
enum ServeIdentity {
    /// Live model bytes equal the stored stamp — safe to rank.
    Match,
    /// The stored stamp is absent though vectors exist (impossible by construction —
    /// the read sets id/checksum/dim together): treat as corruption, render unreadable.
    StampMissing,
    /// Live model bytes differ from the stored stamp — a different embedding regime;
    /// render `PinsMismatch` (rebuild on next index), never rank.
    Mismatch,
}

/// Decide serve authorization from the stored checksum stamp and the live model's
/// checksum. `stored_checksum` is `None` only if the homogeneous stamp was somehow
/// unset while rows existed — surfaced, never silently treated as a match.
fn serve_identity(stored_checksum: Option<&str>, live_checksum: &str) -> ServeIdentity {
    match stored_checksum {
        None => ServeIdentity::StampMissing,
        Some(c) if c == live_checksum => ServeIdentity::Match,
        Some(_) => ServeIdentity::Mismatch,
    }
}

/// Map a model-resolution failure to its honest degraded reason (spec §8.3). The two
/// causes never collapse: not-cached-and-not-fetchable renders `ModelUnavailable`;
/// cached-but-unloadable renders `ModelUnreadable` carrying the loader's TRUE cause
/// (review-2 gap: the mapping `run_semantic_query` performs was previously inline and
/// untested). Extracted (like [`serve_identity`]/[`module_hint`]) so BOTH arms are
/// unit-tested WITHOUT a network model fetch.
///
/// Abstraction one-liner: pure mapping helper / caller: [`run_semantic_query`] / axis:
/// a network-free test seam for the ModelResolveError→DegradeReason mapping / rejected
/// simpler: keep the inline `match` — its `NotResolvable` arm cannot be reached
/// deterministically without either real network or an unearned endpoint-injection seam.
fn degrade_for_resolve_error(e: ModelResolveError) -> DegradeReason {
    match e {
        ModelResolveError::NotResolvable { .. } => DegradeReason::ModelUnavailable,
        ModelResolveError::Load { detail } => DegradeReason::ModelUnreadable { detail },
    }
}

/// Run the semantic tier for `snapshot_uid`'s stored chunk vectors. `storage` is the
/// live connection; `db_path` locates the state-root model cache; `top_n` is the
/// seam cap (≤5 for the fallback tier, ≤10 for `find`).
pub fn run_semantic_query<S>(
    storage: &S,
    snapshot_uid: &str,
    db_path: &Path,
    query: &str,
    top_n: usize,
) -> SemanticResult
where
    S: SeedCorpusRead + ?Sized,
{
    // Read this snapshot's stored vectors. An empty set is the ONLY "absent" case
    // (no vectors yet / pre-migration snapshot); a read error is present-but-unusable.
    let stored = match storage.read_seed_vectors(snapshot_uid) {
        Ok(s) => s,
        // SEED-CHUNK-2 §2.4: a pre-034 store is refused as StaleClassification. When
        // seeding is ENABLED it maps to the self-healing "re-embedding (pending)" state
        // (the daemon caller schedules the re-seed); when seeding is DISABLED nothing will
        // re-embed, so it maps to the distinct truthful "stale + disabled" state (review-2
        // item 2 — never a false "pending"). Genuine corruption stays StoreUnreadable.
        Err(SeedCorpusError::StaleClassification(_)) => {
            return SemanticResult::Unavailable(stale_degrade_reason(crate::seed::seed_enabled()))
        }
        Err(_) => return SemanticResult::Unavailable(DegradeReason::StoreUnreadable),
    };
    if stored.entries.is_empty() {
        return SemanticResult::Unavailable(DegradeReason::NoStore);
    }

    // I3 pin hard-fail: the stored model/dim stamp must equal the runtime model, else
    // the vectors are a different embedding regime and are discarded (never ranked
    // against a query embedded by a different model).
    let pins_ok = matches!(
        (stored.model_id.as_deref(), stored.dim),
        (Some(mid), Some(d)) if mid == MODEL_ID && d as usize == MODEL_DIM
    );
    if !pins_ok {
        return SemanticResult::Unavailable(DegradeReason::PinsMismatch);
    }

    // Load the in-process model (cache-first, else fetch-once), then embed the query.
    // A not-cached-and-not-fetchable model renders honestly absent WITH reason.
    let cache_dir = match model_cache_dir(db_path) {
        Some(c) => c,
        None => return SemanticResult::Unavailable(DegradeReason::ModelUnavailable),
    };
    // Not-cached-and-not-fetchable (offline / 404) → `ModelUnavailable` (specced
    // honest absence); cached-but-unloadable → `ModelUnreadable` with its TRUE cause
    // (gap c) — the mapping lives in the tested `degrade_for_resolve_error` helper so
    // neither cause silently collapses into the other.
    let embedder = match LocalEmbedder::load(&cache_dir) {
        Ok(e) => e,
        Err(e) => return SemanticResult::Unavailable(degrade_for_resolve_error(e)),
    };

    // Serve-time provenance gate (review-1): verify the LOADED model's checksum equals
    // the stored stamp BEFORE embedding — so a byte-changed model at the same id/dim
    // returns here and NEVER reaches `rank` below. Computing the checksum re-hashes the
    // model files once per query; this cost is accepted on the (sub-wall, guess) seed
    // tier for provenance honesty (the local_engine doc-comment's "find never pays this
    // hash" no longer holds — corrected there).
    let live_ck = match embedder.model_checksum() {
        Ok(c) => c,
        // The model loaded but its files became unreadable while we hash them for
        // provenance (TOCTOU delete / permission change). We cannot confirm the regime,
        // so we abstain with the TRUE cause rather than serve an unverified regime.
        Err(e) => {
            return SemanticResult::Unavailable(DegradeReason::ModelUnreadable {
                detail: format!("model files unreadable while verifying provenance: {e}"),
            })
        }
    };
    match serve_identity(stored.model_checksum.as_deref(), &live_ck) {
        ServeIdentity::Match => {}
        ServeIdentity::StampMissing => {
            return SemanticResult::Unavailable(DegradeReason::StoreUnreadable)
        }
        ServeIdentity::Mismatch => return SemanticResult::Unavailable(DegradeReason::PinsMismatch),
    }

    let mut qv = match embedder.embed(&[build_query(query)]) {
        Ok(mut v) => match v.pop() {
            Some(v) => v,
            None => return SemanticResult::Unavailable(DegradeReason::ModelUnavailable),
        },
        Err(_) => return SemanticResult::Unavailable(DegradeReason::ModelUnavailable),
    };
    l2_normalize(&mut qv);

    let refs: Vec<&SeedVectorEntry> = stored.entries.iter().collect();
    let ranked = rank(&qv, &refs, top_n);
    if ranked.is_empty() {
        // Nothing above the zero floor — honest known-zero with the best sub-floor
        // score for the "(best: X)" line (spec §4 floor honesty).
        return SemanticResult::NothingScored {
            best: best_score(&qv, &refs),
        };
    }

    // Genuine owning-module resolution: ONE batch lookup over the candidates' files.
    // A lookup FAILURE is carried as an explicit unavailable-with-reason per
    // candidate — the candidates are still valid, so we do not drop them.
    let file_uids: Vec<String> = ranked.iter().map(|r| r.file_uid.clone()).collect();
    let owners = storage
        .module_owners(snapshot_uid, &file_uids)
        .map_err(|e| e.to_string());

    let out: Vec<SemanticCandidate> = ranked
        .into_iter()
        .map(|r| SemanticCandidate {
            module: module_hint(&r.file_uid, &owners),
            stable_key: r.stable_key,
            path: r.path,
            line: r.line,
            qualified_name: r.qualified_name,
            is_test: r.is_test,
            is_decl: r.is_decl,
            score: r.score as f64,
            model_id: MODEL_ID.to_string(),
        })
        .collect();

    SemanticResult::Fired {
        candidates: out,
        stale_count: 0,
        total: stored.entries.len(),
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
            ModelUnavailable,
            ModelUnreadable {
                detail: "safetensors parse error".to_string(),
            },
            PinsMismatch,
            SeedsReembedding,
            SeedsStaleSeedingDisabled,
        ];
        let strings: Vec<String> = all.iter().map(|r| r.reason()).collect();
        for (i, a) in strings.iter().enumerate() {
            for b in strings.iter().skip(i + 1) {
                assert_ne!(a, b, "degrade reasons must be distinct");
            }
        }
    }

    #[test]
    fn serve_identity_gate_rejects_a_changed_checksum_under_the_same_id() {
        // review-1 point 1: same model id/dim, but the model's BYTES changed (a new
        // checksum). The serve gate must classify this as Mismatch so `run_semantic_query`
        // returns `Unavailable(PinsMismatch)` BEFORE `rank` — no candidate is ever scored
        // against vectors embedded by a different regime.
        assert_eq!(
            serve_identity(Some("sha256:AAA"), "sha256:AAA"),
            ServeIdentity::Match,
            "identical checksum authorizes serving"
        );
        assert_eq!(
            serve_identity(Some("sha256:AAA"), "sha256:BBB"),
            ServeIdentity::Mismatch,
            "a changed checksum under the same id/dim must NOT rank"
        );
        assert_eq!(
            serve_identity(None, "sha256:AAA"),
            ServeIdentity::StampMissing,
            "an absent stamp with rows present is surfaced, never treated as a match"
        );
    }

    #[test]
    fn model_unreadable_carries_true_cause_and_is_not_the_not_cached_string() {
        // gap c: a cached-but-unloadable model renders its TRUE cause, DISTINCT from
        // the not-cached-not-fetchable absence — never collapsed into it.
        let unreadable = DegradeReason::ModelUnreadable {
            detail: "safetensors: header too small".to_string(),
        }
        .reason();
        assert!(
            unreadable.contains("cached embedding model unreadable")
                && unreadable.contains("safetensors: header too small"),
            "true cause surfaced: {unreadable}"
        );
        assert_ne!(
            unreadable,
            DegradeReason::ModelUnavailable.reason(),
            "unreadable must not collapse into not-cached-not-fetchable"
        );
    }

    #[test]
    fn degrade_for_resolve_error_maps_both_causes_distinctly() {
        // review-2 point 2: the ACTUAL mapping `run_semantic_query` performs on a
        // `ModelResolveError`, tested directly (not by hand-building a `DegradeReason`).
        // NotResolvable (not cached / not fetchable) → ModelUnavailable.
        assert_eq!(
            degrade_for_resolve_error(ModelResolveError::NotResolvable {
                detail: "connection refused".to_string(),
            }),
            DegradeReason::ModelUnavailable,
            "not-cached-and-not-fetchable maps to the optional-absence reason",
        );
        // Load (cached bytes present but unloadable) → ModelUnreadable carrying the
        // loader's TRUE cause verbatim — never collapsed into the not-cached string.
        assert_eq!(
            degrade_for_resolve_error(ModelResolveError::Load {
                detail: "safetensors: header too small".to_string(),
            }),
            DegradeReason::ModelUnreadable {
                detail: "safetensors: header too small".to_string(),
            },
            "cached-but-unloadable preserves the loader's true detail",
        );
    }

    /// A fake `SeedCorpusRead` whose vector read reports a pre-034 (StaleClassification)
    /// store — the SEED-CHUNK-2 §2.4 self-heal trigger. `run_semantic_query` reads the
    /// vectors FIRST, so this returns before any model load (no cache needed).
    struct StaleStore;
    impl SeedCorpusRead for StaleStore {
        fn seed_corpus(
            &self,
            _repo_uid: &str,
        ) -> Result<repo_graph_seed::SeedCorpus, SeedCorpusError> {
            unreachable!("run_semantic_query never reads the corpus")
        }
        fn read_seed_vectors(
            &self,
            _snapshot_uid: &str,
        ) -> Result<repo_graph_seed::StoredSeedVectors, SeedCorpusError> {
            Err(SeedCorpusError::StaleClassification(
                "node n predates per-chunk classification (migration 034)".to_string(),
            ))
        }
        fn module_owners(
            &self,
            _snapshot_uid: &str,
            _file_uids: &[String],
        ) -> Result<HashMap<String, String>, SeedCorpusError> {
            Ok(HashMap::new())
        }
    }

    #[test]
    fn pre_034_store_maps_to_seeds_reembedding_not_store_unreadable() {
        // SEED-CHUNK-2 §2.4: a StaleClassification read is the self-healing upgrade state
        // (SeedsReembedding, "re-embedding (pending)"), NEVER collapsed into the terminal
        // StoreUnreadable ("rebuild on next index") — the two carry different truths and
        // the daemon schedules a re-seed only for the former.
        let result = run_semantic_query(
            &StaleStore,
            "snap1",
            Path::new("/x/databases/a.db"),
            "crash recovery",
            5,
        );
        match result {
            SemanticResult::Unavailable(DegradeReason::SeedsReembedding) => {}
            other => panic!("pre-034 store must map to SeedsReembedding, got {other:?}"),
        }
        assert_ne!(
            DegradeReason::SeedsReembedding.reason(),
            DegradeReason::StoreUnreadable.reason(),
            "the self-healing state must not read as terminal corruption"
        );
    }

    #[test]
    fn stale_degrade_reason_tracks_whether_seeding_is_enabled() {
        // review-2 item 2: a pre-034 store maps to the self-healing "(pending)" state ONLY
        // when seeding is enabled (a re-seed IS scheduled). With seeding disabled nothing
        // re-embeds, so it maps to the distinct disabled state — tested via the pure mapper
        // so neither branch mutates the process-global `seed_enabled()` override.
        assert_eq!(
            stale_degrade_reason(true),
            DegradeReason::SeedsReembedding,
            "seeding enabled ⇒ a re-seed is scheduled ⇒ pending is truthful",
        );
        assert_eq!(
            stale_degrade_reason(false),
            DegradeReason::SeedsStaleSeedingDisabled,
            "seeding disabled ⇒ nothing re-embeds ⇒ NOT pending",
        );
    }

    #[test]
    fn disabled_stale_state_never_claims_pending_and_is_distinct() {
        // The disabled render must not contain "pending" / "re-embedding" (there is no
        // re-embed in flight), and must be distinct from both the self-healing state and
        // terminal corruption — it is an opt-out, not a failure.
        let disabled = DegradeReason::SeedsStaleSeedingDisabled.reason();
        assert!(
            !disabled.contains("pending") && !disabled.contains("re-embedding"),
            "disabled state must not falsely claim a re-embed: {disabled}",
        );
        assert_ne!(disabled, DegradeReason::SeedsReembedding.reason());
        assert_ne!(disabled, DegradeReason::StoreUnreadable.reason());
    }

    /// A fake `SeedCorpusRead` returning ONE matching-pin vector, so `run_semantic_query`
    /// clears the pin gate and reaches the real `LocalEmbedder::load` — letting the
    /// end-to-end test below drive the product's model-load degrade branch.
    struct OneMatchingPinVector;
    impl SeedCorpusRead for OneMatchingPinVector {
        fn seed_corpus(
            &self,
            _repo_uid: &str,
        ) -> Result<repo_graph_seed::SeedCorpus, repo_graph_seed::SeedCorpusError> {
            unreachable!("run_semantic_query never reads the corpus")
        }
        fn read_seed_vectors(
            &self,
            _snapshot_uid: &str,
        ) -> Result<repo_graph_seed::StoredSeedVectors, repo_graph_seed::SeedCorpusError> {
            Ok(repo_graph_seed::StoredSeedVectors {
                model_id: Some(MODEL_ID.to_string()),
                model_checksum: Some("sha256:stored".to_string()),
                dim: Some(MODEL_DIM as u32),
                entries: vec![SeedVectorEntry {
                    node_uid: "n".to_string(),
                    stable_key: "k".to_string(),
                    file_uid: "f".to_string(),
                    path: "a.rs".to_string(),
                    line: Some(1),
                    qualified_name: Some("q".to_string()),
                    is_test: false,
                    is_decl: false,
                    content_hash: "h".to_string(),
                    vector: vec![0.0; MODEL_DIM],
                }],
            })
        }
        fn module_owners(
            &self,
            _snapshot_uid: &str,
            _file_uids: &[String],
        ) -> Result<HashMap<String, String>, repo_graph_seed::SeedCorpusError> {
            Ok(HashMap::new())
        }
    }

    /// End-to-end (review-2 point 2): drive the REAL `run_semantic_query` through the
    /// real `LocalEmbedder::load` cached-but-unloadable branch — NO network. We plant a
    /// well-formed hf-hub cache layout (refs/main + snapshots/<hash>/<files>) whose model
    /// files are GARBAGE, so `resolve_model_dir` takes the cache-hit path (no download)
    /// and `StaticModel::from_pretrained` fails to parse → `ModelResolveError::Load` →
    /// the product must render `Unavailable(ModelUnreadable)` with the loader's true
    /// cause, never `ModelUnavailable` and never a panic.
    #[test]
    fn run_semantic_query_renders_cached_unreadable_model_with_true_cause() {
        let state_root = tempfile::tempdir().expect("temp state root");
        // db_path = <state_root>/databases/x.db, so model_cache_dir = <state_root>/seed-model-cache.
        let db_dir = state_root.path().join("databases");
        std::fs::create_dir_all(&db_dir).expect("db dir");
        let db_path = db_dir.join("x.db");

        // Plant the corrupt cache in the exact hf-hub 0.4.3 layout so `get()` is a cache
        // HIT (no network): <cache>/models--minishlab--potion-code-16M-v2/refs/main holds
        // a commit hash; snapshots/<hash>/ holds the (garbage) model files.
        let cache = state_root.path().join("seed-model-cache");
        let repo = cache.join("models--minishlab--potion-code-16M-v2");
        std::fs::create_dir_all(repo.join("refs")).expect("refs dir");
        std::fs::write(repo.join("refs").join("main"), b"deadbeefcafe").expect("ref");
        let snap = repo.join("snapshots").join("deadbeefcafe");
        std::fs::create_dir_all(&snap).expect("snapshot dir");
        for f in ["config.json", "tokenizer.json", "model.safetensors"] {
            std::fs::write(snap.join(f), b"not a real model file").expect("garbage file");
        }

        let result = run_semantic_query(
            &OneMatchingPinVector,
            "snap1",
            &db_path,
            "crash recovery",
            5,
        );
        match result {
            SemanticResult::Unavailable(DegradeReason::ModelUnreadable { detail }) => {
                assert!(
                    !detail.is_empty(),
                    "the loader's true cause must be surfaced, not an empty/fabricated reason"
                );
            }
            other => panic!(
                "cached-but-unloadable model must render ModelUnreadable with its cause, got {other:?}"
            ),
        }
    }
}
