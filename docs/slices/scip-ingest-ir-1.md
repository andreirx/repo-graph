# SCIP-INGEST-IR-1: Canonical Ingestion IR over SCIP

Slice ID: SCIP-INGEST-IR-1
Status: DESIGN READY — D1-D5 resolved 2026-05-30 (see Resolved design decisions).
Implementation proceeds via the migration plan Stage A/B (INGEST-CORE-1 first).
Specification only; no code in this slice.
Depends: `docs/architecture/adr/adr-extraction-substrate-scip-first.md` (gate retired)
Track: Extraction Substrate Pivot
Type: Design / architecture (defines the ingestion boundary; not yet code)

## Foundational principle (non-negotiable)

**SCIP is an upstream fact producer. It is NOT the domain model.** The repo-graph
IR is repo-graph-centered: canonical stable keys, canonical call/reference
semantics, canonical trust/provenance, canonical partition identity. SCIP symbol
IDs, roles, and framing are inputs consumed at the ingestion boundary and then
discarded as identity (kept only as provenance).

A failure mode to actively prevent: the IR becoming a disguised SCIP schema mirror.
If the IR's node identity is a SCIP symbol string, this slice has failed.

## Goal

Define the canonical in-memory IR that the LiveGraph is built from, and the
ingestion boundary that maps SCIP output (plus repo-graph's retained tree-sitter
AST passes) into it — for TypeScript, C/C++, and Rust.

## First-class requirements (locked by the ADR amendment)

### R1 — Canonical stable-key mapping over SCIP symbol IDs
- Every IR node's identity is a repo-graph **canonical stable key**, never a SCIP
  symbol string.
- SCIP global symbol (`<scheme> <manager> <package> <version> <descriptors>`):
  parse descriptors into (file/module path + qualified name + symbol kind);
  **exclude the volatile version field from identity** (record it as provenance) so
  dependency bumps do not churn identity.
- SCIP `local N` symbols are not durable: synthesize a stable key from
  (file path + enclosing qualified name + symbol name + normalized intra-file
  position/signature). Document the synthesis algorithm precisely.
- Maintain a per-partition map `scip_symbol_id -> canonical_stable_key`; SCIP IDs
  persist only as provenance.

### R2 — Call-graph derivation from occurrences + syntax context
- SCIP provides occurrences with roles (Definition vs reference) and **no CALL
  role**. Derive edges:
  - For each non-definition occurrence referencing symbol S, find the enclosing
    definition D (via document symbol ranges / enclosing range) -> candidate edge
    D -> S.
  - Classify edge type using **syntax context from the AST join (R3)**:
    - syntax-confirmed call expression -> `CALLS`
    - import-role occurrence -> `IMPORTS`
    - otherwise -> `REFERENCES` (resolved, but not call-confirmed) — an honestly
      labeled distinct edge type, never silently promoted to CALLS.
- Honest degradation: edges that cannot be classified retain `REFERENCES`, not a
  guessed `CALLS`.

### R3 — AST <-> SCIP correlation contract
- repo-graph's tree-sitter pass is retained (for the value layer AND for call-vs-
  reference syntax classification). Define the join:
  - **Match key:** (document relative path, source range). Specify range
    normalization and an explicit off-by-one tolerance policy.
  - **On match:** attach syntactic classification and value-layer facts (boundary,
    quality, etc.) to the same canonical stable key as the SCIP-derived node.
  - **On unmatched SCIP occurrence:** keep as `REFERENCES` (no syntax confirmation).
  - **On unmatched AST fact:** record with degraded confidence + provenance note.
- C/C++ caveat to resolve: scip-clang ranges are over preprocessed/expanded source;
  tree-sitter is over raw source. Define how macro expansion mismatch is handled
  (Open Decision D3).

### R4 — Rust per-crate ingestion mode + duplicate-symbol dedup
- Rust ingestion is **per-crate** (one `rust-analyzer scip` run per crate).
  Whole-workspace export is not trusted (panics: duplicate-document emission).
- Cross-crate edges resolve by merging per-crate indexes on canonical stable key
  (global symbols are stable across crates).
- **Dedup contract:** rust-analyzer emits duplicate SCIP symbols (test files
  duplicate `crate/` symbols; some defs duplicated). Collapse duplicates by
  canonical stable key; tie-break toward the library (non-test) definition; record
  the dedup decision in provenance. Also handle "definition not in document" emission
  bugs (drop with a logged degradation, do not fabricate).

### R5 — C/C++ build-root / context provenance
- C/C++ ingestion **requires** a `compile_commands.json` (BC-1) and a correct
  project root. Mis-rooting silently externalizes all files (0 documents) — add a
  **coverage sanity check** (documents > 0; in-project ratio above a floor) that
  fails ingestion loudly rather than producing an empty graph.
- Capture build context as provenance and as config identity: compile-db path+hash,
  project root, per-TU compiler/flags/defines. One compile-db = one configuration;
  multiple configurations = multiple partitions (single-config-correct).

### R6 — Provenance from external indexers (cross-cutting)
- Every ingested fact records: source indexer + version, original SCIP symbol id
  (non-durable), build-inputs hash, partition id, ingestion epoch. This feeds the
  trust redefinition (coverage/completeness, not unresolved-rate) and cross-run
  comparability.

## Proposed IR shape (to be refined in the slice)

Repo-graph-centered, reusing existing canonical concepts:
- `IrNode { stable_key, kind, name, file, range, partition_id, provenance }`
- `IrEdge { src_stable_key, dst_stable_key, edge_type: CALLS|IMPORTS|REFERENCES|...,
   syntactic_basis, provenance }`
- `Partition { partition_id, kind: ts_package|cargo_crate|cpp_compdb, root,
   build_context, indexer, indexer_version }`
- `Provenance { indexer, indexer_version, scip_symbol_id?, build_inputs_hash,
   ingestion_epoch }`

Per-language adapters (scip-typescript, scip-clang, rust-analyzer) implement a
common `SciipIngestSource` contract producing IR; the core IR and mapping are
language-agnostic.

## Resolved design decisions (ratified 2026-05-30)

Resolved per the strictness-of-claims / coverage-at-honestly-labeled-outer-layers
posture (see ADR and `docs/architecture/scip-migration-plan.md`). Listed in
dependency order (D3 -> D4 -> D1 -> D2 -> D5).

- **D3 = mixed-mode (C/C++).** The SCIP semantic graph (references/calls) is
  authoritative for C/C++. Value-layer facts attach to canonical symbol identity only
  where a strong join exists; otherwise they remain raw-source-anchored and are explicitly
  labeled "not symbol-correlated." No heuristic force-join.
  **A "strong join" REQUIRES range containment AND name correspondence (CJOIN-PROVE-2,
  2026-05-31). Range-only joining is forbidden: it silently misattaches value facts to the
  wrong callable (15.1% on C++ annotation-macro code, where macros collapse the AST function
  span). A name mismatch forces raw-source anchoring, never attachment. Terminal-name
  correspondence is NECESSARY, not sufficient: same-name overload / signature /
  template-instantiation ambiguity is NOT retired by it — stronger hardening
  (signature / arity / scope correspondence) is deferred.**
- **D4 = separate AST value-layer pass** with an explicit join contract (not a fused
  ingestion pass). Revisit only if C/C++ correlation proves impossible without
  integration.
- **D1 = in-memory IR only.** The warm cache is a later downstream projection
  (PARTITIONED-WARM-CACHE-ARCH-1). The domain model is not constrained by
  serialization / zero-copy needs at this stage.
- **D2 = graded edge model, strict default queries.** Edges carry
  basis/confidence/provenance. Default `callers`/`callees`/`path`/`cycles` operate on
  syntax-confirmed `CALLS` only (preserving the Layer-1 deterministic claim); graded
  `REFERENCES` are carried and surfaced only on explicit request with degradation
  labels. **Cost acknowledged:** confidence-awareness will spread into trust and query
  surfaces — a deliberate cost center, not "just metadata," owned by TRUST-MODEL-REBASE-1.
- **D5 = deterministic Rust canonicalization.** For multi-target duplication, choose
  the canonical owner deterministically (lib > bin > test; non-test > test for the same
  semantic item) and preserve alternates as aliases/provenance — never silently
  dropped. Part of the R1 stable-key policy, not a tie-break.

## Definition of Done (design slice)

1. IR schema specified (nodes, edges, partitions, provenance).
2. R1-R6 contracts specified concretely (algorithms, not prose gestures).
3. Per-language adapter contract defined; core kept language-agnostic.
4. Open decisions D1-D5 resolved or explicitly scheduled.
5. Sign-off, then implementation sub-slices (support module -> adapters -> wiring ->
   tests) per the Operational Sequence in `docs/VISION.md`.

## Validation tracks (reclassified from spike rigor — done against the IR, not before it)

- Precise CALLS-level parity vs `rmap` callers/callees (TS, C, Rust).
- C resolution-parity vs `rmap` on leveldb; a multi-config / `#ifdef`-heavy C repo.
- All-crates Rust ingestion (only `storage` validated in the spike).
- M3 AST<->SCIP correlation off-by-one rate; M4b edit-churn.

## Non-goals
- Warm-cache format, refresh model, partition-granularity-at-scale (deferred).
- Query migration (QUERY-MIGRATION-1) and coherence (COHERENCE-LAYER-1) — later.
- Removing tree-sitter (it is retained for the value layer and call classification).

## References
- `docs/architecture/adr/adr-extraction-substrate-scip-first.md`
- `docs/audits/scip-{ts-parity,clang,rust}-spike-1/findings.md`
- `agent_docs/storage-architecture-v2.md` (tier model, stable-key/A1 continuity)
- `docs/slices/bc-1-compile-commands-import.md` (C/C++ build context)
