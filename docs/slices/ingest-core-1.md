# INGEST-CORE-1: Thin SCIP → Canonical-IR Foundation (TypeScript only)

Slice ID: INGEST-CORE-1
Status: IMPLEMENTED — 4c acceptance harness green (2026-05-31); ready to close on review + commit
Depends: SCIP-INGEST-IR-1 (DESIGN READY, D1–D5 resolved)
Track: Extraction Substrate Pivot — Stage A (`docs/architecture/scip-migration-plan.md`)
Scope discipline: **brutally small.** TS only, single partition, identity + call
derivation + minimal AST-join proof. Nothing else.

## The one uncertainty this slice retires

That repo-graph can ingest SCIP facts into a **repo-graph-owned canonical IR**
without SCIP becoming the domain model. Nothing more. If the slice grows past this
proof, it has failed its purpose and Stage B loses its cheap-foundation premise.

## Target

Single TypeScript partition: **`@fraktag/engine`** (SCIP already captured in the
spike: `engine.scip`, plus `engine2.scip` for re-ingest determinism). One partition,
one language, one ingestion path.

## In scope

- Canonical IR domain model — this slice's subset only: `StableKey`, `IrNode`,
  `IrEdge`, `Partition`, `Provenance`.
- **One concrete scip-typescript ingestion path** (not a generalized adapter).
- **Stable-key synthesis** (global + local) — the identity core.
- **Call derivation** from occurrences + enclosing definition + syntax context.
- **Minimal AST↔SCIP join scaffold** — only what the TS proof needs: call-expression
  classification + attachment of exactly one value-fact kind.
- **Provenance capture** per node / edge / partition.
- **Exit/test harness** proving the seven criteria below.

## Out of scope — hard guardrails (anything here in the diff = scope creep)

warm cache; runtime residency manager; multi-language generalization or a generic
adapter trait beyond the single concrete path; Rust dedup beyond an interface
placeholder; C/C++ macro-mismatch handling beyond an interface placeholder; query
migration beyond what proves ingestion correctness; trust implementation; persistence
design; background refresh scheduling; partition manager; serialization hooks; plugin
architecture.

Rule: if it is not needed to prove (1) canonical identity, (2) call derivation,
(3) one-partition ingestion correctness — it is not in this slice.

## Canonical IR (this slice's subset)

Repo-graph-owned. No SCIP/SQLite/tree-sitter type leaks into these.

- `StableKey` — canonical identity. **Never a SCIP symbol string.**
- `IrNode { stable_key, kind, name, file, range, partition_id, identity_source, provenance }`,
  `identity_source ∈ { AstAdopted, ScipSynthesizedFallback, AstFileScope }`.
- **Materialized FILE nodes.** One `AstFileScope` node per file (the `ts-extractor` FILE
  node; subtype `FileScope`, no SCIP symbol, `scip_symbol_id = None`) is materialized so
  every edge endpoint resolves to a node (see the no-dangling invariant, exit criterion 9).
  A FILE node is source-file scope, NOT a module-architecture / boundary / runtime entity.
- `IrEdge { src: StableKey, dst: StableKey, edge_type: Calls | References,
   basis: SyntaxConfirmedCall | DerivedReference | FileScopeReference, provenance }`
  — `basis` is carried **data** (D2 graded model). NO query/trust logic in this slice.
  `FileScopeReference` marks an edge whose caller is a FILE node (a top-level reference,
  e.g. an import): always `References`, never `Calls`, excluded from strict call-graph
  traversal by default — module-scope provenance, not a callable edge. `Imports` is
  intentionally OUT of this slice: the TS spike (M2) showed `scip-typescript` does not
  reliably emit import/read/write roles, so import edges must be derived from AST syntax —
  deferred to a later slice, never faked from SCIP roles.
- `Partition { id, kind: TsPackage, root, indexer, indexer_version }`
- `Provenance { indexer, indexer_version, scip_symbol_id: Option<String> (substrate,
   non-durable), build_inputs_hash }`

## Canonical identity (the heart) — AST-primary, SCIP-synthesis fallback

Ratified 2026-05-30. The identity rule for this slice:

- **Reuse the existing canonical symbol-key VALUE/format** — the exact string
  `ts-extractor` emits (`repo:file#name:SYMBOL:subtype[:dupN]`, the value A1 governance and
  measurements already target). Held in a thin `repo_graph_ir::CanonicalKey(String)`
  newtype. `state_bindings::StableKey` is **untouched** (it is opaque/resource-only with no
  symbol constructor); literal type unification with it is deferred to a future
  key-infrastructure slice. Value-level reuse: same value = A1 continuity.
- **PRIMARY: AST stable key, adopted by range-join.** `ts-extractor` already emits the
  canonical `repo:file#name:SYMBOL:subtype[:dupN]` key per TS definition. For each SCIP
  definition, join to the AST definition by `(file, range)` and **adopt the AST node's
  stable key as canonical identity.** SCIP contributes compiler-grade *resolution and
  cross-file/cross-symbol linkage*, not identity. This preserves A1 continuity and
  existing measurement keys by construction. The `(file, range)` join compares the AST
  name to a **narrowly reconciled** SCIP name — `<constructor>` → `constructor`,
  `<get>X`/`<set>X` → `X` — the only compiler-marker divergence proven in scip-typescript
  output; every other name compares verbatim (no fuzzy matching), with span containment
  disambiguating. This recovers constructor/getter identities that would otherwise split
  into duplicate AST-key + fallback-key identities (identity drift).
- **FALLBACK ONLY: SCIP-descriptor synthesis.** For an in-partition SCIP definition with
  NO AST match, synthesize a key from SCIP global-symbol descriptors. Fallback events are
  **counted and surfaced** — never silent. Fallback is probe material (and the primary
  path for Stage B non-AST languages), not the primary identity path here.
- **Fallback must not mask a weak join.** If fallback synthesis is rescuing a systemic
  definition-join failure, the slice does **not** pass (see exit criteria). Identity proof
  lives at the definition layer and cannot be concealed.

Two join layers, different expectations:
- **Definition join (identity):** must be **effectively complete** for in-partition TS
  definitions. This is the identity proof.
- **Occurrence join/classification (calls/refs):** lower match tolerable *if* it degrades
  **honestly** to `References` (or raw-anchored fact attachment) — never a guessed
  `Calls`, never silent loss.

External-dependency / version scope unchanged: in-partition symbols only; external-dep
symbols are provenance-bearing substrate (SCIP id + package + version kept as provenance,
no canonical version-collapsed identity); multi-version collision handling deferred beyond
this slice.

## Call derivation

For each non-definition occurrence `O` referencing symbol `S` in document `D`:
- **Caller = innermost enclosing *materialized* IR node** (graph closure / bubble-up).
  Non-materialized enclosing AST nodes (a constructor/getter with no SCIP def, a `local`,
  a destructuring binding) are bubbled past; the FILE node always encloses and is always
  materialized, so the caller — and thus the edge source — always resolves to a node.
- `S`'s key = callee (the matched/fallback node for `S`; an out-of-partition `S` yields
  no in-partition edge).
- Classify via the AST join: **FILE-node caller → `References` (`FileScopeReference`),
  never `Calls`** (a top-level call is module-init execution, not a callable edge);
  otherwise a name-matched call-expression at `O` → `Calls` (`SyntaxConfirmedCall`);
  otherwise → `References` (`DerivedReference`). Never promote an unconfirmed reference
  to `Calls`. There is **no `import-role -> Imports` rule** — SCIP-TS does not reliably
  emit import roles (spike M2); import classification is deferred to AST-derived
  handling in a later slice.

## Minimal AST↔SCIP join

- Reuse the existing `tree-sitter-typescript` capability in the Rust workspace to
  produce, per file: call-expression ranges (for classification) and exactly one value
  fact (proposed: cyclomatic complexity per function).
- Join SCIP occurrence ranges ↔ AST ranges by `(file, range)` with an explicit
  off-by-one tolerance policy. Unmatched SCIP occurrence → `References`. The one value
  fact attaches to the function's `StableKey` on match; otherwise it is recorded
  raw-source-anchored (proves the D3/D4 attachment path on one kind only).

## Exit criteria (exact — all must pass)

1. **Definition-join completeness (identity proof):** AST↔SCIP definition join is
   **effectively complete** for in-partition `@fraktag/engine` symbols; the match rate is
   a **measured, reported** number, not an invisible implementation detail.
2. **Fallback counted and bounded:** SCIP-descriptor synthesis events are counted and
   surfaced; the slice **cannot be declared passing if fallback is masking a systemic
   definition-join failure** (a high definition-layer fallback rate = fail, not pass).
3. Canonical identity is the existing ts-extractor symbol stable-key **value** (held in
   `repo_graph_ir::CanonicalKey`), adopted from the AST for matched definitions and
   asserted **byte-for-byte equal** to the ts-extractor key; identical re-ingest yields
   identical identity (deterministic by construction); SCIP `local N` churn does not
   affect it.
4. Derived `Calls` edges are reproducible and each is **syntax-confirmed** by a
   call-expression range from the AST join; unconfirmed → `References` (honest), never a
   guessed `Calls`.
5. Cross-file references/calls **inside the partition** are represented in the canonical
   IR, carrying SCIP-grade resolution.
6. The AST-derived fact-attachment path is demonstrated on **cyclomatic complexity**
   (the one value fact), attached to the canonical key.
7. Provenance is preserved per node/edge/partition (incl. the original SCIP symbol id and,
   for fallback nodes, a fallback flag).
8. **No SQLite raw-graph dependency** in the core path; `repo-graph-ir` has **zero**
   scip/sqlite/tree-sitter dependencies (enforced by the crate dependency graph).
9. **Referential integrity (no dangling edge endpoints):** every emitted edge `src` and
   `dst` resolves to a node in the same `PartitionIr`. Guaranteed structurally by
   materialized FILE nodes + materialized-caller (bubble-up) resolution. A measured,
   asserted number — **must be 0** for both source and destination.

## Proposed module layout (minimal; names adjustable to house convention)

- New crate `repo-graph-ir` — pure domain, **zero dependencies**. Defines
  `CanonicalKey(String)` (holding the existing ts-extractor symbol-key value),
  `IrNode`/`IrEdge`/`Partition`/`Provenance`, `IdentitySource` (AstAdopted vs
  ScipSynthesizedFallback), and the `Calls | References` edge-type + `EdgeBasis` enums. No
  serde (no serialization hooks this slice). `state_bindings` and `ts-extractor` untouched.
- New crate `repo-graph-scip-ingest` — the one concrete TS path: SCIP decode → stable-key
  synthesis → call derivation → AST join. Depends on `repo-graph-ir`.
- AST join reuses the existing `tree-sitter-typescript` capability.
- Tests live in `repo-graph-scip-ingest`.

**Reuse, do not reinvent:** use the official `scip` Rust crate (crates.io) for SCIP
protobuf decode rather than hand-rolling a decoder.

## Anti-platform guardrails (the caution, made explicit)

No generalized multi-language adapter abstraction; no partition manager; no
serialization hooks; no rich trust states; no generic plugin architecture. The second
language (Stage B `scip-clang` / `rust-analyzer`) is when any adapter trait is
extracted — not before (avoid premature abstraction with one implementation).

## Small decisions to confirm at sign-off (defaults proposed, non-blocking)

- Crate names: `repo-graph-ir` / `repo-graph-scip-ingest`.
- The one value-fact kind for criterion 5: cyclomatic complexity (confirmed).
- SCIP decode: official `scip` Rust crate (proposed) vs `prost` from `scip.proto`.

## Future boundary note (do not act now)

INGEST-CORE-1 reuses the canonical key VALUE, not the `state_bindings::StableKey` type.
Once the substrate proves out, a separate key-infrastructure slice can unify one canonical
key type across state/resource keys, symbol keys, and module keys (and refactor
`ts-extractor`'s inline `make_stable_key` to a shared builder). That is deliberately out of
this slice.

## 4c closure evidence (2026-05-31)

Implementation-complete. Exit criteria 1–9 are machine-asserted. Durable evidence is
committed (fixture + harness + ignored engine test); detailed metrics live in
`docs/audits/ingest-core-1/findings.md` (local — `docs/audits/` is gitignored by
convention, so the summary below travels with the repo instead).

- **Acceptance gate (default CI, portable, off-target):** `tests/harness.rs` — ten
  invariant groups over the committed synthetic fixture (`tests/fixtures/synthetic`,
  frozen `index.scip`) through the headless `ingest_partition` entrypoint. No `/tmp`, no
  FRAKTAG checkout, no network, no fixture generation at test time. Fixture: 15 nodes
  (9 matched, 2 reconciled, 4 fallback, 2 FILE), 11 edges (2 calls / 6 decl-refs /
  3 file-scope), dangling 0/0.
- **Engine regression (manual, `#[ignore]`):** `tests/engine_ignored.rs` — real
  `@fraktag/engine`: fallback 185, calls 717, declaration refs 2799, file-scope 634,
  dangling 0/0; `engine.scip` vs `engine2.scip` yield identical canonical keys. External
  fixtures, NOT committed; NOT in default CI.
- **The ten groups → criteria:** deterministic re-ingest (3), byte-equal identity
  adoption (3), fallback bounded+labeled / FILE = `AstFileScope` (1,2,7), strict calls /
  zero FILE-scope calls (4), reference split incl. `FileScopeReference` (4), graph closure
  / no dangling endpoints (9), cross-file edge (5), complexity attachment (6), provenance
  (7), `repo-graph-ir` dependency boundary (8).
- **Reproduce:** `cargo test -p repo-graph-scip-ingest` (CI);
  `cargo test -p repo-graph-scip-ingest -- --ignored` (engine, where fixtures present).

## References
- `docs/slices/scip-ingest-ir-1.md` (IR design, D1–D5)
- `docs/architecture/scip-migration-plan.md` (Stage A)
- `docs/architecture/adr/adr-extraction-substrate-scip-first.md`
- `docs/audits/scip-ts-parity-spike-1/findings.md` (engine.scip evidence)
