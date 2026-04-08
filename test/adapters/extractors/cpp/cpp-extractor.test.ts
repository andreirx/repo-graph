/**
 * C/C++ extractor — unit tests (first slice).
 */

import { beforeAll, describe, expect, it } from "vitest";
import { CppExtractor } from "../../../../src/adapters/extractors/cpp/cpp-extractor.js";
import { EdgeType, NodeKind, NodeSubtype, Visibility } from "../../../../src/core/model/index.js";

let extractor: CppExtractor;

beforeAll(async () => {
	extractor = new CppExtractor();
	await extractor.initialize();
});

async function extractC(source: string) {
	return extractor.extract(source, "src/main.c", "test:src/main.c", "test", "snap-1");
}

async function extractCpp(source: string) {
	return extractor.extract(source, "src/engine.cpp", "test:src/engine.cpp", "test", "snap-1");
}

// ── FILE node ───────────────────────────────────────────────────────

describe("CppExtractor — FILE node", () => {
	it("emits a FILE node", async () => {
		const result = await extractC("int main() { return 0; }");
		const fileNode = result.nodes.find((n) => n.kind === NodeKind.FILE);
		expect(fileNode).toBeDefined();
		expect(fileNode!.stableKey).toBe("test:src/main.c:FILE");
	});
});

// ── C functions ─────────────────────────────────────────────────────

describe("CppExtractor — C functions", () => {
	it("extracts a function definition", async () => {
		const result = await extractC(`
int helper(int a, int b) {
    return a + b;
}
`);
		const fn = result.nodes.find(
			(n) => n.subtype === NodeSubtype.FUNCTION && n.name === "helper",
		);
		expect(fn).toBeDefined();
		expect(fn!.visibility).toBe(Visibility.EXPORT);
	});

	it("marks static functions as PRIVATE", async () => {
		const result = await extractC(`
static int internal_func(void) {
    return 42;
}
`);
		const fn = result.nodes.find((n) => n.name === "internal_func");
		expect(fn).toBeDefined();
		expect(fn!.visibility).toBe(Visibility.PRIVATE);
	});

	it("extracts main with parameters", async () => {
		const result = await extractC(`
int main(int argc, char *argv[]) {
    return 0;
}
`);
		const fn = result.nodes.find((n) => n.name === "main");
		expect(fn).toBeDefined();
		expect(fn!.signature).toContain("main");
	});
});

// ── C structs / typedefs / enums ────────────────────────────────────

describe("CppExtractor — C types", () => {
	it("extracts a named struct", async () => {
		const result = await extractC(`
struct Point {
    int x;
    int y;
};
`);
		const s = result.nodes.find(
			(n) => n.subtype === NodeSubtype.CLASS && n.name === "Point",
		);
		expect(s).toBeDefined();
	});

	it("extracts a typedef", async () => {
		const result = await extractC(`
typedef unsigned long size_type;
`);
		const td = result.nodes.find(
			(n) => n.subtype === NodeSubtype.TYPE_ALIAS && n.name === "size_type",
		);
		expect(td).toBeDefined();
	});

	it("extracts an enum", async () => {
		const result = await extractC(`
enum Color { RED, GREEN, BLUE };
`);
		const e = result.nodes.find(
			(n) => n.subtype === NodeSubtype.ENUM && n.name === "Color",
		);
		expect(e).toBeDefined();
	});
});

// ── C #include ──────────────────────────────────────────────────────

describe("CppExtractor — #include", () => {
	it("extracts system include", async () => {
		const result = await extractC('#include <stdio.h>\nint main() { return 0; }');
		const edge = result.edges.find(
			(e) => e.type === EdgeType.IMPORTS && e.targetKey === "stdio.h",
		);
		expect(edge).toBeDefined();

		const binding = result.importBindings.find((b) => b.specifier === "stdio.h");
		expect(binding).toBeDefined();
		expect(binding!.isRelative).toBe(false);
	});

	it("extracts local include and marks as relative", async () => {
		const result = await extractC('#include "util.h"\nint main() { return 0; }');
		const edge = result.edges.find(
			(e) => e.type === EdgeType.IMPORTS && e.targetKey === "util.h",
		);
		expect(edge).toBeDefined();

		const binding = result.importBindings.find((b) => b.specifier === "util.h");
		expect(binding).toBeDefined();
		// Quoted includes are always local/project — isRelative must be true.
		expect(binding!.isRelative).toBe(true);
	});

	it("marks quoted path include as relative", async () => {
		const result = await extractC('#include "subdir/util.h"\nint main() { return 0; }');
		const binding = result.importBindings.find((b) => b.specifier === "subdir/util.h");
		expect(binding).toBeDefined();
		expect(binding!.isRelative).toBe(true);
	});
});

// ── C function calls ────────────────────────────────────────────────

describe("CppExtractor — calls", () => {
	it("extracts function calls", async () => {
		const result = await extractC(`
void foo() {
    printf("hello");
    helper(1, 2);
}
`);
		const printfCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "printf",
		);
		expect(printfCall).toBeDefined();

		const helperCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "helper",
		);
		expect(helperCall).toBeDefined();
	});
});

// ── C++ classes ─────────────────────────────────────────────────────

describe("CppExtractor — C++ classes", () => {
	it("extracts a class definition", async () => {
		const result = await extractCpp(`
class Engine {
public:
    void run();
};
`);
		const cls = result.nodes.find(
			(n) => n.subtype === NodeSubtype.CLASS && n.name === "Engine",
		);
		expect(cls).toBeDefined();
	});

	it("extracts declared-only methods (no body)", async () => {
		const result = await extractCpp(`
class Engine {
public:
    void run();
    int get_count() const;
};
`);
		const run = result.nodes.find(
			(n) => n.subtype === NodeSubtype.METHOD && n.name === "run",
		);
		expect(run).toBeDefined();
		expect(run!.qualifiedName).toBe("Engine::run");

		const getCount = result.nodes.find(
			(n) => n.subtype === NodeSubtype.METHOD && n.name === "get_count",
		);
		expect(getCount).toBeDefined();
	});

	it("extracts declared-only constructor (no body)", async () => {
		const result = await extractCpp(`
class Engine {
public:
    Engine(int size);
};
`);
		const ctor = result.nodes.find(
			(n) => n.subtype === NodeSubtype.CONSTRUCTOR && n.name === "Engine",
		);
		expect(ctor).toBeDefined();
		expect(ctor!.qualifiedName).toBe("Engine::Engine");
	});

	it("emits IMPLEMENTS for base classes", async () => {
		const result = await extractCpp(`
class Dog : public Animal {
};
`);
		const impl = result.edges.find(
			(e) => e.type === EdgeType.IMPLEMENTS && e.targetKey === "Animal",
		);
		expect(impl).toBeDefined();
	});
});

// ── C++ namespaces ──────────────────────────────────────────────────

describe("CppExtractor — C++ namespaces", () => {
	it("qualifies functions inside namespaces", async () => {
		const result = await extractCpp(`
namespace mylib {
    int helper() { return 0; }
}
`);
		const fn = result.nodes.find((n) => n.name === "helper");
		expect(fn).toBeDefined();
		expect(fn!.qualifiedName).toBe("mylib::helper");
	});

	it("qualifies out-of-line method definitions", async () => {
		const result = await extractCpp(`
namespace mylib {
    class Engine {
    public:
        void run();
    };
    void Engine::run() {}
}
`);
		const fn = result.nodes.find(
			(n) => n.name === "run" && n.subtype === NodeSubtype.METHOD,
		);
		expect(fn).toBeDefined();
		expect(fn!.qualifiedName).toBe("mylib::Engine::run");
	});
});

// ── Metrics ─────────────────────────────────────────────────────────

describe("CppExtractor — metrics", () => {
	it("computes cyclomatic complexity", async () => {
		const result = await extractC(`
int complex(int x) {
    if (x > 0) {
        for (int i = 0; i < x; i++) {
            if (i % 2 == 0) return i;
        }
    }
    return 0;
}
`);
		const metric = [...result.metrics.values()][0];
		expect(metric).toBeDefined();
		expect(metric.cyclomaticComplexity).toBeGreaterThanOrEqual(4);
	});

	it("counts parameters", async () => {
		const result = await extractC(`
int add(int a, int b, int c) {
    return a + b + c;
}
`);
		const metric = [...result.metrics.values()][0];
		expect(metric.parameterCount).toBe(3);
	});
});

// ── STL call patterns ───────────────────────────────────────────────

describe("CppExtractor — STL calls", () => {
	it("extracts std::sort as a qualified CALLS edge", async () => {
		const result = await extractCpp(`
void foo() { std::sort(v.begin(), v.end()); }
`);
		const sortCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "std::sort",
		);
		expect(sortCall).toBeDefined();
	});

	it("extracts std::make_unique with template arg", async () => {
		const result = await extractCpp(`
void foo() { auto p = std::make_unique<int>(42); }
`);
		const makeCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey?.startsWith("std::make_unique"),
		);
		expect(makeCall).toBeDefined();
	});

	it("extracts receiver.method STL calls with raw receiver", async () => {
		const result = await extractCpp(`
void foo() {
    std::vector<int> v;
    v.push_back(1);
}
`);
		const pushCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "v.push_back",
		);
		expect(pushCall).toBeDefined();
	});

	it("extracts std::find as a qualified CALLS edge", async () => {
		const result = await extractCpp(`
void foo() { auto it = std::find(v.begin(), v.end(), 2); }
`);
		const findCall = result.edges.find(
			(e) => e.type === EdgeType.CALLS && e.targetKey === "std::find",
		);
		expect(findCall).toBeDefined();
	});
});

// ── ExtractorPort contract ──────────────────────────────────────────

describe("CppExtractor — ExtractorPort", () => {
	it("has name cpp-core:0.1.0", () => {
		expect(extractor.name).toBe("cpp-core:0.1.0");
	});

	it("declares c and cpp languages", () => {
		expect(extractor.languages).toEqual(["c", "cpp"]);
	});

	it("provides C/C++ builtins", () => {
		expect(extractor.runtimeBuiltins.identifiers).toContain("printf");
		expect(extractor.runtimeBuiltins.identifiers).toContain("malloc");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("stdio.h");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("vector");
	});
});
