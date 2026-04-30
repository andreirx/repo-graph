//! Declare supersede waiver command.
//!
//! Supersedes an existing waiver declaration with a new one.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_supersede_waiver` handler
//! - supersede waiver argument parsing
//! - `SUPERSEDE_WAIVER_USAGE` constant
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration supersede (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::parse_flag_value;
use crate::cli::{open_storage, utc_now_iso8601};

const SUPERSEDE_WAIVER_USAGE: &str =
    "usage: rmap declare supersede waiver <db_path> <old_declaration_uid> --reason <text> [--expires-at <iso>] [--created-by <actor>] [--rationale-category <cat>] [--policy-basis <text>]";

pub(super) fn run_declare_supersede_waiver(args: &[String]) -> ExitCode {
    let mut positional = Vec::new();
    let mut reason: Option<String> = None;
    let mut expires_at: Option<String> = None;
    let mut created_by: Option<String> = None;
    let mut rationale_category: Option<String> = None;
    let mut policy_basis: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
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
                eprintln!("{}", SUPERSEDE_WAIVER_USAGE);
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    if positional.len() != 2 {
        eprintln!("{}", SUPERSEDE_WAIVER_USAGE);
        return ExitCode::from(1);
    }

    let reason = match reason {
        Some(v) => v,
        None => {
            eprintln!("error: --reason is required");
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

    // Fetch and validate old declaration.
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
    if old_row.kind != "waiver" {
        eprintln!(
            "error: declaration {} is kind '{}', expected 'waiver'",
            old_uid, old_row.kind
        );
        return ExitCode::from(2);
    }

    // Parse old value_json to extract identity fields.
    let old_value: serde_json::Value = match serde_json::from_str(&old_row.value_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: old waiver has malformed value_json: {}", e);
            return ExitCode::from(2);
        }
    };
    let req_id = match old_value["req_id"].as_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("error: old waiver missing req_id in value_json");
            return ExitCode::from(2);
        }
    };
    let requirement_version = match old_value["requirement_version"].as_i64() {
        Some(v) => v,
        None => {
            eprintln!("error: old waiver missing requirement_version in value_json");
            return ExitCode::from(2);
        }
    };
    let obligation_id = match old_value["obligation_id"].as_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("error: old waiver missing obligation_id in value_json");
            return ExitCode::from(2);
        }
    };

    // Build replacement value_json.
    let now = utc_now_iso8601();
    let effective_created_by = created_by.unwrap_or_else(|| "cli".to_string());

    let mut value = serde_json::json!({
        "req_id": req_id,
        "requirement_version": requirement_version,
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

    let new_decl = DeclarationInsert {
        identity_key: waiver_identity_key(
            &old_row.repo_uid,
            &req_id,
            requirement_version,
            &obligation_id,
        ),
        repo_uid: old_row.repo_uid.clone(),
        target_stable_key: old_row.target_stable_key.clone(),
        kind: "waiver".to_string(),
        value_json: value.to_string(),
        created_at: now.clone(),
        created_by: Some(effective_created_by),
        supersedes_uid: None,
        authored_basis_json: None,
    };

    match storage.supersede_declaration(old_uid, &new_decl) {
        Ok(result) => {
            let output = serde_json::json!({
                "old_declaration_uid": result.old_declaration_uid,
                "new_declaration_uid": result.new_declaration_uid,
                "kind": "waiver",
                "req_id": req_id,
                "requirement_version": requirement_version,
                "obligation_id": obligation_id,
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
