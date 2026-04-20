/**
 * Boundary selector model — typed selectors for boundary declarations.
 *
 * Boundary declarations can target different substrate domains:
 *   - directory_module: legacy MODULE nodes (directory-based)
 *   - discovered_module: manifest/workspace-detected module candidates
 *
 * Each domain has its own identity model and violation evaluation semantics.
 * The selector domain is stored in the declaration payload and determines
 * how the boundary is resolved and evaluated at query time.
 *
 * Key design decisions:
 *   - Discovered modules are identified by canonicalRootPath, not moduleKey
 *   - canonicalRootPath is the stable architectural locator
 *   - moduleKey is generated identity (changes with module kind transitions)
 *   - Stale declarations (unresolvable paths) are surfaced explicitly
 */

// ── Selector domain ────────────────────────────────────────────────

/**
 * Discriminator for boundary selector domains.
 *
 * - directory_module: targets directory-based MODULE nodes (existing)
 * - discovered_module: targets manifest-detected module candidates (new)
 */
export type BoundarySelectorDomain = "directory_module" | "discovered_module";

// ── Discovered module selectors ────────────────────────────────────

/**
 * Identity selector for a discovered module.
 * Uses canonicalRootPath as the stable architectural locator.
 */
export interface DiscoveredModuleIdentity {
	readonly canonicalRootPath: string;
}

/**
 * Boundary declaration payload for discovered modules.
 *
 * Both source and target are identified by canonicalRootPath.
 * The source is the module that must not depend on the target.
 */
export interface DiscoveredModuleBoundaryValue {
	readonly selectorDomain: "discovered_module";
	readonly source: DiscoveredModuleIdentity;
	readonly forbids: DiscoveredModuleIdentity;
	readonly reason?: string;
}

// ── Directory module selectors (existing) ──────────────────────────

/**
 * Boundary declaration payload for directory modules (legacy).
 *
 * The source is encoded in Declaration.targetStableKey as `{repoUid}:{path}:MODULE`.
 * The target is the `forbids` field (just the path, not full stable key).
 *
 * selectorDomain is optional for backwards compatibility — missing means
 * "directory_module" (the original semantics).
 */
export interface DirectoryModuleBoundaryValue {
	readonly selectorDomain?: "directory_module";
	readonly forbids: string;
	readonly reason?: string;
}

// ── Union type ─────────────────────────────────────────────────────

/**
 * Boundary declaration value — discriminated union over selector domains.
 *
 * Use `isDiscoveredModuleBoundary()` and `isDirectoryModuleBoundary()`
 * for type-safe discrimination.
 */
export type BoundaryDeclarationValue =
	| DirectoryModuleBoundaryValue
	| DiscoveredModuleBoundaryValue;

// ── Type guards ────────────────────────────────────────────────────

/**
 * Type guard for discovered module boundary declarations.
 */
export function isDiscoveredModuleBoundary(
	value: BoundaryDeclarationValue,
): value is DiscoveredModuleBoundaryValue {
	return value.selectorDomain === "discovered_module";
}

/**
 * Type guard for directory module boundary declarations (legacy).
 * Includes declarations without explicit selectorDomain (backwards compat).
 */
export function isDirectoryModuleBoundary(
	value: BoundaryDeclarationValue,
): value is DirectoryModuleBoundaryValue {
	return (
		value.selectorDomain === "directory_module" ||
		value.selectorDomain === undefined
	);
}

// ── Parsing and validation ─────────────────────────────────────────

/**
 * Parse and validate a boundary declaration value from JSON.
 * Returns the typed value or throws on validation failure.
 */
export function parseBoundaryValue(json: string): BoundaryDeclarationValue {
	const obj = JSON.parse(json) as Record<string, unknown>;

	const domain = obj.selectorDomain as string | undefined;

	// Reject explicit but unrecognized selectorDomain values.
	// Missing domain is backwards-compatible legacy; explicit unknown is corrupted policy.
	if (
		domain !== undefined &&
		domain !== "directory_module" &&
		domain !== "discovered_module"
	) {
		throw new Error(
			`unknown boundary selectorDomain: ${domain} (expected "directory_module" or "discovered_module")`,
		);
	}

	if (domain === "discovered_module") {
		// Validate discovered module boundary
		if (typeof obj.source !== "object" || obj.source === null) {
			throw new Error("discovered_module boundary requires source object");
		}
		if (typeof obj.forbids !== "object" || obj.forbids === null) {
			throw new Error("discovered_module boundary requires forbids object");
		}
		const source = obj.source as Record<string, unknown>;
		const forbids = obj.forbids as Record<string, unknown>;
		if (typeof source.canonicalRootPath !== "string") {
			throw new Error(
				"discovered_module boundary source requires canonicalRootPath",
			);
		}
		if (typeof forbids.canonicalRootPath !== "string") {
			throw new Error(
				"discovered_module boundary forbids requires canonicalRootPath",
			);
		}
		return {
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: source.canonicalRootPath },
			forbids: { canonicalRootPath: forbids.canonicalRootPath },
			reason: typeof obj.reason === "string" ? obj.reason : undefined,
		};
	}

	// Directory module boundary (default)
	if (typeof obj.forbids !== "string") {
		throw new Error("directory_module boundary requires forbids string");
	}
	return {
		selectorDomain:
			domain === "directory_module" ? "directory_module" : undefined,
		forbids: obj.forbids,
		reason: typeof obj.reason === "string" ? obj.reason : undefined,
	};
}
