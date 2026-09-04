//! Doctor-facing snapshot of the seed store's state (spec §9), computed directly
//! from the current snapshot's stored vectors + config — self-contained, no daemon
//! field. SEED-CHUNK-1: vectors live in the per-snapshot `seed_vectors` table and
//! the embedder is the in-process static model, so there is no endpoint probe and
//! no `.vec` sidecar read.
//!
//! Every read that is rendered reports unknown-with-reason (STANDING HONESTY RULE):
//! no vectors for the latest snapshot ⇒ `absent`; a read error ⇒ `degraded`
//! (unreadable); a stored model/dim stamp differing from the runtime ⇒ `degraded`
//! (pins mismatch) — never a fabricated present/measured-zero.

use std::path::Path;

use repo_graph_seed::SeedCorpusRead;

use super::query::DegradeReason;
use super::{MODEL_DIM, MODEL_ID};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SeedDoctorFacts {
    /// "present" | "absent" (not indexed / not seeded yet) | "building" (a pass is
    /// in flight) | "degraded".
    pub state: String,
    pub model_id: String,
    pub dim: usize,
    /// The embedder provenance. SEED-CHUNK-1 serves from an in-process static model,
    /// so this is a factual statement of WHICH model — never a liveness claim (that
    /// is surfaced at serve time as `ModelUnavailable` if the model is uncached).
    pub model_identity: String,
    /// The sha256 checksum stamped on the CURRENT snapshot's stored vectors (spec §2
    /// "checksum recorded") — the recorded provenance of the bytes that produced them.
    /// `None` unless a `present` set was read (absent/degraded leave it `None`), never
    /// a fabricated value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_checksum: Option<String>,
    /// Total stored vectors for the current snapshot. `None` unless a set was read
    /// (absent/degraded leave it `None`, rendered `null` — never a fabricated zero).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// Present only when degraded — the honest cause.
    pub degraded_reason: Option<String>,
}

/// Compute the doctor facts for a repo's seed store (spec §9).
///
/// `is_building` is the coordinator's live "a pass is running for this db_path"
/// signal — when set, the state is `building` (a pass is actively (re)computing),
/// carrying whatever prior counts a published set still has.
pub fn seed_doctor_facts<S>(
    storage: &S,
    repo_uid: &str,
    _db_path: &Path,
    is_building: bool,
) -> SeedDoctorFacts
where
    S: SeedCorpusRead + ?Sized,
{
    let facts = store_facts(storage, repo_uid);
    // A running pass overrides the store-derived label: "building" is the honest
    // current state (the stored vectors, if any, are the PRIOR generation).
    if is_building {
        return SeedDoctorFacts {
            state: "building".to_string(),
            degraded_reason: None,
            ..facts
        };
    }
    facts
}

fn base() -> SeedDoctorFacts {
    SeedDoctorFacts {
        state: "absent".to_string(),
        model_id: MODEL_ID.to_string(),
        dim: MODEL_DIM,
        model_identity: format!("in-process static model ({MODEL_ID})"),
        total: None,
        model_checksum: None,
        degraded_reason: None,
    }
}

/// The store-derived facts (present / absent / degraded), before the `building`
/// override. No latest snapshot / no vectors ⇒ `absent`; a read error ⇒ `degraded`
/// (unreadable); a model/dim stamp mismatch ⇒ `degraded` (pins mismatch).
fn store_facts<S>(storage: &S, repo_uid: &str) -> SeedDoctorFacts
where
    S: SeedCorpusRead + ?Sized,
{
    let corpus = match storage.seed_corpus(repo_uid) {
        Ok(c) => c,
        Err(e) => return degraded(format!("cannot resolve corpus: {e}")),
    };
    let snapshot_uid = match corpus.snapshot_uid {
        Some(s) => s,
        None => return base(), // not indexed → absent
    };
    let stored = match storage.read_seed_vectors(&snapshot_uid) {
        Ok(s) => s,
        Err(_) => return degraded(DegradeReason::StoreUnreadable.reason()),
    };
    if stored.entries.is_empty() {
        return base(); // indexed but not seeded yet → absent
    }
    // Pin check: a different model/dim stamp is a degraded pin mismatch.
    let pins_ok = matches!(
        (stored.model_id.as_deref(), stored.dim),
        (Some(mid), Some(d)) if mid == MODEL_ID && d as usize == MODEL_DIM
    );
    if !pins_ok {
        return degraded(DegradeReason::PinsMismatch.reason());
    }
    SeedDoctorFacts {
        state: "present".to_string(),
        total: Some(stored.entries.len()),
        // Surface the recorded provenance checksum of the served set (spec §2). The
        // homogeneity-validated read guarantees one stamp for the whole set.
        model_checksum: stored.model_checksum,
        ..base()
    }
}

/// Build a `degraded` facts record from the `absent` base + an honest reason.
fn degraded(reason: String) -> SeedDoctorFacts {
    SeedDoctorFacts {
        state: "degraded".to_string(),
        degraded_reason: Some(reason),
        ..base()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_seed::{SeedCorpus, SeedCorpusError, SeedVectorEntry, StoredSeedVectors};
    use std::collections::HashMap;

    /// A fake corpus/vector read driven by the test — the doctor is pure over these
    /// two reads, so the state classification is the only thing under test (no DB).
    struct FakeStore {
        snapshot: Option<String>,
        vectors: Result<StoredSeedVectors, ()>,
    }
    impl SeedCorpusRead for FakeStore {
        fn seed_corpus(&self, _repo_uid: &str) -> Result<SeedCorpus, SeedCorpusError> {
            Ok(SeedCorpus {
                snapshot_uid: self.snapshot.clone(),
                entries: vec![],
            })
        }
        fn read_seed_vectors(
            &self,
            _snapshot_uid: &str,
        ) -> Result<StoredSeedVectors, SeedCorpusError> {
            self.vectors
                .clone()
                .map_err(|_| SeedCorpusError::Read("io".to_string()))
        }
        fn module_owners(
            &self,
            _snapshot_uid: &str,
            _file_uids: &[String],
        ) -> Result<HashMap<String, String>, SeedCorpusError> {
            Ok(HashMap::new())
        }
    }

    fn one_vector(model_id: &str, dim: u32) -> StoredSeedVectors {
        StoredSeedVectors {
            model_id: Some(model_id.to_string()),
            model_checksum: Some("sha256:test".to_string()),
            dim: Some(dim),
            entries: vec![SeedVectorEntry {
                node_uid: "n".to_string(),
                stable_key: "k".to_string(),
                file_uid: "f".to_string(),
                path: "a.rs".to_string(),
                line: Some(1),
                qualified_name: Some("q".to_string()),
                is_test: false,
                content_hash: "h".to_string(),
                vector: vec![1.0; dim as usize],
            }],
        }
    }

    #[test]
    fn not_indexed_is_absent() {
        let s = FakeStore {
            snapshot: None,
            vectors: Ok(StoredSeedVectors {
                model_id: None,
                model_checksum: None,
                dim: None,
                entries: vec![],
            }),
        };
        let f = seed_doctor_facts(&s, "r", Path::new("/x/databases/a.db"), false);
        assert_eq!(f.state, "absent");
        assert!(f.total.is_none());
    }

    #[test]
    fn indexed_but_no_vectors_is_absent() {
        let s = FakeStore {
            snapshot: Some("s1".to_string()),
            vectors: Ok(StoredSeedVectors {
                model_id: None,
                model_checksum: None,
                dim: None,
                entries: vec![],
            }),
        };
        let f = seed_doctor_facts(&s, "r", Path::new("/x/databases/a.db"), false);
        assert_eq!(f.state, "absent", "seeded-yet-empty is absent, not present");
    }

    #[test]
    fn matching_pin_is_present_with_count() {
        let s = FakeStore {
            snapshot: Some("s1".to_string()),
            vectors: Ok(one_vector(MODEL_ID, MODEL_DIM as u32)),
        };
        let f = seed_doctor_facts(&s, "r", Path::new("/x/databases/a.db"), false);
        assert_eq!(f.state, "present");
        assert_eq!(f.total, Some(1));
    }

    #[test]
    fn different_model_is_degraded_pins_mismatch() {
        let s = FakeStore {
            snapshot: Some("s1".to_string()),
            vectors: Ok(one_vector("some-other-model", MODEL_DIM as u32)),
        };
        let f = seed_doctor_facts(&s, "r", Path::new("/x/databases/a.db"), false);
        assert_eq!(f.state, "degraded");
        assert_eq!(
            f.degraded_reason,
            Some(DegradeReason::PinsMismatch.reason())
        );
    }

    #[test]
    fn read_error_is_degraded_unreadable() {
        let s = FakeStore {
            snapshot: Some("s1".to_string()),
            vectors: Err(()),
        };
        let f = seed_doctor_facts(&s, "r", Path::new("/x/databases/a.db"), false);
        assert_eq!(f.state, "degraded");
        assert_eq!(
            f.degraded_reason,
            Some(DegradeReason::StoreUnreadable.reason())
        );
    }

    #[test]
    fn building_overrides_state() {
        let s = FakeStore {
            snapshot: Some("s1".to_string()),
            vectors: Ok(one_vector(MODEL_ID, MODEL_DIM as u32)),
        };
        let f = seed_doctor_facts(&s, "r", Path::new("/x/databases/a.db"), true);
        assert_eq!(f.state, "building");
    }
}
