//! Declare quality-policy command.
//!
//! Creates quality policy declarations with measurement thresholds.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_declare_quality_policy` handler
//! - quality-policy-specific argument parsing
//! - `DECLARE_QUALITY_POLICY_USAGE` constant
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - declaration insertion (belongs in `repo-graph-storage`)
//! - policy validation (belongs in `repo-graph-quality-policy`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::{parse_flag_value, parse_repeatable_flag_value};
use crate::cli::{open_storage, utc_now_iso8601};

const DECLARE_QUALITY_POLICY_USAGE: &str = "usage: rmap declare quality-policy <db_path> <repo_uid> <policy_id> \\
  --measurement <kind> --policy-kind <kind> --threshold <n> [--version <n>] \\
  [--severity <fail|advisory>] [--scope-clause <type>:<selector>]... [--description <text>]";

pub(super) fn run_declare_quality_policy(args: &[String]) -> ExitCode {
    use repo_graph_quality_policy::{
        parse_measurement_kind, validate_quality_policy_payload, SupportedMeasurementKind,
    };
    use repo_graph_storage::crud::declarations::{quality_policy_identity_key, DeclarationInsert};
    use repo_graph_storage::types::{
        QualityPolicyKind, QualityPolicyPayload, QualityPolicySeverity, ScopeClause,
        ScopeClauseKind,
    };

    let mut positional = Vec::new();
    let mut version: Option<String> = None;
    let mut measurement: Option<String> = None;
    let mut policy_kind: Option<String> = None;
    let mut threshold: Option<String> = None;
    let mut severity: Option<String> = None;
    let mut scope_clauses_raw: Vec<String> = Vec::new();
    let mut description: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--version" => match parse_flag_value("--version", &version, args, &mut i) {
                Some(v) => version = Some(v),
                None => return ExitCode::from(1),
            },
            "--measurement" => {
                match parse_flag_value("--measurement", &measurement, args, &mut i) {
                    Some(v) => measurement = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            "--policy-kind" => {
                match parse_flag_value("--policy-kind", &policy_kind, args, &mut i) {
                    Some(v) => policy_kind = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            "--threshold" => match parse_flag_value("--threshold", &threshold, args, &mut i) {
                Some(v) => threshold = Some(v),
                None => return ExitCode::from(1),
            },
            "--severity" => match parse_flag_value("--severity", &severity, args, &mut i) {
                Some(v) => severity = Some(v),
                None => return ExitCode::from(1),
            },
            "--scope-clause" => {
                match parse_repeatable_flag_value("--scope-clause", args, &mut i) {
                    Some(v) => scope_clauses_raw.push(v),
                    None => return ExitCode::from(1),
                }
            }
            "--description" => {
                match parse_flag_value("--description", &description, args, &mut i) {
                    Some(v) => description = Some(v),
                    None => return ExitCode::from(1),
                }
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag: {}", other);
                eprintln!("{}", DECLARE_QUALITY_POLICY_USAGE);
                return ExitCode::from(1);
            }
            _ => positional.push(&args[i]),
        }
        i += 1;
    }

    // Validate positional args: db_path, repo_uid, policy_id.
    if positional.len() != 3 {
        eprintln!("{}", DECLARE_QUALITY_POLICY_USAGE);
        return ExitCode::from(1);
    }

    let db_path = Path::new(positional[0].as_str());
    let repo_uid = positional[1].as_str();
    let policy_id = positional[2].as_str();

    if policy_id.trim().is_empty() {
        eprintln!("error: policy_id must be non-empty");
        return ExitCode::from(1);
    }

    // Version defaults to 1.
    let version_num: i64 = match version {
        Some(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("error: --version must be an integer, got: {}", v);
                return ExitCode::from(1);
            }
        },
        None => 1,
    };

    // Validate and parse measurement kind.
    let measurement_str = match measurement {
        Some(v) => v,
        None => {
            eprintln!("error: --measurement is required");
            eprintln!(
                "supported kinds: {}",
                SupportedMeasurementKind::supported_kinds_display()
            );
            return ExitCode::from(1);
        }
    };
    if let Err(e) = parse_measurement_kind(&measurement_str) {
        eprintln!("error: {}", e);
        eprintln!(
            "supported kinds: {}",
            SupportedMeasurementKind::supported_kinds_display()
        );
        return ExitCode::from(1);
    }

    // Validate and parse policy kind.
    let policy_kind_str = match policy_kind {
        Some(v) => v,
        None => {
            eprintln!("error: --policy-kind is required");
            return ExitCode::from(1);
        }
    };
    let policy_kind_enum = match QualityPolicyKind::from_str(&policy_kind_str) {
        Some(k) => k,
        None => {
            eprintln!(
                "error: invalid --policy-kind: '{}'; valid values: absolute_max, absolute_min, no_new, no_worsened",
                policy_kind_str
            );
            return ExitCode::from(1);
        }
    };

    // Validate and parse threshold.
    let threshold_str = match threshold {
        Some(v) => v,
        None => {
            eprintln!("error: --threshold is required");
            return ExitCode::from(1);
        }
    };
    let threshold_num: f64 = match threshold_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "error: --threshold must be a number, got: {}",
                threshold_str
            );
            return ExitCode::from(1);
        }
    };

    // Parse severity (default: fail).
    let severity_enum = match severity.as_deref() {
        None | Some("fail") => QualityPolicySeverity::Fail,
        Some("advisory") => QualityPolicySeverity::Advisory,
        Some(other) => {
            eprintln!(
                "error: invalid --severity: '{}'; valid values: fail, advisory",
                other
            );
            return ExitCode::from(1);
        }
    };

    // Parse scope clauses from <type>:<selector> format.
    let mut scope_clauses = Vec::new();
    for clause_str in &scope_clauses_raw {
        let parts: Vec<&str> = clause_str.splitn(2, ':').collect();
        if parts.len() != 2 {
            eprintln!(
                "error: invalid --scope-clause format: '{}'; expected <type>:<selector>",
                clause_str
            );
            return ExitCode::from(1);
        }
        let clause_type = parts[0].trim();
        let selector = parts[1].trim();
        if selector.is_empty() {
            eprintln!(
                "error: --scope-clause selector is empty in '{}'",
                clause_str
            );
            return ExitCode::from(1);
        }
        let clause_kind = match ScopeClauseKind::from_str(clause_type) {
            Some(k) => k,
            None => {
                eprintln!(
                    "error: invalid scope clause type: '{}'; valid types: module, file, symbol_kind",
                    clause_type
                );
                return ExitCode::from(1);
            }
        };
        scope_clauses.push(ScopeClause::new(clause_kind, selector));
    }

    // Build the payload.
    let payload = QualityPolicyPayload {
        policy_id: policy_id.to_string(),
        version: version_num,
        scope_clauses,
        measurement_kind: measurement_str.clone(),
        policy_kind: policy_kind_enum,
        threshold: threshold_num,
        severity: severity_enum,
        description,
    };

    // Validate payload using the quality-policy domain crate.
    let errors = validate_quality_policy_payload(&payload);
    if !errors.is_empty() {
        for e in errors {
            eprintln!("error: {}", e);
        }
        return ExitCode::from(1);
    }

    // Open storage.
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Build the declaration.
    let target_stable_key = format!("{}:REPO", repo_uid);
    let now = utc_now_iso8601();

    let value_json = match serde_json::to_string(&payload) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: failed to serialize payload: {}", e);
            return ExitCode::from(2);
        }
    };

    let decl = DeclarationInsert {
        identity_key: quality_policy_identity_key(repo_uid, policy_id, version_num),
        repo_uid: repo_uid.to_string(),
        target_stable_key,
        kind: "quality_policy".to_string(),
        value_json,
        created_at: now,
        created_by: Some("cli".to_string()),
        supersedes_uid: None,
        authored_basis_json: None,
    };

    match storage.insert_declaration(&decl) {
        Ok(result) => {
            let output = serde_json::json!({
                "declaration_uid": result.declaration_uid,
                "kind": "quality_policy",
                "policy_id": policy_id,
                "version": version_num,
                "measurement": measurement_str,
                "policy_kind": policy_kind_str,
                "threshold": threshold_num,
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
