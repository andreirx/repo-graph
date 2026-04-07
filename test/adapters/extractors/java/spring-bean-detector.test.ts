/**
 * Spring bean detector — unit tests.
 *
 * Tests annotation scanning for container-managed liveness detection.
 * Does not require Java compilation or Spring runtime.
 */

import { describe, expect, it } from "vitest";
import { detectSpringBeans } from "../../../../src/adapters/extractors/java/spring-bean-detector.js";

/** Make a symbol entry for testing. */
function sym(
	name: string,
	subtype: "CLASS" | "METHOD" | "INTERFACE",
	lineStart: number,
	qualifiedName?: string,
) {
	return {
		stableKey: `test:src/Test.java#${qualifiedName ?? name}:SYMBOL:${subtype}`,
		name,
		qualifiedName: qualifiedName ?? name,
		subtype,
		lineStart,
	};
}

/** Detect with Spring imports present. */
function detect(
	source: string,
	symbols: ReturnType<typeof sym>[],
) {
	const withImport = `import org.springframework.stereotype.Component;\nimport org.springframework.context.annotation.Bean;\nimport org.springframework.context.annotation.Configuration;\nimport org.springframework.web.bind.annotation.RestController;\n${source}`;
	return detectSpringBeans(withImport, "src/Test.java", symbols);
}

// ── Class-level stereotypes ─────────────────────────────────────────

describe("detectSpringBeans — class-level stereotypes", () => {
	it("detects @Component", () => {
		const results = detect(
			"@Component\npublic class MyRepo {}",
			[sym("MyRepo", "CLASS", 6)],
		);
		expect(results.length).toBe(1);
		expect(results[0].annotation).toBe("@Component");
		expect(results[0].convention).toBe("spring_component");
		expect(results[0].targetStableKey).toContain("MyRepo");
	});

	it("detects @Service", () => {
		const results = detect(
			"@Service\npublic class UserService {}",
			[sym("UserService", "CLASS", 6)],
		);
		expect(results.length).toBe(1);
		expect(results[0].annotation).toBe("@Service");
		expect(results[0].convention).toBe("spring_service");
	});

	it("detects @Repository", () => {
		const results = detect(
			"@Repository\npublic class UserDB {}",
			[sym("UserDB", "CLASS", 6)],
		);
		expect(results.length).toBe(1);
		expect(results[0].annotation).toBe("@Repository");
		expect(results[0].convention).toBe("spring_repository");
	});

	it("detects @RestController", () => {
		const results = detect(
			"@RestController\npublic class ProductController {}",
			[sym("ProductController", "CLASS", 6)],
		);
		expect(results.length).toBe(1);
		expect(results[0].annotation).toBe("@RestController");
		expect(results[0].convention).toBe("spring_rest_controller");
	});

	it("detects @Controller", () => {
		const source = "import org.springframework.stereotype.Controller;\n@Controller\npublic class WebController {}";
		const results = detectSpringBeans(source, "src/Test.java", [
			sym("WebController", "CLASS", 3),
		]);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("spring_controller");
	});

	it("detects @Configuration", () => {
		const results = detect(
			"@Configuration\npublic class AppConfig {}",
			[sym("AppConfig", "CLASS", 6)],
		);
		expect(results.length).toBe(1);
		expect(results[0].annotation).toBe("@Configuration");
		expect(results[0].convention).toBe("spring_configuration");
	});
});

// ── @Bean factory methods ───────────────────────────────────────────

describe("detectSpringBeans — @Bean methods", () => {
	it("detects @Bean factory method", () => {
		const results = detect(
			"@Configuration\npublic class Config {\n    @Bean\n    UserService userService() { return new UserService(); }\n}",
			[
				sym("Config", "CLASS", 6),
				sym("userService", "METHOD", 8, "Config.userService()"),
			],
		);
		// Should detect both the @Configuration class AND the @Bean method.
		expect(results.length).toBe(2);
		const beanMethod = results.find((r) => r.convention === "spring_bean_factory");
		expect(beanMethod).toBeDefined();
		expect(beanMethod!.annotation).toBe("@Bean");
		expect(beanMethod!.targetStableKey).toContain("userService");
	});

	it("detects multiple @Bean methods in one class", () => {
		const results = detect(
			`@Configuration
public class CoreConfig {
    @Bean
    UserService userService() { return new UserService(); }
    @Bean
    ProductService productService() { return new ProductService(); }
}`,
			[
				sym("CoreConfig", "CLASS", 6),
				sym("userService", "METHOD", 8, "CoreConfig.userService()"),
				sym("productService", "METHOD", 10, "CoreConfig.productService()"),
			],
		);
		const beanMethods = results.filter((r) => r.convention === "spring_bean_factory");
		expect(beanMethods.length).toBe(2);
	});
});

// ── glamCRM patterns ────────────────────────────────────────────────

describe("detectSpringBeans — glamCRM patterns", () => {
	it("detects ActivityLogConfig @Configuration + @Bean", () => {
		const source = `package soft.bijuterie.glam.backend.app.config.core;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

@Configuration
public class ActivityLogConfig {
    @Bean
    ActivityLogService activityLogService(
        PostgresActivityLogRepository repo,
        UserAdminService admin
    ) {
        return new ActivityLogService(repo, admin);
    }
}`;
		const results = detectSpringBeans(source, "src/ActivityLogConfig.java", [
			sym("ActivityLogConfig", "CLASS", 7),
			sym("activityLogService", "METHOD", 9, "ActivityLogConfig.activityLogService(PostgresActivityLogRepository,UserAdminService)"),
		]);
		expect(results.length).toBe(2);
		expect(results.some((r) => r.convention === "spring_configuration")).toBe(true);
		expect(results.some((r) => r.convention === "spring_bean_factory")).toBe(true);
	});

	it("detects PostgresActivityLogRepository @Component", () => {
		const source = `package soft.bijuterie.glam.backend.external.postgres;

import org.springframework.stereotype.Component;

@Component
public class PostgresActivityLogRepository {
}`;
		const results = detectSpringBeans(source, "src/PostgresActivityLogRepository.java", [
			sym("PostgresActivityLogRepository", "CLASS", 6),
		]);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("spring_component");
	});
});

// ── Import gate ─────────────────────────────────────────────────────

describe("detectSpringBeans — import gate", () => {
	it("returns empty for non-Spring Java class", () => {
		const source = `package com.example;
public class Utils {
    public static String format(String s) { return s.trim(); }
}`;
		const results = detectSpringBeans(source, "src/Utils.java", [
			sym("Utils", "CLASS", 2),
		]);
		expect(results).toEqual([]);
	});

	it("returns empty for file with no symbols", () => {
		const results = detect("@Component\npublic class Empty {}", []);
		expect(results).toEqual([]);
	});
});

// ── No false positives ──────────────────────────────────────────────

describe("detectSpringBeans — no false positives", () => {
	it("does not match @Component in a single-line comment", () => {
		const results = detect(
			"// @Component is a Spring annotation\npublic class NotABean {}",
			[sym("NotABean", "CLASS", 7)],
		);
		expect(results).toEqual([]);
	});

	it("does not match @Service in a block comment", () => {
		const results = detect(
			"/* @Service */\npublic class NotABean {}",
			[sym("NotABean", "CLASS", 7)],
		);
		expect(results).toEqual([]);
	});

	it("does not match @Repository in a Javadoc comment", () => {
		const results = detect(
			"* @Repository annotation is used for...\npublic class NotABean {}",
			[sym("NotABean", "CLASS", 7)],
		);
		expect(results).toEqual([]);
	});

	it("does not attribute annotation to wrong class (gap > 10 lines)", () => {
		const results = detect(
			"@Component\n\n\n\n\n\n\n\n\n\n\n\npublic class FarAway {}",
			[sym("FarAway", "CLASS", 17)],
		);
		// 12 blank lines between annotation (line 5) and class (line 17) — gap > 10.
		expect(results).toEqual([]);
	});

	it("does not detect @Bean on a non-method symbol", () => {
		const results = detect(
			"@Bean\npublic class NotAMethod {}",
			[sym("NotAMethod", "CLASS", 6)],
		);
		// @Bean should only match METHOD symbols.
		const beanResults = results.filter((r) => r.convention === "spring_bean_factory");
		expect(beanResults).toEqual([]);
	});
});

// ── Output shape ────────────────────────────────────────────────────

describe("detectSpringBeans — output shape", () => {
	it("returns confidence 0.95", () => {
		const results = detect(
			"@Service\npublic class Svc {}",
			[sym("Svc", "CLASS", 6)],
		);
		expect(results[0].confidence).toBe(0.95);
	});

	it("returns reason string", () => {
		const results = detect(
			"@Service\npublic class Svc {}",
			[sym("Svc", "CLASS", 6)],
		);
		expect(typeof results[0].reason).toBe("string");
		expect(results[0].reason.length).toBeGreaterThan(0);
	});
});
