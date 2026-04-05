/**
 * SQLite implementation of AnnotationsPort.
 *
 * This adapter owns reads and writes to the `annotations` table.
 * It receives a shared Database handle from SqliteConnectionProvider.
 *
 * Isolation invariant (annotations-contract.txt §7):
 * This adapter is the only path by which annotations are persisted
 * or retrieved. It is imported ONLY by:
 *   - src/adapters/indexer/*  (write-only, during extraction)
 *   - src/cli/commands/docs.ts (read-only)
 *   - src/main.ts             (composition root)
 *
 * The architecture isolation test verifies no imports from
 * computed-truth paths.
 */

import type Database from "better-sqlite3";
import type {
	Annotation,
	AnnotationContractClass,
	AnnotationKind,
	AnnotationTargetKind,
} from "../../core/annotations/types.js";
import type {
	AnnotationsPort,
	DocsTargetResolution,
} from "../../core/ports/annotations.js";

export class SqliteAnnotationsStorage implements AnnotationsPort {
	/**
	 * The shared Database handle owned by SqliteConnectionProvider.
	 * This adapter does NOT open, close, or migrate the database.
	 */
	constructor(private db: Database.Database) {}

	insertAnnotations(annotations: Annotation[]): void {
		if (annotations.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT INTO annotations
			 (annotation_uid, snapshot_uid, target_kind, target_stable_key,
			  annotation_kind, contract_class, content, content_hash,
			  source_file, source_line_start, source_line_end, language,
			  provisional, extracted_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const a of annotations) {
				stmt.run(
					a.annotation_uid,
					a.snapshot_uid,
					a.target_kind,
					a.target_stable_key,
					a.annotation_kind,
					a.contract_class,
					a.content,
					a.content_hash,
					a.source_file,
					a.source_line_start,
					a.source_line_end,
					a.language,
					a.provisional ? 1 : 0,
					a.extracted_at,
				);
			}
		});
		insertAll();
	}

	deleteAnnotationsBySnapshot(snapshotUid: string): void {
		this.db
			.prepare("DELETE FROM annotations WHERE snapshot_uid = ?")
			.run(snapshotUid);
	}

	getAnnotationsByTarget(
		snapshotUid: string,
		targetStableKey: string,
	): Annotation[] {
		// Deterministic ordering per contract §6 rule:
		//   1. target_stable_key alphabetical
		//   2. annotation_kind fixed order
		//   3. source_file alphabetical
		//   4. source_line_start ascending
		const rows = this.db
			.prepare(
				`SELECT
				   annotation_uid, snapshot_uid, target_kind, target_stable_key,
				   annotation_kind, contract_class, content, content_hash,
				   source_file, source_line_start, source_line_end, language,
				   provisional, extracted_at
				 FROM annotations
				 WHERE snapshot_uid = ? AND target_stable_key = ?
				 ORDER BY
				   target_stable_key ASC,
				   CASE annotation_kind
				     WHEN 'package_description' THEN 1
				     WHEN 'module_readme' THEN 2
				     WHEN 'file_header_comment' THEN 3
				     WHEN 'jsdoc_block' THEN 4
				     ELSE 5
				   END ASC,
				   source_file ASC,
				   source_line_start ASC`,
			)
			.all(snapshotUid, targetStableKey) as Record<string, unknown>[];
		return rows.map(mapRow);
	}

	countAnnotationsBySnapshot(snapshotUid: string): number {
		const row = this.db
			.prepare("SELECT COUNT(*) AS c FROM annotations WHERE snapshot_uid = ?")
			.get(snapshotUid) as { c: number };
		return row.c;
	}

	resolveDocsTarget(snapshotUid: string, query: string): DocsTargetResolution {
		// Repo alias: "." or "" resolves to the repo-level target
		// (target_kind='repo'). Repo targets are synthetic — no graph
		// node corresponds to them — so they cannot be reached via
		// step 2 (path) or step 3 (module name). The "." alias is the
		// stable surface for agents to request repo-level annotations
		// without knowing the repo_uid.
		if (query === "." || query === "") {
			const repoHit = this.db
				.prepare(
					`SELECT DISTINCT target_stable_key FROM annotations
					 WHERE snapshot_uid = ? AND target_kind = 'repo'`,
				)
				.all(snapshotUid) as Array<{ target_stable_key: string }>;
			if (repoHit.length === 1) {
				return {
					kind: "resolved",
					targetStableKey: repoHit[0].target_stable_key,
				};
			}
			if (repoHit.length > 1) {
				// Defensive: should not occur under current attribution rules
				return {
					kind: "ambiguous",
					step: "path",
					candidates: repoHit.map((r) => r.target_stable_key),
				};
			}
			// Falls through to regular resolution if no repo-level
			// annotation exists. Treat "." as not_found in that case.
			return { kind: "not_found" };
		}

		// Step 1: exact stable_key match in the annotations table.
		// The contract anchors this lookup to stored annotations —
		// if a stable_key has annotations in this snapshot, that is
		// the target. stable_keys are unique identities, no ambiguity.
		const stableKeyHit = this.db
			.prepare(
				`SELECT DISTINCT target_stable_key FROM annotations
				 WHERE snapshot_uid = ? AND target_stable_key = ?`,
			)
			.get(snapshotUid, query) as
			| { target_stable_key: string }
			| undefined;
		if (stableKeyHit) {
			return {
				kind: "resolved",
				targetStableKey: stableKeyHit.target_stable_key,
			};
		}

		// Step 2: exact repo-relative path match against FILE + MODULE nodes.
		// stable_key format: `<repo_uid>:<path>:FILE` or `<repo_uid>:<path>:MODULE`.
		// We extract the middle segment (path) via SUBSTR to match exactly.
		const pathMatches = this.db
			.prepare(
				`SELECT stable_key FROM nodes
				 WHERE snapshot_uid = ?
				   AND kind IN ('FILE', 'MODULE')
				   AND stable_key LIKE '%:' || ? || ':FILE' ESCAPE '\\'
				 UNION
				 SELECT stable_key FROM nodes
				 WHERE snapshot_uid = ?
				   AND kind IN ('FILE', 'MODULE')
				   AND stable_key LIKE '%:' || ? || ':MODULE' ESCAPE '\\'`,
			)
			.all(snapshotUid, query, snapshotUid, query) as Array<{
			stable_key: string;
		}>;
		if (pathMatches.length === 1) {
			return {
				kind: "resolved",
				targetStableKey: pathMatches[0].stable_key,
			};
		}
		if (pathMatches.length > 1) {
			return {
				kind: "ambiguous",
				step: "path",
				candidates: pathMatches.map((r) => r.stable_key),
			};
		}

		// Step 3: exact module name / qualified_name match on MODULE nodes.
		const moduleMatches = this.db
			.prepare(
				`SELECT stable_key FROM nodes
				 WHERE snapshot_uid = ?
				   AND kind = 'MODULE'
				   AND (name = ? OR qualified_name = ?)`,
			)
			.all(snapshotUid, query, query) as Array<{ stable_key: string }>;
		if (moduleMatches.length === 1) {
			return {
				kind: "resolved",
				targetStableKey: moduleMatches[0].stable_key,
			};
		}
		if (moduleMatches.length > 1) {
			return {
				kind: "ambiguous",
				step: "module_name",
				candidates: moduleMatches.map((r) => r.stable_key),
			};
		}

		return { kind: "not_found" };
	}
}

function mapRow(row: Record<string, unknown>): Annotation {
	return {
		annotation_uid: row.annotation_uid as string,
		snapshot_uid: row.snapshot_uid as string,
		target_kind: row.target_kind as AnnotationTargetKind,
		target_stable_key: row.target_stable_key as string,
		annotation_kind: row.annotation_kind as AnnotationKind,
		contract_class: row.contract_class as AnnotationContractClass,
		content: row.content as string,
		content_hash: row.content_hash as string,
		source_file: row.source_file as string,
		source_line_start: row.source_line_start as number,
		source_line_end: row.source_line_end as number,
		language: row.language as string,
		provisional: (row.provisional as number) === 1,
		extracted_at: row.extracted_at as string,
	};
}
