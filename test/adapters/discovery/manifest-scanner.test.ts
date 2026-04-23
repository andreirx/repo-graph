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

// ── Kbuild module discovery (Layer 3A) ─────────────────────────────

describe("ManifestScanner — discoverBuildSystemModules", () => {
	it("discovers Kbuild subdirectory modules from obj-y assignments", async () => {
		const root = makeTempRepo();

		writeFile(root, "Makefile", `# Top-level Makefile
obj-y += drivers/
obj-y += fs/
`);
		mkdirSync(join(root, "drivers"), { recursive: true });
		mkdirSync(join(root, "fs"), { recursive: true });

		const result = await scanner.discoverBuildSystemModules(root, "test-repo");

		expect(result.filesScanned).toBe(1);
		expect(result.filesWithModules).toBe(1);
		expect(result.modules.length).toBe(2);

		const paths = result.modules.map((m) => m.rootPath).sort();
		expect(paths).toEqual(["drivers", "fs"]);

		// All modules should have moduleKind: "inferred".
		for (const m of result.modules) {
			expect(m.moduleKind).toBe("inferred");
			expect(m.sourceType).toBe("kbuild");
			expect(m.confidence).toBe(0.9);
		}
	});

	it("discovers nested Kbuild modules in subdirectories", async () => {
		const root = makeTempRepo();

		// Root Makefile
		writeFile(root, "Makefile", `obj-y += kernel/\n`);

		// Nested Makefile
		writeFile(root, "kernel/Makefile", `obj-y += sched/
obj-y += irq/
`);

		mkdirSync(join(root, "kernel/sched"), { recursive: true });
		mkdirSync(join(root, "kernel/irq"), { recursive: true });

		const result = await scanner.discoverBuildSystemModules(root, "test-repo");

		expect(result.filesScanned).toBe(2);
		expect(result.filesWithModules).toBe(2);

		const paths = result.modules.map((m) => m.rootPath).sort();
		expect(paths).toEqual(["kernel", "kernel/irq", "kernel/sched"]);
	});

	it("handles Kbuild files (not just Makefile)", async () => {
		const root = makeTempRepo();

		// Use Kbuild filename instead of Makefile
		writeFile(root, "drivers/Kbuild", `obj-y += gpio/
obj-m += usb/
`);

		mkdirSync(join(root, "drivers/gpio"), { recursive: true });
		mkdirSync(join(root, "drivers/usb"), { recursive: true });

		const result = await scanner.discoverBuildSystemModules(root, "test-repo");

		expect(result.modules.length).toBe(2);

		const paths = result.modules.map((m) => m.rootPath).sort();
		expect(paths).toEqual(["drivers/gpio", "drivers/usb"]);
	});

	it("records diagnostics for skipped conditionals", async () => {
		const root = makeTempRepo();

		writeFile(root, "Makefile", `obj-y += always/
ifdef CONFIG_FOO
obj-y += conditional/
endif
`);

		mkdirSync(join(root, "always"), { recursive: true });
		mkdirSync(join(root, "conditional"), { recursive: true });

		const result = await scanner.discoverBuildSystemModules(root, "test-repo");

		// Only "always" should be discovered (conditional skipped).
		expect(result.modules.length).toBe(1);
		expect(result.modules[0].rootPath).toBe("always");

		// Should have diagnostics for the conditional.
		expect(result.diagnostics.length).toBeGreaterThanOrEqual(2);
		const kinds = result.diagnostics.map((d) => d.kind);
		expect(kinds).toContain("skipped_conditional");
		expect(kinds).toContain("skipped_inside_conditional");
	});

	it("returns empty results for non-Kbuild repo", async () => {
		const root = makeTempRepo();

		// A typical JS project — no Makefile/Kbuild
		writeFile(root, "package.json", JSON.stringify({ name: "js-app" }));
		writeFile(root, "src/index.ts", "export const x = 1;");

		const result = await scanner.discoverBuildSystemModules(root, "test-repo");

		expect(result.modules.length).toBe(0);
		expect(result.diagnostics.length).toBe(0);
		expect(result.filesScanned).toBe(0);
		expect(result.filesWithModules).toBe(0);
	});

	it("skips Documentation and scripts directories", async () => {
		const root = makeTempRepo();

		writeFile(root, "Makefile", `obj-y += kernel/\n`);
		// These should be skipped
		writeFile(root, "Documentation/Makefile", `obj-y += ignored/\n`);
		writeFile(root, "scripts/Makefile", `obj-y += ignored/\n`);

		mkdirSync(join(root, "kernel"), { recursive: true });
		mkdirSync(join(root, "Documentation/ignored"), { recursive: true });
		mkdirSync(join(root, "scripts/ignored"), { recursive: true });

		const result = await scanner.discoverBuildSystemModules(root, "test-repo");

		// Only kernel should be discovered.
		expect(result.modules.length).toBe(1);
		expect(result.modules[0].rootPath).toBe("kernel");
		expect(result.filesScanned).toBe(1);
	});
});
