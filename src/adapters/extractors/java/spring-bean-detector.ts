/**
 * Spring bean/container-managed class detector.
 *
 * Scans Java source files for Spring stereotype annotations and
 * configuration/factory patterns, emitting framework-liveness
 * observations. These are NODE-LEVEL facts: "this class/method is
 * container-managed, therefore live even if the call graph has no
 * inbound edges."
 *
 * Detected annotations:
 *   Class-level stereotype beans:
 *     @Component    — generic container-managed bean
 *     @Service      — business logic bean
 *     @Repository   — data access bean
 *     @RestController — HTTP handler bean (also a @Controller)
 *     @Controller   — MVC controller bean
 *
 *   Configuration/factory:
 *     @Configuration — bean factory class (itself is a bean)
 *     @Bean          — factory method producing a container-managed bean
 *
 * NOT modeled in this first slice:
 *   - @Autowired / constructor injection (DI edges, not liveness)
 *   - @Conditional / @Profile (conditional bean activation)
 *   - Custom stereotype annotations (@MyService = @Service)
 *   - Component scanning scope/base packages
 *   - @Import / @ImportResource (configuration composition)
 *
 * Implementation: LINE-BASED REGEX scanning over raw source text.
 * NOT AST-backed. Sufficient for standard annotation placement.
 *
 * Maturity: PROTOTYPE.
 */

// ── Output type ─────────────────────────────────────────────────────

/**
 * A detected Spring container-managed bean or factory method.
 */
export interface DetectedSpringBean {
	/** Stable key of the annotated class or @Bean method. */
	targetStableKey: string;
	/** The Spring annotation that made this container-managed. */
	annotation: string;
	/** Machine-stable convention identifier. */
	convention: string;
	/** Confidence in the detection (0-1). */
	confidence: number;
	/** Human-readable explanation. */
	reason: string;
}

// ── Annotation vocabulary ───────────────────────────────────────────

/**
 * Class-level Spring stereotype annotations.
 * Each makes the annotated class a container-managed bean.
 */
const CLASS_LEVEL_STEREOTYPES: Record<string, { convention: string; reason: string }> = {
	"Component": {
		convention: "spring_component",
		reason: "class annotated @Component — container-managed bean",
	},
	"Service": {
		convention: "spring_service",
		reason: "class annotated @Service — business logic bean",
	},
	"Repository": {
		convention: "spring_repository",
		reason: "class annotated @Repository — data access bean",
	},
	"RestController": {
		convention: "spring_rest_controller",
		reason: "class annotated @RestController — HTTP handler bean",
	},
	"Controller": {
		convention: "spring_controller",
		reason: "class annotated @Controller — MVC controller bean",
	},
	"Configuration": {
		convention: "spring_configuration",
		reason: "class annotated @Configuration — bean factory class",
	},
};

/** Regex matching any class-level Spring stereotype annotation. */
const CLASS_ANNOTATION_RE = new RegExp(
	`@(${Object.keys(CLASS_LEVEL_STEREOTYPES).join("|")})\\b`,
);

/** Regex matching @Bean annotation on a method. */
const BEAN_METHOD_RE = /@Bean\b/;

// ── Detector ────────────────────────────────────────────────────────

/**
 * Detect Spring container-managed beans in a Java source file.
 *
 * @param source - Full source text of the .java file.
 * @param filePath - Repo-relative path.
 * @param symbols - Already-extracted symbol nodes from the Java extractor,
 *   used to map detected annotations to stable keys.
 * @returns Detected beans (may be empty).
 */
export function detectSpringBeans(
	source: string,
	_filePath: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		qualifiedName: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
): DetectedSpringBean[] {
	const results: DetectedSpringBean[] = [];
	const lines = source.split("\n");

	// Quick gate: skip files that don't import Spring annotations.
	if (!hasSpringAnnotationImport(source)) return results;

	// Pass 1: find class-level stereotype annotations.
	// When a class-level annotation is found, attribute it to the
	// nearest following CLASS symbol.
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];

		// Skip comment lines. A real annotation starts with optional
		// whitespace then @. Lines with // or * before @ are comments.
		if (isCommentLine(line)) continue;

		const match = line.match(CLASS_ANNOTATION_RE);
		if (!match) continue;

		const annotationName = match[1];
		const stereotype = CLASS_LEVEL_STEREOTYPES[annotationName];
		if (!stereotype) continue;

		// Find the CLASS symbol nearest after this annotation line.
		const classSymbol = findNearestClassAfterLine(i + 1, symbols);
		if (!classSymbol) continue;

		// Avoid duplicates: a class may have multiple annotations
		// (e.g. @RestController + @RequestMapping). Only emit once
		// per stable key per convention.
		if (results.some(
			(r) => r.targetStableKey === classSymbol.stableKey &&
				r.convention === stereotype.convention,
		)) continue;

		results.push({
			targetStableKey: classSymbol.stableKey,
			annotation: `@${annotationName}`,
			convention: stereotype.convention,
			confidence: 0.95,
			reason: stereotype.reason,
		});
	}

	// Pass 2: find @Bean factory methods.
	// A @Bean method is a container-managed factory — both the method
	// itself and its return value are live. We mark the method as
	// framework-managed.
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		if (isCommentLine(line)) continue;
		if (!BEAN_METHOD_RE.test(line)) continue;

		// Find the METHOD symbol nearest after this @Bean line.
		const methodSymbol = findNearestMethodAfterLine(i + 1, symbols);
		if (!methodSymbol) continue;

		if (results.some(
			(r) => r.targetStableKey === methodSymbol.stableKey &&
				r.convention === "spring_bean_factory",
		)) continue;

		results.push({
			targetStableKey: methodSymbol.stableKey,
			annotation: "@Bean",
			convention: "spring_bean_factory",
			confidence: 0.95,
			reason: `method annotated @Bean — container-managed factory producing a bean`,
		});
	}

	return results;
}

// ── Helpers ──────────────────────────────────────────────────────────

/**
 * Check if a line is a comment (single-line // or block-comment * line).
 * Lightweight heuristic: if the trimmed line starts with // or * before
 * any @ appears, it's a comment. Does not handle inline comments after
 * code (e.g. `@Bean // comment`) — those are real annotations and should
 * be detected.
 */
function isCommentLine(line: string): boolean {
	const trimmed = line.trimStart();
	return trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("/*");
}

/**
 * Quick gate: does this file import any Spring annotation?
 */
function hasSpringAnnotationImport(source: string): boolean {
	return (
		source.includes("org.springframework.stereotype") ||
		source.includes("org.springframework.context.annotation") ||
		source.includes("org.springframework.web.bind.annotation") ||
		source.includes("org.springframework.beans.factory.annotation")
	);
}

/**
 * Find the nearest CLASS symbol on or after a given line.
 * Spring annotations sit above the class declaration, so the
 * class typically starts 1-5 lines after the annotation.
 */
function findNearestClassAfterLine(
	lineNumber: number,
	symbols: Array<{
		stableKey: string;
		name: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
): { stableKey: string; name: string } | null {
	let best: { stableKey: string; name: string; lineStart: number } | null = null;
	for (const s of symbols) {
		if (s.subtype !== "CLASS") continue;
		if (s.lineStart === null) continue;
		if (s.lineStart < lineNumber) continue;
		// Must be within 10 lines of the annotation (reasonable gap
		// for Javadoc, other annotations, modifiers).
		if (s.lineStart > lineNumber + 10) continue;
		if (!best || s.lineStart < best.lineStart) {
			best = { stableKey: s.stableKey, name: s.name, lineStart: s.lineStart };
		}
	}
	return best;
}

/**
 * Find the nearest METHOD symbol on or after a given line.
 */
function findNearestMethodAfterLine(
	lineNumber: number,
	symbols: Array<{
		stableKey: string;
		name: string;
		subtype: string | null;
		lineStart: number | null;
	}>,
): { stableKey: string; name: string } | null {
	let best: { stableKey: string; name: string; lineStart: number } | null = null;
	for (const s of symbols) {
		if (s.subtype !== "METHOD") continue;
		if (s.lineStart === null) continue;
		if (s.lineStart < lineNumber) continue;
		if (s.lineStart > lineNumber + 10) continue;
		if (!best || s.lineStart < best.lineStart) {
			best = { stableKey: s.stableKey, name: s.name, lineStart: s.lineStart };
		}
	}
	return best;
}
