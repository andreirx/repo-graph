import { randomUUID } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import {
	detectCoverageImporter,
	supportedFormats,
} from "../../../src/adapters/importers/coverage-registry.js";

const FIXTURE_DIR = join(tmpdir(), `rgr-cov-reg-${randomUUID()}`);
mkdirSync(FIXTURE_DIR, { recursive: true });

// Istanbul fixture
const ISTANBUL_PATH = join(FIXTURE_DIR, "coverage-final.json");
writeFileSync(
	ISTANBUL_PATH,
	JSON.stringify({
		"/repo/src/a.ts": { s: { "0": 1 }, f: { "0": 1 }, b: {} },
	}),
);

// Non-coverage JSON
const OTHER_JSON_PATH = join(FIXTURE_DIR, "package.json");
writeFileSync(
	OTHER_JSON_PATH,
	JSON.stringify({ name: "test", version: "1.0.0" }),
);

// Non-JSON file
const TEXT_PATH = join(FIXTURE_DIR, "readme.txt");
writeFileSync(TEXT_PATH, "This is not a coverage report.");

afterAll(() => {
	try {
		const { rmSync } = require("node:fs");
		rmSync(FIXTURE_DIR, { recursive: true });
	} catch {
		// best effort
	}
});

describe("coverage registry", () => {
	it("detects Istanbul format from coverage-final.json", async () => {
		const importer = await detectCoverageImporter(ISTANBUL_PATH);
		expect(importer).not.toBeNull();
		expect(importer?.formatName).toBe("istanbul");
	});

	it("returns null for non-coverage JSON", async () => {
		const importer = await detectCoverageImporter(OTHER_JSON_PATH);
		expect(importer).toBeNull();
	});

	it("returns null for non-JSON files", async () => {
		const importer = await detectCoverageImporter(TEXT_PATH);
		expect(importer).toBeNull();
	});

	it("returns null for nonexistent files", async () => {
		const importer = await detectCoverageImporter("/nonexistent/file.json");
		expect(importer).toBeNull();
	});

	it("lists supported formats", () => {
		const formats = supportedFormats();
		expect(formats).toContain("istanbul");
	});
});
