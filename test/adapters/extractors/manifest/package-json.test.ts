import { describe, expect, it } from "vitest";
import {
	extractPackageDependencies,
	extractPackageManifest,
} from "../../../../src/adapters/extractors/manifest/package-json.js";

describe("extractPackageManifest", () => {
	it("extracts name and version from valid package.json", () => {
		const content = JSON.stringify({
			name: "my-app",
			version: "1.2.3",
			description: "test",
		});
		const result = extractPackageManifest(content, "package.json");
		expect(result).not.toBeNull();
		expect(result?.packageName).toBe("my-app");
		expect(result?.packageVersion).toBe("1.2.3");
		expect(result?.sourcePath).toBe("package.json");
	});

	it("handles missing name field", () => {
		const content = JSON.stringify({ version: "0.1.0" });
		const result = extractPackageManifest(content, "package.json");
		expect(result?.packageName).toBeNull();
		expect(result?.packageVersion).toBe("0.1.0");
	});

	it("handles missing version field", () => {
		const content = JSON.stringify({ name: "my-lib" });
		const result = extractPackageManifest(content, "package.json");
		expect(result?.packageName).toBe("my-lib");
		expect(result?.packageVersion).toBeNull();
	});

	it("returns null for invalid JSON", () => {
		const result = extractPackageManifest("not json", "package.json");
		expect(result).toBeNull();
	});

	it("handles non-string name/version fields", () => {
		const content = JSON.stringify({ name: 42, version: true });
		const result = extractPackageManifest(content, "package.json");
		expect(result?.packageName).toBeNull();
		expect(result?.packageVersion).toBeNull();
	});

	it("preserves the relative path", () => {
		const content = JSON.stringify({ name: "sub-pkg", version: "0.0.1" });
		const result = extractPackageManifest(content, "packages/sub/package.json");
		expect(result?.sourcePath).toBe("packages/sub/package.json");
	});
});

describe("extractPackageDependencies", () => {
	it("unions dependencies + devDependencies + peerDependencies + optionalDependencies", () => {
		const content = JSON.stringify({
			name: "x",
			dependencies: { react: "^18", lodash: "^4" },
			devDependencies: { vitest: "^3", typescript: "^5" },
			peerDependencies: { "react-dom": "^18" },
			optionalDependencies: { fsevents: "^2" },
		});
		const result = extractPackageDependencies(content);
		expect(result).not.toBeNull();
		expect(result?.names).toEqual([
			"fsevents",
			"lodash",
			"react",
			"react-dom",
			"typescript",
			"vitest",
		]);
	});

	it("deduplicates names that appear in multiple fields", () => {
		const content = JSON.stringify({
			dependencies: { react: "^18" },
			peerDependencies: { react: "^18" },
		});
		const result = extractPackageDependencies(content);
		expect(result?.names).toEqual(["react"]);
	});

	it("returns empty names array when no dependency fields exist", () => {
		const content = JSON.stringify({ name: "x", version: "0.1.0" });
		const result = extractPackageDependencies(content);
		expect(result).not.toBeNull();
		expect(result?.names).toEqual([]);
	});

	it("returns null for invalid JSON", () => {
		expect(extractPackageDependencies("not json")).toBeNull();
	});

	it("ignores non-object dependency field values", () => {
		const content = JSON.stringify({
			dependencies: "not an object",
			devDependencies: ["still not right"],
			peerDependencies: null,
		});
		const result = extractPackageDependencies(content);
		expect(result?.names).toEqual([]);
	});

	it("names are sorted deterministically", () => {
		const content = JSON.stringify({
			dependencies: { zeta: "^1", alpha: "^1", mu: "^1" },
		});
		const result = extractPackageDependencies(content);
		expect(result?.names).toEqual(["alpha", "mu", "zeta"]);
	});
});
