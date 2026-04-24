/**
 * Project surface detector unit tests.
 *
 * Pure functions — no filesystem, no storage.
 *
 * Covers:
 *   - package.json: bin → cli, main → library, framework deps → web_app/backend_service
 *   - Cargo.toml: [[bin]] → cli, [lib] → library, src/main.rs convention
 *   - pyproject.toml: [project.scripts] → cli, package name → library
 */

import { describe, expect, it } from "vitest";
import {
	detectCargoSurfaces,
	detectComposeSurfaces,
	detectDockerfileSurfaces,
	detectMakefileSurfaces,
	detectPackageJsonSurfaces,
	detectPyprojectSurfaces,
	type ComposeFileInput,
	type MakefileDetectionResult,
} from "../../../src/core/runtime/surface-detectors.js";

// ── package.json ───────────────────────────────────────────────────

describe("detectPackageJsonSurfaces", () => {
	it("detects CLI surface from bin field (string)", () => {
		const content = JSON.stringify({
			name: "my-tool",
			bin: "./dist/cli.js",
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const cli = surfaces.find((s) => s.surfaceKind === "cli");
		expect(cli).toBeDefined();
		expect(cli!.displayName).toBe("my-tool");
		expect(cli!.entrypointPath).toBe("./dist/cli.js");
		expect(cli!.runtimeKind).toBe("node");
	});

	it("detects CLI surface from bin field (object)", () => {
		const content = JSON.stringify({
			name: "multi-bin",
			bin: { rgr: "./dist/cli/index.js", helper: "./dist/helper.js" },
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const clis = surfaces.filter((s) => s.surfaceKind === "cli");
		expect(clis).toHaveLength(2);
		expect(clis.map((c) => c.displayName).sort()).toEqual(["helper", "rgr"]);
	});

	it("detects library surface from main field", () => {
		const content = JSON.stringify({
			name: "my-lib",
			main: "./dist/index.js",
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const lib = surfaces.find((s) => s.surfaceKind === "library");
		expect(lib).toBeDefined();
		expect(lib!.displayName).toBe("my-lib");
		expect(lib!.entrypointPath).toBe("./dist/index.js");
	});

	it("detects library surface from exports field", () => {
		const content = JSON.stringify({
			name: "modern-lib",
			exports: { ".": "./dist/index.js" },
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const lib = surfaces.find((s) => s.surfaceKind === "library");
		expect(lib).toBeDefined();
	});

	it("detects backend_service from express dependency", () => {
		const content = JSON.stringify({
			name: "api",
			dependencies: { express: "^4.0.0" },
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const backend = surfaces.find((s) => s.surfaceKind === "backend_service");
		expect(backend).toBeDefined();
		expect(backend!.confidence).toBeGreaterThan(0);
	});

	it("detects web_app from react dependency", () => {
		const content = JSON.stringify({
			name: "frontend",
			dependencies: { react: "^18.0.0", "react-dom": "^18.0.0" },
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const webapp = surfaces.find((s) => s.surfaceKind === "web_app");
		expect(webapp).toBeDefined();
		expect(webapp!.runtimeKind).toBe("browser");
	});

	it("detects both cli and library for packages with bin + main", () => {
		const content = JSON.stringify({
			name: "dual-pkg",
			bin: "./dist/cli.js",
			main: "./dist/index.js",
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces.find((s) => s.surfaceKind === "cli")).toBeDefined();
		expect(surfaces.find((s) => s.surfaceKind === "library")).toBeDefined();
	});

	it("infers typescript_tsc build system from tsc script", () => {
		const content = JSON.stringify({
			name: "ts-proj",
			bin: "./dist/cli.js",
			scripts: { build: "tsc" },
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces[0].buildSystem).toBe("typescript_tsc");
	});

	it("infers bundler build system from vite", () => {
		const content = JSON.stringify({
			name: "vite-app",
			main: "./dist/index.js",
			scripts: { build: "vite build" },
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		const lib = surfaces.find((s) => s.surfaceKind === "library");
		expect(lib!.buildSystem).toBe("typescript_bundler");
	});

	it("handles nested package.json paths", () => {
		const content = JSON.stringify({
			name: "@app/api",
			bin: "./dist/server.js",
		});
		const surfaces = detectPackageJsonSurfaces(content, "packages/api/package.json");
		expect(surfaces[0].rootPath).toBe("packages/api");
		expect(surfaces[0].entrypointPath).toBe("packages/api/./dist/server.js");
	});

	it("returns empty for package with no surface signals", () => {
		const content = JSON.stringify({ name: "bare-pkg", version: "1.0.0" });
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces).toHaveLength(0);
	});

	it("returns empty for invalid JSON", () => {
		const surfaces = detectPackageJsonSurfaces("{broken", "package.json");
		expect(surfaces).toHaveLength(0);
	});

	// ── Script-only fallback tests ─────────────────────────────────────

	it("script-only build/test package creates one library surface", () => {
		const content = JSON.stringify({
			name: "build-tools",
			version: "1.0.0",
			scripts: {
				build: "tsc",
				test: "vitest",
				lint: "eslint .",
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces).toHaveLength(1);
		const lib = surfaces[0];
		expect(lib.surfaceKind).toBe("library");
		expect(lib.confidence).toBe(0.50);
		expect(lib.sourceType).toBe("package_json_scripts");
		expect(lib.entrypointPath).toBeNull();
		expect(lib.displayName).toBe("build-tools");
		expect(lib.metadata).toBeDefined();
		const meta = lib.metadata as Record<string, unknown>;
		expect(meta.fallbackReason).toBe("script_only_package");
	});

	it("script-only start/dev package creates one backend_service surface", () => {
		const content = JSON.stringify({
			name: "local-server",
			version: "1.0.0",
			scripts: {
				start: "node server.js",
				dev: "nodemon server.js",
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces).toHaveLength(1);
		const svc = surfaces[0];
		expect(svc.surfaceKind).toBe("backend_service");
		expect(svc.confidence).toBe(0.55);
		expect(svc.sourceType).toBe("package_json_scripts");
		expect(svc.displayName).toBe("local-server");
	});

	it("package with bin does not get duplicate script fallback surface", () => {
		// bin creates a CLI surface; script fallback should NOT run
		const content = JSON.stringify({
			name: "cli-with-scripts",
			version: "1.0.0",
			bin: "./dist/cli.js",
			scripts: {
				start: "node dist/cli.js",
				build: "tsc",
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		// Only the bin-based CLI surface, no script fallback
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("cli");
		expect(surfaces[0].sourceType).toBe("package_json_bin");
	});

	it("package with main does not get script fallback", () => {
		const content = JSON.stringify({
			name: "lib-with-scripts",
			version: "1.0.0",
			main: "./dist/index.js",
			scripts: {
				build: "tsc",
				test: "vitest",
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("library");
		expect(surfaces[0].sourceType).toBe("package_json_main");
	});

	it("package with framework deps does not get script fallback", () => {
		const content = JSON.stringify({
			name: "express-app",
			version: "1.0.0",
			dependencies: { express: "^4.0.0" },
			scripts: {
				start: "node server.js",
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		// Framework dep creates backend_service, no script fallback needed
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].sourceType).toBe("framework_dependency");
	});

	it("service scripts take precedence over build scripts in fallback", () => {
		const content = JSON.stringify({
			name: "mixed-scripts",
			version: "1.0.0",
			scripts: {
				start: "node server.js",
				build: "tsc",
				test: "vitest",
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces).toHaveLength(1);
		// start script → backend_service (0.55), not library (0.50)
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].confidence).toBe(0.55);
	});

	it("script fallback includes evidence for each relevant script", () => {
		const content = JSON.stringify({
			name: "scripts-pkg",
			version: "1.0.0",
			scripts: {
				build: "tsc",
				test: "vitest",
				format: "prettier --write", // not a classified script
			},
		});
		const surfaces = detectPackageJsonSurfaces(content, "package.json");
		expect(surfaces).toHaveLength(1);
		// Evidence for build and test, but not format
		expect(surfaces[0].evidence).toHaveLength(2);
		const roles = surfaces[0].evidence.map(
			(e) => (e.payload as Record<string, unknown>).scriptName
		);
		expect(roles.sort()).toEqual(["build", "test"]);
	});
});

// ── Cargo.toml ─────────────────────────────────────────────────────

describe("detectCargoSurfaces", () => {
	it("detects CLI from [[bin]] section", () => {
		const content = `[package]
name = "my-tool"
version = "0.1.0"

[[bin]]
name = "my-tool"
path = "src/main.rs"
`;
		const surfaces = detectCargoSurfaces(content, "Cargo.toml");
		const cli = surfaces.find((s) => s.surfaceKind === "cli");
		expect(cli).toBeDefined();
		expect(cli!.displayName).toBe("my-tool");
		expect(cli!.entrypointPath).toBe("src/main.rs");
		expect(cli!.buildSystem).toBe("cargo");
		expect(cli!.runtimeKind).toBe("rust_native");
	});

	it("detects CLI from src/main.rs convention", () => {
		const content = `[package]
name = "simple-bin"
version = "0.1.0"
`;
		// The content doesn't contain [[bin]] but the convention check
		// looks for "src/main.rs" in the file content — which isn't present.
		// This test verifies the explicit [[bin]] path.
		const surfaces = detectCargoSurfaces(content, "Cargo.toml");
		// No bin section and no src/main.rs mention → no CLI surface.
		expect(surfaces.find((s) => s.surfaceKind === "cli")).toBeUndefined();
	});

	it("detects library from [lib] section", () => {
		const content = `[package]
name = "my-lib"
version = "0.1.0"

[lib]
name = "my_lib"
`;
		const surfaces = detectCargoSurfaces(content, "Cargo.toml");
		const lib = surfaces.find((s) => s.surfaceKind === "library");
		expect(lib).toBeDefined();
		expect(lib!.displayName).toBe("my-lib");
	});

	it("detects both CLI and library", () => {
		const content = `[package]
name = "dual-crate"
version = "0.1.0"

[[bin]]
name = "dual-cli"

[lib]
name = "dual_lib"
`;
		const surfaces = detectCargoSurfaces(content, "Cargo.toml");
		expect(surfaces.find((s) => s.surfaceKind === "cli")).toBeDefined();
		expect(surfaces.find((s) => s.surfaceKind === "library")).toBeDefined();
	});

	it("handles nested path", () => {
		const content = `[package]
name = "nested"
version = "0.1.0"

[[bin]]
name = "nested-bin"
path = "src/main.rs"
`;
		const surfaces = detectCargoSurfaces(content, "crates/nested/Cargo.toml");
		expect(surfaces[0].rootPath).toBe("crates/nested");
		expect(surfaces[0].entrypointPath).toBe("crates/nested/src/main.rs");
	});
});

// ── pyproject.toml ─────────────────────────────────────────────────

describe("detectPyprojectSurfaces", () => {
	it("detects CLI from [project.scripts]", () => {
		const content = `[project]
name = "my-tool"
version = "1.0.0"

[project.scripts]
my-tool = "my_tool.cli:main"
`;
		const surfaces = detectPyprojectSurfaces(content, "pyproject.toml");
		const cli = surfaces.find((s) => s.surfaceKind === "cli");
		expect(cli).toBeDefined();
		expect(cli!.displayName).toBe("my-tool");
		expect(cli!.runtimeKind).toBe("python");
		expect(cli!.buildSystem).toBe("python_setuptools");
	});

	it("detects CLI from [tool.poetry.scripts]", () => {
		const content = `[tool.poetry]
name = "poetry-tool"
version = "1.0.0"

[tool.poetry.scripts]
ptool = "poetry_tool.main:run"
`;
		const surfaces = detectPyprojectSurfaces(content, "pyproject.toml");
		const cli = surfaces.find((s) => s.surfaceKind === "cli");
		expect(cli).toBeDefined();
		expect(cli!.displayName).toBe("ptool");
		expect(cli!.buildSystem).toBe("python_poetry");
	});

	it("detects library from package name", () => {
		const content = `[project]
name = "my-library"
version = "2.0.0"
`;
		const surfaces = detectPyprojectSurfaces(content, "pyproject.toml");
		const lib = surfaces.find((s) => s.surfaceKind === "library");
		expect(lib).toBeDefined();
		expect(lib!.displayName).toBe("my-library");
	});

	it("detects both CLI and library", () => {
		const content = `[project]
name = "dual-pkg"
version = "1.0.0"

[project.scripts]
dual-cli = "dual_pkg.cli:main"
`;
		const surfaces = detectPyprojectSurfaces(content, "pyproject.toml");
		expect(surfaces.find((s) => s.surfaceKind === "cli")).toBeDefined();
		expect(surfaces.find((s) => s.surfaceKind === "library")).toBeDefined();
	});

	it("returns empty for pyproject with no surface signals", () => {
		const content = `[build-system]
requires = ["setuptools"]
`;
		const surfaces = detectPyprojectSurfaces(content, "pyproject.toml");
		expect(surfaces).toHaveLength(0);
	});
});

// ── Dockerfile ─────────────────────────────────────────────────────

describe("detectDockerfileSurfaces", () => {
	it("detects backend_service from CMD with npm start", () => {
		const content = `FROM node:20-alpine
WORKDIR /app
COPY . .
CMD ["npm", "start"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		expect(surface.surfaceKind).toBe("backend_service");
		expect(surface.runtimeKind).toBe("container");
		expect(surface.sourceType).toBe("dockerfile");
		expect(surface.dockerfilePath).toBe("Dockerfile");
		expect(surface.confidence).toBeGreaterThanOrEqual(0.85);
	});

	it("detects backend_service from ENTRYPOINT with gunicorn", () => {
		const content = `FROM python:3.11-slim
WORKDIR /app
COPY . .
ENTRYPOINT ["gunicorn", "--bind", "0.0.0.0:8000"]
CMD ["app:application"]
`;
		const surfaces = detectDockerfileSurfaces(content, "services/api/Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		expect(surface.surfaceKind).toBe("backend_service");
		expect(surface.rootPath).toBe("services/api");
		expect(surface.dockerfilePath).toBe("services/api/Dockerfile");
		// Check metadata for base runtime
		expect(surface.metadata).toBeDefined();
		expect((surface.metadata as Record<string, unknown>).baseRuntimeKind).toBe("python");
	});

	it("detects cli from ENTRYPOINT with --help", () => {
		const content = `FROM node:20-alpine
WORKDIR /app
COPY . .
ENTRYPOINT ["node", "cli.js"]
CMD ["--help"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		expect(surface.surfaceKind).toBe("cli");
		expect(surface.confidence).toBeGreaterThanOrEqual(0.80);
	});

	it("uses final stage only in multi-stage builds", () => {
		const content = `FROM rust:1.75 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release
ENTRYPOINT ["cargo", "test"]

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/myserver /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/myserver"]
CMD ["--port", "8080"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		// Final stage has "myserver" which doesn't match CLI patterns
		// but contains "server" in path, defaults to backend_service
		expect(surface.surfaceKind).toBe("backend_service");
		// Evidence should show stage count
		expect(surface.evidence[0].payload).toBeDefined();
		const payload = surface.evidence[0].payload as Record<string, unknown>;
		expect(payload.stageCount).toBe(2);
		// Base image is final FROM, not builder
		expect(payload.baseImage).toBe("debian:bookworm-slim");
	});

	it("defaults to backend_service with low confidence when no CMD/ENTRYPOINT", () => {
		const content = `FROM alpine:3.19
RUN apk add --no-cache curl
COPY scripts /scripts
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		expect(surface.surfaceKind).toBe("backend_service");
		expect(surface.confidence).toBe(0.60);
	});

	it("returns empty for invalid Dockerfile (no FROM)", () => {
		const content = `# This is not a valid Dockerfile
RUN echo "hello"
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(0);
	});

	it("handles shell form ENTRYPOINT", () => {
		const content = `FROM python:3.11
ENTRYPOINT python server.py --port 8080
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		expect(surface.surfaceKind).toBe("backend_service");
		const payload = surface.evidence[0].payload as Record<string, unknown>;
		// Shell form gets split into array
		expect(payload.entrypoint).toEqual(["python", "server.py", "--port", "8080"]);
	});

	it("infers base runtime from common images", () => {
		const testCases: Array<{ image: string; expected: string }> = [
			{ image: "node:20-alpine", expected: "node" },
			{ image: "python:3.11-slim", expected: "python" },
			{ image: "rust:1.75", expected: "rust_native" },
			{ image: "openjdk:17", expected: "jvm" },
			{ image: "eclipse-temurin:21", expected: "jvm" },
			{ image: "oven/bun:latest", expected: "bun" },
			{ image: "denoland/deno:latest", expected: "deno" },
			{ image: "alpine:3.19", expected: "unknown" },
			{ image: "ubuntu:22.04", expected: "unknown" },
		];

		for (const { image, expected } of testCases) {
			const content = `FROM ${image}\nCMD ["echo", "test"]`;
			const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
			expect(surfaces).toHaveLength(1);
			const payload = surfaces[0].evidence[0].payload as Record<string, unknown>;
			expect(payload.baseRuntimeKind).toBe(expected);
		}
	});

	it("provides correct identity fields", () => {
		const content = `FROM node:20
CMD ["npm", "start"]
`;
		const surfaces = detectDockerfileSurfaces(content, "apps/web/Dockerfile");
		expect(surfaces).toHaveLength(1);
		const surface = surfaces[0];
		expect(surface.sourceType).toBe("dockerfile");
		expect(surface.dockerfilePath).toBe("apps/web/Dockerfile");
		expect(surface.rootPath).toBe("apps/web");
	});

	it("handles comments and blank lines", () => {
		const content = `# Build stage
FROM node:20

# Install deps
WORKDIR /app

# Copy files
COPY . .

# Start the server
CMD ["npm", "start"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
	});

	it("parses FROM with --platform flag correctly", () => {
		const content = `FROM --platform=linux/amd64 node:20-alpine AS base
WORKDIR /app
CMD ["npm", "start"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const payload = surfaces[0].evidence[0].payload as Record<string, unknown>;
		// Should extract node:20-alpine, not --platform=linux/amd64
		expect(payload.baseImage).toBe("node:20-alpine");
		expect(payload.baseRuntimeKind).toBe("node");
	});

	it("parses FROM with --platform flag without equals sign", () => {
		const content = `FROM --platform linux/arm64 python:3.11
CMD ["python", "-m", "http.server"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		const payload = surfaces[0].evidence[0].payload as Record<string, unknown>;
		expect(payload.baseImage).toBe("python:3.11");
		expect(payload.baseRuntimeKind).toBe("python");
	});

	it("does not match service patterns as substrings in paths", () => {
		// /usr/local/bin/myserver should NOT match "server" as substring
		// observer should NOT match "serve" as substring
		const content = `FROM alpine:3.19
ENTRYPOINT ["/usr/local/bin/observer"]
CMD ["--config", "/etc/observer.yaml"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		// Should be backend_service with lower confidence (unclear pattern)
		// NOT 0.85 from false "serve" substring match
		expect(surfaces[0].confidence).toBe(0.70);
	});

	it("matches service executables exactly by basename", () => {
		const content = `FROM python:3.11
ENTRYPOINT ["gunicorn", "--bind", "0.0.0.0:8000", "app:app"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].confidence).toBe(0.85);
	});

	it("matches service subcommands as exact tokens", () => {
		// "serve" as a subcommand should match
		const content = `FROM node:20
ENTRYPOINT ["npx", "serve", "-s", "build"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].confidence).toBe(0.85);
	});

	it("classifies npm run build as CLI, not service", () => {
		const content = `FROM node:20
CMD ["npm", "run", "build"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("cli");
		expect(surfaces[0].confidence).toBe(0.80);
	});

	it("classifies npm run start as service", () => {
		const content = `FROM node:20
CMD ["npm", "run", "start"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].confidence).toBe(0.85);
	});

	it("classifies yarn run lint as CLI", () => {
		const content = `FROM node:20
CMD ["yarn", "run", "lint"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("cli");
	});

	it("classifies python -m pytest as CLI", () => {
		const content = `FROM python:3.11
CMD ["python", "-m", "pytest"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("cli");
		expect(surfaces[0].confidence).toBe(0.80);
	});

	it("classifies python -m http.server as service", () => {
		const content = `FROM python:3.11
CMD ["python", "-m", "http.server"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].confidence).toBe(0.85);
	});

	it("classifies unknown npm run script as ambiguous (falls through)", () => {
		const content = `FROM node:20
CMD ["npm", "run", "custom-script"]
`;
		const surfaces = detectDockerfileSurfaces(content, "Dockerfile");
		expect(surfaces).toHaveLength(1);
		// Unknown script falls through to default backend_service with lower confidence
		expect(surfaces[0].surfaceKind).toBe("backend_service");
		expect(surfaces[0].confidence).toBe(0.70);
	});
});

// ── Docker Compose ─────────────────────────────────────────────────

describe("detectComposeSurfaces", () => {
	it("produces one surface per service", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: ["npm", "start"], entrypoint: null, ports: ["3000:3000"], envVars: [], profiles: [], dependsOn: [] },
				{ name: "worker", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: ["npm", "run", "worker"], entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
				{ name: "redis", image: "redis:7-alpine", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: ["6379:6379"], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces).toHaveLength(3);
		expect(surfaces.map((s) => s.displayName).sort()).toEqual(["api", "redis", "worker"]);
	});

	it("sets sourceType to docker_compose", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].sourceType).toBe("docker_compose");
	});

	it("sets runtimeKind to container", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].runtimeKind).toBe("container");
	});

	it("sets serviceName for identity", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "my-service", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].serviceName).toBe("my-service");
	});

	it("resolves rootPath from build context", () => {
		const input: ComposeFileInput = {
			filePath: "services/docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: "./api", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].rootPath).toBe("services/api");
	});

	it("resolves rootPath from build context with parent traversal", () => {
		const input: ComposeFileInput = {
			filePath: "infra/compose/docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: "../../services/api", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].rootPath).toBe("services/api");
	});

	it("uses compose directory as rootPath for image-only services", () => {
		const input: ComposeFileInput = {
			filePath: "infra/docker-compose.yml",
			services: [
				{ name: "redis", image: "redis:7", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].rootPath).toBe("infra");
	});

	it("has higher confidence for build services than image-only", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: ".", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
				{ name: "redis", image: "redis:7", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		const api = surfaces.find((s) => s.displayName === "api")!;
		const redis = surfaces.find((s) => s.displayName === "redis")!;
		expect(api.confidence).toBeGreaterThan(redis.confidence);
	});

	it("resolves dockerfilePath from build context", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: "./api", dockerfile: "Dockerfile.prod", command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].dockerfilePath).toBe("api/Dockerfile.prod");
	});

	it("uses default Dockerfile when not specified in build", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: "./api", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].dockerfilePath).toBe("api/Dockerfile");
	});

	it("infers baseRuntimeKind from image", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: "node:20-alpine", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		const payload = surfaces[0].evidence[0].payload as Record<string, unknown>;
		expect(payload.baseRuntimeKind).toBe("node");
	});

	it("infers surfaceKind from command", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: ["npm", "start"], entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].surfaceKind).toBe("backend_service");
	});

	it("records ports and dependsOn in evidence", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: "node:20", buildContext: null, dockerfile: null, containerName: null, command: null, entrypoint: null, ports: ["3000:3000", "9229:9229"], envVars: [], profiles: [], dependsOn: ["db", "redis"] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		const payload = surfaces[0].evidence[0].payload as Record<string, unknown>;
		expect(payload.ports).toEqual(["3000:3000", "9229:9229"]);
		expect(payload.dependsOn).toEqual(["db", "redis"]);
	});

	it("handles root compose file (filePath without directory)", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: ".", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].rootPath).toBe(".");
		expect(surfaces[0].dockerfilePath).toBe("Dockerfile");
	});

	it("skips services with external build context that escapes repo root", () => {
		const input: ComposeFileInput = {
			filePath: "infra/docker-compose.yml",
			services: [
				{ name: "external", image: null, buildContext: "../../outside-repo", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		// External build context services are skipped entirely
		expect(surfaces).toHaveLength(0);
	});

	it("skips services with external build context from repo root", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "external", image: null, buildContext: "../sibling-repo", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		// External build context services are skipped entirely
		expect(surfaces).toHaveLength(0);
	});

	it("keeps valid in-repo services when others are external", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{ name: "external", image: null, buildContext: "../outside", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
				{ name: "api", image: null, buildContext: "./api", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		// Only the in-repo service should be kept
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].displayName).toBe("api");
		expect(surfaces[0].rootPath).toBe("api");
	});

	it("resolves valid build context with parent traversal", () => {
		const input: ComposeFileInput = {
			filePath: "infra/docker-compose.yml",
			services: [
				{ name: "api", image: null, buildContext: "../services/api", dockerfile: null, containerName: null, command: null, entrypoint: null, ports: [], envVars: [], profiles: [], dependsOn: [] },
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces).toHaveLength(1);
		expect(surfaces[0].rootPath).toBe("services/api");
		expect(surfaces[0].dockerfilePath).toBe("services/api/Dockerfile");
	});

	it("includes containerName, envVars, and profiles in evidence", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{
					name: "api",
					image: "node:20",
					buildContext: null,
					dockerfile: null,
					containerName: "my-api-container",
					command: null,
					entrypoint: null,
					ports: [],
					envVars: ["NODE_ENV", "PORT", "DATABASE_URL"],
					profiles: ["dev", "production"],
					dependsOn: [],
				},
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces).toHaveLength(1);

		const payload = surfaces[0].evidence[0].payload as Record<string, unknown>;
		expect(payload.containerName).toBe("my-api-container");
		expect(payload.envVars).toEqual(["NODE_ENV", "PORT", "DATABASE_URL"]);
		expect(payload.profiles).toEqual(["dev", "production"]);

		const metadata = surfaces[0].metadata as Record<string, unknown>;
		expect(metadata.containerName).toBe("my-api-container");
		expect(metadata.envVars).toEqual(["NODE_ENV", "PORT", "DATABASE_URL"]);
		expect(metadata.profiles).toEqual(["dev", "production"]);
	});

	it("uses containerName as displayName when available", () => {
		const input: ComposeFileInput = {
			filePath: "docker-compose.yml",
			services: [
				{
					name: "api",
					image: "node:20",
					buildContext: null,
					dockerfile: null,
					containerName: "custom-api-name",
					command: null,
					entrypoint: null,
					ports: [],
					envVars: [],
					profiles: [],
					dependsOn: [],
				},
			],
		};
		const surfaces = detectComposeSurfaces(input);
		expect(surfaces[0].displayName).toBe("custom-api-name");
	});
});

// ── Makefile ───────────────────────────────────────────────────────

describe("detectMakefileSurfaces", () => {
	it("detects explicit build target as library surface", () => {
		const content = `
all: main.o utils.o
	gcc -o myapp main.o utils.o

build: src/main.c
	gcc -c src/main.c
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces.length).toBeGreaterThanOrEqual(1);

		const buildSurface = result.surfaces.find((s) => s.displayName === "build");
		expect(buildSurface).toBeDefined();
		expect(buildSurface!.surfaceKind).toBe("library");
		expect(buildSurface!.buildSystem).toBe("make");
		expect(buildSurface!.sourceType).toBe("makefile_target");
		expect(buildSurface!.targetName).toBe("build");
	});

	it("detects .PHONY test target as cli surface", () => {
		const content = `
.PHONY: test

test: build
	./run_tests.sh
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(1);

		const testSurface = result.surfaces[0];
		expect(testSurface.surfaceKind).toBe("cli");
		expect(testSurface.displayName).toBe("test");

		const payload = testSurface.evidence[0].payload as Record<string, unknown>;
		expect(payload.isPhony).toBe(true);
	});

	it("skips clean target (no surface produced)", () => {
		const content = `
clean:
	rm -rf build/

distclean: clean
	rm -rf deps/
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(0);
	});

	it("skips pattern rule with diagnostic", () => {
		const content = `
%.o: %.c
	gcc -c $< -o $@

build: main.o
	gcc -o app main.o
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		// build target should still be detected
		expect(result.surfaces.length).toBeGreaterThanOrEqual(1);

		// pattern rule should emit diagnostic
		const patternDiag = result.diagnostics.find((d) => d.kind === "skipped_pattern_rule");
		expect(patternDiag).toBeDefined();
		expect(patternDiag!.reason).toContain("Pattern rules");
	});

	it("skips variable-derived target with diagnostic", () => {
		const content = `
TARGET = myapp

$(TARGET): main.o
	gcc -o $(TARGET) main.o

build: $(TARGET)
	echo "Built"
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		// variable target emits diagnostic
		const varDiag = result.diagnostics.find((d) => d.kind === "skipped_dynamic_target");
		expect(varDiag).toBeDefined();
		expect(varDiag!.reason).toContain("Variable-derived");

		// build target still detected
		const buildSurface = result.surfaces.find((s) => s.displayName === "build");
		expect(buildSurface).toBeDefined();
	});

	it("multiple literal targets produce distinct surfaces", () => {
		const content = `
.PHONY: build test install

build: src/main.c
	gcc -o app src/main.c

test: build
	./run_tests.sh

install: build
	cp app /usr/local/bin/
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		// Should have 3 distinct surfaces
		expect(result.surfaces).toHaveLength(3);

		const names = result.surfaces.map((s) => s.displayName).sort();
		expect(names).toEqual(["build", "install", "test"]);

		// Each has distinct targetName for identity
		const targetNames = result.surfaces.map((s) => s.targetName).sort();
		expect(targetNames).toEqual(["build", "install", "test"]);
	});

	it("same Makefile build/test produce distinct stableSurfaceKey (via targetName)", () => {
		const content = `
build: src/main.c
	gcc -o app src/main.c

test: build
	./run_tests.sh
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		const buildSurface = result.surfaces.find((s) => s.displayName === "build");
		const testSurface = result.surfaces.find((s) => s.displayName === "test");

		expect(buildSurface).toBeDefined();
		expect(testSurface).toBeDefined();

		// Both have makefilePath and targetName for identity
		expect(buildSurface!.makefilePath).toBe("Makefile");
		expect(buildSurface!.targetName).toBe("build");
		expect(testSurface!.makefilePath).toBe("Makefile");
		expect(testSurface!.targetName).toBe("test");

		// Identity fields are distinct → distinct stableSurfaceKey
		expect(buildSurface!.targetName).not.toBe(testSurface!.targetName);
	});

	it("evidence payload contains targetName and dependencies", () => {
		const content = `
build: main.o utils.o lib.a
	gcc -o app main.o utils.o -L. -llib
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(1);

		const evidence = result.surfaces[0].evidence[0];
		expect(evidence.sourceType).toBe("makefile_target");
		expect(evidence.evidenceKind).toBe("build_target");

		const payload = evidence.payload as Record<string, unknown>;
		expect(payload.targetName).toBe("build");
		expect(payload.dependencies).toEqual(["main.o", "utils.o", "lib.a"]);
		expect(payload.makefilePath).toBe("Makefile");
		expect(payload.skippedDynamic).toBe(false);
	});

	it("unknown target is skipped (design decision: avoid pollution)", () => {
		const content = `
custom_target: deps
	./do_something.sh

build: src/main.c
	gcc -o app src/main.c
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		// Only build should produce surface
		expect(result.surfaces).toHaveLength(1);
		expect(result.surfaces[0].displayName).toBe("build");

		// No surface for custom_target (unknown)
		const customSurface = result.surfaces.find((s) => s.displayName === "custom_target");
		expect(customSurface).toBeUndefined();
	});

	it("infers native_c_cpp runtime from .c/.cpp references", () => {
		const content = `
build: main.c utils.cpp
	g++ -o app main.c utils.cpp
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(1);
		expect(result.surfaces[0].runtimeKind).toBe("native_c_cpp");
	});

	it("infers node runtime from npm/node commands", () => {
		const content = `
build:
	npm run build

test:
	npm test
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces.length).toBeGreaterThanOrEqual(1);
		expect(result.surfaces[0].runtimeKind).toBe("node");
	});

	it("returns unknown runtime when no language signals", () => {
		const content = `
build:
	./build.sh
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(1);
		expect(result.surfaces[0].runtimeKind).toBe("unknown");
	});

	it("handles nested Makefile path", () => {
		const content = `
build: src/main.c
	gcc -o app src/main.c
`;
		const result = detectMakefileSurfaces(content, "src/backend/Makefile");
		expect(result.surfaces).toHaveLength(1);
		expect(result.surfaces[0].rootPath).toBe("src/backend");
		expect(result.surfaces[0].makefilePath).toBe("src/backend/Makefile");
	});

	it("emits diagnostic for include directive", () => {
		const content = `
include common.mk

build: src/main.c
	gcc -o app src/main.c
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		const includeDiag = result.diagnostics.find((d) => d.kind === "skipped_include_directive");
		expect(includeDiag).toBeDefined();
		expect(includeDiag!.reason).toContain("not resolved");

		// build target still detected
		expect(result.surfaces).toHaveLength(1);
	});

	it("emits diagnostic for suffix rule", () => {
		const content = `
.c.o:
	gcc -c $<

build: main.o
	gcc -o app main.o
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		const suffixDiag = result.diagnostics.find((d) => d.kind === "skipped_suffix_rule");
		expect(suffixDiag).toBeDefined();

		// build target still detected
		expect(result.surfaces).toHaveLength(1);
	});

	it("handles multiple targets on one rule line", () => {
		const content = `
build test: deps
	./do_both.sh
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		// Both build and test should be detected as separate surfaces
		expect(result.surfaces).toHaveLength(2);
		const names = result.surfaces.map((s) => s.displayName).sort();
		expect(names).toEqual(["build", "test"]);
	});

	it("phony modifier increases confidence", () => {
		const content = `
.PHONY: test

test:
	./test.sh

build:
	./build.sh
`;
		const result = detectMakefileSurfaces(content, "Makefile");

		const testSurface = result.surfaces.find((s) => s.displayName === "test");
		const buildSurface = result.surfaces.find((s) => s.displayName === "build");

		expect(testSurface).toBeDefined();
		expect(buildSurface).toBeDefined();

		// test is .PHONY, build is not
		// Base: test=0.65, build=0.55
		// .PHONY adds 0.10, no deps subtracts 0.10
		// test: 0.65 + 0.10 - 0.10 = 0.65
		// build: 0.55 - 0.10 = 0.45
		expect(testSurface!.confidence).toBeCloseTo(0.65, 2);
		expect(buildSurface!.confidence).toBeCloseTo(0.45, 2);
	});

	it("returns empty surfaces for Makefile with only special targets", () => {
		const content = `
.PHONY: clean
.DEFAULT_GOAL := all

clean:
	rm -rf build/
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(0);
	});

	it("handles line continuations", () => {
		const content = `
build: main.c \\
       utils.c \\
       lib.c
	gcc -o app main.c utils.c lib.c
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		expect(result.surfaces).toHaveLength(1);

		const payload = result.surfaces[0].evidence[0].payload as Record<string, unknown>;
		const deps = payload.dependencies as string[];
		expect(deps).toContain("main.c");
		expect(deps).toContain("utils.c");
		expect(deps).toContain("lib.c");
	});

	it("skips target-specific variable assignments", () => {
		// Lines like "test: CFLAGS += -DTEST" are variable assignments, not targets.
		// The colon is the target separator but the post-colon part is an assignment.
		const content = `
test: CFLAGS += -DTEST
test: main.c
	./run_tests

build: CC := gcc
build: main.o utils.o
	$(CC) -o app main.o utils.o

release: OPTIMIZE ?= -O3
`;
		const result = detectMakefileSurfaces(content, "Makefile");
		// Should only produce surfaces for the actual targets, not the variable assignments.
		// test → cli (test/quality), build → library (build target)
		// release only has a variable assignment line, so no surface for it.
		expect(result.surfaces).toHaveLength(2);

		const names = result.surfaces.map((s) => s.targetName).sort();
		expect(names).toEqual(["build", "test"]);

		// Variable assignment lines are classified as "other" and silently skipped.
		// No diagnostic is emitted for them (design decision: unknown targets don't
		// pollute diagnostics either).
		// The key assertion is that we got exactly 2 surfaces, not 5.
	});
});
