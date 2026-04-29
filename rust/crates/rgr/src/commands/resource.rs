//! Resource command family (SB-5).
//!
//! Queries resource readers and writers from the graph.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};

/// Run the `rmap resource` command dispatcher.
///
/// Usage:
/// - `rmap resource readers <db_path> <repo_uid> <resource_stable_key>`
/// - `rmap resource writers <db_path> <repo_uid> <resource_stable_key>`
pub fn run_resource(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage:");
        eprintln!("  rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
        eprintln!("  rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "readers" => run_resource_readers(&args[1..]),
        "writers" => run_resource_writers(&args[1..]),
        other => {
            eprintln!("unknown resource subcommand: {}", other);
            eprintln!("usage:");
            eprintln!("  rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
            eprintln!("  rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
            ExitCode::from(1)
        }
    }
}

/// Run `rmap resource readers`.
fn run_resource_readers(args: &[String]) -> ExitCode {
    if args.len() != 3 {
        eprintln!("usage: rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];
    let resource_key = &args[2];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve resource (exact stable_key, must be resource kind).
    use repo_graph_storage::queries::ResourceResolveError;
    let target = match storage.resolve_resource(&snapshot.snapshot_uid, resource_key) {
        Ok(r) => r,
        Err(ResourceResolveError::NotFound) => {
            eprintln!("error: resource not found: {}", resource_key);
            return ExitCode::from(2);
        }
        Err(ResourceResolveError::NotAResource(kind)) => {
            eprintln!(
                "error: '{}' is not a resource node (kind: {}). \
                 Expected FS_PATH, DB_RESOURCE, BLOB, or STATE+CACHE.",
                resource_key, kind
            );
            return ExitCode::from(2);
        }
        Err(ResourceResolveError::Storage(e)) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Find readers.
    let readers = match storage.find_resource_readers(&snapshot.snapshot_uid, &target.stable_key) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON output (QueryResult envelope).
    let count = readers.len();
    let mut extra = serde_json::Map::new();
    extra.insert("target".to_string(), serde_json::json!(target.stable_key));
    let output = match build_envelope(
        &storage,
        "resource readers",
        repo_uid,
        &snapshot,
        serde_json::to_value(&readers).unwrap(),
        count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Run `rmap resource writers`.
fn run_resource_writers(args: &[String]) -> ExitCode {
    if args.len() != 3 {
        eprintln!("usage: rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];
    let resource_key = &args[2];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve resource (exact stable_key, must be resource kind).
    use repo_graph_storage::queries::ResourceResolveError;
    let target = match storage.resolve_resource(&snapshot.snapshot_uid, resource_key) {
        Ok(r) => r,
        Err(ResourceResolveError::NotFound) => {
            eprintln!("error: resource not found: {}", resource_key);
            return ExitCode::from(2);
        }
        Err(ResourceResolveError::NotAResource(kind)) => {
            eprintln!(
                "error: '{}' is not a resource node (kind: {}). \
                 Expected FS_PATH, DB_RESOURCE, BLOB, or STATE+CACHE.",
                resource_key, kind
            );
            return ExitCode::from(2)
        }
        Err(ResourceResolveError::Storage(e)) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Find writers.
    let writers = match storage.find_resource_writers(&snapshot.snapshot_uid, &target.stable_key) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON output (QueryResult envelope).
    let count = writers.len();
    let mut extra = serde_json::Map::new();
    extra.insert("target".to_string(), serde_json::json!(target.stable_key));
    let output = match build_envelope(
        &storage,
        "resource writers",
        repo_uid,
        &snapshot,
        serde_json::to_value(&writers).unwrap(),
        count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
