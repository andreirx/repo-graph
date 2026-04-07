/**
 * Spring route fact extractor — unit tests.
 *
 * Tests the AST-backed annotation scanner against known Spring
 * patterns. Does not require Java compilation or jdtls.
 */

import { beforeAll, describe, expect, it } from "vitest";
import {
	extractSpringRoutes,
	initSpringRouteParser,
} from "../../../../src/adapters/extractors/java/spring-route-extractor.js";

beforeAll(async () => {
	await initSpringRouteParser();
});

function extract(source: string) {
	return extractSpringRoutes(source, "src/Controller.java", "test-repo", []);
}

describe("extractSpringRoutes — route composition", () => {
	it("composes class-level @RequestMapping + method-level @GetMapping", () => {
		const source = `
@RequestMapping("/api/v2/products")
public class ProductController {

    @GetMapping("/{id}")
    public Product getById(Long id) { return null; }

    @PostMapping("")
    public Product create(Product p) { return null; }
}`;
		const facts = extract(source);
		expect(facts.length).toBe(2);
		expect(facts[0].address).toBe("/api/v2/products/{id}");
		expect(facts[0].metadata.httpMethod).toBe("GET");
		expect(facts[1].address).toBe("/api/v2/products");
		expect(facts[1].metadata.httpMethod).toBe("POST");
	});

	it("handles class prefix without leading slash", () => {
		const source = `
@RequestMapping("api/orders")
public class OrderController {
    @GetMapping("")
    public List<Order> list() { return null; }
}`;
		const facts = extract(source);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/orders");
	});

	it("handles no class-level prefix (method-only routes)", () => {
		const source = `
public class SimpleController {
    @GetMapping("/health")
    public String health() { return "ok"; }
}`;
		const facts = extract(source);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/health");
	});
});

describe("extractSpringRoutes — HTTP methods", () => {
	it("maps @GetMapping to GET", () => {
		const facts = extract(`
public class C {
    @GetMapping("/x")
    public void m() {}
}`);
		expect(facts[0].metadata.httpMethod).toBe("GET");
	});

	it("maps @PostMapping to POST", () => {
		const facts = extract(`
public class C {
    @PostMapping("/x")
    public void m() {}
}`);
		expect(facts[0].metadata.httpMethod).toBe("POST");
	});

	it("maps @PutMapping to PUT", () => {
		const facts = extract(`
public class C {
    @PutMapping("/x")
    public void m() {}
}`);
		expect(facts[0].metadata.httpMethod).toBe("PUT");
	});

	it("maps @DeleteMapping to DELETE", () => {
		const facts = extract(`
public class C {
    @DeleteMapping("/x")
    public void m() {}
}`);
		expect(facts[0].metadata.httpMethod).toBe("DELETE");
	});

	it("maps @PatchMapping to PATCH", () => {
		const facts = extract(`
public class C {
    @PatchMapping("/x")
    public void m() {}
}`);
		expect(facts[0].metadata.httpMethod).toBe("PATCH");
	});
});

describe("extractSpringRoutes — operation field", () => {
	it("operation combines method + path", () => {
		const facts = extract(`
@RequestMapping("/api")
public class C {
    @GetMapping("/items")
    public void list() {}
}`);
		expect(facts[0].operation).toBe("GET /api/items");
	});
});

describe("extractSpringRoutes — boundary fact shape", () => {
	it("emits mechanism=http", () => {
		const facts = extract(`
public class C {
    @GetMapping("/x")
    public void m() {}
}`);
		expect(facts[0].mechanism).toBe("http");
	});

	it("emits framework=spring-mvc", () => {
		const facts = extract(`
public class C {
    @GetMapping("/x")
    public void m() {}
}`);
		expect(facts[0].framework).toBe("spring-mvc");
	});

	it("emits basis=annotation", () => {
		const facts = extract(`
public class C {
    @GetMapping("/x")
    public void m() {}
}`);
		expect(facts[0].basis).toBe("annotation");
	});
});

// ── AST maturity: multiline annotations ─────────────────────────────

describe("extractSpringRoutes — multiline annotations", () => {
	it("handles multiline @PostMapping with value= attribute", () => {
		const facts = extract(`
@RequestMapping("/api/v2")
public class Controller {
    @PostMapping(
        value = "/products",
        produces = "application/json"
    )
    public Product create(Product p) { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
		expect(facts[0].metadata.httpMethod).toBe("POST");
	});

	it("handles multiline @RequestMapping with method and path", () => {
		const facts = extract(`
@RequestMapping("/api/v2")
public class Controller {
    @RequestMapping(
        method = RequestMethod.DELETE,
        path = "/{id}"
    )
    public void delete(Long id) {}
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/{id}");
		expect(facts[0].metadata.httpMethod).toBe("DELETE");
	});

	it("handles class-level @RequestMapping split across lines", () => {
		const facts = extract(`
@RequestMapping(
    value = "/api/v2/orders"
)
public class OrderController {
    @GetMapping("/{id}")
    public Order getById(Long id) { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/orders/{id}");
	});
});

// ── AST maturity: attribute forms ───────────────────────────────────

describe("extractSpringRoutes — attribute forms", () => {
	it("extracts value= attribute", () => {
		const facts = extract(`
public class Controller {
    @GetMapping(value = "/items")
    public List<Item> list() { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/items");
	});

	it("extracts path= attribute", () => {
		const facts = extract(`
public class Controller {
    @GetMapping(path = "/items")
    public List<Item> list() { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/items");
	});

	it("extracts positional string (no attribute name)", () => {
		const facts = extract(`
public class Controller {
    @GetMapping("/items")
    public List<Item> list() { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/items");
	});

	it("handles empty @GetMapping (no parens)", () => {
		const facts = extract(`
@RequestMapping("/api")
public class Controller {
    @GetMapping
    public List<Item> list() { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api");
	});

	it("handles @GetMapping() with empty parens", () => {
		const facts = extract(`
@RequestMapping("/api")
public class Controller {
    @GetMapping()
    public List<Item> list() { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api");
	});

	it("handles @RequestMapping with method=RequestMethod.PUT", () => {
		const facts = extract(`
public class Controller {
    @RequestMapping(method = RequestMethod.PUT, value = "/items/{id}")
    public Item update(Long id, Item item) { return null; }
}`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("PUT");
		expect(facts[0].address).toBe("/items/{id}");
	});
});

// ── No false positives ──────────────────────────────────────────────

describe("extractSpringRoutes — no false positives", () => {
	it("returns empty for non-Spring Java class", () => {
		const facts = extract(`
public class Utils {
    public static String format(String s) { return s.trim(); }
}`);
		expect(facts).toEqual([]);
	});

	it("does NOT emit class-level @RequestMapping as a route", () => {
		const source = `
@RequestMapping("/api/v2/orders")
public class OrderController {
}`;
		const facts = extract(source);
		expect(facts).toEqual([]);
	});

	it("returns empty without crash when parser is not initialized and no rootNode", () => {
		// Direct call without init and without rootNode.
		// The shared parser may already be initialized from beforeAll,
		// so this test verifies the quick-gate on non-Spring files.
		const facts = extractSpringRoutes(
			"public class Plain { void foo() {} }",
			"src/Plain.java",
			"test",
			[],
		);
		expect(facts).toEqual([]);
	});
});
