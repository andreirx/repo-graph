//! `rmap doctor` command.
//!
//! Health check for repo-graph installation.
//! Reports status of binaries, directories, daemon service, and host integrations.
//!
//! **Architecture:** This module contains policy (what to check, how to report).
//! Platform-specific mechanism (how to query launchd, etc.) lives in `platform/`.

use std::process::ExitCode;

use serde::Serialize;

use crate::platform::{get_adapter, PlatformAdapter, ProbeResult};

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
    let probes = adapter.doctor_probes();

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
        .filter(|p| matches!(p.name.as_str(), "daemon_service" | "plist"))
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
