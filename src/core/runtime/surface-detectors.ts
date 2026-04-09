/**
 * Project surface detectors — pure functions.
 *
 * Take parsed manifest content and return detected project surfaces
 * with evidence. No file I/O.
 *
 * Slice 1 detectors:
 *   - package.json: bin → cli, main/exports → library, scripts → build system
 *   - package.json: framework deps → web_app/backend_service signals
 *   - Cargo.toml: [[bin]] → cli, [lib] → library
 *   - pyproject.toml: [project.scripts] → cli
 *
 * NOT in slice 1:
 *   - Dockerfile/docker-compose → container/service detection
 *   - Terraform/Pulumi/Helm → infra_root
 *   - Gradle application plugin
 *   - Worker/queue detection (requires deeper analysis)
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
		});
	}

	// 3. Framework-based surface detection.
	const frameworkSurface = detectFrameworkFromDeps(allDeps, dir, manifestRelPath, buildSystem);
	if (frameworkSurface) {
		surfaces.push(frameworkSurface);
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
				sourceType: hasBinSection ? "cargo_bin_target" : "cargo_bin_target",
				sourcePath: manifestRelPath,
				evidenceKind: "binary_entrypoint",
				confidence: hasBinSection ? 0.95 : 0.80,
				payload: { binName, binPath, convention: hasBinSection ? "explicit_bin" : "src_main_rs" },
			}],
			metadata: null,
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
			};
		}
	}

	return null;
}
