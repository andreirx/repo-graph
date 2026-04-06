/**
 * Unresolved-edge classifier — pure rule tests.
 *
 * Exercises every precedence step independently and verifies the
 * ordering (later rules only fire when earlier rules do not match).
 */

import { describe, expect, it } from "vitest";
import { UnresolvedEdgeCategory } from "../../../src/core/diagnostics/unresolved-edge-categories.js";
import {
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../../../src/core/diagnostics/unresolved-edge-classification.js";
import type {
	FileSignals,
	SnapshotSignals,
} from "../../../src/core/classification/signals.js";
import {
	classifyUnresolvedEdge,
	extractTargetIdentifier,
} from "../../../src/core/classification/unresolved-classifier.js";
import { EdgeType, Resolution } from "../../../src/core/model/index.js";
import type { ImportBinding, UnresolvedEdge } from "../../../src/core/ports/extractor.js";

// ── Test harness ────────────────────────────────────────────────────

function snapshot(
	runtimeGlobals: string[] = [],
	runtimeModules: string[] = [],
): SnapshotSignals {
	return {
		runtimeBuiltins: {
			identifiers: Object.freeze([...runtimeGlobals]),
			moduleSpecifiers: Object.freeze([...runtimeModules]),
		},
	};
}

function file(
	imports: Array<Partial<ImportBinding> & { identifier: string; specifier: string }> = [],
	valueSymbols: string[] = [],
	classSymbols: string[] = [],
	interfaceSymbols: string[] = [],
	packageNames: string[] = [],
	aliasPatterns: string[] = [],
): FileSignals {
	return {
		importBindings: imports.map((b) => ({
			identifier: b.identifier,
			specifier: b.specifier,
			isRelative: b.isRelative ?? b.specifier.startsWith("."),
			location: b.location ?? null,
			isTypeOnly: b.isTypeOnly ?? false,
		})),
		sameFileValueSymbols: new Set(valueSymbols),
		sameFileClassSymbols: new Set(classSymbols),
		sameFileInterfaceSymbols: new Set(interfaceSymbols),
		packageDependencies: {
			names: Object.freeze([...packageNames].sort()),
		},
		tsconfigAliases: {
			entries: aliasPatterns.map((p) => ({ pattern: p, substitutions: [] })),
		},
	};
}

function edge(
	targetKey: string,
	type: EdgeType = EdgeType.CALLS,
	metadataJson: string | null = null,
): UnresolvedEdge {
	return {
		edgeUid: "e1",
		snapshotUid: "s1",
		repoUid: "r1",
		sourceNodeUid: "n1",
		targetKey,
		type,
		resolution: Resolution.STATIC,
		extractor: "ts-core:0.1.0",
		location: null,
		metadataJson,
	};
}

// ── Category shortcuts ──────────────────────────────────────────────

describe("classifyUnresolvedEdge — category shortcuts", () => {
	it("this.method() → internal via THIS_RECEIVER_IMPLIES_INTERNAL", () => {
		const verdict = classifyUnresolvedEdge(
			edge("this.save"),
			UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL,
		);
	});

	it("this.x.method() → internal via THIS_RECEIVER_IMPLIES_INTERNAL", () => {
		const verdict = classifyUnresolvedEdge(
			edge("this.x.m"),
			UnresolvedEdgeCategory.CALLS_THIS_WILDCARD_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file(),
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL,
		);
	});

	it("TS relative import (stable-key form) → internal", () => {
		const verdict = classifyUnresolvedEdge(
			edge("repo:src/missing:FILE", EdgeType.IMPORTS),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RELATIVE_IMPORT_TARGET_UNRESOLVED,
		);
	});

	it("TS relative import (rawPath in metadata) → internal", () => {
		const verdict = classifyUnresolvedEdge(
			edge("repo:src/missing:FILE", EdgeType.IMPORTS, JSON.stringify({ rawPath: "./missing" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
	});

	it("Rust external crate import (serde in Cargo deps) → external", () => {
		const verdict = classifyUnresolvedEdge(
			edge("serde", EdgeType.IMPORTS, JSON.stringify({ specifier: "serde", identifier: "Serialize" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file([], [], [], [], ["serde"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PACKAGE_DEPENDENCY,
		);
	});

	it("Rust hyphenated Cargo dep (my-crate → my_crate in use path) → external", () => {
		// Cargo.toml has "my-crate", but Rust use path normalizes to my_crate.
		const verdict = classifyUnresolvedEdge(
			edge("my_crate", EdgeType.IMPORTS, JSON.stringify({ specifier: "my_crate", identifier: "Thing" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file([], [], [], [], ["my-crate"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PACKAGE_DEPENDENCY,
		);
	});

	it("Rust hyphenated dep with path (serde_json::Value → serde-json in Cargo) → external", () => {
		const verdict = classifyUnresolvedEdge(
			edge("serde_json", EdgeType.IMPORTS, JSON.stringify({ specifier: "serde_json::Value" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file([], [], [], [], ["serde-json"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
	});

	it("Rust crate::module import → internal (relative)", () => {
		const verdict = classifyUnresolvedEdge(
			edge("crate_module", EdgeType.IMPORTS, JSON.stringify({ specifier: "crate::utils" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RELATIVE_IMPORT_TARGET_UNRESOLVED,
		);
	});

	it("Rust super::module import → internal (relative)", () => {
		const verdict = classifyUnresolvedEdge(
			edge("super_module", EdgeType.IMPORTS, JSON.stringify({ specifier: "super::types" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
	});

	it("Node stdlib import (path) → external via runtime module", () => {
		const verdict = classifyUnresolvedEdge(
			edge("path", EdgeType.IMPORTS, JSON.stringify({ rawPath: "path" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot([], ["path", "node:path"]),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE,
		);
	});

	it("Rust std::collections import → external via runtime module", () => {
		const verdict = classifyUnresolvedEdge(
			edge("std::collections", EdgeType.IMPORTS, JSON.stringify({ specifier: "std::collections" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot([], ["std", "std::collections"]),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE,
		);
	});

	it("Rust std (bare root) import → external via runtime module", () => {
		const verdict = classifyUnresolvedEdge(
			edge("std", EdgeType.IMPORTS, JSON.stringify({ specifier: "std" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot([], ["std"]),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
	});

	it("Rust std::io::Read (nested path, first segment matches) → external", () => {
		// The classifier checks baseSpecifier (first :: segment) against runtime modules.
		const verdict = classifyUnresolvedEdge(
			edge("std::io", EdgeType.IMPORTS, JSON.stringify({ specifier: "std::io::Read" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot([], ["std", "std::io"]),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
	});

	it("Rust crate-internal module (renderer::camera) → internal / heuristic basis", () => {
		const verdict = classifyUnresolvedEdge(
			edge("renderer::camera", EdgeType.IMPORTS, JSON.stringify({ specifier: "renderer::camera", identifier: "Camera" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RUST_CRATE_INTERNAL_MODULE_HEURISTIC,
		);
	});

	it("Rust crate-internal single-segment module (easing) → internal / heuristic basis", () => {
		const verdict = classifyUnresolvedEdge(
			edge("easing", EdgeType.IMPORTS, JSON.stringify({ specifier: "easing", identifier: "EaseIn" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RUST_CRATE_INTERNAL_MODULE_HEURISTIC,
		);
	});

	it("Rust crate-internal with underscores (input_queue) → internal / heuristic basis", () => {
		const verdict = classifyUnresolvedEdge(
			edge("input_queue", EdgeType.IMPORTS, JSON.stringify({ specifier: "input_queue", identifier: "InputQueue" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RUST_CRATE_INTERNAL_MODULE_HEURISTIC,
		);
	});

	it("Rust crate:: prefix uses DEFINITE basis (not heuristic)", () => {
		const verdict = classifyUnresolvedEdge(
			edge("crate_module", EdgeType.IMPORTS, JSON.stringify({ specifier: "crate::utils" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		// crate:: prefix is definite, not heuristic.
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RELATIVE_IMPORT_TARGET_UNRESOLVED,
		);
	});

	it("Rust heuristic does NOT apply to TS imports (rawPath present)", () => {
		// TS import with rawPath metadata — should NOT be caught by Rust heuristic.
		const verdict = classifyUnresolvedEdge(
			edge("mystery", EdgeType.IMPORTS, JSON.stringify({ rawPath: "mystery" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.UNKNOWN,
		);
	});

	it("TS unknown bare import (not in deps, not runtime, not relative) → unknown", () => {
		// TS-origin import (rawPath present) that matches nothing.
		const verdict = classifyUnresolvedEdge(
			edge("mystery_package", EdgeType.IMPORTS, JSON.stringify({ rawPath: "mystery_package" })),
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.UNKNOWN,
		);
	});

	it("OTHER category → unknown", () => {
		const verdict = classifyUnresolvedEdge(
			edge("weird"),
			UnresolvedEdgeCategory.OTHER,
			snapshot(),
			file([{ identifier: "weird", specifier: "lodash" }], [], [], [], ["lodash"]),
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
		);
	});
});

// ── CALLS_FUNCTION precedence walk ──────────────────────────────────

describe("classifyUnresolvedEdge — CALLS_FUNCTION precedence", () => {
	it("rule 2: same-file symbol beats external import (strongest local evidence)", () => {
		// `foo` is both declared in this file AND imported from lodash.
		// Same-file wins.
		const verdict = classifyUnresolvedEdge(
			edge("foo"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([{ identifier: "foo", specifier: "lodash" }], ["foo"], [], [], ["lodash"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL,
		);
	});

	it("rule 3: external import", () => {
		const verdict = classifyUnresolvedEdge(
			edge("debounce"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([{ identifier: "debounce", specifier: "lodash" }], [], [], [], ["lodash"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
		);
	});

	it("rule 4: internal relative import", () => {
		const verdict = classifyUnresolvedEdge(
			edge("helper"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([{ identifier: "helper", specifier: "./helper" }]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_INTERNAL_IMPORT,
		);
	});

	it("rule 5d: alias import (non-relative, no package match)", () => {
		const verdict = classifyUnresolvedEdge(
			edge("helper"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([{ identifier: "helper", specifier: "@/lib/helper" }], [], [], [], [], ["@/*"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PROJECT_ALIAS,
		);
	});

	it("rule 6: identifier present in imports but specifier not recognized → unknown", () => {
		// specifier is neither in package deps nor matches alias nor relative.
		const verdict = classifyUnresolvedEdge(
			edge("foo"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(), // no packages
			file([{ identifier: "foo", specifier: "mystery" }]),
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
	});

	it("rule 6: no signal matches → unknown", () => {
		const verdict = classifyUnresolvedEdge(
			edge("foo"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
		);
	});
});

// ── CALLS_OBJ_METHOD ────────────────────────────────────────────────

describe("classifyUnresolvedEdge — CALLS_OBJ_METHOD receiver classification", () => {
	it("receiver is same-file symbol", () => {
		const verdict = classifyUnresolvedEdge(
			edge("client.send"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file([], ["client"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RECEIVER_MATCHES_SAME_FILE_SYMBOL,
		);
	});

	it("receiver imported from external package", () => {
		const verdict = classifyUnresolvedEdge(
			edge("axios.get"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file([{ identifier: "axios", specifier: "axios" }], [], [], [], ["axios"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RECEIVER_MATCHES_EXTERNAL_IMPORT,
		);
	});

	it("receiver imported via relative path", () => {
		const verdict = classifyUnresolvedEdge(
			edge("local.send"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file([{ identifier: "local", specifier: "./local" }]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RECEIVER_MATCHES_INTERNAL_IMPORT,
		);
	});

	it("receiver imported via project alias", () => {
		const verdict = classifyUnresolvedEdge(
			edge("helper.send"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file([{ identifier: "helper", specifier: "@/lib/helper" }], [], [], [], [], ["@/*"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PROJECT_ALIAS,
		);
	});

	it("long chain: only first segment is the receiver", () => {
		const verdict = classifyUnresolvedEdge(
			edge("client.api.users.find"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file([], ["client"]),
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RECEIVER_MATCHES_SAME_FILE_SYMBOL,
		);
	});

	it("unknown receiver → unknown", () => {
		const verdict = classifyUnresolvedEdge(
			edge("mystery.m"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(),
			file(),
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
	});
});

// ── INSTANTIATES / IMPLEMENTS ───────────────────────────────────────

describe("classifyUnresolvedEdge — INSTANTIATES + IMPLEMENTS", () => {
	it("INSTANTIATES: external class import", () => {
		const verdict = classifyUnresolvedEdge(
			edge("EventEmitter", EdgeType.INSTANTIATES),
			UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND,
			snapshot(),
			file([{ identifier: "EventEmitter", specifier: "events" }], [], [], [], ["events"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
		);
	});

	it("INSTANTIATES does NOT match when same-named FUNCTION exists (subtype mismatch)", () => {
		// File contains a value-symbol `Foo` (e.g. function Foo) but
		// no class Foo. `new Foo()` must NOT be same-file-matched.
		// Instead, the imported external Foo should drive external
		// classification.
		const verdict = classifyUnresolvedEdge(
			edge("Foo", EdgeType.INSTANTIATES),
			UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND,
			snapshot(),
			file([{ identifier: "Foo", specifier: "ext-pkg" }], ["Foo"], [], [], ["ext-pkg"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
	});

	it("IMPLEMENTS does NOT match when same-named value symbol exists (subtype mismatch)", () => {
		// File has a value `IFoo` (not an interface). `implements IFoo`
		// must NOT match — we match only actual INTERFACE subtypes.
		const verdict = classifyUnresolvedEdge(
			edge("IFoo", EdgeType.IMPLEMENTS),
			UnresolvedEdgeCategory.IMPLEMENTS_INTERFACE_NOT_FOUND,
			snapshot(),
			file([], ["IFoo"]),
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
	});

	it("CALLS_FUNCTION does NOT match against type-only name (subtype mismatch)", () => {
		// File has only a type `Foo` (interface). `Foo()` runtime call
		// should NOT match. With an external import for Foo, the
		// verdict should be external.
		const verdict = classifyUnresolvedEdge(
			edge("Foo"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file(
				[{ identifier: "Foo", specifier: "ext-pkg" }],
				[], // no value symbols
				[], // no class symbols
				["Foo"], // Foo is only an interface
				["ext-pkg"],
			),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
		);
	});

	it("CALLS_FUNCTION DOES match when same-file CLASS exists (class is value-bindable)", () => {
		// Foo is a class → it IS a runtime identifier, so `Foo()` can
		// legitimately reference it (callable class/constructor call).
		const verdict = classifyUnresolvedEdge(
			edge("Foo"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([], ["Foo"], ["Foo"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL,
		);
	});

	it("INSTANTIATES matches only against CLASS, not against VARIABLE or CONSTANT", () => {
		// `const Foo = ...` is a value but NOT a class. `new Foo()`
		// may work at runtime, but for the first-slice classifier we
		// stay conservative: only CLASS matches INSTANTIATES.
		const verdict = classifyUnresolvedEdge(
			edge("Foo", EdgeType.INSTANTIATES),
			UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND,
			snapshot(),
			file([], ["Foo"]), // value but not class
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
	});

	it("IMPLEMENTS: same-file interface", () => {
		const verdict = classifyUnresolvedEdge(
			edge("LocalContract", EdgeType.IMPLEMENTS),
			UnresolvedEdgeCategory.IMPLEMENTS_INTERFACE_NOT_FOUND,
			snapshot(),
			file([], [], [], ["LocalContract"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL,
		);
	});
});

// ── metadataJson.rawCalleeName handling ─────────────────────────────

describe("classifyUnresolvedEdge — rawCalleeName pre-rewrite handling", () => {
	it("uses rawCalleeName from metadata for CALLS identifier extraction", () => {
		// targetKey has been rewritten by the extractor, but metadata
		// carries the original "foo" which should match same-file.
		const e = edge(
			"Rewritten.Thing.foo",
			EdgeType.CALLS,
			JSON.stringify({ rawCalleeName: "foo" }),
		);
		const verdict = classifyUnresolvedEdge(
			e,
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([], ["foo"]),
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL,
		);
	});

	it("tolerates malformed metadata — falls back to targetKey", () => {
		const e = edge("foo", EdgeType.CALLS, "not valid json");
		const verdict = classifyUnresolvedEdge(
			e,
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(),
			file([], ["foo"]),
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL,
		);
	});
});

// ── Runtime builtins (last-resort rule) ──────────────────────────────

describe("classifyUnresolvedEdge — runtime builtins", () => {
	it("rule 6: `new Map()` with no binding → external / CALLEE_MATCHES_RUNTIME_GLOBAL", () => {
		const verdict = classifyUnresolvedEdge(
			edge("Map", EdgeType.INSTANTIATES),
			UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND,
			snapshot(["Map", "Set", "Date"]),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_RUNTIME_GLOBAL,
		);
	});

	it("rule 6: `process.exit()` with no binding → external / RECEIVER_MATCHES_RUNTIME_GLOBAL", () => {
		const verdict = classifyUnresolvedEdge(
			edge("process.exit"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot(["process"]),
			file(),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.RECEIVER_MATCHES_RUNTIME_GLOBAL,
		);
	});

	it("rule 5a: `import path from 'path'; path.join()` → external / SPECIFIER_MATCHES_RUNTIME_MODULE", () => {
		const verdict = classifyUnresolvedEdge(
			edge("path.join"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot([], ["path", "node:path"]),
			file([{ identifier: "path", specifier: "path" }]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE,
		);
	});

	it("rule 5a: node: prefixed specifier → external / SPECIFIER_MATCHES_RUNTIME_MODULE", () => {
		const verdict = classifyUnresolvedEdge(
			edge("readFile"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot([], ["fs", "node:fs"]),
			file([{ identifier: "readFile", specifier: "node:fs" }]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE,
		);
	});

	it("runtime module check fires BEFORE package-dep check", () => {
		// "path" is BOTH in runtime modules and (hypothetically) in
		// package.json. Runtime module wins because it's checked first.
		const verdict = classifyUnresolvedEdge(
			edge("path.join"),
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			snapshot([], ["path"]),
			file([{ identifier: "path", specifier: "path" }], [], [], [], ["path"]),
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE,
		);
	});

	it("same-file match beats runtime global", () => {
		// File declares a local `Map` (value symbol). Same-file is
		// stronger evidence than ambient runtime knowledge.
		const verdict = classifyUnresolvedEdge(
			edge("Map"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(["Map"]),
			file([], ["Map"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL,
		);
	});

	it("explicit import binding beats runtime global", () => {
		// File imports Map from a package. Binding is stronger than
		// ambient runtime.
		const verdict = classifyUnresolvedEdge(
			edge("Map"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(["Map"]),
			file([{ identifier: "Map", specifier: "custom-collections" }], [], [], [], ["custom-collections"]),
		);
		expect(verdict.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
		);
	});

	it("identifier NOT in runtime globals → unknown (not misclassified)", () => {
		const verdict = classifyUnresolvedEdge(
			edge("mysteryIdentifier"),
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			snapshot(["Map", "Set"]),
			file(),
		);
		expect(verdict.classification).toBe(UnresolvedEdgeClassification.UNKNOWN);
		expect(verdict.basisCode).toBe(
			UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
		);
	});
});

// ── extractTargetIdentifier (helper) ────────────────────────────────

describe("extractTargetIdentifier", () => {
	it("returns a simple identifier for CALLS_FUNCTION", () => {
		expect(
			extractTargetIdentifier(
				edge("foo"),
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			),
		).toBe("foo");
	});

	it("returns first segment for CALLS_OBJ_METHOD", () => {
		expect(
			extractTargetIdentifier(
				edge("client.send"),
				UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			),
		).toBe("client");
	});

	it("returns class name for INSTANTIATES", () => {
		expect(
			extractTargetIdentifier(
				edge("EventEmitter"),
				UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND,
			),
		).toBe("EventEmitter");
	});

	it("returns null when CALLS_FUNCTION targetKey is not a simple identifier", () => {
		expect(
			extractTargetIdentifier(
				edge("foo.bar"),
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			),
		).toBeNull();
		expect(
			extractTargetIdentifier(
				edge("foo()"),
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			),
		).toBeNull();
	});

	it("returns null for CALLS_OBJ_METHOD when no dot is present", () => {
		expect(
			extractTargetIdentifier(
				edge("noMethod"),
				UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			),
		).toBeNull();
	});

	it("returns null for CALLS_OBJ_METHOD when first segment is not simple identifier", () => {
		expect(
			extractTargetIdentifier(
				edge("1bad.m"),
				UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			),
		).toBeNull();
		expect(
			extractTargetIdentifier(
				edge("[expr].m"),
				UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			),
		).toBeNull();
	});

	it("returns null for THIS-* categories (handled by shortcut)", () => {
		expect(
			extractTargetIdentifier(
				edge("this.m"),
				UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT,
			),
		).toBeNull();
	});

	it("returns null for IMPORTS (handled by shortcut)", () => {
		expect(
			extractTargetIdentifier(
				edge("repo:x:FILE", EdgeType.IMPORTS),
				UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			),
		).toBeNull();
	});
});
