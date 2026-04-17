# rmap state boundaries v1

Status: PLANNED (pre-implementation)

Normative contract: `docs/architecture/state-boundary-contract.txt`.
That contract is authoritative for all data and emission decisions.
This document is the implementation plan: scope, slice order,
acceptance criteria, and deferred items. It is not normative.

## Goal

Ship state-boundary extraction in Rust as the first concrete
realization of the vision's boundary/seam expansion priority.

The normative contract
(`docs/architecture/state-boundary-contract.txt`) is
language-agnostic in DESIGN: stable-key shape, edge types, node
kinds, matcher form, basis tags, and evidence structure are
identical for any language whose extractor consumes the shared
binding table. Slice 1's language COVERAGE is fork-dependent and
selected at SB-0 close:

- **Fork 1 (TS-only):** slice 1 covers TypeScript / JavaScript
  only (via `rust/crates/ts-extractor`). Coverage for Rust,
  Python, Java, and C++ source is deferred to follow-on slices
  that are gated on new or ported extractor adapters.
- **Fork 2 (TS AST + detectors-backed text pass):** slice 1
  covers all five languages via mixed AST extraction (TS) and
  text-pattern seam detection (Rust / Python / Java / C++).
  Requires a contract amendment for the text-pattern matcher.
- **Fork 3 (prerequisite extractor program):** slice 1 covers
  TypeScript / JavaScript only. Coverage for each of the
  remaining languages lands only after the corresponding RE-n
  extractor-adapter program closes.

After the fork-scoped slice 1 closes, any symbol in a covered
language that touches a database driver, standard-library
filesystem API, cache client, or AWS S3 blob endpoint is
represented as a `READS` or `WRITES` edge from the symbol to a
repo-scoped resource node, per the normative contract.

Slice 1 is Rust-first in runtime: TS-codebase extractors
(`src/adapters/extractors/*`) are not modified. See contract §10
and TECH-DEBT.md for the TS-runtime parity posture.

## Scope

### In scope

- New Rust crates:
  - `rust/crates/state-bindings` — pure policy (binding table,
    matcher primitives, stable-key builders, basis tags).
  - `rust/crates/state-extractor` — integrates with existing
    language extractors and emits resource nodes + READS / WRITES
    edges.
- New node kinds:
  - `DB_RESOURCE`
  - `FS_PATH`
  - `BLOB`
  Plus subtype additions on existing `STATE` kind (`CACHE` subtype).
- Reuse of existing edge types: `READS`, `WRITES`.
- Widened `--edge-types` filter on `rmap callers` and `rmap callees`
  to accept `READS` and `WRITES`.
- Binding table (TOML, `include_str!`). Per contract §9.3.1,
  fork-scoped entries MUST be shipped; entries for out-of-fork
  languages MAY be added forward-compatibly but are not required
  in slice 1. The default policy under Fork 1 and Fork 3 is to
  ship TS/JS entries only; Fork 2 ships the full five-language
  matrix.
- Validation on the subset of the four-repository corpus
  exercised by the selected SB-0 fork (see "Validation corpus").
- Documentation: contract, milestone (this doc), schema.txt
  update, TECH-DEBT.md appendix. Under Fork 2 only: a contract
  amendment defining the text-pattern matcher and basis.

### Out of scope (deferred)

- Queue / event-boundary extraction (`EMITS`, `CONSUMES`, `QUEUE`
  node kind).
- Config / env seam graph emission. State-boundary extraction
  carries config/env provenance in edge evidence only (via
  `logical_name_source = "env_key"`); it does NOT emit
  `CONFIG_KEY` nodes or companion edges. Companion-node emission,
  explicit config→resource wiring edges, and cross-language
  CONFIG_KEY identity are reserved for a separate config/env
  seam slice. See state-boundary-contract.txt §5.6.
- Test detectors, coverage ingestion, sanitizer artifacts.
- SQL-string parsing; per-table granularity.
- ORM / repository / DAO pattern inference.
- Compiler / LSP type-enrichment for receiver resolution.
- `rmap state` or any new dedicated CLI command.
- TS-side extractor parity.
- GCP / Azure blob SDKs, non-AWS cloud clients.
- C++ database / cache / blob SDKs.

## Crate layout (G1a)

```
rust/crates/state-bindings/
  src/
    lib.rs                  Public API surface
    table.rs                TOML loader, schema types, validators
    matcher.rs              Form-A matcher primitives
    stable_key.rs           Stable-key builders (5 shapes)
    basis.rs                Basis tag enum
  bindings.toml             Embedded via include_str!
  tests/
    table.rs                Binding table parse + schema validation
    stable_key.rs           Stable-key builder contract tests
    matcher.rs              Form-A matcher unit tests

rust/crates/state-extractor/
  src/
    lib.rs                  Public API surface
    emit.rs                 Node + edge emission (dedup within
                            snapshot, evidence shape)
    languages/
      mod.rs
      rust.rs
      typescript.rs
      python.rs
      java.rs
      cpp.rs
  tests/
    emit.rs                 Fixture-based end-to-end unit tests
    languages/
      rust.rs
      typescript.rs
      python.rs
      java.rs
      cpp.rs
```

Rationale: `state-bindings` is pure policy with different change
frequency than `state-extractor`. CCP/CRP-clean separation. The
extractor crate depends on `state-bindings`, not the reverse.

## Slice inventory

Implementation order follows a characterization-first Feathers
discipline. Slice inventory is split into two phases:

- **Phase A — prerequisite discovery and fork resolution.**
  SB-0 produces a written inventory of Rust extractor ownership
  per language and forces an explicit fork decision. Phase B slices
  are conditional on that decision.

- **Phase B — implementation.** Exact shape depends on the SB-0
  fork outcome. Three candidate shapes are enumerated below; one
  is selected at SB-0 close.

### Phase A — prerequisite discovery

| Slice | Content |
|-------|---------|
| SB-0  | Extractor-ownership inventory + fork decision. See "SB-0 detail" below. Blocks all implementation. No code changes. |

### SB-0 detail

SB-0 is a discovery and decision slice, not a coding slice. Its
deliverable is a short written inventory and a locked fork
decision. Concretely:

1. **Workspace inventory.** For each source language the product
   claims to support (Rust, TypeScript, Python, Java, C, C++),
   record the current state in the Rust workspace:
   - Does a concrete extractor adapter crate exist? If yes, name
     it and its `ExtractorPort` implementation entry point.
   - Is the file-routing vocabulary wired to it? (Check
     `rust/crates/indexer/src/routing.rs::detect_language` — any
     language returning `None` has no Rust-side extractor.)
   - Are there any in-progress extractor adapters not yet
     registered?

   Known at contract-design time (2026-04-17):
   - Rust workspace has exactly one registered language extractor:
     `rust/crates/ts-extractor` (TypeScript / JavaScript / TSX /
     JSX only).
   - `rust/crates/detectors` is a text/pattern seam-detection
     library, not an AST extractor. It does NOT own AST facts
     for any language.
   - `rust/crates/indexer/src/routing.rs` has extension
     vocabulary for rust/java/python/c/cpp but `detect_language`
     returns `None` for all of them. The file routing can list
     these extensions as source, but no adapter is registered to
     extract from them.
   - All current non-TS/JS extractor adapters live under the TS
     codebase at `src/adapters/extractors/{java,python,rust,cpp,
     manifest,common,cli}/` and are not ported to Rust.

2. **Verify design-time gate properties (re-confirm each
   before Phase B begins).**
   - No `CHECK` constraint or enumerated lookup table on
     `nodes.kind` or `edges.type` (TS and Rust storage).
     (Verified at contract-design time; re-verify as a gate
     before SB-1 lands.)
   - TS storage adapter tolerates `nodes.kind` values it does
     not recognize (required for cross-runtime interop after
     `DB_RESOURCE`, `FS_PATH`, `BLOB` are introduced). Add a
     specific interop test case in SB-2 or SB-8 that exercises
     a Rust-written DB containing the three new kinds.

3. **Fork decision.** After the inventory, choose ONE of the
   following slice-1 shapes and record the choice with its
   rationale in this milestone document:

   **Fork 1 — TS-only first, other languages deferred.**

   Slice 1 ships state-boundary extraction for TypeScript /
   JavaScript only, integrated with `rust/crates/ts-extractor`.
   Java, Python, Rust-language, and C++ state-boundary coverage
   become separate future slices, each gated on first building
   or porting the respective language's Rust extractor adapter.

   Pros: smallest scope aligned with shipped substrate. No
   new extractor adapter work. Validates end-to-end
   state-binding → edge emission pipeline on real TS code.
   Matches the user's Node/AWS repo directly.

   Cons: Java/Python/C++ state-boundary value comes later. The
   Spring validation corpus cannot be exercised in slice 1
   unless it also has a TS/JS companion or the slice boundary
   is redrawn.

   **Fork 2 — TS extractor + detectors-backed text-pattern
   seam pass for other languages.**

   Slice 1 integrates `state-extractor` with `ts-extractor` for
   TS/JS, AND extends `detectors` (or adds a peer crate) with a
   text-pattern pass for Rust/Java/Python/C++ source files that
   emits state-boundary edges without full AST ownership.

   Pros: covers all five languages in slice 1 using shipped
   infrastructure. Validates all four validation corpora.
   Delivers the ecosystem-neutral goal in one slice.

   Cons: introduces a second emission path (AST-based for TS,
   text-pattern for others). Text-pattern detection has
   different precision/recall and evidence provenance than
   AST extraction; the contract currently assumes
   import-anchored AST matching (Form A, contract §6.1). This
   fork requires an explicit contract amendment defining how
   text-pattern matching fits Form A, or a new Form A' with
   its own basis tag. Not zero-cost.

   **Fork 3 — build out Rust extractor adapters as a
   prerequisite program, before state-boundary edges ship for
   non-TS languages.**

   Slice 1 ships TS-only state-boundary edges. A parallel or
   preceding program ports / builds Rust, Java, Python, and
   C++ extractor adapters to the Rust workspace. Non-TS
   state-boundary extraction waits until the respective
   adapter is registered in `detect_language`.

   Pros: no compromise on AST fidelity. Every language's
   extractor is a full adapter implementing `ExtractorPort`.
   Long-term correct direction.

   Cons: extractor-adapter work is large (five languages × TS
   parity). Delays ecosystem-neutral state-boundary delivery
   by the duration of that program. Partially competes with
   other vision priorities (module discovery Layer 2, runtime/
   build environment model).

4. **Lock acceptance criteria.** Only after the fork is chosen,
   rewrite Phase B slices below (currently placeholders) to
   match. Until SB-0 closes, Phase B slices are declared
   "Pending fork selection" and MUST NOT be executed.

### Phase B — implementation (pending SB-0 fork selection)

Exact Phase B shape depends on the SB-0 outcome. Three candidate
inventories below; only one is selected.

**If SB-0 selects Fork 1 (TS-only):**

| Slice | Content |
|-------|---------|
| SB-1  | `state-bindings` crate: binding table loader, TOML schema validator, stable-key builders, basis tag enum. Unit tests. No extractor integration. |
| SB-2  | `state-extractor` crate skeleton: emission API, resource-node dedup, evidence shape. Consumes `state-bindings`. Unit tests with synthetic fixtures. Cross-runtime interop test case added for `DB_RESOURCE`, `FS_PATH`, `BLOB` node kinds. |
| SB-3  | Integrate `state-extractor` with `rust/crates/ts-extractor`. Ship `fs`, `node:fs`, `fs/promises` (stdlib) and `@aws-sdk/client-s3`, `pg`, `mysql2`, `better-sqlite3`, `sqlite3`, `redis`, `ioredis` (SDK) bindings for TypeScript. |
| SB-4  | Validate on the user's Node/AWS repo. Validate on the user's React/Next repo. Spot-check and record closure evidence per milestone "Acceptance criteria". |
| SB-5  | Widen `rmap callers` / `rmap callees` `--edge-types` to accept `READS`, `WRITES`. Exclude resource kinds from `rmap dead`. Update per-command tests. |
| SB-6  | Closure: schema.txt, TECH-DEBT.md, ROADMAP updates; contract frozen at v1. Java / Python / Rust-language / C++ state-boundary extraction explicitly declared deferred with their own follow-on slices. Spring and C++ corpus validation deferred (non-blocker). |

**If SB-0 selects Fork 2 (TS AST + detectors-backed text pass):**

| Slice | Content |
|-------|---------|
| SB-1  | `state-bindings` crate (same as Fork 1). |
| SB-2  | `state-extractor` crate skeleton (same as Fork 1). |
| SB-3  | Contract amendment: define how text-pattern matching fits Form A (or add Form A'). Additional basis tag if required. Amendment shipped before any text-pattern edges land. |
| SB-4  | `ts-extractor` integration + Node/AWS corpus validation (same as Fork-1 SB-3/SB-4). |
| SB-5  | Text-pattern extension in `detectors` (or peer crate): shared seam pattern library consumed by the per-language text pass. Emits resource nodes and edges with the text-pattern basis. |
| SB-6  | Python bindings + validation. |
| SB-7  | Java bindings + Spring corpus validation. |
| SB-8  | Rust-language bindings + small fixture validation (repo-graph self-index acceptable). |
| SB-9  | C++ bindings + public C++ corpus validation. |
| SB-10 | CLI widening (same as Fork-1 SB-5). |
| SB-11 | Closure (same as Fork-1 SB-6). |

**If SB-0 selects Fork 3 (prerequisite extractor program):**

| Slice | Content |
|-------|---------|
| SB-1  | `state-bindings` crate (same as Fork 1). |
| SB-2  | `state-extractor` crate skeleton (same as Fork 1). |
| SB-3  | `ts-extractor` integration + Node/AWS corpus validation (same as Fork-1 SB-3/SB-4). |
| SB-4  | Declare slice 1 closed on TS. Open a parallel program: RE-n for each language's Rust extractor adapter (RE-rust, RE-python, RE-java, RE-cpp). Each is its own milestone with a separate milestone doc. State-boundary coverage for each language blocks on the corresponding RE-n closing. |
| SB-5  | CLI widening (same as Fork-1 SB-5). |
| SB-6  | Closure for TS-only slice 1. Non-TS state-boundary slices follow once the corresponding RE-n lands. |

Slice numbering is SB-* (state boundaries) to avoid collision with
the structural Rust-N numbering of the prior milestone. RE-n (Rust
Extractor for language n) is reserved for the Fork-3 prerequisite
program if selected.

## Validation corpus

The full corpus is four repositories. Which subset is exercised in
slice 1 depends on the SB-0 fork outcome:

- **Fork 1 (TS-only):** Node/AWS and React/Next exercised in
  slice 1. Spring and C++ deferred to their own follow-on slices
  (gated on Java / C++ extractor availability).
- **Fork 2 (TS AST + detectors-backed text pass):** all four
  exercised in slice 1.
- **Fork 3 (prerequisite extractor program):** Node/AWS and
  React/Next exercised in slice 1. Spring and C++ gated on
  RE-java and RE-cpp closure.

Full corpus inventory (repos and selection criteria):

1. Node + AWS Lambda repo (user-supplied, private).
2. Spring repo (user-supplied, private).
3. React / Next repo with real storage touchpoints (user-supplied,
   private).
4. One public C++ repo with real filesystem / process / state use
   (to be selected before the C++ slice, whichever fork it lands
   in).

Synthetic fixtures are unit / integration support, not primary
evidence. For each corpus entry exercised by the selected fork,
closure evidence MUST include:

- Total count of READS / WRITES state-boundary edges.
- Distribution across the resource kinds emitted (kinds not
  relevant to a corpus, e.g. BLOB on a repo without S3 use, may
  legitimately be zero).
- Distribution across `basis` tags (stdlib_api vs sdk_call; Fork 2
  adds text-pattern basis as a third distribution bin).
- Count of call sites where dynamic-target drop (contract §5.4)
  suppressed an edge, to gauge miss rate.
- At least five manually spot-checked edges per corpus with the
  source line, binding key, and resource node noted.

## Acceptance criteria

Slice 1 is accepted when ALL of the following hold:

1. SB-0 is closed with a recorded fork decision and a workspace
   inventory. If Fork 2 or Fork 3 adds new crates or amendments
   (text-pattern pass; RE-n extractor program), those additions
   are reflected in this document before Phase B begins.
2. Both new crates (`state-bindings`, `state-extractor`) build
   and pass their unit tests.
3. `pnpm run test:rust` is green (full Rust workspace).
4. `pnpm run test:interop` is green and asserts that the TS
   storage adapter reads Rust-written databases containing the
   three new `nodes.kind` values (`DB_RESOURCE`, `FS_PATH`,
   `BLOB`) without crashing or dropping rows.
5. On each validation corpus EXERCISED by the selected fork:
   - Indexing completes without error.
   - At least one READS and one WRITES state-boundary edge is
     emitted when the corpus has relevant code.
   - No resource node appears in `rmap dead` output.
   - Spot-checked edges align with the contract's stable-key,
     evidence, and direction rules.
6. `docs/architecture/schema.txt` updated with the new node
   kinds and subtypes (already done at contract-design time for
   the three net-new kinds; re-verify no further schema change
   was needed during implementation).
7. `docs/TECH-DEBT.md` updated with:
   - The Rust-only / TS parity gap (done at contract-design time;
     update with any corrections discovered during implementation).
   - The set of languages whose state-boundary coverage remains
     deferred per the selected fork (e.g. under Fork 1: Java,
     Python, Rust-language, C++ deferred).
8. `docs/ROADMAP.md` updated to reflect slice 1 shipped status,
   the selected fork, and the next-slice pointer (remaining
   language coverage under the chosen fork, then queue
   boundaries, then test detectors, then coverage ingestion).
9. The contract version remains `state_boundary_version: 1`
   unless a contract amendment for Fork 2 (text-pattern basis)
   was shipped, in which case the version bump rules in
   contract section 13 apply.

## Deferred items (post-slice-1)

Tracked here for visibility; each becomes its own future slice.

- SB-next-A: Queue / event boundaries. `EMITS`, `CONSUMES`, `QUEUE`
  node kind. Kafka, SQS, SNS, RabbitMQ, Redis pub/sub.
- SB-next-B: TS-side parity. Port `state-bindings` + `state-extractor`
  logic to TS adapters or decide parity is permanently Rust-only.
- SB-next-C: SQL-string parsing for table-level DB granularity.
  Introduces `TABLE` node kind usage and `sql_literal` basis.
- SB-next-D: ORM / repository pattern inference (Prisma, JPA,
  SQLAlchemy, TypeORM).
- SB-next-E: GCP / Azure blob coverage; C++ DB/cache/blob coverage.
- SB-next-F: Type-enrichment-backed matching (Form B) where
  receiver resolution is available.
- SB-next-G: Dedicated `rmap state` CLI command for resource-node
  enumeration and cross-symbol touchpoint queries.
- SB-next-H: State-boundary trust / reliability axis in the trust
  report once a ground-truth corpus is available.

## Open questions (to be answered during implementation, not now)

- Within SB-1, exactly which TOML deserializer crate to use
  (`toml` vs `basic-toml`). Either is acceptable; choice is
  implementation detail.
- Within SB-2, whether resource-node dedup uses a per-snapshot
  hash map in-memory or defers to the storage adapter's unique
  index on `(snapshot_uid, stable_key)`. Both viable; prefer
  in-memory dedup to avoid per-call round-trips.
- Within the TS-extractor integration slice (Fork 1 SB-3 /
  Fork 2 SB-4 / Fork 3 SB-3), whether `ts-extractor`'s
  import-binding resolver surfaces the imported symbol path at
  sufficient fidelity for Form-A matching. Inspect during
  SB-0 discovery; if insufficient, a prerequisite extractor-side
  slice is required before state-boundary edges can emit from
  TS sources. This is a known unknown.

These are implementation-local and do not affect the contract
(with the Fork-2 caveat: a text-pattern basis amendment IS a
contract change and is tracked in the SB-0 fork decision).

## Non-goals

- No coverage claim that this slice improves dead-code detection.
  Resource nodes are simply excluded from dead-code results;
  symbol-level dead-code soundness is unchanged.
- No claim about runtime execution evidence. Slice 1 is static
  extraction only.
- No binary-size or performance budget. Real-corpus indexing
  time regression must be measured but no threshold is committed
  in slice 1.

