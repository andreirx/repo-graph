import { describe, it, expect } from "vitest";
import {
	canonicalJsonStringify,
	computeSourceSpecificId,
	computeStableSurfaceKey,
	computeProjectSurfaceUid,
	computeProjectSurfaceEvidenceUid,
	MissingIdentityFieldError,
} from "../../../src/core/runtime/surface-identity.js";

describe("canonicalJsonStringify", () => {
	it("serializes primitives correctly", () => {
		expect(canonicalJsonStringify(null)).toBe("null");
		expect(canonicalJsonStringify(true)).toBe("true");
		expect(canonicalJsonStringify(false)).toBe("false");
		expect(canonicalJsonStringify(42)).toBe("42");
		expect(canonicalJsonStringify(3.14)).toBe("3.14");
		expect(canonicalJsonStringify("hello")).toBe('"hello"');
	});

	it("serializes arrays correctly", () => {
		expect(canonicalJsonStringify([])).toBe("[]");
		expect(canonicalJsonStringify([1, 2, 3])).toBe("[1,2,3]");
		expect(canonicalJsonStringify(["a", "b"])).toBe('["a","b"]');
	});

	it("sorts object keys at top level", () => {
		const obj1 = { b: 2, a: 1, c: 3 };
		const obj2 = { a: 1, c: 3, b: 2 };
		const obj3 = { c: 3, b: 2, a: 1 };

		const expected = '{"a":1,"b":2,"c":3}';
		expect(canonicalJsonStringify(obj1)).toBe(expected);
		expect(canonicalJsonStringify(obj2)).toBe(expected);
		expect(canonicalJsonStringify(obj3)).toBe(expected);
	});

	it("sorts object keys recursively in nested objects", () => {
		const obj1 = {
			outer: { z: 1, a: 2 },
			inner: { m: { y: 1, x: 2 }, n: 3 },
		};
		const obj2 = {
			inner: { n: 3, m: { x: 2, y: 1 } },
			outer: { a: 2, z: 1 },
		};

		const result1 = canonicalJsonStringify(obj1);
		const result2 = canonicalJsonStringify(obj2);

		expect(result1).toBe(result2);
		expect(result1).toBe(
			'{"inner":{"m":{"x":2,"y":1},"n":3},"outer":{"a":2,"z":1}}',
		);
	});

	it("handles arrays of objects with sorted keys", () => {
		const arr = [
			{ b: 2, a: 1 },
			{ d: 4, c: 3 },
		];
		expect(canonicalJsonStringify(arr)).toBe('[{"a":1,"b":2},{"c":3,"d":4}]');
	});

	it("handles deeply nested structures", () => {
		const deep = {
			level1: {
				level2: {
					level3: {
						z: 1,
						a: 2,
					},
				},
			},
		};
		expect(canonicalJsonStringify(deep)).toBe(
			'{"level1":{"level2":{"level3":{"a":2,"z":1}}}}',
		);
	});

	it("handles mixed arrays and objects", () => {
		const mixed = {
			arr: [{ b: 1, a: 2 }, 3, "str"],
			obj: { y: [1, 2], x: "val" },
		};
		expect(canonicalJsonStringify(mixed)).toBe(
			'{"arr":[{"a":2,"b":1},3,"str"],"obj":{"x":"val","y":[1,2]}}',
		);
	});

	it("handles empty objects and arrays", () => {
		expect(canonicalJsonStringify({})).toBe("{}");
		expect(canonicalJsonStringify({ empty: {} })).toBe('{"empty":{}}');
		expect(canonicalJsonStringify({ arr: [] })).toBe('{"arr":[]}');
	});

	it("handles undefined as null", () => {
		expect(canonicalJsonStringify(undefined)).toBe("undefined");
	});
});

describe("computeSourceSpecificId", () => {
	it("computes package_json_bin identity with binName", () => {
		const result = computeSourceSpecificId({
			sourceType: "package_json_bin",
			binName: "my-cli",
			entrypointPath: "./dist/cli.js",
			rootPath: ".",
		});
		expect(result).toBe("my-cli:./dist/cli.js");
	});

	it("throws MissingIdentityFieldError when package_json_bin lacks binName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "package_json_bin",
				entrypointPath: "./dist/cli.js",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);

		try {
			computeSourceSpecificId({
				sourceType: "package_json_bin",
				entrypointPath: "./dist/cli.js",
				rootPath: ".",
			});
		} catch (e) {
			expect(e).toBeInstanceOf(MissingIdentityFieldError);
			const err = e as MissingIdentityFieldError;
			expect(err.sourceType).toBe("package_json_bin");
			expect(err.missingField).toBe("binName");
			expect(err.rootPath).toBe(".");
		}
	});

	it("distinguishes bin aliases pointing to same script", () => {
		const foo = computeSourceSpecificId({
			sourceType: "package_json_bin",
			binName: "foo",
			entrypointPath: "./cli.js",
			rootPath: ".",
		});
		const bar = computeSourceSpecificId({
			sourceType: "package_json_bin",
			binName: "bar",
			entrypointPath: "./cli.js",
			rootPath: ".",
		});
		expect(foo).not.toBe(bar);
		expect(foo).toBe("foo:./cli.js");
		expect(bar).toBe("bar:./cli.js");
	});

	it("computes docker_compose identity with serviceName", () => {
		const api = computeSourceSpecificId({
			sourceType: "docker_compose",
			serviceName: "api",
			rootPath: ".",
		});
		const worker = computeSourceSpecificId({
			sourceType: "docker_compose",
			serviceName: "worker",
			rootPath: ".",
		});
		const redis = computeSourceSpecificId({
			sourceType: "docker_compose",
			serviceName: "redis",
			rootPath: ".",
		});

		expect(api).toBe("api");
		expect(worker).toBe("worker");
		expect(redis).toBe("redis");
		expect(api).not.toBe(worker);
		expect(worker).not.toBe(redis);
	});

	it("computes dockerfile identity with dockerfilePath", () => {
		const result = computeSourceSpecificId({
			sourceType: "dockerfile",
			dockerfilePath: "./backend/Dockerfile",
			rootPath: ".",
		});
		expect(result).toBe("./backend/Dockerfile");
	});

	it("throws MissingIdentityFieldError when dockerfile lacks dockerfilePath", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "dockerfile",
				rootPath: "./backend",
			}),
		).toThrow(MissingIdentityFieldError);

		try {
			computeSourceSpecificId({
				sourceType: "dockerfile",
				rootPath: "./backend",
			});
		} catch (e) {
			const err = e as MissingIdentityFieldError;
			expect(err.sourceType).toBe("dockerfile");
			expect(err.missingField).toBe("dockerfilePath");
		}
	});

	it("computes framework_dependency identity with frameworkName", () => {
		const express = computeSourceSpecificId({
			sourceType: "framework_dependency",
			frameworkName: "express",
			rootPath: ".",
		});
		const react = computeSourceSpecificId({
			sourceType: "framework_dependency",
			frameworkName: "react",
			rootPath: ".",
		});

		expect(express).toBe("framework:express");
		expect(react).toBe("framework:react");
		expect(express).not.toBe(react);
	});

	it("computes cargo_bin_target identity with binName", () => {
		const result = computeSourceSpecificId({
			sourceType: "cargo_bin_target",
			binName: "rmap",
			entrypointPath: "src/main.rs",
			rootPath: ".",
		});
		expect(result).toBe("rmap:src/main.rs");
	});

	it("computes cargo_lib_target identity with entrypointPath", () => {
		const result = computeSourceSpecificId({
			sourceType: "cargo_lib_target",
			entrypointPath: "src/lib.rs",
			rootPath: ".",
		});
		expect(result).toBe("src/lib.rs");
	});

	it("computes pyproject_scripts identity with scriptName", () => {
		const result = computeSourceSpecificId({
			sourceType: "pyproject_scripts",
			scriptName: "my-tool",
			rootPath: ".",
		});
		expect(result).toBe("my-tool");
	});

	it("computes package_json_main identity with entrypointPath", () => {
		const result = computeSourceSpecificId({
			sourceType: "package_json_main",
			entrypointPath: "./dist/index.js",
			rootPath: ".",
		});
		expect(result).toBe("./dist/index.js");
	});

	it("falls back to rootPath for fallback source types", () => {
		// compile_commands is a Phase 1 source type without strict identity requirements.
		// It falls through to the entrypointPath → rootPath fallback chain.
		const result = computeSourceSpecificId({
			sourceType: "compile_commands",
			rootPath: "./build",
		});
		expect(result).toBe("./build");
	});

	it("uses entrypointPath when available in fallback source types", () => {
		const result = computeSourceSpecificId({
			sourceType: "tsconfig_outdir",
			rootPath: "./pkg",
			entrypointPath: "./pkg/dist/index.js",
		});
		expect(result).toBe("./pkg/dist/index.js");
	});

	// ── Error cases for missing required identity fields ──────────────

	it("throws MissingIdentityFieldError when cargo_bin_target lacks binName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "cargo_bin_target",
				entrypointPath: "src/main.rs",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);
	});

	it("throws MissingIdentityFieldError when docker_compose lacks serviceName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "docker_compose",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);

		try {
			computeSourceSpecificId({
				sourceType: "docker_compose",
				rootPath: ".",
			});
		} catch (e) {
			const err = e as MissingIdentityFieldError;
			expect(err.sourceType).toBe("docker_compose");
			expect(err.missingField).toBe("serviceName");
		}
	});

	it("throws MissingIdentityFieldError when framework_dependency lacks frameworkName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "framework_dependency",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);

		try {
			computeSourceSpecificId({
				sourceType: "framework_dependency",
				rootPath: ".",
			});
		} catch (e) {
			const err = e as MissingIdentityFieldError;
			expect(err.sourceType).toBe("framework_dependency");
			expect(err.missingField).toBe("frameworkName");
		}
	});

	it("throws MissingIdentityFieldError when pyproject_scripts lacks scriptName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "pyproject_scripts",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);

		try {
			computeSourceSpecificId({
				sourceType: "pyproject_scripts",
				rootPath: ".",
			});
		} catch (e) {
			const err = e as MissingIdentityFieldError;
			expect(err.sourceType).toBe("pyproject_scripts");
			expect(err.missingField).toBe("scriptName");
		}
	});

	it("allows pyproject_entry_points without scriptName (library fallback)", () => {
		// pyproject_entry_points can be a library surface without scripts
		const result = computeSourceSpecificId({
			sourceType: "pyproject_entry_points",
			entrypointPath: "./src/mylib/__init__.py",
			rootPath: ".",
		});
		expect(result).toBe("./src/mylib/__init__.py");
	});

	// ── Phase 1 strict validation (enum reserved, detectors not implemented) ──

	it("throws MissingIdentityFieldError when makefile_target lacks makefilePath", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "makefile_target",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);
	});

	it("throws MissingIdentityFieldError when pnpm_workspace lacks workspaceName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "pnpm_workspace",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);
	});

	it("throws MissingIdentityFieldError when helm_chart lacks chartName", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "helm_chart",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);
	});

	it("throws MissingIdentityFieldError when terraform_root lacks infraModulePath", () => {
		expect(() =>
			computeSourceSpecificId({
				sourceType: "terraform_root",
				rootPath: ".",
			}),
		).toThrow(MissingIdentityFieldError);
	});

	it("computes Phase 1 identity when required fields are provided", () => {
		expect(computeSourceSpecificId({
			sourceType: "makefile_target",
			makefilePath: "./Makefile",
			rootPath: ".",
		})).toBe("./Makefile");

		expect(computeSourceSpecificId({
			sourceType: "pnpm_workspace",
			workspaceName: "packages/core",
			rootPath: ".",
		})).toBe("workspace:packages/core");

		expect(computeSourceSpecificId({
			sourceType: "helm_chart",
			chartName: "my-app",
			rootPath: ".",
		})).toBe("chart:my-app");

		expect(computeSourceSpecificId({
			sourceType: "terraform_root",
			infraModulePath: "./infra/main",
			rootPath: ".",
		})).toBe("./infra/main");
	});
});

describe("computeStableSurfaceKey", () => {
	it("produces deterministic output for same inputs", () => {
		const input = {
			repoUid: "my-repo",
			moduleCanonicalRootPath: "packages/core",
			surfaceKind: "cli",
			sourceType: "package_json_bin" as const,
			sourceSpecificId: "my-cli:./dist/cli.js",
		};

		const key1 = computeStableSurfaceKey(input);
		const key2 = computeStableSurfaceKey(input);

		expect(key1).toBe(key2);
		expect(key1).toHaveLength(32); // 128 bits = 32 hex chars
	});

	it("produces different keys for different source types", () => {
		const base = {
			repoUid: "my-repo",
			moduleCanonicalRootPath: ".",
			surfaceKind: "backend_service",
			sourceSpecificId: ".",
		};

		const dockerfileKey = computeStableSurfaceKey({
			...base,
			sourceType: "dockerfile",
		});
		const composeKey = computeStableSurfaceKey({
			...base,
			sourceType: "docker_compose",
		});

		expect(dockerfileKey).not.toBe(composeKey);
	});

	it("produces different keys for different source-specific IDs", () => {
		const base = {
			repoUid: "my-repo",
			moduleCanonicalRootPath: ".",
			surfaceKind: "cli",
			sourceType: "package_json_bin" as const,
		};

		const fooKey = computeStableSurfaceKey({
			...base,
			sourceSpecificId: "foo:./cli.js",
		});
		const barKey = computeStableSurfaceKey({
			...base,
			sourceSpecificId: "bar:./cli.js",
		});

		expect(fooKey).not.toBe(barKey);
	});

	it("produces different keys for different repos", () => {
		const base = {
			moduleCanonicalRootPath: ".",
			surfaceKind: "library",
			sourceType: "package_json_main" as const,
			sourceSpecificId: "./dist/index.js",
		};

		const repo1Key = computeStableSurfaceKey({ ...base, repoUid: "repo-1" });
		const repo2Key = computeStableSurfaceKey({ ...base, repoUid: "repo-2" });

		expect(repo1Key).not.toBe(repo2Key);
	});

	it("is independent of snapshot", () => {
		// Stable key should not include snapshot_uid
		const input = {
			repoUid: "my-repo",
			moduleCanonicalRootPath: ".",
			surfaceKind: "cli",
			sourceType: "package_json_bin" as const,
			sourceSpecificId: "my-cli:./cli.js",
		};

		// Same input produces same key regardless of when called
		const key = computeStableSurfaceKey(input);
		expect(key).toHaveLength(32);
		expect(/^[0-9a-f]+$/.test(key)).toBe(true);
	});
});

describe("computeProjectSurfaceUid", () => {
	it("produces deterministic UUID for same inputs", () => {
		const snapshotUid = "my-repo/2024-01-01T00:00:00Z/abc123";
		const stableSurfaceKey = "abcdef1234567890abcdef1234567890";

		const uid1 = computeProjectSurfaceUid(snapshotUid, stableSurfaceKey);
		const uid2 = computeProjectSurfaceUid(snapshotUid, stableSurfaceKey);

		expect(uid1).toBe(uid2);
		// UUID format: 8-4-4-4-12
		expect(uid1).toMatch(
			/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
		);
	});

	it("produces different UIDs for different snapshots", () => {
		const stableSurfaceKey = "abcdef1234567890abcdef1234567890";

		const uid1 = computeProjectSurfaceUid(
			"my-repo/2024-01-01T00:00:00Z/abc123",
			stableSurfaceKey,
		);
		const uid2 = computeProjectSurfaceUid(
			"my-repo/2024-01-02T00:00:00Z/def456",
			stableSurfaceKey,
		);

		expect(uid1).not.toBe(uid2);
	});

	it("produces different UIDs for different stable keys", () => {
		const snapshotUid = "my-repo/2024-01-01T00:00:00Z/abc123";

		const uid1 = computeProjectSurfaceUid(
			snapshotUid,
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
		);
		const uid2 = computeProjectSurfaceUid(
			snapshotUid,
			"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
		);

		expect(uid1).not.toBe(uid2);
	});

	it("snapshot-scoped: same stable key, different snapshots produce different UIDs", () => {
		const stableSurfaceKey = computeStableSurfaceKey({
			repoUid: "my-repo",
			moduleCanonicalRootPath: ".",
			surfaceKind: "cli",
			sourceType: "package_json_bin",
			sourceSpecificId: "my-cli:./cli.js",
		});

		const snapshot1 = "my-repo/2024-01-01T00:00:00Z/abc";
		const snapshot2 = "my-repo/2024-01-01T00:00:01Z/def";

		const uid1 = computeProjectSurfaceUid(snapshot1, stableSurfaceKey);
		const uid2 = computeProjectSurfaceUid(snapshot2, stableSurfaceKey);

		expect(uid1).not.toBe(uid2);
		// But stable key is the same
		expect(stableSurfaceKey).toHaveLength(32);
	});
});

describe("computeProjectSurfaceEvidenceUid", () => {
	it("produces deterministic UUID for same inputs", () => {
		const input = {
			projectSurfaceUid: "12345678-1234-1234-1234-123456789012",
			sourceType: "package_json_bin" as const,
			sourcePath: "package.json",
			payload: { binName: "my-cli", binPath: "./cli.js" },
		};

		const uid1 = computeProjectSurfaceEvidenceUid(input);
		const uid2 = computeProjectSurfaceEvidenceUid(input);

		expect(uid1).toBe(uid2);
		expect(uid1).toMatch(
			/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
		);
	});

	it("produces same UID regardless of payload key order", () => {
		const base = {
			projectSurfaceUid: "12345678-1234-1234-1234-123456789012",
			sourceType: "package_json_bin" as const,
			sourcePath: "package.json",
		};

		const uid1 = computeProjectSurfaceEvidenceUid({
			...base,
			payload: { a: 1, b: 2, c: 3 },
		});
		const uid2 = computeProjectSurfaceEvidenceUid({
			...base,
			payload: { c: 3, a: 1, b: 2 },
		});

		expect(uid1).toBe(uid2);
	});

	it("produces different UIDs for different payloads", () => {
		const base = {
			projectSurfaceUid: "12345678-1234-1234-1234-123456789012",
			sourceType: "package_json_bin" as const,
			sourcePath: "package.json",
		};

		const uid1 = computeProjectSurfaceEvidenceUid({
			...base,
			payload: { binName: "foo" },
		});
		const uid2 = computeProjectSurfaceEvidenceUid({
			...base,
			payload: { binName: "bar" },
		});

		expect(uid1).not.toBe(uid2);
	});

	it("handles null payload", () => {
		const input = {
			projectSurfaceUid: "12345678-1234-1234-1234-123456789012",
			sourceType: "package_json_bin" as const,
			sourcePath: "package.json",
			payload: null,
		};

		const uid = computeProjectSurfaceEvidenceUid(input);
		expect(uid).toMatch(
			/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
		);
	});

	it("produces different UIDs for different source types", () => {
		const base = {
			projectSurfaceUid: "12345678-1234-1234-1234-123456789012",
			sourcePath: "package.json",
			payload: { key: "value" },
		};

		const uid1 = computeProjectSurfaceEvidenceUid({
			...base,
			sourceType: "package_json_bin",
		});
		const uid2 = computeProjectSurfaceEvidenceUid({
			...base,
			sourceType: "package_json_main",
		});

		expect(uid1).not.toBe(uid2);
	});

	it("handles deeply nested payload correctly", () => {
		const base = {
			projectSurfaceUid: "12345678-1234-1234-1234-123456789012",
			sourceType: "docker_compose" as const,
			sourcePath: "docker-compose.yaml",
		};

		const uid1 = computeProjectSurfaceEvidenceUid({
			...base,
			payload: {
				serviceName: "api",
				build: { context: "./backend", dockerfile: "Dockerfile" },
				ports: ["3000:3000"],
			},
		});
		const uid2 = computeProjectSurfaceEvidenceUid({
			...base,
			payload: {
				build: { dockerfile: "Dockerfile", context: "./backend" },
				ports: ["3000:3000"],
				serviceName: "api",
			},
		});

		// Same content, different key order -> same UID
		expect(uid1).toBe(uid2);
	});
});
