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
