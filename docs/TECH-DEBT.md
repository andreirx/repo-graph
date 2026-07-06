# Technical Debt and Known Limitations

> The first two sections below — **Checkpoint Smoke + Two-Agent Gate** (2026-06-29, newest)
> and **Pre-Merge Hardening + E2E Usefulness Findings** — are the cross-cutting backlog
> (placed first deliberately: these are first-class product/VISION problems, not per-subsystem
> notes). The per-subsystem catalogue follows, starting at *Extraction — TypeScript*.

## Checkpoint Smoke (v0.3.1, nginx) + Two-Agent Usefulness Gate — 2026-06-29

Second run of the End-to-End Usefulness Protocol, AFTER the daemon-robustness arc shipped: a
full repo-wide-surface smoke on the **nginx** source tree (C) + an independent **Codex** reviewer
pass (the protocol's two-agent gate). Evidence: `smoke-runs/2026-06-29T12-42-18Z/` + the Codex
verdict. Headline: the orientation + honesty CORE is strong (`trust` grades reliability and
refuses unsafe call/dead claims; `dead` is honestly disabled; boundaries/resources/complexity
are useful orientation), but several surfaces **render unresolved / unsupported relationship data
as known-zero or exact fact** — a Fact-Certainty violation BOTH agents flagged as a trust-contract
breach, not polish. The honesty lapses (C1-C4) are consolidated into **`HONEST-DEGRADATION-1`** (a
cross-surface honest-degradation contract spec) and PROMOTED ahead of capability (ENRICH-LIFECYCLE-1)
by the gate. None is a regression from v0.3.1 (all pre-existing) — the patch shipped narrowly as
daemon robustness; these are the next track.

- **C1. `stats` renders resolution-derived zeros with NO caveat (P1).** Every package group shows
  `fan_in=0/fan_out=0` and Martin `D/I/A` (`A=1.00`) while `trust` reports import-graph reliability
  LOW + 6 zero-connectivity modules — resolution-failure presented as measured architectural absence.
  Codex: "the clearest VISION violation." Extends #6. → HONEST-DEGRADATION-1.
- **C2. `deps list` labels non-JS repos `ecosystem: npm` (P2).** nginx (C): `ecosystem:"npm", count:0,
  total_external_imports:56` — a wrong-ecosystem label that risks implying "npm graph evaluated and
  empty." Render side of R3 (no C manifest reader). → HONEST-DEGRADATION-1 (label) + a C-manifest
  reader (separate capability slice).
- **C3. `orient` footer `Certainty: exact/fresh` collapses freshness with semantic certainty (P2, NEW).**
  The same answer carries inferred (0.7) modules, LOW call-graph reliability, and LiveGraph-unavailable,
  yet the footer reads as global factual certainty. (Found by the Codex pass — missed in the first
  analyst read.) → HONEST-DEGRADATION-1.
- **C4. Cross-surface symbol-count mismatch (P2, NEW).** Same snapshot: `index` 4393 nodes / `orient`
  "3977 symbols" / `stats` "1816 symbols" (files 396/397/396). All say "symbols" to the reader with no
  explanation of what each counts — a semantic-contract defect (the reader cannot tell which to trust),
  regardless of whether the counts are intentionally different. → HONEST-DEGRADATION-1.
- **C5. `orient` budget tiers are bimodal (P3, NEW — usability).** small=8 / medium=12 / large=390 /
  full=390 lines: a ~32x cliff with no true middle tier. Honesty + wording stay consistent across tiers;
  this is progressive-disclosure usability, not correctness. → separate orient-density slice.
- **C6. `rmap enrich` is not in the loop and not ergonomic (P2).** Index emits "...(13023 unresolved)"
  with NO next-step advice; `rmap enrich` requires `<db_path> <repo_uid>` positionally (does not resolve
  repo from cwd like every other command) and supports `rust|typescript|java` only — no C resolver,
  though clangd is installed on the machine. Extends R4. → ENRICH-LIFECYCLE-1.
- **C7. The smoke harness conflates non-zero exit with failure (P3, NEW — tooling).** It marked nginx
  "failed" because `check` exited 1 (found a real reliability issue) and `dead` exited 2 (intentionally
  disabled) — the same over-counting shape as R1. → smoke-harness fix (distinguish "errored" from
  "reported findings / disabled").

### Resolution (2026-07-01)

The honesty lapses C1-C4 shipped as the ratified **HONEST-DEGRADATION-1** contract (spec `c1a539e`,
two-agent decision-reviewed, D1-D5): **C1 + C4 RESOLVED** by IMPL-1 (`cf73d6e` — stats count reconciled
to one canonical number; degenerate zero-degree → `unknown` / JSON `null`, never known-zero); **C2 + C3
RESOLVED** by IMPL-2 (`c8f1f1c` — deps ecosystem honest, no false `npm`; orient `Serving:` relabel), with
the pure helpers extracted to a module (`3952c49`, un-bloating dispatch.rs). This also RESOLVES the older
stats debts **#5** (false-zero) and **#6** (fan-in/out overclaim) — subsumed by IMPL-1 (D1+D4). **C5
RESOLVED** (`2872c4a` — progressive orient budget ladder, real medium tier). **C7 RESOLVED** (`a2fb02c` —
verdict-exit handling: check/dead/gate non-zero no longer fail the repo). **C6 DEFERRED to
ENRICH-LIFECYCLE-1** (the `rmap enrich` cwd-resolving ergonomics folded into the enrich-path rework to
avoid touching it twice). All items from this pass are dispositioned.

## Pre-Merge Hardening + E2E Usefulness Findings (arch/scip-substrate-pivot)

Items surfaced during the pre-merge hardening pass and the **first run of the
End-to-End Usefulness Protocol** (`docs/testing/end-to-end-usefulness-protocol.md`)
on the `arch/scip-substrate-pivot` branch. Evidence: the smoke captures under
`smoke-runs/2026-06-21T15-55-40Z/`, the `RMAP_PERF` index instrumentation, the
manual nginx capture, and direct source reads (cited per item). Each was
**observed**, not imagined.

These are recorded as **first-class problems** because each one does one of three
things the VISION forbids: it makes the *primary orientation surface* tell an agent
something structurally wrong (#3, #4); it renders an *unknown as a fact*, violating
the Fact-Certainty layer model "outer layers must surface unknowns" (#5, #6); or it
*contradicts the daemon architecture* the VISION commits to (#1, #2). None is a
regression introduced by this branch, so none blocks its merge — they are the
product's standing honesty-and-robustness debt and the natural next track. Tracked
here so they do not drift (CLAUDE.md Hard Constraint #5), following the
*Operational Dependency Seam Slice (deferred items)* precedent below.

Severity matches the rest of this file: **P1** = high blast radius on core value or
core architecture; **P2** = real defect on a secondary surface, or a compounding
robustness gap; **P3** = lead or coverage gap, no specific defect yet proven.

### 1. Daemon is strictly serial — head-of-line blocking (P1)

- **OBSERVED (`rust/crates/daemon-transport/src/socket.rs:257-288`):** `run_socket`
  is a single accept loop that calls `handle_connection` **inline** (line 274) — no
  per-connection `thread::spawn` or async task. The client stream is set back to
  blocking (271); the non-blocking mode exists only so the loop can poll the
  shutdown flag. `handle_connection` (147-186) reads line-delimited requests and
  `parse_and_dispatch`es each **synchronously**. A long request — an index/refresh
  (Linux kernel: tens of minutes) or any heavy query — blocks `listener.accept()`
  for its full duration; every other client waits behind it. The only `thread::spawn`
  on this path is the test harness spawning the whole server.
- **Impact (VISION — Operational Architecture + "Daemon purpose clarified"):** the
  VISION commits to a long-lived daemon that "enables **concurrent queries**" and is
  "the future **multi-agent coordination authority** for shared repo databases, with
  many readers, fewer writers." A serial loop is the direct contradiction: one heavy
  write freezes orientation for every other agent, and the headline promise of
  "orientation in milliseconds" becomes unbounded behind any concurrent heavy
  request. This is the gap between the *shipped* daemon and the *VISION* daemon.
- **Required fix:** concurrent connection handling (thread-per-connection or an async
  runtime) with explicit reader/writer coordination over SQLite + LiveGraph (many
  readers, serialized writers). An architecture change, not a parameter tune —
  comparable in kind to the all-in-memory indexer redesign under *Large-Repo
  Scalability*.

### 2. No cancellation on the query paths (P2) — RESOLVED 2026-06-26

**RESOLVED** by B2 (daemon-concurrency-1 §14 D-K), shipped as DAEMON-CANCEL-1/2/3: every heavy
query path (cycles, path, stats, orient, check, trust, explain) now cooperatively cancels
mid-flight on peer-disconnect (Rust-loop checkpoints + `sqlite3_interrupt` for SQL), reusing
the D5b emitter seam; worker-internal failure stays `WorkerVanished`, not `Cancelled`. K-A
limitation (a connected-but-not-reading peer can block the heartbeat write) documented; K-B
fd-watcher is the named deferred upgrade. Original finding preserved below for the audit trail.

- **OBSERVED:** cancellation today is coupled to **progress emission**. Per *Daemon —
  Progress abort checkpoint granularity* (D5b), a transport-write failure during a
  progress callback aborts index/refresh at the next checkpoint. But a query
  (`orient`/`stats`/`cycles`/`trust`/…) writes only its **final** response
  (`socket.rs:185`) — it emits nothing mid-computation, so a peer disconnect is
  invisible until the work is already done. RMAPD-PERF-2 recorded the same shape
  ("Daemon continued processing after client disconnect"). So a disconnected or
  timed-out client (e.g. the relay's `--timeout`) leaves a heavy query running to
  completion.
- **Impact:** wasted compute, and — combined with #1 — actively harmful: an abandoned
  heavy query keeps the serial loop blocked for its full duration with no consumer.
  Cooperative cancellation is a prerequisite for the daemon being a trustworthy
  shared multi-agent authority.
- **Required fix:** cooperative cancellation checkpoints on the long query paths
  (not just progress-emitting index/refresh), keyed off peer-disconnect detection.
  Couples to #1's concurrency rework. Extends D5b from index/refresh to queries.

### 2b. LiveGraph preload/refresh acquire no coordinator (P2 — latent; active under concurrency)

- **OBSERVED (DAEMON-CONCURRENCY-1 decision review, 2026-06-23):** `handle_livegraph_preload`
  and `handle_livegraph_refresh` mutate the in-memory LiveGraph but acquire **no** repo
  coordinator — they call only `resolve_and_load_repo` then swap the graph
  (`dispatch.rs:764-797`, `:804-880`), unlike the production writer `handle_refresh` which
  takes `acquire_write` + `acquire_refresh` (`dispatch.rs:2044`, `:2047`).
- **Impact:** harmless in today's **serial** daemon (nothing else runs concurrently). But
  under DAEMON-CONCURRENCY-1's concurrent accept loop, these handlers can swap the LiveGraph
  **under live readers** regardless of the refresh-reader policy (W-A or W-B) — a cross-store
  inconsistency. This is why B1 (`DAEMON-CONCURRENCY-IMPL-1`) MUST bring them under the
  coordinator; the same fix is what a background enrich writer (ENRICH-LIFECYCLE-1) reuses.
- **Required fix:** classify `livegraph_refresh`/`preload` (and background enrich) as
  writer-ish under the repo coordinator. Tracked in `daemon-concurrency-1.md` §14 (B1 scope).
  The deeper request-level epoch binding is `DAEMON-W-B-EPOCH-1` (ROADMAP).

### 3. orient under-segments deeply-nested package layouts (P1)

- **OBSERVED (spring-petclinic, `smoke-runs/2026-06-21T15-55-40Z/`):** `orient --full`
  reports `1 module: .` and `Modules (by size): . — 47 files`, while `stats`
  enumerates **11** module-sized units (`…/owner` 12 files, `…/vet` 6, `…/system` 5,
  …). The repo has obvious `owner`/`vet`/`system` package separation; orient
  flattens it to a single root module.
- **Root cause (CONFIRMED against code — `docs/slices/module-model-1.md` §2):** NOT
  the umbrella heuristic — that hypothesis was disproven. spring-petclinic ships a
  declared Gradle root, so inferred detection runs in **gap-fill mode and the
  umbrella heuristic never runs** here. The real cause is the unmigrated **dual-path
  model**: `orient`/`modules` read `module_candidates` (one declared Gradle module →
  "1"); `stats` reads `nodes` kind='MODULE' leaf directories with a `files>0` filter
  (→ "11"). This is the dual-truth ORIENT-BUG-1 diagnosed; ORIENT-BUG-1 migrated
  `trust` onto `module_candidates` but left `stats` on the old `nodes` source. The
  umbrella-descent gap (fixed depth 2, ≥2 children) is REAL but only a **secondary,
  manifest-less variant** (`src/.../{a,b,c}` with no manifest) — it does not explain
  this case.
- **Impact (VISION — Primary Use Case + Discovery clarity):** `orient` is THE primary
  orientation surface, and "what modules exist and what they own" is named as the
  first thing an agent needs. "1 module: ." is not merely thin — it is a
  *structurally wrong* model the agent carries into every later decision. It fails
  the Protocol-Surface Layer-2 test ("can an agent learn the truth from the output
  alone?") in the worst way: it learns something false.
- **Related:** ORIENT-BUG-1 fixed an earlier orient/trust module-**count** mismatch
  by sharing `module_candidates`. This is distinct: under-segmentation of the
  inferred-module model itself on nested layouts.
- **Required fix (per `module-model-1.md`):** name the **package/directory topology**
  (a Layer-0/1 fact the indexer already computes) as `orient`'s structure headline,
  and unify the cross-command "module" notion (#4) so each count is self-labelled —
  finishing ORIENT-BUG-1's migration by moving `stats` off `nodes` kind='MODULE'. The
  umbrella-chain descent is a **secondary** fix for the manifest-less variant only.
  Six decisions (D1–D6) are surfaced in the spec for ratification before the IMPL.

### 4. "module" denotes different things across orient/modules vs stats (P2)

- **OBSERVED:** same run — `orient`/`modules list` count **inferred/declared
  modules** (1); `stats` counts **directory-prefix groups** (11). The word "module"
  names two different things across the command surface, with no marker telling the
  agent which notion it is reading.
- **Impact (VISION — Protocol Surface Standard):** the surface must be a coherent
  machine protocol. Two commands answering "modules?" with 1 vs 11, unlabelled,
  forces the consumer to reverse-engineer which notion each means. At minimum each
  count must be self-labelling (inferred-module vs directory-group); ideally they
  share one model.
- **Required fix (per `module-model-1.md` D1):** keep **two self-labelled notions** —
  *package/directory groups* (Layer-0/1 topology, what `stats` and `orient`'s
  structure line show) vs *declared/inferred modules* (Layer-1/2 `module_candidates`,
  what `modules`/`trust` show); no command emits a bare "modules: N" that means
  directory groups. (Collapsing to one notion was considered and rejected — see D1.)
  Pairs with #3 as one slice.

### 5. stats reports `total_symbols: 0` on rmap-indexed repos — false zero (P2)

- **OBSERVED (spring-petclinic):** `stats` prints `total_symbols: 0` and `symbols=0`
  per directory, while `orient` on the same snapshot reports **290 symbols**. The
  symbols demonstrably exist; stats shows zero.
- **Root cause (HYPOTHESIS — needs confirmation):** likely the Rust-path
  module-metadata gap under *Module Resolution Dual-Path Model* — the `rmap` (Rust)
  indexer does not populate the per-module symbol metadata the stats path reads, so
  it defaults to `0` rather than "not populated."
- **Impact (VISION — Layer model / Fact-Certainty):** a **false zero rendered as a
  fact** is worse than a missing value. "Outer layers must surface unknowns";
  presenting an unpopulated metric as `0` is exactly the overclaim the layer model
  forbids — an agent can read "total_symbols: 0" as "empty/dead area" and act on it.
- **Required fix:** populate the metric on the Rust path, or render "not measured"
  (never `0`) when the source table is unpopulated. Confirm the source first.

### 6. stats fan-in / fan-out / distance overclaim under low import resolution (P2)

- **OBSERVED:** `stats` reports Martin-style fan-in/fan-out and "distance from main
  sequence" as bare numbers. On C/C++ indexed syntax-only (no `compile_commands.json`;
  `#include` largely unresolved — see *Extraction — C/C++ → Shared limitations*), the
  underlying import graph is highly incomplete, so these ratios are computed on a
  fraction of the real edges yet presented with no reliability marker.
- **Impact (VISION — "outer layers must surface unknowns"):** the layer model's own
  worked example is this: "raw counts without coverage or confidence markers are
  overclaims." Post ORIENT-DENSITY-IMPL-1, `orient` surfaces an import-graph
  reliability caveat in its Reliability/Degradation section; `stats` does not — so
  the same incomplete graph is honest in one command and overclaimed in another.
- **Required fix:** attach an import-resolution reliability marker to stats'
  dependency-derived metrics (mirror orient's reliability line), or suppress
  distance/instability when resolution is below the reliability threshold.

### 7. REG-1 protocol-surface drift: stale `--help` for governance/write commands (P2)

- **OBSERVED:** `declare`/`policy`/`boundaries`/`contracts` still take the pre-REG-1
  positional `<db_path> <repo_uid>` form (write/governance migration was
  intentionally deferred — ROADMAP → REG-1 "Explicitly deferred"), but the top-level
  `rmap --help` does not reflect the **mixed** contract: it is stale for `declare`,
  so an agent reading help constructs the wrong invocation.
- **Impact (VISION — Protocol Surface Standard, Layers 1 + 3):** repo-graph "is a
  machine-readable engineering protocol for agents"; `--help` is part of that
  protocol. Help that shows the wrong invocation form actively misdirects — worse
  than silence. The deferred *migration* is already tracked (ROADMAP REG-1); the
  **help/protocol drift** is the new, separately-actionable finding (Hard
  Constraint #5: silent drift).
- **Required fix:** make `--help` tell the truth about the mixed contract — mark the
  still-positional governance/write commands explicitly — independent of when the
  migration itself lands.

### 8. Postpass ≈ 50% of full-index time (P3)

- **OBSERVED (`RMAP_PERF` instrumentation, validation sweep):** the postpass phase is
  roughly half of total index wall-clock across the smoke repos. This instrumentation
  is itself the closure of RMAPD-PERF-1's remaining-debt item *"Indexing phase timing
  not instrumented"*; the ≈50% share is its first result.
- **Impact (VISION — fast orientation / cheap incremental maintenance):** a lead, not
  a defect. Complements the refresh-side note already recorded ("Postpasses are
  conservative: run on all files … erodes refresh win"); the new datum is the ~50%
  share on a *full* index.
- **Required fix (lead):** profile which postpasses dominate and whether they can be
  scoped, batched, or folded into extraction. Set a quantified target before any
  rework.

### 9. Peripheral command output-words not yet truth-audited (P3)

- **OBSERVED:** OUTPUT-DOC-TRUTH-AUDIT-1 audited the high-traffic surfaces (`orient` /
  `trust` / `check` / `modules` / `explain`) and corrected real overclaims (e.g.
  `dead` → `unref?`). The long tail (`surfaces` / `boundaries` / `contracts` /
  `policy` / `resource` / `inferences` / `deps` / `coverage` / `metrics` / `churn` /
  `risk` / `assess` detail wording) was **not** audited for output-word truth.
- **Impact (VISION — Protocol Surface Standard, Layer 2):** the output contract must
  be honest across the *whole* surface; an un-audited peripheral command can carry
  the same class of overclaim `dead` did and misdirect an agent. This is a **coverage
  gap**, not a proven defect.
- **Required fix:** extend the OUTPUT-DOC-TRUTH-AUDIT rubric across the remaining
  command surface; relabel any metric whose words claim more than its layer supports.

---

## Resolution, Attribution & Honest Reliability (the "unresolved" signal)

Surfaced 2026-06-23 (roadmap-planning conversation): on a clean-compiling repo, `orient`
reports "call-graph reliability LOW (22%) / 427 unresolved imports" — a signal that describes
repo-graph's own resolution pipeline, not the reader's code, and conflates expected external-
dependency usage with extraction failure. Context store for the ROADMAP "Resolution &
attribution" track. Verified against code (cites).

### R1. Reliability rate counts out-of-scope calls as failures (P1)

- **OBSERVED:** `call_resolution_rate = resolved_calls / (resolved_calls + unresolved_calls)`
  (`trust/src/rules.rs:215`; `agent/src/aggregators/trust.rs:46`); the external/internal
  **classification is an explicitly orthogonal axis** (`classification/src/types.rs:315`) that
  does NOT factor into the rate. So calls into `serde`/`std`/framework APIs — out of source
  scope, unresolvable by design — are counted in `unresolved_calls` and depress the rate.
  Thresholds: 50%/85% bands (`trust/rules.rs:220-225`); agent LOW at 0.20 (`trust.rs:26`).
- **Impact (VISION — labels speak the reader's language; outer layers surface unknowns):** a
  clean repo that simply uses libraries reads as "LOW reliability." The signal grades
  repo-graph, not the reader's code.
- **Required fix:** compute the in-scope rate over in-scope references only; present the
  external share as a reader-context coverage map (named libraries). ROADMAP "reframe
  reliability."

### R2. The unresolved set is categorized but not attributed (P1)

- **OBSERVED:** `UnresolvedEdgeClassification` = `ExternalLibraryCandidate / InternalCandidate /
  FrameworkBoundaryCandidate / Unknown` (`classification/src/types.rs:320`) — four coarse
  buckets, no named dependency, no distinct stdlib/system bucket. The distinguishing info
  exists in the basis codes (`SpecifierMatchesPackageDependency`, `SpecifierMatchesRuntimeModule`,
  … `types.rs:343-372`) but is collapsed into the bucket and not surfaced.
- **Impact:** the best an agent sees is "external_library_candidate" (internal-context), never
  "library call → serde" (reader-context).
- **Required fix:** surface reader-context labels + named attribution (lib/stdlib/system/
  dynamic), using the basis codes + manifest deps. ROADMAP "attribute the unresolved set."

### R3. Manifest/dependency readers missing on the Rust path (P2 — prereq for R2 on Java)

- **OBSERVED:** `SpecifierMatchesPackageDependency` matches against package.json (TS). On the
  `rmap` path, Java/Gradle has no dependency reader (see "No Gradle reader on the Rust side"
  below) → the rule can't fire → imports fall to `Unknown` (spring-petclinic: 1327 unknown vs
  54 external_library_candidate).
- **Required fix:** a Rust-path Gradle (build.gradle/settings.gradle) dependency reader — the
  prerequisite for Java attribution. (Rust/TS/Python attribution is unblocked.)

### R4. Enrichment is opt-in and not in the pipeline (P1)

- **OBSERVED:** the LSP enrichment pass is a standalone command (`rgr/src/commands/enrich.rs`);
  `index`/`refresh` never invoke it. So the pass that resolves `obj.method()` (the in-scope
  gap) does not run unless invoked → "enrichment phase did not run" → the agent must babysit
  the tool.
- **Impact (VISION — installable infrastructure; daemon runtime; orientation not babysitting):**
  repo-graph offloads its own job onto the consumer.
- **Required fix:** run enrichment automatically as a daemon background task after
  index/refresh, with atomic snapshot hand-off, toolchain-aware. NOT blocked on
  DAEMON-CONCURRENCY-1 (different capability). ROADMAP "run enrichment automatically."

## Extraction — TypeScript

- Call graph resolution: 33% on self-index with import-binding-assisted resolution
  (up from ~15% syntax-only). Strong on class-heavy architectures, weak on
  SDK-heavy/functional patterns. Compiler enrichment (`rgr enrich`) resolves
  ~81% of remaining unknown receiver types via TypeChecker.
- **Imported free-function call resolution:** bare-identifier calls that match an
  import binding are resolved using the binding's source module. Disambiguates
  when the same function name exists in multiple files.
- **Aliased named imports now resolved:** `import { foo as bar }` — the binding
  stores both local name (`bar`) and original name (`foo`) in `imported_name`.
  The resolver uses `imported_name` for symbol lookup in the target module.
  Fixed in TS-IMPORT-RESOLUTION-1 Phase 1 (2026-05-23).
- **Namespace imports now resolved:** `import * as ns from "./m"; ns.foo()` —
  the resolver extracts the member name (`foo`) and looks it up in the target
  module. Requires explicit import-kind modeling (`ImportKind` enum) to
  distinguish from default imports. Fixed in TS-IMPORT-RESOLUTION-1 Phase 2-3
  (2026-05-23).
- **Default import member access not resolved:** `import fs from "fs"; fs.readFile()`
  — requires modeling default export structure. Conservative: no resolution,
  no false positives. Out of scope.
- Inherited method resolution: 11 remaining cases on FRAKTAG (diminishing returns).
- External SDK types (node_modules): not indexed.
- Destructured bindings, reassignment: not tracked.

## Extraction — Rust

### Rust TS-side extractor (used by `rgr` TypeScript CLI)
- Rust extractor indexes: structs, enums, traits, impl methods, functions,
  constants, statics, type aliases. `use` declarations produce IMPORTS edges +
  import bindings. Method/function calls produce CALLS edges.
- `#[cfg(...)]` conditional duplicates deduplicated (first emission wins).
- Compiler enrichment via rust-analyzer LSP resolves ~85% of unknown receiver types.
- **Crate-internal module heuristic is not infallible:** `use renderer::Camera`
  classified as `internal_candidate` via `RUST_CRATE_INTERNAL_MODULE_HEURISTIC`.
  A mistyped or undeclared external crate with a lowercase name would be
  misclassified as internal. This is a documented limitation of the heuristic,
  not a defect in the model.
- **No Rust framework detectors yet:** Actix-web, Axum, Rocket, Warp routes
  are unmodeled. Same gap as pre-Express TS had.

### Rust-side extractor (used by `rmap` Rust CLI) — Slice A
- **New in Slice A:** Native tree-sitter-rust extractor in
  `rust/crates/rust-extractor/`. Uses tree-sitter 0.24 with tree-sitter-rust
  0.23 grammar. Same extraction scope as TS-side extractor.
- 29 unit tests covering: FILE nodes, SYMBOL extraction (functions, structs,
  enums, traits, impl methods, consts, statics, type aliases), IMPORTS edges,
  CALLS edges, IMPLEMENTS edges, visibility handling, doc comment extraction,
  `#[cfg(...)]` deduplication.
- **Wildcard imports not tracked:** `use foo::*` produces no ImportBinding
  record. This is intentional — wildcard imports bring in an indeterminate
  set of symbols at parse time. For dependency analysis, use Cargo.toml deps.
- **Cargo dependency resolution:** Nearest-ancestor Cargo.toml lookup with
  hyphen-to-underscore normalization. Handles [dependencies], [dev-dependencies],
  [build-dependencies], and [dependencies.name] sub-table syntax. Does NOT
  handle target-specific deps (`[target.'cfg(...)'.dependencies]`), renamed
  deps, or workspace deps — these are edge cases for import classification.
- **Language-isolated dependency dispatch (compose.rs):** `prepare_repo_inputs`
  uses an explicit 3-arm match on detected language to prevent cross-language
  signal contamination in mixed-language repos. Rust files receive only
  Cargo.toml deps; TS/JS/TSX/JSX files receive only package.json + tsconfig;
  all other languages (Java, C, C++, unknown) receive empty signals until
  dedicated manifest readers exist for those languages. This is intentional
  isolation, not a defect. The behavior is pinned by the
  `mixed_lang_language_isolation` integration test
  (`rust/crates/repo-index/tests/integration.rs`).
- **No Gradle reader on the Rust side:** Java files indexed by `rmap` receive
  `package_dependencies_json = null` (no build dependency signals). The
  TS-side extractor path (`rgr`) has a working Gradle reader. On the Rust
  path, Java import classification uses only the symbols visible to tree-sitter
  with no external package context. A Rust Gradle reader (build.gradle /
  build.gradle.kts parser) is required to close this gap. Until it lands,
  Java CALLS edges to external library types on Rust-indexed repos will not
  classify against Gradle-declared coordinates.

## Extraction — Java

- Java extractor (tree-sitter-java) indexes: classes, interfaces, enums,
  methods, constructors, fields, annotation types. Overloads disambiguated
  by parameter type signatures in stable keys.
- Gradle dependency reader parses build.gradle / build.gradle.kts.
  **TS path only** (`rgr`). The Rust path (`rmap`) has no Gradle reader;
  see "No Gradle reader on the Rust side" under Extraction — Rust above.
- **Gradle-to-Java-package namespace gap:** Maven group IDs (e.g.
  `org.springframework.boot`) do not directly correspond to Java package
  paths (e.g. `org.springframework.web.bind.annotation`). The classifier
  uses a 2-segment prefix heuristic (group `org.springframework.boot` →
  also matches `org.springframework.*` imports) which catches most
  transitive framework packages but can over-classify unrelated packages
  under the same vendor root. This is a fundamental limitation of
  matching build coordinates against source imports without JAR manifest
  or transitive dependency resolution. Documented as approximate.
- **Spring framework-liveness classifier (shipped, both TS and Rust):**
  `@Component`, `@Service`, `@Repository`, `@Configuration`, `@RestController`,
  `@Controller` (class-level), `@Bean` (method-level) detected. Emitted as
  `spring_container_managed` inferences. Suppresses dead-code false positives
  for container-managed symbols.
  **Rust implementation (post-extraction classifier):** reads `metadata_json.annotations`
  from Java extractor output (`repo-graph-classification::spring_liveness`),
  runs during index/refresh (`repo-graph-repo-index::compose`), persists to
  `inferences` table. Pure classifier — no storage dependency.
  **TS implementation (detector hook):** regex line scanning with comment-line filtering.
  Known gaps (both implementations):
  - **Direct annotation match only.** Meta-annotations not expanded (e.g.,
    `@SpringBootApplication` ≠ `@Configuration` without transitive resolution).
  - Methods inside container-managed classes still show dead (handler methods have
    no Java-caller inbound edges; Spring dispatcher invokes them at runtime).
  - Plain classes instantiated only by `@Bean` factories remain dead until the `new`
    call is resolved by enrichment or bean-return-type analysis.
  - `@Autowired` / constructor injection not modeled (DI edges, not liveness).
  - Custom stereotype annotations not detected.
  - JAX-RS, servlet/container entrypoints not yet modeled.
- **Java semantic enrichment operational but fragile:** jdtls (Eclipse JDT
  Language Server) adapter exists at `src/adapters/enrichment/java-receiver-resolver.ts`
  and has produced results on spring-petclinic and glamCRM. However, reliability is
  below TS (~81%) and Rust (~85%) enrichment. Known issues:
  - Cold-start penalty: jdtls Gradle import is slow (minutes on large projects).
    The current `rgr enrich` model (start process → query → stop) amplifies this.
  - Workspace caching helps on repeated runs but does not eliminate first-run cost.
  - Server readiness detection (language/status ServiceReady) is correct but
    does not guarantee all project symbols are indexed.
  Viable improvements: pre-warmed jdtls daemon, javac-based type resolution
  for simpler cases, or persistent background server.

## Extraction — Python

- Python extractor (tree-sitter-python): functions, classes, methods, constructors,
  variables, imports, calls, complexity metrics. Syntax-only.
- **Import resolution (Rust `rmap` path):** relative and absolute local imports
  resolve correctly. `from .service import X` and `from src.service import X`
  both resolve to the target `.py` file. Stdlib imports (json, typing, os)
  remain unresolved (correct behavior — no local file exists).
  - Relative imports emit repo-scoped stable keys at extraction time.
  - `__init__.py` package resolution: `from .pkg import X` resolves to
    `pkg/__init__.py` when the package exists.
  - TS-side extractor does NOT have this resolution; it uses the legacy
    dotted specifier form.
- Dependency reader: pyproject.toml + requirements.txt. PEP 508 parsing.
- **Package-name-to-import-specifier gap:** `pyyaml` → `import yaml`,
  `beautifulsoup4` → `import bs4`. Exact name matches work; mismatches
  remain unclassified. Curated alias map not yet implemented.
- Pytest detector: test_* functions, Test* classes, @pytest.fixture.
  Non-decorated conftest.py functions not detected.
- **Shadowed definitions:** Two-pass extraction emits only the last
  same-name `def`/`class` at each scope level (module root, class body).
  Earlier shadowed definitions are silently suppressed. No diagnostic
  channel exists to report them. Future: emit extractor diagnostics
  for shadowed definitions so downstream tools can flag dead redefinitions.
- **No Python semantic enrichment** (pyright/mypy) yet.
- **No Python framework detectors** (Django, Flask, FastAPI) yet.
- **PY-EXT-2 module organization:** Variables, constructors, and metrics logic
  are inline in `extractor.rs` rather than separate modules (`variables.rs`,
  `constructors.rs`, `metrics.rs`) as proposed in slice doc. Refactor deferred.
- **PY-EXT-2 performance validation:** Tracked as follow-on slice `PY-EXT-2-PERF`
  in ROADMAP.md. No baseline exists from pre-PY-EXT-2 commit. Original benchmark
  command was invalid (library crate, no binary). Requires benchmark harness
  infrastructure before execution.

## Extraction — C/C++

### TS-side C/C++ extractor (`rgr`)

- C/C++ extractor (tree-sitter-c + tree-sitter-cpp): functions, structs, classes,
  typedefs, enums, namespaces, methods, constructors, #include, CALLS, complexity.
- Handles both `.c/.h` (C grammar) and `.cpp/.hpp/.cc/.cxx` (C++ grammar).

### Rust-side C extractor (`rmap`)

- Native tree-sitter-c extractor for `.c/.h` files only.
- Functions, structs, enums, typedefs, #include, CALLS, complexity.
- Validated on swupdate, buildroot, Linux kernel.

### Rust-side C++ extractor (`rmap`)

- Native tree-sitter-cpp extractor for `.cpp/.hpp/.cc/.cxx/.hxx` files.
- Namespaces, classes, structs, enums, methods, constructors, destructors.
- Namespace-qualified names (e.g., `ui::Widget::display`).
- IMPLEMENTS edges from base class clauses.
- `extern "C"` linkage detection: symbol-level metadata and file-level C ABI statistics.
- Design doc: `docs/milestones/cpp-extractor-v1.md`
- Validated on C++11 Deep Dives repo (165 files, 1106 nodes).

### Shared limitations (both runtimes)

- **Syntax-only, no compile_commands.json integration.** Header search paths
  are not resolved — `#include "util.h"` does not resolve to a FILE node
  unless the path matches the repo-relative filename exactly.
- **No macro expansion or preprocessor evaluation.** Code inside `#ifdef` blocks
  is extracted from all branches (first-wins dedup prevents collisions).
- **No template instantiation tracking.** Templates are parsed syntactically but
  instantiation-specific edges are not emitted.
- **No overload resolution.** Multiple same-named functions produce ambiguous
  CALLS edges, same as other languages.
- **STL calls:** Qualified calls (std::sort, std::find) are extracted and
  recognized as stdlib. Receiver-method calls (v.push_back) are extracted
  with raw receiver text — no type resolution.
- **Large-file guard:** Files > 1MB are skipped. This is operational containment
  for generated register headers (Linux AMD GPU headers: 200k+ lines). Not a
  semantic correctness feature.
- **Large-repo streaming/batched pipeline:**
  - Bulk `.all()` eliminated: `queryResolverNodes`, `queryStagedEdges`, `queryAllNodes`
    no longer called on the indexing hot path.
  - Resolver index built from row-at-a-time DB iterator.
  - Staged edges resolved in cursor-based batches (default 10K, configurable).
  - Detector/boundary passes use per-file `querySymbolsByFile`.
  - Dead Phase 1 in-memory maps removed (resolverByStableKey, resolverByName,
    resolverNodeToFile, nodeUidToFileUid).
  - Classification loads file signals per-batch from DB (migration 010:
    packageDependenciesJson + tsconfigAliasesJson added to file_signals).
    Same-file symbol sets rebuilt from persisted nodes via querySymbolsByFile.
    No snapshot-wide fileSignalsCache on the classification path.
  - `fileSignalsCache` retained only for Lambda entrypoint detection — stores
    import bindings only (empty stubs for other fields). TS/JS files only.
  - Multi-batch seam tests: edgeBatchSize=1 and edgeBatchSize=3 verified to
    produce identical results to default batch size.
  - **Linux-scale status:** not yet validated. Previous run hit V8 heap OOM at
    3.6 GB during bulk `.all()` materialization, which is now eliminated. Rerun
    required to discover the next blocker, if any.
- **Delta indexing (slice 1):**
  - Invalidation planner, copy-forward storage, durable extraction edges shipped.
  - `refreshRepo` uses delta path: scan → hash → plan → copy forward → extract
    only invalidated files → resolve all edges → postpasses → finalize.
  - Trust metadata persisted in extraction diagnostics (`delta` block with
    per-category file counts and per-artifact-kind copy counts).
  - ~~**Config-file tracking gap:**~~ **FIXED (2026-05-08):** Config files (package.json,
    tsconfig.json, Cargo.toml, etc.) now participate in refresh invalidation planning.
    Architecture: `routing::is_config_file()` identifies configs; scanner includes them;
    `ConfigFileState` passed to `refresh_repo()` for invalidation planning; config file
    changes trigger scope-widening of unchanged source files. Config files are NOT extracted
    (no FILE nodes). **files_total caveat:** config files are NOT counted in `IndexResult.files_total`
    but ARE counted in snapshot table `files_total` (computed from `COUNT(*) FROM file_versions`).
  - **Postpasses are conservative:** run on all files, not just invalidated scope.
    Erodes refresh win for detector-heavy repos.
  - ~~**File-local fact reuse not implemented:**~~ **FIXED (2026-05-08):** Measurements,
    inferences, boundary_surfaces now copied forward for unchanged files.
    See `refresh_copy_forward_impl.rs`. Remaining gap: `project_surfaces` family blocked
    on module_candidates (TS-only table). Note: contract schemas are re-indexed, not
    copied forward (see below).
  - **Large-repo delta refresh not validated:** verified on repo-graph (235 files),
    not on Linux or other large repos.
  - **Contract schemas are RE-INDEXED during refresh (not copied forward):** By design, contract
    files (`.proto`) are always re-indexed during refresh. The orchestrator's `refresh_repo`
    explicitly runs `proto_indexer::index_proto_files()` for all contract files. The
    `copy_forward_contract_schemas()` function exists but is not used because proto files
    are not included in the `unchanged_files` list (they're handled separately from source
    files). This is intentional — ensures proto schema facts are always fresh.
  - **Contract schema UID fix (2026-05-08):** `proto_indexer.rs` now uses fresh UUIDs per
    snapshot instead of deterministic UIDs based on content hash. The old approach caused
    `INSERT OR IGNORE` to silently drop inserts during refresh because a row with the same
    `schema_uid` already existed from the parent snapshot. Fixed by switching to
    `Uuid::new_v4().to_string()` per schema.
- ~~**Timestamps in compose-layer file versions:**~~ **FIXED (2026-05-08):** `persist_read_failures()`
    and `persist_config_file_versions()` now use `chrono::Utc::now()` for real timestamps.
- **No clangd/libclang enrichment** for receiver-type resolution yet.

## Enrichment Subsystem — Rust

**Status:** Slice 1 + Slice 2 complete.

Rust-native enrichment subsystem for compiler-assisted receiver type resolution.
Replaces TS `rgr enrich` subprocess model with native Rust code in `rmap`.

### Crate Architecture

| Crate | Role | Volatility |
|-------|------|------------|
| `enrichment` | DTOs, pipeline, resolver trait, promotion, reporting | Stable core |
| `lsp-subprocess` | Shared LSP transport (Content-Length, reader thread, timeout) | Outer support |
| `rust-analyzer-resolver` | rust-analyzer LSP subprocess adapter | Volatile mechanism |
| `tsserver-resolver` | tsserver subprocess adapter for TS/JS | Volatile mechanism |
| `jdtls-resolver` | Eclipse JDT Language Server adapter for Java | Volatile mechanism |

The separation enforces the dependency rule: stable enrichment core does not
import volatile LSP/subprocess machinery. LSP-speaking resolvers share transport
via `lsp-subprocess` to avoid duplicating timeout/framing logic.

### `enrichment` Modules

- `contracts.rs`: DTOs (EligibleEdge, ReceiverTypeResult, PromotionCandidate, PromotedEdge)
- `status.rs`: EnrichmentState enum, EnrichmentReport, ReportBuilder
- `eligibility.rs`: EnrichmentStoragePort trait (storage port pattern)
- `promotion.rs`: 8-gate safety filter for promotion
- `resolver.rs`: ReceiverTypeResolver trait for language resolvers
- `pipeline.rs`: Orchestration of eligibility → resolution → persistence → promotion

### `lsp-subprocess` Modules

- `lib.rs`: Content-Length framing, reader thread, timeout enforcement, ID correlation

### `rust-analyzer-resolver` Modules

- `transport.rs`: Re-exports from lsp-subprocess
- `cargo.rs`: Cargo.toml discovery, file grouping per Cargo context
- `types.rs`: Type extraction from hover markdown, validation, external detection
- `client.rs`: RustAnalyzerResolver implementing ReceiverTypeResolver

### `tsserver-resolver` Modules

- `transport.rs`: Newline-delimited JSON framing, seq number correlation, reader thread
- `protocol.rs`: TSServer request/response DTOs, QuickInfo body parsing
- `project.rs`: tsconfig.json/jsconfig.json/package.json discovery, file grouping
- `client.rs`: TsServerResolver implementing ReceiverTypeResolver

### `jdtls-resolver` Modules

- `project.rs`: Maven/Gradle/Eclipse detection, module root + workspace launch root distinction
- `client.rs`: JdtlsResolver implementing ReceiverTypeResolver (uses lsp-subprocess)

### 8-Gate Promotion Filter

| Gate | Rule |
|------|------|
| 1 | Category is CallsObjMethodNeedsTypeInfo or CallsThisWildcardMethodNeedsTypeInfo |
| 2 | Config opt-in (placeholder) |
| 3 | Enrichment succeeded (origin != Failed) |
| 4 | Type is internal (is_external_type = false) |
| 5 | Type maps to exactly one CLASS in graph |
| 6 | Method maps to exactly one METHOD on class (rejects overloads) |
| 7 | No union/intersection types |
| 8 | Simple receiver.method or this.field.method shape (rejects optional chaining, element access) |

### Remaining work

Enrichment pipeline is complete for Rust, TypeScript, and Java.

Future work:
- Python resolver (pylsp or pyright)
- C++ resolver (clangd)
- Configuration file for jdtls_path and other resolver settings

### Tests

- `enrichment`: 32 unit tests (gates, DTOs, regression tests)
- `lsp-subprocess`: 8 unit tests (transport, timeout, correlation)
- `rust-analyzer-resolver`: 13 unit tests (cargo grouping, type extraction)
- `tsserver-resolver`: 32 unit tests (transport, project grouping, protocol, type extraction)
- `jdtls-resolver`: 16 unit tests (project grouping, type extraction, build system detection)

### Provisional Heuristics

The following are bootstrap heuristics, not guaranteed truth:

**Rust (rust-analyzer):**
- Type extraction from hover markdown (pattern-based, may miss edge cases)
- PascalCase validation for type names (Rust convention, not enforced by compiler)
- std-only external type detection (future: check against indexed crate deps)
- **Array type syntax (`[T]`) not handled:** hover returns `[String]` or `[&str]` for slice
  types; these fail is_valid_rust_type_name validation. Needs regex refinement.
- **String literals in test fixtures:** hover over string literals like `"John Doe"` returns
  the literal text, which fails type validation. Not a defect — these are not type positions.

**TypeScript (tsserver):**
- Type extraction from QuickInfo displayString (regex-based parsing of "(kind) name: Type")
- Skips union/intersection types, anonymous object types, function types
- Array types (T[]) normalized to "Array"
- Generic types (Promise<T>) extract base type only
- External type detection via static list (Node.js, RxJS, Express, React, Angular)
- Project detection via tsconfig.json > jsconfig.json > package.json fallback

**Java (jdtls):**
- Type extraction from hover text (Java "Type varName" pattern parsing)
- Strips generic parameters (List<String> -> List)
- Strips package prefix (java.util.List -> List)
- Skips Java primitives (int, long, void, etc.) and Object
- External type detection via static list (Java stdlib, Spring, JPA, Servlet)
- Project detection: Maven (pom.xml) > Gradle (build.gradle*) > Eclipse (.project)
- Gradle workspace promotion: module root may differ from workspace launch root
- Timeout defaults are provisional (120s init, 45 warmup retries at 3s)
- Requires explicit jdtls_path configuration (no auto-discovery)

### Known Limitations

**TypeScript resolver:**
- `calls_this_wildcard_method_needs_type_info` category now supported via tree-sitter.
  The `receiver_locator` module extracts the receiver expression (e.g., `this.field`
  from `this.field.method()`) using tree-sitter syntax parsing, then queries tsserver
  for the type at that position. Explicit failure reasons for unsupported patterns:
  `receiver_locator_no_receiver`, `receiver_locator_unsupported:<reason>`,
  `receiver_locator_parse_error:<reason>`.
- Primary category `calls_obj_method_needs_type_info` (e.g., `obj.method()`) works
  correctly because cursor position is at the receiver variable itself.
- **Multi-tsconfig repos:** The `ownership` module in `tsserver-resolver` now determines
  file→tsconfig ownership by evaluating actual config semantics (`files`, `include`,
  `exclude`, `extends`, `references`). This replaces the naive "nearest config by
  directory" heuristic. Files with no owning config fail explicitly as
  `ts_project_ownership_not_found`. Files claimed by multiple configs fail as
  `ts_project_ownership_ambiguous`. Remaining limitation: glob matching is simplified
  (uses Rust patterns, not full TypeScript semantics) and `extends` from `node_modules`
  packages is not fully supported.

**Java resolver:**
- Same `calls_this_wildcard_method_needs_type_info` limitation as TypeScript.
- Build system maturity: Maven fully supported, Gradle operational, Eclipse minimal.
- Requires explicit jdtls path via `--jdtls-path <path>` flag or `JDTLS_PATH` env var.
  If `--language java` is specified without a jdtls path, CLI exits with error.
  If Java is not explicitly requested and no jdtls path is configured, Java resolver
  is silently skipped (Rust/TS still run).

### Timeout Enforcement

All LSP-based resolvers use a dedicated reader thread (from `lsp-subprocess`)
with channel timeout (`recv_timeout`) for true timeout enforcement on blocking I/O.

**rust-analyzer:** `init_timeout_secs` (120s), `hover_timeout_secs` (30s), `warmup_retries` (60),
  `warmup_delay_ms` (3000). Warm-up strategy: require actual type extraction OR 3+ consecutive
  null responses after 15s minimum delay. This distinguishes "still loading" from "loaded,
  position genuinely has no hover".
**tsserver:** `quickinfo_timeout_secs` (15s), `warmup_retries` (20), `warmup_delay_ms` (1500).
**jdtls:** `init_timeout_secs` (120s), `hover_timeout_secs` (15s), `warmup_retries` (45), `warmup_delay_ms` (3000).

Java defaults are more generous due to JVM startup + dependency resolution overhead.
All values are configurable and documented as provisional.

## Policy Facts — PF-1 STATUS_MAPPING

- **PF-1 temporary re-parse postpass (TECH DEBT):** STATUS_MAPPING extraction
  from C files is implemented as a postpass in `repo-graph-repo-index::compose.rs`
  that re-parses C files after the main extraction completes. This duplicates
  the tree-sitter parsing work already done by the C extractor.
  - **Why temporary:** The target architecture is extraction-time integration
    where the C extractor carries policy-fact output directly, eliminating the
    duplicate parse.
  - **Why accepted now:** PF-1 must populate automatically during `rmap index`
    so that `rmap policy` queries current-state discovery facts without a second
    manual pipeline. Re-parsing is acceptable as temporary debt; hidden manual
    extraction is not.
  - **Migration path:** When the Rust C extractor (`repo-graph-c-extractor`)
    is extended to output policy facts alongside nodes/edges, the postpass can
    be removed and STATUS_MAPPING extraction happens at extraction time.
  - **Files affected:**
    - `rust/crates/repo-index/src/compose.rs`: `persist_policy_facts()` postpass
    - `rust/crates/repo-index/Cargo.toml`: tree-sitter deps for re-parse
  - **Tracking:** This entry. Remove when migration to extraction-time integration
    is complete.
- **Switch discriminant limitation:** PF-1 detects switches on any qualifying
  parameter, including pointer dereference (`*param`). Nested switches, multiple
  switches in the same function, and complex discriminant expressions (e.g.,
  `param->field`) are not handled.
- **No cross-language coverage:** STATUS_MAPPING extraction is C-only in PF-1.
  Similar patterns exist in Rust (`From`/`Into` impls with match) and Java
  (enum switch methods) but are not extracted.

## Policy Facts — PF-3 RETURN_FATE

- **Method chain dedup:** C++ method chains like `target()->Method()` create
  nested call_expressions with the same start position. Both inner and outer
  calls were being classified, causing duplicate (caller_key, line, col) records
  that violated the storage UNIQUE constraint.
  - **Root cause:** .h files containing C++ code parsed by tree-sitter-c.
    Method chaining creates nested call_expressions sharing the same start.
  - **Fix:** Extractor dedup before returning results. For duplicate keys,
    keeps the entry with the longer callee_name.
  - **Heuristic limitation:** "Keep longer callee_name" assumes the outer method
    in a chain is the semantically relevant callee. This is a containment
    heuristic, not a universal truth. Counterexamples:
    - `get_status()->code` where inner `get_status()` is the interesting call
    - Chains where both calls have equal-length names
    The heuristic works for observed patterns (leveldb, poco, duckdb) but may
    misclassify edge cases. A more robust approach would track AST nesting depth
    and prefer the outermost call_expression, but that requires additional
    traversal state.
  - **Validated on:** leveldb (133 files), poco (3267 files), duckdb (5109 files).

## Symbol Identity — Stable Key Contract

- **Duplicate symbol disambiguation (`:dupN` suffix):** The Rust TS/JS
  extractor assigns stable keys using the pattern `repo:file#name:SYMBOL:subtype`.
  When the same `(name, subtype)` appears multiple times in a file (common in
  test files with repeated `function Component()` or `const container`), the
  extractor appends `:dup2`, `:dup3`, etc. to subsequent occurrences.
- **Occurrence-order sensitivity:** The `:dupN` ordinal is assigned by AST
  preorder traversal during extraction. This means:
  - Inserting an earlier same-name symbol in a file can renumber later duplicates.
  - Cross-snapshot symbol identity is not preserved when same-name symbols
    are added/removed/reordered.
  - This is a practical collision fix, not a semantic identity model.
- **Acceptable for current use cases:** Indexing, hotspots, and graph queries
  work correctly because each snapshot has internally consistent keys. The
  limitation matters for:
  - Cross-snapshot symbol tracking (e.g., "did this function's complexity
    change between commits?")
  - Incremental delta refresh where unchanged files might reference symbols
    whose keys shifted in changed files (this case is rare in practice).
- **Not yet fixed:** A proper semantic identity model would require scope-
  qualified names (e.g., `describe_block.it_block.Component`) or content
  hashing. Deferred until cross-snapshot symbol tracking becomes a product
  requirement.

## Coverage / Churn Import

- File filtering uses repo-level file inventory (getFilesByRepo), not
  snapshot-scoped FILE nodes. Adequate for single-snapshot model, needs
  tightening for multi-snapshot.

## Test Coverage Gaps

### General
- `supersedes_uid` linkage in declaration supersession: implemented but not
  verified because `declare list --json` does not expose the field.
- `inheritObligationIds()` matcher is tested as a pure helper but not yet
  exercised by any live supersession path.

### Rust-Specific
- **pnpm test is not giving a clean signal in the current workspace** due to
  recurring `better-sqlite3` NODE_MODULE_VERSION drift. The issue is
  environmental (Node.js version changes between invocations), not code-related.
  Fix: `pnpm rebuild better-sqlite3`.
- **Storage parity test failure:** `cargo test -p repo-graph-storage --test parity`
  fails because expected.json fixtures were last updated in commit c0fb0dc, before
  migrations 027 (freshness/provenance) was added in commit 50b4882. The test dumps
  the full DB schema including migrations table; the expected fixtures expect fewer
  migrations. Fix: regenerate expected.json files with `RGR_STORAGE_PARITY_EMIT_ACTUAL=1`
  and commit the updated fixtures. This is a fixture-drift issue, not a storage parity
  regression. Unrelated to any specific slice work.
- ~~**modules_violations_command tests failing:**~~ **FIXED (2026-05-13).** Added
  `clear_auto_generated_modules()` helper that clears indexer-generated module
  candidates before tests insert their own. The indexer auto-discovers npm packages
  as module candidates, which conflicted with manually-inserted test data.

### TypeScript-Specific
- `package-name extends` in tsconfig: `extends: "@tsconfig/node18"` not
  resolved (requires node_modules lookup). Near-zero impact on current repo set.

## Dead Code Trust Boundary

- `graph dead` answers "no known inbound graph edges AND no framework-liveness
  inference," not "semantically unreachable."
- Three suppression layers: (1) inbound edges, (2) entrypoint declarations,
  (3) framework-liveness inferences (`framework_entrypoint`, `spring_container_managed`).
- Registry/plugin-driven architectures produce false positives.
- Spring bean detection suppresses class-level false positives but not method-level
  (handler methods, injected fields inside beans remain dead in the graph).
- See v1-validation-report.txt for the full extraction capability boundary.

## Classifier Limitations

- **Classifier version 6** is the current persisted format. Rows from earlier
  versions are distinguishable by `classifier_version` column on `unresolved_edges`.
- The crate-internal module heuristic (`RUST_CRATE_INTERNAL_MODULE_HEURISTIC`)
  labels likely-internal Rust imports but cannot prove they are internal without
  crate module-tree awareness.
- Blast-radius HIGH is always 0 (entrypoint-path detection deferred).
- No systematic accuracy spot-check of classifier verdicts has been performed
  since the v4 spot-check (100% precision on 96 sampled edges). Rust-specific
  precision has not been formally measured.

### Framework detection without proportion gating (P1)

**Discovered:** 2026-04-26 during Hadoop large-repo validation
**Trigger:** Hadoop repo (14K Java files) contains a React UI subproject
**Symptom:** `nextjs_app_router_detected` triggers in trust report
**Root cause:** Single `layout.tsx` file at
`hadoop-yarn-project/.../src/main/webapp/src/app/routes/layout.tsx` matches
the Next.js pattern detector.

**Impact:** Trust degradation (`framework_heavy_suspicion: true`) applies
repo-wide when only one small subproject matches. A Java repo gets framework
downgrade warnings for having any React UI code.

**Required fix:** Framework detection needs proportion/majority gating. Options:
1. Threshold: trigger only when >X% of files match the framework pattern
2. Language scoping: apply framework detectors only to files of matching language
3. Subtree isolation: detect framework patterns per subtree, not repo-wide

**Workaround:** None. Trust report shows misleading framework_heavy_suspicion.

### Vendored code pollution in hotspot rankings (P2)

**Discovered:** 2026-04-22 during swupdate validation
**Trigger:** swupdate repo embeds `mongoose/mongoose.c` (vendored HTTP library)
**Symptom:** `mongoose/mongoose.c` (complexity 5129) dominates hotspot rankings
**Root cause:** No automatic vendored-code exclusion in raw hotspot computation

**Impact:** Agent steering degraded — top-ranked files are irrelevant vendored
code, not actual project hotspots.

**Mitigation:** `--exclude-vendored` presentation flag (excludes standard paths
like `vendor/`, `third_party/`). Does NOT exclude project-specific vendored
paths like `mongoose/`.

**Required fix:** Config-driven vendored path patterns. Allow projects to declare
custom vendored directories that should be excluded from hotspot rankings.

## Boundary Interaction Model

### HTTP provider extractor (Spring)
- **MATURE — AST-backed** via tree-sitter-java. Handles multiline annotations,
  `value=`/`path=` attributes, `method=RequestMethod.X`, marker annotations
  (no parens). Validated on glamCRM (97 routes, identical output to regex version).
- **Known inefficiency:** each Java file is parsed twice during indexing (once by
  the Java extractor, once by the Spring route extractor) because the Java extractor
  does not expose its parse tree. Deferred until measurable cost.
- **Not supported:** array-valued paths `@GetMapping({"/a", "/b"})`, custom composed
  annotations, Spring WebFlux functional endpoints.

### HTTP provider extractor (Express)
- **PROTOTYPE — line-based regex.** Receiver provenance gated to `{app, router, server}`.
  Express import gate prevents false positives on non-Express files.
- Consumes `FileLocalStringResolver` for constant-backed route paths.
- Validated on fraktag (47 routes).
- **Not supported:** aliased receivers (`const api = express.Router(); api.get(...)`),
  `app.route("/x").get().post()` chaining, middleware-only registrations (`app.use`),
  mounted router prefixes (`app.use("/api", router)`).

### HTTP consumer extractor (TS/JS)
- **PROTOTYPE — line-based regex** with file-local string resolution via
  `FileLocalStringResolver`. Resolves base-URL constants including bare identifiers.
  glamCRM: 85 consumers, 94.1% match rate. fraktag: 42 consumers, 97.6% match rate.
- **FileLocalStringResolver scope (v1):**
  - Same-file only. Does not follow imports.
  - Top-level `const` and `export const` declarations. No `let`, `var`, destructuring.
  - String literals, template literals, binary `+` concatenation.
  - References to previously-resolved same-file constants (chained bindings).
  - Environment variable placeholders (`import.meta.env.*`, `process.env.*`)
    treated as opaque prefix, stripped from resolved value.
  - No function calls, no object property access, no computed expressions.
  - Bounded recursion (max 10 steps) for cycle safety.
- **Remaining consumer gaps (glamCRM 5 of 85):**
  - Genuine path mismatches between frontend and backend (e.g. `with-docx` vs
    `with-doc`, `POST /estimates` vs `POST /estimates/create`). These are
    application-level facts, not extractor failures.
- **Not yet supported:**
  - Imported constants (`import { BASE_URL } from "./config"`)
  - Wrapper functions (`function apiGet(path) { return axios.get(BASE + path) }`)
  - Object property URLs (`config.baseUrl`)
  - Multi-line URL arguments
  - RTK Query / TanStack Query patterns

### Boundary links (derived)
- Stored in separate `boundary_links` table, NOT in core `edges` table.
- Materialized at index time for intra-repo convenience. Discardable.
- No cross-repo matching yet (architecture supports it, no implementation).

### Dead-code suppression via framework inferences
- `findDeadNodes` excludes nodes with `framework_entrypoint` (Lambda),
  `spring_container_managed` (Spring bean), `pytest_test`, `pytest_fixture`.
- **Remaining gap:** methods inside container-managed classes (e.g. `@GetMapping`
  handler methods, injected fields) still show dead because they have no
  Java-caller inbound edges. Spring's HTTP dispatcher invokes them at runtime.

## Large-Repo Scalability

- **All-in-memory indexer architecture:** `fileContents` Map holds all source
  text, `allNodes` and `allUnresolvedEdges` arrays accumulate all extracted
  data before persistence. This works up to ~1000 files but OOMs on
  Linux-scale repos (63k C files).
- **Large-file guard (1MB):** operational containment for generated headers
  (AMD GPU register masks: 200k+ lines). Does not fix the aggregate memory
  problem from tens of thousands of normal-sized files.
- **Required fix:** batch persistence + bounded resolution windows. Remove
  `fileContents`, persist nodes/edges incrementally during extraction, then
  resolve in chunks. This is an architecture redesign, not a parameter tune.

## Operational Dependency Seam Slice (deferred items)

Items surfaced during the env + fs operational dependency seam slice that
were intentionally not addressed in that slice. Tracked here so they do
not drift. Each item names the in-slice context in which it was discovered.

### Pin Node version via .nvmrc and tighten engines (P2)
- `package.json` declares `engines.node: ">=20"`, which permits any of
  v20, v22, v24. `better-sqlite3` is a native addon and its compiled
  binary is keyed to the Node ABI version. Switching Node versions
  silently invalidates the binary, producing recurring `NODE_MODULE_VERSION`
  mismatches that cost minutes per encounter and create false-baseline
  test claims when the rebuild ran against a different Node than the
  developer's interactive shell.
- **Fix:** add `.nvmrc` (or `.tool-versions`) pinning a specific Node
  version, and tighten `engines.node` to a narrow range (e.g. `">=22 <23"`)
  so an accidental v24 invocation fails fast.
- **Severity:** P2. Affects every contributor with multiple Node versions
  installed (homebrew + nvm + asdf are common). Discovered during the
  seam slice's Step 0 floor stabilization.

### CLAUDE.md rebuild note must clarify Node-version scope (P3)
- The current note `pnpm rebuild better-sqlite3` does not state that the
  rebuild is only valid for the Node version active when the rebuild ran.
  Switching Node afterwards silently re-breaks the binary.
- **Fix:** two-line addition to CLAUDE.md noting that the rebuild is
  Node-version-scoped and must be re-run after `nvm use` / version
  switches.
- **Severity:** P3. Documentation. Cheap.

### Evaluate node:sqlite as a substrate replacement (P2)
- Node ≥22.5 ships a built-in `node:sqlite` module. Migrating off
  `better-sqlite3` would eliminate the entire native-addon ABI failure
  class. This is the durable structural fix that the .nvmrc pin only
  postpones.
- **Not an automatic win.** Capability parity must be evaluated
  before adopting:
  - synchronous API shape (better-sqlite3 is sync, node:sqlite is sync)
  - prepared statement behavior and lifecycle
  - transaction ergonomics
  - WAL mode behavior
  - performance on the sizes this codebase actually exercises
- **Severity:** P2. Substrate decision, not a maintenance tweak. Should
  be a separate evaluation slice. May be subsumed by the eventual Rust
  daemon move (which uses `rusqlite` and bypasses the issue entirely).

### Live jdtls test self-skip is incomplete (P2) — RESOLVED in D-1

**RESOLUTION (D-1, live-enrichment test gating cleanup):** Closed via fix
option 3 from the original entry. Both live integration tests
(`java-enrichment-integration.test.ts`, `rust-enrichment-integration.test.ts`)
were moved from `test/adapters/enrichment/` to `test/live/`. The default
vitest config (`vitest.config.ts`) now excludes `test/live/**` via
`configDefaults.exclude` extension. A new `pnpm run test:live` script runs
the live tests explicitly as opt-in observability via a dedicated
`vitest.live.config.ts` that inherits shared test options from the default
config and includes only `test/live/**/*.test.ts`. After D-1, `pnpm run test`
and `pnpm run test:all` no longer transit `test/live/**` and therefore can
no longer be made false-negative by jdtls workspace drift, rust-analyzer
indexing race, or any other live external-tool state. The Rust-1
acceptance report's "non-admissible surfaces" caveat about live-tool
contamination is historically resolved.

**SCOPE LIMIT — what D-1 does NOT fix.** D-1 closed the live-tool
contamination only. The default `pnpm test` surface still has a separate,
unrelated instability source: the `pnpm test is not giving a clean signal`
debt about `better-sqlite3` NODE_MODULE_VERSION drift listed under
`Test-Scope Debt > Rust-Specific` earlier in this file. That debt is
environmental (Node.js version changes between invocations) and is
independent of D-1's scope. Acceptance language for any future slice
that uses `pnpm run test` or `pnpm run test:all` as evidence should
phrase those surfaces as "green in this shell with the current Node ABI
aligned" rather than as universally deterministic, until the
NODE_MODULE_VERSION drift debt is also closed.

**Original problem statement (preserved for audit trail):**

- `test/adapters/enrichment/java-enrichment-integration.test.ts` gates
  itself on `which jdtls`. If jdtls is on PATH, the tests run; if not,
  they self-skip. The gate does NOT detect the case where jdtls is on
  PATH but its workspace state is corrupted, the JDK toolchain is
  incompatible, or the Gradle import fails to complete. In those cases
  hover requests return `null` and assertions like `expect(receiverType).toBe("HashMap")`
  fail with no obvious connection to environment state.
- **Effect:** `pnpm run test` produces false-negative baselines whenever
  jdtls workspace state drifts. Affects every contributor with jdtls
  installed.
- **Fix options (option 3 was selected and implemented in D-1):**
  1. Add a sentinel hover precheck that verifies jdtls + JDK + fixture
     import are actually working before running the receiver-type
     assertions. Skip the live tests if the precheck fails.
  2. Gate the live test behind an explicit `RGR_LIVE_INTEGRATION=1`
     env var so it never runs by default.
  3. Move all live integration tests into a separate `pnpm run test:live`
     script and exclude them from the default `pnpm run test`.
- **Severity:** P2. Discovered during the seam slice's Step 6
  re-baseline. Caused divergence between the AI's environment (test
  passed) and the user's shell (test failed). Out of scope for the
  seam slice itself.

### String-literal-embedded env access false positives (P3)
- The comment masker landed in the seam slice (`src/core/seams/comment-masker.ts`)
  preserves string literal contents by design — fs detectors rely on
  literal first-argument paths inside strings (e.g.
  `fs.writeFile("real_path")`). As a side effect, env-access patterns
  embedded in string literals like `"http://example.com/path/process.env.X"`
  are still matched by the env detector regex.
- **Severity:** P3. Lower than the comment-derived false positives that
  the masker DID fix. String-embedded env patterns are rare in real
  production code and were not visible in repo-graph dogfood after the
  masker landed.
- **Possible fix paths (any of):**
  - Tighten env regex to require code-context anchors (e.g., assignment
    operator, declaration, or call argument boundary).
  - Add a second pre-pass that masks string literal contents
    specifically for the env detector (the fs detector cannot use this
    because it depends on string literal contents).
  - Move env detection from regex to a real lexer so context is
    available.
- Discovered during seam slice Step 6 dogfood after C-1 (comment
  masking) shipped.

### Detector externalization to shared YAML/TOML tables (P2)
- Env, fs, and other seam detectors are currently regex tables defined
  inline in TypeScript. The architectural decision recorded earlier in
  this conversation (R-Q3) is to externalize detector rules to shared
  YAML or TOML tables consumed by both the TypeScript implementation
  and the eventual Rust daemon implementation. This avoids regex drift
  between two language runtimes during the prototype-then-port window.
- **Scope:** detector tables become data; detector wiring becomes a
  YAML loader plus a generic regex-table walker. Comment masking
  remains language-specific code.
- **Severity:** P2. Substrate refactor, scheduled after the operational
  dependency seam feature surface ships and after detector contracts
  stabilize from real-repo exposure. Not P1 because the current
  hardcoded tables work and the parity problem only matters once Rust
  porting begins.
- **Critical sequencing:** must NOT be folded into a feature slice.
  This is its own substrate move. Recorded as the "next substrate
  slice" in the conversation roadmap.

## Rust CLI (`rmap`) — known divergences and deferred items

Frozen at milestone `rmap-structural-v1` (Rust-7B through Rust-20).
See `docs/milestones/rmap-structural-v1.md` for the full milestone
document. This section tracks what is not yet ported and what
intentionally diverges.

### Deferred TS CLI features (not yet ported to Rust)

- **`--edge-types` filter: narrow type set.** TS accepts all 18
  canonical edge types for callers/callees. Rust (Rust-17) accepts
  only CALLS and INSTANTIATES. Widening to all 18 is a one-line
  change in `VALID_EDGE_TYPES` but deferred until needed.
- **`--min-lines` filter for dead.** TS supports `--min-lines N`.
  Rust omits this filter.
- **`graph imports --depth`.** TS supports `--depth N` for transitive
  imports. Rust (Rust-18) is one-hop only.
- **`graph imports` module input.** TS accepts both file paths and
  module/symbol names (falls back to `resolveSymbolKey`). Rust
  (Rust-18) accepts file paths only (constructs `{repo}:{path}:FILE`
  stable key). Module input is a future extension.
- **`graph path` parameter surface.** TS supports `--max-depth N`
  (default 8) and `--edge-types CALLS,IMPORTS`. Rust (Rust-19)
  hardcodes both defaults. Also, TS accepts FILE/MODULE stable
  keys via `resolveSymbolKey`; Rust is symbol-only at both endpoints.
- **`graph metrics`.** Partially ported. `rmap metrics` queries
  measurements by kind with sorting and limit. Does not include
  `--module` aggregate mode (TS `rgr graph metrics --module`).
- **`graph versions`.** Not ported.
- **Table output format.** Rust is JSON-only. No human-readable
  tables. No `--json` flag (always JSON).

### Accepted divergences (intentional, not debt)

- **Rust `callers`/`callees` envelope includes `target` field.**
  The TS CLI does not include the resolved target in the callers/
  callees JSON output (it uses a different formatting path via
  `outputNodeResults`). The Rust side adds `target` as a
  convenience field. This is a superset, not a contract break.
- **Rust `dead` envelope includes `kind_filter` field.** TS does
  not emit this. The Rust side adds it for transparency. Superset.
- **`trust` command does not use QueryResult envelope.** Both TS
  and Rust emit a trust-specific report shape. No drift.
- **Rust `gate` waiver overlay: PASS obligations are not waivable (Rust-25).**
  TS unconditionally sets `effective_verdict = WAIVED` when any
  matching active waiver exists, regardless of `computed_verdict`.
  This means a PASS obligation with a waiver shows as WAIVED in TS,
  inflating `waived` counts and hiding the distinction between
  obligations that needed an exception and obligations that passed
  on merit. Rust applies waivers only to non-PASS computed verdicts.
  A PASS obligation stays PASS with `waiver_basis = null`, even if
  a matching waiver exists. This is the corrected policy model:
  `effective_verdict` represents the verdict after policy
  transformation, and no transformation occurs for PASS.
  The TS prototype should be aligned to match if/when TS is still
  actively maintained.

- **Rust declaration UIDs are deterministic, TS uses random UUIDs (Rust-32).**
  TS `declare` commands generate `uuidv4()` — a random UUID per
  insert. Repeated declare runs create duplicate rows with different
  UIDs. Rust uses UUID v5 (SHA-1 namespace hash) derived from the
  semantic identity tuple `(kind, identity_key)` where identity_key
  is constructed from typed policy fields:
    - boundary: `{repo}:{module_path}:{forbids}`
    - requirement: `{repo}:{req_id}:{version}`
    - waiver: `{repo}:{req_id}:{requirement_version}:{obligation_id}`
  This makes `insert_declaration` idempotent at the policy level:
  the same logical policy object always produces the same UID, and
  INSERT OR IGNORE prevents duplicates. Cosmetic changes to
  `value_json` (reason text, obligation wording) do NOT alter the
  UID — they are not policy identity. This is a deliberate
  correction to the TS prototype's append-only-by-accident pattern.

### Known contract gaps

- **`index` and `refresh` have no JSON output.** They print
  progress to stderr only. TS `rgr repo index` also uses stderr
  for progress, so this is aligned.
- **No `--level file` flag for cycles.** TS supports `--level
  file` to detect file-level cycles. Rust hardcodes module-level.
- **No staleness tracking for `index`/`refresh` commands.** Only
  read-side commands compute the `stale` field.

## Module Resolution Dual-Path Model

**Current state:** Module graph loading and edge derivation are now
consolidated in the `repo-graph-module-queries` support crate. CLI
handlers are thin adapters that consume preloaded facts.

The crate provides:
- `ModuleQueryContext` — abstracts over TS/Rust backing stores
- `ModuleGraphFacts` — bundled context + edges + diagnostics + module_refs
- `load_module_graph_facts()` — single-load orchestration
- `evaluate_violations_from_facts()` — violation evaluation on preloaded facts

### Backing representations

Two storage paths are unified at query time:

- **TS path:** `module_candidates` table + `module_file_ownership` table
- **Rust path:** `nodes` table (kind='MODULE') + `edges` table (type='OWNS')

The fallback logic is application-level normalization policy, not raw
storage truth. It lives in the support crate (not storage CRUD) to
preserve honest method semantics in the storage adapter.

### Structural limitations

- **Rust directory modules are synthesized compatibility projections,
  not full module-candidate parity.** The Rust indexer creates MODULE
  nodes for directories containing files. These lack metadata that
  TS-indexed module candidates have (module_kind, display_name,
  metadata_json carry less information).

- **`modules files` returns degraded output on Rust-indexed repos.**
  The fallback path provides `file_path` and `is_test` but not
  `language`, real `assignment_kind` (hardcoded to "inferred"),
  or real `confidence` (hardcoded to 1.0). The detailed file
  metadata lives only in the `module_file_ownership` table which
  the Rust indexer does not populate.

- **Dual persistence model remains until unified module read model.**
  The ideal fix is for the Rust indexer to populate the same
  `module_candidates` and `module_file_ownership` tables as the
  TS indexer. Until then, the support crate fallback bridges the gap.

### Commands using preloaded facts

Commands that need module graph + violations now call
`load_module_graph_facts()` once, then pass `&facts` to downstream
consumers:

- `modules list` — uses facts for catalog + violation rollups
- `modules show` — uses facts for module briefing + violations
- `modules deps` — filters `facts.edges` directly
- `modules violations` — passes facts to `evaluate_violations_from_facts()`
- `rmap violations` — same preloaded-facts pattern

Commands that need context only (no edges/violations) use
`ModuleQueryContext::load()` directly:

- `modules files` — resolves module, queries file ownership
- `modules boundary` — resolves source/target modules, inserts declaration

### Previous duplication (resolved)

Before Slice 12 P2 fix, module edge derivation was duplicated:
1. Each CLI command loaded module graph independently
2. Advisory violations helper re-queried the graph for rollups
3. `modules violations` and `modules list` could produce inconsistent
   violation counts due to timing differences

Now all graph loading flows through `load_module_graph_facts()`. The
shared `evaluate_violations_from_facts()` operates on the same
preloaded facts used by callers, eliminating duplicate queries and
ensuring consistency.

## Inferred Module Identity Evolution (Phase 3.2)

Inferred module identities are stable for a given heuristic version but may
change when the heuristic is intentionally upgraded.

### Umbrella-directory splitting (Phase 3.2)

The umbrella-directory splitting heuristic changes identity for qualifying
directories:

**Before Phase 3.2:**
- `inferred:nginx:src` (single module for all of src/)

**After Phase 3.2:**
- `inferred:nginx:src/core`
- `inferred:nginx:src/http`
- `inferred:nginx:src/event`

This applies when:
- Directory is an umbrella prefix (`src`, `packages`, `services`, `apps`, `libs`, `modules`)
- At least 2 children have 5+ source files each
- Parent has 5 or fewer direct source files

**Impact:**
- Module UIDs change (hash includes full path)
- Module keys change (`inferred:{repo}:{path}`)
- File ownership reassigned to child modules
- No backward-compatible dual identity

This is intentional heuristic evolution, not a breaking change. Inferred modules
are orientation-grade data (confidence 0.7), not declared truth. Agents should
not assume inferred module identities are permanent across heuristic upgrades.

## Rust agent use-case crate (`repo-graph-agent`)

Current state: Rust-43A. Repo-level `orient` use case with
typed DTOs, `AgentStorageRead` port, ranking, budget
truncation, confidence derivation, AND gate signal coverage
(`GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`) via dependency
on `repo-graph-gate`. No CLI (Rust-43B), no `check`/`explain`
(later slices), no module/symbol focus (Rust-44/45). See
`docs/architecture/agent-orientation-contract.md` for the
normative surface description.

### Rust-43 F1/F2/F3 fix slice — CLOSED

The repo-graph self-index spike
(`docs/spikes/2026-04-15-orient-on-repo-graph.md`) exposed
three semantic defects in the Rust-43B `orient` contract:

- **F1:** The DEAD_CODE signal reported 86% of symbols as
  dead on a Rust-indexed repo, because the Rust indexer does
  not populate framework-liveness inferences and the
  aggregator had no reliability gate. An agent reading that
  output would prioritize mass deletion over investigation.
- **F2:** TRUST_NO_ENRICHMENT was silently suppressed on
  Rust-indexed repos because `enrichment_applied: bool`
  collapsed "phase never ran" and "phase ran with zero
  eligible edges" into one state.
- **F3:** The DEAD_CODE aggregator had no coupling to trust
  reliability, so the signal ranked at #1 (Medium severity)
  regardless of call-graph quality.

Action taken:

1. New DTOs in the agent crate:
   - `AgentReliabilityLevel { Low, Medium, High }`
   - `AgentReliabilityAxis { level, reasons: Vec<String> }`
   - `EnrichmentState { Ran, NotApplicable, NotRun }`
2. `AgentTrustSummary` widened with `call_graph_reliability`,
   `dead_code_reliability`, and `enrichment_state`. The old
   `enrichment_applied: bool` was removed.
3. New `LimitCode::DeadCodeUnreliable` with a static summary.
   The `Limit` struct gained a `reasons: Vec<String>` field
   (serialized only when non-empty, preserving backward
   compatibility for every other limit).
4. Dead-code aggregator signature gained
   `trust: &AgentTrustSummary`. Emission is gated on
   `trust.dead_code_reliability.level == High`. When the
   level is not High, the aggregator emits
   `DEAD_CODE_UNRELIABLE` carrying the trust layer's reason
   vector verbatim (fallback: the stable string
   `"dead_code_reliability_not_high"` if trust returned
   empty reasons).
5. Trust aggregator rule update: `TRUST_NO_ENRICHMENT`
   fires iff `enrichment_state == NotRun`.
6. Confidence derivation update: `NotRun` degrades
   high→medium on the enrichment axis; `Ran` and
   `NotApplicable` are silent on this axis.
7. Storage adapter `get_trust_summary` projects the trust
   crate's composite reliability axes into the agent DTO
   without re-deriving any thresholds. The trust crate is
   the authority.
8. Tests: 7 new integration tests in
   `rust/crates/agent/tests/orient_repo_dead_code_reliability.rs`
   pin the reliable path, the Low/Medium/Low-reason-fallback
   paths, the JSON shape of `reasons`, and the
   `skip_serializing_if = "Vec::is_empty"` behavior.
   Existing tests updated to construct the new DTO shape.
   Storage-side smoke test asserts the empty-snapshot
   reliability-gate case end-to-end.
9. Spike re-run verified the fix on the same database
   (`/tmp/rgspike.db`). See the spike document for the
   before/after comparison.

Zero workspace regressions: 1128 tests passing, 0 failures.

#### P2 follow-up: enrichment state disambiguation

The initial F1/F2/F3 fix mapped `Option<EnrichmentStatus>`
directly: `None → NotRun`, `Some(_) → Ran` or `NotApplicable`
based on `eligible > 0`. A code-review P2 identified that
`None` from the trust layer covers THREE distinct cases:

1. No eligible `CallsObjMethodNeedsTypeInfo` samples at all.
2. Samples exist but none are in that category.
3. Samples exist AND are in that category, but the
   enrichment phase never ran.

Cases 1 and 2 are `NotApplicable` semantically — no work for
enrichment to do — but the pre-P2 adapter reported all three
as `NotRun`, which would emit a spurious `TRUST_NO_ENRICHMENT`
signal and degrade confidence on the enrichment axis for any
repo with nothing to enrich.

Fix:

- Added a `#[serde(skip, default)] pub enrichment_eligible_count: u64`
  field to `trust::types::TrustReport`. The field is internal
  to the Rust trust crate and never serializes, so the TS
  parity contract and existing fixtures are unchanged
  (verified by running the trust parity harness).
- `compute_blast_radius_and_enrichment` now returns a
  3-tuple with the eligible count as its third element, and
  `compute_trust_report` populates the new field from that
  value. When `all_classification_counts.is_empty()` the
  count is 0 (short-circuit path, no compute run).
- The agent storage adapter reads the counter to disambiguate:
  `None && count == 0 → NotApplicable`, `None && count > 0 →
  NotRun`, `Some(_) → Ran`.

Tests added:

- 2 new trust unit tests pinning `enrichment_eligible_count`
  at 0 for "no samples at all" and "samples present but none
  are CallsObjMethodNeedsTypeInfo".
- 1 updated trust unit test pinning the counter at 1 in the
  "phase did not run" case.
- 1 new storage integration test
  (`empty_snapshot_maps_to_enrichment_state_not_applicable`)
  that exercises the adapter through `StorageConnection` and
  verifies the empty-snapshot mapping is `NotApplicable` (not
  the pre-P2 `NotRun`).
- The spike re-run on `/tmp/rgspike.db` produced identical
  output to the post-F1/F2/F3 output, confirming that the
  repo-graph self-index is legitimately in the `NotRun` state
  (eligible samples exist, phase did not run). The P2 fix
  did not alter this; it removed the false-positive case for
  repos with nothing to enrich.

Post-P2 workspace: 1131 tests passing, 0 failures (+3
regression pins).

### Dead-code surface withdrawal (Option D) — 2026-04-28

**Status:** Public surfaces removed. Internal substrate preserved.

The dead-code surface (`DEAD_CODE` signal, `DEAD_CODE_UNRELIABLE` limit,
`dead_code_reliability` in explain trust evidence) is withdrawn from all
user-facing `rmap` outputs:

- `orient` emits no `DEAD_CODE` signal regardless of reliability level
- `orient` emits no `DEAD_CODE_UNRELIABLE` limit regardless of reliability
- `check` does not evaluate `DEAD_CODE_RELIABILITY` condition (removed from check reducer)
- `explain` trust evidence omits `dead_code_reliability` field
- `trust` command omits `dead_code` field from JSON output (via `#[serde(skip_serializing)]`)
- JSON output contains no `DEAD_CODE` or `dead_code` vocabulary

**Why withdrawn:** Self-index spike showed 86% of symbols flagged as dead
on repo-graph. Without coverage-backed evidence or mature framework-liveness
detection, the surface misleads agents toward mass deletion instead of
investigation.

**What is preserved (internal substrate):**

- `AgentTrustSummary.dead_code_reliability` field (internal use only)
- `find_dead_nodes` storage query
- `dead_code::aggregate()` function (returns empty output, logic preserved)
- `AgentDeadNode` DTO
- Trust crate's `dead_code_reliability` axis computation
- All dead-code related storage queries and trust computations

**Reintroduction requirements — BINDING POLICY:**

Public dead-code surfaces (`rmap dead`, `DEAD_CODE` signal, `DEAD_CODE_RELIABILITY`
condition) must NOT be reintroduced from structural graph heuristics alone.

**Mandatory evidence floor:** Measured execution evidence must exist in the Rust
product path before any dead-code claim resurfaces. Accepted evidence classes:
- Line coverage (e.g., lcov, cobertura), OR
- Function/call coverage (e.g., llvm-cov function-level)

The exact ingestion mechanism and coverage source format are unspecified pending
roadmap commitment. The product requirement is the evidence class, not the
implementation mechanism.

**What is NOT sufficient for dead-code reintroduction:**
- Framework entrypoint detection (Spring, React, Axum, FastAPI, etc.)
- Entrypoint declarations (`rmap declare entrypoint`)
- Structural graph orphan analysis (no inbound edges)
- Trust reliability axes alone

These signals improve discovery and liveness understanding. They suppress false
positives in the internal substrate. They are NOT proof of deadness.

**Architectural distinction (binding):**
- **Orphans / structurally unreferenced:** May be reintroduced as a separate
  heuristic discovery surface (`rmap orphans`). Explicit framing: "not currently
  referenced in the graph we built." Useful for orientation, not deletion.
- **Dead code:** Requires runtime measurement evidence. Framing: "unexecuted
  under measured scenarios AND structurally weakly connected." Actionable for
  cleanup decisions.

This distinction is central to repo-graph's agent-facing product model:
- **Discovery surfaces:** What exists, what changed, what is structurally isolated
- **Runtime-liveness hints:** Framework detection, entrypoint declarations
- **Coverage-backed deadness claims:** Only with measured execution evidence

When reintroducing, restore:

1. `SignalCode::DeadCode` variant (removed from enum)
2. `Signal::dead_code()` constructor
3. `DeadCodeEvidence` / `DeadSymbolEvidence` structs
4. `SignalEvidence::DeadCode` variant
5. `LimitCode::DeadCodeUnreliable` variant
6. `ConditionCode::DeadCodeReliability` variant in check/types.rs
7. `dead_code_reliability` field in `CheckInput`
8. Dead-code evaluation section in check/evaluate.rs
9. `dead_code_reliability` field in `ExplainTrustEvidence`
10. Remove `#[serde(skip_serializing)]` from trust `dead_code` field
11. Re-enable `dead_code::aggregate()` to emit signals when reliability is High

### Deferred items (explicit)

- **Output-quality cleanup (F4/F5/F6).** Deferred to a
  separate slice. Items: (F4) test-file filter on `top_dead`
  when DEAD_CODE does fire, (F5) polyglot-blind
  `MODULE_SUMMARY.languages` on non-TS indexed repos,
  (F6) ranking tie-break between DEAD_CODE and
  IMPORT_CYCLES in the Structure/Medium tier. None of these
  block further slices but all would improve output quality.
  The F2 fix also left a minor wording tweak on
  `TRUST_NO_ENRICHMENT`'s summary — still says "No compiler
  enrichment applied" when it should say "Enrichment phase
  did not run" under the new NotRun semantics.
- **Module, path, and symbol focus.** `orient(focus = Some(_))`
  currently returns `OrientError::FocusNotImplementedYet`. This
  is deliberately an error variant, not a silent degraded
  response, so a caller that requests a focus area learns
  immediately that their request is not honored. Module focus
  ships in Rust-44, symbol focus in Rust-45.
- **`check` is repo-level only.** Scoped check (file/path/symbol
  focus) is not implemented. Only whole-repo check is available.
- **`check` does not expose individual condition exit codes.**
  The CLI returns only the aggregate verdict exit code (0/1/2).
  Individual condition pass/fail/incomplete status is available
  in the JSON output only.
- **`check` CLI uses `<db_path> <repo_uid>`.** Same temporary
  positional shape as orient, pending repo registry.
- **`explain` use case.** Only `orient` and `check` are
  implemented. The DTO envelope is shared; the `explain`
  aggregator pipeline is not yet written.
- **Binary renamed; repo registry deferred.** Rust-43A
  relocated gate. Rust-43B added the `orient` CLI command.
  Rust-43C renamed the binary from `rgr-rust` to `rmap`
  (test harnesses, docs, CLAUDE.md all updated).
  Repo registry (`rmap repo add` equivalent) is still
  deferred.
- **`orient` positional shape diverges from the contract.**
  The agent orientation contract specifies
  `rmap orient <repo_name>` with an implicit repo registry
  and `--db <path>` as an escape hatch. `rmap orient`
  currently ships with `<db_path> <repo_uid>` because
  `rmap` has no repo registry yet — no equivalent of
  `rmap repo add`. Until a registry slice lands, every
  `rmap` command including `orient` takes the
  `<db_path> <repo_uid>` pair. Repo-name invocation and the
  `--db` escape hatch will land together in the registry
  slice.
- **`--focus` CLI grammar is locked but runtime is
  deferred.** The `rmap orient --focus <string>` flag
  parses and validates, then exits 2 with a
  `FocusNotImplementedYet` diagnostic. Rust-44 (module/path)
  and Rust-45 (symbol) will implement the runtime without
  changing the flag grammar.
- **`COMPLEXITY_UNAVAILABLE`.** Emitted only when no cyclomatic
  complexity measurements exist for the snapshot. The Rust
  indexer produces complexity measurements, so Rust-indexed
  repos emit `HIGH_COMPLEXITY` instead of this limit. The limit
  still fires for snapshots without measurement data (e.g., old
  snapshots predating the measurement pipeline).
- **`MODULE_DATA_UNAVAILABLE`.** Emitted only when both the
  `module_candidates` table and the MODULE nodes in the `nodes`
  table are empty. TS-indexed repos populate `module_candidates`;
  Rust-indexed repos produce MODULE nodes. The agent storage
  adapter falls back to MODULE nodes when `module_candidates` is
  empty, so both indexing paths surface module evidence. The
  limit fires only on truly module-less snapshots. When module
  data exists, `MODULE_SUMMARY` includes `discovered_module_count`
  and `module_kinds` breakdown (declared/operational/inferred).
  MODULE nodes count as inferred (directory-derived).
- **Next-action emission.** The repo-level `next` list is
  always empty. Structured `NextAction` records become
  meaningful under module/symbol focus.
- **Staleness wording discipline.** `TRUST_STALE_SNAPSHOT`
  uses `get_stale_files` as its data source and describes the
  storage-internal condition only (`"Snapshot has N stale
  files recorded in storage."`). It does NOT claim that the
  repository has changed since the last index — that would
  require a filesystem or git comparison the use-case layer
  intentionally does not perform. If a future slice adds a
  `current_commit: Option<&str>` parameter, the wording and
  the signal code should both be revisited so the distinction
  between "parse-staleness" and "git-staleness" stays explicit.
- **`now` is a required parameter on `orient()`.** The
  orient entry point takes `now: &str` as its final argument
  and threads it through to the gate aggregator, which passes
  it to `GateStorageRead::find_waivers` for lexicographic ISO
  8601 expiry comparison. The agent crate is deliberately
  clock-free: the function signature forces callers (CLI,
  daemon, tests) to supply an explicit wall-clock value.
  A previous draft used a constant `AGENT_NOW_SENTINEL`
  (`"9999-12-31T23:59:59Z"`) which silently mis-evaluated
  finite-expiry waivers — a far-future sentinel makes
  `expires_at > now` false for every realistic expiry, so
  every finite waiver appears already expired. The sentinel
  was removed and replaced with this explicit parameter.
  Regression tests in
  `rust/crates/agent/tests/orient_repo_gate.rs`
  (`finite_waiver_applies_before_expiry_at_orient_level`,
  `finite_waiver_does_not_apply_after_expiry_at_orient_level`,
  `perpetual_waiver_applies_regardless_of_now`) pin the
  correct semantics.

### Gate.rs relocation — CLOSED in Rust-43A

Prior state (Rust-42): `gate.rs` lived inside the `rgr` binary
crate. The agent crate had no way to call it, so orient
emitted `GATE_UNAVAILABLE` as a limit in every response.

Action taken (Rust-43A):

1. Created `rust/crates/gate/` as a new policy crate
   `repo-graph-gate`. Two-layer design:
   - `compute(input: GateInput) -> GateReport` — pure, no
     I/O, no storage, no clock.
   - `assemble(storage: &impl GateStorageRead, ...) -> GateReport`
     and `assemble_from_requirements(...)` — thin
     orchestration around storage reads, delegates to
     `compute`.
2. Defined `GateStorageRead` port (agent-style concrete error,
   mirroring `AgentStorageRead`). Implemented on
   `StorageConnection` in `rust/crates/storage/src/gate_impl.rs`.
3. Defined gate-owned DTOs (`GateRequirement`,
   `GateObligation`, `GateWaiver`, `GateMeasurement`,
   `GateInference`, `GateBoundaryDeclaration`,
   `GateImportEdge`). Gate does NOT import
   `repo_graph_storage::queries::*`.
4. `rgr/src/main.rs::run_gate` updated to call
   `repo_graph_gate::assemble`. Deleted `rgr/src/gate.rs`.
   Stderr wording preserved verbatim via a
   `format_gate_error` helper in the CLI (the gate crate
   itself has CLI-agnostic error types).
5. Added `repo-graph-agent` dependency on
   `repo-graph-gate`. `orient_repo`'s trait bound widened to
   `S: AgentStorageRead + GateStorageRead`. New
   `aggregators::gate` aggregator emits `GATE_PASS`,
   `GATE_FAIL`, `GATE_INCOMPLETE` from `GateReport.outcome`
   (always `GateMode::Default`).
6. Replaced `LimitCode::GateUnavailable` with
   `LimitCode::GateNotConfigured`. The new limit fires only
   when the repo has no active requirements.
7. Gate-crate dependency direction: `agent → gate`,
   `storage → gate`, `rgr → gate`. NO reverse edge. Gate has
   no knowledge of agent orientation.

Behavioral preservation guarantees:

- Default / strict / advisory mode semantics are byte-
  identical to pre-relocation `gate.rs`. Exit code tests in
  `rust/crates/rgr/tests/gate_command.rs` pass unchanged.
- PASS-not-waivable divergence (Rust-25) preserved. Tests
  `pass_obligation_stays_pass_even_with_matching_waiver` in
  `compute.rs` and `gate_pass_with_waiver_stays_pass` in
  `gate_command.rs` both pass.
- Malformed measurement/inference error strings preserved
  verbatim: `"malformed coverage measurement for {}: ..."`,
  `"coverage measurement for {} missing numeric \"value\" field"`,
  and the equivalents for complexity and hotspot. These are
  emitted from the new `assemble.rs` pre-parse helpers.

Future gate work:

- Storage integration tests for `GateStorageRead` on
  `StorageConnection` currently reuse the `gate_command.rs`
  CLI suite (which exercises the full pipeline). A
  dedicated `rust/crates/storage/tests/gate_impl.rs` that
  isolates the adapter layer was deliberately not added in
  this slice to keep the diff focused; adding one is a
  small follow-up.

### Agent storage port narrowness

The `AgentStorageRead` trait (defined in
`rust/crates/agent/src/storage_port.rs`) is deliberately
narrow: one method per orient data need, with agent-owned
DTOs and a storage-agnostic `AgentStorageError` type. The
trait will grow when `check`/`explain` ship. Each addition
must stay narrow — no generic escape-hatch methods, no
passing through of `StorageConnection`, no leaking of
`rusqlite::Error` or `StorageError`.

The `get_trust_summary` method is the one place in the trait
that sits on top of a second policy crate (`repo-graph-trust`).
The storage adapter calls `trust::assemble_trust_report`
internally and projects the result into the agent-owned
`AgentTrustSummary` DTO. If the trust surface gains new
fields that orient wants to read, extend `AgentTrustSummary`
— do NOT expose `TrustReport` through the port.

### Signal evidence serialization

`SignalEvidence` is a produce-only tagged enum with a hand-
written `Serialize` impl that forwards to the inner variant
struct with no discriminator tag. The `Signal` parent carries
the `code` field which serves as the discriminator in the
JSON output. Adding deserialization — for instance to consume
signals on the daemon's client side — would require
redesigning the discriminator, which is intentionally out of
scope today. If/when that need arises, the options are:
container-tagged serde (add a `kind` field inside
`evidence`), internally-tagged serde, or a dedicated
deserializer that routes by parent `code`.

### Agent signal code coverage

The enumeration in `SignalCode` is complete — every code the
agent contract mentions has a variant — but only the codes
the repo-level pipeline actually constructs have evidence
variants and named constructors. The unused codes
(`HighComplexity`, `HighFanOut`, `HighInstability`,
`CallersSummary`, `CalleesSummary`, and the gate variants)
are reserved for later slices. When they are wired up, each
addition must:

1. Add its evidence struct in `signal.rs`.
2. Add a variant to `SignalEvidence`.
3. Extend the manual `Serialize` match arm.
4. Add a named constructor to `Signal`.
5. Add a unit test asserting the code ↔ category ↔ severity
   descriptor and the evidence variant match.

## State-Boundary Extraction (slice 1)

Normative contract: `docs/architecture/state-boundary-contract.txt`.
Milestone plan: `docs/milestones/rmap-state-boundaries-v1.md`.

### Rust-only posture — TS parity gap

State-boundary extraction ships Rust-first. The TypeScript extractor
path (`src/adapters/extractors/*`) does NOT emit READS / WRITES
edges with target kind DB_RESOURCE, FS_PATH, STATE, or BLOB. This
is deliberate and documented here rather than papered over with a
stub. CONFIG_KEY is not a state-boundary target in this slice on
either runtime (see state-boundary-contract.txt §4.3, §5.6).

Implications:

- Databases indexed with `rgr repo index` (TS) will not contain
  state-boundary edges; databases indexed with `rmap index` (Rust)
  will.
- Cross-runtime interop (`pnpm run test:interop`) verifies that the
  TS storage adapter tolerates Rust-written databases containing
  the three new `nodes.kind` values. It does NOT verify that TS
  emits equivalent facts.
- Any future product surface that needs state-boundary edges on
  TS-indexed repos must either (a) re-index with `rmap`, or (b)
  port the extractor logic. The canonical direction is (a) for
  now; (b) is a planned future slice only if TS-indexed repos
  must carry these facts.
- Parity harness (`test/fixtures/parity-*`) has no state-boundary
  fixture. A Rust-only parity harness entry (Rust-half only) is
  acceptable; a TS-half stub is NOT acceptable (would misrepresent
  feature availability).

### Deferred items within the state-boundary program

Not in slice 1. Each is its own planned future slice.

- **Queue / event boundaries.** `EMITS`, `CONSUMES`, and the
  `QUEUE` node kind. Kafka, SQS, SNS, RabbitMQ, Redis pub/sub.
- **Config / env seam graph emission.** `CONFIG_KEY` node
  emission, explicit config→resource wiring edges, cross-language
  CONFIG_KEY identity normalization. Slice 1 carries config/env
  provenance in edge evidence only (via `logical_name_source =
  "env_key"`) and does NOT emit CONFIG_KEY nodes. See
  state-boundary-contract.txt §5.6.

### TS-side `importedName` and `kind` population deferred (SB-3-pre, TS-IMPORT-RESOLUTION-1)

`ImportBinding.importedName` and `ImportBinding.kind` (TS interface in
`src/core/ports/extractor.ts`; Rust struct in
`rust/crates/classification/src/types.rs`) are used for import
resolution:

**importedName:** Original exported symbol name for named imports:
- `import { readFile } from "fs"` → `importedName = "readFile"`.
- `import { readFile as rf } from "fs"` → `importedName = "readFile"`, `identifier = "rf"`.
- `import fs from "fs"` (default) → `importedName = null`.
- `import * as fs from "fs"` (namespace) → `importedName = null`.

**kind:** Import kind enum (`NAMED`, `DEFAULT`, `NAMESPACE`):
- `import { X }` or `import { X as Y }` → `kind = NAMED`.
- `import X from "m"` → `kind = DEFAULT`.
- `import * as X from "m"` → `kind = NAMESPACE`.

**Rust populated.** `rust/crates/ts-extractor/src/extractor.rs`
populates both fields correctly for all patterns. The resolver uses
`kind` to distinguish namespace imports (resolvable) from default
imports (conservative).

**TS NOT populated.** All five TS extractors
(`src/adapters/extractors/{typescript,python,rust,java,cpp}/`)
pass `importedName: null` and omit `kind` (defaults to `NAMED`).
This is a deliberate Fork-1 posture: TS-side state-boundary emission
is deferred, so no current TS consumer needs these fields. The
resolver only uses these fields when populated by Rust extractors.

**Parity impact.** Cross-runtime parity harness
(`test/ts-extractor-parity/ts-extractor-parity.test.ts:88-100`)
projects `ImportBinding` to a fixed field set that does NOT
include `importedName` or `kind`. The Rust serde attributes
`#[serde(default)]` keep absent values invisible on the wire.
Net effect: no parity harness impact, no serialization drift.

**Follow-on slice.** When TS-side state-boundary emission is
prioritized (or any other TS consumer needs these fields), ship
a dedicated slice that ports the Rust import-resolution logic
to each TS extractor. Until then, TS consumers of `ImportBinding`
must treat `importedName` as potentially null and `kind` as
potentially absent (defaulting to `NAMED`).

### Refresh stale-orphan resource nodes (SB-4-pre Fix B)

`rmap refresh` (incremental re-index) copies resource nodes
(`file_uid IS NULL`, kinds `DB_RESOURCE` / `FS_PATH` / `BLOB` /
`STATE`) from the parent snapshot unconditionally. If a resource
was referenced ONLY by a file that was subsequently changed or
deleted, the resource node persists as a stale orphan in the
refresh snapshot. It is not reachable from any symbol via
READS/WRITES edges (the old edges belonged to the changed file
and are NOT copy-forwarded), but it occupies the graph as an
unreferenced node.

Workaround: run `rmap index` (full re-index) to produce a clean
graph without stale orphans.

Root cause: copy-forward cannot determine which resource nodes
are still referenced without re-running state-boundary extraction
on ALL files, which defeats the performance benefit of delta
refresh. Correct fix requires either:
- persisting resolved_callsites for delta reuse, or
- a post-copy GC pass that prunes unreferenced resource nodes

Both are deferred to a dedicated delta-aware state-boundary
slice. The stale-orphan behavior is pinned by a test
(`refresh_mixed_unchanged_and_changed_files` in
`repo-index/tests/state_boundary_integration.rs`) so that an
unintentional fix or regression is detected.

### `rmap dead` excludes resource kinds — TS divergence (SB-5)

`rmap dead` (Rust `find_dead_nodes` in `storage/src/queries.rs`)
excludes resource node kinds (FS_PATH, DB_RESOURCE, BLOB, STATE+CACHE)
from the dead-node result set. This is correct behavior: resource
nodes have no inbound static edges by construction (they are targets
of READS/WRITES edges, not sources), so they would appear as mass
false positives without this exclusion.

**TS divergence:** The TypeScript `findDeadNodes` implementation
(`src/adapters/storage/sqlite/sqlite-storage.ts`) does NOT yet have
this exclusion. This creates a cross-runtime query behavior split:

- `rmap dead amodx` excludes resource nodes.
- `rgr graph dead amodx` includes resource nodes (if any exist).

Impact: TS CLI users may see resource nodes in dead results on
databases that contain state-boundary facts (i.e., Rust-indexed DBs
opened via TS CLI). This is a documentation/awareness gap, not a
data-loss bug.

**Fix path:** Either (a) port the exclusion to the TS `findDeadNodes`
query, or (b) accept the divergence and document it in CLI help text.
Option (a) is preferred for query-surface consistency.

### SB-3 binding-table coverage is FS-only

Slice SB-3 ships the state-extractor TS integration plus a
binding table restricted to Node.js filesystem stdlib APIs. The
milestone-listed SDK/DB/Cache modules are NOT in the SB-3 binding
table. Reasons and scope:

**Not in SB-3** (deferred to SB-3-next and beyond):

- AWS SDK S3 (`@aws-sdk/client-s3`): the identifying payload is
  `{Bucket: "..."}` inside a `new PutObjectCommand(...)` argument
  — an object-property pattern. SB-3's arg-0 classifier only
  handles string literals and `process.env.NAME` member reads.
  Object-property extraction is SB-3-next scope.
- Database drivers (`pg`, `mysql2`, `better-sqlite3`, `sqlite3`):
  the resource identity is at client CONSTRUCTION (e.g. `new
  Client({connectionString: ...})`), not at the `.query()`
  call. SB-3 does not track constructor arguments. Constructor
  tracking is SB-3-next-next scope.
- Redis clients (`redis`, `ioredis`): same pattern as DB drivers
  — `createClient({url: ...})` or `new Redis({host, port})` at
  construction. Deferred to constructor-tracking slice.
- FS metadata-only ops (`readdir`, `stat`, `access`, `realpath`,
  etc.): out of slice-1 scope (content touchpoints only).
  Deferred pending a consumer need.
- `fs.open` / `fs.openSync` / `fs.promises.open`: direction is
  flag-dependent (`r`/`w`/`a`/...). Without flag parsing a
  single `direction` would misrepresent the call. Deferred.

**Shipped in SB-3** binding-table coverage:

- `fs`, `node:fs`: `readFile`, `readFileSync`, `writeFile`,
  `writeFileSync`, `appendFile`, `appendFileSync`,
  `createReadStream`, `createWriteStream` (8 symbols × 2
  specifiers = 16 entries).
- `fs/promises`, `node:fs/promises`: `readFile`, `writeFile`,
  `appendFile` (3 symbols × 2 specifiers = 6 entries).

Total: 22 binding entries. Exact module matching; module-
specifier normalization (e.g. `node:fs` → `fs`) is a separate
substrate decision and is NOT part of SB-3.

### `ResolvedCallsite` population is Rust-only (SB-3-pre)

`ExtractionResult.resolvedCallsites` / `resolved_callsites` is
structurally present on BOTH runtimes (TS `src/core/ports/extractor.ts`
and Rust `rust/crates/indexer/src/types.rs` at the
`ExtractionResult` boundary). Z-a (see SB-3-pre locks) kept the
extractor-port contract unified across runtimes.

POPULATION is Rust-only:

- Rust `ts-extractor` populates `resolved_callsites` for call
  expressions that match Form-A resolution and slice-1 arg-0
  patterns.
- Every TS extractor
  (`src/adapters/extractors/{typescript,python,rust,java,cpp}/`)
  returns `resolvedCallsites: []` under the Fork-1 posture.

Port-contract invariant: a consumer reading `ExtractionResult`
sees the same field set regardless of producer runtime. Under
Fork 1, TS-sourced `resolvedCallsites` is uniformly empty; this
is a population gap, not a contract split. When TS-side state-
boundary emission opens, the port already carries the field —
only the TS extractor logic needs to land.

The type is slice-scoped:

- `Arg0Payload` variants `StringLiteral` / `EnvKeyRead` are the
  only slice-1 argument-0 patterns (both runtimes).
- Object-property extraction (e.g., `{Bucket: "x"}`), constructor
  tracking (`new Client({...})`), and argument positions beyond
  0 are intentionally out of scope and will land in their own
  future slices with their own typed additions to `Arg0Payload`
  OR new sibling types.
- The name `Arg0Payload` carries the slice scope in its name
  (not `CallArgumentModel` or `UniversalArgumentShape`) to
  prevent future expansion from silently inheriting a name that
  claims more than it delivers.
- **SQL-string parsing.** Until this lands, DB targets are
  `DB_RESOURCE` (logical-connection granularity). `TABLE` node
  kind remains reserved.
- **ORM / repository / DAO pattern inference.** Prisma, TypeORM,
  JPA/Hibernate, SQLAlchemy, Diesel. Currently out of scope;
  ORM call sites will NOT produce state-boundary edges until a
  dedicated slice.
- **GCP and Azure blob SDKs.** Only AWS S3 SDKs for Node, Java,
  Python, Rust are in the slice-1 binding table.
- **C++ DB, cache, and blob SDKs.** Only `std::filesystem` and
  POSIX `open` / `fopen` ship in C++ for slice 1. Vendor-specific
  C++ libraries are deferred.
- **Type-enrichment-backed matching (Form B).** Receiver-type
  resolution via TS TypeChecker / rust-analyzer / JDT-LS is
  explicitly NOT used for state-boundary matching in slice 1.
  Matcher form A (import-anchored call) only.
- **Dynamic-target emission.** When the target is a parameter of
  unknown provenance or a runtime-composed string, slice 1 emits
  NOTHING (see contract §5.4). A later slice with value tracking
  may fill this gap.
- **Dedicated `rmap state` command.** No resource-node
  enumeration CLI in slice 1. Agents use existing `callees` /
  `callers` with `--edge-types READS,WRITES`.
- **State-boundary reliability axis in trust report.** Current
  trust reliability measures CALLS resolution. State-boundary
  coverage reliability requires a ground-truth corpus; deferred.

### Known gaps discovered during contract design

- **Pre-existing READS / WRITES / EMITS / CONSUMES declared but
  unused.** Before slice 1, these four edge types existed in the
  canonical `EdgeType` enum (both TS `src/core/model/types.ts`
  and Rust `rust/crates/storage/src/types.rs`) and in the CLI
  edge-type filter list, but NO extractor emitted them. Similarly,
  node kinds `STATE`, `TABLE`, `QUEUE`, `CONFIG_KEY` were declared
  but unused. Slice 1 resolves this for READS / WRITES and for
  STATE. CONFIG_KEY / EMITS / CONSUMES / QUEUE remain
  declared-but-unused after slice 1: CONFIG_KEY waits on the
  future config/env seam slice, EMITS / CONSUMES / QUEUE wait on
  the queue-boundary slice. TABLE remains reserved pending SQL
  parsing. This transitional state is not a defect; it documents
  that the canonical vocabulary was designed ahead of
  implementation.

- **Schema column `nodes.kind` and `edges.type` have no CHECK
  constraint or enumerated lookup table on either runtime
  (verified at slice-1 design time).** Adding node kinds or edge
  types therefore requires only enum-value additions in the TS
  and Rust domain models; no SQL migration. This is a schema
  property relied on by this slice and by any future vocabulary
  expansion. If a CHECK constraint is ever introduced, the
  impact on existing and future vocabulary-expansion slices
  must be reassessed.

- **Resource nodes have no inbound static edges by construction.**
  Dead-code reducers MUST exclude resource kinds emitted by
  state-boundary extraction (DB_RESOURCE, FS_PATH, STATE, BLOB)
  to avoid mass false positives. Slice 1 enforces this in the
  `rmap dead` path; future kind-filter additions to dead-code
  must follow the same rule. CONFIG_KEY is outside this slice
  and is handled by the config/env seam slice when it ships.

### CPP-SB-1: C++ State Boundaries — Known Limits

**Local type map limits (D3):** The intra-function local type map
for `.open()` receiver resolution tracks only local variable
declarations in the same function body. Explicit limits:

| Supported | Not Supported |
|-----------|---------------|
| Local variable declarations | Parameters (`void f(ifstream& s)`) |
| Same function body | Cross-function propagation |
| Direct identifier receiver (`file.open()`) | Factory returns (`getStream().open()`) |
| Simple declarations | Aliases (`auto& ref = file`) |
| | References/pointers |
| | Reassignment |
| | Member fields (`this->file_.open()`) |

These are deliberate design limits. Generalized receiver-type
resolution is future substrate work (not part of CPP-SB-1).

**Mode parsing:** Limited to literal `std::ios::*` patterns.
Variable modes or complex expressions default to `read_write`.

**Binding duplication:** 16 bindings total (8 C-style APIs
duplicated for `language = "cpp"` + 8 stream family entries).
If this becomes a maintenance burden, future slice can introduce
binding-table substrate extension (language families, shared
bindings, alias groups).

Validated by E2E tests: `rust/crates/repo-index/tests/cpp_sb_1_integration.rs`
(20 tests covering constructor path, D3 .open(), C-style APIs, negative
limits, and refresh path).

## Measurement Commands (`rmap churn`, `rmap hotspots`)

### Hotspots — validated for v1 (RS-MS-3d)

Signal quality validated on repo-graph (2026-04-21):

- **Head quality (triage-relevant).** Top 20: 19 production files, 1
  test file. Top 50: 45 production files, 5 test files. No
  generated/vendor/build contamination in the head.
- **Ranked production files are correct.** Extractors, storage,
  indexer, CLI orchestration files surface appropriately.
- **Test files in results are not scoring defects.** A high-churn,
  high-complexity test file IS a hotspot by definition. Whether to
  hide them is a view-policy issue, not a correctness fix.

Formula: `hotspot_score = lines_changed × sum_complexity`. No change
required from validation.

### Deferred: `--exclude-tests` filter

Test files appear in hotspot results by design. The `is_test` metadata
IS persisted (checked: 225 files flagged in repo-graph). A
`--exclude-tests` filter is implementable using real metadata, not
heuristics. Deferred as a presentation filter, not a correctness issue.

### Deferred: `--exclude-generated` filter

No `is_generated` metadata is currently populated. Do not add this
filter until "generated" can be defined from real persisted metadata.
Heuristic path-based detection (e.g., `*/generated/*`) is explicitly
rejected as unreliable.

### Coverage import — not operational

**TS coverage import broken.** `rgr graph coverage` exists but format
detection fails on any Istanbul report > 4KB. Bug: `canHandle()` in
`src/adapters/importers/coverage-registry.ts` tries to JSON.parse the
first 4KB of the file, which fails when truncated.

**Rust has no coverage import.** No `rmap coverage` command.

**No coverage measurements exist.** Database contains only complexity
metrics (`cyclomatic_complexity`, `max_nesting_depth`, `parameter_count`,
`function_length`, `cognitive_complexity`).

### RS-MS-4 (`rmap risk`) — blocked

Risk formula: `risk = hotspot_score × (1 - coverage)`.

Without coverage data, risk is either null (no ranking) or degrades to
hotspots (if `coverage = 0`). This is semantic collapse, not graceful
degradation. Coverage is a defining term of risk, not optional garnish.

**Gate decision (2026-04-21):** Do not ship `rmap risk` until Rust has
its own coverage import surface and measurements are validated. TS
coverage repair is not the dependency path — Rust should implement its
own importer.

Prerequisite sequence:
1. RS-MS-4-prereq: Rust coverage import (`rmap coverage <db> <repo> <report>`)
2. Validate coverage measurements exist and match indexed file paths
3. RS-MS-4: `rmap risk` command

### Quality Control Phase A — limitations (2026-04-25)

`function_length` and `cognitive_complexity` measurements are now
computed for TS/JS functions. Known limitations:

- **Recursion penalty deferred.** Cognitive complexity does not add +1
  for recursive calls. Precise detection requires call resolution which
  is incomplete (especially for `this.method()` patterns). Sonar adds
  this penalty. Phase A defers rather than implement partial detection.

- **Early return penalty deferred.** Cognitive complexity does not
  penalize early returns. Sonar adds +1 for `return` statements that
  are not at the end of a function. This requires control-flow analysis
  beyond tree-sitter AST walking.

- **TS/JS only.** Java, Rust, Python, C/C++ extractors return 0 for
  `function_length` and `cognitive_complexity`. Cross-language rollout
  is Phase B scope.

- **No file-level aggregates.** Per-function measurements only. File or
  module aggregates (sum, max, avg) are query-time computation via
  `rmap metrics`, not persisted measurements.

See `docs/architecture/quality-control-phase-a.md` for full spec.

### Quality-Policy Gate Integration — waiver overlay deferred (2026-04-27)

Quality-policy assessments are integrated into the gate outcome. Gate
consumes pre-computed assessments and reduces them into the unified verdict.
Quality assessments are reported separately in `GateReport.quality_assessments`.

**Deferred: waiver overlay for quality policies.**

Quality-policy assessments do NOT participate in the waiver system. A FAIL
assessment with severity=Fail blocks the gate regardless of waiver presence.
The waiver infrastructure exists for requirement-based obligations but has
no quality-policy support.

Resolution path:
1. Define waiver target semantics for quality policies (policy_id? policy_uid?)
2. Extend `GateStorageRead` with quality-waiver fetch
3. Extend `reduce_outcome` with quality waiver overlay
4. Decide whether PASS assessments with waivers stay PASS (Rust-25 semantics)

Priority: Low. No customer demand. Quality policies are comparative
(`no_new`, `no_worsened`) so waivers would typically apply to specific
violations, not to the policy-level verdict.

### Quality Policy Runner — architectural debt (2026-04-26)

The `QualityPolicyStoragePort` trait lives in `repo-graph-storage` instead
of in `repo-graph-quality-policy-runner`. This inverts the Clean Architecture
dependency rule: the application/use-case crate depends on an adapter-owned
abstraction instead of owning its own boundary contract.

**Why this happened:** Circular crate dependency prevention.
- `repo-graph-quality-policy` depends on `repo-graph-storage` (for DTOs)
- `repo-graph-quality-policy-runner` depends on both
- If the port trait lived in `runner`, `storage` would need to depend on `runner`
  to implement it, creating a cycle.

**Consequences:**
- The runner boundary is pinned to storage semantics and cannot evolve independently
- Error types are storage-native (`StorageError`), not use-case-specific
- The port DTOs (`EnrichedMeasurement`, `LoadedPolicy`) are storage-owned

**Correct resolution (deferred):** Create a separate `repo-graph-quality-policy-ports`
crate containing only the port trait and its DTOs. Both `runner` and `storage`
depend on `ports`. This adds one crate but restores the dependency rule.

**Pragmatic acceptance:** The current design works and the coupling is
narrow (3 methods). The debt is documented; resolution can be prioritized
when the port interface needs to evolve or when a second storage backend
appears.

## Rust CLI (`rmap`) — Temporary Gaps

The Rust CLI is the primary binary. These are temporary gaps to be closed,
not intentional design differences.

For intentional contract differences (design decisions), see
`docs/cli/rmap-contracts.md`.

### `dead` command deliberately disabled (2026-04-27)

**Status:** Removed from CLI surface. Command exists but returns exit 2.

**Reason:** Smoke-run validation on 5 real-world codebases (hexmanos,
zap-engine, amodx, glamCRM, zap-squad) showed 85-95% false positive
rates. A misleading "dead" label is worse than no label — it directs
agents toward the wrong investigation frontier.

**Root causes:**
- No Spring framework detector (Java beans appear dead)
- No React entrypoint detection (components appear dead)
- No Rust framework detector (Axum/Actix handlers appear dead)
- No Python framework detector (FastAPI/Django handlers appear dead)
- No entrypoint declarations in any tested repo
- No coverage-backed evidence

**Underlying substrate preserved:**
- `storage::find_dead_nodes()` works
- `trust::assess_dead_confidence()` works
- `DeadNodeOutput` DTO kept for reintroduction
- Tests remain

**Reintroduction plan:** Split into two separate products:

1. **`rmap orphans`** — Pure graph heuristic. No deadness claim.
   "Not currently referenced in the graph we built."
   Useful for orientation, NOT deletion.

2. **`rmap dead`** — Requires stronger evidence:
   - Coverage-backed (executed vs not-executed under measured scenarios), OR
   - Framework-liveness-backed (entrypoint/handler detection mature), OR
   - Explicit entrypoint declarations
   
   Meaning: "Unexecuted AND structurally weakly connected."

**Criteria for reintroduction:**
- Framework entrypoint detection mature for at least one of:
  Spring, React, Axum, FastAPI, OR
- Coverage import surface operational on Rust side, OR
- Entrypoint declaration workflow established and adopted

See `docs/cli/rmap-contracts.md` for contract details.

### Other temporary gaps

- **`--edge-types` on callers/callees:** Accepts CALLS, INSTANTIATES, READS,
  WRITES only. TS accepts all 18 edge types.

- **No `graph metrics` command:** Not ported yet.

- **`path` command:** Symbol-only endpoints, CALLS+IMPORTS fixed, max-depth 8
  fixed. No `--edge-types` or `--max-depth` flags.

- **`imports` command:** One-hop only (no `--depth`), file paths only (no
  module/symbol fallback).

- **`trust` command envelope:** Does not use QueryResult wrapper. Has own
  report shape (matching TS).

- **`index` and `refresh`:** Use stderr for progress, no JSON output.

## Boundary Interaction Extraction — Slice 1A (Local IPC)

Policy crate: `rust/crates/boundary-interaction/`
Extractor crate: `rust/crates/boundary-interaction-extractor/`
Design doc: `docs/design/boundary-interaction-ipc-device.md`

### Multi-binding resolution (resolved 2026-05-04)

`BindingTable::find_by_function(language, function_name)` now returns all
matching entries for a given language and function name. The uniqueness
constraint changed from `(language, function)` to `(language, function, channel_kind)`.

The emitter evaluates candidates in TOML declaration order and uses guard
predicates to select the appropriate binding based on callsite evidence.

**Files changed:**
- `boundary-interaction/src/table.rs`: dedup key, `find_by_function` returns Vec
- `boundary-interaction/bindings.toml`: TCP/UDP entries added after Unix socket
- `boundary-interaction-extractor/src/emit.rs`: multi-candidate evaluation

## Boundary Interaction Extraction — BI-1B (TCP/UDP Sockets)

### fd→family/type context propagation — IMPLEMENTED (Phase 2)

**Status:** RESOLVED (2026-05-12). Intra-function fd tracking shipped.

**What shipped (BI-1B Phase 2):**
- C extractor emits `assigned_identifier` for socket() LHS, `fd_argument` for
  bind/listen/connect/accept arg0
- `socket_lineage.rs`: FdRegistry (identifier → socket family), RoleEvidence
  (bind/listen/connect/accept flags), TrackedChannelKind enum
- Compose phase: function-grouped processing with FdRegistry per function
- Role detection state machine: TCP server (bind+listen) → Provider, TCP client
  (connect) → Consumer, UDP → Bidirectional (no strong role semantics)
- D3: bind alone insufficient for provider classification
- `update_surface_direction()` for direction refinement at function boundary

**Explicit limits (Phase 2):**
- C only (no C++ in this slice)
- Function-local fd tracking only (cleared at function boundary)
- Direct declarations: `int fd = socket(...)`
- Direct identifier use in bind/listen/connect/accept
- Does NOT track: cross-function propagation, aliases, parameters, globals

**Remaining deferred items (Phase 3+):**
- C++ socket wrapper support (separate slice)
- Cross-function fd propagation (requires dataflow)
- Endpoint extraction (host:port from bind/connect arguments)
- Scope classification (inter_process vs inter_device)
- UDP role semantics (if ever needed)

**Validation:** 96 C extractor tests, 14 socket_lineage tests, 45 boundary-interaction-extractor
tests, 7 TCP/UDP E2E integration tests. See `docs/shipped/slices/bi-1b-tcp-udp-sockets.md`.

### Extractor contract: conservative field population

The C-extractor integration contract (established 2026-04-30):

- **`socket_family`**: Only when confidently derived from `socket()` arg0 or
  equivalent tracked context. Do not guess.
- **`mmap_flags`**: Only when `MAP_SHARED` vs `MAP_PRIVATE` can be determined
  from arg4. Unresolved expressions → `None`.
- **`mknod_mode`**: Only when mode bits are actually extracted from arg2.
  Unresolved expressions → `None`.
- **`extracted_argument`**: Only when normalized channel identity is known.
  **Critical for `bind`/`connect`**: Only populate when the argument is a
  Unix socket path (`sockaddr_un`), not generic IP:port text or unresolved
  sockaddr rendering. The emitter accepts `bind`/`connect` without
  `socket_family` if `extracted_argument.is_some()` — this is safe ONLY if
  `extracted_argument` means Unix socket path evidence.

If the extractor cannot prove a field's value, it MUST leave the field unset
(`None`) and let the emitter decline to emit. No generic argument text
masquerading as channel identity.

## Boundary Interaction Extraction — MB-1A (RabbitMQ/AMQP)

### Current state

- **amqplib only:** Direct amqplib API usage in TS/JS. Framework wrappers
  (NestJS @RabbitSubscriber, TypeORM, etc.) not detected.
- **Import presence guard:** File must have direct `amqplib` import/require to
  emit any AMQP surfaces. Generic `.publish()/.consume()/.assertQueue()` on
  non-amqplib objects are NOT detected. This prevents false positives in
  codebases using generic method names.
- **No Spring AMQP:** @RabbitListener, RabbitTemplate not detected.
- **No Python pika:** pika patterns not detected.
- **No Go amqp:** streadway/amqp and rabbitmq/amqp091-go not detected.

### Known limitations

- **boundaryScope always "unknown":** Broker topology (single-node vs cluster,
  local vs remote) cannot be inferred from code. Would require config analysis.
- **interactionPattern always "fire_and_forget":** RPC pattern (correlationId,
  replyTo) not yet detected. The rpc_client.js and rpc_server.js fixtures use
  RPC pattern but are not distinguished from simple pub/sub.
- **Queue/exchange names not extracted as channel identity:** The `assertQueue`
  and `assertExchange` calls contain queue/exchange names, but these are not
  yet extracted as channel identity (would enable cross-repo linking).
- **No fanout/direct/topic exchange type differentiation:** Exchange type is
  in the assertExchange args but not extracted or surfaced.

### Deferred to MB-1B/1C

- Spring AMQP detection (Java)
- Python pika detection
- Go amqp detection
- Queue/exchange name extraction as channel identity
- RPC vs pub/sub pattern detection
- Exchange type surfacing

## Boundary Interaction Extraction — MB-2A (Kafka)

### Current state

- **kafkajs only:** Direct kafkajs API usage in TS/JS. Framework wrappers
  (NestJS, TypeORM, etc.) not detected.
- **Scope guards (triple):**
  1. Import presence guard: File must have direct `kafkajs` import/require
  2. Receiver provenance guard: Receiver must be assigned from `*.producer()`
     or `*.consumer()` factory call in same file
  3. Topic evidence guard: Call must have extractable topic argument
- **Detected patterns:** `send({ topic, ... })`, `subscribe({ topic })`,
  `subscribe({ topics: [...] })`.
- **Intentionally NOT detected:** `consumer.run({ eachMessage, eachBatch })`.
  This call provides no topic evidence and would overclaim consumer surfaces.
  Correlation with subscribe() is deferred to future work.
- **No Java kafka-clients:** kafka-clients patterns not detected.
- **No Spring Kafka:** @KafkaListener, KafkaTemplate not detected.
- **No Python kafka:** confluent-kafka, aiokafka not detected.

### Receiver provenance tracking scope (deliberately narrow)

The receiver provenance guard tracks same-file, direct factory assignments:
- `const producer = kafka.producer()` → tracks `producer` as valid producer receiver
- `const consumer = kafka.consumer({...})` → tracks `consumer` as valid consumer receiver

What is NOT tracked (by design):
- Cross-file receivers (receiver assigned in another file)
- Wrapper functions (function that returns producer/consumer)
- Object property assignments (`this.producer = kafka.producer()`)
- Destructured assignments
- Alias chains (`const p = producer; p.send(...)`)

This keeps detection local and deterministic. Broader tracking would require
interprocedural dataflow analysis, which is out of scope for syntax-only extraction.

### Known limitations

- **boundaryScope always "unknown":** Broker topology cannot be inferred from
  code. Would require config analysis.
- **Topic names extracted but not linked:** Topics are captured in evidence but
  producer/consumer pairing across files is not performed.
- **Consumer group ID not yet extracted:** groupId is visible in consumer config
  but not yet surfaced in evidence.
- **Partition/offset semantics not modeled:** Consumer position, rebalance,
  commit patterns are out of scope.
- **run() not correlated with subscribe():** A file with both subscribe() and
  run() emits only the subscribe() surface. Future work could enrich subscribe()
  surfaces with callback evidence from associated run() calls.
- **Cross-file receivers not tracked:** If producer/consumer is assigned in one
  file and used in another, detection fails. This is intentional — cross-file
  dataflow is out of scope for syntax-only extraction.
- **sendBatch intentionally deferred:** The kafkajs `sendBatch` API uses
  `{ topicMessages: [{ topic, messages }, ...] }` structure where topics are
  nested inside an array. Current extraction only looks for `topic` at the top
  level of the first object argument. `sendBatch` is intentionally excluded from
  detection until nested topic extraction is implemented. No binding exists.

### Deferred to MB-2B/2C

- Java kafka-clients detection
- Spring Kafka detection (@KafkaListener, KafkaTemplate)
- Python Kafka clients (confluent-kafka, aiokafka)
- Topic-based producer/consumer linking
- Consumer group ID extraction
- Partition assignment patterns
- run() correlation: enrich subscribe() surfaces with callback mode evidence
- Cross-file receiver tracking (would require dataflow analysis)

## rgistr Policy Hints (2026-04-28)

### What shipped

rgistr MAP generation now includes two advisory sections:

- **Policy Signals (file-level):** Status/error translation functions, retry loops,
  default policy constants, orchestration loops, result-fate patterns.
- **Policy Seams (folder-level):** Aggregated policy signals from child files
  identifying cross-layer policy propagation points.

These sections are LLM-generated advisory hints, NOT deterministic extraction.

### What this is NOT

This is discovery compression for agent orientation. It does NOT provide:

- Deterministic extraction guarantees
- Source-anchor provenance (line numbers, AST nodes)
- Programmatic queryability (not in SQLite, not structured)
- Cross-file policy flow tracing
- Contract-level reproducibility

### Next slice: policy-facts support module

The policy-hints surface reveals the gap but does not close it. A deterministic
policy-facts support module for `rmap` requires:

1. AST-anchored extraction of status translation patterns
2. Control-flow analysis for retry/restart behavior
3. Return-fate tracking (ignored, propagated, transformed)
4. Default-provenance extraction from config parsing
5. Cross-layer edge materialization in the graph

Design doc: `docs/design/policy-facts-support-module.md`.

### Validation evidence

Policy Signals verified on swupdate/corelib regeneration:
- `server_utils_c_MAP.md`: "map_channel_retcode contains status/error translation
  functions mapping channel_op_res_t codes to server_op_res_t codes"
- `channel_curl_c_MAP.md`: "channel_get_file() contains retry loop with sleep and
  resume" and default policy constants identified

The hints surface the correct architectural seams. They do not provide the
provenance or queryability that a support module would deliver.

## rgistr Productization Gaps (2026-05-05)

Current `tools/rgistr` gaps recorded from source inspection:

- large files are skipped rather than chunked
- whole-file vs digest choice is based on byte size, not token budget
- no per-chunk artifact contract exists
- no backend discovery layer exists
- no MLX support exists
- no llama.cpp support exists
- OpenAI-compatible local backends are not unified
- model capability metadata is not explicit
- generation preflight does not print a discovery/selection report

Planned closure path:
- `docs/design/rgistr-productization-plan.md`

Debt classification:
- productization debt
- support-module gap
- contract gap

Non-negotiable closure rule:
- never skip source files due to size; chunk and synthesize instead

## rgistr Repo-Context Classification (2026-05-07)

### Implemented

New support module at `tools/rgistr/src/support/repo-context/`:

- `types.ts`: RepoContextClass, RepoContextHint, RepoProfile types
- `repoProfile.ts`: Derives coarse repo type from manifests and structure
- `folderClassify.ts`: Deterministic folder-role classification using weighted evidence
- `index.ts`: Exports
- `repo-context.test.ts`: 22 unit tests

Classification taxonomy:
- `product_code`, `test_support`, `artifact_storage`, `fixture_storage`
- `external_code_fixtures`, `validation_corpus`, `unknown`

Evidence sources:
1. Path taxonomy (smoke*, fixtures*, vendor*, etc.)
2. Repo-type mismatch (Linux drivers in code-analysis repo)
3. Copied source tree patterns
4. Artifact-shape signals

Integration:
- RepoProfile built once at generation start
- RepoContextHint computed per-folder before prompt generation
- Hint injected into folder prompt immediately after FOLDER path
- LLM uses hint as strong prior for Folder Role section

### Validated behavior

- `smoke-runs/linux-inter-core-subset`: Now `validation_corpus` (was `driver/hardware-facing`)
- `tools/rgistr/src/support`: Still `Orchestration/control` (product code preserved)

### Remaining limitations

- Artifact-shape signals (hasSmokeProtocolFiles, hasReportFiles) are defined in
  types but not yet used in classification. Classification relies on path taxonomy
  and repo-type mismatch only.
- No manifest inspection beyond root Cargo.toml / package.json. Deep workspace
  members not queried.
- Confidence calibration is heuristic. No corpus validation of thresholds.
- RepoType derivation uses simple keyword matching. Could miss obscure domains.

## Contract Schema Extraction (CS-1)

### Current state

- Parser: `contract-schema` crate with tree-sitter-proto parsing (proto2 + proto3)
- Storage: `contract_schemas` + `contract_elements` tables, `ProtoSchemaStorePort`
- Scanner: admits both source and contract extensions (`is_source_extension() || is_contract_extension()`)
- Orchestrator: Dual-pipeline architecture wired (`index_repo`/`refresh_repo`
  accept `contract_files` parameter, run proto indexing under shared snapshot
  lifecycle)
- File inventory: Contract files tracked in `tracked_files`/`file_versions`,
  included in `files_total` and `all_file_paths`
- Parse status: File versions reflect actual parse outcome (`Parsed` vs `Failed`)
  based on `parse_failures` from proto indexer
- Compose layer: `prepare_repo_inputs()` partitions files via
  `routing::is_contract_extension()`, passes contract files to orchestrator
- Failure semantics: `ContractIndexResult.storage_error` surfaces all storage-level
  failures explicitly (schema storage + file catalog writes)

### CLI surface

`rmap contracts list/show/elements` commands wired in `commands/contracts.rs`.
Read-side queries via `ContractSchemaStoragePort`.

### Validation needed

The full pipeline is wired but not yet smoke-tested on real repos with `.proto`
files. Need to run `rmap index` on a repo with protos and verify:
- `contract_schemas` table populated
- `contract_elements` table populated
- `tracked_files` contains `.proto` entries
- `IndexResult.contracts` reports correct counts

See `docs/slices/cs-1-protobuf-schema.md` for full spec.

## Generated Code Mapping (CS-2A)

### Current state

- Mapper: `java_code_mapper` module matches Java generated protobuf/gRPC classes
  to schema elements via java_package/java_outer_classname options and naming conventions
- Storage: `generated_code_mappings` table with confidence tiers and mapping basis
- Integration: Runs after contract indexing + source extraction in orchestrator
- CLI: `rmap contracts usages` command surfaces mappings with filters

### Delta refresh limitation

**Refresh mapping test deferred:** The adapter-level test for mapping summary on
`rmap refresh` is deferred because delta indexing may not preserve Java symbol
metadata in all cases.

- **Root cause:** Delta refresh only re-extracts changed files. If Java generated
  files are unchanged but proto files changed (or vice versa), the symbol query
  for mapping may return incomplete results.
- **Impact:** Index path works correctly; refresh path may undercount mappings
  when symbol metadata from unchanged files is not fully preserved.
- **Mitigation:** The `index` path (full indexing) is the primary surface and is
  fully tested. Use `rmap index` rather than `rmap refresh` when mapping accuracy
  is critical.
- **Fix path:** Delta indexing needs to copy forward Java symbol metadata for
  unchanged files, or the mapper needs to query symbols from both current
  extraction and copied-forward data.

See `docs/slices/cs-2a-java-generated-code-mapping.md` for full spec.

## gRPC Implementation Hints (GR-1A)

### Current state

- Detector: `grpc_impl_hint` module finds Java classes extending `*Grpc.*ImplBase`
  and links them to proto services via CS-2A mappings
- Storage: `boundary_interaction_surfaces` (hints) + `boundary_contracts` (associations)
- Integration: Runs after CS-2A in orchestrator; gated on `mappings_persisted > 0`
- Explicit degradation: `GrpcImplHintResult` in `IndexResult`
- CLI visibility: Full contract info exposed in `rmap boundaries list/show`
  - List: `contract_name`, `contract_kind` fields
  - Show: `contracts` array with full association details
- Test coverage: 4 storage unit tests + 2 CLI integration tests

### Pending

1. **Smoke validation**: Real gRPC repo test (grpc-java examples or similar)
2. **CLI summary**: `rmap index`/`rmap refresh` stderr summaries do not include GR-1A counts.
   `GrpcImplHintResult` is in library-level `IndexResult` but not surfaced in CLI output.

See `docs/slices/gr-1a-java-grpc-server.md` for full spec.

## rgistr Artifact Frontmatter Inconsistency

### Current state

Chunked file generation uses the chunking support module's `serializeFileArtifact()`
with its own frontmatter schema, while non-chunked files use `writeMap()` with
the existing schema.

Chunked file artifact frontmatter:
- `scope: file`
- `source_file`
- `file_hash`
- `synthesis_mode: chunk_rollup`
- `chunk_basis: [...]`
- `uncertainty_notes: [...]`
- `generated_at`, `generator`, `model`, `provider`

Non-chunked file artifact frontmatter:
- `scope: file`
- `path` (vs `source_file`)
- `basis_commit`
- `adapter`, `model`
- `synthesis_basis`
- `confidence`
- `source_filename`

### Impact

- Both produce valid markdown artifacts
- Both are readable by existing MAP consumers
- Schema divergence may complicate future tooling that parses frontmatter

### Resolution path

When rgistr reaches MATURE status, unify the frontmatter contracts:
- Either migrate `writeMap()` to use chunking module's schema
- Or extend chunking module to match existing schema
- Update documentation and any downstream consumers

Low priority - the additive approach was the correct business decision for now.

## rgistr CLI-level generate command coverage gap

### Current state

`tools/rgistr/src/cli.test.ts` covers `discover`, `--help`, and `--version`.
The `generate` command has no CLI-level tests.

Generator business logic is tested through the adapter seam in
`tools/rgistr/src/core/generator.test.ts` (7 tests covering routing,
chunking, freshness, artifacts).

### Gap

No regression test proves:
- CLI forwards `maxFileSize: Number.MAX_SAFE_INTEGER` to scanner
- Oversized file reaches chunked path from actual CLI entrypoint

### Why deferred

`generate` requires an LLM adapter. CLI-level testing would require:
- Mock adapter injection at commander level
- Or test mode flag that uses mock adapter

The generator tests cover the actual business logic. The CLI is thin wiring.

### Resolution path

When rgistr reaches MATURE status:
- Add `--test-mode` flag that uses mock adapter
- Or refactor CLI to accept adapter factory for testability
- Add CLI-level integration test for oversized file path

## Daemon — Progress abort checkpoint granularity

### Current state (D5b)

Progress callbacks are now abort checkpoints. Transport failure during
long-running operations triggers `ControlFlow::Break`, propagating
`IndexError::Aborted` / `ComposeError::Aborted` back to the daemon.

Abort checkpoint placement:
- Compose layer: before `ensure_repo`, before each persist step
- Indexer `index_repo`: before `create_snapshot`, before each file extraction, before resolving, before persisting
- Indexer `refresh_repo`: before `create_snapshot`, before `copy_forward_unchanged_files`, before `upsert_files`, then same as full index

### Residual limitation

Abort is **checkpoint-granular**, not instruction-granular.

Between two checkpoints, multiple storage writes may occur. If transport
fails after checkpoint N but before checkpoint N+1, any writes that
completed between those checkpoints are persisted.

Example: during the per-file extraction loop, each file has a checkpoint
before extraction. Within file extraction, tracked_files and file_versions
are accumulated in memory, then batch-written after the loop completes.
If abort happens mid-loop, files extracted before the abort checkpoint
are accumulated but not yet persisted (good). However, after the loop,
if abort happens between `upsert_files` and `insert_nodes`, files are
persisted but nodes are not (partial state).

### Why acceptable

1. Snapshot status transitions to FAILED on any pipeline error, including
   abort. Partial state is tagged as failed, not used for queries.
2. The dominant mutation block (extraction loop) is now interruptible at
   per-file granularity, which is the most important checkpoint.
3. Making every storage call an abort checkpoint would add significant
   overhead for marginal benefit.

### Future improvement path

If finer granularity is required:
1. Wrap entire `run_pipeline` in a database transaction with rollback on abort
2. Add checkpoints between batch storage writes (more overhead, more protection)
3. SQLite savepoints for partial rollback within a transaction

Current checkpoint granularity is sufficient for the daemon's transport
failure recovery use case.

---

## Refresh Integrity Parity — Partial Implementation

**Added:** 2026-05-08
**Slice:** `docs/slices/refresh-integrity-parity.md`
**Status:** IN PROGRESS

### What's Done

- Refresh context wiring: `IndexResult` carries `parent_snapshot_uid`, `unchanged_files`
- Copy-forward for: measurements, inferences, boundary surfaces/channels
- Contract schemas: re-indexed (not copied forward) with fresh UIDs per snapshot
- Changed-file filtering for boundary/policy postpass extraction
- Bug fix: boundary insertion PK collision (fresh UUIDs per snapshot)
- Bug fix: contract schema UID collision (fresh UUIDs per snapshot)
- Copy-forward diagnostics surfaced: `ArtifactCopyForward` struct in `IndexResult`, CLI shows counts
- Config-file invalidation widening: scanner now includes config files, widening triggers correctly
- Boundary + contract parity integration tests (6 tests in refresh.rs)

### What Remains

1. **Project surfaces family copy-forward** — BLOCKED on Rust module parity.
   The Rust indexer does not populate `project_surfaces`, `module_candidates`, or
   related tables. These are TypeScript-only features. Copy-forward blocked until
   `rust-module-parity.md` slice is complete.
   - `project_surfaces`
   - `project_surface_evidence`
   - `surface_entrypoints`
   - `surface_config_roots`
   - `surface_env_dependencies`
   - `surface_env_evidence`
   - `surface_fs_mutations`
   - `surface_fs_mutation_evidence`

2. **Boundary interaction links regeneration** — BLOCKED on architectural issue.
   GR-3A (`run_grpc_link_detection`) joins:
   - `boundary_interaction_surfaces` (copied with new UIDs)
   - `boundary_contracts` (NOT copied — links surface_uid to contract_element_uid)
   - `contract_elements` (re-indexed every refresh with new UIDs)

   **Problem:** `boundary_contracts` uses two per-snapshot UIDs as FKs. Neither is
   stable across refresh. Even copying boundary_contracts wouldn't help because
   contract_element_uid changes when contracts are re-indexed.

   **Fix required:** Re-run GR-1A/GR-2A/GR-3A AFTER copy-forward, not before. Requires
   moving gRPC detection chain from `orchestrator::refresh_repo` to `compose.rs`.

3. **End-to-end agent parity validation** — Integration tests added for boundary and
   contract parity (storage/query level). Full semantic parity not verified for CLI output
   normalization: `rmap boundaries`, `rmap contracts`, `rmap surfaces`, `rmap orient`,
   `rmap check`. Slice acceptance requires normalized command output comparison.

### Impact

- `rmap refresh` now preserves boundaries, contracts, measurements, inferences for unchanged files
- `rmap refresh` does NOT preserve project surfaces or boundary links
- Config changes (Cargo.toml, package.json, etc.) NOW trigger invalidation widening
- Agent discovery commands may return incomplete results on refresh snapshots

### Fix Path

Complete remaining items per `docs/slices/refresh-integrity-parity.md` Task Sets A3, A4, Phase 3, and full test matrix validation.

---

## ACR-2 Architecture-Carried Deferrals

**Added:** 2026-05-08
**Slice:** `docs/slices/acr-2-refresh-policy-integration.md`
**Status:** INTENTIONAL DEFERRALS (not accidental omissions)

### Context

ACR-2 established contract-driven dispatch for refresh operations. The following
items are explicitly deferred to later slices because they require scaffolding
that does not yet exist.

### 1. ContractSchemas / ContractElements Full Reindex

**Current behavior:** Refresh re-indexes all `.proto` files every time.

**Contract says:** `ReextractChangedInputs` (copy-forward unchanged, re-extract changed).

**Why deferred:** Proto refresh has no provenance/freshness scaffolding. Full reindex
is the safest honest behavior until ACR-3/5 provides the infrastructure.

**Deferred to:** ACR-3 (schema) / ACR-5 (proof case)

**Location:** `indexer/src/orchestrator.rs` at proto indexer invocation

### ~~2. Inferences Copy-Forward Instead of MarkImpactedDeferRecompute~~ PARTIALLY ADDRESSED (ACR-4)

**Previous behavior:** Unchanged inferences were copied forward without provenance or freshness tracking.

**Current behavior (ACR-4):**
- Spring liveness inferences populate `provenance_json` with target node dependency
- Copy-forward preserves `provenance_json` and `freshness_state` columns
- Impact propagation marks inferences as `impacted` when their provenance references changed L0 keys
- Changed file inferences are regenerated fresh (not copy-forwarded)

**Remaining limitation:** Spring liveness provenance is self-referential (depends on target node).
True `MarkImpactedDeferRecompute` requires cross-file provenance (e.g., inference for ClassB
depending on InterfaceA). When InterfaceA changes, ClassB's inference should be marked impacted.
This requires inference producers that track cross-file dependencies.

**Location:** `repo-index/src/compose.rs` at `persist_spring_liveness_inferences()`

### ~~3. Per-Row Freshness/Provenance Not Wired~~ PARTIALLY ADDRESSED (ACR-3/4)

**Previous state:** No freshness or provenance tracking for derived artifacts.

**Current state (ACR-3/4):**
- Schema: `freshness_state`, `freshness_updated_at`, `provenance_json` columns on 12 tables
- Storage port: `FreshnessStoragePort` with full CRUD for freshness/provenance
- First adopter: Spring liveness inferences populate provenance and track freshness
- Copy-forward: Preserves freshness_state and provenance_json for unchanged files

**Remaining work:**
- Other inference producers (framework entrypoints, Lambda detection) don't populate provenance yet

### Fix Path

1. ~~ACR-3: Add freshness/provenance schema, migration, storage methods~~ DONE
2. ~~ACR-4: Implement impact propagation from L0 changes~~ DONE
3. ~~ACR-5: Complete boundary contract proof case with full integrity story~~ DONE
4. Extend provenance adoption to other inference producers

## ACR-5 — Boundary Contract Proof Case

**Slice:** `docs/slices/acr-5-boundary-contract-proof.md`
**Status:** DONE

### Summary

Full end-to-end provenance and freshness tracking for the boundary contract family.

### Completed Work

1. **Typed provenance in port structs:**
   - `GrpcImplContractInput`, `GrpcClientContractInput`, `BoundaryInteractionLinkInput` have `provenance: Option<Provenance>`
   - Storage derives `freshness_state` from provenance presence (`current` when present, `unknown` when absent)

2. **Provenance computation in GR chain code:**
   - GR-1A, GR-2A, GR-3A compute provenance using stable key pattern
   - Stable key: `{repo}:{proto_file}#{element_kind}:{full_name}`

3. **End-to-end GR chain provenance tests:**
   - `acr5_gr1a_populates_provenance_and_freshness` (grpc_impl_hint_port_impl.rs)
   - `acr5_gr2a_populates_provenance_and_freshness` (grpc_impl_hint_port_impl.rs)
   - `acr5_gr3a_populates_provenance_and_freshness` (grpc_impl_hint_port_impl.rs)

4. **Storage semantics tests (raw SQL inserts):**
   - `acr5_boundary_contract_with_provenance_is_current`
   - `acr5_boundary_contract_without_provenance_is_unknown`
   - `acr5_boundary_interaction_link_with_provenance_is_current`
   - `acr5_no_fk_leakage_on_snapshot_deletion`
   - `acr5_impact_propagation_on_surviving_boundary_links`

5. **`boundary_contracts` FK-join impact propagation:**
   - `build_mark_impacted_sql()` in `freshness_impl.rs` generates table-specific SQL
   - `boundary_contracts` joins through `surface_uid` → `boundary_interaction_surfaces.snapshot_uid`
   - Proof tests:
     - `acr5_boundary_contracts_fk_join_impact_propagation` — verifies impact works via FK-join
     - `acr5_boundary_contracts_fk_join_respects_snapshot_scope` — verifies snapshot isolation
     - `acr5_boundary_contracts_no_provenance_not_impacted` — verifies unknown baseline

## ACR-3 — Freshness and Provenance Schema

**Slice:** `docs/slices/acr-3-provenance-and-freshness-schema.md`
**Status:** DONE (schema + storage port complete; parity blocked on TS migration)

### ~~Scaffolding Limitation: String-Based Provenance Matching~~ FIXED (ACR-4)

**Location:** `rust/crates/storage/src/freshness_impl.rs`, `mark_impacted_by_stable_keys()`

**Fixed in ACR-4:** Now uses SQLite's structured JSON functions (`json_each()`,
`json_extract()`) for proper provenance matching. Regression test
`mark_impacted_does_not_false_match_prefix` validates exact matching.

**Previous behavior (ACR-3 scaffolding):** Used `LIKE '%"stable_key":"<key>"%'`
pattern matching which could false-match substrings.

**Current behavior:**
```sql
UPDATE {table} SET freshness_state = 'impacted', freshness_updated_at = ?
WHERE snapshot_uid = ?
  AND freshness_state != 'impacted'
  AND provenance_json IS NOT NULL
  AND EXISTS (
    SELECT 1 FROM json_each(provenance_json, '$.depends_on')
    WHERE json_extract(value, '$.stable_key') = ?
  )
```

### Carry-over: TS Parity Blocked

**Current state:** Rust has migration 027; TypeScript fixtures do not.

**Consequence:** Parity test (`cargo test -p repo-graph-storage -- parity`)
fails until TS implements the corresponding migration.

**Not a defect:** Deliberate asymmetry during Rust-led schema evolution.
TS parity catch-up is tracked but not blocking.

## DEP-1 — Dependency Reconciliation Surface

**Slice:** `docs/shipped/slices/dep-1-dependency-reconciliation-surface.md`
**Status:** SHIPPED (2026-05-11)

### Resolved Issue (2026-05-11)

Upstream signal pollution resolved via Option B: callee identifiers are resolved to import
specifiers using `file_signals.import_bindings_json`.

**Implementation:**
- `resolve.rs` in module-queries: `resolve_import_specifier()` resolves callee identifiers
- Storage: `get_external_import_bindings_for_snapshot()` loads import bindings
- compose.rs and deps.rs both use resolution before reconciliation/filtering

**Validation:**
- `deps list` → `react` is `declared_and_used` with `import_count: 2`
- `deps why react` → shows both `useState` and `React.createElement` usages
- `deps drift` → correctly identifies `react-dom` as unused

### Known Limitations

1. **Manifest path derived from convention, not storage:**
   `package_dependencies_json` is stored on source files (per-file dependency
   context), not manifest files. DEP-1 derives `manifest_path` from
   `canonical_root_path` + ecosystem convention (npm → package.json,
   cargo → Cargo.toml). If a manifest is not at the conventional path, the
   derived path will be incorrect.

2. **Workspace hoisting not implemented:**
   Dependencies attributed only to manifest-owning module. Root workspace deps
   are not visible to child packages. Documented in slice as deferred to DEP-1B.

3. **Python/Java manifest context not attached:**
   `compose.rs` language dispatch only handles Cargo.toml and package.json.
   Python (pyproject.toml) and Java (build.gradle) files return
   `manifest_scope_unavailable`. Documented in slice as deferred.

4. **Empty modules excluded:**
   Modules with no imports AND no declared deps are silently excluded from
   `deps list` output (vanish mode). No diagnostic emitted.

## FD-1A — Express Detector Implementation

### Current state

AST-based Express route detection for TypeScript/JavaScript files. Persists Layer 3
surfaces (`http_provider`) via compose-phase postpass.

Validation: 16 routes detected from corpus. E2E integration tests pass.

### Known Limitations

1. **Parity with TS prototype: VALIDATED (2026-05-12)**
   
   Comparison executed via `fd-1a-parity-validation.md`. Results:
   - 15 of 17 routes match exactly
   - Rust includes USE middleware mounts (TS excludes) — acceptable enhancement
   - Rust skips dynamic template literals (TS strips and keeps partial) — higher precision
   
   See `docs/slices/fd-1a-parity-report.md` for full comparison.

2. **Handler symbol attribution not implemented:**
   Routes are detected but not linked to their handler functions.
   Documented in slice as FD-1A-4 (deferred).

3. **Router mount composition not implemented:**
   `app.use('/api', router)` prefix propagation is not modeled.
   Documented in slice as FD-1A-2 (deferred).

## FD-1B — React Detector Implementation

### Current state

AST-based React component and hook detection for TSX/JSX files. Persists Layer 3
inferences (`react_component`, `react_hook_usage`) via compose-phase postpass.

Validation: 10 components, 14 hooks from corpus. E2E integration tests pass.

### Known Limitations

1. **Extension coverage: FIXED (2026-05-12)**
   
   - **Hook detection:** Now covers full JS/TS family (`.ts`, `.js`, `.mts`, `.cts`, `.mjs`, `.cjs`)
   - **Component detection:** Still TSX/JSX only (requires JSX syntax)
   - **JSX pragma support:** Still not implemented (`.ts`/`.js` files with JSX pragma not detected)
   
   See `docs/slices/fd-support-ext-jsts.md` and `docs/slices/fd-1b-ext-react-extension-widening.md`.

2. **CLI regression tests for `rmap inferences list`: FIXED (2026-05-12)**
   
   Added `rust/crates/rgr/tests/inferences_command.rs` with 6 test cases:
   - usage error, missing DB, repo not found, empty result, kind filter, output structure
   
   See `docs/slices/fd-support-3-inferences-cli-regression.md`.

3. **Class components not detected:**
   `extends React.Component` pattern is out of scope for first cut.
   Documented in slice as deferred.

4. **Component props not analyzed:**
   Detection reports component existence but does not extract prop types or defaults.
   Documented in slice as deferred.

## Distribution / Install / Host Integration

### REL-1 Implementation (2026-05-12)

**Binary architecture decision:** Separate `rmap` (CLI) and `rmapd` (daemon) binaries.

~~**Active blocker:** The `rmapd` binary target does not exist.~~ **RESOLVED (2026-05-12).**
RMAPD-1 completed. REL-1 is IMPLEMENTED. v0.1.0 tag pushed 2026-05-13 (draft release created).

**CI parity test exclusion:**

`.github/workflows/ci.yml` excludes the storage parity test (`-- --skip parity`) because
fixtures need regeneration after migration 027. Remove this exclusion once fixtures are updated.

~~**{OWNER} placeholder in release artifacts:**~~ **FIXED (2026-05-13).**

Updated to use `andreirx`:
- `scripts/install.sh` — `REPO_OWNER="${RMAP_REPO_OWNER:-andreirx}"`
- `docs/slices/rel-1-release-pipeline.md` — documentation URLs

**Remaining placeholder:**
- `docs/slices/linux-1-linux-installer.md` — systemd unit Documentation field (update when LINUX-1 activates)

**Stub implementations in bootstrap installer:**

REL-1 provides a **bootstrap installer** that installs binaries but defers:

1. ~~**Daemon service setup** (`scripts/install.sh`):~~ **UNBLOCKED (2026-05-15).**
   - MAC-1: launchd service installation implemented
   - LINUX-1: systemd user service installation implemented
   - ~~BLOCKED by RMAPD-2~~ **RESOLVED:** daemon now uses Unix socket transport
   - Ready for validation against new socket daemon contract

2. ~~**Host integration detection** (`scripts/install.sh`):~~ **RESOLVED (2026-05-14).**
   - CLAUDE-1: Claude Code integration implemented
   - CODEX-1: Codex CLI integration implemented
   - `rmap integrate claude-code` and `rmap integrate codex` commands operational

3. **Missing standard files for release archives:**
   - `LICENSE` — not created (choose license before release)
   - `CHANGELOG.md` — not created (use conventional commits or manual changelog)
   - Install script handles missing files gracefully (copies only what exists)

### RMAPD-2: Daemon Transport/Model Mismatch (2026-05-14)

**STATUS: IMPLEMENTED (2026-05-15)**

**Original issue:** Linux validation (v0.1.2) exposed stdio vs socket mismatch.

**Resolution:**
- Unix socket transport implemented as default mode
- `--stdio` retained for debug/test mode only
- Daemon is now a true resident process
- Auto-load repo into daemon registry after successful `index` (enables immediate `refresh`)

**Verified (real-machine, daemon running):**
- `cargo test -p repo-graph-daemon-transport`: 39 passed
- `cargo test -p repo-graph-rgr --lib`: 123 passed
- `cargo test -p repo-graph-rgr --test index_contract_summary -- --ignored`: 9 passed
- `cargo test -p repo-graph-rgr --test daemon_integration`: 6 passed

**Command-path adoption complete:**
- `index` and `refresh` are daemon-required operations
- `stats` and other read-only commands use `execute_or_fallback` pattern
- Daemon responses include summary data (contracts, generated_code_mappings, artifact_copy_forward)
- CLI formats daemon response summaries for output parity with pre-daemon behavior

See `docs/slices/rmapd-2-socket-transport.md` for full specification.

### REG-1: CLI Contract Leakage (2026-05-15)

**STATUS: ACTIVE DEBT — BLOCKING**

The current CLI exposes internal storage concepts that should be daemon-internal:

**Leaked concepts:**
- `db_path`: User must specify database file path for every command
- `repo_uid`: Internal storage identity (e.g., `pmc/2026-05-15T13:20:55.279Z/bf171385`)

**Current (leaky) contract:**
```bash
rmap index ./path/to/repo ./repo.db
rmap orient ./repo.db pmc/2026-05-15T13:20:55.279Z/bf171385
```

**Target (daemon-native) contract:**
```bash
rmap index .
rmap orient
```

**Why this is blocking:**
- Contradicts daemon-native product story (daemon owns repo state)
- Forces users to understand and manage SQLite plumbing
- Every tutorial, support thread, and screenshot gets polluted with internal identifiers
- Turns temporary contract debt into adoption debt

**Required resolution (REG-1):**
1. Daemon maintains repo registry (path → db_path + repo_uid)
2. CLI auto-discovers repo from cwd
3. Database files live in standard location (`~/.local/share/rmap/databases/`)
4. `repo_uid` becomes internal-only (visible in debug/doctor output only)
5. Explicit `--db`/`--repo-uid` retained as diagnostic escape hatches

See `docs/ROADMAP.md` for REG-1 priority status.

### HOOK-1 Implementation (2026-05-13)

**Implemented:** All six `rmap hook` commands functional with full flag support.

**Deferred items:**

1. **`post-edit` actual refresh execution:**
   - Currently marks files as dirty in session state only
   - Does not execute actual `rmap refresh` or incremental index update
   - Rationale: Refresh semantics complex (batch window, dirty tracking, partial refresh)
   - Future: Integrate with hooks.toml `post_edit.batch_window_seconds` for debounced refresh

2. **Full orientation summary in `session-start`:**
   - Returns simplified orientation (db exists, repo detected, stale flag)
   - Does not query snapshot-level counts (module_candidates, boundary_count, etc.)
   - Rationale: Orientation queries require snapshot_uid, not just repo existence
   - Future: Requires snapshot discovery or "most recent snapshot" resolution

3. **Integration detection accuracy:**
   - `rmap hook status` checks for hook strings in config files
   - Does not validate hook configuration structure or version compatibility
   - Future: Structured JSON parsing of host config files

4. **Session cleanup:**
   - Session state files persist in sessions_dir indefinitely
   - No automatic pruning of old sessions
   - Future: Add session expiry or `rmap hook sessions prune` command

5. ~~**`--from-stdin` transport not implemented:**~~ **RESOLVED (2026-05-13).**
   HOOK-1A implemented `--from-stdin` transport. Both Claude Code and Codex use stdin JSON.

### CODEX-1 Volatility (2026-05-14)

**Codex hooks are experimental.** Per OpenAI documentation (verified May 2026):

> "Hooks are experimental and may change in future releases."

**Implications:**
- Codex hook schema may change without notice
- Implementation is isolated in `commands/integrate/codex.rs` for easy updates
- Shared config transformation layer (`config.rs`) reduces schema change impact
- Monitor https://developers.openai.com/codex/hooks for changes

**Contract verification (May 2026):**
- Codex uses **stdin JSON transport** (not environment variables as previously assumed)
- HOST-1 v1 assumptions about CODEX_* env vars were incorrect and have been amended
- Schema structure matches Claude Code (nested matcher groups)
- Timeout unit is seconds (not milliseconds)


### REG-1 CLI Tests (2026-05-16)

**Context:** REG-1 changed CLI contract from `rmap <cmd> <db_path> <repo_uid> [args]` to
`rmap <cmd> [args]` with daemon-based repo resolution from cwd.

**Deferred test migration:**

Many CLI tests that verify success behavior now require daemon infrastructure:
- `edge_type_filter.rs`: 6 ignored (edge-type filtering behavior)
- `envelope_contract.rs`: 4 ignored (JSON envelope shape)
- `explain_command.rs`: 5 ignored (explain success paths)
- `imports_command.rs`: 6 ignored (imports query behavior)
- `orient_command.rs`: 10 ignored (orient success paths)
- `path_command.rs`: 7 ignored (path finding behavior)
- `stats_command.rs`: 5 ignored (stats metrics)

**Total: ~43 ignored tests**

**Rationale:** Tests require:
1. A running daemon
2. An indexed repo registered in daemon registry
3. REG-1 cwd-based resolution working

**Resolution path:**
- Move tests to `daemon_dispatch.rs` which has proper daemon test infrastructure
- Or create dedicated daemon harness for each test file

**Note:** Usage error tests and daemon-unavailable tests were updated to work without
daemon - only success path tests are ignored.

### REG-1 Duplicated Classification Helpers (2026-05-16)

**Context:** During REG-1 migration of `modules unowned` command, classification helper
functions were duplicated from CLI to daemon-runtime.

**Duplicated functions in `daemon-runtime/src/dispatch.rs`:**
- `classify_unowned_reason(path, module_roots)` — classifies why a file is unowned
- `is_excluded_directory(dir_name)` — checks if directory is in exclusion list
- `is_test_directory(dir_name)` — checks if directory is a test directory
- `is_source_file_for_unowned(path)` — checks if file is a source file
- `infer_language_for_unowned(path)` — infers language from file extension

**Root cause:** These pure functions existed only in CLI layer (`rgr/src/commands/modules/unowned.rs`).
For REG-1 migration, the handler needed these functions in the daemon, but there was no shared
classification crate where they belong.

**Impact:**
- Code duplication between CLI and daemon layers
- Maintenance burden if classification rules change

**Resolution path:**
1. Create a shared classification module in `repo-graph-classification` crate
2. Move helpers to shared module (DIP: both layers depend on abstraction)
3. Remove duplicates from CLI and daemon

**Severity:** Low. Pure functions, no state, minimal change frequency.

### CLI-OUT-1 Test Harness Pre-Build Dependency (2026-05-18)

**Context:** The `cli_output_mode.rs` test file contains 6 CLI success-path tests that
spawn an actual `rmapd` daemon process to test human/JSON output modes.

**Problem:** Tests require manual `cargo build -p rmapd` before running. Cargo does not
automatically build binaries from other packages when running `cargo test`.

**Location:** `rust/crates/rgr/tests/cli_output_mode.rs`

**Harness assumption:** The test locates `rmapd` by assuming it's a sibling of `rmap`
in the target directory (derived from `CARGO_BIN_EXE_rmap` → `parent.join("rmapd")`).

**Impact:**
- Test proof surface is not self-contained
- If package layout changes, harness becomes fragile
- Contributors may run `cargo test` and see confusing failures

**Resolution paths:**
1. Workspace-level test runner script that builds required binaries first
2. Convert to in-process testing (avoid subprocess spawning entirely)
3. Use `cargo test --workspace` with proper dependency ordering
4. Accept as documented limitation for integration tests

**Additional constraint:** Tests require Unix socket bind permission. They will fail
in sandboxed environments (e.g., Claude Code sandbox). This is environmental, not a
code defect, but further limits where the proof can be validated.

**Severity:** Medium. Divergence from ideal test automation, but tests are still
structurally valid when run correctly.

### CLI-OUT-3 Test Harness Pre-Build Dependency (2026-05-19)

**Context:** The `cli_out_3_drilldown.rs` test file contains 10 CLI success-path tests
that spawn an actual `rmapd` daemon process to test human/JSON output modes for
callers, callees, path, imports, and ambiguous symbol error formatting.

**Problem:** Same as CLI-OUT-1 (TD-CLI-OUT-1-A). Tests require manual
`cargo build -p rmapd` before running. They are marked `#[ignore]` and run opt-in.

**Location:** `rust/crates/rgr/tests/cli_out_3_drilldown.rs`

**Run command:**
```
cargo build -p rmapd
cargo test -p repo-graph-rgr --test cli_out_3_drilldown -- --ignored
```

**Rationale:** Consistent with existing CLI integration test pattern. The tests
exist as proof surface but are not part of the default `cargo test` path.

**Severity:** Medium. Same class as CLI-OUT-1 debt.

### CLI-OUT-6 Legacy Command Success-Path Test Gap (2026-05-20)

**Context:** CLI-OUT-6 Group 1 (`churn`, `hotspots`) commands use legacy direct-storage
contract (explicit db_path/repo_uid), not REG-1 daemon. The existing test harness pattern
(daemon spawn + socket) does not apply.

**Problem:** The `cli_out_6_quality.rs` test file contains 13 tests, but all are
error-path tests (usage errors, flag acceptance, invalid DB handling). No tests verify
success-path behavior:
- Human output on valid data
- JSON output on valid data
- Output mode switching between human/JSON

**Location:** `rust/crates/rgr/tests/cli_out_6_quality.rs`

**Gap classification:**
- **NOT** the same class as CLI-OUT-1/CLI-OUT-3 daemon pre-build debt
- **IS** a coverage gap for legacy-contract commands

**Current evidence:** Success-path behavior validated manually against repo-graph corpus
(OBSERVED in review packet), not automated.

**Disposition:** Accepted gap for Group 1. Revisit at CLI-OUT-6 slice closure or
future SMOKE-1 harness cleanup slice. The existing corpus validation provides
sufficient evidence for current slice, but automated regression is missing.

**Resolution paths:**
1. Create fixture-based test that uses a pre-populated test database (no daemon needed)
2. Add success-path tests to existing file with appropriate fixtures
3. Accept as documented manual validation until SMOKE-1 addresses broader harness

**Severity:** Low. Commands work correctly (OBSERVED). Gap is in automated regression,
not in functionality.

### Smoke Script Validation Model Defects (2026-05-18)

**Context:** The smoke scripts (`smoke-rmap.sh`, `smoke-validation-repos.sh`) were updated
for REG-1 daemon-based CLI but have structural weaknesses that prevent them from being
trusted as a product-grade validation surface.

**Defects identified:**

#### A. Weak multi-command model
The script treats remaining positional args as separate commands:
```bash
COMMANDS=("$@")
for COMMAND in "${COMMANDS[@]}"
```

This works for simple commands (`trust check orient`) but cannot safely model:
- `orient --budget small`
- `explain src/foo.cpp --json`

No structured command-list encoding (e.g., repeated `--cmd`, JSON manifest, or
newline-delimited spec file).

#### B. Output file typing lies about content type
Scripts write `.json` extensions regardless of actual content:
- `trust.json`, `orient.json`, `check.json`

But after CLI-OUT-1, default output for several commands is plain text, not JSON.
Artifact naming does not reflect content type.

**Resolution:** Use `.txt` for human output, `.json` only when `--json` flag is used.

#### C. Conflates execution failure with domain verdict failure
Any non-zero exit code is marked as failure. But `check` can legitimately exit
non-zero because the repo fails quality/trust checks.

Cannot distinguish:
- Product worked and reported FAIL (domain verdict)
- Product failed to execute (transport error)

**Resolution:** Metadata should separate:
- `transport_status`: ok / daemon_error / timeout / invalid_output
- `command_exit_code`: raw exit code
- `semantic_verdict`: pass / fail / unknown / not_applicable

#### D. Incorrect metadata field names
Scripts write:
```json
"repo_uid": "$REPO_NAME"
```

This is structurally wrong. `repo_uid` is the daemon-generated stable identifier,
not the directory name.

**Resolution:** Use correct field names:
- `repo_name`: directory basename
- `repo_path`: full path
- `repo_uid`: only if retrieved from daemon response

#### E. Build-environment coupling
Scripts use `cargo run --release` for every command invocation. This couples the
smoke protocol to:
- Local cargo installation
- Workspace shape
- Repeated compilation behavior

**Resolution for release-smoke:** Build once, run explicit binaries. Or point at
installed binaries. Current model is acceptable for dev validation only.

**Severity:** Medium. Does not invalidate the Tarjan SCC fix validation, but blocks
trusting the smoke harness as a stable protocol tool.

**Immediate fix applied:** Changed `.json` → `.txt` for non-JSON output modes.

**Full resolution:** Requires a dedicated cleanup slice addressing A-E above.

## CLI Contract Migration

### Declare Family Still Uses Legacy Contract

**Added:** LEGACY-CONTRACT-MIGRATION-1C (2026-05-22)

The `declare` command family (`declare boundary`, `declare requirement`, `declare waiver`,
`declare quality-policy`, `declare deactivate`, `declare supersede`) still uses the legacy
`<db_path> <repo_uid>` contract while read-side commands have migrated to REG-1 daemon contract.

**Affected commands:**
- `rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target>`
- `rmap declare requirement <db_path> <repo_uid> ...`
- `rmap declare waiver <db_path> <repo_uid> ...`
- `rmap declare quality-policy <db_path> <repo_uid> ...`
- `rmap declare deactivate <db_path> <repo_uid> ...`
- `rmap declare supersede <db_path> <repo_uid> ...`

**Impact:**
- Test `declare_boundary_visible_to_violations` marked `#[ignore]` — declares a boundary
  via legacy contract but `violations` now uses REG-1, so the test cannot validate the
  integration path without daemon harness.
- Users cannot use `declare` commands from cwd like other commands.

**Target contract (REG-1 pattern):**
```bash
rmap declare boundary <module_path> --forbids <target>  # repo resolved from cwd
```

**Resolution:** A future slice (LEGACY-CONTRACT-MIGRATION-2 or similar) should migrate
the `declare` family to REG-1, adding daemon handlers for write operations. This requires
daemon-side write coordination similar to `assess` (acquire write lock + refresh lock).

## CLI Presentation

### display_name in shared response DTOs

**Added:** CLI-OUT-2B (2026-05-18)

`display_name: Option<String>` is a daemon-populated presentation field embedded
in shared response DTOs (`OrientResult`, `TrustReport`, cycles JSON).

**Why this is an architectural compromise:**

The field exists to avoid renderer-side identity guessing. Before this change,
the CLI would have to derive human-readable repo names from:
- cwd-based heuristics
- registry lookups duplicating daemon logic

Instead, the daemon computes display identity once (registry alias if present,
else path basename) and embeds it in the response.

**Why it is acceptable:**

- Daemon populates it; agent code clearly marks it as daemon-populated
- Field is `Option<String>` with `#[serde(default)]`
- No business logic depends on it
- Renderers fall back to internal `repo` UID when absent

**What this is NOT:**

- Not a domain field — it has no meaning to core business rules
- Not required for correctness — only for human presentation

**Affected DTOs:**

- `OrientResult` (agent/src/dto/envelope.rs) — used by orient, check, explain
- `TrustReport` (trust/src/types.rs) — used by trust
- cycles inline JSON (daemon-runtime/src/dispatch.rs) — used by cycles

**Resolution if this becomes problematic:**

Move display identity to a separate presentation envelope that wraps the core
DTO at the CLI boundary. This would require presentation-layer types distinct
from daemon response types. Current approach avoids that complexity.

## Performance — Daemon/Transport

### RMAPD-PERF-1: Stats Query Performance — STATS FIXED

**Added:** 2026-05-18  
**Stats query fixed:** 2026-05-19

**Stats root cause (OBSERVED):** `compute_module_stats` query had three
correlated subqueries in the SELECT clause, running once per module with
O(modules × edges × symbols) complexity.

**Fix:** Rewrote query to use CTEs, computing module→file mapping and
per-file symbol stats in single passes, then aggregating.

**Results (OBSERVED):**
- Django stats: 760,594ms → 2,981ms (255x improvement)
- DuckDB stats: 5,537ms (acceptable)
- All repos complete stats in under 6 seconds

**Remaining mitigations (retained as defensive measures):**
- 300s client read timeout (was 30s)
- Pre-computation heartbeat emission

**Remaining debt (NOT measured/fixed):**
- Trust query timings not instrumented
- Cycles/Tarjan SCC timings not instrumented
- Indexing phase timing not instrumented
- No mid-query keepalive for genuinely long operations
- Index utilization not audited for non-stats queries

The timeout class is mitigated but not universally solved. One real
pathological query was found and fixed. Future heavy operations could
still encounter issues.

See `docs/slices/rmapd-perf-1-timeout.md` for honest assessment.

### RMAPD-PERF-2: Refresh Copy-Forward Query Performance — COMPLETE

**Discovered:** 2026-05-21 during ORIENT-BUG-1 validation  
**Fixed:** 2026-05-21

**Symptom:** `rmap refresh` on large repos (Django) took 6+ minutes, causing
client timeout. Daemon continued processing after client disconnect.

**Root Cause (OBSERVED via timing instrumentation):**

`copy_forward_measurements()` executed one SQL INSERT...SELECT per unchanged file:
```
for file_path in unchanged_file_paths {  // 3015 iterations
    conn.execute("INSERT ... WHERE ... LIKE ?3", ...)?;  // ~23ms each
}
// Total: 3015 * 23ms = 69,345ms (69 seconds)
```

**Evidence:**
```
[PERF] refresh: copy_measurements=71170ms copied=11386  // BEFORE FIX (per-file)
[PERF] refresh: copy_measurements=34416ms copied=121015 // AFTER FIX (batched)
```

**Fix:** Replaced per-file loop with batched temp-table approach:
1. Insert all unchanged paths into temp table `_unchanged_files_m`
2. Single INSERT...SELECT with SUBSTR extraction and IN subquery

**Bug fix:** `:FILE` key extraction off-by-one.
- Wrong: `LENGTH(target_stable_key) - prefix_len - 5`
- Correct: `LENGTH(target_stable_key) - prefix_len - 4`
- This truncated last character of file path for file-level measurements/inferences.

**Files modified:**
- `rust/crates/storage/src/refresh_copy_forward_impl.rs`

**Result:**
- Django refresh: 6+ min (timeout) → ~100 seconds (completes)
- copy_loop: 71s → 34s for 121K measurements (2x improvement)

**Separately fixed (RMAP-IO-1):**
- EAGAIN / os error 35: Transport timeout classification. See RMAPD-PERF-2C (now FIXED).

**Remaining performance note:**
SQLite cannot index SUBSTR expressions. The 34s copy time is due to
full-table scan with expression evaluation. Further optimization would
require schema changes (storing extracted path as indexed column).
See RMAPD-PERF-2B below.

### RMAPD-PERF-2B: Copy-Forward Path Extraction Not Indexed — DEFERRED

**Discovered:** 2026-05-21 during RMAPD-PERF-2 fix

**Symptom:** Copy-forward queries take 34s for 121K measurements due to
SUBSTR evaluation on every row (O(n) full scan).

**Current implementation:**
```sql
SUBSTR(target_stable_key, 32, INSTR(target_stable_key, '#') - 32)
  IN (SELECT path FROM _unchanged_files_m)
```

**Why not indexed:** SQLite cannot create indexes on expression results.
The SUBSTR extraction runs for every row in measurements table.

**Potential fix:** Add denormalized `anchor_file_path` column to measurements
table, populated at write time. Add index on (snapshot_uid, anchor_file_path).

**Trade-off:**
- Pro: Copy-forward becomes O(log n) index scan instead of O(n) full scan
- Con: Storage overhead, write-time extraction, schema migration

**Priority:** Low (current 34s is acceptable, only affects large repos)

**Not addressed in RMAPD-PERF-2 fix because:**
- Current performance is acceptable (~100s total vs 6+ min timeout)
- Schema changes require migration strategy
- Complexity vs benefit ratio unfavorable

### RMAPD-PERF-2C: EAGAIN / os error 35 Transport Bug — FIXED (RMAP-IO-1)

**Discovered:** 2026-05-21 during ORIENT-BUG-1 validation  
**Status:** FIXED

**Symptom:** Client reported `Resource temporarily unavailable (os error 35)`
during refresh on large repos. This is EAGAIN from socket read timeout.

**Root Cause (IDENTIFIED):**
- macOS socket read timeout returns EAGAIN (error 35), not ETIMEDOUT
- Client mapped all I/O errors to `ReadFailed`, losing timeout distinction
- User saw cryptic "os error 35" instead of "timed out after 300s"

**Fix (RMAP-IO-1):**
- Added `Timeout` variant to `DaemonClientError`
- Map `io::ErrorKind::WouldBlock` and `io::ErrorKind::TimedOut` to `Timeout`
- User now sees: "daemon response timed out after 300s"

**Files modified:**
- `rust/crates/rgr/src/daemon_client/connection.rs`

See `docs/slices/rmap-io-1.md`.

## Sandbox Mode Detection — macOS-Specific

**Added:** 2026-05-28  
**Status:** Known limitation

**Context:** STATE-ROOT-SEPARATION-1 introduced sandbox mode to constrain
authority writes (A1: baselines, aliases, declarations) when the daemon
operates in an isolated environment where durable state would be lost.

**Platform-agnostic concept:**
- Daemon cannot rely on normal shared socket path / shared durable root
- Authority writes must be blocked to prevent silent data loss
- Local cache/state may need an isolated root

This concept is valid on any platform with sandboxing or container isolation.

**Platform-specific implementation:**
- Current detection: state root path starts with `/private/tmp/`
- This is macOS-specific (Codex sandbox writes to `/private/tmp/repo-graph-agent/<uid>/`)
- Linux/container sandbox scenarios are not modeled

**Consequences:**
- Integration test `index_allowed_in_sandbox_mode_proves_a2_and_b_writes` is `#[cfg(target_os = "macos")]`
- Linux gets no sandbox-mode test coverage
- Future Linux sandbox scenarios would need detection logic extension

**Future refactor direction:**
- Decouple sandbox-mode classification from path-prefix heuristics
- Make `StateRootMode::SandboxLocal` derivable from:
  - Explicit state-root mode enum set at startup
  - Transport fallback context (stdio fallback = sandbox)
  - Configuration flag
- Tests could then construct sandbox state directly without OS-specific paths

**Impact:** Low — macOS is the primary sandbox scenario (Codex). Linux container
isolation typically uses different mechanisms (volume mounts, network namespaces)
that may not trigger the same socket-access-denied pattern.

See `docs/slices/state-root-separation-1.md`.

## Parity Test Fixtures — Schema Drift

**Discovered:** 2026-05-22 during release preparation  
**Status:** BLOCKED — fixture regeneration required

**Issue:** The `storage-parity-fixtures/` corpus was created before migration
027-freshness-provenance. Migration 027 adds `freshness_state`,
`freshness_updated_at`, and `provenance_json` columns to 12 tables:

- boundary_contracts
- boundary_interaction_links
- inferences
- project_surfaces
- project_surface_evidence
- surface_entrypoints
- surface_config_roots
- surface_env_dependencies
- surface_env_evidence
- surface_fs_mutations
- surface_fs_mutation_evidence
- module_candidates

The parity test (`rust/crates/storage/tests/parity.rs`) compares actual
database schema against expected.json fixtures. With migration 027 applied,
all 6 fixtures fail because their schema definitions lack the new columns.

**Temporary Mitigation:** Test marked `#[ignore]` to unblock CI.

**Required Fix:** Regenerate all expected.json files:
1. Run each fixture's operations.json through StorageConnection
2. Capture actual schema dump with `RGR_STORAGE_PARITY_EMIT_ACTUAL=1`
3. Update expected.json files with new schema structure
4. Remove `#[ignore]` from test

**Alternative:** Write a schema-diff-tolerant parity mode that ignores
column differences for tables with freshness columns. This trades precision
for maintenance burden.

**Priority:** Medium — the parity test is the cross-runtime contract gate
for Rust-2. Until regenerated, schema drift between TS and Rust adapters
may go undetected.

## Quality Handler Bug Fixes

### Git Spawn Failure in Quality Commands — FIXED (2026-05-22)

**Discovered:** 2026-05-22 during LEGACY-CONTRACT-MIGRATION-1B validation  
**Status:** FIXED

**Symptom:** `rmap churn`, `rmap hotspots`, `rmap risk` commands failed with:
```
error: InternalError: git churn failed: failed to spawn git: No such file or directory (os error 2)
```

**Root Cause Analysis:**

1. **Missing PATH in launchd:** The daemon launchd plist lacked PATH environment variable.
   Fix: Added `PATH=/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin` to plist template.

2. **Relative `root_path` not resolved:** The `repos.root_path` column stores paths
   relative to the db_path location (e.g., `../../../../Documents/repo`). The handler
   passed this relative path directly to `Command::new("git").current_dir(path)`.

   Problem: The daemon runs with cwd=`/` (root). Resolving `../../../../Documents/repo`
   from `/` fails when there aren't enough parent segments to reach the target.

   Fix: Added `resolve_root_path(db_path, relative_root_path)` helper in
   `handlers/quality.rs` that joins the path relative to db_path's parent directory
   and canonicalizes.

**Files modified:**
- `scripts/templates/com.repo-graph.rmapd.plist` — added PATH
- `rust/crates/daemon-runtime/src/handlers/quality.rs` — resolve_root_path helper

**Verification:** All quality commands (churn, hotspots, risk, coverage) working
on live daemon with launchd service.

---

## Fresh-Eyes Review (2026-07-02, v0.4.0 self-dogfood)

A fresh-eyes review at v0.4.0 (VISION distilled → `docs/FUTURE-ITERATIONS.md`;
codebase audit; rmap self-dogfood on an isolated index of repo-graph itself).
Findings F1–F4; dispositions inline.

**F1 — `orient --budget medium` does not cap the package-groups section (P2).**
On repo-graph's self-index (316 package groups) `medium` printed ALL groups
(~450 lines); on nginx (15 groups) medium is 53 lines. The C5 progressive
ladder caps other sections but not this one, so information density collapses
exactly on high-group-count repos. Fix direction: cap groups at medium (top-N
by size + "+N more — rmap stats"), uncap only at large/full.
**Disposition: OPEN — quick-win candidate (ORIENT-DENSITY follow-up class).**

**F2 — The Rust extractor emitted no complexity (P1, honesty).**
*(Premise corrected 2026-07-02 by METRIC-LANG-COVERAGE-1's build evidence:
the original claim that Java/Python were also unmeasured was WRONG —
`java-extractor` has `metrics.rs`, `python-extractor` computes cyclomatic
inline; the audit grep never covered those crates.)* Top-500 complexity on
the self-index contained zero Rust functions while `orient`/`hotspots`
rendered the ranking as repo-wide fact. Same violation class as
HONEST-DEGRADATION-1 D1. **Disposition: SLICED —
`docs/slices/metric-lang-coverage-1.md` (data-driven coverage-caveat
mechanism as general honesty infrastructure + Rust emission).**

**F3 — No "retired tree" concept in the module/orientation model (P3).**
There is no way to tell the index that a tracked tree is legacy (downweight /
label in orientation). Made acute by the TS prototype (F4); after
TS-PROTOTYPE-RETIREMENT-1 deletes it, the general need may not recur — do not
build the mechanism speculatively (VISION acid test).
**Disposition: WATCH — revisit only if a real retained-legacy case appears.**

**F4 — The legacy TS prototype dominates every self-index signal (P1, focus).**
~90k LOC (`src/` + `test/` + `parity-fixtures/`), stale since 2026-04-26: all
complexity centers, all top hotspots, 4/6 module cycles, and ~60 package
groups point into it. **Disposition: SLICED —
`docs/slices/ts-prototype-retirement-1.md` (verify-then-delete; git history
is the archive).**

**F5 — SUSPECTED DEFECT: indexes never reach READY on the second machine (P1, investigate before next release).**
(2026-07-03, v0.4.0 installed build.) Two consecutive `rmap index` runs on the
160k-file repo — the second left running for HOURS — produced snapshots that
`orient` will not serve ("no READY snapshot"); `doctor` counts 2 snapshots.
Snapshots are created `building` and flip to `ready` only at finalize
(`storage/src/crud/snapshots.rs`), so either (H1) finalize is dying on that
machine (sleep/panic/OOM — daemon.log will show it) or (H2) a daemon index
path in 0.4.0 completes without calling `update_snapshot_status(ready)`, or
(H3) repo-identity mismatch (snapshots attached to a different repo_uid than
orient resolves). **ROOT CAUSE FOUND (2026-07-03, from the machine's daemon.log — 2x "Write
error to (unnamed): broken pipe (os error 32)" and nothing else):** the index
progress callback in `daemon-runtime/src/dispatch.rs` (`handle_index`) returns
`ControlFlow::Break` when `emitter.emit()` fails. A >300s silent phase on a
big repo times out the CLI (default read timeout) -> client closes the socket
-> the daemon's NEXT progress emit gets broken pipe -> Break -> **the index
aborts mid-flight**. The `building` snapshot + extracted GBs remain (doctor:
"2 snapshots, 4 GB"); the ready-flip never runs (orient: "no READY snapshot");
`record_index` + `registry.save()` live only in the success branch (repo info:
"repo not indexed", even by uid). The client's own timeout KILLS the work it
is waiting for. Smoke never catches it: no validation repo has a >300s silent
phase. **Disposition: INDEX-DISCONNECT-1 hotfix required BEFORE the next
release (design decision — detached completion vs cancel-with-cleanup — needs
operator ratification); DAEMON-VISIBILITY-1 (in flight) fixes the reporting
half (client timeout honesty + progress exposure).**

**F6 — `rmap check` F2 residual (ratified 2026-07-04, DAEMON-VISIBILITY-1 dv1-check-f2-scope = C).**
`check`'s READY-requiring error still says "No READY snapshot. Index the repo
first." without naming an existing non-READY partial (state/size/next
actions), because the message lives in `rust/crates/agent` (out of the
slice's ratified scope; daemon-side text wrapping was rejected as verdict-
ownership drift). Every other READY-requiring surface (orient, explain,
enrich, governance/inventory handlers) is F2-compliant, and
INDEX-DISCONNECT-1 removes the main producer of lingering partials.
**Disposition: FAST-FOLLOW — fold the agent-crate F2 message into the next
agent-crate slice; the pure helper (snapshot_facts) already exists to call.**

**F7-F12 — Daemon crash mid-index leaves unreconcilable state (2026-07-06, v0.5.0 second-machine, 87k-file repo).**
Extraction completed (progress streaming + honest timeout worked as shipped); the daemon then died
mid-postpass (peak 5.8 GB; "daemon startup (cold)" mid-log + vanished file lock = restart; no macOS
crash record — memory-pressure kills are often unlogged). Aftermath, each its own defect:
- **F7 (P0): no startup reconciliation.** INDEX-DISCONNECT-1's "no building limbo" holds only for
  deaths the daemon survives. Crash-orphaned `building` snapshots (3 of them, 11 GB) are invisible:
  retention stats show total=3 with ZERO in every class; `maintenance prune` says "no prunable
  snapshots found." Next boot must detect building-with-no-live-op, mark interrupted(daemon-restart),
  log it, and make them prunable.
- **F8 (P1): daemon log is mute on operations.** The whole incident's log: startup + 3 broken pipes.
  Op lifecycle lines (start/phase transitions/outcome) must go to the LOG, not only doctor — forensics
  cannot depend on doctor being healthy.
- **F9 (P2): doctor storage probe rendered the restart race as raw FAIL** ("failed to open storage
  connection: database is locked" while the dying process held the lock) — reader-frame case needed.
- **F10 (P1): orient's no-READY error on this path bypassed F2** — bare "index the repo first" while
  an 11 GB partial sat there (the exact gaslighting F2 was built to kill). Find and fix the path.
- **F11 (P0, = F7 mechanism): prune's interrupted-detection requires evidence a crash never wrote.**
- **F12 (P2): retention stats render 0-in-every-class with total=3 without naming the states** — the
  table should say "3 building (orphaned)" instead of implying an empty store.
Postpass memory (peak 5.8 GB on 87k files) escalates existing debt #8 from perf to stability.
**Disposition: SLICED — `docs/slices/daemon-crash-recovery-1.md` (F7/F8/F10/F11/F12; F9 folded);
postpass memory tracked under #8, promoted if the clean-slate retry crashes again.**

**F13 — REPRODUCIBLE: daemon dies mid-postpass on the 87k-file repo (P0 — blocks indexing entirely on that machine).**
(2026-07-06, v0.5.0.) Clean slate (`repo remove --delete-db`) + re-index reproduced the death: new
database, same wreckage. Two branch hypotheses, discriminated by `git ls-files | wc -l` vs the
indexer's 87,280:
- (a) **Inventory walks ignored trees** (node_modules/build/vendored): explains file count, 11 GB db,
  5.8 GB postpass peak, and the crash — fix is ignore-respecting inventory (cheap, and a VISION fix:
  orientation is about the reader's code, not library internals). ALSO suspect for the day-1
  "160k-file" repo.
- (b) Repo genuinely has ~87k tracked files: real postpass memory bounding needed (promotes #8 from
  perf to stability — batch/stream dominant postpasses, memory ceiling with honest degradation).
Interim operator guidance: index scoped subtrees via `--include-root` until fixed.
**ROOT CAUSE FOUND (2026-07-06, operator profile on legacy-codebases/linux): `fatal runtime
error: stack overflow, aborting` at `persisting: 5/8` = `persist_boundary_interactions` (BI-1A
re-parse postpass, recursive AST descent — depth scales with tree, deterministic at kernel scale;
RSS was a red herring: peaked 10 GB in extraction then FELL before death). Fix:
`docs/slices/persist-recursion-1.md` (iterative walks + depth guard + honest per-file skip).**
Prior branch analysis (kept for the record): `git ls-files` = 151,765 — MORE than the indexer's 87,280
(the extractor-routed subset). The repo is genuinely kernel-scale; postpass memory bounding is the
real work → POSTPASS-MEMORY-1 (P0). The scanner's root-only-gitignore gap (walkdir +
`load_root_gitignore`; no nested .gitignore, no .git/info/exclude, nothing at all without a root
file) stays filed as a LATENT defect — fix by moving to the `ignore` crate's WalkBuilder when
touched. Operator profiling run on legacy-codebases/linux to name the dominant postpass before
slicing. Interim on affected machines: `--include-root` scoping.**

Related (not new): REG-1 help truth confirmed live (`rmap metrics` usage is
still positional `<db_path> <repo_uid>`); call-graph 21% resolved pre-enrichment
on self-index with the D5 next-action line correctly pointing at `rmap enrich`
(ENRICH-LIFECYCLE-1 remains the fix for the lifecycle half).
