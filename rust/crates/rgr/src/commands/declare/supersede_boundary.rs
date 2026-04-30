//! Declare supersede boundary command.
//!
//! Supersedes an existing boundary declaration with a new one.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_supersede_boundary` handler
//! - supersede boundary argument parsing
//! - `SUPERSEDE_BOUNDARY_USAGE` constant
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration supersede (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::extract_module_path_from_key;
use crate::cli::{open_storage, utc_now_iso8601};

const SUPERSEDE_BOUNDARY_USAGE: &str =
    "usage: rmap declare supersede boundary <db_path> <old_declaration_uid> --forbids <target> [--reason <text>]";

pub(super) fn run_declare_supersede_boundary(args: &[String]) -> ExitCode {
    let mut positional = Vec::new();
    let mut forbids: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--forbids" => {
                if forbids.is_some() {
                    eprintln!("error: --forbids specified more than once");
                    return ExitCode::from(1);
                }
                i += 1;
                if i >= args.len() || args[i].starts_with('-') {
                    eprintln!("error: --forbids requires a non-empty value");
                    return ExitCode::from(1);
                }
                let v = args[i].trim().to_string();
                if v.is_empty() {
                    eprintln!("error: --forbids requires a non-empty value");
                    return ExitCode::from(1);
                }
                forbids = Some(v);
            }
            "--reason" => {
                if reason.is_some() {
                    eprintln!("error: --reason specified more than once");
                    return ExitCode::from(1);
                }
                i += 1;
                if i >= args.len() || args[i].starts_with('-') {
                    eprintln!("error: --reason requires a non-empty value");
                    return ExitCode::from(1);
                }
                let v = args[i].trim().to_string();
                if v.is_empty() {
                    eprintln!("error: --reason requires a non-empty value");
                    return ExitCode::from(1);
                }
                reason = Some(v);
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag: {}", other);
                eprintln!("{}", SUPERSEDE_BOUNDARY_USAGE);
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    if positional.len() != 2 {
        eprintln!("{}", SUPERSEDE_BOUNDARY_USAGE);
        return ExitCode::from(1);
    }

    let forbids = match forbids {
        Some(f) => f,
        None => {
            eprintln!("error: --forbids is required");
            return ExitCode::from(1);
        }
    };

    let db_path = Path::new(positional[0].as_str());
    let old_uid = positional[1].as_str();

    if old_uid.trim().is_empty() {
        eprintln!("error: old_declaration_uid must be non-empty");
        return ExitCode::from(1);
    }

    let mut storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Fetch old declaration and validate.
    let old_row = match storage.get_declaration_by_uid(old_uid) {
        Ok(Some(row)) => row,
        Ok(None) => {
            eprintln!("error: declaration {} does not exist", old_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    if !old_row.is_active {
        eprintln!("error: declaration {} is already inactive", old_uid);
        return ExitCode::from(2);
    }

    if old_row.kind != "boundary" {
        eprintln!(
            "error: declaration {} is kind '{}', expected 'boundary'",
            old_uid, old_row.kind
        );
        return ExitCode::from(2);
    }

    // Extract module_path from target_stable_key: {repo}:{path}:MODULE
    let module_path = match extract_module_path_from_key(&old_row.target_stable_key) {
        Some(p) => p,
        None => {
            eprintln!(
                "error: cannot parse module path from target_stable_key: {}",
                old_row.target_stable_key
            );
            return ExitCode::from(2);
        }
    };

    // Build replacement.
    use repo_graph_storage::crud::declarations::{boundary_identity_key, DeclarationInsert};

    let mut value = serde_json::json!({ "forbids": forbids });
    if let Some(ref r) = reason {
        value["reason"] = serde_json::Value::String(r.clone());
    }

    let now = utc_now_iso8601();

    let new_decl = DeclarationInsert {
        identity_key: boundary_identity_key(&old_row.repo_uid, &module_path, &forbids),
        repo_uid: old_row.repo_uid.clone(),
        target_stable_key: old_row.target_stable_key.clone(),
        kind: "boundary".to_string(),
        value_json: value.to_string(),
        created_at: now,
        created_by: Some("cli".to_string()),
        supersedes_uid: None, // overridden by supersede_declaration
        authored_basis_json: None,
    };

    match storage.supersede_declaration(old_uid, &new_decl) {
        Ok(result) => {
            let output = serde_json::json!({
                "old_declaration_uid": result.old_declaration_uid,
                "new_declaration_uid": result.new_declaration_uid,
                "kind": "boundary",
                "target": module_path,
                "forbids": forbids,
                "superseded": true,
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
