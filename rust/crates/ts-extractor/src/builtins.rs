//! TypeScript/JavaScript runtime builtins corpus.
//!
//! Mirror of `src/adapters/extractors/typescript/runtime-builtins.ts`.
//!
//! Two categories:
//!   1. `identifiers` — names globally available at runtime without
//!      any import (ES built-ins, Node globals, browser globals).
//!   2. `module_specifiers` — stdlib/runtime module specifiers that
//!      exist without package.json declaration (Node stdlib, bare
//!      and `node:`-prefixed).

use repo_graph_classification::types::RuntimeBuiltinsSet;

// ── ES built-in globals ──────────────────────────────────────────

const ES_GLOBALS: &[&str] = &[
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

// ── Node.js globals ──────────────────────────────────────────────

const NODE_GLOBALS: &[&str] = &[
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

// ── Browser globals (conservative) ───────────────────────────────

const BROWSER_GLOBALS: &[&str] = &[
	"window", "document", "navigator", "location", "history",
	"fetch", "Request", "Response", "Headers",
	"FormData", "URL", "URLSearchParams",
	"Blob", "File", "FileReader",
	"localStorage", "sessionStorage",
	"XMLHttpRequest", "WebSocket", "EventSource",
	"alert",
	"requestAnimationFrame", "cancelAnimationFrame",
];

// ── Node stdlib module specifiers ────────────────────────────────

const NODE_STDLIB_BARE: &[&str] = &[
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

/// Build the full runtime builtins set for the TS/JS ecosystem.
pub fn ts_js_runtime_builtins() -> RuntimeBuiltinsSet {
	let mut identifiers: Vec<String> = Vec::new();
	identifiers.extend(ES_GLOBALS.iter().map(|s| s.to_string()));
	identifiers.extend(NODE_GLOBALS.iter().map(|s| s.to_string()));
	identifiers.extend(BROWSER_GLOBALS.iter().map(|s| s.to_string()));

	let mut module_specifiers: Vec<String> = Vec::new();
	for &m in NODE_STDLIB_BARE {
		module_specifiers.push(m.to_string());
		module_specifiers.push(format!("node:{}", m));
	}

	RuntimeBuiltinsSet {
		identifiers,
		module_specifiers,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn builtins_have_expected_counts() {
		let b = ts_js_runtime_builtins();
		// ES (42) + Node (18) + Browser (20) = 80 identifiers.
		assert!(
			b.identifiers.len() >= 70,
			"expected >= 70 identifiers, got {}",
			b.identifiers.len()
		);
		// NODE_STDLIB_BARE has ~47 entries, doubled = ~94 specifiers.
		assert!(
			b.module_specifiers.len() >= 80,
			"expected >= 80 module specifiers, got {}",
			b.module_specifiers.len()
		);
	}

	#[test]
	fn builtins_include_key_entries() {
		let b = ts_js_runtime_builtins();
		assert!(b.identifiers.contains(&"Map".to_string()));
		assert!(b.identifiers.contains(&"console".to_string()));
		assert!(b.identifiers.contains(&"fetch".to_string()));
		assert!(b.identifiers.contains(&"process".to_string()));
		assert!(b.module_specifiers.contains(&"fs".to_string()));
		assert!(b.module_specifiers.contains(&"node:fs".to_string()));
		assert!(b.module_specifiers.contains(&"path".to_string()));
		assert!(b.module_specifiers.contains(&"node:path".to_string()));
	}
}
