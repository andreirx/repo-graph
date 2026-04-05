/**
 * Effective-verdict resolver tests.
 *
 * Uses real SQLite storage to exercise the full evaluator + waiver
 * lookup stack. The resolver is a thin wrapper, so integration-style
 * tests are more honest than mock-based unit tests: they verify that
 * the storage query contract (active, non-expired, tuple-matched)
 * actually behaves correctly.
 *
 * Core audit principle under test:
 *   computed_verdict is NEVER erased by a waiver.
 *   effective_verdict is the governance overlay.
 *   waiver_basis captures the full exception audit trail.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { resolveEffectiveVerdict } from "../../../src/core/evaluator/effective-verdict.js";
import type { VerificationObligation } from "../../../src/core/model/declaration.js";
import {
	DeclarationKind,
	SnapshotKind,
} from "../../../src/core/model/index.js";

const REPO_UID = "eff-repo";
const REQ_ID = "REQ-EFF-001";
const OBL_ID = "obl-eff-id";
const SNAPSHOT_UID_PLACEHOLDER = "placeholder-snap";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let dbPath: string;
let snapshotUid: string;

function insertWaiver(value: {
	req_id?: string;
	requirement_version?: number;
	obligation_id?: string;
	reason?: string;
	expires_at?: string;
	created_by?: string;
	rationale_category?: string;
	policy_basis?: string;
	is_active?: boolean;
	created_at?: string;
}): string {
	const uid = randomUUID();
	const payload = {
		req_id: value.req_id ?? REQ_ID,
		requirement_version: value.requirement_version ?? 1,
		obligation_id: value.obligation_id ?? OBL_ID,
		reason: value.reason ?? "test waiver",
		created_at: value.created_at ?? new Date().toISOString(),
		...(value.created_by && { created_by: value.created_by }),
		...(value.expires_at && { expires_at: value.expires_at }),
		...(value.rationale_category && {
			rationale_category: value.rationale_category,
		}),
		...(value.policy_basis && { policy_basis: value.policy_basis }),
	};
	storage.insertDeclaration({
		declarationUid: uid,
		repoUid: REPO_UID,
		snapshotUid: null,
		targetStableKey: `${REPO_UID}:waiver:${payload.req_id}`,
		kind: DeclarationKind.WAIVER,
		valueJson: JSON.stringify(payload),
		createdAt: payload.created_at,
		createdBy: "test",
		supersedesUid: null,
		isActive: value.is_active ?? true,
		authoredBasisJson: null,
	});
	return uid;
}

// Manual-review obligation produces UNSUPPORTED from the evaluator
// without needing any measurements. Keeps test setup minimal while
// exercising the waiver overlay logic faithfully.
const unsupportedObligation: VerificationObligation = {
	obligation_id: OBL_ID,
	obligation: "manual review required",
	method: "manual_review",
};

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-effective-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());

	storage.addRepo({
		repoUid: REPO_UID,
		name: "eff-test",
		rootPath: "/tmp/eff",
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});

	const snap = storage.createSnapshot({
		repoUid: REPO_UID,
		kind: SnapshotKind.FULL,
	});
	snapshotUid = snap.snapshotUid;
	// Mark as placeholder; evaluator doesn't require snapshot status
	// for the test's UNSUPPORTED path.
	void SNAPSHOT_UID_PLACEHOLDER;
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// best effort
	}
});

describe("resolveEffectiveVerdict", () => {
	it("no waiver: effective_verdict == computed_verdict", () => {
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.computed_verdict).toBe("UNSUPPORTED");
		expect(result.effective_verdict).toBe("UNSUPPORTED");
		expect(result.waiver_basis).toBeNull();
	});

	it("active waiver: effective_verdict == WAIVED, computed preserved", () => {
		insertWaiver({
			reason: "pilot phase, tracked in ADR-003",
			created_by: "ops-lead",
			rationale_category: "pilot",
			policy_basis: "gate-v1",
		});
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		// Audit preservation: computed verdict is unchanged
		expect(result.computed_verdict).toBe("UNSUPPORTED");
		// Governance overlay: effective becomes WAIVED
		expect(result.effective_verdict).toBe("WAIVED");
		// Waiver audit trail is surfaced
		expect(result.waiver_basis).not.toBeNull();
		expect(result.waiver_basis!.reason).toBe(
			"pilot phase, tracked in ADR-003",
		);
		expect(result.waiver_basis!.created_by).toBe("ops-lead");
		expect(result.waiver_basis!.rationale_category).toBe("pilot");
		expect(result.waiver_basis!.policy_basis).toBe("gate-v1");
		expect(result.waiver_basis!.waiver_uid).toBeDefined();
	});

	it("waiver with future expires_at: still suppresses verdict", () => {
		insertWaiver({ expires_at: "2099-01-01T00:00:00Z" });
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.effective_verdict).toBe("WAIVED");
		expect(result.waiver_basis!.expires_at).toBe("2099-01-01T00:00:00Z");
	});

	it("expired waiver (via now param): effective == computed", () => {
		insertWaiver({ expires_at: "2025-06-01T00:00:00Z" });
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
			"2025-12-01T00:00:00Z",
		);
		expect(result.computed_verdict).toBe("UNSUPPORTED");
		expect(result.effective_verdict).toBe("UNSUPPORTED");
		expect(result.waiver_basis).toBeNull();
	});

	it("waiver for different obligation_id: no suppression", () => {
		insertWaiver({ obligation_id: "different-obl" });
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.effective_verdict).toBe("UNSUPPORTED");
		expect(result.waiver_basis).toBeNull();
	});

	it("waiver for different requirement_version: no suppression", () => {
		insertWaiver({ requirement_version: 2 });
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.effective_verdict).toBe("UNSUPPORTED");
		expect(result.waiver_basis).toBeNull();
	});

	it("waiver for different req_id: no suppression", () => {
		insertWaiver({ req_id: "REQ-OTHER" });
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.effective_verdict).toBe("UNSUPPORTED");
		expect(result.waiver_basis).toBeNull();
	});

	it("deactivated waiver: no suppression", () => {
		const waiverUid = insertWaiver({});
		storage.deactivateDeclaration(waiverUid);
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.effective_verdict).toBe("UNSUPPORTED");
		expect(result.waiver_basis).toBeNull();
	});

	it("multiple active waivers: most recent wins", () => {
		// Older waiver
		insertWaiver({
			reason: "old reason",
			created_at: "2025-01-01T00:00:00Z",
		});
		// Newer waiver
		insertWaiver({
			reason: "new reason",
			created_at: "2026-01-01T00:00:00Z",
		});
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.effective_verdict).toBe("WAIVED");
		expect(result.waiver_basis!.reason).toBe("new reason");
	});

	it("waiver_basis defaults optional fields to null", () => {
		// Minimal waiver (only required fields)
		insertWaiver({ reason: "bare minimum" });
		const result = resolveEffectiveVerdict(
			unsupportedObligation,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.waiver_basis).not.toBeNull();
		expect(result.waiver_basis!.reason).toBe("bare minimum");
		expect(result.waiver_basis!.created_by).toBeNull();
		expect(result.waiver_basis!.expires_at).toBeNull();
		expect(result.waiver_basis!.rationale_category).toBeNull();
		expect(result.waiver_basis!.policy_basis).toBeNull();
	});

	it("obligation fields pass through unchanged", () => {
		const obl: VerificationObligation = {
			obligation_id: "obl-passthrough",
			obligation: "text",
			method: "manual_review",
			target: "src/core",
			threshold: 0.5,
			operator: ">=",
		};
		const result = resolveEffectiveVerdict(
			obl,
			REQ_ID,
			1,
			storage,
			snapshotUid,
			REPO_UID,
		);
		expect(result.obligation_id).toBe("obl-passthrough");
		expect(result.obligation).toBe("text");
		expect(result.method).toBe("manual_review");
		expect(result.target).toBe("src/core");
		expect(result.threshold).toBe(0.5);
		expect(result.operator).toBe(">=");
	});
});
