/**
 * Framework entrypoint detection — pure function tests.
 */

import { describe, expect, it } from "vitest";
import { detectLambdaEntrypoints } from "../../../src/core/classification/framework-entrypoints.js";
import type { ImportBinding } from "../../../src/core/ports/extractor.js";

function binding(identifier: string, specifier: string): ImportBinding {
	return { identifier, specifier, isRelative: false, location: null, isTypeOnly: false };
}

function sym(
	name: string,
	stableKey: string,
	visibility: string | null = "export",
	subtype: string | null = "FUNCTION",
) {
	return { stableKey, name, visibility, subtype };
}

const LAMBDA_IMPORT = [binding("Handler", "aws-lambda")];
const POWERTOOLS_IMPORT = [binding("Logger", "@aws-lambda-powertools/logger")];
const MIDDY_IMPORT = [binding("middy", "@middy/core")];

describe("detectLambdaEntrypoints", () => {
	it("detects exported handler with aws-lambda import", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION")],
		});
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("lambda_exported_handler");
		expect(results[0].confidence).toBe(0.9);
		expect(results[0].targetStableKey).toBe("repo:src/fn.ts#handler:SYMBOL:FUNCTION");
	});

	it("detects exported const handler (arrow function)", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:CONSTANT", "export", "CONSTANT")],
		});
		expect(results.length).toBe(1);
	});

	it("does NOT detect non-callable export named handler (class, P1 regression)", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:CLASS", "export", "CLASS")],
		});
		expect(results).toEqual([]);
	});

	it("does NOT detect type alias named handler (P1 regression)", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:TYPE_ALIAS", "export", "TYPE_ALIAS")],
		});
		expect(results).toEqual([]);
	});

	it("does NOT detect exported main (P2 — too broad, removed)", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("main", "repo:src/fn.ts#main:SYMBOL:FUNCTION")],
		});
		expect(results).toEqual([]);
	});

	it("detects handler with @aws-lambda-powertools import", () => {
		const results = detectLambdaEntrypoints({
			importBindings: POWERTOOLS_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION")],
		});
		expect(results.length).toBe(1);
	});

	it("detects handler with @middy/core import", () => {
		const results = detectLambdaEntrypoints({
			importBindings: MIDDY_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION")],
		});
		expect(results.length).toBe(1);
	});

	it("returns empty when no Lambda import exists (no false positives)", () => {
		const results = detectLambdaEntrypoints({
			importBindings: [binding("express", "express")],
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION")],
		});
		expect(results).toEqual([]);
	});

	it("returns empty when handler is not exported", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION", "private")],
		});
		expect(results).toEqual([]);
	});

	it("returns empty when handler is null visibility", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION", null)],
		});
		expect(results).toEqual([]);
	});

	it("returns empty when no conventional handler name exists", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [
				sym("processEvent", "repo:src/fn.ts#processEvent:SYMBOL:FUNCTION"),
				sym("doStuff", "repo:src/fn.ts#doStuff:SYMBOL:FUNCTION"),
			],
		});
		expect(results).toEqual([]);
	});

	it("detects handler but ignores non-handler exports in the same file", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [
				sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION"),
				sym("processEvent", "repo:src/fn.ts#processEvent:SYMBOL:FUNCTION"),
			],
		});
		expect(results.length).toBe(1);
		expect(results[0].targetStableKey).toContain("handler");
	});

	it("ignores non-handler exports even with Lambda import", () => {
		const results = detectLambdaEntrypoints({
			importBindings: LAMBDA_IMPORT,
			exportedSymbols: [
				sym("helper", "repo:src/fn.ts#helper:SYMBOL:FUNCTION"),
				sym("handler", "repo:src/fn.ts#handler:SYMBOL:FUNCTION"),
			],
		});
		// Only handler matches, not helper.
		expect(results.length).toBe(1);
		expect(results[0].targetStableKey).toContain("handler");
	});
});
