//! SB-7B Java integration test: end-to-end JDBC state boundary validation.
//!
//! Validates that `rmap index` (via compose::index_path) produces
//! DB_RESOURCE nodes and READS/WRITES edges when indexed Java source
//! contains `DriverManager.getConnection(String)` calls with literal
//! string arguments.
//!
//! This test exercises the FULL production path:
//!   compose::index_path
//!     -> StateBoundaryHook (constructed in compose)
//!     -> orchestrator::index_repo with hook
//!     -> java-extractor produces ResolvedCallsite (JE-1)
//!     -> hook.on_extraction_result -> JavaAdapter -> state-extractor emit
//!     -> hook.drain_snapshot_extras -> merged into persistence
//!     -> SQLite DB contains DB_RESOURCE nodes + READS/WRITES edges
//!
//! Test coverage:
//!   1. Literal JDBC URL produces DB_RESOURCE node
//!   2. stable_key is URL-encoded, name is decoded for display
//!   3. read_write direction produces both READS and WRITES edges
//!   4. Dynamic URL arguments produce no state-boundary facts

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

// ── Happy path: literal JDBC URL produces DB_RESOURCE ──────────────

#[test]
fn index_java_jdbc_literal_produces_db_resource_node() {
	let source = r#"
package com.example;

import java.sql.DriverManager;
import java.sql.Connection;
import java.sql.SQLException;

public class App {
    public void connect() throws SQLException {
        Connection conn = DriverManager.getConnection("jdbc:h2:mem:testdb");
    }
}
"#;
	let (_dir, repo) = temp_repo(&[("src/main/java/com/example/App.java", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.expect("indexing must succeed");

	assert!(result.nodes_total > 0, "should have at least file + symbol nodes");

	// Open the DB and verify state-boundary facts are present.
	let storage = StorageConnection::open(&db_path).unwrap();
	let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

	let db_resource_nodes: Vec<_> = nodes
		.iter()
		.filter(|n| n.kind == "DB_RESOURCE")
		.collect();

	assert_eq!(
		db_resource_nodes.len(),
		1,
		"expected exactly one DB_RESOURCE node for jdbc:h2:mem:testdb, got: {:?}",
		db_resource_nodes.iter().map(|n| &n.stable_key).collect::<Vec<_>>()
	);

	// Stable key should be URL-encoded.
	assert!(
		db_resource_nodes[0].stable_key.contains("jdbc%3Ah2%3Amem%3Atestdb"),
		"stable_key should contain URL-encoded JDBC URL, got: {}",
		db_resource_nodes[0].stable_key
	);
	assert_eq!(db_resource_nodes[0].subtype.as_deref(), Some("CONNECTION"));
}

// ── Display name is decoded ────────────────────────────────────────

#[test]
fn index_java_jdbc_resource_name_is_decoded() {
	let source = r#"
package com.example;

import java.sql.DriverManager;
import java.sql.Connection;
import java.sql.SQLException;

public class App {
    public void connect() throws SQLException {
        Connection conn = DriverManager.getConnection("jdbc:postgresql://localhost:5432/mydb");
    }
}
"#;
	let (_dir, repo) = temp_repo(&[("src/main/java/com/example/App.java", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.unwrap();

	let storage = StorageConnection::open(&db_path).unwrap();

	// Use list_resources which decodes the name for display.
	let resources = storage.list_resources(&result.snapshot_uid, Some("DB_RESOURCE")).unwrap();

	assert_eq!(resources.len(), 1, "expected one DB_RESOURCE");

	// Name should be decoded for display (SB-7B display contract).
	assert_eq!(
		resources[0].name,
		"jdbc:postgresql://localhost:5432/mydb",
		"name should be decoded for display"
	);

	// Stable key should remain encoded.
	assert!(
		resources[0].stable_key.contains("jdbc%3Apostgresql%3A"),
		"stable_key should remain URL-encoded, got: {}",
		resources[0].stable_key
	);
}

// ── read_write binding produces both READS and WRITES edges ────────

#[test]
fn index_java_jdbc_produces_reads_and_writes_edges() {
	let source = r#"
package com.example;

import java.sql.DriverManager;
import java.sql.Connection;
import java.sql.SQLException;

public class App {
    public void connect() throws SQLException {
        Connection conn = DriverManager.getConnection("jdbc:h2:mem:testdb");
    }
}
"#;
	let (_dir, repo) = temp_repo(&[("src/main/java/com/example/App.java", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.unwrap();

	let storage = StorageConnection::open(&db_path).unwrap();

	// Check edge counts via list_resources.
	let resources = storage.list_resources(&result.snapshot_uid, Some("DB_RESOURCE")).unwrap();
	assert_eq!(resources.len(), 1);

	// read_write binding should produce one reader and one writer.
	assert_eq!(
		resources[0].readers,
		1,
		"read_write binding should produce READS edge"
	);
	assert_eq!(
		resources[0].writers,
		1,
		"read_write binding should produce WRITES edge"
	);
}

// ── Negative: dynamic URL produces no state-boundary facts ─────────

#[test]
fn index_java_jdbc_dynamic_url_produces_no_resource() {
	let source = r#"
package com.example;

import java.sql.DriverManager;
import java.sql.Connection;
import java.sql.SQLException;

public class App {
    public void connect(String url) throws SQLException {
        Connection conn = DriverManager.getConnection(url);
    }
}
"#;
	let (_dir, repo) = temp_repo(&[("src/main/java/com/example/App.java", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.unwrap();

	let storage = StorageConnection::open(&db_path).unwrap();
	let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

	let db_resource_nodes: Vec<_> = nodes
		.iter()
		.filter(|n| n.kind == "DB_RESOURCE")
		.collect();

	assert!(
		db_resource_nodes.is_empty(),
		"dynamic URL must produce no DB_RESOURCE nodes, got: {:?}",
		db_resource_nodes.iter().map(|n| &n.stable_key).collect::<Vec<_>>()
	);
}

// ── Multiple literal URLs produce multiple DB_RESOURCE nodes ───────

#[test]
fn index_java_jdbc_multiple_urls_produce_multiple_resources() {
	let source = r#"
package com.example;

import java.sql.DriverManager;
import java.sql.Connection;
import java.sql.SQLException;

public class App {
    public void connectH2() throws SQLException {
        Connection conn = DriverManager.getConnection("jdbc:h2:mem:testdb");
    }

    public void connectPostgres() throws SQLException {
        Connection conn = DriverManager.getConnection("jdbc:postgresql://localhost/mydb");
    }
}
"#;
	let (_dir, repo) = temp_repo(&[("src/main/java/com/example/App.java", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.unwrap();

	let storage = StorageConnection::open(&db_path).unwrap();
	let resources = storage.list_resources(&result.snapshot_uid, Some("DB_RESOURCE")).unwrap();

	assert_eq!(
		resources.len(),
		2,
		"two different JDBC URLs should produce two DB_RESOURCE nodes"
	);

	let names: Vec<&str> = resources.iter().map(|r| r.name.as_str()).collect();
	assert!(names.contains(&"jdbc:h2:mem:testdb"), "H2 URL missing: {:?}", names);
	assert!(names.contains(&"jdbc:postgresql://localhost/mydb"), "PostgreSQL URL missing: {:?}", names);
}
