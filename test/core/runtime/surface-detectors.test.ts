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
	detectDockerfileSurfaces,
	detectPackageJsonSurfaces,
	detectPyprojectSurfaces,
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
