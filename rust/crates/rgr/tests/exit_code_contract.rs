//! Source-level guard for the public process-exit contract.
//!
//! Rust's `ExitCode` does not retain the symbolic constant used to construct it,
//! so this test deterministically enumerates the rgr source sites. It rejects
//! numeric literals, `ExitCode::SUCCESS`, and computed values at the conversion
//! boundary: every site must visibly name one constant from `daemon_command.rs`.

use std::path::{Path, PathBuf};

use repo_graph_rgr::daemon_command::{
    EXIT_CHECK_FAIL, EXIT_CHECK_INCOMPLETE, EXIT_CHECK_PASS, EXIT_GATE_FAIL, EXIT_GATE_INCOMPLETE,
    EXIT_GATE_PASS, EXIT_HEALTHY, EXIT_HOOK_ERROR, EXIT_HOOK_OK, EXIT_HOOK_WARNING,
    EXIT_NO_VIOLATIONS, EXIT_REFUSED_BY_POLICY, EXIT_RUNTIME_ERROR, EXIT_STILL_RUNNING,
    EXIT_SUCCESS, EXIT_UNHEALTHY, EXIT_USAGE_ERROR, EXIT_VIOLATIONS_FOUND,
};

#[test]
fn exit_code_values_preserve_both_contract_families() {
    assert_eq!(EXIT_SUCCESS, 0);
    assert_eq!(EXIT_USAGE_ERROR, 1);
    assert_eq!(EXIT_RUNTIME_ERROR, 2);
    assert_eq!(EXIT_STILL_RUNNING, 3);
    assert_eq!(EXIT_REFUSED_BY_POLICY, 4);

    assert_eq!(EXIT_CHECK_PASS, 0);
    assert_eq!(EXIT_CHECK_FAIL, 1);
    assert_eq!(EXIT_CHECK_INCOMPLETE, 2);
    assert_eq!(EXIT_GATE_PASS, 0);
    assert_eq!(EXIT_GATE_FAIL, 1);
    assert_eq!(EXIT_GATE_INCOMPLETE, 2);
    assert_eq!(EXIT_HOOK_OK, 0);
    assert_eq!(EXIT_HOOK_WARNING, 1);
    assert_eq!(EXIT_HOOK_ERROR, 2);
    assert_eq!(EXIT_HEALTHY, 0);
    assert_eq!(EXIT_UNHEALTHY, 1);
    assert_eq!(EXIT_NO_VIOLATIONS, 0);
    assert_eq!(EXIT_VIOLATIONS_FOUND, 1);
}

#[test]
fn every_process_exit_site_uses_a_named_constant() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&source_root, &mut rust_files);
    rust_files.sort();

    let mut sites = Vec::new();
    let mut violations = Vec::new();

    for path in rust_files {
        let source = std::fs::read_to_string(&path).unwrap();
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            if line.contains("ExitCode::SUCCESS") {
                violations.push(format!(
                    "{}:{} uses ExitCode::SUCCESS instead of EXIT_SUCCESS",
                    relative(&source_root, &path).display(),
                    line_index + 1
                ));
            }

            for marker in ["ExitCode::from(", "process::exit("] {
                let mut remainder = line;
                while let Some(start) = remainder.find(marker) {
                    let after = &remainder[start + marker.len()..];
                    let argument = after.split(')').next().unwrap_or(after).trim();
                    let site = format!(
                        "{}:{} {marker}{argument})",
                        relative(&source_root, &path).display(),
                        line_index + 1
                    );
                    sites.push(site.clone());
                    if !argument.contains("EXIT_") {
                        violations
                            .push(format!("{site} does not reference a named EXIT_* constant"));
                    }
                    remainder = after;
                }
            }
        }
    }

    assert!(
        !sites.is_empty(),
        "the enumeration must cover real exit sites"
    );
    assert!(
        violations.is_empty(),
        "unnamed process exit sites:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn relative<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap()
}
