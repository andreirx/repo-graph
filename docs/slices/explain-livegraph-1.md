# EXPLAIN-LIVEGRAPH-1: apply the coherence contract to `rmap explain`

Slice ID: EXPLAIN-LIVEGRAPH-1
Status: **DESIGN / SPEC-FIRST — NOT IMPLEMENTED — DECISION-COMPLETE.** This document SPECIFIES the THIRD
per-command application of the ratified COHERENCE-LAYER-1 contract (orient was first, check second). It
produces NO source code, NO table deletion, NO schema/data migration, NO default flip. The implementation is
a LATER slice and depends on COHERENCE-ENVELOPE-1 (the support module) + ORIENT-LIVEGRAPH-1 (the wrapper
pattern + the `CoherentOrientResult` container + the ratified `trust_briefing` field) landing first.
**No open DECISION_REQUIRED remains.** Every explain-specific point below is DECIDE-AND-RECORD *within* the
ratified contract (the contract's explain source map already assigns each section its posture,
coherence-layer-1.md §"Per-command source map" / explain row) or a direct reuse of an orient/check ratified
decision; none is a boundary decision *beyond* the contract. The exhaustive matrices are retained for the
decision audit trail.

**COMPLETE OUTPUT ENUMERATION (the load-bearing requirement — codex rejected sibling drafts that omitted
signals):** `rmap explain` produces THREE distinct output surfaces, ALL enumerated FIRST-HAND below from the
explain command code: (1) the daemon/API envelope — its ELEVEN signal codes, the GENERIC per-signal
serialized shape (§1a-shape), their conditions, the envelope fields, AND the daemon-injected trust overlay
(§1a–§1d); (2) the human-render content (§1e); (3)
the CLI PROCESS wrapper — stdout/stderr + exit code + the `--budget` flag (§1f). explain is NOT CI-facing
(`run_explain_cmd` returns `ExitCode::SUCCESS` in BOTH success arms, like orient, UNLIKE check), so the
wrapper migration is simpler than check's — a human-render deserialization remap with NO exit-code remap.

**LOAD-BEARING FIRST-HAND CORRECTION (the orient precedent was wrong about explain):** `handle_explain`
INJECTS a daemon trust overlay (dispatch.rs:2800-2816), IDENTICAL to orient's — same `has_degradation() ||
!caveats.is_empty()` gate, same `"CALLS+IMPORTS"` basis, same post-serialize `"trust"` key. So explain DOES
produce an overlay. ORIENT-LIVEGRAPH-1 §3b (orient-livegraph-1.md:539) and its W3 (orient-livegraph-1.md:762)
incidentally asserted "**check/explain** produce no overlay" — that claim is FALSE for explain
(OBSERVED first-hand). CONSEQUENCE: explain is the **SECOND** command to POPULATE the ratified `trust_briefing`
field (orient first; check leaves it `None` per D-CHECK-2). This CORRECTS orient's incidental aside and
STRENGTHENS the ratified O2 decision (the shared container field serves orient AND explain, exactly as a
shared field should). It does NOT re-open O2 (D-EXPLAIN-TRUST-BRIEFING). [OBSERVED, first-hand:
dispatch.rs:2800-2816; presentation/explain.rs:64-81 has no `trust` field.]

Goal: specify how `rmap explain` serves its multi-section deep-dive from current-state **LiveGraph**
structural facts combined with durable **SQLite/persisted authority**, wrapped in `CoherenceEnvelope<T>`,
with honest degradation and no false completeness — by DIRECT REUSE of the already-migrated
callers/callees/imports/cycles surfaces for its structural sections, the heaviest but most directly reusable
of the four coherence commands.

Track: Stage D, SQLITE-RAW-DECOMMISSION path — third per-command coherence build.

Authoritative contract (RATIFIED + AMENDED, read FIRST): `docs/slices/coherence-layer-1.md`. This slice
REUSES that contract's `CoherenceEnvelope<T> { value, provenance, trust, freshness }` wrapper, its explain
source map (coherence-layer-1.md §"Per-command source map" → explain row), its MEET fold (D3), its
authority-overlay rule (D5), its multi-source-leaf `Provenance.source: BTreeSet<Source>` (D8), and its
safe-fallback ladder (ORIENT/EXPLAIN row). It does NOT re-open COHERENCE-ENVELOPE-SHAPE (RATIFIED = Option B
wrapper), TRUST-DISPOSITION (RATIFIED = hybrid; trust is a separate later slice), or any D1–D8 decision.

Precedent (followed for SHAPE, reused — NOT re-derived):
- `docs/slices/orient-livegraph-1.md` — the FIRST per-command application. This doc mirrors its structure
  (current-outputs enumeration → source map → envelope wiring → degradation → validation → scope →
  forced-decision matrices → risks → evidence log) and reuses its ratified container `CoherentOrientResult`
  (the shared `OrientResult` with its `signals` slot re-typed to leaf envelopes, contract D7) AND its
  ratified `trust_briefing: Option<...>` field (D-ORIENT-6 = O2) — which explain POPULATES (the correction
  above), unlike check. explain's LG-first leaves are the SAME callers/callees/cycles reuses orient ratified
  (D-ORIENT-SYMBOL-CALLGRAPH = LG-first), plus file-focus imports.
- `docs/slices/check-livegraph-1.md` — the SECOND application. This doc reuses its three-surface enumeration
  discipline (daemon envelope / human render / CLI wrapper), its multi-source-leaf treatment (D8 / D-CHECK-5),
  and its CLI-wrapper remap analysis (§3e) — minus the exit-code half (explain is not CI-facing).

Depends (precedent, reused — NOT re-derived here):
- COHERENCE-LAYER-1 — the ratified + amended mixed-source contract (envelope shape, explain source map, MEET,
  D8 multi-source provenance, fallback ladder).
- COHERENCE-ENVELOPE-1 — the SUPPORT module that realizes `CoherenceEnvelope<T>` + `CoherentOrientResult` +
  the MEET fold + the `Provenance.source: BTreeSet<Source>` build + the FreshnessInfo→FreshnessState
  reconciliation. **MUST land before this slice's implementation** (architecture.md §Build Order: support
  module → feature).
- ORIENT-LIVEGRAPH-1 — the first feature build. It ratifies `CoherentOrientResult`, the `trust_briefing`
  field (O2), the cert-gated LG-first leaf pattern for cycles/complexity/callers/callees, and the zero-signal
  resolution-only root (D-ORIENT-4). The contract sequences EXPLAIN **after** ORIENT precisely so explain
  "becomes LG-first leaves by direct reuse of the migrated drilldown answers ... after the pattern is
  de-risked on orient" (coherence-layer-1.md §slice sequence). explain MUST NOT land before orient.
- IMPORTS/CYCLES/STATS-LIVEGRAPH-DEFAULT-FASTPATH-1 + QUERY-MIGRATION-1 + LIVEGRAPH-INTEGRATION-1B — the
  cert-gated fastpath, the SQLite-free fingerprint, the labelled SQLite fallback, and the migrated
  `callers`/`callees`/`live_import_view`/`module_import_cycles` LiveGraph surfaces this slice reuses for
  explain's LG-first leaves.

## Spec-first note (read first)
```text
This is a SPECIFICATION. Per the repo evidence law (CLAUDE.md §Evidence Law), every claim is labelled
OBSERVED or INFERRED.
  OBSERVED [first-hand, this turn] = reads I performed this turn, with file:line:
      rust/crates/agent/src/explain/mod.rs (run_explain + explain_symbol + explain_file + explain_path +
        build_no_match + the ambiguous inline builder + build_trust_signal + build_gate_signal +
        group_by_module — read end-to-end, the whole file 1-889)
      rust/crates/agent/src/dto/signal.rs:256-435 (SignalCode enum incl. the 11 Explain codes :284-294;
        as_str :319-329; tier_priority Explain 0-10 :370-380; descriptor Explain→(Explain,Low) :414-424);
        the GENERIC Signal record :947-959 + its hand-written Serialize impl :961-987 (7 base fields ALWAYS;
        scope ONLY when !is_direct; freshness ONLY when Some); SignalScope :147-169; build :1042-1060 (rank 0,
        scope Direct, freshness None defaults); with_module_context :1030-1033; with_freshness :1066-1069;
        + :1379-1538 (the 11 explain_* constructors, all SourceRef::ExplainPipeline, constructor-derived summary)
      rust/crates/daemon-runtime/src/dispatch.rs:2734-2819 (handle_explain — target/budget parse, run_explain
        call, display_name inject :2787, AND the trust-overlay injection :2800-2816 — the load-bearing find)
      rust/crates/rgr/src/presentation/explain.rs:1-671 (ExplainResponse struct :64-81 — NO trust field;
        render_human :118-155; render_target :157-188; the 10 section renderers :246-518; the hides-internal
        and deserialize tests)
      rust/crates/rgr/src/commands/orient.rs:334-496 (run_explain_cmd — arg parse incl. --budget, cwd/
        canonicalize, daemon connect, the BOTH-arms ExitCode::SUCCESS success path, the error→exit-code map)
  OBSERVED [via contract / precedent, first-hand THERE] = facts the ratified contract or the orient/check
      slices read first-hand and cited with file:line; reused here without re-reading (e.g. agent_impl.rs
      concrete SQL behind the AgentStorageRead port; the AnswerEnvelope axis vocabulary in
      repo-graph-trust-model; the LiveGraph surface offsets livegraph/src/lib.rs:443/560/662/1264/1574; the
      livegraph_feed.rs cert ladder + FallbackReason; the storage-architecture-v2 Tier model). Labelled inline.
  INFERRED = my design judgment over those OBSERVED facts (the envelope wiring, the per-leaf provenance
      mapping, the safe-fallback rules, the validation plan), grounded in the ratified contract.
Spine claims I PERSONALLY verified this turn are marked [OBSERVED, first-hand].

NO live `rmap` graph orientation was run: the daemon socket is absent. [EXECUTED this turn: `rmap explain src`
-> "error: daemon connection failed: socket does not exist:
/Users/apple/Library/Application Support/repo-graph/daemon.sock".] A spec-only slice does not start the
daemon or run the index/refresh sequence (that mutates state). Orientation was grounded in first-hand source
reads — the stronger evidence basis for a contract about code structure. The socket-absent result is itself
recorded below as explain's transport-level degradation path (§4 TRANSPORT-LEVEL DEGRADATION), identical to
orient's and check's.
```

## Why now (priority path)
```text
[OBSERVED: docs/slices/coherence-layer-1.md §slice sequence + CURRENT_SLICE.md STATUS banner.]
COHERENCE-LAYER-1 is RATIFIED (operator sign-off 2026-06-08; amended 2026-06-09). Its slice sequence is
ORIENT-LIVEGRAPH-1 -> CHECK-LIVEGRAPH-1 -> EXPLAIN-LIVEGRAPH-1 -> TRUST-LIVEGRAPH-1. ORIENT and CHECK are
DECISION-COMPLETE (operator sign-off 2026-06-09), de-risking the wrapper, the container, the trust_briefing
field, and the multi-source leaf. explain is the contract's NEXT per-command build: "heaviest; CALLERS/
CALLEES/IMPORTS/cycles sections become LG-first leaves by direct reuse of the migrated drilldown answers;
boundary/declarations stay SQLite. Depends: ORIENT-LIVEGRAPH-1 (wrapper proven)" (coherence-layer-1.md
§slice sequence).

[OBSERVED, first-hand: dispatch.rs handle_explain:2734-2819; LiveGraph is wired into dispatch ONLY for
callers/callees/imports/stats/cycles/path/preload/refresh (per coherence-layer-1.md:59-65); the explain
handler body 2734-2819 calls repo_graph_agent::run_explain(&repo_state.storage, ...) (:2770) and consults NO
LiveGraph branch.] => explain today is 100% SQLite + Authority with NO served LiveGraph path. It is one of
the LAST four SQLite-eager defaults and a precondition for SQLITE-RAW-DECOMMISSION-1: the raw `nodes`/`edges`
substrate cannot be decommissioned while explain reads it eagerly on every call for identity, callers,
callees, imports, symbols, files, and cycles.

Higher blast radius than the drilldown defaults, lower-novelty than trust: explain is the HEAVIEST aggregator
(11 signal codes across three focus pipelines) but the MOST DIRECTLY reusable — its structural sections ARE
the migrated callers/callees/imports/cycles answers re-projected. The risk is NOT a new source; it is (a)
explain's NO-LIMITS design, where a section's ABSENCE silently reads as known-zero (the central honesty
hazard under LG-first, RISK-E-C), and (b) the identity section's snapshot-scoped coordinate fields
(RISK-E-D). The wrapper's per-leaf labels are explain's ONLY honest-degradation channel (it has no limits[]).
```

---

## 1. What `rmap explain` returns today (OBSERVED, first-hand)

explain is a multi-section deep-dive aggregator [OBSERVED: explain/mod.rs:1-9 module doc]. `run_explain`
[OBSERVED: mod.rs:45-204] resolves the target to a SYMBOL, FILE, or PATH-area, then runs the matching
pipeline. Every pipeline collects typed `Signal`s, runs `ranking::sort_and_rank` + `truncate_signals`,
derives `confidence`, and builds ONE shared `OrientResult` envelope with `command = EXPLAIN_COMMAND`. The
daemon handler sets `display_name` on the struct [OBSERVED: dispatch.rs:2787] and then, ONLY when degraded,
injects a separate top-level `trust` overlay key into the JSON object AFTER serialization [OBSERVED:
dispatch.rs:2800-2816] — IDENTICAL to orient's overlay (the §intro correction).

Every storage read goes through `storage: &S where S: AgentStorageRead + GateStorageRead` [OBSERVED:
mod.rs:45-46]; the daemon passes the SQLite `StorageConnection` [OBSERVED: dispatch.rs:2770-2776
`repo_graph_agent::run_explain(&repo_state.storage, ...)`]. The in-memory `LiveGraph` on `RepoState` is NOT
consulted by explain. Hence: **every signal below is SQLite or Authority today; LiveGraph contribution = NONE.**

### 1a. The ELEVEN signal codes (OBSERVED, first-hand: signal.rs:284-294, :414-424, :370-380)

explain owns 11 signal codes; ALL carry category `Explain` + severity `Low` [OBSERVED descriptor :414-424],
ALL use `SourceRef::ExplainPipeline` [OBSERVED constructors :1381-1538], and they render in a FIXED section
order by `tier_priority` 0..10 [OBSERVED :370-380]:

| # | Code | tier_priority | Constructor (signal.rs) | Evidence struct |
|---|---|---|---|---|
| 0 | `EXPLAIN_IDENTITY` | 0 | explain_identity:1381 | ExplainIdentityEvidence |
| 1 | `EXPLAIN_CALLERS` | 1 | explain_callers:1391 | ExplainCallersEvidence (count, top_modules, items) |
| 2 | `EXPLAIN_CALLEES` | 2 | explain_callees:1405 | ExplainCalleesEvidence (count, top_modules, items) |
| 3 | `EXPLAIN_IMPORTS` | 3 | explain_imports:1419 | ExplainImportsEvidence (count, items) |
| 4 | `EXPLAIN_SYMBOLS` | 4 | explain_symbols:1433 | ExplainSymbolsEvidence (count, items) |
| 5 | `EXPLAIN_FILES` | 5 | explain_files:1447 | ExplainFilesEvidence (count, items) |
| 6 | `EXPLAIN_CYCLES` | 6 | explain_cycles:1461 | ExplainCyclesEvidence (count, items) |
| 7 | `EXPLAIN_BOUNDARY` | 7 | explain_boundary:1475 | ExplainBoundaryEvidence (violation_count, items) |
| 8 | `EXPLAIN_GATE` | 8 | explain_gate:1493 | ExplainGateEvidence (outcome, obligation_count, items) |
| 9 | `EXPLAIN_TRUST` | 9 | explain_trust:1512 | ExplainTrustEvidence (call_resolution_rate, reliability, enrichment) |
| 10 | `EXPLAIN_MEASUREMENTS` | 10 | explain_measurements:1526 | ExplainMeasurementsEvidence (items) |

No focus emits all 11. Each of the three pipelines emits a DIFFERENT subset; §1a-sym/§1a-file/§1a-path give
the per-focus truth and §1c the matrix.

### 1a-shape. The GENERIC serialized `Signal` envelope — the per-signal fields that wrap every code (OBSERVED, first-hand: signal.rs:947-987, :1042-1060, :1381-1538)

Each of the 11 codes above is carried inside the SAME generic `Signal` record [OBSERVED struct fields
signal.rs:947-959; hand-written `Serialize` :961-987]. The serialized object is SEVEN base fields ALWAYS
(`code`/`rank`/`severity`/`category`/`summary`/`evidence`/`source`, :972-978) plus TWO conditional fields
(`scope`, `freshness`). This is explain's COMPLETE per-signal wire shape; under the wrapper each `Signal`
becomes a leaf `CoherenceEnvelope<Signal>` and these inner fields stay PRISTINE (un-widened, contract D1) —
the coherence provenance/trust/freshness ride the wrapper SIBLINGS, NOT these inner fields (§3a). This closes
the "complete current outputs" criterion at the per-signal granularity (the §1a table gives the code/evidence
axis; this gives the generic-field axis).

| Field | Serialized? (OBSERVED signal.rs) | How explain sets it (OBSERVED) | Source posture |
|---|---|---|---|
| `code` | ALWAYS (:972) | the SignalCode discriminator — one of the 11 (§1a) | static per constructor |
| `rank` | ALWAYS (:973) | `0` at construction (build :1051); OVERWRITTEN by the ranking pass `set_rank` (:1024) after collection (`sort_and_rank`, mod.rs:428/550/716) | DERIVED ordering — NO source-of-truth posture; pristine inner field |
| `severity` | ALWAYS (:974) | `Low` for ALL 11 (descriptor :414-424) | static |
| `category` | ALWAYS (:975) | `Explain` for ALL 11 (descriptor :414-424) | static |
| `summary` | ALWAYS (:976) | a human one-liner DERIVED by the per-code constructor from the evidence (e.g. `explain_callers` → `"{count} direct caller{s}."` :1392; `explain_identity` → `"Identity: {target_kind} target."` :1382) | DERIVED — FOLLOWS the evidence it summarizes (LG-first for the LG-first leaves, SQLite/Authority otherwise); pristine inner field |
| `evidence` | ALWAYS (:977) | the per-code typed struct (§1a) | THE per-signal source map (§2); pristine/un-widened (D1) |
| `source` | ALWAYS (:978) | `SourceRef::ExplainPipeline` for ALL 11 (:1387 etc.) | LEGACY pipeline-identity SourceRef — coarse "which aggregator produced this", DISTINCT from the coherence `Provenance.source` axis {livegraph\|sqlite\|filesystem\|declaration}. Stays `ExplainPipeline` (pristine inner field); the coherence source rides the wrapper sibling (§3a). DO NOT conflate the two. |
| `scope` | ONLY when `!is_direct()` (:979-981) — ABSENT for `Direct`, `"module_context"` for ModuleContext (SignalScope serialize :162-169) | `Direct` by default (build :1057); `ModuleContext` via `with_module_context()` (:1030) — the symbol-focus inherited CYCLES/BOUNDARY/GATE only (mod.rs:354/393/883) | DERIVED from emission context, source-agnostic |
| `freshness` | ONLY when `Some` (:982-984) | ABSENT for ALL 11 explain signals — `build` defaults `None` (:1058) and NO `explain_*` constructor calls `with_freshness` (:1066; constructors :1381-1538) | inner `Signal.freshness` = the RISK-G/R1 reconciliation target; the OUTER leaf FreshnessState is authoritative (§3c R1). Vacuous for explain today. |

LOAD-BEARING (the §2/§3 boundary): the inner `Signal.source` (`ExplainPipeline`) is the legacy aggregator
tag, NOT the coherence provenance. The coherence per-leaf source posture (LG-first vs SQLite-first vs
Authority, incl. the multi-source leaves) is §2 + the wrapper sibling `provenance.source` (§3a); the two must
not be conflated. `rank`/`scope` are derived ordering/context fields with no source-of-truth posture;
`summary` follows its evidence; `freshness` is inner-`None` for every explain signal (the outer leaf
FreshnessState rules). [OBSERVED, first-hand: signal.rs:947-987 / :1042-1060 / :1381-1538.]

### 1a-sym. Symbol-focus signals (OBSERVED, first-hand: explain_symbol mod.rs:253-456) — the focus that adds callers/callees

The SYMBOL pipeline is reached when the target resolves to a symbol stable key (or a unique symbol name)
with a non-`None` symbol context [OBSERVED mod.rs:105-121, :149-169]. It emits identity + the callgraph pair
+ (only if the symbol has an owning module) the inherited module-context signals + trust:

| Emitted (signal code) | Condition | Storage read (OBSERVED mod.rs:line) | Source today | Layer |
|---|---|---|---|---|
| `EXPLAIN_IDENTITY` | ALWAYS | `get_symbol_context` (mod.rs:107/155; pushed :269) | SQLite nodes/files (symbol context) | 0-1 |
| `EXPLAIN_CALLERS` | ALWAYS — **"0 callers is meaningful positive info"** (mod.rs:283-285) | `find_symbol_callers` (mod.rs:286; pushed :299) | SQLite edges/nodes (caller rows carry `module_path`) | 1 |
| `EXPLAIN_CALLEES` | ALWAYS — same reasoning (mod.rs:308-309) | `find_symbol_callees` (mod.rs:310; pushed :323) | SQLite edges/nodes (callee rows carry `module_path`) | 1 |
| `EXPLAIN_CYCLES` (`.with_module_context()`) | `context.module_path` Some AND cycles non-empty (mod.rs:334/337) | `find_cycles_involving_module` (mod.rs:336; pushed :347) | SQLite nodes/edges (module-scoped) | 1 |
| `EXPLAIN_BOUNDARY` (`.with_module_context()`) | `context.module_path` Some AND total>0 (mod.rs:364/384) | `get_active_boundary_declarations` (mod.rs:359) filtered by `source_module == module_path` (:362) + `find_imports_between_paths` (:368) | SQLite `declarations` (**Authority**) + edges | 4 + 1 |
| `EXPLAIN_GATE` (`.with_module_context()`) | `context.module_path` Some AND matching obligations total>0 | `build_gate_signal(module_context=true)` (mod.rs:399; `get_active_requirements`:798 + `assemble_from_requirements`:844) | SQLite `declarations` (**Authority**) | 4 |
| `EXPLAIN_TRUST` | ALWAYS | `build_trust_signal(&trust)` (mod.rs:412); `trust` = `get_trust_summary` (:333) | SQLite trust-core (v1) | 1 |
| `EXPLAIN_MEASUREMENTS` | `measurement_items` non-empty -> **NEVER today** (`Vec::new()` mod.rs:418) | (none) | DORMANT (see §1d) | 1 |

Note (symbol): the three inherited signals (`EXPLAIN_CYCLES`/`EXPLAIN_BOUNDARY`/`EXPLAIN_GATE`) are emitted
ONLY inside the `if let Some(ref module_path) = context.module_path` block (mod.rs:334-409) and carry
`SignalScope::ModuleContext` via `.with_module_context()` (mod.rs:354/393/883). A symbol with NO owning
module emits NEITHER — and explain emits NO limit to say so (§1d).

### 1a-file. File-focus signals (OBSERVED, first-hand: explain_file mod.rs:460-578)

The FILE pipeline is reached for an exact file match or a File stable key [OBSERVED mod.rs:78-88, :122-133].
It is the narrowest structural set — identity + imports + the in-file symbol list + trust. It does NOT run
callers/callees, cycles, boundary, or gate. `now` is unused here (mod.rs:469 `let _ = now;`).

| Emitted (signal code) | Condition | Storage read (OBSERVED mod.rs:line) | Source today | Layer |
|---|---|---|---|---|
| `EXPLAIN_IDENTITY` | ALWAYS (kind="file"; carries `language` + `symbol_count`) | `compute_file_summary` (mod.rs:476; pushed :477) | SQLite nodes/files | 0-1 |
| `EXPLAIN_IMPORTS` | imports non-empty (mod.rs:493) | `find_file_imports` (mod.rs:492; pushed :502) | SQLite edges (file imports) | 1 |
| `EXPLAIN_SYMBOLS` | symbols non-empty (mod.rs:514) | `list_symbols_in_file` (mod.rs:512; pushed :525) | SQLite nodes | 1 |
| `EXPLAIN_TRUST` | ALWAYS | `build_trust_signal(&trust)` (mod.rs:534); `trust` = `get_trust_summary` (:511) | SQLite trust-core (v1) | 1 |
| `EXPLAIN_MEASUREMENTS` | DORMANT (`Vec::new()` mod.rs:540) | (none) | DORMANT | 1 |
| (`EXPLAIN_CALLERS`/`CALLEES`/`CYCLES`/`BOUNDARY`/`GATE`) | **NOT emitted at file scope** | (none) | n/a — intentionally omitted | — |

### 1a-path. Path-area (subtree) focus signals (OBSERVED, first-hand: explain_path mod.rs:582-744)

The PATH pipeline is reached for a path prefix / MODULE stable key [OBSERVED mod.rs:90-100, :134-146]. It
emits identity + the file listing + path-scoped cycles/boundary/gate + trust. Its cycles/boundary are
**NOT** `ModuleContext` (no `.with_module_context()`); they are the path-scoped variants.

| Emitted (signal code) | Condition | Storage read (OBSERVED mod.rs:line) | Source today | Layer |
|---|---|---|---|---|
| `EXPLAIN_IDENTITY` | ALWAYS (kind="path"; carries `file_count` + `symbol_count`) | `compute_path_summary` (mod.rs:597; pushed :598) | SQLite nodes/files | 0-1 |
| `EXPLAIN_FILES` | files non-empty (mod.rs:614) | `list_files_in_path` (mod.rs:613; pushed :625) | SQLite files | 1 |
| `EXPLAIN_CYCLES` (NO module context) | cycles non-empty (mod.rs:636) | `find_cycles_involving_path` (mod.rs:635; pushed :646) | SQLite nodes/edges (path-scoped) | 1 |
| `EXPLAIN_BOUNDARY` (NO module context) | total>0 (mod.rs:676) | `find_boundary_declarations_in_path` (mod.rs:655) + `find_imports_between_paths` (:660) | SQLite `declarations` (**Authority**) + edges | 4 + 1 |
| `EXPLAIN_GATE` (NO module context) | matching obligations total>0 | `build_gate_signal(module_context=false)` (mod.rs:688; prefix match `t == target || t.starts_with("{target}/")`, :819) | SQLite `declarations` (**Authority**) | 4 |
| `EXPLAIN_TRUST` | ALWAYS | `build_trust_signal(&trust)` (mod.rs:700); `trust` = `get_trust_summary` (:634) | SQLite trust-core (v1) | 1 |
| `EXPLAIN_MEASUREMENTS` | DORMANT (`Vec::new()` mod.rs:706) | (none) | DORMANT | 1 |
| (`EXPLAIN_CALLERS`/`CALLEES`/`IMPORTS`/`SYMBOLS`) | **NOT emitted at path scope** | (none) | n/a — intentionally omitted | — |

### 1b. Envelope-level fields (OBSERVED, first-hand)

All five terminals (the 3 pipelines + ambiguous + no_match) build the shared `OrientResult` [OBSERVED
mod.rs:180-200/228-248/436-455/558-577/724-743]:

| Field | Source today (OBSERVED mod.rs:line) | Source class |
|---|---|---|
| `schema` / `command` | `ORIENT_SCHEMA` / `EXPLAIN_COMMAND` constants (:437-438/559-560/725-726) | static |
| `repo` (the repo NAME, not the uid) | `repo.name` (:439 via :81, :561 via :124, :727 via :96; ambiguous :183; no_match :231). Looked up by `get_repo(repo_uid)` (:59) but the serialized value is `name` | SQLite `repos` |
| `display_name` | `None` from the use case (:440/562/728/184/232); daemon injects `Some(display_name)` (dispatch.rs:2787) | daemon operational metadata |
| `snapshot` | `snapshot.snapshot_uid` (`get_latest_snapshot`:66) | SQLite `snapshots` |
| `focus` | `Focus::symbol`:434 / `Focus::file`:556 / `Focus::path_area`:722 / `Focus::ambiguous`:186 / `Focus::no_match`:234 | derived |
| `confidence` | 3 pipelines: `derive_repo_confidence(&trust, stale)` (:432/554/720; confidence.rs:43). **AMBIGUOUS + NO-MATCH: STATIC `Confidence::High`** (:187/235), `derive_repo_confidence` NOT called | SQLite trust-core (pipelines); static (ambiguous/no-match) |
| `documentation` | **`None` ALWAYS** (:444/566/732/188/236) — explain builds NO documentation section (UNLIKE orient) | static `None` |
| `signals[]` (+ `signals_truncated`/`signals_omitted_count`) | aggregator push + `sort_and_rank` + `truncate_signals` (:428-429/550-551/716-717); flags = `sig_tx.truncated.then_some(...)` (:446-447/568-569/734-735); `None` in ambiguous/no-match (:190-191/238-239) | derived |
| `limits[]` (+ truncation flags) | **`Vec::new()` ALWAYS / `None` ALWAYS** (:448-450/570-572/736-738/192-194/240-242) — explain emits NO limits (UNLIKE orient/check) | static empty |
| `next[]` (+ truncation flags) | **`Vec::new()` ALWAYS / `None` ALWAYS** (:451-453/573-575/739-741/195-197/243-245) — explain emits NO next-actions | static empty |
| `truncated` (top-level bool) | 3 pipelines: `sig_tx.truncated` (:454/576/742); ambiguous + no-match: `false` (:198/246) | derived |
| daemon `trust` overlay key — `TrustOverlaySummary` (NOT an OrientResult field; post-serialize JSON injection) | `compute_trust_overlay_for_snapshot(..., "CALLS+IMPORTS")` (dispatch.rs:2802), inserted iff `has_degradation() \|\| !caveats.is_empty()` (:2808-2811) | SQLite trust-core; D-EXPLAIN-TRUST-BRIEFING: renamed to `trust_briefing`, lifted onto the struct, reusing orient O2 |

**AMBIGUOUS / NO-MATCH envelope shape (OBSERVED, first-hand — the zero-signal terminals).** The ambiguous
inline builder (mod.rs:180-200) and `build_no_match` (mod.rs:222-248) do NOT run any pipeline. They emit a
VALID result with `confidence: Confidence::High` (STATIC, :187/235), `documentation: None`,
`signals/limits/next: Vec::new()`, all truncation flags `None`, `truncated: false`. The only populated fields
are operational identity (`repo` NAME, `snapshot` uid) and `focus` (ambiguous candidate list / unmatched
focus string). The static `High` is certainty in the RESOLUTION OUTCOME, NOT a structural-completeness claim
— load-bearing for the wrapper's zero-leaf root (D-EXPLAIN-ZEROSIGNAL; §3b; mirrors orient D-ORIENT-4).

### 1c. Focus-coverage matrix (OBSERVED, first-hand across explain_symbol / explain_file / explain_path)

The three pipelines emit DIFFERENT signal sets — they are NOT one pipeline with a filter. `cond.` = emitted
only when the read is non-empty; `ModuleContext` = inherited owning-module variant; `always*` = always
emitted but `EXPLAIN_MEASUREMENTS` is dormant (never reached because `measurement_items` is hard-coded empty).

| Signal code | symbol | file | path |
|---|---|---|---|
| `EXPLAIN_IDENTITY` | yes | yes | yes |
| `EXPLAIN_CALLERS` | **yes (always; 0 meaningful)** | no | no |
| `EXPLAIN_CALLEES` | **yes (always; 0 meaningful)** | no | no |
| `EXPLAIN_IMPORTS` | no | cond. | no |
| `EXPLAIN_SYMBOLS` | no | cond. | no |
| `EXPLAIN_FILES` | no | no | cond. |
| `EXPLAIN_CYCLES` | cond. (ModuleContext) | no | cond. (path-scoped) |
| `EXPLAIN_BOUNDARY` | cond. (ModuleContext) | no | cond. (path-scoped) |
| `EXPLAIN_GATE` | cond. (ModuleContext) | no | cond. (path-scoped) |
| `EXPLAIN_TRUST` | yes | yes | yes |
| `EXPLAIN_MEASUREMENTS` | dormant (never) | dormant (never) | dormant (never) |

CRITICAL READING (the explain-specific honesty seam): `EXPLAIN_CALLERS`/`EXPLAIN_CALLEES` are the ONLY
"always" structural signals — their `0` is a meaningful KNOWN-ZERO. Every other structural section
(`IMPORTS`/`SYMBOLS`/`FILES`/`CYCLES`/`BOUNDARY`/`GATE`) is CONDITIONAL: ABSENT ⇒ the read returned empty
⇒ known-zero TODAY (because every read is a synchronous SQLite query that always returns an answer). explain
emits NO limit to distinguish "known-zero" from "unknown". Under LG-first, a non-resident / degraded
LiveGraph read could make a section absent — and absence MUST NOT silently read as known-zero (RISK-E-C).

### 1d. What `rmap explain` does NOT emit (the negative space — load-bearing for completeness)

ALL verified first-hand; this pins the OPPOSITE risk (falsely attributing surfaces to explain) AND surfaces
the load-bearing trust-overlay correction:

- **NO limits[] — ever.** Every pipeline sets `limits: Vec::new()` (mod.rs:448/570/736). explain has NO
  labelled-limit channel for known-zero-vs-unknown (CONTRAST orient's COMPLEXITY_UNAVAILABLE /
  MODULE_DATA_UNAVAILABLE / GATE_NOT_CONFIGURED, and check's none-but-conditions). Gate-not-configured /
  no-matching-obligation is a SILENT OMISSION: `build_gate_signal` returns `Ok(())` WITHOUT pushing a signal
  or a limit (mod.rs:802-803/840-841/856-857). This makes the wrapper's per-leaf labels explain's SOLE
  honest-degradation channel (§4, RISK-E-C). [OBSERVED, first-hand.]
- **NO documentation section — ever.** `documentation: None` in every terminal (mod.rs:444/566/732). explain
  has NO filesystem source (CONTRAST orient's `get_doc_inventory` FS scan). [OBSERVED, first-hand.]
- **NO next-actions — ever.** `next: Vec::new()` everywhere. [OBSERVED, first-hand.]
- **explain DOES inject a daemon trust overlay (the CORRECTION).** `handle_explain` injects a post-serialize
  `"trust"` key when `has_degradation() || !caveats.is_empty()` (dispatch.rs:2800-2816), graph_basis
  `"CALLS+IMPORTS"` (:2806) — IDENTICAL to orient. ORIENT-LIVEGRAPH-1 incidentally claimed "check/explain
  produce no overlay" (orient-livegraph-1.md:539, W3 :762); that is WRONG for explain. explain is the SECOND
  command to POPULATE `trust_briefing` (D-EXPLAIN-TRUST-BRIEFING). [OBSERVED, first-hand: dispatch.rs:2800-2816.]
- **The human renderer does NOT read the trust overlay.** `ExplainResponse` has NO `trust` field
  (presentation/explain.rs:64-81; CONTRAST orient's `OrientResponse.trust`). So the overlay is JSON-only in
  explain today: injected into the envelope, ignored by the human render. Under the wrapper, `trust_briefing`
  stays JSON-only for explain unless the implementation CHOOSES to render it (a render-only enhancement, out
  of scope). [OBSERVED, first-hand.]
- **NO non-trivial exit code.** `run_explain_cmd` returns `ExitCode::SUCCESS` in BOTH success arms
  (orient.rs:457/469); it derives NO exit code from any signal (CONTRAST check's 0/1/2). So explain has NO
  check-style exit-code remap obligation (§1f, §3e, D-EXPLAIN-CLI). [OBSERVED, first-hand.]
- **NO check-style verdict, NO orient-style measurement/module/dead-code signals.** explain's structural
  surface is exactly the 11 codes in §1a.

Net: **explain's complete daemon/API output = the shared OrientResult envelope (EXPLAIN_COMMAND) + daemon
display_name + the degraded-only daemon trust overlay; signals ∈ the focus-specific subset of the 11 codes;
limits/next/documentation always empty/None.** The downstream human-render surface is §1e; the CLI
process-wrapper surface is §1f.

### 1e. The human CLI renderer surface (OBSERVED, first-hand: rgr/src/presentation/explain.rs)

The CLI human renderer is `ExplainResponse::render_human` [OBSERVED explain.rs:118-155]. It deserializes a
SUBSET of the envelope into `ExplainResponse` [explain.rs:64-81] and emits exactly these lines — the COMPLETE
human surface explain produces today:

| Rendered surface (what the user sees) | Built from (OBSERVED explain.rs:line) | Source posture today → under this slice |
|---|---|---|
| `Repo: <name>` line | `kv_line("Repo", &self.repo)` (:122) | SQLite `repos` name (§1b). Unchanged. |
| `Target:` / `Kind:` / `File:` (symbol) OR `Target: <path> (<kind>)` + `Language`/`Symbols`/`Files` (file/path) | `render_target` (:157-188) reading the EXPLAIN_IDENTITY evidence via `get_identity_name`/`get_identity_info` (:190-227) | the identity leaf (§1b/§2). Unchanged content; identity leaf gains labels (§3a). |
| `Confidence: <high\|medium\|low>` line | `kv_line("Confidence", &self.confidence)` (:124) | derived (§1b). Under the wrapper, confidence becomes one MEET input (D-EXPLAIN-CONF). |
| Ambiguous: `Ambiguous target - multiple matches found` + candidate bullets | `render_candidates` (:229-244), reached when `!resolved && reason=="ambiguous"` (:128-134) | zero-signal terminal (§1b). Unchanged. |
| Unresolved (no_match): header only, then return (:137-139) | `render_human` early return | zero-signal terminal. Unchanged. |
| Per-section headings + bullets: `Callers (N)`/`Callees (N)` (take 10), `Imports (N)`/`Symbols (N)`/`Files (N)` (take 15), `Import cycles (N)` (take 5, None if 0), `Boundary violations (N)` (take 10, None if 0), `Gate (OUTCOME: N obligations)` (take 10), `Trust` (resolution/reliability/enrichment) | `render_signal_section` dispatch (:246-263) + the 10 renderers (:265-518); `EXPLAIN_IDENTITY` → None (handled in header, :259) | the signal leaves (§2). Section TEXT byte-unchanged; the renderer reads `signals[].evidence` which moves under `value.signals[*].value` (§3e). |
| `[Output truncated. Use --json for full results.]` | `self.truncated` (:150-151) | derived. Unchanged. |

NOT rendered today (deserialized but suppressed, OBSERVED first-hand):
- `snapshot`: `#[allow(dead_code)]` (explain.rs:73-74); deserialized, NEVER printed (test `render_hides_internal_fields` :639-644).
- The `ExplainResponse` struct has **NO `trust` field at all** (explain.rs:64-81) — first-hand confirmation
  that explain's human render IGNORES the daemon trust overlay (CONTRAST orient's `OrientResponse.trust`).
  So `trust_briefing` is JSON-only for explain (§1d, D-EXPLAIN-TRUST-BRIEFING).

CONSEQUENCE FOR THIS SLICE (INFERRED, grounded in the OBSERVED renderer): the renderer reads `signals[].code`
+ `signals[].evidence` (explain.rs:107-112, :246-263); under the wrapper those move under
`value.signals[*].value`. The section TEXT stays byte-identical (P1). The renderer does NOT read the trust
overlay today and need not start (the briefing stays JSON-only unless surfacing is chosen — render-only,
§5 W4). No other human-surface line changes.

### 1f. The CLI process-wrapper surface (`run_explain_cmd`) — stdout/stderr + exit code + `--budget` (OBSERVED, first-hand: rgr/src/commands/orient.rs:334-496)

`rmap explain` is also a PROCESS: `run_explain_cmd` [OBSERVED orient.rs:340-496] parses args, resolves cwd,
connects to the daemon, calls `client.request("explain", ...)`, and maps the outcome to stdout/stderr + an
exit code. UNLIKE check, explain is NOT CI-facing: it returns `ExitCode::SUCCESS` in BOTH success arms.

| Wrapper output (OBSERVED orient.rs:line) | Channel | Exit | Source posture today → effect of this slice |
|---|---|---|---|
| `--json` sets json_mode; `--budget medium\|large` (default medium); positional `<target>` (:341-414) | — | — | static CLI arg parser. UNIQUE to explain: the `--budget` flag (orient/check have none). Unchanged. |
| `--budget` errors (repeat / missing value / flag-as-value / invalid value) → usage + `error: ...` (:354-413) | stderr | **1** | static arg parser; NO daemon. Unchanged. |
| unknown flag → `error: unknown flag: {flag}` + usage (:375-378); unexpected positional → `error: unexpected argument: {arg}` (:381-384); missing target → `error: missing target argument` (:395) | stderr | **1** | static arg parser; NO daemon. Unchanged. |
| `current_dir()` fails (:419-421); `canonicalize()` fails (:427-429) | stderr | **2** | process env / filesystem (local); NO daemon. Unchanged. |
| `DaemonClient::new()` fails → `error: {e}` (:436-438) | stderr | **2** | transport/client connect (the socket-absent path EXECUTED this turn, §4). Unchanged. |
| `--json` success → `to_string_pretty(&result)` to stdout (:454-457) | stdout | **SUCCESS** | the daemon envelope VERBATIM. Under this slice prints the FULL `CoherenceEnvelope<CoherentOrientResult>`. |
| `--json` serialize error → `error: {e}` (:459-461) | stderr | **2** | local serializer. Unchanged. |
| human success → `from_value::<ExplainResponse>(result)` then `render_human()` to stdout (:466-469) | stdout | **SUCCESS** | renderer projection of the envelope (§1e). Under this slice `ExplainResponse` projects `value` (§3e). |
| human parse/render error → `error: failed to parse explain response: {e}` (:471-473) | stderr | **2** | local deserializer/renderer. Unchanged. |
| `DaemonError{code="RepoNotFound"}` → `error: repo not indexed` + `hint: run 'rmap index .'` (:478-482) | stderr | **2** | daemon / repo registry. Unchanged. |
| `DaemonError{code,message}` (other) → `error: {code}: {message}` (:483); catch-all `Err(e)` → `error: {e}` (:488) | stderr | **2** | daemon / transport. Unchanged. |

EXIT-CODE SEMANTICS (OBSERVED orient.rs — preserve verbatim): SUCCESS in both `--json` and human success arms
(:457/469); usage errors = 1; everything else (cwd/canonicalize/connect/serialize/parse/daemon) = 2. There is
NO signal-derived exit code. CONSEQUENCE: this slice has NO check-style exit-code remap — explain's only
wrapper-forced change is the human-render deserialization shift to `value` (§3e, D-EXPLAIN-CLI). A green/red
repo does not change explain's exit code (it is always SUCCESS on a served answer), so there is no silent-CI-
break hazard analogous to check's.

---

## 2. Per-signal source map (the field-level boundary)

Legend (per COHERENCE-LAYER-1 §source map): **LG-first** = LiveGraph-first via the cert-gated fastpath,
SQLite labelled fallback (Q4). **SQLite-first** = SQLite is source of truth (Q5). **Authority** = Tier-A1
`declarations`, permanent SQLite, overlays-never-erases. Layer = Fact Certainty Model layer. There is NO
**FS** row (explain has no documentation section, §1d). This table REFINES the contract's explain row
(coherence-layer-1.md §"Per-command source map" → explain) with first-hand signal-code granularity; no
posture here contradicts the contract.

| Signal / field | Layer | Target posture | LiveGraph surface (when LG-first) | Notes |
|---|---|---|---|---|
| `EXPLAIN_IDENTITY` | 0-1 | **LG-first anchor + SQLite coordinates → D8 MULTI-SOURCE `{livegraph, sqlite}` leaf** | xref `CanonicalKey` + IR symbol-attributes substrate (name/subtype) | Contract explain row = LG-first. NO single migrated symbol-context surface exists; the identity ANCHOR (CanonicalKey/name/subtype/module_path) is LG-derivable from the SAME substrate `stats` uses, but the snapshot-scoped COORDINATE fields (`line_start`, `file_path`, and file/path `language`) are served from SQLite (the LiveGraph does not track live lines). So the SYMBOL-focus served leaf DERIVES its value from BOTH sources → a D8 multi-source `{livegraph, sqlite}` leaf (the sibling of EXPLAIN_BOUNDARY's `{declaration, sqlite}`), trust+freshness = MEET of the two contributors. (The FILE/PATH-focus identity is the SQLite listings/summary case — D-EXPLAIN-LISTINGS — and serves `{sqlite}`.) It COLLAPSES to the `{sqlite}` singleton (fallback_reason set) when the anchor cert is RED/non-resident/non-TS, and always at file/path focus (no symbol anchor). Never Exact with a live line the LiveGraph does not track. **D-EXPLAIN-IDENTITY; RISK-E-D.** |
| `EXPLAIN_CALLERS` (symbol) | 1 | **LG-first** | `callers` -> `AnswerEnvelope<CallersAnswer>` (livegraph lib.rs:443; daemon feed `callers_engine_response`, livegraph_feed.rs:448) | Direct reuse of migrated `callers` (contract explain CALLERS row). Full item list (cap 15/50) + top-3 module group. SQLite fallback = `find_symbol_callers` (carries `module_path`). PARTITION→MODULE mapping: non-resident referencing partitions → labelled SQLite fallback (RISK-E-E). **ALWAYS emitted (0 meaningful) → never Exact-empty under a residency gap** (RISK-E-C). |
| `EXPLAIN_CALLEES` (symbol) | 1 | **LG-first** | `callees` -> `AnswerEnvelope<CalleesAnswer>` (livegraph lib.rs:560; feed `callees_engine_response`, livegraph_feed.rs:544) | Direct reuse of migrated `callees` (contract explain CALLEES row). RESIDENCY ASYMMETRY: summary callees over a non-resident DEFINING partition is a ratified LiveGraph deferral (lib.rs:200-205; CURRENT_SLICE.md:129-131) → labelled SQLite fallback (`find_symbol_callees`) is the COMMON path until it lifts. **ALWAYS emitted → never Exact-empty.** |
| `EXPLAIN_IMPORTS` (file) | 1 | **LG-first** | `live_import_view` (livegraph lib.rs:1574) | Direct reuse of the migrated imports surface (contract explain IMPORTS row). Conditional (non-empty). SQLite fallback = `find_file_imports`. |
| `EXPLAIN_CYCLES` (symbol ModuleContext / path) | 1 | **LG-first** | `module_import_cycles` (livegraph lib.rs:1264) | Direct reuse of migrated cycles (contract explain cycles row). symbol = `find_cycles_involving_module` (module-scoped); path = `find_cycles_involving_path` (path-scoped); both project the LiveGraph module cycles, filtered. Conditional (non-empty). SQLite labelled fallback. |
| `EXPLAIN_SYMBOLS` (file) / `EXPLAIN_FILES` (path) | 1 | **SQLite-first** (listing-coherence); structural COUNTS LG-derivable (deferred) | — | Contract = "file/path summaries, listings | LG-first where structural; else SQLite". The LISTINGS carry snapshot-scoped per-item fields (`line_start`, file `path`, `is_test`, `subtype`) → SQLite; the structural COUNTS (`symbol_count`/`file_count`) are LG-derivable but kept SQLite WITH the listing for coherence (mirror orient D-ORIENT-2 count-anchor; RISK-E-E). **D-EXPLAIN-LISTINGS.** |
| `EXPLAIN_BOUNDARY` (symbol ModuleContext / path) | 4 + 1 | **Authority + SQLite-first** | — | Declaration drives the signal (Authority, overlay-preserves-computed); the structural import-edge half is LG-derivable but kept SQLite-first per contract. Conditional (total>0). **D-EXPLAIN-AUTH.** |
| `EXPLAIN_GATE` (symbol ModuleContext / path) | 4 | **Authority — SQLite-first** | — | Requirement/obligation/waiver evaluation; declarations have no LiveGraph home (contract Q2a). Conditional (matching obligations total>0); NotConfigured / no-match → SILENT omission, NO limit (§1d; RISK-E-C). **D-EXPLAIN-AUTH.** |
| `EXPLAIN_TRUST` | 1 | **SQLite-first** (trust-core v1) | — | Outgoing-extractor reliability; ALWAYS emitted. The hybrid rebase is TRUST-LIVEGRAPH-1, NOT here; if it later changes `get_trust_summary`, explain inherits it through the port with no edit. |
| `EXPLAIN_MEASUREMENTS` | 1 | **DORMANT today**; complexity → LG-first WHEN activated | `value_facts` CyclomaticComplexity (livegraph lib.rs:662) when active | `measurement_items` is hard-coded empty (mod.rs:418/540/706) → never emitted today. Do NOT migrate a dormant surface (mirror orient D-ORIENT-3). When/if activated: complexity → LG-first (same as orient HIGH_COMPLEXITY); coverage/churn/risk → SQLite-first. **D-EXPLAIN-MEASUREMENTS.** |
| `confidence` (envelope) | — | **derived; root MEET (pipelines) / static resolution-only (ambiguous+no-match)** | — | Pipelines: ONE contributor to the root MEET (D-EXPLAIN-CONF); never exceeds legacy `derive_repo_confidence`. Ambiguous/no-match: STATIC `Confidence::High` resolution-only posture (D-EXPLAIN-ZEROSIGNAL). |
| `stale` (freshness driver) | A2 | **SQLite-first** | — | `get_stale_files` non-empty ⇒ snapshot Stale (drives the MEET freshness of SQLite-sourced leaves). |
| `trust_briefing` (was daemon `trust` overlay key) | 1 | **SQLite-first** (trust-core) | — | explain DOES inject the overlay (dispatch.rs:2800-2816); REUSE orient's ratified O2 `trust_briefing` field. explain is the SECOND populator (`Some` when degraded), unlike check (always `None`). JSON-only in explain's human render. **D-EXPLAIN-TRUST-BRIEFING.** |

**Net for explain: FIVE LG-first structural sections — `EXPLAIN_IDENTITY` (a D8 MULTI-SOURCE `{livegraph,
sqlite}` leaf: LG anchor + SQLite coordinates), `EXPLAIN_CALLERS`, `EXPLAIN_CALLEES`, `EXPLAIN_IMPORTS`,
`EXPLAIN_CYCLES` (the latter four single-source `{livegraph}` when served)** — all direct reuses of
already-migrated LiveGraph surfaces (`callers`/`callees`/`live_import_view`/`module_import_cycles`) except the
identity anchor, via the SAME cert-gated fastpath + labelled-fallback mechanism. `EXPLAIN_MEASUREMENTS`' complexity sub-field is
LG-first WHEN it activates (dormant today). Everything else is SQLite-first or Authority. NO new producer, NO
new LiveGraph query is introduced by explain. [INFERRED from the OBSERVED source map + contract Q4 + the
explain row.]

---

## 3. CoherenceEnvelope<T> wiring for explain (INFERRED, grounded in the RATIFIED contract)

Per COHERENCE-LAYER-1 §"The shared coherence answer-envelope" (RATIFIED Option B), the wrapper is applied
COMPOSITIONALLY at two granularities. explain is the THIRD command to instantiate it and the one with the
MOST LG-first leaves (five), reusing orient's container UNCHANGED and POPULATING orient's `trust_briefing`.

### 3a. Leaf — `CoherenceEnvelope<Signal>` (one per emitted signal)

```text
Each `Signal` explain emits is wrapped as a LEAF `CoherenceEnvelope<Signal>` [Signal is the REAL shared DTO;
it is NOT widened — its evidence payload stays pristine]. The leaf's provenance/trust/freshness ride in the
wrapper SIBLING fields and describe THAT signal's source:

  provenance.source per explain signal (a BTreeSet<Source>, D8 — a SINGLETON for a single-source leaf; a SET
    when the leaf's value is DERIVED from facts of more than one source, contract D8):
    - { livegraph }  (singleton) -> EXPLAIN_CALLERS, EXPLAIN_CALLEES, EXPLAIN_IMPORTS, EXPLAIN_CYCLES ...
                        when the cert is GREEN at the current fingerprint AND the needed partitions are
                        resident. Each is a DIRECT reuse of a migrated SINGLE-source LiveGraph answer.
    - { livegraph, sqlite }  (MULTI-SOURCE leaf, D8) -> EXPLAIN_IDENTITY in its SYMBOL-focus served case
                        (where get_symbol_context yields a CanonicalKey anchor; the FILE/PATH-focus identity is
                        the SQLite listings/summary case, D-EXPLAIN-LISTINGS): the identity ANCHOR (CanonicalKey/
                        name/subtype/module_path) is served from LiveGraph (cert GREEN + resident + TS) WHILE the
                        snapshot-scoped COORDINATE fields (line_start/file_path/language) are served from
                        SQLite — ONE leaf whose value is derived from BOTH sources, exactly the D8 multi-source
                        case (the direct sibling of EXPLAIN_BOUNDARY). Its trust + freshness are the MEET of the
                        two contributors (the LiveGraph-anchor posture ∧ the SQLite-coordinate posture) — so the
                        leaf is NEVER Exact on a live line the LiveGraph does not track (D-EXPLAIN-IDENTITY;
                        RISK-E-D). When the anchor cert is RED/stale/missing OR the partition is
                        non-resident/non-TS (and always at file/path focus, which has no symbol anchor), even
                        the anchor cannot be LG-served and the WHOLE leaf collapses to the { sqlite } singleton
                        below (fallback_reason set).
    - { sqlite }     (singleton) -> EXPLAIN_TRUST, EXPLAIN_SYMBOLS, EXPLAIN_FILES, and ANY LG-first leaf that
                        FELL BACK (provenance.fallback_reason set) — including EXPLAIN_IDENTITY when its anchor
                        cannot be LG-served (above), and for EXPLAIN_CALLEES the COMMON path while the
                        summary-callees residency deferral stands (RISK-E-C).
    - { declaration, sqlite } / { declaration }  (MULTI-SOURCE / Authority, D8) -> EXPLAIN_BOUNDARY is the
                        MULTI-SOURCE leaf { declaration, sqlite } (the forbidden-import declaration rule + the
                        SQLite import edges, mod.rs:359-368/655-664); EXPLAIN_GATE is { declaration }
                        (requirement/obligation/waiver). Tier-A1 Authority (contract Q2a); they OVERLAY, never
                        erase (D-EXPLAIN-AUTH).

  trust (TrustPosture) projects the AnswerEnvelope axes verbatim (class/completeness/degradation_reasons/
    contributing_languages). For the LG-first leaves this is the EXISTING AnswerEnvelope the LiveGraph already
    returns — `AnswerEnvelope<CallersAnswer>`/`<CalleesAnswer>` (lib.rs:443/560), the module cycles answer
    (lib.rs:1264), the import view (lib.rs:1574). For SQLite/Authority leaves it is a Fresh/Complete/Exact
    posture for the snapshot (no LiveGraph epoch involved). For the MULTI-SOURCE leaves — EXPLAIN_IDENTITY
    `{livegraph, sqlite}` (served case) and EXPLAIN_BOUNDARY `{declaration, sqlite}` — the leaf trust +
    freshness are the INTERNAL MEET of the contributing postures (monotone — can only lower; never Exact above
    the weakest contributor).

  freshness (FreshnessState) = Fresh | Stale | PrecisionPending | RefreshFailed | Unavailable. LG-first leaves
    inherit the LiveGraph partition freshness; SQLite leaves are snapshot-scoped (Fresh for the current index,
    Stale when get_stale_files is non-empty — OBSERVED mod.rs:431/553/719).

Leaf construction MUST delegate to (or mirror) the AnswerEnvelope smart constructors so the six invariants
hold AT THE LEAF (contract §invariant preservation I1-I6). The multi-source leaves — EXPLAIN_BOUNDARY
({declaration, sqlite}) and EXPLAIN_IDENTITY ({livegraph, sqlite}, served case) — fold their contributor
postures by an internal MEET (monotone — can only lower).
```

### 3b. Root — `CoherenceEnvelope<CoherentOrientResult>` (per command)

```text
The root `value` is the SAME `CoherentOrientResult` container orient ratified (contract D7) = `OrientResult`
with its `signals` slot re-typed `Vec<Signal>` -> `Vec<CoherenceEnvelope<Signal>>`, plus orient's
`trust_briefing: Option<...>` field (D-ORIENT-6 = O2). explain reuses it UNCHANGED and adds NO field of its
own. UNLIKE check (which leaves `trust_briefing = None`), explain POPULATES it `Some(...)` when the snapshot
is degraded (D-EXPLAIN-TRUST-BRIEFING) — explain is the SECOND populator after orient.

  root.value      = CoherentOrientResult {
                      ... ,                                   // all OrientResult fields verbatim (§1b)
                      signals: Vec<CoherenceEnvelope<Signal>>,    // the focus-specific leaf set
                      trust_briefing: Option<TrustOverlaySummary> // D-EXPLAIN-TRUST-BRIEFING; Some only when
                                                                  // degraded (the existing dispatch.rs:2808
                                                                  // gate, preserved). UNLIKE check (always None).
                    }
  root.provenance = { source: SET union of leaf sources (livegraph ∪ sqlite ∪ declaration — NEVER filesystem,
                      explain has no doc scan); basis/missing_partitions/fallback_reason aggregated from leaves }
  root.trust      = the MEET fold of the leaf TrustPostures (contract D3 — greatest-lower-bound, monotone).
  root.freshness  = the MEET fold of the leaf freshness states.
  CoherentOrientResult.confidence is DERIVED from the root MEET and NEVER exceeds the weakest contributor
  (D-EXPLAIN-CONF). The legacy derive_repo_confidence(trust, stale) result becomes ONE input to the MEET.

  ZERO-LEAF ROOT (AMBIGUOUS + NO-MATCH — the zero-signal terminals, mod.rs:180-200/222-248). Same as orient
  D-ORIENT-4: the MEET fold has NO inputs, so the root is NOT served by the empty fold's lattice-TOP (which
  would falsely read Exact/Fresh/Complete over un-analyzed structure). INSTEAD the root carries an explicit
  RESOLUTION-ONLY posture:
    root.provenance.source = { sqlite } operational identity ONLY (repo + snapshot; NO structural source).
    root.confidence        = the STATIC Confidence::High preserved verbatim (mod.rs:187/235).
    root.trust             = a resolution-outcome TrustPosture (labelled "resolution: ambiguous|no_match; no
                             structural analysis"), NEVER a structural Exact.
    root.freshness         = Fresh, scoped to the operational snapshot-identity epoch ONLY.
  `value.trust_briefing` follows the FOCUS-INDEPENDENT snapshot-degradation gate (dispatch.rs:2800-2808: it
  fetches get_snapshot(result.snapshot) then computes the overlay), so a degraded-snapshot ambiguous/no_match
  MAY carry it — orthogonal to the zero-leaf root posture, exactly as orient D-ORIENT-4 specifies. [OBSERVED
  first-hand: dispatch.rs:2801 reads result.snapshot, which the ambiguous/no_match builders DO populate,
  mod.rs:185/233.]

  ENVELOPE limits[]: explain emits NO limits today (§1d). Under the wrapper it MAY gain the contract's
  provenance-derived codes (LIVEGRAPH_PARTIAL, SQLITE_SNAPSHOT_STALE, AUTHORITY_OVERLAY_APPLIED,
  PRECISION_PENDING, PRODUCER_UNAVAILABLE) so degradation is machine-discoverable at the envelope level — this
  is explain's PRIMARY machine-honesty channel precisely because it has no pre-existing limits (RISK-E-C). It
  is the FIRST coherence command for which the provenance-derived limit codes are net-new (orient retained its
  existing limits; check had none and stayed none — explain has none and GAINS them as the degradation
  channel). [INFERRED, grounded in the contract's envelope-limits spec + explain's no-limits finding.]
```

### 3c. Reconciliation points implied by adopting the wrapper (RECORDED, not re-decided)

```text
These are realization details the ratified wrapper IMPLIES; they belong to COHERENCE-ENVELOPE-1 / this
slice's implementation, NOT new boundary decisions (CLAUDE.md §Decision Autonomy: "choices a ratified
decision already imply -> decide and record"):

  R1 (contract RISK-G / orient D-ORIENT-7 / check RP-2). The `Signal.freshness: Option<FreshnessInfo>` field
     exists on the shared Signal DTO, but explain's constructors do NOT set it (the explain_* builders pass
     only evidence + SourceRef, OBSERVED signal.rs:1381-1538 — no freshness arg). So explain populates NO
     inner FreshnessInfo today; the OUTER leaf FreshnessState is authoritative with no reconciliation tension
     (largely vacuous for explain, like check RP-2). COHERENCE-ENVELOPE-1 owns the single mapping; explain
     consumes it.
  R2. confidence semantics change from f(trust) to MEET (D-EXPLAIN-CONF). A symbol whose callers leaf is
     PrecisionPending (SCIP refresh pending) caps root confidence below the legacy value — intended honest
     degradation. Validation pins monotonicity (§5 E1).
  R3. trust_briefing populate-site (D-EXPLAIN-TRUST-BRIEFING / R4 of orient). The daemon POPULATES
     `trust_briefing` on the struct BEFORE serialization (like display_name, dispatch.rs:2787) instead of the
     post-serialize JSON insert under key `trust` (dispatch.rs:2811). The computation and the degraded-only
     gate are PRESERVED verbatim; only the sink moves. This applies the SAME O2 realization orient specified
     — to explain, the SECOND command that needs it. CROSS-SLICE NOTE: orient's R4 said trust_briefing is
     "populated ONLY by orient"; that is incorrect — COHERENCE-ENVELOPE-1 / the shared container MUST allow
     BOTH orient AND explain to populate it (check leaves it None). Recorded so the support module does not
     hard-code an orient-only populate path.
  R4. The CLI human-render deserialization moves to `value` (D-EXPLAIN-CLI; §3e). No exit-code remap (explain
     is always SUCCESS).
```

### 3d. CLI-wrapper human-render remap under the wrapper (INFERRED, forced by the ratified shape)

```text
The wrapper is daemon-INTERNAL (it re-shapes what handle_explain serializes). run_explain_cmd reads that
serialized shape in ONE place that matters (the human-render deserialization); adopting the wrapper FORCES a
mechanical remap there — NOT a new decision, a direct consequence of the ratified `value`-nesting (D7).

HUMAN-RENDER DESERIALIZATION (OBSERVED orient.rs:466):
  today:  serde_json::from_value::<ExplainResponse>(result)      (ExplainResponse over the bare OrientResult)
  after:  the wrapper's `value` projects into ExplainResponse — it deserializes the CoherentOrientResult
          carried under `value`, reading each signal leaf's inner `.value` for code + evidence (§1e) — OR
          run_explain_cmd unwraps `result["value"]` before from_value. Render CONTENT stays byte-identical
          (the section text, §1e); the absent `trust` field stays absent (explain's renderer never read the
          overlay, §1d).

JSON MODE (OBSERVED orient.rs:454): `--json` prints `result` verbatim → under the wrapper it prints the FULL
  `CoherenceEnvelope<CoherentOrientResult>` (value + provenance + trust + freshness; value.trust_briefing
  present only when degraded). No change needed beyond the daemon emitting the wrapper.

NO EXIT-CODE REMAP. Both success arms return ExitCode::SUCCESS (orient.rs:457/469); there is no
signal-derived exit code to re-extract (CONTRAST check §3e). So explain's CLI remap is the human-render
deserialization ONLY — strictly simpler than check, with NO silent-CI-break hazard. If the daemon emits the
wrapper but run_explain_cmd's `from_value::<ExplainResponse>` is NOT updated, the human render fails to parse
→ `error: failed to parse explain response` + exit 2 (a LOUD failure, not a silent wrong answer). Both must
move in lockstep; a wire-shape fixture (§5 W1) pins it.
```

---

## 4. Degradation / safe-fallback behaviour for explain (honest labelling, no false completeness)

```text
PER-LEAF, INDEPENDENT (contract §safe-fallback, ORIENT/EXPLAIN row): each LG-first leaf degrades INDEPENDENTLY
via the cert ladder.
  precondition met + GREEN cert at the current fingerprint + needed partitions resident -> serve LiveGraph,
    SQLite SKIPPED for that leaf (provenance.source={livegraph}, fallback_reason=null).
  precondition unmet (non-TS / non-resident / stale partition) OR cert RED/stale/missing/build-failed -> that
    leaf's provenance.source FLIPS to {sqlite} with provenance.fallback_reason set (the proven imports/cycles/
    stats/callers/callees ladder). The SQLite answer is the PROVEN PRIMARY; LiveGraph is the accelerant.
  a contributing partition non-resident/non-TS/PrecisionPending -> the leaf is Partial/Stale/PrecisionPending
    with an explicit DegradationReason; NEVER dropped, NEVER marked Exact (forbids contract F1-F4).
  MULTI-SOURCE EXCEPTION (EXPLAIN_IDENTITY, D8). The "SQLite SKIPPED on GREEN" rule above is for the
    SINGLE-source LG-first leaves (CALLERS/CALLEES/IMPORTS/CYCLES). EXPLAIN_IDENTITY is the documented
    multi-source leaf: even on GREEN it serves provenance.source={livegraph, sqlite} (anchor from LiveGraph +
    coordinate fields ALWAYS from SQLite), trust/freshness = MEET — SQLite is NOT skipped for its coordinate
    half. Only when the ANCHOR cert is RED/non-resident/non-TS does the WHOLE leaf collapse to {sqlite} with
    fallback_reason set. EXPLAIN_BOUNDARY is likewise multi-source {declaration, sqlite}. (§3a, D-EXPLAIN-IDENTITY.)

THE EXPLAIN-SPECIFIC CORE — KNOWN-ZERO vs UNKNOWN (RISK-E-C, the central honesty seam). explain emits NO
  limits (§1d) and OMITS conditional sections when their read is empty. Today that is SAFE: every read is a
  synchronous SQLite query that always returns a definite answer, so "section absent" == "known-zero". Under
  LG-first that equivalence BREAKS: a non-resident / degraded LiveGraph read could make a section absent for
  an UNKNOWN reason. The wrapper MUST encode the distinction — and the wrapper's per-leaf trust/provenance is
  explain's ONLY honest channel (no limits to fall back on):
    - EXPLAIN_CALLERS / EXPLAIN_CALLEES are ALWAYS emitted (0 is meaningful). When the LiveGraph cannot answer
      (non-resident partition / callees-residency deferral), the leaf is Partial/Unavailable+reason OR a
      labelled SQLite fallback — NEVER an Exact-empty "0 callers" (contract F3 "Unavailable is not empty").
    - The CONDITIONAL sections (IMPORTS/SYMBOLS/FILES/CYCLES/BOUNDARY/GATE) MUST NOT be silently omitted when
      a contributing LiveGraph read is degraded. A degraded LG-first structural section MUST surface a
      labelled leaf (Partial/Unavailable+reason, OR a SQLite fallback that recovers the true answer) — NOT a
      silent omission that an agent reads as known-zero. Where the SQLite fallback recovers the answer, the
      section is served from SQLite (labelled) and the known-zero/non-empty distinction is preserved by the
      SQLite read itself (the proven primary). Where neither can answer, the section becomes an explicit
      Unavailable leaf (or an envelope-level provenance limit, §3b), never absence-as-known-zero.
  This is WHY explain GAINS the envelope provenance limit codes (§3b): they are the machine-discoverable
  degradation markers explain lacked. [INFERRED from §1d no-limits + contract F3/Q8b + architecture.md Rule 6
  "null=unknown, empty=known-zero".]

IDENTITY COORDINATE FIDELITY (RISK-E-D, D-EXPLAIN-IDENTITY). EXPLAIN_IDENTITY is a D8 MULTI-SOURCE leaf in
  its served case: the anchor (CanonicalKey/name/subtype/module_path) is LG-derivable, but line_start/
  file_path/language are snapshot-scoped coordinates the LiveGraph does not track live (VISION §"Deterministic
  Discovery As Token Reduction": rgr owns identity, the agent owns live source location). So the served leaf
  derives its value from BOTH livegraph (anchor) AND sqlite (coordinates) → provenance.source =
  { livegraph, sqlite } with trust + freshness = the MEET of the two contributors. The implementation MUST NOT
  mint an Exact identity leaf asserting a line_start the LiveGraph cannot reproduce field-exactly: the
  SQLite-coordinate contributor folds into the MEET, so the leaf is never Exact above its snapshot-scoped
  coordinate posture. The cert governs the ANCHOR's field-exactness ONLY; the coordinate half is structurally
  SQLite (never cert-gated). When the anchor cert is RED/stale/missing OR the partition is non-resident/non-TS,
  the anchor cannot be LG-served either and the WHOLE leaf collapses to the { sqlite } singleton
  (fallback_reason set). Until a dedicated symbol-context LiveGraph projection exists (deferred, out of scope),
  that { sqlite } collapse is the EXPECTED path for a non-TS / non-resident target — honest, not a regression
  (cf. orient CALLEES residency).

AUTHORITY OVERLAY NEVER ERASES (D-EXPLAIN-AUTH / contract D5): a waiver suppressing an EXPLAIN_GATE failure,
  or a declaration driving EXPLAIN_BOUNDARY, OVERLAYS the computed structural fact; both computed and
  effective views stay queryable across the LiveGraph/SQLite seam. explain consumes the EFFECTIVE
  (waiver-overlaid) gate outcome (via assemble_from_requirements, mod.rs:844); the COMPUTED verdict remains
  queryable via `rmap gate`. explain must keep labelling EXPLAIN_BOUNDARY/EXPLAIN_GATE source=declaration.

TRUST_BRIEFING IS A DEGRADED-STATE SURFACE, DISJOINT FROM THE AXES (D-EXPLAIN-TRUST-BRIEFING): like orient,
  `value.trust_briefing` is present ONLY when degraded (the existing dispatch.rs:2808 gate, preserved) and is
  the HUMAN caveat prose; it is NOT the machine certainty signal (that is `root.trust: TrustPosture`, always
  present). For explain it is JSON-only (the human renderer ignores it today, §1d) — its presence is a
  machine-readable degradation marker, never a substitute for the per-leaf trust. Absence of trust_briefing
  means "not degraded enough to brief", NOT "trust unknown".

TRANSPORT-LEVEL DEGRADATION (OBSERVED, first-hand, distinct from the envelope's internal seam):
  [EXECUTED this turn: `rmap explain src` with the daemon down -> "error: daemon connection failed: socket
  does not exist: /Users/apple/Library/Application Support/repo-graph/daemon.sock".] When the daemon socket
  is absent the CLI NEVER reaches handle_explain: it returns a CONNECTION ERROR (exit 2) and NO envelope. This
  is honest failure (transport error, not a false-complete answer) and is OUTSIDE the CoherenceEnvelope's
  scope. IMPLICATION FOR VALIDATION: explain's coherence degradation is exercised daemon-side (agent +
  livegraph unit/integration tests with a live RepoState), NOT through a socketless CLI. Identical to orient
  §4 / check §4.

NO FALSE-COMPLETENESS, enumerated against the contract's F-list:
  F1 (Exact over non-resident/non-TS): forbidden — LG-first leaves are Partial+missing_partitions/language.
  F2 (confidence HIGH over stale/pending): forbidden — confidence is MEET-capped (D-EXPLAIN-CONF).
  F3 (empty as known-zero): forbidden — the central explain guard (RISK-E-C); CALLERS/CALLEES never
     Exact-empty under residency gaps; conditional sections never silently omitted when degraded.
  F4 (SCIP-dependent refresh-pending folded to Exact): forbidden — invariant I6; callers/callees/cycles leaves
     are Partial+PrecisionPending under refresh.
  F5 (v1-trust as current-state): N/A here (EXPLAIN_TRUST stays SQLite-first labelled; the rebase is
     TRUST-LIVEGRAPH-1).
  F6 (authority overlay erasing computed fact): forbidden — D-EXPLAIN-AUTH (overlay-preserves-computed).
```

---

## 5. Validation plan (for the eventual implementation)

```text
Off-target first (architecture.md §Off-Target Testability + §Build Order). The wrapper type + MEET fold +
the Provenance set-typed field live in COHERENCE-ENVELOPE-1 (pure, unit-tested there); this slice validates
the EXPLAIN WIRING. explain's existing renderer/use-case tests (presentation/explain.rs:521-671;
agent explain tests) MUST stay green unchanged (the section logic is NOT touched; only the surrounding
envelope gains labels).

PARITY (no discovery loss vs today's SQLite explain):
  P1. With LiveGraph populated + GREEN cert + resident partitions: the LG-first section VALUE payloads —
      EXPLAIN_CALLERS, EXPLAIN_CALLEES, EXPLAIN_IMPORTS, EXPLAIN_CYCLES (the four migrated single-source
      answers) and EXPLAIN_IDENTITY (the multi-source leaf: LG anchor + SQLite coordinates ASSEMBLED into the
      same evidence struct) — are byte-identical to the SQLite-computed equivalents; only the surrounding
      wrapper gains labels (for IDENTITY, the {livegraph, sqlite} provenance + MEET trust/freshness). [Reuse
      the imports/cycles/callers/callees parity precedent.]
  P2. The SQLite-first / Authority signals (EXPLAIN_SYMBOLS, EXPLAIN_FILES, EXPLAIN_BOUNDARY, EXPLAIN_GATE,
      EXPLAIN_TRUST) are value-unchanged vs today's OrientResult.
  P3. Signal ORDERING is unchanged: the fixed tier_priority 0..10 section order (signal.rs:370-380) holds
      post-wrapping (sort_and_rank runs pre-wrap, mod.rs:428/550/716). Budget/truncation (cap 15 medium / 50
      large, mod.rs:26-31) unchanged; limits[]/next[] stay empty; truncation flags unchanged.
  P4. FOCUS PARITY (the §1c matrix is the assertion oracle): symbol / file / path / ambiguous / no_match each
      produce the SAME signal set as today, now wrapped (dispatch unchanged). Per focus, EXACTLY:
        - SYMBOL: EXPLAIN_IDENTITY + EXPLAIN_CALLERS + EXPLAIN_CALLEES + (ModuleContext CYCLES/BOUNDARY/GATE
          iff owning module) + EXPLAIN_TRUST. ASSERT NO IMPORTS/SYMBOLS/FILES; ASSERT MEASUREMENTS absent
          (dormant).
        - FILE: EXPLAIN_IDENTITY + EXPLAIN_IMPORTS (cond.) + EXPLAIN_SYMBOLS (cond.) + EXPLAIN_TRUST. ASSERT
          NO CALLERS/CALLEES/CYCLES/BOUNDARY/GATE/FILES.
        - PATH: EXPLAIN_IDENTITY + EXPLAIN_FILES (cond.) + EXPLAIN_CYCLES (cond., path-scoped, NOT
          ModuleContext) + EXPLAIN_BOUNDARY (cond.) + EXPLAIN_GATE (cond.) + EXPLAIN_TRUST. ASSERT NO
          CALLERS/CALLEES/IMPORTS/SYMBOLS.
        - AMBIGUOUS / NO_MATCH: ZERO signals (the resolution-only root, E1z).
      GUARD: no focus-parity test may expect EXPLAIN_MEASUREMENTS (dormant); the symbol test must pin the
      ModuleContext scope on CYCLES/BOUNDARY/GATE; the path test must pin the NON-ModuleContext scope.
  P5. CALLGRAPH SUMMARY PARITY: EXPLAIN_CALLERS/EXPLAIN_CALLEES evidence (count + top-3 module group +
      per-item stable_key/name/module) is byte-identical whether the rows came from the migrated LiveGraph
      callers/callees answer or from SQLite find_symbol_callers/find_symbol_callees — only the wrapper gains
      labels. The summary projection (group_by_module top-3, mod.rs:289/313/750) is source-agnostic.

DEGRADATION:
  D-V1. GREEN cert + resident -> the LG-first leaves read NO SQLite (panicking-SQLite-closure style);
        fallback_reason=null.
  D-V2. RED/stale/missing cert OR non-TS / non-resident partition -> the LG-first leaf flips to source=sqlite
        with fallback_reason set; the leaf VALUE equals the SQLite answer.
  D-V3. PrecisionPending partition (SCIP refresh pending) -> the LG-first leaf is Partial+PrecisionPending,
        never Exact (invariant I6); root confidence MEET-capped accordingly.
  D-V4. KNOWN-ZERO vs UNKNOWN (the central explain guard, RISK-E-C):
        (a) CALLERS/CALLEES over a NON-resident partition -> the always-emitted leaf is Partial/Unavailable+
            reason OR a labelled SQLite fallback, NEVER Exact-empty "0 callers". Assert: never Exact-empty
            from a residency gap; the served value equals the SQLite-derived answer.
        (b) A CONDITIONAL section (e.g. CYCLES/IMPORTS) whose LiveGraph read is degraded MUST surface a
            labelled leaf or a recovered SQLite answer — assert it is NOT silently omitted (which an agent
            would read as known-zero). Where SQLite recovers a non-empty answer, assert the section IS present
            (source=sqlite, labelled). Where the true answer is empty AND the source is authoritative, assert
            the section is omitted as today (genuine known-zero).
  D-V5. SYMBOL CALLEES residency (the ratified deferral, lib.rs:200-205): with the defining partition
        NON-resident, EXPLAIN_CALLEES falls back to SQLite find_symbol_callees (labelled) — assert this is the
        path taken, the leaf is never Exact-empty, and the value equals the SQLite answer. Resident + GREEN
        cert -> LG-first serves (P5).
  D-V6. IDENTITY multi-source leaf + coordinate fidelity (RISK-E-D, D8): in the served (cert-GREEN + resident
        + TS) case, assert provenance.source = { livegraph, sqlite } (anchor LG + coordinate fields SQLite),
        leaf trust + freshness = the MEET of the two contributors, and the leaf is NOT Exact above its
        snapshot-scoped coordinate posture (never Exact on a line_start the LiveGraph cannot reproduce
        field-exactly). In the anchor-fallback case (cert RED / non-resident / non-TS) assert the leaf
        COLLAPSES to the { sqlite } singleton with fallback_reason set. Sibling assertion to EXPLAIN_BOUNDARY's
        { declaration, sqlite } (E2/E3).
  D-V7. WAIVED gate (effective Pass over a computed Fail) -> EXPLAIN_GATE leaf provenance.source includes
        declaration; assert the computed gate verdict remains queryable via `rmap gate` (D-EXPLAIN-AUTH / D5).
  D-V8. Transport: socket-absent -> connection error, NO envelope, exit 2 (OBSERVED this turn; assert UNCHANGED).

ENVELOPE CORRECTNESS:
  E1. MEET monotonicity (pipelines): coherent root confidence <= legacy derive_repo_confidence on identical
      input; no fold yields an Exact root from a non-Exact leaf.
  E1z. ZERO-SIGNAL ROOT (AMBIGUOUS + NO_MATCH, mod.rs:180-200/222-248): assert the empty-signal terminals do
      NOT take the empty MEET's lattice-TOP. Assert the explicit resolution-only posture (D-EXPLAIN-ZEROSIGNAL):
      confidence = STATIC High preserved; provenance.source = { sqlite } operational identity ONLY; trust is a
      labelled resolution-outcome, NEVER a structural Exact; freshness = Fresh (snapshot identity). Assert
      value.trust_briefing follows the snapshot-degradation gate (focus-independent), so a degraded-snapshot
      ambiguous/no_match MAY carry it. Pin that a zero-signal explain can NEVER serialize a structural-
      completeness Exact.
  E2. Invariants I1-I6 hold at every leaf and survive the fold (Exact requires Fresh+Complete; Partial
      justified; Unavailable carries a reason; Stale!=Fresh; null!=empty; PrecisionPending!=Exact w/o non-SCIP
      basis). The multi-source leaves — EXPLAIN_BOUNDARY {declaration, sqlite} and EXPLAIN_IDENTITY
      {livegraph, sqlite} (served case) — fold their contributor postures by a monotone internal MEET.
  E3. provenance.source is correct per leaf (the BTreeSet<Source>, D8): { livegraph } (callers/callees/imports/
      cycles when served) / { livegraph, sqlite } (EXPLAIN_IDENTITY served/mixed case) / { sqlite } (trust/
      symbols/files + any fallback, incl. identity collapsed) / { declaration, sqlite } (EXPLAIN_BOUNDARY) /
      { declaration } (EXPLAIN_GATE); the root provenance.source is the exact SET union; NEVER includes
      filesystem (explain has no doc scan).
  E4. Authority overlay preserves computed fact (D-EXPLAIN-AUTH): D-V7.
  E5. Envelope limits[] carry the provenance codes when (and only when) the matching degradation occurred —
      the FIRST coherence command where these limit codes are net-new (explain had none); assert a non-degraded
      explain still emits ZERO limits (parity with today).

WIRE SHAPE / RENDERER / FIXTURES:
  W1. WIRE SHAPE: the top-level JSON is `CoherenceEnvelope<CoherentOrientResult>`; `value` carries the
      EXPLAIN_COMMAND envelope with the re-typed signals slot; `root.trust` (TrustPosture) + `root.freshness`
      (FreshnessState) PRESENT. The reused structural VALUE payloads stay byte-identical (P1).
  W2. trust_briefing — DEGRADED: `value.trust_briefing` PRESENT (the former overlay value, byte-identical to
      today's `trust` key) when has_degradation()||!caveats.is_empty(); the OLD top-level `trust` overlay key
      ABSENT (renamed). This is the explain-SPECIFIC contrast with check (check W3 pins trust_briefing ALWAYS
      absent; explain pins it PRESENT-when-degraded). Assert exactly ONE briefing under value.trust_briefing.
  W3. trust_briefing — NOT DEGRADED: `value.trust_briefing` ABSENT (skip_serializing_if), matching today's
      behaviour where the `trust` key is omitted when not degraded; `root.trust` still present.
  W4. RENDERER: the human render reads signal code+evidence from `value.signals[*].value` (§3e). Section TEXT
      byte-identical (§1e). The `trust` field stays absent from ExplainResponse (explain never rendered it);
      assert NO new trust rendering unless surfacing trust_briefing is explicitly chosen (render-only).
  W5. FIXTURES: update JSON-contract fixtures in lockstep with COHERENCE-ENVELOPE-1 — a degraded-explain
      fixture (value.trust_briefing present, root.trust present, no top-level `trust`), a non-degraded fixture
      (no trust_briefing, root.trust present), and a focus-coverage fixture per pipeline. Bump the shared
      schema id only if the contract tests pin the top-level shape (one bump across orient/check/explain).

CLI-WRAPPER (run_explain_cmd; orient.rs:340-496 — §1f/§3e). Driveable off-target with a recorded daemon-
  response fixture (the wrapped envelope).
  CW1. `rmap explain <symbol> --json` over a wrapped fixture -> stdout is the FULL
       `CoherenceEnvelope<CoherentOrientResult>` (value + provenance + trust + freshness) AND exit SUCCESS.
  CW2. `rmap explain <symbol>` (human) over the SAME fixture -> render byte-identical to today (the §1e
       sections) AND exit SUCCESS. Assert the human path reads the WRAPPED signal values (ExplainResponse
       projects `value`, §3e); a fixture pinning the OLD top-level `signals` path is no longer used.
  CW3. usage errors unchanged: unknown flag / unexpected arg / missing target / `--budget` errors -> usage +
       exit 1, NO daemon contacted (OBSERVED orient.rs:354-413/375-399). `--budget large` accepted (cap 50).
  CW4. socket-absent -> stderr daemon-connection error, NO envelope on stdout, exit 2 (= D-V8; EXECUTED this
       turn). RepoNotFound -> `error: repo not indexed` + hint, exit 2. Unchanged by this slice.
  CW5. NO EXIT-CODE REMAP (the explain-vs-check contrast): assert both `--json` and human success arms return
       exit SUCCESS regardless of the verdict-free signal content; there is NO signal-derived exit code to
       break. (This is the parity guarantee that explain has no check-style silent-CI-break surface.)

LIVE (after off-target green; macOS, ./scripts/dev-install-local.sh):
  L1. `rmap explain <symbol>` on a TS pilot with a populated LiveGraph + resident partition -> CALLERS/CALLEES
      leaves source={livegraph} (GREEN cert, no per-call SQLite for those leaves); the IDENTITY leaf
      source={livegraph, sqlite} (anchor LG + coordinate fields SQLite, the D8 multi-source leaf), trust/
      freshness = MEET; human render unchanged in shape.
  L2. `rmap explain <file>` on the TS pilot -> EXPLAIN_IMPORTS source=livegraph (live_import_view);
      EXPLAIN_SYMBOLS/identity coordinate fields source=sqlite (labelled).
  L3. `rmap explain <path>` -> EXPLAIN_CYCLES source=livegraph (module_import_cycles); EXPLAIN_BOUNDARY/
      EXPLAIN_GATE source=declaration (Authority); EXPLAIN_FILES source=sqlite.
  L4. `rmap explain <symbol>` on a NON-TS repo -> every LG-first leaf falls back to sqlite (labelled); all
      sections intact; human render unchanged in shape.
  L5. Degraded snapshot (stale files) -> `rmap explain <symbol>` -> value.trust_briefing PRESENT (the explain
      contrast with check); root.trust/root.freshness reflect Stale; --json shows the wrapper.
  L6. Re-refresh -> fingerprint change -> cert rebuild -> next explain still serves correctly.
```

---

## 6. Scope boundary

```text
IN SCOPE: `rmap explain` ONLY — all focuses (symbol / file / path / ambiguous / no_match). Wrap explain's
answer in `CoherenceEnvelope<CoherentOrientResult>` (reuse orient's ratified container UNCHANGED); cert-gate
the FIVE LG-first structural sections with labelled SQLite fallback — EXPLAIN_IDENTITY (anchor),
EXPLAIN_CALLERS, EXPLAIN_CALLEES (symbol), EXPLAIN_IMPORTS (file), EXPLAIN_CYCLES (symbol ModuleContext /
path), all DIRECT reuses of the migrated callers/callees/live_import_view/module_import_cycles surfaces; keep
EXPLAIN_SYMBOLS/EXPLAIN_FILES (listing-coherence SQLite), EXPLAIN_BOUNDARY/EXPLAIN_GATE (Authority),
EXPLAIN_TRUST (SQLite trust-core) at their mapped postures; per-leaf provenance + root MEET. POPULATE orient's
ratified `trust_briefing` field `Some(...)` when degraded (D-EXPLAIN-TRUST-BRIEFING — explain is the SECOND
populator; correct orient's "only by orient" assumption). ALSO IN SCOPE (a PARITY obligation forced by the
wrapper, §3e): the run_explain_cmd human-render deserialization MUST be remapped to the `value`-nested shape;
NO exit-code remap (explain is always SUCCESS). GAIN the envelope provenance-derived limit codes as explain's
machine-degradation channel (explain has no pre-existing limits).

OUT OF SCOPE (separate later slices, per the contract slice sequence):
  - ORIENT-LIVEGRAPH-1 + CHECK-LIVEGRAPH-1 (DONE/decision-complete; this slice DEPENDS on orient for the
    wrapper pattern, the container, and the trust_briefing field).
  - TRUST-LIVEGRAPH-1 — the fourth coherence command + the hybrid trust rebase (TRUST-DISPOSITION). If it
    later rebases the trust core, explain's EXPLAIN_TRUST updates THROUGH the existing get_trust_summary port
    with no explain edit.
  - COHERENCE-ENVELOPE-1 — the support module (the wrapper type, the MEET fold, the BUILD of the multi-source
    Provenance set-typed field per D8, the FreshnessInfo reconciliation). This slice DEPENDS on it; not built
    here. CONSTRAINT this slice imposes: the container's trust_briefing populate path MUST permit BOTH orient
    AND explain (not orient-only — R3 cross-slice note).
  - EXPLAIN_MEASUREMENTS activation — the dormant measurement surface is NOT activated here (D-EXPLAIN-
    MEASUREMENTS); when a producer exists, complexity → LG-first, others → SQLite.
  - A dedicated symbol-context LiveGraph projection (so the identity COORDINATE fields could be LG-served) —
    a deferred optimization (D-EXPLAIN-IDENTITY); not built here.
  - SQLITE-RAW-DECOMMISSION-1 — explain still reads SQLite to build certs + serve fallbacks + remains the
    source for SYMBOLS/FILES/BOUNDARY/GATE/TRUST + the identity coordinates; NO table is decommissioned here.
    COHERENCE-READINESS-RECOMPUTE-1 must record explain's retained eager SQLite reads as still load-bearing.

HARD GUARDRAILS (this slice's out-of-scope, mirroring the contract):
  NO source code (spec-first). NO table deletion, NO schema/data migration, NO default flip beyond specifying
  it. NO new producer for measurements/boundary/inferences. NO change to declarations/gate/authority
  semantics. NO change to explain's section logic (mod.rs is untouched — the coherence layer wraps, it does
  not re-aggregate). NO raw nodes/edges decommission. NO non-TS LiveGraph support (non-TS -> SQLite fallback).
  NO edit to docs/ROADMAP.md or CURRENT_SLICE.md. NO live daemon run / index / refresh.
```

---

## Forced decisions — every cell filled

### D-EXPLAIN-1 — LG-first leaf set = {IDENTITY-anchor, CALLERS, CALLEES, IMPORTS, CYCLES} (DECIDED, within contract)
```text
explain's LG-first structural sections are the five the contract's explain source map assigns LG-first:
  - EXPLAIN_IDENTITY (anchor half) — LiveGraph xref CanonicalKey + IR symbol-attributes substrate; the served
    leaf is the D8 MULTI-SOURCE {livegraph, sqlite} leaf (anchor LG + coordinates SQLite, D-EXPLAIN-IDENTITY).
  - EXPLAIN_CALLERS / EXPLAIN_CALLEES (symbol) — migrated callers/callees (lib.rs:443/560).
  - EXPLAIN_IMPORTS (file) — migrated live_import_view (lib.rs:1574).
  - EXPLAIN_CYCLES (symbol ModuleContext / path) — migrated module_import_cycles (lib.rs:1264).
DECIDED, not asked: directly implied by COHERENCE-LAYER-1 Q4 + the explain source map (which the operator
ratified). All four migrated surfaces are direct reuses via the SAME cert-gated fastpath; NO new producer.
EXPLAIN_SYMBOLS/FILES/BOUNDARY/GATE/TRUST are SQLite-first/Authority; EXPLAIN_MEASUREMENTS is dormant.
Recorded.
```

### D-EXPLAIN-IDENTITY — EXPLAIN_IDENTITY = D8 MULTI-SOURCE `{livegraph, sqlite}` leaf (LG anchor + SQLite coordinates) (DECIDED, within contract; NOT an escalation)
```text
QUESTION: The contract's explain row assigns "identity / symbol context" LG-first, but get_symbol_context
reads nodes/files/edges OWNS (a COMPOSITE: an LG-derivable identity anchor + snapshot-scoped coordinate
fields line_start/file_path/language), and there is NO single migrated LiveGraph symbol-context surface (unlike
callers/callees/imports/cycles, which directly reuse migrated answers). How is the coordinate half handled
under the LG-first posture?

| Option | Identity anchor | Coordinate fields (line_start/file_path) | Contract consistency | New surface/producer | Fact-Certainty fit | Verdict |
|---|---|---|---|---|---|---|
| O-A D8 MULTI-SOURCE `{livegraph, sqlite}` leaf: LG anchor + SQLite coordinates (cert governs the anchor; leaf trust/freshness = MEET) | anchor source=livegraph from xref + IR symbol-attributes substrate (the stats substrate) | coordinates ALWAYS source=sqlite (LG never tracks live lines); leaf = `{livegraph, sqlite}`, MEET trust/freshness; collapses to `{sqlite}` when the anchor cert is RED/non-resident/non-TS | CONSISTENT — contract row = LG-first anchor; coordinates honest-SQLite; D8 multi-source leaf (sibling of EXPLAIN_BOUNDARY) | none (reuse stats substrate; SQLite read survives) | clean — anchor current-state, coordinates honest-MEET; never Exact-wrong-line | **DECIDED** |
| O-B keep EXPLAIN_IDENTITY fully SQLite-first | SQLite | SQLite | DIVERGES — contradicts the ratified explain row (identity=LG-first); re-opens a ratified posture | none | weaker — snapshot-bound where the anchor is LG-derivable | NOT CHOSEN — re-opening a ratified decision is forbidden |
| O-C build a new dedicated symbol-context LiveGraph projection now | LG | LG (live line tracking) | over-reaches — a NEW surface beyond "direct reuse of migrated answers" | NEW producer (the contract's D6 forbids new producers) | over-claims live coordinates rgr does not own (VISION) | NOT CHOSEN — new producer out of scope; VISION assigns live coordinates to the agent |

DECIDED: **Option A.** It is the literal realization of the ratified contract's explain row ("identity =
LG-first") under the ratified cert ladder AND the ratified D8 multi-source LEAF provenance: the identity
ANCHOR (CanonicalKey/name/subtype/module_path) is LG-derivable from the SAME IR symbol-attributes substrate
the `stats` fastpath uses (no new producer); the snapshot-scoped COORDINATE fields (line_start, file_path,
language) are served from SQLite, because VISION assigns live source-location to the agent, not to rgr's
snapshot-scoped index (VISION §"Deterministic Discovery As Token Reduction"). The served identity leaf
therefore DERIVES its value from BOTH sources → it is a D8 MULTI-SOURCE `{livegraph, sqlite}` leaf (the direct
sibling of EXPLAIN_BOUNDARY's `{declaration, sqlite}`), with leaf trust + freshness = the MEET of the
LiveGraph-anchor posture and the SQLite-coordinate posture. The MEET makes the honesty automatic: the
SQLite-coordinate contributor caps the leaf, so it is NEVER Exact on a line_start the LiveGraph cannot
reproduce field-exactly. The cert governs the ANCHOR's field-exactness ONLY; the coordinate half is
structurally SQLite (never cert-gated). When the anchor cert is RED/stale/missing OR the partition is
non-resident/non-TS, the anchor cannot be LG-served either and the WHOLE leaf collapses to the `{sqlite}`
singleton (fallback_reason set). Until a dedicated symbol-context projection exists (deferred, out of scope),
that `{sqlite}` collapse is the EXPECTED path for a non-TS / non-resident target — honest,
ratified-mechanism-governed, not a regression.

NOT A NEW ESCALATION (the deliberate distinction from orient's D-ORIENT-SYMBOL-CALLGRAPH). orient escalated
CALLERS/CALLEES because the contract's REPO-focus row had assigned them NO posture (genuinely unsettled). Here
the contract HAS assigned identity LG-first (explain row); re-opening that is forbidden ("Do NOT re-open
ratified coherence decisions"). The ONLY open point is the REALIZATION (coordinate-field handling), and it is
fully determined by ratified mechanisms (the cert ladder's field-exactness + the IR symbol-attributes
substrate + VISION's identity/coordinate doctrine). Per CLAUDE.md §Decision Autonomy ("choices a ratified
decision already imply -> decide and record"), this is decide-and-record + RISK-E-D, not escalation. LEVER FOR
THE REVIEWER (audit-trail honesty): if the operator deems the "no single migrated symbol-context surface" gap
a genuine boundary call (analogous to why CALLERS/CALLEES were escalated), this matrix is the escalation
record; but per the ratified contract I do NOT re-open it. Cheap to unwind: O-C remains addable later as a
dedicated projection without re-judging any posture.
```

### D-EXPLAIN-LISTINGS — EXPLAIN_SYMBOLS / EXPLAIN_FILES / identity counts stay SQLite-first (DECIDED, within contract)
```text
The contract's explain row says "file/path summaries, listings | LG-first where structural; else SQLite". The
LISTINGS (EXPLAIN_SYMBOLS items: name/subtype/line_start; EXPLAIN_FILES items: path/symbol_count/is_test;
identity language/file_count/symbol_count) MIX structural COUNTS (LG-derivable) with snapshot-scoped per-item
coordinate/inventory fields (line_start, path, is_test, language). DECIDED, not asked: keep these leaves
SQLite-first for LISTING-COHERENCE + coordinate fidelity — the structural COUNT sub-fields are LG-derivable
but kept SQLite WITH the listing (a deferred optimization), mirroring orient D-ORIENT-2 (count-anchor
consistency) and avoiding RISK-E (LiveGraph dirname/partition aggregation vs SQLite file/symbol inventory
divergence). This honors the contract's "else SQLite" half. Recorded. Cheap to unwind: the count sub-fields
can be promoted to a LiveGraph cert later without re-judging the listing.
```

### D-EXPLAIN-AUTH — EXPLAIN_BOUNDARY + EXPLAIN_GATE = Authority, overlay-preserves-computed (DECIDED, within contract)
```text
Both read Tier-A1 `declarations` (Authority); EXPLAIN_BOUNDARY is a MULTI-SOURCE leaf ({declaration, sqlite}:
the forbidden-import rule + the SQLite import edges, mod.rs:359-368/655-664), EXPLAIN_GATE is
{declaration}. They OVERLAY the computed structural fact and never erase it; both computed and effective
views stay queryable across the seam (explain consumes the effective gate outcome; `rmap gate` keeps the
computed). DECIDED, not asked: VISION §Agent Priorities #2 + contract D5 + D8 (multi-source leaf) applied to
explain (mirrors orient D-ORIENT-5). NotConfigured/no-matching-obligation → SILENT omission (explain has no
limit) — the wrapper's per-leaf labels are the honest channel (RISK-E-C). Recorded.
```

### D-EXPLAIN-MEASUREMENTS — EXPLAIN_MEASUREMENTS is dormant; no leaf today (DECIDED, within contract)
```text
[OBSERVED first-hand: measurement_items is hard-coded `Vec::new()` (mod.rs:418/540/706); the section is gated
on non-empty, so EXPLAIN_MEASUREMENTS is NEVER emitted today.] DECISION: do NOT migrate a dormant surface
(mirror orient D-ORIENT-3 "do not migrate a withdrawn surface"). explain gains NO measurements leaf. When/if
the section activates (a measurement producer exists), complexity → LG-first via value_facts (same posture as
orient HIGH_COMPLEXITY); coverage/churn/risk → SQLite-first (no producer). Recorded; activation is a later
slice, out of scope.
```

### D-EXPLAIN-TRUST-BRIEFING — explain POPULATES orient's ratified trust_briefing field (DECIDED, reuse of O2; NOT an escalation)
```text
[OBSERVED first-hand: handle_explain injects a degraded-only daemon trust overlay (dispatch.rs:2800-2816),
IDENTICAL to orient — same gate has_degradation()||!caveats.is_empty(), same "CALLS+IMPORTS" basis, same
post-serialize "trust" key.] This CONTRADICTS orient-livegraph-1.md's incidental claim that "check/explain
produce no overlay" (orient:539, W3:762) — which is correct for check but WRONG for explain.

DECIDED, not asked: explain REUSES orient's RATIFIED D-ORIENT-6 = O2 disposition (RETAIN RENAMED). The daemon
populates `trust_briefing: Option<TrustOverlaySummary>` on the shared CoherentOrientResult BEFORE serialize
(replacing the post-serialize `trust` insert), `Some(...)` only when degraded. explain is the SECOND command
to populate it (orient first; check leaves it None per D-CHECK-2). This is NOT a new boundary decision: the
field, its Option+skip_serializing_if shape, and its populate-before-serialize realization are ALL ratified by
O2; explain is in orient's EXACT structural position (both inject the same overlay via the same util). Applying
a ratified decision to the second command that needs it is decide-and-record, not escalation.

EXPLAIN-SPECIFIC NUANCES (recorded):
  (1) explain's human renderer does NOT read the overlay (ExplainResponse has no `trust` field,
      explain.rs:64-81; CONTRAST orient's OrientResponse.trust). So trust_briefing is JSON-only for explain;
      surfacing it in the human render would be a render-only enhancement (out of scope, §5 W4).
  (2) CROSS-SLICE CORRECTION: orient's R4 / §3b assert trust_briefing is "populated ONLY by orient". That is
      false. COHERENCE-ENVELOPE-1 / the shared container MUST allow BOTH orient AND explain to populate it
      (check leaves it None). Recorded so the support module does not hard-code an orient-only path (R3).
  (3) Validation contrast: check pins trust_briefing ALWAYS absent (check W3); explain pins it PRESENT-when-
      degraded (§5 W2). Both are correct against their first-hand handler behaviour.
This RECORD does not re-open O2 — it confirms the shared field works as a shared field should.
```

### D-EXPLAIN-CONF — confidence becomes one contributor to the root MEET (DECIDED, within contract)
```text
explain computes confidence via the SAME derive_repo_confidence orient/check use (mod.rs:432/554/720;
confidence.rs:43), and a static Confidence::High for ambiguous/no_match (mod.rs:187/235). DECIDED (mirrors
orient D-ORIENT-4 / check D-CHECK-3, implied by contract D3 MEET): coherent root confidence is DERIVED from
the root MEET and never exceeds the weakest contributor; the legacy result is ONE MEET input. The
ambiguous/no_match static High is preserved as the resolution-only posture (D-EXPLAIN-ZEROSIGNAL), NOT
recomputed from an empty MEET. Not a boundary decision — a local mechanism implied by the ratified MEET.
Recorded.
```

### D-EXPLAIN-ZEROSIGNAL — ambiguous/no_match take the resolution-only root posture (DECIDED, reuse of orient D-ORIENT-4)
```text
[OBSERVED first-hand: the ambiguous inline builder (mod.rs:180-200) and build_no_match (mod.rs:222-248) emit
ZERO signal leaves and a STATIC Confidence::High (mod.rs:187/235), documentation None, all flags None.] Same
as orient's zero-signal carve-out: the structural MEET fold has NO inputs, so the root is NOT served by the
empty fold's lattice-TOP (which would mint a false structural Exact). INSTEAD: provenance.source = { sqlite }
operational identity only; confidence = the static High preserved; trust = a labelled resolution-outcome
posture, never a structural Exact; freshness = Fresh (snapshot identity epoch). value.trust_briefing follows
the focus-independent snapshot-degradation gate, so a degraded-snapshot ambiguous/no_match MAY carry it.
DECIDED, not asked: a decide-and-record realization of the ratified MEET (the empty-fold edge case), reused
verbatim from orient D-ORIENT-4 — it REMOVES a false-trust risk. Recorded. Validation: E1z.
```

### D-EXPLAIN-CLI — human-render deserialization remap, NO exit-code remap (DECIDED, mechanical)
```text
[OBSERVED first-hand: run_explain_cmd returns ExitCode::SUCCESS in BOTH success arms (orient.rs:457/469); the
human path is from_value::<ExplainResponse>(result) (:466).] DECIDED: adopting the wrapper forces ONE
mechanical CLI remap — the human-render deserialization must project `value` (ExplainResponse reads
value.signals[*].value, OR run_explain_cmd unwraps result["value"] before from_value, §3e). There is NO
exit-code remap (explain derives no signal exit code, UNLIKE check) and NO silent-CI-break hazard (a stale
deserialization fails LOUDLY → exit 2). The --budget medium|large flag + cap (15/50) are preserved. Not a
boundary decision — a direct consequence of the ratified value-nesting (D7). Recorded; validation §5 CW2/CW5.
```

### D-EXPLAIN-SCOPE — scope (DECIDED, recorded)
```text
This slice SPECIFIES explain ONLY (all focuses). NO command implementation here. NO LiveGraph support added
beyond reusing the migrated surfaces. NO new producer (measurements stays dormant; no symbol-context
projection built). NO change to declarations/gate/measurements/trust producers. NO change to explain's section
logic. NO raw nodes/edges decommission (all retained, now honestly labelled). Mirrors contract D6.
```

---

## Risks (explain-specific projections of the contract risks; each the implementation must address)

```text
RISK-E-A — EPOCH SKEW MINTING FALSE FRESHNESS (= contract RISK-A). A LiveGraph structural partition Fresh
  while the SQLite snapshot is a stale index (or vice-versa) -> a blended explain could read Fresh-overall.
  MITIGATION: the MEET fold (D-EXPLAIN-CONF) + the shared cert fingerprint (spans LiveGraph partition epochs
  AND the SQLite snapshot_uid). Monotone — cannot raise to Fresh.
RISK-E-B — AUTHORITY/STRUCTURE SEAM ERASING COMPUTED FACT (= contract RISK-B). EXPLAIN_BOUNDARY/EXPLAIN_GATE
  overlay structural facts via declarations. MITIGATION: D-EXPLAIN-AUTH — both computed and effective views
  queryable; the multi-source {declaration, sqlite} leaf labels the Authority origin.
RISK-E-C — KNOWN-ZERO vs UNKNOWN (the CENTRAL explain risk; = contract RISK-C/F3 sharpened by explain's
  no-limits design). explain emits NO limits[] and OMITS conditional sections when empty (§1d/§1c). Today
  "absent == known-zero" is safe (synchronous SQLite always answers). Under LG-first, a non-resident/degraded
  LiveGraph read could make a section absent for an UNKNOWN reason — and explain has no limit to say so.
  MITIGATION: the wrapper's per-leaf trust/provenance is the ONLY honest channel; CALLERS/CALLEES (always
  emitted) are NEVER Exact-empty under a residency gap; a degraded conditional section surfaces a labelled
  leaf or a recovered SQLite answer, NEVER a silent omission; explain GAINS the envelope provenance limit
  codes (§3b) as its machine-degradation marker. Validated by D-V4. BOUNDED, not open.
RISK-E-D — IDENTITY COORDINATE FIDELITY / MULTI-SOURCE LEAF (= contract F1/F4 + D8 at the identity leaf). The
  identity evidence mixes an LG-derivable anchor with snapshot-scoped coordinates (line_start/file_path/
  language) the LiveGraph does not track live. A careless impl could mint an Exact identity leaf asserting a
  live line the LiveGraph cannot reproduce, OR hide the SQLite coordinate contributor (false single-source
  provenance). MITIGATION: D-EXPLAIN-IDENTITY — the served leaf is a D8 multi-source { livegraph, sqlite } leaf
  (anchor LG + coordinates SQLite) whose trust + freshness is the MEET of both contributors, so it is never
  Exact above its coordinate posture; the cert governs the anchor only; the whole leaf collapses to { sqlite }
  when the anchor falls back. Both sources are labelled (never false single-source). Validated by D-V6/E3. BOUNDED.
RISK-E-E — MODULE/PARTITION-IDENTITY CORRESPONDENCE (= contract RISK-E + orient RISK-O-H). EXPLAIN_CALLERS/
  CALLEES are MODULE-grouped (group_by_module top-3, mod.rs:289/313) + carry a full per-item list, while the
  migrated LiveGraph answers are PARTITION-grouped (CallersAnswer/CalleesAnswer, lib.rs:117-136) with a
  ratified callees-residency asymmetry (lib.rs:200-205). EXPLAIN_SYMBOLS/FILES counts: LiveGraph aggregation
  vs SQLite inventory may diverge. MITIGATION: derive the callgraph summary only where partitions are
  resident, else labelled SQLite fallback (find_symbol_callers/callees carry module_path directly); keep
  EXPLAIN_SYMBOLS/FILES listings SQLite-first (D-EXPLAIN-LISTINGS). Never Exact from partition-only data;
  never Exact-empty. Validated by D-V4/D-V5. BOUNDED.
RISK-E-F — ENVELOPE SHAPE CHURN (= contract RISK-F, shared with orient/check). The wrapper changes explain's
  JSON wire shape (top level becomes CoherenceEnvelope; value = CoherentOrientResult) AND populates
  trust_briefing (renaming the degraded `trust` overlay key). ACCEPTED, bounded. MITIGATION: land the wrapper
  ONCE in COHERENCE-ENVELOPE-1; keep the reused structural VALUE payloads byte-identical (P1); update the CLI
  human-render deserialization + JSON-contract fixtures in lockstep; ONE shared schema bump across
  orient/check/explain (not a per-command bump).
RISK-E-G — TRUST_BRIEFING SHARED-FIELD POPULATE PATH (NEW, explain-specific cross-slice). orient asserted
  trust_briefing is "populated ONLY by orient"; first-hand, explain ALSO injects the overlay and MUST
  populate it. A COHERENCE-ENVELOPE-1 / orient implementation that hard-codes an orient-only populate path
  would silently drop explain's briefing. MITIGATION: D-EXPLAIN-TRUST-BRIEFING / R3 — the shared container's
  populate path MUST permit both orient and explain; check leaves it None. Recorded so the support module is
  built for both. Validated by §5 W2 (explain present-when-degraded) vs check W3 (always absent).
RISK-E-H — Signal.freshness reconciliation (= contract RISK-G). Largely VACUOUS for explain: its constructors
  set NO inner Signal.freshness (signal.rs:1381-1538). MITIGATION: the OUTER leaf FreshnessState is
  authoritative; COHERENCE-ENVELOPE-1 owns the single FreshnessInfo->FreshnessState mapping; explain consumes
  it. Recorded, contract-deferred.
```

---

## References
```text
GOVERNANCE / MODEL:
- docs/VISION.md §Fact Certainty Model / §Product Layer Model / §Agent Priorities (#2 preserve computed
  truth) / §"Deterministic Discovery As Token Reduction" (rgr owns identity; the agent owns live source
  location — the basis for D-EXPLAIN-IDENTITY's coordinate-field fallback).
- agent_docs/architecture.md §Product Layer Stack (Layer 0-4) / Rule 6 "null=unknown, empty=known-zero" /
  §Build Order (support module -> feature).
- CLAUDE.md §Fact Certainty Model / §Decision Autonomy / §Evidence Law.

AUTHORITATIVE CONTRACT + PRECEDENT:
- docs/slices/coherence-layer-1.md — RATIFIED + AMENDED (2026-06-09, D8 multi-source LEAF provenance). Cited
  by SECTION / DECISION ID (stable across the D8 line shift): §"Per-command source map" (explain row — the
  ratified postures this doc refines); Q1/Q4 (explain IDENTITY/CALLERS/CALLEES/IMPORTS/cycles = LG-first
  direct reuse); §"The shared coherence answer-envelope" (the wrapper, D7 CoherentOrientResult, the AMENDED
  Provenance.source: BTreeSet<Source>); D3 (MEET); D5 (authority overlay); D8 (multi-source leaf — the basis
  for EXPLAIN_BOUNDARY's {declaration, sqlite} leaf AND EXPLAIN_IDENTITY's {livegraph, sqlite} served leaf);
  §"Safe-fallback contract" (ORIENT/EXPLAIN row);
  §"Proposed follow-up slice sequence" (EXPLAIN depends on ORIENT); RISK-A/B/E/F/G.
- docs/slices/orient-livegraph-1.md — the SHAPE precedent. CoherentOrientResult reuse; D-ORIENT-4 confidence
  MEET + zero-signal carve-out; D-ORIENT-5 authority overlay; D-ORIENT-SYMBOL-CALLGRAPH (the LG-first
  callers/callees posture explain shares); D-ORIENT-6 = O2 trust_briefing (which explain POPULATES — CORRECTING
  orient:539/762's "check/explain produce no overlay"); §4 transport degradation.
  CAVEAT (evidence-law honesty): orient-livegraph-1.md:539 and W3:762 state "check/explain produce no
  overlay". FIRST-HAND OBSERVED FALSE for explain (handle_explain injects the overlay, dispatch.rs:2800-2816).
  This doc corrects it (D-EXPLAIN-TRUST-BRIEFING / RISK-E-G).
- docs/slices/check-livegraph-1.md — the SECOND application. Three-surface enumeration discipline; D-CHECK-2
  (check has NO overlay — the contrast that makes explain's overlay notable); D8/D-CHECK-5 multi-source leaf;
  §3e CLI-wrapper remap analysis (explain reuses the human-render half, NOT the exit-code half).

EXPLAIN IMPLEMENTATION TODAY (all SQLite/Authority; LiveGraph=NONE) [OBSERVED, first-hand this turn]:
- rust/crates/agent/src/explain/mod.rs — run_explain:45 (focus dispatch :76/105/149); explain_symbol:253
  (IDENTITY:269 via get_symbol_context:107/155; CALLERS find_symbol_callers:286/:299; CALLEES
  find_symbol_callees:310/:323; module-context block :334; CYCLES find_cycles_involving_module:336/:347;
  BOUNDARY get_active_boundary_declarations:359 + find_imports_between_paths:368/:386; GATE
  build_gate_signal:399; TRUST build_trust_signal:412; MEASUREMENTS dormant :418; stale get_stale_files:431;
  confidence:432); explain_file:460 (IDENTITY compute_file_summary:476/:477; IMPORTS find_file_imports:492/:502;
  SYMBOLS list_symbols_in_file:512/:525; TRUST:534; `_ = now`:469); explain_path:582 (IDENTITY
  compute_path_summary:597/:598; FILES list_files_in_path:613/:625; CYCLES find_cycles_involving_path:635/:646;
  BOUNDARY find_boundary_declarations_in_path:655/:678; GATE build_gate_signal:688; TRUST:700); build_no_match
  zero-signal:222-248; ambiguous inline zero-signal:180-200; group_by_module top-3:750; build_trust_signal:767
  (dead_code_reliability removed:776); build_gate_signal:786 (get_active_requirements:798;
  assemble_from_requirements:844; counts.total==0->Ok:856; with_module_context:882-884).
- rust/crates/agent/src/dto/signal.rs — SignalCode 11 Explain codes:284-294; as_str:319-329; tier_priority
  Explain 0-10:370-380; descriptor Explain->(Explain,Low):414-424; the 11 explain_* constructors
  (SourceRef::ExplainPipeline):1381-1538.
- rust/crates/daemon-runtime/src/dispatch.rs — handle_explain:2734-2819 (target/budget parse :2742-2761;
  run_explain :2770; display_name :2787; get_snapshot :2801; compute_trust_overlay_for_snapshot "CALLS+IMPORTS"
  :2802; gate has_degradation()||!caveats.is_empty() :2808; map.insert("trust",...) :2811; NO LiveGraph branch).
- rust/crates/rgr/src/presentation/explain.rs — ExplainResponse (NO trust field; snapshot #[allow(dead_code)])
  :64-81; render_human:118-155; render_target:157-188; get_identity_name/info:190-227; render_signal_section
  (identity->None:259):246-263; renderers callers:265/callees:290/imports:315/symbols:335/files:360/cycles:384/
  boundary:411/gate:443/trust:474/measurements:496; render_hides_internal_fields test:639-644.
- rust/crates/rgr/src/commands/orient.rs — run_explain_cmd:340-496 (arg parse incl. --budget:341-414; missing
  target exit1:395; cwd/canonicalize exit2:419/429; DaemonClient::new exit2:438; request("explain"):450;
  --json full envelope SUCCESS:454-457; human from_value::<ExplainResponse>+render SUCCESS:466-469; parse error
  exit2:472; RepoNotFound+hint exit2:478-482; catch-all exit2:488); print_explain_usage:494.

ANSWER-ENVELOPE VOCABULARY + LIVEGRAPH SURFACE + CERT-FASTPATH [OBSERVED via contract / precedent]:
- rust/crates/repo-graph-trust-model/src/lib.rs — AnswerClass / FreshnessState / DegradationReason /
  QueryCompleteness / ProvenanceBasis + the 6 invariants (cited via coherence-layer-1.md).
- rust/crates/repo-graph-livegraph/src/lib.rs — callers:443, callees:560, value_facts:662 (complexity only),
  module_import_cycles:1264, live_import_view:1574; CallersAnswer/CalleesAnswer partition-grouped:117-136;
  callees summary-residency asymmetry:200-205 (cited via contract / orient precedent).
- rust/crates/daemon-runtime/src/livegraph_feed.rs — callers_engine_response:448, callees_engine_response:544,
  FallbackReason enum, import_cert_fingerprint, the cert ladder + serve_*_sqlite (cited via contract / orient
  precedent — the mechanism explain's five LG-first leaves reuse).

EVIDENCE LOG:
- [EXECUTED, this turn] `rmap explain src` -> "error: daemon connection failed: socket does not exist:
  /Users/apple/Library/Application Support/repo-graph/daemon.sock" (transport degradation path, §4).
- [OBSERVED, first-hand, this turn] explain/mod.rs (whole file 1-889); dispatch.rs:2734-2819 (incl. the
  trust-overlay injection :2800-2816 — the load-bearing correction); presentation/explain.rs:1-671;
  commands/orient.rs:334-496; signal.rs:256-435 + the generic Signal record + Serialize impl :947-987 +
  build/with_module_context/with_freshness :1030-1069 + the 11 explain_* constructors :1379-1538.
- [OBSERVED, via contract/precedent] coherence-layer-1.md (full); orient-livegraph-1.md (full);
  check-livegraph-1.md (full); agent_impl.rs concrete SQL behind AgentStorageRead + the Tier model + the
  livegraph/livegraph_feed offsets (cited, not re-read).
- [INFERRED] the CoherenceEnvelope wiring (§3), the safe-fallback / known-zero-vs-unknown rules (§4), the
  validation plan (§5), the forced-decision verdicts (D-EXPLAIN-1..SCOPE) — grounded in the ratified contract
  + the orient/check precedents.
- [DECISION STATUS] No open DECISION_REQUIRED. Every explain-specific point is decide-and-record within the
  ratified contract or a direct reuse of an orient/check ratified decision. The load-bearing first-hand
  correction (explain DOES inject the trust overlay; orient's "explain produces no overlay" is wrong) is
  recorded (D-EXPLAIN-TRUST-BRIEFING / RISK-E-G), strengthening — not re-opening — the ratified O2.
```
