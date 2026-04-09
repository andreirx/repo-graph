/**
 * Unit tests for manifest-based module discovery detectors.
 *
 * Pure functions — no filesystem, no storage. Tests cover:
 *   - package.json workspaces (array + object forms, root name)
 *   - package.json standalone root
 *   - pnpm-workspace.yaml
 *   - Cargo.toml (workspace members + standalone crate)
 *   - settings.gradle / settings.gradle.kts
 *   - pyproject.toml ([project] + [tool.poetry])
 */

import { describe, expect, it } from "vitest";
import {
	detectCargoManifest,
	detectGradleSettings,
	detectPackageJsonRoot,
	detectPackageJsonWorkspaces,
	detectPnpmWorkspace,
	detectPyprojectToml,
} from "../../../src/core/modules/manifest-detectors.js";

// ── package.json workspaces ────────────────────────────────────────

describe("detectPackageJsonWorkspaces", () => {
	it("detects array-form workspaces", () => {
		const content = JSON.stringify({
			name: "my-monorepo",
			workspaces: ["packages/*", "apps/*"],
		});
		const result = detectPackageJsonWorkspaces(content, "package.json");
		expect(result).not.toBeNull();
		expect(result!.patterns).toEqual(["packages/*", "apps/*"]);
		expect(result!.rootName).toBe("my-monorepo");
	});

	it("detects object-form workspaces with packages key", () => {
		const content = JSON.stringify({
			name: "yarn-v1-monorepo",
			workspaces: { packages: ["libs/*", "services/*"] },
		});
		const result = detectPackageJsonWorkspaces(content, "package.json");
		expect(result).not.toBeNull();
		expect(result!.patterns).toEqual(["libs/*", "services/*"]);
	});

	it("returns null when no workspaces field", () => {
		const content = JSON.stringify({ name: "simple-pkg", version: "1.0.0" });
		const result = detectPackageJsonWorkspaces(content, "package.json");
		expect(result).toBeNull();
	});

	it("returns null for empty workspaces array", () => {
		const content = JSON.stringify({ name: "empty", workspaces: [] });
		const result = detectPackageJsonWorkspaces(content, "package.json");
		expect(result).toBeNull();
	});

	it("returns null for invalid JSON", () => {
		const result = detectPackageJsonWorkspaces("{broken", "package.json");
		expect(result).toBeNull();
	});

	it("returns null rootName when name field is absent", () => {
		const content = JSON.stringify({ workspaces: ["packages/*"] });
		const result = detectPackageJsonWorkspaces(content, "package.json");
		expect(result).not.toBeNull();
		expect(result!.rootName).toBeNull();
	});

	it("filters non-string entries from workspaces array", () => {
		const content = JSON.stringify({
			workspaces: ["packages/*", 42, null, "apps/*"],
		});
		const result = detectPackageJsonWorkspaces(content, "package.json");
		expect(result).not.toBeNull();
		expect(result!.patterns).toEqual(["packages/*", "apps/*"]);
	});
});

// ── package.json standalone root ───────────────────────────────────

describe("detectPackageJsonRoot", () => {
	it("detects a standalone package with name", () => {
		const content = JSON.stringify({ name: "my-app", version: "2.0.0" });
		const result = detectPackageJsonRoot(content, "packages/backend/package.json");
		expect(result).not.toBeNull();
		expect(result!.rootPath).toBe("packages/backend");
		expect(result!.displayName).toBe("my-app");
		expect(result!.evidenceKind).toBe("package_root");
		expect(result!.confidence).toBeGreaterThan(0);
	});

	it("returns null for workspace roots (has workspaces field)", () => {
		const content = JSON.stringify({
			name: "root",
			workspaces: ["packages/*"],
		});
		const result = detectPackageJsonRoot(content, "package.json");
		expect(result).toBeNull();
	});

	it("returns null when name is missing", () => {
		const content = JSON.stringify({ version: "1.0.0" });
		const result = detectPackageJsonRoot(content, "package.json");
		expect(result).toBeNull();
	});

	it("handles root-level package.json (no directory prefix)", () => {
		const content = JSON.stringify({ name: "root-pkg" });
		const result = detectPackageJsonRoot(content, "package.json");
		expect(result).not.toBeNull();
		expect(result!.rootPath).toBe(".");
	});
});

// ── pnpm-workspace.yaml ────────────────────────────────────────────

describe("detectPnpmWorkspace", () => {
	it("detects quoted patterns", () => {
		const content = `packages:
  - 'packages/*'
  - 'apps/*'
`;
		const result = detectPnpmWorkspace(content, "pnpm-workspace.yaml");
		expect(result).not.toBeNull();
		expect(result).toEqual(["packages/*", "apps/*"]);
	});

	it("detects unquoted patterns", () => {
		const content = `packages:
  - packages/*
  - tools/cli
`;
		const result = detectPnpmWorkspace(content, "pnpm-workspace.yaml");
		expect(result).toEqual(["packages/*", "tools/cli"]);
	});

	it("detects double-quoted patterns", () => {
		const content = `packages:
  - "libs/*"
`;
		const result = detectPnpmWorkspace(content, "pnpm-workspace.yaml");
		expect(result).toEqual(["libs/*"]);
	});

	it("returns null when no packages key", () => {
		const content = `catalog:
  react: ^18.0.0
`;
		const result = detectPnpmWorkspace(content, "pnpm-workspace.yaml");
		expect(result).toBeNull();
	});

	it("stops at next top-level key", () => {
		const content = `packages:
  - 'packages/*'
catalog:
  - not-a-package
`;
		const result = detectPnpmWorkspace(content, "pnpm-workspace.yaml");
		expect(result).toEqual(["packages/*"]);
	});
});

// ── Cargo.toml ─────────────────────────────────────────────────────

describe("detectCargoManifest", () => {
	it("detects workspace members", () => {
		const content = `[workspace]
members = ["crates/*", "tools/formatter"]

[workspace.dependencies]
serde = "1.0"
`;
		const result = detectCargoManifest(content, "Cargo.toml");
		expect(result.workspaceMembers).toEqual(["crates/*", "tools/formatter"]);
		expect(result.crateRoot).toBeNull();
	});

	it("detects standalone crate root", () => {
		const content = `[package]
name = "my-tool"
version = "0.1.0"
edition = "2021"
`;
		const result = detectCargoManifest(content, "crates/my-tool/Cargo.toml");
		expect(result.workspaceMembers).toBeNull();
		expect(result.crateRoot).not.toBeNull();
		expect(result.crateRoot!.rootPath).toBe("crates/my-tool");
		expect(result.crateRoot!.displayName).toBe("my-tool");
		expect(result.crateRoot!.evidenceKind).toBe("crate_root");
	});

	it("emits crate root even when workspace members exist", () => {
		const content = `[workspace]
members = ["crates/*"]

[package]
name = "workspace-root"
version = "0.0.0"
`;
		const result = detectCargoManifest(content, "Cargo.toml");
		expect(result.workspaceMembers).toEqual(["crates/*"]);
		// Workspace root with [package] IS a crate — emit both.
		expect(result.crateRoot).not.toBeNull();
		expect(result.crateRoot!.displayName).toBe("workspace-root");
		expect(result.crateRoot!.rootPath).toBe(".");
	});

	it("handles single-quoted members", () => {
		const content = `[workspace]
members = ['crate-a', 'crate-b']
`;
		const result = detectCargoManifest(content, "Cargo.toml");
		expect(result.workspaceMembers).toEqual(["crate-a", "crate-b"]);
	});

	it("returns null members and null crate for empty manifest", () => {
		const result = detectCargoManifest("", "Cargo.toml");
		expect(result.workspaceMembers).toBeNull();
		expect(result.crateRoot).toBeNull();
	});

	it("handles root-level Cargo.toml (no directory prefix)", () => {
		const content = `[package]
name = "root-crate"
version = "0.1.0"
`;
		const result = detectCargoManifest(content, "Cargo.toml");
		expect(result.crateRoot).not.toBeNull();
		expect(result.crateRoot!.rootPath).toBe(".");
	});
});

// ── settings.gradle ────────────────────────────────────────────────

describe("detectGradleSettings", () => {
	it("detects Groovy include with single quotes", () => {
		const content = `rootProject.name = 'my-project'
include 'backend', 'frontend'
`;
		const result = detectGradleSettings(content, "settings.gradle");
		expect(result).toHaveLength(2);
		expect(result.map((r) => r.rootPath).sort()).toEqual(["backend", "frontend"]);
		expect(result[0].evidenceKind).toBe("subproject");
	});

	it("detects Kotlin DSL include with double quotes", () => {
		const content = `rootProject.name = "my-project"
include("backend", "frontend", "shared")
`;
		const result = detectGradleSettings(content, "settings.gradle.kts");
		expect(result).toHaveLength(3);
		expect(result.map((r) => r.rootPath).sort()).toEqual(["backend", "frontend", "shared"]);
	});

	it("detects colon-prefixed subprojects", () => {
		const content = `include ':app', ':lib:core', ':lib:util'
`;
		const result = detectGradleSettings(content, "settings.gradle");
		expect(result.map((r) => r.rootPath).sort()).toEqual(["app", "lib/core", "lib/util"]);
	});

	it("detects includeFlat", () => {
		const content = `includeFlat 'sibling-project'
`;
		const result = detectGradleSettings(content, "settings.gradle");
		expect(result).toHaveLength(1);
		expect(result[0].rootPath).toBe("sibling-project");
	});

	it("deduplicates repeated includes", () => {
		const content = `include 'api'
include 'api'
`;
		const result = detectGradleSettings(content, "settings.gradle");
		expect(result).toHaveLength(1);
	});

	it("returns empty array for settings with no includes", () => {
		const content = `rootProject.name = 'solo-project'
`;
		const result = detectGradleSettings(content, "settings.gradle");
		expect(result).toHaveLength(0);
	});
});

// ── pyproject.toml ─────────────────────────────────────────────────

describe("detectPyprojectToml", () => {
	it("detects [project] name", () => {
		const content = `[project]
name = "my-package"
version = "1.0.0"
`;
		const result = detectPyprojectToml(content, "pyproject.toml");
		expect(result).not.toBeNull();
		expect(result!.rootPath).toBe(".");
		expect(result!.displayName).toBe("my-package");
		expect(result!.evidenceKind).toBe("package_root");
		expect(result!.sourceType).toBe("pyproject_toml");
	});

	it("detects [tool.poetry] name", () => {
		const content = `[tool.poetry]
name = "poetry-pkg"
version = "2.0.0"
`;
		const result = detectPyprojectToml(content, "services/api/pyproject.toml");
		expect(result).not.toBeNull();
		expect(result!.rootPath).toBe("services/api");
		expect(result!.displayName).toBe("poetry-pkg");
	});

	it("returns null when no project name declared", () => {
		const content = `[build-system]
requires = ["setuptools"]
`;
		const result = detectPyprojectToml(content, "pyproject.toml");
		expect(result).toBeNull();
	});

	it("prefers [project] over [tool.poetry] when both exist", () => {
		const content = `[project]
name = "project-name"

[tool.poetry]
name = "poetry-name"
`;
		const result = detectPyprojectToml(content, "pyproject.toml");
		expect(result).not.toBeNull();
		expect(result!.displayName).toBe("project-name");
	});

	it("returns null for empty file", () => {
		const result = detectPyprojectToml("", "pyproject.toml");
		expect(result).toBeNull();
	});
});
