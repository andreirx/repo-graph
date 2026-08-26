//! Deps-purpose runtime-builtin vocabulary (DEPS-LIST-REWRITE-1 §2.1).
//!
//! The `deps list` classifier ([`super::classify`]) needs to recognise language builtins /
//! globals / stdlib prefixes so they classify as builtins instead of leaking into the package
//! namespace (the audit's `Map`/`Set`/`Math.sqrt` for TS, `StringBuilder`/`java.util` for Java,
//! `AssertionError`/`asyncio` for Python). This module owns a curated, PACKAGE-NAME-LEVEL set per
//! ecosystem, selected by [`deps_runtime_builtins`] and injected through the existing
//! `ComposeDependenciesInput.runtime_builtins` seam.
//!
//! ## Why a purpose-specific set, not the extractors' sets
//!
//! The authoritative per-language sets (`ts_js_runtime_builtins`, …) live in the tree-sitter
//! extractor crates as PRIVATE items; exposing them would add four new public APIs (blocked by
//! this slice's scope rule) with no way to reuse without a dependency edge. These sets are ALSO
//! curated for a different job — symbol-level classification during indexing — whereas the deps
//! gate matches at the specifier/package level (dotted stdlib prefixes like `java.util`, receiver
//! heads like `Math`). A focused, documented duplicate is the correct, in-scope choice; the
//! divergence risk is accepted and noted here. It extends the pre-existing pattern in
//! `compose.rs` (which already owns `npm_runtime_builtins`/`cargo_runtime_builtins`).
//!
//! Axis of variation: ecosystem (npm / cargo / python / java). Sole current user: the daemon
//! `deps_list` dispatch arm (via the crate re-export).

use std::collections::HashSet;

use super::compose::{cargo_runtime_builtins, npm_runtime_builtins};

/// The injected builtin set for an ecosystem: identifiers ∪ module specifiers ∪ stdlib prefixes,
/// at the granularity the classifier matches (full token, normalized package, and dot-prefixes).
/// Unknown ecosystems (`none-detected`, explicit others) get an empty set — nothing is claimed a
/// builtin, and the classifier degrades to shape-only rejection.
pub fn deps_runtime_builtins(ecosystem: &str) -> HashSet<String> {
    match ecosystem {
        "npm" => {
            let mut s = npm_runtime_builtins();
            s.extend(JS_GLOBALS.iter().map(|x| x.to_string()));
            s
        }
        "cargo" => cargo_runtime_builtins(),
        "python" => PYTHON_BUILTINS
            .iter()
            .chain(PYTHON_STDLIB.iter())
            .map(|x| x.to_string())
            .collect(),
        "java" => JAVA_LANG_CLASSES
            .iter()
            .chain(JDK_PACKAGE_PREFIXES.iter())
            .map(|x| x.to_string())
            .collect(),
        _ => HashSet::new(),
    }
}

/// ECMAScript / Node / browser globals that appear as unresolved call receivers and would
/// otherwise be hoisted into the package namespace. Not exhaustive by design — the leak vectors
/// the audit named plus the common globals.
const JS_GLOBALS: &[&str] = &[
    // ES intrinsics
    "Object",
    "Array",
    "String",
    "Number",
    "Boolean",
    "BigInt",
    "Symbol",
    "Math",
    "JSON",
    "Date",
    "RegExp",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Promise",
    "Proxy",
    "Reflect",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "Function",
    "Infinity",
    "NaN",
    "isNaN",
    "isFinite",
    "parseInt",
    "parseFloat",
    "encodeURI",
    "decodeURI",
    "encodeURIComponent",
    "decodeURIComponent",
    "globalThis",
    "Intl",
    "ArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    // Node globals
    "process",
    "console",
    "Buffer",
    "global",
    "require",
    "module",
    "exports",
    "__dirname",
    "__filename",
    "setTimeout",
    "clearTimeout",
    "setInterval",
    "clearInterval",
    "setImmediate",
    "queueMicrotask",
    "structuredClone",
    // Common browser/runtime globals
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "fetch",
    "URL",
    "URLSearchParams",
    "Headers",
    "Request",
    "Response",
    "FormData",
    "Blob",
    "File",
    "FileReader",
    "WebSocket",
    "XMLHttpRequest",
    "localStorage",
    "sessionStorage",
    "atob",
    "btoa",
    "TextEncoder",
    "TextDecoder",
    "AbortController",
    "AbortSignal",
    "Event",
    "CustomEvent",
    "EventTarget",
];

/// Python builtins (the `builtins` namespace) that appear as unresolved call heads.
const PYTHON_BUILTINS: &[&str] = &[
    // builtin functions
    "print",
    "len",
    "range",
    "enumerate",
    "zip",
    "map",
    "filter",
    "sorted",
    "reversed",
    "sum",
    "min",
    "max",
    "abs",
    "round",
    "open",
    "input",
    "iter",
    "next",
    "isinstance",
    "issubclass",
    "getattr",
    "setattr",
    "hasattr",
    "delattr",
    "callable",
    "repr",
    "hash",
    "id",
    "type",
    "vars",
    "dir",
    "format",
    "super",
    "property",
    "staticmethod",
    "classmethod",
    "any",
    "all",
    // builtin types
    "dict",
    "list",
    "tuple",
    "set",
    "frozenset",
    "str",
    "bytes",
    "bytearray",
    "int",
    "float",
    "complex",
    "bool",
    "object",
    "slice",
    "memoryview",
    // builtin exceptions
    "Exception",
    "BaseException",
    "ValueError",
    "TypeError",
    "KeyError",
    "IndexError",
    "AttributeError",
    "RuntimeError",
    "StopIteration",
    "AssertionError",
    "NotImplementedError",
    "FileNotFoundError",
    "OSError",
    "IOError",
    "ImportError",
    "ModuleNotFoundError",
    "NameError",
    "ZeroDivisionError",
    "ArithmeticError",
    "OverflowError",
    "LookupError",
    "PermissionError",
];

/// Python standard-library top-level modules (leaked as fake packages when imported).
const PYTHON_STDLIB: &[&str] = &[
    "os",
    "sys",
    "re",
    "json",
    "math",
    "random",
    "datetime",
    "time",
    "collections",
    "itertools",
    "functools",
    "typing",
    "abc",
    "asyncio",
    "io",
    "pathlib",
    "subprocess",
    "threading",
    "multiprocessing",
    "logging",
    "unittest",
    "argparse",
    "copy",
    "pickle",
    "csv",
    "sqlite3",
    "socket",
    "struct",
    "hashlib",
    "hmac",
    "base64",
    "secrets",
    "uuid",
    "enum",
    "dataclasses",
    "contextlib",
    "warnings",
    "traceback",
    "inspect",
    "importlib",
    "gc",
    "weakref",
    "operator",
    "string",
    "textwrap",
    "decimal",
    "fractions",
    "statistics",
    "array",
    "bisect",
    "heapq",
    "queue",
    "shutil",
    "tempfile",
    "glob",
    "fnmatch",
    "stat",
    "signal",
    "select",
    "ssl",
    "http",
    "urllib",
    "email",
    "xml",
    "html",
    "gzip",
    "zipfile",
    "tarfile",
    "configparser",
    "getpass",
    "platform",
    "ctypes",
    "types",
    "keyword",
    "token",
    "tokenize",
    "ast",
    "dis",
    "codecs",
    "unicodedata",
    "locale",
    "gettext",
    "calendar",
    "zoneinfo",
    "concurrent",
    "wsgiref",
    // Additional stdlib top-level modules (imported directly, so they surface in the bucket).
    "atexit",
    "binascii",
    "builtins",
    "bz2",
    "lzma",
    "zlib",
    "compileall",
    "contextvars",
    "cProfile",
    "profile",
    "pstats",
    "difflib",
    "doctest",
    "errno",
    "faulthandler",
    "filecmp",
    "fileinput",
    "ftplib",
    "getopt",
    "graphlib",
    "imaplib",
    "ipaddress",
    "linecache",
    "mimetypes",
    "numbers",
    "pdb",
    "pkgutil",
    "posixpath",
    "ntpath",
    "genericpath",
    "pprint",
    "py_compile",
    "pyclbr",
    "quopri",
    "reprlib",
    "sched",
    "shelve",
    "shlex",
    "smtplib",
    "sysconfig",
    "timeit",
    "trace",
    "tracemalloc",
    "venv",
    "wave",
    "webbrowser",
    "xmlrpc",
    "zipapp",
    "zipimport",
    "dbm",
    "curses",
    "readline",
    "rlcompleter",
    "cmd",
    "code",
    "codeop",
    "pydoc",
    "poplib",
    "mailbox",
    "encodings",
    "site",
    "opcode",
    "marshal",
    "modulefinder",
    "runpy",
    "distutils",
    "ensurepip",
    "turtle",
    "tkinter",
    "colorsys",
    "fcntl",
    "grp",
    "pipes",
    "resource",
    "syslog",
    "termios",
    "tty",
    "pty",
    "msvcrt",
    "winreg",
    "winsound",
    "socketserver",
    "http",
    "xml",
    "html",
];

/// Java `java.lang` classes available without import (leaked as fake identifiers).
const JAVA_LANG_CLASSES: &[&str] = &[
    "Object",
    "String",
    "Integer",
    "Long",
    "Double",
    "Float",
    "Boolean",
    "Character",
    "Byte",
    "Short",
    "Number",
    "Math",
    "System",
    "Thread",
    "Runnable",
    "Exception",
    "RuntimeException",
    "Error",
    "Throwable",
    "IllegalArgumentException",
    "IllegalStateException",
    "NullPointerException",
    "IndexOutOfBoundsException",
    "ArrayIndexOutOfBoundsException",
    "ClassCastException",
    "UnsupportedOperationException",
    "NumberFormatException",
    "ArithmeticException",
    "StringBuilder",
    "StringBuffer",
    "CharSequence",
    "Comparable",
    "Iterable",
    "Class",
    "Enum",
    "Void",
    "Cloneable",
    "AutoCloseable",
    "Record",
    "Override",
    "Deprecated",
    "SuppressWarnings",
    "FunctionalInterface",
    "SafeVarargs",
];

/// JDK package prefixes (any dotted specifier under these is a stdlib usage, not a package).
const JDK_PACKAGE_PREFIXES: &[&str] = &[
    "java.lang",
    "java.util",
    "java.io",
    "java.nio",
    "java.net",
    "java.time",
    "java.text",
    "java.math",
    "java.security",
    "java.sql",
    "java.awt",
    "java.applet",
    "java.beans",
    "java.rmi",
    "java.lang.reflect",
    "java.lang.annotation",
    "java.util.concurrent",
    "java.util.function",
    "java.util.stream",
    "java.util.regex",
    "javax.swing",
    "javax.net",
    "javax.crypto",
    "javax.sql",
    "javax.xml",
    "javax.annotation",
    "javax.naming",
    "jdk",
    "sun",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_set_has_globals_and_node_modules() {
        let s = deps_runtime_builtins("npm");
        assert!(s.contains("Map"));
        assert!(s.contains("Math"));
        assert!(s.contains("Promise"));
        assert!(s.contains("fs"));
        assert!(s.contains("node:fs"));
    }

    #[test]
    fn python_set_has_stdlib_and_builtins() {
        let s = deps_runtime_builtins("python");
        assert!(s.contains("asyncio"));
        assert!(s.contains("os"));
        assert!(s.contains("AssertionError"));
    }

    #[test]
    fn java_set_has_lang_and_prefixes() {
        let s = deps_runtime_builtins("java");
        assert!(s.contains("StringBuilder"));
        assert!(s.contains("IllegalArgumentException"));
        assert!(s.contains("java.util"));
    }

    #[test]
    fn cargo_set_has_prelude() {
        let s = deps_runtime_builtins("cargo");
        assert!(s.contains("std"));
    }

    #[test]
    fn unknown_ecosystem_is_empty() {
        assert!(deps_runtime_builtins("none-detected").is_empty());
    }
}
