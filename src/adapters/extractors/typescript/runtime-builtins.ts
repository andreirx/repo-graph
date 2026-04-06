/**
 * TypeScript/JavaScript runtime builtins corpus.
 *
 * Language-specific data for the unresolved-edge classifier. Exported
 * as a RuntimeBuiltinsSet DTO via the ExtractorPort.runtimeBuiltins
 * field. The core classifier consumes it agnostically — it does not
 * know these names are TS/JS-specific.
 *
 * Two categories:
 *   1. `identifiers` — names globally available at runtime without
 *      any import statement (ES built-ins, Node globals, browser
 *      globals). These allow the classifier to recognize `new Map()`
 *      or `process.exit()` as runtime-global references.
 *   2. `moduleSpecifiers` — specifiers for stdlib/runtime modules
 *      that exist without being declared in package.json (Node
 *      stdlib with and without the `node:` prefix). These allow
 *      the classifier to recognize `import path from "path"` as a
 *      runtime module, not a missing dependency.
 *
 * Curation scope:
 *   - ES globals: value-bindable names from the ES specification.
 *     Does not include keywords (null, undefined, true, false).
 *   - Node globals: runtime-injected identifiers.
 *   - Browser globals: high-confidence operational APIs (window,
 *     fetch, URL, etc.). Does NOT include the broad DOM class
 *     hierarchy (HTMLElement, HTMLDivElement, etc.) — those are
 *     deferred to a later slice.
 *   - Node stdlib: the canonical module list. Each entry appears
 *     twice: bare name ("fs") and node-prefixed ("node:fs").
 *
 * Curation discipline:
 *   - Adding identifiers is non-breaking.
 *   - Removing identifiers that were previously persisted as
 *     classified rows is breaking and requires a
 *     CURRENT_CLASSIFIER_VERSION bump.
 */

import type { RuntimeBuiltinsSet } from "../../../core/classification/signals.js";

// ── ES built-in globals ─────────────────────────────────────────────

const ES_GLOBALS: readonly string[] = [
	// Fundamental objects
	"Object", "Function", "Boolean", "Symbol",
	// Error types
	"Error", "TypeError", "RangeError", "SyntaxError",
	"ReferenceError", "URIError", "EvalError", "AggregateError",
	// Numbers and math
	"Number", "BigInt", "Math", "NaN", "Infinity",
	// Dates
	"Date",
	// Strings + text
	"String", "RegExp",
	// Collections
	"Array", "Map", "Set", "WeakMap", "WeakSet", "WeakRef",
	// Typed arrays
	"Int8Array", "Uint8Array", "Uint8ClampedArray",
	"Int16Array", "Uint16Array", "Int32Array", "Uint32Array",
	"Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array",
	// Buffers
	"ArrayBuffer", "SharedArrayBuffer", "DataView",
	// Async + promise
	"Promise",
	// Structured data
	"JSON",
	// Meta-programming
	"Proxy", "Reflect",
	// Console
	"console",
	// Intl
	"Intl",
	// Global identity
	"globalThis",
	// Global functions
	"isNaN", "isFinite", "parseInt", "parseFloat",
	"encodeURI", "decodeURI", "encodeURIComponent", "decodeURIComponent",
	// Iteration
	"Iterator",
	// Finalization
	"FinalizationRegistry",
	// Atomics
	"Atomics",
];

// ── Node.js globals ─────────────────────────────────────────────────

const NODE_GLOBALS: readonly string[] = [
	"process", "Buffer", "global",
	"__dirname", "__filename",
	"setImmediate", "clearImmediate",
	"setTimeout", "clearTimeout",
	"setInterval", "clearInterval",
	"queueMicrotask", "structuredClone",
	"performance",
	"AbortController", "AbortSignal",
	"TextEncoder", "TextDecoder",
	"atob", "btoa",
];

// ── Browser globals (conservative) ──────────────────────────────────

const BROWSER_GLOBALS: readonly string[] = [
	"window", "document", "navigator", "location", "history",
	"fetch", "Request", "Response", "Headers",
	"FormData", "URL", "URLSearchParams",
	"Blob", "File", "FileReader",
	"localStorage", "sessionStorage",
	"XMLHttpRequest", "WebSocket", "EventSource",
	"alert",
	"requestAnimationFrame", "cancelAnimationFrame",
];

// ── Node stdlib module specifiers ───────────────────────────────────

const NODE_STDLIB_BARE: readonly string[] = [
	"assert", "assert/strict",
	"async_hooks",
	"buffer",
	"child_process",
	"cluster",
	"console",
	"constants",
	"crypto",
	"dgram",
	"diagnostics_channel",
	"dns", "dns/promises",
	"domain",
	"events",
	"fs", "fs/promises",
	"http", "http2", "https",
	"inspector", "inspector/promises",
	"module",
	"net",
	"os",
	"path", "path/posix", "path/win32",
	"perf_hooks",
	"process",
	"punycode",
	"querystring",
	"readline", "readline/promises",
	"repl",
	"stream", "stream/consumers", "stream/promises", "stream/web",
	"string_decoder",
	"test",
	"timers", "timers/promises",
	"tls",
	"trace_events",
	"tty",
	"url",
	"util", "util/types",
	"v8",
	"vm",
	"wasi",
	"worker_threads",
	"zlib",
];

// Duplicate each with `node:` prefix.
const NODE_STDLIB_SPECIFIERS: readonly string[] = [
	...NODE_STDLIB_BARE,
	...NODE_STDLIB_BARE.map((m) => `node:${m}`),
];

// ── Exported DTO ────────────────────────────────────────────────────

const IDENTIFIERS = Object.freeze([
	...ES_GLOBALS,
	...NODE_GLOBALS,
	...BROWSER_GLOBALS,
]);

const MODULE_SPECIFIERS = Object.freeze(NODE_STDLIB_SPECIFIERS);

export const TS_JS_RUNTIME_BUILTINS: RuntimeBuiltinsSet = {
	identifiers: IDENTIFIERS,
	moduleSpecifiers: MODULE_SPECIFIERS,
};
