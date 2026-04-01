import { DeclarationKind } from "./types.js";

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
}

// ── Declaration value shapes ───────────────────────────────────────────────

export interface ModuleDeclarationValue {
	purpose?: string;
	maturity?: string;
	owner?: string;
}

export interface BoundaryDeclarationValue {
	forbids: string;
	reason?: string;
}

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

// ── Discriminated union: kind -> value ──────────────────────────────────────

/** Maps each declaration kind to its typed value shape. */
export interface DeclarationValueMap {
	[DeclarationKind.MODULE]: ModuleDeclarationValue;
	[DeclarationKind.BOUNDARY]: BoundaryDeclarationValue;
	[DeclarationKind.ENTRYPOINT]: EntrypointDeclarationValue;
	[DeclarationKind.INVARIANT]: InvariantDeclarationValue;
	[DeclarationKind.OWNER]: OwnerDeclarationValue;
	[DeclarationKind.MATURITY]: MaturityDeclarationValue;
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
		requireString(obj, "forbids", "boundary");
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
