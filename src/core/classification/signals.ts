/**
 * Classifier signal DTOs and pure predicates.
 *
 * These types define the evidence shapes the unresolved-edge
 * classifier consumes. They are plain data — no behavior, no
 * I/O — because classification is a pure function over these
 * inputs (see `unresolved-classifier.ts`).
 *
 * Signal scope:
 *   - SnapshotSignals: invariant across a snapshot (package deps,
 *     tsconfig aliases).
 *   - FileSignals: per-source-file (import bindings, same-file
 *     symbol names).
 *
 * Adapters produce the raw DTOs from filesystem sources:
 *   - `src/adapters/config/tsconfig-reader.ts` produces TsconfigAliases
 *   - `src/adapters/extractors/manifest/package-json.ts` produces PackageDependencySet
 *
 * The indexer orchestrates assembly: it gathers snapshot signals
 * once per snapshot, and builds FileSignals once per source file
 * from the extractor's output.
 *
 * The predicates in this file are pure and testable without any
 * filesystem or database access.
 */

import type { ImportBinding } from "../ports/extractor.js";

// ── Package dependencies ────────────────────────────────────────────

/**
 * The set of package names declared as dependencies of a package.
 * Union of dependencies + devDependencies + peerDependencies +
 * optionalDependencies, de-duplicated and sorted for stability.
 *
 * The DTO uses readonly arrays rather than a mutable Set so the
 * boundary between adapter and core is immutable-looking. Callers
 * who want O(1) membership can construct a Set locally.
 */
export interface PackageDependencySet {
	readonly names: readonly string[];
}

export function hasPackageDependency(
	deps: PackageDependencySet,
	name: string,
): boolean {
	// Linear scan is fine at dep-list scale (<1000 names). Callers
	// running this in a hot loop should construct a local Set.
	for (const n of deps.names) {
		if (n === name) return true;
	}
	return false;
}

// ── Runtime builtins ────────────────────────────────────────────────

/**
 * Language-agnostic DTO representing identifiers and module
 * specifiers that exist at runtime without a declaration in source.
 *
 * Populated by the extractor adapter from language-specific lists
 * (e.g. TS/JS globals, Node stdlib). The classifier sees only the
 * abstract set — no language-specific knowledge crosses the boundary.
 */
export interface RuntimeBuiltinsSet {
	/** Identifiers globally available at runtime (Map, Date, process, etc.) */
	readonly identifiers: readonly string[];
	/** Module specifiers for stdlib/runtime modules ("path", "node:fs", etc.) */
	readonly moduleSpecifiers: readonly string[];
}

export function hasRuntimeBuiltinIdentifier(
	builtins: RuntimeBuiltinsSet,
	identifier: string,
): boolean {
	for (const name of builtins.identifiers) {
		if (name === identifier) return true;
	}
	return false;
}

export function hasRuntimeBuiltinModule(
	builtins: RuntimeBuiltinsSet,
	specifier: string,
): boolean {
	for (const name of builtins.moduleSpecifiers) {
		if (name === specifier) return true;
	}
	return false;
}

// ── tsconfig path aliases ───────────────────────────────────────────

/**
 * A single entry from `compilerOptions.paths` in tsconfig.json.
 *
 * Example:
 *   "@/*": ["./src/*"]
 * → { pattern: "@/*", substitutions: ["./src/*"] }
 *
 * `substitutions` is preserved even though the first-slice classifier
 * only needs `pattern`: future rules may reason about what the alias
 * resolves to (e.g. "alias resolves to an internal path").
 */
export interface TsconfigAliasEntry {
	pattern: string;
	substitutions: string[];
}

export interface TsconfigAliases {
	readonly entries: readonly TsconfigAliasEntry[];
}

/**
 * Test whether a module specifier matches any tsconfig path alias.
 *
 * Matching semantics follow TypeScript's `paths` resolution:
 *   - Pattern ending in "*" is a PREFIX match. "@/*" matches any
 *     specifier starting with "@/".
 *   - Pattern with no "*" is an EXACT match. "@types" matches only
 *     the specifier "@types" (no prefix extension).
 *   - Pattern consisting solely of "*" is skipped: it would match
 *     every specifier, which is useless as a classification signal.
 *
 * This predicate does NOT attempt to resolve the alias to a target
 * path, nor to verify the target exists. Membership is purely
 * lexical. Substitution resolution is a future-slice concern.
 */
export function matchesAnyAlias(
	specifier: string,
	aliases: TsconfigAliases,
): boolean {
	for (const entry of aliases.entries) {
		const pattern = entry.pattern;
		if (pattern === "*") continue;
		if (pattern.endsWith("*")) {
			const prefix = pattern.slice(0, -1);
			if (prefix === "") continue;
			if (specifier.startsWith(prefix)) return true;
		} else if (specifier === pattern) {
			return true;
		}
	}
	return false;
}

// ── Aggregate signal bundles ────────────────────────────────────────

/**
 * Snapshot-scoped signals. Built once per indexing pass; shared
 * across every file's classification in that snapshot.
 */
export interface SnapshotSignals {
	readonly packageDependencies: PackageDependencySet;
	readonly tsconfigAliases: TsconfigAliases;
	readonly runtimeBuiltins: RuntimeBuiltinsSet;
}

/**
 * Per-source-file signals. Built once per source file from the
 * extractor's output. Passed to the classifier alongside each
 * unresolved edge originating in that file.
 *
 * Same-file symbol matching is SUBTYPE-AWARE. A flat name-set
 * confuses roles: e.g. a type-only `Foo` must NOT cause a runtime
 * `Foo()` call to classify as same-file. Each classifier rule picks
 * the appropriate set based on the category being classified.
 *
 *   - importBindings: every identifier-bearing import binding in
 *     this file (see ExtractionResult.importBindings).
 *   - sameFileValueSymbols: runtime-bindable identifiers declared
 *     in this file. Populated for subtypes that produce a runtime
 *     binding: FUNCTION, CLASS, METHOD, VARIABLE, CONSTANT, ENUM,
 *     ENUM_MEMBER, CONSTRUCTOR, GETTER, SETTER, PROPERTY.
 *     Used for CALLS and CALLS_OBJ_METHOD receiver matching.
 *   - sameFileClassSymbols: only NodeSubtype.CLASS. Used for
 *     INSTANTIATES (new Foo()) matching.
 *   - sameFileInterfaceSymbols: only NodeSubtype.INTERFACE. Used
 *     for IMPLEMENTS matching. Does NOT include CLASS names even
 *     though TS permits `implements ClassName` — conservative.
 *
 * Type-only subtypes (INTERFACE, TYPE_ALIAS) are NEVER members of
 * sameFileValueSymbols: their names do not bind runtime identifiers.
 */
export interface FileSignals {
	readonly importBindings: readonly ImportBinding[];
	readonly sameFileValueSymbols: ReadonlySet<string>;
	readonly sameFileClassSymbols: ReadonlySet<string>;
	readonly sameFileInterfaceSymbols: ReadonlySet<string>;
}

/**
 * Empty SnapshotSignals factory. Used when the repo has no
 * package.json or no tsconfig.json, or when those files are
 * unparseable. The classifier degrades gracefully: all rules
 * that depend on these signals simply do not fire.
 */
export function emptySnapshotSignals(): SnapshotSignals {
	return {
		packageDependencies: { names: Object.freeze([]) },
		tsconfigAliases: { entries: Object.freeze([]) },
		runtimeBuiltins: { identifiers: Object.freeze([]), moduleSpecifiers: Object.freeze([]) },
	};
}

/**
 * Empty FileSignals factory. Used for files the extractor did
 * not emit bindings for (e.g. non-TypeScript files, files with
 * zero imports and zero declarations). The classifier falls
 * through to "unknown" for every edge in such files.
 */
export function emptyFileSignals(): FileSignals {
	return {
		importBindings: Object.freeze([]),
		sameFileValueSymbols: new Set<string>(),
		sameFileClassSymbols: new Set<string>(),
		sameFileInterfaceSymbols: new Set<string>(),
	};
}
