/**
 * Docker Compose file parser — adapter layer.
 *
 * Parses docker-compose.yml/yaml, compose.yml/yaml into a typed DTO.
 * YAML parsing happens here; core receives only the typed DTO.
 *
 * Supported compose file formats:
 *   - docker-compose.yml / docker-compose.yaml
 *   - compose.yml / compose.yaml
 *   - Compose Specification v3.x (services, build, image)
 *
 * Not supported (out of scope):
 *   - Compose v1 (deprecated)
 *   - Kubernetes extensions
 *   - Profiles (captured but not interpreted)
 *   - Variable substitution / .env expansion
 */

import { parse as parseYaml } from "yaml";

// ── Compose DTO ────────────────────────────────────────────────────

/**
 * Typed DTO for a parsed docker-compose file.
 * Passed to core detector — no YAML types leak across boundary.
 */
export interface ComposeFile {
	/** Compose file version (if declared). */
	version: string | null;
	/** Services defined in the compose file. */
	services: ComposeService[];
	/** Repo-relative path to the compose file. */
	filePath: string;
}

/**
 * A single service from a docker-compose file.
 */
export interface ComposeService {
	/** Service name (key in the services map). */
	name: string;
	/** Image to use (if image-only service). */
	image: string | null;
	/** Build configuration (if build service). */
	build: ComposeBuild | null;
	/** Container name override (if specified). */
	containerName: string | null;
	/** Command override (if specified). */
	command: string[] | null;
	/** Entrypoint override (if specified). */
	entrypoint: string[] | null;
	/** Ports exposed (host:container or just container). */
	ports: string[];
	/** Environment variables (names only, not values). */
	envVars: string[];
	/** Profiles this service belongs to. */
	profiles: string[];
	/** Dependencies (depends_on). */
	dependsOn: string[];
}

/**
 * Build configuration for a compose service.
 */
export interface ComposeBuild {
	/** Build context path (relative to compose file). */
	context: string;
	/** Dockerfile path (relative to context). */
	dockerfile: string | null;
	/** Build target (multi-stage). */
	target: string | null;
}

// ── Parser ─────────────────────────────────────────────────────────

/**
 * Parse a docker-compose file content into a typed DTO.
 *
 * @param content - Raw YAML content
 * @param filePath - Repo-relative path to the compose file
 * @returns ComposeFile DTO, or null if parsing fails or no services found
 */
export function parseComposeFile(
	content: string,
	filePath: string,
): ComposeFile | null {
	let raw: unknown;
	try {
		raw = parseYaml(content);
	} catch {
		return null;
	}

	if (!raw || typeof raw !== "object") {
		return null;
	}

	const doc = raw as Record<string, unknown>;

	// Extract version (optional in Compose v3+).
	const version = typeof doc.version === "string" ? doc.version : null;

	// Extract services.
	const rawServices = doc.services;
	if (!rawServices || typeof rawServices !== "object") {
		return null;
	}

	const servicesMap = rawServices as Record<string, unknown>;
	const services: ComposeService[] = [];

	for (const [name, rawService] of Object.entries(servicesMap)) {
		if (!rawService || typeof rawService !== "object") {
			continue;
		}
		const svc = rawService as Record<string, unknown>;
		services.push(parseService(name, svc));
	}

	if (services.length === 0) {
		return null;
	}

	return { version, services, filePath };
}

/**
 * Parse a single service definition.
 */
function parseService(
	name: string,
	svc: Record<string, unknown>,
): ComposeService {
	return {
		name,
		image: typeof svc.image === "string" ? svc.image : null,
		build: parseBuild(svc.build),
		containerName: typeof svc.container_name === "string" ? svc.container_name : null,
		command: parseStringOrArray(svc.command),
		entrypoint: parseStringOrArray(svc.entrypoint),
		ports: parseStringArray(svc.ports),
		envVars: parseEnvVarNames(svc.environment),
		profiles: parseStringArray(svc.profiles),
		dependsOn: parseDependsOn(svc.depends_on),
	};
}

/**
 * Parse build configuration.
 */
function parseBuild(raw: unknown): ComposeBuild | null {
	if (!raw) {
		return null;
	}

	// Short form: build: ./path
	if (typeof raw === "string") {
		return {
			context: raw,
			dockerfile: null,
			target: null,
		};
	}

	// Long form: build: { context: ..., dockerfile: ..., target: ... }
	if (typeof raw === "object") {
		const build = raw as Record<string, unknown>;
		return {
			context: typeof build.context === "string" ? build.context : ".",
			dockerfile: typeof build.dockerfile === "string" ? build.dockerfile : null,
			target: typeof build.target === "string" ? build.target : null,
		};
	}

	return null;
}

/**
 * Parse a string or array of strings into string array.
 * Handles both `command: "npm start"` and `command: ["npm", "start"]`.
 */
function parseStringOrArray(raw: unknown): string[] | null {
	if (!raw) {
		return null;
	}
	if (typeof raw === "string") {
		// Shell form — split on whitespace (simplified).
		return raw.split(/\s+/).filter((s) => s.length > 0);
	}
	if (Array.isArray(raw)) {
		return raw.filter((s) => typeof s === "string") as string[];
	}
	return null;
}

/**
 * Parse an array of strings.
 */
function parseStringArray(raw: unknown): string[] {
	if (!Array.isArray(raw)) {
		return [];
	}
	return raw.filter((s) => typeof s === "string") as string[];
}

/**
 * Parse environment variable names (not values).
 * Handles both array form and object form.
 */
function parseEnvVarNames(raw: unknown): string[] {
	if (!raw) {
		return [];
	}

	// Array form: ["VAR1=value", "VAR2=value"]
	if (Array.isArray(raw)) {
		return raw
			.filter((s) => typeof s === "string")
			.map((s) => {
				const eq = (s as string).indexOf("=");
				return eq > 0 ? (s as string).slice(0, eq) : (s as string);
			});
	}

	// Object form: { VAR1: value, VAR2: value }
	if (typeof raw === "object") {
		return Object.keys(raw as Record<string, unknown>);
	}

	return [];
}

/**
 * Parse depends_on field.
 * Handles both simple array and extended object syntax.
 */
function parseDependsOn(raw: unknown): string[] {
	if (!raw) {
		return [];
	}

	// Simple form: ["db", "redis"]
	if (Array.isArray(raw)) {
		return raw.filter((s) => typeof s === "string") as string[];
	}

	// Extended form: { db: { condition: service_healthy }, redis: ... }
	if (typeof raw === "object") {
		return Object.keys(raw as Record<string, unknown>);
	}

	return [];
}

// ── File pattern helpers ───────────────────────────────────────────

/**
 * Check if a filename is a compose file pattern.
 */
export function isComposeFileName(name: string): boolean {
	const lower = name.toLowerCase();
	return (
		lower === "docker-compose.yml" ||
		lower === "docker-compose.yaml" ||
		lower === "compose.yml" ||
		lower === "compose.yaml"
	);
}
