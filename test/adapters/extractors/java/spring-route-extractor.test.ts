/**
 * Spring route fact extractor — unit tests (prototype).
 *
 * Tests the regex-based annotation scanner against known Spring
 * patterns. Does not require Java compilation or jdtls.
 */

import { describe, expect, it } from "vitest";
import { extractSpringRoutes } from "../../../../src/adapters/extractors/java/spring-route-extractor.js";

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
		// Class-level annotation alone produces no route facts.
		expect(facts).toEqual([]);
	});
});
