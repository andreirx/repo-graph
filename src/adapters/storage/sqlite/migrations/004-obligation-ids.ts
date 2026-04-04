/**
 * Migration 004: Backfill obligation_id on verification entries.
 *
 * Rationale
 * ---------
 * VerificationObligation now carries a stable `obligation_id` field that
 * serves as the reference target for waiver declarations. Pre-migration
 * requirement declarations stored obligations without this field.
 *
 * This migration assigns a fresh UUID to every verification entry
 * that lacks one, across ALL requirement declarations (active and
 * historical). The semantic-match inheritance rule applies only to
 * post-migration supersessions — pre-existing supersession chains are
 * NOT retroactively re-linked, because no waivers exist pre-migration,
 * so inheritance has no waiver-continuity cost to preserve.
 *
 * Hard-fail policy
 * ----------------
 * This migration fails loudly on malformed legacy data. Silent skipping
 * would corrupt the audit trail. Expected structural invariants:
 *   - value_json must parse as a JSON object
 *   - req_id must be a non-empty string
 *   - version must be a number
 *   - verification (if present) must be an array
 *   - each verification entry must have a non-empty method string
 *
 * Violation of any of the above throws MalformedRequirementError and
 * aborts the migration. The database is left in the pre-migration
 * state (SQLite transaction rollback).
 *
 * Idempotence
 * -----------
 * Recorded in schema_migrations(version=4). Subsequent runs are no-ops.
 * Within a single run, entries that already have obligation_id are
 * left alone.
 */

import { randomUUID } from "node:crypto";
import type Database from "better-sqlite3";

export class MalformedRequirementError extends Error {
	constructor(
		public readonly declarationUid: string,
		public readonly reason: string,
	) {
		super(
			`Migration 004: malformed requirement declaration ${declarationUid}: ${reason}`,
		);
		this.name = "MalformedRequirementError";
	}
}

interface DeclarationRow {
	declaration_uid: string;
	value_json: string;
}

export function runMigration004(db: Database.Database): void {
	const rows = db
		.prepare(
			"SELECT declaration_uid, value_json FROM declarations WHERE kind = 'requirement'",
		)
		.all() as DeclarationRow[];

	const updateStmt = db.prepare(
		"UPDATE declarations SET value_json = ? WHERE declaration_uid = ?",
	);

	const processAll = db.transaction(() => {
		for (const row of rows) {
			const updated = backfillObligationIds(row.declaration_uid, row.value_json);
			if (updated !== null) {
				updateStmt.run(updated, row.declaration_uid);
			}
		}

		db.prepare(
			"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (4, '004-obligation-ids', datetime('now'))",
		).run();
	});

	processAll();
}

/**
 * Parse a requirement declaration's value_json, validate its structure,
 * and backfill obligation_id on every verification entry that lacks one.
 *
 * Returns:
 *   - the updated JSON string if changes were made
 *   - null if no changes needed (all entries already have obligation_id,
 *     or there is no verification array)
 *
 * Throws MalformedRequirementError on any structural violation.
 *
 * Exported for direct testing.
 */
export function backfillObligationIds(
	declarationUid: string,
	valueJson: string,
): string | null {
	let parsed: unknown;
	try {
		parsed = JSON.parse(valueJson);
	} catch (err) {
		throw new MalformedRequirementError(
			declarationUid,
			`value_json is not valid JSON: ${err instanceof Error ? err.message : String(err)}`,
		);
	}

	if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
		throw new MalformedRequirementError(
			declarationUid,
			"value_json must be a JSON object",
		);
	}

	const obj = parsed as Record<string, unknown>;

	// Structural invariants
	if (typeof obj.req_id !== "string" || obj.req_id.length === 0) {
		throw new MalformedRequirementError(
			declarationUid,
			"req_id is missing or not a non-empty string",
		);
	}
	if (typeof obj.version !== "number") {
		throw new MalformedRequirementError(
			declarationUid,
			"version is missing or not a number",
		);
	}

	if (obj.verification === undefined) {
		return null; // nothing to backfill
	}
	if (!Array.isArray(obj.verification)) {
		throw new MalformedRequirementError(
			declarationUid,
			"verification is not an array",
		);
	}

	let changed = false;
	for (let i = 0; i < obj.verification.length; i++) {
		const entry = obj.verification[i] as unknown;
		if (typeof entry !== "object" || entry === null) {
			throw new MalformedRequirementError(
				declarationUid,
				`verification[${i}] is not an object`,
			);
		}
		const obl = entry as Record<string, unknown>;
		if (typeof obl.method !== "string" || obl.method.length === 0) {
			throw new MalformedRequirementError(
				declarationUid,
				`verification[${i}].method is missing or not a non-empty string`,
			);
		}
		if (typeof obl.obligation_id !== "string" || obl.obligation_id.length === 0) {
			obl.obligation_id = randomUUID();
			changed = true;
		}
	}

	return changed ? JSON.stringify(obj) : null;
}
