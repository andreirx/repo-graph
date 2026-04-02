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

	// ── Interface member extraction ────────────────────────────────────

	it("extracts interface property members as child nodes", () => {
		const properties = result.nodes.filter(
			(n) =>
				n.subtype === NodeSubtype.PROPERTY &&
				n.parentNodeUid !== null &&
				n.qualifiedName?.startsWith("User."),
		);
		const names = properties.map((n) => n.name).sort();
		expect(names).toEqual(["email", "id", "name"]);
	});

	it("extracts interface method members as child nodes", () => {
		const method = result.nodes.find(
			(n) =>
				n.subtype === NodeSubtype.METHOD &&
				n.qualifiedName === "User.getDisplayName",
		);
		expect(method).toBeDefined();
		expect(method?.kind).toBe(NodeKind.SYMBOL);
		expect(method?.visibility).toBe(Visibility.PUBLIC);
		expect(method?.signature).toBe("getDisplayName()");
	});

	it("sets parent_node_uid to the interface node for members", () => {
		const iface = result.nodes.find(
			(n) => n.name === "User" && n.subtype === NodeSubtype.INTERFACE,
		);
		const members = result.nodes.filter(
			(n) => n.parentNodeUid === iface?.nodeUid,
		);
		expect(members.length).toBe(4); // id, name, email, getDisplayName
	});

	it("generates stable keys with qualified names for interface members", () => {
		const method = result.nodes.find(
			(n) => n.qualifiedName === "User.getDisplayName",
		);
		expect(method?.stableKey).toBe(
			`${REPO_UID}:src/types.ts#User.getDisplayName:SYMBOL:METHOD`,
		);
	});

	// ── Interface overload merging ─────────────────────────────────────

	it("merges overloaded interface methods into one node", () => {
		const formatNodes = result.nodes.filter(
			(n) => n.qualifiedName === "Formatter.format",
		);
		// Three overload signatures, but only one node
		expect(formatNodes.length).toBe(1);
		expect(formatNodes[0].subtype).toBe(NodeSubtype.METHOD);
	});

	it("keeps the first overload's signature", () => {
		const formatNode = result.nodes.find(
			(n) => n.qualifiedName === "Formatter.format",
		);
		// First overload: format(value: string): string
		expect(formatNode?.signature).toBe("format(value: string)");
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
		// Receiver type binding: this.repo -> UserRepository (from constructor param)
		expect(calleeNames).toContain("UserRepository.findById");
		expect(calleeNames).toContain("UserRepository.save");
		expect(calleeNames).toContain("UserRepository.updateRole");
		expect(calleeNames).toContain("generateId");
	});

	it("preserves raw call name in metadata when binding resolves", () => {
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const findByIdEdge = calls.find((e) => {
			const meta = JSON.parse(e.metadataJson ?? "{}");
			return meta.calleeName === "UserRepository.findById";
		});
		expect(findByIdEdge).toBeDefined();
		const meta = JSON.parse(findByIdEdge?.metadataJson ?? "{}");
		expect(meta.rawCalleeName).toBe("this.repo.findById");
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
	it("produces symbol stable keys in canonical format (includes subtype)", async () => {
		const result = await extractFile("types.ts");
		const iface = result.nodes.find((n) => n.name === "User");
		expect(iface?.stableKey).toBe(
			`${REPO_UID}:src/types.ts#User:SYMBOL:INTERFACE`,
		);
	});

	it("produces FILE stable keys in canonical format", async () => {
		const result = await extractFile("types.ts");
		const fileNode = result.nodes.find((n) => n.kind === NodeKind.FILE);
		expect(fileNode?.stableKey).toBe(`${REPO_UID}:src/types.ts:FILE`);
	});
});

// ── this.method() class-context resolution ────────────────────────────

describe("this.method() resolves to enclosing class", () => {
	it("rewrites this.method() to ClassName.method", async () => {
		const source = `
class UserService {
	save() {}
	doWork() {
		this.save();
	}
}
`;
		const result = await extractor.extract(
			source,
			"src/this-method-test.ts",
			`${REPO_UID}:src/this-method-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("UserService.save");
		expect(calleeNames).not.toContain("this.save");
	});

	it("does not resolve this.method() outside a class", async () => {
		const source = `
function standalone() {
	this.save(); // not inside a class — should stay as this.save
}
`;
		const result = await extractor.extract(
			source,
			"src/this-no-class-test.ts",
			`${REPO_UID}:src/this-no-class-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("this.save");
	});

	it("preserves raw this.method in metadata", async () => {
		const source = `
class Repo {
	commit() {
		this.flush();
	}
	flush() {}
}
`;
		const result = await extractor.extract(
			source,
			"src/this-meta-test.ts",
			`${REPO_UID}:src/this-meta-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const flushEdge = calls.find((e) => {
			const meta = JSON.parse(e.metadataJson ?? "{}");
			return meta.calleeName === "Repo.flush";
		});
		expect(flushEdge).toBeDefined();
		const meta = JSON.parse(flushEdge?.metadataJson ?? "{}");
		expect(meta.rawCalleeName).toBe("this.flush");
	});
});

// ── Receiver binding scope regression ──────────────────────────────────

describe("receiver binding respects parameter shadowing", () => {
	it("resolves typed parameters and shadows untyped parameters", async () => {
		const source = `
const repo: RepoA = new RepoA();

function usesFileScope() {
	repo.save(); // should resolve to RepoA.save via file-scope binding
}

function hasTypedParam(repo: RepoB) {
	repo.save(); // should resolve to RepoB.save via parameter binding (not RepoA)
}

function hasUntypedParam(repo) {
	repo.save(); // should stay as repo.save — untyped param shadows without rewriting
}
`;
		const result = await extractor.extract(
			source,
			"src/shadow-test.ts",
			`${REPO_UID}:src/shadow-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);

		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);

		// File-scope binding: repo -> RepoA
		expect(calleeNames).toContain("RepoA.save");
		// Typed parameter binding: repo -> RepoB (overrides file-scope)
		expect(calleeNames).toContain("RepoB.save");
		// Untyped parameter: shadows file-scope, no rewrite
		expect(calleeNames).toContain("repo.save");
		// Each appears exactly once
		expect(calleeNames.filter((n) => n === "RepoA.save").length).toBe(1);
		expect(calleeNames.filter((n) => n === "RepoB.save").length).toBe(1);
		expect(calleeNames.filter((n) => n === "repo.save").length).toBe(1);
	});
});

// ── Function-local bindings with block scope ──────────────────────────

describe("function-local bindings and block shadowing", () => {
	it("resolves calls via function-local const declarations", async () => {
		const source = `
function doWork() {
	const repo: UserRepository = new UserRepository();
	repo.save();
}
`;
		const result = await extractor.extract(
			source,
			"src/local-test.ts",
			`${REPO_UID}:src/local-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("UserRepository.save");
	});

	it("resolves calls via new ClassName() without explicit annotation", async () => {
		const source = `
function doWork() {
	const repo = new UserRepository();
	repo.save();
}
`;
		const result = await extractor.extract(
			source,
			"src/local-new-test.ts",
			`${REPO_UID}:src/local-new-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("UserRepository.save");
	});

	it("inner block shadows outer binding", async () => {
		const source = `
function doWork() {
	const repo: RepoA = new RepoA();
	repo.save(); // -> RepoA.save
	if (true) {
		const repo: RepoB = new RepoB();
		repo.save(); // -> RepoB.save (shadows outer)
	}
	repo.save(); // -> RepoA.save (outer scope restored)
}
`;
		const result = await extractor.extract(
			source,
			"src/block-shadow-test.ts",
			`${REPO_UID}:src/block-shadow-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames.filter((n) => n === "RepoA.save").length).toBe(2);
		expect(calleeNames.filter((n) => n === "RepoB.save").length).toBe(1);
	});

	it("does not resolve call before its declaration (temporal correctness)", async () => {
		const source = `
function f() {
	repo.save(); // call BEFORE declaration — must NOT resolve
	const repo: Repo = new Repo();
	repo.save(); // call AFTER declaration — should resolve
}
`;
		const result = await extractor.extract(
			source,
			"src/temporal-test.ts",
			`${REPO_UID}:src/temporal-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// First call: before declaration, unresolved
		expect(calleeNames).toContain("repo.save");
		// Second call: after declaration, resolved
		expect(calleeNames).toContain("Repo.save");
		expect(calleeNames.filter((n) => n === "Repo.save").length).toBe(1);
	});

	it("var before assignment does not resolve (hoisted but uninitialized)", async () => {
		const source = `
function f() {
	repo.save(); // var hoisted but not assigned yet — must NOT resolve
	var repo: Repo = new Repo();
	repo.save(); // after assignment — should resolve
}
`;
		const result = await extractor.extract(
			source,
			"src/var-temporal-test.ts",
			`${REPO_UID}:src/var-temporal-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// Before assignment: hoisted name shadows file-scope, no typed binding
		expect(calleeNames).toContain("repo.save");
		// After assignment: typed binding is active
		expect(calleeNames).toContain("Repo.save");
		expect(calleeNames.filter((n) => n === "Repo.save").length).toBe(1);
	});

	it("TDZ shadows outer binding before inner declaration", async () => {
		const source = `
const repo: Outer = new Outer();
function f() {
	{
		repo.save(); // inner const repo exists — TDZ, must NOT resolve to Outer
		const repo: Inner = new Inner();
		repo.save(); // after declaration — should resolve to Inner
	}
}
`;
		const result = await extractor.extract(
			source,
			"src/tdz-shadow-test.ts",
			`${REPO_UID}:src/tdz-shadow-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// Before inner declaration: TDZ shadow blocks outer
		expect(calleeNames).toContain("repo.save");
		expect(calleeNames).not.toContain("Outer.save");
		// After inner declaration
		expect(calleeNames).toContain("Inner.save");
	});

	it("var declaration is visible after the block it was declared in", async () => {
		const source = `
function f() {
	if (true) {
		var repo: Repo = new Repo();
	}
	repo.save(); // var is function-scoped, should resolve
}
`;
		const result = await extractor.extract(
			source,
			"src/var-hoist-test.ts",
			`${REPO_UID}:src/var-hoist-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("Repo.save");
	});

	// ── Loop-header scope ──────────────────────────────────────────────

	it("for-loop const initializer binds in loop body", async () => {
		const source = `
function f() {
	for (const repo = new Repo(); ; ) {
		repo.save();
		break;
	}
}
`;
		const result = await extractor.extract(
			source,
			"src/for-const-test.ts",
			`${REPO_UID}:src/for-const-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("Repo.save");
	});

	it("for-loop const initializer shadows outer binding", async () => {
		const source = `
const repo: Outer = new Outer();
function f() {
	for (const repo = new Inner(); ; ) {
		repo.save();
		break;
	}
}
`;
		const result = await extractor.extract(
			source,
			"src/for-shadow-test.ts",
			`${REPO_UID}:src/for-shadow-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("Inner.save");
		expect(calleeNames).not.toContain("Outer.save");
	});

	it("for-of loop variable shadows outer binding", async () => {
		const source = `
const repo: Outer = new Outer();
function f(repos: any[]) {
	for (const repo of repos) {
		repo.save(); // should NOT resolve to Outer.save
	}
}
`;
		const result = await extractor.extract(
			source,
			"src/for-of-shadow-test.ts",
			`${REPO_UID}:src/for-of-shadow-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("repo.save");
		expect(calleeNames).not.toContain("Outer.save");
	});

	it("for-var initializer typed binding survives loop and reaches later code", async () => {
		const source = `
function f() {
	for (var repo = new Repo(); ; ) {
		repo.save(); // inside loop: should resolve
		break;
	}
	repo.save(); // after loop: var is function-scoped, should resolve
}
`;
		const result = await extractor.extract(
			source,
			"src/for-var-test.ts",
			`${REPO_UID}:src/for-var-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames.filter((n) => n === "Repo.save").length).toBe(2);
	});

	// ── Initializer self-reference soundness ───────────────────────────

	it("const binding is not visible inside its own initializer", async () => {
		const source = `
const repo: Outer = new Outer();
function f() {
	const repo: Inner = setup(repo.save());
}
`;
		const result = await extractor.extract(
			source,
			"src/init-self-ref-test.ts",
			`${REPO_UID}:src/init-self-ref-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// repo.save() inside the initializer should NOT resolve to Inner.save.
		// The inner const is in TDZ at that point, so it shadows Outer too.
		expect(calleeNames).not.toContain("Inner.save");
		expect(calleeNames).not.toContain("Outer.save");
		expect(calleeNames).toContain("repo.save");
	});

	it("var binding is not visible inside its own initializer", async () => {
		const source = `
function f() {
	var repo = new Repo(repo.save());
}
`;
		const result = await extractor.extract(
			source,
			"src/var-init-self-ref-test.ts",
			`${REPO_UID}:src/var-init-self-ref-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// repo.save() inside new Repo() should NOT resolve to Repo.save
		expect(calleeNames).not.toContain("Repo.save");
		expect(calleeNames).toContain("repo.save");
	});

	it("for-loop initializer binding is not visible inside its own initializer", async () => {
		const source = `
const repo: Outer = new Outer();
function f() {
	for (const repo: Inner = setup(repo.save()); ; ) { break; }
}
`;
		const result = await extractor.extract(
			source,
			"src/for-init-self-ref-test.ts",
			`${REPO_UID}:src/for-init-self-ref-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).not.toContain("Inner.save");
		expect(calleeNames).not.toContain("Outer.save");
		expect(calleeNames).toContain("repo.save");
	});

	it("untyped local variable shadows file-scope binding", async () => {
		const source = `
const repo: RepoA = new RepoA();

function doWork() {
	const repo = getRepo(); // no type annotation, no new — shadow only
	repo.save(); // should NOT resolve to RepoA.save
}
`;
		const result = await extractor.extract(
			source,
			"src/shadow-local-test.ts",
			`${REPO_UID}:src/shadow-local-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("repo.save");
		expect(calleeNames).not.toContain("RepoA.save");
	});
});

// ── 3-part chain resolution ────────────────────────────────────────────

describe("3-part chain resolution (variable.property.method)", () => {
	it("resolves ctx.storage.insertNodes() via memberTypes", async () => {
		const source = `
interface AppContext {
	storage: StoragePort;
	indexer: IndexerPort;
}

function bootstrap(ctx: AppContext) {
	ctx.storage.insertNodes([]);
	ctx.indexer.indexRepo("x");
}
`;
		const result = await extractor.extract(
			source,
			"src/chain-test.ts",
			`${REPO_UID}:src/chain-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		expect(calleeNames).toContain("StoragePort.insertNodes");
		expect(calleeNames).toContain("IndexerPort.indexRepo");
	});

	it("falls back to partial resolution when property type unknown", async () => {
		const source = `
interface Config {
	debug: boolean;
}

function run(cfg: Config) {
	cfg.logger.info("hello");
}
`;
		const result = await extractor.extract(
			source,
			"src/chain-partial-test.ts",
			`${REPO_UID}:src/chain-partial-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// logger is not in Config's memberTypes (boolean is not a type_identifier),
		// so falls back to Config.logger.info (partially resolved)
		expect(calleeNames).toContain("Config.logger.info");
	});

	it("does not resolve 3-part chain when variable type is imported (not in file)", async () => {
		const source = `
function run(ctx: ExternalType) {
	ctx.db.query("SELECT 1");
}
`;
		const result = await extractor.extract(
			source,
			"src/chain-external-test.ts",
			`${REPO_UID}:src/chain-external-test.ts`,
			REPO_UID,
			SNAPSHOT_UID,
		);
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const calleeNames = calls.map(
			(e) => JSON.parse(e.metadataJson ?? "{}").calleeName,
		);
		// ExternalType is not defined in this file, so no memberTypes available.
		// Falls back to partially resolved: ExternalType.db.query
		expect(calleeNames).toContain("ExternalType.db.query");
	});
});

// ── Dual export regression (stable key v2) ────────────────────────────

describe("dual export: const + type with same name", () => {
	let result: ExtractionResult;

	beforeAll(async () => {
		result = await extractFile("dual-export.ts");
	});

	it("extracts both the const and the type alias as separate nodes", () => {
		const statusNodes = result.nodes.filter((n) => n.name === "Status");
		expect(statusNodes.length).toBe(2);

		const subtypes = statusNodes.map((n) => n.subtype).sort();
		expect(subtypes).toEqual([NodeSubtype.CONSTANT, NodeSubtype.TYPE_ALIAS]);
	});

	it("assigns distinct stable_keys to const and type with same name", () => {
		const statusNodes = result.nodes.filter((n) => n.name === "Status");
		const keys = statusNodes.map((n) => n.stableKey).sort();
		expect(keys.length).toBe(2);
		expect(keys[0]).not.toBe(keys[1]);

		expect(keys).toContain(
			`${REPO_UID}:src/dual-export.ts#Status:SYMBOL:CONSTANT`,
		);
		expect(keys).toContain(
			`${REPO_UID}:src/dual-export.ts#Status:SYMBOL:TYPE_ALIAS`,
		);
	});

	it("handles multiple companion-type pairs in the same file", () => {
		const priorityNodes = result.nodes.filter((n) => n.name === "Priority");
		expect(priorityNodes.length).toBe(2);

		const keys = priorityNodes.map((n) => n.stableKey).sort();
		expect(keys[0]).not.toBe(keys[1]);
	});

	it("marks both exports as EXPORT visibility", () => {
		const statusNodes = result.nodes.filter((n) => n.name === "Status");
		for (const node of statusNodes) {
			expect(node.visibility).toBe(Visibility.EXPORT);
		}
	});
});
