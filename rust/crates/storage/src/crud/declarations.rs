//! Declaration CRUD methods (Rust-32).
//!
//! Insert and deactivate declarations in the `declarations` table.
//! Declarations are authored governance inputs: boundaries,
//! requirements, waivers, entrypoints, etc.
//!
//! UID strategy: deterministic, derived from the semantic identity
//! of the policy object — NOT from the raw `value_json` content.
//! This makes `insert_declaration` idempotent at the policy level:
//! re-declaring the same logical policy object is a no-op, while
//! cosmetic changes (rewording reason text, changing obligation
//! descriptions) do not create duplicate active policy rows.
//!
//! The caller provides an `identity_key` string constructed from
//! the typed semantic fields that define the policy object's
//! identity. The storage layer hashes `(kind, identity_key)` to
//! produce a deterministic UUID v5.
//!
//! Identity keys by kind (caller responsibility):
//!   boundary:    `{repo_uid}:{module_path}:{forbids}`
//!   requirement: `{repo_uid}:{req_id}:{version}`
//!   waiver:      `{repo_uid}:{req_id}:{requirement_version}:{obligation_id}`
//!
//! This differs from the TS prototype which uses random UUIDs,
//! creating duplicate rows on repeated declare runs. Documented
//! in TECH-DEBT.md.

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// Input for inserting a declaration.
///
/// `identity_key` is a caller-constructed string from the typed
/// semantic fields that define this policy object's identity.
/// The storage layer uses it (together with `kind`) to derive a
/// deterministic UID. The `value_json` is NOT part of the identity
/// — cosmetic changes to value content do not create a new
/// declaration.
#[derive(Debug, Clone)]
pub struct DeclarationInsert {
	/// Semantic identity string for UID derivation. Constructed by
	/// the caller from typed fields (e.g. `{repo}:{module}:{forbids}`
	/// for boundaries). Must be stable for the same logical policy
	/// object regardless of value_json cosmetic changes.
	pub identity_key: String,
	pub repo_uid: String,
	pub target_stable_key: String,
	pub kind: String,
	pub value_json: String,
	pub created_at: String,
	pub created_by: Option<String>,
	pub supersedes_uid: Option<String>,
	pub authored_basis_json: Option<String>,
}

/// Result of an insert operation.
#[derive(Debug, Clone)]
pub struct DeclarationInsertResult {
	pub declaration_uid: String,
	/// True if the row was newly inserted; false if it already existed
	/// (INSERT OR IGNORE hit a duplicate).
	pub inserted: bool,
}

impl StorageConnection {
	/// Insert a declaration with a deterministic UID.
	///
	/// The UID is derived from `(kind, identity_key)` via UUID v5.
	/// `identity_key` encodes the semantic policy identity (not the
	/// raw JSON). This makes the operation idempotent at the policy
	/// level: re-inserting the same logical declaration with changed
	/// cosmetic fields (reason text, obligation wording) is a no-op.
	///
	/// Returns the UID and whether the row was newly inserted.
	pub fn insert_declaration(
		&self,
		decl: &DeclarationInsert,
	) -> Result<DeclarationInsertResult, StorageError> {
		let uid = deterministic_uid(&decl.kind, &decl.identity_key);

		let rows_changed = self.connection().execute(
			"INSERT OR IGNORE INTO declarations
			 (declaration_uid, repo_uid, snapshot_uid, target_stable_key, kind,
			  value_json, created_at, created_by, supersedes_uid, is_active, authored_basis_json)
			 VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, 1, ?)",
			rusqlite::params![
				uid,
				decl.repo_uid,
				decl.target_stable_key,
				decl.kind,
				decl.value_json,
				decl.created_at,
				decl.created_by,
				decl.supersedes_uid,
				decl.authored_basis_json,
			],
		)?;

		Ok(DeclarationInsertResult {
			declaration_uid: uid,
			inserted: rows_changed > 0,
		})
	}

	/// Deactivate a declaration by UID (set `is_active = 0`).
	///
	/// Returns the number of rows affected (0 or 1).
	/// A deactivated declaration remains in the table for audit trail
	/// but is excluded from all active-declaration queries.
	pub fn deactivate_declaration(
		&self,
		declaration_uid: &str,
	) -> Result<usize, StorageError> {
		let rows = self.connection().execute(
			"UPDATE declarations SET is_active = 0 WHERE declaration_uid = ?",
			rusqlite::params![declaration_uid],
		)?;
		Ok(rows)
	}
}

/// Derive a deterministic UUID v5 from declaration identity.
///
/// Uses the DNS namespace as the base (arbitrary but stable choice).
/// The input string is `{kind}:{identity_key}`.
///
/// `identity_key` is constructed by the caller from the typed
/// semantic fields that define policy object identity. The raw
/// `value_json` is deliberately excluded — cosmetic changes to
/// value content (reason text, obligation wording) must not alter
/// the UID.
fn deterministic_uid(kind: &str, identity_key: &str) -> String {
	use uuid::Uuid;
	let input = format!("{}:{}", kind, identity_key);
	Uuid::new_v5(&Uuid::NAMESPACE_DNS, input.as_bytes()).to_string()
}

// ── Identity key constructors ───────────────────────────────────
//
// Pure functions that build identity keys from typed fields.
// Exported for use by CLI declare commands (Rust-33+).

/// Build the identity key for a boundary declaration.
/// Identity: `(repo, module_path, forbids)`.
pub fn boundary_identity_key(repo_uid: &str, module_path: &str, forbids: &str) -> String {
	format!("{}:{}:{}", repo_uid, module_path, forbids)
}

/// Build the identity key for a requirement declaration.
/// Identity: `(repo, req_id, version)`.
pub fn requirement_identity_key(repo_uid: &str, req_id: &str, version: i64) -> String {
	format!("{}:{}:{}", repo_uid, req_id, version)
}

/// Build the identity key for a waiver declaration.
/// Identity: `(repo, req_id, requirement_version, obligation_id)`.
pub fn waiver_identity_key(
	repo_uid: &str,
	req_id: &str,
	requirement_version: i64,
	obligation_id: &str,
) -> String {
	format!("{}:{}:{}:{}", repo_uid, req_id, requirement_version, obligation_id)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::fresh_storage;

	fn make_boundary_decl(repo_uid: &str, module: &str, forbids: &str) -> DeclarationInsert {
		let target_key = format!("{}:{}:MODULE", repo_uid, module);
		let value = serde_json::json!({ "forbids": forbids });
		DeclarationInsert {
			identity_key: boundary_identity_key(repo_uid, module, forbids),
			repo_uid: repo_uid.to_string(),
			target_stable_key: target_key,
			kind: "boundary".to_string(),
			value_json: value.to_string(),
			created_at: "2024-01-01T00:00:00Z".to_string(),
			created_by: Some("cli".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		}
	}

	fn make_requirement_decl(repo_uid: &str, req_id: &str, version: i64) -> DeclarationInsert {
		let target_key = format!("{}:requirement:{}:{}", repo_uid, req_id, version);
		let value = serde_json::json!({
			"req_id": req_id,
			"version": version,
			"verification": [{
				"obligation_id": "obl-1",
				"obligation": "test obligation",
				"method": "arch_violations",
				"target": "src/core",
			}],
		});
		DeclarationInsert {
			identity_key: requirement_identity_key(repo_uid, req_id, version),
			repo_uid: repo_uid.to_string(),
			target_stable_key: target_key,
			kind: "requirement".to_string(),
			value_json: value.to_string(),
			created_at: "2024-01-01T00:00:00Z".to_string(),
			created_by: Some("cli".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		}
	}

	fn make_waiver_decl(
		repo_uid: &str,
		req_id: &str,
		version: i64,
		obligation_id: &str,
	) -> DeclarationInsert {
		let target_key = format!("{}:waiver:{}#{}", repo_uid, req_id, obligation_id);
		let value = serde_json::json!({
			"req_id": req_id,
			"requirement_version": version,
			"obligation_id": obligation_id,
			"reason": "test waiver",
			"created_at": "2024-01-01T00:00:00Z",
		});
		DeclarationInsert {
			identity_key: waiver_identity_key(repo_uid, req_id, version, obligation_id),
			repo_uid: repo_uid.to_string(),
			target_stable_key: target_key,
			kind: "waiver".to_string(),
			value_json: value.to_string(),
			created_at: "2024-01-01T00:00:00Z".to_string(),
			created_by: Some("cli".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		}
	}

	fn setup_repo(storage: &StorageConnection) {
		storage
			.connection()
			.execute_batch(
				"INSERT INTO repos (repo_uid, name, root_path, created_at)
				 VALUES ('r1', 'test-repo', '/tmp/r1', '2024-01-01T00:00:00Z')",
			)
			.unwrap();
	}

	// ── insert_declaration ──────────────────────────────────────

	#[test]
	fn insert_declaration_returns_uid_and_inserted_true() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let decl = make_boundary_decl("r1", "src/core", "src/adapters");
		let result = storage.insert_declaration(&decl).unwrap();

		assert!(!result.declaration_uid.is_empty());
		assert!(result.inserted);
	}

	#[test]
	fn insert_declaration_is_idempotent_on_same_identity() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let decl = make_boundary_decl("r1", "src/core", "src/adapters");
		let first = storage.insert_declaration(&decl).unwrap();
		let second = storage.insert_declaration(&decl).unwrap();

		assert_eq!(first.declaration_uid, second.declaration_uid);
		assert!(first.inserted);
		assert!(!second.inserted, "second insert should be a no-op");
	}

	/// Cosmetic value_json changes (e.g. added reason text) must NOT
	/// create a second active declaration for the same policy object.
	#[test]
	fn insert_declaration_cosmetic_value_change_is_idempotent() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let decl1 = DeclarationInsert {
			identity_key: boundary_identity_key("r1", "src/core", "src/adapters"),
			repo_uid: "r1".to_string(),
			target_stable_key: "r1:src/core:MODULE".to_string(),
			kind: "boundary".to_string(),
			value_json: r#"{"forbids":"src/adapters"}"#.to_string(),
			created_at: "2024-01-01T00:00:00Z".to_string(),
			created_by: Some("cli".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		};

		// Same identity but different value_json (added reason).
		let decl2 = DeclarationInsert {
			identity_key: boundary_identity_key("r1", "src/core", "src/adapters"),
			repo_uid: "r1".to_string(),
			target_stable_key: "r1:src/core:MODULE".to_string(),
			kind: "boundary".to_string(),
			value_json: r#"{"forbids":"src/adapters","reason":"clean architecture"}"#.to_string(),
			created_at: "2024-01-02T00:00:00Z".to_string(),
			created_by: Some("cli".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		};

		let first = storage.insert_declaration(&decl1).unwrap();
		let second = storage.insert_declaration(&decl2).unwrap();

		assert_eq!(first.declaration_uid, second.declaration_uid);
		assert!(first.inserted);
		assert!(!second.inserted, "cosmetic change must not create duplicate");
	}

	#[test]
	fn insert_declaration_different_kinds_get_different_uids() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let boundary = make_boundary_decl("r1", "src/core", "src/adapters");
		let requirement = make_requirement_decl("r1", "REQ-001", 1);
		let waiver = make_waiver_decl("r1", "REQ-001", 1, "obl-1");

		let b = storage.insert_declaration(&boundary).unwrap();
		let r = storage.insert_declaration(&requirement).unwrap();
		let w = storage.insert_declaration(&waiver).unwrap();

		assert_ne!(b.declaration_uid, r.declaration_uid);
		assert_ne!(b.declaration_uid, w.declaration_uid);
		assert_ne!(r.declaration_uid, w.declaration_uid);
	}

	/// Different `forbids` targets for the same module must produce
	/// distinct declarations (different policy objects).
	#[test]
	fn insert_declaration_different_forbids_are_distinct() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let b1 = make_boundary_decl("r1", "src/core", "src/adapters");
		let b2 = make_boundary_decl("r1", "src/core", "src/util");

		let r1 = storage.insert_declaration(&b1).unwrap();
		let r2 = storage.insert_declaration(&b2).unwrap();

		assert_ne!(r1.declaration_uid, r2.declaration_uid);
		assert!(r1.inserted);
		assert!(r2.inserted);
	}

	#[test]
	fn insert_declaration_persists_all_fields() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let decl = DeclarationInsert {
			identity_key: boundary_identity_key("r1", "src/core", "src/adapters"),
			repo_uid: "r1".to_string(),
			target_stable_key: "r1:src/core:MODULE".to_string(),
			kind: "boundary".to_string(),
			value_json: r#"{"forbids":"src/adapters"}"#.to_string(),
			created_at: "2024-06-15T12:00:00Z".to_string(),
			created_by: Some("test-user".to_string()),
			supersedes_uid: None,
			authored_basis_json: Some(r#"{"commit":"abc123"}"#.to_string()),
		};
		let result = storage.insert_declaration(&decl).unwrap();

		let conn = storage.connection();
		let row: (String, String, String, String, i32, String, Option<String>) = conn
			.query_row(
				"SELECT target_stable_key, kind, value_json, created_at, is_active, created_by, authored_basis_json
				 FROM declarations WHERE declaration_uid = ?",
				rusqlite::params![result.declaration_uid],
				|row| {
					Ok((
						row.get(0)?,
						row.get(1)?,
						row.get(2)?,
						row.get(3)?,
						row.get(4)?,
						row.get::<_, String>(5)?,
						row.get(6)?,
					))
				},
			)
			.unwrap();

		assert_eq!(row.0, "r1:src/core:MODULE");
		assert_eq!(row.1, "boundary");
		assert_eq!(row.2, r#"{"forbids":"src/adapters"}"#);
		assert_eq!(row.3, "2024-06-15T12:00:00Z");
		assert_eq!(row.4, 1);
		assert_eq!(row.5, "test-user");
		assert_eq!(row.6.as_deref(), Some(r#"{"commit":"abc123"}"#));
	}

	#[test]
	fn inserted_declaration_is_readable_by_existing_queries() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let decl = make_boundary_decl("r1", "src/core", "src/adapters");
		storage.insert_declaration(&decl).unwrap();

		let boundaries = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(boundaries.len(), 1);
		assert_eq!(boundaries[0].boundary_module, "src/core");
		assert_eq!(boundaries[0].forbids, "src/adapters");
	}

	// ── deactivate_declaration ──────────────────────────────────

	#[test]
	fn deactivate_declaration_hides_from_active_queries() {
		let storage = fresh_storage();
		setup_repo(&storage);

		let decl = make_boundary_decl("r1", "src/core", "src/adapters");
		let result = storage.insert_declaration(&decl).unwrap();

		let before = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(before.len(), 1);

		let rows = storage.deactivate_declaration(&result.declaration_uid).unwrap();
		assert_eq!(rows, 1);

		let after = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(after.len(), 0, "deactivated declaration should not appear");
	}

	#[test]
	fn deactivate_nonexistent_returns_zero() {
		let storage = fresh_storage();
		let rows = storage.deactivate_declaration("nonexistent-uid").unwrap();
		assert_eq!(rows, 0);
	}

	// ── deterministic_uid ───────────────────────────────────────

	#[test]
	fn deterministic_uid_is_stable() {
		let a = deterministic_uid("boundary", "r1:src/core:src/adapters");
		let b = deterministic_uid("boundary", "r1:src/core:src/adapters");
		assert_eq!(a, b, "same inputs must produce same UID");
	}

	#[test]
	fn deterministic_uid_varies_on_kind_change() {
		let a = deterministic_uid("boundary", "r1:src/core:src/adapters");
		let b = deterministic_uid("requirement", "r1:src/core:src/adapters");
		assert_ne!(a, b);
	}

	#[test]
	fn deterministic_uid_varies_on_identity_key_change() {
		let a = deterministic_uid("boundary", "r1:src/core:src/adapters");
		let b = deterministic_uid("boundary", "r1:src/core:src/util");
		assert_ne!(a, b);
	}

	// ── identity key constructors ───────────────────────────────

	#[test]
	fn boundary_identity_key_encodes_all_fields() {
		let key = boundary_identity_key("r1", "src/core", "src/adapters");
		assert_eq!(key, "r1:src/core:src/adapters");
	}

	#[test]
	fn requirement_identity_key_encodes_all_fields() {
		let key = requirement_identity_key("r1", "REQ-001", 2);
		assert_eq!(key, "r1:REQ-001:2");
	}

	#[test]
	fn waiver_identity_key_encodes_all_fields() {
		let key = waiver_identity_key("r1", "REQ-001", 1, "obl-1");
		assert_eq!(key, "r1:REQ-001:1:obl-1");
	}

	// ── schema corruption ───────────────────────────────────────

	#[test]
	fn insert_declaration_propagates_error_on_schema_corruption() {
		let storage = fresh_storage();
		storage
			.connection()
			.execute_batch("DROP TABLE declarations")
			.unwrap();

		let decl = make_boundary_decl("r1", "src/core", "src/adapters");
		let result = storage.insert_declaration(&decl);
		assert!(result.is_err());
	}
}
