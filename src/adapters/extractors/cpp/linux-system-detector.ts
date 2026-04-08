/**
 * Linux kernel / systems C framework detector.
 *
 * Scans C/C++ source files for system-level framework conventions
 * that make symbols live through kernel/runtime mechanisms rather
 * than direct call graph edges.
 *
 * Detected conventions:
 *
 *   Linux kernel module:
 *     - module_init(func)      → func is live (kernel calls on insmod)
 *     - module_exit(func)      → func is live (kernel calls on rmmod)
 *     - module_platform_driver(drv)  → init/exit macro expansion
 *     - builtin_platform_driver(drv) → built-in driver init macro
 *
 *   GCC attributes:
 *     - __attribute__((constructor))  → function is live (runtime pre-main)
 *     - __attribute__((destructor))   → function is live (runtime post-main)
 *
 *   Handler registration (swupdate style):
 *     - register_handler("name", func, ...) → func is live through registry
 *
 * Implementation: LINE-BASED REGEX scanning. Preprocessor macros are
 * matched by their invocation pattern, not by expansion.
 *
 * Maturity: PROTOTYPE. Covers the most common Linux driver and
 * embedded systems patterns.
 */

/**
 * A detected system framework convention.
 */
export interface DetectedSystemEntry {
	/** Stable key of the live symbol. */
	targetStableKey: string;
	/** Machine-stable convention identifier. */
	convention: string;
	/** Confidence in the detection (0-1). */
	confidence: number;
	/** Human-readable explanation. */
	reason: string;
}

/**
 * Detect Linux/system framework conventions in a C/C++ source file.
 *
 * @param source - Full source text.
 * @param filePath - Repo-relative path.
 * @param symbols - Already-extracted symbols from the C/C++ extractor.
 */
export function detectLinuxSystemPatterns(
	source: string,
	_filePath: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		qualifiedName: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
): DetectedSystemEntry[] {
	const results: DetectedSystemEntry[] = [];
	const lines = source.split("\n");
	const seen = new Set<string>();

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const trimmed = line.trim();

		// Skip comment lines.
		if (trimmed.startsWith("//") || trimmed.startsWith("/*") || trimmed.startsWith("*")) continue;

		// ── module_init(func) / module_exit(func) ─────────────────
		const moduleInitMatch = line.match(/\bmodule_init\s*\(\s*(\w+)\s*\)/);
		if (moduleInitMatch) {
			emitForFunc(moduleInitMatch[1], "linux_module_init",
				`function "${moduleInitMatch[1]}" registered via module_init — kernel calls on insmod`,
				0.95, symbols, results, seen);
		}

		const moduleExitMatch = line.match(/\bmodule_exit\s*\(\s*(\w+)\s*\)/);
		if (moduleExitMatch) {
			emitForFunc(moduleExitMatch[1], "linux_module_exit",
				`function "${moduleExitMatch[1]}" registered via module_exit — kernel calls on rmmod`,
				0.95, symbols, results, seen);
		}

		// ── module_platform_driver(drv) / builtin_platform_driver(drv) ──
		const platformDriverMatch = line.match(
			/\b(?:module_platform_driver|builtin_platform_driver)\s*\(\s*(\w+)\s*\)/,
		);
		if (platformDriverMatch) {
			// The macro argument is a struct, not a function.
			// Mark the struct as live (it contains .probe and .remove function pointers).
			emitForSymbol(platformDriverMatch[1], "linux_platform_driver",
				`struct "${platformDriverMatch[1]}" registered as platform driver — kernel manages lifecycle`,
				0.95, symbols, results, seen);
		}

		// ── __attribute__((constructor)) / __attribute__((destructor)) ──
		const constructorMatch = line.match(
			/__attribute__\s*\(\s*\(\s*(constructor|destructor)\s*\)\s*\)/,
		);
		if (constructorMatch) {
			const attrType = constructorMatch[1];
			// Find the function on this or next few lines.
			const func = findNearestFunctionAfterLine(i + 1, symbols, 5);
			if (func && !seen.has(func.stableKey)) {
				seen.add(func.stableKey);
				results.push({
					targetStableKey: func.stableKey,
					convention: `gcc_${attrType}`,
					confidence: 0.95,
					reason: `function "${func.name}" has __attribute__((${attrType})) — runtime calls ${attrType === "constructor" ? "before" : "after"} main`,
				});
			}
		}

		// ── register_handler("name", func, ...) ──────────────────
		const registerMatch = line.match(
			/\bregister_handler\s*\(\s*"[^"]*"\s*,\s*(\w+)/,
		);
		if (registerMatch) {
			emitForFunc(registerMatch[1], "register_handler",
				`function "${registerMatch[1]}" registered via register_handler — invoked through handler registry`,
				0.90, symbols, results, seen);
		}
	}

	return results;
}

function emitForFunc(
	funcName: string,
	convention: string,
	reason: string,
	confidence: number,
	symbols: Array<{ stableKey: string; name: string; subtype: string | null }>,
	results: DetectedSystemEntry[],
	seen: Set<string>,
): void {
	const sym = symbols.find(
		(s) => s.name === funcName &&
			(s.subtype === "FUNCTION" || s.subtype === "METHOD"),
	);
	if (sym && !seen.has(sym.stableKey)) {
		seen.add(sym.stableKey);
		results.push({ targetStableKey: sym.stableKey, convention, confidence, reason });
	}
}

function emitForSymbol(
	name: string,
	convention: string,
	reason: string,
	confidence: number,
	symbols: Array<{ stableKey: string; name: string; subtype: string | null }>,
	results: DetectedSystemEntry[],
	seen: Set<string>,
): void {
	const sym = symbols.find((s) => s.name === name);
	if (sym && !seen.has(sym.stableKey)) {
		seen.add(sym.stableKey);
		results.push({ targetStableKey: sym.stableKey, convention, confidence, reason });
	}
}

function findNearestFunctionAfterLine(
	lineNumber: number,
	symbols: Array<{
		stableKey: string;
		name: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
	maxGap: number,
): { stableKey: string; name: string } | null {
	let best: { stableKey: string; name: string; lineStart: number } | null = null;
	for (const s of symbols) {
		if (s.subtype !== "FUNCTION" && s.subtype !== "METHOD") continue;
		if (s.lineStart === null) continue;
		if (s.lineStart < lineNumber) continue;
		if (s.lineStart > lineNumber + maxGap) continue;
		if (!best || s.lineStart < best.lineStart) {
			best = { stableKey: s.stableKey, name: s.name, lineStart: s.lineStart };
		}
	}
	return best;
}
