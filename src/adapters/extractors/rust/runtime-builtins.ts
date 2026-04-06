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
 *   2. `moduleSpecifiers` -- empty. Rust's module system uses `use`
 *      declarations with crate paths, not free-standing module
 *      specifiers like Node.js.
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

// -- Exported DTO --------------------------------------------------------

export const RUST_RUNTIME_BUILTINS: RuntimeBuiltinsSet = {
	identifiers: Object.freeze(RUST_STD_TYPES),
	moduleSpecifiers: Object.freeze([]),
};
