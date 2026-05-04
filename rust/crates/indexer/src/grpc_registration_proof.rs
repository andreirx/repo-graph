//! GR-1B: gRPC registration proof detection.
//!
//! Detects `addService()` / `bindService()` calls that register gRPC service
//! implementations with a server. Boosts confidence of matching GR-1A surfaces
//! from 0.85 to 0.90.
//!
//! This is hint-strengthening, not a new discovery surface.

use regex::Regex;

use crate::storage_port::{GrpcRegistrationProofPort, RegistrationSiteInput};

use serde::{Deserialize, Serialize};

/// Result of GR-1B registration proof pass.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcRegistrationProofResult {
    /// Number of addService/bindService calls found.
    pub calls_found: usize,
    /// Number of calls that matched existing GR-1A surfaces.
    pub calls_matched: usize,
    /// Number of surfaces boosted.
    pub surfaces_boosted: usize,
    /// Errors encountered during processing.
    pub errors: Vec<String>,
}

/// Run GR-1B registration proof detection.
///
/// 1. Query for addService/bindService calls
/// 2. Extract class names from call patterns
/// 3. Match to existing GR-1A surfaces
/// 4. Boost confidence and append registration evidence
pub fn run_grpc_registration_proof<S>(
    storage: &mut S,
    snapshot_uid: &str,
) -> GrpcRegistrationProofResult
where
    S: GrpcRegistrationProofPort,
    <S as GrpcRegistrationProofPort>::Error: ToString,
{
    let mut result = GrpcRegistrationProofResult::default();

    // Query for addService/bindService calls
    let calls = match storage.query_add_service_calls(snapshot_uid) {
        Ok(c) => c,
        Err(e) => {
            result.errors.push(format!("Failed to query add_service calls: {}", e));
            return result;
        }
    };

    result.calls_found = calls.len();

    // Regex to extract class name from patterns like:
    // - "addService(new GreeterImpl())"
    // - "addService(greeterImpl)"
    // - ".addService(new FooImpl())"
    let class_pattern = Regex::new(r"(?:addService|bindService)\s*\(\s*(?:new\s+)?(\w+)").unwrap();

    for call in &calls {
        // Extract class name from call pattern
        let class_name = match extract_class_name(&class_pattern, &call.call_pattern) {
            Some(name) => name,
            None => {
                // Could not extract class name - skip this call
                continue;
            }
        };

        // Find matching GR-1A surface with source file context for disambiguation
        let surface = match storage.find_grpc_impl_surface_by_class(
            snapshot_uid,
            &class_name,
            Some(&call.source_file),
        ) {
            Ok(Some(s)) => s,
            Ok(None) => {
                // No matching surface - this might be a registration of
                // a non-ImplBase class or unresolvable reference
                continue;
            }
            Err(e) => {
                result.errors.push(format!(
                    "Failed to find surface for {}: {}",
                    class_name,
                    e
                ));
                continue;
            }
        };

        result.calls_matched += 1;

        // Build registration site evidence
        let site = RegistrationSiteInput {
            file: call.source_file.clone(),
            line: call.line_start.unwrap_or(0),
            method: call.source_method_name.clone(),
            pattern: extract_short_pattern(&call.call_pattern),
        };

        // Boost confidence
        match storage.boost_grpc_impl_confidence(&surface.surface_uid, &site) {
            Ok(true) => {
                result.surfaces_boosted += 1;
            }
            Ok(false) => {
                // Surface not updated (maybe already boosted or not found)
            }
            Err(e) => {
                result.errors.push(format!(
                    "Failed to boost surface {}: {}",
                    surface.surface_uid,
                    e
                ));
            }
        }
    }

    result
}

/// Extract class name from a call pattern like "addService(new GreeterImpl())".
fn extract_class_name(pattern: &Regex, call_pattern: &str) -> Option<String> {
    pattern
        .captures(call_pattern)
        .and_then(|caps: regex::Captures| caps.get(1))
        .map(|m: regex::Match| m.as_str().to_string())
}

/// Extract a short pattern for evidence (just the addService call, not the full chain).
fn extract_short_pattern(full_pattern: &str) -> String {
    // Find the addService/bindService portion
    let method_start = full_pattern
        .find("addService")
        .or_else(|| full_pattern.find("bindService"));

    if let Some(idx) = method_start {
        let rest = &full_pattern[idx..];
        // Find the matching closing paren by counting
        if let Some(open_idx) = rest.find('(') {
            let mut depth = 0;
            for (i, c) in rest[open_idx..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return rest[..open_idx + i + 1].to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
        return rest.to_string();
    }
    full_pattern.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_class_name_inline_instantiation() {
        let pattern = Regex::new(r"(?:addService|bindService)\s*\(\s*(?:new\s+)?(\w+)").unwrap();

        // Inline instantiation
        let result = extract_class_name(&pattern, "addService(new GreeterImpl())");
        assert_eq!(result, Some("GreeterImpl".to_string()));

        // Variable reference
        let result = extract_class_name(&pattern, "addService(greeterImpl)");
        assert_eq!(result, Some("greeterImpl".to_string()));

        // Full chain
        let result = extract_class_name(
            &pattern,
            "ServerBuilder.forPort(port).addService(new GreeterImpl()).build()",
        );
        assert_eq!(result, Some("GreeterImpl".to_string()));

        // bindService
        let result = extract_class_name(&pattern, "bindService(new FooServiceImpl())");
        assert_eq!(result, Some("FooServiceImpl".to_string()));
    }

    #[test]
    fn extract_short_pattern_works() {
        let full = "ServerBuilder.forPort(port).addService(new GreeterImpl()).build()";
        let short = extract_short_pattern(full);
        assert_eq!(short, "addService(new GreeterImpl())");

        let full2 = ".addService(foo)";
        let short2 = extract_short_pattern(full2);
        assert_eq!(short2, "addService(foo)");
    }
}
