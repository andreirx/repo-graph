# ORIENT-LIVEGRAPH-1: apply the coherence contract to `rmap orient`

Slice ID: ORIENT-LIVEGRAPH-1
Status: **DESIGN / SPEC-FIRST — NOT IMPLEMENTED — DECISION-COMPLETE.** This document SPECIFIES the first
per-command application of the ratified COHERENCE-LAYER-1 contract. It produces NO source code, NO table
deletion, NO schema/data migration, NO default flip. The implementation is a LATER slice and depends on
COHERENCE-ENVELOPE-1 (the support module) landing first. **No open DECISION_REQUIRED remains:** BOTH
escalated orient-specific boundary decisions are RATIFIED (operator sign-off 2026-06-09):
  - **D-ORIENT-6 = O2 RETAIN RENAMED** (escalated iteration 0): the daemon degraded-state trust overlay is
    kept as a distinct `trust_briefing` field on `CoherentOrientResult`, alongside — and disjoint from — the
    envelope root `trust: TrustPosture`. See §D-ORIENT-6.
  - **D-ORIENT-SYMBOL-CALLGRAPH = LG-first** (escalated iteration 1): orient's SYMBOL-focus
    `CALLERS_SUMMARY` / `CALLEES_SUMMARY` signals are served LiveGraph-first via the already-migrated
    callers/callees surfaces, with labelled SQLite fallback — matching the contract's existing posture for
    the same reads (coherence-layer-1.md:341-342). See §D-ORIENT-SYMBOL-CALLGRAPH.
The wire shape, CLI renderer behaviour, contract-fixture plan, source map (now COMPLETE — incl. the two
symbol-focus callgraph leaves), and validation plan below are finalized against BOTH decisions.

ITERATION-3 CORRECTION (review-2 fix, decide-and-record — no new boundary decision): the per-FOCUS signal
coverage was corrected against first-hand source. `HIGH_COMPLEXITY` is a **REPO-FOCUS-ONLY** signal (file/
path/symbol emit the static `COMPLEXITY_UNAVAILABLE` limit instead); `IMPORT_CYCLES` is emitted at repo +
path focus and as the symbol-focus `ModuleContext` variant, but **NOT at file focus**; `BOUNDARY_LINKS_SUMMARY`
is REPO-FOCUS-ONLY. Explicit file-focus (§1a-file) and path-focus (§1a-path) signal tables plus a single
focus-coverage matrix (§1c) were added, and §2 / §3 / §4 / §5 / §6 / D-ORIENT-1 were made consistent with that
matrix. [OBSERVED first-hand iteration 3: orient/file.rs:37-119; orient/path.rs:39-132; aggregators/
dead_code.rs:42-80 (all three aggregate* variants return `AggregatorOutput::empty()`); aggregators/
module_summary.rs:41-143 (aggregate_file:112 / aggregate_path:141 emit `MODULE_DATA_UNAVAILABLE`
unconditionally; aggregate:67-76 conditionally).]

ITERATION-5 CORRECTION (review-4 fix, decide-and-record — no new boundary decision): three factual-accuracy
corrections against first-hand source. (1) §1b: the `repo` envelope field carries the repo NAME (`repo.name`,
repo.rs:169; the `repo_name` arg in every other builder, file.rs:97 / path.rs:110 / symbol.rs:163 /
mod.rs:281/:319) — NOT the `repo_uid`; the prior `repo (repo_uid)` label was inexact. (2) §1b: the AMBIGUOUS
and NO-MATCH builders (mod.rs:262-341) set `confidence` to a STATIC `Confidence::High` and `documentation` to
`None` — they do NOT call `derive_repo_confidence` — so the envelope-field rows now record that exception.
(3) D-ORIENT-4 / §3b / validation E1 now specify the ZERO-LEAF root posture: because ambiguous/no-match emit
zero signal leaves, the MEET fold has NO inputs; rather than defaulting to the empty fold's lattice-TOP (which
would falsely read Exact/Fresh/Complete over un-analyzed structure), the root takes an explicitly labelled
RESOLUTION-ONLY posture (operational-identity provenance; static High preserved; never a structural Exact).
[OBSERVED first-hand iteration 5: repo.rs:169; file.rs:84/:97; path.rs:99/:110; symbol.rs:148/:163;
mod.rs:281/:285/:287/:319/:323/:325; confidence.rs:43.]

Goal: specify how `rmap orient` serves its answer from current-state **LiveGraph** structural facts combined
with durable **SQLite/persisted authority**, wrapped in `CoherenceEnvelope<T>`, with honest degradation and
no false completeness. orient is the highest-traffic entrypoint and the most overlap with the
already-migrated drilldown commands, so it is the de-risking first build (COHERENCE-LAYER-1 §slice
sequence).

Track: Stage D, SQLITE-RAW-DECOMMISSION path — first per-command coherence build.

Authoritative contract (RATIFIED, read FIRST): `docs/slices/coherence-layer-1.md`. This slice REUSES that
contract's `CoherenceEnvelope<T> { value, provenance, trust, freshness }` wrapper, its orient source map, its
MEET fold (D3), its authority-overlay rule (D5), and its safe-fallback ladder. It does NOT re-open
COHERENCE-ENVELOPE-SHAPE (RATIFIED = Option B wrapper) or TRUST-DISPOSITION (RATIFIED = hybrid; trust is a
separate later slice anyway). TWO orient-specific boundary decisions the contract left open were escalated
across iterations 0-1 and are now BOTH **RATIFIED** (operator sign-off 2026-06-09):
  1. **D-ORIENT-6 = O2 RETAIN RENAMED** — where orient's daemon-level degraded-state trust overlay lives once
     the wrapper introduces a root `trust: TrustPosture` sibling. Resolution: the overlay is retained as a
     distinct `trust_briefing` field on `CoherentOrientResult`, renamed from the old top-level `trust` key,
     disjoint from the envelope root `trust: TrustPosture`. See §D-ORIENT-6 for the full matrix.
  2. **D-ORIENT-SYMBOL-CALLGRAPH = LG-first** — the source posture for orient's SYMBOL-focus
     `CALLERS_SUMMARY` / `CALLEES_SUMMARY` signals (OMITTED from the contract's repo-focus orient row, and
     from this doc's iteration-1 draft). Resolution: served LiveGraph-first via the already-migrated
     callers/callees surfaces, with labelled SQLite fallback — the SAME posture the contract already assigns
     these reads in its explain row (coherence-layer-1.md:341-342). See §D-ORIENT-SYMBOL-CALLGRAPH for the
     full matrix. Consequence: orient has FOUR LG-first signal types, not two (§2 corrected accordingly).

Depends (precedent, reused — NOT re-derived here):
- COHERENCE-LAYER-1 — the ratified mixed-source contract (envelope shape, source map, MEET, fallback).
- COHERENCE-ENVELOPE-1 — the SUPPORT module that realizes `CoherenceEnvelope<T>` + `CoherentOrientResult`
  + the MEET fold + the FreshnessInfo→FreshnessState reconciliation. **MUST land before this slice's
  implementation** (architecture.md §Build Order: support module → feature).
- IMPORTS/CYCLES/STATS-LIVEGRAPH-DEFAULT-FASTPATH-1 — the cert-gated fastpath + the SQLite-free
  fingerprint + the labelled SQLite fallback this slice reuses for orient's cycles/complexity LG-first leaves
  (IMPORT_CYCLES, HIGH_COMPLEXITY).
- QUERY-MIGRATION-1 + LIVEGRAPH-INTEGRATION-1B — the migrated `callers`/`callees` LiveGraph surfaces this
  slice reuses for orient's symbol-focus LG-first leaves (CALLERS_SUMMARY, CALLEES_SUMMARY) per RATIFIED
  D-ORIENT-SYMBOL-CALLGRAPH; same cert-gated feed + FallbackReason mechanism.
- ORIENT-BUG-1 (`docs/slices/orient-bug-1-module-count.md`) — anchored the module count to SQLite
  `module_candidates`; this slice preserves that anchoring (D-ORIENT-2).

## Spec-first note (read first)
```text
This is a SPECIFICATION. Per the repo evidence law (CLAUDE.md §Evidence Law), every claim is labelled
OBSERVED or INFERRED. This doc was authored across two iterations; the OBSERVED provenance is split so a
reviewer can re-verify each claim against the turn that read it:
  OBSERVED [iteration 0, first-hand] = the full orient command SURFACE, read end-to-end at authoring:
      rust/crates/agent/src/orient/mod.rs (focus dispatch)
      rust/crates/agent/src/orient/repo.rs (repo pipeline)
      rust/crates/agent/src/aggregators/{snapshot,trust,cycles,boundary,boundary_links,dead_code,
        module_summary,gate,complexity}.rs (every signal source)
      rust/crates/agent/src/confidence.rs (confidence derivation)
      rust/crates/agent/src/dto/envelope.rs:300-339 (OrientResult); dto/signal.rs:947-959 (Signal DTO).
  OBSERVED [iteration 1, first-hand THIS turn] = re-verified the D-ORIENT-6 wire path + the pipeline spine
    while finalizing the ratified decision, with current file:line:
      rust/crates/agent/src/orient/repo.rs:58-191 (pipeline order, confidence :153, documentation :161/208,
        complexity gate :138/142, OrientResult build :166-190 — unchanged from iteration 0)
      rust/crates/agent/src/aggregators/dead_code.rs:38-52 (surface withdrawn; aggregate() returns
        AggregatorOutput::empty() unconditionally — confirms D-ORIENT-3)
      rust/crates/daemon-runtime/src/util/trust.rs:11-35 (compute_trust_overlay_for_snapshot ->
        Option<repo_graph_trust::TrustOverlaySummary>)
      rust/crates/daemon-runtime/src/dispatch.rs handle_orient:2600-2668 (display_name set on struct :2615;
        overlay computed :2638 with graph_basis "CALLS+IMPORTS" :2642; inserted as a post-serialize
        top-level JSON "trust" key iff has_degradation() || !caveats.is_empty() :2644-2648)
      rust/crates/rgr/src/presentation/orient.rs:83 (OrientResponse.trust: Option<TrustOverlay>),
        :144-172 (TrustOverlay/ReliabilitySection/ReliabilityAxis shape), :204-210 + :362 (render_degradation
        reads self.trust)
  OBSERVED [iteration 2, first-hand THIS turn] = the SYMBOL-focus pipeline + the callers/callees LiveGraph
    surfaces, read to finalize the ratified D-ORIENT-SYMBOL-CALLGRAPH decision, with current file:line:
      rust/crates/agent/src/orient/symbol.rs:64-185 (orient_symbol pipeline; CALLERS_SUMMARY via
        find_symbol_callers :89-93 emitted ONLY when callers non-empty; CALLEES_SUMMARY via
        find_symbol_callees :96-100; inherited module-context BOUNDARY_VIOLATIONS :108-113 / IMPORT_CYCLES
        :116-120 via find_cycles_involving_module :302 / gate :123-128 — only when the symbol has an owning
        module; COMPLEXITY_UNAVAILABLE limit :135; NO HIGH_COMPLEXITY signal and NO MODULE_SUMMARY at symbol
        scope :136-140; group_by_module summary :193/208-223 keyed on AgentCaller/CalleeRow.module_path)
      rust/crates/agent/src/dto/signal.rs:1339-1357 (Signal::callers_summary -> SignalCode::CallersSummary,
        SourceRef::StorageFindSymbolCallers); :1359-1377 (callees_summary symmetric, StorageFindSymbolCallees)
      rust/crates/agent/src/storage_port.rs:403-420 (AgentCallerRow / AgentCalleeRow — both carry
        module_path + module_stable_key; "enriched with module ownership"); :607/:615/:623 (find_symbol_callers
        / find_symbol_callees / find_cycles_involving_module storage-port reads — the labelled SQLite fallback)
      rust/crates/repo-graph-livegraph/src/lib.rs:443 (LiveGraph::callers -> AnswerEnvelope<CallersAnswer>),
        :560 (LiveGraph::callees -> AnswerEnvelope<CalleesAnswer>), :117-136 (CallersAnswer / CalleesAnswer —
        PARTITION-grouped: per_partition_counts + (partition,key) identities for RESIDENT partitions only),
        :200-205 (ratified residency asymmetry: summary-level `callees` over non-resident defining partitions
        DEFERRED — confirms the labelled SQLite fallback is the common callees path)
      rust/crates/daemon-runtime/src/livegraph_feed.rs:448 (callers_engine_response), :544
        (callees_engine_response), :131-182 (FallbackReason enum) — the cert-gated fastpath + labelled
        fallback this slice reuses for the two symbol-focus callgraph leaves, identical mechanism to
        imports/cycles/stats.
  OBSERVED [iteration 3, first-hand THIS turn] = the FILE-focus and PATH-focus pipelines + the withdrawn
    dead-code variants + the file/path module-summary limit behaviour, read to correct the per-focus signal
    coverage flagged by the iteration-2 review, with current file:line:
      rust/crates/agent/src/orient/file.rs:37-119 (file pipeline: snapshot :54, trust :58, dead_code
        aggregate_file :62, module_summary aggregate_file :72, STATIC COMPLEXITY_UNAVAILABLE :76, documentation
        :87; NO cycles/boundary/boundary_links/gate/complexity aggregators run — file.rs:13-14 doc + :50-76 body)
      rust/crates/agent/src/orient/path.rs:39-132 (path pipeline: snapshot :55, trust :59, cycles
        aggregate_path :63, boundary aggregate_path :67, dead_code aggregate_path :72, module_summary
        aggregate_path :82, gate aggregate_path :86, STATIC COMPLEXITY_UNAVAILABLE :91, documentation :102; NO
        boundary_links or complexity aggregator run)
      rust/crates/agent/src/orient/repo.rs:101-103 (boundary_links aggregator — invoked ONLY in the repo
        pipeline, so BOUNDARY_LINKS_SUMMARY is repo-focus-only) and :133-143 (complexity is measurement-gated
        and invoked ONLY in the repo pipeline, so HIGH_COMPLEXITY is repo-focus-only)
      rust/crates/agent/src/aggregators/dead_code.rs:42-52/58-66/72-80 (aggregate / aggregate_file /
        aggregate_path ALL return AggregatorOutput::empty() unconditionally — the withdrawn no-op holds for
        every focus)
      rust/crates/agent/src/aggregators/module_summary.rs:41-85 (repo aggregate: MODULE_DATA_UNAVAILABLE
        CONDITIONAL on empty module_candidates) vs :92-114 (aggregate_file) and :122-143 (aggregate_path):
        BOTH emit MODULE_DATA_UNAVAILABLE UNCONDITIONALLY (module discovery is repo-scoped, never file/path
        scoped — module_summary.rs:112/141)
  OBSERVED [iteration 5, first-hand THIS turn] = the `repo` envelope-field population, the confidence
    derivation across all four resolving pipelines, and the AMBIGUOUS / NO-MATCH zero-signal builders, read to
    correct the review-4 factual gaps (repo-NAME field; ambiguous/no-match static confidence + None
    documentation; the zero-leaf root posture):
      rust/crates/agent/src/orient/repo.rs:169 (`repo: repo.name` — the field carries the repo NAME, not the
        repo_uid)
      rust/crates/agent/src/orient/{file.rs:97, path.rs:110, symbol.rs:163} (`repo: repo_name.to_string()` —
        every other builder stores the `repo_name` arg, never the uid)
      rust/crates/agent/src/orient/{file.rs:84, path.rs:99, symbol.rs:148} (confidence =
        `derive_repo_confidence` — all four resolving pipelines use it, matching repo.rs:153)
      rust/crates/agent/src/orient/mod.rs:262-303 (build_ambiguous_result) + :310-341 (build_no_match_result):
        confidence STATIC `Confidence::High` (:285/:323), documentation `None` (:287/:325), signals/limits
        empty (:289/:293, :327/:331), all truncation flags `None`, next empty, truncated `false` (:301/:339);
        repo = `repo_name` (:281/:319); snapshot = `snapshot.snapshot_uid` (:283/:321)
      rust/crates/agent/src/confidence.rs:43-70 (derive_repo_confidence — High/Medium/Low from resolution rate
        + stale + enrichment; NOT called by the zero-signal builders)
  INFERRED = my design judgment over those OBSERVED facts (the envelope wiring, the per-leaf provenance
    mapping, the fallback rules, the validation plan) — grounded in the ratified contract + D-ORIENT-6 +
    D-ORIENT-SYMBOL-CALLGRAPH.
NO live `rmap` orientation of the graph was run: the daemon socket is absent. [EXECUTED iteration 0:
`rmap orient` -> "error: daemon connection failed: socket does not exist:
~/Library/Application Support/repo-graph/daemon.sock".] A spec-only slice does not start the daemon or run
the index/refresh sequence (that mutates state). Orientation was grounded in first-hand source reads — the
stronger evidence basis for a contract about code structure. The socket-absent result is itself recorded
below as orient's transport-level degradation path (§Degradation).
INCIDENTAL OBSERVATION (not in scope, recorded for honesty): repo.rs:105-116 carries a STALE comment
describing dead_code as "reliability-gated" (emitting a DEAD_CODE_UNRELIABLE limit), but the aggregate()
body it calls is now an unconditional empty no-op (dead_code.rs:48-52). Code/comment drift in the target,
NOT a spec change here; it does not alter D-ORIENT-3's conclusion (orient emits no dead-code output).
```

## Why now (priority path)
```text
[OBSERVED: docs/slices/coherence-layer-1.md §slice sequence + CURRENT_SLICE.md STATUS banner.]
COHERENCE-LAYER-1 is RATIFIED (operator sign-off 2026-06-08): both load-bearing boundary decisions
(COHERENCE-ENVELOPE-SHAPE = wrapper; TRUST-DISPOSITION = hybrid) are settled, so the dependent per-command
builds are unblocked. The contract's slice sequence names ORIENT-LIVEGRAPH-1 the FIRST feature build:
"highest-traffic entrypoint, most overlap with migrated commands" — it de-risks the wrapper/provenance-tag
pattern before the heavier explain and the conceptually-novel hybrid trust.

[OBSERVED, first-hand: dispatch.rs handle_orient:2550-2668; the grep for `livegraph` in dispatch.rs shows
wiring ONLY for callers/callees/imports/stats/cycles/path/preload/refresh — all at lines <=1701 — and NO
livegraph reference between 1701 and the orient handler at 2550.] => orient today is 100% SQLite with NO
served LiveGraph path. It is one of the LAST four SQLite-eager defaults and a precondition for
SQLITE-RAW-DECOMMISSION-1: the raw `nodes`/`edges` substrate cannot be decommissioned while orient reads it
eagerly on every call for cycle topology and complexity.
```

---

## 1. What `rmap orient` returns today (OBSERVED, first-hand)

orient is a COMPOSITE multi-signal aggregator, NOT a single query. The repo-focus pipeline
[OBSERVED: orient/repo.rs:58-191] runs nine aggregators in a fixed order, collects their `Signal`s and
`Limit`s, sorts+ranks, truncates to budget, derives `confidence`, attaches the documentation section, and
builds one `OrientResult` [OBSERVED: envelope.rs:300-339]. The daemon handler sets `display_name` ON the
struct before serialization [OBSERVED: dispatch.rs:2615] and then, ONLY when degraded, injects a separate
top-level `trust` overlay key (a serialized `repo_graph_trust::TrustOverlaySummary`) into the JSON object
AFTER serialization [OBSERVED: dispatch.rs:2638-2648; util/trust.rs:11-35]. That post-serialize `trust` key
is the artifact D-ORIENT-6 (RATIFIED = O2) renames to `trust_briefing` and lifts onto the struct; see §1b
and §D-ORIENT-6.

Every aggregator takes a `storage: &S where S: AgentStorageRead` handle; the daemon passes the SQLite
`StorageConnection` [OBSERVED: dispatch.rs:2603 `repo_graph_agent::orient(&repo_state.storage, ...)`]. The
in-memory `LiveGraph` on `RepoState` is NOT consulted by orient. Hence: **every signal below is SQLite or
filesystem today; LiveGraph contribution = NONE.**

### 1a. Signals + limits emitted (repo focus), in aggregator order

| Emitted (signal code / limit code) | Condition | Aggregator (OBSERVED file:line) | Storage read (OBSERVED) | Source today |
|---|---|---|---|---|
| `SNAPSHOT_INFO` signal | always | snapshot.rs:22 | `get_latest_snapshot` (repo.rs:72) | SQLite `snapshots` |
| `TRUST_LOW_RESOLUTION` signal | resolution rate < 0.20 | trust.rs:48 | `get_trust_summary` (trust.rs:39) | SQLite trust-core |
| `TRUST_STALE_SNAPSHOT` signal | `get_stale_files` non-empty | trust.rs:57 | `get_stale_files` (trust.rs:40) | SQLite `files`/`file_versions` |
| `TRUST_NO_ENRICHMENT` signal | enrichment_state == NotRun | trust.rs:68 | `get_trust_summary` (trust.rs:39) | SQLite trust-core |
| `IMPORT_CYCLES` signal | >= 1 module cycle | cycles.rs:43 | `find_module_cycles` (cycles.rs:21) | SQLite `nodes`/`edges` |
| `BOUNDARY_VIOLATIONS` signal | >= 1 violating edge | boundary.rs:108 | `get_active_boundary_declarations` (boundary.rs:50) + `find_imports_between_paths` (boundary.rs:70) | SQLite `declarations` (Authority) + `edges` |
| `BOUNDARY_LINKS_SUMMARY` signal | >= 1 boundary link; carries `Signal.freshness` | boundary_links.rs:51 | `get_boundary_links_freshness` (boundary_links.rs:25) | SQLite `boundary_interaction_links` |
| (dead-code) | **WITHDRAWN — emits nothing** | dead_code.rs:42-52 returns `AggregatorOutput::empty()` | (none called) | n/a |
| `MODULE_SUMMARY` signal | always | module_summary.rs:82 | `compute_repo_summary` (module_summary.rs:46) + `get_module_summary` (module_summary.rs:49) | SQLite `nodes`/`files` + `module_candidates` |
| `MODULE_DATA_UNAVAILABLE` limit | `module_candidates` empty | module_summary.rs:31/76 | (same) | SQLite degradation marker |
| `GATE_PASS`/`GATE_FAIL`/`GATE_INCOMPLETE` signal | per gate outcome (>=1 obligation) | gate.rs:185/191/196 | `get_active_requirements` (gate.rs:47) + `assemble_from_requirements` (gate.rs:58) | SQLite `declarations` (Authority) |
| `GATE_NOT_CONFIGURED` limit | no active requirements | gate.rs:54 | `get_active_requirements` (gate.rs:47) | SQLite Authority |
| `HIGH_COMPLEXITY` signal | >= 1 symbol over cyclomatic threshold (20) | complexity.rs:63 | `count_high_complexity_symbols` + `query_high_complexity_symbols` (complexity.rs:37/45) | SQLite `measurements` (cyclomatic) |
| `COMPLEXITY_UNAVAILABLE` limit | no complexity measurements | repo.rs:142 | `has_complexity_measurements` (repo.rs:138) | SQLite degradation marker |

### 1a-file. File-focus signals + limits (OBSERVED, first-hand) — the NARROWEST focus

The FILE-focus pipeline [OBSERVED orient/file.rs:37-119] emits the narrowest signal set. It runs ONLY snapshot,
trust, the withdrawn dead-code no-op, and the file-scoped module summary, then appends two STATIC limits and
the documentation section. It does NOT run the cycles, boundary, boundary-links, gate, or complexity
aggregators — so `IMPORT_CYCLES`, `BOUNDARY_VIOLATIONS`, `BOUNDARY_LINKS_SUMMARY`, `GATE_*`, and
`HIGH_COMPLEXITY` are ALL absent at file scope [OBSERVED file.rs:13-14 module doc + file.rs:50-76 body].

| Emitted (signal code / limit code) | Condition | Aggregator (OBSERVED file:line) | Storage read (OBSERVED) | Source today |
|---|---|---|---|---|
| `SNAPSHOT_INFO` signal | always | snapshot.rs:22 (file.rs:54) | `get_latest_snapshot` (caller) | SQLite `snapshots` |
| `TRUST_*` signals | repo-wide, unchanged | trust.rs (file.rs:58) | `get_trust_summary` / `get_stale_files` | SQLite trust-core |
| (dead-code) | **WITHDRAWN — emits nothing** | `dead_code::aggregate_file` returns `AggregatorOutput::empty()` (dead_code.rs:58-66); invoked file.rs:62 | (none) | n/a |
| `MODULE_SUMMARY` signal | always (file-scoped counts) | `module_summary::aggregate_file` (file.rs:72; module_summary.rs:92-111) | `compute_file_summary` (module_summary.rs:97) | SQLite `nodes`/`files` |
| `MODULE_DATA_UNAVAILABLE` limit | **always (STATIC)** — module discovery is repo-scoped, never file-scoped | `module_summary::aggregate_file` (module_summary.rs:112) | (none) | static degradation marker |
| `COMPLEXITY_UNAVAILABLE` limit | **always (STATIC)** | file.rs:76 | (none) | static degradation marker |
| `documentation` section | relevant docs exist | `build_documentation_section` (file.rs:87) | `get_doc_inventory` | FS live-scan |
| (`IMPORT_CYCLES`, `BOUNDARY_VIOLATIONS`, `BOUNDARY_LINKS_SUMMARY`, `GATE_*`, `HIGH_COMPLEXITY`) | **NOT emitted at file scope** | file.rs:13-14 / :50-76 (those aggregators not run) | (none) | n/a — intentionally omitted |

File-focus note vs repo focus: both `COMPLEXITY_UNAVAILABLE` and `MODULE_DATA_UNAVAILABLE` are UNCONDITIONAL
static limits here, whereas at repo focus `COMPLEXITY_UNAVAILABLE` is conditional on `has_complexity_measurements`
and `MODULE_DATA_UNAVAILABLE` is conditional on an empty `module_candidates` [OBSERVED repo.rs:138-143;
module_summary.rs:41-85]. `HIGH_COMPLEXITY` can therefore NEVER appear at file focus.

### 1a-path. Path-area (subtree) focus signals + limits (OBSERVED, first-hand)

The PATH-focus pipeline [OBSERVED orient/path.rs:39-132] runs snapshot, trust, PATH-scoped cycles, PATH-scoped
boundary, the withdrawn dead-code no-op, the PATH-scoped module summary, and PATH-scoped gate, then appends two
STATIC limits and the documentation section. Relative to repo focus it OMITS the repo-only
`BOUNDARY_LINKS_SUMMARY` and the `HIGH_COMPLEXITY` signal — the boundary-links and complexity aggregators are
NOT run [OBSERVED path.rs:54-91].

| Emitted (signal code / limit code) | Condition | Aggregator (OBSERVED file:line) | Storage read (OBSERVED) | Source today |
|---|---|---|---|---|
| `SNAPSHOT_INFO` signal | always | snapshot.rs:22 (path.rs:55) | `get_latest_snapshot` (caller) | SQLite `snapshots` |
| `TRUST_*` signals | repo-wide, unchanged | trust.rs (path.rs:59) | `get_trust_summary` / `get_stale_files` | SQLite trust-core |
| `IMPORT_CYCLES` signal | >= 1 cycle involving a module under the prefix | `cycles::aggregate_path` (path.rs:63) | path-scoped cycle read (inside `aggregate_path`) | SQLite `nodes`/`edges` |
| `BOUNDARY_VIOLATIONS` signal | >= 1 violating edge under the prefix | `boundary::aggregate_path` (path.rs:67) | `get_active_boundary_declarations` + `find_imports_between_paths` (path-scoped) | SQLite `declarations` (Authority) + `edges` |
| (dead-code) | **WITHDRAWN — emits nothing** | `dead_code::aggregate_path` returns `AggregatorOutput::empty()` (dead_code.rs:72-80); invoked path.rs:72 | (none) | n/a |
| `MODULE_SUMMARY` signal | always (path-scoped counts) | `module_summary::aggregate_path` (path.rs:82; module_summary.rs:122-140) | `compute_path_summary` (module_summary.rs:127) | SQLite `nodes`/`files` |
| `MODULE_DATA_UNAVAILABLE` limit | **always (STATIC)** — module discovery is repo-scoped, never path-scoped | `module_summary::aggregate_path` (module_summary.rs:141) | (none) | static degradation marker |
| `GATE_*` signal / `GATE_NOT_CONFIGURED` limit (plus a focus-applicability limit when obligations exist but none match the prefix — INFERRED, by parity with the symbol-focus exact-match gate; not re-read in `gate::aggregate_path` this turn) | per gate outcome; obligations filtered by target prefix [OBSERVED path.rs:86 + path.rs:11 doc] | `gate::aggregate_path` (path.rs:86) | `get_active_requirements` + `assemble_from_requirements` | SQLite `declarations` (Authority) |
| `COMPLEXITY_UNAVAILABLE` limit | **always (STATIC)** | path.rs:91 | (none) | static degradation marker |
| `documentation` section | relevant docs exist | `build_documentation_section` (path.rs:102) | `get_doc_inventory` | FS live-scan |
| (`BOUNDARY_LINKS_SUMMARY`, `HIGH_COMPLEXITY`) | **NOT emitted at path scope** | path.rs:54-91 (boundary-links + complexity aggregators not run) | (none) | n/a — intentionally omitted |

Path-focus note: as with file focus, both static limits are unconditional; `HIGH_COMPLEXITY` never appears.
`IMPORT_CYCLES`, `BOUNDARY_VIOLATIONS`, and `GATE_*` ARE present (path-scoped), unlike file focus.

### 1a-sym. Symbol-focus signals + limits (OBSERVED, first-hand) — the focus that adds callers/callees

The SYMBOL-focus pipeline [OBSERVED: orient/symbol.rs:64-185] is NOT the repo pipeline with a filter: it emits
a DIFFERENT signal set. It adds TWO structural-callgraph signals (`CALLERS_SUMMARY`, `CALLEES_SUMMARY`) that
NO other focus emits, and it OMITS `MODULE_SUMMARY` and the `HIGH_COMPLEXITY` signal. This table is the gap
the iteration-1 review flagged (the prior draft modeled only repo focus); it is the OBSERVED basis for the
ratified D-ORIENT-SYMBOL-CALLGRAPH leaves in §2.

| Emitted (signal code / limit code) | Condition | Aggregator (OBSERVED file:line) | Storage read (OBSERVED) | Source today |
|---|---|---|---|---|
| `SNAPSHOT_INFO` signal | always | snapshot.rs:22 (symbol.rs:81) | `get_latest_snapshot` | SQLite `snapshots` |
| `TRUST_*` signals | repo-wide, unchanged | trust.rs (symbol.rs:85) | `get_trust_summary` / `get_stale_files` | SQLite trust-core |
| `CALLERS_SUMMARY` signal | `find_symbol_callers` non-empty | symbol.rs:89-93; `Signal::callers_summary` (signal.rs:1339, `SourceRef::StorageFindSymbolCallers`) | `find_symbol_callers` (symbol.rs:89) | SQLite `edges`/`nodes` (caller rows enriched with `module_path`, storage_port.rs:403-409) |
| `CALLEES_SUMMARY` signal | `find_symbol_callees` non-empty | symbol.rs:96-100; `Signal::callees_summary` (signal.rs:1359, `SourceRef::StorageFindSymbolCallees`) | `find_symbol_callees` (symbol.rs:96) | SQLite `edges`/`nodes` (callee rows enriched with `module_path`, storage_port.rs:414-420) |
| `BOUNDARY_VIOLATIONS` signal (`ModuleContext`) | owning module exists + violating edge | symbol.rs:108-113 (`aggregate_boundary_for_module`, exact match) | `get_active_boundary_declarations` + `find_imports_between_paths` | SQLite `declarations` (Authority) + `edges` |
| `IMPORT_CYCLES` signal (`ModuleContext`) | owning module exists + cycle involves it | symbol.rs:116-120 (`aggregate_cycles_for_module`) | `find_cycles_involving_module` (symbol.rs:302) | SQLite `nodes`/`edges` (module-scoped read, NOT repo-wide `find_module_cycles`) |
| `GATE_*` signal (`ModuleContext`) | owning module exists + matching obligations | symbol.rs:123-128 (`aggregate_gate_for_module`, exact target) | `get_active_requirements` + `assemble_from_requirements` | SQLite `declarations` (Authority) |
| `GATE_NOT_CONFIGURED` / `GATE_NOT_APPLICABLE_TO_FOCUS` limit | per gate config | symbol.rs:350/381 | `get_active_requirements` | SQLite Authority |
| `COMPLEXITY_UNAVAILABLE` limit | always (static) | symbol.rs:135 | (none) | static degradation marker |
| (`MODULE_SUMMARY`, `MODULE_DATA_UNAVAILABLE`, `HIGH_COMPLEXITY`, dead-code) | **NOT emitted at symbol scope** | symbol.rs:12-14/27-28/102-103/136-140 | (none) | n/a — intentionally omitted |

Net symbol-focus delta vs repo focus: **+`CALLERS_SUMMARY`, +`CALLEES_SUMMARY`** (the two new LG-first leaves
of D-ORIENT-SYMBOL-CALLGRAPH); the inherited `BOUNDARY_VIOLATIONS`/`IMPORT_CYCLES`/`GATE_*` are the same
signal codes under `SignalScope::ModuleContext`, scoped by EXACT owning-module match; `MODULE_SUMMARY` and
`HIGH_COMPLEXITY` are absent. [OBSERVED first-hand symbol.rs.]

### 1b. Envelope-level fields (OBSERVED)

| Field | Source today (OBSERVED) | Source class |
|---|---|---|
| `schema` / `command` | compile-time constants (envelope.rs:343/346) | static |
| `repo` (carries the repo NAME, not the uid) | the FIELD stores the repo NAME — `repo.name` in the repo pipeline (repo.rs:169), the `repo_name: &str` arg in every other builder (file.rs:97 / path.rs:110 / symbol.rs:163 / mod.rs:281 ambiguous / mod.rs:319 no-match). The record is LOOKED UP by `get_repo(repo_uid)` (repo.rs:65), but the serialized value is `name`, NEVER the uid | SQLite `repos` |
| `display_name` | `resolve_and_load_repo_with_display_name` (dispatch.rs:2559), injected at :2615 | daemon operational metadata |
| `snapshot` | `get_latest_snapshot` (repo.rs:72) | SQLite `snapshots` |
| `focus` | focus dispatch (mod.rs:64-67); repo focus = `Focus::repo()` (repo.rs:172) | derived |
| `confidence` | the FOUR resolving pipelines: `derive_repo_confidence(trust_summary, stale)` (repo.rs:153 / file.rs:84 / path.rs:99 / symbol.rs:148; confidence.rs:43). **AMBIGUOUS + NO-MATCH: a STATIC `Confidence::High`** — `derive_repo_confidence` is NOT called (mod.rs:285 ambiguous, mod.rs:323 no-match); the value is hard-coded High because the RESOLUTION OUTCOME (ambiguous / unmatched) is itself certain, not because any structure was analyzed (D-ORIENT-4 zero-signal branch) | SQLite trust-core (resolving pipelines); static (ambiguous/no-match) |
| `documentation` | resolving pipelines: `build_documentation_section` -> `get_doc_inventory` (repo.rs:161/208). **AMBIGUOUS + NO-MATCH: `None`** (mod.rs:287, :325) — no documentation section is built for an unresolved focus | **filesystem live-scan** (doc-facts); `None` for ambiguous/no-match |
| `signals[]` / `limits[]` (+ the signal/limit truncation flags `signals_truncated` / `signals_omitted_count` / `limits_truncated` / `limits_omitted_count`) | aggregator merge + ranking/truncation (repo.rs:82-150); each flag = `then_some` of the ranking-truncation result (repo.rs:178-183, file.rs:106-110, path.rs:119-123, symbol.rs:172-176); `None` in the ambiguous/no-match builders (mod.rs:290-295 / 328-333) | derived from above |
| `next[]` (`Vec<NextAction>`) | `Vec::new()` in EVERY builder — orient emits NO next-actions today (repo.rs:185, file.rs:113, path.rs:126, symbol.rs:179, mod.rs:297 ambiguous, mod.rs:335 no-match) | static empty |
| `next_truncated` / `next_omitted_count` | `None` in EVERY builder (repo.rs:186-187, file.rs:114-115, path.rs:127-128, symbol.rs:180-181, mod.rs:298-299 / 336-337); vacuously so — `next[]` is never populated, so it can never truncate | static `None` |
| `truncated` (top-level bool) | real pipelines: `truncated_any = sig_tx.truncated \|\| lim_tx.truncated` (repo.rs:164 -> :189, file.rs:90 -> :117, path.rs:105 -> :130, symbol.rs:156 -> :183); ambiguous + no-match: `false` unconditionally (mod.rs:301, :339) | derived from signal/limit ranking truncation |
| daemon `trust` overlay key — `TrustOverlaySummary` (NOT an OrientResult field today; post-serialize JSON injection) | `compute_trust_overlay_for_snapshot` (dispatch.rs:2638; util/trust.rs:11-35), inserted iff `has_degradation() \|\| !caveats.is_empty()` (:2644-2648) | SQLite trust-core; RATIFIED **D-ORIENT-6 = O2**: renamed to `trust_briefing`, lifted onto the struct |

**AMBIGUOUS / NO-MATCH envelope shape (OBSERVED, first-hand — the zero-signal builders).** The ambiguous
(`build_ambiguous_result`, mod.rs:262-303) and no-match (`build_no_match_result`, mod.rs:310-341) builders do
NOT run the aggregator pipeline. They emit a VALID result with: `confidence: Confidence::High` (STATIC, NOT
`derive_repo_confidence`), `documentation: None`, `signals: Vec::new()`, `limits: Vec::new()`, all truncation
flags `None`, `next: Vec::new()`, and `truncated: false` [OBSERVED mod.rs:285-301 ambiguous / :323-339
no-match]. The only populated fields are the operational identity (`repo` NAME, `snapshot` uid) and `focus`
(`Focus::ambiguous` carries the candidate list; `Focus::no_match` carries the unmatched focus string). The
static `High` is confidence in the RESOLUTION OUTCOME (the focus is definitively ambiguous / unmatched), NOT a
structural-completeness claim — a distinction that is load-bearing for the wrapper's root posture and is
specified in D-ORIENT-4 (zero-signal branch), §3b (zero-leaf root), and validation E1z.

### 1c. Focus variants + focus-coverage matrix (OBSERVED: mod.rs:64-231; per-pipeline reads §1a / §1a-file / §1a-path / §1a-sym)

orient dispatches on `focus` [OBSERVED mod.rs:64-231]: `None` -> repo pipeline; an exact FILE -> file pipeline;
a path-area or MODULE -> path pipeline; a SYMBOL (stable key or unique name) -> symbol pipeline; multiple names
-> ambiguous (candidates, zero signals); no match -> valid `no_match` result (zero signals, zero limits).

The four pipelines emit DIFFERENT signal sets — they are NOT one pipeline with a filter. Precisely, OBSERVED
first-hand (this corrects an earlier draft that lumped "file/path reuse the same aggregators"; file and path
differ sharply):
  - FILE focus [file.rs:37-119] is the NARROWEST: snapshot + trust + the withdrawn dead-code no-op + the
    file-scoped `module_summary::aggregate_file` + two STATIC limits (`MODULE_DATA_UNAVAILABLE`,
    `COMPLEXITY_UNAVAILABLE`) + documentation. It does NOT run the cycles, boundary, boundary-links, gate, or
    complexity aggregators — so `IMPORT_CYCLES` / `BOUNDARY_VIOLATIONS` / `BOUNDARY_LINKS_SUMMARY` / `GATE_*` /
    `HIGH_COMPLEXITY` are ALL absent (§1a-file).
  - PATH focus [path.rs:39-132] runs the PATH-scoped `aggregate_path` variants of cycles, boundary, dead-code
    (withdrawn), module_summary, and gate [OBSERVED cycles aggregate_path path.rs:63, boundary aggregate_path
    path.rs:67, module_summary aggregate_path path.rs:82, gate aggregate_path path.rs:86]. Relative to repo
    focus it OMITS the repo-only `BOUNDARY_LINKS_SUMMARY` and `HIGH_COMPLEXITY` (§1a-path).
  - SYMBOL focus [orient/symbol.rs:64-185] is the EXCEPTION: a distinct pipeline (§1a-sym) that ADDS
    `CALLERS_SUMMARY` / `CALLEES_SUMMARY`, scopes the inherited boundary/cycle/gate signals to the symbol's
    owning module by EXACT match, and OMITS `MODULE_SUMMARY` + `HIGH_COMPLEXITY`.
  - REPO focus [repo.rs:58-191] is the ONLY focus that runs the boundary-links aggregator (repo.rs:102) and the
    measurement-gated complexity aggregator (repo.rs:138-139), so `BOUNDARY_LINKS_SUMMARY` and `HIGH_COMPLEXITY`
    are REPO-FOCUS-ONLY signals.

FOCUS-COVERAGE MATRIX (OBSERVED, first-hand across repo.rs / file.rs / path.rs / symbol.rs — the per-signal x
focus truth the §2 source map and the D-ORIENT-1 LG-first leaf set are keyed against; `cond.` = conditional
limit, `static` = unconditional static limit, `ModuleContext` = inherited owning-module variant):

| Signal / limit | repo | file | path | symbol |
|---|---|---|---|---|
| `SNAPSHOT_INFO` | yes | yes | yes | yes |
| `TRUST_*` | yes | yes | yes | yes |
| `IMPORT_CYCLES` | yes (repo-wide) | no | yes (path-scoped) | yes (ModuleContext) |
| `BOUNDARY_VIOLATIONS` | yes | no | yes | yes (ModuleContext) |
| `BOUNDARY_LINKS_SUMMARY` | yes | no | no | no |
| `MODULE_SUMMARY` | yes | yes | yes | no |
| `MODULE_DATA_UNAVAILABLE` | cond. | static | static | no |
| `GATE_*` | yes | no | yes | yes (ModuleContext) |
| `HIGH_COMPLEXITY` | yes (ONLY here) | no | no | no |
| `COMPLEXITY_UNAVAILABLE` | cond. | static | static | static |
| `CALLERS_SUMMARY` / `CALLEES_SUMMARY` | no | no | no | yes |
| dead-code (surface withdrawn) | no | no | no | no |
| `documentation` | yes | yes | yes | yes |

The source map in §2 is keyed by SIGNAL CODE, so each signal's posture (LG-first / SQLite-first / Authority /
FS) holds wherever it is emitted; this matrix records WHICH focuses emit each code so the LG-first leaves are
scoped correctly: `IMPORT_CYCLES` (repo / path, plus symbol `ModuleContext`), `HIGH_COMPLEXITY` (repo ONLY),
`CALLERS_SUMMARY` / `CALLEES_SUMMARY` (symbol ONLY). The repo focus is the canonical anchor; the symbol focus
contributes the two extra LG-first callgraph leaves; focus-scoped reads degrade identically per signal.

---

## 2. Per-signal source map (the field-level boundary)

Legend (per COHERENCE-LAYER-1 §source map): **LG-first** = LiveGraph-first via the cert-gated fastpath,
SQLite labelled fallback. **SQLite-first** = SQLite is source of truth. **Authority** = Tier-A1
`declarations`, permanent SQLite, overlays-never-erases. **FS** = filesystem live-scan. Layer = Fact
Certainty Model layer (architecture.md §Product Layer Stack).

This table REFINES the COHERENCE-LAYER-1 orient table with first-hand signal-code granularity. It is
CONSISTENT with the contract: no posture here contradicts the contract's orient row.

| Signal / field | Layer | Target posture | LiveGraph surface (when LG-first) | Notes |
|---|---|---|---|---|
| `SNAPSHOT_INFO` / repo+snapshot identity | A2 (operational) | **SQLite-first** | — | Operational identity; not rebuildable structure. |
| `TRUST_LOW_RESOLUTION` / `TRUST_STALE_SNAPSHOT` / `TRUST_NO_ENRICHMENT` | 1 | **SQLite-first** (trust-core) | — | Outgoing-extractor reliability; the hybrid trust rebase is TRUST-LIVEGRAPH-1, not here. |
| `confidence` (envelope) | — | **derived; root MEET (resolving pipelines) / static resolution-only (ambiguous+no-match)** | — | Resolving pipelines (repo/file/path/symbol): ONE contributor to the root MEET (D-ORIENT-4); never exceeds legacy `derive_repo_confidence`. Ambiguous/no-match: STATIC `Confidence::High` via the zero-signal resolution-only posture (D-ORIENT-4 zero-signal branch), NOT a MEET. |
| `IMPORT_CYCLES` | 1 | **LG-first** | `module_import_cycles` (livegraph lib.rs:1264) | Emitted at **repo + path focus**, and as the symbol-focus `ModuleContext` variant (exact owning-module match via `find_cycles_involving_module`); **NOT at file focus** (§1c). Cert-gated; RED/stale/missing/precondition-unmet -> SQLite `find_module_cycles` (repo) / path-scoped / module-scoped read, labelled. |
| `HIGH_COMPLEXITY` | 1 | **LG-first** (cyclomatic only) | `value_facts` CyclomaticComplexity (livegraph lib.rs:662, kinds :154-190) | orient surfaces cyclomatic ONLY; coverage/churn/risk are different commands, not in orient. SQLite fallback = `measurements`. **REPO focus ONLY** — file/path/symbol emit the static `COMPLEXITY_UNAVAILABLE` limit instead and never `HIGH_COMPLEXITY` (OBSERVED file.rs:76 / path.rs:91 / symbol.rs:135; only repo.rs:138-139 runs the complexity aggregator). |
| `CALLERS_SUMMARY` (symbol focus) | 1 | **LG-first** | `callers` -> `AnswerEnvelope<CallersAnswer>` (livegraph lib.rs:443; daemon feed `callers_engine_response`, livegraph_feed.rs:448) | RATIFIED **D-ORIENT-SYMBOL-CALLGRAPH = LG-first**. Same posture as the contract's explain CALLERS row (coherence-layer-1.md:341). Summary = count + top-3 owning modules. SQLite fallback = `find_symbol_callers` (storage_port.rs:607). DATA-SHAPE NOTE: the LG answer is PARTITION-grouped, orient's summary is MODULE-grouped → module mapping needed; non-resident referencing partitions degrade the grouping → labelled SQLite fallback (RISK-O-H). |
| `CALLEES_SUMMARY` (symbol focus) | 1 | **LG-first** | `callees` -> `AnswerEnvelope<CalleesAnswer>` (livegraph lib.rs:560; daemon feed `callees_engine_response`, livegraph_feed.rs:544) | RATIFIED **D-ORIENT-SYMBOL-CALLGRAPH = LG-first**. Same posture as the contract's explain CALLEES row (coherence-layer-1.md:342). SQLite fallback = `find_symbol_callees` (storage_port.rs:615). RESIDENCY NOTE: summary-level callees over a NON-resident defining partition is a ratified LiveGraph deferral (lib.rs:200-205; CURRENT_SLICE.md:129-131) → labelled SQLite fallback is the COMMON callees path until that deferral lifts (RISK-O-H). Never Exact-empty. |
| `BOUNDARY_VIOLATIONS` | 4 + 1 | **Authority + SQLite-first** | — | Declaration drives the signal (Authority); the structural import-edge half is LG-derivable in principle but kept SQLite-first per contract; overlay-preserves-computed (D-ORIENT-5). At symbol scope this is the inherited `ModuleContext` variant (exact module match), same posture. |
| `BOUNDARY_LINKS_SUMMARY` (+ `Signal.freshness`) | 2-3 | **SQLite-first** | — | **REPO focus ONLY** (the boundary-links aggregator runs only in the repo pipeline, repo.rs:102; not called by file/path/symbol). No LiveGraph producer for `boundary_interaction_links`. The existing `FreshnessInfo` reconciles to the leaf envelope freshness via COHERENCE-ENVELOPE-1 (D-ORIENT-7 / contract RISK-G). |
| `MODULE_SUMMARY` (+ `MODULE_DATA_UNAVAILABLE`) | 1 | **SQLite-first** | — | Count anchored to `module_candidates` by ORIENT-BUG-1 (D-ORIENT-2); RISK-E identity divergence forbids a naive LG count. file/path structural counts are LG-derivable but kept SQLite for count-anchor consistency (deferred optimization, out of scope). |
| `GATE_*` (+ `GATE_NOT_CONFIGURED`) | 4 | **Authority — SQLite-first** | — | Requirement/obligation/waiver evaluation; declarations have no LiveGraph home by construction (contract Q2a). |
| `documentation` section | 1 | **FS current-state** | — | Already current-state (live filesystem scan); stays. Not LiveGraph, not SQLite. |
| `trust_briefing` (was daemon `trust` overlay key) | 1 | **SQLite-first** (trust-core) | — | RATIFIED **D-ORIENT-6 = O2**: retained as a distinct `trust_briefing` field on `CoherentOrientResult`, disjoint from the envelope root `trust: TrustPosture`. Source unchanged (trust-core); only the wire KEY + PLACEMENT change. |

**Net for orient: FOUR LG-first signal types — `IMPORT_CYCLES` (repo + path focus, plus the symbol-focus
`ModuleContext` variant), `HIGH_COMPLEXITY` (repo focus ONLY), and `CALLERS_SUMMARY` + `CALLEES_SUMMARY`
(symbol focus only; §1c matrix).** Everything else is SQLite-first, Authority, or FS. (The iteration-1 draft said "exactly TWO" — that omitted the symbol-focus
callgraph leaves and is CORRECTED here per ratified D-ORIENT-SYMBOL-CALLGRAPH.) This is still a small,
de-risking surface: all four are DIRECT reuses of already-migrated LiveGraph surfaces (`module_import_cycles`,
`value_facts`, `callers`, `callees`) via the SAME cert-gated fastpath + labelled-fallback mechanism — NO new
producer, NO new LiveGraph query is introduced by orient. [INFERRED from the OBSERVED source map + contract
Q4 + coherence-layer-1.md:341-342.]

---

## 3. CoherenceEnvelope<T> wiring for orient (INFERRED, grounded in the RATIFIED contract)

Per COHERENCE-LAYER-1 §"The shared coherence answer-envelope" (RATIFIED), the wrapper is applied
COMPOSITIONALLY at two granularities. orient is the first command to instantiate it.

### 3a. Leaf — `CoherenceEnvelope<Signal>` (one per emitted signal)

```text
Each `Signal` orient emits is wrapped as a LEAF `CoherenceEnvelope<Signal>` [Signal DTO is the REAL shared
type, OBSERVED signal.rs:947-959; it is NOT widened — its evidence payload stays pristine]. The leaf's
provenance/trust/freshness ride in the wrapper SIBLING fields and describe THAT signal's source:

  provenance.source per orient signal:
    - livegraph  -> IMPORT_CYCLES (repo/path focus + symbol ModuleContext), HIGH_COMPLEXITY (repo focus
                    ONLY), and CALLERS_SUMMARY, CALLEES_SUMMARY (symbol focus) ... when the cert is GREEN at
                    the current fingerprint.
    - sqlite     -> SNAPSHOT_INFO, TRUST_*, BOUNDARY_LINKS_SUMMARY, MODULE_SUMMARY,
                    BOUNDARY_VIOLATIONS' structural-edge half, and any LG-first leaf that FELL BACK
                    (provenance.fallback_reason set — for CALLEES_SUMMARY this is the COMMON path while the
                    summary-callees residency deferral stands, RISK-O-H).
    - declaration-> BOUNDARY_VIOLATIONS (the forbidden-import rule), GATE_* (requirement/obligation/waiver).
                    These are Tier-A1 Authority (contract Q2a); they OVERLAY, never erase (D-ORIENT-5).
    - filesystem -> the documentation section.

  trust (TrustPosture) projects the AnswerEnvelope axes verbatim (class/completeness/degradation_reasons/
    contributing_languages). For the LG-first leaves this is the EXISTING AnswerEnvelope the LiveGraph
    already returns — for module_import_cycles / value_facts (cycles/complexity) and for callers / callees
    (the migrated `AnswerEnvelope<CallersAnswer>` / `<CalleesAnswer>`, livegraph lib.rs:443/560) (contract
    Q1). For SQLite/Authority/FS leaves it is a Fresh/Complete/Exact posture for the snapshot (no LiveGraph
    epoch involved).

  freshness (FreshnessState) = Fresh | Stale | PrecisionPending | RefreshFailed | Unavailable. The LG-first
    leaves inherit the LiveGraph partition freshness; SQLite leaves are snapshot-scoped (Fresh for the
    current index, Stale when `get_stale_files` is non-empty — the SAME stale signal TRUST_STALE_SNAPSHOT
    already reports, OBSERVED trust.rs:40-41).

Leaf construction MUST delegate to (or mirror) the AnswerEnvelope smart constructors so the six invariants
hold AT THE LEAF (contract §invariant preservation I1-I6).
```

### 3b. Root — `CoherenceEnvelope<CoherentOrientResult>` (per command)

```text
The root `value` is the NEW container DTO `CoherentOrientResult` (contract D7) = `OrientResult`
[OBSERVED envelope.rs:300-339] with the contract's ONE re-typed slot — `signals: Vec<Signal>` ->
`signals: Vec<CoherenceEnvelope<Signal>>` (the leaves) — PLUS exactly ONE ratified additive field for orient:
`trust_briefing` (D-ORIENT-6 = O2). Every other OrientResult field is copied verbatim (schema, command, repo,
display_name, snapshot, focus, confidence, documentation, limits[] + truncation flags, next[]). So the
per-signal value payload stays pristine while the COMMAND CONTAINER shape changes at the signals slot — both
true (contract resolves the iteration-1 contradiction).

  root.value      = CoherentOrientResult {
                      ... ,                                  // all OrientResult fields verbatim
                      signals: Vec<CoherenceEnvelope<Signal>>,   // the contract's re-typed slot (D7)
                      trust_briefing: Option<TrustOverlaySummary> // D-ORIENT-6 = O2; Some only when degraded.
                                                                  // Exact field type + owning crate ride the
                                                                  // COHERENCE-ENVELOPE-1 home-crate decision
                                                                  // (contract-deferred) — see §D-ORIENT-6
                                                                  // CRATE-HOME NOTE. Shown daemon-side here.
                    }
  root.provenance = { source: SET of contributing sources (livegraph + sqlite + declaration + filesystem),
                      basis/missing_partitions/fallback_reason aggregated from the leaves }
  root.trust      = the MEET fold of the leaf TrustPostures (contract D3 — greatest-lower-bound, monotone).
                    [NON-EMPTY leaf set — the resolving pipelines. Zero-leaf case below.]
  root.freshness  = the MEET fold of the leaf freshness states. [NON-EMPTY leaf set; zero-leaf case below.]

  CoherentOrientResult.confidence [OBSERVED envelope.rs:313; Confidence{High,Medium,Low} :216] is DERIVED
  from the root MEET and NEVER exceeds the weakest contributor (D-ORIENT-4). The legacy
  derive_repo_confidence(trust, stale) result becomes ONE input to the MEET, not the sole confidence source.

  ZERO-LEAF ROOT (AMBIGUOUS + NO-MATCH — the empty-signal builders, mod.rs:262-341). When orient resolves to
  ambiguous or no-match there are ZERO signal leaves, so the MEET fold above has NO inputs. The root is NOT
  served by the empty fold's lattice-TOP (the GLB of the empty set is TOP = Exact/Fresh/Complete, which would
  falsely read as exhaustive structural analysis over un-analyzed structure — D-ORIENT-4). INSTEAD the root
  carries an explicit RESOLUTION-ONLY posture:
    root.provenance.source = { sqlite } operational identity ONLY (repo + snapshot; NO structural source —
                             the anti-false-completeness GUARD: it cannot be read as "structure analyzed").
    root.confidence        = the STATIC Confidence::High preserved verbatim (mod.rs:285/:323) — resolution
                             certainty, NOT structural completeness; legacy derive_repo_confidence is not
                             called for these builders.
    root.trust             = a resolution-outcome TrustPosture (no degradation; completeness = Complete *as a
                             resolution outcome*, labelled "resolution: ambiguous|no_match; no structural
                             analysis"), NEVER a structural Exact.
    root.freshness         = Fresh, scoped to the operational snapshot-identity epoch ONLY (mod.rs:283/:321);
                             no structural-partition epoch is involved (no aggregator ran).
  The candidate list (ambiguous) / unmatched focus string (no-match) rides in `value.focus` exactly as today.
  `value.trust_briefing` is daemon-injected (D-ORIENT-6 = O2), NOT set by these agent-side builders; its
  presence follows the FOCUS-INDEPENDENT snapshot-degradation gate (has_degradation() || !caveats.is_empty(),
  OBSERVED dispatch.rs:2637-2652) — a degraded snapshot attaches the briefing even for ambiguous/no-match, so
  it is orthogonal to (and never substitutes for) the zero-leaf root posture. Full rationale + the rejected
  empty-fold-to-TOP alternative: D-ORIENT-4 zero-signal branch; validation E1z.

  TWO DISJOINT TRUST ARTIFACTS — NOT REDUNDANT, NOT RECONCILED (D-ORIENT-6 = O2, RATIFIED):
    root.trust : TrustPosture          = the AXIS-typed certainty posture (class/completeness/
                                         degradation_reasons/contributing_languages), the MEET of the leaves.
                                         ALWAYS present. Machine-readable.
    value.trust_briefing : Option<...> = the existing daemon-level HUMAN BRIEFING prose (reliability axes +
                                         caveats[]); SQLite trust-core; Some ONLY when degraded
                                         (has_degradation() || !caveats.is_empty()), else absent
                                         (#[serde(skip_serializing_if = "Option::is_none")]).
    They are DISJOINT by construction (axes vs prose), honour the contract's "no parallel structures that
    must agree" posture (Q7-4), and carry NO `trust` name collision. The daemon-side field holds
    `repo_graph_trust::TrustOverlaySummary` [OBSERVED util/trust.rs:16]; the CLI mirror deserializes it into
    `Option<TrustOverlay>` [OBSERVED presentation/orient.rs:83/144-172] — the operator's `Option<TrustOverlay>`
    is the CLI-side name. The rename touches ONLY the wire KEY (`trust` -> `trust_briefing`) and the
    PLACEMENT (post-serialize JSON sibling -> declared struct field); the overlay's INTERNAL shape is
    unchanged.

  SHARED-CONTAINER NON-CROSSING (recorded realization, NOT a new boundary decision): the contract's
    CoherentOrientResult is SHARED by orient/check/explain (D7). `trust_briefing` is added as `Option<...>` +
    `skip_serializing_if = "Option::is_none"` and is populated ONLY by orient; check/explain produce no
    overlay, so their serialized wire shape is UNCHANGED (field None -> absent). This is the same pattern the
    container already uses for `display_name`/`documentation` (OBSERVED envelope.rs:309/317). The contract's
    D7 field enumeration omitted the overlay because it was never an OrientResult field (daemon-injected
    post-serialize) — that omission is precisely the gap D-ORIENT-6 closes; O2 RESOLVES the delegated gap, it
    does not contradict D7.

ENVELOPE limits[]: gains the contract's provenance-derived codes (LIVEGRAPH_PARTIAL, SQLITE_SNAPSHOT_STALE,
  AUTHORITY_OVERLAY_APPLIED, PRECISION_PENDING, PRODUCER_UNAVAILABLE) so degradation is machine-discoverable
  at the envelope level, not only inside per-leaf trust. orient's existing degradation limits
  (MODULE_DATA_UNAVAILABLE, COMPLEXITY_UNAVAILABLE, GATE_NOT_CONFIGURED) are RETAINED unchanged — they are
  orthogonal known-zero/unavailable markers, not provenance codes.

The MEET fold is MONOTONE: it can only LOWER class/freshness/completeness, never raise. No fold can
manufacture an Exact root from non-Exact leaves — the formal anti-false-completeness guarantee (contract
§invariant preservation). For orient specifically: a PrecisionPending IMPORT_CYCLES leaf (SCIP refresh
pending) caps root confidence at Medium even if every SQLite leaf is Fresh.
```

### 3c. Reconciliation points implied by adopting the wrapper (RECORDED, not re-decided)

```text
These are realization details the ratified wrapper IMPLIES; they belong to COHERENCE-ENVELOPE-1 / this
slice's implementation, NOT new boundary decisions (CLAUDE.md §Decision Autonomy: "choices a ratified
decision already imply -> decide and record"). They are recorded here so the implementation does not
rediscover them:

  R1 (D-ORIENT-7 / contract RISK-G). The `Signal.freshness: Option<FreshnessInfo>` field (Current/Impacted/
     Unknown from artifact_contracts) that BOUNDARY_LINKS_SUMMARY already sets [OBSERVED boundary_links.rs:
     46-51; signal.rs:958] is a DIFFERENT vocabulary from the leaf envelope's trust-model FreshnessState.
     COHERENCE-ENVELOPE-1 defines the single FreshnessInfo->FreshnessState mapping; the OUTER leaf freshness
     is authoritative; FreshnessInfo is retired or kept render-only. orient must not surface two freshness
     truths for one signal.

  R2. `confidence` semantics change from f(trust) to MEET (D-ORIENT-4). Behavioural consequence: a repo
     that is High today can become Medium under the wrapper if a LiveGraph leaf is PrecisionPending/Partial.
     This is the intended honest degradation (VISION §Agent Priorities). Validation pins monotonicity
     (§5): coherent confidence <= legacy confidence on identical input.

  R3. The documentation section and the existing degradation limits keep their place verbatim inside
     CoherentOrientResult (they are copied fields, contract D7). No re-typing.

  R4 (D-ORIENT-6 = O2, RATIFIED). The daemon POPULATES `trust_briefing` on the `CoherentOrientResult`
     struct BEFORE serialization — the same way it already sets `display_name` [OBSERVED dispatch.rs:2615] —
     instead of the current post-serialize JSON-map insert under key `trust` [OBSERVED dispatch.rs:2645-2648].
     The computation (`compute_trust_overlay_for_snapshot`, graph_basis "CALLS+IMPORTS") and the
     degraded-only gate (`has_degradation() || !caveats.is_empty()`) are PRESERVED verbatim; only the sink
     changes from `map.insert("trust", ...)` to `result.trust_briefing = Some(...)`. This removes the
     post-serialization mutation of the JSON object — a clean-architecture improvement that makes the overlay
     follow the same struct-field discipline as every other orient field. No new computation, no new source.
```

---

## 4. Degradation / safe-fallback behaviour for orient (honest labelling, no false completeness)

```text
PER-LEAF, INDEPENDENT (contract §safe-fallback, ORIENT/EXPLAIN row):
  - LG-first leaf (IMPORT_CYCLES at repo/path focus + the symbol ModuleContext variant; HIGH_COMPLEXITY at
    repo focus ONLY; CALLERS_SUMMARY, CALLEES_SUMMARY at symbol focus): the cert ladder applies PER LEAF.
      precondition met + GREEN cert at current fingerprint -> serve LiveGraph, SQLite SKIPPED for that leaf
        (provenance.source=livegraph, fallback_reason=null).
      precondition unmet (non-TS / non-resident / stale partition) OR cert RED/stale/missing/build-failed
        -> that leaf's provenance.source FLIPS to sqlite with provenance.fallback_reason set (the proven
        imports/cycles/stats/callers/callees ladder). The SQLite answer is the PROVEN PRIMARY; LiveGraph is
        the accelerant.
      a contributing partition non-resident/non-TS/PrecisionPending -> the leaf is Partial/Stale/
        PrecisionPending with an explicit DegradationReason; it is NEVER dropped, NEVER marked Exact
        (forbids contract F1-F4).

SYMBOL-FOCUS CALLGRAPH SUMMARIES — DEGRADATION SPECIFICS (D-ORIENT-SYMBOL-CALLGRAPH; RISK-O-H). The
  callers/callees summaries are MODULE-grouped (count + top-3 owning modules, group_by_module symbol.rs:208)
  while the migrated LiveGraph `CallersAnswer`/`CalleesAnswer` are PARTITION-grouped (lib.rs:117-136). Two
  honest consequences the implementation MUST encode, both absorbed by the labelled fallback (never false
  completeness):
    - CALLERS_SUMMARY: when the referencing partitions are resident, the per-caller `(partition,key)`
      identities permit module mapping → LG-first summary served. When referencing partitions are
      non-resident, only partition COUNTS are known (not per-caller module) → the module-grouped summary is
      not fully derivable from LiveGraph → leaf FALLS BACK to SQLite `find_symbol_callers` (labelled), which
      carries `module_path` directly. Either way: Partial/fallback with reason, NEVER an Exact module-grouped
      summary from partition-only data.
    - CALLEES_SUMMARY: summary-level callees over a NON-resident defining partition is a RATIFIED LiveGraph
      deferral (lib.rs:200-205; CURRENT_SLICE.md:129-131 — the always-resident xref retains only INCOMING
      adjacency). So LG-first callees serves only when the defining partition is resident; otherwise the leaf
      falls back to SQLite `find_symbol_callees` (labelled). Until that deferral lifts, the SQLite fallback is
      the EXPECTED COMMON callees path — this is honest, ratified upstream, not a regression. Never Exact-empty.
  - SQLite-first / Authority / FS leaves: always carry their fixed source (sqlite / declaration /
    filesystem); they degrade only on snapshot staleness (Stale leaf when get_stale_files non-empty) or a
    read error (Unavailable leaf with a reason — "Unavailable is not empty", contract F3 / architecture.md
    Rule 6 "null=unknown, empty=known-zero").
  - ROOT: trust/freshness = MEET, so ONE degraded leaf lowers overall confidence but NEVER blanks the
    answer. The other leaves stay at their own posture (DISJOINT, not reconciled — contract Q7-4).

AUTHORITY OVERLAY NEVER ERASES (D-ORIENT-5 / contract D5): a waiver suppressing a GATE failure, or an
  entrypoint declaration suppressing a (withdrawn) dead-code signal, OVERLAYS the computed structural fact;
  both computed and effective views stay queryable across the LiveGraph/SQLite seam. orient must not let the
  seam become an excuse to drop the computed fact.

TRUST_BRIEFING IS A DEGRADED-STATE SURFACE, DISJOINT FROM THE AXES (D-ORIENT-6 = O2): `value.trust_briefing`
  is present ONLY when orient is degraded (the existing `has_degradation() || !caveats.is_empty()` gate,
  PRESERVED) and is the HUMAN caveat prose — a discovery surface (VISION §Orientation). It is NOT the
  machine certainty signal: that is `root.trust: TrustPosture` (always present, the MEET of the leaves). The
  two never substitute for each other and never need to agree (Q7-4). Absence of `trust_briefing` means "not
  degraded enough to brief", NOT "trust unknown" — the unknown/degraded posture always lives in `root.trust`
  + the leaves, so dropping the optional briefing can never hide degradation (no false completeness).

TRANSPORT-LEVEL DEGRADATION (OBSERVED, first-hand, distinct from the envelope's internal seam):
  [EXECUTED both iterations (i0 + i1 this turn), identical result: `rmap orient` with the daemon down ->
  "error: daemon connection failed: socket does not exist:
  ~/Library/Application Support/repo-graph/daemon.sock".] When the daemon socket is absent the
  CLI NEVER reaches handle_orient: it returns a CONNECTION ERROR and NO envelope at all. This is honest
  failure (a transport error, not a false-complete answer) and is OUTSIDE the CoherenceEnvelope's scope —
  the envelope models the daemon-INTERNAL LiveGraph x SQLite seam, not client<->daemon transport.
  IMPLICATION FOR VALIDATION: orient's coherence degradation is exercised daemon-side (agent + livegraph
  unit/integration tests with a live RepoState), NOT through a socketless CLI. The socketless path is a
  separate, already-correct transport behaviour; this slice neither changes it nor depends on it.

EMPTY vs UNKNOWN (orient-specific, preserve the existing discipline):
  orient ALREADY distinguishes known-zero from unknown via limits: MODULE_DATA_UNAVAILABLE (module
  discovery not queryable) vs an absent MODULE_SUMMARY module count; COMPLEXITY_UNAVAILABLE (no
  measurements) vs a zero HIGH_COMPLEXITY [OBSERVED module_summary.rs:31-39, repo.rs:138-143]. The wrapper
  MUST preserve this: a LiveGraph residency gap on the complexity leaf is UNKNOWN (Partial/Unavailable +
  reason), NOT an empty "known-zero" complexity list (forbids contract F3).
  FOCUS NUANCE (OBSERVED): COMPLEXITY_UNAVAILABLE is CONDITIONAL only at repo focus (emitted iff
  has_complexity_measurements is false, repo.rs:138-142); at file/path/symbol focus it is an UNCONDITIONAL
  static limit (file.rs:76, path.rs:91, symbol.rs:135) and HIGH_COMPLEXITY is never emitted there. The
  HIGH_COMPLEXITY LG-first leaf and its complexity cert ladder therefore exist ONLY at repo focus. The
  wrapper MUST NOT manufacture a HIGH_COMPLEXITY leaf — or issue a complexity LiveGraph read — at
  file/path/symbol focus; those focuses keep the static COMPLEXITY_UNAVAILABLE limit verbatim.
```

---

## 5. Validation plan (for the eventual implementation)

```text
Off-target first (architecture.md §Off-Target Testability + §Build Order: support module -> feature ->
tests). The cert machinery + MEET fold live in COHERENCE-ENVELOPE-1 (pure, unit-tested there); this slice
validates the ORIENT WIRING.

PARITY (no discovery loss vs today's SQLite orient):
  P1. With LiveGraph populated + GREEN cert (REPO focus, where both leaves co-occur): the IMPORT_CYCLES and
      HIGH_COMPLEXITY leaf VALUE payloads are byte-identical to the SQLite-computed equivalents (the migrated
      cycles/value-facts answers) — only the surrounding wrapper gains labels. HIGH_COMPLEXITY is repo-focus
      only; IMPORT_CYCLES parity is re-asserted per-focus (repo/path) in P4. [Reuse the imports/cycles/stats
      parity precedent.]
  P2. The SQLite-first / Authority / FS signals (SNAPSHOT_INFO, TRUST_*, BOUNDARY_*, MODULE_SUMMARY, GATE_*,
      documentation) are value-unchanged vs today's OrientResult.
  P3. Signal ordering/ranking/truncation + the limits set are unchanged (the ranking pass is unaffected by
      wrapping — wrapping is post-aggregation).
  P4. Focus parity (the §1c focus-coverage matrix is the assertion oracle): repo / file / path / symbol /
      ambiguous / no_match each produce the SAME signal+limit set as today, now wrapped (mod.rs dispatch
      unchanged). Per focus, EXACTLY:
        - REPO focus: the full §1a set, incl. BOUNDARY_LINKS_SUMMARY and the measurement-gated HIGH_COMPLEXITY.
        - FILE focus: ONLY snapshot, trust, the withdrawn dead-code no-op, the file-scoped MODULE_SUMMARY, the
          STATIC MODULE_DATA_UNAVAILABLE + COMPLEXITY_UNAVAILABLE limits, and documentation. ASSERT NO
          IMPORT_CYCLES, NO BOUNDARY_VIOLATIONS, NO BOUNDARY_LINKS_SUMMARY, NO GATE_*, and NO HIGH_COMPLEXITY;
          ASSERT the static COMPLEXITY_UNAVAILABLE limit IS present (§1a-file).
        - PATH focus: snapshot, trust, path-scoped IMPORT_CYCLES, path-scoped BOUNDARY_VIOLATIONS, the
          withdrawn dead-code no-op, path-scoped MODULE_SUMMARY, path-scoped GATE_*, the STATIC
          MODULE_DATA_UNAVAILABLE + COMPLEXITY_UNAVAILABLE limits, and documentation. ASSERT NO
          BOUNDARY_LINKS_SUMMARY and NO HIGH_COMPLEXITY; ASSERT the static COMPLEXITY_UNAVAILABLE limit IS
          present (§1a-path).
        - SYMBOL focus: still emits CALLERS_SUMMARY / CALLEES_SUMMARY (when non-empty) + the inherited
          ModuleContext signals, and still OMITS MODULE_SUMMARY / HIGH_COMPLEXITY (symbol.rs parity).
      GUARD: no focus-parity test may expect HIGH_COMPLEXITY outside REPO focus; the file/path tests MUST pin
      the static COMPLEXITY_UNAVAILABLE limit (and its MODULE_DATA_UNAVAILABLE companion) as present.
  P5. SYMBOL-FOCUS CALLGRAPH PARITY (D-ORIENT-SYMBOL-CALLGRAPH): with LiveGraph populated + GREEN cert and the
      needed partitions resident, the CALLERS_SUMMARY / CALLEES_SUMMARY leaf VALUE payloads (count + top-3
      module group, CallersSummaryEvidence / CalleesSummaryEvidence) are byte-identical whether the underlying
      rows came from the migrated LiveGraph `callers`/`callees` answer or from SQLite
      `find_symbol_callers`/`find_symbol_callees` — only the wrapper gains labels. The summary projection
      (group_by_module, top-3, count) is source-agnostic; assert identical evidence for both row sources.
      [Reuse the callers/callees migration parity precedent + the explain CALLERS/CALLEES posture.]

DEGRADATION:
  D-V1. GREEN cert -> the LG-first leaves read NO SQLite (panicking-SQLite-closure style, as the fastpath
        tests prove); fallback_reason=null.
  D-V2. RED/stale/missing cert OR precondition unmet (non-TS repo, non-resident partition) -> the LG-first
        leaf flips to source=sqlite with fallback_reason set; the leaf VALUE equals the SQLite answer.
  D-V3. PrecisionPending partition (SCIP refresh pending) -> the LG-first leaf is Partial+PrecisionPending,
        never Exact (invariant I6); root confidence MEET-capped accordingly.
  D-V4. Unavailable != empty: a residency gap on the complexity leaf yields Partial/Unavailable+reason, NOT
        an empty known-zero list.
  D-V5. Transport: socket-absent -> connection error, no envelope (OBSERVED today; assert UNCHANGED).
  D-V6. SYMBOL CALLERS residency: with referencing partitions NON-resident, CALLERS_SUMMARY does NOT mint an
        Exact module-grouped summary from partition-only counts -> it is Partial+reason OR falls back to
        SQLite `find_symbol_callers` (labelled, provenance.fallback_reason set). Assert: never Exact from
        partition-only data; the served summary equals the SQLite-derived module grouping.
  D-V7. SYMBOL CALLEES residency (the ratified deferral, lib.rs:200-205): with the defining partition
        NON-resident, CALLEES_SUMMARY falls back to SQLite `find_symbol_callees` (labelled) — assert this is
        the path taken, the leaf is never Exact-empty, and the value equals the SQLite answer. With the
        defining partition resident + GREEN cert, assert LG-first serves (per P5).

ENVELOPE CORRECTNESS:
  E1. MEET monotonicity (RESOLVING pipelines — repo/file/path/symbol): coherent root confidence <= legacy
      derive_repo_confidence on identical input; no fold yields an Exact root from a non-Exact leaf.
  E1z. ZERO-SIGNAL ROOT (AMBIGUOUS + NO-MATCH, mod.rs:262-341): assert the empty-signal builders do NOT take
      the empty MEET's lattice-TOP. Assert instead the explicit resolution-only posture (D-ORIENT-4 / §3b):
      root.confidence is the STATIC High preserved verbatim (legacy derive_repo_confidence is never called for
      these, so the E1 `<=` comparison is N/A); root.provenance.source = { sqlite } operational identity ONLY
      (no livegraph/declaration/filesystem); root.trust is labelled resolution-outcome, NEVER a structural
      Exact and never claims structural completeness; root.freshness = Fresh scoped to snapshot identity.
      `value.trust_briefing` is NOT a zero-signal-builder field — it is daemon-injected on the
      snapshot-degradation gate (focus-independent, dispatch.rs:2637-2652), so a degraded-snapshot
      ambiguous/no-match may still carry it; assert it follows the snapshot gate, not the focus. Pin that a
      zero-signal orient can NEVER serialize a structural-completeness Exact (the false-completeness guard).
  E2. Invariants I1-I6 hold at every leaf and survive the fold (Exact requires Fresh+Complete; Partial
      justified; Unavailable carries a reason; Stale!=Fresh; null!=empty; PrecisionPending!=Exact w/o
      non-SCIP basis).
  E3. provenance.source is correct per leaf (livegraph/sqlite/declaration/filesystem) and the root
      provenance.source is the exact SET union.
  E4. Authority overlay preserves computed fact (D-ORIENT-5): a waiver/entrypoint suppression keeps the
      computed view queryable alongside the effective view.
  E5. Envelope limits[] carry the provenance codes when (and only when) the matching degradation occurred.

WIRE SHAPE / RENDERER / FIXTURES (D-ORIENT-6 = O2 — the ratified decision's explicit finalization targets):
  W1. WIRE SHAPE, degraded: the top-level JSON is `CoherenceEnvelope<CoherentOrientResult>`; `value` carries
      `trust_briefing` (the former overlay shape, byte-identical to today's `trust` value); `root.trust`
      (TrustPosture) is present; the OLD top-level `trust` overlay key is ABSENT (renamed). Assert exactly
      ONE briefing object, under `value.trust_briefing`, and exactly ONE axis posture, under `root.trust`.
  W2. WIRE SHAPE, not degraded: `value.trust_briefing` is ABSENT (skip_serializing_if), matching today's
      behaviour where the `trust` key is omitted when not degraded; `root.trust` is still present.
  W3. SIBLING NON-CROSSING: a check/explain coherent response serializes with NO `trust_briefing` key (the
      shared container's Option field is None there). Pin this so the additive field never leaks into
      sibling-command wire shapes.
  W4. RENDERER: the CLI degradation section is rendered from `value.trust_briefing` (the renamed field;
      OBSERVED today it reads `self.trust` -> render_degradation at presentation/orient.rs:204-210/:362), and
      the certainty/confidence axes render from `root.trust`. Assert NO double-trust rendering and NO orphaned
      read of a now-absent top-level `trust` key. Degraded human output is text-equivalent to today's.
  W5. FIXTURES: the JSON-contract fixtures are updated in lockstep — a DEGRADED orient fixture (has
      `value.trust_briefing` + `root.trust`, no top-level `trust`), a NON-DEGRADED orient fixture (no
      `trust_briefing`, has `root.trust`), and a sibling-command fixture (no `trust_briefing`). Bump the
      orient schema id if the contract tests pin the top-level shape (RISK-O-F).

LIVE (after off-target green; macOS, ./scripts/dev-install-local.sh):
  L1. `rmap orient` (repo focus) on a TS pilot with a populated LiveGraph -> cycles/complexity leaves
      source=livegraph, GREEN cert, no per-call SQLite for those leaves; human render unchanged in shape.
  L2. `rmap orient` on a non-TS repo -> every LG-first leaf (cycles/complexity; and callers/callees at symbol
      focus) falls back to sqlite (labelled); all other signals intact.
  L3. Re-refresh -> fingerprint change -> cert rebuild -> next orient still serves correctly.
  L4. `rmap orient <symbol>` (symbol focus) on the TS pilot with the symbol's partition resident -> the
      CALLERS_SUMMARY (and CALLEES_SUMMARY when its defining partition is resident) leaves serve
      source=livegraph; with the defining partition non-resident, CALLEES_SUMMARY is labelled sqlite-fallback
      (D-V7); the module-grouped summary text is unchanged in shape vs today.
```

---

## 6. Scope boundary

```text
IN SCOPE: `rmap orient` ONLY — all focuses (repo/file/path/symbol/ambiguous/no_match). Wrap orient's answer
in CoherenceEnvelope<CoherentOrientResult>; cert-gate the FOUR LG-first leaf types with labelled SQLite
fallback — IMPORT_CYCLES (repo/path focus + symbol ModuleContext) + HIGH_COMPLEXITY (repo focus ONLY) AND, per
RATIFIED D-ORIENT-SYMBOL-CALLGRAPH,
CALLERS_SUMMARY + CALLEES_SUMMARY (symbol focus, served from the migrated `callers`/`callees` surfaces with
SQLite `find_symbol_callers`/`find_symbol_callees` fallback); keep
SNAPSHOT_INFO/TRUST_*/BOUNDARY_*/MODULE_SUMMARY/GATE_*/documentation at their mapped postures; per-leaf
provenance + root MEET. Per RATIFIED D-ORIENT-6 (O2):
rename the daemon trust overlay key `trust` -> `trust_briefing`, lift it onto the `CoherentOrientResult`
struct (populate before serialize, like `display_name`), keep it `Option` + degraded-only, and update the
CLI renderer + JSON-contract fixtures to read `trust_briefing` and `root.trust` distinctly (§5 W1-W5).

OUT OF SCOPE (separate later slices, per the contract slice sequence):
  - CHECK-LIVEGRAPH-1, EXPLAIN-LIVEGRAPH-1, TRUST-LIVEGRAPH-1 — the other three coherence commands.
  - COHERENCE-ENVELOPE-1 — the support module (the wrapper type, the MEET fold, the FreshnessInfo
    reconciliation). This slice DEPENDS on it; it is not built here.
  - The hybrid trust rebase — TRUST-DISPOSITION is RATIFIED but realized in TRUST-LIVEGRAPH-1, not orient.
  - SQLITE-RAW-DECOMMISSION-1 — orient still reads SQLite to build certs + serve fallbacks; no table is
    decommissioned here.

HARD GUARDRAILS (this slice's out-of-scope, mirroring the contract):
  NO source code (spec-first). NO table deletion, NO schema/data migration, NO default flip beyond
  specifying it. NO new producer for measurements/boundary/inferences. NO change to declarations/gate/
  authority semantics. NO raw nodes/edges decommission. NO non-TS LiveGraph support (non-TS -> SQLite
  fallback). NO edit to docs/ROADMAP.md or CURRENT_SLICE.md. NO live daemon run / index / refresh.
```

---

## Forced decisions — every cell filled

### D-ORIENT-1 — LG-first leaf set = {IMPORT_CYCLES, HIGH_COMPLEXITY, CALLERS_SUMMARY, CALLEES_SUMMARY} (DECIDED + RATIFIED)
```text
orient's LG-first leaves are FOUR signal types across all focuses:
  - IMPORT_CYCLES — module-cycle topology (LiveGraph module_import_cycles), repo + path focus, plus the
    inherited module-scoped variant at symbol focus when the symbol has an owning module
    (find_cycles_involving_module); NOT emitted at file focus.
  - HIGH_COMPLEXITY — cyclomatic-complexity (LiveGraph value_facts), repo focus ONLY (file/path/symbol emit the
    static COMPLEXITY_UNAVAILABLE limit instead; OBSERVED file.rs:76 / path.rs:91 / symbol.rs:135).
  - CALLERS_SUMMARY, CALLEES_SUMMARY — symbol-focus structural callgraph summaries (LiveGraph callers/callees
    surfaces), RATIFIED LG-first by D-ORIENT-SYMBOL-CALLGRAPH (operator 2026-06-09).
IMPORT_CYCLES + HIGH_COMPLEXITY were DECIDED, not asked: directly implied by COHERENCE-LAYER-1 Q4 + the
orient source map. CALLERS_SUMMARY + CALLEES_SUMMARY were ESCALATED (iteration 1) and RATIFIED — see
§D-ORIENT-SYMBOL-CALLGRAPH for the matrix. All other signals are SQLite-first/Authority/FS. (Supersedes the
iteration-1 "two LG-first leaves" statement.) Recorded.
```

### D-ORIENT-2 — MODULE_SUMMARY stays SQLite-first (DECIDED, recorded)
```text
The module count is anchored to SQLite `module_candidates` by ORIENT-BUG-1; RISK-E (LiveGraph dirname
aggregation vs manifest module_candidates identity divergence) forbids a naive LG count. DECIDED, not
asked: the contract's orient row pins it SQLite-first. The file/path structural counts are LG-derivable
but kept SQLite for count-anchor consistency — a deferred optimization, out of scope. Recorded.
```

### D-ORIENT-3 — dead-code: NO migration; surface is withdrawn (DECIDED, recorded — resolves contract RISK-D)
```text
COHERENCE-LAYER-1 RISK-D flagged that orient "still CALLS find_dead_nodes" and required ORIENT-LIVEGRAPH-1
to confirm whether that path surfaces output or is dormant. CONFIRMED first-hand: the dead_code aggregator
is WITHDRAWN — `aggregate()` returns `AggregatorOutput::empty()` UNCONDITIONALLY and does NOT call
find_dead_nodes [OBSERVED dead_code.rs:42-52; module header "surface withdrawn"]. The repo pipeline invokes
the aggregator [OBSERVED repo.rs:114-116] but it emits NOTHING. So orient produces NO dead-code signal
today. DECISION: do NOT migrate a withdrawn surface; orient gains no dead-code leaf. EVIDENCE NOTE: the
contract's RISK-D phrasing ("find_dead_nodes invoked at repo.rs:114") reflected the call SITE; the callee
became a no-op at surface withdrawal, so find_dead_nodes is not actually reached. This RECONCILES RISK-D
(the contract explicitly delegated the resolution here); it does not contradict the contract. Recorded.
```

### D-ORIENT-4 — confidence becomes one contributor to the root MEET; zero-signal builders take an explicit resolution-only posture (DECIDED, recorded)
```text
RESOLVING PIPELINES (repo/file/path/symbol — SNAPSHOT_INFO + TRUST_* are always emitted, so the leaf set is
non-empty). Today `confidence = derive_repo_confidence(trust_summary, stale)` [OBSERVED repo.rs:153 /
file.rs:84 / path.rs:99 / symbol.rs:148, confidence.rs:43]. Under the wrapper, root confidence is DERIVED from
the MEET of all leaves and never exceeds the weakest (contract Q6 + D3). DECIDED, not asked: implied by the
ratified MEET. The legacy value becomes ONE MEET input; the fold is monotone so coherent confidence <= legacy
confidence (validation E1/R2). Recorded.

ZERO-SIGNAL BUILDERS (AMBIGUOUS + NO-MATCH) — the empty-fold carve-out (DECIDED, recorded; resolves review-4
item 3). [OBSERVED first-hand: build_ambiguous_result mod.rs:262-303 and build_no_match_result mod.rs:310-341
emit ZERO signal leaves and a STATIC `Confidence::High`, NOT `derive_repo_confidence` (mod.rs:285/:323).]
These produce NO structural signal leaves, so the structural MEET fold (which folds over the leaf
TrustPostures) has NO inputs. The contract's MEET is a greatest-lower-bound; the GLB of the EMPTY set is the
lattice TOP (Exact/Fresh/Complete). Taking that default SILENTLY would mint an Exact/Fresh/Complete root over
structure that was never analyzed — a contract-F-class FALSE COMPLETENESS (the exact Fact-Certainty hazard
review-4 flagged). So the zero-signal case is NOT served by the empty fold defaulting to TOP. INSTEAD the
zero-signal builders take an EXPLICITLY LABELLED RESOLUTION-ONLY posture, constructed directly — the
reviewer's option-1+3 synthesis (operational-identity-carried AND explicitly labelled):
  - root.provenance.source = { sqlite } ONLY — the operational repo + snapshot IDENTITY (Layer A2) that DID
    resolve. NO livegraph / declaration / filesystem source is claimed, because none was consulted. This label
    is the anti-false-completeness GUARD: a consumer reading provenance sees operational-identity-only and
    CANNOT mistake the result for "structural analysis complete, zero findings".
  - root.confidence = the legacy STATIC `Confidence::High` PRESERVED VERBATIM (mapped straight through, NOT
    recomputed from an empty MEET). The resolution outcome (ambiguous / no-match) is certain.
  - root.trust (TrustPosture): a resolution-outcome posture — degradation_reasons = none, completeness =
    Complete *as a resolution outcome* (the focus resolution is definitive: ambiguous candidates listed / no
    match), NOT a structural-completeness Exact. The label MUST read "resolution: ambiguous|no_match; no
    structural analysis performed", carried by the operational-identity-only provenance above so it is never
    read as exhaustive structural coverage.
  - root.freshness = Fresh — scoped to the operational SNAPSHOT IDENTITY epoch only (the latest READY snapshot
    the builder resolved, mod.rs:283/:321), NOT a structural-partition epoch. ambiguous/no-match run no stale
    check and no aggregator, so there is no structural freshness to report; the operational identity IS current.
The distinction is load-bearing: "High" here = certainty about the RESOLUTION, NOT about structural
completeness. This carve-out is a decide-and-record realization of the ratified MEET (the empty-fold edge case
the MEET does not itself specify), NOT a new boundary decision — it REMOVES a false-trust risk rather than
creating one, so it is recorded, not escalated (CLAUDE.md §Decision Autonomy). The CoherentOrientResult shape
is unchanged (confidence: Confidence already exists; the zero-signal builders already exist). Validation: E1z
(zero-signal assertions); §3b (zero-leaf root).
```

### D-ORIENT-5 — BOUNDARY_VIOLATIONS + GATE = Authority, overlay-preserves-computed (DECIDED, recorded)
```text
Both read Tier-A1 `declarations` (Authority); they OVERLAY the computed structural fact and never erase it;
both computed and effective views stay queryable across the seam. DECIDED, not asked: VISION §Agent
Priorities #2 + contract D5 applied to orient. Recorded.
```

### D-ORIENT-6 — daemon `trust` overlay disposition under the wrapper (RATIFIED — operator sign-off 2026-06-09 = O2 RETAIN RENAMED)
```text
STATUS: RATIFIED (operator sign-off 2026-06-09 = O2). Escalated at iteration 0; resolved by the operator at
iteration 1. The matrix and gap analysis below are RETAINED as the decision record (CLAUDE.md §Decision
Autonomy: surface a boundary decision as an exhaustive matrix). NO open DECISION_REQUIRED remains.

THE GAP (why this was a genuine boundary decision, retained for the record). orient's CURRENT wire output is
NOT just OrientResult: the daemon, when the answer is degraded, inserts a SEPARATE top-level `trust` key
built by compute_trust_overlay_for_snapshot [OBSERVED dispatch.rs:2638-2648; type
repo_graph_trust::TrustOverlaySummary, util/trust.rs:16]. This overlay is a daemon-runtime "briefing
surface" object (with has_degradation() + human caveats), NOT an `OrientResult` field and NOT a `Signal` —
so it does NOT become a leaf automatically, and the contract's CoherentOrientResult definition ("OrientResult
with the signals slot re-typed, ALL other fields verbatim", D7) does NOT account for it.

Under the wrapper the root gains a `trust: TrustPosture` SIBLING (contract Q6). That produced a NAME +
SEMANTIC overlap with the legacy daemon `trust` overlay key: two different `trust`-named objects at
different levels with overlapping meaning — a data shape crossing the daemon->CLI boundary that the ratified
contract did not settle (CLAUDE.md §Decision Autonomy: "data shape crossing a boundary" -> STOP and surface
as an exhaustive matrix). The operator resolved it as O2; the rest of this spec was fully determined without
the resolution and is now consistent with it.

| Option | Wire shape | Briefing caveats preserved? | `trust` name collision w/ root sibling | Consumer migration cost | Fact-Certainty fit | Verdict |
|---|---|---|---|---|---|---|
| O1 SUPERSEDE — drop the daemon overlay; rely on root `trust: TrustPosture` + the per-leaf trust leaves | smallest (one `trust` at root, axis-typed) | NO (human caveat prose lost) | none | medium (overlay-key consumers re-read root.trust) | clean (single trust truth) | NOT CHOSEN — would lose the briefing prose (a discovery surface) |
| O2 RETAIN as a distinct, explicitly-named field on CoherentOrientResult (`trust_briefing: Option<TrustOverlay>`), separate from root `trust` | +1 named field; root `trust` sibling = axes, `trust_briefing` = prose | YES | none (renamed) | low-medium (key renamed; both present) | clean (disjoint: axes vs briefing, no two-truths) | **RATIFIED (operator sign-off 2026-06-09)** — preserves the briefing AND removes the collision; matches the contract's DISJOINT-not-reconciled posture (Q7-4) |
| O3 PROMOTE the overlay into the root `trust` sibling (map TrustOverlay->TrustPosture) | one `trust` at root, widened | PARTIAL (only if TrustPosture is extended to carry caveats) | none | medium (shape of root.trust changes) | risky (forces a richer overlay into an axis type; risks conflating briefing with axes) | NOT CHOSEN — conflates briefing prose with axis-typed posture |
| O4 KEEP the daemon overlay as a separate top-level key ALONGSIDE the wrapper (status quo position) | two `trust`-named things at different levels | YES | YES (collision) | lowest (no overlay change) | WORST (two trust truths, ambiguous authority — a Fact-Certainty hazard) | NOT CHOSEN — keeps the name collision / two-truths hazard |

RESOLUTION (RATIFIED, operator sign-off 2026-06-09 = O2 RETAIN RENAMED). The degraded-state daemon trust
overlay is RETAINED as a distinct, explicitly-named field `trust_briefing: Option<TrustOverlay>` on
`CoherentOrientResult`, alongside — and disjoint from — the CoherenceEnvelope root `trust: TrustPosture`
(axis-typed). This preserves the human caveat prose (a discovery surface — VISION §Orientation), eliminates
the name/semantic collision (root `trust` = axes; `trust_briefing` = prose), and honours the contract's
"no parallel structures that must agree" discipline (Q7-4). The wire KEY changes (old top-level `trust` ->
`value.trust_briefing`).

What the ratification finalizes (all consistent above):
  - WIRE SHAPE: `value.trust_briefing` holds the existing `TrustOverlaySummary` value (daemon side), present
    ONLY when degraded (gate preserved verbatim); `root.trust: TrustPosture` always present; the old
    top-level `trust` key is gone. (§3b, §5 W1-W3.)
  - RENDERER: the CLI degradation section renders from `trust_briefing`; the certainty axes render from
    `root.trust`; no double-trust render, no orphaned top-level `trust` read. (§5 W4.)
  - FIXTURES: degraded / non-degraded orient fixtures + a sibling-command fixture pin the shape; schema-id
    bump if the contract tests pin the top level. (§5 W5, RISK-O-F.)
  - REALIZATION (decide-and-record, implied by the ratified decision — NOT a new boundary call): the daemon
    populates `trust_briefing` on the struct before serialize (like `display_name`, dispatch.rs:2615),
    replacing the post-serialize JSON insert; the field is `Option` + `skip_serializing_if`, so check/explain
    (no overlay) keep an unchanged wire shape. (§3b SHARED-CONTAINER NON-CROSSING, §3c R4.)

TYPE NOTE: "TrustOverlay" in the field signature is the CLI-side deserialization mirror
[OBSERVED presentation/orient.rs:144-172]; the daemon serializes `repo_graph_trust::TrustOverlaySummary`
[OBSERVED util/trust.rs:16] under the `trust_briefing` key — exactly as it does today under `trust`. The
overlay's internal shape is unchanged; only the key name and the populate-site move.

CRATE-HOME / DEPENDENCY-EDGE NOTE (contract-deferred, NOT settled here): making `trust_briefing` a TYPED
field on `CoherentOrientResult` raises the question of which crate owns the field's type and whether that
crate may depend on `repo-graph-trust` (where `TrustOverlaySummary` lives). The contract EXPLICITLY DEFERS
the CoherenceEnvelope / CoherentOrientResult home-crate + dependency decision to COHERENCE-ENVELOPE-1
[OBSERVED coherence-layer-1.md:385-386, 601: "the HOME crate ... is a small boundary call DEFERRED to
COHERENCE-ENVELOPE-1, not re-opened here"]. So this slice does NOT settle the dependency edge; `trust_briefing`
inherits whatever COHERENCE-ENVELOPE-1 chooses. CONSTRAINT this slice imposes on that later choice: the
realization MUST NOT create a forbidden inward dependency (e.g. a stable DTO crate depending on a more
volatile one). The three dependency-clean realizations available to COHERENCE-ENVELOPE-1 — (a) the container
crate depends on `repo-graph-trust` and holds `TrustOverlaySummary` directly; (b) a DTO mirror of the overlay
in the container crate, daemon maps `TrustOverlaySummary` -> mirror; (c) an opaque `serde_json::Value`
carrier set by the daemon — are all consistent with O2's wire shape; picking among them is the deferred
home-crate call, not an orient decision.
```

### D-ORIENT-SYMBOL-CALLGRAPH — symbol-focus CALLERS_SUMMARY / CALLEES_SUMMARY source posture (RATIFIED — operator sign-off 2026-06-09 = LG-first)
```text
STATUS: RATIFIED (operator sign-off 2026-06-09 = LG-first). Escalated at iteration 1; resolved by the
operator. The matrix below is RETAINED as the decision record. NO open DECISION_REQUIRED remains.

THE GAP (why this was a genuine boundary decision, retained for the record). The contract's repo-focus orient
source map (coherence-layer-1.md:311-323) enumerates the REPO-focus signals only; it did NOT list orient's
SYMBOL-focus structural callgraph signals CALLERS_SUMMARY / CALLEES_SUMMARY [OBSERVED first-hand
orient/symbol.rs:89-100; Signal constructors signal.rs:1339-1377]. This doc's iteration-1 draft inherited that
omission and asserted "exactly TWO LG-first leaves". The iteration-1 review flagged it: these two symbol-focus
signals read storage today (find_symbol_callers/find_symbol_callees, SQLite) but the SAME underlying reads are
ALREADY served LiveGraph-first elsewhere — the migrated `callers`/`callees` surfaces (livegraph lib.rs:443/560;
daemon feed livegraph_feed.rs:448/544), which the contract itself assigns LG-first in its EXPLAIN row
(coherence-layer-1.md:341-342). Whether orient's symbol-focus summaries cross to LiveGraph-first or stay
snapshot-bound is a source-boundary decision (LiveGraph/SQLite seam) the submitted document had not settled —
hence the escalation.

| Option | Source posture | Contract consistency | Parity / degradation cost | Fact-Certainty fit | Verdict |
|---|---|---|---|---|---|
| O-LG — LG-first via the migrated callers/callees surfaces, labelled SQLite fallback | symbol callgraph summaries derive from `AnswerEnvelope<CallersAnswer/CalleesAnswer>`; fall back to find_symbol_callers/callees when LG can't fully answer | CONSISTENT — matches the contract's explain CALLERS/CALLEES rows (coherence-layer-1.md:341-342); no new producer | adds symbol-focus callgraph parity (P5) + residency degradation (D-V6/D-V7); partition→module grouping + callees-residency handled by labelled fallback (RISK-O-H) | clean — current-state where derivable, honest fallback otherwise; never Exact from partition-only data; never Exact-empty | **RATIFIED (operator sign-off 2026-06-09)** — current-state discovery for the highest-value drilldown signals, reusing already-migrated surfaces |
| O-SQL — keep symbol callgraph summaries SQLite-first for this slice | snapshot-bound; orient symbol callers/callees never consult LiveGraph | DIVERGES from the explain-row posture for the SAME reads; leaves orient inconsistent with explain | lower build cost; but symbol-focus callers/callees stay stale-bound despite a resident LiveGraph | weaker — snapshot-bound where a current-state surface exists; not false, but loses freshness | NOT CHOSEN — would re-split the seam the contract already unified; deferral unjustified given structural LG availability |
| O-OOS — exclude symbol-focus callgraph from ORIENT-LIVEGRAPH-1 | the two signals carry no posture in this slice | CONFLICTS — the slice's DoD is to cover orient's CURRENT outputs exactly; symbol focus emits them today | n/a (scope cut) | WORST — an incomplete source map silently omitting live outputs reads as false completeness about the spec | NOT CHOSEN — changes the ratified slice scope; the DoD requires the full output set |

RESOLUTION (RATIFIED, operator sign-off 2026-06-09 = LG-first). orient's symbol-focus CALLERS_SUMMARY /
CALLEES_SUMMARY are LiveGraph-first leaves, served from the already-migrated callers/callees surfaces
(livegraph lib.rs:443/560 via the cert-gated feed livegraph_feed.rs:448/544), with labelled SQLite fallback to
find_symbol_callers / find_symbol_callees. They join IMPORT_CYCLES + HIGH_COMPLEXITY as orient's LG-first
leaves (now FOUR types; §2, D-ORIENT-1). Same posture the contract already assigns these reads in its explain
row — orient and explain are now consistent on the callers/callees seam.

What the ratification finalizes:
  - SOURCE MAP: §2 gains the CALLERS_SUMMARY / CALLEES_SUMMARY LG-first rows; the "two LG-first leaves"
    statement is corrected to FOUR.
  - SUMMARY PROJECTION (decide-and-record, implied — NOT a new boundary call): orient's summary is MODULE-
    grouped (count + top-3 owning modules, group_by_module symbol.rs:208) while the migrated LiveGraph answer
    is PARTITION-grouped (CallersAnswer/CalleesAnswer, lib.rs:117-136). The summary projection runs over
    WHICHEVER row source serves; when the LiveGraph answer cannot yield the module grouping at full fidelity
    (non-resident referencing partitions for callers), the leaf falls back to SQLite (labelled), which carries
    module_path directly. The group-by-module/top-3/count logic is unchanged and source-agnostic.
  - CALLEES RESIDENCY (decide-and-record, ratified UPSTREAM): summary-level callees over a non-resident
    defining partition is a ratified LiveGraph deferral (lib.rs:200-205; CURRENT_SLICE.md:129-131). So LG-first
    callees serves only when the defining partition is resident; otherwise labelled SQLite fallback — the
    expected common path until the deferral lifts. Honest degradation, not a regression.
  - VALIDATION: §5 P5 (parity, source-agnostic evidence) + D-V6/D-V7 (residency degradation) + L4 (live).
  - RISK: RISK-O-H records the partition/module + callees-residency seam; the labelled fallback bounds it.

NOT A NEW ESCALATION. The realization constraints above (partition-vs-module grouping; callees residency) do
NOT threaten the ratified invariants (LG-first WITH labelled SQLite fallback; no false completeness): the
fallback is the explicit safety valve the ratified posture names, and no-false-completeness is preserved by
Partial/fallback labelling and the never-Exact-empty rule. Per CLAUDE.md §Decision Autonomy these are
decide-and-record details a ratified decision implies, not fresh boundary calls — so they are recorded here,
not re-escalated.
```

### D-ORIENT-7 — boundary_links FreshnessInfo reconciliation deferred to COHERENCE-ENVELOPE-1 (RECORDED, contract-deferred)
```text
BOUNDARY_LINKS_SUMMARY sets `Signal.freshness: Option<FreshnessInfo>` (Current/Impacted/Unknown) [OBSERVED
boundary_links.rs:46-51]. COHERENCE-LAYER-1 RISK-G assigns the FreshnessInfo->FreshnessState mapping to
COHERENCE-ENVELOPE-1 (retire FreshnessInfo or keep it render-only; the OUTER leaf freshness is
authoritative). RECORDED, not re-decided here: a realization detail the ratified wrapper implies. orient's
implementation consumes the mapping COHERENCE-ENVELOPE-1 provides; it does not invent a second one.
```

---

## Risks (orient-specific projections of the contract risks; each the implementation must address)
```text
RISK-O-A (= contract RISK-A, EPOCH SKEW). A LiveGraph cycle/complexity partition Fresh while the SQLite
  snapshot is a stale index (or vice-versa) -> a blended orient could read Fresh-overall. MITIGATION: the
  MEET fold (D-ORIENT-4) + the shared cert fingerprint (spans LiveGraph partition epochs AND the SQLite
  snapshot_uid). The fold is monotone — cannot raise to Fresh.
RISK-O-B (= contract RISK-B, AUTHORITY/STRUCTURE SEAM). A careless impl could drop the computed boundary/
  gate fact when applying a declaration overlay. MITIGATION: D-ORIENT-5 — both computed and effective views
  queryable.
RISK-O-E (= contract RISK-E, MODULE IDENTITY). LiveGraph dirname aggregation vs SQLite module_candidates
  identity may diverge. MITIGATION: D-ORIENT-2 — MODULE_SUMMARY count stays SQLite (ORIENT-BUG-1 anchor);
  no LG module count in orient.
RISK-O-F (= contract RISK-F, ENVELOPE SHAPE CHURN). The wrapper changes orient's JSON wire shape (the
  wrapper becomes the new top level; CoherentOrientResult becomes its value) AND, per RATIFIED D-ORIENT-6,
  renames the degraded-state `trust` overlay to `value.trust_briefing`. ACCEPTED, bounded. MITIGATION:
  land the wrapper ONCE in COHERENCE-ENVELOPE-1 before this build; keep the reused structural VALUE payloads
  (cycles/complexity) byte-identical; update the CLI renderer + JSON-contract fixtures in lockstep
  (schema-version bump if the contract tests pin the top-level shape). The D-ORIENT-6 wire/renderer/fixture
  consequences are RATIFIED (O2) and specified in §5 W1-W5; no open decision remains.
RISK-O-G (= contract RISK-G, EXISTING FRESHNESS). Two freshness vocabularies on BOUNDARY_LINKS_SUMMARY.
  MITIGATION: D-ORIENT-7 — single mapping from COHERENCE-ENVELOPE-1; outer leaf freshness authoritative.
RISK-O-H (NEW, orient-specific; D-ORIENT-SYMBOL-CALLGRAPH). SYMBOL CALLGRAPH SUMMARY SEAM. orient's
  CALLERS_SUMMARY / CALLEES_SUMMARY are MODULE-grouped summaries, but the migrated LiveGraph callers/callees
  answers are PARTITION-grouped (CallersAnswer/CalleesAnswer, lib.rs:117-136) and callees carries a ratified
  residency asymmetry (summary callees over non-resident defining partitions deferred, lib.rs:200-205). A
  careless impl could (a) mint an Exact module-grouped summary from partition-only counts, or (b) Exact-empty
  callees when the defining partition is merely non-resident. MITIGATION: derive the summary only where the
  needed partitions are resident; otherwise FALL BACK to SQLite find_symbol_callers/find_symbol_callees
  (labelled), which carries module_path directly. Never Exact from partition-only data; never Exact-empty
  (contract F3). Validated by P5 + D-V6/D-V7. The labelled fallback is the explicit safety valve the ratified
  LG-first-with-fallback posture names — this risk is BOUNDED, not open.
```

## References
```text
ORIENT COMMAND (OBSERVED first-hand; [i0]=iteration-0 authoring read, [i1]=re-verified THIS turn):
- [i0] rust/crates/agent/src/orient/mod.rs (focus dispatch :57-232); [i1] orient/repo.rs (repo pipeline
  :58-191; confidence :153; documentation :161/208; complexity gate :138/142; OrientResult build :166-190).
- [i0] rust/crates/agent/src/aggregators/snapshot.rs:13-25; trust.rs:34-82; cycles.rs:17-46; boundary.rs:
  45-111; boundary_links.rs:21-57; module_summary.rs:41-85; gate.rs:40-206; complexity.rs:23-66.
  [i1] dead_code.rs:38-52 (surface withdrawn; aggregate() -> AggregatorOutput::empty() unconditionally).
- [i0] rust/crates/agent/src/confidence.rs:43-70 (derive_repo_confidence).
- [i1] rust/crates/daemon-runtime/src/dispatch.rs handle_orient:2600-2668 (SQLite assembly; display_name set
  on struct :2615; overlay computed :2638, graph_basis "CALLS+IMPORTS" :2642; post-serialize JSON insert iff
  has_degradation()||!caveats.is_empty() :2644-2648); util::compute_trust_overlay_for_snapshot (imported :40).
- [i1] rust/crates/daemon-runtime/src/util/trust.rs:11-35 (compute_trust_overlay_for_snapshot ->
  Option<repo_graph_trust::TrustOverlaySummary>).
- [i1] rust/crates/rgr/src/presentation/orient.rs:83 (OrientResponse.trust: Option<TrustOverlay>); :144-172
  (TrustOverlay / ReliabilitySection / ReliabilityAxis); :204-210 + :362 (render_degradation reads self.trust).
- [i1] rust/crates/agent/src/dto/envelope.rs:300-339 (OrientResult; display_name :309-310, documentation
  :317-318, confidence :313); dto/signal.rs:947-959 (Signal DTO, freshness:958).
- [i2] rust/crates/agent/src/orient/symbol.rs:64-185 (symbol pipeline; CALLERS_SUMMARY find_symbol_callers
  :89-93; CALLEES_SUMMARY find_symbol_callees :96-100; inherited ModuleContext boundary :108-113 / cycles
  :116-120 via find_cycles_involving_module :302 / gate :123-128; COMPLEXITY_UNAVAILABLE :135; MODULE_SUMMARY +
  HIGH_COMPLEXITY omitted :136-140; group_by_module :208-223). dto/signal.rs:1339-1377 (callers_summary ->
  SourceRef::StorageFindSymbolCallers; callees_summary -> StorageFindSymbolCallees).
- [i2] rust/crates/agent/src/storage_port.rs:403-420 (AgentCallerRow / AgentCalleeRow carry module_path +
  module_stable_key); :607/:615/:623 (find_symbol_callers / find_symbol_callees / find_cycles_involving_module).
- [i2] rust/crates/repo-graph-livegraph/src/lib.rs:443 (LiveGraph::callers -> AnswerEnvelope<CallersAnswer>),
  :560 (LiveGraph::callees -> AnswerEnvelope<CalleesAnswer>), :117-136 (CallersAnswer/CalleesAnswer
  partition-grouped), :200-205 (ratified callees summary-residency asymmetry/deferral).
- [i2] rust/crates/daemon-runtime/src/livegraph_feed.rs:448 (callers_engine_response), :544
  (callees_engine_response), :131-182 (FallbackReason) — the cert-gated fastpath reused for the two callgraph leaves.
- [OBSERVED i2] docs/slices/coherence-layer-1.md:341-342 (explain CALLERS/CALLEES = LG-first, direct reuse of
  migrated callers/callees — the posture orient now matches); CURRENT_SLICE.md:129-131 (callees residency
  deferral ratified upstream).
- [EXECUTED i0] `rmap orient` -> daemon socket-absent connection error (transport degradation path).
- D-ORIENT-6 RATIFIED: operator sign-off 2026-06-09 = O2 RETAIN RENAMED (selection packet; this turn).
- D-ORIENT-SYMBOL-CALLGRAPH RATIFIED: operator sign-off 2026-06-09 = LG-first (selection packet; this turn).

CONTRACT + MODEL:
- docs/slices/coherence-layer-1.md (RATIFIED) — Q1-Q8, the orient source map, the envelope spec (D7), the
  MEET fold (D3), authority overlay (D5), the safe-fallback ladder, RISK-A/B/D/E/F/G, the slice sequence.
- docs/VISION.md §Fact Certainty Model / §Product Layer Model / §Agent Priorities #2.
- agent_docs/architecture.md §Product Layer Stack / Rule 6 (null=unknown, empty=known-zero) / §Build Order.
- docs/slices/orient-bug-1-module-count.md (the module-count SQLite anchor).

PRECEDENT (the cert-gated fastpath this slice reuses for the FOUR LG-first leaves):
- docs/slices/imports-livegraph-default-fastpath-1.md; cycles-livegraph-default-fastpath-1.md;
  stats-livegraph-1.md (cycles/complexity precedent); query-migration-1.md +
  livegraph-integration-1b.md (the migrated callers/callees surfaces this slice reuses for the symbol leaves).
- rust/crates/daemon-runtime/src/livegraph_feed.rs (the fastpath ladder + FallbackReason + the shared
  fingerprint; callers_engine_response:448, callees_engine_response:544); rust/crates/repo-graph-livegraph/
  src/lib.rs (module_import_cycles:1264, value_facts:662, callers:443, callees:560).
```
