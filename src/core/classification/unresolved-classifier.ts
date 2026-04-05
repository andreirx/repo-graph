/**
 * Unresolved-edge classifier (pure core logic).
 *
 * Given an unresolved edge + its extractor-determined category +
 * snapshot-level and file-level signals, returns a classification
 * verdict: { classification, basisCode }.
 *
 * Pure function. No I/O, no storage, no state. All inputs are
 * plain data. The function is deterministic with respect to its
 * inputs.
 *
 * Rule precedence (first-match-wins):
 *
 *   1. `this`-receiver shortcut  — immediate INTERNAL verdict.
 *   2. Same-file symbol match    — local lexical evidence is
 *                                  the strongest source of truth.
 *   3. External import binding   — receiver/callee came in from a
 *                                  package.json dependency.
 *   4. Internal relative import  — receiver/callee came in from
 *                                  a relative path in this file.
 *   5. tsconfig alias import     — receiver/callee came in from a
 *                                  specifier matching a path alias.
 *   6. Otherwise                 — UNKNOWN.
 *
 * The ordering reflects evidence strength: a lexical same-file
 * binding is more certain than an imported binding. Alias-matching
 * is ranked last among internal categories because first-slice
 * alias data may be incomplete.
 *
 * For `imports_file_not_found`:
 *   Current extractor emits unresolved IMPORTS only for relative
 *   specifiers. Every such observation is therefore INTERNAL with
 *   the RELATIVE_IMPORT_TARGET_UNRESOLVED basis.
 *
 * For `other`:
 *   Always UNKNOWN. No rule fires.
 */

import { UnresolvedEdgeCategory } from "../diagnostics/unresolved-edge-categories.js";
import {
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../diagnostics/unresolved-edge-classification.js";
import type { ImportBinding, UnresolvedEdge } from "../ports/extractor.js";
import {
	hasPackageDependency,
	matchesAnyAlias,
	type FileSignals,
	type SnapshotSignals,
} from "./signals.js";

export interface ClassifierVerdict {
	classification: UnresolvedEdgeClassification;
	basisCode: UnresolvedEdgeBasisCode;
}

/**
 * Classify a single unresolved edge.
 *
 * @param edge - The extractor-emitted unresolved observation.
 * @param category - Category produced by the existing categorizer.
 * @param snapshotSignals - Package deps + tsconfig aliases.
 * @param fileSignals - Import bindings + same-file symbols for
 *                      the source file of this edge.
 */
export function classifyUnresolvedEdge(
	edge: UnresolvedEdge,
	category: UnresolvedEdgeCategory,
	snapshotSignals: SnapshotSignals,
	fileSignals: FileSignals,
): ClassifierVerdict {
	// 1. `this`-receiver shortcut.
	if (
		category === UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT ||
		category ===
			UnresolvedEdgeCategory.CALLS_THIS_WILDCARD_METHOD_NEEDS_TYPE_INFO
	) {
		return internal(UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL);
	}

	// IMPORTS_FILE_NOT_FOUND: current extractor emits unresolved
	// IMPORTS edges only for relative specifiers.
	if (category === UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND) {
		return internal(UnresolvedEdgeBasisCode.RELATIVE_IMPORT_TARGET_UNRESOLVED);
	}

	// OTHER: no semantic rules apply.
	if (category === UnresolvedEdgeCategory.OTHER) {
		return unknown();
	}

	// Every remaining category classifies by an IDENTIFIER extracted
	// from the edge's targetKey (possibly after rewriting — use
	// metadataJson.rawCalleeName for CALLS when available).
	const identifier = extractTargetIdentifier(edge, category);
	if (identifier === null) {
		return unknown();
	}

	// Rule 2: same-file symbol match (strongest lexical evidence).
	// Subtype-aware: CALLS/receiver checks value-bindable names,
	// INSTANTIATES checks CLASS names, IMPLEMENTS checks INTERFACE
	// names. A type-only `Foo` does NOT cause a runtime `Foo()` call
	// to classify as same-file, and a function `Bar` does NOT cause
	// `new Bar()` to classify as same-file.
	if (matchesSameFileByRole(identifier, category, fileSignals)) {
		return internal(sameFileBasisFor(category));
	}

	// Find the import binding (if any) that introduced this identifier.
	const binding = findBindingForIdentifier(
		identifier,
		fileSignals.importBindings,
	);

	if (binding) {
		const isBareExternal =
			!binding.isRelative &&
			hasPackageDependency(snapshotSignals.packageDependencies, binding.specifier);
		// Rule 3: external import binding match.
		if (isBareExternal) {
			return external(externalBasisFor(category));
		}
		// Rule 4: internal relative import binding match.
		if (binding.isRelative) {
			return internal(internalImportBasisFor(category));
		}
		// Rule 5: tsconfig alias match (non-relative, non-package).
		if (matchesAnyAlias(binding.specifier, snapshotSignals.tsconfigAliases)) {
			return internal(internalImportBasisFor(category));
		}
		// Non-relative specifier that is neither an external package
		// nor a known alias. Fall through to unknown.
	}

	// Rule 6: unknown.
	return unknown();
}

// ── Helpers ─────────────────────────────────────────────────────────

/**
 * Subtype-aware same-file match.
 *
 * Each category consults the symbol set appropriate to its runtime
 * role:
 *   - CALLS_FUNCTION / CALLS_OBJ_METHOD → sameFileValueSymbols.
 *     Runtime identifiers only. Type-only names are excluded.
 *   - INSTANTIATES → sameFileClassSymbols. Only CLASS.
 *   - IMPLEMENTS → sameFileInterfaceSymbols. Only INTERFACE.
 *   - everything else → no same-file match path (handled upstream).
 */
function matchesSameFileByRole(
	identifier: string,
	category: UnresolvedEdgeCategory,
	fileSignals: FileSignals,
): boolean {
	switch (category) {
		case UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING:
		case UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO:
			return fileSignals.sameFileValueSymbols.has(identifier);
		case UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND:
			return fileSignals.sameFileClassSymbols.has(identifier);
		case UnresolvedEdgeCategory.IMPLEMENTS_INTERFACE_NOT_FOUND:
			return fileSignals.sameFileInterfaceSymbols.has(identifier);
		default:
			return false;
	}
}


function external(basis: UnresolvedEdgeBasisCode): ClassifierVerdict {
	return {
		classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		basisCode: basis,
	};
}

function internal(basis: UnresolvedEdgeBasisCode): ClassifierVerdict {
	return {
		classification: UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
		basisCode: basis,
	};
}

function unknown(): ClassifierVerdict {
	return {
		classification: UnresolvedEdgeClassification.UNKNOWN,
		basisCode: UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
	};
}

/**
 * Category-aware basis code for a "same-file symbol" match.
 * Receiver-centric categories use RECEIVER_MATCHES_SAME_FILE_SYMBOL;
 * all others use CALLEE_MATCHES_SAME_FILE_SYMBOL. (Instantiates /
 * implements use the callee-shaped basis because they operate on
 * type names, which semantically resemble callee identifiers.)
 */
function sameFileBasisFor(
	category: UnresolvedEdgeCategory,
): UnresolvedEdgeBasisCode {
	if (category === UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO) {
		return UnresolvedEdgeBasisCode.RECEIVER_MATCHES_SAME_FILE_SYMBOL;
	}
	return UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL;
}

function externalBasisFor(
	category: UnresolvedEdgeCategory,
): UnresolvedEdgeBasisCode {
	if (category === UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO) {
		return UnresolvedEdgeBasisCode.RECEIVER_MATCHES_EXTERNAL_IMPORT;
	}
	return UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT;
}

function internalImportBasisFor(
	category: UnresolvedEdgeCategory,
): UnresolvedEdgeBasisCode {
	if (category === UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO) {
		return UnresolvedEdgeBasisCode.RECEIVER_MATCHES_INTERNAL_IMPORT;
	}
	return UnresolvedEdgeBasisCode.CALLEE_MATCHES_INTERNAL_IMPORT;
}

/**
 * Find the ImportBinding that introduced the given local identifier
 * in this file, or null if none does.
 *
 * The first match is returned. Duplicate bindings for the same
 * identifier are syntactically impossible (would be a TS error).
 */
function findBindingForIdentifier(
	identifier: string,
	bindings: readonly ImportBinding[],
): ImportBinding | null {
	for (const binding of bindings) {
		if (binding.identifier === identifier) return binding;
	}
	return null;
}

/**
 * Conservative category-aware identifier extraction.
 *
 * Returns the identifier to look up against fileSignals (callee,
 * receiver, or type name). Returns null if the targetKey shape is
 * ambiguous — the caller falls through to "unknown" classification
 * rather than making a guessed match.
 *
 * For CALLS edges, reads metadataJson.rawCalleeName if present
 * (the extractor stores original pre-rewrite text here) so
 * receiver-type-rewritten keys classify against original receivers.
 *
 * Not a parser. Matches only simple identifier shapes. Anything
 * with brackets, calls, template literals, etc. returns null.
 */
export function extractTargetIdentifier(
	edge: UnresolvedEdge,
	category: UnresolvedEdgeCategory,
): string | null {
	// Read pre-rewrite targetKey from metadata when present.
	let key = edge.targetKey;
	if (edge.metadataJson) {
		try {
			const meta = JSON.parse(edge.metadataJson) as Record<string, unknown>;
			if (typeof meta.rawCalleeName === "string") {
				key = meta.rawCalleeName;
			}
		} catch {
			// malformed metadata — use targetKey as-is
		}
	}

	switch (category) {
		case UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND:
		case UnresolvedEdgeCategory.IMPLEMENTS_INTERFACE_NOT_FOUND:
		case UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING: {
			// Expect a single identifier. Reject anything else.
			if (isSimpleIdentifier(key)) return key;
			return null;
		}
		case UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO: {
			// Expect "receiver.x.y.method". First dotted segment is
			// the receiver. Reject if the format doesn't hold.
			const dotIdx = key.indexOf(".");
			if (dotIdx <= 0) return null;
			const receiver = key.slice(0, dotIdx);
			if (isSimpleIdentifier(receiver)) return receiver;
			return null;
		}
		default:
			// this-* and OTHER are handled by category shortcuts above.
			// IMPORTS is handled by category shortcut.
			return null;
	}
}

function isSimpleIdentifier(s: string): boolean {
	return /^[A-Za-z_$][\w$]*$/.test(s);
}
