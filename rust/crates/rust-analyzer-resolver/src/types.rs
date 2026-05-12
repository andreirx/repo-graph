//! Type extraction from rust-analyzer hover responses.
//!
//! **Provisional heuristics.** These patterns are bootstrapped from
//! observed rust-analyzer behavior and may need refinement based on
//! real-repo validation.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Extract a type name from rust-analyzer hover markdown.
///
/// rust-analyzer returns hover info as markdown with ```rust code blocks.
/// This function extracts the type name from various patterns:
///
/// - Type annotations: `name: Type` or `let name: Type`
/// - Struct/enum/trait definitions: `pub struct MyType`
/// - Reference types: `&Type` or `&mut Type`
/// - Plain type names: `MyType`
///
/// **Provisional:** This is heuristic-based and may miss edge cases.
pub fn extract_type_from_hover(hover_text: &str) -> Option<String> {
    // Strip markdown code fences
    let stripped = hover_text
        .replace("```rust\n", "")
        .replace("```rust", "")
        .replace("```\n", "")
        .replace("```", "")
        .trim()
        .to_string();

    if stripped.is_empty() {
        return None;
    }

    // Pattern 1: Type annotation — "name: Type" or "let name: Type"
    // Captures qualified paths like `crate::engine::EngineContext`
    if let Some(type_name) = extract_type_annotation(&stripped) {
        return Some(clean_type_name(&type_name));
    }

    // Pattern 2: Struct/enum/trait/type definition header
    if let Some(caps) = extract_definition_header(&stripped) {
        return Some(caps);
    }

    // Pattern 3: Reference type "&Type" or "&mut Type"
    if let Some(type_name) = extract_reference_type(&stripped) {
        return Some(type_name);
    }

    // Pattern 4: Plain type name (PascalCase)
    if let Some(type_name) = extract_plain_type(&stripped) {
        return Some(type_name);
    }

    None
}

/// Extract type from annotation pattern: `name: Type`
fn extract_type_annotation(text: &str) -> Option<String> {
    // Look for `: Type` pattern
    // Handle `&` and `&mut` prefixes
    // Handle qualified paths `a::b::Type`
    // Handle generics `Type<T>` (we strip the generic part)

    for line in text.lines() {
        let line = line.trim();

        // Find `: ` that indicates type annotation
        if let Some(colon_pos) = line.find(": ") {
            let after_colon = &line[colon_pos + 2..];
            let type_part = after_colon.trim();

            if !type_part.is_empty() {
                // Take until we hit something that ends the type
                // (comma, equals, semicolon, or end)
                let type_str: String = type_part
                    .chars()
                    .take_while(|c| !matches!(c, ',' | '=' | ';'))
                    .collect();

                let type_str = type_str.trim();
                if !type_str.is_empty() {
                    return Some(type_str.to_string());
                }
            }
        }
    }

    None
}

/// Extract type from definition header: `pub struct MyType`
fn extract_definition_header(text: &str) -> Option<String> {
    let patterns = ["struct ", "enum ", "trait ", "type "];

    for line in text.lines() {
        let line = line.trim();
        // Skip `pub ` prefix if present
        let line = line.strip_prefix("pub ").unwrap_or(line);

        for pattern in &patterns {
            if let Some(rest) = line.strip_prefix(pattern) {
                // Take the type name (identifier)
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();

                if !name.is_empty()
                    && name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                {
                    return Some(name);
                }
            }
        }
    }

    None
}

/// Extract type from reference: `&Type` or `&mut Type`
fn extract_reference_type(text: &str) -> Option<String> {
    let text = text.trim();

    // Strip leading &
    let text = text.strip_prefix('&')?;
    // Strip optional `mut `
    let text = text.strip_prefix("mut ").unwrap_or(text).trim();

    // Take identifier
    let name: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if !name.is_empty()
        && name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    {
        Some(name)
    } else {
        None
    }
}

/// Extract plain PascalCase type name
fn extract_plain_type(text: &str) -> Option<String> {
    let text = text.trim();

    // Must start with uppercase
    if !text
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        return None;
    }

    // Take identifier
    let name: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if name.len() >= 2 {
        Some(name)
    } else {
        None
    }
}

/// Clean a raw type string:
/// - Strip leading `&` / `&mut`
/// - Strip lifetime parameters `'a` (both inline `&'a T` and generic `<'a>`)
/// - Strip generic parameters (take base type)
/// - Take last segment of qualified path `a::b::Type` -> `Type`
fn clean_type_name(raw: &str) -> String {
    let mut name = raw.trim().to_string();

    // Strip reference markers
    if name.starts_with('&') {
        name = name[1..].trim().to_string();
    }
    if name.starts_with("mut ") {
        name = name[4..].trim().to_string();
    }

    // Strip inline lifetime after & (e.g., "'a str" -> "str")
    if name.starts_with('\'') {
        // Find end of lifetime: next space or identifier start
        if let Some(space_pos) = name.find(' ') {
            name = name[space_pos..].trim().to_string();
        }
    }

    // Strip lifetime-only generics like `<'a>` or `<'a, 'b>`
    // (but not type generics like `<T>`)
    while let Some(start) = name.find("<'") {
        if let Some(end) = name[start..].find('>') {
            let generic_content = &name[start + 1..start + end];
            // Check if it's only lifetimes
            if generic_content
                .split(',')
                .all(|part| part.trim().starts_with('\''))
            {
                name = format!("{}{}", &name[..start], &name[start + end + 1..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Strip type generics — take base type before `<`
    if let Some(angle_pos) = name.find('<') {
        name = name[..angle_pos].trim().to_string();
    }

    // For qualified paths, take the last segment
    if name.contains("::") {
        if let Some(last) = name.rsplit("::").next() {
            name = last.trim().to_string();
        }
    }

    name
}

/// Validate that a string is a plausible Rust type name.
///
/// Rejects:
/// - Rust keywords
/// - Single-letter identifiers (too ambiguous)
/// - Names that don't follow type conventions
///
/// Accepts:
/// - PascalCase names (2+ chars)
/// - Primitive types (i32, u64, bool, etc.)
///
/// **Provisional:** This is a heuristic filter.
pub fn is_valid_rust_type_name(name: &str) -> bool {
    if name.is_empty() || name.len() < 2 {
        return false;
    }

    // Reject keywords and common non-type tokens
    if REJECT_TOKENS.contains(name) {
        return false;
    }

    // Reject names with newlines (hover markdown leaks)
    if name.contains('\n') {
        return false;
    }

    // Accept primitives
    if PRIMITIVES.contains(name) {
        return true;
    }

    // Must start with uppercase (Rust type convention)
    name.chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

/// Check if a Rust type is external (from std or well-known crates).
///
/// **Provisional:** Currently only checks std types.
/// Future: could check against indexed crate dependencies.
pub fn is_external_type(type_name: &str) -> bool {
    STD_TYPES.contains(type_name)
}

// ─────────────────────────────────────────────────────────────────────────────
// Static token sets
// ─────────────────────────────────────────────────────────────────────────────

static REJECT_TOKENS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "self",
        "Self",
        "let",
        "mut",
        "fn",
        "pub",
        "mod",
        "use",
        "impl",
        "trait",
        "struct",
        "enum",
        "type",
        "const",
        "static",
        "return",
        "if",
        "else",
        "match",
        "for",
        "while",
        "loop",
        "break",
        "continue",
        "async",
        "await",
        "move",
        "ref",
        "where",
        "as",
        "in",
        "true",
        "false",
        "crate",
        "super",
        "any",
        "unknown",
        "{unknown}",
        "test",
        "def",
        "dyn",
        "unsafe",
        "extern",
    ]
    .into_iter()
    .collect()
});

static PRIMITIVES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "bool", "char", "str", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
        "u64", "u128", "usize", "f32", "f64",
    ]
    .into_iter()
    .collect()
});

static STD_TYPES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Collections
        "Vec",
        "String",
        "HashMap",
        "HashSet",
        "BTreeMap",
        "BTreeSet",
        "VecDeque",
        "LinkedList",
        "BinaryHeap",
        // Smart pointers
        "Box",
        "Rc",
        "Arc",
        "Cell",
        "RefCell",
        "Mutex",
        "RwLock",
        // Option/Result
        "Option",
        "Result",
        "Cow",
        // Path/OS
        "Path",
        "PathBuf",
        "OsString",
        "OsStr",
        // I/O
        "File",
        "BufReader",
        "BufWriter",
        // Network
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        // Time
        "Duration",
        "Instant",
        "SystemTime",
        // Process
        "Command",
        "Child",
        // Error
        "Error",
    ]
    .into_iter()
    .collect()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_annotation() {
        assert_eq!(
            extract_type_from_hover("receiver: Engine"),
            Some("Engine".to_string())
        );

        assert_eq!(
            extract_type_from_hover("let ctx: &EngineContext"),
            Some("EngineContext".to_string())
        );

        assert_eq!(
            extract_type_from_hover("field: crate::engine::Engine"),
            Some("Engine".to_string())
        );
    }

    #[test]
    fn test_extract_definition_header() {
        assert_eq!(
            extract_type_from_hover("pub struct MyClass"),
            Some("MyClass".to_string())
        );

        assert_eq!(
            extract_type_from_hover("enum Status"),
            Some("Status".to_string())
        );

        assert_eq!(
            extract_type_from_hover("trait Handler"),
            Some("Handler".to_string())
        );
    }

    #[test]
    fn test_extract_reference_type() {
        assert_eq!(
            extract_type_from_hover("&Engine"),
            Some("Engine".to_string())
        );

        assert_eq!(
            extract_type_from_hover("&mut Context"),
            Some("Context".to_string())
        );
    }

    #[test]
    fn test_extract_from_markdown() {
        let hover = "```rust\nfield: MyType\n```";
        assert_eq!(extract_type_from_hover(hover), Some("MyType".to_string()));
    }

    #[test]
    fn test_clean_type_name_generics() {
        assert_eq!(clean_type_name("Vec<T>"), "Vec");
        assert_eq!(clean_type_name("HashMap<K, V>"), "HashMap");
        assert_eq!(clean_type_name("&'a str"), "str");
    }

    #[test]
    fn test_clean_type_name_qualified() {
        assert_eq!(clean_type_name("std::vec::Vec"), "Vec");
        assert_eq!(clean_type_name("crate::engine::Engine"), "Engine");
    }

    #[test]
    fn test_is_valid_rust_type_name() {
        // Valid
        assert!(is_valid_rust_type_name("Engine"));
        assert!(is_valid_rust_type_name("MyClass"));
        assert!(is_valid_rust_type_name("i32"));
        assert!(is_valid_rust_type_name("Vec"));

        // Invalid
        assert!(!is_valid_rust_type_name("self"));
        assert!(!is_valid_rust_type_name("let"));
        assert!(!is_valid_rust_type_name("a")); // too short
        assert!(!is_valid_rust_type_name("foo")); // not PascalCase
    }

    #[test]
    fn test_is_external_type() {
        assert!(is_external_type("Vec"));
        assert!(is_external_type("String"));
        assert!(is_external_type("HashMap"));
        assert!(is_external_type("Arc"));

        assert!(!is_external_type("Engine"));
        assert!(!is_external_type("MyCustomType"));
    }
}
