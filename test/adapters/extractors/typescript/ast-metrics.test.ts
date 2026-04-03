import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { Language, Parser } from "web-tree-sitter";
import { computeFunctionMetrics } from "../../../../src/adapters/extractors/typescript/ast-metrics.js";

let parser: Parser;

beforeAll(async () => {
	await Parser.init();
	parser = new Parser();
	const lang = await Language.load(
		join(
			import.meta.dirname,
			"../../../../grammars/tree-sitter-typescript.wasm",
		),
	);
	parser.setLanguage(lang);
});

function parseBody(source: string): {
	body: ReturnType<Parser["parse"]>["rootNode"];
	params: ReturnType<Parser["parse"]>["rootNode"] | null;
} {
	const tree = parser.parse(source);
	const fn =
		tree.rootNode.descendantsOfType("function_declaration")[0] ??
		tree.rootNode.descendantsOfType("method_definition")[0];
	const body = fn.childForFieldName("body");
	const params = fn.childForFieldName("parameters");
	if (!body) throw new Error("No body found");
	return { body, params };
}

describe("computeFunctionMetrics", () => {
	it("base complexity of empty function is 1", () => {
		const { body, params } = parseBody("function f() {}");
		const m = computeFunctionMetrics(body, params);
		expect(m.cyclomaticComplexity).toBe(1);
		expect(m.parameterCount).toBe(0);
		expect(m.maxNestingDepth).toBe(0);
	});

	it("counts if/else if as decision points", () => {
		const { body, params } = parseBody(`
			function f(x: number) {
				if (x > 0) {
					return 1;
				} else if (x < 0) {
					return -1;
				} else {
					return 0;
				}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + if(1) + else-if(1) = 3
		expect(m.cyclomaticComplexity).toBe(3);
		expect(m.parameterCount).toBe(1);
	});

	it("counts for loops, while loops, and do-while", () => {
		const { body, params } = parseBody(`
			function f() {
				for (let i = 0; i < 10; i++) {}
				while (true) { break; }
				do {} while (false);
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + for(1) + while(1) + do(1) = 4
		expect(m.cyclomaticComplexity).toBe(4);
	});

	it("counts switch cases", () => {
		const { body, params } = parseBody(`
			function f(x: string) {
				switch (x) {
					case "a": break;
					case "b": break;
					case "c": break;
					default: break;
				}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + case(1) + case(1) + case(1) = 4
		// default is switch_default in tree-sitter, not a decision point
		expect(m.cyclomaticComplexity).toBe(4);
	});

	it("counts logical operators && || ??", () => {
		const { body, params } = parseBody(`
			function f(a: boolean, b: boolean, c: string) {
				if (a && b) {}
				const x = c ?? "default";
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + if(1) + &&(1) + ??(1) = 4
		expect(m.cyclomaticComplexity).toBe(4);
		expect(m.parameterCount).toBe(3);
	});

	it("counts ternary expressions", () => {
		const { body, params } = parseBody(`
			function f(x: number) {
				return x > 0 ? "pos" : "neg";
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + ternary(1) = 2
		expect(m.cyclomaticComplexity).toBe(2);
	});

	it("counts catch clauses", () => {
		const { body, params } = parseBody(`
			function f() {
				try {
					doStuff();
				} catch (e) {
					handleError(e);
				}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + catch(1) = 2
		expect(m.cyclomaticComplexity).toBe(2);
	});

	it("measures nesting depth", () => {
		const { body, params } = parseBody(`
			function f(items: any[]) {
				for (const item of items) {
					if (item.valid) {
						try {
							process(item);
						} catch (e) {
							log(e);
						}
					}
				}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// for(1) > if(2) > try(3) > catch(4)
		expect(m.maxNestingDepth).toBe(4);
	});

	it("does not count nested function bodies", () => {
		const { body, params } = parseBody(`
			function f() {
				const inner = () => {
					if (true) {}
					if (true) {}
				};
				if (true) {}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// base(1) + outer if(1) = 2. Inner arrow function's ifs are excluded.
		expect(m.cyclomaticComplexity).toBe(2);
	});

	it("else-if chains do not inflate nesting depth", () => {
		const { body, params } = parseBody(`
			function f(x: number, y: number) {
				if (x > 0) {
					doA();
				} else if (y > 0) {
					doB();
				} else if (x < 0) {
					doC();
				} else {
					doD();
				}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// All branches are at the same nesting level (1), not stacking.
		// if(1), else_clause doesn't add, else-if's if doesn't add (it's
		// a continuation), else_clause doesn't add.
		expect(m.maxNestingDepth).toBe(1);
		// base(1) + if(1) + else-if(1) + else-if(1) = 4
		expect(m.cyclomaticComplexity).toBe(4);
	});

	it("does not count nested generator function bodies", () => {
		const { body, params } = parseBody(`
			function f() {
				function* gen() {
					if (true) {}
					if (true) {}
				}
			}
		`);
		const m = computeFunctionMetrics(body, params);
		// Only the base complexity of the outer function.
		expect(m.cyclomaticComplexity).toBe(1);
		expect(m.maxNestingDepth).toBe(0);
	});

	it("counts parameter variants correctly", () => {
		const { body, params } = parseBody(`
			function f(a: string, b?: number, ...rest: any[]) {
				return a;
			}
		`);
		const m = computeFunctionMetrics(body, params);
		expect(m.parameterCount).toBe(3);
	});
});
