import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { TypeScriptExtractor } from "../../../../src/adapters/extractors/typescript/ts-extractor.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Visibility,
} from "../../../../src/core/model/index.js";
import type { ExtractionResult } from "../../../../src/core/ports/extractor.js";

const FIXTURES = join(
	import.meta.dirname,
	"../../../fixtures/typescript/simple-imports/src",
);
const REPO_UID = "test-repo";
const SNAPSHOT_UID = "test-snapshot";

let extractor: TypeScriptExtractor;

async function extractFile(filename: string): Promise<ExtractionResult> {
	const filePath = `src/${filename}`;
	const fileUid = `${REPO_UID}:${filePath}`;
	const source = await readFile(join(FIXTURES, filename), "utf-8");
	return extractor.extract(source, filePath, fileUid, REPO_UID, SNAPSHOT_UID);
}

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
});

// ── FILE node emission ─────────────────────────────────────────────────

describe("FILE node emission", () => {
	it("emits a FILE node for every extracted file", async () => {
		const result = await extractFile("types.ts");
		const fileNodes = result.nodes.filter((n) => n.kind === NodeKind.FILE);
		expect(fileNodes.length).toBe(1);
		expect(fileNodes[0].name).toBe("types.ts");
		expect(fileNodes[0].qualifiedName).toBe("src/types.ts");
		expect(fileNodes[0].stableKey).toBe(`${REPO_UID}:src/types.ts:FILE`);
	});

	it("FILE node is the source of IMPORTS edges", async () => {
		const result = await extractFile("repository.ts");
		const fileNode = result.nodes.find((n) => n.kind === NodeKind.FILE);
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(fileNode).toBeDefined();
		expect(imports.length).toBe(1);
		expect(imports[0].sourceNodeUid).toBe(fileNode?.nodeUid);
	});
});

// ── types.ts ───────────────────────────────────────────────────────────

describe("types.ts extraction", () => {
	let result: ExtractionResult;

	beforeAll(async () => {
		result = await extractFile("types.ts");
	});

	it("extracts exported interface", () => {
		const iface = result.nodes.find(
			(n) => n.name === "User" && n.subtype === NodeSubtype.INTERFACE,
		);
		expect(iface).toBeDefined();
		expect(iface?.kind).toBe(NodeKind.SYMBOL);
		expect(iface?.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts exported type alias", () => {
		const typeAlias = result.nodes.find(
			(n) => n.name === "UserId" && n.subtype === NodeSubtype.TYPE_ALIAS,
		);
		expect(typeAlias).toBeDefined();
	});

	it("extracts exported enum", () => {
		const enumNode = result.nodes.find(
			(n) => n.name === "Role" && n.subtype === NodeSubtype.ENUM,
		);
		expect(enumNode).toBeDefined();
	});

	it("has no import edges (no imports in types.ts)", () => {
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(imports.length).toBe(0);
	});

	it("extracts JSDoc comments", () => {
		const iface = result.nodes.find((n) => n.name === "User");
		expect(iface?.docComment).toBe("/** A user in the system. */");
	});
});

// ── repository.ts ──────────────────────────────────────────────────────

describe("repository.ts extraction", () => {
	let result: ExtractionResult;

	beforeAll(async () => {
		result = await extractFile("repository.ts");
	});

	it("extracts the UserRepository class", () => {
		const cls = result.nodes.find(
			(n) => n.name === "UserRepository" && n.subtype === NodeSubtype.CLASS,
		);
		expect(cls).toBeDefined();
		expect(cls?.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts class methods", () => {
		const methods = result.nodes.filter(
			(n) => n.subtype === NodeSubtype.METHOD,
		);
		const methodNames = methods.map((m) => m.name).sort();
		expect(methodNames).toContain("findById");
		expect(methodNames).toContain("save");
		expect(methodNames).toContain("updateRole");
	});

	it("extracts import edge to types.ts with FILE stable key target", () => {
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(imports.length).toBe(1);
		expect(imports[0].targetKey).toBe(`${REPO_UID}:src/types:FILE`);
	});
});

// ── service.ts ─────────────────────────────────────────────────────────

describe("service.ts extraction", () => {
	let result: ExtractionResult;

	beforeAll(async () => {
		result = await extractFile("service.ts");
	});

	it("extracts the UserService class", () => {
		const cls = result.nodes.find(
			(n) => n.name === "UserService" && n.subtype === NodeSubtype.CLASS,
		);
		expect(cls).toBeDefined();
	});

	it("extracts the private generateId function", () => {
		const fn = result.nodes.find(
			(n) => n.name === "generateId" && n.subtype === NodeSubtype.FUNCTION,
		);
		expect(fn).toBeDefined();
		expect(fn?.visibility).toBe(Visibility.PRIVATE);
	});

	it("extracts two import edges (types + repository)", () => {
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(imports.length).toBe(2);
		const targets = imports.map((e) => e.targetKey).sort();
		expect(targets).toContain(`${REPO_UID}:src/types:FILE`);
		expect(targets).toContain(`${REPO_UID}:src/repository:FILE`);
	});

	it("extracts CALLS edges for method invocations", () => {
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("this.repo.findById");
		expect(calleeNames).toContain("this.repo.save");
		expect(calleeNames).toContain("this.repo.updateRole");
		expect(calleeNames).toContain("generateId");
	});

	it("extracts class methods with qualified names", () => {
		const methods = result.nodes.filter(
			(n) => n.subtype === NodeSubtype.METHOD,
		);
		const qualifiedNames = methods.map((m) => m.qualifiedName).sort();
		expect(qualifiedNames).toContain("UserService.getUser");
		expect(qualifiedNames).toContain("UserService.createUser");
		expect(qualifiedNames).toContain("UserService.promoteToAdmin");
	});
});

// ── index.ts ───────────────────────────────────────────────────────────

describe("index.ts extraction", () => {
	let result: ExtractionResult;

	beforeAll(async () => {
		result = await extractFile("index.ts");
	});

	it("extracts import edges to service and repository", () => {
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(imports.length).toBe(2);
		const targets = imports.map((e) => e.targetKey).sort();
		expect(targets).toContain(`${REPO_UID}:src/service:FILE`);
		expect(targets).toContain(`${REPO_UID}:src/repository:FILE`);
	});

	it("extracts INSTANTIATES edges for new UserRepository() and new UserService()", () => {
		const instantiates = result.edges.filter(
			(e) => e.type === EdgeType.INSTANTIATES,
		);
		const classNames = instantiates.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").className,
		);
		expect(classNames).toContain("UserRepository");
		expect(classNames).toContain("UserService");
	});

	it("marks service as exported via export { service } list", () => {
		const serviceNode = result.nodes.find(
			(n) => n.name === "service" && n.kind === NodeKind.SYMBOL,
		);
		expect(serviceNode).toBeDefined();
		expect(serviceNode?.visibility).toBe(Visibility.EXPORT);
	});

	it("marks repo as private (not in export list)", () => {
		const repoNode = result.nodes.find(
			(n) => n.name === "repo" && n.kind === NodeKind.SYMBOL,
		);
		expect(repoNode).toBeDefined();
		expect(repoNode?.visibility).toBe(Visibility.PRIVATE);
	});
});

// ── Stable key format ──────────────────────────────────────────────────

describe("stable key format", () => {
	it("produces symbol stable keys in canonical format", async () => {
		const result = await extractFile("types.ts");
		const iface = result.nodes.find((n) => n.name === "User");
		expect(iface?.stableKey).toBe(`${REPO_UID}:src/types.ts#User:SYMBOL`);
	});

	it("produces FILE stable keys in canonical format", async () => {
		const result = await extractFile("types.ts");
		const fileNode = result.nodes.find((n) => n.kind === NodeKind.FILE);
		expect(fileNode?.stableKey).toBe(`${REPO_UID}:src/types.ts:FILE`);
	});
});
