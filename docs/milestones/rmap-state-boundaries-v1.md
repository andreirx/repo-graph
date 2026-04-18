# rmap state boundaries v1

Status: ACTIVE. SB-0 closed 2026-04-17, Fork 1 locked. SB-1 is
the next slice.

Normative contract: `docs/architecture/state-boundary-contract.txt`.
That contract is authoritative for all data and emission decisions.
This document is the implementation plan: scope, slice order,
acceptance criteria, and deferred items. It is not normative.

SB-0 closure and the workspace inventory that justified Fork 1 are
recorded in `docs/milestones/rmap-state-boundaries-sb-0.md`.

## Goal

Ship state-boundary extraction in Rust for TypeScript, TSX,
JavaScript, and JSX as the first concrete realization of the
vision's boundary/seam expansion priority.

After slice 1 closes, any symbol in a TS-family source file that
touches a database driver, a standard-library filesystem API, a
cache client, or an AWS S3 blob endpoint is represented as a
`READS` or `WRITES` edge from the symbol to a repo-scoped
resource node, per the normative contract.

The normative contract is language-agnostic in design (stable-key
shape, edge types, node kinds, matcher form, basis tags, and
evidence structure are identical for any language whose extractor
consumes the shared binding table). Slice 1's language coverage
is TS-family only because that is the only AST-backed extractor
registered in the Rust workspace today
(`rust/crates/ts-extractor/src/extractor.rs:63`). Coverage for
Rust, Python, Java, and C++ sources is deferred; each becomes a
dedicated follow-on slice gated on the availability of a Rust-
workspace extractor adapter for that language.

Slice 1 is Rust-first in runtime: TS-codebase extractors
(`src/adapters/extractors/*`) are not modified. See contract §10
and TECH-DEBT.md for the TS-runtime parity posture.

## Scope

### In scope

- New Rust crates:
  - `rust/crates/state-bindings` — pure policy (binding table,
    matcher primitives, stable-key builders, basis tags).
  - `rust/crates/state-extractor` — consumes `state-bindings`
    and integrates with `rust/crates/ts-extractor` to emit
    resource nodes and `READS` / `WRITES` edges during
    extraction.
- New node kinds (emitted by slice 1):
  - `DB_RESOURCE`
  - `FS_PATH`
  - `BLOB`
  Plus a new `CACHE` subtype on the existing `STATE` kind.
- Reuse of existing edge types: `READS`, `WRITES`.
- Widened `--edge-types` filter on `rmap callers` and `rmap
  callees` to accept `READS` and `WRITES`.
- Binding table (TOML, `include_str!`). Slice 1 ships TS/JS
  entries only. Entries for Rust, Python, Java, and C++ may be
  added forward-compatibly if trivial but are not required
  (contract §9.3.1).
- Exclusion of resource kinds from `rmap dead` output.
- Validation on the user's Node/AWS repo and React/Next repo.
- Documentation: contract (already written), milestone (this
  doc), schema.txt (already updated for new node kinds),
  TECH-DEBT.md (already appended with Fork-1 deferrals).

### Out of scope

- **Non-TS language state-boundary coverage.** Rust-language,
  Python, Java (including Spring), and C++ state-boundary
  extraction are explicitly deferred. Each requires a Rust-
  workspace extractor adapter for the respective language,
  which does not exist today. Each becomes a separate future
  slice gated on its RE-n extractor program.
- Queue / event-boundary extraction (`EMITS`, `CONSUMES`,
  `QUEUE` node kind).
- Config / env seam graph emission. State-boundary extraction
  carries config/env provenance in edge evidence only (via
  `logical_name_source = "env_key"`); it does NOT emit
  `CONFIG_KEY` nodes or companion edges. Companion-node
  emission, explicit config→resource wiring edges, and cross-
  language CONFIG_KEY identity are reserved for a separate
  config/env seam slice. See state-boundary-contract.txt §5.6.
- Test detectors, coverage ingestion, sanitizer artifacts.
- SQL-string parsing; per-table granularity.
- ORM / repository / DAO pattern inference.
- Compiler / LSP type-enrichment for receiver resolution.
- `rmap state` or any new dedicated CLI command.
- TS-runtime parity (TS-codebase extractors emit no state-
  boundary edges; documented gap).
- GCP / Azure blob SDKs, non-AWS cloud clients.
- Use of `rust/crates/detectors` for state-boundary emission
  (rejected at SB-0 closure; see `rmap-state-boundaries-sb-0.md`
  §2.3 for the rationale).

## Crate layout (G1a)

The state-extractor crate starts with TS-only language support.
Additional language modules are added when their respective
extractors become available in the Rust workspace.

```
rust/crates/state-bindings/
  src/
    lib.rs                  Public API surface
    table.rs                TOML loader, schema types, validators
    matcher.rs              Form-A matcher primitives
    stable_key.rs           Stable-key builders (4 shapes: db, fs, cache, blob)
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
      typescript.rs         TS/TSX/JS/JSX integration with ts-extractor
  tests/
    emit.rs                 Fixture-based end-to-end unit tests
    languages/
      typescript.rs
```

Rationale: `state-bindings` is pure policy with different change
frequency than `state-extractor`. CCP/CRP-clean separation. The
extractor crate depends on `state-bindings`, not the reverse.

The `languages/` directory intentionally ships with only
`typescript.rs` in slice 1. Adding language modules for Rust,
Python, Java, or C++ is scoped to the respective future slice,
not slice 1.

## Slice inventory

Implementation order follows a characterization-first Feathers
discipline.

| Slice | Status | Content |
|-------|--------|---------|
| SB-0  | Closed | Extractor-ownership inventory and Fork-1 selection. See `rmap-state-boundaries-sb-0.md`. |
| SB-1  | Next   | `state-bindings` crate: binding table loader, TOML schema validator, stable-key builders, basis tag enum, matcher primitives. Unit tests only. No extractor integration. |
| SB-2  | Planned | `state-extractor` crate skeleton: emission API, resource-node dedup (in-memory per snapshot), evidence shape. Consumes `state-bindings`. Unit tests with synthetic fixtures. Cross-runtime interop test case added for the three new node kinds (`DB_RESOURCE`, `FS_PATH`, `BLOB`). |
| SB-3  | Planned | Integrate `state-extractor` with `rust/crates/ts-extractor`. Ship TS/JS binding entries: `fs`, `node:fs`, `fs/promises` (stdlib), `@aws-sdk/client-s3` (SDK blob), `pg`, `mysql2`, `better-sqlite3`, `sqlite3` (SDK db), `redis`, `ioredis` (SDK cache). Verify `ts-extractor`'s import-binding resolver surfaces the imported symbol path at sufficient fidelity for Form-A matching; if insufficient, an extractor-side prerequisite slice is triggered before SB-3 closes. |
| SB-4  | Planned | Validate on the user's Node/AWS repo. Validate on the user's React/Next repo. Spot-check and record closure evidence per "Acceptance criteria". |
| SB-5  | Planned | Widen `rmap callers` / `rmap callees` `--edge-types` to accept `READS` and `WRITES`. Exclude resource kinds (`DB_RESOURCE`, `FS_PATH`, `STATE`, `BLOB`) from `rmap dead`. Update per-command tests. |
| SB-6  | Planned | Closure: re-verify `schema.txt` and `TECH-DEBT.md` (already pre-updated at contract-design time); amend with any implementation-discovered corrections. Update `docs/ROADMAP.md` to reflect slice 1 shipped under Fork 1. Freeze contract at `state_boundary_version: 1`. |

Slice numbering is SB-* (state boundaries) to avoid collision
with the structural Rust-N numbering of the prior milestone.

## Validation corpus

Exercised in slice 1:

1. Node + AWS Lambda repo (user-supplied, private).
2. React / Next repo with real storage touchpoints (user-
   supplied, private).

Deferred to follow-on slices (not blockers for slice 1 closure):

3. Spring repo (user-supplied, private) — gated on a Java
   Rust-workspace extractor.
4. One public C++ repo with real filesystem / process / state
   use — gated on a C++ Rust-workspace extractor.

Synthetic fixtures are unit / integration support, not primary
evidence. For each corpus entry exercised by slice 1, closure
evidence MUST include:

- Total count of READS / WRITES state-boundary edges.
- Distribution across the resource kinds emitted (kinds not
  relevant to a corpus, e.g. BLOB on a repo without S3 use,
  may legitimately be zero).
- Distribution across `basis` tags (`stdlib_api` vs `sdk_call`).
- Count of call sites where dynamic-target drop (contract
  §5.4) suppressed an edge, to gauge miss rate.
- At least five manually spot-checked edges per corpus with
  the source line, binding key, and resource node noted.

## Acceptance criteria

Slice 1 is accepted when ALL of the following hold:

1. Both new crates (`state-bindings`, `state-extractor`) build
   and pass their unit tests.
2. `pnpm run test:rust` is green (full Rust workspace).
3. `pnpm run test:interop` is green and asserts that the TS
   storage adapter reads Rust-written databases containing the
   three new `nodes.kind` values (`DB_RESOURCE`, `FS_PATH`,
   `BLOB`) without crashing or dropping rows.
4. On each exercised validation corpus (Node/AWS,
   React/Next):
   - Indexing completes without error.
   - At least one READS and one WRITES state-boundary edge is
     emitted when the corpus has relevant code.
   - No resource node appears in `rmap dead` output.
   - Spot-checked edges align with the contract's stable-key,
     evidence, and direction rules.
5. `docs/architecture/schema.txt` reflects the three new node
   kinds and their subtypes (already done at contract-design
   time; re-verify no further schema change was needed during
   implementation).
6. `docs/TECH-DEBT.md` reflects:
   - The Rust-only / TS-runtime parity gap (already done at
     contract-design time; update with any corrections
     discovered during implementation).
   - The explicit language-coverage deferrals under Fork 1
     (Java, Python, Rust-language, C++).
7. `docs/ROADMAP.md` updated to reflect slice 1 shipped under
   Fork 1 and the follow-on slice pointer (queue boundaries,
   then config/env seam, then test detectors, then coverage
   ingestion; non-TS language coverage on the extractor-adapter
   program track).
8. Contract version remains `state_boundary_version: 1`.

## Alternatives considered and rejected

Recorded here for traceability. See
`docs/milestones/rmap-state-boundaries-sb-0.md` for the full
rationale.

- **Fork 2 — TS AST + detectors-backed text-pattern pass.**
  Rejected. `rust/crates/detectors` does exist as a cross-
  language seam-detection substrate, but using it for state-
  boundary emission would require a contract amendment for a
  text-pattern matcher basis, a translator layer from detector
  facts to graph nodes/edges (the detector's output is file-
  scoped surface facts, not symbol-scoped graph edges), and
  would introduce mixed-fidelity substrate that contaminates
  trust, dead-code, and explain downstream consumers. Net cost
  greater than initially framed.

- **Fork 3 — prerequisite extractor program.**
  Rejected as scope for slice 1. Architecturally correct long-
  term, but porting four language extractors (Rust, Java,
  Python, C++) from the TS codebase to the Rust workspace is a
  program on its own scale with its own acceptance criteria and
  priority discussion. Yoking it to the state-boundary slice
  delays state-boundary delivery indefinitely. The follow-on
  language coverage slices each become, in effect, a scoped
  RE-n program selected against other vision priorities as they
  open.

## Deferred items (post-slice-1)

Tracked here for visibility; each becomes its own future slice.

- SB-next-A: Queue / event boundaries. `EMITS`, `CONSUMES`,
  `QUEUE` node kind. Kafka, SQS, SNS, RabbitMQ, Redis pub/sub.
- SB-next-B: Config / env seam slice. `CONFIG_KEY` graph
  emission, explicit config→resource wiring edges, cross-
  language CONFIG_KEY identity. May reuse existing
  `rust/crates/detectors` env-detection via a translator.
- SB-next-C: SQL-string parsing for table-level DB granularity.
  Introduces `TABLE` node kind usage and `sql_literal` basis.
- SB-next-D: ORM / repository pattern inference (Prisma, JPA,
  SQLAlchemy, TypeORM).
- SB-next-E: GCP / Azure blob coverage.
- SB-next-F: Type-enrichment-backed matching (Form B) where
  receiver resolution is available.
- SB-next-G: Dedicated `rmap state` CLI command for
  resource-node enumeration and cross-symbol touchpoint
  queries.
- SB-next-H: State-boundary trust / reliability axis in the
  trust report once a ground-truth corpus is available.
- SB-next-I (language coverage track): Java / Spring state-
  boundary coverage. Blocked on a Java Rust-workspace extractor.
- SB-next-J (language coverage track): Python state-boundary
  coverage. Blocked on a Python Rust-workspace extractor.
- SB-next-K (language coverage track): Rust-language state-
  boundary coverage. Blocked on a Rust-language Rust-workspace
  extractor.
- SB-next-L (language coverage track): C++ state-boundary
  coverage. Blocked on a C++ Rust-workspace extractor.
- SB-next-M: TS-runtime parity (port `state-bindings` +
  `state-extractor` logic to TS adapters, or formally declare
  Rust-only parity permanent).

## Open questions (to be answered during implementation)

- SB-1: exactly which TOML deserializer crate to use (`toml`
  vs `basic-toml`). Either is acceptable; choice is
  implementation detail.
- SB-2: whether resource-node dedup uses a per-snapshot hash
  map in-memory or defers to the storage adapter's unique
  index on `(snapshot_uid, stable_key)`. Both viable; prefer
  in-memory dedup to avoid per-call round-trips.
- SB-3: whether `rust/crates/ts-extractor`'s import-binding
  resolver surfaces the imported symbol path at sufficient
  fidelity for Form-A matching. Inspect early in SB-3; if
  insufficient, a prerequisite extractor-side slice precedes
  state-boundary edge emission from TS sources. This is the
  principal known unknown.

These are implementation-local and do not affect the contract.

## Non-goals

- No coverage claim that this slice improves dead-code
  detection. Resource nodes are simply excluded from dead-code
  results; symbol-level dead-code soundness is unchanged.
- No claim about runtime execution evidence. Slice 1 is static
  extraction only.
- No binary-size or performance budget. Real-corpus indexing
  time regression must be measured but no threshold is
  committed in slice 1.
