//! Declare boundary command.
//!
//! Creates module boundary declarations.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_boundary` handler
//! - boundary-specific argument parsing
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration insertion (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{open_storage, utc_now_iso8601};

pub(super) fn run_declare_boundary(args: &[String]) -> ExitCode {
    // Parse positional args and flags.
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
                eprintln!("usage: rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]");
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    if positional.len() != 3 {
        eprintln!("usage: rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]");
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
    let repo_uid = positional[1].as_str();
    let module_path = positional[2].as_str();

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Build the declaration.
    use repo_graph_storage::crud::declarations::{boundary_identity_key, DeclarationInsert};

    let target_stable_key = format!("{}:{}:MODULE", repo_uid, module_path);

    let mut value = serde_json::json!({ "forbids": forbids });
    if let Some(ref r) = reason {
        value["reason"] = serde_json::Value::String(r.clone());
    }

    let now = utc_now_iso8601();

    let decl = DeclarationInsert {
        identity_key: boundary_identity_key(repo_uid, module_path, &forbids),
        repo_uid: repo_uid.to_string(),
        target_stable_key,
        kind: "boundary".to_string(),
        value_json: value.to_string(),
        created_at: now,
        created_by: Some("cli".to_string()),
        supersedes_uid: None,
        authored_basis_json: None,
    };

    match storage.insert_declaration(&decl) {
        Ok(result) => {
            let output = serde_json::json!({
                "declaration_uid": result.declaration_uid,
                "kind": "boundary",
                "target": module_path,
                "forbids": forbids,
                "inserted": result.inserted,
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
