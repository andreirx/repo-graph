//! repo-graph-state-extractor — state-boundary adapter layer.
//!
//! This crate bridges the policy surface of
//! `repo-graph-state-bindings` (binding table, matcher, stable-
//! key builders) with the extraction-output surface of
//! `repo-graph-indexer` (`ExtractedNode`, `ExtractedEdge`).
//! It is an adapter, not a policy crate; its place in the G1a
//! split is spelled out in the milestone
//! (`docs/milestones/rmap-state-boundaries-v1.md`).
//!
//! Slice-1 scope (SB-2): emission API, in-memory resource-node
//! dedup, typed evidence struct, synthetic-fixture unit tests.
//! No language integration (SB-3), no corpus validation (SB-4),
//! no config/env seam graph emission (deferred slice), no queue/
//! event boundaries (deferred slice).
//!
//! Module map:
//!
//! - `emit` — `StateBoundaryEmitter` struct + input / output
//!   DTOs (`StateBoundaryCallsite`, `EmitterContext`,
//!   `EmittedFacts`, `EmitError`).
//! - `evidence` — `StateBoundaryEvidence` struct +
//!   `LogicalNameSource` enum + versioning constant.
//!
//! Design locks (recorded in
//! `docs/milestones/rmap-state-boundaries-v1.md` SB-2 section):
//!
//! - SB-2.1: stateful emitter struct.
//! - SB-2.2: reuse `repo_graph_indexer::types::{ExtractedNode,
//!   ExtractedEdge}` for outputs only. Inputs stay crate-owned.
//! - SB-2.3: in-memory `HashMap<stable_key, ExtractedNode>`
//!   dedup.
//! - SB-2.4: serde struct `StateBoundaryEvidence` + `serde_json`.
//! - SB-2.5: `LogicalNameSource` lives here, not in
//!   state-bindings.
//! - SB-2.6: schema-tolerance interop test already delivered by
//!   SB-2-pre + SB-2-pre-2 (no additional test added here).
//! - SB-2.7: no `languages/` subdir in slice 1; integration is
//!   SB-3.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod emit;
pub mod evidence;

// Re-export the most commonly used surface at the crate root.
pub use emit::{
	CallsiteLogicalName, EmittedFacts, EmitError, EmitterContext, StateBoundaryCallsite,
	StateBoundaryEmitter,
};
pub use evidence::{
	LogicalNameSource, StateBoundaryEvidence, STATE_BOUNDARY_EVIDENCE_VERSION,
};
