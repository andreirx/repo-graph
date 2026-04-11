//! Migration 004 — backfill `obligation_id` on verification entries.
//!
//! Hand-port of `004-obligation-ids.ts`. This is the most complex
//! migration in the corpus and the most failure-prone surface in
//! the entire R2-C substep. Read the original TS file before
//! modifying anything here.
//!
//! ── Behavioral contract ───────────────────────────────────────
//!
//! For every row in `declarations WHERE kind = 'requirement'`:
//!
//!   1. Parse the row's `value_json` as JSON.
//!   2. Validate structural invariants:
//!      - The parsed value must be a JSON object.
//!      - `req_id` must be a non-empty string.
//!      - `version` must be a number.
//!      - If `verification` is present, it must be an array.
//!      - Each verification entry must be an object.
//!      - Each verification entry must have a non-empty `method`
//!        string.
//!   3. For each verification entry that lacks a non-empty
//!      `obligation_id` string, generate a fresh RFC 4122 v4
//!      UUID and assign it to `obligation_id`.
//!   4. If any verification entry was modified, re-serialize the
//!      `value_json` and write it back via UPDATE.
//!
//! ── Hard-fail policy ──────────────────────────────────────────
//!
//! Any structural invariant violation throws
//! `StorageError::MalformedRequirement` with the offending
//! `declaration_uid` and a reason. The migration is wrapped in a
//! SQLite transaction; the error propagates up, the transaction
//! is rolled back automatically when the `Transaction` is
//! dropped without `commit()`, and the database is left in the
//! pre-migration state.
//!
//! Mirrors the TypeScript `MalformedRequirementError` class
//! semantics exactly. The error message format is byte-equivalent
//! to the TS message: `"Migration 004: malformed requirement
//! declaration <uid>: <reason>"`.
//!
//! ── Idempotency ───────────────────────────────────────────────
//!
//! Recorded in `schema_migrations(version=4)`. Subsequent runs
//! find max_version >= 4 in the orchestrator and skip this
//! migration entirely. Within a single run, verification entries
//! that already have non-empty `obligation_id` are left alone.
//!
//! ── Empty-string `obligation_id` is treated as missing ────────
//!
//! Per the TS test "treats empty-string obligation_id as missing
//! and replaces it" — an entry with `obligation_id: ""` is
//! considered to lack an obligation_id and gets a fresh UUID.
//! This matches the TS check `typeof obl.obligation_id !==
//! "string" || obl.obligation_id.length === 0`.
//!
//! ── JSON key order preservation ───────────────────────────────
//!
//! `serde_json` is configured with the `preserve_order` feature
//! (see `Cargo.toml`). When parsing and re-serializing the
//! `value_json`, key insertion order is preserved exactly,
//! matching TypeScript `JSON.parse` + `JSON.stringify` behavior.
//! Without this feature, Rust would alphabetize keys and produce
//! semantically equivalent but byte-different output, causing
//! spurious parity failures.

use rusqlite::Connection;
use uuid::Uuid;

use crate::error::{malformed_requirement, StorageError};
use crate::migrations::record_migration;

/// Run migration 004 against the given connection.
///
/// Wraps all work in a single SQLite transaction so that any
/// hard-fail invariant violation rolls back partial state.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	let tx = conn.transaction()?;

	// Read all requirement declarations into memory. The TS code
	// does the same: load all rows, then iterate and update.
	let rows: Vec<(String, String)> = {
		let mut stmt = tx.prepare(
			"SELECT declaration_uid, value_json FROM declarations WHERE kind = 'requirement'",
		)?;
		let iter = stmt.query_map([], |row| {
			Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
		})?;
		iter.collect::<Result<Vec<_>, _>>()?
	};

	for (declaration_uid, value_json) in &rows {
		let updated = backfill_obligation_ids(declaration_uid, value_json)?;
		if let Some(new_json) = updated {
			tx.execute(
				"UPDATE declarations SET value_json = ? WHERE declaration_uid = ?",
				rusqlite::params![new_json, declaration_uid],
			)?;
		}
	}

	record_migration(&tx, 4, "004-obligation-ids")?;
	tx.commit()?;
	Ok(())
}

/// Parse a requirement declaration's `value_json`, validate its
/// structure, and backfill `obligation_id` on every verification
/// entry that lacks one.
///
/// Returns:
///   - `Ok(Some(string))` if changes were made (the new
///     re-serialized JSON)
///   - `Ok(None)` if no changes needed (all entries already have
///     a non-empty `obligation_id`, or there is no `verification`
///     field at all)
///   - `Err(StorageError::MalformedRequirement)` on any
///     structural invariant violation
///
/// **Pure function.** Takes a `&str`, returns owned data, no I/O.
/// All 17 test cases in `004-obligation-ids.test.ts` map directly
/// onto this function.
///
/// Mirrors the TS `backfillObligationIds(declarationUid,
/// valueJson)` function from `004-obligation-ids.ts:98`.
pub fn backfill_obligation_ids(
	declaration_uid: &str,
	value_json: &str,
) -> Result<Option<String>, StorageError> {
	// Parse step. JSON syntax errors → MalformedRequirement.
	let mut parsed: serde_json::Value = serde_json::from_str(value_json).map_err(|e| {
		malformed_requirement(
			declaration_uid,
			format!("value_json is not valid JSON: {}", e),
		)
	})?;

	// Top-level type check: must be a JSON object (not array, null, string, etc.).
	if !parsed.is_object() {
		return Err(malformed_requirement(
			declaration_uid,
			"value_json must be a JSON object",
		));
	}

	// Validate top-level invariants in their own scope so the
	// borrow ends before we take the mutable borrow below.
	{
		// Safe unwrap: we just verified is_object() above.
		let obj = parsed.as_object().expect("just verified is_object");

		// req_id must be a non-empty string.
		match obj.get("req_id") {
			Some(serde_json::Value::String(s)) if !s.is_empty() => {}
			_ => {
				return Err(malformed_requirement(
					declaration_uid,
					"req_id is missing or not a non-empty string",
				));
			}
		}

		// version must be a number.
		match obj.get("version") {
			Some(serde_json::Value::Number(_)) => {}
			_ => {
				return Err(malformed_requirement(
					declaration_uid,
					"version is missing or not a number",
				));
			}
		}
	}

	// Now mutate the verification array. Take the mutable borrow
	// in its own scope so it ends before re-serialization below.
	let mut changed = false;
	{
		let obj = parsed.as_object_mut().expect("just verified is_object");

		let verification = match obj.get_mut("verification") {
			None => {
				// No verification field → nothing to backfill, no error.
				return Ok(None);
			}
			Some(serde_json::Value::Array(arr)) => arr,
			Some(_) => {
				return Err(malformed_requirement(
					declaration_uid,
					"verification is not an array",
				));
			}
		};

		for (i, entry) in verification.iter_mut().enumerate() {
			let entry_obj = match entry.as_object_mut() {
				Some(o) => o,
				None => {
					return Err(malformed_requirement(
						declaration_uid,
						format!("verification[{}] is not an object", i),
					));
				}
			};

			// method must be a non-empty string.
			match entry_obj.get("method") {
				Some(serde_json::Value::String(s)) if !s.is_empty() => {}
				_ => {
					return Err(malformed_requirement(
						declaration_uid,
						format!(
							"verification[{}].method is missing or not a non-empty string",
							i
						),
					));
				}
			}

			// Check obligation_id. Missing OR non-string OR empty
			// string all count as "needs backfill".
			let needs_backfill = !matches!(
				entry_obj.get("obligation_id"),
				Some(serde_json::Value::String(s)) if !s.is_empty()
			);

			if needs_backfill {
				let new_uuid = Uuid::new_v4().to_string();
				entry_obj.insert(
					"obligation_id".to_string(),
					serde_json::Value::String(new_uuid),
				);
				changed = true;
			}
		}
	}

	if changed {
		let serialized = serde_json::to_string(&parsed).map_err(|e| {
			malformed_requirement(
				declaration_uid,
				format!("re-serialize failed: {}", e),
			)
		})?;
		Ok(Some(serialized))
	} else {
		Ok(None)
	}
}

#[cfg(test)]
mod tests {
	//! Pure-function tests for `backfill_obligation_ids`.
	//!
	//! Mirrors the 17 test cases from
	//! `test/adapters/storage/migrations/004-obligation-ids.test.ts`
	//! exactly. Each test below has the same intent as its
	//! TypeScript counterpart and pins the same behavioral contract.
	//!
	//! These tests do not exercise the `run` function directly
	//! (which requires a real connection); the
	//! `run_migrations_applies_all_sixteen_migrations` integration
	//! test in `migrations/mod.rs` covers the integration path.
	//! Per the TS test file's own comment: "The runMigration004
	//! integration path is covered indirectly by the existing
	//! storage tests (fresh databases run all migrations)."

	use super::*;

	// ── Happy path ────────────────────────────────────────────

	#[test]
	fn returns_none_when_there_is_no_verification_array() {
		let value = r#"{"req_id":"REQ-001","version":1,"objective":"x"}"#;
		assert!(backfill_obligation_ids("decl-1", value).unwrap().is_none());
	}

	#[test]
	fn returns_none_when_all_entries_already_have_obligation_id() {
		let value = r#"{"req_id":"REQ-001","version":2,"objective":"x","verification":[{"obligation_id":"existing-1","method":"arch_violations"},{"obligation_id":"existing-2","method":"coverage_threshold"}]}"#;
		assert!(backfill_obligation_ids("decl-1", value).unwrap().is_none());
	}

	#[test]
	fn assigns_obligation_id_to_entries_that_lack_one() {
		let value = r#"{"req_id":"REQ-001","version":2,"objective":"x","verification":[{"obligation":"a","method":"arch_violations"},{"obligation":"b","method":"coverage_threshold"}]}"#;
		let updated = backfill_obligation_ids("decl-1", value).unwrap();
		assert!(updated.is_some());

		let parsed: serde_json::Value =
			serde_json::from_str(&updated.unwrap()).unwrap();
		let entries = parsed
			.get("verification")
			.and_then(|v| v.as_array())
			.unwrap();
		assert_eq!(entries.len(), 2);

		let id_0 = entries[0].get("obligation_id").and_then(|v| v.as_str()).unwrap();
		let id_1 = entries[1].get("obligation_id").and_then(|v| v.as_str()).unwrap();
		assert!(!id_0.is_empty());
		assert!(!id_1.is_empty());
		assert_ne!(id_0, id_1, "fresh UUIDs must differ");
	}

	#[test]
	fn preserves_existing_obligation_id_while_filling_missing_ones() {
		let value = r#"{"req_id":"REQ-001","version":2,"objective":"x","verification":[{"obligation_id":"keep-me","method":"arch_violations"},{"obligation":"new","method":"coverage_threshold"}]}"#;
		let updated = backfill_obligation_ids("decl-1", value).unwrap();
		assert!(updated.is_some());

		let parsed: serde_json::Value =
			serde_json::from_str(&updated.unwrap()).unwrap();
		let entries = parsed
			.get("verification")
			.and_then(|v| v.as_array())
			.unwrap();

		assert_eq!(
			entries[0].get("obligation_id").and_then(|v| v.as_str()).unwrap(),
			"keep-me"
		);
		let id_1 = entries[1].get("obligation_id").and_then(|v| v.as_str()).unwrap();
		assert!(!id_1.is_empty());
		assert_ne!(id_1, "keep-me");
	}

	#[test]
	fn treats_empty_string_obligation_id_as_missing_and_replaces_it() {
		let value = r#"{"req_id":"REQ-001","version":2,"objective":"x","verification":[{"obligation_id":"","method":"arch_violations"}]}"#;
		let updated = backfill_obligation_ids("decl-1", value).unwrap();
		assert!(updated.is_some());

		let parsed: serde_json::Value =
			serde_json::from_str(&updated.unwrap()).unwrap();
		let id = parsed
			.get("verification")
			.and_then(|v| v.as_array())
			.unwrap()[0]
			.get("obligation_id")
			.and_then(|v| v.as_str())
			.unwrap();
		assert!(!id.is_empty());
	}

	// ── Hard-fail contracts ───────────────────────────────────

	#[test]
	fn throws_on_invalid_json() {
		let err = backfill_obligation_ids("decl-1", "{ not json").unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_value_is_not_a_json_object_array() {
		let err = backfill_obligation_ids("decl-1", "[]").unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_value_is_not_a_json_object_string() {
		let err = backfill_obligation_ids("decl-1", "\"string\"").unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_value_is_not_a_json_object_null() {
		let err = backfill_obligation_ids("decl-1", "null").unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_req_id_is_missing() {
		let value = r#"{"version":1,"objective":"x"}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_req_id_is_empty() {
		let value = r#"{"req_id":"","version":1,"objective":"x"}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_version_is_not_a_number() {
		let value = r#"{"req_id":"REQ-001","version":"1","objective":"x"}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_verification_is_not_an_array() {
		let value = r#"{"req_id":"REQ-001","version":1,"objective":"x","verification":"not an array"}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_a_verification_entry_is_not_an_object() {
		let value = r#"{"req_id":"REQ-001","version":1,"objective":"x","verification":["not an object"]}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_a_verification_entry_has_no_method() {
		let value = r#"{"req_id":"REQ-001","version":1,"objective":"x","verification":[{"obligation":"x"}]}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn throws_when_a_verification_entry_has_empty_method() {
		let value = r#"{"req_id":"REQ-001","version":1,"objective":"x","verification":[{"method":""}]}"#;
		let err = backfill_obligation_ids("decl-1", value).unwrap_err();
		assert!(matches!(err, StorageError::MalformedRequirement { .. }));
	}

	#[test]
	fn includes_declaration_uid_in_the_error_for_audit() {
		let err = backfill_obligation_ids("decl-abc", "{ bad json").unwrap_err();
		match err {
			StorageError::MalformedRequirement {
				declaration_uid,
				reason,
			} => {
				assert_eq!(declaration_uid, "decl-abc");
				assert!(reason.contains("not valid JSON"));
			}
			_ => panic!("wrong error variant"),
		}
		// Display format should contain the declaration_uid for audit logs.
		let err = backfill_obligation_ids("decl-abc", "{ bad json").unwrap_err();
		assert!(err.to_string().contains("decl-abc"));
	}

	// ── Cross-runtime parity properties ───────────────────────

	#[test]
	fn generated_obligation_ids_have_uuid_v4_shape() {
		// Per the user's R2-C lock note: parity for migration 004
		// must compare UUID SHAPE, not literal value. UUID v4 strings
		// match the regex /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.
		// This test pins the UUID shape contract.
		let value = r#"{"req_id":"REQ-001","version":1,"verification":[{"method":"arch_violations"}]}"#;
		let updated = backfill_obligation_ids("decl-1", value).unwrap().unwrap();
		let parsed: serde_json::Value = serde_json::from_str(&updated).unwrap();
		let id = parsed
			.get("verification")
			.and_then(|v| v.as_array())
			.unwrap()[0]
			.get("obligation_id")
			.and_then(|v| v.as_str())
			.unwrap();

		// UUID v4: 36 chars, 8-4-4-4-12 with hyphens, version nibble = 4,
		// variant nibble in {8,9,a,b}.
		assert_eq!(id.len(), 36, "UUID must be 36 chars");
		let bytes = id.as_bytes();
		assert_eq!(bytes[8], b'-');
		assert_eq!(bytes[13], b'-');
		assert_eq!(bytes[18], b'-');
		assert_eq!(bytes[23], b'-');
		assert_eq!(bytes[14], b'4', "version nibble must be '4' for UUID v4");
		assert!(
			matches!(bytes[19], b'8' | b'9' | b'a' | b'b'),
			"variant nibble must be one of 8/9/a/b, got {}",
			bytes[19] as char
		);
	}

	#[test]
	fn json_key_order_is_preserved_through_round_trip() {
		// Per the preserve_order serde_json feature lock: parsing
		// and re-serializing must preserve key insertion order so
		// Rust output byte-matches TS JSON.stringify output.
		//
		// This test pins the contract by checking that req_id
		// appears before version in the re-serialized JSON when
		// the input had them in that order.
		let value = r#"{"req_id":"REQ-001","version":1,"verification":[{"method":"arch_violations"}]}"#;
		let updated = backfill_obligation_ids("decl-1", value).unwrap().unwrap();
		let req_id_pos = updated.find("\"req_id\"").unwrap();
		let version_pos = updated.find("\"version\"").unwrap();
		let verification_pos = updated.find("\"verification\"").unwrap();
		assert!(
			req_id_pos < version_pos,
			"req_id must precede version in re-serialized JSON (preserve_order)"
		);
		assert!(
			version_pos < verification_pos,
			"version must precede verification in re-serialized JSON (preserve_order)"
		);
	}
}
