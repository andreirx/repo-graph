/**
 * Spring bean detector — indexer integration test.
 *
 * Exercises the full product seam:
 *   Java file with @Component/@Configuration/@Bean
 *   → detectSpringBeans()
 *   → inference persistence in RepoIndexer
 *   → suppression from findDeadNodes()
 *
 * Uses a temp directory with synthetic Java files.
 */

import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { JavaExtractor } from "../../../../src/adapters/extractors/java/java-extractor.js";
import { RepoIndexer } from "../../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { NodeKind } from "../../../../src/core/model/index.js";

// ── Fixture content ─────────────────────────────────────────────────

const SERVICE_JAVA = `
package com.example;

import org.springframework.stereotype.Service;

@Service
public class UserService {
    public String getUser(String id) { return id; }
}
`;

const REPOSITORY_JAVA = `
package com.example;

import org.springframework.stereotype.Repository;

@Repository
public interface UserDB {
    void save(String user);
}
`;

const CONFIG_JAVA = `
package com.example;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

@Configuration
public class AppConfig {
    @Bean
    UserService userService() {
        return new UserService();
    }
}
`;

const PLAIN_JAVA = `
package com.example;

public class Utils {
    public static String format(String s) { return s.trim(); }
}
`;

const BUILD_GRADLE = `
plugins { id 'org.springframework.boot' version '3.0.0' }
dependencies { implementation 'org.springframework.boot:spring-boot-starter-web:3.0.0' }
`;

// ── Test setup ──────────────────────────────────────────────────────

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let javaExtractor: JavaExtractor;
let indexer: RepoIndexer;
let dbPath: string;
let fixtureRoot: string;

const REPO_UID = "spring-bean-test";

beforeAll(async () => {
	javaExtractor = new JavaExtractor();
	await javaExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-spring-bean-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	indexer = new RepoIndexer(storage, [javaExtractor]);

	fixtureRoot = join(tmpdir(), `rgr-spring-fixture-${randomUUID()}`);
	mkdirSync(join(fixtureRoot, "src"), { recursive: true });
	writeFileSync(join(fixtureRoot, "src", "UserService.java"), SERVICE_JAVA);
	writeFileSync(join(fixtureRoot, "src", "UserDB.java"), REPOSITORY_JAVA);
	writeFileSync(join(fixtureRoot, "src", "AppConfig.java"), CONFIG_JAVA);
	writeFileSync(join(fixtureRoot, "src", "Utils.java"), PLAIN_JAVA);
	writeFileSync(join(fixtureRoot, "build.gradle"), BUILD_GRADLE);

	storage.addRepo({
		repoUid: REPO_UID,
		name: REPO_UID,
		rootPath: fixtureRoot,
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});
});

afterEach(() => {
	provider.close();
	try { unlinkSync(dbPath); } catch { /* ignore */ }
	try { rmSync(fixtureRoot, { recursive: true, force: true }); } catch { /* ignore */ }
});

// ── Integration tests ───────────────────────────────────────────────

describe("Spring bean detector — indexer integration", () => {
	it("persists spring_container_managed inferences for annotated classes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const inferences = storage.queryInferences(
			result.snapshotUid,
			"spring_container_managed",
		);

		// UserService(@Service), AppConfig(@Configuration), userService(@Bean) = 3 inferences.
		// UserDB(@Repository) uses interface — let's check if the extractor emits it.
		// The detector finds @Repository but needs a CLASS symbol. UserDB is an interface
		// which the Java extractor emits as INTERFACE subtype, not CLASS. So @Repository
		// on an interface is NOT detected by the current detector.
		expect(inferences.length).toBeGreaterThanOrEqual(3);

		const serviceInf = inferences.find((i) => {
			const val = JSON.parse(i.valueJson);
			return val.convention === "spring_service";
		});
		expect(serviceInf).toBeDefined();

		const configInf = inferences.find((i) => {
			const val = JSON.parse(i.valueJson);
			return val.convention === "spring_configuration";
		});
		expect(configInf).toBeDefined();

		const beanInf = inferences.find((i) => {
			const val = JSON.parse(i.valueJson);
			return val.convention === "spring_bean_factory";
		});
		expect(beanInf).toBeDefined();
	});

	it("suppresses @Service class from findDeadNodes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		// UserService should NOT be in dead list (it has @Service inference).
		const userServiceDead = dead.find((d) => d.symbol === "UserService");
		expect(userServiceDead).toBeUndefined();
	});

	it("suppresses @Configuration class from findDeadNodes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		const configDead = dead.find((d) => d.symbol === "AppConfig");
		expect(configDead).toBeUndefined();
	});

	it("does NOT suppress plain unannotated class from findDeadNodes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		// Utils has no Spring annotation — should remain dead.
		const utilsDead = dead.find((d) => d.symbol === "Utils");
		expect(utilsDead).toBeDefined();
	});

	it("suppresses @Bean factory method from findDeadNodes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		// The @Bean method "userService" in AppConfig should be suppressed.
		const beanMethodDead = dead.filter((d) =>
			d.symbol.includes("userService") && d.file.includes("AppConfig"),
		);
		expect(beanMethodDead.length).toBe(0);
	});
});
