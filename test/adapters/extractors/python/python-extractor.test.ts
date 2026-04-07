/**
 * Python extractor — unit tests (first slice).
 *
 * Tests tree-sitter-python extraction of symbols, imports, calls,
 * and metrics from Python source files.
 */

import { beforeAll, describe, expect, it } from "vitest";
import { PythonExtractor } from "../../../../src/adapters/extractors/python/python-extractor.js";
import { EdgeType, NodeKind, NodeSubtype, Visibility } from "../../../../src/core/model/index.js";

let extractor: PythonExtractor;

beforeAll(async () => {
	extractor = new PythonExtractor();
	await extractor.initialize();
});

async function extract(source: string) {
	return extractor.extract(source, "src/main.py", "test:src/main.py", "test", "snap-1");
}

// ── FILE node ───────────────────────────────────────────────────────

describe("PythonExtractor — FILE node", () => {
	it("emits a FILE node for every file", async () => {
		const result = await extract("x = 1");
		const fileNode = result.nodes.find((n) => n.kind === NodeKind.FILE);
		expect(fileNode).toBeDefined();
		expect(fileNode!.stableKey).toBe("test:src/main.py:FILE");
		expect(fileNode!.name).toBe("main.py");
	});
});

// ── Functions ───────────────────────────────────────────────────────

describe("PythonExtractor — functions", () => {
	it("extracts a top-level function", async () => {
		const result = await extract(`
def process(items):
    return len(items)
`);
		const fn = result.nodes.find(
			(n) => n.subtype === NodeSubtype.FUNCTION && n.name === "process",
		);
		expect(fn).toBeDefined();
		expect(fn!.kind).toBe(NodeKind.SYMBOL);
		expect(fn!.visibility).toBe(Visibility.EXPORT);
	});

	it("marks _private functions as PRIVATE", async () => {
		const result = await extract(`
def _helper():
    pass
`);
		const fn = result.nodes.find((n) => n.name === "_helper");
		expect(fn).toBeDefined();
		expect(fn!.visibility).toBe(Visibility.PRIVATE);
	});

	it("extracts function signature with parameters", async () => {
		const result = await extract(`
def greet(name: str, times: int = 1) -> str:
    return name * times
`);
		const fn = result.nodes.find((n) => n.name === "greet");
		expect(fn).toBeDefined();
		expect(fn!.signature).toContain("name: str");
		expect(fn!.signature).toContain("-> str");
	});

	it("extracts function docstring", async () => {
		const result = await extract(`
def foo():
    """This is a docstring."""
    pass
`);
		const fn = result.nodes.find((n) => n.name === "foo");
		expect(fn).toBeDefined();
		expect(fn!.docComment).toContain("docstring");
	});
});

// ── Classes ─────────────────────────────────────────────────────────

describe("PythonExtractor — classes", () => {
	it("extracts a class definition", async () => {
		const result = await extract(`
class UserService:
    pass
`);
		const cls = result.nodes.find(
			(n) => n.subtype === NodeSubtype.CLASS && n.name === "UserService",
		);
		expect(cls).toBeDefined();
		expect(cls!.kind).toBe(NodeKind.SYMBOL);
		expect(cls!.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts methods inside a class", async () => {
		const result = await extract(`
class Foo:
    def bar(self):
        pass

    def baz(self, x: int) -> int:
        return x
`);
		const bar = result.nodes.find(
			(n) => n.subtype === NodeSubtype.METHOD && n.name === "bar",
		);
		expect(bar).toBeDefined();
		expect(bar!.qualifiedName).toBe("Foo.bar");

		const baz = result.nodes.find(
			(n) => n.subtype === NodeSubtype.METHOD && n.name === "baz",
		);
		expect(baz).toBeDefined();
		expect(baz!.qualifiedName).toBe("Foo.baz");
	});

	it("extracts __init__ as CONSTRUCTOR", async () => {
		const result = await extract(`
class Foo:
    def __init__(self, x):
        self.x = x
`);
		const init = result.nodes.find(
			(n) => n.subtype === NodeSubtype.CONSTRUCTOR,
		);
		expect(init).toBeDefined();
		expect(init!.qualifiedName).toBe("Foo.__init__");
	});

	it("emits IMPLEMENTS edge for base classes", async () => {
		const result = await extract(`
class Dog(Animal):
    pass
`);
		const impl = result.edges.find(
			(e) => e.type === EdgeType.IMPLEMENTS && e.targetKey === "Animal",
		);
		expect(impl).toBeDefined();
	});

	it("handles decorated class methods", async () => {
		const result = await extract(`
class Foo:
    @staticmethod
    def bar():
        pass
`);
		const bar = result.nodes.find((n) => n.name === "bar");
		expect(bar).toBeDefined();
		expect(bar!.subtype).toBe(NodeSubtype.METHOD);
		expect(bar!.qualifiedName).toBe("Foo.bar");
	});
});

// ── Imports ─────────────────────────────────────────────────────────

describe("PythonExtractor — imports", () => {
	it("extracts import statement", async () => {
		const result = await extract("import os");
		const edge = result.edges.find(
			(e) => e.type === EdgeType.IMPORTS && e.targetKey === "os",
		);
		expect(edge).toBeDefined();

		const binding = result.importBindings.find((b) => b.identifier === "os");
		expect(binding).toBeDefined();
		expect(binding!.specifier).toBe("os");
		expect(binding!.isRelative).toBe(false);
	});

	it("extracts from...import statement", async () => {
		const result = await extract("from typing import List, Optional");
		const edge = result.edges.find(
			(e) => e.type === EdgeType.IMPORTS && e.targetKey === "typing",
		);
		expect(edge).toBeDefined();

		const listBinding = result.importBindings.find((b) => b.identifier === "List");
		expect(listBinding).toBeDefined();
		expect(listBinding!.specifier).toBe("typing");

		const optBinding = result.importBindings.find((b) => b.identifier === "Optional");
		expect(optBinding).toBeDefined();
	});

	it("extracts relative import", async () => {
		const result = await extract("from .utils import helper");
		const binding = result.importBindings.find((b) => b.identifier === "helper");
		expect(binding).toBeDefined();
		expect(binding!.isRelative).toBe(true);
		expect(binding!.specifier).toBe(".utils");
	});

	it("extracts dotted import", async () => {
		const result = await extract("import os.path");
		const edge = result.edges.find(
			(e) => e.type === EdgeType.IMPORTS && e.targetKey === "os.path",
		);
		expect(edge).toBeDefined();

		const binding = result.importBindings.find((b) => b.identifier === "os");
		expect(binding).toBeDefined();
		expect(binding!.specifier).toBe("os.path");
	});

	it("extracts aliased import", async () => {
		const result = await extract("import numpy as np");
		const binding = result.importBindings.find((b) => b.identifier === "np");
		expect(binding).toBeDefined();
		expect(binding!.specifier).toBe("numpy");
	});
});

// ── Calls ───────────────────────────────────────────────────────────

describe("PythonExtractor — calls", () => {
	it("extracts function calls", async () => {
		const result = await extract(`
def foo():
    print("hello")
    len([1, 2, 3])
`);
		const printCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "print",
		);
		expect(printCall).toBeDefined();

		const lenCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "len",
		);
		expect(lenCall).toBeDefined();
	});

	it("extracts method calls with receiver", async () => {
		const result = await extract(`
def foo():
    self.db.find(user_id)
`);
		const call = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "self.db.find",
		);
		expect(call).toBeDefined();
	});
});

// ── Top-level variables ─────────────────────────────────────────────

describe("PythonExtractor — variables", () => {
	it("extracts top-level assignments as VARIABLE nodes", async () => {
		const result = await extract(`
API_URL = "https://api.example.com"
MAX_RETRIES = 3
`);
		const apiUrl = result.nodes.find(
			(n) => n.subtype === NodeSubtype.VARIABLE && n.name === "API_URL",
		);
		expect(apiUrl).toBeDefined();
		expect(apiUrl!.visibility).toBe(Visibility.EXPORT);

		const maxRetries = result.nodes.find(
			(n) => n.name === "MAX_RETRIES",
		);
		expect(maxRetries).toBeDefined();
	});

	it("marks _private variables as PRIVATE", async () => {
		const result = await extract("_internal = 42");
		const v = result.nodes.find(
			(n) => n.subtype === NodeSubtype.VARIABLE && n.name === "_internal",
		);
		expect(v).toBeDefined();
		expect(v!.visibility).toBe(Visibility.PRIVATE);
	});
});

// ── Metrics ─────────────────────────────────────────────────────────

describe("PythonExtractor — metrics", () => {
	it("computes cyclomatic complexity", async () => {
		const result = await extract(`
def complex_func(x):
    if x > 0:
        for i in range(x):
            if i % 2 == 0:
                print(i)
    elif x < 0:
        while x < 0:
            x += 1
    return x
`);
		const metric = [...result.metrics.values()][0];
		expect(metric).toBeDefined();
		// Base 1 + if + for + if + elif + while = 6
		expect(metric.cyclomaticComplexity).toBeGreaterThanOrEqual(5);
	});

	it("counts parameters excluding self", async () => {
		const result = await extract(`
class Foo:
    def bar(self, x: int, y: str):
        pass
`);
		const metric = [...result.metrics.values()][0];
		expect(metric).toBeDefined();
		expect(metric.parameterCount).toBe(2);
	});
});

// ── Runtime builtins ────────────────────────────────────────────────

describe("PythonExtractor — runtime builtins", () => {
	it("provides Python builtins", () => {
		expect(extractor.runtimeBuiltins.identifiers).toContain("print");
		expect(extractor.runtimeBuiltins.identifiers).toContain("len");
		expect(extractor.runtimeBuiltins.identifiers).toContain("dict");
		expect(extractor.runtimeBuiltins.identifiers).toContain("Exception");
	});

	it("provides Python stdlib modules", () => {
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("os");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("sys");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("typing");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("json");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("pathlib");
	});
});

// ── ExtractorPort contract ──────────────────────────────────────────

describe("PythonExtractor — ExtractorPort", () => {
	it("has name python-core:0.1.0", () => {
		expect(extractor.name).toBe("python-core:0.1.0");
	});

	it("declares python language", () => {
		expect(extractor.languages).toEqual(["python"]);
	});
});
