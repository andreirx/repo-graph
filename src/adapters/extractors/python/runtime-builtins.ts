/**
 * Python runtime builtins corpus.
 *
 * Language-specific data for the unresolved-edge classifier. Exported
 * as a RuntimeBuiltinsSet DTO via the ExtractorPort.runtimeBuiltins
 * field. The core classifier consumes it agnostically.
 *
 * Categories:
 *   1. `identifiers` — Python builtins available globally without import:
 *      built-in functions (print, len, range, etc.), built-in types
 *      (int, str, list, dict, etc.), built-in exceptions, constants
 *      (True, False, None).
 *   2. `moduleSpecifiers` — Python standard library module names.
 *      The import classifier checks if an import specifier matches a
 *      known stdlib module to classify it as runtime rather than
 *      external or internal.
 *
 * Scope: CPython 3.10+ builtins and commonly-used stdlib modules.
 * Does NOT cover every stdlib module — focuses on modules that appear
 * frequently in real codebases to minimize false unknown classifications.
 */

import type { RuntimeBuiltinsSet } from "../../../core/classification/signals.js";

// -- Built-in functions and types -------------------------------------------

const PYTHON_BUILTINS: readonly string[] = [
	// Built-in functions
	"abs", "all", "any", "ascii", "bin", "bool", "breakpoint", "bytearray",
	"bytes", "callable", "chr", "classmethod", "compile", "complex",
	"delattr", "dict", "dir", "divmod", "enumerate", "eval", "exec",
	"filter", "float", "format", "frozenset", "getattr", "globals",
	"hasattr", "hash", "help", "hex", "id", "input", "int", "isinstance",
	"issubclass", "iter", "len", "list", "locals", "map", "max",
	"memoryview", "min", "next", "object", "oct", "open", "ord", "pow",
	"print", "property", "range", "repr", "reversed", "round", "set",
	"setattr", "slice", "sorted", "staticmethod", "str", "sum", "super",
	"tuple", "type", "vars", "zip",
	// Built-in constants
	"True", "False", "None", "NotImplemented", "Ellipsis", "__name__",
	"__file__", "__doc__", "__package__", "__spec__",
	// Built-in exceptions (commonly referenced)
	"Exception", "ValueError", "TypeError", "KeyError", "IndexError",
	"AttributeError", "ImportError", "FileNotFoundError", "OSError",
	"RuntimeError", "StopIteration", "NotImplementedError", "IOError",
	"PermissionError", "TimeoutError", "ConnectionError",
];

// -- Standard library modules -----------------------------------------------

const PYTHON_STDLIB_MODULES: readonly string[] = [
	// Core
	"sys", "os", "io", "abc", "typing", "types", "collections",
	"functools", "itertools", "operator", "contextlib", "copy",
	"dataclasses", "enum", "warnings",
	// Data structures / algorithms
	"heapq", "bisect", "array", "queue", "struct",
	// String / text
	"re", "string", "textwrap", "unicodedata", "difflib",
	// Math / numeric
	"math", "decimal", "fractions", "random", "statistics",
	// File / path
	"pathlib", "os.path", "shutil", "glob", "fnmatch", "tempfile",
	"fileinput",
	// Serialization
	"json", "csv", "pickle", "shelve", "xml", "html",
	"configparser", "tomllib",
	// Date / time
	"datetime", "time", "calendar", "zoneinfo",
	// Networking / web
	"http", "http.client", "http.server", "urllib", "urllib.parse",
	"urllib.request", "socket", "ssl", "email", "smtplib",
	// Async
	"asyncio", "concurrent", "concurrent.futures", "threading",
	"multiprocessing", "subprocess",
	// Crypto / hashing
	"hashlib", "hmac", "secrets",
	// Logging / debugging
	"logging", "traceback", "pdb", "inspect", "dis",
	// Testing
	"unittest", "doctest", "unittest.mock",
	// Compression
	"gzip", "zipfile", "tarfile", "lzma", "bz2", "zlib",
	// Database
	"sqlite3", "dbm",
	// Misc
	"argparse", "getopt", "signal", "atexit", "weakref",
	"pprint", "textwrap", "locale", "gettext",
	// Import system
	"importlib", "pkgutil",
];

export const PYTHON_RUNTIME_BUILTINS: RuntimeBuiltinsSet = {
	identifiers: Object.freeze(PYTHON_BUILTINS),
	moduleSpecifiers: Object.freeze(PYTHON_STDLIB_MODULES),
};
