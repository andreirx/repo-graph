//! Declare command family.
//!
//! Governance and policy declaration management:
//! - `boundary` — module boundary declarations
//! - `requirement` — requirement declarations with obligations
//! - `waiver` — waiver declarations for obligations
//! - `quality-policy` — quality policy declarations
//! - `deactivate` — deactivate existing declarations
//! - `supersede` — supersede and replace declarations
//!
//! # Boundary rules
//!
//! This module owns declare command-family behavior:
//! - command handlers
//! - family-local DTOs
//! - family-local argument parsing
//! - family-local helpers
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration storage (lives in `repo-graph-storage`)
//! - policy validation (lives in `repo-graph-quality-policy`)

mod boundary;
mod deactivate;
mod quality_policy;
mod requirement;
mod shared;
mod supersede_boundary;
mod supersede_requirement;
mod supersede_waiver;
mod waiver;

use std::process::ExitCode;

use boundary::run_declare_boundary;
use deactivate::run_declare_deactivate;
use quality_policy::run_declare_quality_policy;
use requirement::run_declare_requirement;
use supersede_boundary::run_declare_supersede_boundary;
use supersede_requirement::run_declare_supersede_requirement;
use supersede_waiver::run_declare_supersede_waiver;
use waiver::run_declare_waiver;

/// Dispatcher for `rmap declare <subcommand>`.
pub fn run_declare(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap declare <subcommand> ...");
        eprintln!("subcommands: boundary, requirement, waiver, quality-policy, deactivate, supersede");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "boundary" => run_declare_boundary(&args[1..]),
        "requirement" => run_declare_requirement(&args[1..]),
        "waiver" => run_declare_waiver(&args[1..]),
        "quality-policy" => run_declare_quality_policy(&args[1..]),
        "deactivate" => run_declare_deactivate(&args[1..]),
        "supersede" => run_declare_supersede(&args[1..]),
        other => {
            eprintln!("unknown declare subcommand: {}", other);
            eprintln!("subcommands: boundary, requirement, waiver, quality-policy, deactivate, supersede");
            ExitCode::from(1)
        }
    }
}

/// Dispatcher for `rmap declare supersede <kind>`.
fn run_declare_supersede(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap declare supersede <kind> ...");
        eprintln!("kinds: boundary, requirement, waiver");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "boundary" => run_declare_supersede_boundary(&args[1..]),
        "requirement" => run_declare_supersede_requirement(&args[1..]),
        "waiver" => run_declare_supersede_waiver(&args[1..]),
        other => {
            eprintln!("unknown supersede kind: {}", other);
            eprintln!("kinds: boundary, requirement, waiver");
            ExitCode::from(1)
        }
    }
}
