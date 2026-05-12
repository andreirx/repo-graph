//! Integration test against real coverage report.
//!
//! This test validates the parser against the actual coverage-final.json
//! in the repo-graph repository. It serves as a smoke test for real-world
//! Istanbul/c8 output.

use repo_graph_coverage::parse_istanbul_file;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // Navigate from rust/crates/coverage/tests to repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates
        .unwrap()
        .parent() // rust
        .unwrap()
        .parent() // repo root
        .unwrap()
        .to_path_buf()
}

#[test]
fn parse_real_coverage_report() {
    let root = repo_root();
    let report_path = root.join("coverage/coverage-final.json");

    if !report_path.exists() {
        // Skip if coverage report doesn't exist (CI may not have run tests)
        eprintln!(
            "Skipping: coverage report not found at {}",
            report_path.display()
        );
        return;
    }

    let result = parse_istanbul_file(report_path.to_str().unwrap(), root.to_str().unwrap())
        .expect("should parse real coverage report");

    // Basic sanity checks
    assert!(
        !result.facts.is_empty(),
        "should have at least one file with coverage"
    );

    // All paths should be repo-relative (no leading /)
    for fact in &result.facts {
        assert!(
            !fact.file_path.starts_with('/'),
            "path should be repo-relative: {}",
            fact.file_path
        );
        assert!(
            !fact.file_path.contains('\\'),
            "path should use / separators: {}",
            fact.file_path
        );
    }

    // Coverage values should be in valid range
    for fact in &result.facts {
        assert!(
            fact.line_coverage >= 0.0 && fact.line_coverage <= 1.0,
            "coverage should be 0-1: {} = {}",
            fact.file_path,
            fact.line_coverage
        );
    }

    // Should recognize TypeScript source files
    let ts_files: Vec<_> = result
        .facts
        .iter()
        .filter(|f| f.file_path.ends_with(".ts"))
        .collect();
    assert!(
        !ts_files.is_empty(),
        "should have TypeScript files in coverage"
    );

    // Print summary for manual inspection
    println!("Parsed {} files", result.facts.len());
    println!("Unnormalized: {} paths", result.unnormalized_paths.len());

    // Report any unnormalized paths (these are paths outside repo root)
    if !result.unnormalized_paths.is_empty() {
        println!("Unnormalized paths (expected for node_modules, etc.):");
        for path in result.unnormalized_paths.iter().take(5) {
            println!("  {}", path);
        }
    }
}

#[test]
fn parsed_paths_include_src_files() {
    let root = repo_root();
    let report_path = root.join("coverage/coverage-final.json");

    if !report_path.exists() {
        return;
    }

    let result =
        parse_istanbul_file(report_path.to_str().unwrap(), root.to_str().unwrap()).unwrap();

    // Should include files under src/
    let src_files: Vec<_> = result
        .facts
        .iter()
        .filter(|f| f.file_path.starts_with("src/"))
        .collect();

    assert!(
        !src_files.is_empty(),
        "should have at least one src/ file with coverage"
    );

    // Print the file distribution for manual inspection
    let test_files = result
        .facts
        .iter()
        .filter(|f| f.file_path.starts_with("test/"))
        .count();
    println!("src/ files: {}", src_files.len());
    println!("test/ files: {}", test_files);
}
