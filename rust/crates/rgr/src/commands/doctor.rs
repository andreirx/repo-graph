//! `rmap doctor` command.
//!
//! Health check for repo-graph installation.
//! Reports status of binaries, directories, daemon service, and host integrations.
//!
//! **Architecture:** This module contains policy (what to check, how to report).
//! Platform-specific mechanism (how to query launchd, etc.) lives in `platform/`.

use std::process::ExitCode;

use serde::Serialize;

use crate::cli::paths;
use crate::daemon_client::DaemonClient;
use crate::platform::{
    get_adapter, granular_socket_probes, socket_resolution_probes, PlatformAdapter, ProbeResult,
};

/// Doctor output for JSON mode.
#[derive(Debug, Serialize)]
struct DoctorOutput {
    platform: String,
    probes: Vec<ProbeOutput>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct ProbeOutput {
    name: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

#[derive(Debug, Serialize)]
struct Summary {
    total: usize,
    passed: usize,
    failed: usize,
    healthy: bool,
}

impl From<ProbeResult> for ProbeOutput {
    fn from(p: ProbeResult) -> Self {
        Self {
            name: p.name,
            passed: p.passed,
            message: p.message,
            details: p.details,
        }
    }
}

/// Run the doctor command.
pub fn run_doctor(args: &[String]) -> ExitCode {
    let mut json_output = false;

    // Parse arguments
    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_output = true;
            }
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown option: {}", other);
                print_usage();
                return ExitCode::from(1);
            }
        }
    }

    let (output, healthy) = execute_doctor();

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        print_human_output(&output);
    }

    if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn execute_doctor() -> (DoctorOutput, bool) {
    let adapter = get_adapter();
    let mut probes = adapter.doctor_probes();

    // Add socket resolution diagnostics for detailed output
    let resolution_probes = socket_resolution_probes();
    probes.extend(resolution_probes);

    // Add granular socket probes for agent-parseable diagnostics (DAEMON-SOCKET-HEALTH-1)
    let granular_probes = granular_socket_probes();
    probes.extend(granular_probes);

    // Add storage summary probe (PERF-OBS-1)
    // Uses DaemonClient which handles transport fallback (socket → stdio)
    // so this works in both normal and sandboxed environments
    if let Some(storage_probe) = storage_summary_probe() {
        probes.push(storage_probe);
    }

    // Add state root mode probe (STATE-ROOT-SEPARATION-1)
    if let Some(state_root_probe) = state_root_mode_probe() {
        probes.push(state_root_probe);
    }

    let passed = probes.iter().filter(|p| p.passed).count();
    let failed = probes.len() - passed;
    let healthy = failed == 0;

    let platform = if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    };

    let output = DoctorOutput {
        platform,
        probes: probes.into_iter().map(ProbeOutput::from).collect(),
        summary: Summary {
            total: passed + failed,
            passed,
            failed,
            healthy,
        },
    };

    (output, healthy)
}

/// Query daemon for storage summary (DB size, snapshot count).
///
/// Returns a probe with storage info on success, or a degraded/info probe on error.
/// Never returns None — failures should be visible in diagnostics.
fn storage_summary_probe() -> Option<ProbeResult> {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            return Some(ProbeResult {
                name: "storage".to_string(),
                passed: false,
                message: "failed to get cwd".to_string(),
                details: Some(format!("{}", e)),
            });
        }
    };

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            return Some(ProbeResult {
                name: "storage".to_string(),
                passed: false,
                message: "daemon unavailable".to_string(),
                details: Some(format!("{}", e)),
            });
        }
    };

    let params = serde_json::json!({
        "path": cwd.to_string_lossy()
    });

    // DEV-INSTALL-DOCTOR-WAIT-1: use the cheap `storage_health` summary, NOT the heavy `perf`
    // diagnostic (which runs a per-table COUNT(*) scan — ~80-100s on large repos).
    let response = match client.request("storage_health", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("not indexed") {
                // Not an error — just no repo in cwd
                return Some(ProbeResult {
                    name: "storage".to_string(),
                    passed: true,
                    message: "no repo indexed in cwd".to_string(),
                    details: None,
                });
            }
            // Other errors are degraded diagnostics
            return Some(ProbeResult {
                name: "storage".to_string(),
                passed: false,
                message: "query failed".to_string(),
                details: Some(msg),
            });
        }
    };

    // Parse response (DEV-INSTALL-DOCTOR-WAIT-1: flat `storage_health` shape).
    let db_size = response
        .get("db_size_bytes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let snapshot_count = response
        .get("total_snapshots")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // CACHE-SEMANTICS-1: prunable snapshot count
    let prunable_count = response
        .get("prunable_snapshots")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let size_human = format_size(db_size);

    // Include prunable count if > 0
    let message = if prunable_count > 0 {
        format!(
            "db: {}, {} snapshots ({} prunable)",
            size_human, snapshot_count, prunable_count
        )
    } else {
        format!("db: {}, {} snapshots", size_human, snapshot_count)
    };

    Some(ProbeResult {
        name: "storage".to_string(),
        passed: true,
        message,
        details: None,
    })
}

/// Query daemon for authority write policy.
///
/// STATE-ROOT-SEPARATION-1: Reports whether authority writes (baselines, aliases,
/// declarations) are permitted in the current daemon execution mode.
///
/// The key diagnostic question: "Can I write authority data from this context?"
fn state_root_mode_probe() -> Option<ProbeResult> {
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            return Some(ProbeResult {
                name: "authority_policy".to_string(),
                passed: false,
                message: "daemon unavailable".to_string(),
                details: Some(format!("{}", e)),
            });
        }
    };

    let response = match client.request("daemon_info", None) {
        Ok(r) => r,
        Err(e) => {
            return Some(ProbeResult {
                name: "authority_policy".to_string(),
                passed: false,
                message: "query failed".to_string(),
                details: Some(format!("{}", e)),
            });
        }
    };

    let authority_writes = response
        .get("authority_writes_allowed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // In sandbox mode, authority writes are blocked — this is expected behavior, not a failure
    // The probe always passes; it's informational
    if authority_writes {
        Some(ProbeResult {
            name: "authority_policy".to_string(),
            passed: true,
            message: "baselines, aliases, declarations: allowed".to_string(),
            details: None,
        })
    } else {
        Some(ProbeResult {
            name: "authority_policy".to_string(),
            passed: true, // Not a failure - sandbox mode is valid operation
            message: "baselines, aliases, declarations: blocked (sandbox mode)".to_string(),
            details: Some("authority writes require socket daemon".to_string()),
        })
    }
}

/// Format size in human-readable form.
fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn print_human_output(output: &DoctorOutput) {
    println!("repo-graph health check ({})", output.platform);
    println!();

    // Group probes by category
    let binary_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| matches!(p.name.as_str(), "rmap" | "rmapd" | "rgistr"))
        .collect();

    let dir_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| p.name.ends_with("_dir"))
        .collect();

    let service_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| {
            matches!(
                p.name.as_str(),
                "daemon_service"
                    | "daemon_socket"
                    | "plist"
                    | "unit_file"
                    | "pid_file"
                    | "socket_file"
                    | "socket_connect"
                    | "socket_ping"
                    | "transport"
                    | "state_root"
                    | "authority_policy"
            )
        })
        .collect();

    let resolution_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| {
            matches!(
                p.name.as_str(),
                "effective_uid"
                    | "env_home"
                    | "canonical_home"
                    | "socket_path"
                    | "socket_resolution"
                    | "socket_override"
            )
        })
        .collect();

    // Binaries
    println!("Binaries:");
    for probe in &binary_probes {
        print_probe(probe);
    }
    println!();

    // Directories
    println!("Directories:");
    for probe in &dir_probes {
        print_probe(probe);
    }
    println!();

    // Daemon
    println!("Daemon:");
    for probe in &service_probes {
        print_probe(probe);
    }
    println!();

    // Socket Resolution (only show if there's a problem or using fallback)
    let has_resolution_issue = resolution_probes.iter().any(|p| !p.passed);
    let using_legacy = paths::is_using_legacy_fallback();

    if has_resolution_issue || using_legacy {
        println!("Socket Resolution:");
        for probe in &resolution_probes {
            print_probe(probe);
        }
        println!();

        if using_legacy {
            println!("Warning: Using legacy socket path from $HOME.");
            println!("         Restart daemon to use canonical path.");
            println!();
        }
    }

    // Storage (PERF-OBS-1)
    let storage_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| p.name == "storage")
        .collect();

    if !storage_probes.is_empty() {
        println!("Storage:");
        for probe in &storage_probes {
            print_probe(probe);
        }
        println!();
    }

    // Summary
    if output.summary.healthy {
        println!(
            "Status: healthy ({}/{} checks passed)",
            output.summary.passed, output.summary.total
        );
    } else {
        println!(
            "Status: UNHEALTHY ({}/{} checks failed)",
            output.summary.failed, output.summary.total
        );
    }
}

fn print_probe(probe: &ProbeOutput) {
    let status = if probe.passed { "ok" } else { "FAIL" };
    println!("  [{}] {}: {}", status, probe.name, probe.message);
    if let Some(ref details) = probe.details {
        println!("        {}", details);
    }
}

fn print_usage() {
    eprintln!("usage: rmap doctor [OPTIONS]");
    eprintln!();
    eprintln!("Check repo-graph installation health.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --json            Output JSON instead of human-readable text");
    eprintln!("  --help, -h        Show this help message");
    eprintln!();
    eprintln!("Exit codes:");
    eprintln!("  0    All checks passed (healthy)");
    eprintln!("  1    One or more checks failed (unhealthy)");
}
