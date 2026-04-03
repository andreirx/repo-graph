import { describe, expect, it } from "vitest";
import { extractPackageManifest } from "../../../../src/adapters/extractors/manifest/package-json.js";

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
