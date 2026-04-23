/**
 * Unit tests for module discovery diagnostics.
 *
 * Tests cover:
 *   - deterministic UID generation (same input → same UID)
 *   - UID uniqueness (different inputs → different UIDs)
 *   - storage insert/query round-trip
 *   - filter functionality
 */

import { beforeEach, describe, expect, it } from "vitest";
import {
	buildDiagnosticUid,
	type ModuleDiscoveryDiagnosticInput,
} from "../../../src/core/modules/module-candidate.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";

// ── Deterministic UID generation ───────────────────────────────────

describe("buildDiagnosticUid — determinism", () => {
	const baseDiagnostic: ModuleDiscoveryDiagnosticInput = {
		snapshotUid: "snap-1",
		sourceType: "kbuild",
		diagnosticKind: "skipped_config_gated",
		filePath: "drivers/net/Makefile",
		line: 42,
		rawText: "obj-$(CONFIG_NET) += net/",
		message: "Skipped: CONFIG_* gated assignment",
		severity: "info",
		metadataJson: null,
	};

	it("generates identical UID for identical input", () => {
		const uid1 = buildDiagnosticUid(baseDiagnostic);
		const uid2 = buildDiagnosticUid({ ...baseDiagnostic });
		expect(uid1).toBe(uid2);
	});

	it("generates UID starting with 'diag:' prefix and 32 hex chars", () => {
		const uid = buildDiagnosticUid(baseDiagnostic);
		// 128-bit truncated SHA-256 = 32 hex chars
		expect(uid).toMatch(/^diag:[0-9a-f]{32}$/);
	});

	it("generates different UID for different file path", () => {
		const uid1 = buildDiagnosticUid(baseDiagnostic);
		const uid2 = buildDiagnosticUid({
			...baseDiagnostic,
			filePath: "drivers/usb/Makefile",
		});
		expect(uid1).not.toBe(uid2);
	});

	it("generates different UID for different line number", () => {
		const uid1 = buildDiagnosticUid(baseDiagnostic);
		const uid2 = buildDiagnosticUid({
			...baseDiagnostic,
			line: 100,
		});
		expect(uid1).not.toBe(uid2);
	});

	it("generates different UID for different diagnostic kind", () => {
		const uid1 = buildDiagnosticUid(baseDiagnostic);
		const uid2 = buildDiagnosticUid({
			...baseDiagnostic,
			diagnosticKind: "skipped_conditional",
		});
		expect(uid1).not.toBe(uid2);
	});

	it("generates different UID for different snapshot", () => {
		const uid1 = buildDiagnosticUid(baseDiagnostic);
		const uid2 = buildDiagnosticUid({
			...baseDiagnostic,
			snapshotUid: "snap-2",
		});
		expect(uid1).not.toBe(uid2);
	});

	it("handles null line correctly", () => {
		const withLine = buildDiagnosticUid(baseDiagnostic);
		const withoutLine = buildDiagnosticUid({
			...baseDiagnostic,
			line: null,
		});
		expect(withLine).not.toBe(withoutLine);
	});

	it("handles null rawText correctly", () => {
		const withRaw = buildDiagnosticUid(baseDiagnostic);
		const withoutRaw = buildDiagnosticUid({
			...baseDiagnostic,
			rawText: null,
		});
		expect(withRaw).not.toBe(withoutRaw);
	});
});

// ── Storage round-trip ─────────────────────────────────────────────

describe("module discovery diagnostics — storage", () => {
	let provider: SqliteConnectionProvider;
	let storage: SqliteStorage;
	let snapshotUid: string;

	beforeEach(() => {
		provider = new SqliteConnectionProvider(":memory:");
		provider.initialize();
		storage = new SqliteStorage(provider.getDatabase());

		// Create minimal repo and snapshot for FK constraints
		storage.addRepo({
			repoUid: "r1",
			name: "test-repo",
			rootPath: "/tmp/test",
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});
		const snap = storage.createSnapshot({
			repoUid: "r1",
			kind: "full",
			basisCommit: null,
			parentSnapshotUid: null,
		});
		snapshotUid = snap.snapshotUid;
	});

	it("inserts and queries diagnostics", () => {
		const diagnostics: ModuleDiscoveryDiagnosticInput[] = [
			{
				snapshotUid,
				sourceType: "kbuild",
				diagnosticKind: "skipped_conditional",
				filePath: "drivers/Makefile",
				line: 10,
				rawText: "ifeq ($(CONFIG_NET),y)",
				message: "Skipped: conditional directive",
				severity: "info",
				metadataJson: null,
			},
			{
				snapshotUid,
				sourceType: "kbuild",
				diagnosticKind: "skipped_config_gated",
				filePath: "drivers/Makefile",
				line: 20,
				rawText: "obj-$(CONFIG_USB) += usb/",
				message: "Skipped: CONFIG_* gated assignment",
				severity: "info",
				metadataJson: null,
			},
		];

		storage.insertModuleDiscoveryDiagnostics(diagnostics);

		const results = storage.queryModuleDiscoveryDiagnostics(snapshotUid);
		expect(results).toHaveLength(2);
		expect(results[0].sourceType).toBe("kbuild");
		expect(results[0].diagnosticKind).toBe("skipped_conditional");
		expect(results[1].diagnosticKind).toBe("skipped_config_gated");
	});

	it("filters by source type", () => {
		const diagnostics: ModuleDiscoveryDiagnosticInput[] = [
			{
				snapshotUid,
				sourceType: "kbuild",
				diagnosticKind: "skipped_conditional",
				filePath: "drivers/Makefile",
				line: 10,
				rawText: null,
				message: "Skipped",
				severity: "info",
				metadataJson: null,
			},
		];

		storage.insertModuleDiscoveryDiagnostics(diagnostics);

		const kbuildResults = storage.queryModuleDiscoveryDiagnostics(snapshotUid, {
			sourceType: "kbuild",
		});
		expect(kbuildResults).toHaveLength(1);

		const otherResults = storage.queryModuleDiscoveryDiagnostics(snapshotUid, {
			sourceType: "other",
		});
		expect(otherResults).toHaveLength(0);
	});

	it("filters by diagnostic kind", () => {
		const diagnostics: ModuleDiscoveryDiagnosticInput[] = [
			{
				snapshotUid,
				sourceType: "kbuild",
				diagnosticKind: "skipped_conditional",
				filePath: "a/Makefile",
				line: 1,
				rawText: null,
				message: "A",
				severity: "info",
				metadataJson: null,
			},
			{
				snapshotUid,
				sourceType: "kbuild",
				diagnosticKind: "malformed_assignment",
				filePath: "b/Makefile",
				line: 2,
				rawText: null,
				message: "B",
				severity: "warning",
				metadataJson: null,
			},
		];

		storage.insertModuleDiscoveryDiagnostics(diagnostics);

		const conditionalResults = storage.queryModuleDiscoveryDiagnostics(
			snapshotUid,
			{ diagnosticKind: "skipped_conditional" },
		);
		expect(conditionalResults).toHaveLength(1);
		expect(conditionalResults[0].message).toBe("A");

		const warningResults = storage.queryModuleDiscoveryDiagnostics(snapshotUid, {
			diagnosticKind: "malformed_assignment",
		});
		expect(warningResults).toHaveLength(1);
		expect(warningResults[0].severity).toBe("warning");
	});

	it("handles duplicate inserts idempotently", () => {
		const diagnostic: ModuleDiscoveryDiagnosticInput = {
			snapshotUid,
			sourceType: "kbuild",
			diagnosticKind: "skipped_variable",
			filePath: "drivers/Makefile",
			line: 15,
			rawText: "obj-y += $(SUBDIR)/",
			message: "Skipped: variable reference",
			severity: "info",
			metadataJson: null,
		};

		// Insert twice
		storage.insertModuleDiscoveryDiagnostics([diagnostic]);
		storage.insertModuleDiscoveryDiagnostics([diagnostic]);

		const results = storage.queryModuleDiscoveryDiagnostics(snapshotUid);
		expect(results).toHaveLength(1);
	});

	it("returns empty array for snapshot with no diagnostics", () => {
		const results = storage.queryModuleDiscoveryDiagnostics("nonexistent");
		expect(results).toEqual([]);
	});
});
