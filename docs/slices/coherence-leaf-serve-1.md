# COHERENCE-LEAF-SERVE-1: bounded (b)-leaf serve-then-fallback for the coherence commands — SPEC (PREREQ-1)

Slice ID: COHERENCE-LEAF-SERVE-1
Status: **SPEC ONLY (spec-first; no code, no deletion, no migration, no default flip).** This document SPECS
PREREQ-1 of the ratified SQLITE-RAW-DECOMMISSION-1 contract (Option A, `f9cfe23`): the BOUNDED serve-then-
fallback that makes the LG-DERIVABLE resolved/non-trust leaves (class (b), readiness-10 §191–202) of the
coherence commands (orient / explain / trust) servable from the LiveGraph on GREEN, while the (c) trust
unresolved-call contributor stays SQLite-LABELLED FOREVER (Contract Clause 3) and the (d) fallback/cert/non-TS
reads remain (PREREQ-2, a LATER slice). It STARTS NO IMPL. The two open architecture-boundary decisions it
surfaces (DR-CLS-2 re-ratify, DR-CLS-FOCUS the focus-resolution gap) are DECISION_REQUIRED, not decided here.
Track: Stage D — SQLite-raw decommission. PREREQ-1 of SQLITE-RAW-DECOMMISSION-1.
Baseline / grounding: `docs/slices/sqlite-raw-decommission-1.md` (the contract; §4 rows 6–10, §6 the (c) floor,
§9 PREREQ-1/PREREQ-2) + `sqlite-raw-decommission-readiness-10.md` (the (a)–(d) partition, §180–227) +
`orient-sqlite-free-1.md` (DR-2/DR-4 + the composite cert) + `explain-sqlite-free-1.md` (DR-E2) +
`trust-livegraph-1.md` (the served LG half) + `trust-summary-livegraph-1.md` (the 8-field table) +
the COHERENCE-LAYER-1 `CoherenceEnvelope<T>` contract + the drilldown cert-fastpaths
(imports/cycles/stats-livegraph default fastpaths).
HEAD at authoring: `f9cfe23` (SQLITE-RAW-DECOMMISSION-1 contract ratified). Working tree clean.

> **DECISION RESOLUTION (ratified by operator, 2026-06-14):**
> - **DR-CLS-2-MODULE-SUMMARY-COUNTS → Option A (RE-SOURCE).** orient's file/symbol/language counts are
>   re-sourced from LiveGraph `module_stats` (cert-gated, no-loss); `discovered_module_count` stays on
>   `module_candidates` (Tier-B SQLite). Re-ratifies the D-ORIENT-2 SQLite-first posture for the structural counts.
> - **DR-CLS-FOCUS-RESOLUTION-PRODUCER → Option B (BUILD THE PRODUCER).** A LiveGraph focus-resolution producer
>   (focus string → IR symbol/file/module identity; filling the module-node-identity + `qualified_name` sub-gaps)
>   is to be BUILT — FEASIBLE (the IR has `name` + `range.file`), unlike the refuted (c) trust producer. This
>   FULLY closes PREREQ-1 (orient all-focuses + explain), converting the bounded `nodes`/`edges` retirement from
>   PARKED to mechanically-ready. Option A (orient-repo-focus-only) NOT taken (it leaves explain holding `nodes`,
>   so the retirement stays parked); Option C (abandon) rejected.
> **NEXT BUILD: the focus-resolution producer (spec-first), THEN this COHERENCE-LEAF-SERVE impl** (whose IMPL is
> gated on that producer + DR-CLS-2 A). The (c) trust boundary remains FIXED (Contract Clause 3 / Option A).

> **DR-CLS-2 REVISED — `module_stats` CANNOT source MODULE_SUMMARY (operator re-ratified 2026-06-15, during
> COHERENCE-LEAF-SERVE-IMPL-1).** First-hand reads (codex-confirmed) proved the original DR-CLS-2 conflated two
> distinct SQLite surfaces: orient's `compute_{repo,path,file}_summary` count ALL files (incl. non-TS), ALL
> `kind='SYMBOL'` nodes, and ALL languages; the TS-only `module_stats` counts only TS FILE-scope nodes
> (root-excluded), EXPORTED symbols, and no languages — NOT equal (cert RED by construction on any repo with a
> non-TS file / non-exported symbol / root file; the dogfood fixture itself: orient 4 files vs module_stats 2).
> The shipped stats cert proves `module_stats == rmap stats`, NOT `== compute_*_summary`. **REVISED RESOLUTION →
> Option A: MODULE_SUMMARY counts stay SQLite-LABELLED** (treated like the (c) trust leaf — honest SQLite read,
> provenance.source = sqlite, NEVER LG-served). The LG-served (b) leaves are **focus-resolution (the producer),
> cycles, and callers/callees** (the last needs a NEW cacheable repo-wide callgraph no-loss cert mirroring
> cycles_cert — decide-and-record, no new SQLite surface). `resolved_calls` rides with the (c) trust read
> (SQLite-labelled). **Achievable: SYMBOL-focus orient becomes `nodes`-free on green** (its pipeline emits no
> MODULE_SUMMARY); REPO/PATH/FILE keep the `compute_*_summary` `nodes` read as a permanent SQLite contributor
> (same posture as the (c) trust `edges` read). Output EXACT (no regression). Options B/C/D rejected
> (no-win/regression / out-of-scope non-TS / dominated). The focus-resolution producer is confirmed sufficient.

> **DR-CLS-CYCLES — cycles CANNOT be no-loss-served either (operator ratified CYCLES-A, 2026-06-15).** During
> COHERENCE-LEAF-SERVE-IMPL-1, first-hand reads (codex-confirmed) proved the set-based `cycles_cert` (canonical
> BTreeSet path-SETS, order/rotation-independent) does NOT license an ordered serve of orient's cycle output,
> which is ORDER- and IDENTITY-sensitive (`take(3)` selection depends on list order; ring-order render;
> basename vs qualified naming; the LG and SQLite Tarjan orders differ on the same set -> `take(3)` can pick
> DIFFERENT cycles -> a different orient answer on green vs fallback). Same false-parity class as `module_stats`.
> **RATIFIED -> CYCLES-A: cycles STAY SQLite-served, unchanged** (delegated to SQLite like MODULE_SUMMARY + the
> (c) leaf). The LG-served (b) leaves are now ONLY **focus-resolution (the producer) + callers/callees
> (callgraph cert)**. SYMBOL-focus orient stays `nodes`-free on green (cycles is an `edges` read that remains
> alongside the (c) trust `edges` read). Output EXACT. CYCLES-B (canonicalize orient's cycle-output contract,
> then serve cycles from the LiveGraph) is a DEFERRED follow-up (a visible wire change to a shipped signal);
> CYCLES-C (ordered cert) rejected (RED by construction). Revise item: `build_orient_envelope`'s callgraph LABEL
> path must route through the callgraph cert (zero-read on green), not the per-call SQLite gate (codex finding).

> **COHERENCE-LEAF-SERVE-IMPL-2 — explain consumer scope + honest bound (operator-directed 2026-06-20; first-hand
> reads of `agent/src/explain/mod.rs` + `daemon-runtime/src/{orient_serve,dispatch.rs}`).** The orient consumer
> (`765583b`) shipped a serve decorator that already overrides ALL FOUR focus-resolution methods
> (`resolve_path_focus` / `resolve_stable_key_focus` / `resolve_symbol_name` / `get_symbol_context`) + callgraph —
> MORE than orient alone exploits. explain resolves its focus through those SAME storage-port methods (`run_explain`
> -> `resolve_path_focus` UNCONDITIONAL at mod.rs:86) and the `explain_symbol` pipeline emits NO MODULE_SUMMARY. So
> wiring the SAME decorator + bounded cert (`focus_resolution ∧ callgraph`) into `handle_explain` on green makes
> **explain SYMBOL-focus `nodes`-free on green** — the explain analogue of orient SYMBOL, the SECOND real eager-read
> elimination of the arc. **HONEST BOUND (first-hand):** `explain_file` reads `compute_file_summary` +
> `list_symbols_in_file`; `explain_path` reads `compute_path_summary` + `list_files_in_path` — ALL `nodes`, for
> their identity/symbols/files leaves, NOT focus resolution, NOT decorator-served. FILE/PATH keep those `nodes`
> reads as PERMANENT SQLite contributors (the explain analogue of orient REPO/PATH/FILE's MODULE_SUMMARY); they are
> NOT `nodes`-free. The (c) trust read + cycles `edges` remain in EVERY explain pipeline (never `edges`-free; Clause
> 3 + CYCLES-A). REUSE the decorator + cert as-is (two callers now — decide-and-record; NO rename this slice:
> cosmetic + churn). `build_explain_envelope` already REBUILDS its leaf VALUES from the LiveGraph independent of the
> use-case storage (the `82b6557` path), so its labels follow its own rebuild source — explain likely needs NO
> envelope change (no orient review-3 desync); VERIFY first-hand, thread `serve_from_lg` ONLY if a real desync
> exists. Output EXACT.

> **HEADLINE VERDICT (the load-bearing question, answered decisively, evidence-first):**
> **Focus resolution (focus string → IR symbol/file/module identity) has NO LiveGraph producer — it is a
> SECOND PRODUCER GAP.** [OBSERVED, first-hand: the LiveGraph is keyed EXCLUSIVELY by `CanonicalKey`; no
> name/path/stable-key resolver surface exists (grep-empty); the IR carries NO `qualified_name` and NO
> first-class MODULE-node identity.] This gap is FEASIBLE-BUT-UNBUILT (unlike the (c) trust gap, which is
> impossible-by-substrate): the IR DOES carry `name` + `range.file`, so a symbol-by-name / file-by-path
> resolver CAN be built — but module-by-path identity and `get_symbol_context`'s `qualified_name` are partial
> sub-gaps needing reconciliation. **Consequence — PREREQ-1 is BOUNDED:**
>   · **orient REPO-focus** (`focus=None`) calls NO focus resolution → its (b) leaves are servable on green;
>     PREREQ-1 is CLOSABLE for it (modulo DR-CLS-2 + the cert build).
>   · **focused orient (symbol/path/file) + EVERY explain** resolve their focus by reading `nodes`
>     UNCONDITIONALLY and FIRST (before any leaf / before the cert precondition) → serving the (b) leaf VALUES
>     does NOT eliminate their eager `nodes` read. They are GATED on a focus-resolution producer built FIRST.
>   · **trust** is repo-wide, has NO focus resolution → no focus gap; its only (b) leaf is `resolved_calls`,
>     entangled in the Half-B resolution leaf with the (c) `call_resolution_rate`.
> This MIRRORS how the trust-core producer gap gated the prior arc. It is surfaced as **DR-CLS-FOCUS**
> (orient-repo-focus-only vs build a focus-resolution producer first), the packet STOP_CONDITION honored.

---

## 0. What this spec is (and is not)

```text
WHAT IT IS [INFERRED over the OBSERVED contract + readiness-10 + the arc specs]:
  PREREQ-1 of SQLITE-RAW-DECOMMISSION-1 (contract §9) is "the (b) leaves served": the LG-derivable
  resolved/non-trust leaves of orient/explain/trust must ACTUALLY be served from the LiveGraph on green (the
  marginal P1 fastpaths), gated by a no-loss cert, with SQLite fallback. This document SPECS that work:
  enumerates the (b) leaves per command, fixes the bounded composite cert (the (b)-only AND-fold), states the
  (c) labelled boundary, answers the focus-resolution load-bearing question, and bounds the achievable scope.

WHAT IT IS NOT:
  Not an implementation. Not a deletion / migration / default flip. It SHIPS NO fastpath, DROPS NO read,
  flips NO gate. It is PREREQ-1 (a step toward the bounded nodes/edges retirement), NOT the retirement.
  PREREQ-2 (the covered-subset (d) fallback/cert handling), the bounded retirement IMPL, and P2 (non-TS
  coverage) are SEPARATE, LATER tracks (§11 scope boundary). The (c) trust unresolved-call boundary is FIXED
  by Contract Clause 3 / Option A — this spec does NOT re-open it.

EVIDENCE LABELS (repo Evidence Law, agent_docs/validation.md):
  OBSERVED  = a source/doc I read first-hand THIS turn (file:line cited). FIRST-HAND code reads are marked
              "OBSERVED first-hand"; quotations of a committed doc are marked "OBSERVED (doc)".
  INFERRED  = my classification/synthesis over those OBSERVED facts (the cert design, the bound, the scope).
  EXECUTED  = a command I ran this turn with output observed (the validation ledger, §13).
  NOT RUN   = skipped, with reason.
  A spec-first slice does NOT start the daemon and does NOT run index/refresh/dev-install (state-mutating;
  scripts/** is out of scope) — the SAME stance the four arc specs + the contract took. No clause below
  depends on a live capture; every structural claim is a first-hand read of source or a committed doc.
```

---

## 1. VISION tie — Orientation over Perfection + the Fact-Certainty Model

```text
TWO VISION principles ground this spec [OBSERVED (doc): docs/VISION.md § "VISION: Orientation over
Perfection"; § "Fact Certainty Model"; CLAUDE.md "Persistence Completeness"]:

  (1) ORIENTATION OVER PERFECTION. Serving the (b) leaves from the current-state LiveGraph improves
      orientation freshness (the LiveGraph is the current-state substrate; the SQLite snapshot is the
      outgoing one) WITHOUT touching precision: every served leaf is gated by an EXISTING-OR-SPEC'D no-loss
      cert, and on any divergence/non-residency/non-TS it falls back to the byte-identical SQLite read. The
      win is freshness + a reduced eager read, not a new fact class.

  (2) FACT-CERTAINTY / HONEST DEGRADATION. The (c) trust unresolved-call fields have NO current-state source
      (probe NO-GO); they stay SQLite-LABELLED (provenance.source = sqlite), NEVER re-cast as current-state
      LiveGraph resolution (the coherence F5 rule). The bounded cert AND-folds ONLY the (b) leaves; it never
      certifies the (c) leaf, so a GREEN cert can NEVER mislabel an outgoing-extractor fact as current-state.
      This is the Fact-Certainty Model applied at the leaf boundary.

PERSISTENCE-COMPLETENESS framing [OBSERVED (doc): CLAUDE.md "Persistence Completeness"]: a (b) leaf is
"served" only with write path (the LiveGraph surface) + read path (the fastpath) + refresh behavior (the
shared fingerprint) + trust impact (the no-loss cert + the labelled fallback) + CLI visibility (backend_used
/ provenance.source) + validation (parity compare + no-eager-read proof). This spec fixes each for the (b)
leaves; the IMPL realizes them.
```

---

## 2. Where the coherence commands stand TODAY (OBSERVED, first-hand) — the eager-read baseline

```text
The four coherence commands serve a CoherenceEnvelope but STILL read SQLite eagerly via the base use case
every call (readiness-10 §253–271; CURRENT_SLICE banner). orient/explain/trust read `nodes`/`edges` eagerly;
their LiveGraph serving (where present) is assembled ON TOP, not INSTEAD OF the eager read. The eager reads,
first-hand:

orient [OBSERVED first-hand: rust/crates/agent/src/orient/mod.rs]:
  · mod.rs:64-66 — `match focus { None => orient_repo(...), Some(s) => orient_focused(...) }`. REPO focus
    (focus=None) routes to orient_repo with NO focus resolution. FOCUSED routes to orient_focused.
  · orient_focused (mod.rs:76) resolves the focus string via resolve_path_focus (mod.rs:101),
    resolve_stable_key_focus (mod.rs:139), resolve_symbol_name (mod.rs:191), get_symbol_context (mod.rs:144).
  · repo focus reads (5-path map, orient-sqlite-free-1.md:161-169, OBSERVED doc): trust-core (edges +
    unresolved_edges, UNCONDITIONAL), cycles (edges, UNCONDITIONAL), MODULE_SUMMARY structural counts
    (nodes, UNCONDITIONAL), boundary/gate (edges, conditional).

explain [OBSERVED first-hand: rust/crates/agent/src/explain/mod.rs]:
  · run_explain(... target: &str ...) (mod.rs:58) — `target` is a REQUIRED &str; explain has NO repo focus.
  · mod.rs:85-86 — comment "Resolve focus (reusing orient's resolution logic)" then
    `let resolution = storage.resolve_path_focus(snapshot_uid, target)?;` — UNCONDITIONAL on every call.
    Then resolve_stable_key_focus (mod.rs:115), get_symbol_context (mod.rs:117/165), resolve_symbol_name
    (mod.rs:159) on the branch paths.
  · explain ALSO reads trust-core (edges + unresolved_edges) unconditionally in every pipeline
    (explain-sqlite-free-1.md:19-24, 211-212, OBSERVED doc).
  · explain ALREADY SERVES 5 green leaf VALUES from the LiveGraph (commit 82b6557): EXPLAIN_CALLERS,
    EXPLAIN_CALLEES, EXPLAIN_IMPORTS (file focus), EXPLAIN_CYCLES, EXPLAIN_IDENTITY anchor
    (explain-sqlite-free-1.md:205-208, OBSERVED doc).

trust [OBSERVED (doc): trust-livegraph-1.md:299-300, 693-694]:
  · "NO focus dispatch. trust is repo-wide ALWAYS (no file/path/symbol pipeline)." => no focus-resolution
    `nodes` read. It serves Half-A (the LiveGraph current-state posture leaf) BESIDE Half-B (the SQLite-
    labelled v1 diagnostics axes). resolved_calls is served {sqlite} today (count_edges_by_type(CALLS),
    trust-livegraph-1.md:333, OBSERVED doc).

The focus-resolution functions, first-hand [OBSERVED first-hand: rust/crates/storage/src/agent_impl.rs]:
  · resolve_path_focus (agent_impl.rs:366) — `FROM nodes n JOIN files f` (:378/:393/:406/:421): path → exact
    FILE node / content-under-prefix / MODULE node (returns file_stable_key + module_stable_key).
  · resolve_stable_key_focus (agent_impl.rs:437) — `FROM nodes n LEFT JOIN files f` (:443): stable_key →
    (kind, file).
  · resolve_symbol_name (agent_impl.rs:800) — `FROM nodes n LEFT JOIN files f` (:808): name → up to 5
    SYMBOL candidates.
  · get_symbol_context (agent_impl.rs:834) — `FROM nodes n LEFT JOIN files f LEFT JOIN ... edges own` (:840):
    stable_key → (name, qualified_name, subtype, line_start, file_path, module_path, module_stable_key).
  · list_symbols_in_file (agent_impl.rs:1037), list_files_in_path (agent_impl.rs:1070) — `nodes` listings.
  EVERY focus-resolution function reads `nodes` (joining files / edges). This is the read PREREQ-1 must
  eliminate for explain + focused orient to be `nodes`-free — and it has no LiveGraph producer (§5).
```

---

## 3. The (b) leaves per command (the PREREQ-1 enumeration) — OBSERVED, file:line

```text
Each (b) leaf names its LiveGraph surface + its no-loss cert (existing or spec'd). Grounding: contract §4
rows 6–10 + readiness-10 §191–202 + the arc specs. The LiveGraph surfaces are OBSERVED first-hand in
rust/crates/repo-graph-livegraph/src/lib.rs. CLASS (a)/(b)/(c)/(d) per readiness-10 §180.
```

| # | Command | (b) leaf | Reads today | LiveGraph surface (OBSERVED first-hand) | No-loss cert | Served today? |
|---|---|---|---|---|---|---|
| 1 | orient + explain | IMPORT_CYCLES | `edges` | `module_import_cycles` (lib.rs:1317) | cycles cert — `build_and_store_cycles_cert` (is_exact: missing=0 ∧ extra=0) **[EXISTS]** | orient: no · explain: **YES** (82b6557) |
| 2 | orient (symbol focus) + explain | CALLERS / CALLEES | `edges` | `callers` (lib.rs:469) / `callees` (lib.rs:586) | per-symbol key-set no-loss compare **[EXISTS]** | orient: no · explain: **YES** (82b6557) |
| 3 | explain (file focus) | IMPORTS | `edges` (IMPORTS) | `live_import_view` (lib.rs:1675) | import cert — `build_and_store_import_cert` **[EXISTS]** | **YES** (82b6557) |
| 4 | explain | IDENTITY anchor (name/subtype) | `nodes` | `node_display` (lib.rs:1051) | identity anchor cert (serve_identity ladder); multi-source `{livegraph, sqlite}` (D8) **[EXISTS]** | **YES** (82b6557) |
| 5 | orient | MODULE_SUMMARY structural counts (file/symbol/language) | `nodes` (compute_repo_summary, agent_impl.rs:241) | `module_stats` (lib.rs:1376) | stats field-exact cert — `build_and_store_stats_cert` (is_exact) **[EXISTS; needs DR-CLS-2 to re-source + reconcile identity divergence]** | no (SQLite-first today) |
| 6 | trust + orient | `resolved_calls` (count) | `edges` count (`count_edges_by_type(CALLS)`, trust_impl.rs:116) | count IR `EdgeType::Calls` edges | direct count compare (LG Calls-edge count == SQLite count) **[LG-derivable now; SPEC]** | no (served `{sqlite}` in Half-B) |

```text
NET (b) inventory [INFERRED over OBSERVED]:
  · orient REPO-focus (b) leaves cleanly separable: row 1 (cycles), row 5 (MODULE_SUMMARY counts). Row 6
    (resolved_calls) is (b) but ENTANGLED in the trust-core (c) read (see §4, §6).
  · orient FOCUSED (b) leaves: row 1 (cycles), row 2 (callers/callees, symbol focus). GATED on focus
    resolution (§5).
  · explain (b) leaves: rows 1,2,3,4 — ALREADY served as VALUES (82b6557). GATED on focus resolution to
    eliminate the eager `nodes` read (§5).
  · trust (b) leaf: row 6 (resolved_calls) only. ENTANGLED in the Half-B resolution leaf (§6).

CLASS CHECK against readiness-10 §191–202 [OBSERVED (doc)]: rows 1–6 are exactly the readiness-10 (b)
bullets (IMPORT_CYCLES / CALLERS-CALLEES / resolved_calls / MODULE_SUMMARY counts / imports). NO row classed
(b) here is actually (a)/(c)/(d) — no contradiction; STOP_CONDITION-2 NOT triggered.
```

---

## 4. The (c) boundary — FIXED by Contract Clause 3 / Option A (NOT re-opened)

```text
[OBSERVED (doc): sqlite-raw-decommission-1.md §6 Contract Clause 3; readiness-10 §204-212; trust-livegraph-1
§Half-B; trust-summary-livegraph-1 §3a.] STATED AS FIXED, not re-opened (packet FILES_OUT_OF_SCOPE +
STOP_CONDITION-3):

  · The trust unresolved-call fields — unresolved_calls(_external/_internal_like), call_resolution_rate, the
    call_graph/change_impact reliability axes, classifications[]/categories[], unknown_calls_blast_radius,
    enrichment_status/state — are RETAINED + SQLite-LABELLED FOREVER (provenance.source = sqlite). Of the 8
    AgentTrustSummary fields, exactly ONE (`resolved_calls`) is LG-derivable; the other 7 are NEEDS-EXTENSION
    and the extension is refuted (probe NO-GO). [OBSERVED (doc): trust-summary-livegraph-1.md:298-310.]
  · These fields are EXCLUDED from the bounded cert (§7). The bounded cert AND-folds ONLY the (b) leaves; it
    NEVER certifies a (c) leaf. The (c) leaf is served via the Half-B labelled posture, OUTSIDE the cert.
  · This spec does NOT re-open Clause 3. The (c) read stays eager + SQLite-labelled in orient/explain/trust
    regardless of PREREQ-1. PREREQ-1 reduces the (b) eager read; it does NOT remove the (c) eager read.

CONSEQUENCE for the eager read [INFERRED]: because the (c) trust-core read is UNCONDITIONAL in orient +
explain and reads `edges` + `unresolved_edges` every call, NEITHER orient NOR explain can ever be
`edges`/`unresolved_edges`-free on green — independent of PREREQ-1. PREREQ-1's reachable target is the
`nodes` read (focus resolution + MODULE_SUMMARY counts) and the (b) `edges` readers that have a LiveGraph
surface (cycles, callers/callees, resolved_calls), NOT the (c) `edges`/`unresolved_edges` read.
```

---

## 5. THE LOAD-BEARING QUESTION — focus resolution: LG-servable, or a SECOND PRODUCER GAP?

### 5a. The question (the packet's FOCUS RESOLUTION)

```text
Can focus resolution (focus string → IR symbol/file/module identity) be served from the current-state
LiveGraph (a new resolver surface), or is there NO producer (a second gap)? Verified FIRST-HAND, the way the
probe verified the unresolved-call question — NOT assumed.
```

### 5b. First-hand evidence (OBSERVED first-hand)

```text
[E1] The LiveGraph is keyed EXCLUSIVELY by CanonicalKey; it has NO focus-string resolver surface.
     [OBSERVED first-hand: rust/crates/repo-graph-livegraph/src/lib.rs.]
     · The public lookup surfaces all take a CanonicalKey or a key-STRING that must already BE a CanonicalKey:
         node(&CanonicalKey) (ir/lib.rs:407), node_location(&CanonicalKey) (lib.rs:1031),
         node_display(&CanonicalKey) (lib.rs:1051).
         callers(target: &str) (lib.rs:469) / callees(target: &str) (lib.rs:586) / value_facts(symbol: &str)
         (lib.rs:688) / path(from,to: &str) (lib.rs:842) — the `target` string is matched against
         `s.defines` / `s.ref_counts` (lib.rs:470-477), which are keyed by `n.key.as_str()` — the
         CanonicalKey string (contribution(), lib.rs:246-255). So the caller MUST already hold a
         CanonicalKey; these are NOT focus-string resolvers.
     · grep over lib.rs for `by_name|find_by|resolve_name|resolve_focus|resolve_symbol|qualified_name|
       stable_key` → EMPTY (no match). There is NO name→key, NO path→node, NO stable_key→node surface.

[E2] The IR carries NO `qualified_name` and NO first-class MODULE-node identity.
     [OBSERVED first-hand: rust/crates/repo-graph-ir/src/lib.rs.]
     · IrNode (lib.rs:338-360) has: key (CanonicalKey), subtype (String), name (String),
       range (Option<SourceRange> — range.file at :277), partition_id, identity_source, provenance,
       attributes (Option<SymbolAttributes>: visibility/is_top_level/symbol_kind). It does NOT have a
       `qualified_name` field (grep `qualified_name` over ir/lib.rs → EMPTY).
     · There is NO MODULE node. `IdentitySource::AstFileScope` (lib.rs:60-65) is "the file/module-scope
       structural node (ts-extractor FILE node). Has NO SCIP [identity] ... a module-architecture / boundary /
       runtime entity." Modules are DERIVED aggregations (module_stats / module_import_cycles compute by
       module path), NOT addressable nodes with stable keys.

[E3] The SQLite stable_key and the IR CanonicalKey are the SAME namespace (so a key→node lookup is sound).
     [OBSERVED (doc): key-namespace-repo-relative-1.md:14-22, IMPLEMENTED b72b075.] Keys are repo-relative:
       FILE `{repo}:{file_path}:FILE`; symbol `{repo}:{file_path}#{name}:SYMBOL:{subtype}`. LIVEGRAPH-
       INTEGRATION-1B proved "SCIP keys byte-equal to SQLite via repo_uid" for callers/callees. So a
       stable_key (a user-supplied or prior-resolved key) IS a valid LiveGraph lookup key.

[E4] The arc specs CORROBORATE the gap first-hand-equivalently.
     · orient-sqlite-free-1.md:202 (OBSERVED doc): focus resolution — "none — focus→node resolution was never
       migrated to the LiveGraph ... No-LG-producer." DR-4 (BLOCKING for focused orient).
     · explain-sqlite-free-1.md:550-556 (OBSERVED doc): "each reads `nodes` (agent_impl.rs:366/437/800/834).
       explain has NO repo/None focus (dispatch.rs:2738 requires a target), so ... the `nodes` read is
       UNCONDITIONAL ... No LiveGraph focus-resolution path exists." DR-E2.
```

### 5c. The VERDICT (decisive, INFERRED over OBSERVED)

```text
FOCUS RESOLUTION HAS NO LIVEGRAPH PRODUCER. It is a SECOND PRODUCER GAP. BUT its nature differs critically
from the (c) trust gap:

  · The (c) trust unresolved-call gap is IMPOSSIBLE-BY-SUBSTRATE (SCIP drops the unresolved-call fact; probe
    Q1/Q2 NO-GO). NOTHING under the SCIP substrate closes it.
  · The focus-resolution gap is FEASIBLE-BUT-UNBUILT. The IR DOES carry the data for the MAJORITY of
    resolution: `name` (symbol-by-name, like resolve_symbol_name's LIMIT-5 candidates), `range.file`
    (file-by-path; the FileInventory in rebuild_xpart_overlay already builds a path→FILE-key map,
    lib.rs:1077-1088), and the shared key namespace (stable_key→node, E3). A NEW LiveGraph resolver surface
    COULD serve these.
  · TWO sub-gaps are NOT clean: (i) MODULE-by-path resolution returns `module_stable_key` from a MODULE node
    (resolve_path_focus, agent_impl.rs:419); there is NO MODULE-node identity in the IR (E2) — a derived
    module-identity model would be needed. (ii) get_symbol_context returns `qualified_name` + module context;
    `qualified_name` is ABSENT from the IR (E2) — a display-field gap (note: qualified_name is a DISPLAY
    field, not a resolution KEY, so name/path/key resolution itself does not need it; it bites the symbol-
    context PAYLOAD parity).

SO: a focus-resolution producer is a NEW PRODUCER + a NEW data-shape (focus string → identity) crossing an
architectural boundary — an architecture-boundary decision (CLAUDE.md "Stop and ask"). It is NOT free, NOT
already present, and NOT fully parity-feasible without reconciling the two sub-gaps. It must be BUILT (a
SCIP-UNRESOLVED-CALL-PROBE-1 analogue for resolution feasibility, then a producer slice) BEFORE explain +
focused orient can be `nodes`-free.

THE ASYMMETRY THAT BOUNDS PREREQ-1 [OBSERVED first-hand: orient/mod.rs:64-66 vs explain/mod.rs:58,85-86]:
  · orient REPO-focus (focus=None) → orient_repo, NO focus resolution. Its (b) leaves (cycles, MODULE_SUMMARY
    counts) are servable on green WITHOUT a focus-resolution producer. PREREQ-1 CLOSABLE for it.
  · focused orient + EVERY explain resolve their focus by reading `nodes` UNCONDITIONALLY and FIRST. Serving
    the (b) leaf VALUES (which explain already does, 82b6557) does NOT eliminate that `nodes` read — the read
    happens before the cert precondition can even be evaluated (explain-sqlite-free-1.md:387-389: "the
    precondition itself needs a LiveGraph TARGET→PARTITION map without a SQLite focus read — i.e. DR-E2").
    They are GATED on the focus-resolution producer FIRST.

This mirrors the prior arc exactly: just as the trust-core producer gap was the decisive gate (and was probed
before any impl), the focus-resolution producer gap is the decisive gate for PREREQ-1's explain + focused-
orient coverage. It is surfaced as DR-CLS-FOCUS (§12), NOT assumed away (packet STOP_CONDITION-1 honored).
```

---

## 6. The bounded composite cert (the (b)-only AND-fold)

```text
DESIGN [INFERRED, mirroring orient-sqlite-free-1 §4b + explain-sqlite-free-1 §4b + the drilldown cert-
fastpaths]. The orient/explain composite cert AND-folds 7–9 contributors INCLUDING the trust-core contributor
(orient cert (5) / explain cert (6)), which made it RED-BY-CONSTRUCTION (no LiveGraph trust producer; trust
runs unconditionally) — orient-sqlite-free-1.md:280-286, OBSERVED doc. The BOUNDED cert REMOVES that
contributor (Clause 3): it folds ONLY the (b) leaves' no-loss certs. By excluding (c), the bounded cert CAN
be GREEN.

  BOUNDED-CLS cert {verdict: GREEN | RED, fingerprint} — on RepoState, in-memory RwLock<Option<...>>, S1
    (rebuilt on restart), mirroring import_cert / cycles_cert / stats_cert [OBSERVED (doc): state.rs:205,212
    pattern; cycles-livegraph-default-fastpath-1.md:158-173; stats-livegraph-1.md:233-251]. Keyed by the
    SHARED SQLite-free fingerprint `certificate_inputs_fingerprint` (partition {epoch/fresh/ts/hash/producer}
    ⊕ snapshot_uid ⊕ policy version) — NO new invalidation key [OBSERVED (doc): imports-livegraph-default-
    fastpath-1.md:25-44]. Lazily built once per fingerprint; the SQLite read survives ONLY (i) to BUILD the
    cert and (ii) on fallback (the drilldown invariant).

  GREEN iff ALL contributing (b) no-loss verdicts are GREEN at the current fingerprint, FOR THE FOCUS/LEAVES
    THIS CALL EMITS (a subset folds per command/focus):
      (1) cycles no-loss cert            — reuse build_and_store_cycles_cert (is_exact)        [EXISTS]
      (2) callers/callees no-loss compare (symbol focus only)                                 [EXISTS]
      (3) import no-loss cert (explain file focus) — reuse build_and_store_import_cert        [EXISTS]
      (4) identity-anchor cert (explain) — serve_identity ladder; multi-source {livegraph,sqlite}[EXISTS]
      (5) MODULE_SUMMARY structural-count cert (orient) — reuse the stats field-exact compare [needs DR-CLS-2]
      (6) resolved_calls count cert — LG Calls-edge count == SQLite count_edges_by_type(CALLS) [SPEC]
    AND precondition: every contributing partition resident + Fresh + TS-primary (non-TS ⇒ precondition unmet).

  IT IS AN AND-FOLD: the WEAKEST contributor decides (the MEET discipline the coherence root uses). A RED or
  missing contributor ⇒ BOUNDED-CLS cert RED ⇒ SQLite fallback for the (b) leaves.

  EXPLICITLY EXCLUDED from the fold [the two exclusions, distinct in kind]:
    · the (c) trust-core contributor — EXCLUDED PERMANENTLY (Clause 3). The (c) leaf is served Half-B
      SQLite-labelled OUTSIDE the cert; the cert never certifies it. This is the exclusion that lets the
      bounded cert be GREEN at all.
    · focus resolution — EXCLUDED because it has NO producer (§5) and runs UPSTREAM of the cert (it resolves
      the focus needed to evaluate the precondition). The bounded cert gates the (b) leaf VALUES; it CANNOT
      gate the focus-resolution `nodes` read. This exclusion is exactly what BOUNDS PREREQ-1 (DR-CLS-FOCUS).
```

```text
Serve-then-fallback control flow [INFERRED, mirroring cycles/imports/stats default fastpaths +
explain-sqlite-free-1 §5]:

  handle_<coherence>:
    1. PRECONDITION CHECK (SQLite-free): contributing partition(s) resident + Fresh + TS-primary?
         · orient REPO-focus: evaluable SQLite-free (no focus to resolve). → step 2.
         · focused orient + explain: the precondition needs a TARGET→PARTITION map, which needs focus
           resolution — and focus resolution reads `nodes` (no producer). So the precondition CANNOT be
           reached `nodes`-free until DR-CLS-FOCUS lands. Until then, focused orient + explain ALWAYS read
           `nodes` for resolution before any cert decision (the bound).
         NOT met → FALLBACK: the eager base read, wrapped in CoherenceEnvelope, every (b) leaf
           provenance.source = sqlite, fallback_reason ∈ {UnsupportedLanguage, Partial, Stale}.
    2. BOUNDED-CLS cert lookup at the current fingerprint:
         missing/stale → LAZY BUILD (reads SQLite ONCE per fingerprint via the per-contributor compares).
         RED  → FALLBACK (fallback_reason = the failing contributor: CallgraphDivergence / ImportDivergence /
                CycleDivergence / StatsDivergence / ResolvedCallsDivergence).
         GREEN → step 3.
    3. FASTPATH (GREEN): assemble the (b) leaves from the LiveGraph surfaces (§3 table), provenance.source =
         livegraph; the (c) trust leaf stays Half-B SQLite-labelled; the retained (d)/non-graph SQLite reads
         (Authority declarations, repos/snapshots, get_stale_files) UNCHANGED. Root MEET trust/freshness +
         set-UNION provenance exactly as the shipped CoherenceEnvelope (coherence-layer-1 §D7/§D8).
```

```text
CoherenceEnvelope labelling [OBSERVED (doc): coherence-layer-1.md:380-448 (the wrapper + D7 root + D8 SET
source), :498-520 (the safe-fallback contract)]:
  · each (b) LG-first leaf degrades INDEPENDENTLY: precondition-unmet / RED cert ⇒ that leaf's
    provenance.source flips to {sqlite} with fallback_reason set (the cert ladder). NEVER drop a leaf;
    NEVER mark a degraded leaf Exact (F1–F4).
  · the (c) leaves always carry source = {sqlite} (+ {declaration} for the downgrade-triggers leaf, D8),
    LABELLED as the OUTGOING extractor's snapshot-scoped model — NEVER claimed current-state (F5).
  · root.provenance.source = set-UNION of leaf sources (monotone); root.trust/freshness = MEET of leaves.
    A bounded-GREEN orient/explain therefore reports source = {livegraph, sqlite} (b from LiveGraph, c from
    SQLite) — HONEST about the two-source posture, never a false "all current-state" claim.
```

---

## 7. ACHIEVABLE SCOPE (honest) — what PREREQ-1 closes, per command/focus

```text
[INFERRED over the OBSERVED §3 inventory + the §5 verdict.] PREREQ-1 = "the (b) leaves served." Honest,
per command/focus:

  ┌─────────────────────┬──────────────────────────────────────────────┬───────────────────────────────────┐
  │ Command / focus      │ (b) leaves servable on green by PREREQ-1?     │ Eager `nodes`/`edges` read after  │
  ├─────────────────────┼──────────────────────────────────────────────┼───────────────────────────────────┤
  │ orient REPO          │ YES — cycles (row1), MODULE_SUMMARY counts    │ `nodes`: ELIMINATED on green      │
  │ (focus=None)         │ (row5, needs DR-CLS-2). resolved_calls (row6) │   (no focus res; counts→module_   │
  │                      │ entangled in (c). No focus resolution.        │   stats). `edges`: STAYS (trust-  │
  │                      │ ⇒ CLOSABLE for orient REPO-focus.             │   core (c) reads edges+unres.).   │
  ├─────────────────────┼──────────────────────────────────────────────┼───────────────────────────────────┤
  │ orient FOCUSED       │ leaf VALUES servable (cycles, callers/callees)│ `nodes`: STAYS (focus resolution  │
  │ (symbol/path/file)   │ BUT focus resolution reads `nodes` first.     │   reads `nodes`, no producer).    │
  │                      │ ⇒ GATED on DR-CLS-FOCUS.                      │   `edges`: STAYS ((c) + focus).   │
  ├─────────────────────┼──────────────────────────────────────────────┼───────────────────────────────────┤
  │ EVERY explain        │ leaf VALUES ALREADY served (82b6557: callers, │ `nodes`: STAYS (UNCONDITIONAL     │
  │ (target required)    │ callees, imports, cycles, identity anchor).   │   focus resolution, no producer). │
  │                      │ BUT focus resolution is UNCONDITIONAL.        │   `edges`: STAYS ((c) + focus).   │
  │                      │ ⇒ GATED on DR-CLS-FOCUS.                      │                                   │
  ├─────────────────────┼──────────────────────────────────────────────┼───────────────────────────────────┤
  │ trust (repo-wide)    │ resolved_calls (row6) only, ENTANGLED in the  │ `edges`/`unres.`: STAYS ((c)      │
  │                      │ Half-B resolution leaf with (c) call_         │   half + the entangled count).    │
  │                      │ resolution_rate. Half-A posture already served.│   `nodes`: trust reads no `nodes` │
  │                      │ ⇒ MARGINAL leaf-internal split; low value.    │   (no focus dispatch).            │
  └─────────────────────┴──────────────────────────────────────────────┴───────────────────────────────────┘

WHAT PREREQ-1 CLOSES (honest):
  · A real, shippable win: orient REPO-focus becomes `nodes`-FREE ON GREEN (MODULE_SUMMARY counts → module_
    stats via DR-CLS-2; cycles already LG-servable; no focus resolution). This is the cleanest PREREQ-1
    increment and the recommended FIRST impl scope.
  · explain's 5 leaf VALUES are already LiveGraph-served (82b6557); PREREQ-1 adds the bounded cert that GATES
    them no-loss (formalizing the existing serving), but does NOT make explain `nodes`-free.

WHAT PREREQ-1 LEAVES BOUNDED (honest):
  · explain + focused orient cannot be made `nodes`-free by serving the (b) leaves — focus resolution (a
    second producer gap, §5) reads `nodes` first. They are GATED on DR-CLS-FOCUS (build a focus-resolution
    producer first).
  · NO command can be `edges`/`unresolved_edges`-free: the (c) trust-core read is unconditional in orient +
    explain (Clause 3); trust's (c) half + the entangled resolved_calls keep the `edges` read.
```

---

## 8. What this does NOT achieve (the honest negative)

```text
[INFERRED, mirroring readiness-10 §191-202 + contract §4 Clause 1.]
  · PREREQ-1 flips NO deletion gate ALONE. Gate 1 stays RED for orient/explain because the (c) read + the
    focus-resolution `nodes` read + the (d) fallbacks + non-TS remain. PREREQ-1 is a MARGINAL eager-read
    reduction (orient REPO-focus `nodes`-free on green), valuable as cleanup + freshness, NOT a decommission
    step. It is PREREQ-1 — a step toward the bounded nodes/edges retirement — NOT the retirement.
  · It DROPS no table, runs no migration, flips no default. The eager SQLite read survives to BUILD the cert
    and on every fallback (the drilldown invariant).
  · It does NOT close PREREQ-2 (the covered-subset (d) fallback/cert handling) — a LATER slice. Dropping
    `nodes`/`edges` while a fallback or a cert-BUILD still reads them BREAKS the non-resident/stale/RED path.
  · It does NOT touch the (c) boundary (Clause 3 / Option A; FIXED) or the bounded retirement IMPL or P2
    non-TS — all out of scope (§11).
```

---

## 9. DECISION_REQUIRED — DR-CLS-2 (re-ratify MODULE_SUMMARY structural counts)

```text
DECISION_REQUIRED:
- ID: DR-CLS-2-MODULE-SUMMARY-COUNTS  [mirrors ORIENT-SQLITE-FREE-1 DR-2; re-ratify at the contract level]
  QUESTION: orient's MODULE_SUMMARY file/symbol/language counts read `nodes` (compute_repo_summary,
    agent_impl.rs:241) and were ratified SQLite-first (ORIENT-LIVEGRAPH-1 D-ORIENT-2: the module_candidates
    anchor + RISK-E module-identity divergence). `module_stats` (lib.rs:1376) proves the COUNT is LG-derivable
    and the stats field-exact cert (build_and_store_stats_cert) proves it no-loss. Re-source the file/symbol/
    language COUNTS to `module_stats` (cert-gated), keeping `discovered_module_count` anchored to
    module_candidates (Tier-B SQLite — NOT a nodes/edges read, harmless)? This re-opens a ratified SQLite-
    first decision → an architecture-boundary call the operator must make.
  OPTIONS (exhaustive; every cell filled):
  - Option A (RECOMMENDED) — RE-SOURCE-COUNTS-TO-MODULE-STATS, KEEP-DISCOVERED-COUNT-SQLITE.
      Serve file/symbol/language counts from module_stats (cert-gated via the stats field-exact compare,
      summed across module rows — a GREEN stats cert requires missing=0 ∧ extra=0 ∧ field-equal, so the
      summed counts are no-loss by construction). Keep discovered_module_count on module_candidates (Tier-B
      SQLite; it is NOT a nodes/edges read, so it does not block the decommission).
      CONSEQUENCE: removes the `nodes` count read on orient REPO-focus on green → the load-bearing enabler of
      orient REPO-focus being `nodes`-free (§7). Crosses the ratified SQLite-first posture → must be re-
      ratified. SUB-RISK TO RESOLVE IN IMPL (not a blocker): RISK-E module-IDENTITY divergence (trust-summary-
      livegraph-1 §4 — module_stats identities differ; not byte-equal without reconciliation) applies to the
      module-IDENTITY ROWS (the trust modules[] leaf), NOT to the repo-wide structural COUNTS this decision
      re-sources; the language-count axis must be confirmed present/derivable on module_stats (if absent,
      language counts stay SQLite-first and only file/symbol counts move). The cert is the gate: any
      divergence ⇒ RED ⇒ SQLite fallback, so no divergent count is ever served.
  - Option B — KEEP-SQLITE-FIRST. Leave MODULE_SUMMARY counts on `nodes`.
      CONSEQUENCE: an UNCONDITIONAL `nodes` read survives on orient REPO-focus → orient is NEVER `nodes`-free
      on green even with everything else served. DEFEATS the only clean PREREQ-1 win (orient REPO-focus). The
      stats slice already proved the LG count no-loss, so the SQLite-first posture is no longer justified by a
      correctness gap — only by inertia. NOT RECOMMENDED.
  RECOMMENDED: Option A. The stats slice (28ed216) already proved the LG count no-loss; the anchor
    (discovered_module_count) stays SQLite (Tier-B, harmless). It is the enabler of the one clean PREREQ-1
    increment. Re-ratifying a ratified decision is an architecture-boundary call the operator must make.
  BLOCKING_REASON: the count read is UNCONDITIONAL on orient REPO-focus; B defeats orient's `nodes`-free-on-
    green path (the cleanest PREREQ-1 deliverable). Re-sourcing crosses ORIENT-LIVEGRAPH-1 D-ORIENT-2 (a
    ratified SQLite-first posture). Per CLAUDE.md Decision Autonomy ("Contradiction with a ratified decision"
    + "data shape crossing a boundary" → stop and ask), it is surfaced here, not decided unilaterally.
```

---

## 10. DECISION_REQUIRED — DR-CLS-FOCUS (the focus-resolution gap bounds PREREQ-1)

```text
DECISION_REQUIRED:
- ID: DR-CLS-FOCUS-RESOLUTION-PRODUCER  [the load-bearing decision; mirrors how the trust-core producer gap
    gated the prior arc]
  QUESTION: focus resolution (focus string → IR symbol/file/module identity) has NO LiveGraph producer
    (§5b: the LiveGraph is keyed exclusively by CanonicalKey; no name/path/stable-key resolver surface;
    the IR has no qualified_name + no MODULE-node identity). FOCUSED orient (orient/mod.rs:66 → resolve_*_
    focus) and EVERY explain (explain/mod.rs:58,86 — target REQUIRED, resolve_path_focus UNCONDITIONAL)
    resolve their focus by reading `nodes` BEFORE any leaf / before the cert precondition. Serving the (b)
    leaf VALUES does NOT eliminate that read. How is PREREQ-1 scoped: ship it for orient REPO-focus only
    (the clean `nodes`-free-on-green win), or build a focus-resolution producer FIRST so focused orient +
    explain can also be `nodes`-free?
  OPTIONS (exhaustive; every cell filled):
  - Option A (RECOMMENDED) — SCOPE-PREREQ-1-TO-ORIENT-REPO-FOCUS; DEFER A FOCUS-RESOLUTION PRODUCER.
      Ship PREREQ-1 (the bounded cert + serve-then-fallback) for orient REPO-focus (`nodes`-free on green via
      DR-CLS-2) + formalize explain's already-served 5 leaf VALUES under the bounded cert (no-loss gating, but
      NOT `nodes`-free). Record explain + focused orient as BOUNDED-PENDING a focus-resolution producer.
      CONSEQUENCE: the smallest HONEST increment lands now; PREREQ-1 closes for orient REPO-focus and is
      recorded BOUNDED for the rest. The focus-resolution producer is a SEPARATE later slice (feasibility-
      probe-first, like SCIP-UNRESOLVED-CALL-PROBE-1). Matches the arc's S3/S1 discipline (smallest honest
      increment first; producer-program second). PREREQ-1 of the contract is thereby CLOSABLE-BOUNDED, which
      converts the bounded-decommission contract from PARKED to mechanically-ready FOR THE orient-REPO-focus
      subset.
  - Option B — BUILD-A-FOCUS-RESOLUTION-PRODUCER-FIRST (a prerequisite of PREREQ-1).
      Specify + build a LiveGraph focus→identity resolver (symbol-by-name over IR `name`; file-by-path over
      the FileInventory; stable_key→node over the shared key namespace) + reconcile the two sub-gaps (a
      derived MODULE-node identity model; the `qualified_name` display field), with a no-loss cert vs the
      SQLite focus resolution. ONE producer serves BOTH focused orient + explain (explain/mod.rs:85 reuses
      orient's resolution logic).
      CONSEQUENCE: unblocks explain + focused orient to be `nodes`-free on green — the FULL PREREQ-1. But it
      is a NEW PRODUCER + a NEW data-shape crossing a boundary (architecture-boundary), MUST be feasibility-
      probed first (the module-identity + qualified_name sub-gaps are unproven for parity), and is strictly
      more work than Option A. It front-loads the hardest part before banking the easy win.
  - Option C — ABANDON-THE-NODES-FREE-GOAL-FOR-COHERENCE; serve (b) VALUES from LiveGraph for freshness only.
      Serve the (b) leaf VALUES from the LiveGraph on green (freshness win) but accept that orient/explain
      ALWAYS read `nodes` (focus resolution + MODULE_SUMMARY if DR-CLS-2 not taken). Do NOT pursue a `nodes`-
      free path for any coherence command.
      CONSEQUENCE: PREREQ-1's read-elimination goal is dropped; the eager `nodes` read stays everywhere. The
      bounded nodes/edges retirement for the coherence subset becomes unreachable (the drop would strand the
      eager read). Defensible ONLY if the operator values freshness-labelling over the decommission entirely;
      it leaves PREREQ-1 unable to convert the contract from PARKED. NOT RECOMMENDED.
  RECOMMENDED: Option A. It banks the one clean, honest PREREQ-1 win (orient REPO-focus `nodes`-free on green)
    now, formalizes explain's existing serving under the bounded cert, and records the focus-resolution
    producer as the explicit prerequisite for the rest — the same "smallest honest increment, producer
    second" discipline the arc ratified (orient DR-0 → S3; explain DR-0 → S1). It does NOT front-load an
    unproven producer.
  BLOCKING_REASON: this fixes PREREQ-1's SCOPE (which commands/focuses PREREQ-1 covers) and whether a NEW
    focus-resolution producer (a producer + a data-shape crossing an architectural boundary) is in or out of
    PREREQ-1. Per CLAUDE.md Decision Autonomy ("a discovered mechanism that threatens a ratified invariant" +
    "data shape crossing a boundary" → stop and ask) and the packet STOP_CONDITION ("If focus resolution has
    NO LiveGraph producer, STOP and emit DECISION_REQUIRED ... orient-repo-focus-only vs build a focus-
    resolution producer first"), it is surfaced here, not decided unilaterally. It blocks any
    COHERENCE-LEAF-SERVE-IMPL scoping.
```

---

## 11. Scope boundary (what this spec does NOT touch)

```text
[INFERRED, mirroring contract §2 + the packet FILES_OUT_OF_SCOPE.] In scope: this SPEC (PREREQ-1). Out of
scope, each a SEPARATE later track:
  · PREREQ-2 — the covered-subset (d) fallback/cert handling (the 6 drilldowns' fallback paths + the
    imports/cycles/stats cert BUILDs made SQLite-free or removed). A LATER slice. Dropping nodes/edges while a
    fallback or cert-build still reads them strands the non-resident/stale/RED path.
  · The bounded RETIREMENT IMPL — the actual drop/retire of the (a)∪(b)-covered nodes/edges reads. Gated on
    PREREQ-1 + PREREQ-2 for the covered subset; the global drop additionally needs gate 2 (non-TS).
  · P2 non-TS coverage — the structural-ceiling program (gate 2 / class (d)). Multi-slice, months.
  · The (c) trust unresolved-call boundary — FIXED by Contract Clause 3 / Option A (§4). NOT re-opened.
  · A focus-resolution producer IMPL — surfaced as DR-CLS-FOCUS (§10); if ratified (Option B), it is its own
    feasibility-probe-then-producer slice, NOT this spec.
  · ROADMAP.md / CURRENT_SLICE.md reconciliation — out of scope (read-only here).
  · ANY code, deletion, migration, default flip — spec-first; src/**, rust/**, scripts/** out of scope.
```

---

## 12. Validation plan (for the IMPL; SPEC'D here, NOT RUN)

```text
[INFERRED, mirroring the drilldown fastpaths' live validation + the coherence parity discipline.] When
COHERENCE-LEAF-SERVE-IMPL runs (a later slice), it must EXECUTE:

  V1 PARITY (green-compare on the (b) leaves): for each (b) leaf (§3), an `--engine compare`-style harness
     proving the LiveGraph leaf VALUE is byte/field-equal to the SQLite leaf on a GREEN-cert repo (cycles
     set-equal; callers/callees key-set-equal; imports byte-equal; identity anchor name/subtype-equal;
     MODULE_SUMMARY counts field-equal [DR-CLS-2]; resolved_calls count-equal). RED on any divergence.
  V2 NO-EAGER-(b)-READ PROOF: a storage spy / panicking-closure over the (b) SQLite readers
     (compute_repo_summary for MODULE_SUMMARY; find_module_cycles* for cycles; count_edges_by_type for
     resolved_calls), proving that on a GREEN cert the (b) read does NOT fire (orient REPO-focus: NO `nodes`
     read at all). Mirrors imports/cycles fastpath validation (no per-call find_imports / find_cycles).
  V3 (c) STAYS LABELLED: assert the trust unresolved-call leaf carries provenance.source = {sqlite} on a
     GREEN-cert orient/explain/trust — NEVER {livegraph}. The cert never certifies (c) (§6). This REPLACES a
     parity cert for (c) (no parity is achievable — Clause 3 / probe 0≠3).
  V4 FALLBACK CORRECTNESS: non-resident / non-TS / stale / RED-cert ⇒ the (b) leaf falls back to the byte-
     identical SQLite read with provenance.source = {sqlite} + fallback_reason set; the answer is UNCHANGED
     from today's eager path. NEVER drop a leaf; NEVER mark a degraded leaf Exact (F1–F4).
  V5 FOCUS-RESOLUTION BOUND (if Option A): assert that focused orient + explain STILL read `nodes` for focus
     resolution (the recorded bound) — i.e. PREREQ-1 did NOT silently claim them `nodes`-free. Honest-bound
     regression guard.

SCOPE-BOUNDARY of the validation: PREREQ-1 ONLY. It does NOT validate the nodes/edges DROP (that is the
retirement IMPL), PREREQ-2 fallbacks, or non-TS. The validation proves the (b) leaves are served no-loss with
honest fallback + the (c) label, NOT that any table is droppable.
```

---

## 13. Validation / evidence ledger (this slice)

```text
EXECUTED (command run, output observed first-hand THIS turn):
- ls docs/slices/coherence-leaf-serve-1.md (pre-write) → "No such file or directory" — the deliverable did
  not pre-exist; this slice CREATES it.
- git status --short (pre-write) → empty (clean tree). Clean baseline; the only change is this new spec doc.
- git log --oneline -3 → HEAD `f9cfe23` (SQLITE-RAW-DECOMMISSION-1 contract) ← `78feb81` (readiness-10) ←
  `7d4b3bb` (probe NO-GO). Confirms this spec sits above the ratified contract.
- grep over rust/crates/repo-graph-livegraph/src/lib.rs for `by_name|find_by|resolve_name|resolve_focus|
  resolve_symbol|qualified_name|stable_key` → EMPTY. Confirms NO focus-string resolver surface (§5b E1).
- grep over rust/crates/repo-graph-ir/src/lib.rs for `qualified_name` → EMPTY; the CanonicalKey + AstFileScope
  + IrNode reads → no MODULE node, no qualified_name (§5b E2).

OBSERVED (source/doc read first-hand THIS turn — the grounding for every clause):
- rust/crates/storage/src/agent_impl.rs:366/437/800/834/1037/1070 — the focus-resolution functions; every one
  reads `nodes` (joining files/edges). FIRST-HAND.
- rust/crates/agent/src/orient/mod.rs:57-66, 76-215 — orient focus dispatch (None → orient_repo, NO
  resolution; Some → orient_focused → resolve_*_focus). FIRST-HAND.
- rust/crates/agent/src/explain/mod.rs:55-196 — run_explain(target: &str REQUIRED); resolve_path_focus
  UNCONDITIONAL at :86 ("reusing orient's resolution logic"). FIRST-HAND.
- rust/crates/repo-graph-livegraph/src/lib.rs:233-242 (LiveGraph fields — slots keyed by id), :246-255
  (contribution — defines/ref_counts keyed by CanonicalKey string), :469-551 (callers — target matched
  against defines/ref_counts), :1031-1064 (node_location/node_display — take &CanonicalKey), :1077-1088
  (FileInventory from AstFileScope keys), :1317/:1376/:1675 (module_import_cycles/module_stats/
  live_import_view). FIRST-HAND.
- rust/crates/repo-graph-ir/src/lib.rs:28 (CanonicalKey(String)), :60-65 (AstFileScope — no SCIP identity),
  :338-360 (IrNode — name/subtype/range, NO qualified_name), :383-419 (PartitionIr::node/outgoing/incoming
  by key). FIRST-HAND.
- docs/slices/sqlite-raw-decommission-1.md — §4 rows 6-10 (the (b) table), §6 Clause 3 (the (c) floor),
  §9 PREREQ-1/PREREQ-2.
- docs/slices/sqlite-raw-decommission-readiness-10.md:180-227 — the (a)-(d) partition; §191-202 the (b)
  bullets; §253-271 the eager-read baseline.
- docs/slices/orient-sqlite-free-1.md:161-169 (5-path map), :202 (focus-resolution No-LG-producer, DR-4),
  :218-230 (CLASS 1/2/3), :259-286 (the composite cert + RED-by-construction), :446-459 (DR-2).
- docs/slices/explain-sqlite-free-1.md:58-68 (PRODUCER UPDATE — DR-E1 refuted), :184-193/:205-217 (the reads
  + the 5 served leaves), :338-407 (the composite cert + flow), :550-567 (DR-E2 — UNCONDITIONAL, no producer).
- docs/slices/trust-livegraph-1.md:22-28/:325-401/:415-441 (Half-A/Half-B + the hybrid envelope), :299-300
  (NO focus dispatch). docs/slices/trust-summary-livegraph-1.md:298-310 (the 8-field table; 1 LG-derivable).
- docs/slices/key-namespace-repo-relative-1.md:14-22 — stable_key == CanonicalKey namespace (repo-relative).
- docs/slices/{cycles,imports,stats}-livegraph-*fastpath*.md + coherence-layer-1.md:380-632 — the cert-
  fastpath routing + the CoherenceEnvelope<T> shape + D8 multi-source leaf.
- docs/VISION.md (Orientation over Perfection; Fact Certainty Model) + CLAUDE.md (Decision Autonomy;
  Persistence Completeness) + agent_docs/validation.md (Evidence Law).

NOT RUN (skipped, with reason):
- Build / test (cargo) + ./scripts/dev-install-local.sh — spec-first; no source path touched; dev-install
  restarts the daemon (state-mutating; scripts/** out of scope).
- Live `rmap orient/explain/trust` capture — the daemon is not running (`rmap` 0.2.1 installed; queries need a
  daemon; starting it runs index/refresh, state-mutating). NO-DAEMON posture, the SAME stance the four arc
  specs + the contract took. Every structural claim is a first-hand read of source or a committed doc; no
  clause depends on a live capture.
```

---

## 14. Guardrails honored

```text
No code. No deletion. No migration. No decommission. No default flip. No new ratified priority invented (the
next BUILD — Option A vs B of DR-CLS-FOCUS, and DR-CLS-2 — is left an OPEN governance call). Spec doc only.
The contract + readiness-10 + the arc specs are read-only (already committed). First-hand source reads back
every load-bearing OBSERVED claim (the focus-resolution verdict is verified first-hand, not assumed — packet
requirement). The (c) boundary is stated as FIXED (Clause 3 / Option A), NOT re-opened (STOP_CONDITION-3).
The two genuine open architecture-boundary decisions (DR-CLS-2 re-ratify; DR-CLS-FOCUS the focus-resolution
gap) are surfaced as DECISION_REQUIRED with exhaustive matrices, not decided unilaterally.

STOP-condition check (packet):
  · STOP_CONDITION-1 (focus resolution has NO LiveGraph producer → emit DECISION_REQUIRED, orient-repo-focus-
    only vs build a producer first): TRIGGERED + HONORED — the verdict is decisive (§5), surfaced as
    DR-CLS-FOCUS (§10) with the exact matrix axis. The spec does NOT work around it (no silent focus-
    resolution producer; no assumption of servability).
  · STOP_CONDITION-2 (a (b) leaf classed LG-derivable is actually not): NOT triggered — the §3 rows are
    exactly readiness-10's (b) bullets; the focus-resolution read is NOT classed (b) (it is the second gap).
  · STOP_CONDITION-3 (closing PREREQ-1 would require touching the (c) boundary or the retirement impl): NOT
    triggered — the (c) boundary is stated FIXED (§4); the retirement impl is out of scope (§11); PREREQ-1 is
    spec-only and touches neither.
```

---

## 15. References
- `docs/slices/sqlite-raw-decommission-1.md` (`f9cfe23`) — the contract; §4 rows 6-10 (the (b) table); §6 Clause 3 (the (c) floor); §9 PREREQ-1/PREREQ-2 (the gate this spec closes-bounded)
- `docs/slices/sqlite-raw-decommission-readiness-10.md` — the (a)-(d) partition (§180-227); the (b) bullets (§191-202); the eager-read baseline (§253-271)
- `docs/slices/orient-sqlite-free-1.md` (`e10a455`) — the 5-path map; DR-2 (MODULE_SUMMARY counts); DR-4 (focus-resolution No-LG-producer); the composite ORIENT cert (§4b)
- `docs/slices/explain-sqlite-free-1.md` (`f3237f9`) — DR-E1 (shared trust-core, refuted); DR-E2 (focus resolution UNCONDITIONAL, no producer); the 5 served leaves; the composite EXPLAIN cert
- `docs/slices/trust-livegraph-1.md` (`dc55114`) — Half-A (LG posture) + Half-B (SQLite-labelled diagnostics); the hybrid envelope; trust has NO focus dispatch
- `docs/slices/trust-summary-livegraph-1.md` (`94fc506`) — the 8-field AgentTrustSummary table (1 LG-derivable: resolved_calls; 7 NEEDS-EXTENSION)
- `docs/slices/scip-unresolved-call-probe-1.md` (`7d4b3bb`) — the probe NO-GO that fixed the (c) floor (Option A); the model this spec mirrors for the focus-resolution gap (probe-first)
- `docs/slices/key-namespace-repo-relative-1.md` (`b72b075`) — stable_key == CanonicalKey namespace (repo-relative), the basis for a sound key→node lookup
- `docs/slices/coherence-layer-1.md` (`6ed17b8` + `5129f44`) — the `CoherenceEnvelope<T>` contract; D7 root; D8 multi-source-leaf SET provenance; the safe-fallback contract
- `docs/slices/{cycles,imports,stats}-livegraph-{default-fastpath,1}.md` — the cert-fastpath routing (build/green-serve/fallback) + the shared `certificate_inputs_fingerprint` this spec's bounded cert mirrors
- `rust/crates/storage/src/agent_impl.rs:366/437/800/834` — the focus-resolution functions (every one reads `nodes`)
- `rust/crates/agent/src/{orient,explain}/mod.rs` — the focus dispatch (orient None-exempt; explain unconditional)
- `rust/crates/repo-graph-livegraph/src/lib.rs` + `rust/crates/repo-graph-ir/src/lib.rs` — the LiveGraph surfaces (CanonicalKey-keyed; no resolver) + the IR shape (no qualified_name, no MODULE node)
- `docs/VISION.md` (Orientation over Perfection; Fact Certainty Model) + `CLAUDE.md` (Decision Autonomy; Persistence Completeness)
