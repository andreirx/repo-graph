/**
 * Rust runtime builtins corpus.
 *
 * Language-specific data for the unresolved-edge classifier. Exported
 * as a RuntimeBuiltinsSet DTO via the ExtractorPort.runtimeBuiltins
 * field. The core classifier consumes it agnostically.
 *
 * Categories:
 *   1. `identifiers` -- globally-available std types and macros that
 *      appear in Rust code without explicit `use` statements (prelude
 *      types, common macros).
 *   2. `moduleSpecifiers` -- Rust standard library module paths.
 *      The import classifier checks if the first path segment of a
 *      `use` declaration matches a known stdlib module. For Rust,
 *      `std`, `core`, and `alloc` are the three stdlib crate roots.
 *      Individual module paths (e.g. `std::collections`) are also
 *      listed so the classifier can match `use std::io;` imports.
 *
 * Scope: Rust prelude types (Vec, String, Option, Result, Box, etc.)
 * and commonly-used std types that appear in nearly every codebase.
 * Also includes standard macros (println!, format!, etc.) which
 * are listed without the trailing `!` since tree-sitter strips it.
 */

import type { RuntimeBuiltinsSet } from "../../../core/classification/signals.js";

// -- Prelude and common std types ----------------------------------------

const RUST_STD_TYPES: readonly string[] = [
	// Prelude types
	"Vec",
	"String",
	"Option",
	"Result",
	"Box",
	"Some",
	"None",
	"Ok",
	"Err",
	"Copy",
	"Clone",
	"Debug",
	"Default",
	"PartialEq",
	"Eq",
	"PartialOrd",
	"Ord",
	"Hash",
	"Display",
	"Iterator",
	"Into",
	"From",
	"TryInto",
	"TryFrom",
	"AsRef",
	"AsMut",
	"Send",
	"Sync",
	"Sized",
	"Unpin",
	"Drop",
	"Fn",
	"FnMut",
	"FnOnce",
	"ToOwned",
	"ToString",
	// Smart pointers and concurrency
	"Rc",
	"Arc",
	"Cell",
	"RefCell",
	"Mutex",
	"RwLock",
	// Collections
	"HashMap",
	"HashSet",
	"BTreeMap",
	"BTreeSet",
	"VecDeque",
	"LinkedList",
	"BinaryHeap",
	// IO
	"Read",
	"Write",
	"Seek",
	"BufRead",
	"BufReader",
	"BufWriter",
	// Common macros (without trailing !)
	"println",
	"eprintln",
	"print",
	"eprint",
	"format",
	"write",
	"writeln",
	"todo",
	"unimplemented",
	"unreachable",
	"panic",
	"assert",
	"assert_eq",
	"assert_ne",
	"dbg",
	"cfg",
	"env",
	"vec",
	"include_str",
	"include_bytes",
	"concat",
	"stringify",
	"file",
	"line",
	"column",
];

// -- Rust stdlib module specifiers ----------------------------------------
// The three stdlib crate roots plus their top-level modules.
// The import classifier checks the FIRST path segment of a `use`
// declaration against this list. E.g. `use std::collections::HashMap`
// → first segment "std" matches → external/runtime.

const RUST_STDLIB_MODULES: readonly string[] = [
	// Crate roots
	"std",
	"core",
	"alloc",
	// Top-level std modules (commonly appear as first segment in use paths)
	"std::collections",
	"std::io",
	"std::fs",
	"std::path",
	"std::env",
	"std::fmt",
	"std::sync",
	"std::thread",
	"std::time",
	"std::net",
	"std::process",
	"std::os",
	"std::cell",
	"std::rc",
	"std::any",
	"std::borrow",
	"std::cmp",
	"std::convert",
	"std::default",
	"std::error",
	"std::hash",
	"std::iter",
	"std::marker",
	"std::mem",
	"std::num",
	"std::ops",
	"std::option",
	"std::result",
	"std::slice",
	"std::str",
	"std::string",
	"std::vec",
	"std::boxed",
	"std::pin",
	"std::future",
	"std::task",
	// core equivalents
	"core::fmt",
	"core::ops",
	"core::iter",
	"core::option",
	"core::result",
	"core::marker",
	"core::mem",
	"core::cell",
	"core::pin",
	"core::future",
	// alloc equivalents
	"alloc::vec",
	"alloc::string",
	"alloc::boxed",
	"alloc::collections",
];

// -- Exported DTO --------------------------------------------------------

export const RUST_RUNTIME_BUILTINS: RuntimeBuiltinsSet = {
	identifiers: Object.freeze(RUST_STD_TYPES),
	moduleSpecifiers: Object.freeze(RUST_STDLIB_MODULES),
};
