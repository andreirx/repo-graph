/**
 * Project surface detectors — pure functions.
 *
 * Take parsed manifest content and return detected project surfaces
 * with evidence. No file I/O.
 *
 * Phase 0 detectors:
 *   - package.json: bin → cli, main/exports → library, scripts → build system
 *   - package.json: framework deps → web_app/backend_service signals
 *   - Cargo.toml: [[bin]] → cli, [lib] → library
 *   - pyproject.toml: [project.scripts] → cli
 *
 * Phase 1A detectors:
 *   - Dockerfile → container surface (backend_service or cli)
 *   - docker-compose → container surfaces per service
 *
 * NOT yet implemented:
 *   - Terraform/Pulumi/Helm → infra_root
 *   - Gradle application plugin
 *   - Worker/queue detection (requires deeper analysis)
 *   - Makefile/CMake targets
 *   - Workspace member detection (separate module discovery track)
 */

import type {
	BuildSystem,
	RuntimeKind,
	SurfaceEvidenceKind,
	SurfaceEvidenceSourceType,
	SurfaceKind,
} from "./project-surface.js";

// ── Detector output ────────────────────────────────────────────────

/**
 * Lightweight detection result. The orchestrator enriches this
 * with UIDs, snapshot context, and module linkage.
 *
 * Identity fields (sourceType + source-specific fields) are required
 * for stable surface key computation. New detected surfaces must have
 * explicit identity evidence.
 */
export interface DetectedSurface {
	surfaceKind: SurfaceKind;
	displayName: string | null;
	rootPath: string;
	entrypointPath: string | null;
	buildSystem: BuildSystem;
	runtimeKind: RuntimeKind;
	confidence: number;
	evidence: DetectedSurfaceEvidence[];
	metadata: Record<string, unknown> | null;

	// ── Identity fields (required for stable key computation) ──────────
	/**
	 * Primary source type for this surface. Required.
	 * Determines how sourceSpecificId is computed.
	 */
	sourceType: SurfaceEvidenceSourceType;

	// ── Source-specific identity fields (populate based on sourceType) ─

	// Phase 0 identity fields
	/** For package_json_bin, cargo_bin_target: the binary name. */
	binName?: string;
	/** For framework_dependency: the framework name (e.g., "express", "react"). */
	frameworkName?: string;
	/** For pyproject_scripts: the script/entrypoint name. */
	scriptName?: string;
	/** For docker_compose: the service name. Required when sourceType is docker_compose. */
	serviceName?: string;
	/** For dockerfile: path to the Dockerfile. Required when sourceType is dockerfile. */
	dockerfilePath?: string;

	// Phase 1 identity fields (reserved, detectors not implemented)
	/** For makefile_target, cmake_target: path to Makefile/CMakeLists.txt. */
	makefilePath?: string;
	/** For workspace source types: workspace member name or path. */
	workspaceName?: string;
	/** For helm_chart: chart name. */
	chartName?: string;
	/** For terraform_root, pulumi_yaml: infra module path. */
	infraModulePath?: string;
}

export interface DetectedSurfaceEvidence {
	sourceType: SurfaceEvidenceSourceType;
	sourcePath: string;
	evidenceKind: SurfaceEvidenceKind;
	confidence: number;
	payload: Record<string, unknown> | null;
}

// ── package.json detector ──────────────────────────────────────────

/**
 * Detect project surfaces from a package.json.
 *
 * Signals:
 *   - `bin` field → cli surface
 *   - `main` or `exports` → library surface
 *   - framework deps (express, fastify, next, react, vue) → web_app / backend_service
 *   - `scripts.test` → test_harness (only if this is a test-focused package)
 *   - `scripts.build` → build system inference
 */
export function detectPackageJsonSurfaces(
	content: string,
	manifestRelPath: string,
): DetectedSurface[] {
	let pkg: Record<string, unknown>;
	try {
		pkg = JSON.parse(content);
	} catch {
		return [];
	}

	const surfaces: DetectedSurface[] = [];
	const dir = manifestRelPath.includes("/")
		? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
		: ".";
	const pkgName = typeof pkg.name === "string" ? pkg.name : null;

	// Infer build system from scripts + type field.
	const scripts = (typeof pkg.scripts === "object" && pkg.scripts !== null)
		? pkg.scripts as Record<string, string>
		: {};
	const buildSystem = inferJsBuildSystem(scripts, pkg);

	// Collect all deps for framework detection.
	const allDeps = collectDeps(pkg);

	// 1. CLI surface: bin field.
	if (pkg.bin) {
		const binEntries = typeof pkg.bin === "string"
			? { [pkgName ?? "cli"]: pkg.bin as string }
			: (typeof pkg.bin === "object" && pkg.bin !== null)
				? pkg.bin as Record<string, string>
				: {};

		for (const [binName, binPath] of Object.entries(binEntries)) {
			surfaces.push({
				surfaceKind: "cli",
				displayName: binName,
				rootPath: dir,
				entrypointPath: normalizePath(dir, binPath),
				buildSystem,
				runtimeKind: "node",
				confidence: 0.95,
				evidence: [{
					sourceType: "package_json_bin",
					sourcePath: manifestRelPath,
					evidenceKind: "binary_entrypoint",
					confidence: 0.95,
					payload: { binName, binPath },
				}],
				metadata: null,
				// Identity fields
				sourceType: "package_json_bin",
				binName,
			});
		}
	}

	// 2. Library surface: main, module, or exports field.
	const hasMain = typeof pkg.main === "string";
	const hasModule = typeof pkg.module === "string";
	const hasExports = pkg.exports !== undefined;
	if (hasMain || hasModule || hasExports) {
		const mainPath = typeof pkg.main === "string" ? pkg.main : null;
		surfaces.push({
			surfaceKind: "library",
			displayName: pkgName,
			rootPath: dir,
			entrypointPath: mainPath ? normalizePath(dir, mainPath) : null,
			buildSystem,
			runtimeKind: "node",
			confidence: 0.85,
			evidence: [{
				sourceType: "package_json_main",
				sourcePath: manifestRelPath,
				evidenceKind: "main_export",
				confidence: 0.85,
				payload: {
					main: pkg.main ?? null,
					module: pkg.module ?? null,
					hasExports,
				},
			}],
			metadata: null,
			// Identity fields
			sourceType: "package_json_main",
		});
	}

	// 3. Framework-based surface detection.
	const frameworkSurface = detectFrameworkFromDeps(allDeps, dir, manifestRelPath, buildSystem);
	if (frameworkSurface) {
		surfaces.push(frameworkSurface);
	}

	// 4. Script-only fallback — if no surfaces were produced from bin/main/framework,
	//    check for operational scripts and create a low-confidence surface.
	if (surfaces.length === 0 && Object.keys(scripts).length > 0) {
		const scriptFallback = detectScriptOnlyFallback(
			scripts,
			pkgName,
			dir,
			manifestRelPath,
			buildSystem,
		);
		if (scriptFallback) {
			surfaces.push(scriptFallback);
		}
	}

	return surfaces;
}

// ── Cargo.toml detector ────────────────────────────────────────────

/**
 * Detect project surfaces from a Cargo.toml.
 *
 * Signals:
 *   - `[[bin]]` or `src/main.rs` → cli surface
 *   - `[lib]` or `src/lib.rs` exists in package → library surface
 */
export function detectCargoSurfaces(
	content: string,
	manifestRelPath: string,
): DetectedSurface[] {
	const surfaces: DetectedSurface[] = [];
	const dir = manifestRelPath.includes("/")
		? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
		: ".";

	const lines = content.split("\n");
	let inPackage = false;
	let inBin = false;
	let crateName: string | null = null;
	let hasBinSection = false;
	let hasLibSection = false;
	let binName: string | null = null;
	let binPath: string | null = null;

	for (const rawLine of lines) {
		const line = rawLine.trim();
		if (line === "[package]") { inPackage = true; inBin = false; continue; }
		if (line === "[[bin]]") { inBin = true; inPackage = false; hasBinSection = true; continue; }
		if (line === "[lib]") { inPackage = false; inBin = false; hasLibSection = true; continue; }
		if (line.startsWith("[") && line.endsWith("]")) {
			inPackage = false; inBin = false; continue;
		}

		if (inPackage) {
			const nameMatch = line.match(/^name\s*=\s*"([^"]+)"/);
			if (nameMatch) crateName = nameMatch[1];
		}
		if (inBin) {
			const nameMatch = line.match(/^name\s*=\s*"([^"]+)"/);
			if (nameMatch) binName = nameMatch[1];
			const pathMatch = line.match(/^path\s*=\s*"([^"]+)"/);
			if (pathMatch) binPath = pathMatch[1];
		}
	}

	// CLI surface: explicit [[bin]] or implicit src/main.rs convention.
	if (hasBinSection || content.includes("src/main.rs")) {
		const effectiveBinName = binName ?? crateName ?? "default";
		surfaces.push({
			surfaceKind: "cli",
			displayName: binName ?? crateName,
			rootPath: dir,
			entrypointPath: binPath
				? normalizePath(dir, binPath)
				: normalizePath(dir, "src/main.rs"),
			buildSystem: "cargo",
			runtimeKind: "rust_native",
			confidence: hasBinSection ? 0.95 : 0.80,
			evidence: [{
				sourceType: "cargo_bin_target",
				sourcePath: manifestRelPath,
				evidenceKind: "binary_entrypoint",
				confidence: hasBinSection ? 0.95 : 0.80,
				payload: { binName, binPath, convention: hasBinSection ? "explicit_bin" : "src_main_rs" },
			}],
			metadata: null,
			// Identity fields
			sourceType: "cargo_bin_target",
			binName: effectiveBinName,
		});
	}

	// Library surface: explicit [lib] or implicit src/lib.rs convention.
	if (hasLibSection || content.includes("src/lib.rs")) {
		surfaces.push({
			surfaceKind: "library",
			displayName: crateName,
			rootPath: dir,
			entrypointPath: normalizePath(dir, "src/lib.rs"),
			buildSystem: "cargo",
			runtimeKind: "rust_native",
			confidence: hasLibSection ? 0.95 : 0.80,
			evidence: [{
				sourceType: "cargo_lib_target",
				sourcePath: manifestRelPath,
				evidenceKind: "main_export",
				confidence: hasLibSection ? 0.95 : 0.80,
				payload: { convention: hasLibSection ? "explicit_lib" : "src_lib_rs" },
			}],
			metadata: null,
			// Identity fields
			sourceType: "cargo_lib_target",
		});
	}

	return surfaces;
}

// ── pyproject.toml detector ────────────────────────────────────────

/**
 * Detect project surfaces from a pyproject.toml.
 *
 * Signals:
 *   - [project.scripts] → cli surface
 *   - [tool.poetry.scripts] → cli surface
 *   - [project] with name → library surface (Python package)
 */
export function detectPyprojectSurfaces(
	content: string,
	manifestRelPath: string,
): DetectedSurface[] {
	const surfaces: DetectedSurface[] = [];
	const dir = manifestRelPath.includes("/")
		? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
		: ".";

	const lines = content.split("\n");
	let inProjectScripts = false;
	let inPoetryScripts = false;
	let inProject = false;
	let packageName: string | null = null;
	const cliEntrypoints: Array<{ name: string; target: string }> = [];

	for (const rawLine of lines) {
		const line = rawLine.trim();
		if (line === "[project.scripts]") { inProjectScripts = true; inPoetryScripts = false; inProject = false; continue; }
		if (line === "[tool.poetry.scripts]") { inPoetryScripts = true; inProjectScripts = false; inProject = false; continue; }
		if (line === "[project]") { inProject = true; inProjectScripts = false; inPoetryScripts = false; continue; }
		if (line.startsWith("[") && line.endsWith("]")) {
			inProjectScripts = false; inPoetryScripts = false; inProject = false; continue;
		}

		if (inProject) {
			const nameMatch = line.match(/^name\s*=\s*"([^"]+)"/);
			if (nameMatch) packageName = nameMatch[1];
		}

		if (inProjectScripts || inPoetryScripts) {
			const entryMatch = line.match(/^(\S+)\s*=\s*"([^"]+)"/);
			if (entryMatch) {
				cliEntrypoints.push({ name: entryMatch[1], target: entryMatch[2] });
			}
		}
	}

	// CLI surfaces from script entrypoints.
	for (const ep of cliEntrypoints) {
		surfaces.push({
			surfaceKind: "cli",
			displayName: ep.name,
			rootPath: dir,
			entrypointPath: null, // Python console_scripts point to module:function, not file paths
			buildSystem: content.includes("[tool.poetry]") ? "python_poetry" : "python_setuptools",
			runtimeKind: "python",
			confidence: 0.90,
			evidence: [{
				sourceType: "pyproject_scripts",
				sourcePath: manifestRelPath,
				evidenceKind: "binary_entrypoint",
				confidence: 0.90,
				payload: { scriptName: ep.name, target: ep.target },
			}],
			metadata: null,
			// Identity fields
			sourceType: "pyproject_scripts",
			scriptName: ep.name,
		});
	}

	// Library surface if package name is declared and no explicit bin-only signals.
	if (packageName) {
		surfaces.push({
			surfaceKind: "library",
			displayName: packageName,
			rootPath: dir,
			entrypointPath: null,
			buildSystem: content.includes("[tool.poetry]") ? "python_poetry" : "python_setuptools",
			runtimeKind: "python",
			confidence: 0.75,
			evidence: [{
				sourceType: "pyproject_entry_points",
				sourcePath: manifestRelPath,
				evidenceKind: "main_export",
				confidence: 0.75,
				payload: { packageName },
			}],
			metadata: null,
			// Identity fields
			sourceType: "pyproject_entry_points",
		});
	}

	return surfaces;
}

// ── Helpers ────────────────────────────────────────────────────────

function normalizePath(dir: string, relPath: string): string {
	if (dir === ".") return relPath;
	return `${dir}/${relPath}`;
}

function inferJsBuildSystem(
	scripts: Record<string, string>,
	pkg: Record<string, unknown>,
): BuildSystem {
	const buildScript = scripts.build ?? "";
	if (buildScript.includes("tsc")) return "typescript_tsc";
	if (buildScript.includes("vite") || buildScript.includes("webpack") ||
		buildScript.includes("esbuild") || buildScript.includes("rollup") ||
		buildScript.includes("next build") || buildScript.includes("nuxt build")) {
		return "typescript_bundler";
	}
	if (pkg.type === "module" || pkg.types || pkg.typings) return "typescript_tsc";
	return "unknown";
}

function collectDeps(pkg: Record<string, unknown>): Set<string> {
	const deps = new Set<string>();
	for (const field of ["dependencies", "devDependencies", "peerDependencies"]) {
		const d = pkg[field];
		if (typeof d === "object" && d !== null) {
			for (const name of Object.keys(d as Record<string, unknown>)) {
				deps.add(name);
			}
		}
	}
	return deps;
}

/**
 * Detect web_app or backend_service from framework dependencies.
 */
function detectFrameworkFromDeps(
	deps: Set<string>,
	dir: string,
	manifestRelPath: string,
	buildSystem: BuildSystem,
): DetectedSurface | null {
	// Backend service frameworks.
	const backendFrameworks = ["express", "fastify", "koa", "hapi", "nestjs", "@nestjs/core", "hono"];
	for (const fw of backendFrameworks) {
		if (deps.has(fw)) {
			return {
				surfaceKind: "backend_service",
				displayName: null,
				rootPath: dir,
				entrypointPath: null,
				buildSystem,
				runtimeKind: "node",
				confidence: 0.75,
				evidence: [{
					sourceType: "framework_dependency",
					sourcePath: manifestRelPath,
					evidenceKind: "framework_signal",
					confidence: 0.75,
					payload: { framework: fw },
				}],
				metadata: { framework: fw },
				// Identity fields
				sourceType: "framework_dependency",
				frameworkName: fw,
			};
		}
	}

	// Frontend/web app frameworks.
	const webAppFrameworks = ["next", "nuxt", "react", "vue", "svelte", "angular", "@angular/core", "solid-js", "astro"];
	for (const fw of webAppFrameworks) {
		if (deps.has(fw)) {
			return {
				surfaceKind: "web_app",
				displayName: null,
				rootPath: dir,
				entrypointPath: null,
				buildSystem: buildSystem === "typescript_tsc" ? "typescript_bundler" : buildSystem,
				runtimeKind: "browser",
				confidence: 0.70,
				evidence: [{
					sourceType: "framework_dependency",
					sourcePath: manifestRelPath,
					evidenceKind: "framework_signal",
					confidence: 0.70,
					payload: { framework: fw },
				}],
				metadata: { framework: fw },
				// Identity fields
				sourceType: "framework_dependency",
				frameworkName: fw,
			};
		}
	}

	return null;
}

/**
 * Script-only fallback — produces a low-confidence surface when a package.json
 * has scripts but no bin, main/exports, or framework deps.
 *
 * Detection rules:
 *   - If scripts.start, scripts.dev, scripts.serve, or scripts.server exists:
 *     → backend_service surface, confidence 0.55
 *   - Else if scripts.build, scripts.test, scripts.lint, or scripts.typecheck exists:
 *     → library surface, confidence 0.50
 *   - Else: null (no meaningful scripts)
 *
 * The low confidence (0.50-0.55) signals that the surface kind is inferred
 * from scripts alone, not from explicit entrypoint declarations.
 */
function detectScriptOnlyFallback(
	scripts: Record<string, string>,
	pkgName: string | null,
	dir: string,
	manifestRelPath: string,
	buildSystem: BuildSystem,
): DetectedSurface | null {
	// Classify scripts by role.
	const serviceScripts = ["start", "dev", "serve", "server"];
	const buildScripts = ["build", "test", "lint", "typecheck", "check", "compile"];

	const hasServiceScript = serviceScripts.some(s => s in scripts);

	// Determine which scripts are present.
	const scriptRoles: Record<string, string> = {};
	for (const name of serviceScripts) {
		if (name in scripts) scriptRoles[name] = "service";
	}
	for (const name of buildScripts) {
		if (name in scripts) scriptRoles[name] = "build";
	}

	// Build evidence for each relevant script.
	const evidence: DetectedSurfaceEvidence[] = [];
	for (const [scriptName, command] of Object.entries(scripts)) {
		const role = scriptRoles[scriptName];
		if (role) {
			evidence.push({
				sourceType: "package_json_scripts",
				sourcePath: manifestRelPath,
				evidenceKind: "script_command",
				confidence: hasServiceScript ? 0.55 : 0.50,
				payload: { scriptName, command, role },
			});
		}
	}

	if (evidence.length === 0) {
		return null;
	}

	// Determine surface kind.
	const surfaceKind: SurfaceKind = hasServiceScript ? "backend_service" : "library";
	const confidence = hasServiceScript ? 0.55 : 0.50;

	return {
		surfaceKind,
		displayName: pkgName,
		rootPath: dir,
		entrypointPath: null, // Script-only packages have no explicit entrypoint
		buildSystem,
		runtimeKind: "node",
		confidence,
		evidence,
		metadata: {
			fallbackReason: "script_only_package",
			scripts: Object.fromEntries(
				Object.entries(scripts).filter(([name]) => name in scriptRoles)
			),
			scriptRoles,
		},
		// Identity fields
		sourceType: "package_json_scripts",
	};
}

// ── Dockerfile detector ────────────────────────────────────────────

/**
 * Detect project surface from a Dockerfile.
 *
 * Parsing strategy:
 *   - Line-oriented, no shell expansion or build arg substitution
 *   - Multi-stage: only final FROM stage is considered
 *   - ENTRYPOINT and CMD in final stage determine surfaceKind
 *
 * Surface kind inference:
 *   - Explicit ENTRYPOINT/CMD with long-running patterns → backend_service (0.85)
 *   - Explicit ENTRYPOINT/CMD with one-shot patterns → cli (0.80)
 *   - No ENTRYPOINT/CMD → backend_service with lower confidence (0.60)
 *
 * Runtime:
 *   - runtimeKind is always "container"
 *   - Base image runtime (node, python, rust, etc.) stored in metadata
 */
export function detectDockerfileSurfaces(
	content: string,
	manifestRelPath: string,
): DetectedSurface[] {
	const dir = manifestRelPath.includes("/")
		? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
		: ".";

	// Parse Dockerfile instructions in final stage only.
	const parsed = parseDockerfileFinalStage(content);
	if (!parsed) {
		// No FROM instruction found — not a valid Dockerfile.
		return [];
	}

	// Infer surface kind from ENTRYPOINT/CMD.
	const { surfaceKind, confidence } = inferDockerfileSurfaceKind(
		parsed.entrypoint,
		parsed.cmd,
	);

	// Infer base runtime from final FROM image.
	const baseRuntimeKind = inferBaseRuntime(parsed.baseImage);

	return [{
		surfaceKind,
		displayName: null,
		rootPath: dir,
		entrypointPath: null,
		buildSystem: "unknown",
		runtimeKind: "container",
		confidence,
		evidence: [{
			sourceType: "dockerfile",
			sourcePath: manifestRelPath,
			evidenceKind: "container_config",
			confidence,
			payload: {
				baseImage: parsed.baseImage,
				baseRuntimeKind,
				entrypoint: parsed.entrypoint,
				cmd: parsed.cmd,
				stageCount: parsed.stageCount,
			},
		}],
		metadata: {
			baseImage: parsed.baseImage,
			baseRuntimeKind,
			entrypoint: parsed.entrypoint,
			cmd: parsed.cmd,
		},
		// Identity fields
		sourceType: "dockerfile",
		dockerfilePath: manifestRelPath,
	}];
}

/**
 * Parsed Dockerfile final stage information.
 */
interface DockerfileParsed {
	baseImage: string;
	entrypoint: string[] | null;
	cmd: string[] | null;
	stageCount: number;
}

/**
 * Parse Dockerfile and extract final stage information.
 *
 * Multi-stage handling:
 *   - Counts FROM instructions to determine stage count
 *   - Only ENTRYPOINT/CMD after the last FROM are considered
 *
 * Returns null if no FROM instruction found.
 */
function parseDockerfileFinalStage(content: string): DockerfileParsed | null {
	const lines = content.split("\n");

	let stageCount = 0;
	let baseImage: string | null = null;
	let entrypoint: string[] | null = null;
	let cmd: string[] | null = null;

	for (const rawLine of lines) {
		const line = rawLine.trim();

		// Skip comments and empty lines.
		if (line.startsWith("#") || line === "") {
			continue;
		}

		// Handle line continuations (trailing backslash).
		// For simplicity, we don't join continued lines — we just parse
		// the instruction keyword from the first line.

		const upperLine = line.toUpperCase();

		if (upperLine.startsWith("FROM ")) {
			// New stage — reset entrypoint/cmd, capture base image.
			stageCount++;
			entrypoint = null;
			cmd = null;
			// Extract image name: skip flags (--platform=...), stop before AS.
			baseImage = parseFromInstruction(line);
		} else if (upperLine.startsWith("ENTRYPOINT ")) {
			entrypoint = parseDockerInstruction(line, "ENTRYPOINT");
		} else if (upperLine.startsWith("CMD ")) {
			cmd = parseDockerInstruction(line, "CMD");
		}
	}

	if (stageCount === 0 || baseImage === null) {
		return null;
	}

	return { baseImage, entrypoint, cmd, stageCount };
}

/**
 * Parse FROM instruction to extract the image name.
 *
 * Syntax: FROM [--platform=<platform>] <image> [AS <name>]
 *
 * Skips any leading flags (--platform, etc.) and stops before AS.
 * Returns null if no valid image token found.
 */
function parseFromInstruction(line: string): string | null {
	// Remove "FROM " prefix.
	const afterFrom = line.slice(5).trim();
	const tokens = afterFrom.split(/\s+/);

	for (let i = 0; i < tokens.length; i++) {
		const token = tokens[i];

		// Skip flags (--platform=..., --platform ...).
		if (token.startsWith("--")) {
			// If flag doesn't contain '=', next token is the value — skip it too.
			if (!token.includes("=") && i + 1 < tokens.length) {
				i++;
			}
			continue;
		}

		// Stop before AS (case-insensitive).
		if (token.toUpperCase() === "AS") {
			break;
		}

		// First non-flag, non-AS token is the image.
		return token;
	}

	return null;
}

/**
 * Parse ENTRYPOINT or CMD instruction value.
 *
 * Supports:
 *   - JSON form: ENTRYPOINT ["executable", "arg1"]
 *   - Shell form: ENTRYPOINT executable arg1 (converted to array)
 *
 * Returns null if parsing fails.
 */
function parseDockerInstruction(line: string, instruction: string): string[] | null {
	// Remove instruction keyword.
	const value = line.slice(instruction.length).trim();

	if (value.startsWith("[")) {
		// JSON form.
		try {
			const parsed = JSON.parse(value);
			if (Array.isArray(parsed) && parsed.every((e) => typeof e === "string")) {
				return parsed;
			}
		} catch {
			// Fall through to shell form parsing.
		}
	}

	// Shell form — split on whitespace (simplified, no quote handling).
	const parts = value.split(/\s+/).filter((p) => p.length > 0);
	return parts.length > 0 ? parts : null;
}

/**
 * Infer surfaceKind from ENTRYPOINT and CMD.
 *
 * Token-level matching to avoid false positives from substring scans.
 * Examines:
 *   - Executable basename (first token, path stripped)
 *   - Subcommand (second token for package managers)
 *   - Flag tokens (--help, --version)
 *
 * Confidence:
 *   - Explicit ENTRYPOINT/CMD with clear pattern → 0.85 (service) or 0.80 (cli)
 *   - Explicit ENTRYPOINT/CMD with unclear pattern → 0.70 (default to service)
 *   - No ENTRYPOINT/CMD → 0.60 (default to service)
 */
function inferDockerfileSurfaceKind(
	entrypoint: string[] | null,
	cmd: string[] | null,
): { surfaceKind: SurfaceKind; confidence: number } {
	if (!entrypoint && !cmd) {
		// No explicit command — default to backend_service with low confidence.
		return { surfaceKind: "backend_service", confidence: 0.60 };
	}

	// Combine ENTRYPOINT and CMD for analysis.
	const fullCommand = [...(entrypoint ?? []), ...(cmd ?? [])];
	if (fullCommand.length === 0) {
		return { surfaceKind: "backend_service", confidence: 0.60 };
	}

	// Normalize tokens to lowercase.
	const tokens = fullCommand.map((t) => t.toLowerCase());

	// Extract executable basename from first token (strip path).
	const executable = getBasename(tokens[0]);

	// Check for CLI flag tokens anywhere (exact match).
	const cliFlags = new Set(["--help", "-h", "--version", "-v", "help", "version"]);
	for (const token of tokens) {
		if (cliFlags.has(token)) {
			return { surfaceKind: "cli", confidence: 0.80 };
		}
	}

	// Service executables — exact match on basename.
	const serviceExecutables = new Set([
		"gunicorn", "uvicorn", "nginx", "httpd", "apache2",
		"supervisord", "daphne", "hypercorn",
	]);
	if (serviceExecutables.has(executable)) {
		return { surfaceKind: "backend_service", confidence: 0.85 };
	}

	// Package manager + subcommand patterns.
	if (tokens.length >= 2) {
		const subcommand = tokens[1];

		// Direct service subcommands (no script name needed).
		// npm start, yarn start, pnpm start, flask run
		const directServiceSubcommands: Record<string, Set<string>> = {
			npm: new Set(["start"]),
			yarn: new Set(["start"]),
			pnpm: new Set(["start"]),
			flask: new Set(["run"]),
		};
		if (directServiceSubcommands[executable]?.has(subcommand)) {
			return { surfaceKind: "backend_service", confidence: 0.85 };
		}

		// Direct CLI subcommands (no script name needed).
		const directCliSubcommands: Record<string, Set<string>> = {
			npm: new Set(["test", "install", "build", "lint", "ci"]),
			yarn: new Set(["test", "install", "build", "lint"]),
			pnpm: new Set(["test", "install", "build", "lint"]),
		};
		if (directCliSubcommands[executable]?.has(subcommand)) {
			return { surfaceKind: "cli", confidence: 0.80 };
		}

		// "run" subcommand requires inspecting the script name (third token).
		// npm run <script>, yarn run <script>, pnpm run <script>, bun run <script>
		const runSubcommandPMs = new Set(["npm", "yarn", "pnpm", "bun"]);
		if (runSubcommandPMs.has(executable) && subcommand === "run" && tokens.length >= 3) {
			const scriptName = tokens[2];
			// Service-like script names.
			const serviceScripts = new Set(["start", "serve", "server", "dev", "develop", "watch"]);
			if (serviceScripts.has(scriptName)) {
				return { surfaceKind: "backend_service", confidence: 0.85 };
			}
			// CLI-like script names.
			const cliScripts = new Set(["build", "test", "lint", "check", "typecheck", "format", "migrate", "ci"]);
			if (cliScripts.has(scriptName)) {
				return { surfaceKind: "cli", confidence: 0.80 };
			}
			// Unknown script — ambiguous, fall through to later heuristics.
		}

		// python -m <module> requires inspecting the module name (third token).
		if (executable === "python" && subcommand === "-m" && tokens.length >= 3) {
			const moduleName = tokens[2];
			// Service-like modules.
			const serviceModules = new Set(["http.server", "flask", "uvicorn", "gunicorn", "hypercorn"]);
			if (serviceModules.has(moduleName)) {
				return { surfaceKind: "backend_service", confidence: 0.85 };
			}
			// CLI-like modules.
			const cliModules = new Set(["pytest", "unittest", "pip", "build", "mypy", "black", "ruff", "pylint"]);
			if (cliModules.has(moduleName)) {
				return { surfaceKind: "cli", confidence: 0.80 };
			}
			// Unknown module — ambiguous, fall through.
		}
	}

	// Service-indicating subcommands at any position (exact token match).
	const serviceSubcommands = new Set(["serve", "server", "start", "daemon", "listen", "worker"]);
	for (const token of tokens.slice(1)) {
		if (serviceSubcommands.has(token)) {
			return { surfaceKind: "backend_service", confidence: 0.85 };
		}
	}

	// CLI-indicating subcommands at any position (exact token match).
	const cliSubcommands = new Set(["build", "test", "migrate", "init", "setup", "install", "compile", "lint", "check"]);
	for (const token of tokens.slice(1)) {
		if (cliSubcommands.has(token)) {
			return { surfaceKind: "cli", confidence: 0.80 };
		}
	}

	// Unclear pattern — default to backend_service.
	return { surfaceKind: "backend_service", confidence: 0.70 };
}

/**
 * Extract basename from a path (strips directory component).
 * "/usr/local/bin/node" → "node"
 * "python3" → "python3"
 */
function getBasename(path: string): string {
	const lastSlash = path.lastIndexOf("/");
	return lastSlash >= 0 ? path.slice(lastSlash + 1) : path;
}

/**
 * Infer base runtime from Docker image name.
 *
 * Maps common base images to RuntimeKind values.
 * Returns "unknown" for unrecognized images.
 */
function inferBaseRuntime(baseImage: string): RuntimeKind {
	const image = baseImage.toLowerCase();

	// Node.js variants.
	if (image.startsWith("node:") || image.includes("/node:") ||
		image === "node" || image.startsWith("node-")) {
		return "node";
	}

	// Python variants.
	if (image.startsWith("python:") || image.includes("/python:") ||
		image === "python" || image.startsWith("python-")) {
		return "python";
	}

	// Rust variants.
	if (image.startsWith("rust:") || image.includes("/rust:") ||
		image === "rust" || image.startsWith("rust-")) {
		return "rust_native";
	}

	// JVM variants (Java, Kotlin, etc.).
	if (image.startsWith("openjdk:") || image.startsWith("eclipse-temurin:") ||
		image.startsWith("amazoncorretto:") || image.startsWith("adoptopenjdk:") ||
		image.includes("/openjdk:") || image.includes("java")) {
		return "jvm";
	}

	// Bun runtime.
	if (image.startsWith("oven/bun:") || image === "oven/bun" ||
		image.startsWith("bun:")) {
		return "bun";
	}

	// Deno runtime.
	if (image.startsWith("denoland/deno:") || image === "denoland/deno" ||
		image.startsWith("deno:")) {
		return "deno";
	}

	// Generic Linux bases — unknown runtime.
	// alpine, ubuntu, debian, centos, fedora, etc.
	return "unknown";
}

// ── Docker Compose detector ────────────────────────────────────────

/**
 * Input DTO for a compose service.
 * Core defines this interface; adapter layer provides the data.
 */
export interface ComposeServiceInput {
	/** Service name (key in the services map). */
	name: string;
	/** Image to use (if image-only service). */
	image: string | null;
	/** Build context path (relative to compose file, null if image-only). */
	buildContext: string | null;
	/** Dockerfile path (relative to build context). */
	dockerfile: string | null;
	/** Container name override (if specified). */
	containerName: string | null;
	/** Command override (if specified). */
	command: string[] | null;
	/** Entrypoint override (if specified). */
	entrypoint: string[] | null;
	/** Ports exposed. */
	ports: string[];
	/** Environment variable names (not values). */
	envVars: string[];
	/** Profiles this service belongs to. */
	profiles: string[];
	/** Dependencies (depends_on). */
	dependsOn: string[];
}

/**
 * Input DTO for a compose file.
 */
export interface ComposeFileInput {
	/** Repo-relative path to the compose file. */
	filePath: string;
	/** Services in the compose file. */
	services: ComposeServiceInput[];
}

/**
 * Detect project surfaces from a docker-compose file.
 *
 * Each service becomes one surface:
 *   - sourceType = "docker_compose"
 *   - runtimeKind = "container"
 *   - serviceName required for identity
 *
 * Build services (with build context):
 *   - rootPath = build context resolved relative to compose file
 *   - Higher confidence (0.85)
 *   - Skipped if build context escapes repo root (external)
 *
 * Image-only services:
 *   - rootPath = compose file directory
 *   - Lower confidence (0.70)
 */
export function detectComposeSurfaces(
	input: ComposeFileInput,
): DetectedSurface[] {
	const surfaces: DetectedSurface[] = [];
	const composeDir = input.filePath.includes("/")
		? input.filePath.slice(0, input.filePath.lastIndexOf("/"))
		: ".";

	for (const service of input.services) {
		const hasBuild = service.buildContext !== null;
		const hasImage = service.image !== null;

		// Compute rootPath: build context or compose file directory.
		let rootPath: string;
		if (hasBuild && service.buildContext) {
			const resolved = resolvePath(composeDir, service.buildContext);
			if (resolved === null) {
				// Build context escapes repo root — skip this service.
				// We cannot analyze code outside the repo, and attaching
				// to compose directory would create false module ownership.
				continue;
			}
			rootPath = resolved;
		} else {
			rootPath = composeDir;
		}

		// Compute dockerfilePath for evidence (if build specifies dockerfile).
		let dockerfilePath: string | null = null;
		if (hasBuild) {
			if (service.dockerfile) {
				dockerfilePath = resolvePath(rootPath, service.dockerfile);
			} else {
				// Default Dockerfile in build context.
				dockerfilePath = resolvePath(rootPath, "Dockerfile");
			}
		}

		// Infer surface kind from command/entrypoint.
		const { surfaceKind, confidence: cmdConfidence } = inferDockerfileSurfaceKind(
			service.entrypoint,
			service.command,
		);

		// Base confidence: build services higher than image-only.
		const baseConfidence = hasBuild ? 0.85 : 0.70;
		// If command/entrypoint gave a strong signal, use it; otherwise use base.
		const confidence = cmdConfidence > 0.70 ? cmdConfidence : baseConfidence;

		// Infer base runtime from image.
		const baseRuntimeKind = hasImage ? inferBaseRuntime(service.image!) : "unknown";

		surfaces.push({
			surfaceKind,
			displayName: service.containerName ?? service.name,
			rootPath,
			entrypointPath: null,
			buildSystem: "unknown",
			runtimeKind: "container",
			confidence,
			evidence: [{
				sourceType: "docker_compose",
				sourcePath: input.filePath,
				evidenceKind: "container_config",
				confidence,
				payload: {
					serviceName: service.name,
					containerName: service.containerName,
					image: service.image,
					buildContext: service.buildContext,
					dockerfile: service.dockerfile,
					ports: service.ports,
					envVars: service.envVars,
					profiles: service.profiles,
					dependsOn: service.dependsOn,
					baseRuntimeKind,
				},
			}],
			metadata: {
				serviceName: service.name,
				containerName: service.containerName,
				image: service.image,
				buildContext: service.buildContext,
				dockerfilePath,
				ports: service.ports,
				envVars: service.envVars,
				profiles: service.profiles,
				dependsOn: service.dependsOn,
				baseRuntimeKind,
			},
			// Identity fields
			sourceType: "docker_compose",
			serviceName: service.name,
			dockerfilePath: dockerfilePath ?? undefined,
		});
	}

	return surfaces;
}

/**
 * Resolve a relative path against a base directory.
 * "services/api" + "../shared" → "services/shared"
 * "." + "./app" → "app"
 *
 * Returns null if the path escapes the repo root (too many ".." components).
 * This prevents mapping external paths back onto repo root incorrectly.
 */
function resolvePath(base: string, relative: string): string | null {
	// Normalize "./" prefix.
	let rel = relative.startsWith("./") ? relative.slice(2) : relative;

	// Handle "." base — any ".." escapes the repo.
	if (base === ".") {
		if (rel.startsWith("..")) {
			return null; // Escapes repo root.
		}
		return rel || ".";
	}

	// Simple case: no .. navigation.
	if (!rel.startsWith("..")) {
		return `${base}/${rel}`.replace(/\/+/g, "/").replace(/\/$/, "");
	}

	// Handle .. navigation with escape detection.
	const baseParts = base.split("/").filter((p) => p && p !== ".");
	const relParts = rel.split("/").filter((p) => p);

	for (const part of relParts) {
		if (part === "..") {
			if (baseParts.length === 0) {
				// Would escape repo root.
				return null;
			}
			baseParts.pop();
		} else if (part !== ".") {
			baseParts.push(part);
		}
	}

	return baseParts.length > 0 ? baseParts.join("/") : ".";
}
