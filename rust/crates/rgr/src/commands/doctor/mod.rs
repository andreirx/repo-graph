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

/// `daemon_info`-derived probes (authority policy + daemon RSS + total storage).
/// Extracted into a child module to keep this file under the 500-line structural
/// guardrail. `format_size` (defined below) is shared into it via `super::`.
mod daemon_info;

/// `storage_health`-derived storage probe (DAEMON-VISIBILITY-1 E + F). Extracted into a child
/// module for the same 500-line-guardrail reason; `format_size` is shared via `super::`.
mod storage_probe;

/// `storage_health`-derived "Semantic seeding" probe (EMBED-SEED-IMPL-1 §9). Extracted into a
/// child module for the same 500-line-guardrail reason (review-6 #2); `ProbeOutput`/`DoctorOutput`/
/// `print_probe_labeled` are shared via `super::`.
mod seed;

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

    // Add storage summary probe (PERF-OBS-1) + the witness-ledger operational probe when the
    // daemon attached one (RECON-M-R3a; absent on zero-SCIP repos — R-0 data-driven absence).
    // Uses DaemonClient which handles transport fallback (socket → stdio)
    // so this works in both normal and sandboxed environments
    probes.extend(storage_summary_probes());

    // daemon_info-derived probes: authority policy (STATE-ROOT-SEPARATION-1) plus
    // daemon memory + total storage (DOCTOR-RESOURCE-REPORT), from one round-trip.
    probes.extend(daemon_info::probes());

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

/// Query daemon for storage summary (DB size, snapshot count) + the witness-ledger
/// operational block (RECON-M-R3a) — ONE `storage_health` round-trip, one or two probes.
///
/// Always returns at least the `storage` probe (failures visible in diagnostics, never
/// silent); the `witness_ledger` probe rides only when the daemon attached the block.
/// (Renamed from `storage_summary_probe` with the plural contract — local, recorded.)
fn storage_summary_probes() -> Vec<ProbeResult> {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            return vec![ProbeResult {
                name: "storage".to_string(),
                passed: false,
                message: "failed to get cwd".to_string(),
                details: Some(format!("{}", e)),
            }];
        }
    };

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            return vec![ProbeResult {
                name: "storage".to_string(),
                passed: false,
                message: "daemon unavailable".to_string(),
                details: Some(format!("{}", e)),
            }];
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
                return vec![ProbeResult {
                    name: "storage".to_string(),
                    passed: true,
                    message: "no repo indexed in cwd".to_string(),
                    details: None,
                }];
            }
            // Other errors are degraded diagnostics
            return vec![ProbeResult {
                name: "storage".to_string(),
                passed: false,
                message: "query failed".to_string(),
                details: Some(msg),
            }];
        }
    };

    let mut probes = vec![storage_probe::storage_probe_from_facts(&response)];
    probes.extend(storage_probe::witness_probe_from_facts(&response));
    probes.push(seed::semantic_seeding_from_facts(&response));
    probes
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
                    // DAEMON-VISIBILITY-1 (D): "what is the daemon doing right now" line.
                    | "activity"
                    // SNAPSHOT-RETENTION-1: "what did the background cleanup pass last do" line
                    // (pruned N / reclaimed X / nothing to prune). Pairs with `activity`; without
                    // this arm the probe is counted but never shown in HUMAN output (JSON had it).
                    | "retention"
                    // ENRICH-LIFECYCLE-1: the enrichment lifecycle line (completed / skipped /
                    // disabled / none yet). Same reason as `retention` — must be listed to render.
                    | "enrichment"
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

    // Storage (PERF-OBS-1) + the witness-ledger operational line (RECON-M-R3a; the probe name
    // must be listed here or human output hides it — the section-filter gotcha above).
    let storage_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| {
            matches!(
                p.name.as_str(),
                "storage" | "witness_ledger" | "orphan_storage"
            )
        })
        .collect();

    if !storage_probes.is_empty() {
        println!("Storage:");
        for probe in &storage_probes {
            let label = match probe.name.as_str() {
                // Reader-frame label for the machine-readable probe name.
                "witness_ledger" => "call-graph witnesses",
                // FORGET-REPO-1: the orphaned-storage line (reclaimable via `rmap maintenance gc`).
                "orphan_storage" => "orphaned storage",
                other => other,
            };
            print_probe_labeled(probe, label);
        }
        println!();
    }

    // EMBED-SEED-IMPL-1 (spec §9): the "Semantic seeding" section (extracted to
    // `seed.rs`, review-6 #2).
    seed::print_seed_section(output);

    // Resources (DOCTOR-RESOURCE-REPORT): daemon RAM (RSS) + total on-disk storage.
    // `daemon_memory` and `total_storage` belong to no other group, so this section is
    // the only place they surface in human output. The probe `name` is machine-readable
    // snake_case (the JSON contract); the human line maps it to a friendly label.
    let resource_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| matches!(p.name.as_str(), "daemon_memory" | "total_storage"))
        .collect();

    if !resource_probes.is_empty() {
        println!("Resources:");
        for probe in &resource_probes {
            let label = match probe.name.as_str() {
                "daemon_memory" => "daemon memory",
                "total_storage" => "total storage",
                other => other,
            };
            print_probe_labeled(probe, label);
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
    print_probe_labeled(probe, &probe.name);
}

/// Render a probe with an explicit display label, so the human line can differ from the
/// machine-readable `name`. DOCTOR-RESOURCE-REPORT uses this for the Resources section
/// (JSON `daemon_memory` → human "daemon memory").
fn print_probe_labeled(probe: &ProbeOutput, label: &str) {
    let status = if probe.passed { "ok" } else { "FAIL" };
    println!("  [{}] {}: {}", status, label, probe.message);
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
