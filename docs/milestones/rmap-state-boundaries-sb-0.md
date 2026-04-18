# SB-0 Closure — Extractor-Ownership Inventory and Fork Decision

Status: CLOSED. Fork 1 accepted and locked 2026-04-17.
Date:   2026-04-17

This document is the deliverable of SB-0, the discovery and
decision slice of the state-boundary program
(`docs/milestones/rmap-state-boundaries-v1.md`). It contains the
two artifacts SB-0 was required to produce:

1. Extractor-ownership inventory.
2. Fork decision with rationale and locked Phase B sequence.

The Fork-1 selection recorded in §2 is the operative plan. The
main milestone document has been updated to reflect the locked
sequence; Fork 2 and Fork 3 are recorded there only as
alternatives-considered-and-rejected.

This document is preserved as the discovery record that
justified the selection. It is not updated once closed; if
future evidence warrants reopening the fork decision, a new
SB-0.x document is created rather than editing this one.


## 1. Extractor-ownership inventory

### 1.1 Concrete `ExtractorPort` implementations

Search: `impl ExtractorPort` across `rust/crates/`.

Results:

| Location | Type | Role |
|---|---|---|
| `rust/crates/ts-extractor/src/extractor.rs:63` | `TsExtractor` | The only production extractor. Handles `typescript`, `tsx`, `javascript`, `jsx`. |
| `rust/crates/indexer/src/orchestrator.rs:1201` | `MockExtractor` | Test-only. |
| `rust/crates/indexer/src/orchestrator.rs:1410` | `FailingExtractor` | Test-only. |
| `rust/crates/indexer/src/routing.rs:529` | `FakeExtractor` | Test-only. |

**Conclusion:** the Rust workspace ships exactly ONE production
language extractor. It emits AST-backed graph nodes and edges
for TypeScript, TSX, JavaScript, and JSX via `tree-sitter-typescript`.

### 1.2 Language routing vocabulary

`rust/crates/indexer/src/routing.rs`:

- `is_source_extension` (line 23) recognizes extensions for TS/JS,
  Rust, Java, Python, C, and C++.
- `language_to_extensions` (line 98) knows extension sets for
  `typescript`, `tsx`, `javascript`, `rust`, `java`, `python`,
  `c`, `cpp`.
- `detect_language` (line 83) returns `Some("typescript")`,
  `Some("tsx")`, `Some("javascript")`, `Some("jsx")` — and
  `None` for EVERY other extension, including `.rs`, `.java`,
  `.py`, `.c`, `.h`, `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx`.

**Conclusion:** the Rust workspace has EXTENSION RECOGNITION
vocabulary for all five languages but no adapter is registered
to produce AST facts from non-TS sources. Rust/Java/Python/C/C++
files walk the tree as source files but no `ExtractorPort`
impl consumes them.

### 1.3 AST-backed fact production, by language

| Language | AST extractor in Rust workspace | AST extractor in TS workspace |
|---|---|---|
| TypeScript / TSX / JS / JSX | `rust/crates/ts-extractor` | `src/adapters/extractors/typescript/` |
| Rust (language) | NONE | `src/adapters/extractors/rust/` |
| Java | NONE | `src/adapters/extractors/java/` (incl. Spring bean/route detectors) |
| Python | NONE | `src/adapters/extractors/python/` |
| C / C++ | NONE | `src/adapters/extractors/cpp/` |

**Conclusion:** Rust-workspace AST extraction is currently
possible ONLY for the TS family. All other language extractors
live exclusively in the TypeScript codebase and have not been
ported. Any slice 1 extraction path that claims cross-language
coverage in the Rust workspace today CANNOT be AST-backed for
non-TS sources.

### 1.4 The `detectors` crate — capability and limitations

Location: `rust/crates/detectors/`.

Public API (`rust/crates/detectors/src/lib.rs`):

```rust
pub use pipeline::{detect_env_accesses, detect_fs_mutations, production_pipeline};
```

Capabilities:

- Cross-language seam detection across `Ts`, `Py`, `Rs`, `Java`,
  `C` (includes C++). See
  `rust/crates/detectors/src/types.rs:99-107`.
- TOML-driven detector graph loaded from
  `src/core/seams/detectors/detectors.toml` (shared file,
  embedded via `include_str!` —
  `rust/crates/detectors/src/pipeline.rs:53-55`).
- Comment-masked preprocessing (`comment_masker.rs`).
- Pattern-based walker with hooks (`walker.rs`, `hooks.rs`).
- TS-parity harness: byte-identical output vs the TS
  implementation (`tests/parity.rs`).

Output types (`rust/crates/detectors/src/types.rs`):

- `DetectedEnvDependency` — per-file detected env-variable
  access.
- `DetectedFsMutation` — per-file detected filesystem
  mutation.

**Output is NOT graph nodes and edges.** It is a stream of
typed fact structs. In the TS runtime these are consumed by
`src/adapters/indexer/repo-indexer.ts:1131-1132` and persisted
into dedicated surface-scoped tables:

- `surface_env_dependencies` (migration
  `src/adapters/storage/sqlite/migrations/015-env-dependencies.ts`).
- `surface_fs_mutations` (migration 016).

These tables are scoped to `project_surface_uid`, not to
symbol-level graph nodes. There are NO `READS` or `WRITES`
graph edges produced from detector output in either runtime
today.

**In the Rust workspace, the detectors crate is not invoked
at all** — no imports of `repo_graph_detectors::detect_env_accesses`
or `detect_fs_mutations` exist outside the detectors crate
itself. The Rust indexer does not call detectors. Detector
output is currently reached only through the crate's own
tests and the parity harness.

**Can `detectors` legally serve as a temporary non-AST seam
pass without violating the state-boundary contract?**

Answer: **No, not without a contract amendment.**

Reasons:

1. **Matcher form.** The state-boundary contract §6.1 requires
   Form A: import-anchored call matching. `detectors` does
   pattern-based walking with hooks, not import anchoring. The
   two matcher forms are not interchangeable.

2. **Output granularity.** The contract §4 requires resource
   nodes (DB_RESOURCE, FS_PATH, STATE, BLOB) and edges (READS,
   WRITES) at symbol-to-resource granularity. `detectors`
   output is file-scoped facts (`DetectedFsMutation` is a
   per-occurrence record, not a symbol-level edge).

3. **Persistence path.** Current detector output is routed to
   surface-scoped tables, not to the `nodes` / `edges` tables.
   Routing it to graph edges requires a translator layer that
   does not exist.

4. **Basis tags.** Contract §7 defines the slice-1 basis
   vocabulary as `{stdlib_api, sdk_call}`. Text-pattern
   matching is neither. A `text_pattern` basis would be a
   contract amendment.

A Fork 2 selection therefore requires, at minimum:

- A contract amendment introducing a text-pattern matcher
  (Form A') and a new basis tag (`text_pattern` or similar).
- A translator layer from `DetectedFsMutation` /
  `DetectedEnvDependency` / future state-detection outputs
  into graph nodes + edges, mapping surface-scope to
  symbol-scope (the latter requires selecting an enclosing
  symbol for each detection — not trivial with text-pattern
  matching).
- Either new detector families for DB/cache/blob (not present
  today) or acceptance that Fork 2 slice 1 ships FS coverage
  only across the four non-TS languages and defers DB/cache/
  blob coverage for them.

### 1.5 Orchestrator registration and delta-refresh interaction

`rust/crates/indexer/src/orchestrator.rs` takes a `Vec<Box<dyn
ExtractorPort>>` via its public entry point. Only `TsExtractor`
is registered by `rmap index`/`refresh` callers
(`rust/crates/rgr/src/...` — not re-inspected line-by-line
in SB-0 since it is not decision-relevant; registration is
uniform across commands).

`rust/crates/indexer/src/extractor_port.rs` (the port
definition) is sync, produces `ExtractionResult`, and accepts
per-file source text. State-boundary extraction that piggybacks
on AST extraction must either:

- Extend the existing extractor to emit additional edges (same
  extraction pass).
- Run as a separate pass after extraction, consuming already-
  extracted edges / bindings (post-pass).

The contract §6.1 permits either shape; the shared
`state-bindings` crate model (G1a lock) is compatible with both.

### 1.6 Pre-existing Spring / Lambda detection (context)

`src/adapters/extractors/java/spring-bean-detector.ts`,
`src/adapters/extractors/java/spring-route-extractor.ts`, and
`src/core/classification/framework-entrypoints.ts` exist in TS.
These are NOT ported to the Rust workspace. They do NOT affect
SB-0 directly but their existence is the TS-side baseline for
future Java-extractor work.


## 2. Fork recommendation

### 2.1 Summary recommendation

**Fork 1 — TS-only slice 1.**

Selection basis: the Rust workspace today supports AST-backed
extraction for exactly one language family (TS/JS). Slice 1's
normative contract is Form-A / AST-backed. The only path that
satisfies the contract without amendment AND without a multi-
language extractor port program is TS-only.

Fork 2 is not zero-cost: it requires a contract amendment, a
detector-to-graph translator layer, and the acceptance of
mixed-fidelity substrate (AST for TS, text-pattern for the
rest). It is technically achievable and leverages substantial
existing `detectors` infrastructure, but it changes the shape
of the product: trust/reliability, explain, and dead-code all
become per-basis aware. That is a larger commitment than the
user's goal statement ("earliest solid delivery on real
repos") implies.

Fork 3 is architecturally cleanest but delays slice 1 by the
duration of a four-language extractor port program
(`src/adapters/extractors/{rust,java,python,cpp}` → Rust
workspace). That program is large enough to warrant its own
milestone inventory and is not justified by slice-1 goals
alone.

### 2.2 Trade-off table for Fork 1

| Axis | Fork 1 outcome |
|---|---|
| Languages covered in slice 1 | TypeScript, TSX, JavaScript, JSX only. |
| Corpora validated in slice 1 | Node/AWS, React/Next. Spring and public C++ repos deferred. |
| Matcher fidelity | Uniform AST-backed Form A across covered languages. |
| Contract amendments required | None. |
| Crate additions | `rust/crates/state-bindings`, `rust/crates/state-extractor` (per G1a). |
| Existing-crate modifications | `rust/crates/ts-extractor` gains state-boundary emission. `rust/crates/rgr` CLI widens `--edge-types` to accept READS/WRITES. `rust/crates/indexer` updates dead-code to exclude resource kinds. |
| Mixed-fidelity substrate | No. |
| TS-runtime parity gap | Pre-existing and deferred; no change from slice-1 plan. |
| Java/Spring state-boundary coverage | Deferred. Future slice gated on a Java Rust-workspace extractor (separate milestone). |
| Python state-boundary coverage | Deferred. Same posture. |
| Rust-language state-boundary coverage | Deferred. Same posture. |
| C++ state-boundary coverage | Deferred. Same posture. |
| Use of existing `detectors` infrastructure | None in slice 1. Remains available for the future config/env seam slice and cross-language ports. |
| Risk | Low. Single extraction path, AST-backed, no new basis tags, no translator layer, proven extractor infrastructure. |
| Product lift | TypeScript/JS-stack state-boundary edges on real repos (Node/AWS, React/Next). Enables orient/check/explain improvements on those corpora. |

### 2.3 Why not Fork 2

Fork 2 is genuinely tempting because the `detectors` crate
already does the hard cross-language substrate work. Declined
because:

- **Mixed-fidelity contamination.** TS edges would carry
  `basis = sdk_call` / `stdlib_api`; non-TS edges would carry
  `basis = text_pattern`. Every downstream consumer (trust
  report, dead-code reducer, explain surface, agent orient
  pipeline) would need to reason about the two tiers
  separately. That complexity is larger than it looks on the
  surface.

- **Symbol identity problem.** Detector output is per-file,
  per-occurrence. Turning that into a symbol-to-resource edge
  requires selecting the enclosing symbol for each detection.
  Without AST ownership, the enclosing-symbol resolution is
  imprecise at best. The translator layer becomes a second
  matcher, which is the kind of architectural leak the
  contract's Form-A rule exists to prevent.

- **Coverage asymmetry within Fork 2.** Existing detector
  families are `Env` and `Fs` only. DB / cache / blob
  detection does not exist in `detectors` today. A Fork 2
  slice 1 that claimed five-language coverage would deliver FS
  only for non-TS languages and DB/cache/blob only for TS —
  which is itself a mixed-coverage result, not a uniform
  cross-stack ship.

- **Amendment scope creep.** Introducing a `text_pattern`
  basis opens a category that will grow. Future detector
  families would accumulate under the text-pattern umbrella,
  creating pressure to treat text-pattern-matched edges as
  first-class. Deferring this decision until the AST path is
  validated on real repos is the more reversible choice.

### 2.4 Why not Fork 3

Fork 3 is the architecturally correct long-term answer but is
out of proportion with slice 1's scope:

- Porting four language extractors (Rust, Java, Python, C++)
  from TS to Rust is a program on the order of tens of
  thousands of lines, multiple parity harnesses, and likely
  months of work even under focused effort.
- Slice 1's goal is state-boundary extraction, not extractor-
  adapter breadth. Yoking the two programs together delays
  state-boundary delivery until the port program completes.
- The extractor port program should be its own milestone with
  its own scope, acceptance criteria, and priority discussion
  against other vision items (delta indexing completion,
  daemon, registry/framework liveness).

Fork 3 remains the correct framing for the follow-on program
AFTER Fork 1 slice 1 closes.

### 2.5 Known gap created by Fork 1

Selecting Fork 1 means the user's Spring repo is NOT validated
in slice 1. This is a direct consequence of "only AST-backed
extraction in the Rust workspace today is TS/JS." The Spring
validation becomes a follow-on slice gated on a Java Rust-
workspace extractor.

This is a transparent trade-off, not a hidden cost. It is
recorded explicitly in the Fork 1 trade-off table and the
proposed Phase B sequence below.


## 3. Proposed Phase B slice sequence (Fork 1)

Only applies if the user accepts the Fork 1 recommendation.
Reproduces the milestone doc's Fork-1 Phase B inventory for
traceability; no new content relative to that table.

| Slice | Content |
|---|---|
| SB-1 | `rust/crates/state-bindings` crate: binding table loader, TOML schema validator, stable-key builders, basis tag enum. Unit tests only. No extractor integration. |
| SB-2 | `rust/crates/state-extractor` crate skeleton: emission API, resource-node dedup (in-memory per snapshot), evidence shape. Consumes `state-bindings`. Unit tests with synthetic fixtures. Cross-runtime interop test case added for the three new node kinds (`DB_RESOURCE`, `FS_PATH`, `BLOB`). |
| SB-3 | Integrate `state-extractor` with `rust/crates/ts-extractor`. Ship `fs`, `node:fs`, `fs/promises` (stdlib) and `@aws-sdk/client-s3`, `pg`, `mysql2`, `better-sqlite3`, `sqlite3`, `redis`, `ioredis` (SDK) TS/JS binding entries. Verify `ts-extractor`'s import-binding resolver surfaces the imported symbol path at sufficient fidelity (see SB-0 §1.5 interaction note); if insufficient, an extractor-side prerequisite slice is triggered before SB-3 closes. |
| SB-4 | Validate on the user's Node/AWS repo. Validate on the user's React/Next repo. Spot-check and record closure evidence per milestone "Acceptance criteria". |
| SB-5 | Widen `rmap callers` / `rmap callees` `--edge-types` to accept `READS`, `WRITES`. Exclude resource kinds from `rmap dead`. Update per-command tests. |
| SB-6 | Closure: schema.txt and TECH-DEBT.md already pre-updated at contract-design time; re-verify and amend with any implementation-discovered corrections. Update ROADMAP.md to reflect slice 1 shipped status under Fork 1. Record the Spring / Python / Rust-language / C++ gaps explicitly as follow-on slices (each gated on a Rust-workspace extractor for the respective language). Freeze contract at `state_boundary_version: 1`. |

### 3.1 Follow-on slice posture

After SB-6 closes, the next honest priorities (in recommended
order) are:

1. **Queue / event-boundary slice.** `EMITS`, `CONSUMES`,
   `QUEUE` across TS/JS. Smaller than a new language
   extractor. Extends the same `state-bindings` TOML.
2. **Config / env seam slice.** `CONFIG_KEY` graph emission,
   explicit config→resource wiring edges. May reuse
   `detectors` env-detection infrastructure via a translator,
   since the config seam is explicitly defined as outside the
   state-boundary contract.
3. **RE-typescript-rust-workspace-parity**: not needed since
   TS is already in the Rust workspace.
4. **RE-python, RE-java, RE-rust-lang, RE-cpp** in whatever
   order your real-repo demand prioritizes. Each is a
   separate extractor-adapter milestone. Spring depth
   (`spring-bean-detector` port + expansion) rides on
   RE-java.

These are proposals, not locks. Each becomes its own decision
slice when it opens.


## 4. Closure record

Fork 1 accepted 2026-04-17.

Downstream effects on closure:

- `docs/milestones/rmap-state-boundaries-v1.md` rewritten from
  a fork-conditional plan to the locked Fork-1 sequence. Fork 2
  and Fork 3 moved to an "Alternatives considered and rejected"
  section there.
- Next active slice: SB-1 (`state-bindings` crate).
- No code written during SB-0.
- No contract amendment triggered (Fork 1 requires none).

The language-coverage gaps introduced by Fork 1 (Java / Spring,
Python, Rust-language, C++) are tracked as deferred items in
the main milestone document and as explicit deferrals in
`docs/TECH-DEBT.md`. Each becomes its own slice when a
Rust-workspace extractor for the respective language opens.
