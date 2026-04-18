/**
 * Canonical enums for the Repo-Graph domain model.
 * These are the vocabulary of the graph. Language-neutral, backend-neutral.
 * Nothing in this file imports from adapters, CLI, or node_modules.
 */

// ── Edge Types ─────────────────────────────────────────────────────────────

export const EdgeType = {
	// Structural
	IMPORTS: "IMPORTS",
	CALLS: "CALLS",
	IMPLEMENTS: "IMPLEMENTS",
	INSTANTIATES: "INSTANTIATES",
	// Data flow
	READS: "READS",
	WRITES: "WRITES",
	// Async / event
	EMITS: "EMITS",
	CONSUMES: "CONSUMES",
	// Framework
	ROUTES_TO: "ROUTES_TO",
	REGISTERED_BY: "REGISTERED_BY",
	GATED_BY: "GATED_BY",
	// Relational
	DEPENDS_ON: "DEPENDS_ON",
	OWNS: "OWNS",
	TESTED_BY: "TESTED_BY",
	COVERS: "COVERS",
	// Exception flow
	THROWS: "THROWS",
	CATCHES: "CATCHES",
	// State machine
	TRANSITIONS_TO: "TRANSITIONS_TO",
} as const;

export type EdgeType = (typeof EdgeType)[keyof typeof EdgeType];

// ── Edge Resolution ────────────────────────────────────────────────────────

export const Resolution = {
	/** Deterministically resolved from source code */
	STATIC: "static",
	/** Known to exist but resolved at runtime */
	DYNAMIC: "dynamic",
	/** Guessed from naming, proximity, or heuristics */
	INFERRED: "inferred",
} as const;

export type Resolution = (typeof Resolution)[keyof typeof Resolution];

// ── Node Kinds ─────────────────────────────────────────────────────────────

export const NodeKind = {
	MODULE: "MODULE",
	FILE: "FILE",
	SYMBOL: "SYMBOL",
	ENDPOINT: "ENDPOINT",
	EVENT_TOPIC: "EVENT_TOPIC",
	TABLE: "TABLE",
	CONFIG_KEY: "CONFIG_KEY",
	TEST: "TEST",
	STATE: "STATE",
	QUEUE: "QUEUE",
	JOB: "JOB",
	// ── State-boundary slice 1 (SB-2-pre) ───────────────────────────────────
	// Canonical vocabulary for state-boundary resource nodes. See
	// `docs/architecture/state-boundary-contract.txt` §4.2 for the semantic
	// definitions. These kinds are NOT emitted by any TS extractor under Fork 1.
	// On the Rust runtime, emission begins in SB-3 once `state-extractor`
	// ships. On the TS runtime, emission is deferred entirely under the
	// Rust-only posture. TS consumers of these kinds are READ-only today.
	DB_RESOURCE: "DB_RESOURCE",
	FS_PATH: "FS_PATH",
	BLOB: "BLOB",
} as const;

export type NodeKind = (typeof NodeKind)[keyof typeof NodeKind];

// ── Node Subtypes ──────────────────────────────────────────────────────────

export const NodeSubtype = {
	// SYMBOL subtypes
	FUNCTION: "FUNCTION",
	CLASS: "CLASS",
	METHOD: "METHOD",
	INTERFACE: "INTERFACE",
	TYPE_ALIAS: "TYPE_ALIAS",
	VARIABLE: "VARIABLE",
	CONSTANT: "CONSTANT",
	ENUM: "ENUM",
	ENUM_MEMBER: "ENUM_MEMBER",
	PROPERTY: "PROPERTY",
	CONSTRUCTOR: "CONSTRUCTOR",
	GETTER: "GETTER",
	SETTER: "SETTER",
	// FILE subtypes
	SOURCE: "SOURCE",
	TEST_FILE: "TEST_FILE",
	CONFIG: "CONFIG",
	MIGRATION: "MIGRATION",
	SCHEMA: "SCHEMA",
	// MODULE subtypes
	PACKAGE: "PACKAGE",
	NAMESPACE: "NAMESPACE",
	DIRECTORY: "DIRECTORY",
	// ENDPOINT subtypes
	ROUTE: "ROUTE",
	RPC_METHOD: "RPC_METHOD",
	GRAPHQL_RESOLVER: "GRAPHQL_RESOLVER",
	WEBSOCKET_HANDLER: "WEBSOCKET_HANDLER",
	// TEST subtypes
	TEST_SUITE: "TEST_SUITE",
	TEST_CASE: "TEST_CASE",
	// ── State-boundary slice 1 (SB-2-pre-2) ───────────────────────────────
	// Resource-kind subtypes. Not emitted by any TS extractor under Fork 1
	// (the Rust runtime emits these via the future `state-extractor` in SB-2).
	// TS consumers are READ-only today. The pre-existing NAMESPACE entry is
	// reused for BLOB-namespace contexts (single shared string value).
	// DB_RESOURCE subtype — logical database connection / data source.
	CONNECTION: "CONNECTION",
	// FS_PATH subtype — literal filesystem file path.
	FILE_PATH: "FILE_PATH",
	// FS_PATH subtype — literal filesystem directory path.
	DIRECTORY_PATH: "DIRECTORY_PATH",
	// FS_PATH subtype — logical (config/env-derived) FS resource name.
	LOGICAL: "LOGICAL",
	// STATE subtype — cache endpoint (Redis, Memcached, etc.). The existing
	// STATE_VALUE / STATE_FIELD subtypes remain reserved.
	CACHE: "CACHE",
	// BLOB subtype — object-storage bucket.
	BUCKET: "BUCKET",
	// BLOB subtype — Azure Blob Storage container (and similarly-named
	// providers).
	CONTAINER: "CONTAINER",
} as const;

export type NodeSubtype = (typeof NodeSubtype)[keyof typeof NodeSubtype];

// ── Snapshot ───────────────────────────────────────────────────────────────

export const SnapshotKind = {
	FULL: "full",
	REFRESH: "refresh",
	WORKING: "working",
	SEALED: "sealed",
} as const;

export type SnapshotKind = (typeof SnapshotKind)[keyof typeof SnapshotKind];

export const SnapshotStatus = {
	BUILDING: "building",
	READY: "ready",
	STALE: "stale",
	FAILED: "failed",
} as const;

export type SnapshotStatus =
	(typeof SnapshotStatus)[keyof typeof SnapshotStatus];

// ── File Parse Status ──────────────────────────────────────────────────────

export const ParseStatus = {
	PARSED: "parsed",
	SKIPPED: "skipped",
	FAILED: "failed",
	STALE: "stale",
} as const;

export type ParseStatus = (typeof ParseStatus)[keyof typeof ParseStatus];

// ── Declarations ───────────────────────────────────────────────────────────

export const DeclarationKind = {
	MODULE: "module",
	BOUNDARY: "boundary",
	ENTRYPOINT: "entrypoint",
	INVARIANT: "invariant",
	OWNER: "owner",
	MATURITY: "maturity",
	REQUIREMENT: "requirement",
	WAIVER: "waiver",
} as const;

export type DeclarationKind =
	(typeof DeclarationKind)[keyof typeof DeclarationKind];

// ── Module Maturity ────────────────────────────────────────────────────────

export const ModuleMaturity = {
	PROTOTYPE: "PROTOTYPE",
	MATURE: "MATURE",
	PRODUCTION: "PRODUCTION",
} as const;

export type ModuleMaturity =
	(typeof ModuleMaturity)[keyof typeof ModuleMaturity];

// ── Inference Kinds ────────────────────────────────────────────────────────

export const InferenceKind = {
	ARCH_ROLE: "arch_role",
	DEAD_LIKELIHOOD: "dead_likelihood",
	OWNERSHIP_GUESS: "ownership_guess",
	HOTSPOT_SCORE: "hotspot_score",
	CLONE_CLUSTER: "clone_cluster",
	STATE_MACHINE: "state_machine",
} as const;

export type InferenceKind = (typeof InferenceKind)[keyof typeof InferenceKind];

// ── Artifact Kinds ─────────────────────────────────────────────────────────

export const ArtifactKind = {
	AST_MANIFEST: "ast_manifest",
	COVERAGE_REPORT: "coverage_report",
	TRACE_DUMP: "trace_dump",
	GIT_METRICS: "git_metrics",
	CONTRACT_COMPARISON: "contract_comparison",
	CLONE_REPORT: "clone_report",
	HOTSPOT_DATA: "hotspot_data",
} as const;

export type ArtifactKind = (typeof ArtifactKind)[keyof typeof ArtifactKind];

// ── Visibility ─────────────────────────────────────────────────────────────

export const Visibility = {
	PUBLIC: "public",
	PRIVATE: "private",
	PROTECTED: "protected",
	INTERNAL: "internal",
	EXPORT: "export",
} as const;

export type Visibility = (typeof Visibility)[keyof typeof Visibility];

// ── Location ───────────────────────────────────────────────────────────────

export interface SourceLocation {
	lineStart: number;
	colStart: number;
	lineEnd: number;
	colEnd: number;
}
