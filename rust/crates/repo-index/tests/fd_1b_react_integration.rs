//! FD-1B React integration test: end-to-end React component and hook detection validation.
//!
//! Validates that `rmap index` (via compose::index_path) produces
//! react_component and react_hook_usage inferences when indexed TSX source
//! contains React component definitions and hook usage.
//!
//! This test exercises the FULL production path:
//!   compose::index_path
//!     -> orchestrator::index_repo (extracts files)
//!     -> persist_react_inferences (detects components/hooks, persists inferences)
//!     -> SQLite DB contains inferences rows with react_component/react_hook_usage kinds
//!
//! Test coverage:
//!   1. Component detection (function, arrow, FC typed)
//!   2. Hook detection (builtin and custom)
//!   3. Inference persistence
//!   4. Negative cases (non-React files, lowercase functions)

use std::fs;
use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_path, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn temp_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    for (path, content) in files {
        let full = repo.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    (dir, repo)
}

// ── Happy path: React components produce react_component inferences ──

#[test]
fn index_react_components_produces_inferences() {
    let package_json = r#"{
  "name": "test-react-app",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0"
  }
}"#;

    let source = r#"
import React from 'react';

function UserProfile() {
  return <div>User Profile</div>;
}

const Dashboard = () => {
  return <main>Dashboard</main>;
};

const Card: React.FC<{ title: string }> = ({ title }) => {
  return <article>{title}</article>;
};
"#;

    let (_dir, repo) = temp_repo(&[
        ("package.json", package_json),
        ("src/components.tsx", source),
    ]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
        .expect("indexing must succeed");

    assert!(result.nodes_total > 0, "should have file + symbol nodes");

    // Query inferences.
    let storage = StorageConnection::open(&db_path).unwrap();
    let component_inferences = storage
        .list_inferences_for_snapshot(&result.snapshot_uid, Some("react_component"))
        .expect("query inferences");

    assert_eq!(
        component_inferences.len(),
        3,
        "expected 3 react_component inferences, got: {:?}",
        component_inferences
            .iter()
            .map(|i| &i.target_stable_key)
            .collect::<Vec<_>>()
    );

    // Verify component names are in stable keys.
    let keys: Vec<&str> = component_inferences
        .iter()
        .map(|i| i.target_stable_key.as_str())
        .collect();
    assert!(keys.iter().any(|k| k.contains("UserProfile")));
    assert!(keys.iter().any(|k| k.contains("Dashboard")));
    assert!(keys.iter().any(|k| k.contains("Card")));
}

// ── Hook detection ───────────────────────────────────────────────────

#[test]
fn index_react_hooks_produces_inferences() {
    let package_json = r#"{"name": "test-app", "dependencies": {"react": "^18.2.0"}}"#;

    let source = r#"
import React, { useState, useEffect, useCallback } from 'react';

function Counter() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    console.log(count);
  }, [count]);
  const increment = useCallback(() => setCount(c => c + 1), []);
  return <button onClick={increment}>{count}</button>;
}
"#;

    let (_dir, repo) = temp_repo(&[("package.json", package_json), ("app.tsx", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let hook_inferences = storage
        .list_inferences_for_snapshot(&result.snapshot_uid, Some("react_hook_usage"))
        .expect("query inferences");

    assert_eq!(
        hook_inferences.len(),
        3,
        "expected 3 react_hook_usage inferences (useState, useEffect, useCallback)"
    );

    // Verify hook names in value_json.
    let hook_names: Vec<String> = hook_inferences
        .iter()
        .map(|i| {
            let v: serde_json::Value = serde_json::from_str(&i.value_json).unwrap();
            v["hook_name"].as_str().unwrap().to_string()
        })
        .collect();
    assert!(hook_names.contains(&"useState".to_string()));
    assert!(hook_names.contains(&"useEffect".to_string()));
    assert!(hook_names.contains(&"useCallback".to_string()));
}

// ── Custom hook detection ────────────────────────────────────────────

#[test]
fn index_custom_hooks_produces_inferences() {
    let package_json = r#"{"name": "test-app", "dependencies": {"react": "^18.2.0"}}"#;

    let source = r#"
import React, { useState } from 'react';

function useCustomData() {
  const [data, setData] = useState(null);
  return { data, setData };
}

function MyComponent() {
  const { data } = useCustomData();
  return <div>{JSON.stringify(data)}</div>;
}
"#;

    let (_dir, repo) = temp_repo(&[("package.json", package_json), ("hooks.tsx", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let hook_inferences = storage
        .list_inferences_for_snapshot(&result.snapshot_uid, Some("react_hook_usage"))
        .expect("query inferences");

    // Should have 2 hooks: useState (in useCustomData) and useCustomData (in MyComponent)
    assert_eq!(hook_inferences.len(), 2);

    // Verify custom hook detection.
    let hook_categories: Vec<String> = hook_inferences
        .iter()
        .map(|i| {
            let v: serde_json::Value = serde_json::from_str(&i.value_json).unwrap();
            v["hook_category"].as_str().unwrap().to_string()
        })
        .collect();
    assert!(hook_categories.contains(&"builtin".to_string())); // useState
    assert!(hook_categories.contains(&"custom".to_string())); // useCustomData
}

// ── Negative: non-React files produce no inferences ──────────────────

#[test]
fn non_react_files_produce_no_inferences() {
    let package_json = r#"{"name": "test-app", "dependencies": {}}"#;

    // Non-TSX file (no React import).
    let source = r#"
export function UserProfile() {
  return "User Profile";
}

export const useState = () => {};
"#;

    let (_dir, repo) = temp_repo(&[("package.json", package_json), ("utils.ts", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let component_inferences = storage
        .list_inferences_for_snapshot(&result.snapshot_uid, Some("react_component"))
        .expect("query inferences");
    let hook_inferences = storage
        .list_inferences_for_snapshot(&result.snapshot_uid, Some("react_hook_usage"))
        .expect("query inferences");

    assert!(
        component_inferences.is_empty(),
        "non-React files should not produce component inferences"
    );
    assert!(
        hook_inferences.is_empty(),
        "non-React files should not produce hook inferences"
    );
}

// ── Negative: lowercase functions are not components ─────────────────

#[test]
fn lowercase_functions_not_detected_as_components() {
    let package_json = r#"{"name": "test-app", "dependencies": {"react": "^18.2.0"}}"#;

    let source = r#"
import React from 'react';

// lowercase - should NOT be detected as component
function helper() {
  return <div>Helper</div>;
}

// PascalCase - should be detected
function RealComponent() {
  return <div>Real</div>;
}
"#;

    let (_dir, repo) = temp_repo(&[("package.json", package_json), ("mixed.tsx", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let component_inferences = storage
        .list_inferences_for_snapshot(&result.snapshot_uid, Some("react_component"))
        .expect("query inferences");

    // Should only have RealComponent, not helper.
    assert_eq!(component_inferences.len(), 1);
    assert!(component_inferences[0]
        .target_stable_key
        .contains("RealComponent"));
}
