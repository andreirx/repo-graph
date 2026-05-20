# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Strategic center

The stable product center is **legacy-code relationship discovery**.

Repo-graph exists to model the relationships that determine how legacy systems can be
understood and changed safely:
- seams and enabling points
- sensing and separation barriers
- module and boundary relationships
- state/resource touchpoints
- policy-propagation paths
- testability constraints
- migration/replacement relationships

This is Feathers-driven product logic. Multi-language support exists to feed this
relationship substrate. It is not a collection of unrelated per-language extractors.

## Strategic language tracks

Two primary tracks feed the relationship substrate:

**Server/systems track:**
- TypeScript/JavaScript
- Rust
- Python
- Java
- C
- C++

**Mobile/client track:**
- Objective-C
- Objective-C++
- Swift
- Kotlin
- Dart

Later: Go, Scala.

These are strategic priorities, not shipped capability claims. See "Current state"
for what is actually operational.

## Distribution / Install / Host Integration Track

This track makes repo-graph installable developer infrastructure, not just a CLI.

### Platform Priority (Locked)

1. **macOS** — primary supported platform
2. **Linux** — second priority
3. **Windows** — explicitly deferred

### Track Sequence

| Slice | Scope | Status |
|-------|-------|--------|
| **DIST-1** | Distribution and install contract (binary-first, manifest model, safety rules) | IMPLEMENTED |
| **HOST-1** | Host integration contract (Claude/Codex hooks, Cursor MCP, detection model) | IMPLEMENTED |
| **REL-1** | Release pipeline (GitHub Actions, artifact matrix, checksums) | IMPLEMENTED |
| **REL-SUPPORT-1** | Version authority (workspace version, bump/cut scripts, CI validation) | IMPLEMENTED |
| **RGISTR-1** | rgistr binary packaging (Node SEA, same archive as rmap/rmapd) | IMPLEMENTED |
| **RMAPD-1** | Daemon binary target (`rmapd`) + daemon-runtime crate | IMPLEMENTED |
| **HOOK-1** | `rmap hook` CLI surface (session-start, prompt-submit, post-edit, pre-compact, stop) | IMPLEMENTED |
| **MAC-1** | macOS installer + daemon service (launchd, paths, health checks) | IMPLEMENTED (blocked) |
| **HOOK-1A** | Stdin JSON transport for Claude Code hooks (`--from-stdin` flag) | IMPLEMENTED |
| **CLAUDE-1** | Claude Code integration (`.claude/settings.json` hooks) | IMPLEMENTED |
| **CODEX-1** | Codex CLI integration (`hooks.json`) | IMPLEMENTED |
| **LINUX-1** | Linux installer + daemon service (systemd user unit) | IMPLEMENTED |
| **RMAPD-2** | Unix socket transport (resident daemon model) | IMPLEMENTED |
| **REG-1** | Repo registry + cwd auto-discovery (read-side contract) | IMPLEMENTED |
| **CLI-OUT-1** | Presentation layer (human-default output, --json opt-in) | IMPLEMENTED |
| **CLI-OUT-2A** | Cross-repo output audit (findings + contracts) | HANDOFF COMPLETE |
| **CLI-OUT-2B** | First-contact discovery output (orient, trust, cycles) | IMPLEMENTED |
| **RMAPD-PERF-1** | Large repo timeout investigation | STATS FIXED |
| **ORIENT-BUG-1** | Module count mismatch (data bug) | QUEUED |
| **CLI-OUT-2C** | stats renderer | IMPLEMENTED |
| **CLI-OUT-3** | Graph drilldown output (callers, callees, path, imports, explain) | IMPLEMENTED |
| **CLI-OUT-4** | Module/architecture output (modules, surfaces, boundaries) | COMPLETE |
| **CLI-OUT-5** | Inventory output (docs, resource, policy) | COMPLETE |
| **CLI-OUT-6** | Quality/risk output (churn, hotspots, risk, coverage) | QUEUED |
| **CLI-OUT-7** | Governance output (violations, gate, assess) | QUEUED |
| **SMOKE-1** | Validation harness cleanup (command model, verdict semantics) | QUEUED |
| **CURSOR-1** | Cursor MCP/rules integration | QUEUED |
| **WIN-1** | Windows distribution/install | DEFERRED |
| **MAC-2** | macOS signing/notarization | DEFERRED |
| **UPDATE-1** | Updater/repair channel | DEFERRED |

### Current Priority

**CLI-OUT-5: Inventory/Policy Output** — COMPLETE (2026-05-20)

Human renderers for 6 inventory and policy commands, in 3 groups:

| Group | Commands | Status |
|-------|----------|--------|
| 1 | docs list, extract | COMPLETE |
| 2 | resource list, readers, writers | COMPLETE |
| 3 | policy | COMPLETE |

Implementation order: documentation inventory first (smallest), resource inventory
second (same vocabulary), policy last (different semantic class).

See `docs/slices/cli-out-5-inventory.md` for specification.

### Recently Completed

**CLI-OUT-4: Module/Architecture Output** — COMPLETE (2026-05-20)

Delivered:
- Human renderers for 11 read-side architecture commands
- modules list, show, files, unowned, deps, violations
- surfaces list, show
- boundaries list, show, summary
- Groups 1-3: corpus-validated (OpenXcom, django, duckdb)
- Groups 4-5: empty-case corpus-validated, populated-case fixture-validated

See `docs/slices/cli-out-4-modules.md` for specification.

**CLI-OUT-3: Graph Drilldown Output** — IMPLEMENTED (2026-05-19)

Delivered:
- Human renderer for `callers` and `callees` (shared `graph_edges.rs` module)
- Human renderer for `path` with query-term-preserving header
- Human renderer for `imports` with depth and resolution status
- `--json` flag for machine mode on all commands
- Structured `AmbiguousSymbol` error handling with daemon data payload
- CLI renders numbered match list with disambiguation hint
- Validated on 3-repo corpus (OpenXcom, django, duckdb)

See `docs/slices/cli-out-3-drilldown.md` for specification.

**CLI-OUT-2C: Stats Renderer** — IMPLEMENTED (2026-05-19)

Delivered:
- Human renderer for `stats` with full sorted sections
- No arbitrary top-N clipping or threshold-based labeling
- Sections: Summary, By size, By fan-in, By fan-out, By distance from main sequence
- `--json` flag for machine mode
- Validated on 5-repo corpus

See `docs/slices/cli-out-2c-stats-renderer.md` for specification.

**CLI-OUT-2B: First-Contact Discovery Output** — IMPLEMENTED (2026-05-18)

Delivered:
- Human renderer for `orient` with repo name, cycle topology, evidence-bearing degradation
- Human renderer for `trust` with resolution rates, reliability breakdown
- Human renderer for `cycles` with topology
- All commands default to human output, `--json` for machine mode
- Validated on 5-repo corpus (OpenXcom, buildroot, django, duckdb, grpc-java)

See `docs/audits/cli-out-2b/review-packet.md` for validation evidence.

**RMAPD-PERF-1: Large Repo Timeout** — STATS FIXED (2026-05-19)

Stats query pathology identified and fixed. Timeout class mitigated.

**Stats root cause:** `compute_module_stats` query had correlated subqueries
with O(modules × edges × symbols) complexity. Django stats took 760 seconds.

**Fix:** Rewrote query to use CTEs, computing aggregates in single passes.
Django stats now takes 3 seconds (255x improvement).

**Evidence:** Timing instrumentation (`--features perf-trace`) confirmed
before/after performance on stats.

**Not proven:** Trust, cycles, and other query performance not instrumented.
The broader timeout class is mitigated but not universally solved.

See `docs/slices/rmapd-perf-1-timeout.md` for honest assessment.

### Handoff Complete

**CLI-OUT-2A: Cross-Repo Output Audit** — HANDOFF COMPLETE

Audit sufficient to drive first implementation wave.

Completed:
- 5 of 7 repos audited (gstreamer/hadoop blocked at time, now unblocked)
- Contracts proposed for first-contact discovery commands

Gaps handed off:
- ORIENT-BUG-1: Module count mismatch (still queued)

See `docs/audits/cli-out-2a/synthesis.md` for findings.

---

### Queued Bug/Support Slices

**ORIENT-BUG-1: Module Count Mismatch** — QUEUED

Data/query bug. Orient shows 2-17 modules when trust shows 19-240+.
Not a renderer issue. Requires storage/query investigation.

See `docs/slices/orient-bug-1-module-count.md`.

---

### Output Program Wave Model

| Wave | Slice | Commands | Status |
|------|-------|----------|--------|
| 1 | CLI-OUT-2B | orient, trust, cycles, check | IMPLEMENTED |
| 1b | CLI-OUT-2C | stats | IMPLEMENTED |
| 2 | CLI-OUT-3 | callers, callees, path, imports, explain | IMPLEMENTED |
| 3 | CLI-OUT-4 | modules (list/show/files/unowned/deps/violations), surfaces, boundaries | COMPLETE |
| 4 | CLI-OUT-5 | docs (2), resource (3), policy (1) | COMPLETE |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | QUEUED |
| 6 | CLI-OUT-7 | violations, gate, assess | QUEUED |

Each wave: audit, define contracts, implement renderers, validate.

---

**SMOKE-1: Validation Harness Cleanup** — QUEUED

Support infrastructure slice. Addresses structural defects in smoke scripts.
Current harness is imperfect but sufficient for audit-phase manual review.
More important before broad implementation automation (CLI-OUT-2B) than before
human review of existing outputs (CLI-OUT-2A).

See `docs/slices/smoke-1-validation-harness-cleanup.md` for specification.

---

**CURSOR-1: Cursor Integration** — QUEUED

Different integration model from Claude Code / Codex. Moved back from current
to prioritize product-surface quality (CLI-OUT-2A/2B) based on real-repo
validation evidence from Tarjan SCC fix smoke runs.

### Recently Completed

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

Delivered:
- Human-default plain text output for `orient`, `check`, `explain`
- `--json` flag for machine mode (full daemon envelope)
- `presentation/` module with typed response structs
- 50 tests (37 renderer unit + 7 flag parsing + 6 CLI success-path)

See `docs/slices/cli-out-1-presentation-layer.md` for specification.

---

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

**Closure scope: Read-side contract migration complete.**
- Write/governance families intentionally deferred
- Presentation quality handed off to CLI-OUT-1

Delivered:
- Daemon-owned repo registry with persistence
- CWD-based resolution for normal read/query surface
- `db_path`/`repo_uid` removed from normal read workflows
- All read commands migrated (orient, check, explain, callers, callees, path, imports, cycles, stats, trust, gate, modules/*, boundaries/*, surfaces/*, contracts, docs, deps, inferences, resource)

Explicitly deferred:
- Write operations: `assess`, `enrich`, `modules boundary`
- Governance: `declare/*`, `policy`
- Quality commands: `quality/*` (churn, risk, hotspots, metrics, coverage)
- Other legacy: top-level `violations`
- Override flags: `--repo`, `--repo-path`, `--db`, `--repo-uid`

See `docs/slices/reg-1-repo-registry.md` for full specification.

Completed:
- REL-SUPPORT-1: v0.1.1 release (CI validated)
- RGISTR-1: rgistr packaged as Node SEA binary in release archive
- DIST-1: Distribution contract locked (binary-first, manifest, security rules)
- HOST-1: Host integration contract locked (thin shim model, transport model clarified)
- HOOK-1: `rmap hook` CLI surface implemented (2026-05-13)
- HOOK-1A: Stdin JSON transport for Claude Code hooks (2026-05-13)
- MAC-1: macOS installer + daemon service implemented (2026-05-13)
- CLAUDE-1: Claude Code integration implemented (2026-05-13)
- CODEX-1: Codex CLI integration implemented (2026-05-14)

MAC-1 delivered:
- `rmap doctor` health check command with JSON/human output
- `rmap uninstall` with manifest-driven paths, --dry-run, --force, --remove-data
- Platform adapter pattern (PlatformAdapter trait, macos.rs implementation)
- launchd service management (plist template, bootstrap/bootout)
- Install manifest with service metadata
- Claude Code hook schema in installer (correct schema)

HOOK-1A delivered (2026-05-13):
- `--from-stdin` flag on all hook commands
- `StdinPayload` parsing from stdin JSON
- Normalization to existing `HookContext` (policy handlers unchanged)
- Precedence: explicit args > stdin payload > env transport > discovery
- Both Claude Code and Codex use stdin JSON (verified May 2026)

HOOK-1 delivered:
- All six hook commands: session-start, prompt-submit, post-edit, pre-compact, stop, status
- Full resolution chain: explicit args → RMAP_* env → host env → discovery
- hooks.toml configuration infrastructure
- Session state persistence
- --classify, --prompt, --require-validation, --transcript flags
- Platform-native directories per DIST-1 D3

### Execution Order

1. ~~DIST-1 — contract before implementation~~ (IMPLEMENTED)
2. ~~HOST-1 — host integration rules before host-specific work~~ (IMPLEMENTED)
3. ~~REL-1 — release artifacts and verification~~ (IMPLEMENTED)
4. ~~REL-SUPPORT-1 — version authority enforcement (v0.1.1)~~ (IMPLEMENTED)
5. ~~RGISTR-1 — rgistr binary packaging~~ (IMPLEMENTED)
6. ~~RMAPD-1 — daemon binary target~~ (IMPLEMENTED)
7. ~~HOOK-1 — `rmap hook` commands before host shims use them~~ (IMPLEMENTED)
8. ~~MAC-1 — macOS installer + daemon service~~ (IMPLEMENTED)
9. ~~HOOK-1A — stdin JSON transport for Claude Code~~ (IMPLEMENTED)
10. ~~CLAUDE-1 — Claude Code integration on macOS~~ (IMPLEMENTED)
11. ~~CODEX-1 — Codex integration on macOS~~ (IMPLEMENTED)
12. ~~LINUX-1 — Linux installer + daemon service~~ (IMPLEMENTED)
13. ~~RMAPD-2 — Unix socket transport~~ (IMPLEMENTED)
14. ~~REG-1 — Repo registry + cwd auto-discovery (read-side)~~ (IMPLEMENTED)
15. ~~CLI-OUT-1 — Presentation layer (human-default output)~~ (IMPLEMENTED)
16. ~~CLI-OUT-2A — Cross-repo output audit~~ (HANDOFF COMPLETE)
17. ~~CLI-OUT-2B — First-contact discovery output~~ (IMPLEMENTED)
18. ~~CLI-OUT-2C — stats renderer~~ (IMPLEMENTED)
19. ~~CLI-OUT-3 — Graph drilldown output~~ (IMPLEMENTED)
20. ~~CLI-OUT-4 — Module/architecture output~~ (COMPLETE)
21. **CLI-OUT-5 — Inventory/policy output (IN PROGRESS)**
22. SMOKE-1 — Validation harness cleanup (QUEUED)
23. CURSOR-1 — Cursor integration (QUEUED)

### Artifact Matrix (REL-1)

**Must-have:**
- macOS ARM64 (Apple Silicon)
- Linux x86_64

**Later:**
- macOS x86_64
- Windows x86_64 (deferred with WIN-1)

### Design Principles

See `docs/VISION.md` § Distribution and Host Integration for:
- Binary-first distribution rationale
- Host integration safety rules
- Hook integration model (thin shim + `rmap hook` policy)
- Enforcement progression (informational first)
- Trust boundary rules

## Current state (as of last commit)

- **1464 tests** across 78 test files.
- **Shipped language support:** TS/JS, Rust, Java, Python, C, and C++ are fully operational
  in `rmap`. Mobile track languages (Objective-C, Swift, Kotlin, Dart) are not yet implemented.
- **Enrichment:** Rust operational (~77% on repo-graph, warm-up fixed), TS has ownership
  resolver for multi-tsconfig repos (explicit failure on unowned/ambiguous files), Java
  operational but requires explicit jdtls path. All three wired.
- **Repo-root anchoring:** Fixed. CLI computes and stores `repos.root_path` relative to the DB
  file location at index/refresh time. Filesystem-backed surfaces (docs, churn, coverage, risk,
  hotspots) resolve using that DB-relative path. Portable DB + cwd-independent resolution.
- **Classifier version:** 6.
  - v4: language-aware imports
  - v5: Rust crate-internal module heuristic
  - v6: hyphen-normalized Cargo deps + heuristic basis-code distinction
- **Boundary interaction model:** HTTP slice shipped (Spring + Express providers, TS consumers).
  - glamCRM: 97 providers, 85 consumers, 80 links (82.5% provider, 94.1% consumer match rate)
  - fraktag: 47 providers, 42 consumers, 41 links (70.2% provider, 97.6% consumer match rate)
  Local IPC slice (BI-1A) shipped: Unix sockets, named/anonymous pipes, shared memory,
  message queues. C extractor + `rmap boundaries` CLI surface. Validated on swupdate
  (14 surfaces across 12 files).
  TCP/UDP socket detection (BI-1B) shipped: multi-binding table, socket_type
  extraction (SOCK_STREAM, SOCK_DGRAM), guard predicates for AF_INET/AF_INET6.
  Phase 2: FD role tracking implemented — function-local fd registry tracks
  socket lineages, accumulates bind/listen/connect evidence, refines direction:
  TCP server (bind+listen) → Provider, TCP client (connect) → Consumer,
  UDP → Bidirectional (no strong role semantics). D3: bind alone insufficient
  for provider classification. CLI supports `--kind tcp` and `--kind udp` filters.
  AMQP/RabbitMQ detection (MB-1A) shipped: amqplib patterns (sendToQueue, publish,
  consume, assertQueue, assertExchange, bindQueue). Channel kind `amqp_queue`,
  protocol family `message_broker`. Validated on rabbitmq-tutorials (31 surfaces).
  Kafka detection (MB-2A) shipped: kafkajs patterns (send, subscribe) with triple
  scope guard (import + receiver provenance + topic evidence). Channel kind
  `kafka_topic`, interaction pattern `publish_subscribe`. Receiver provenance
  tracks `*.producer()` and `*.consumer()` factory assignments in same file.
  Deferred: `sendBatch` (nested topic extraction), `run()` (no topic evidence).
  NATS detection (MB-3A) shipped: nats npm package patterns (publish, subscribe)
  with triple scope guard (import + connection provenance + subject evidence).
  Channel kind `nats_subject`, interaction pattern `publish_subscribe`. Connection
  provenance tracks `connect()` assignments in same file. Deferred: `request()`
  (mixed semantics — outbound + reply handling).
- **Spring framework detectors:** container-managed bean liveness via `@Component`, `@Service`,
  `@Repository`, `@Configuration`, `@RestController`, `@Controller`, `@Bean`. Suppresses 93
  false dead-code reports on glamCRM. Also fixes Lambda entrypoint suppression in `findDeadNodes`.
- **Imported free-function call resolution:** TS/JS import-binding-assisted call
  resolution. repo-graph call_resolution_rate improved from ~15% to 33%.
- **Quality discovery substrate:** measurement storage, AST-derived
  cyclomatic complexity, cognitive complexity, nesting depth, parameter count,
  function length, coverage, churn, hotspots, risk. Quality-policy declarations
  and policy-backed assessments shipped as governance substrate
  (`rmap declare quality-policy`, `rmap assess`). Missing discovery layer:
  snapshot-to-snapshot quality diff, quality delta surfacing in `orient`/`check`,
  risk prioritization combining complexity with churn/coverage/boundary violations.
  LOC/SLOC-style size measurement is still missing from the surfaced discovery set even
  though it is cheap and deterministic.
- **Streaming/batched indexing pipeline:** Linux kernel indexes successfully.
  - Resolver index built from row-at-a-time DB iterator (no bulk `.all()`).
  - Staged edges resolved in cursor-based batches (10K default).
  - Classification loads file signals per-batch from DB (migration 010:
    packageDependenciesJson + tsconfigAliasesJson in file_signals table).
    Same-file symbol sets rebuilt from persisted nodes. No snapshot-wide
    fileSignalsCache on the classification hot path.
  - Detector/boundary passes use per-file `querySymbolsByFile`.
  - Dead Phase 1 in-memory maps eliminated.
  - Multi-batch seam tests verify count/breakdown parity at batch sizes 1 and 3.
- **Python two-pass extraction:** last-definition-wins semantics for same-scope function
  and class redefinitions. Shadowed definitions suppressed (no node, no edges, no metrics).
- **Repo set:** amodx, fraktag, glamCRM, repo-graph, mempalace, glam-scrapers,
  unelte, swupdate, buildroot, C++11 Deep Dives, **Linux kernel**.
  - TS-only, TS+Rust, TS+Java, Python, C/C++, mixed multi-language.
  - C validated strongly on swupdate (208 files, 3422 nodes) and buildroot (645 files, 5249 nodes).
  - C++ extractor shipped and validated: classes, methods, constructors, destructors,
    namespace-qualified names, extern "C" linkage metadata, file-level C ABI boundary
    statistics. Validated on tier-1 repos: leveldb (133 files, 1670 nodes), poco (3267
    files, 27565 nodes), duckdb (5109 files, 76556 nodes).
  - Linux kernel: 63,701 files, 1,045,482 nodes, 2,045,964 resolved edges,
    2,775,402 unresolved edges. Indexed in 77 min. Syntax-only (no compile_commands.json
    in this run). High unresolved rate expected without build-system context.
- **Framework detection:** Express routes, Lambda handlers, Spring beans, pytest tests/fixtures,
  Linux kernel system patterns (module_init, platform_driver, GCC constructor/destructor).
  Framework detectors suppress false positives in internal dead-code substrate; they are
  NOT sufficient for public dead-code claims (see Dead-code surface withdrawal below).
- **Dead-code surface withdrawal:** All public dead-code vocabulary removed from `rmap`.
  `rmap dead` disabled, `DEAD_CODE` signal removed from orient, `DEAD_CODE_RELIABILITY`
  removed from check. Internal substrate preserved. Reintroduction blocked on measured
  execution evidence (line or function coverage). See `docs/TECH-DEBT.md` for policy.
- **Native Python extraction in `rmap`:** Rust-side tree-sitter-python extractor with
  Python import resolution through shared file-resolution model. Extracts functions,
  classes, methods, constructors (`__init__`), variables (local and module-level,
  including annotated assignments), imports, calls. Complexity metrics (cyclomatic,
  nesting depth, parameter count, function length) computed and persisted to
  measurements table. PY-EXT-2 functional implementation complete; performance
  validation deferred (see `PY-EXT-2-PERF` in execution queue).
- **Documentation-first direction:** docs inventory is primary orientation evidence. Current
  discovery-oriented authored knowledge is still split between documents and declaration rows.
  Direction: hand-discovered architectural knowledge should move toward document-backed items,
  with DB projections kept for indexing, query acceleration, and governance substrate.
- **Daemon purpose clarified:** the daemon is not just warmed startup. It is the future
  multi-agent coordination authority for shared repo databases, with many readers, fewer
  writers, and daemon-owned synchronization over SQLite access.
- **CLI boundary model:** HTTP + cli_command mechanisms. Commander providers, package.json
  script consumers, shell script consumers, Makefile recipe consumers. Binary-prefix matching.
- **Module discovery:** Declared modules detected from manifests/workspaces
  (package.json, pnpm-workspace.yaml, Cargo.toml, settings.gradle, pyproject.toml).
  Module candidates, evidence, and file ownership persisted in dedicated tables
  (migration 011). CLI surface: `rgr modules list|evidence|files`.
  Additive — coexists with directory MODULE nodes.
- **Key architectural decisions made:**
  - Nearest-owning per-file context for deps, tsconfig, Cargo, Gradle, pyproject.toml.
  - Enrichment is a separate `rgr enrich` pass, not inline with indexing.
  - Edge promotion (D2c) is opt-in via `--promote`, 8-gate safety filter.
  - Boundary facts stored separately from core edges table (derived links discardable).
  - Large-file guard: files > 1MB skipped during indexing (operational containment).
  - Imported free-function calls resolved via import bindings (no compiler needed).
  - Sharded indexing (shard-local extraction + global integration pass) is the target
    architecture for Linux-scale repos beyond the current streaming pipeline.

## Shipped

### Rust structural CLI (`rmap-structural-v1`)
- 10 commands: index, refresh, trust, callers, callees, path, imports, dead, cycles, stats
- JSON-only output with TS-compatible QueryResult envelopes
- Exact symbol resolution (SYMBOL-only, 3-step)
- Edge-type filters on callers/callees (CALLS, INSTANTIATES)
- Shortest-path BFS (CALLS+IMPORTS, depth 8)
- Module-level cycle detection, structural metrics, dead-code analysis
- Cross-runtime interop proven (19 tests: Rust writes, TS reads + formats)
- Contract tests, envelope tests, per-command deterministic test matrices
- Milestone doc: `docs/milestones/rmap-structural-v1.md`
- Built across Rust-7B through Rust-20 (14 slices)

### Rust governance CLI (post-v1)
- `violations`: boundary violation check via declared forbidden IMPORTS
  (Rust-22). Reads boundary declarations, queries cross-module
  IMPORTS edges, reports violations.
- `gate`: CI gate with obligation evaluation (Rust-24). Narrow first
  gate: arch_violations method only, default mode only.
  Exit codes: 0 pass, 1 fail, 2 incomplete. TS-compatible gate
  report shape with toolchain, computed/effective verdicts.
- `gate`: Rust-25 adds active waiver resolution. Exact 3-tuple
  match (req_id, requirement_version, obligation_id). Expiry via
  lexicographic ISO 8601 comparison. waiver_basis audit trail in
  output. Deliberate divergence from TS: PASS obligations are not
  waivable (corrected policy model, see TECH-DEBT.md).
- `gate`: Rust-26 adds strict/advisory modes via `--strict` and
  `--advisory` flags (mutually exclusive). Default mode unchanged.
  Strict: MISSING_EVIDENCE/UNSUPPORTED treated as fail (exit 1).
  Advisory: MISSING_EVIDENCE/UNSUPPORTED informational (exit 0).
  WAIVED non-failing in all three modes. Mirrors TS flag interface.
- `gate`: Rust-28 adds `coverage_threshold` method. Reads
  `line_coverage` measurements from the measurements table (Rust-27
  plumbing). Filters by target path prefix, computes average, compares
  against threshold with operator (default `>=`). Evidence includes
  avg_coverage, threshold, operator, files_measured.
- `gate`: Rust-29 adds `complexity_threshold` method. Reads
  `cyclomatic_complexity` measurements, finds max across matching
  functions (prefix filter), compares against threshold with operator
  (default `<=`). Evidence: max_complexity, threshold, operator,
  functions_measured. Strict parsing on malformed measurement JSON.
  Uses `starts_with` for prefix matching (TS uses `includes` which
  is arguably a bug).
- `gate`: Rust-31 adds `hotspot_threshold` method. Reads
  `hotspot_score` inferences (Rust-30 plumbing), finds max
  `normalized_score`. Target optional (whole-repo if omitted).
  Default operator `<=`. Evidence: max_hotspot_score, threshold.
  Strict parsing on malformed inference JSON. This completes the
  TS gate method set: all four methods are now ported to Rust.
- `declare`: Rust-32 adds declaration write substrate in storage
  crate. `insert_declaration` with deterministic UUID v5 UIDs
  (idempotent, INSERT OR IGNORE). `deactivate_declaration` for
  soft-delete. Supports boundary, requirement, waiver kinds.
  Deliberate divergence from TS random UIDs (see TECH-DEBT.md).
- `declare boundary`: Rust-33 adds the first governance write
  CLI command. `rmap declare boundary <db> <repo> <module>
  --forbids <target> [--reason <text>]`. Uses Rust-32 storage
  substrate with semantic identity key. Idempotent (reason text
  does not affect UID). JSON output with declaration_uid, kind,
  target, forbids, inserted. Proven end-to-end: declared boundary
  is visible to `violations` command.
- `declare requirement`: Rust-34. Single-obligation requirement
  per command. Required: --version, --obligation-id, --method,
  --obligation. Optional: --target, --threshold, --operator.
  Operator validated against `>=, >, <=, <, ==`. Threshold
  validated as number. Identity: `(repo, req_id, version)` only —
  obligation text/method/target do not affect UID. Idempotent.
  Proven end-to-end: declared requirement visible to `gate`.
- `declare waiver`: Rust-35. Required: --requirement-version,
  --obligation-id, --reason. Optional: --expires-at, --created-by,
  --rationale-category, --policy-basis. Identity:
  `(repo, req_id, requirement_version, obligation_id)` — reason
  and optional fields do not affect UID. Idempotent. Proven
  end-to-end: declare boundary + declare requirement + gate FAIL
  + declare waiver + gate PASS (WAIVED with waiver_basis). Expired
  waiver correctly ignored by gate.
  This completes the governance write surface: boundary,
  requirement, and waiver can all be authored from Rust CLI.
- `declare deactivate`: Rust-36. Soft-delete by UID. Idempotent:
  nonexistent/already-deactivated UID returns `deactivated: false`,
  exit 0. Proven end-to-end: deactivated boundary removes
  violations, deactivated requirement removes gate obligations,
  deactivated waiver restores gate failure.
- `declare supersede substrate`: Rust-37. Storage-level
  `get_declaration_by_uid` and `supersede_declaration`. Atomic
  transaction: verify active → insert new (fresh UUID v4) with
  `supersedes_uid` → deactivate old. Old missing or inactive
  returns `SupersedeError`. 8 new tests including active-query
  visibility and double-supersede rejection.
- `declare supersede boundary`: Rust-38. Reads old row via
  `get_declaration_by_uid`, validates kind=boundary + active +
  parseable MODULE target key. Inherits repo_uid and module_path
  from old row. Builds replacement with new --forbids and optional
  --reason. Calls `supersede_declaration` (Rust-37 substrate).
  Proven end-to-end: old boundary produced violations, superseded
  boundary to non-imported module eliminates violations.
- `declare supersede requirement`: Rust-39. Reads old row,
  validates kind=requirement + active + parseable value_json
  with req_id and version. Inherits repo_uid, req_id, version
  from old row. Builds replacement with new obligation. Proven
  end-to-end: old requirement targeting src/core (PASS) superseded
  to target src/adapters (FAIL) — gate sees replacement only.
- `declare supersede waiver`: Rust-40. Reads old row, validates
  kind=waiver + active + parseable value_json with req_id,
  requirement_version, obligation_id. Inherits identity from old
  row. Builds replacement with new reason and optional fields.
  Proven end-to-end: reason update visible in gate waiver_basis,
  supersede to expired expiry restores gate failure.
  This completes the declaration lifecycle surface: create,
  deactivate, and supersede for all three governance kinds.
- Binding direction: declaration rows remain the governance/enforcement
  substrate. Discovery-oriented authored architectural knowledge should
  not keep expanding as opaque DB-only declarations when a document-backed
  surface is more appropriate.
- Deferred: multi-obligation requirements, evidence, obligations
- Deferred: measurement commands, table output, full edge-type set

### State-boundary extraction (`rmap-state-boundaries-v1`)
- Fork 1 (TS-only) delivered: TS/TSX/JS/JSX state-boundary edges
- New node kinds: DB_RESOURCE, FS_PATH, BLOB, STATE+CACHE subtype
- Edge types: READS, WRITES (targeting resource nodes)
- Form A matcher: import-anchored calls with literal arguments
- Binding table: TS/JS entries for fs, node:fs, fs/promises,
  node:fs/promises (stdlib FS only). SDK/DB/cache entries deferred
  pending object-property extraction and constructor tracking.
- CLI: `--edge-types READS,WRITES` on callers/callees, resource kinds
  excluded from `rmap dead`, `rmap resource readers/writers` commands
- Contract frozen at `state_boundary_version: 1`
- Milestone doc: `docs/milestones/rmap-state-boundaries-v1.md`
- Built across SB-0 through SB-6 (7 slices, plus SB-3-pre prerequisite)

### Multi-language graph engine
- TypeScript/JavaScript extractor (tree-sitter, syntax-only)
- Rust extractor: TS-side (tree-sitter-rust) for `rgr`, Rust-side (native
  tree-sitter) for `rmap`. Both share extraction scope: structs, enums,
  traits, impl methods, functions, consts, statics, type aliases, use
  imports, call edges, implements edges.
- Python extractor: TS-side (tree-sitter-python) for `rgr`, Rust-side (native
  tree-sitter) for `rmap`. Rust-side scope: functions, classes, methods,
  imports, calls. Python import resolution through shared file-resolution
  model. TS-side has broader scope (constructors, variables, complexity
  metrics) not yet ported to Rust.
- Java extractor (tree-sitter-java)
- Multi-extractor indexer: routes files by extension
- Language-aware manifest isolation (.rs → Cargo.toml, .java → build.gradle, .ts → package.json)
- **Cargo dependency resolution (Slice A):** `rmap index` resolves nearest-
  ancestor Cargo.toml for Rust files. Hyphen-to-underscore normalization.
  Handles dependencies, dev-dependencies, build-dependencies sections.
- Java overload disambiguation via parameter type signatures in stable keys
- Gradle dependency reader (Groovy + Kotlin DSL) with 2-segment prefix heuristic

### Unresolved-edge classification (classifier v6)
- 4-bucket vocabulary: external_library_candidate, internal_candidate,
  framework_boundary_candidate, unknown
- Per-file signals: import bindings, same-file symbols (subtype-aware),
  package dependencies (nearest-owning), tsconfig aliases (nearest-owning
  with extends)
- Runtime builtins: ES/Node/browser globals, Node stdlib, Rust std/core/alloc
- Language-aware import classification: relative, package dep, runtime stdlib,
  project alias, Rust crate-internal heuristic, unknown
- Hyphen-normalized Cargo dep matching (my-crate ↔ my_crate)
- Basis codes: per-rule audit trail on every classified edge, with distinct
  heuristic vs definite basis for Rust internal imports

### Trust reporting
- `rgr trust <repo>`: reliability axes, downgrade triggers, category/classification
  counts, blast-radius breakdown, enrichment status
- `rgr trust unresolved-samples <repo>`: per-edge samples with source file, line,
  classification, basis, blast-radius, receiver type (when enriched)
- Variant A call-graph reweighting: externals excluded from denominator

### Framework detection
- Express route/middleware registration (edge-level, receiver-provenance gated)
- Lambda exported handler entrypoints (node-level, inference-based)
- Spring container-managed bean detection: `@Component`, `@Service`, `@Repository`,
  `@Configuration`, `@RestController`, `@Controller`, `@Bean` factory methods.
  Emitted as inferences (kind: `spring_container_managed`). Suppresses false
  dead-code reports via `findDeadNodes` inference exclusion.
- `findDeadNodes` now consults framework-liveness inferences (both `framework_entrypoint`
  and `spring_container_managed`), not just entrypoint declarations. This also fixes
  the pre-existing Lambda entrypoint gap where detected entrypoints were persisted but
  not consumed by dead-code analysis.

### Boundary interaction model (HTTP slice — mature)
- Generic boundary-fact architecture: provider facts, consumer facts, derived links.
- Separate storage: `boundary_provider_facts`, `boundary_consumer_facts` (source of
  truth), `boundary_links` (derived artifact, discardable).
- NOT in the core `edges` table. Boundary links are protocol-level inferred facts,
  not language-level extraction edges.
- Mechanism-keyed matcher: `BoundaryMatchStrategy` interface, `HttpBoundaryMatchStrategy`
  first implementation. Strategy pattern for future gRPC, IOCTL, CLI mechanisms.
  Link candidates carry stable persisted fact UIDs, not object references.
- Structured segment normalization: Spring `{id}`, Express `:id`, consumer `{param}`
  all normalize to `{_}` for matching. Raw paths preserved in facts.
- HTTP provider extractors:
  - Spring: AST-backed via tree-sitter-java (MATURE). Handles multiline annotations,
    `value=`/`path=` attributes, `method=RequestMethod.X`. Known double-parse
    inefficiency (Java extractor does not expose parse tree to route extractor).
  - Express: regex-based (PROTOTYPE). Receiver provenance gated to `app`, `router`,
    `server`. Express import gate prevents false positives. Consumes FileLocalStringResolver
    for constant-backed route paths.
- HTTP consumer extractor: `axios.get/post/put/delete/patch`, `fetch()` (PROTOTYPE,
  regex). Supports bare identifier URL arguments resolved via binding table.
- FileLocalStringResolver: reusable support module for file-local constant string
  propagation. Resolves `const`, template literals, binary `+`, chained bindings,
  env-var prefix stripping. Used by both consumer and Express provider extractors.
- Indexer wiring: boundary extraction runs during indexing, facts persisted, intra-repo
  links materialized as convenience.
- CLI surface: `rgr boundary summary|providers|consumers|links|unmatched`.
- Validated on glamCRM: 97 providers, 85 consumers, 80 links, 82.5%/94.1% match rates.
- Validated on fraktag: 47 providers, 42 consumers, 41 links, 70.2%/97.6% match rates.

### Boundary interaction model (Local IPC slice — BI-1A)
- Two-level model: BoundaryInteractionSurface (Level 1) + ChannelDetail (Level 2).
- Surfaces capture architectural relationships (what/where); channels capture
  mechanism-specific addressing (socket paths, shm keys, pipe descriptors).
- Storage: `boundary_interaction_surfaces`, `boundary_channel_details` tables.
- C extractor: binding-table-driven detection for POSIX IPC APIs.
- Slice 1A scope (local IPC):
  - Unix domain sockets (socket AF_UNIX, bind, connect, listen, accept)
  - Named pipes / FIFOs (mkfifo, open O_WRONLY|O_RDONLY)
  - Anonymous pipes (pipe, pipe2)
  - POSIX shared memory (shm_open, mmap MAP_SHARED)
  - POSIX message queues (mq_open, mq_send, mq_receive)
- CLI surface: `rmap boundaries list|show|summary` with filters.
  - `--kind` (unix_socket, named_pipe, shared_memory, message_queue, anonymous_pipe)
  - `--scope` (inter_process, inter_device, unknown)
  - `--direction` (provider, consumer, bidirectional)
  - `--family` (socket, pipe, shared_memory, message_queue)
  - `--file`, `--file-prefix` (path filtering)
  - `--symbol` (enclosing symbol stable key exact match)
- Validated on:
  - swupdate: 14 surfaces (5 unix_socket, 6 anonymous_pipe, 1 named_pipe, 2 shared_memory)
  - sqlite: 7 surfaces (7 anonymous_pipe)
  - nginx: 7 surfaces (6 shared_memory, 1 anonymous_pipe)
- Maturity: MATURE (22 CLI tests, read-side port trait, explicit-degradation on
  unknown enum values, deterministic ordering).
- Design doc: `docs/design/boundary-interaction-ipc-device.md`.
- BI-1B (TCP/UDP sockets): Shipped. Detects socket() with AF_INET/AF_INET6 +
  SOCK_STREAM (TCP) or SOCK_DGRAM (UDP). Phase 2 FD role tracking: function-local
  fd registry accumulates bind/listen/connect evidence, refines direction to
  Provider (bind+listen) or Consumer (connect). UDP stays Bidirectional.
- Explicit exclusions (deferred to later slices):
  - Serial/CAN (Slice 2 — inter_device scope)
  - MQTT/ZeroMQ/D-Bus (Slice 3 — library wrappers)
  - I2C/SPI/USB (Slice 4 — low-level device protocols)

### CLI boundary model (first slice)
- Mechanism: `cli_command` — second boundary mechanism after HTTP.
- Commander.js provider extractor: command registrations → provider facts. Handles
  nested command composition, positional args, options, descriptions. Import gate +
  receiver-variable tracking for parent-child command path composition.
- package.json script consumer extractor: direct tool invocations → consumer facts.
  Shell chain splitting (`&&`, `||`, `;`), npx/pnpm-exec/yarn-exec unwrapping,
  script-indirection filtering (npm run/pnpm run skipped), shell-builtin filtering.
- `CliBoundaryMatchStrategy`: exact command-path matching + guarded binary-prefix
  heuristic (strips leading binary from 3+ token consumer paths). Guard prevents
  false positives where 2-word external tool invocations (vite build, cargo test)
  would match single-word internal commands.
- Validated on repo-graph: 80 CLI providers, 15 CLI consumers, 9 CLI links.
- Known limitations: cross-file Commander composition (commands registered via function
  params from different modules), aliased receivers (const api = express.Router()),
  binary-prefix matching requires 3+ tokens.

### Python extractor (syntax-plus-dependency-context baseline)
- tree-sitter-python extractor: functions, classes, methods, constructors,
  variables, imports, calls, cyclomatic complexity, nesting depth.
- Python dependency reader: pyproject.toml ([project].dependencies +
  optional-dependencies) and requirements.txt. PEP 508 parsing.
- Python runtime builtins: 60+ identifiers + 80+ stdlib modules.
- Language-aware manifest isolation: .py → pyproject.toml/requirements.txt.
- Classifier integration: stdlib imports classified as external via runtime
  builtins, pip deps classified via specifier_matches_package_dependency,
  relative imports (.utils) classified as internal.
- Pytest detector: test_* functions, Test* classes, @pytest.fixture.
  Emitted as pytest_test/pytest_fixture inferences. Suppresses test code
  from dead-code reports.
- Grammar build infrastructure: scripts/build-grammars.mjs for reproducible
  WASM grammar builds from npm packages.
- Validated on: mempalace (30 files, 396 nodes), glam-scrapers, unelte,
  swupdate (18 Python files + shell scripts).
- Known limitation: Python package names do not map 1:1 to import specifiers
  (pyyaml → import yaml, beautifulsoup4 → import bs4). Exact name matches
  work; mismatches remain unclassified.

### CLI boundary expansion (shell scripts)
- Shell script consumer extractor: .sh/.bash files → cli_command consumer facts.
  Line-based conservative extraction with heredoc detection, continuation
  line skipping, control flow skipping, pipe exclusion, env-prefix stripping.
- Shared cli-invocation-parser support module: chain splitting, wrapper
  unwrapping, builtin filtering, command parsing. Reused by both package.json
  and shell script consumers.
- Validated on: swupdate (66 shell consumers from CI/build scripts),
  glamCRM (15 shell consumers from deploy/setup scripts).

### Makefile CLI consumer extraction
- Makefile recipe-line consumer extractor: tab-detection, @/-/+ prefix stripping,
  $(Q) quiet-prefix handling, $(VAR) expansion skipping, quoted-$(VAR) filtering,
  Make directive filtering. Reuses shared cli-invocation-parser.
- Validated on: buildroot (3410 Makefiles → 1551 CLI consumers), swupdate.

### Imported free-function call resolution (TS/JS)
- Import-binding-assisted call resolution for bare-identifier CALLS edges.
- When global name lookup is ambiguous (multiple same-name functions across files),
  import bindings disambiguate by narrowing to the imported source file.
- repo-graph call_resolution_rate: ~15% → 33%. Resolved calls: ~300 → 1004.
- No compiler enrichment needed — uses existing import bindings from extractors.
- Known limitation: aliased imports (`import { foo as bar }`) not yet resolved.

### C/C++ extractor (syntax-only first slice)
- tree-sitter-c + tree-sitter-cpp grammars (WASM, built via build-grammars.mjs).
- Extracts: functions, structs, classes, typedefs, enums, namespaces, methods,
  constructors, #include directives, CALLS edges, cyclomatic complexity.
- Dual grammar: .c/.h use C grammar, .cpp/.hpp/.cc/.cxx use C++ grammar.
- Preprocessor guard recursion: symbols inside #ifndef/#ifdef blocks are extracted.
- Stable-key dedup: typedef+struct and #ifdef branch collisions handled.
- Quoted #include classified as internal (rawPath metadata with "./" prefix).
- STL qualified calls (std::sort, std::make_unique) extracted and pinned.
- Large-file guard: files > 1MB skipped (operational containment for generated headers).
- Validated on: swupdate (208 files, 3422 nodes), buildroot (645 files, 5249 nodes),
  C++11 Deep Dives (165 files, 829 nodes).
- Linux kernel: 63k C files. Staged architecture removed primary memory
  bottleneck; runs ~18 min before runtime crash. Sharded indexing architecture
  is the target solution (see Next #1).

### Streaming/batched indexing pipeline (Linux-scale validated)
- Per-file persistence: read → extract → insertNodes → insertStagedEdges →
  insertFileSignals → discard source. Source text no longer retained across files.
- Staging tables: `staged_edges` and `file_signals` (migrations 009-010), both
  CASCADE-scoped to snapshot. Migration 010 added `package_dependencies_json`
  and `tsconfig_aliases_json` to `file_signals`.
- Resolver index built from row-at-a-time DB iterator (`queryResolverNodesIter`).
  No bulk `.all()` materialization. Peak memory = Maps only.
- Staged edges resolved in cursor-based batches (`queryStagedEdgesBatch`, default
  10K, configurable via `IndexOptions.edgeBatchSize`). Per batch: read → resolve →
  classify → persist → discard.
- Classification loads file signals per-batch from DB. Same-file symbol sets
  rebuilt from persisted nodes via `querySymbolsByFile`. No snapshot-wide
  `fileSignalsCache` on the classification hot path.
- `queryAllNodes` eliminated from all indexer call sites. Module-edge creation
  uses pre-built Maps from resolver iterator. Detector/boundary passes use
  per-file `querySymbolsByFile`.
- Dead Phase 1 in-memory maps removed: `resolverByStableKey`, `resolverByName`,
  `resolverNodeToFile`, `nodeUidToFileUid`.
- tree.delete() in try/finally across all 5 extractors (WASM heap hygiene).
- Parser reset every 5000 parses (C/C++ extractor).
- Read failures registered as FAILED file versions (not silently dropped).
- Oversized files registered as SKIPPED with isExcluded=true.
- Multi-batch seam tests: edgeBatchSize=1 and edgeBatchSize=3 verified to
  produce identical counts/breakdown vs default.
- **Validated on Linux kernel:** 63,701 files, 1,045,482 nodes, 2,045,964
  edges, 77 min. Exit code 0, clean stderr.

### Python two-pass last-definition-wins extraction
- Pre-scan determines winning (last) definition for each function/class name
  at every scope level (module root, class body).
- Shadowed definitions fully suppressed: no node, no edges, no metrics emitted.
- Matches Python runtime semantics (last same-name def at a scope shadows earlier).
- Handles decorated definitions (unwraps decorated_definition to inner def/class).
- No diagnostic channel for reporting shadowed definitions yet (TECH-DEBT).

### compile_commands.json reader
- Per-translation-unit include path and define extraction.
- Relative directory resolution against compile_commands.json location.
- Used by indexer for per-TU include resolution in C/C++ import targets.

### Linux system detector
- module_init/module_exit macro patterns.
- platform_driver registration.
- GCC constructor/destructor attributes.
- register_handler patterns.
- Emitted as `linux_system_managed` inferences. Suppresses false dead-code
  reports for kernel-managed symbols.

### C include resolution v1.2 (angle-bracket + configured roots)
- **v1.1** (conventional roots): same-dir → configured → conventional
  (`include/`, `inc/`, `src/include/`). Works for quoted-include codebases
  (swupdate, sqlite).
- **v1.2** (angle-bracket): both quoted and angle-bracket includes attempt
  resolution. Same-dir remains quote-only. Angle-bracket proceeds to
  configured/conventional roots.
- CLI: `--include-root <path>` for project-specific include directories.
- Validated on nginx: requires explicit roots (`--include-root src/core`
  etc.) because nginx has no conventional root directories.
- nginx v1.2 results: zero-connectivity modules 13→0, unresolved imports
  1,079→294, `src/core` fan_in 0→13.
- Trust labeling: ambiguous matches labeled `IMPORTS (ambiguous match)`.
- See `docs/milestones/c-include-resolution-v1.2.md` for design.

### Hotspot presentation filtering
- `--exclude-tests`: removes files with `is_test=true` (scanner-persisted
  metadata from path patterns: `.test.`, `.spec.`, `tests/`, `__tests__/`).
- `--exclude-vendored`: removes files under vendored path segments (exact
  segment match: `vendor`, `vendors`, `third_party`, `third-party`,
  `external`, `deps`, `node_modules`).
- View-policy only: filtering applied after scoring, before output.
  Stored measurements unchanged.
- Explicit opt-in: raw mode (no flags) is default, returns all hotspots.
- Output envelope includes `filtering` metadata when flags active (omitted
  when no flags). Per-filter counts: `excluded_tests_count`,
  `excluded_vendored_count`, `excluded_count` (union).
- Validated on swupdate (10 test files excluded, mongoose/ NOT excluded —
  correct, not a standard segment) and nginx (neutral baseline, 0 excluded).
- See `docs/milestones/hotspot-presentation-filtering.md` for design.

### Compiler enrichment
- `rgr enrich <repo>`: post-index receiver-type resolution
- TypeScript: via `ts.Program` / `TypeChecker` (~81% enrichment rate)
- Rust: via rust-analyzer LSP subprocess (~85% enrichment rate)
- Java: via Eclipse JDT Language Server (~76% enrichment rate on glamCRM)
- Per-project-context routing: one language server per nearest build manifest
- Safe-subset edge promotion (`--promote`): 8-gate filter, inferred resolution

### Blast-radius axis
- Query-time derived: receiver origin + enclosing scope significance
- Scoped to unknown CALLS (low = private scope, medium = exported scope)

### Module discovery support module (Layer 1 — declared)
- Core model: ModuleCandidate, ModuleCandidateEvidence, ModuleFileOwnership.
  Identity anchored by repo-relative root path, not package name.
  moduleKey format: `{repoUid}:{rootPath}:DISCOVERED_MODULE`.
- Pure detectors: package.json workspaces, pnpm-workspace.yaml, Cargo.toml
  (workspace + crate), settings.gradle, pyproject.toml ([project] + [tool.poetry]).
- Pure orchestrator: dedup by root path, evidence merging, longest-prefix
  file ownership assignment, confidence propagation.
- Discovery port: narrow interface, ManifestScanner adapter.
- Migration 011: module_candidates, module_candidate_evidence, module_file_ownership.
  All CASCADE on snapshot/repo deletion.
- Source-specific evidence attribution: pnpm vs package.json patterns expanded
  separately. Evidence attributed only to the source that matched.
- Cargo workspace root with [workspace] + [package] emits crate root.
- Additive: existing directory MODULE nodes unchanged.
- 57 tests: 33 detector unit, 16 orchestrator unit, 4 scanner attribution,
  8 integration (monorepo workspace + mixed-lang + rollups + owned files).
- Validated on repo-graph: 9 candidates, 10 evidence items, 228 ownership rows.

### Module discovery Layer 2 — operational modules
- Three-tier module kind: declared > operational > inferred.
- Layer 2 promotion: unattached project surfaces promote to operational
  module candidates when surface kind in {cli, backend_service, web_app, worker}
  and max evidence confidence >= 0.70.
- Confidence ceiling: Layer 2 modules capped at 0.85 (Layer 1 reaches 1.0).
- Exact-root collision dedup: surfaces at existing declared roots are not promoted.
- Same-pass re-linkage: after promotion, surfaces re-link to extended candidate set.
- Kind-precedence tiebreak: when declared and operational candidates share root,
  declared wins for file ownership (longest-prefix still primary).
- Pure orchestrator: `promoteUnattachedSurfaces` in `src/core/modules/operational-promotion.ts`.
- `assignFileOwnershipForCandidates` exported for pipeline use.
- PromotionDiagnostics: surfacesEvaluated, surfacesPromoted, surfacesSkippedKind,
  surfacesSkippedConfidence, promotionSkippedExistingRoot.
- 28 unit tests: 23 operational-promotion, 5 kind-precedence ownership.
- Contract: docs/architecture/module-discovery-layers.txt.

### Module graph and file-to-module ownership (feature slice 1)
- `rgr modules list <repo>` — catalog with rollups (file count, symbol count,
  test files, evidence count, languages, has-directory-module).
- `rgr modules evidence <repo> <module>` — evidence items per module.
- `rgr modules files <repo> <module>` — owned files with language, test flag,
  assignment kind, confidence. Targeted SQL query per module, not whole-snapshot.
- `queryModuleCandidateRollups` — single SQL with LEFT JOINs.
- Deterministic languages output (GROUP_CONCAT with ORDER BY).
- All commands support --json.
- No directory MODULE replacement. No module dependency edges. No arch violations.

### Operational dependency seams (env + fs feature surface)
- Cross-language detectors (TS/JS, Python, Rust, Java, C/C++) for environment
  variable accesses and filesystem mutations. Pure functions, line-based regex.
- Identity + evidence persistence pattern:
  - env identity: `(snapshot, surface, env_name)`, evidence per source occurrence
  - fs identity: `(snapshot, surface, target_path, mutation_kind)`, evidence
    per source occurrence including dynamic-path occurrences with null FK
- Pure linkage cores: `linkEnvDependencies`, `linkFsMutations`. Multi-surface
  linkage when files are shared across surfaces. Identity dedup per surface.
- Pure cross-surface aggregator: `aggregateEnvAcrossSurfaces`,
  `aggregateFsAcrossSurfaces`, `summarizeFsDynamicEvidence` in
  `src/core/seams/module-seam-rollup.ts`. Aggregation rules:
  - env accessKind: required-if-any > optional-if-any > unknown
  - env defaultValue: first non-null in deterministic surface order
  - env hasConflictingDefaults: true if ≥2 distinct non-null defaults
  - fs union by `(target_path, mutation_kind)` with destinationPaths union
  - fs dynamic summary aggregated across surfaces (totalCount, distinctFileCount, byKind)
- Storage migrations 015 (env) + 016 (fs). Targeted per-surface query methods
  for both env and fs evidence (`querySurfaceEnvEvidenceBySurface`,
  `querySurfaceFsMutationEvidenceBySurface`).
- Feature surface in CLI:
  - `rgr surfaces show <surface>` exposes direct env + fs sections
    (envDependencies, fsMutations.literal, fsMutations.dynamic) in both
    `--json` and human output.
  - `rgr modules show <module>` exposes module-level cross-surface rollup
    (rollup.envDependencies, rollup.fsMutations) plus per-surface direct
    env + fs blocks. Human output uses a compact `Seams: env=N fs=M +K dyn`
    breadcrumb per surface; `surfaces show` is the drill-down surface for
    full per-surface tables.
- Positional comment masker (`src/core/seams/comment-masker.ts`) pre-pass
  for both env and fs detectors. Masks line, block, and JSDoc comments
  while preserving newlines (line-number stability) and string literal
  contents (fs detectors depend on literal first-arg paths). C-style and
  Python language families.
- Test files excluded from seam detection at the indexer seam-pass entry
  point. The `isTestFile` heuristic recognizes `__tests__`, `.test.`,
  `.spec.`, `/test/`, `/tests/`, plus top-level `test/`, `tests/`, and
  `__tests__/` paths (the prior heuristic missed root-level conventions).
- See `docs/cli/v1-cli.txt` for the full JSON contract and worked examples.
- See `docs/TECH-DEBT.md` for deferred items (string-literal-embedded env
  false positives, detector externalization, jdtls live-test gating, Node
  version pinning, node:sqlite evaluation).

### Module discovery expansion (Layer 3 partial)
- **Layer 3 A1: Kbuild detector** — obj-y, obj-m, xxx-objs patterns for Linux
  kernel module boundaries. Pure detector, wired to indexer. Produces
  `kbuild` module candidates with evidence.
- **Layer 3 B1: directory detector** — heuristic module boundaries from
  directory structure (src/, lib/, packages/ with files). Pure detector,
  lower confidence than manifest-based. Fills gaps in repos without
  workspace manifests.
- **Module discovery diagnostics** — persisted via migration 017. Tracks
  skipped patterns, parse failures, confidence adjustments. CLI surface
  via `rgr modules diagnostics`.
- **Module CLI provenance visibility** — `source_type` and evidence chain
  exposed in `modules list` and `modules show` output.
- Deferred to future slice: Layer 3 C1 graph clustering, `__init__.py`
  evidence, GNU Makefile/CMake module discovery.

### Module graph — dependency edges and violations
- Cross-module IMPORTS edge derivation from file ownership.
- `rmap modules deps` command with `--outbound` / `--inbound` filters.
- `rmap modules violations` command for discovered-module boundary violations.
- `rmap violations` unified output includes both declared and discovered
  module violations in separate sections.
- Module boundary declarations via `rmap modules boundary` (semantic alias
  for `rmap declare boundary` with module-key target).
- Weighted neighbor computation (`import_count`, `source_file_count`) for
  `modules show` output.
- Violation diagnostics (`imports_edges_total`, `imports_source_no_module`,
  etc.) for degraded graph detection.
- P2/P3/P4 bug closeout: rollup degradation handling, policy parse failure
  recovery, deterministic ordering.

### Runtime/build Phase 0 — identity infrastructure
- Migration 018: `source_type`, `source_specific_id`, `stable_surface_key`
  columns on `project_surfaces` table. FK-safe table rebuild.
- `computeStableKey()` and `computeSourceSpecificId()` pure functions in
  `src/core/runtime/surface-identity.ts`.
- `DetectedSurface` now requires `sourceType` and source-specific identity
  fields. Nullable only on legacy persisted `ProjectSurface` rows.
- Fixture repair: existing detectors updated with explicit identity fields.

### Runtime/build Phase 1A — container detectors
- **Dockerfile detector** — `backend_service` or `cli` surface with
  `container` runtime. Base image runtime in evidence payload as
  `baseRuntimeKind`. CMD/ENTRYPOINT extraction.
- **docker-compose detector** — per-service surfaces from YAML. Service
  name as identity. Build context and image evidence.
- Indexer wiring: `detectDockerfileSurfaces()` and `detectDockerComposeSurfaces()`
  called during surface discovery pass.
- Validated on test fixtures.

### Runtime/build Phase 1B — script-only fallback
- **Package.json script-only fallback** — packages with `scripts` but no
  bin/main/exports/framework deps now produce a low-confidence surface.
- Detection order preserved: bin → main/exports → framework deps → scripts fallback.
- Fallback rules:
  - `scripts.start`/`dev`/`serve`/`server` → `backend_service`, confidence 0.55
  - Only build/test/lint scripts → `library`, confidence 0.50
- Evidence: one item per relevant script with `script_command` kind.
- Metadata includes `fallbackReason: "script_only_package"`.
- Unit tests (7) and integration tests (5) with dedicated fixtures.

### Runtime/build Phase 1C — Makefile v1
- **Makefile target detection** — conservative parsing, one surface per target.
- Identity: `makefilePath:targetName` prevents collapse of multiple targets.
- Recognized: `Makefile`, `makefile`, `GNUmakefile`. `*.mk` excluded.
- Target classification:
  - CLI targets: `run`, `serve`, `test`, `lint`, `install`, `deploy`, etc.
  - Library targets: `all`, `build`, `lib`, `static`, `shared`, etc.
  - Skipped: `clean`, `distclean`, pattern rules, variable-derived targets.
- RuntimeKind: `native_c_cpp` if C/C++ signals, else `node`/`python`/`unknown`.
- Diagnostics returned but not persisted (tech debt).
- Unit tests (20) and integration tests (5) with `makefile-basic/` fixture.

### Quality policy assessment surface (governance substrate)
Available governance/enforcement substrate. Not the primary product surface.
See `docs/VISION.md` §Absolute Priority for discovery-first positioning.

- `rmap declare quality-policy` — policy declarations over measurements.
  Supported policy kinds: `absolute_max`, `absolute_min`, `no_new`,
  `no_worsened`. Supported measurements: `cognitive_complexity`,
  `cyclomatic_complexity`, `function_length`, `max_nesting_depth`,
  `parameter_count`. Policies produce assessments, not mutations.
- `rmap assess` — evaluate declared policies against latest snapshot.
  Produces per-policy verdicts (PASS/FAIL/NOT_APPLICABLE/NOT_COMPARABLE).
  Comparative policies (`no_new`, `no_worsened`) require `--baseline`.
  Assessments persisted to `quality_assessments` table with computed
  verdict, evidence, and optional baseline reference.
- Baseline validation: nonexistent, wrong-repo, and non-ready baselines
  rejected at assessment time (exit 2), not silently degraded.
- Storage: quality policies stored in `declarations` table with
  `kind='quality_policy'` (shared declaration lifecycle with boundary,
  requirement, waiver kinds). `quality_assessments` is the dedicated
  assessment result table with FK to snapshot and policy declaration.

This is the enforcement substrate for teams that need CI blocking and
formal compliance workflows. Discovery (snapshot diff, quality delta
surfacing, risk prioritization) is the primary product direction.

### Rust CLI surfaces parity
- `rmap surfaces list` — surface catalog with filters (`--kind`, `--runtime`,
  `--source`, `--module`). Module enrichment via JOIN. Evidence count.
  Deterministic ordering.
- `rmap surfaces show` — surface detail with module ref, evidence items,
  parsed metadata. Multi-strategy surface ref resolution (UID, UID prefix,
  stable key, stable key prefix, display name). Ambiguity handling.
- Repo-ref resolution: supports UID, name, or root_path.
- 22 CLI regression tests covering filters, ambiguity, legacy NULL fields,
  invalid metadata preservation.

### Runtime/build v1 closeout
- **Status:** COMPLETE. 196 tests passing.
- **Closeout doc:** `docs/validation/runtime-build-v1-closeout.md`
- **Deferred items (explicitly not v1):**
  - CMake File API: requires execution boundary design (running `cmake -B`)
  - Workspace membership: module discovery enrichment, not runtime surface
  - Infra roots (Terraform, Pulumi, Helm): separate deployment-surface slice
  - Makefile diagnostics persistence: tech debt for v2

### Dead-code surface withdrawal (Option D)
- **Status:** COMPLETE. All public dead-code vocabulary removed from `rmap`.
- Public surfaces withdrawn:
  - `rmap dead` command disabled (returns exit 2)
  - `DEAD_CODE` signal removed from orient
  - `DEAD_CODE_UNRELIABLE` limit removed from orient
  - `DEAD_CODE_RELIABILITY` condition removed from check
  - `dead_code_reliability` field removed from explain trust evidence
  - `dead_code` field hidden from trust JSON output
- Internal substrate preserved for future coverage-backed reintroduction
- Framework detectors and entrypoint declarations remain active for liveness
  suppression in the internal substrate
- **Binding policy:** Public dead-code surfaces blocked until measured
  execution evidence (line or function coverage) exists in the Rust product
  path. See `docs/TECH-DEBT.md` §Dead-code surface withdrawal for full policy.

## Next

### Immediate: Gap-Closing and Dependency Surface

The next execution priority is **closing TS/Rust extraction gaps** and **surfacing
dependency relationships**. This strengthens Layer 0–2 facts before expanding
Layer 3 framework detection.

**Execution queue (in order):**

| Slice | Scope | Layer | Status |
|-------|-------|-------|--------|
| PY-EXT-2 | Python extractor depth | L0–1 | IMPLEMENTED (functional) |
| PY-EXT-2-PERF | Python extractor performance validation | L0–1 | DEFERRED |
| SB-7A | State boundaries support substrate | L2 | **SHIPPED** |
| SB-7C | Python state boundaries | L2 | **SHIPPED** |
| DEP-1 | Dependency reconciliation surface | L2 | **SHIPPED** |
| JE-1 | Java resolved callsites | L0–1 | **IMPLEMENTED** |
| SB-7B | Java state boundaries | L2 | **SHIPPED** |
| CPP-SB-1 | C++ state boundaries | L2 | **SHIPPED** |
| FD-1A | Rust Express detector parity | L3 | **SHIPPED** |
| FD-1B | Rust React detector parity | L3 | **IMPLEMENTED** |

**PY-EXT-2-PERF note:** Performance acceptance (throughput ≥ 0.95x, memory ≤ 1.1x)
was not validated because no baseline exists and the original benchmark command was
invalid (library crate has no binary). Deferred until a proper benchmark harness
and pre-change baseline are established. Functional Layer 0–1 work is complete.

**Why this order:**

1. **PY-EXT-2** strengthens Layer 0–1 facts — COMPLETE (functional), performance deferred
2. **SB-7A** creates Layer 2 support substrate — SHIPPED
3. **SB-7C** uses SB-7A substrate for Python state boundaries — SHIPPED
4. **DEP-1** promoted: cross-cutting query surface over existing facts, no extractor surgery, immediate value across JS/TS and Rust repos
5. **JE-1** implemented: Java extractor now emits `ResolvedCallsite` facts with arg0 payload and import resolution.
6. **SB-7B** shipped: narrow first-cut (`DriverManager.getConnection(String)` only) complete.
7. **CPP-SB-1** shipped: C++ stream family (ifstream, ofstream, fstream constructors + .open()) with D3 intra-function local type map. Duplicated C bindings for language="cpp". 20 E2E tests.
8. **FD-1A** shipped: AST-based Express detection with module resolution, parity-validated against TS prototype.
9. **FD-1B** implemented: React component/hook detection via AST, 10 components + 14 hooks in validation corpus. Uses `inferences` table (Layer 3).
10. **PY-EXT-2-PERF** is backlog — requires benchmark harness infrastructure before execution

**Slice docs:**

- `docs/slices/py-ext-2-python-extractor-depth.md`
- `docs/shipped/slices/sb-7a-state-boundaries-support-substrate.md`
- `docs/shipped/slices/sb-7c-python-state-boundaries.md`
- `docs/shipped/slices/dep-1-dependency-reconciliation-surface.md`
- `docs/slices/je-1-java-resolved-callsites.md`
- `docs/shipped/slices/sb-7b-java-state-boundaries.md`
- `docs/shipped/slices/cpp-sb-1-cpp-state-boundaries.md`
- `docs/shipped/slices/fd-1a-rust-express-detector-parity.md`
- `docs/slices/fd-1b-rust-react-detector-parity.md`

---

### Backlog: Framework Detection Follow-on Slices

FD-1A is SHIPPED (parity-validated). FD-1B is IMPLEMENTED. These follow-on slices
complete the framework detection work:

| Slice | Type | Scope | Status |
|-------|------|-------|--------|
| FD-1A-PARITY | Validation | Rust vs TS Express detector comparison | **COMPLETED** |
| FD-SUPPORT-EXT-JSTS | Support | Unified JS/TS extension contract | **IMPLEMENTED** |
| FD-1B-EXT | Feature | React detector extension widening | **IMPLEMENTED** |
| FD-SUPPORT-3 | Support | CLI regression tests for `rmap inferences` | **IMPLEMENTED** |

**Dependency chain:**

1. **FD-1A-PARITY** — standalone validation, may trigger FD-1A-FIX if gaps found
2. **FD-SUPPORT-EXT-JSTS** — support substrate for extension handling
3. **FD-1B-EXT** — depends on FD-SUPPORT-EXT-JSTS
4. **FD-SUPPORT-3** — standalone, can execute independently

**Note:** Implementation priority is NOT determined by this ordering. These are
backlog slices with captured scope, not prioritized execution queue items.

**Slice docs:**

- `docs/slices/fd-1a-parity-validation.md`
- `docs/slices/fd-support-ext-jsts.md`
- `docs/slices/fd-1b-ext-react-extension-widening.md`
- `docs/slices/fd-support-3-inferences-cli-regression.md`

---

### Shipped: Artifact Contract Registry (ACR)

The **Artifact Contract Registry** codifies artifact semantics in code rather than prose.
All slices shipped.

**Architecture docs:**
- `docs/architecture/artifact-contract-model.md` — full specification
- `docs/architecture/adr/adr-artifact-contract-registry.md` — decision record

**Execution slices:**

| Slice | Scope | Status |
|-------|-------|--------|
| ACR-1 | Create `artifact-contracts` crate with registry | **SHIPPED** |
| ACR-2 | Make refresh pipeline consume registry | **SHIPPED** |
| ACR-3 | Add per-row freshness and provenance schema | **SHIPPED** |
| ACR-4 | Implement impact propagation from L0 changes | **SHIPPED** |
| ACR-5 | Boundary contract proof case (first fix) | **SHIPPED** |
| ACR-6 | Wire query surfaces to report freshness/degradation | **SHIPPED** |

---

### Shipped: Module Truth-Model Unification

Module truth-model unification (`docs/slices/rust-module-parity.md`) is complete.
Rust indexer populates `module_candidates` tables through declared module detection
(Cargo.toml, package.json, pyproject.toml, settings.gradle) and inferred module
heuristics (top-level directory detection with umbrella splitting).

MODULE-node fallback path deprecated as of 2026-05-10.

---

### Shipped: Linux IPC Family Expansion (BI-LX)

Linux/Unix IPC mechanisms were high-value discovery targets for legacy C
codebases, embedded systems, and kernel-adjacent code. All planned slices shipped.

**Slice queue:**

| Slice | Family | Functions | Status |
|-------|--------|-----------|--------|
| BI-LX-1 | SysV shared memory | shmget, shmat, shmdt, shmctl | **SHIPPED** |
| BI-LX-2 | SysV message queues | msgget, msgsnd, msgrcv, msgctl | **SHIPPED** |
| BI-LX-3 | SysV + named POSIX semaphores | semget, semop, semctl, sem_open, sem_close, sem_unlink | **SHIPPED** |
| BI-LX-4 | memfd_create | memfd_create | **SHIPPED** |

Note: BI-LX-3 covers SysV semaphores plus named POSIX semaphores. Unnamed POSIX
semaphore operations (sem_wait, sem_post, etc.) are deferred until pshared/identity
correlation is available — otherwise thread synchronization would be misclassified as IPC.

**Slice docs:**

- `docs/slices/bi-lx-1-sysv-shared-memory.md` — SHIPPED
- `docs/slices/bi-lx-2-sysv-message-queues.md` — SHIPPED
- `docs/slices/bi-lx-3-semaphores.md` — SHIPPED
- `docs/slices/bi-lx-4-memfd-create.md` — SHIPPED
- `docs/slices/bi-em-1-inter-core-mailbox.md` — SHIPPED

**After BI-LX:**

| Slice | Family | Description | Status |
|-------|--------|-------------|--------|
| BI-EM-1 | Inter-core messaging | Mailbox + RPMsg messaging APIs (no remoteproc lifecycle) | **SHIPPED** |
| BI-EM-2 | ~~DMA / descriptor-ring~~ | ~~dma_alloc_*, descriptor setup~~ | **WITHDRAWN** |

**BI-EM-2 withdrawal:** DMA API usage is hardware I/O plumbing, not software-to-software
boundary interaction. Concept deferred to future hardware-resource hints track.
See `docs/slices/bi-em-2-dma-descriptor-rings.md` for rationale.

BI-EM-1 smoke validated against Linux kernel drivers/rpmsg + drivers/mailbox.
See `smoke-runs/2026-05-05T14-30-08Z/` (protocol v3 compliant).

---

### Deferred (explicitly not next)

The following are valuable but explicitly lower priority than refresh integrity:

- **Snapshot-to-snapshot quality diff** — cross-cutting, not mechanism breadth
- **Quality delta surfacing** — cross-cutting, not mechanism breadth
- **PF-3 (RETURN_FATE)** — SHIPPED (policy-fact depth)
- **BI-1B fd-tracking completion** — SHIPPED (TCP/UDP role detection)
- **MB-3B (NATS request/reply)** — broker depth, not new mechanism family

Return to these after refresh integrity and module parity are complete.

---

### Previously listed execution sequence

Items 1-2 are done. Items 3-8 are deferred per precedence rule above.

1. **Trust overlay on read surfaces** — DONE
2. **Dead-confidence stratification** — DONE
3. ~~Snapshot-to-snapshot quality diff~~ — deferred (cross-cutting)
4. ~~Quality delta surfacing~~ — deferred (cross-cutting)
5. ~~Comparability/identity caveats~~ — deferred (cross-cutting)
6. ~~Document-backed authored relationship items~~ — deferred
7. **Long-lived daemon** — D1-D6 SHIPPED (see §3)
8. ~~Seam expansion~~ — deferred (architectural extraction)

---

### 1. Quality discovery surface (remaining)

**The goal is discovery, not enforcement.** See `docs/VISION.md` §Absolute
Priority. The agent needs to see what changed, what got worse, and where
the risks are. Policy enforcement exists as available substrate.

Quality-policy declarations, assessments, and baseline validation are
shipped (see Shipped section). This is available governance substrate,
not the primary next frontier.

**Trust/correctness surfacing** (execution items 1-2)
- Trust overlay on read surfaces — DONE
- Dead-confidence stratification — DONE

**Quality discovery** (execution items 3-5)
- Snapshot-to-snapshot quality diff: compare two snapshots, surface deltas.
- Quality delta surfacing in `check` and `orient`: what got worse, what
  got better, what is risky in the current scope.
- Comparability/identity caveats: when snapshots are non-comparable,
  surface that explicitly rather than forcing a verdict.

**Agent-facing surface integration**
- `rmap check` should include top complexity/risk deltas before an agent
  hands off a change.
- `rmap orient` should include current quality-risk hotspots for the
  focused module/symbol.
- `rmap risk` should combine complexity + churn + coverage gap + boundary
  violations into prioritized risk assessments.

**Available governance substrate** (shipped, not expanding)
- Quality-policy declarations (`rmap declare quality-policy`)
- Policy-backed assessments (`rmap assess`)
- Comparative policies (`no_new`, `no_worsened` kinds)
- Gate integration for CI blocking (works, not primary)

**Support module: metric expansion**
- Near-term cheap deterministic addition: LOC/SLOC measurements at
  file/function/module granularity for spotting oversized legacy units.
- Deferred until needed: NPath complexity for combinatorial path explosion detection.
- Deferred until needed: measurement version/source identifiers for comparison rejection.

**Why discovery is strategically #1:**
- Agents need to see what changed before deciding what to do.
- "3 functions got more complex" is actionable discovery.
- "gate fail" is not actionable without the underlying discovery.
- Risk prioritization helps agents focus on what matters.

### 2. Documentation inventory (shipped + remaining)
Documentation files are first-class orientation evidence. The docs
themselves are the data — not a narrow ontology of extracted facts.

**Workflow target:**
- repo-graph finds relevant docs and exposes when facts/doc context should be reviewed
- the agent using repo-graph writes or repairs documentation in the target repo
- repo-graph should reduce raw-file archaeology, not become the authoring system itself

**Shipped:**
- `rmap docs list` — documentation inventory surface (live discovery)
- `rmap docs extract` — semantic hints extraction (secondary layer)
- `rmap orient` — `documentation.relevant_files[]` section with ranked docs
- `DocInventoryEntry` DTO shared across inventory surfaces
- Generated flag: path-based advisory hint only (MAP.md → generated)
- Relevance tiering: exact match > descendant > ancestor > root doc
- Semantic facts stored but demoted to secondary hints

**Documentation model (binding):**
- **Docs inventory is PRIMARY.** Surface doc file paths; let agents read them.
- **Generated flag is ADVISORY.** Path-convention hint, not semantic truth.
  A generated MAP.md for a module may be more useful than a root README.
- **Semantic facts are SECONDARY.** Useful for ranking/filtering, never
  canonical. Repos with docs but no semantic_facts still show their docs.
- See `docs/design/documentation-semantic-facts.md` §Two Distinct Surfaces.

**Remaining:**
- `rmap explain` integration: doc excerpts and file references
- Persisted inventory (optimization, currently live filesystem discovery)
- Document-backed authored relationship items with anchors/references for
  hand-discovered seams, migrations, replacements, constraints, and other
  architectural knowledge that should be readable in git outside the DB

**Implementation constraints (still binding):**

1. **`rmap docs list` must NOT derive from semantic_facts.**
   It uses documentation inventory (live discovery or future persistence).

2. **In orient, each relevant doc includes a relevance reason:**
   ```json
   {
     "path": "src/core/README.md",
     "kind": "readme",
     "generated": false,
     "reason": "module_path_match"
   }
   ```

3. **Relevance tiering (structural, not fuzzy):**
   - Tier 1 (exact match): doc path matches focus exactly
   - Tier 2 (descendant): doc is under the focused path
   - Tier 3 (ancestor): doc is an ancestor of the focused path
   - Tier 5 (root): repo-root docs as fallback
   - Within tiers: authored > generated (mild tie-breaker only)

4. **Generated flag is path-based only in inventory layer.**
   Content-based frontmatter detection belongs in semantic-fact extraction,
   not inventory classification. Do NOT add content-authoritative detection
   to the inventory surface.

### 3. Long-lived analysis daemon + daemon-backed CLI — D1-D6 SHIPPED
Two-part item: support module (multi-agent coordination runtime) + feature (CLI client).

**Implementation status (2026-05-08):**

| Slice | Status | Description |
|-------|--------|-------------|
| D1 | DONE | Core policy module — RepoCoordinator with FIFO writer queue |
| D2 | DONE | Stdio adapter — NDJSON transport over stdin/stdout |
| D3 | DONE | Application service bridge — direct Rust invocation |
| D4 | DONE | Write operations — index/refresh with DB + repo coordination |
| D5a | DONE | Agent services — orient/check/explain through daemon |
| D5b | DONE | Progress streaming + transport-failure abort checkpoints |
| D5c | DEFERRED | Cancellation support — reuses abort seam, separate concern |
| D6 | DONE | Smoke validation on real repos (repo-graph self-index) |

**D6 validation results (2026-05-07, updated 2026-05-08):**
- Functional parity: index, refresh, enrich, orient, check, explain all match one-shot `rmap`
- Protocol: progress events before final response, request IDs correlate, no stray output
- Coordination: concurrent reads, write serialization, multi-DB isolation all verified
- Validation report: `docs/testing/daemon-validation-report.md`

**Daemon mode currently supports:**
- Multi-DB runtime (multiple databases loaded simultaneously)
- Repo load/unload/list
- Graph queries: callers, callees, imports
- Agent services: orient, check, explain
- Write operations: index, refresh, enrich
- Progress streaming (per-file granularity during extraction, phase tracking during enrich)
- Transport-failure abort checkpoints (stops before next mutation on channel loss)

**Implemented methods:** `ping`, `echo`, `load_repo`, `unload_repo`, `list_repos`,
`callers`, `callees`, `imports`, `index`, `refresh`, `enrich`, `orient`, `check`, `explain`

**Key design decisions implemented:**
- Transport: NDJSON over stdin/stdout (not Unix sockets)
- Identity: composite key (db_path + repo_uid) at API boundary
- Coordination: two levels — DB-scoped write lock + repo-scoped reader/writer
- Progress streaming: request-scoped emitter, separate NDJSON lines
- Abort checkpoints: `ControlFlow<()>` callback seam through compose and indexer

See `docs/design/rmap-daemon-architecture.md` for full design and implementation details.

**Residual limitations:**
- Abort is checkpoint-granular, not instruction-granular. Between two checkpoints,
  batch writes may complete partially. Mitigated: snapshot transitions to FAILED.
- Cancellation (D5c) not shipped. Client-initiated cancel requires token threading.

**Deferred:**
- D5c cancellation support — will reuse the existing abort checkpoint seam

Support: daemon-owned runtime for shared repo databases. It must solve:
- many concurrent AI-agent reads
- fewer writes/refreshes
- readers-writer coordination above SQLite
- daemon-owned DB handles/queues so clients do not stomp over each other

It also eliminates repeated CLI bootstrap cost (WASM grammar load,
extractor initialization, SQLite open, migration checks). Runtime
components: prepared SQLite statements, request routing (NDJSON over
stdin/stdout), per-repo write locks, snapshot pinning, progress streaming,
cancellation tokens.

Feature: CLI becomes a thin client (connect, send request, render
response). Auto-start daemon on first command if absent. Progress
rendering via stderr. Fallback to direct execution if daemon unavailable.

**Dependency constraint:**
Daemonization hardens the execution boundary. The discovery surfaces
must stabilize first:
- Trust overlay on read surfaces — DONE
- Dead-confidence stratification — DONE

The purpose is multi-agent correctness first, warm-runtime latency second.

### 4. Delta indexing support module (infrastructure)
**Architectural principle:** Git owns historical truth. Repo-graph owns
structured current-state truth. Delta indexing is a recomputation
strategy for current-state truth, not a substitute history system.

Full indexing (77 min on Linux) is a bootstrap path, not operational
flow. Delta indexing changes the amount of work, not just the startup
cost. But it is infrastructure optimization, not product capability.

**Support module: delta invalidation planner**
- determine what changed (git diff, working-tree diff)
- determine what must be invalidated (reverse edge traversal)
- determine what can be reused from the parent snapshot
- classify invalidation scope:
  - file-local only (body change, no interface change)
  - outbound dependency change (import/include changes)
  - public surface change (exported symbols, signatures)
  - config/manifest change (package.json, Cargo.toml, tsconfig, compile_commands)
  - structural/module change (file moved, module root changed)
- output: affected files, affected modules, invalidation plan, reuse plan

**Feature: parent-snapshot incremental indexing**
- parent snapshot is baseline
- copy/reuse unchanged truth logically
- delete stale rows only for affected scope
- re-extract only affected files
- recompute only affected derived artifacts
- carry forward unchanged nodes, edges, file signals, module candidates
- explicit trust metadata: what was reused vs recomputed vs widened

**Design constraints:**
- Explicitly ephemeral: optimize current-state recomputation only.
- No archival snapshot accumulation as product history.
- Retention policy biases toward latest snapshot plus minimal transient
  comparison state. A future `rmap clean` command prunes stale snapshots.
- Longitudinal analysis uses git-backed re-extraction on demand, not
  retained graph snapshots.

**Special considerations for C/C++:**
- header changes have wide blast radius
- compile_commands.json changes can invalidate per-TU resolution
- macro-heavy code may require conservative widening

### 5. Sharded indexing architecture
The streaming pipeline handles Linux-scale repos in a single pass
(77 min, exit 0). Sharding is the next scaling tier for:
- reducing peak memory further (bounded by shard, not snapshot)
- enabling parallel shard extraction
- natural fit for build-aware C/C++ partitioning

Architecture: shard-local extraction + global integration pass.
Same pattern as Clang tooling, LSIF pipelines, large code intelligence.

**Shard partition keys** (language-dependent):
- C/C++: translation-unit shards (compile_commands.json), subsystem
  directories, Kbuild/object groups
- JS/TS: package/workspace roots
- Java: Gradle subproject or Maven module
- Python: package roots
- Monorepos: workspace manifest entries

**Invariants:**
- Global stable keys regardless of shard
- Per-TU compile context attached to source file
- Cross-shard unresolved edges are expected, not failures
- All shards in same logical snapshot
- Module/subsystem aggregation happens last

### 6. C/C++ semantic maturation
The syntax-only C extractor + compile_commands.json reader + Linux
system detector are shipped. C++ is strategically in-scope but Rust-primary
maturity is behind C.

**Next concrete slice: Rust-primary C++ syntax extraction with C ABI boundary evidence**

Design doc: `docs/milestones/cpp-extractor-v1.md`

Scope:
- Separate `cpp-extractor` crate (tree-sitter-cpp)
- `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx` routing
- Namespaces, classes, methods, constructors, destructors
- IMPLEMENTS edges from inheritance
- `extern "C"` linkage detection as symbol/file metadata
- Parity with TS-side `cpp-extractor.ts`

This slice treats C++ not as a language badge but as a source of legacy-code
relationships — especially C/C++ interop boundaries, ABI seams, wrapper/glue
modules, and mixed-language ownership patterns.

**Layer 2 (future):** compile_commands.json integration for C++ (already
exists for C), header/source ownership heuristics.

**Layer 3 (future):** Clangd/libclang enrichment for receiver-type resolution
(same architectural pattern as TS TypeChecker / rust-analyzer / jdtls).

Remaining system/framework detectors:
- RTOS task/thread registration (FreeRTOS, Zephyr)
- IOCTL/shared-memory boundary extraction
- Driver registration patterns beyond platform_driver

### 7. CLI progress rendering
The indexer emits progress events via callback. The CLI layer should
render them to stderr. stdout remains reserved for final `--json`
output. Not implemented yet — indexing runs silently until completion.

Options:
- `--progress` flag rendering to stderr (human-readable)
- `--progress=jsonl` for machine-readable progress stream
- daemon progress streaming (once daemon exists)

### 8. Dead-code confidence stratification — DONE
**See execution sequence item #2 at top of Next section.**

Implemented. Every `rmap dead` result now carries:
- `trust.dead_confidence`: HIGH, MEDIUM, or LOW
- `trust.reasons[]`: stable vocabulary of degradation signals

Top-level repo trust summary retained alongside per-result confidence.
See `docs/cli/rmap-contracts.md` for full output contract.

### 9. CLI boundary expansion (remaining)
Shell and Makefile consumer extraction shipped. Remaining adapters:
- CI configs (`.github/workflows/*.yml`, `.gitlab-ci.yml`)
- Dockerfiles (`ENTRYPOINT`, `CMD`) — deferred further

Also:
- Provider adapters for other CLI frameworks (yargs, clap, argparse)
- Cross-file Commander composition (commands registered via function params)
- Binary-identity verification for prefix matching (package.json bin field)
- Barrel-cycle normalization: separate export-only/barrel cycles from
  logic cycles in cycle reporting

### 10. Trust overlay on structural queries — DONE
**See execution sequence item #1 at top of Next section.**

Inline trust summary in callers, callees, path, dead, modules show,
orient, explain. Option A: only present when degraded. Includes:
- `summary_scope: "repo_snapshot"` for repo-level context
- `graph_basis` (CALLS, IMPORTS, CALLS+IMPORTS)
- `reliability` axes (call_graph, dead_code, import_graph, change_impact)
- `degradation_flags` and `caveats` when non-HIGH

Per-result markers (dead confidence, edge confidence) designed but
reserved for dead-confidence stratification slice.

### 11. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated. Also
enables Rust HTTP boundary provider extraction for the boundary model.

### 12. Java semantic enrichment operationalization
jdtls is operational but fragile. The remaining issues are not polish:
- Cold-start/workspace reliability
- Protocol/client completeness
- Operational determinism on large repos

### 13. State-boundary expansion (post-slice-1)
Slice 1 shipped (see Shipped section above). Remaining work:

**Language coverage:**
- TypeScript/JavaScript — **SHIPPED** (SB-7A)
- Python — **SHIPPED** (SB-7C)
- Java — **SHIPPED** (SB-7B: narrow first-cut `DriverManager.getConnection` only)
- C — **SHIPPED** (C-SB-1: fopen, open, sqlite3_open)
- C++ — **SHIPPED** (CPP-SB-1: stream family + C-style APIs, D3 local type map)
- Rust-language — blocked on Rust extractor `ResolvedCallsite` emission

**Feature expansion:**
- Queue/event boundaries: EMITS, CONSUMES, QUEUE node kind (Kafka, SQS, SNS, RabbitMQ)
- Config/env seam: CONFIG_KEY graph emission, explicit config→resource wiring edges
- SQL-string parsing: table-level DB granularity, TABLE node kind
- ORM/repository pattern inference (Prisma, JPA, SQLAlchemy, TypeORM)
- GCP/Azure blob coverage
- Form B matching (type-enriched receiver resolution)
- Dedicated `rmap state` command for resource enumeration
- TS-runtime parity

See `docs/milestones/rmap-state-boundaries-v1.md` §Deferred for the full
SB-next-* inventory.

### 14. Policy-facts support module (design phase)
Cross-layer policy propagation extraction: status translation patterns,
retry/restart behavior, return-fate tracking, default-provenance extraction.

**Gap identified:** rgistr MAP generation now surfaces policy signals as
advisory LLM-generated hints (Policy Signals, Policy Seams). This reveals
the architectural need but does not provide deterministic extraction,
source-anchor provenance, or queryability.

**Support module scope:**
- AST-anchored extraction of status/error translation functions
- Control-flow analysis for retry loops with backoff/resume
- Return-fate tracking (result ignored, propagated, transformed)
- Default-provenance extraction from config parsing patterns
- Cross-layer edge materialization in the graph

**Extraction families (C-first proving ground):**
- `status_mapping`: function transforms one status/error code to another — **SHIPPED (PF-1)**
- `behavioral_marker`: retry loops, resume offsets — **SHIPPED (PF-2)**
- `branch_outcome`: switch/match arm produces specific result/action
- `return_fate`: function result is ignored, propagated, or transformed
- `default_provenance`: where default values originate and propagate

**Design constraints:**
- Pure support module first, then `rmap policy` CLI surface
- Deterministic extraction (same input → same output)
- Source-anchor provenance (line numbers, AST node references)
- Stored in graph for programmatic queries
- C/C++ as proving ground (swupdate codebase)

**Shipped slices:**
- **PF-1 (STATUS_MAPPING):** Status/error code translation functions.
  `rmap policy <db> <repo> --kind STATUS_MAPPING`. Validated on swupdate
  (map_channel_retcode, channel_map_curl_error, channel_map_http_code).
- **PF-2 (BEHAVIORAL_MARKER):** Retry loops and resume offsets.
  `rmap policy <db> <repo> --kind BEHAVIORAL_MARKER`. Detects RETRY_LOOP
  (loops with sleep/delay) and RESUME_OFFSET (curl CURLOPT_RESUME_FROM*).
  Validated on swupdate channel_get_file (2 retry loops + 1 resume offset).
- **PF-3 (RETURN_FATE):** What happens to function return values at call sites.
  `rmap policy <db> <repo> --kind RETURN_FATE`. Classifies IGNORED, CHECKED,
  PROPAGATED, TRANSFORMED, STORED. Validated on swupdate (296 facts across
  5 fate kinds). Same-file callee_key resolution; cross-file deferred to PF-3b.

**Deferred:** PF-4+ (BRANCH_OUTCOME, DEFAULT_PROVENANCE) — broader scope, needs
explicit prioritization. See design doc for scope decisions.

See slice docs:
- `docs/shipped/policy-facts/pf-1-status-mapping.md`
- `docs/shipped/policy-facts/pf-2-behavioral-marker.md`
- `docs/shipped/policy-facts/pf-3-return-fate.md`
Design doc: `docs/design/policy-facts-support-module.md`.

### 15. rgistr productization plan (phases 1-5 shipped)

`tools/rgistr` productization complete through Phase 5.

**Shipped:**
- Provider discovery support module (OpenAI cloud, OpenAI-compatible local, Ollama)
- Model capability support module (registry, budget calculation)
- Chunking support module (planning, identity, artifact serialization)
- Two-mode file routing: whole-file (≤200KB) or chunked (>200KB)
- No silent file skipping — all code files processed regardless of size
- `rgistr discover` CLI command with machine-readable output
- Discovery-assisted preflight in `generate` (fail-closed, no auto-selection)
- Repo-context support module for deterministic folder-role classification
- Folder role anti-hallucination: validation corpus vs product code distinction
- Mixed-model routing support (folderLLM option for different folder synthesis model)

**Remaining:**
- End-to-end validation on real repositories with oversized files
- CLI-level integration tests (documented as deferred in TECH-DEBT.md)

**Design doc:** `docs/design/rgistr-productization-plan.md`

**Product rule:** rgistr never auto-selects a backend. If no `--adapter` is
specified, it runs discovery, prints available providers, and exits requiring
explicit selection.

### 16. Multi-track boundary detection (design phase)

Two-track architecture for boundary detection over a unified model:

**Track A: Raw Transport**
- BI-1B: TCP/UDP sockets — SHIPPED (socket detection + fd role tracking)
- BI-1C: SharedArrayBuffer (JS/TS worker boundaries) — SHIPPED
- BI-1D: Process signals (POSIX signal send/handle) — SHIPPED (C API, direction, signal names)

**Track A-LX: Linux IPC (priority track)**
- BI-LX-1: SysV shared memory (shmget, shmat, shmdt, shmctl) — SHIPPED
- BI-LX-2: SysV message queues (msgget, msgsnd, msgrcv, msgctl) — SHIPPED
- BI-LX-3: SysV + named POSIX semaphores (semget, semop, sem_open, etc.) — **SHIPPED**
- BI-LX-4: memfd_create — **SHIPPED**

**Track A-EM: Embedded/Inter-core**
- BI-EM-1: Inter-core messaging (mailbox + RPMsg, no remoteproc) — **SHIPPED**
- ~~BI-EM-2: DMA / descriptor-ring~~ — WITHDRAWN (not boundary interaction)

**Track B: Schema-Backed RPC** — ORIENTATION-SUFFICIENT
- CS-1: Protobuf schema extraction (.proto parser) — COMPLETE
- CS-2: Generated code provenance mapping — COMPLETE
- GR-1A: gRPC server implementation hints (ImplBase inheritance) — FIXTURE-VALIDATED
- GR-1B: gRPC server registration proof (addService, bindService) — COMPLETE
- GR-2A: gRPC client stub hints (newBlockingStub, newFutureStub, newStub) — FIXTURE-VALIDATED
- GR-3A: gRPC contract-based linking — **SHIPPED**
- GR-1C: gRPC server endpoint evidence (bind address, port) — DEFERRED
- GR-2B: gRPC client endpoint evidence (channel host, port) — DEFERRED
- GR-3B: gRPC endpoint-aligned linking — DEFERRED
- GR-3C: gRPC method-level linking — DEFERRED
- ER-1: eRPC IDL extraction (future)

**Message Broker Track (after A+B):**
- MB-1: RabbitMQ / AMQP basic detection — SHIPPED (MB-1A)
- MB-2: Kafka topic detection — SHIPPED (MB-2A)
- MB-3: NATS / Redis pub-sub detection — PARTIAL (MB-3A shipped, MB-3B pending)
- MB-4: Cloud broker detection (future: SQS, SNS, Pub/Sub, EventBridge)

**Confirmed implementation order:**
1. CS-1 (Protobuf schema) — COMPLETE (full dual-pipeline, CLI wired, smoke tested)
2. CS-2A (Java generated code mapping) — COMPLETE (validated on Hadoop: 28 mappings, 0 false positives)
3. GR-1A (gRPC server impl hints) — FIXTURE-VALIDATED
   - Detects `*ImplBase` inheritance pattern in Java gRPC implementations
   - Orchestrator wiring complete (after CS-2A in index/refresh)
   - Explicit degradation in IndexResult.grpc_impl_hints
   - Contract visibility: boundary_contracts exposed through rmap boundaries list/show
   - CLI test coverage: 2 integration tests proving contract fields visible in JSON output
   - **Fixture-validated:** grpc-java-minimal (1 impl, correct contract association)
   - **Spot-checked:** open-source grpc-java/examples — 0 hints because generated
     `*Grpc.java` files absent from repo snapshot
   - **Substrate assumption:** Requires generated stub files to be present in indexed tree.
     Projects that generate stubs at build time (Gradle/Maven) without checking them in
     will show 0 hints until post-build indexing.
   - **Pending:** Validation on post-build grpc-java with generated stubs present
4. GR-1B (gRPC server registration proof) — COMPLETE
   - Option A (hint-strengthening) implemented
   - Detects `addService(new Impl())` inline instantiation in Java
   - Same-file-first disambiguation + refuse-on-ambiguity (no false positives)
   - Boosts confidence from 0.85 → 0.90
   - Appends `registration_sites` array to evidence_json
   - CLI adapter tests verify boosted confidence and registration evidence
   - Same substrate assumption as GR-1A (requires generated stubs)
5. GR-2A (gRPC client stub hints) — FIXTURE-VALIDATED
   - Detects `*Grpc.newBlockingStub`, `*Grpc.newFutureStub`, `*Grpc.newStub` in Java
   - Links to proto service via CS-2A mappings (join through contract_elements by service name)
   - Direction = consumer (symmetric to GR-1A's provider)
   - Hint-grade confidence (0.85), basis = stub_creation
   - Orchestrator wiring complete (after CS-2A, parallel to GR-1A)
   - Refuse-on-ambiguity when multiple proto services share same simple name
   - Surface UID includes grpc_class + line_start for distinct call sites
   - Unit tests (11) + storage integration tests (3) + CLI adapter tests (3) + fixture validation test (1)
   - Fixture: `grpc-java-minimal` extended with `HelloWorldClient.java` and stub classes
   - Full indexed fixture run validates: extractor → CS-2A → GR-2A → CLI
   - **Same substrate assumption as GR-1A** (requires generated stubs)
**── gRPC Track: ORIENTATION-SUFFICIENT ──**

The Java gRPC track has reached orientation sufficiency. Implemented slices:
- CS-1 (schema extraction)
- CS-2A (generated-code provenance)
- GR-1A (server hints)
- GR-1B (registration strengthening)
- GR-2A (client hints)
- GR-3A (contract-based linking)

This is sufficient as an orientation substrate. An agent can:
- Identify gRPC servers and clients
- See which proto services they reference
- See which providers and consumers share contracts
- Navigate to the relevant code

**Deferred depth slices** (endpoint/topology refinement — return only if real-repo navigation proves insufficient):
- GR-1C (server endpoint evidence)
- GR-2B (client endpoint evidence)
- GR-3B (endpoint-aligned linking)
- GR-3C (method-level linking)

**── Breadth-First: Next Mechanism Families ──**

6. BI-1B (TCP/UDP sockets) — SHIPPED
   - Multi-binding table architecture: `(language, function, channel_kind)` uniqueness
   - Binding candidates evaluated in TOML declaration order (Unix sockets first)
   - Socket type extraction: SOCK_STREAM, SOCK_DGRAM, SOCK_RAW, SOCK_SEQPACKET
   - Guard predicates: AF_INET/AF_INET6 + SOCK_STREAM → tcp_socket, SOCK_DGRAM → udp_socket
   - Disambiguation: bind/connect refuse when socket_type unavailable (no TCP-by-precedence)
   - Phase 2: FD role tracking — function-local registry, evidence accumulation,
     direction refinement (Provider/Consumer/Bidirectional)
   - `InteractionPattern::Datagram` added for UDP semantics
   - Files: `boundary-interaction/src/types.rs`, `boundary-interaction/bindings.toml`,
     `boundary-interaction/src/table.rs`, `boundary-interaction-extractor/src/emit.rs`,
     `c-extractor/src/boundary_detector.rs`, `repo-index/src/compose.rs`,
     `storage/src/boundary_interaction_read_impl.rs`
   - Tests: 7 socket_type extraction tests, 10 emitter guard tests (incl. disambiguation)
   - Phase 2: FD role tracking — function-local registry, evidence accumulation,
     direction refinement (Provider for bind+listen, Consumer for connect, Bidirectional for UDP)
   - Implementation: compose-phase lineage suppression, `update_surface_direction()` for refinement
   - CLI: `rmap boundaries list --kind tcp`, `rmap boundaries list --kind udp`
7. BI-1D (Process signals) — SHIPPED
   - C signal API detection: kill, killpg, raise, sigqueue, pthread_kill (senders)
   - C signal handler detection: signal, sigaction, sigwait, sigwaitinfo, sigtimedwait, signalfd
   - Direction: provider for senders, consumer for handlers
   - Signal name extraction as channel identity (SIGTERM, SIGUSR1, etc.)
   - Scope: inter_process (kill), intra_process (raise)
   - Files: `boundary-interaction/src/types.rs`, `boundary-interaction/bindings.toml`,
     `c-extractor/src/boundary_detector.rs`, `rgr/src/commands/boundaries.rs`
   - Tests: 8 signal extraction tests, 3 integration tests
   - CLI: `rmap boundaries list --kind signal`, `rmap boundaries list --family signal`
8. BI-1C (SharedArrayBuffer / Atomics boundaries) — SHIPPED
   - TS/JS binding table with 12 patterns (SharedArrayBuffer + 11 Atomics.* methods)
   - Scope: intra_process (same OS process, different V8 isolates)
   - Direction: provider (SAB allocation, Atomics.notify), consumer (Atomics.wait),
     bidirectional (Atomics.store/load/add/sub/and/or/xor/exchange/compareExchange)
   - **Option A applied:** Worker and postMessage NOT detected (no SAB correlation
     possible without dataflow). This prevents false SAB claims in worker-heavy repos.
   - CLI: `rmap boundaries list --kind shared_array_buffer` (aliases: sab, atomics)
   - Fixture: test/fixtures/shared-array-buffer/ (main.ts, worker.ts)
   - Tests: 9 unit tests (boundary_detector), 3 integration tests (bi_shared_array_buffer.rs),
     4 CLI adapter tests (boundaries_command.rs)
   - Deferred: BI-1E (Web Worker) slice for general worker patterns with `web_worker` channel kind
9. MB-1A (RabbitMQ/AMQP basic detection) — SHIPPED
   - TS/JS binding table with 6 amqplib patterns: sendToQueue, publish, consume,
     assertQueue, assertExchange, bindQueue
   - Channel kind: `amqp_queue` (protocol family: `message_broker`)
   - Scope: unknown (broker topology not inferrable without config)
   - Direction: provider (sendToQueue, publish), consumer (consume),
     bidirectional (assertQueue, assertExchange, bindQueue)
   - CLI: `rmap boundaries list --kind amqp_queue` (aliases: amqp, rabbitmq)
   - Fixture: test/fixtures/amqp-basic/ (producer.ts, consumer.ts, publisher.ts)
   - Validation: rabbitmq-tutorials/javascript-nodejs (31 surfaces across 12 files)
   - Tests: 10 unit tests (amqp_detector), 3 integration tests (mb_1a_amqp.rs)
   - Deferred: Spring AMQP (@RabbitListener, RabbitTemplate), Python pika, Go amqp
10. MB-2A (Kafka topic detection) — SHIPPED
    - TS/JS kafkajs patterns: send, subscribe
    - Channel kind: `kafka_topic` (protocol family: `message_broker`)
    - Interaction pattern: `publish_subscribe` (unlike AMQP fire_and_forget)
    - Scope: unknown (broker endpoint may be local or remote)
    - Direction: provider (send), consumer (subscribe)
    - Scope guards (triple):
      1. Import presence guard: file must have direct `kafkajs` import/require
      2. Receiver provenance guard: receiver must be assigned from `*.producer()`
         or `*.consumer()` factory call in same file
      3. Topic evidence guard: call must have extractable topic argument
    - Provenance tracking scope (deliberately narrow):
      - Same file only, direct assignment only, direct receiver identifier only
      - No interprocedural flow, no wrapper inference, no alias tracking
    - Intentionally NOT detected (deferred):
      - `producer.sendBatch()` — nested `topicMessages[].topic` extraction not implemented
      - `consumer.run()` — no topic evidence, deferred for subscribe() correlation
    - CLI: `rmap boundaries list --kind kafka_topic` (aliases: kafka)
    - Fixture: test/fixtures/kafka-basic/ (producer.ts, consumer.ts)
    - Tests: 25 unit tests (kafka_detector), 4 integration tests (mb_2a_kafka.rs),
      4 CLI adapter tests (boundaries_command.rs)
    - Deferred: Java kafka-clients, Spring Kafka, Python kafka, topic linking,
      run() correlation with subscribe(), cross-file receiver tracking,
      sendBatch nested topicMessages extraction
11. MB-3A (NATS subject detection) — SHIPPED
    - TS/JS nats npm package patterns: publish, subscribe
    - Channel kind: `nats_subject` (protocol family: `message_broker`)
    - Interaction pattern: `publish_subscribe`
    - Scope: unknown (broker endpoint may be local or remote)
    - Direction: provider (publish), consumer (subscribe)
    - Scope guards (triple):
      1. Import presence guard: file must have direct `nats` import/require
      2. Connection provenance guard: receiver must be assigned from `connect(...)`
         call in same file
      3. Subject evidence guard: call must have extractable subject argument
    - Provenance tracking scope (deliberately narrow):
      - Same file only, direct assignment only, direct receiver identifier only
      - No interprocedural flow, no wrapper inference, no alias tracking
    - Intentionally NOT detected (deferred to MB-3B):
      - `nc.request()` — mixed semantics (outbound request + reply handling),
        direction is muddy
      - Subject wildcards
      - Queue groups
      - JetStream patterns
    - CLI: `rmap boundaries list --kind nats_subject` (aliases: nats)
    - Tests: 19 unit tests (nats_detector)
    - Deferred: request() (MB-3B), Java nats-client, Python nats, Go nats
12. MB-3B (NATS request/reply) — future

**Shared infrastructure:**
- Contract/IDL substrate module (`repo-graph-contract-schema`)
- Storage schema extension (contract_schemas, contract_elements, boundary_contracts)
- Transport expansion module (channel kinds, scope heuristics)

**Detection maturity ladder:**
1. Mechanism presence (cheap, broad)
2. Boundary surfaces (provider/consumer roles, channel identity)
3. Contract association (message/service/layout crossing boundary)
4. Provider/consumer linking (cross-language edges)

Design doc: `docs/design/boundary-detection-multitrack.md`.
Slice docs: `docs/slices/bi-*.md`, `docs/slices/cs-*.md`, `docs/slices/gr-*.md`, `docs/slices/mb-*.md`.

## Deferred

### Dead-code public surface reintroduction (blocked)
**Status:** Blocked on coverage-backed evidence.

Public dead-code surfaces (`rmap dead`, `DEAD_CODE` signal, `DEAD_CODE_RELIABILITY`
check condition) are withdrawn and must NOT be reintroduced from structural graph
heuristics alone.

**Mandatory prerequisite:** Measured execution evidence in the Rust product path.
- Line coverage (lcov, cobertura), OR
- Function/call coverage (llvm-cov function-level)

**What does NOT unblock this:**
- Framework entrypoint detection (Spring, React, Axum, FastAPI)
- Entrypoint declarations (`rmap declare entrypoint`)
- Structural orphan analysis (no inbound edges)
- Trust reliability improvements

Framework detectors and entrypoint declarations are valuable for:
- Orientation (understanding what symbols are framework-managed)
- Liveness suppression (reducing false positives in internal substrate)
- Discovery (what is structurally isolated)

They are NOT sufficient proof of deadness. The distinction:
- **`rmap orphans`** (future): "not currently referenced in the graph we built"
- **`rmap dead`** (blocked): "unexecuted under measured scenarios"

See `docs/TECH-DEBT.md` §Dead-code surface withdrawal for the full policy.

### Halstead metrics
Computable and historically established, but lower priority than cognitive
complexity and NPath for the current agent-control loop. Halstead can be added
later if a concrete consumer needs volume/difficulty vocabulary. Until then,
avoid expanding the metric set just because the formula is available.

### Entrypoint declarations (operational)
`rgr declare entrypoint` on the 6 smoke repos would lift dead-code
reliability out of LOW. Not a code change — an adoption step. Best
done when a team starts using rgr operationally on a repo.

### Trust-score reweighting (policy)
Call-graph reliability thresholds (50%/85%) have not been changed
since the denominator was corrected. May need recalibration after
enrichment and framework detection have stabilized across languages.

### D2c expansion: this.x.method() with field-type binding
The `not_simple_receiver_method` skip bucket (301 edges) is the
largest remaining promotable frontier. Requires class field type
resolution in the promotion gate.

### tsconfig package-name extends
`extends: "@tsconfig/node18"` requires node_modules lookup.
Near-zero impact on current repo set.

### Full TS semantic extraction (Path C broadening)
Replace tree-sitter for TS with Program/TypeChecker for all symbol
extraction. Largest architectural investment. Only justified after
the current syntax-first + classifier + enrichment model has been
pushed to its natural ceiling.

### Python semantic enrichment
Equivalent of TS TypeChecker / Rust rust-analyzer / Java jdtls for Python.
pyright/mypy for type inference. Follows the same enrichment adapter pattern.

### Go extractor
Useful, but lower priority than Python and C/C++ given the current repo set.
Add only after the primary language stack is mature:
TypeScript/JavaScript + Rust + Python + Java + C/C++.

Why later:
- Current demand is exploratory rather than product-critical.
- It should not displace Python breadth or the C/C++ systems track.
- When added, it will reuse the shared scaffolding:
  multi-extractor indexer, manifest routing, trust/reporting, and the
  boundary-interaction model. Semantic enrichment would likely use `gopls`.

### Mobile and native client track

Mobile codebases introduce relationship classes distinct from server code:
- lifecycle entrypoints (AppDelegate, Activity, Fragment, Service)
- UI navigation/routes
- dependency injection containers
- persistence boundaries
- permissions / platform capability boundaries
- native/managed interop boundaries (bridging headers, JNI, platform channels)
- background job / service / worker boundaries
- FFI seams

These are exactly the kind of relationships repo-graph should surface.

**Priority order:**

#### 1. Objective-C / Objective-C++ (highest leverage)

**Objective-C++ is a bridge-layer priority.** It connects the server/systems track
(C/C++) with the mobile/client track (Apple). `.mm` files often host the most
important interop seams in Apple repos — where C/C++ libraries meet Apple app code.

Why first:
- shares ecosystem with C/C++
- directly intersects existing native systems direction
- `extern "C"` / ABI seam thinking extends naturally into Obj-C++
- Clang/LLVM ecosystem is strongest here
- `.mm` (Objective-C++) files are often the real bridge layer in Apple codebases
- extracting Objective-C++ gives you interop seam intelligence, not just another language badge

Target relationship classes:
- Objective-C / Swift lifecycle entrypoints
- AppDelegate / SceneDelegate / UIApplicationMain
- UIKit / SwiftUI navigation surfaces
- Objective-C protocols / categories
- bridging headers
- Swift <-> Objective-C interop
- Objective-C++ / C++ bridge layers
- C ABI edges in Apple-native repos

Tooling path: tree-sitter-objc + libclang for semantic enrichment.

#### 2. Kotlin (explicit, not vague)

Why second:
- Android is too common to leave vague
- JVM/Gradle context already exists from Java work
- Kotlin introduces mobile-specific boundary and lifecycle patterns

Target relationship classes:
- Activity / Fragment / Service / BroadcastReceiver entrypoints
- navigation graphs
- DI entrypoints (Hilt, Koin, Dagger)
- Room / persistence seams
- WorkManager / background execution
- JNI boundaries
- Kotlin <-> Java ownership and call surfaces

Tooling path: tree-sitter-kotlin + existing Gradle reader. Semantic enrichment
via kotlin-compiler or IntelliJ platform APIs.

#### 3. Swift (ecosystem-aware tooling path)

Why third:
- major iOS codebases
- strong interop story with Objective-C and C/C++
- tooling path needs deliberate choice (not just tree-sitter)

Tooling path: tree-sitter-swift for syntax, but SourceKit-LSP or libSwiftAST
for semantic enrichment. Apple's tooling is more closed than LLVM/Clang.

#### 4. Dart / Flutter (after native mobile substrate)

Why fourth:
- Flutter adds its own boundary model distinct from native
- widget tree, routes, platform channels, Dart FFI, plugin seams

Target relationship classes:
- widget entrypoints
- route / navigation surfaces
- state-management seams (Provider, Bloc, Riverpod)
- platform channels
- FFI boundaries
- plugin registration seams

Tooling path: tree-sitter-dart + Dart analyzer for semantic enrichment.

### LLVM/Clang ecosystem integration

The LLVM/Clang ecosystem provides tooling beyond tree-sitter that is directly
useful for repo-graph, especially on the native/mobile track.

**Immediate value (Layer 2 build-context):**

- **compile_commands.json** — already relevant for C/C++, also matters for
  Objective-C, Objective-C++, mixed native/mobile modules. Provides include
  paths, defines, translation-unit context.

- **libclang / Clang AST** — more valuable than tree-sitter once you need
  resolved declarations, selectors, protocols/categories, linkage/interop
  details, trustworthy symbol boundaries in native code. Objective-C and
  Objective-C++ benefit most.

**Enrichment (Layer 3):**

- **clangd** — semantic lookup, resolved call targets, type-aware navigation,
  receiver/type resolution. Same architectural role as TS TypeChecker,
  rust-analyzer, jdtls.

**Imported evidence (measurement/risk layer):**

- **llvm-cov** — highest immediate value. Line coverage, function coverage,
  coverage-backed dead/liveness claims, risk weighting, stale/untested area
  discovery. This is the prerequisite for reintroducing dead-code public
  surfaces (see TECH-DEBT.md §Dead-code surface withdrawal).

- **sanitizer findings** (ASan, UBSan, TSan) — useful as imported runtime
  evidence, risk weighting, hotspot enrichment, validation signals. NOT as
  primary structural discovery or substitute for graph extraction.
  - ASan: memory safety failures, use-after-free, buffer overflow zones
  - UBSan: undefined-behavior hotspots, integer/shift/null issues
  - TSan: concurrency hazard evidence

- **clang-tidy / Clang Static Analyzer** — imported findings for structural
  smells, ownership hazards, unsafe patterns, modernization pressure. Fits
  repo-graph as risk evidence, not graph truth.

**Priority for LLVM integration:**

1. `llvm-cov` import path (coverage-backed liveness)
2. `compile_commands.json` for C++ / Objective-C (already have reader)
3. ASan / UBSan findings import
4. TSan findings import
5. clang-tidy / static analyzer import
6. libclang / clangd semantic enrichment

### Scala extractor

Later than Java and later than the mobile track.

Why later:
- Reuses JVM build-context scaffolding but is not a "free" Java extension
- Current product pressure is higher on mobile and C/C++ legacy-code coverage
- Add only after Java operationalization, mobile track progress, and the
  daemon/shared-db path are in place
