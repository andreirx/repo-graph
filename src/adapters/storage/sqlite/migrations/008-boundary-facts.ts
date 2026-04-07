/**
 * Migration 008: Boundary interaction fact tables + derived link table.
 *
 * Three tables, two trust tiers:
 *
 *   SOURCE OF TRUTH (raw facts):
 *     - boundary_provider_facts: something exposes an entry surface
 *     - boundary_consumer_facts: something reaches across a boundary
 *
 *   DERIVED ARTIFACT (materialized convenience):
 *     - boundary_links: matched provider-consumer pairs
 *
 * Architectural contract:
 *   - Raw facts are the durable substrate. They persist across
 *     re-indexing and re-matching.
 *   - Derived links are discardable. They can be recomputed from
 *     raw facts at any time. A re-index or explicit command may
 *     regenerate them.
 *   - Derived links NEVER contaminate the core edges table.
 *     Boundary links are NOT language edges. They live in a
 *     separate table with explicit query surfaces.
 *
 * Matching key:
 *   Both fact tables carry a `matcher_key` column — a normalized
 *   operation identifier computed at insert time by the boundary
 *   matcher strategy for the relevant mechanism. For HTTP, this
 *   is the method + path with parameter segments normalized to
 *   wildcard tokens (e.g. "GET /api/v2/products/{_}"). The raw
 *   address/operation fields are preserved verbatim.
 *
 *   The matcher_key enables efficient SQL-level matching (index
 *   join) between providers and consumers of the same mechanism.
 *   Different mechanisms produce different key formats — the
 *   column is opaque to storage; semantics are owned by the
 *   mechanism-specific match strategy.
 *
 * No FK on handler/caller stable keys:
 *   Facts are independent observations. They must survive graph
 *   re-indexing. The handler_stable_key / caller_stable_key
 *   columns reference graph nodes but are NOT foreign keys.
 *   This matches the unresolved_edges precedent where target_key
 *   has no FK.
 *
 * FK policy on derived links:
 *   boundary_links references both fact tables. Plain REFERENCES
 *   without CASCADE, same discipline as edges/unresolved_edges.
 *   Deletion of derived links is explicit (DELETE by snapshot).
 *
 * Provenance:
 *   - observed_at: when the fact was extracted. Immutable.
 *   - extractor: which adapter + version produced the fact.
 *   - materialized_at: when a derived link was computed. Mutable
 *     on re-materialization.
 */

import type Database from "better-sqlite3";

export function runMigration008(db: Database.Database): void {
	// ── Provider facts (source of truth) ──────────────────────────────

	db.exec(`
		CREATE TABLE IF NOT EXISTS boundary_provider_facts (
			fact_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			mechanism           TEXT NOT NULL,
			operation           TEXT NOT NULL,
			address             TEXT NOT NULL,
			matcher_key         TEXT NOT NULL,
			handler_stable_key  TEXT NOT NULL,
			source_file         TEXT NOT NULL,
			line_start          INTEGER NOT NULL,
			framework           TEXT NOT NULL,
			basis               TEXT NOT NULL,
			schema_ref          TEXT,
			metadata_json       TEXT,
			extractor           TEXT NOT NULL,
			observed_at         TEXT NOT NULL
		)
	`);

	// Primary query pattern: "all providers in this snapshot for this mechanism."
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bp_facts_snapshot_mechanism ON boundary_provider_facts(snapshot_uid, mechanism)",
	);
	// Matching pattern: "find providers with this matcher_key."
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bp_facts_snapshot_matcher_key ON boundary_provider_facts(snapshot_uid, matcher_key)",
	);
	// Lookup by handler: "which routes does this symbol handle?"
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bp_facts_snapshot_handler ON boundary_provider_facts(snapshot_uid, handler_stable_key)",
	);

	// ── Consumer facts (source of truth) ──────────────────────────────

	db.exec(`
		CREATE TABLE IF NOT EXISTS boundary_consumer_facts (
			fact_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			mechanism           TEXT NOT NULL,
			operation           TEXT NOT NULL,
			address             TEXT NOT NULL,
			matcher_key         TEXT NOT NULL,
			caller_stable_key   TEXT NOT NULL,
			source_file         TEXT NOT NULL,
			line_start          INTEGER NOT NULL,
			basis               TEXT NOT NULL,
			confidence          REAL NOT NULL,
			schema_ref          TEXT,
			metadata_json       TEXT,
			extractor           TEXT NOT NULL,
			observed_at         TEXT NOT NULL
		)
	`);

	// Primary query pattern: "all consumers in this snapshot for this mechanism."
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bc_facts_snapshot_mechanism ON boundary_consumer_facts(snapshot_uid, mechanism)",
	);
	// Matching pattern: "find consumers with this matcher_key."
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bc_facts_snapshot_matcher_key ON boundary_consumer_facts(snapshot_uid, matcher_key)",
	);
	// Lookup by caller: "what boundary calls does this symbol make?"
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bc_facts_snapshot_caller ON boundary_consumer_facts(snapshot_uid, caller_stable_key)",
	);

	// ── Derived links (materialized convenience) ──────────────────────
	//
	// EXPLICITLY DERIVED. These rows can be deleted and recomputed
	// from raw facts without data loss. Do not treat as source of truth.

	db.exec(`
		CREATE TABLE IF NOT EXISTS boundary_links (
			link_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			provider_fact_uid   TEXT NOT NULL REFERENCES boundary_provider_facts(fact_uid),
			consumer_fact_uid   TEXT NOT NULL REFERENCES boundary_consumer_facts(fact_uid),
			match_basis         TEXT NOT NULL,
			confidence          REAL NOT NULL,
			metadata_json       TEXT,
			materialized_at     TEXT NOT NULL
		)
	`);

	// Primary query: "all links in this snapshot."
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bl_snapshot ON boundary_links(snapshot_uid)",
	);
	// Lookup by provider: "which consumers matched this provider?"
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bl_provider ON boundary_links(provider_fact_uid)",
	);
	// Lookup by consumer: "which provider did this consumer match?"
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_bl_consumer ON boundary_links(consumer_fact_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (8, '008-boundary-facts', datetime('now'))",
	);
}
