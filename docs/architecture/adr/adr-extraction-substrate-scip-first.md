# ADR: SCIP-First Extraction Substrate

Slice ID: EXTRACTION-SUBSTRATE-ADR-1
Status: ACCEPTED. TypeScript GO, C/C++ GO, Rust GO-with-caveats — multi-language
substrate viability gate RETIRED (2026-05-29). Operational decisions (refresh
model, cache format, partition granularity at scale) remain DEFERRED. Next:
SCIP-INGEST-IR-1.
Date: 2026-05-29
Deciders: Product owner (Andrei)

## Run 1 Spike Amendment (2026-05-29)

SCIP-TS-PARITY-SPIKE-1 Run 1 (FRAKTAG `engine` + `api`) returned **GO** for buildable
TypeScript. See `docs/audits/scip-ts-parity-spike-1/findings.md`. This amendment
promotes Run-1-settled items from open to decided; it reopens nothing.

Promoted to DECIDED (evidence-grounded):
- **Stable-key mapping is mandatory.** 41% of engine symbols are non-durable
  `local` IDs; globals embed package version; identical re-index is
  byte-deterministic. SCIP symbols are substrate provenance; canonical stable keys
  remain product identity, with synthesized keys for locals.
- **SCIP global symbols carry cross-partition linkage.** `api -> engine` resolved
  95 references compiler-grade, un-built, via the workspace symlink. Confirms the
  "per-partition detail + always-resident global cross-reference index" design.
- **Locals are not durable identity** (corollary of the above).
- **Trust redefinition direction confirmed.** SCIP resolves stdlib + dependency
  symbols; unresolved-rate is meaningless under it; trust moves to coverage.

Still DEFERRED (unchanged): refresh model, warm-cache format, partition
granularity at scale.

**Multi-language substrate NOT settled.** SCIP-first is proven only for TypeScript.
`scip-clang` (C/C++) and `rust-analyzer --> SCIP` (Rust) are untested and are the
concentrated remaining risk, gating the C/Linux and self-host ambitions.

## Multi-Language Gate Amendment (2026-05-29)

SCIP-CLANG-SPIKE-1 and SCIP-RUST-SPIKE-1 are complete. The substrate viability gate
is **RETIRED**. Evidence: `docs/audits/scip-clang-spike-1/`, `scip-rust-spike-1/`,
`scip-ts-parity-spike-1/`.

Promoted to DECIDED:
- **SCIP-first is viable for TypeScript (GO), C/C++ (GO), and Rust (GO-with-caveats).**
- Stable-key mapping is mandatory across all three languages (TS 41% locals, Rust
  72% locals; C/C++ 0 local symbol-records but local-symbol occurrences exist).
- Global SCIP symbols carry cross-partition linkage (TS `api->engine` 95 refs; Rust
  cross-crate; C in-project `.` 10,276 refs).
- Locals are substrate-local provenance, never durable product identity.
- **Rust whole-workspace `rust-analyzer scip` export is NOT trusted** (panics:
  duplicate-document emission). **Per-crate is the supported Rust ingestion mode**,
  with mandatory duplicate-symbol dedup.
- **C/C++ requires explicit project rooting and build-context** (`compile_commands.json`,
  BC-1). Mis-rooting silently externalizes all files (0 documents) — a hard
  ingestion precondition with a coverage sanity check.

Remaining spike measures (precise CALLS parity, multi-config C, all-crates Rust,
M3 correlation, M4b edit-churn) are reclassified as **validation tracks inside
SCIP-INGEST-IR-1**, not blockers to starting it.

## Relationship to Prior Decisions

This ADR builds on the existing storage tier model and revises specific
assumptions in prior slices. It does not discard them.

- **Revises STORAGE-ARCH-1** (`agent_docs/storage-architecture-v2.md`):
  the Decision Log entry "Tier B backing store: SQLite initially" is revised.
  Raw L0/L1 graph facts leave SQLite for a partitioned binary warm cache
  (format DEFERRED). The A1/A2 tier model and Invariants 1 and 3 (authority
  survives cache loss; no cross-tier authority leakage) are retained and become
  load-bearing for the stable-key mapping below.
- **Supersedes SE-1's framing** (`docs/slices/se-1-clangd-enrichment.md`):
  SE-1 and the existing `rgr enrich` pass (tsserver for TS, rust-analyzer ~77%
  for Rust, jdtls for Java) treat compiler-grade tooling as an *optional
  enrichment over tree-sitter primary*. This ADR inverts that: compiler-grade
  tooling becomes the *primary* L0/L1 producer. SE-1's LSP-client design is
  retained as prior art for the deferred live-consumption path (see M6 in the
  spike).
- **Revises LIVE-GRAPH-1/2** (`docs/slices/live-graph-1.md`, `live-graph-2.md`):
  the in-memory `LiveGraph` struct and the callers/callees/path migration intent
  stand. The loader source changes (SCIP-derived facts, not SQLite-extracted
  `nodes`/`edges`) and residency granularity changes (per-partition, not
  per-repo).
- **Reuses BC-1** (`docs/slices/bc-1-compile-commands-import.md`):
  `compile_commands.json` import is the build-context and translation-unit
  partition foundation that `scip-clang` requires for C/C++.
- **Reuses TC-1** (toolchain inventory): detected toolchains gate which
  indexers can run.

## Context

The current L0/L1 substrate has three structural problems, all evidenced:

1. **Precision ceiling.** Homegrown tree-sitter + heuristic resolution achieves
   ~20-33% call resolution on TypeScript and produces more unresolved than
   resolved edges on Linux (2.05M resolved vs 2.78M unresolved). The ROADMAP
   attributes the latter directly to "no build-system context."
2. **Storage bloat.** Raw graph facts in SQLite produce gigabyte caches
   (`hadoop.db` 9.58 GB, `repo-graph.db` 755 MB) of rebuildable data, and prune
   of that data does not complete within operational timeouts.
3. **Compilation-context blindness.** Syntax-only extraction cannot represent
   that a file means different things under different builds (`#ifdef`/Kconfig).

The VISION already commits to the destination: a long-lived daemon with the
current graph in memory as the conceptual center; SQLite as a transitional
mechanism; "do not race vendors on generic indexing — own the layer above."

**Mission narrowing (accepted by product owner):** the product targets
**living, working code with a usable build/toolchain context** — not archaeology
of code that barely parses or does not build. This narrowing is what makes
compiler-grade extraction viable as the primary path.

## Decision — Committed Now

1. **SCIP-first.** SCIP (Sourcegraph, Apache-2.0) plus per-language
   compiler-grade indexers (`scip-typescript`, `scip-python`, `scip-java`,
   `scip-clang`, `rust-analyzer --> SCIP`) become the **primary** producers of
   L0/L1 symbol, definition, reference, and import facts for buildable code.
2. **The AST value layer survives and remains repo-graph-owned.** SCIP produces
   none of these; they require an AST:
   - boundaries / seams / IPC / transports
   - state / resource reads and writes
   - framework / runtime / build detection
   - module discovery (manifests/workspaces)
   - contract / schema extraction
   - quality metrics (cyclomatic, cognitive, nesting, params, length)
   tree-sitter does **not** disappear. It is demoted from symbol-resolution duty
   to value-layer duty.
3. **Deletion target.** The homegrown cross-file resolver, the `unresolved_edges`
   machinery, and the classifier (v6) built around them become legacy and are
   deleted **after** parity is proven (see SCIP-TS-PARITY-SPIKE-1), not before.
4. **Canonical identity stays with repo-graph stable keys.** SCIP symbols —
   especially `local <n>` symbols, which are document-scoped and not stable
   across re-index — are treated as substrate-local provenance, **not** durable
   product identity. Ingestion maps SCIP symbols onto the existing canonical
   stable-key scheme; local symbols receive synthesized stable keys
   (path + qualified name + signature). This is non-negotiable because it
   preserves:
   - A1 governance continuity (declarations/waivers/baselines target stable keys),
   - warm-cache invalidation granularity,
   - cross-run comparability.
5. **The call graph is derived, not given.** SCIP provides occurrences with
   roles (definition/reference/read/write/import), not call edges.
   callers/callees/path/cycles are derived from reference occurrences plus
   enclosing ranges. Distinguishing a CALL from a generic reference may require
   AST context (measured in the spike).
6. **Trust is redefined.** Unresolved-edge rate stops being meaningful under
   compiler-grade resolution (it approaches 100%). Trust moves toward
   indexer/build completeness, partition coverage, configuration coverage, and
   toolchain/version provenance. Exact grading shape is DEFERRED.
7. **SCIP <-> AST correlation.** Value-layer AST facts are keyed to the same
   canonical stable keys and correlated to SCIP-derived symbols by source range.
   Ingestion is semantic reconciliation, not format mapping.

## Decision — Explicitly Deferred (Evidence-Gated by SCIP-TS-PARITY-SPIKE-1)

Deciding these now would be theory-first. They are gated on spike measurement:

- **Refresh model:** partition re-run vs two-speed (fast tree-sitter delta +
  slower SCIP truth, with explicit degradation) vs explicit-refresh-only.
- **Consumption mode (sub-fork of refresh):** batch SCIP file ingestion vs live
  LSP query. The existing `rgr enrich` LSP integration and SE-1's client design
  are prior art for the live path; batch SCIP is build-oriented and weak at
  single-file incrementality.
- **Partition granularity:** coupled to the refresh model and to natural build
  units (tsconfig project, Cargo crate, Maven module, translation unit).
- **Warm-cache format:** zero-copy archive (`rkyv` / Cap'n Proto / FlatBuffers)
  vs deserialize (`bincode`) vs embedded KV (`redb` / LMDB) vs a composition.
- **Exact trust grading shape:** depends on the refresh model chosen.

## Consequences

### Positive
- Resolution precision rises to compiler-grade for buildable code.
- Compilation-context is handled as "this file in *this* build," correctly and
  deterministically, via BC-1 build context. (Multi-config = multiple index
  runs / partitions, by design.)
- Large reduction in maintained extraction code (resolver + classifier deleted).
- Multi-language coverage tracks the maintained SCIP indexer ecosystem.
- Aligns with VISION strategic position (own the interpretation layer, not the
  commodity indexer).

### Negative
- Indexing gains a build-environment dependency (toolchain present, build runnable,
  `compile_commands.json` for C/C++). This is the cost of the mission narrowing.
- Batch SCIP indexers collide with the fast incremental-refresh ambition; the
  refresh model (deferred) must resolve this.
- Determinism/comparability now depends on external indexer and build-input
  versions; the snapshot provenance surface (`snapshots.toolchain_json`) must
  expand to capture them.
- Large rewrite of the bottom of the stack; substantial transitional debt.

### Neutral
- tree-sitter remains for the value layer.
- SQLite remains correct for A1/A2 and persisted derived summaries.
- The partitioned binary warm cache is new infrastructure.

## Alternatives Considered

- **Keep owning extraction, fix only storage.** Rejected: leaves the resolution
  ceiling and compilation-context blindness unsolved; product owner rejected the
  cheap-subset path.
- **SCIP/compiler tooling as enrichment only (SE-1 status quo).** Rejected:
  keeps tree-sitter heuristic resolution as primary truth with its precision
  ceiling; compiler-grade stays second-class.
- **stack-graphs / pure no-build resolver as primary.** Rejected as primary
  given the mission narrowing to buildable code; retained as a candidate for a
  future no-build degraded mode.
- **Raw graph in SQLite, or process heap-dump cache.** Rejected earlier in the
  design thread (relational cleanup cost; brittle, unsafe reload, respectively).

## Open Questions (for the spike and the follow-on cache ADR)

1. How stable are SCIP symbol IDs across re-index, and how badly do `local`
   symbols churn? (Decides whether the synthesized-stable-key layer is
   mandatory — expected yes.)
2. How often can a CALL be distinguished from a generic reference using SCIP
   data alone vs requiring AST context?
3. How reliable is AST<->SCIP range correlation for value-layer facts?
4. What is the per-partition SCIP re-index latency, and does any indexer support
   cheap single-file incrementality?
5. How mature is `scip-clang` on real C repos (the entire C/Linux ambition rests
   on it), and how complete is `rust-analyzer --> SCIP` for self-hosting?

## Implementation

1. **SCIP-TS-PARITY-SPIKE-1** — PARTIAL; GO for TypeScript.
   See `docs/audits/scip-ts-parity-spike-1/findings.md`.
2. **SCIP-CLANG-SPIKE-1** — PARTIAL; GO. scip-clang real (leveldb: 39 TUs -> 90 docs,
   16,643 occ, 0 errors, 0.3s). See `docs/audits/scip-clang-spike-1/findings.md`.
3. **SCIP-RUST-SPIKE-1** — PARTIAL; GO WITH CAVEATS (per-crate works; whole-workspace
   panics; dedup required). See `docs/audits/scip-rust-spike-1/findings.md`.
4. **SCIP-INGEST-IR-1** (NEXT — gate retired) — canonical repo-graph IR over SCIP;
   stable-key mapping; call-graph derivation; AST correlation; Rust per-crate+dedup;
   C rooting/build-context provenance. See `docs/slices/scip-ingest-ir-1.md`.
5. **PARTITIONED-WARM-CACHE-ARCH-1** — binary warm cache; format decision.
6. **QUERY-MIGRATION-1** — callers/callees/path/cycles on LiveGraph partitions.
7. **COHERENCE-LAYER-1** — orient/check/trust mixed live+persisted contract.

### Governance reconciliation required (NOT performed in this ADR)
These truth surfaces must be updated under separate authorization:
- `docs/ROADMAP.md` Storage Architecture Track (resequence; SCIP track).
- `CURRENT_SLICE.md` (current priority changes from BACKLOG-REMEDIATION-1).
- `STORAGE-ARCH-1` status note (Tier B backing-store decision revised).
- `SE-1` status note (framing superseded; LSP client retained as prior art).
- `LIVE-GRAPH-1/2` revision notes (loader source, residency granularity).
- `docs/TECH-DEBT.md` (transitional debt: dual extraction during migration).

## References
- `docs/VISION.md` — operational architecture, value frontier, strategic position
- `agent_docs/storage-architecture-v2.md` — STORAGE-ARCH-1 tier model
- `docs/slices/se-1-clangd-enrichment.md`, `bc-1-compile-commands-import.md`
- `docs/slices/live-graph-1.md`, `live-graph-2.md`
- SCIP: Sourcegraph Code Intelligence Protocol (Apache-2.0)
