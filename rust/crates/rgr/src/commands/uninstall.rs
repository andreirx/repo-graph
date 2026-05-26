//! `rmap uninstall` command.
//!
//! Removes repo-graph installation based on install manifest.
//! Per DIST-1 D4 (Uninstall Contract):
//! 1. Stop daemon service
//! 2. Unregister daemon service
//! 3. Restore host integration backups
//! 4. Remove host integration patches
//! 5. Remove binaries
//! 6. Remove runtime data (optional, prompt user)
//! 7. Remove config (optional, prompt user)
//! 8. Remove manifest
//!
//! **Architecture:** This module contains policy (what to uninstall, in what order).
//! Platform-specific mechanism (how to stop launchd, etc.) lives in `platform/`.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use crate::cli::paths;
use crate::platform::{get_adapter, PlatformAdapter, ServiceStatus};

/// Run the uninstall command.
pub fn run_uninstall(args: &[String]) -> ExitCode {
    let mut dry_run = false;
    let mut force = false;
    let mut remove_data = false;

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" | "-n" => {
                dry_run = true;
                i += 1;
            }
            "--force" | "-f" => {
                force = true;
                i += 1;
            }
            "--remove-data" => {
                remove_data = true;
                i += 1;
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

    match execute_uninstall(dry_run, force, remove_data) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn execute_uninstall(dry_run: bool, force: bool, remove_data: bool) -> Result<(), String> {
    let adapter = get_adapter();

    println!("repo-graph uninstall");
    println!();

    if dry_run {
        println!("[dry-run mode - no changes will be made]");
        println!();
    }

    // Read manifest to know what's installed
    let manifest = match adapter.read_manifest() {
        Ok(m) => Some(m),
        Err(e) => {
            if force {
                eprintln!("warning: could not read manifest: {}", e);
                eprintln!("continuing with --force (using default paths)");
                None
            } else {
                return Err(format!(
                    "could not read install manifest: {}\nUse --force to proceed without manifest",
                    e
                ));
            }
        }
    };

    // Confirm unless forced
    if !force && !dry_run {
        println!("This will remove:");
        println!("  - Daemon service");
        println!("  - CLI binaries (rmap, rmapd, rgistr)");
        if remove_data {
            println!("  - Configuration and data directories");
        }
        println!();

        if !confirm("Proceed with uninstall?") {
            println!("Aborted.");
            return Ok(());
        }
        println!();
    }

    // 1. Stop daemon service
    println!("Stopping daemon service...");
    let status = adapter.service_status();
    match status {
        ServiceStatus::Running { .. } => {
            if dry_run {
                println!("  [dry-run] would stop service");
            } else {
                adapter.stop_service()?;
                println!("  Service stopped");
            }
        }
        ServiceStatus::Stopped => {
            println!("  Service already stopped");
        }
        ServiceStatus::NotInstalled => {
            println!("  Service not installed");
        }
        ServiceStatus::Unknown { reason } => {
            eprintln!("  Warning: could not determine service status: {}", reason);
        }
    }

    // 2. Remove service registration
    println!("Removing service registration...");
    if dry_run {
        println!("  [dry-run] would remove service registration");
    } else {
        match adapter.remove_service() {
            Ok(()) => println!("  Service registration removed"),
            Err(e) => eprintln!("  Warning: {}", e),
        }
    }

    // 3-4. Host integration backups (TODO: implement when CLAUDE-1/CODEX-1 are done)
    // For now, just note that manual restoration may be needed
    println!("Host integrations...");
    println!("  Note: Check ~/.claude/settings.json.rmap-backup for Claude Code backup");
    println!("  Note: Check ~/.codex/hooks.json.rmap-backup for Codex backup");

    // 5. Remove binaries (paths from manifest, fallback to defaults)
    println!("Removing binaries...");

    // Use canonical home to find actual installation regardless of $HOME
    let default_install_dir = paths::canonical_home()
        .map(|h| h.join(".local").join("bin"))
        .ok_or_else(|| "could not determine home directory".to_string())?;

    // Build list of binary paths from manifest or defaults
    let binary_paths: Vec<std::path::PathBuf> = if let Some(ref m) = manifest {
        let mut paths = Vec::new();
        if let Some(ref c) = m.components.rmap {
            paths.push(c.path.clone());
        } else {
            paths.push(default_install_dir.join("rmap"));
        }
        if let Some(ref c) = m.components.rmapd {
            paths.push(c.path.clone());
        } else {
            paths.push(default_install_dir.join("rmapd"));
        }
        if let Some(ref c) = m.components.rgistr {
            paths.push(c.path.clone());
        } else {
            paths.push(default_install_dir.join("rgistr"));
        }
        paths
    } else {
        // No manifest (--force mode): use default paths
        vec![
            default_install_dir.join("rmap"),
            default_install_dir.join("rmapd"),
            default_install_dir.join("rgistr"),
        ]
    };

    for path in &binary_paths {
        if path.exists() {
            if dry_run {
                println!("  [dry-run] would remove {}", path.display());
            } else if let Err(e) = std::fs::remove_file(path) {
                eprintln!("  Warning: could not remove {}: {}", path.display(), e);
            } else {
                println!("  Removed {}", path.display());
            }
        } else {
            println!("  {} not found", path.display());
        }
    }

    // 6-7. Remove data and config (optional, paths from manifest or defaults)
    let (config_dir, data_dir, logs_dir) = if let Some(ref m) = manifest {
        // Manifest is authoritative for paths
        let cfg = if m.directories.config.as_os_str().is_empty() {
            paths::config_dir()
        } else {
            Some(m.directories.config.clone())
        };
        let dat = if m.directories.data.as_os_str().is_empty() {
            paths::data_dir()
        } else {
            Some(m.directories.data.clone())
        };
        let log = if m.directories.logs.as_os_str().is_empty() {
            paths::logs_dir()
        } else {
            Some(m.directories.logs.clone())
        };
        (cfg, dat, log)
    } else {
        // No manifest (--force mode): use default paths
        (paths::config_dir(), paths::data_dir(), paths::logs_dir())
    };

    if remove_data {
        println!("Removing configuration and data...");

        if let Some(ref dir) = config_dir {
            remove_directory(dir, dry_run);
        }
        if let Some(ref dir) = data_dir {
            // Only remove if different from config_dir
            if config_dir.as_ref() != Some(dir) {
                remove_directory(dir, dry_run);
            }
        }
        if let Some(ref dir) = logs_dir {
            remove_directory(dir, dry_run);
        }
    } else {
        println!("Configuration and data preserved.");
        if let Some(ref dir) = config_dir {
            println!("  To remove: rm -rf \"{}\"", dir.display());
        }
        if let Some(ref dir) = logs_dir {
            println!("  To remove: rm -rf \"{}\"", dir.display());
        }
    }

    // 8. Remove manifest (already removed if config dir was removed)
    if !remove_data {
        if let Some(ref dir) = config_dir {
            let manifest_path = dir.join("install-manifest.json");
            if manifest_path.exists() {
                if dry_run {
                    println!("  [dry-run] would remove manifest");
                } else {
                    let _ = std::fs::remove_file(&manifest_path);
                }
            }
        }
    }

    println!();
    if dry_run {
        println!("[dry-run complete - no changes made]");
    } else {
        println!("Uninstall complete.");
    }

    Ok(())
}

fn remove_directory(path: &Path, dry_run: bool) {
    if path.exists() {
        if dry_run {
            println!("  [dry-run] would remove {}", path.display());
        } else if let Err(e) = std::fs::remove_dir_all(path) {
            eprintln!("  Warning: could not remove {}: {}", path.display(), e);
        } else {
            println!("  Removed {}", path.display());
        }
    }
}

fn confirm(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    let input = input.trim().to_lowercase();
    input == "y" || input == "yes"
}

fn print_usage() {
    eprintln!("usage: rmap uninstall [OPTIONS]");
    eprintln!();
    eprintln!("Remove repo-graph installation.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --dry-run, -n     Show what would be removed without making changes");
    eprintln!("  --force, -f       Proceed without confirmation, ignore missing manifest");
    eprintln!("  --remove-data     Also remove configuration and data directories");
    eprintln!("  --help, -h        Show this help message");
    eprintln!();
    eprintln!("By default, configuration and data are preserved for reinstall.");
}
