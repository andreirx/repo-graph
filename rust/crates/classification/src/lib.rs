//! repo-graph-classification — Rust port of the core classification
//! substrate.
//!
//! This crate is the Rust-side mirror of the TypeScript
//! classification modules at `src/core/classification/` plus the
//! necessary diagnostic type definitions from
//! `src/core/diagnostics/`. It is built incrementally as the
//! Rust-3 classification parity slice progresses.
//!
//! Slice substep state (Rust-3):
//!   - R3-A workspace skeleton ........... done
//!   - R3-B types module ................. done
//!   - R3-C signals + blast-radius ....... done
//!   - R3-D unresolved classifier ........ done
//!   - R3-E boundary matcher ............. done
//!   - R3-F framework detection .......... done
//!   - R3-G parity harness ................ done
//!   - R3-H script integration ............ done
//!   - R3-I final acceptance gate ......... done
//!
//! ── Locked slice contract ─────────────────────────────────────
//!
//! ── Module/file mapping ──────────────────────────────────────
//!
//! In-scope TS files ported to Rust across R3-B through R3-F:
//!
//!   - src/core/classification/api-boundary.ts
//!     → `types` module (BoundaryMechanism, BoundaryProviderFact,
//!       BoundaryConsumerFact, BoundaryLinkCandidate)
//!   - src/core/classification/signals.ts
//!     → `signals` module (SnapshotSignals, FileSignals DTOs +
//!       pub(crate) pure predicates)
//!   - src/core/classification/blast-radius.ts
//!     → `blast_radius` module (deriveBlastRadius function +
//!       ReceiverOrigin / EnclosingScope / BlastRadius enums)
//!   - src/core/classification/boundary-matcher.ts
//!     → `boundary_matcher` module (HttpBoundaryMatchStrategy +
//!       matchBoundaryFacts orchestrator + computeMatcherKey)
//!   - src/core/classification/framework-boundary.ts
//!     → `framework_boundary` module (Express route /
//!       middleware detection)
//!   - src/core/classification/framework-entrypoints.ts
//!     → `framework_entrypoints` module (Lambda handler
//!       detection)
//!   - src/core/classification/unresolved-classifier.ts
//!     → `unresolved_classifier` module (the main 6-rule
//!       precedence classifier)
//!
//! Transitive type deps also ported (into the `types` module):
//!
//!   - src/core/diagnostics/unresolved-edge-categories.ts
//!     → `UnresolvedEdgeCategory` enum
//!   - src/core/diagnostics/unresolved-edge-classification.ts
//!     → `UnresolvedEdgeClassification`, `UnresolvedEdgeBasisCode`
//!
//! Mirrored from the extractor port (types only, NOT the full
//! port interface):
//!
//!   - src/core/ports/extractor.ts::ImportBinding
//!     → `ImportBinding` DTO in the `types` module
//!
//! ── Public API surface (locked at R3 lock phase) ──────────────
//!
//! The stable public surface is intentionally narrow. Only the
//! types module + the top-level classification entry points are
//! public. Signals helper predicates stay `pub(crate)` — they
//! are implementation vocabulary, not external contract.
//!
//! Public (stable API):
//!
//! ```text
//! pub mod types;  // all DTOs + typed enums
//!
//! pub fn classify_unresolved_edge(...) -> ClassifierVerdict;
//! pub fn derive_blast_radius(...) -> BlastRadiusAssessment;
//! pub fn compute_matcher_key(...) -> Option<String>;
//! pub fn match_boundary_facts(...) -> Vec<BoundaryLinkCandidate>;
//! pub fn detect_framework_boundary(...) -> Option<ClassifierVerdict>;
//! pub fn detect_lambda_entrypoints(...) -> Vec<DetectedEntrypoint>;
//!
//! // Empty-signal constructors exposed if downstream code
//! // needs them (e.g. SnapshotSignals::empty()).
//! ```
//!
//! Crate-internal (NOT stable API):
//!
//! ```text
//! pub(crate) mod signals;        // predicates + helpers
//! pub(crate) mod blast_radius;   // internal impl
//! pub(crate) mod unresolved_classifier;
//! pub(crate) mod boundary_matcher;
//! pub(crate) mod framework_boundary;
//! pub(crate) mod framework_entrypoints;
//! ```
//!
//! Rationale: future Rust-4+ consumers (trust reporting, indexer)
//! need the classifier outputs and boundary matching results,
//! not dozens of helper-predicate entry points. Narrower public
//! surface = smaller blast radius on API changes + clearer
//! contract.
//!
//! ── DTO boundary discipline (D-A2 + lock additional rule) ────
//!
//! Typed Rust enums with serde rename for all string-literal-
//! union types (UnresolvedEdgeCategory, UnresolvedEdgeClassification,
//! UnresolvedEdgeBasisCode, BoundaryMechanism, ReceiverOrigin,
//! EnclosingScope, BlastRadius). Type-safe Rust API +
//! exhaustive match support. Downstream Rust consumers get
//! compile-time exhaustiveness checking.
//!
//! **DTO collection rule:** NO `HashSet` / `HashMap` in public
//! parity-boundary DTO types. The TS side uses convenience shapes
//! like `Set<string>` for same-file symbol collections; these
//! work in TS because JS Sets preserve insertion order. In Rust,
//! `HashSet` / `HashMap` have no defined iteration order, so
//! using them in parity-boundary types would produce
//! nondeterministic JSON serialization.
//!
//! Instead: DTOs expose deterministic raw collections
//! (`Vec<String>`, sorted by name if the TS source expects sort-
//! order semantics). The classification functions convert to
//! local sets internally for lookup performance, then discard
//! the sets before returning. Example:
//!
//! ```ignore
//! pub struct FileSignals {
//!     pub same_file_value_symbols: Vec<String>,
//!     // NOT HashSet<String>
//! }
//!
//! fn classify(file_signals: &FileSignals) -> ... {
//!     // Build a local HashSet for O(1) lookup.
//!     let values: HashSet<&str> = file_signals
//!         .same_file_value_symbols
//!         .iter()
//!         .map(String::as_str)
//!         .collect();
//!     // ... use `values` for lookup ...
//! }
//! ```
//!
//! This rule is locked at the R3 lock phase. Every public DTO
//! field that holds a collection uses `Vec<T>`, not `HashSet<T>`
//! or `HashMap<K, V>`.
//!
//! ── Parity acceptance model (R3-G / R3-I) ────────────────────
//!
//!   - Format: dispatch-by-`fn` field. Each fixture is a single
//!     function call with committed input + expected output.
//!     `classification-parity-fixtures/<name>/input.json` plus
//!     `classification-parity-fixtures/<name>/expected.json`.
//!     One function call per fixture.
//!
//!   - Normalization: **none**. Classification is pure policy
//!     with no generated UIDs, no timestamps, no randomness.
//!     Outputs are byte-equal across runtimes by construction.
//!
//!   - Coverage rule: scenario-driven, NOT compression-driven.
//!     Every distinct TS-pinned behavior class must be
//!     represented in the shared corpus. The fixture count is
//!     NOT pre-locked to a target number; the rule is
//!     "at least one fixture per distinct classifier precedence
//!     path + every boundary-matcher behavior class + every
//!     framework-boundary/lambda positive-and-negative path +
//!     every signal predicate behavior currently externally
//!     test-pinned in TS."
//!
//!   - Minimum fixtures per dimension (floors, not ceilings):
//!       * classify_unresolved_edge: one per distinct
//!         precedence path (6 rules × at least one positive
//!         case each)
//!       * derive_blast_radius: at least one per receiver-
//!         origin + scope combination currently test-pinned
//!       * compute_matcher_key: at least one per distinct
//!         segment-normalization rule currently test-pinned
//!       * match_boundary_facts: at least one per match /
//!         nomatch class currently test-pinned
//!       * detect_framework_boundary: at least one per Express
//!         route pattern AND at least one negative case
//!       * detect_lambda_entrypoints: at least one per Lambda
//!         handler pattern AND at least one negative case
//!       * signals predicates: at least one per externally
//!         test-pinned behavior
//!
//!   - Both halves always compare against committed
//!     `expected.json` — neither half regenerates at test time.
//!
//! ── Tech stack (R3 lock + D-A1 override + R3-B timing fix) ───
//!
//! Dep discipline is the strictest of any Rust workspace crate
//! so far. Per the R3 D-A1 override, as corrected at R3-B:
//!
//!   - R3-A: serde only (for derive macros on typed enums +
//!     parity-boundary structs).
//!   - R3-B: `serde_json` added (narrow D-A1 timing correction).
//!     Required because `BoundaryProviderFact.metadata` and
//!     `BoundaryConsumerFact.metadata` are
//!     `Record<string, unknown>` in the TS DTO contract, and the
//!     only idiomatic Rust equivalent is
//!     `serde_json::Map<String, Value>`. The `preserve_order`
//!     feature is enabled for the same key-order-parity reason
//!     documented in Rust-2's `Cargo.toml`. Scope is limited to
//!     the metadata fields; no other DTO field uses `Value` or
//!     `Map`. This is a concrete structural requirement, not a
//!     speculative dep — the D-A1 rule "no speculative deps"
//!     still holds for everything else.
//!   - `thiserror` is NOT added unless a real public error
//!     surface appears during implementation. Classification is
//!     deterministic transform code; most failure modes are
//!     "the input didn't match any rule" which produces an
//!     UNKNOWN verdict, not an error.
//!   - No rusqlite, no uuid, no regex, no tree-sitter, no
//!     tempfile, no walkdir. Speculative deps are the wrong
//!     shape for a pure-policy slice.
//!
//! ── Explicit deferrals (NOT part of R3 contract) ─────────────
//!
//! The following are NOT claimed, NOT tested, and NOT promised
//! by Rust-3. They are recorded here so they are visible as
//! known future work, not as silent gaps:
//!
//!   - The extractor port itself (`src/core/ports/extractor.ts`
//!     beyond the `ImportBinding` type mirror). Extractor
//!     adapters, tree-sitter glue, and grammar version alignment
//!     are future slices.
//!
//!   - Indexer orchestration. Classification is consumed by the
//!     indexer; the indexer stays TS-only in Rust-3.
//!
//!   - Storage integration. Classification is stateless; match
//!     results are returned as DTOs, not persisted.
//!
//!   - Trust reporting. Rust-4 candidate. Classification is the
//!     substrate trust builds on.
//!
//!   - Cross-mechanism boundary matching (HTTP ↔ gRPC ↔ IPC
//!     etc.). TS explicitly calls this out as a PROTOTYPE-level
//!     limit. Rust-3 mirrors the TS PROTOTYPE maturity, not
//!     better.
//!
//!   - Query parameter / route prefix matching. Same reason.
//!
//!   - New framework detectors beyond Express route registration
//!     and Lambda handler detection.
//!
//!   - Async / tokio / runtime scaffolding. Classification is
//!     pure sync.
//!
//! ── Public API surface (filled in incrementally by substeps) ─
//!
//! R3-A: empty public API (workspace skeleton only).
//! R3-B: `types` module exposing all DTOs, typed enums,
//!       classifier verdict, blast-radius vocabulary, boundary
//!       fact types, signal types, the `ImportBinding` +
//!       `SourceLocation` mirror from the extractor port, and
//!       the classification-local `ClassifierEdgeInput` (narrow
//!       2-field input type replacing the full extractor-shaped
//!       `UnresolvedEdge` per the R3 locked boundary — see
//!       `types.rs` for the rationale). `SnapshotSignals::empty()`
//!       and `FileSignals::empty()` constructors exposed.
//! R3-C: `signals` module (pub(crate)) with pure predicates +
//!       `blast_radius` module with `derive_blast_radius`.
//! R3-D: `unresolved_classifier` module with the main classifier
//!       function.
//! R3-E: `boundary_matcher` module with `compute_matcher_key` +
//!       `match_boundary_facts`.
//! R3-F: `framework_boundary` + `framework_entrypoints` modules
//!       with the two framework-detection functions.

pub(crate) mod blast_radius;
pub(crate) mod boundary_matcher;
pub mod boundary_parser;
pub(crate) mod framework_boundary;
pub(crate) mod framework_entrypoints;
pub mod module_edges;
pub(crate) mod signals;
pub mod types;
pub(crate) mod unresolved_classifier;

// ── Public re-exports (locked R3 API surface) ─────────────────
//
// Each top-level classification function is defined in its
// pub(crate) module and re-exported here as the stable public
// entry point. This keeps the internal module organization
// private while giving downstream consumers a flat API.

pub use blast_radius::derive_blast_radius;
pub use boundary_matcher::{compute_matcher_key, match_boundary_facts};
pub use framework_boundary::detect_framework_boundary;
pub use framework_entrypoints::detect_lambda_entrypoints;
pub use unresolved_classifier::classify_unresolved_edge;
