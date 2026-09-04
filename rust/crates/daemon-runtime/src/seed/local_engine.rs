//! SEED-CHUNK-1 — the in-process static-embedding engine: `potion-code-16M-v2`
//! (`minishlab/potion-code-16M-v2`) via `model2vec-rs`, the ratified replacement
//! for the lmstudio HTTP embedder in the `find` seed tier.
//!
//! ## Where this sits (architecture Rule 3)
//! This is the IMPURE model-runtime edge. It implements the pure
//! [`repo_graph_seed::ports::Embedder`] port that the pure `repo-graph-seed` crate
//! defines; the pure crate never sees `model2vec-rs`, `hf-hub`, safetensors, or a
//! filesystem path. Only `Vec<Vec<f32>>` crosses the port boundary — the same
//! [`Embedder`](repo_graph_seed::ports::Embedder) contract the now-deleted lmstudio
//! HTTP embedder honored, served in-process instead of over a loopback socket.
//!
//! Abstraction one-liner: `LocalEmbedder` — the second concrete `Embedder` impl
//! (the port's own doc names "embedded … second impl" as the ratification-pending
//! growth axis; SEED-CHUNK-1 ratifies it). Concrete current user (SEED-CHUNK-1
//! phase 2, not yet wired): the daemon seed pass, which will construct it once per
//! embed pass and hand it to `repo_graph_seed::pass::build_store`. Axis of
//! variation: the D-ES-4 model-runtime distribution choice (endpoint → in-process).
//! Rejected simpler: keep the HTTP endpoint — the spike measured the endpoint path
//! as the source of 20-min offline latency + silent-absence when lmstudio is down
//! (`docs/audits/2026-09-03-seed-chunk-spike-1.md`).
//!
//! ## Model resolution + state isolation (operator ruling DEP-HFHUB-EDGE, 2026-09-03)
//! The model is resolved through a DIRECT `hf-hub` edge into an explicit cache dir
//! UNDER the app state root, then `model2vec-rs` is handed the LOCAL PATH.
//! `model2vec-rs`'s own `from_pretrained("<repo-id>")` resolver builds `hf_hub::
//! Api::new()`, which caches in the global `~/.cache/huggingface` — that would make
//! proofs and the product share a cache OUTSIDE the state root and let
//! `dogfood-isolated` silently depend on the operator's home. Controlling the cache
//! dir ourselves is the smallest evidenced path to the state-isolation invariant.
//!
//! ## Honest absence (STANDING HONESTY RULE 1)
//! Resolution is cache-first, else fetch-once. When the model is neither cached nor
//! fetchable (offline, no cache), [`LocalEmbedder::load`] returns
//! [`ModelResolveError::NotResolvable`] whose [`reason`](ModelResolveError::reason)
//! is the exact spec §2 string — the seed tier then renders honestly absent WITH
//! its reason. Never a panic, never a silent empty, never a stale fallback.

use std::path::{Path, PathBuf};

use hf_hub::api::sync::ApiBuilder;
use model2vec_rs::model::StaticModel;
use repo_graph_seed::ports::{EmbedError, Embedder};
use sha2::{Digest, Sha256};

/// The ratified seed model id (spec §2). Operator-asserted; it becomes the store
/// pin's `model_id`, so a model change invalidates prior vectors (rebuild
/// semantics — the zg precedent, spec §3).
pub const MODEL_ID: &str = "minishlab/potion-code-16M-v2";

/// The model's embedding dimension. MEASURED in-process against the real model
/// (the SEED-CHUNK-1 run gate: `dim=256`, `first_norm=1.0000`), NOT copied from a
/// doc — potion-code-16M-v2 emits 256-dim L2-normalized vectors. This is the store
/// pin's `dim`; any returned vector of a different length is a hard
/// [`EmbedError::DimMismatch`] (never padded/truncated).
pub const MODEL_DIM: usize = 256;

/// The exact files `model2vec-rs` resolves for the model2vec (potion) layout,
/// mirrored so our hf-hub fetch lands the identical set into our cache dir. Verified
/// against `model2vec-rs-0.2.1::match_hub_layout` (config.json + tokenizer.json +
/// model.safetensors at the repo root; empty prefix).
const MODEL_FILES: &[&str] = &["config.json", "tokenizer.json", "model.safetensors"];

/// Every way model resolution/loading can fail — each maps to an honest degraded
/// state (STANDING HONESTY RULE 1). The engine DECLINES; it never guesses and never
/// crashes the index.
#[derive(Debug)]
pub enum ModelResolveError {
    /// The model is neither in the state-root cache nor fetchable (offline / HF
    /// unreachable / 404). The seed tier renders honestly absent with
    /// [`reason`](Self::reason) — the specced "not cached and not fetchable" state.
    NotResolvable { detail: String },
    /// The files resolved but the runtime failed to load/parse them (corrupt cache,
    /// unexpected layout). Distinct from unreachable: the bytes are here but unusable.
    Load { detail: String },
}

impl ModelResolveError {
    /// The reader-facing reason (spec §2 / VISION "labels speak the reader's
    /// language"). `NotResolvable` is the exact specced string so every seed surface
    /// renders one honest cause.
    pub fn reason(&self) -> String {
        match self {
            ModelResolveError::NotResolvable { .. } => {
                "embedding model not cached and not fetchable".to_string()
            }
            ModelResolveError::Load { detail } => {
                format!("embedding model failed to load: {detail}")
            }
        }
    }
}

impl std::fmt::Display for ModelResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelResolveError::NotResolvable { detail } => {
                write!(f, "embedding model not cached and not fetchable ({detail})")
            }
            ModelResolveError::Load { detail } => {
                write!(f, "embedding model failed to load: {detail}")
            }
        }
    }
}

impl std::error::Error for ModelResolveError {}

/// The in-process `Embedder`: a loaded `model2vec-rs` static model plus the pins it
/// stamps into the store. Cheap to `embed` (no I/O, no network) once loaded.
pub struct LocalEmbedder {
    model: StaticModel,
    model_id: String,
    dim: usize,
    /// The resolved local snapshot directory the model files live in. Retained so
    /// [`model_checksum`](Self::model_checksum) can hash the files on demand WITHOUT
    /// re-resolving through hf-hub. Read by the background pass (spec §2 provenance
    /// stamp) AND, since review-1, by the serve path's provenance gate — the `embed`
    /// hot loop itself never touches it.
    model_dir: PathBuf,
}

impl LocalEmbedder {
    /// Resolve the model into `cache_dir` (cache-first, else fetch-once) and load it
    /// in-process. `cache_dir` MUST be under the app state root (e.g.
    /// `<state_root>/seed-model-cache`) so proofs and the product never share HF's
    /// global home (state-isolation invariant, operator ruling DEP-HFHUB-EDGE).
    ///
    /// The dim is verified against [`MODEL_DIM`] with a real single-token probe so a
    /// silently-swapped model (a different snapshot at the same repo id) can never
    /// stamp a wrong pin onto the store.
    pub fn load(cache_dir: &Path) -> Result<Self, ModelResolveError> {
        let dir = resolve_model_dir(cache_dir, MODEL_ID)?;
        // `from_pretrained` on an EXISTING local dir takes the local branch (no
        // network, no global cache) and reads the model2vec layout we just fetched.
        // `normalize = None` honors the model config (potion normalizes → unit norm),
        // matching the run-gate measurement.
        let model = StaticModel::from_pretrained(&dir, None, None, None).map_err(|e| {
            ModelResolveError::Load {
                detail: e.to_string(),
            }
        })?;
        // Probe the real output dim once; refuse a model whose geometry is not the
        // pinned 256 rather than stamp a wrong `dim` onto every stored vector.
        let probe = model.encode(&["probe".to_string()]);
        match probe.first() {
            Some(v) if v.len() == MODEL_DIM => {}
            Some(v) => {
                return Err(ModelResolveError::Load {
                    detail: format!("model emits dim {}, expected {MODEL_DIM}", v.len()),
                })
            }
            None => {
                return Err(ModelResolveError::Load {
                    detail: "model produced no probe vector".to_string(),
                })
            }
        }
        Ok(Self {
            model,
            model_id: MODEL_ID.to_string(),
            dim: MODEL_DIM,
            model_dir: dir,
        })
    }

    /// The sha256 provenance checksum of the model's files (spec §2 "checksum
    /// recorded"), computed on demand from the already-resolved [`model_dir`] — the
    /// background pass stamps it into every row it writes so the store records WHICH
    /// model bytes produced it. It hashes the [`MODEL_FILES`] set in a FIXED order,
    /// each domain-separated by `name\0len\0`, into one hex digest — a byte change in
    /// any file (a silently-swapped model at the same repo id) yields a different
    /// digest. The stored SERVE stamp is `(model_id, checksum, dim)`; the pass writes
    /// all three and the serve path re-verifies the checksum (see the caller note below).
    ///
    /// An I/O failure reading a file we JUST loaded is a genuine fault (TOCTOU
    /// deletion / permission change) — surfaced honestly to the caller, which skips
    /// the pass with the reason, never a fabricated/empty checksum.
    ///
    /// SERVE-PATH CALLER (review-1): the earlier claim that "find never pays this hash"
    /// no longer holds. `run_semantic_query` also calls this, once per query, to verify
    /// the live model's checksum equals the stored stamp before ranking — the `(model_id,
    /// dim)` pin cannot catch a byte-changed model at the same id. The per-query re-hash
    /// is accepted on the sub-wall guess tier for provenance honesty (correct over fast).
    pub fn model_checksum(&self) -> std::io::Result<String> {
        let mut hasher = Sha256::new();
        for name in MODEL_FILES {
            let bytes = std::fs::read(self.model_dir.join(name))?;
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(b"\0");
            hasher.update(&bytes);
        }
        Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
    }
}

/// Resolve the model's snapshot directory, fetching the model2vec file set into
/// `cache_dir` via the DIRECT hf-hub edge. `get()` is cache-first: a cached file
/// returns its local path with no download; a miss fetches once into `cache_dir`.
/// ANY resolution failure (offline + uncached, 404, I/O, lock) is a single honest
/// [`ModelResolveError::NotResolvable`] — never a panic, never a silent empty
/// (STANDING HONESTY RULE 1).
fn resolve_model_dir(cache_dir: &Path, repo_id: &str) -> Result<PathBuf, ModelResolveError> {
    let api = ApiBuilder::new()
        .with_progress(false)
        .with_cache_dir(cache_dir.to_path_buf())
        .with_token(None)
        .build()
        .map_err(|e| ModelResolveError::NotResolvable {
            detail: e.to_string(),
        })?;
    let repo = api.model(repo_id.to_string());

    // Fetch every required file; they all land in the same snapshot dir, so its
    // parent is stable across the set. We hand model2vec that dir (not a file).
    let mut snapshot_dir: Option<PathBuf> = None;
    for file in MODEL_FILES {
        let path = repo
            .get(file)
            .map_err(|e| ModelResolveError::NotResolvable {
                detail: format!("{file}: {e}"),
            })?;
        let parent = path
            .parent()
            .ok_or_else(|| ModelResolveError::NotResolvable {
                detail: format!("{file}: resolved path has no parent directory"),
            })?
            .to_path_buf();
        snapshot_dir = Some(parent);
    }
    snapshot_dir.ok_or_else(|| ModelResolveError::NotResolvable {
        detail: "no model files configured".to_string(),
    })
}

impl Embedder for LocalEmbedder {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let vecs = self.model.encode(texts);
        // One vector per input, each of the pinned dim, all finite. A deviation is a
        // real engine fault surfaced honestly — NEVER padded, truncated, or dropped
        // (which would serve a partial/degenerate result under the guise of a full one).
        if vecs.len() != texts.len() {
            return Err(EmbedError::Malformed {
                detail: format!(
                    "engine returned {} vectors for {} inputs",
                    vecs.len(),
                    texts.len()
                ),
            });
        }
        for v in &vecs {
            if v.len() != self.dim {
                return Err(EmbedError::DimMismatch {
                    expected: self.dim,
                    got: v.len(),
                });
            }
            if v.iter().any(|x| !x.is_finite()) {
                return Err(EmbedError::Malformed {
                    detail: "non-finite component in embedding".to_string(),
                });
            }
        }
        Ok(vecs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_pins_match_the_measured_potion_geometry() {
        // The run-gate measurement (`dim=256`) is the store pin. If a future model
        // swap changes this, the pin — and thus vector invalidation — must be
        // reconsidered deliberately, so it is asserted here rather than left implicit.
        assert_eq!(MODEL_DIM, 256);
        assert_eq!(MODEL_ID, "minishlab/potion-code-16M-v2");
        assert_eq!(MODEL_FILES.len(), 3);
    }

    #[test]
    fn resolve_error_reason_is_the_specced_honest_string() {
        let e = ModelResolveError::NotResolvable {
            detail: "connection refused".to_string(),
        };
        assert_eq!(e.reason(), "embedding model not cached and not fetchable");
    }

    /// Live in-process load + embed against the real model. `#[ignore]` because it
    /// fetches ~30 MB from HuggingFace on a cold cache; run explicitly with
    /// `cargo test -p repo-graph-daemon-runtime --lib -- --ignored local_engine`.
    /// Uses an isolated temp cache dir — never the operator's HF home or state root.
    #[test]
    #[ignore = "network: fetches potion-code-16M-v2 from HuggingFace"]
    fn live_load_and_embed_produces_pinned_dim_unit_vectors() {
        let tmp = tempfile::tempdir().expect("temp cache dir");
        let embedder = LocalEmbedder::load(tmp.path()).expect("load model into isolated cache");
        assert_eq!(embedder.dim(), MODEL_DIM);
        assert_eq!(embedder.model_id(), MODEL_ID);

        let texts = vec![
            "recover from crash by replaying the write-ahead log".to_string(),
            "delete obsolete sstable files during compaction".to_string(),
        ];
        let vecs = embedder.embed(&texts).expect("embed");
        assert_eq!(vecs.len(), 2);
        for v in &vecs {
            assert_eq!(v.len(), MODEL_DIM);
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(
                (norm - 1.0).abs() < 1e-2,
                "potion emits L2-normalized vectors; got norm {norm}"
            );
        }
        // A second load from the now-warm cache must NOT hit the network (cache-first).
        let again = LocalEmbedder::load(tmp.path()).expect("warm-cache reload");
        assert_eq!(again.dim(), MODEL_DIM);
    }
}
