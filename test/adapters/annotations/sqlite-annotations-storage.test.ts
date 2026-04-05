/**
 * SqliteAnnotationsStorage — storage-layer integration tests.
 *
 * Covers:
 *   - CRUD (insert, delete by snapshot, count, read by target)
 *   - Deterministic ordering contract (§6 rules)
 *   - Target resolution 3-step precedence (§6 rules)
 *   - Target resolution error cases (ambiguity, not_found)
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteAnnotationsStorage } from "../../../src/adapters/annotations/sqlite-annotations-storage.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import type { Annotation } from "../../../src/core/annotations/types.js";
import {
	AnnotationContractClass,
	AnnotationKind,
	AnnotationTargetKind,
} from "../../../src/core/annotations/types.js";
import {
	NodeKind,
	NodeSubtype,
	SnapshotKind,
	Visibility,
} from "../../../src/core/model/index.js";

const REPO_UID = "ann-test-repo";

let provider: SqliteConnectionProvider;
let storage: SqliteStorage;
let annotations: SqliteAnnotationsStorage;
let dbPath: string;
let snapshotUid: string;

function makeAnnotation(
	overrides: Partial<Annotation> = {},
): Annotation {
	return {
		annotation_uid: randomUUID(),
		snapshot_uid: snapshotUid,
		target_kind: AnnotationTargetKind.MODULE,
		target_stable_key: `${REPO_UID}:src/core:MODULE`,
		annotation_kind: AnnotationKind.MODULE_README,
		contract_class: AnnotationContractClass.HINT,
		content: "default content",
		content_hash: "sha256:abc",
		source_file: "src/core/README.md",
		source_line_start: 1,
		source_line_end: 5,
		language: "markdown",
		provisional: true,
		extracted_at: new Date().toISOString(),
		...overrides,
	};
}

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-ann-test-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	annotations = new SqliteAnnotationsStorage(provider.getDatabase());

	storage.addRepo({
		repoUid: REPO_UID,
		name: "ann-test",
		rootPath: "/tmp/ann-test",
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});
	const snap = storage.createSnapshot({
		repoUid: REPO_UID,
		kind: SnapshotKind.FULL,
	});
	snapshotUid = snap.snapshotUid;
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// ignore
	}
});

// ── CRUD ────────────────────────────────────────────────────────

describe("SqliteAnnotationsStorage — CRUD", () => {
	it("inserts and reads annotations by target", () => {
		const a = makeAnnotation();
		annotations.insertAnnotations([a]);
		const read = annotations.getAnnotationsByTarget(
			snapshotUid,
			a.target_stable_key,
		);
		expect(read).toHaveLength(1);
		expect(read[0].annotation_uid).toBe(a.annotation_uid);
		expect(read[0].content).toBe(a.content);
		expect(read[0].provisional).toBe(true);
	});

	it("handles bulk insert", () => {
		const batch = Array.from({ length: 10 }, (_, i) =>
			makeAnnotation({
				content: `entry ${i}`,
				target_stable_key: `${REPO_UID}:src/mod-${i}:MODULE`,
			}),
		);
		annotations.insertAnnotations(batch);
		expect(annotations.countAnnotationsBySnapshot(snapshotUid)).toBe(10);
	});

	it("insertAnnotations is a no-op on empty input", () => {
		annotations.insertAnnotations([]);
		expect(annotations.countAnnotationsBySnapshot(snapshotUid)).toBe(0);
	});

	it("deleteAnnotationsBySnapshot removes all annotations for that snapshot", () => {
		annotations.insertAnnotations([makeAnnotation(), makeAnnotation()]);
		expect(annotations.countAnnotationsBySnapshot(snapshotUid)).toBe(2);
		annotations.deleteAnnotationsBySnapshot(snapshotUid);
		expect(annotations.countAnnotationsBySnapshot(snapshotUid)).toBe(0);
	});

	it("getAnnotationsByTarget returns empty array for unknown target", () => {
		const result = annotations.getAnnotationsByTarget(
			snapshotUid,
			"nonexistent:target:MODULE",
		);
		expect(result).toEqual([]);
	});
});

// ── Ordering contract (§6) ──────────────────────────────────────

describe("SqliteAnnotationsStorage — ordering", () => {
	it("orders by annotation_kind fixed enum", () => {
		const target = `${REPO_UID}:src/core:MODULE`;
		// Insert in reverse order to verify sort
		annotations.insertAnnotations([
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.JSDOC_BLOCK,
				content: "jsdoc",
				source_file: "a.ts",
			}),
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.FILE_HEADER_COMMENT,
				content: "file header",
				source_file: "a.ts",
			}),
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.MODULE_README,
				content: "module readme",
				source_file: "README.md",
			}),
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.PACKAGE_DESCRIPTION,
				content: "package desc",
				source_file: "package.json",
			}),
		]);
		const read = annotations.getAnnotationsByTarget(snapshotUid, target);
		expect(read.map((a) => a.annotation_kind)).toEqual([
			AnnotationKind.PACKAGE_DESCRIPTION,
			AnnotationKind.MODULE_README,
			AnnotationKind.FILE_HEADER_COMMENT,
			AnnotationKind.JSDOC_BLOCK,
		]);
	});

	it("orders by source_file alphabetical, then source_line_start ascending", () => {
		const target = `${REPO_UID}:src/core:MODULE`;
		annotations.insertAnnotations([
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.JSDOC_BLOCK,
				source_file: "z.ts",
				source_line_start: 5,
			}),
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.JSDOC_BLOCK,
				source_file: "a.ts",
				source_line_start: 20,
			}),
			makeAnnotation({
				target_stable_key: target,
				annotation_kind: AnnotationKind.JSDOC_BLOCK,
				source_file: "a.ts",
				source_line_start: 1,
			}),
		]);
		const read = annotations.getAnnotationsByTarget(snapshotUid, target);
		expect(read.map((a) => `${a.source_file}:${a.source_line_start}`)).toEqual(
			["a.ts:1", "a.ts:20", "z.ts:5"],
		);
	});
});

// ── Target resolution ──────────────────────────────────────────

describe("SqliteAnnotationsStorage — resolveDocsTarget", () => {
	function insertModuleNode(path: string, name?: string) {
		storage.insertNodes([
			{
				nodeUid: randomUUID(),
				snapshotUid,
				repoUid: REPO_UID,
				stableKey: `${REPO_UID}:${path}:MODULE`,
				kind: NodeKind.MODULE,
				subtype: NodeSubtype.DIRECTORY,
				name: name ?? path.split("/").pop() ?? path,
				qualifiedName: path,
				fileUid: null,
				parentNodeUid: null,
				location: { lineStart: 0, colStart: 0, lineEnd: 0, colEnd: 0 },
				signature: null,
				visibility: Visibility.EXPORT,
				docComment: null,
				metadataJson: null,
			},
		]);
	}

	function insertFileNode(path: string) {
		// Insert the file row first to satisfy the FK on nodes.file_uid
		storage.upsertFiles([
			{
				fileUid: `${REPO_UID}:${path}`,
				repoUid: REPO_UID,
				path,
				language: "typescript",
				isTest: false,
				isGenerated: false,
				isExcluded: false,
			},
		]);
		storage.insertNodes([
			{
				nodeUid: randomUUID(),
				snapshotUid,
				repoUid: REPO_UID,
				stableKey: `${REPO_UID}:${path}:FILE`,
				kind: NodeKind.FILE,
				subtype: NodeSubtype.SOURCE,
				name: path.split("/").pop() ?? path,
				qualifiedName: path,
				fileUid: `${REPO_UID}:${path}`,
				parentNodeUid: null,
				location: { lineStart: 0, colStart: 0, lineEnd: 0, colEnd: 0 },
				signature: null,
				visibility: Visibility.EXPORT,
				docComment: null,
				metadataJson: null,
			},
		]);
	}

	it("step 1: exact stable_key match wins when annotations exist for it", () => {
		const sk = `${REPO_UID}:src/core:MODULE`;
		annotations.insertAnnotations([
			makeAnnotation({ target_stable_key: sk }),
		]);
		const result = annotations.resolveDocsTarget(snapshotUid, sk);
		expect(result).toEqual({ kind: "resolved", targetStableKey: sk });
	});

	it("step 2: exact path match against MODULE nodes", () => {
		insertModuleNode("src/core");
		const result = annotations.resolveDocsTarget(snapshotUid, "src/core");
		expect(result).toEqual({
			kind: "resolved",
			targetStableKey: `${REPO_UID}:src/core:MODULE`,
		});
	});

	it("step 2: exact path match against FILE nodes", () => {
		insertFileNode("src/core/index.ts");
		const result = annotations.resolveDocsTarget(
			snapshotUid,
			"src/core/index.ts",
		);
		expect(result).toEqual({
			kind: "resolved",
			targetStableKey: `${REPO_UID}:src/core/index.ts:FILE`,
		});
	});

	it("step 2: ambiguous when both FILE and MODULE share a path", () => {
		insertModuleNode("src/core");
		insertFileNode("src/core");
		const result = annotations.resolveDocsTarget(snapshotUid, "src/core");
		expect(result.kind).toBe("ambiguous");
		if (result.kind === "ambiguous") {
			expect(result.step).toBe("path");
			expect(result.candidates).toHaveLength(2);
		}
	});

	it("step 3: exact module name match", () => {
		insertModuleNode("packages/engine/src/core", "core");
		const result = annotations.resolveDocsTarget(snapshotUid, "core");
		expect(result).toEqual({
			kind: "resolved",
			targetStableKey: `${REPO_UID}:packages/engine/src/core:MODULE`,
		});
	});

	it("step 3: ambiguous when multiple modules share a name", () => {
		insertModuleNode("packages/a/src/core", "core");
		insertModuleNode("packages/b/src/core", "core");
		const result = annotations.resolveDocsTarget(snapshotUid, "core");
		expect(result.kind).toBe("ambiguous");
		if (result.kind === "ambiguous") {
			expect(result.step).toBe("module_name");
			expect(result.candidates).toHaveLength(2);
		}
	});

	it("returns not_found when no match at any step", () => {
		const result = annotations.resolveDocsTarget(
			snapshotUid,
			"totally-nonexistent-xyz",
		);
		expect(result).toEqual({ kind: "not_found" });
	});

	it("step 2 takes precedence over step 3 (path before name)", () => {
		// a MODULE named "core" at path "core" exists
		insertModuleNode("core", "core");
		// Additional module with qualified_name different but name=core
		insertModuleNode("other/nested/core", "nested");
		const result = annotations.resolveDocsTarget(snapshotUid, "core");
		// Path match (single result) wins at step 2
		expect(result.kind).toBe("resolved");
		if (result.kind === "resolved") {
			expect(result.targetStableKey).toBe(`${REPO_UID}:core:MODULE`);
		}
	});
});
