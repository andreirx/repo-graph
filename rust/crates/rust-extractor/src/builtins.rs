//! Rust runtime builtins corpus.
//!
//! Language-specific data for the unresolved-edge classifier. Exported
//! via `ExtractorPort::runtime_builtins()`. The core classifier consumes
//! it agnostically.
//!
//! Categories:
//!   1. `identifiers` -- globally-available std types and macros that
//!      appear in Rust code without explicit `use` statements (prelude
//!      types, common macros).
//!   2. `module_specifiers` -- Rust standard library module paths.
//!      The import classifier checks if the first path segment of a
//!      `use` declaration matches a known stdlib module. For Rust,
//!      `std`, `core`, and `alloc` are the three stdlib crate roots.
//!
//! Mirrors the TS implementation at:
//!   src/adapters/extractors/rust/runtime-builtins.ts

use repo_graph_classification::types::RuntimeBuiltinsSet;

/// Prelude types, common std types, and macro names (without trailing !).
const RUST_STD_TYPES: &[&str] = &[
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

/// Rust stdlib module specifiers.
///
/// The three stdlib crate roots plus their top-level modules.
/// The import classifier checks the FIRST path segment of a `use`
/// declaration against this list.
const RUST_STDLIB_MODULES: &[&str] = &[
    // Crate roots
    "std",
    "core",
    "alloc",
    // Top-level std modules
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

/// Build the RuntimeBuiltinsSet for Rust.
pub fn rust_runtime_builtins() -> RuntimeBuiltinsSet {
    RuntimeBuiltinsSet {
        identifiers: RUST_STD_TYPES.iter().map(|s| s.to_string()).collect(),
        module_specifiers: RUST_STDLIB_MODULES.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_contains_vec() {
        let builtins = rust_runtime_builtins();
        assert!(builtins.identifiers.contains(&"Vec".to_string()));
    }

    #[test]
    fn builtins_contains_std() {
        let builtins = rust_runtime_builtins();
        assert!(builtins.module_specifiers.contains(&"std".to_string()));
    }

    #[test]
    fn builtins_contains_core() {
        let builtins = rust_runtime_builtins();
        assert!(builtins.module_specifiers.contains(&"core".to_string()));
    }
}
