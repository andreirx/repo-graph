//! Snapshot-info aggregator.
//!
//! Emits `SNAPSHOT_INFO` informational signal unconditionally
//! for every successful orient call. Snapshot identity is not a
//! zero-count surface — it is always present because we have
//! already verified the snapshot exists before reaching this
//! point.

use super::AggregatorOutput;
use crate::dto::signal::{Signal, SnapshotInfoEvidence};
use crate::storage_port::AgentSnapshot;

pub fn aggregate(snapshot: &AgentSnapshot) -> AggregatorOutput {
	let evidence = SnapshotInfoEvidence {
		snapshot_uid: snapshot.snapshot_uid.clone(),
		scope: snapshot.scope.clone(),
		basis_commit: snapshot.basis_commit.clone(),
		created_at: snapshot.created_at.clone(),
	};

	AggregatorOutput {
		signals: vec![Signal::snapshot_info(evidence)],
		limits: Vec::new(),
	}
}
