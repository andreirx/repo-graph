//! Java runtime builtins corpus.
//!
//! Two categories:
//!   1. `identifiers` — class names globally available without import
//!      (java.lang package is implicitly imported).
//!   2. `module_specifiers` — package prefixes that exist as part of
//!      the JDK runtime (java.*, javax.*, etc.).
//!
//! These are used by the classifier to distinguish JDK symbols from
//! project symbols in unresolved-edge verdicts.

use repo_graph_classification::types::RuntimeBuiltinsSet;

// ── java.lang auto-imported classes ──────────────────────────────
//
// Classes in java.lang are implicitly imported and can be referenced
// without an import statement.

const JAVA_LANG_CLASSES: &[&str] = &[
    // Core object types
    "Object", "Class", "ClassLoader",
    // Primitive wrappers
    "Boolean", "Byte", "Character", "Short", "Integer", "Long", "Float", "Double",
    "Number", "Void",
    // String types
    "String", "StringBuffer", "StringBuilder", "CharSequence",
    // Exceptions and errors
    "Throwable", "Exception", "Error",
    "RuntimeException", "NullPointerException", "IllegalArgumentException",
    "IllegalStateException", "IndexOutOfBoundsException", "ArrayIndexOutOfBoundsException",
    "StringIndexOutOfBoundsException", "UnsupportedOperationException",
    "ArithmeticException", "ClassCastException", "ClassNotFoundException",
    "CloneNotSupportedException", "InterruptedException", "NoSuchMethodException",
    "NoSuchFieldException", "SecurityException",
    "OutOfMemoryError", "StackOverflowError", "AssertionError",
    "LinkageError", "NoClassDefFoundError", "ExceptionInInitializerError",
    // System
    "System", "Runtime", "Process", "ProcessBuilder",
    "Thread", "ThreadGroup", "ThreadLocal", "InheritableThreadLocal",
    "Runnable",
    // Math
    "Math", "StrictMath",
    // Reflection
    "Package", "Module",
    // Annotations
    "Override", "Deprecated", "SuppressWarnings", "SafeVarargs", "FunctionalInterface",
    // Records (Java 14+)
    "Record",
    // Enums
    "Enum",
    // Misc
    "Comparable", "Cloneable", "Iterable", "AutoCloseable",
    "Appendable", "Readable", "StackTraceElement",
];

// ── JDK package prefixes ─────────────────────────────────────────
//
// Import statements starting with these prefixes are JDK stdlib,
// not project dependencies.

const JDK_PACKAGE_PREFIXES: &[&str] = &[
    // Core Java
    "java.lang",
    "java.util",
    "java.io",
    "java.nio",
    "java.net",
    "java.time",
    "java.math",
    "java.text",
    "java.security",
    "java.sql",
    "java.beans",
    "java.awt",
    "java.applet",
    "java.rmi",
    "java.concurrent",
    // javax
    "javax.swing",
    "javax.sql",
    "javax.xml",
    "javax.crypto",
    "javax.net",
    "javax.security",
    "javax.sound",
    "javax.imageio",
    "javax.print",
    "javax.naming",
    "javax.management",
    "javax.annotation",
    // Jakarta EE (successor to javax for EE APIs)
    "jakarta.servlet",
    "jakarta.persistence",
    "jakarta.validation",
    "jakarta.inject",
    "jakarta.enterprise",
    // Sun/Oracle internal (legacy, but commonly seen)
    "sun.misc",
    "com.sun.net",
];

/// Build the full runtime builtins set for the Java ecosystem.
pub fn java_runtime_builtins() -> RuntimeBuiltinsSet {
    let identifiers: Vec<String> = JAVA_LANG_CLASSES
        .iter()
        .map(|s| s.to_string())
        .collect();

    let module_specifiers: Vec<String> = JDK_PACKAGE_PREFIXES
        .iter()
        .map(|s| s.to_string())
        .collect();

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
        let b = java_runtime_builtins();
        // JAVA_LANG_CLASSES has ~60 entries
        assert!(
            b.identifiers.len() >= 50,
            "expected >= 50 identifiers, got {}",
            b.identifiers.len()
        );
        // JDK_PACKAGE_PREFIXES has ~30 entries
        assert!(
            b.module_specifiers.len() >= 20,
            "expected >= 20 module specifiers, got {}",
            b.module_specifiers.len()
        );
    }

    #[test]
    fn builtins_include_key_entries() {
        let b = java_runtime_builtins();
        assert!(b.identifiers.contains(&"String".to_string()));
        assert!(b.identifiers.contains(&"Object".to_string()));
        assert!(b.identifiers.contains(&"System".to_string()));
        assert!(b.identifiers.contains(&"Exception".to_string()));
        assert!(b.module_specifiers.contains(&"java.util".to_string()));
        assert!(b.module_specifiers.contains(&"java.io".to_string()));
    }
}
