//! Validation tests against swupdate corelib.
//!
//! These tests validate policy-fact extraction on the real swupdate codebase:
//! - PF-1: STATUS_MAPPING (docs/slices/pf-1-status-mapping.md)
//! - PF-2: BEHAVIORAL_MARKER (docs/slices/pf-2-behavioral-marker.md)

use repo_graph_policy_facts::extractors::behavioral_marker::extract_behavioral_markers;
use repo_graph_policy_facts::extractors::status_mapping::extract_status_mappings;
use repo_graph_policy_facts::{MarkerEvidence, MarkerKind};
use std::path::Path;

fn parse_c_file(source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
    parser.set_language(&language).unwrap();
    parser.parse(source, None).unwrap()
}

fn read_swupdate_file(relative_path: &str) -> Option<String> {
    // Try multiple possible paths
    let paths = [
        // From repo-graph/rust/crates/policy-facts relative path
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../legacy-codebases/swupdate")
            .join(relative_path),
        // Absolute path as fallback
        Path::new("/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/swupdate")
            .join(relative_path),
    ];

    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }
    None
}

#[test]
fn test_map_channel_retcode() {
    let source = match read_swupdate_file("corelib/server_utils.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let mappings = extract_status_mappings(&tree, source.as_bytes(), "corelib/server_utils.c", "swupdate");

    // Should find map_channel_retcode
    let map_channel = mappings.iter().find(|m| m.function_name == "map_channel_retcode");
    assert!(map_channel.is_some(), "map_channel_retcode not found");

    let m = map_channel.unwrap();

    // Validate types
    assert_eq!(m.source_type, "channel_op_res_t", "source type mismatch");
    assert_eq!(m.target_type, "server_op_res_t", "target type mismatch");

    // Validate mappings
    // Should have CHANNEL_ENONET -> SERVER_EAGAIN (with fallthrough siblings)
    let eagain_mapping = m.mappings.iter().find(|cm| cm.output == "SERVER_EAGAIN");
    assert!(eagain_mapping.is_some(), "SERVER_EAGAIN mapping not found");

    let eagain = eagain_mapping.unwrap();
    assert!(
        eagain.inputs.contains(&"CHANNEL_ENONET".to_string()),
        "CHANNEL_ENONET not in SERVER_EAGAIN mapping"
    );
    assert!(
        eagain.inputs.contains(&"CHANNEL_EAGAIN".to_string()),
        "CHANNEL_EAGAIN not in SERVER_EAGAIN mapping (fallthrough)"
    );

    // Validate default
    assert_eq!(m.default_output, Some("SERVER_EERR".to_string()), "default mismatch");

    // Print summary for debugging
    eprintln!("map_channel_retcode: {} mappings, default={:?}",
        m.mappings.len(), m.default_output);
    for cm in &m.mappings {
        eprintln!("  {:?} -> {}", cm.inputs, cm.output);
    }
}

#[test]
fn test_channel_map_curl_error() {
    let source = match read_swupdate_file("corelib/channel_curl.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let mappings = extract_status_mappings(&tree, source.as_bytes(), "corelib/channel_curl.c", "swupdate");

    // Should find channel_map_curl_error
    let curl_error = mappings.iter().find(|m| m.function_name == "channel_map_curl_error");
    assert!(curl_error.is_some(), "channel_map_curl_error not found");

    let m = curl_error.unwrap();

    // Validate types
    assert_eq!(m.source_type, "CURLcode", "source type mismatch");
    assert_eq!(m.target_type, "channel_op_res_t", "target type mismatch");

    // ── P2: Validate actual mapping semantics ─────────────────────────
    //
    // These assertions catch preprocessor-boundary bugs where case groups
    // are incorrectly merged. If the extractor doesn't walk #if blocks
    // inside switch bodies, SSL init errors get merged with network errors.

    // 1. CURLE_COULDNT_RESOLVE_HOST must map to CHANNEL_ENONET (network error)
    let enonet_mapping = m.mappings.iter().find(|cm| cm.output == "CHANNEL_ENONET");
    assert!(enonet_mapping.is_some(), "CHANNEL_ENONET mapping not found");
    let enonet = enonet_mapping.unwrap();
    assert!(
        enonet.inputs.contains(&"CURLE_COULDNT_RESOLVE_HOST".to_string()),
        "CURLE_COULDNT_RESOLVE_HOST should map to CHANNEL_ENONET, got: {:?}",
        find_output_for_input(m, "CURLE_COULDNT_RESOLVE_HOST")
    );

    // 2. CURLE_SSL_ENGINE_INITFAILED must map to CHANNEL_EINIT (SSL init error)
    //    NOT merged with CHANNEL_ENONET
    let einit_mapping = m.mappings.iter().find(|cm| cm.output == "CHANNEL_EINIT");
    assert!(einit_mapping.is_some(), "CHANNEL_EINIT mapping not found");
    let einit = einit_mapping.unwrap();
    assert!(
        einit.inputs.contains(&"CURLE_SSL_ENGINE_INITFAILED".to_string()),
        "CURLE_SSL_ENGINE_INITFAILED should map to CHANNEL_EINIT, got: {:?}",
        find_output_for_input(m, "CURLE_SSL_ENGINE_INITFAILED")
    );

    // 3. CURLE_PEER_FAILED_VERIFICATION must map to CHANNEL_ESSLCERT
    //    NOT merged with CHANNEL_ESSLCONNECT
    let esslcert_mapping = m.mappings.iter().find(|cm| cm.output == "CHANNEL_ESSLCERT");
    assert!(esslcert_mapping.is_some(), "CHANNEL_ESSLCERT mapping not found");
    let esslcert = esslcert_mapping.unwrap();
    assert!(
        esslcert.inputs.contains(&"CURLE_PEER_FAILED_VERIFICATION".to_string()),
        "CURLE_PEER_FAILED_VERIFICATION should map to CHANNEL_ESSLCERT, got: {:?}",
        find_output_for_input(m, "CURLE_PEER_FAILED_VERIFICATION")
    );

    // 4. Verify CHANNEL_ENONET does NOT contain SSL init errors
    assert!(
        !enonet.inputs.contains(&"CURLE_SSL_ENGINE_INITFAILED".to_string()),
        "CHANNEL_ENONET wrongly contains SSL init error CURLE_SSL_ENGINE_INITFAILED"
    );
    assert!(
        !enonet.inputs.contains(&"CURLE_SSL_ENGINE_NOTFOUND".to_string()),
        "CHANNEL_ENONET wrongly contains SSL init error CURLE_SSL_ENGINE_NOTFOUND"
    );

    // 5. Default should be CHANNEL_EINIT
    assert_eq!(
        m.default_output,
        Some("CHANNEL_EINIT".to_string()),
        "default should be CHANNEL_EINIT"
    );

    // Print summary
    eprintln!("channel_map_curl_error: {} mappings, default={:?}",
        m.mappings.len(), m.default_output);
    for cm in &m.mappings {
        eprintln!("  {:?} -> {}", cm.inputs, cm.output);
    }
}

/// Helper to find which output a specific input maps to.
fn find_output_for_input(m: &repo_graph_policy_facts::StatusMapping, input: &str) -> Option<String> {
    for cm in &m.mappings {
        if cm.inputs.contains(&input.to_string()) {
            return Some(cm.output.clone());
        }
    }
    None
}

#[test]
fn test_channel_map_http_code() {
    let source = match read_swupdate_file("corelib/channel_curl.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let mappings = extract_status_mappings(&tree, source.as_bytes(), "corelib/channel_curl.c", "swupdate");

    // Should find channel_map_http_code
    let http_code = mappings.iter().find(|m| m.function_name == "channel_map_http_code");
    assert!(http_code.is_some(), "channel_map_http_code not found");

    let m = http_code.unwrap();

    // Validate types - source is long (from *http_response_code dereference)
    assert_eq!(m.source_type, "long", "source type mismatch");
    assert_eq!(m.target_type, "channel_op_res_t", "target type mismatch");

    // Should have case mappings for HTTP codes
    assert!(!m.mappings.is_empty(), "no case mappings found");

    // Check for some expected HTTP code mappings
    let has_200 = m.mappings.iter().any(|cm| cm.inputs.contains(&"200".to_string()));
    let has_401 = m.mappings.iter().any(|cm| cm.inputs.contains(&"401".to_string()));
    let has_404 = m.mappings.iter().any(|cm| cm.inputs.contains(&"404".to_string()));

    assert!(has_200 || has_401 || has_404, "expected HTTP code mappings not found");

    // Print summary
    eprintln!("channel_map_http_code: {} mappings, default={:?}",
        m.mappings.len(), m.default_output);
    for cm in &m.mappings {
        eprintln!("  {:?} -> {}", cm.inputs, cm.output);
    }
}

#[test]
fn test_no_false_positives_in_corelib() {
    // Verify we don't have excessive false positives
    let files = &[
        "corelib/server_utils.c",
        "corelib/channel_curl.c",
    ];

    let mut total_mappings = 0;

    for file in files {
        let source = match read_swupdate_file(file) {
            Some(s) => s,
            None => {
                eprintln!("SKIP: {} not found", file);
                return;
            }
        };

        let tree = parse_c_file(&source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), file, "swupdate");

        eprintln!("{}: {} STATUS_MAPPING functions", file, mappings.len());
        for m in &mappings {
            eprintln!("  - {} ({} -> {})", m.function_name, m.source_type, m.target_type);
        }

        total_mappings += mappings.len();
    }

    // We expect exactly 3 mappings in these two files:
    // - map_channel_retcode (server_utils.c)
    // - channel_map_curl_error (channel_curl.c)
    // - channel_map_http_code (channel_curl.c)
    //
    // If we get significantly more, we have false positives.
    assert!(
        total_mappings <= 5,
        "Too many mappings ({}) - likely false positives",
        total_mappings
    );
    assert!(
        total_mappings >= 3,
        "Missing expected mappings (got {})",
        total_mappings
    );
}

// =============================================================================
// PF-2: BEHAVIORAL_MARKER validation
// =============================================================================

#[test]
fn test_channel_get_file_retry_loops() {
    let source = match read_swupdate_file("corelib/channel_curl.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let markers =
        extract_behavioral_markers(&tree, source.as_bytes(), "corelib/channel_curl.c", "swupdate");

    // Filter to channel_get_file function markers
    let channel_get_file_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.function_name == "channel_get_file")
        .collect();

    // Should have at least 2 RETRY_LOOP markers (IPC retry + download retry)
    let retry_loops: Vec<_> = channel_get_file_markers
        .iter()
        .filter(|m| m.kind == MarkerKind::RetryLoop)
        .collect();

    assert_eq!(
        retry_loops.len(),
        2,
        "expected exactly 2 RETRY_LOOP markers in channel_get_file, got {}",
        retry_loops.len()
    );

    eprintln!(
        "channel_get_file RETRY_LOOP markers: {} found",
        retry_loops.len()
    );
    for m in &retry_loops {
        if let MarkerEvidence::RetryLoop {
            loop_kind,
            sleep_call,
            delay_ms,
            max_attempts,
            ..
        } = &m.evidence
        {
            eprintln!(
                "  lines {}-{}: {} loop, sleep={:?}, delay_ms={:?}, max_attempts={:?}",
                m.line_start, m.line_end, loop_kind, sleep_call, delay_ms, max_attempts
            );
        }
    }

    // ── IPC retry loop validation ─────────────────────────────────────────
    // Pattern: for (int retries = 3; retries >= 0; retries--) { ... sleep(1); }
    let ipc_retry = retry_loops.iter().find(|m| {
        if let MarkerEvidence::RetryLoop {
            loop_kind,
            delay_ms,
            max_attempts,
            ..
        } = &m.evidence
        {
            loop_kind == "for" && *delay_ms == Some(1000) && *max_attempts == Some(4)
        } else {
            false
        }
    });

    assert!(
        ipc_retry.is_some(),
        "IPC retry loop not found: expected for loop with delay_ms=1000, max_attempts=4"
    );

    // ── Download retry loop validation ────────────────────────────────────
    // Pattern: do { ... sleep(channel_data->retry_sleep); } while (...)
    let download_retry = retry_loops.iter().find(|m| {
        if let MarkerEvidence::RetryLoop { loop_kind, .. } = &m.evidence {
            loop_kind == "do_while"
        } else {
            false
        }
    });

    assert!(
        download_retry.is_some(),
        "download retry do-while loop not found"
    );

    // Download retry has dynamic delay (should be None)
    if let MarkerEvidence::RetryLoop { delay_ms, .. } = &download_retry.unwrap().evidence {
        assert_eq!(
            *delay_ms, None,
            "download retry delay should be None (dynamic)"
        );
    }
}

#[test]
fn test_channel_get_file_resume_offset() {
    let source = match read_swupdate_file("corelib/channel_curl.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let markers =
        extract_behavioral_markers(&tree, source.as_bytes(), "corelib/channel_curl.c", "swupdate");

    // Filter to channel_get_file function markers
    let channel_get_file_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.function_name == "channel_get_file")
        .collect();

    // Should have at least 1 RESUME_OFFSET marker
    let resume_offsets: Vec<_> = channel_get_file_markers
        .iter()
        .filter(|m| m.kind == MarkerKind::ResumeOffset)
        .collect();

    assert_eq!(
        resume_offsets.len(),
        1,
        "expected exactly 1 RESUME_OFFSET marker in channel_get_file, got {}",
        resume_offsets.len()
    );

    eprintln!(
        "channel_get_file RESUME_OFFSET markers: {} found",
        resume_offsets.len()
    );

    // Validate the RESUME_OFFSET marker
    let resume = resume_offsets[0];
    if let MarkerEvidence::ResumeOffset {
        api_call,
        option_name,
        offset_source,
    } = &resume.evidence
    {
        assert_eq!(api_call, "curl_easy_setopt", "api_call mismatch");
        assert_eq!(
            option_name.as_deref(),
            Some("CURLOPT_RESUME_FROM_LARGE"),
            "option_name mismatch"
        );
        eprintln!(
            "  line {}: {} with {} offset={}",
            resume.line_start,
            api_call,
            option_name.as_deref().unwrap_or("?"),
            offset_source.as_deref().unwrap_or("?")
        );
    } else {
        panic!("expected ResumeOffset evidence");
    }
}

#[test]
fn test_no_false_positive_behavioral_markers() {
    // Verify curl_init and downloader functions don't produce markers
    let source = match read_swupdate_file("corelib/channel_curl.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let markers =
        extract_behavioral_markers(&tree, source.as_bytes(), "corelib/channel_curl.c", "swupdate");

    // channel_curl_init should NOT have markers (setup function, no retry/resume)
    let init_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.function_name == "channel_curl_init")
        .collect();

    assert!(
        init_markers.is_empty(),
        "channel_curl_init should not produce behavioral markers, got: {:?}",
        init_markers
            .iter()
            .map(|m| format!("{:?}", m.kind))
            .collect::<Vec<_>>()
    );

    eprintln!("Behavioral marker counts by function:");
    let mut by_func: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for m in &markers {
        *by_func.entry(&m.function_name).or_insert(0) += 1;
    }
    for (func, count) in by_func {
        eprintln!("  {}: {}", func, count);
    }
}

#[test]
fn test_behavioral_marker_count_channel_get_file() {
    // PF-2 acceptance: channel_get_file produces 3 markers (2 RETRY_LOOP + 1 RESUME_OFFSET)
    let source = match read_swupdate_file("corelib/channel_curl.c") {
        Some(s) => s,
        None => {
            eprintln!("SKIP: swupdate not found at expected path");
            return;
        }
    };

    let tree = parse_c_file(&source);
    let markers =
        extract_behavioral_markers(&tree, source.as_bytes(), "corelib/channel_curl.c", "swupdate");

    let channel_get_file_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.function_name == "channel_get_file")
        .collect();

    let retry_count = channel_get_file_markers
        .iter()
        .filter(|m| m.kind == MarkerKind::RetryLoop)
        .count();
    let resume_count = channel_get_file_markers
        .iter()
        .filter(|m| m.kind == MarkerKind::ResumeOffset)
        .count();

    eprintln!(
        "channel_get_file: {} RETRY_LOOP + {} RESUME_OFFSET = {} total",
        retry_count,
        resume_count,
        channel_get_file_markers.len()
    );

    // Acceptance criteria from PF-2 spec - exact counts to catch overmatching
    assert_eq!(
        retry_count, 2,
        "expected exactly 2 RETRY_LOOP markers, got {}",
        retry_count
    );
    assert_eq!(
        resume_count, 1,
        "expected exactly 1 RESUME_OFFSET marker, got {}",
        resume_count
    );
    assert_eq!(
        channel_get_file_markers.len(),
        3,
        "expected exactly 3 behavioral markers (2 RETRY_LOOP + 1 RESUME_OFFSET), got {}",
        channel_get_file_markers.len()
    );
}
