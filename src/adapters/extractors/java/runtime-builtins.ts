/**
 * Java runtime builtins corpus.
 *
 * Language-specific data for the unresolved-edge classifier. Exported
 * as a RuntimeBuiltinsSet DTO via the ExtractorPort.runtimeBuiltins
 * field. The core classifier consumes it agnostically.
 *
 * Categories:
 *   1. `identifiers` -- globally-available Java standard library types
 *      that appear in Java code without explicit import statements
 *      (java.lang types are auto-imported, plus commonly used types
 *      from java.util and other packages that nearly every codebase
 *      imports).
 *   2. `moduleSpecifiers` -- Java standard library package prefixes.
 *      The import classifier checks if the import declaration specifier
 *      matches a known stdlib package. For Java, `java.*` and `javax.*`
 *      are the primary stdlib roots.
 *
 * Scope: java.lang auto-imported types (String, Integer, Object, etc.)
 * and commonly-used types from java.util, java.io, and java.time that
 * appear in nearly every codebase.
 */

import type { RuntimeBuiltinsSet } from "../../../core/classification/signals.js";

// -- java.lang and commonly used standard library types -----------------

const JAVA_STD_TYPES: readonly string[] = [
	// java.lang (auto-imported)
	"Object",
	"String",
	"Integer",
	"Long",
	"Double",
	"Float",
	"Boolean",
	"Byte",
	"Short",
	"Character",
	"Number",
	"Math",
	"System",
	"Class",
	"Thread",
	"Throwable",
	// Exceptions
	"Exception",
	"RuntimeException",
	"Error",
	"NullPointerException",
	"IllegalArgumentException",
	"IllegalStateException",
	"UnsupportedOperationException",
	"IOException",
	// Collections (java.util -- nearly always imported)
	"List",
	"ArrayList",
	"Map",
	"HashMap",
	"Set",
	"HashSet",
	"Collection",
	"Collections",
	"Arrays",
	"Optional",
	// Streams (java.util.stream)
	"Stream",
	"Collectors",
	// String builders
	"StringBuilder",
	"StringBuffer",
	// Date/time (java.time)
	"Date",
	"Calendar",
	"Instant",
	"Duration",
	"LocalDate",
	"LocalDateTime",
];

// -- Java stdlib package specifiers ------------------------------------
// The import classifier checks the package prefix of import declarations
// against this list. E.g. `import java.util.HashMap;`
// -> specifier "java.util" matches -> external/runtime.

const JAVA_STDLIB_MODULES: readonly string[] = [
	// Core packages
	"java.lang",
	"java.util",
	"java.io",
	"java.nio",
	"java.net",
	"java.math",
	// Date/time and text
	"java.time",
	"java.text",
	// Database and security
	"java.sql",
	"java.security",
	// Functional and concurrent
	"java.util.stream",
	"java.util.function",
	"java.util.concurrent",
	// javax
	"javax.annotation",
];

// -- Exported DTO --------------------------------------------------------

export const JAVA_RUNTIME_BUILTINS: RuntimeBuiltinsSet = {
	identifiers: Object.freeze(JAVA_STD_TYPES),
	moduleSpecifiers: Object.freeze(JAVA_STDLIB_MODULES),
};
