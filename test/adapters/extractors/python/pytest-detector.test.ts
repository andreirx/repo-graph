/**
 * Pytest detector — unit tests.
 *
 * Tests detection of pytest test functions, test classes, and fixtures.
 */

import { describe, expect, it } from "vitest";
import { detectPytestItems } from "../../../../src/adapters/extractors/python/pytest-detector.js";

function sym(
	name: string,
	subtype: "FUNCTION" | "METHOD" | "CLASS" | "VARIABLE",
	lineStart: number,
	qualifiedName?: string,
) {
	return {
		stableKey: `test:src/test.py#${qualifiedName ?? name}:SYMBOL:${subtype}`,
		name,
		qualifiedName: qualifiedName ?? name,
		subtype,
		lineStart,
	};
}

// ── Test functions ──────────────────────────────────────────────────

describe("detectPytestItems — test functions", () => {
	it("detects test_* function in test file", () => {
		const results = detectPytestItems(
			"def test_basic():\n    assert True",
			"test_main.py",
			[sym("test_basic", "FUNCTION", 1)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("pytest_test_function");
		expect(results[0].confidence).toBe(0.95);
	});

	it("detects multiple test functions", () => {
		const results = detectPytestItems(
			"def test_a():\n    pass\ndef test_b():\n    pass",
			"test_main.py",
			[
				sym("test_a", "FUNCTION", 1),
				sym("test_b", "FUNCTION", 3),
			],
		);
		expect(results.length).toBe(2);
	});

	it("does not detect non-test functions in test files", () => {
		const results = detectPytestItems(
			"def helper():\n    pass\ndef test_x():\n    pass",
			"test_main.py",
			[
				sym("helper", "FUNCTION", 1),
				sym("test_x", "FUNCTION", 3),
			],
		);
		const testResults = results.filter((r) => r.convention === "pytest_test_function");
		expect(testResults.length).toBe(1);
		expect(testResults[0].targetStableKey).toContain("test_x");
	});

	it("detects test function in file with pytest import", () => {
		const results = detectPytestItems(
			"import pytest\ndef test_foo():\n    pass",
			"src/checks.py",
			[sym("test_foo", "FUNCTION", 2)],
		);
		expect(results.length).toBe(1);
	});

	it("does not detect test_* in non-test file without pytest import", () => {
		const results = detectPytestItems(
			"def test_connection():\n    pass",
			"src/utils.py",
			[sym("test_connection", "FUNCTION", 1)],
		);
		expect(results).toEqual([]);
	});
});

// ── Test classes ────────────────────────────────────────────────────

describe("detectPytestItems — test classes", () => {
	it("detects Test* class", () => {
		const results = detectPytestItems(
			"class TestUserService:\n    def test_get(self):\n        pass",
			"test_service.py",
			[
				sym("TestUserService", "CLASS", 1),
				sym("test_get", "METHOD", 2, "TestUserService.test_get"),
			],
		);
		const cls = results.find((r) => r.convention === "pytest_test_class");
		expect(cls).toBeDefined();
		expect(cls!.targetStableKey).toContain("TestUserService");

		const method = results.find((r) => r.convention === "pytest_test_function");
		expect(method).toBeDefined();
	});

	it("does not detect class not starting with Test + uppercase", () => {
		const results = detectPytestItems(
			"class Testing:\n    pass",
			"test_main.py",
			[sym("Testing", "CLASS", 1)],
		);
		const cls = results.filter((r) => r.convention === "pytest_test_class");
		expect(cls).toEqual([]);
	});
});

// ── Fixtures ────────────────────────────────────────────────────────

describe("detectPytestItems — fixtures", () => {
	it("detects @pytest.fixture", () => {
		const results = detectPytestItems(
			"import pytest\n\n@pytest.fixture\ndef db():\n    return MockDB()",
			"conftest.py",
			[sym("db", "FUNCTION", 4)],
		);
		const fixture = results.find((r) => r.convention === "pytest_fixture");
		expect(fixture).toBeDefined();
		expect(fixture!.targetStableKey).toContain("db");
	});

	it("detects @pytest.fixture with scope argument", () => {
		const results = detectPytestItems(
			"import pytest\n\n@pytest.fixture(scope='session')\ndef app():\n    return create_app()",
			"conftest.py",
			[sym("app", "FUNCTION", 4)],
		);
		const fixture = results.find((r) => r.convention === "pytest_fixture");
		expect(fixture).toBeDefined();
	});

	it("detects fixture in conftest.py", () => {
		const results = detectPytestItems(
			"import pytest\n@pytest.fixture\ndef client():\n    pass",
			"tests/conftest.py",
			[sym("client", "FUNCTION", 3)],
		);
		expect(results.length).toBeGreaterThanOrEqual(1);
		expect(results.some((r) => r.convention === "pytest_fixture")).toBe(true);
	});
});

// ── Edge cases ──────────────────────────────────────────────────────

describe("detectPytestItems — edge cases", () => {
	it("returns empty for non-test Python file", () => {
		const results = detectPytestItems(
			"def process():\n    pass",
			"src/main.py",
			[sym("process", "FUNCTION", 1)],
		);
		expect(results).toEqual([]);
	});

	it("returns empty for file with no symbols", () => {
		const results = detectPytestItems(
			"import pytest",
			"test_empty.py",
			[],
		);
		expect(results).toEqual([]);
	});

	it("handles file named *_test.py", () => {
		const results = detectPytestItems(
			"def test_foo():\n    pass",
			"service_test.py",
			[sym("test_foo", "FUNCTION", 1)],
		);
		expect(results.length).toBe(1);
	});
});

// ── Output shape ────────────────────────────────────────────────────

describe("detectPytestItems — output shape", () => {
	it("includes targetStableKey", () => {
		const results = detectPytestItems(
			"def test_x():\n    pass",
			"test_a.py",
			[sym("test_x", "FUNCTION", 1)],
		);
		expect(results[0].targetStableKey).toBeDefined();
	});

	it("includes reason string", () => {
		const results = detectPytestItems(
			"def test_x():\n    pass",
			"test_a.py",
			[sym("test_x", "FUNCTION", 1)],
		);
		expect(typeof results[0].reason).toBe("string");
		expect(results[0].reason.length).toBeGreaterThan(0);
	});
});
