//! FD-1A Express integration test: end-to-end Express route detection validation.
//!
//! Validates that `rmap index` (via compose::index_path) produces
//! http_provider surfaces with evidence when indexed TypeScript source
//! contains Express route registrations (`app.get()`, `router.post()`, etc.).
//!
//! This test exercises the FULL production path:
//!   compose::index_path
//!     -> orchestrator::index_repo (extracts files)
//!     -> persist_npm_modules (creates module candidates)
//!     -> persist_express_surfaces (detects routes, persists surfaces + evidence)
//!     -> SQLite DB contains project_surfaces + project_surface_evidence rows
//!
//! Test coverage:
//!   1. Basic route detection (app.get, router.post)
//!   2. Path parameter normalization (`:id` -> `{id}`)
//!   3. Evidence persistence (evidence_count > 0)
//!   4. Module resolution (surfaces linked to npm module)
//!   5. Negative cases (non-Express receivers ignored)

use std::fs;
use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_path, ComposeOptions};
use repo_graph_storage::crud::project_surfaces::SurfaceFilter;
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

// ── Happy path: Express routes produce http_provider surfaces ──────

#[test]
fn index_express_routes_produces_http_provider_surfaces() {
	let package_json = r#"{
  "name": "test-express-app",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0"
  }
}"#;

	let source = r#"
import express from 'express';

const app = express();

app.get('/api/users', (req, res) => {
  res.json({ users: [] });
});

app.post('/api/users', (req, res) => {
  res.status(201).json({ created: true });
});

app.listen(3000);
"#;

	let (_dir, repo) = temp_repo(&[
		("package.json", package_json),
		("src/index.ts", source),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
		.expect("indexing must succeed");

	assert!(result.nodes_total > 0, "should have file + symbol nodes");

	// Open the DB and verify surfaces are present.
	let storage = StorageConnection::open(&db_path).unwrap();
	let filter = SurfaceFilter {
		kind: Some("http_provider".to_string()),
		..Default::default()
	};
	let surfaces = storage
		.get_project_surfaces_for_snapshot(&result.snapshot_uid, &filter)
		.expect("query surfaces");

	assert_eq!(
		surfaces.len(),
		2,
		"expected 2 http_provider surfaces (GET + POST), got: {:?}",
		surfaces.iter().map(|s| &s.display_name).collect::<Vec<_>>()
	);

	// Verify surface properties.
	let get_surface = surfaces.iter().find(|s| s.display_name.as_deref() == Some("GET /api/users"));
	assert!(get_surface.is_some(), "GET /api/users surface should exist");

	let post_surface = surfaces.iter().find(|s| s.display_name.as_deref() == Some("POST /api/users"));
	assert!(post_surface.is_some(), "POST /api/users surface should exist");

	// Verify module linkage.
	let get = get_surface.unwrap();
	assert!(
		get.module_candidate_uid.starts_with("npm-mod-"),
		"surface should be linked to npm module, got: {}",
		get.module_candidate_uid
	);
}

// ── Path parameter normalization ───────────────────────────────────

#[test]
fn express_routes_normalize_path_params() {
	let package_json = r#"{"name": "test-app", "dependencies": {"express": "^4.18.0"}}"#;

	let source = r#"
import express from 'express';
const app = express();
app.get('/users/:userId/posts/:postId', (req, res) => res.json({}));
"#;

	let (_dir, repo) = temp_repo(&[
		("package.json", package_json),
		("app.ts", source),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
		.expect("indexing must succeed");

	let storage = StorageConnection::open(&db_path).unwrap();
	let filter = SurfaceFilter {
		kind: Some("http_provider".to_string()),
		..Default::default()
	};
	let surfaces = storage
		.get_project_surfaces_for_snapshot(&result.snapshot_uid, &filter)
		.expect("query surfaces");

	assert_eq!(surfaces.len(), 1);
	assert_eq!(
		surfaces[0].display_name.as_deref(),
		Some("GET /users/{userId}/posts/{postId}"),
		"path params should be normalized"
	);
}

// ── Evidence persistence ───────────────────────────────────────────

#[test]
fn express_routes_have_evidence_persisted() {
	let package_json = r#"{"name": "test-app", "dependencies": {"express": "^4.18.0"}}"#;

	let source = r#"
import express from 'express';
const app = express();
app.get('/health', (req, res) => res.json({ ok: true }));
"#;

	let (_dir, repo) = temp_repo(&[
		("package.json", package_json),
		("app.ts", source),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
		.expect("indexing must succeed");

	let storage = StorageConnection::open(&db_path).unwrap();
	let filter = SurfaceFilter {
		kind: Some("http_provider".to_string()),
		..Default::default()
	};
	let surfaces = storage
		.get_project_surfaces_for_snapshot(&result.snapshot_uid, &filter)
		.expect("query surfaces");

	assert_eq!(surfaces.len(), 1);

	// Query evidence for this surface.
	let evidence = storage
		.get_project_surface_evidence(&surfaces[0].project_surface_uid)
		.expect("query evidence");

	assert_eq!(
		evidence.len(),
		1,
		"each surface should have exactly one evidence record"
	);
	assert_eq!(evidence[0].evidence_kind, "route_registration");
	assert_eq!(evidence[0].source_type, "code_detection");
	assert!(evidence[0].source_path.ends_with("app.ts"));
}

// ── Negative: non-Express receivers ignored ────────────────────────

#[test]
fn non_express_receivers_ignored() {
	// No express import, no package.json dependency.
	let package_json = r#"{"name": "test-app", "dependencies": {}}"#;

	let source = r#"
const cache = new Map();
cache.get('/api/users');
cache.set('/api/users', []);
"#;

	let (_dir, repo) = temp_repo(&[
		("package.json", package_json),
		("app.ts", source),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(&repo, &db_path, "test-app", &ComposeOptions::default())
		.expect("indexing must succeed");

	let storage = StorageConnection::open(&db_path).unwrap();
	let filter = SurfaceFilter {
		kind: Some("http_provider".to_string()),
		..Default::default()
	};
	let surfaces = storage
		.get_project_surfaces_for_snapshot(&result.snapshot_uid, &filter)
		.expect("query surfaces");

	assert!(
		surfaces.is_empty(),
		"non-Express receivers should not produce surfaces"
	);
}

// ── Directory-boundary-safe module resolution ──────────────────────

#[test]
fn module_resolution_is_directory_boundary_safe() {
	// Two packages: packages/app and packages/app2
	// Files in packages/app2 should NOT be assigned to packages/app.
	let root_package = r#"{"name": "monorepo", "workspaces": ["packages/*"]}"#;
	let app_package = r#"{"name": "app", "dependencies": {"express": "^4.18.0"}}"#;
	let app2_package = r#"{"name": "app2", "dependencies": {"express": "^4.18.0"}}"#;

	let app_source = r#"
import express from 'express';
const app = express();
app.get('/app-route', (req, res) => res.json({}));
"#;

	let app2_source = r#"
import express from 'express';
const app = express();
app.get('/app2-route', (req, res) => res.json({}));
"#;

	let (_dir, repo) = temp_repo(&[
		("package.json", root_package),
		("packages/app/package.json", app_package),
		("packages/app/index.ts", app_source),
		("packages/app2/package.json", app2_package),
		("packages/app2/index.ts", app2_source),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(&repo, &db_path, "monorepo", &ComposeOptions::default())
		.expect("indexing must succeed");

	let storage = StorageConnection::open(&db_path).unwrap();
	let filter = SurfaceFilter {
		kind: Some("http_provider".to_string()),
		..Default::default()
	};
	let surfaces = storage
		.get_project_surfaces_for_snapshot(&result.snapshot_uid, &filter)
		.expect("query surfaces");

	assert_eq!(
		surfaces.len(),
		2,
		"should have 2 surfaces (one per package)"
	);

	// Verify different module UIDs.
	let app_surface = surfaces.iter().find(|s| s.display_name.as_deref() == Some("GET /app-route"));
	let app2_surface = surfaces.iter().find(|s| s.display_name.as_deref() == Some("GET /app2-route"));

	assert!(app_surface.is_some() && app2_surface.is_some());
	assert_ne!(
		app_surface.unwrap().module_candidate_uid,
		app2_surface.unwrap().module_candidate_uid,
		"surfaces in different packages must have different module UIDs"
	);
}
