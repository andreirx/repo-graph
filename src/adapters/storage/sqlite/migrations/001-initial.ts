/**
 * Initial schema migration, inlined as a TypeScript constant.
 *
 * The canonical SQL source is 001-initial.sql (kept for reference).
 * This inlined version is what the adapter actually executes, so it
 * travels with the compiled output without runtime file reads.
 */
export const INITIAL_MIGRATION = `
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS repos (
  repo_uid        TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  root_path       TEXT NOT NULL,
  default_branch  TEXT,
  created_at      TEXT NOT NULL,
  metadata_json   TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_repos_root_path ON repos(root_path);

CREATE TABLE IF NOT EXISTS snapshots (
  snapshot_uid        TEXT PRIMARY KEY,
  repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  parent_snapshot_uid TEXT REFERENCES snapshots(snapshot_uid),
  kind                TEXT NOT NULL,
  basis_ref           TEXT,
  basis_commit        TEXT,
  dirty_hash          TEXT,
  status              TEXT NOT NULL,
  files_total         INTEGER NOT NULL DEFAULT 0,
  nodes_total         INTEGER NOT NULL DEFAULT 0,
  edges_total         INTEGER NOT NULL DEFAULT 0,
  created_at          TEXT NOT NULL,
  completed_at        TEXT,
  label               TEXT
);

CREATE INDEX IF NOT EXISTS idx_snapshots_repo ON snapshots(repo_uid);

CREATE TABLE IF NOT EXISTS files (
  file_uid        TEXT PRIMARY KEY,
  repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  path            TEXT NOT NULL,
  language        TEXT,
  is_test         INTEGER NOT NULL DEFAULT 0,
  is_generated    INTEGER NOT NULL DEFAULT 0,
  is_excluded     INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_files_repo_path ON files(repo_uid, path);

CREATE TABLE IF NOT EXISTS file_versions (
  snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  file_uid        TEXT NOT NULL REFERENCES files(file_uid) ON DELETE CASCADE,
  content_hash    TEXT NOT NULL,
  ast_hash        TEXT,
  extractor       TEXT,
  parse_status    TEXT NOT NULL,
  size_bytes      INTEGER,
  line_count      INTEGER,
  indexed_at      TEXT NOT NULL,
  PRIMARY KEY (snapshot_uid, file_uid)
);

CREATE INDEX IF NOT EXISTS idx_file_versions_snapshot ON file_versions(snapshot_uid);

CREATE TABLE IF NOT EXISTS nodes (
  node_uid        TEXT PRIMARY KEY,
  snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  stable_key      TEXT NOT NULL,
  kind            TEXT NOT NULL,
  subtype         TEXT,
  name            TEXT NOT NULL,
  qualified_name  TEXT,
  file_uid        TEXT REFERENCES files(file_uid),
  parent_node_uid TEXT REFERENCES nodes(node_uid),
  line_start      INTEGER,
  col_start       INTEGER,
  line_end        INTEGER,
  col_end         INTEGER,
  signature       TEXT,
  visibility      TEXT,
  doc_comment     TEXT,
  metadata_json   TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_snapshot_key ON nodes(snapshot_uid, stable_key);
CREATE INDEX IF NOT EXISTS idx_nodes_snapshot_kind ON nodes(snapshot_uid, kind);
CREATE INDEX IF NOT EXISTS idx_nodes_snapshot_file ON nodes(snapshot_uid, file_uid);
CREATE INDEX IF NOT EXISTS idx_nodes_repo_key ON nodes(repo_uid, stable_key);

CREATE TABLE IF NOT EXISTS edges (
  edge_uid        TEXT PRIMARY KEY,
  snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  source_node_uid TEXT NOT NULL REFERENCES nodes(node_uid) ON DELETE CASCADE,
  target_node_uid TEXT NOT NULL REFERENCES nodes(node_uid) ON DELETE CASCADE,
  type            TEXT NOT NULL,
  resolution      TEXT NOT NULL,
  extractor       TEXT NOT NULL,
  line_start      INTEGER,
  col_start       INTEGER,
  line_end        INTEGER,
  col_end         INTEGER,
  metadata_json   TEXT
);

CREATE INDEX IF NOT EXISTS idx_edges_snapshot_src ON edges(snapshot_uid, source_node_uid);
CREATE INDEX IF NOT EXISTS idx_edges_snapshot_dst ON edges(snapshot_uid, target_node_uid);
CREATE INDEX IF NOT EXISTS idx_edges_snapshot_type ON edges(snapshot_uid, type);
CREATE INDEX IF NOT EXISTS idx_edges_snapshot_type_src ON edges(snapshot_uid, type, source_node_uid);
CREATE INDEX IF NOT EXISTS idx_edges_snapshot_type_dst ON edges(snapshot_uid, type, target_node_uid);

CREATE TABLE IF NOT EXISTS declarations (
  declaration_uid     TEXT PRIMARY KEY,
  repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  snapshot_uid        TEXT REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  target_stable_key   TEXT NOT NULL,
  kind                TEXT NOT NULL,
  value_json          TEXT NOT NULL,
  created_at          TEXT NOT NULL,
  created_by          TEXT,
  supersedes_uid      TEXT REFERENCES declarations(declaration_uid),
  is_active           INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_declarations_target ON declarations(repo_uid, target_stable_key, kind);
CREATE INDEX IF NOT EXISTS idx_declarations_active ON declarations(repo_uid, is_active);

CREATE TABLE IF NOT EXISTS inferences (
  inference_uid       TEXT PRIMARY KEY,
  snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  target_stable_key   TEXT NOT NULL,
  kind                TEXT NOT NULL,
  value_json          TEXT NOT NULL,
  confidence          REAL NOT NULL,
  basis_json          TEXT NOT NULL,
  extractor           TEXT NOT NULL,
  created_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_inferences_target ON inferences(snapshot_uid, target_stable_key, kind);
CREATE INDEX IF NOT EXISTS idx_inferences_kind ON inferences(snapshot_uid, kind);

CREATE TABLE IF NOT EXISTS artifacts (
  artifact_uid    TEXT PRIMARY KEY,
  snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
  kind            TEXT NOT NULL,
  relative_path   TEXT NOT NULL,
  content_hash    TEXT,
  size_bytes      INTEGER,
  format          TEXT,
  created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artifacts_snapshot ON artifacts(snapshot_uid, kind);

CREATE TABLE IF NOT EXISTS evidence_links (
  evidence_link_uid   TEXT PRIMARY KEY,
  snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  subject_type        TEXT NOT NULL,
  subject_uid         TEXT NOT NULL,
  artifact_uid        TEXT NOT NULL REFERENCES artifacts(artifact_uid) ON DELETE CASCADE,
  note                TEXT
);

CREATE INDEX IF NOT EXISTS idx_evidence_subject ON evidence_links(snapshot_uid, subject_type, subject_uid);

CREATE TABLE IF NOT EXISTS schema_migrations (
  version     INTEGER PRIMARY KEY,
  name        TEXT NOT NULL,
  applied_at  TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_migrations (version, name, applied_at)
VALUES (1, '001-initial', datetime('now'));
`;
