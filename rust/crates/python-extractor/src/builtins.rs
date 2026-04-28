//! Python runtime builtins corpus.
//!
//! Language-specific data for the unresolved-edge classifier. Exported
//! via `ExtractorPort::runtime_builtins()`. The core classifier consumes
//! it agnostically.
//!
//! Categories:
//!   1. `identifiers` -- built-in functions, types, and constants that
//!      are always available in Python without imports.
//!   2. `module_specifiers` -- Python standard library module names.
//!      The import classifier checks if an import specifier matches
//!      a known stdlib module.

use repo_graph_classification::types::RuntimeBuiltinsSet;

/// Python built-in functions, types, constants, and exceptions.
///
/// These are always available without imports. From Python 3.12 builtins.
const PYTHON_BUILTINS: &[&str] = &[
    // Built-in functions
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
    // Built-in constants
    "True",
    "False",
    "None",
    "Ellipsis",
    "NotImplemented",
    "__debug__",
    "__name__",
    "__doc__",
    "__package__",
    "__loader__",
    "__spec__",
    "__file__",
    "__cached__",
    "__builtins__",
    // Built-in exceptions
    "BaseException",
    "Exception",
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "EnvironmentError",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "NotADirectoryError",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
];

/// Python standard library module names.
///
/// Top-level modules from Python 3.12 standard library.
/// The import classifier checks if an import specifier starts with
/// one of these names.
const PYTHON_STDLIB_MODULES: &[&str] = &[
    // Text Processing
    "string",
    "re",
    "difflib",
    "textwrap",
    "unicodedata",
    "stringprep",
    "readline",
    "rlcompleter",
    // Binary Data
    "struct",
    "codecs",
    // Data Types
    "datetime",
    "zoneinfo",
    "calendar",
    "collections",
    "heapq",
    "bisect",
    "array",
    "weakref",
    "types",
    "copy",
    "pprint",
    "reprlib",
    "enum",
    "graphlib",
    // Numeric and Math
    "numbers",
    "math",
    "cmath",
    "decimal",
    "fractions",
    "random",
    "statistics",
    // Functional Programming
    "itertools",
    "functools",
    "operator",
    // File and Directory Access
    "pathlib",
    "fileinput",
    "stat",
    "filecmp",
    "tempfile",
    "glob",
    "fnmatch",
    "linecache",
    "shutil",
    // Data Persistence
    "pickle",
    "copyreg",
    "shelve",
    "marshal",
    "dbm",
    "sqlite3",
    // Data Compression
    "zlib",
    "gzip",
    "bz2",
    "lzma",
    "zipfile",
    "tarfile",
    // File Formats
    "csv",
    "configparser",
    "tomllib",
    "netrc",
    "plistlib",
    // Cryptographic
    "hashlib",
    "hmac",
    "secrets",
    // OS Services
    "os",
    "io",
    "time",
    "argparse",
    "getopt",
    "logging",
    "getpass",
    "curses",
    "platform",
    "errno",
    "ctypes",
    // Concurrent Execution
    "threading",
    "multiprocessing",
    "concurrent",
    "subprocess",
    "sched",
    "queue",
    "contextvars",
    // Networking
    "asyncio",
    "socket",
    "ssl",
    "select",
    "selectors",
    "signal",
    "mmap",
    // Internet Data Handling
    "email",
    "json",
    "mailbox",
    "mimetypes",
    "base64",
    "binascii",
    "quopri",
    // HTML/XML
    "html",
    "xml",
    // Internet Protocols
    "webbrowser",
    "wsgiref",
    "urllib",
    "http",
    "ftplib",
    "poplib",
    "imaplib",
    "smtplib",
    "uuid",
    "socketserver",
    "xmlrpc",
    "ipaddress",
    // Multimedia
    "wave",
    "colorsys",
    // Internationalization
    "gettext",
    "locale",
    // Program Frameworks
    "turtle",
    "cmd",
    "shlex",
    // GUI
    "tkinter",
    // Development Tools
    "typing",
    "pydoc",
    "doctest",
    "unittest",
    "test",
    // Debugging
    "bdb",
    "faulthandler",
    "pdb",
    "timeit",
    "trace",
    "tracemalloc",
    // Runtime
    "sys",
    "sysconfig",
    "builtins",
    "__main__",
    "warnings",
    "dataclasses",
    "contextlib",
    "abc",
    "atexit",
    "traceback",
    "__future__",
    "gc",
    "inspect",
    "site",
    // Importing
    "importlib",
    "zipimport",
    "pkgutil",
    "modulefinder",
    "runpy",
    // Python Language Services
    "ast",
    "symtable",
    "token",
    "keyword",
    "tokenize",
    "tabnanny",
    "pyclbr",
    "py_compile",
    "compileall",
    "dis",
    "pickletools",
    // Common third-party that feels like stdlib
    // (not included - keep this list pure stdlib)
];

/// Build the RuntimeBuiltinsSet for Python.
pub fn python_runtime_builtins() -> RuntimeBuiltinsSet {
    RuntimeBuiltinsSet {
        identifiers: PYTHON_BUILTINS.iter().map(|s| s.to_string()).collect(),
        module_specifiers: PYTHON_STDLIB_MODULES.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_contains_print() {
        let builtins = python_runtime_builtins();
        assert!(builtins.identifiers.contains(&"print".to_string()));
    }

    #[test]
    fn builtins_contains_dict() {
        let builtins = python_runtime_builtins();
        assert!(builtins.identifiers.contains(&"dict".to_string()));
    }

    #[test]
    fn builtins_contains_exception() {
        let builtins = python_runtime_builtins();
        assert!(builtins.identifiers.contains(&"Exception".to_string()));
    }

    #[test]
    fn stdlib_contains_os() {
        let builtins = python_runtime_builtins();
        assert!(builtins.module_specifiers.contains(&"os".to_string()));
    }

    #[test]
    fn stdlib_contains_json() {
        let builtins = python_runtime_builtins();
        assert!(builtins.module_specifiers.contains(&"json".to_string()));
    }

    #[test]
    fn stdlib_contains_typing() {
        let builtins = python_runtime_builtins();
        assert!(builtins.module_specifiers.contains(&"typing".to_string()));
    }
}
