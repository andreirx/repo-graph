/**
 * Framework entrypoint detection (node-level).
 *
 * Detects symbols that are invoked by an external framework runtime
 * (not by internal code). These are NODE-LEVEL facts, not edge
 * reclassifications — the important truth is "this function is
 * externally entered," not "calls inside it are framework-boundary."
 *
 * Emitted as inferences (kind: "framework_entrypoint") with
 * confidence and basis. Consumed by:
 *   - dead-code analysis (symbol is live despite no internal callers)
 *   - trust reporting (detected entrypoints vs declared entrypoints)
 *
 * First-slice detectors:
 *   - Lambda/serverless exported handler convention
 *
 * Separation from edge-level framework-boundary detection
 * (framework-boundary.ts) is intentional: Express route registration
 * is a CALL that IS the boundary act; Lambda handler-ness is a
 * FUNCTION PROPERTY set by convention, not by a registration call.
 */

import type { ImportBinding } from "../ports/extractor.js";

// ── Lambda handler detection ────────────────────────────────────────

/**
 * Conventional Lambda handler function names.
 * AWS Lambda invokes the exported function matching the configured
 * handler name. "handler" is the default; "main" is a common
 * alternative.
 */
const LAMBDA_HANDLER_NAMES = new Set(["handler"]);

/**
 * Import specifiers that indicate AWS Lambda usage.
 * The detector requires BOTH an exported handler name AND a
 * Lambda-related import, to avoid false positives on non-Lambda
 * projects that happen to export a function named "handler."
 */
const LAMBDA_SPECIFIERS = new Set([
	"aws-lambda",
	"@types/aws-lambda",
	"@aws-lambda-powertools/commons",
	"@aws-lambda-powertools/logger",
	"@aws-lambda-powertools/tracer",
	"@aws-lambda-powertools/metrics",
	"@aws-lambda-powertools/parameters",
	"@middy/core",
]);

/**
 * Symbol subtypes that represent callable runtime values eligible
 * for Lambda handler detection. Excludes classes, interfaces,
 * type aliases, enums, and other non-callable export shapes.
 */
const CALLABLE_SUBTYPES = new Set([
	"FUNCTION",
	"VARIABLE",
	"CONSTANT",
]);

export interface DetectedEntrypoint {
	/** Stable key of the symbol identified as a framework entrypoint. */
	targetStableKey: string;
	/** Machine-stable convention that matched. */
	convention: string;
	/** Confidence in the detection (0-1). */
	confidence: number;
	/** Human-readable explanation. */
	reason: string;
}

/**
 * Scan a file's exported symbols and import bindings for Lambda
 * handler conventions.
 *
 * Returns detected entrypoints (may be empty). Pure function.
 *
 * Detection requires TWO signals:
 *   (a) file imports from a Lambda-related package
 *   (b) file exports a function with a conventional handler name
 *
 * Signal (a) prevents false positives on non-Lambda projects.
 * Signal (b) prevents flagging every export in a Lambda-adjacent file.
 */
export function detectLambdaEntrypoints(input: {
	importBindings: readonly ImportBinding[];
	exportedSymbols: ReadonlyArray<{
		stableKey: string;
		name: string;
		visibility: string | null;
		subtype: string | null;
	}>;
}): DetectedEntrypoint[] {
	// Check signal (a): file has a Lambda-related import.
	let hasLambdaImport = false;
	for (const binding of input.importBindings) {
		if (LAMBDA_SPECIFIERS.has(binding.specifier)) {
			hasLambdaImport = true;
			break;
		}
		// Also match scoped packages starting with known prefixes.
		if (binding.specifier.startsWith("@aws-lambda-powertools/")) {
			hasLambdaImport = true;
			break;
		}
	}
	if (!hasLambdaImport) return [];

	// Check signal (b): file exports a handler-named function.
	const results: DetectedEntrypoint[] = [];
	for (const sym of input.exportedSymbols) {
		if (
			sym.visibility === "export" &&
			LAMBDA_HANDLER_NAMES.has(sym.name) &&
			sym.subtype !== null &&
			CALLABLE_SUBTYPES.has(sym.subtype)
		) {
			results.push({
				targetStableKey: sym.stableKey,
				convention: "lambda_exported_handler",
				confidence: 0.9,
				reason: `exported function "${sym.name}" in file importing Lambda types`,
			});
		}
	}
	return results;
}
