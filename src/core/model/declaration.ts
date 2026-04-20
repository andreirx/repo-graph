import type { BoundaryDeclarationValue } from "./boundary-selector.js";
import { DeclarationKind } from "./types.js";

// Re-export boundary selector types for convenience.
export type { BoundaryDeclarationValue } from "./boundary-selector.js";
export {
	type BoundarySelectorDomain,
	type DiscoveredModuleBoundaryValue,
	type DiscoveredModuleIdentity,
	type DirectoryModuleBoundaryValue,
	isDiscoveredModuleBoundary,
	isDirectoryModuleBoundary,
	parseBoundaryValue,
} from "./boundary-selector.js";

/**
 * Storage-level declaration entity.
 *
 * This is what the database stores. `valueJson` is a raw JSON string
 * because SQLite stores TEXT. Use `parseDeclarationValue()` to get
 * a typed value, and `TypedDeclaration` for code that needs the
 * kind-to-value mapping enforced at the type level.
 */
export interface Declaration {
	declarationUid: string;
	repoUid: string;
	/** NULL for repo-wide declarations, non-NULL for snapshot-scoped */
	snapshotUid: string | null;
	targetStableKey: string;
	kind: DeclarationKind;
	/** Raw JSON. Parse with parseDeclarationValue() for type safety. */
	valueJson: string;
	createdAt: string;
	createdBy: string | null;
	supersedesUid: string | null;
	isActive: boolean;
	/** Identity basis at authoring time. See versioning-model.txt.
	 *  NULL means "unknown basis — treat as potentially stale." */
	authoredBasisJson: string | null;
}

// ── Declaration value shapes ───────────────────────────────────────────────

export interface ModuleDeclarationValue {
	purpose?: string;
	maturity?: string;
	owner?: string;
}

// BoundaryDeclarationValue is now defined in boundary-selector.ts
// and re-exported above.

export interface EntrypointDeclarationValue {
	type: "route_handler" | "event_listener" | "cron_job" | "public_export";
	description?: string;
}

export interface InvariantDeclarationValue {
	text: string;
	severity?: "critical" | "high" | "medium" | "low";
}

export interface OwnerDeclarationValue {
	team: string;
	contact?: string;
}

export interface MaturityDeclarationValue {
	level: "PROTOTYPE" | "MATURE" | "PRODUCTION";
	notes?: string;
}

/**
 * A structured requirement/constraint declaration.
 *
 * This is the first Tier 1 primitive: versioned engineering intent
 * with explicit objectives, constraints, and non-goals. The
 * `verification` array is populated by `declare obligation`, which
 * appends a new VerificationObligation and supersedes the requirement
 * with an incremented version.
 *
 * Each obligation carries a stable obligation_id that serves as the
 * reference target for waiver declarations. The semantic-match
 * contract (SEMANTIC_MATCH_FIELDS) controls when obligation_id is
 * inherited across supersession — see inheritObligationIds().
 *
 * See docs/VISION.md "Process As Entropy Containment" for the strategic context.
 */
export interface RequirementDeclarationValue {
	/** Human-assigned identifier, e.g. "REQ-001". */
	req_id: string;
	/** Requirement version. Bumped on material change. */
	version: number;
	/** What must be achieved. */
	objective: string;
	/** What must not happen. */
	constraints?: string[];
	/** Explicitly out of scope. */
	non_goals?: string[];
	/** Verification obligations: what evidence closes this requirement. */
	verification?: VerificationObligation[];
	/** Requirement lifecycle status. */
	status?: "active" | "superseded" | "withdrawn";
}

/**
 * Waiver declaration: a recorded exception to a verification obligation.
 *
 * Scope: VERSION-SCOPED. A waiver references a specific
 * (req_id, requirement_version, obligation_id) tuple. When the
 * referenced requirement is superseded (version bumped), the waiver
 * remains in the audit trail but is no longer active for the new
 * version. A new waiver must be explicitly issued.
 *
 * Lifecycle: an active waiver suppresses gate failure for its target
 * obligation. An expired waiver (expires_at in the past) behaves as
 * if it does not exist — the underlying computed verdict resumes.
 * The expired waiver remains queryable for audit.
 *
 * Audit contract: a waiver NEVER erases the underlying computed
 * verdict. Consumers (gate, evidence) must surface BOTH computed and
 * effective verdicts, plus the waiver basis. See
 * docs/architecture/measurement-model.txt (assessment layer).
 */
export interface WaiverDeclarationValue {
	/** Identifier of the requirement being waived against. */
	req_id: string;
	/** Exact requirement version this waiver applies to. */
	requirement_version: number;
	/** Stable obligation_id within that requirement version. */
	obligation_id: string;
	/** Human-readable explanation of why the exception is acceptable. */
	reason: string;
	/** When the waiver was created (ISO 8601). */
	created_at: string;
	/** Optional authorship marker (user, team, or system). */
	created_by?: string;
	/** Optional expiry. Expired waivers stop suppressing verdict. ISO 8601. */
	expires_at?: string;
	/** Optional review deadline (governance reminder). ISO 8601. */
	review_by?: string;
	/** Optional categorization for reporting (e.g. "tech_debt", "pilot"). */
	rationale_category?: string;
	/** Optional reference to the policy/gate version this waiver was made under. */
	policy_basis?: string;
}

export interface VerificationObligation {
	/**
	 * Stable identity for this obligation. UUID, inherited across
	 * requirement supersession when the semantic-match tuple
	 * (see SEMANTIC_MATCH_FIELDS) matches the prior version's obligation.
	 *
	 * This field is the reference target for waiver declarations.
	 * Waivers reference (req_id, requirement_version, obligation_id) as
	 * a version-scoped tuple. Changes to any SEMANTIC_MATCH_FIELDS field
	 * produce a new obligation_id on the next supersession, naturally
	 * invalidating existing waivers.
	 */
	obligation_id: string;
	/** Human-readable description of what must be proven. */
	obligation: string;
	/** Verification method: what kind of check satisfies this. */
	method:
		| "arch_violations"
		| "coverage_threshold"
		| "complexity_threshold"
		| "hotspot_threshold"
		| "manual_review"
		| "test_exists"
		| "custom";
	/** Target module or file path this obligation applies to. */
	target?: string;
	/** Numeric threshold (for threshold-type methods). */
	threshold?: number;
	/** Comparison operator for thresholds. Default: ">=" for coverage, "<=" for complexity. */
	operator?: ">=" | "<=" | "==" | ">" | "<";
}

/**
 * Frozen identity contract for verification obligations.
 *
 * Two obligations are the "same obligation" (share obligation_id across
 * supersession) if and only if all fields in this tuple match exactly.
 * Changing any of these fields constitutes a semantic change that
 * generates a new obligation_id on the next supersession, which in
 * turn invalidates any existing waiver targeted at the old id.
 *
 * This contract is part of the waiver identity model. Any modification
 * to this list is a breaking change to the waiver contract and
 * requires a migration that re-assigns obligation_ids across history.
 *
 * Fields NOT in this tuple (e.g. `obligation` text) are treated as
 * cosmetic and do not affect identity.
 */
export const SEMANTIC_MATCH_FIELDS = Object.freeze([
	"method",
	"target",
	"threshold",
	"operator",
] as const);

export type SemanticMatchField = (typeof SEMANTIC_MATCH_FIELDS)[number];

/**
 * Compare two obligations by their semantic identity tuple.
 * Returns true iff all fields in SEMANTIC_MATCH_FIELDS match exactly
 * (including shared undefined/missing values).
 *
 * This is the canonical check for "is this the same obligation across
 * a requirement version bump?" Used by supersession inheritance logic.
 */
export function obligationsSemanticallyMatch(
	a: Partial<VerificationObligation>,
	b: Partial<VerificationObligation>,
): boolean {
	for (const field of SEMANTIC_MATCH_FIELDS) {
		if (a[field] !== b[field]) return false;
	}
	return true;
}

/**
 * Apply the obligation identity contract across requirement supersession.
 *
 * For each obligation in `next`, search `prior` for a semantically-matching
 * obligation (by SEMANTIC_MATCH_FIELDS). If one is found, the next obligation
 * inherits its obligation_id. Otherwise a fresh id is generated.
 *
 * Matching is greedy and one-to-one: each prior obligation can be claimed
 * at most once. This prevents a single prior id from being propagated onto
 * multiple next obligations when they share the same semantic tuple (which
 * is itself a declaration model error, but should not corrupt identities).
 *
 * Inputs are not mutated. Returns a new array with obligation_id set on
 * every entry.
 *
 * Reversion protection: this function matches only against the immediate
 * prior version. It does NOT search across the full supersession history,
 * so a threshold that was 0.8 -> 0.85 -> 0.8 does not resurrect the
 * original obligation_id from version 1. The id lineage is:
 *   v1: id=A (threshold 0.8)
 *   v2: id=B (threshold 0.85, A did not match)
 *   v3: id=C (threshold 0.8, B did not match)
 * This is the correct behavior for a governed system.
 */
export function inheritObligationIds(
	prior: VerificationObligation[],
	next: Array<Omit<VerificationObligation, "obligation_id">>,
	generateId: () => string,
): VerificationObligation[] {
	const claimed = new Set<number>();
	const result: VerificationObligation[] = [];
	for (const n of next) {
		let inherited: string | null = null;
		for (let i = 0; i < prior.length; i++) {
			if (claimed.has(i)) continue;
			if (obligationsSemanticallyMatch(prior[i], n)) {
				inherited = prior[i].obligation_id;
				claimed.add(i);
				break;
			}
		}
		result.push({
			...n,
			obligation_id: inherited ?? generateId(),
		});
	}
	return result;
}

// ── Discriminated union: kind -> value ──────────────────────────────────────

/** Maps each declaration kind to its typed value shape. */
export interface DeclarationValueMap {
	[DeclarationKind.MODULE]: ModuleDeclarationValue;
	[DeclarationKind.BOUNDARY]: BoundaryDeclarationValue;
	[DeclarationKind.ENTRYPOINT]: EntrypointDeclarationValue;
	[DeclarationKind.INVARIANT]: InvariantDeclarationValue;
	[DeclarationKind.OWNER]: OwnerDeclarationValue;
	[DeclarationKind.MATURITY]: MaturityDeclarationValue;
	[DeclarationKind.REQUIREMENT]: RequirementDeclarationValue;
	[DeclarationKind.WAIVER]: WaiverDeclarationValue;
}

/**
 * A declaration with its value parsed and typed according to its kind.
 * Use this in domain logic that needs type safety beyond raw JSON.
 */
export type TypedDeclaration<K extends DeclarationKind = DeclarationKind> =
	Omit<Declaration, "valueJson" | "kind"> & {
		kind: K;
		value: DeclarationValueMap[K];
	};

/**
 * Parse and validate a Declaration's valueJson against the required fields
 * for its kind. Throws if the JSON is malformed or missing required fields.
 */
export function parseDeclarationValue<K extends DeclarationKind>(
	kind: K,
	valueJson: string,
): DeclarationValueMap[K] {
	const parsed: unknown = JSON.parse(valueJson);
	if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
		throw new DeclarationValidationError(kind, "value must be a JSON object");
	}
	const obj = parsed as Record<string, unknown>;
	validateDeclarationValue(kind, obj);
	return parsed as unknown as DeclarationValueMap[K];
}

export class DeclarationValidationError extends Error {
	constructor(
		public readonly kind: string,
		public readonly reason: string,
	) {
		super(`Invalid ${kind} declaration: ${reason}`);
		this.name = "DeclarationValidationError";
	}
}

// ── Per-kind required field validators ─────────────────────────────────────

const requiredFieldValidators: Record<
	DeclarationKind,
	(obj: Record<string, unknown>) => void
> = {
	[DeclarationKind.MODULE]: (obj) => {
		// All fields optional, but maturity is constrained when present
		if (obj.maturity !== undefined) {
			const valid = ["PROTOTYPE", "MATURE", "PRODUCTION"];
			if (!valid.includes(obj.maturity as string)) {
				throw new DeclarationValidationError(
					"module",
					`maturity must be one of: ${valid.join(", ")}`,
				);
			}
		}
	},
	[DeclarationKind.BOUNDARY]: (obj) => {
		// Boundary declarations have two forms based on selectorDomain.
		// - directory_module (legacy): requires forbids as string
		// - discovered_module: requires source.canonicalRootPath and forbids.canonicalRootPath
		// Missing domain is backwards-compatible legacy; explicit unknown is corrupted policy.
		const domain = obj.selectorDomain as string | undefined;
		if (
			domain !== undefined &&
			domain !== "directory_module" &&
			domain !== "discovered_module"
		) {
			throw new DeclarationValidationError(
				"boundary",
				`unknown selectorDomain: ${domain} (expected "directory_module" or "discovered_module")`,
			);
		}
		if (domain === "discovered_module") {
			// Validate discovered module boundary
			if (typeof obj.source !== "object" || obj.source === null) {
				throw new DeclarationValidationError(
					"boundary",
					"discovered_module boundary requires source object",
				);
			}
			if (typeof obj.forbids !== "object" || obj.forbids === null) {
				throw new DeclarationValidationError(
					"boundary",
					"discovered_module boundary requires forbids object",
				);
			}
			const source = obj.source as Record<string, unknown>;
			const forbids = obj.forbids as Record<string, unknown>;
			if (typeof source.canonicalRootPath !== "string") {
				throw new DeclarationValidationError(
					"boundary",
					"discovered_module boundary source requires canonicalRootPath",
				);
			}
			if (typeof forbids.canonicalRootPath !== "string") {
				throw new DeclarationValidationError(
					"boundary",
					"discovered_module boundary forbids requires canonicalRootPath",
				);
			}
		} else {
			// Directory module boundary (default)
			requireString(obj, "forbids", "boundary");
		}
	},
	[DeclarationKind.ENTRYPOINT]: (obj) => {
		requireString(obj, "type", "entrypoint");
		const valid = [
			"route_handler",
			"event_listener",
			"cron_job",
			"public_export",
		];
		if (!valid.includes(obj.type as string)) {
			throw new DeclarationValidationError(
				"entrypoint",
				`type must be one of: ${valid.join(", ")}`,
			);
		}
	},
	[DeclarationKind.INVARIANT]: (obj) => {
		requireString(obj, "text", "invariant");
		if (obj.severity !== undefined) {
			const valid = ["critical", "high", "medium", "low"];
			if (!valid.includes(obj.severity as string)) {
				throw new DeclarationValidationError(
					"invariant",
					`severity must be one of: ${valid.join(", ")}`,
				);
			}
		}
	},
	[DeclarationKind.OWNER]: (obj) => {
		requireString(obj, "team", "owner");
	},
	[DeclarationKind.MATURITY]: (obj) => {
		requireString(obj, "level", "maturity");
		const valid = ["PROTOTYPE", "MATURE", "PRODUCTION"];
		if (!valid.includes(obj.level as string)) {
			throw new DeclarationValidationError(
				"maturity",
				`level must be one of: ${valid.join(", ")}`,
			);
		}
	},
	[DeclarationKind.WAIVER]: (obj) => {
		requireString(obj, "req_id", "waiver");
		if (typeof obj.requirement_version !== "number") {
			throw new DeclarationValidationError(
				"waiver",
				"requirement_version must be a number",
			);
		}
		requireString(obj, "obligation_id", "waiver");
		requireString(obj, "reason", "waiver");
		requireString(obj, "created_at", "waiver");
		validateIsoDate(obj.created_at as string, "created_at", "waiver");
		// Optional ISO dates
		if (obj.expires_at !== undefined) {
			if (typeof obj.expires_at !== "string") {
				throw new DeclarationValidationError(
					"waiver",
					"expires_at must be a string when present",
				);
			}
			validateIsoDate(obj.expires_at, "expires_at", "waiver");
		}
		if (obj.review_by !== undefined) {
			if (typeof obj.review_by !== "string") {
				throw new DeclarationValidationError(
					"waiver",
					"review_by must be a string when present",
				);
			}
			validateIsoDate(obj.review_by, "review_by", "waiver");
		}
		// Optional freeform strings
		if (obj.created_by !== undefined && typeof obj.created_by !== "string") {
			throw new DeclarationValidationError(
				"waiver",
				"created_by must be a string when present",
			);
		}
		if (
			obj.rationale_category !== undefined &&
			typeof obj.rationale_category !== "string"
		) {
			throw new DeclarationValidationError(
				"waiver",
				"rationale_category must be a string when present",
			);
		}
		if (obj.policy_basis !== undefined && typeof obj.policy_basis !== "string") {
			throw new DeclarationValidationError(
				"waiver",
				"policy_basis must be a string when present",
			);
		}
	},
	[DeclarationKind.REQUIREMENT]: (obj) => {
		requireString(obj, "req_id", "requirement");
		if (typeof obj.version !== "number") {
			throw new DeclarationValidationError(
				"requirement",
				"version must be a number",
			);
		}
		requireString(obj, "objective", "requirement");
		if (obj.status !== undefined) {
			const valid = ["active", "superseded", "withdrawn"];
			if (!valid.includes(obj.status as string)) {
				throw new DeclarationValidationError(
					"requirement",
					`status must be one of: ${valid.join(", ")}`,
				);
			}
		}
		// Each verification obligation must carry a stable obligation_id
		// and a method. Pre-migration data is normalized by migration 004.
		if (obj.verification !== undefined) {
			if (!Array.isArray(obj.verification)) {
				throw new DeclarationValidationError(
					"requirement",
					"verification must be an array",
				);
			}
			for (let i = 0; i < obj.verification.length; i++) {
				const entry = obj.verification[i] as Record<string, unknown>;
				if (typeof entry !== "object" || entry === null) {
					throw new DeclarationValidationError(
						"requirement",
						`verification[${i}] must be an object`,
					);
				}
				if (
					typeof entry.obligation_id !== "string" ||
					entry.obligation_id.length === 0
				) {
					throw new DeclarationValidationError(
						"requirement",
						`verification[${i}].obligation_id is required and must be a non-empty string`,
					);
				}
				if (typeof entry.method !== "string" || entry.method.length === 0) {
					throw new DeclarationValidationError(
						"requirement",
						`verification[${i}].method is required and must be a non-empty string`,
					);
				}
			}
		}
	},
};

function validateDeclarationValue(
	kind: DeclarationKind,
	obj: Record<string, unknown>,
): void {
	requiredFieldValidators[kind](obj);
}

function requireString(
	obj: Record<string, unknown>,
	field: string,
	kind: string,
): void {
	if (typeof obj[field] !== "string" || (obj[field] as string).length === 0) {
		throw new DeclarationValidationError(
			kind,
			`"${field}" is required and must be a non-empty string`,
		);
	}
}

/**
 * Validate an ISO 8601 date-time string.
 *
 * Uses Date.parse for format tolerance (accepts both date-only and
 * full date-time forms). Rejects NaN results.
 *
 * This is a shallow check — it does not enforce strict ISO 8601
 * grammar. For waiver expiry comparisons, lexicographic ordering on
 * ISO 8601 strings is the primary correctness concern; a relaxed
 * accept here is acceptable as long as all dates are parseable.
 */
function validateIsoDate(
	value: string,
	field: string,
	kind: string,
): void {
	const ms = Date.parse(value);
	if (Number.isNaN(ms)) {
		throw new DeclarationValidationError(
			kind,
			`"${field}" must be a valid ISO 8601 date-time string (got "${value}")`,
		);
	}
}
