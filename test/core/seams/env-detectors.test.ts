/**
 * Environment variable detector unit tests.
 *
 * Pure functions — no filesystem, no storage.
 * Covers: JS/TS, Python, Rust, Java, C/C++.
 */

import { describe, expect, it } from "vitest";
import { detectEnvAccesses } from "../../../src/core/seams/env-detectors.js";

// ── JS/TS ──────────────────────────────────────────────────────────

describe("detectEnvAccesses — JS/TS", () => {
	it("detects process.env.VAR_NAME", () => {
		const deps = detectEnvAccesses(
			`const port = process.env.PORT;`,
			"src/server.ts",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("PORT");
		expect(deps[0].accessPattern).toBe("process_env_dot");
		expect(deps[0].accessKind).toBe("required");
	});

	it("detects process.env[\"VAR_NAME\"]", () => {
		const deps = detectEnvAccesses(
			`const url = process.env["DATABASE_URL"];`,
			"src/db.ts",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("DATABASE_URL");
		expect(deps[0].accessPattern).toBe("process_env_bracket");
	});

	it("detects destructured env vars", () => {
		const deps = detectEnvAccesses(
			`const { NODE_ENV, PORT, SECRET_KEY } = process.env;`,
			"src/config.ts",
		);
		expect(deps).toHaveLength(3);
		expect(deps.map((d) => d.varName).sort()).toEqual(["NODE_ENV", "PORT", "SECRET_KEY"]);
		expect(deps[0].accessPattern).toBe("process_env_destructure");
	});

	it("detects optional access with || fallback", () => {
		const deps = detectEnvAccesses(
			`const port = process.env.PORT || "3000";`,
			"src/server.ts",
		);
		expect(deps[0].accessKind).toBe("optional");
		expect(deps[0].defaultValue).toBe("3000");
	});

	it("detects optional access with ?? fallback", () => {
		const deps = detectEnvAccesses(
			`const env = process.env.NODE_ENV ?? "development";`,
			"src/config.ts",
		);
		expect(deps[0].accessKind).toBe("optional");
		expect(deps[0].defaultValue).toBe("development");
	});

	it("deduplicates same var in same file", () => {
		const deps = detectEnvAccesses(
			`if (process.env.DEBUG) console.log("debug");
const d = process.env.DEBUG;`,
			"src/util.ts",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("DEBUG");
	});

	it("detects multiple different vars", () => {
		const deps = detectEnvAccesses(
			`const a = process.env.API_KEY;
const b = process.env.API_SECRET;`,
			"src/auth.ts",
		);
		expect(deps).toHaveLength(2);
		expect(deps.map((d) => d.varName).sort()).toEqual(["API_KEY", "API_SECRET"]);
	});

	it("ignores dynamic access (no string literal)", () => {
		const deps = detectEnvAccesses(
			`const val = process.env[key];`,
			"src/dynamic.ts",
		);
		expect(deps).toHaveLength(0);
	});

	it("ignores lowercase var names", () => {
		const deps = detectEnvAccesses(
			`const val = process.env.lowercase;`,
			"src/misc.ts",
		);
		expect(deps).toHaveLength(0);
	});
});

// ── Python ─────────────────────────────────────────────────────────

describe("detectEnvAccesses — Python", () => {
	it("detects os.environ[\"VAR\"]", () => {
		const deps = detectEnvAccesses(
			`db_url = os.environ["DATABASE_URL"]`,
			"src/config.py",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("DATABASE_URL");
		expect(deps[0].accessKind).toBe("required");
		expect(deps[0].accessPattern).toBe("os_environ");
	});

	it("detects os.environ.get with default", () => {
		const deps = detectEnvAccesses(
			`port = os.environ.get("PORT", "8080")`,
			"src/server.py",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].accessKind).toBe("optional");
		expect(deps[0].defaultValue).toBe("8080");
	});

	it("detects os.getenv", () => {
		const deps = detectEnvAccesses(
			`secret = os.getenv("SECRET_KEY")`,
			"src/auth.py",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("SECRET_KEY");
		expect(deps[0].accessPattern).toBe("os_getenv");
	});

	it("detects os.getenv with default", () => {
		const deps = detectEnvAccesses(
			`env = os.getenv("ENV", "production")`,
			"src/config.py",
		);
		expect(deps[0].accessKind).toBe("optional");
		expect(deps[0].defaultValue).toBe("production");
	});
});

// ── Rust ───────────────────────────────────────────────────────────

describe("detectEnvAccesses — Rust", () => {
	it("detects std::env::var", () => {
		const deps = detectEnvAccesses(
			`let port = std::env::var("PORT").unwrap();`,
			"src/main.rs",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("PORT");
		expect(deps[0].accessPattern).toBe("std_env_var");
	});

	it("detects env::var without std prefix", () => {
		const deps = detectEnvAccesses(
			`let key = env::var("API_KEY")?;`,
			"src/config.rs",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("API_KEY");
	});
});

// ── Java ───────────────────────────────────────────────────────────

describe("detectEnvAccesses — Java", () => {
	it("detects System.getenv", () => {
		const deps = detectEnvAccesses(
			`String dbUrl = System.getenv("DATABASE_URL");`,
			"src/Config.java",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("DATABASE_URL");
		expect(deps[0].accessPattern).toBe("system_getenv");
	});
});

// ── C/C++ ──────────────────────────────────────────────────────────

describe("detectEnvAccesses — C/C++", () => {
	it("detects getenv", () => {
		const deps = detectEnvAccesses(
			`char* home = getenv("HOME");`,
			"src/main.c",
		);
		expect(deps).toHaveLength(1);
		expect(deps[0].varName).toBe("HOME");
		expect(deps[0].accessPattern).toBe("c_getenv");
	});
});

// ── Edge cases ─────────────────────────────────────────────────────

describe("detectEnvAccesses — edge cases", () => {
	it("returns empty for unsupported file type", () => {
		const deps = detectEnvAccesses(
			`process.env.SECRET`,
			"src/data.json",
		);
		expect(deps).toHaveLength(0);
	});

	it("returns empty for empty file", () => {
		const deps = detectEnvAccesses("", "src/empty.ts");
		expect(deps).toHaveLength(0);
	});

	it("reports correct line numbers", () => {
		const deps = detectEnvAccesses(
			`// line 1
// line 2
const x = process.env.MY_VAR;`,
			"src/lines.ts",
		);
		expect(deps[0].lineNumber).toBe(3);
	});
});
