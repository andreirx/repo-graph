/**
 * Gradle dependency reader tests.
 */

import { describe, expect, it } from "vitest";
import { parseGradleDependencies } from "../../../src/adapters/config/gradle-reader.js";

describe("parseGradleDependencies", () => {
	it("extracts group IDs, coordinates, and shortened prefixes", () => {
		const content = `
dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web:3.3.5'
    testImplementation 'org.junit.jupiter:junit-jupiter:5.10.0'
}
`;
		const result = parseGradleDependencies(content);
		// Full group IDs.
		expect(result.names).toContain("org.springframework.boot");
		expect(result.names).toContain("org.junit.jupiter");
		// Full coordinates.
		expect(result.names).toContain("org.springframework.boot:spring-boot-starter-web");
		// Shortened prefixes (first 2 dot-segments) for transitive matching.
		expect(result.names).toContain("org.springframework");
		expect(result.names).toContain("org.junit");
	});

	it("handles Kotlin DSL format", () => {
		const content = `
dependencies {
    implementation("com.fasterxml.jackson.core:jackson-databind:2.17.0")
}
`;
		const result = parseGradleDependencies(content);
		expect(result.names).toContain("com.fasterxml.jackson.core");
	});

	it("handles double-quoted Groovy strings", () => {
		const content = `
dependencies {
    implementation "org.slf4j:slf4j-api:2.0.0"
}
`;
		const result = parseGradleDependencies(content);
		expect(result.names).toContain("org.slf4j");
	});

	it("returns sorted names", () => {
		const content = `
dependencies {
    implementation 'z.z:z-lib:1'
    implementation 'a.a:a-lib:1'
}
`;
		const result = parseGradleDependencies(content);
		const idx_a = result.names.indexOf("a.a");
		const idx_z = result.names.indexOf("z.z");
		expect(idx_a).toBeLessThan(idx_z);
	});

	it("returns empty for content with no dependencies block", () => {
		const result = parseGradleDependencies("plugins { id 'java' }");
		expect(result.names).toEqual([]);
	});

	it("deduplicates across configurations", () => {
		const content = `
dependencies {
    implementation 'org.x:y:1'
    testImplementation 'org.x:y:2'
}
`;
		const result = parseGradleDependencies(content);
		// "org.x" should appear only once.
		expect(result.names.filter((n) => n === "org.x").length).toBe(1);
	});
});
