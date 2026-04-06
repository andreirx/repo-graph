/**
 * Cargo.toml dependency reader tests.
 */

import { randomUUID } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
	parseCargoDependencies,
	readCargoDependencies,
} from "../../../src/adapters/config/cargo-reader.js";

let workDir: string;

beforeEach(() => {
	workDir = mkdtempSync(join(tmpdir(), `rgr-cargo-${randomUUID()}-`));
});

afterEach(() => {
	try {
		rmSync(workDir, { recursive: true, force: true });
	} catch {
		// ignore
	}
});

describe("parseCargoDependencies", () => {
	it("extracts dependencies from [dependencies] section", () => {
		const content = `
[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1", features = ["full"] }
log = "0.4"
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["log", "serde", "tokio"]);
	});

	it("extracts from [dev-dependencies] and [build-dependencies]", () => {
		const content = `
[dependencies]
serde = "1.0"

[dev-dependencies]
criterion = "0.5"

[build-dependencies]
cc = "1.0"
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["cc", "criterion", "serde"]);
	});

	it("handles dotted key form (foo.version)", () => {
		const content = `
[dependencies]
serde.version = "1.0"
serde.features = ["derive"]
tokio = "1"
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toContain("serde");
		expect(result.names).toContain("tokio");
	});

	it("handles workspace inheritance (foo.workspace = true)", () => {
		const content = `
[dependencies]
serde.workspace = true
tokio.workspace = true
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["serde", "tokio"]);
	});

	it("ignores non-dependency sections", () => {
		const content = `
[package]
name = "my-crate"
version = "0.1.0"

[features]
default = ["std"]
std = []
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual([]);
	});

	it("ignores comments and empty lines", () => {
		const content = `
[dependencies]
# This is a comment
serde = "1.0"

# Another comment
tokio = "1"
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["serde", "tokio"]);
	});

	it("deduplicates across sections", () => {
		const content = `
[dependencies]
serde = "1.0"

[dev-dependencies]
serde = { version = "1.0", features = ["derive"] }
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["serde"]);
	});

	it("returns sorted names", () => {
		const content = `
[dependencies]
z-crate = "1"
a-crate = "1"
m-crate = "1"
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["a-crate", "m-crate", "z-crate"]);
	});

	it("returns empty for content with no dep sections", () => {
		const result = parseCargoDependencies("[package]\nname = 'x'");
		expect(result.names).toEqual([]);
	});

	it("handles hyphenated and underscored crate names", () => {
		const content = `
[dependencies]
my-crate = "1"
my_other_crate = "1"
`;
		const result = parseCargoDependencies(content);
		expect(result.names).toEqual(["my-crate", "my_other_crate"]);
	});
});

describe("readCargoDependencies — file I/O", () => {
	it("reads Cargo.toml from directory", async () => {
		writeFileSync(
			join(workDir, "Cargo.toml"),
			'[dependencies]\nserde = "1.0"\n',
			"utf-8",
		);
		const result = await readCargoDependencies(workDir);
		expect(result).not.toBeNull();
		expect(result?.names).toEqual(["serde"]);
	});

	it("returns null when Cargo.toml does not exist", async () => {
		expect(await readCargoDependencies(workDir)).toBeNull();
	});
});
