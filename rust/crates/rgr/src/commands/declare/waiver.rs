//! Declare waiver command.
//!
//! Creates waiver declarations for requirement obligations.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_waiver` handler
//! - waiver-specific argument parsing
//! - `DECLARE_WAIVER_USAGE` constant
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration insertion (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::parse_flag_value;
use crate::cli::{open_storage, utc_now_iso8601};

const DECLARE_WAIVER_USAGE: &str =
    "usage: rmap declare waiver <db_path> <repo_uid> <req_id> --requirement-version <n> --obligation-id <id> --reason <text> [--expires-at <iso>] [--created-by <actor>] [--rationale-category <cat>] [--policy-basis <text>]";

pub(super) fn run_declare_waiver(args: &[String]) -> ExitCode {
    let mut positional = Vec::new();
    let mut requirement_version: Option<String> = None;
    let mut obligation_id: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut expires_at: Option<String> = None;
    let mut created_by: Option<String> = None;
    let mut rationale_category: Option<String> = None;
    let mut policy_basis: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--requirement-version" => {
                match parse_flag_value("--requirement-version", &requirement_version, args, &mut i)
                {
                    Some(v) => requirement_version = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            "--obligation-id" => {
                match parse_flag_value("--obligation-id", &obligation_id, args, &mut i) {
                    Some(v) => obligation_id = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            "--reason" => match parse_flag_value("--reason", &reason, args, &mut i) {
                Some(v) => reason = Some(v),
                None => return ExitCode::from(1),
            },
            "--expires-at" => match parse_flag_value("--expires-at", &expires_at, args, &mut i) {
                Some(v) => expires_at = Some(v),
                None => return ExitCode::from(1),
            },
            "--created-by" => match parse_flag_value("--created-by", &created_by, args, &mut i) {
                Some(v) => created_by = Some(v),
                None => return ExitCode::from(1),
            },
            "--rationale-category" => {
                match parse_flag_value("--rationale-category", &rationale_category, args, &mut i) {
                    Some(v) => rationale_category = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            "--policy-basis" => {
                match parse_flag_value("--policy-basis", &policy_basis, args, &mut i) {
                    Some(v) => policy_basis = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag: {}", other);
                eprintln!("{}", DECLARE_WAIVER_USAGE);
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    if positional.len() != 3 {
        eprintln!("{}", DECLARE_WAIVER_USAGE);
        return ExitCode::from(1);
    }

    // Validate required flags.
    let version_str = match requirement_version {
        Some(v) => v,
        None => {
            eprintln!("error: --requirement-version is required");
            return ExitCode::from(1);
        }
    };
    let version_num: i64 = match version_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "error: --requirement-version must be an integer, got: {}",
                version_str
            );
            return ExitCode::from(1);
        }
    };
    let obligation_id = match obligation_id {
        Some(v) => v,
        None => {
            eprintln!("error: --obligation-id is required");
            return ExitCode::from(1);
        }
    };
    let reason = match reason {
        Some(v) => v,
        None => {
            eprintln!("error: --reason is required");
            return ExitCode::from(1);
        }
    };

    let db_path = Path::new(positional[0].as_str());
    let repo_uid = positional[1].as_str();
    let req_id = positional[2].as_str();

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let now = utc_now_iso8601();
    let effective_created_by = created_by.unwrap_or_else(|| "cli".to_string());

    // Build value_json — only include optional fields when present.
    let mut value = serde_json::json!({
        "req_id": req_id,
        "requirement_version": version_num,
        "obligation_id": obligation_id,
        "reason": reason,
        "created_at": now,
        "created_by": effective_created_by,
    });
    if let Some(ref exp) = expires_at {
        value["expires_at"] = serde_json::Value::String(exp.clone());
    }
    if let Some(ref rc) = rationale_category {
        value["rationale_category"] = serde_json::Value::String(rc.clone());
    }
    if let Some(ref pb) = policy_basis {
        value["policy_basis"] = serde_json::Value::String(pb.clone());
    }

    use repo_graph_storage::crud::declarations::{waiver_identity_key, DeclarationInsert};

    let target_stable_key = format!("{}:waiver:{}#{}", repo_uid, req_id, obligation_id);

    let decl = DeclarationInsert {
        identity_key: waiver_identity_key(repo_uid, req_id, version_num, &obligation_id),
        repo_uid: repo_uid.to_string(),
        target_stable_key,
        kind: "waiver".to_string(),
        value_json: value.to_string(),
        created_at: now.clone(),
        created_by: Some(effective_created_by),
        supersedes_uid: None,
        authored_basis_json: None,
    };

    match storage.insert_declaration(&decl) {
        Ok(result) => {
            let output = serde_json::json!({
                "declaration_uid": result.declaration_uid,
                "kind": "waiver",
                "req_id": req_id,
                "requirement_version": version_num,
                "obligation_id": obligation_id,
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
