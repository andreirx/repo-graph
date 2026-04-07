/**
 * Python dependency reader — unit tests.
 *
 * Tests parsing of pyproject.toml and requirements.txt.
 */

import { describe, expect, it } from "vitest";
import {
	parsePyprojectDependencies,
	parseRequirementsTxt,
} from "../../../src/adapters/config/python-deps-reader.js";

// ── pyproject.toml ──────────────────────────────────────────────────

describe("parsePyprojectDependencies", () => {
	it("extracts dependencies from [project] section", () => {
		const result = parsePyprojectDependencies(`
[project]
name = "mypackage"
dependencies = [
    "chromadb>=0.4.0",
    "pyyaml>=6.0",
]
`);
		expect(result.names).toContain("chromadb");
		expect(result.names).toContain("pyyaml");
	});

	it("extracts dependencies from inline array", () => {
		const result = parsePyprojectDependencies(`
[project]
dependencies = ["requests>=2.0", "click"]
`);
		expect(result.names).toContain("requests");
		expect(result.names).toContain("click");
	});

	it("lowercases package names", () => {
		const result = parsePyprojectDependencies(`
[project]
dependencies = ["PyYAML>=6.0"]
`);
		expect(result.names).toContain("pyyaml");
	});

	it("strips version specifiers", () => {
		const result = parsePyprojectDependencies(`
[project]
dependencies = [
    "foo>=1.0,<2.0",
    "bar~=1.5",
    "baz==2.0.0",
    "qux!=1.0",
]
`);
		expect(result.names).toEqual(["foo", "bar", "baz", "qux"]);
	});

	it("strips extras from specifiers", () => {
		const result = parsePyprojectDependencies(`
[project]
dependencies = ["requests[security]>=2.0"]
`);
		expect(result.names).toContain("requests");
		expect(result.names).not.toContain("requests[security]");
	});

	it("extracts optional-dependencies", () => {
		const result = parsePyprojectDependencies(`
[project]
dependencies = ["click"]

[project.optional-dependencies]
dev = ["pytest>=7.0", "ruff>=0.1.0"]
`);
		expect(result.names).toContain("click");
		expect(result.names).toContain("pytest");
		expect(result.names).toContain("ruff");
	});

	it("returns empty for file with no dependencies", () => {
		const result = parsePyprojectDependencies(`
[build-system]
requires = ["setuptools"]
`);
		expect(result.names).toEqual([]);
	});

	it("ignores other sections", () => {
		const result = parsePyprojectDependencies(`
[tool.ruff]
line-length = 100

[project]
dependencies = ["click"]

[tool.pytest.ini_options]
testpaths = ["tests"]
`);
		expect(result.names).toEqual(["click"]);
	});

	it("handles mempalace pyproject.toml pattern", () => {
		const result = parsePyprojectDependencies(`
[build-system]
requires = ["setuptools>=64"]
build-backend = "setuptools.build_meta"

[project]
name = "mempalace"
version = "3.0.0"
dependencies = [
    "chromadb>=0.4.0",
    "pyyaml>=6.0",
]

[project.optional-dependencies]
dev = ["pytest>=7.0", "build>=1.0", "twine>=4.0"]
`);
		expect(result.names).toContain("chromadb");
		expect(result.names).toContain("pyyaml");
		expect(result.names).toContain("pytest");
		expect(result.names).toContain("build");
		expect(result.names).toContain("twine");
	});
});

// ── requirements.txt ────────────────────────────────────────────────

describe("parseRequirementsTxt", () => {
	it("extracts package names", () => {
		const result = parseRequirementsTxt(`
chromadb>=0.4.0
pyyaml>=6.0
`);
		expect(result.names).toContain("chromadb");
		expect(result.names).toContain("pyyaml");
	});

	it("lowercases package names", () => {
		const result = parseRequirementsTxt("PyYAML>=6.0");
		expect(result.names).toContain("pyyaml");
	});

	it("strips version specifiers", () => {
		const result = parseRequirementsTxt(`
requests>=2.0
click==8.0.0
flask~=2.0
`);
		expect(result.names).toEqual(["requests", "click", "flask"]);
	});

	it("skips comments", () => {
		const result = parseRequirementsTxt(`
# This is a comment
requests
# Another comment
click
`);
		expect(result.names).toEqual(["requests", "click"]);
	});

	it("skips -r and -e lines", () => {
		const result = parseRequirementsTxt(`
-r base.txt
-e git+https://github.com/foo/bar.git
requests
`);
		expect(result.names).toEqual(["requests"]);
	});

	it("skips --index-url lines", () => {
		const result = parseRequirementsTxt(`
--index-url https://pypi.org/simple
requests
`);
		expect(result.names).toEqual(["requests"]);
	});

	it("returns empty for empty file", () => {
		const result = parseRequirementsTxt("");
		expect(result.names).toEqual([]);
	});

	it("handles bare package names without versions", () => {
		const result = parseRequirementsTxt(`
requests
click
flask
`);
		expect(result.names).toEqual(["requests", "click", "flask"]);
	});
});
