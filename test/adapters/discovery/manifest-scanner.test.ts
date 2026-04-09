/**
 * Manifest scanner adapter tests.
 *
 * Tests the filesystem-backed scanner against controlled temp
 * directories to verify source-specific evidence attribution.
 *
 * Covers the cases that integration tests on static fixtures cannot:
 *   - pnpm-only workspace members (no package.json workspaces)
 *   - divergent package.json vs pnpm-workspace.yaml member sets
 *   - Cargo workspace root with both [workspace] and [package]
 */

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { ManifestScanner } from "../../../src/adapters/discovery/manifest-scanner.js";

const scanner = new ManifestScanner();
const tempDirs: string[] = [];

function makeTempRepo(): string {
	const dir = join(tmpdir(), `rgr-scanner-test-${randomUUID()}`);
	mkdirSync(dir, { recursive: true });
	tempDirs.push(dir);
	return dir;
}

function writeFile(base: string, relPath: string, content: string): void {
	const abs = join(base, relPath);
	mkdirSync(join(abs, ".."), { recursive: true });
	writeFileSync(abs, content, "utf-8");
}

afterEach(() => {
	for (const d of tempDirs) {
		try { rmSync(d, { recursive: true, force: true }); } catch { /* ignore */ }
	}
	tempDirs.length = 0;
});

// ── pnpm-only workspace ────────────────────────────────────────────

describe("ManifestScanner — pnpm-only workspace", () => {
	it("emits only pnpm evidence when package.json has no workspaces", async () => {
		const root = makeTempRepo();

		// Root package.json WITHOUT workspaces field.
		writeFile(root, "package.json", JSON.stringify({
			name: "my-monorepo",
			private: true,
		}));

		// pnpm-workspace.yaml declares members.
		writeFile(root, "pnpm-workspace.yaml", `packages:
  - 'packages/*'
`);

		// Create two member directories with package.json.
		writeFile(root, "packages/api/package.json", JSON.stringify({
			name: "@app/api",
		}));
		writeFile(root, "packages/web/package.json", JSON.stringify({
			name: "@app/web",
		}));
		// Need at least one source file for the directory to be real.
		writeFile(root, "packages/api/src/index.ts", "export const x = 1;");
		writeFile(root, "packages/web/src/index.ts", "export const y = 2;");

		const roots = await scanner.discoverDeclaredModules(root, "test-repo");

		// Should find the two members.
		const members = roots.filter((r) => r.evidenceKind === "workspace_member");
		expect(members.length).toBe(2);

		// All member evidence should be pnpm_workspace_yaml, NOT package_json_workspaces.
		for (const m of members) {
			expect(m.sourceType).toBe("pnpm_workspace_yaml");
		}

		// The root package.json (no workspaces) should appear as standalone.
		const standalones = roots.filter((r) => r.evidenceKind === "package_root");
		expect(standalones.length).toBeGreaterThanOrEqual(1);
		const rootStandalone = standalones.find((r) => r.rootPath === ".");
		expect(rootStandalone).toBeDefined();
		expect(rootStandalone!.displayName).toBe("my-monorepo");
	});
});

// ── Divergent package.json vs pnpm member sets ─────────────────────

describe("ManifestScanner — divergent workspace sources", () => {
	it("attributes evidence correctly when sources declare different members", async () => {
		const root = makeTempRepo();

		// package.json declares packages/api only.
		writeFile(root, "package.json", JSON.stringify({
			name: "divergent-repo",
			private: true,
			workspaces: ["packages/api"],
		}));

		// pnpm-workspace.yaml declares packages/web only.
		writeFile(root, "pnpm-workspace.yaml", `packages:
  - 'packages/web'
`);

		writeFile(root, "packages/api/package.json", JSON.stringify({ name: "@app/api" }));
		writeFile(root, "packages/web/package.json", JSON.stringify({ name: "@app/web" }));
		writeFile(root, "packages/api/src/index.ts", "");
		writeFile(root, "packages/web/src/index.ts", "");

		const roots = await scanner.discoverDeclaredModules(root, "test-repo");

		const members = roots.filter((r) => r.evidenceKind === "workspace_member");

		// api should have package_json_workspaces evidence only.
		const apiMembers = members.filter((r) => r.rootPath === "packages/api");
		expect(apiMembers).toHaveLength(1);
		expect(apiMembers[0].sourceType).toBe("package_json_workspaces");

		// web should have pnpm_workspace_yaml evidence only.
		const webMembers = members.filter((r) => r.rootPath === "packages/web");
		expect(webMembers).toHaveLength(1);
		expect(webMembers[0].sourceType).toBe("pnpm_workspace_yaml");
	});

	it("emits both evidence types when both sources match the same member", async () => {
		const root = makeTempRepo();

		// Both sources declare the same pattern.
		writeFile(root, "package.json", JSON.stringify({
			name: "both-repo",
			private: true,
			workspaces: ["packages/*"],
		}));
		writeFile(root, "pnpm-workspace.yaml", `packages:
  - 'packages/*'
`);

		writeFile(root, "packages/shared/package.json", JSON.stringify({ name: "@app/shared" }));
		writeFile(root, "packages/shared/src/index.ts", "");

		const roots = await scanner.discoverDeclaredModules(root, "test-repo");

		const sharedMembers = roots.filter(
			(r) => r.rootPath === "packages/shared" && r.evidenceKind === "workspace_member",
		);
		expect(sharedMembers).toHaveLength(2);

		const sourceTypes = sharedMembers.map((r) => r.sourceType).sort();
		expect(sourceTypes).toEqual(["package_json_workspaces", "pnpm_workspace_yaml"]);
	});
});

// ── Cargo workspace root with [package] ────────────────────────────

describe("ManifestScanner — Cargo workspace+package root", () => {
	it("emits crate root for workspace root that also has [package]", async () => {
		const root = makeTempRepo();

		writeFile(root, "Cargo.toml", `[workspace]
members = ["crates/core"]

[package]
name = "workspace-root-crate"
version = "0.1.0"
`);
		mkdirSync(join(root, "crates/core"), { recursive: true });
		writeFile(root, "crates/core/Cargo.toml", `[package]
name = "core-crate"
version = "0.1.0"
`);
		writeFile(root, "src/main.rs", "fn main() {}");
		writeFile(root, "crates/core/src/lib.rs", "pub fn hello() {}");

		const roots = await scanner.discoverDeclaredModules(root, "test-repo");

		// Should find:
		// 1. crates/core as workspace_member
		// 2. "." as crate_root (from root Cargo.toml [package])
		const workspaceMembers = roots.filter((r) => r.evidenceKind === "workspace_member");
		expect(workspaceMembers.length).toBeGreaterThanOrEqual(1);
		const coreMember = workspaceMembers.find((r) => r.rootPath === "crates/core");
		expect(coreMember).toBeDefined();
		expect(coreMember!.sourceType).toBe("cargo_workspace");

		const crateRoots = roots.filter((r) => r.evidenceKind === "crate_root");
		const rootCrate = crateRoots.find((r) => r.rootPath === ".");
		expect(rootCrate).toBeDefined();
		expect(rootCrate!.displayName).toBe("workspace-root-crate");

		// Core member should also have its own crate_root evidence
		// (from its own Cargo.toml).
		const coreCrate = crateRoots.find((r) => r.rootPath === "crates/core");
		expect(coreCrate).toBeDefined();
		expect(coreCrate!.displayName).toBe("core-crate");
	});
});
