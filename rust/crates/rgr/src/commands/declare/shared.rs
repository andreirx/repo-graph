//! Shared helpers for declare command family.
//!
//! # Boundary rules
//!
//! This module owns:
//! - shared flag parsing helpers
//! - shared identity/key extraction helpers
//! - shared constants
//!
//! This module does **not** own:
//! - command handlers (live in their respective files)
//! - shared CLI infrastructure (lives in `crate::cli`)

/// Valid comparison operators for requirement thresholds.
pub const VALID_OPERATORS: &[&str] = &[">=", ">", "<=", "<", "=="];

/// Parse a flag value that must not repeat.
///
/// Returns `None` and prints an error if:
/// - the flag is already set (repeated)
/// - the value is missing
/// - the value looks like another flag (starts with `-`)
/// - the value is empty after trimming
pub fn parse_flag_value<'a>(
    flag_name: &str,
    current: &Option<String>,
    args: &'a [String],
    i: &mut usize,
) -> Option<String> {
    if current.is_some() {
        eprintln!("error: {} specified more than once", flag_name);
        return None;
    }
    *i += 1;
    if *i >= args.len() || args[*i].starts_with('-') {
        eprintln!("error: {} requires a non-empty value", flag_name);
        return None;
    }
    let v = args[*i].trim().to_string();
    if v.is_empty() {
        eprintln!("error: {} requires a non-empty value", flag_name);
        return None;
    }
    Some(v)
}

/// Parse a repeatable flag value.
///
/// Unlike `parse_flag_value`, this does not check for repetition.
/// Returns `None` if the value is missing, looks like a flag, or is empty.
pub fn parse_repeatable_flag_value<'a>(
    flag_name: &str,
    args: &'a [String],
    i: &mut usize,
) -> Option<String> {
    *i += 1;
    if *i >= args.len() || args[*i].starts_with('-') {
        eprintln!("error: {} requires a non-empty value", flag_name);
        return None;
    }
    let v = args[*i].trim().to_string();
    if v.is_empty() {
        eprintln!("error: {} requires a non-empty value", flag_name);
        return None;
    }
    Some(v)
}

/// Extract module path from a MODULE stable key: `{repo}:{path}:MODULE`
///
/// Returns `None` if the key does not end with `:MODULE` or has no colon
/// separating repo from path.
pub fn extract_module_path_from_key(stable_key: &str) -> Option<String> {
    if !stable_key.ends_with(":MODULE") {
        return None;
    }
    let without_suffix = &stable_key[..stable_key.len() - ":MODULE".len()];
    let colon_pos = without_suffix.find(':')?;
    Some(without_suffix[colon_pos + 1..].to_string())
}
