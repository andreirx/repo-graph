//! Doctor-facing snapshot of the seed store's state (spec §9), computed directly
//! from the sidecar + current corpus + config — self-contained, no daemon field.
//!
//! Every read that is rendered reports unknown-with-reason (STANDING HONESTY
//! RULE): `io::NotFound` on the sidecar ⇒ absent; any other read/validate outcome
//! ⇒ a classified `degraded` reason; a CORPUS-read failure ⇒ `degraded` ("cannot
//! verify staleness"), NEVER an empty map silently rendered as "0 of 0 changed".

use std::collections::HashMap;
use std::path::Path;

use repo_graph_seed::rank::partition_fresh;
use repo_graph_seed::SeedCorpusRead;

use super::query::DegradeReason;
use super::{sidecar_path, EndpointEmbedder, ModelIdentity, SeedEndpointConfig};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SeedDoctorFacts {
    /// "present" | "absent" (never indexed / just built) | "building" (a pass is
    /// in flight) | "degraded".
    pub state: String,
    pub model_id: String,
    pub dim: usize,
    /// The pinned model id's identity provenance (spec §9), from a live doctor-time
    /// endpoint probe: `endpoint-echoed` (verified), `operator-asserted (…)`,
    /// `MISMATCH …`, or `unverified (…)`. NEVER a bare "operator-asserted" asserted
    /// as if verified (review-2 #4).
    pub model_identity: String,
    /// Files changed since embed (staleness numerator). `None` when NOT measured —
    /// only a successful sidecar decode + corpus freshness evaluation (state
    /// `present`, and the prior-generation `building` it carries forward) yields a
    /// count. Absent/degraded/unknown states leave it `None` (rendered as `null`):
    /// the count is UNKNOWN, never a fabricated measured-zero (STANDING HONESTY RULE,
    /// review-5 #1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_count: Option<usize>,
    /// Total entries in the store. `None` unless measured (see `stale_count`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// Present only when degraded — the honest cause.
    pub degraded_reason: Option<String>,
}

/// Compute the doctor facts for a repo's seed store (spec §9).
///
/// `is_building` is the coordinator's live "a pass is running for this db_path"
/// signal — when set, the state is `building` (a pass is actively (re)computing),
/// carrying whatever prior counts a published store still has.
///
/// Model identity provenance is resolved by a LIVE endpoint probe here (not a
/// stored value): the pass's wire-time echo check (§7.1) is not persisted, so a
/// probe is the only honest way to report `endpoint-echoed`. An unreachable
/// endpoint ⇒ `unverified (…)`, never a false claim.
pub fn seed_doctor_facts<S>(
    storage: &S,
    repo_uid: &str,
    db_path: &Path,
    is_building: bool,
) -> SeedDoctorFacts
where
    S: SeedCorpusRead + ?Sized,
{
    let cfg = SeedEndpointConfig::from_env();
    // An invalid seed config (e.g. a non-integer `RMAP_SEED_DIM`) is a degraded
    // cause in its own right — reported with the SPECIFIC bad value (the doctor is
    // the detailed diagnostic surface), never silently defaulted (review-4 #1).
    if let Some(reason) = &cfg.dim_config_error {
        return SeedDoctorFacts {
            state: "degraded".to_string(),
            model_id: cfg.model_id.clone(),
            dim: cfg.dim,
            model_identity: "unverified (seed configuration invalid)".to_string(),
            stale_count: None,
            total: None,
            degraded_reason: Some(reason.clone()),
        };
    }
    let model_identity = match EndpointEmbedder::from_config(&cfg) {
        Ok(e) => e.probe_model_identity().label(),
        Err(e) => ModelIdentity::Unverified {
            reason: e.to_string(),
        }
        .label(),
    };
    let base = SeedDoctorFacts {
        state: "absent".to_string(),
        model_id: cfg.model_id.clone(),
        dim: cfg.dim,
        model_identity,
        stale_count: None,
        total: None,
        degraded_reason: None,
    };

    let facts = store_facts(storage, repo_uid, db_path, &cfg, base);
    // A running pass overrides the store-derived label: "building" is the honest
    // current state (the on-disk store, if any, is the PRIOR generation).
    if is_building {
        return SeedDoctorFacts {
            state: "building".to_string(),
            degraded_reason: None,
            ..facts
        };
    }
    facts
}

/// The store-derived facts (present / absent / degraded), before the `building`
/// override. `io::NotFound` on the sidecar ⇒ `absent`; any other read/validate
/// outcome ⇒ a classified `degraded` reason; a CORPUS-read failure ⇒ `degraded`
/// ("cannot verify staleness"), never an empty map silently rendered as "0 of 0".
fn store_facts<S>(
    storage: &S,
    repo_uid: &str,
    db_path: &Path,
    cfg: &SeedEndpointConfig,
    base: SeedDoctorFacts,
) -> SeedDoctorFacts
where
    S: SeedCorpusRead + ?Sized,
{
    let sidecar = match sidecar_path(db_path) {
        Some(p) => p,
        None => return degraded(&base, "cannot derive sidecar path".to_string()),
    };
    let key = cfg.store_key();
    // review-10 #1: go through `read_validated` (the metadata-pre-guarded reader every
    // other consumer uses) so an over-budget sidecar is rejected WITHOUT loading it,
    // `FileTooLarge` reports the budget degradation, and a magic mismatch reads as a
    // CORRUPT store (not our file) rather than being misclassified as a pin mismatch.
    let body = match repo_graph_seed::store::read_validated(&sidecar, &key) {
        Ok(b) => b,
        Err(repo_graph_seed::store::SeedStoreError::NotFound) => return base,
        Err(e) => {
            let reason = match &e {
                repo_graph_seed::store::SeedStoreError::TooLarge { .. }
                | repo_graph_seed::store::SeedStoreError::FileTooLarge { .. } => {
                    DegradeReason::StoreTooLarge.reason()
                }
                repo_graph_seed::store::SeedStoreError::KeyMismatch
                | repo_graph_seed::store::SeedStoreError::SchemaMismatch { .. } => {
                    DegradeReason::PinsMismatch.reason()
                }
                _ => DegradeReason::StoreUnreadable.reason(),
            };
            return degraded(&base, reason.to_string());
        }
    };

    // Corpus read: a FAILURE means staleness is unknown — report degraded WITH the
    // reason, never an empty map (which would fabricate "0 of N changed").
    let current: HashMap<String, String> = match storage.seed_corpus(repo_uid) {
        Ok(c) => c
            .into_iter()
            .map(|e| (e.file_uid, e.content_hash))
            .collect(),
        Err(e) => {
            return degraded(
                &base,
                format!("cannot verify staleness (corpus read failed: {e})"),
            )
        }
    };
    let part = partition_fresh(&body, &current);
    SeedDoctorFacts {
        state: "present".to_string(),
        // Measured — a successful decode + corpus evaluation is the ONLY place counts
        // become known (review-5 #1).
        stale_count: Some(part.stale_count),
        total: Some(part.total),
        ..base
    }
}

/// Build a `degraded` facts record from the `absent` base + an honest reason.
fn degraded(base: &SeedDoctorFacts, reason: String) -> SeedDoctorFacts {
    SeedDoctorFacts {
        state: "degraded".to_string(),
        degraded_reason: Some(reason),
        ..base.clone()
    }
}

#[cfg(test)]
mod tests {
    //! review-10 #1 (test half): the doctor now loads the sidecar through the guarded
    //! `read_validated`; these pin its classification of the two rejection causes the
    //! reviewer named — an OVER-BUDGET sidecar (`FileTooLarge` ⇒ `StoreTooLarge`, the
    //! budget degradation, no longer an unbounded read) and a CORRUPT/foreign store
    //! (decode failure ⇒ `StoreUnreadable`, the same `_` arm a `MagicMismatch` takes —
    //! never misread as a pin mismatch). `store_facts` is driven directly (no endpoint
    //! probe, no env, no daemon spawn) so the classification is the only thing under test.
    use super::*;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    /// A `SeedCorpusRead` that is never consulted: `store_facts` short-circuits at the
    /// sidecar-read failure BEFORE the corpus read on both paths under test. Both methods
    /// return the honest empty result rather than panic, so a future change that DID reach
    /// them fails loudly on a wrong assertion, not an opaque unwrap.
    struct UnusedCorpus;
    impl SeedCorpusRead for UnusedCorpus {
        fn seed_corpus(
            &self,
            _repo_uid: &str,
        ) -> Result<Vec<repo_graph_seed::SeedCorpusEntry>, repo_graph_seed::SeedCorpusError>
        {
            Ok(vec![])
        }
        fn module_owners(
            &self,
            _snapshot_uid: &str,
            _file_uids: &[String],
        ) -> Result<HashMap<String, String>, repo_graph_seed::SeedCorpusError> {
            Ok(HashMap::new())
        }
    }

    fn cfg() -> SeedEndpointConfig {
        SeedEndpointConfig {
            endpoint: "http://127.0.0.1:9/v1/embeddings".to_string(),
            model_id: "m".to_string(),
            dim: 8,
            dim_config_error: None,
        }
    }

    fn base() -> SeedDoctorFacts {
        SeedDoctorFacts {
            state: "absent".to_string(),
            model_id: "m".to_string(),
            dim: 8,
            model_identity: "unverified (test)".to_string(),
            stale_count: None,
            total: None,
            degraded_reason: None,
        }
    }

    /// A tempdir with the `<root>/databases/<hash>.db` + `<root>/seed-vectors/` layout
    /// `sidecar_path` expects; returns `(root, db_path, sidecar_path)`.
    fn layout() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("databases")).unwrap();
        std::fs::create_dir_all(root.path().join("seed-vectors")).unwrap();
        let db_path = root.path().join("databases").join("deadbeefdeadbeef.db");
        let sidecar = sidecar_path(&db_path).unwrap();
        (root, db_path, sidecar)
    }

    #[test]
    fn oversized_sidecar_is_store_too_large_not_an_unbounded_read() {
        let (_root, db_path, sidecar) = layout();
        // Sparse file (set_len, no data blocks): the metadata guard must reject it on
        // length ALONE — this allocates no gigabytes and never reads the body.
        let f = File::create(&sidecar).unwrap();
        f.set_len(repo_graph_seed::store::MAX_FILE_BYTES + 1)
            .unwrap();
        drop(f);
        let facts = store_facts(&UnusedCorpus, "r", &db_path, &cfg(), base());
        assert_eq!(facts.state, "degraded", "oversized ⇒ degraded: {facts:?}");
        assert_eq!(
            facts.degraded_reason.as_deref(),
            Some(DegradeReason::StoreTooLarge.reason()),
            "an over-budget sidecar reports the budget degradation, not a pin/corrupt cause"
        );
        // Counts stay UNKNOWN — never a fabricated measured-zero on a rejected store.
        assert!(facts.stale_count.is_none() && facts.total.is_none());
    }

    #[test]
    fn corrupt_sidecar_is_store_unreadable_not_a_pin_mismatch() {
        let (_root, db_path, sidecar) = layout();
        // Garbage bytes fail `decode` (the same `_` arm a wrong MAGIC takes) — the doctor
        // must call this a corrupt/foreign store, NEVER a pin mismatch (which would tell
        // the operator to "rebuild with the same model" for a file that is simply not ours).
        let mut f = File::create(&sidecar).unwrap();
        f.write_all(b"this is not a repo-graph .vec file").unwrap();
        drop(f);
        let facts = store_facts(&UnusedCorpus, "r", &db_path, &cfg(), base());
        assert_eq!(facts.state, "degraded", "corrupt ⇒ degraded: {facts:?}");
        assert_eq!(
            facts.degraded_reason.as_deref(),
            Some(DegradeReason::StoreUnreadable.reason()),
            "corruption is 'unreadable', never a pin mismatch"
        );
        assert_ne!(
            facts.degraded_reason.as_deref(),
            Some(DegradeReason::PinsMismatch.reason()),
            "review-10 #1: a corrupt/foreign store must NOT be misclassified as pins"
        );
    }

    #[test]
    fn missing_sidecar_is_absent_not_degraded() {
        // The honesty boundary: only `io::NotFound` is "absent"; it must NOT collapse into
        // the degraded taxonomy the two cases above produce.
        let (_root, db_path, _sidecar) = layout(); // no file written
        let facts = store_facts(&UnusedCorpus, "r", &db_path, &cfg(), base());
        assert_eq!(facts.state, "absent", "no sidecar ⇒ absent: {facts:?}");
        assert!(facts.degraded_reason.is_none());
    }
}
