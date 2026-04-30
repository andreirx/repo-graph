//! Declare requirement command.
//!
//! Creates requirement declarations with verification obligations.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_requirement` handler
//! - requirement-specific argument parsing
//! - `DECLARE_REQUIREMENT_USAGE` constant
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration insertion (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::{parse_flag_value, VALID_OPERATORS};
use crate::cli::{open_storage, utc_now_iso8601};

const DECLARE_REQUIREMENT_USAGE: &str =
    "usage: rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]";

pub(super) fn run_declare_requirement(args: &[String]) -> ExitCode {
    let mut positional = Vec::new();
    let mut version: Option<String> = None;
    let mut obligation_id: Option<String> = None;
    let mut method: Option<String> = None;
    let mut obligation: Option<String> = None;
    let mut target: Option<String> = None;
    let mut threshold: Option<String> = None;
    let mut operator: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--version" => match parse_flag_value("--version", &version, args, &mut i) {
                Some(v) => version = Some(v),
                None => return ExitCode::from(1),
            },
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
                eprintln!("{}", DECLARE_REQUIREMENT_USAGE);
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    // Validate positional args: db_path, repo_uid, req_id.
    if positional.len() != 3 {
        eprintln!("{}", DECLARE_REQUIREMENT_USAGE);
        return ExitCode::from(1);
    }

    // Validate required flags.
    let version_str = match version {
        Some(v) => v,
        None => {
            eprintln!("error: --version is required");
            return ExitCode::from(1);
        }
    };
    let version_num: i64 = match version_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("error: --version must be an integer, got: {}", version_str);
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
    let repo_uid = positional[1].as_str();
    let req_id = positional[2].as_str();

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Build obligation object.
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
        "version": version_num,
        "verification": [obl],
    });

    use repo_graph_storage::crud::declarations::{requirement_identity_key, DeclarationInsert};

    let target_stable_key = format!("{}:requirement:{}:{}", repo_uid, req_id, version_num);
    let now = utc_now_iso8601();

    let decl = DeclarationInsert {
        identity_key: requirement_identity_key(repo_uid, req_id, version_num),
        repo_uid: repo_uid.to_string(),
        target_stable_key,
        kind: "requirement".to_string(),
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
                "kind": "requirement",
                "req_id": req_id,
                "version": version_num,
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
