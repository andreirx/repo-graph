/**
 * Pytest test/fixture detector.
 *
 * Scans Python source files for pytest conventions and emits
 * framework-liveness observations. These are NODE-LEVEL facts:
 * "this function/class is a test or fixture invoked by the pytest
 * runner, therefore live even if the call graph has no inbound edges."
 *
 * Detected conventions:
 *   - test functions: `def test_*()` at module or class level
 *   - test classes: `class Test*:` containing test methods
 *   - fixtures: `@pytest.fixture` decorated functions (explicit decorator required)
 *   - parametrized tests: `@pytest.mark.parametrize` (still test functions)
 *
 * NOT detected:
 *   - conftest.py functions without @pytest.fixture decorator
 *     (would over-suppress; not all conftest functions are fixtures)
 *
 * Emitted as inferences:
 *   - kind: "pytest_test" for test functions/methods/classes
 *   - kind: "pytest_fixture" for fixture functions
 *
 * Does NOT detect:
 *   - unittest.TestCase subclasses (different framework)
 *   - doctest patterns
 *   - custom pytest plugins or hooks
 *   - pytest.ini / pyproject.toml [tool.pytest] configuration
 *
 * Maturity: PROTOTYPE. Sufficient for standard pytest projects.
 */

/**
 * A detected pytest convention.
 */
export interface DetectedPytestItem {
	/** Stable key of the test/fixture symbol. */
	targetStableKey: string;
	/** Machine-stable convention identifier. */
	convention: string;
	/** Confidence in the detection (0-1). */
	confidence: number;
	/** Human-readable explanation. */
	reason: string;
}

/**
 * Detect pytest tests and fixtures in a Python source file.
 *
 * @param source - Full source text of the .py file.
 * @param filePath - Repo-relative path.
 * @param symbols - Already-extracted symbol nodes from the Python extractor.
 */
export function detectPytestItems(
	source: string,
	filePath: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		qualifiedName: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
): DetectedPytestItem[] {
	const results: DetectedPytestItem[] = [];
	const isConftest = filePath.endsWith("conftest.py");

	// Quick gate: skip files that don't look like test files or conftest.
	// Test files conventionally: test_*.py, *_test.py, conftest.py,
	// or contain pytest import.
	const fileName = filePath.split("/").pop() ?? "";
	const isTestFile =
		fileName.startsWith("test_") ||
		fileName.endsWith("_test.py") ||
		isConftest;
	const hasPytestImport = source.includes("pytest");

	if (!isTestFile && !hasPytestImport) return results;

	const lines = source.split("\n");

	// Detect test functions/methods: def test_*
	for (const sym of symbols) {
		if (sym.name.startsWith("test_") &&
			(sym.subtype === "FUNCTION" || sym.subtype === "METHOD")) {
			results.push({
				targetStableKey: sym.stableKey,
				convention: "pytest_test_function",
				confidence: 0.95,
				reason: `${sym.subtype === "METHOD" ? "method" : "function"} "${sym.name}" matches pytest test_* convention`,
			});
		}
	}

	// Detect test classes: class Test*
	for (const sym of symbols) {
		if (sym.name.startsWith("Test") &&
			sym.subtype === "CLASS" &&
			/^Test[A-Z]/.test(sym.name)) {
			results.push({
				targetStableKey: sym.stableKey,
				convention: "pytest_test_class",
				confidence: 0.95,
				reason: `class "${sym.name}" matches pytest Test* convention`,
			});
		}
	}

	// Detect @pytest.fixture decorated functions.
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i].trim();
		if (!line.startsWith("@pytest.fixture")) continue;

		// Find the function symbol nearest after this decorator line.
		const funcSymbol = findNearestFunctionAfterLine(i + 1, symbols);
		if (!funcSymbol) continue;

		if (results.some(
			(r) => r.targetStableKey === funcSymbol.stableKey &&
				r.convention === "pytest_fixture",
		)) continue;

		results.push({
			targetStableKey: funcSymbol.stableKey,
			convention: "pytest_fixture",
			confidence: 0.95,
			reason: `function "${funcSymbol.name}" decorated with @pytest.fixture`,
		});
	}

	// In conftest.py, all top-level functions are potentially fixtures
	// even without explicit @pytest.fixture decorator (though best
	// practice is to use it). We already detect the decorated ones above.
	// Non-decorated conftest functions are not marked — that would
	// over-suppress.

	return results;
}

function findNearestFunctionAfterLine(
	lineNumber: number,
	symbols: Array<{
		stableKey: string;
		name: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
): { stableKey: string; name: string } | null {
	let best: { stableKey: string; name: string; lineStart: number } | null = null;
	for (const s of symbols) {
		if (s.subtype !== "FUNCTION" && s.subtype !== "METHOD") continue;
		if (s.lineStart === null) continue;
		if (s.lineStart < lineNumber) continue;
		if (s.lineStart > lineNumber + 10) continue;
		if (!best || s.lineStart < best.lineStart) {
			best = { stableKey: s.stableKey, name: s.name, lineStart: s.lineStart };
		}
	}
	return best;
}
