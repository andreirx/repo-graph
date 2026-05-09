//! CargoModuleStorePort implementation for StorageConnection.
//!
//! Implements the indexer-owned storage port for persisting
//! Cargo.toml-derived module candidates and evidence.
//!
//! This is the adapter layer: storage (adapter) implements traits
//! defined by indexer (policy).

use crate::connection::StorageConnection;
use crate::error::StorageError;

use repo_graph_indexer::cargo_manifest::{
	CargoModuleCandidateInput, CargoModuleEvidenceInput, CargoModuleStorePort,
	FileOwnershipInput,
};

impl CargoModuleStorePort for StorageConnection {
	type Error = StorageError;

	fn insert_cargo_module_candidates(
		&mut self,
		candidates: &[CargoModuleCandidateInput],
	) -> Result<usize, StorageError> {
		if candidates.is_empty() {
			return Ok(0);
		}

		let conn = self.connection_mut();
		let mut stmt = conn.prepare(
			r#"
			INSERT OR REPLACE INTO module_candidates (
				module_candidate_uid, snapshot_uid, repo_uid,
				module_key, module_kind, canonical_root_path,
				confidence, display_name, metadata_json
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)?;

		let mut inserted = 0;
		for candidate in candidates {
			let rows_changed = stmt.execute(rusqlite::params![
				candidate.module_candidate_uid,
				candidate.snapshot_uid,
				candidate.repo_uid,
				candidate.module_key,
				candidate.module_kind,
				candidate.canonical_root_path,
				candidate.confidence,
				candidate.display_name,
				candidate.metadata_json,
			])?;
			inserted += rows_changed;
		}

		Ok(inserted)
	}

	fn insert_cargo_module_evidence(
		&mut self,
		evidence: &[CargoModuleEvidenceInput],
	) -> Result<usize, StorageError> {
		if evidence.is_empty() {
			return Ok(0);
		}

		let conn = self.connection_mut();
		let mut stmt = conn.prepare(
			r#"
			INSERT OR REPLACE INTO module_candidate_evidence (
				evidence_uid, module_candidate_uid, snapshot_uid, repo_uid,
				source_type, source_path, evidence_kind, confidence, payload_json
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)?;

		let mut inserted = 0;
		for ev in evidence {
			let rows_changed = stmt.execute(rusqlite::params![
				ev.evidence_uid,
				ev.module_candidate_uid,
				ev.snapshot_uid,
				ev.repo_uid,
				ev.source_type,
				ev.source_path,
				ev.evidence_kind,
				ev.confidence,
				ev.payload_json,
			])?;
			inserted += rows_changed;
		}

		Ok(inserted)
	}

	fn insert_file_ownership(
		&mut self,
		ownership: &[FileOwnershipInput],
	) -> Result<usize, StorageError> {
		if ownership.is_empty() {
			return Ok(0);
		}

		let conn = self.connection_mut();
		let mut stmt = conn.prepare(
			r#"
			INSERT OR REPLACE INTO module_file_ownership (
				snapshot_uid, repo_uid, file_uid,
				module_candidate_uid, assignment_kind, confidence, basis_json
			) VALUES (?, ?, ?, ?, ?, ?, ?)
			"#,
		)?;

		let mut inserted = 0;
		for o in ownership {
			let rows_changed = stmt.execute(rusqlite::params![
				o.snapshot_uid,
				o.repo_uid,
				o.file_uid,
				o.module_candidate_uid,
				o.assignment_kind,
				o.confidence,
				o.basis_json,
			])?;
			inserted += rows_changed;
		}

		Ok(inserted)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{fresh_storage, make_repo};
	use crate::types::CreateSnapshotInput;
	use repo_graph_indexer::cargo_manifest::{
		generate_evidence_uid, generate_module_key, generate_module_uid,
	};

	fn setup_test_snapshot(conn: &mut StorageConnection) -> (String, String) {
		let repo = make_repo("test-repo");
		conn.add_repo(&repo).expect("add repo");

		let snapshot = conn
			.create_snapshot(&CreateSnapshotInput {
				repo_uid: repo.repo_uid.clone(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.expect("create snapshot");

		(repo.repo_uid, snapshot.snapshot_uid)
	}

	#[test]
	fn insert_and_read_cargo_module_candidates() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&mut conn);

		let module_uid = generate_module_uid(&repo_uid, "rust/crates/storage");
		let module_key = generate_module_key(&repo_uid, "rust/crates/storage");

		let candidates = vec![CargoModuleCandidateInput {
			module_candidate_uid: module_uid.clone(),
			snapshot_uid: snapshot_uid.clone(),
			repo_uid: repo_uid.clone(),
			module_key: module_key.clone(),
			module_kind: "declared".to_string(),
			canonical_root_path: "rust/crates/storage".to_string(),
			confidence: 1.0,
			display_name: "repo-graph-storage".to_string(),
			metadata_json: None,
		}];

		let inserted = conn
			.insert_cargo_module_candidates(&candidates)
			.expect("insert");
		assert_eq!(inserted, 1);

		// Verify via existing read path
		let result = conn
			.get_module_candidates_for_snapshot(&snapshot_uid)
			.expect("query");
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].module_key, module_key);
		assert_eq!(result[0].module_kind, "declared");
		assert_eq!(result[0].canonical_root_path, "rust/crates/storage");
		assert_eq!(result[0].display_name, Some("repo-graph-storage".to_string()));
	}

	#[test]
	fn insert_and_read_cargo_module_evidence() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&mut conn);

		// First insert the candidate (FK constraint)
		let module_uid = generate_module_uid(&repo_uid, "crates/core");
		let module_key = generate_module_key(&repo_uid, "crates/core");

		let candidates = vec![CargoModuleCandidateInput {
			module_candidate_uid: module_uid.clone(),
			snapshot_uid: snapshot_uid.clone(),
			repo_uid: repo_uid.clone(),
			module_key,
			module_kind: "declared".to_string(),
			canonical_root_path: "crates/core".to_string(),
			confidence: 1.0,
			display_name: "core".to_string(),
			metadata_json: None,
		}];

		conn.insert_cargo_module_candidates(&candidates)
			.expect("insert candidate");

		// Now insert evidence
		let evidence_uid = generate_evidence_uid(&module_uid, "crates/core/Cargo.toml");
		let evidence = vec![CargoModuleEvidenceInput {
			evidence_uid: evidence_uid.clone(),
			module_candidate_uid: module_uid.clone(),
			snapshot_uid: snapshot_uid.clone(),
			repo_uid: repo_uid.clone(),
			source_type: "cargo_toml".to_string(),
			source_path: "crates/core/Cargo.toml".to_string(),
			evidence_kind: "manifest_declaration".to_string(),
			confidence: 1.0,
			payload_json: r#"{"package_name":"core","crate_root":"crates/core","workspace_member":true}"#.to_string(),
		}];

		let inserted = conn
			.insert_cargo_module_evidence(&evidence)
			.expect("insert evidence");
		assert_eq!(inserted, 1);

		// Verify via raw query (no existing read method for evidence)
		let c = conn.connection();
		let count: i64 = c
			.query_row(
				"SELECT COUNT(*) FROM module_candidate_evidence WHERE module_candidate_uid = ?",
				[&module_uid],
				|row| row.get(0),
			)
			.expect("count");
		assert_eq!(count, 1);

		// Verify payload
		let payload: String = c
			.query_row(
				"SELECT payload_json FROM module_candidate_evidence WHERE evidence_uid = ?",
				[&evidence_uid],
				|row| row.get(0),
			)
			.expect("payload");
		assert!(payload.contains("package_name"));
		assert!(payload.contains("workspace_member"));
	}

	#[test]
	fn empty_inputs_return_zero() {
		let mut conn = fresh_storage();
		let (_repo_uid, _snapshot_uid) = setup_test_snapshot(&mut conn);

		let inserted = conn
			.insert_cargo_module_candidates(&[])
			.expect("insert empty");
		assert_eq!(inserted, 0);

		let inserted = conn
			.insert_cargo_module_evidence(&[])
			.expect("insert empty evidence");
		assert_eq!(inserted, 0);
	}

	#[test]
	fn multiple_candidates_insert() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&mut conn);

		let candidates: Vec<CargoModuleCandidateInput> = ["crates/a", "crates/b", "crates/c"]
			.iter()
			.map(|path| {
				let uid = generate_module_uid(&repo_uid, path);
				let key = generate_module_key(&repo_uid, path);
				CargoModuleCandidateInput {
					module_candidate_uid: uid,
					snapshot_uid: snapshot_uid.clone(),
					repo_uid: repo_uid.clone(),
					module_key: key,
					module_kind: "declared".to_string(),
					canonical_root_path: path.to_string(),
					confidence: 1.0,
					display_name: path.rsplit('/').next().unwrap().to_string(),
					metadata_json: None,
				}
			})
			.collect();

		let inserted = conn
			.insert_cargo_module_candidates(&candidates)
			.expect("insert");
		assert_eq!(inserted, 3);

		let result = conn
			.get_module_candidates_for_snapshot(&snapshot_uid)
			.expect("query");
		assert_eq!(result.len(), 3);
	}

	#[test]
	fn insert_and_read_file_ownership() {
		use repo_graph_indexer::cargo_manifest::FileOwnershipInput;

		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&mut conn);

		// First insert the candidate (FK constraint)
		let module_uid = generate_module_uid(&repo_uid, "crates/core");
		let module_key = generate_module_key(&repo_uid, "crates/core");

		let candidates = vec![CargoModuleCandidateInput {
			module_candidate_uid: module_uid.clone(),
			snapshot_uid: snapshot_uid.clone(),
			repo_uid: repo_uid.clone(),
			module_key,
			module_kind: "declared".to_string(),
			canonical_root_path: "crates/core".to_string(),
			confidence: 1.0,
			display_name: "core".to_string(),
			metadata_json: None,
		}];

		conn.insert_cargo_module_candidates(&candidates)
			.expect("insert candidate");

		// Now insert file ownership for multiple files
		let ownership = vec![
			FileOwnershipInput {
				snapshot_uid: snapshot_uid.clone(),
				repo_uid: repo_uid.clone(),
				file_uid: format!("{}:crates/core/src/lib.rs", repo_uid),
				module_candidate_uid: module_uid.clone(),
				assignment_kind: "manifest_prefix".to_string(),
				confidence: 1.0,
				basis_json: None,
			},
			FileOwnershipInput {
				snapshot_uid: snapshot_uid.clone(),
				repo_uid: repo_uid.clone(),
				file_uid: format!("{}:crates/core/src/utils.rs", repo_uid),
				module_candidate_uid: module_uid.clone(),
				assignment_kind: "manifest_prefix".to_string(),
				confidence: 1.0,
				basis_json: Some(r#"{"reason":"longest-prefix-match"}"#.to_string()),
			},
		];

		let inserted = conn
			.insert_file_ownership(&ownership)
			.expect("insert ownership");
		assert_eq!(inserted, 2);

		// Verify via raw query
		let c = conn.connection();
		let count: i64 = c
			.query_row(
				"SELECT COUNT(*) FROM module_file_ownership WHERE module_candidate_uid = ?",
				[&module_uid],
				|row| row.get(0),
			)
			.expect("count");
		assert_eq!(count, 2);

		// Verify assignment_kind
		let kind: String = c
			.query_row(
				"SELECT assignment_kind FROM module_file_ownership WHERE file_uid = ?",
				[&format!("{}:crates/core/src/lib.rs", repo_uid)],
				|row| row.get(0),
			)
			.expect("assignment_kind");
		assert_eq!(kind, "manifest_prefix");
	}

	#[test]
	fn empty_ownership_returns_zero() {
		let mut conn = fresh_storage();
		let (_repo_uid, _snapshot_uid) = setup_test_snapshot(&mut conn);

		let inserted = conn
			.insert_file_ownership(&[])
			.expect("insert empty ownership");
		assert_eq!(inserted, 0);
	}
}
