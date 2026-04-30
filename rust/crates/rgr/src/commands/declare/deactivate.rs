//! Declare deactivate command.
//!
//! Deactivates an existing declaration.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_deactivate` handler
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration deactivation (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::open_storage;

pub(super) fn run_declare_deactivate(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap declare deactivate <db_path> <declaration_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let declaration_uid = &args[1];

    if declaration_uid.trim().is_empty() {
        eprintln!("error: declaration_uid must be non-empty");
        return ExitCode::from(1);
    }

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    match storage.deactivate_declaration(declaration_uid) {
        Ok(rows) => {
            let output = serde_json::json!({
                "declaration_uid": declaration_uid,
                "deactivated": rows > 0,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
