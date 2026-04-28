//! Python tree-sitter extractor for repo-graph.
//!
//! This crate provides a concrete `ExtractorPort` implementation for
//! Python source files (`.py`). It uses the native tree-sitter binding
//! with the `tree-sitter-python` grammar, statically linked.
//!
//! # Extraction scope (v1)
//!
//! - **FILE nodes** for each `.py` file
//! - **SYMBOL nodes**: functions, classes, methods
//! - **IMPORTS edges**: `import x`, `from x import y`
//! - **CALLS edges**: function/method invocations
//! - **Docstrings**: extracted from function/class/method definitions
//!
//! # Not in scope (v1)
//!
//! - Framework detectors (FastAPI, Django, Flask)
//! - Decorators as liveness signals
//! - Type inference or stub resolution
//! - Complexity metrics

mod builtins;
mod extractor;

pub use extractor::PythonExtractor;
