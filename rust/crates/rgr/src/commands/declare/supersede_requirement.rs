//! Declare supersede requirement command.
//!
//! Supersedes an existing requirement declaration with a new one.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_supersede_requirement` handler
//! - supersede requirement argument parsing
//! - `SUPERSEDE_REQUIREMENT_USAGE` constant
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration supersede (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::{parse_flag_value, VALID_OPERATORS};
use crate::cli::{open_storage, utc_now_iso8601};

const SUPERSEDE_REQUIREMENT_USAGE: &str =
    "usage: rmap declare supersede requirement <db_path> <old_declaration_uid> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]";

pub(super) fn run_declare_supersede_requirement(args: &[String]) -> ExitCode {
    let mut positional = Vec::new();
    let mut obligation_id: Option<String> = None;
    let mut method: Option<String> = None;
    let mut obligation: Option<String> = None;
    let mut target: Option<String> = None;
    let mut threshold: Option<String> = None;
    let mut operator: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--obligation-id" => {
                match parse_flag_value("--obligation-id", &obligation_id, args, &mut i) {
                    Some(v) => obligation_id = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            "--method" => match parse_flag_value("--method", &method, args, &mut i) {
                Some(v) => method = Some(v),
                None => return ExitCode::from(1),
            },
            "--obligation" => match parse_flag_value("--obligation", &obligation, args, &mut i) {
                Some(v) => obligation = Some(v),
                None => return ExitCode::from(1),
            },
            "--target" => match parse_flag_value("--target", &target, args, &mut i) {
                Some(v) => target = Some(v),
                None => return ExitCode::from(1),
            },
            "--threshold" => match parse_flag_value("--threshold", &threshold, args, &mut i) {
                Some(v) => threshold = Some(v),
                None => return ExitCode::from(1),
            },
            "--operator" => match parse_flag_value("--operator", &operator, args, &mut i) {
                Some(v) => operator = Some(v),
                None => return ExitCode::from(1),
            },
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag: {}", other);
                eprintln!("{}", SUPERSEDE_REQUIREMENT_USAGE);
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    if positional.len() != 2 {
        eprintln!("{}", SUPERSEDE_REQUIREMENT_USAGE);
        return ExitCode::from(1);
    }

    // Validate required flags.
    let obligation_id = match obligation_id {
        Some(v) => v,
        None => {
            eprintln!("error: --obligation-id is required");
            return ExitCode::from(1);
        }
    };
    let method = match method {
        Some(v) => v,
        None => {
            eprintln!("error: --method is required");
            return ExitCode::from(1);
        }
    };
    let obligation = match obligation {
        Some(v) => v,
        None => {
            eprintln!("error: --obligation is required");
            return ExitCode::from(1);
        }
    };

    // Validate optional typed fields.
    let threshold_num: Option<f64> = match threshold {
        Some(ref t) => match t.parse() {
            Ok(v) => Some(v),
            Err(_) => {
                eprintln!("error: --threshold must be a number, got: {}", t);
                return ExitCode::from(1);
            }
        },
        None => None,
    };
    if let Some(ref op) = operator {
        if !VALID_OPERATORS.contains(&op.as_str()) {
            eprintln!(
                "error: --operator must be one of {:?}, got: {}",
                VALID_OPERATORS, op
            );
            return ExitCode::from(1);
        }
    }

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
    if old_row.kind != "requirement" {
        eprintln!(
            "error: declaration {} is kind '{}', expected 'requirement'",
            old_uid, old_row.kind
        );
        return ExitCode::from(2);
    }

    // Parse old value_json to extract req_id and version.
    let old_value: serde_json::Value = match serde_json::from_str(&old_row.value_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: old requirement has malformed value_json: {}", e);
            return ExitCode::from(2);
        }
    };
    let req_id = match old_value["req_id"].as_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("error: old requirement missing req_id in value_json");
            return ExitCode::from(2);
        }
    };
    let version = match old_value["version"].as_i64() {
        Some(v) => v,
        None => {
            eprintln!("error: old requirement missing version in value_json");
            return ExitCode::from(2);
        }
    };

    // Build replacement obligation.
    let mut obl = serde_json::json!({
        "obligation_id": obligation_id,
        "obligation": obligation,
        "method": method,
    });
    if let Some(ref t) = target {
        obl["target"] = serde_json::Value::String(t.clone());
    }
    if let Some(t) = threshold_num {
        obl["threshold"] = serde_json::json!(t);
    }
    if let Some(ref op) = operator {
        obl["operator"] = serde_json::Value::String(op.clone());
    }

    let value = serde_json::json!({
        "req_id": req_id,
        "version": version,
        "verification": [obl],
    });

    use repo_graph_storage::crud::declarations::{requirement_identity_key, DeclarationInsert};

    let now = utc_now_iso8601();

    let new_decl = DeclarationInsert {
        identity_key: requirement_identity_key(&old_row.repo_uid, &req_id, version),
        repo_uid: old_row.repo_uid.clone(),
        target_stable_key: old_row.target_stable_key.clone(),
        kind: "requirement".to_string(),
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
                "kind": "requirement",
                "req_id": req_id,
                "version": version,
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
