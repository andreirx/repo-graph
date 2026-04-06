/**
 * Blast-radius derivation — pure function tests.
 */

import { describe, expect, it } from "vitest";
import {
	deriveBlastRadius,
	type BlastRadiusAssessment,
} from "../../../src/core/classification/blast-radius.js";
import { UnresolvedEdgeBasisCode } from "../../../src/core/diagnostics/unresolved-edge-classification.js";
import { UnresolvedEdgeCategory } from "../../../src/core/diagnostics/unresolved-edge-categories.js";

function derive(
	basisCode: string,
	category: string = UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
	visibility: string | null = null,
): BlastRadiusAssessment {
	return deriveBlastRadius({
		category,
		basisCode,
		sourceNodeVisibility: visibility,
	});
}

describe("deriveBlastRadius — receiver origin mapping", () => {
	it("this_receiver_implies_internal → this_bound", () => {
		const r = derive(UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL);
		expect(r.receiverOrigin).toBe("this_bound");
	});

	it("callee_matches_external_import → import_bound_external", () => {
		const r = derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT);
		expect(r.receiverOrigin).toBe("import_bound_external");
	});

	it("receiver_matches_internal_import → import_bound_internal", () => {
		const r = derive(UnresolvedEdgeBasisCode.RECEIVER_MATCHES_INTERNAL_IMPORT);
		expect(r.receiverOrigin).toBe("import_bound_internal");
	});

	it("specifier_matches_project_alias → import_bound_internal", () => {
		const r = derive(UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PROJECT_ALIAS);
		expect(r.receiverOrigin).toBe("import_bound_internal");
	});

	it("callee_matches_same_file_symbol → same_file_declared", () => {
		const r = derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL);
		expect(r.receiverOrigin).toBe("same_file_declared");
	});

	it("callee_matches_runtime_global → runtime_global", () => {
		const r = derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_RUNTIME_GLOBAL);
		expect(r.receiverOrigin).toBe("runtime_global");
	});

	it("specifier_matches_runtime_module → runtime_module", () => {
		const r = derive(UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE);
		expect(r.receiverOrigin).toBe("runtime_module");
	});

	it("no_supporting_signal → local_like", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL);
		expect(r.receiverOrigin).toBe("local_like");
	});
});

describe("deriveBlastRadius — enclosing scope significance", () => {
	it("EXPORT visibility → exported_public", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, "export");
		expect(r.enclosingScopeSignificance).toBe("exported_public");
	});

	it("PRIVATE visibility → internal_private", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, "private");
		expect(r.enclosingScopeSignificance).toBe("internal_private");
	});

	it("null visibility → internal_private", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, null);
		expect(r.enclosingScopeSignificance).toBe("internal_private");
	});
});

describe("deriveBlastRadius — radius computation", () => {
	it("local_like + internal_private → LOW", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, null);
		expect(r.blastRadius).toBe("low");
	});

	it("local_like + exported_public → MEDIUM", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, "export");
		expect(r.blastRadius).toBe("medium");
	});

	it("this_bound + internal_private → LOW", () => {
		const r = derive(UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL,
			UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT, null);
		expect(r.blastRadius).toBe("low");
	});

	it("this_bound + exported_public → MEDIUM", () => {
		const r = derive(UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL,
			UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT, "export");
		expect(r.blastRadius).toBe("medium");
	});

	it("import_bound_internal → MEDIUM regardless of scope", () => {
		expect(derive(UnresolvedEdgeBasisCode.RECEIVER_MATCHES_INTERNAL_IMPORT,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, null).blastRadius).toBe("medium");
		expect(derive(UnresolvedEdgeBasisCode.RECEIVER_MATCHES_INTERNAL_IMPORT,
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO, "export").blastRadius).toBe("medium");
	});

	it("import_bound_external → LOW", () => {
		expect(derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT).blastRadius).toBe("low");
	});

	it("same_file_declared → LOW", () => {
		expect(derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL).blastRadius).toBe("low");
	});

	it("runtime_global → LOW", () => {
		expect(derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_RUNTIME_GLOBAL).blastRadius).toBe("low");
	});
});

describe("deriveBlastRadius — non-CALLS categories", () => {
	it("IMPORTS → not_applicable", () => {
		const r = derive(UnresolvedEdgeBasisCode.RELATIVE_IMPORT_TARGET_UNRESOLVED,
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND);
		expect(r.blastRadius).toBe("not_applicable");
		expect(r.receiverOrigin).toBe("not_applicable");
	});

	it("INSTANTIATES → not_applicable", () => {
		const r = derive(UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
			UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND);
		expect(r.blastRadius).toBe("not_applicable");
	});

	it("OTHER → not_applicable", () => {
		const r = derive(UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			UnresolvedEdgeCategory.OTHER);
		expect(r.blastRadius).toBe("not_applicable");
	});
});
