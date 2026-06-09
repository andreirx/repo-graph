# TRUST-LIVEGRAPH-1: apply the coherence contract + ratified hybrid to `rmap trust`

Slice ID: TRUST-LIVEGRAPH-1
Status: **DESIGN / SPEC-FIRST — NOT IMPLEMENTED — DECISION-COMPLETE.** This document SPECIFIES the FOURTH and
FINAL per-command application of the ratified COHERENCE-LAYER-1 contract (orient → check → explain → trust).
It produces NO source code, NO table deletion, NO schema/data migration, NO default flip. The implementation
is a LATER slice and depends on COHERENCE-ENVELOPE-1 (the support module) + EXPLAIN-LIVEGRAPH-1 landing first
(contract slice sequence). **No open DECISION_REQUIRED remains.** trust's defining boundary decision —
TRUST-DISPOSITION = **hybrid labelled model** — was RATIFIED at the contract level (operator sign-off
2026-06-08, coherence-layer-1.md D2); this slice APPLIES it and does NOT re-open it. Every realization choice
below is implied by the ratified wrapper (D1), the ratified hybrid (D2), the multi-source-leaf amendment (D8),
and the orient/check/explain precedents — each recorded as **decide-and-record** with an exhaustive matrix
where a genuine choice existed (CLAUDE.md §Decision Autonomy).

**COMPLETE OUTPUT ENUMERATION (completeness is load-bearing — the selection packet's first requirement):**
`rmap trust` produces THREE distinct output surfaces, ALL enumerated FIRST-HAND below from the trust command
code: (1) the daemon/API envelope — the full `TrustReport` DTO, EVERY field with file:line (§1a); (2) the
human-render content — `TrustResponse::render_human` (§1b); (3) the CLI PROCESS wrapper — stdout/stderr + the
exit code a shell/CI caller observes (§1c). §1d pins the negative space (what trust does NOT emit). Each output
is then assigned a hybrid source posture (current-state LiveGraph posture vs residual SQLite diagnostic, §2).

**THE RATIFIED HYBRID, RESTATED (coherence-layer-1.md D2 + trust row + envelope spec):** `rmap trust` becomes
a TWO-HALF report under ONE `CoherenceEnvelope`:
  - **Half A — current-state reliability posture (source = `livegraph`).** A projection of the LiveGraph's
    EXISTING per-answer posture (partition residency / per-partition freshness / contributing-language maturity
    / producer availability / migrated-answer capability) into the AnswerEnvelope vocabulary. NEW. It does NOT
    recompute the v1 reliability levels over the LiveGraph — that is Option B (the full rebase), explicitly
    DEFERRED by the contract. (The anti-Option-B guard: §D-TRUST-2.)
  - **Half B — residual extraction diagnostics (source = `sqlite`).** The ENTIRE existing `TrustReport`
    (resolution counts, reliability axes, downgrade triggers, categories, classifications, blast radius,
    enrichment, modules, diagnostics meta) — RETAINED verbatim, each axis re-typed to a `CoherenceEnvelope`
    leaf LABELLED as describing the OUTGOING homegrown extractor's snapshot-scoped unresolved-edge model. NOT
    dropped, NEVER presented as current-state.
Each leaf carries an explicit source + freshness label; the root folds by MEET. The two halves are DISJOINT
(joined only by co-location in one report, never two computations of the same fact — contract Q7-4).

Goal: specify how `rmap trust` serves the CURRENT per-answer reliability posture (LiveGraph-derived) PLUS the
residual SQLite trust-core diagnostics, wrapped in `CoherenceEnvelope<T>`, with explicit per-line
source/freshness labels and honest degradation — WITHOUT minting the false-completeness the Fact Certainty
Model forbids (presenting the outgoing-extractor reliability report as if it described the current-state graph,
contract F5).

Track: Stage D, SQLITE-RAW-DECOMMISSION path — fourth and final per-command coherence build.

Authoritative contract (RATIFIED, read FIRST): `docs/slices/coherence-layer-1.md` (committed + amended
2026-06-09, D8). This slice REUSES that contract's `CoherenceEnvelope<T> { value, provenance, trust,
freshness }` wrapper, its trust source map (the trust row), the ratified TRUST-DISPOSITION = hybrid (D2), the
MEET fold (D3), the per-signal-provenance rule (D4), the authority-overlay rule (D5), the multi-source LEAF
provenance amendment (D8 — `Provenance.source: BTreeSet<Source>`), and the safe-fallback ladder (the TRUST
row). It does NOT re-open COHERENCE-ENVELOPE-SHAPE (RATIFIED = Option B wrapper) or TRUST-DISPOSITION
(RATIFIED = hybrid — Option C, NOT the freeze-v1 Option A, NOT the full-rebase Option B).

Precedent (followed for SHAPE, reused — NOT re-derived): `docs/slices/orient-livegraph-1.md` (the first
per-command application — ratified `CoherenceEnvelope<T>`, the `CoherentOrientResult` container D7, the root
MEET D-ORIENT-4, the zero-leaf posture), `docs/slices/check-livegraph-1.md` (the first MULTI-SOURCE composite
leaf D-CHECK-1/D8, the CLI process-wrapper enumeration §1f, the MEET-freshness-not-fastpath posture), and
`docs/slices/explain-livegraph-1.md` (the heaviest aggregator, the D8 multi-source `{livegraph, sqlite}` /
`{declaration, sqlite}` leaves, the EXPLAIN_TRUST v1-trust-core section, the trust-overlay/`trust_briefing`
correction). This doc mirrors their structure (current-outputs enumeration → source map → envelope wiring →
degradation → validation → scope → forced-decision matrices → risks → references → evidence log). trust is the
ONLY command whose change is CONCEPTUAL, not a re-projection of a migrated drilldown answer — so it does NOT
reuse `CoherentOrientResult` (trust returns the bespoke `TrustReport`, not `OrientResult`), and it is the LONG
POLE the contract sequenced last (coherence-layer-1.md slice sequence + critical-path note).

Depends (precedent, reused — NOT re-derived here):
- COHERENCE-LAYER-1 — the ratified mixed-source contract (envelope shape D1, hybrid disposition D2, trust
  source map, MEET D3, multi-source leaf D8, fallback ladder).
- COHERENCE-ENVELOPE-1 — the SUPPORT module that realizes `CoherenceEnvelope<T>` + the MEET fold + the
  `BTreeSet<Source>` provenance (D8) + the FreshnessInfo→FreshnessState reconciliation. **MUST land before this
  slice's implementation** (architecture.md §Build Order: support module → feature). trust is the FIRST command
  to put a `livegraph`-sourced leaf BESIDE an `sqlite`-sourced leaf describing a DISJOINT fact (the hybrid).
- EXPLAIN-LIVEGRAPH-1 — the contract sequences TRUST after EXPLAIN ("explain after the pattern is de-risked on
  orient … trust last"). trust MUST NOT land before the wrapper is proven on the three OrientResult commands.
- TRUST-MODEL-REBASE-1 (`repo-graph-trust-model`) — the per-ANSWER `AnswerEnvelope` trust vocabulary
  (AnswerClass / FreshnessState / DegradationReason / LanguageSupport / QueryCompleteness) that Half A projects.
- LIVEGRAPH-RUNTIME-1 / QUERY-MIGRATION-1 / LIVEGRAPH-INTEGRATION-1B/1C — the LiveGraph runtime state Half A
  READS (partition residency + `missing_partitions`; per-partition epoch/freshness; D1 `contributing_languages`
  union; the `ProducerUnavailable` failure class; the cert fingerprint). **Half A reads existing runtime state;
  it introduces NO new producer** (contract D6; §D-TRUST-2).
- ORIENT-BUG-1 — anchored module COUNTS to SQLite `module_candidates`; trust's module rows keep the v1 framing
  (Half B, source=sqlite), so the same anchoring holds (§D-TRUST inherits RISK-E).

## Spec-first note (read first)
```text
This is a SPECIFICATION. Per the repo evidence law (CLAUDE.md §Evidence Law; agent_docs/validation.md), every
claim is labelled OBSERVED or INFERRED.
  OBSERVED [first-hand, this turn] = reads I performed this turn, with file:line:
      rust/crates/trust/src/types.rs (the full TrustReport DTO + every sub-DTO; the skip_serializing /
        serde(skip) fields)
      rust/crates/trust/src/service.rs (assemble_trust_report — the 8 storage reads; compute_trust_report —
        the 8 phases; build_caveats; the edges_resolved==edges_total mapping; compute_blast_radius_and_enrichment)
      rust/crates/trust/src/rules.rs (the 4 downgrade detectors + the 4 reliability formulas + thresholds)
      rust/crates/trust/src/storage_port.rs (the TrustStorageRead trait — the 8 narrow read methods)
      rust/crates/daemon-runtime/src/dispatch.rs:2825-2920 (handle_trust — NO LiveGraph branch, NO post-serialize
        trust overlay injection; the snapshot gate; display_name inject; the error arms) vs the orient/explain
        overlay injection (:2647 / :2811) — the contrast proving trust has NO `trust_briefing` analogue
      rust/crates/rgr/src/presentation/trust.rs (TrustResponse human renderer — the deserialized SUBSET + the 8
        rendered sections + the fields deserialized-but-not-rendered + the fields not-deserialized)
      rust/crates/rgr/src/commands/trust.rs (run_trust — the CLI PROCESS wrapper: arg parse, cwd/canonicalize,
        daemon connect, --json vs human, the exit-code arms — SUCCESS is ALWAYS 0, no verdict-derived exit)
  OBSERVED [via contract / precedent, first-hand THERE] = facts the ratified contract or the orient/check/explain
      slices read first-hand and cited with file:line; reused here without re-reading (e.g. the AnswerEnvelope
      axis vocabulary repo-graph-trust-model/src/lib.rs; the LiveGraph surface offsets livegraph/src/lib.rs;
      the cert fingerprint livegraph_feed.rs; storage-architecture-v2 Tier model; the Signal.freshness RISK-G
      point). Labelled inline.
  INFERRED = my design judgment over those OBSERVED facts (the CoherenceEnvelope wiring, the Half-A posture
      field set, the per-leaf source mapping, the MEET freshness rules, the degradation mapping, the validation
      plan), grounded in the ratified contract + the ratified hybrid (D2) + the precedents.
Spine claims I PERSONALLY verified this turn are marked [OBSERVED, first-hand].

NO live `rmap` graph orientation was run: the daemon socket is absent. [EXECUTED this turn: `rmap trust` ->
"error: daemon connection failed: socket does not exist:
/Users/apple/Library/Application Support/repo-graph/daemon.sock". The exit code was NOT captured at runtime
(masked by a pipe); the source maps DaemonClient::new() failure -> ExitCode::from(2), commands/trust.rs:78-80
— OBSERVED in source, not at runtime.] A spec-only slice does not start the daemon or run the index/trust
sequence (that mutates state). Orientation was grounded in first-hand source reads — the stronger evidence
basis for a contract about code structure. The socket-absent result is itself recorded below as trust's
transport-level degradation path (§4 TRANSPORT-LEVEL DEGRADATION), identical to orient/check/explain.

INCIDENTAL OBSERVATION (recorded for honesty per the evidence law; NOT reconciled here — pre-existing behaviour,
out of scope; the trust IMPLEMENTATION is a later slice): in compute_trust_report the summary field
`edges_resolved` is assigned `diagnostics.edges_total` — the SAME value as `edges_total` (service.rs:420-424,
both map `|d| d.edges_total`). So `edges_resolved == edges_total` ALWAYS, and the human renderer's
"Edges: N% resolved" line therefore always shows 100% (presentation/trust.rs:218-226). This is a latent
quirk/likely-bug in the v1 report; it is a Half-B (residual SQLite diagnostics) datum and stays byte-identical
under this slice (the hybrid retains Half B verbatim). Reconciling it is out of scope (RISK-T-F).
```

## Why now (priority path)
```text
[OBSERVED: docs/slices/coherence-layer-1.md §"Proposed follow-up slice sequence" + CURRENT_SLICE.md STATUS
banner + docs/ROADMAP.md §Current Priority.] COHERENCE-LAYER-1 is RATIFIED (operator sign-off 2026-06-08;
amended 2026-06-09 D8). Its slice sequence is ORIENT → CHECK → EXPLAIN → TRUST. orient, check, explain are
DECISION-COMPLETE; trust is the LAST per-command build and the contract's named LONG POLE: "the hybrid is a
larger output contract and its source-split logic is unique … the heaviest/most-novel case (trust) last"
(coherence-layer-1.md critical-path note). It is "the only command whose CHANGE is conceptual, not a
re-projection" (contract trust row).

[OBSERVED, first-hand: dispatch.rs handle_trust:2825-2920. The handler calls assemble_trust_report over
`repo_state.storage` (the SQLite StorageConnection, :2881-2882); the in-memory LiveGraph on RepoState is NOT
consulted; there is NO LiveGraph branch in the handler body, and NO post-serialize `trust` overlay injection
(contrast orient :2647, explain :2811).] => trust today is 100% SQLite + Authority with NO served LiveGraph
path. It is the LAST of the four SQLite-eager coherence defaults and a precondition for
SQLITE-RAW-DECOMMISSION-1: the raw `nodes`/`edges`/`unresolved_edges` + `snapshots.extraction_diagnostics_json`
substrate cannot be decommissioned while trust reads it eagerly on every call.

HONEST DECOMMISSION CAVEAT (from the contract's slice sequence): the ratified hybrid RETAINS the
unresolved-edge / extraction-diagnostics tables (Half B still REPORTS them, labelled), so TRUST-LIVEGRAPH-1
does NOT by itself unblock their decommission. COHERENCE-READINESS-RECOMPUTE-1 must record them as STILL
load-bearing (§6).
```

---

## 1. What `rmap trust` returns today (OBSERVED, first-hand)

trust is a **meta-report**, not a query and not an aggregator fan-out. The daemon handler [OBSERVED, first-hand:
dispatch.rs handle_trust:2825-2920] resolves the repo (with display_name, :2830-2834), acquires a read lock
(:2839), fetches the latest snapshot (:2844) — erroring if absent (`SnapshotNotFound` "no snapshot found",
:2846-2851) or not `ready` (:2861-2869) — emits a heartbeat (:2872), calls `assemble_trust_report` over the
SQLite `StorageConnection` (:2881-2887), injects `display_name` (:2899), and serializes the `TrustReport`
DIRECTLY (:2913-2914). **It injects NOTHING else** — NO `trust` overlay key (the orient/explain artifact), NO
`OrientResult` envelope. trust IS the full report (§1d).

`assemble_trust_report` [OBSERVED service.rs:591-705] performs EIGHT SQLite reads through the `TrustStorageRead`
port [OBSERVED storage_port.rs:142-190], parses the diagnostics + toolchain JSON, and delegates to the PURE
`compute_trust_report` [OBSERVED service.rs:210-457], which runs 8 phases (detection rules → reliability
formulas → category rows → classification rows → blast-radius+enrichment → module rows → caveats → resolution
rate). **Every input is SQLite or Authority; LiveGraph contribution = NONE.**

### 1a. The daemon/API envelope — the full `TrustReport` DTO (OBSERVED, first-hand: trust/src/types.rs:240-300; service.rs:408-456)

Legend for Source today: **SQLite-diag** = the snapshot `extraction_diagnostics_json` blob (outgoing-extractor
artifact). **SQLite-edges/nodes** = the raw graph. **SQLite-unres** = the `unresolved_edges` table.
**SQLite-mods** = `nodes`/`edges`/`module_candidates`. **Authority** = `declarations` table. **A2** =
operational SQLite (snapshots). **derived** = computed in `compute_trust_report` from the above.

| Field (OBSERVED types.rs) | Built by (OBSERVED file:line) | Backing read (OBSERVED) | Source today | Layer |
|---|---|---|---|---|
| `snapshot_uid` | service.rs:409 | `get_latest_snapshot` (handler:2844) | A2 | A2 op |
| `display_name` (skip_if_none) | daemon inject dispatch.rs:2899 | registry alias / path basename | daemon op metadata | — |
| `basis_commit` | service.rs:411 | `snapshot.basis_commit` (handler:2885) | A2 | A2 op |
| `toolchain` (object|null) | service.rs:412 | `snapshot.toolchain_json` (handler:2886) | provenance version | A2 op |
| `diagnostics_version` | service.rs:413 | diagnostics blob | SQLite-diag | 1 |
| `diagnostics_available` (bool) | service.rs:211/454 | diagnostics blob present? | SQLite-diag presence flag | 1 |
| `summary.edges_total` | service.rs:415-419 | diagnostics blob | SQLite-diag | 1 |
| `summary.edges_resolved` (**== edges_total**, QUIRK) | service.rs:420-424 | diagnostics blob | SQLite-diag | 1 |
| `summary.unresolved_total` | service.rs:425-429 | diagnostics blob | SQLite-diag | 1 |
| `summary.resolved_calls` | service.rs:430 | `count_edges_by_type(CALLS)` (assemble:644) | SQLite-edges | 1 |
| `summary.unresolved_calls` | service.rs:431 (sum CALLS-family) | diagnostics breakdown (rules.rs:314) | SQLite-diag | 1 |
| `summary.unresolved_calls_external` | service.rs:432 | `count_unresolved_edges_by_classification` calls-filter, ExternalLibraryCandidate (assemble:655) | SQLite-unres | 1 |
| `summary.unresolved_calls_internal_like` | service.rs:433 (external subtracted) | derived | derived | 1 |
| `summary.call_resolution_rate` | service.rs:398-405 | derived (resolved/(resolved+internal_like)) | derived | 1 |
| `summary.reliability.import_graph` {level,reasons} | rules.rs:171 (compute_import_graph_reliability) | alias-suspicion + registry + unresolved_imports | derived (inferred) | 1→2 |
| `summary.reliability.call_graph` {level,reasons} | rules.rs:204 (Variant A; <0.5 LOW, <0.85 MEDIUM) | resolved_calls + internal_like | derived (inferred) | 1→2 |
| `summary.reliability.dead_code` {level,reasons} | rules.rs:238 | — | **NOT serialized** (`skip_serializing`, types.rs:140-141); internal only | — |
| `summary.reliability.change_impact` {level,reasons} | rules.rs:275 (inherits import_graph) | — | derived (inferred) | 1→2 |
| `summary.triggered_downgrades.framework_heavy_suspicion` | rules.rs:77 | `get_file_paths_by_repo` (assemble:628) | SQLite (files) | 2-3 |
| `summary.triggered_downgrades.registry_pattern_suspicion` | rules.rs:98 | `find_path_prefix_module_cycles` (assemble:636) | SQLite-nodes/edges | 2-3 |
| `summary.triggered_downgrades.missing_entrypoint_declarations` | rules.rs:132 | `count_active_declarations(entrypoint)` (assemble:640) | **Authority** (declarations) | 4 |
| `summary.triggered_downgrades.alias_resolution_suspicion` | rules.rs:149 | `compute_module_stats` suspicious count (assemble:632) | SQLite-mods | 2 |
| `categories[]` {category,label,unresolved} | service.rs:291-308 | diagnostics breakdown | SQLite-diag | 1 |
| `classifications[]` {classification,count} | service.rs:311-336 | `count_unresolved_edges_by_classification` all (assemble:665) | SQLite-unres | 1 |
| `unknown_calls_blast_radius` {low,medium,high}? | service.rs:348-353/468-536 | `query_unresolved_edges`(unknown,100k) (assemble:677) + derive_blast_radius | SQLite-unres | 1-2 |
| `enrichment_status` {eligible,enriched,top_types[]}? | service.rs:348-353/551-571 | unknown samples `metadata_json` enrichment markers | SQLite-unres (enrichment artifact) | 1-2 |
| `modules[]` {module_stable_key,qualified_name,fan_in,fan_out,file_count,suspicious_zero_connectivity,trust_notes} | service.rs:362-386 | `compute_module_stats` (assemble:632) | SQLite-mods | 1 |
| `caveats[]` | service.rs:155-199 | reliability levels + diagnostics_available + permanent cycle caveat | derived | 1 |
| `enrichment_eligible_count` | service.rs:455 | unknown samples | **NOT serialized** (`serde(skip)`, types.rs:298-299); internal disambiguator | — |

**The 8 storage reads (ALL SQLite, OBSERVED assemble_trust_report service.rs:591-705 + storage_port.rs:142-190):**
`get_snapshot_extraction_diagnostics` (snapshots blob, :600), `get_file_paths_by_repo` (files, :628),
`compute_module_stats` (nodes/edges/module_candidates, :632), `find_path_prefix_module_cycles` (nodes/edges,
:636), `count_active_declarations(entrypoint)` (**declarations = Authority**, :640), `count_edges_by_type(CALLS)`
(edges, :644), `count_unresolved_edges_by_classification` (unresolved_edges; calls-filter :655 + all :665),
`query_unresolved_edges(unknown,100k)` (unresolved_edges, :677). PLUS `snapshot.basis_commit` +
`snapshot.toolchain_json` from `get_latest_snapshot` in the handler (:2885-2886).

### 1b. The human CLI renderer surface (OBSERVED, first-hand: rgr/src/presentation/trust.rs)

The daemon returns the `TrustReport` JSON (§1a). The CLI human renderer is `TrustResponse::render_human`
[OBSERVED presentation/trust.rs:139-198]. It deserializes a SUBSET of the report into `TrustResponse`
[trust.rs:46-62] and emits the sections below — this is the COMPLETE human surface `rmap trust` produces today.
(Enumerated first-hand because the packet requires EVERY current output surface, and the human render is
distinct from the API envelope.)

| Rendered section (what the user sees) | Built from (OBSERVED file:line) | Condition | Source posture today |
|---|---|---|---|
| `Trust Report: <display_name \| snapshot_uid>` | render_human:144-148 (`display_name` else `snapshot_uid`) | always | daemon op / A2 |
| `Snapshot: <uid truncated to 17+...>` | render_human:149 (truncate_uid:407-413) | always | A2 |
| `Resolution` heading + `Calls: N% resolved (R of T)` + `Edges: N% resolved (R of T)` | render_resolution:200-229 | always | SQLite-edges/diag (Edges line is the 100% quirk, §spec-first note) |
| `Reliability` heading + `Call-graph: LEVEL (humanized reasons)` + `Import-graph: …` + `Change-impact: …` | render_reliability:231-240; format_axis:354-361; humanize_reason:364-405 | always (3 axes; **NO dead-code axis** — matches skip_serializing) | derived (inferred) |
| `Unresolved Breakdown` heading + `N <label>` bullets | render_unresolved_breakdown:242-259 | ONLY non-zero categories; section omitted if empty | SQLite-diag |
| `Classification` heading + `N <classification>` bullets | render_classification:261-278 | ONLY non-zero; omitted if empty | SQLite-unres |
| `Suspicious Modules (zero connectivity)` heading + `<qualified_name>` bullets (take 10 + `... (N more)`) | render_suspicious_modules:280-299 | ONLY `suspicious_zero_connectivity` rows; omitted if none | SQLite-mods |
| `Triggered Downgrades` heading + `<trigger>: <reason>` bullets | render_downgrades:301-351 | ONLY the triggered ones (of 4); omitted if none | mixed: SQLite + Authority (missing_entrypoint) |
| `Caveats` heading + bullets | render_human:189-195 | ONLY when `caveats` non-empty | derived |

**NOT deserialized by the human renderer (JSON-`--json`-only outputs — OBSERVED `TrustResponse` trust.rs:46-62):**
`toolchain`, `diagnostics_version`, `unknown_calls_blast_radius`, `enrichment_status`, `diagnostics_available`.
These appear in `--json` but NEVER in the human render. **Deserialized-but-not-rendered:** `basis_commit`,
`summary.call_resolution_rate`, `summary.unresolved_total`, `summary.unresolved_calls_external`,
`summary.unresolved_calls_internal_like`, and the module `module_stable_key`/`fan_in`/`fan_out`/`file_count`/
`trust_notes` (only `qualified_name` of suspicious rows is printed). [OBSERVED: render_* methods read only the
fields above.] CONSEQUENCE FOR THIS SLICE (§3e): under the wrapper these reads move under `value`; the SECTION
TEXT stays byte-identical (Half B retained verbatim), and the human surface GAINS the source/freshness labels +
the Half-A current-state posture section (the visible hybrid).

### 1c. The CLI process-wrapper surface (`run_trust`) — stdout/stderr + EXIT CODE (OBSERVED, first-hand: rgr/src/commands/trust.rs:35-130)

`rmap trust` is also a PROCESS: `run_trust` parses args, resolves cwd, connects to the daemon, calls
`client.request("trust", {repo})`, and maps the outcome to stdout/stderr + an exit code. This is a DISTINCT
surface from the envelope content (it is what a shell/CI caller observes). **The load-bearing contrast with
check: trust's SUCCESS exit code is ALWAYS 0 (`ExitCode::SUCCESS`) — it is NOT derived from any verdict
signal** [OBSERVED trust.rs:95/107]. So trust is NOT CI-gating like check; it is like orient. Exit 1 = usage
only; exit 2 = every runtime/transport/daemon failure.

| Wrapper output (OBSERVED trust.rs:line) | Channel | Exit | Source posture today → effect of this slice |
|---|---|---|---|
| `--json` sets json_mode; only `--json` accepted (:39-43) | — | — | static arg parser (local). Unchanged. |
| unknown `--flag` → `error: unknown flag: {flag}` + usage (:44-48) | stderr | **1** | static parser; NO daemon. Unchanged. |
| unexpected positional → `error: unexpected argument: {other}` + usage (:49-53) | stderr | **1** | static parser; NO daemon. Unchanged. |
| `current_dir()` fails → `error: cannot get current directory: {e}` (:58-63) | stderr | **2** | process env (local). Unchanged. |
| `canonicalize()` fails → `error: cannot canonicalize current directory: {e}` (:66-72) | stderr | **2** | filesystem (local). Unchanged. |
| `DaemonClient::new()` fails → `error: {e}` (:75-81) | stderr | **2** | transport connect (the socket-absent path EXECUTED this turn, §4). Unchanged. |
| `--json` success → `to_string_pretty(&result)` to stdout (:90-96) | stdout | **0** | the daemon envelope VERBATIM. Under this slice prints the FULL `CoherenceEnvelope<CoherentTrustReport>` (was the bare TrustReport). |
| `--json` serialize error → `error: failed to serialize result: {e}` (:97-101) | stderr | **2** | local serializer. Unchanged. |
| human success → `from_value::<TrustResponse>(result)` then `render_human()` to stdout (:104-108) | stdout | **0** | renderer projection of the envelope (§1b). Under this slice `TrustResponse` projects `value` (§3e). |
| human parse/render error → `error: failed to parse trust response: {e}` (:109-113) | stderr | **2** | local deserializer/renderer over the envelope. Unchanged. |
| `DaemonError{code="RepoNotFound"}` → `error: repo not indexed` + `hint: run 'rmap index .'` (:116-124) | stderr | **2** | daemon registry. Unchanged. |
| `DaemonError{code,message}` (other) → `error: {code}: {message}` (:121) | stderr | **2** | daemon error (incl. the handler's `SnapshotNotFound` "no snapshot found"/"not ready" and `InternalError` assembly failure, §1d). Unchanged. |
| `Err(e)` (Send/Read/InvalidResponse/Timeout) → `error: {e}` (:125-128) | stderr | **2** | transport (post-connect). Unchanged. |

EXIT-CODE SEMANTICS (trust's CLI contract, OBSERVED trust.rs — preserve verbatim): success = 0 ALWAYS (both
modes); usage error = 1; ALL runtime/transport/daemon errors = 2. **There is NO verdict-derived exit code**, so
the wrapper change carries NO silent-CI-breakage hazard analogous to check's (check derived 0/1/2 from the
verdict signal; trust does not). The ONLY wrapper-forced change is the human deserialization shift to `value`
(§3e) — if missed, the human render breaks (parse error → exit 2), a LOUD failure, not a silent green→red flip.

### 1d. What `rmap trust` does NOT emit (the negative space — load-bearing for completeness)

ALL verified first-hand, to forbid falsely attributing sibling-command surfaces to trust:
- **NO `OrientResult` envelope.** trust returns the bespoke `TrustReport` (types.rs:240-300), NOT the
  `OrientResult`/`CoherentOrientResult` shared by orient/check/explain. => trust does NOT reuse
  `CoherentOrientResult`; its coherent container is a SEPARATE `CoherentTrustReport` (§3b, D-TRUST-1).
- **NO daemon `trust` overlay key / NO `trust_briefing`.** `handle_trust` (dispatch.rs:2913-2914) serializes the
  report and returns; it does NOT call `compute_trust_overlay_for_snapshot` and does NOT inject a post-serialize
  `trust` key. CONTRAST `handle_orient` (:2647) and `handle_explain` (:2811), which DO. The overlay
  (`TrustOverlaySummary`, util/trust.rs) is a SMALLER, DIFFERENT object that orient/explain attach BESIDE their
  envelope; trust IS the full report and has no such overlay. => D-ORIENT-6 / D-EXPLAIN-TRUST-BRIEFING are
  STRUCTURALLY ABSENT here (§D-TRUST-1 note). [OBSERVED, first-hand.]
- **NO LiveGraph read today.** The handler passes `repo_state.storage` (SQLite) to `assemble_trust_report`
  (:2881-2882); the RepoState LiveGraph is not consulted. trust has ZERO LG-first leaves TODAY (Half A is the
  NEW addition this slice specifies, NOT a today-output). [OBSERVED, first-hand.]
- **`dead_code` reliability axis is computed but NOT serialized** (`skip_serializing`, types.rs:140-141;
  build_caveats elides its caveat, service.rs:175-178). It stays internal — surfacing it would re-introduce the
  withdrawn `rmap dead` vocabulary (VISION §"Dead-code surface withdrawal"). (§D-TRUST-5.)
- **`enrichment_eligible_count` is NOT serialized** (`serde(skip)`, types.rs:298-299) — an internal
  disambiguator for the agent storage adapter; not part of the wire contract.
- **NO focus dispatch.** trust is repo-wide ALWAYS (no file/path/symbol pipeline). => no callers/callees
  summary, no symbol-focus posture; D-ORIENT-SYMBOL-CALLGRAPH is structurally absent.
- **NO `signals[]`/`limits[]`/`next[]`/`documentation`/`confidence`.** Those are OrientResult fields; trust's
  shape is the `TrustReport` axes (§1a). trust has no `confidence` field and no FS doc scan.
- **Handler-level pre-envelope errors (NOT envelope content):** no-snapshot → `SnapshotNotFound` "no snapshot
  found" (:2846-2851); snapshot not `ready` → `SnapshotNotFound` "latest snapshot is not ready (status: …)"
  (:2861-2869); assembly failure → `InternalError` (:2889-2894); serialize failure → `InternalError`
  (:2915-2917). These surface to the CLI as DaemonError → exit 2 (§1c). The snapshot gate is PRESERVED (§D-TRUST-6).

Net: **trust's complete current output = the full `TrustReport` (§1a) serialized directly; the human-render
SUBSET (§1b); and the process-wrapper stdout/stderr + always-0-success exit (§1c). Nothing is LiveGraph-served;
there is no overlay, no OrientResult, no focus, no verdict-derived exit.**

---

## 2. Per-output source map — the hybrid field-level boundary (the trust row, made first-hand-precise)

Legend (per COHERENCE-LAYER-1 §source map + the ratified hybrid D2): **Half A / LG-posture** = the NEW
current-state reliability posture leaf(s), source = `livegraph`, freshness = LiveGraph current freshness
(§3c). **Half B / residual** = the RETAINED v1 axis, source = `sqlite`, LABELLED as the OUTGOING-extractor
snapshot-scoped model. **Authority** = a Tier-A1 `declarations` read that contributes a `declaration` source to
a multi-source leaf (D8). Layer = Fact Certainty Model layer (architecture.md §Product Layer Stack).

This table REFINES the contract's trust row (coherence-layer-1.md trust source map) with first-hand DTO-field
granularity. No posture here contradicts the contract.

| Output (§1a) | Layer | Target posture | Source set (D8) | Notes |
|---|---|---|---|---|
| `snapshot_uid` / `display_name` / `basis_commit` / `toolchain` (identity & provenance meta) | A2 | **Half B / residual (identity)** | `{sqlite}` (display_name = daemon op) | Operational identity for the snapshot the diagnostics describe; not rebuildable structure. |
| `diagnostics_version` / `diagnostics_available` / `summary.edges_*` / `unresolved_total` | 1 | **Half B / residual** | `{sqlite}` | The outgoing-extractor diagnostics blob; LABELLED outgoing. `edges_resolved==edges_total` quirk retained verbatim (RISK-T-F). |
| `summary.resolved_calls` | 1 | **Half B / residual** | `{sqlite}` | `count_edges_by_type(CALLS)` over the snapshot edges. NOT the LiveGraph callgraph (that is Half A's capability posture, not a count re-derivation here). |
| `summary.unresolved_calls(_external/_internal_like)` / `call_resolution_rate` | 1 | **Half B / residual** | `{sqlite}` | The Variant-A reweighted v1 resolution metric. Describes the OUTGOING extractor's unresolved edges; under SCIP "unresolved edge" changes meaning (RISK-T-D). |
| `summary.reliability.{import_graph,call_graph,change_impact}` | 1→2 | **Half B / residual** | `{sqlite}` | The v1 reliability LEVELS. NOT recomputed over the LiveGraph (that is Option B, deferred — §D-TRUST-2). |
| `summary.reliability.dead_code` | — | **NOT surfaced** (internal) | — | `skip_serializing`; stays internal (§D-TRUST-5). |
| `summary.triggered_downgrades` (4 triggers) | 2-4 | **Half B / residual — MULTI-SOURCE** | `{sqlite, declaration}` | `missing_entrypoint_declarations` reads `count_active_declarations(entrypoint)` — the `declarations` Authority table (assemble:640) — on EVERY report. So the downgrade-triggers leaf carries `{sqlite, declaration}` (D8; the trust analogue of check's verdict leaf). The Authority read happens unconditionally, even when the trigger is not fired. |
| `categories[]` / `classifications[]` / `unknown_calls_blast_radius` / `enrichment_status` | 1-2 | **Half B / residual** | `{sqlite}` | The unresolved-edge classification + blast-radius + enrichment surface — entirely the outgoing-extractor model. |
| `modules[]` (degree + suspicion + trust_notes) | 1 | **Half B / residual** | `{sqlite}` | Module degree is LG-derivable in principle (the LiveGraph `module_stats`), but the trust FRAMING (suspicious-zero-connectivity, alias-resolution notes) is the v1 model → kept SQLite-first. RISK-E module-identity divergence forbids a naive LG swap (RISK-T-H). |
| `caveats[]` | 1 | **Half B / residual** | `{sqlite}` | Derived from the v1 reliability levels + diagnostics-availability. |
| **`current_state_posture` (NEW)** | 1 | **Half A / LG-posture** | `{livegraph}` | The NEW current-state reliability posture: partition residency / per-partition freshness / contributing-language maturity / producer availability / migrated-answer capability. A PROJECTION of EXISTING LiveGraph runtime state into the AnswerEnvelope vocabulary — NOT a recomputed reliability score (§D-TRUST-2). source = `livegraph`; freshness = LiveGraph current freshness. Degrades to Unavailable/Partial (never empty-as-known-zero) when the LiveGraph is cold/non-resident/non-TS/producer-absent (§4). |

**Net for trust: ONE NEW LG-posture surface (Half A, `current_state_posture`, source=`livegraph`) added BESIDE
the ENTIRE retained v1 report (Half B, source=`sqlite`, one multi-source downgrade leaf `{sqlite, declaration}`).**
trust is the ONLY coherence command whose new surface is a CONCEPTUAL addition rather than a re-projection of a
migrated drilldown answer (contract trust row). [INFERRED mapping from the OBSERVED §1 outputs onto the OBSERVED
contract postures + the ratified hybrid D2; no posture diverges from the contract.]

---

## 3. CoherenceEnvelope<T> wiring for trust (INFERRED, grounded in the RATIFIED contract + hybrid D2)

Per COHERENCE-LAYER-1 §"The shared coherence answer-envelope" (RATIFIED Option B wrapper) + the trust envelope
spec ("the full `TrustReport` is retained, and EACH reported axis is wrapped as a `CoherenceEnvelope` leaf …
source=livegraph for the current-state per-answer posture, source=sqlite for the residual outgoing-extractor
diagnostics"), the wrapper is applied COMPOSITIONALLY at two granularities. trust is the LAST command to
instantiate it and the ONLY one that wraps a NON-OrientResult value.

### 3a. Leaf — `CoherenceEnvelope<AxisPayload>` (one per reported axis)

```text
The leaf granularity is the AXIS/SECTION, not the scalar (wrapping every `edges_total` would be absurd and is
not the contract's "each reported axis"). Each axis payload stays PRISTINE (the Option-B principle — the
existing TrustReport sub-values are NOT widened); provenance/trust/freshness ride in the wrapper SIBLING fields.

HALF B — RESIDUAL DIAGNOSTICS LEAVES (source=sqlite; the v1 axes, RETAINED verbatim, LABELLED outgoing-extractor):
  - resolution leaf        = the edge/call counts + call_resolution_rate (summary.* numeric). source={sqlite}.
  - reliability leaf       = the 3 serialized reliability axes (import_graph/call_graph/change_impact), pristine.
                             source={sqlite}. (dead_code stays internal, NOT a leaf — D-TRUST-5.)
  - downgrade-triggers leaf= the 4 triggers, pristine. source={sqlite, declaration} (MULTI-SOURCE, D8 — the
                             missing_entrypoint trigger reads the declarations Authority table on every report,
                             assemble:640). The trust analogue of check's verdict leaf (D-CHECK-1/D8).
  - categories leaf        = the unresolved breakdown. source={sqlite}.
  - classifications leaf   = the classifier-bucket counts. source={sqlite}.
  - blast-radius leaf      = unknown_calls_blast_radius. source={sqlite}.
  - enrichment leaf        = enrichment_status. source={sqlite}.
  - modules leaf           = the per-module trust rows (degree + suspicion). source={sqlite}.
  - diagnostics-meta leaf  = diagnostics_version / diagnostics_available / edges_total / unresolved_total.
                             source={sqlite}.
  Every Half-B leaf's TrustPosture/freshness describes the SNAPSHOT (the outgoing extractor's epoch): freshness
  = Fresh for the current index, Stale when the snapshot lags the worktree, Unavailable/Unknown when the
  diagnostics blob is absent (diagnostics_available=false → the diagnostics-meta + dependent leaves are
  Unavailable/Unknown + the existing "Extraction diagnostics unavailable" caveat — NOT Fresh zeros, §4 F3).
  CRITICAL (F5): a Half-B leaf is NEVER source=livegraph and NEVER labelled current-state. Its TrustPosture
  must carry a DegradationReason / label naming it the OUTGOING-extractor snapshot-scoped model.

HALF A — CURRENT-STATE POSTURE LEAF (source=livegraph; the NEW per-answer reliability posture):
  - current-state-posture leaf = a COMPOSITE leaf (the trust analogue of check's composite verdict leaf)
    carrying the LiveGraph posture payload:
        partition residency        (resident vs missing partitions — LIVEGRAPH-RUNTIME-1 residency/missing_partitions)
        per-partition freshness     (Fresh/Stale/PrecisionPending/RefreshFailed/Unavailable — the epoch model)
        contributing-language set   (LanguageSupport union — TS mature, Rust beta-with-caveats, etc.; QUERY-MIGRATION-1 D1)
        producer availability       (SCIP producer present? → ProducerUnavailable; LIVEGRAPH-INTEGRATION-1C D6)
        migrated-answer capability  (can the LiveGraph serve Exact for callers/callees/imports/cycles/stats at
                                     the current fingerprint? — the livegraph_feed cert state)
    value     = the posture payload (a NEW small DTO; reads existing runtime state, NO new producer — D-TRUST-2).
    provenance.source = {livegraph}.
    trust (TrustPosture) = the AnswerEnvelope posture (class/completeness/degradation_reasons/contributing_languages),
                 folded by MEET over the resident partitions. NEVER Exact when a contributing partition is
                 non-resident/non-TS/PrecisionPending (invariants I1/I2/I6).
    freshness = the MEET of the resident partitions' freshness (the LiveGraph current freshness).
  HARD GUARD (anti-Option-B, D-TRUST-2): this leaf reports a POSTURE in the AnswerEnvelope vocabulary; it does
  NOT recompute import/call/change reliability LEVELS over the LiveGraph. Producing repo-wide LiveGraph
  reliability levels is Option B (the full rebase), DEFERRED by the contract; doing it here would re-open the
  ratified TRUST-DISPOSITION and invent the deferred producer (forbidden, §6).

Leaf construction MUST delegate to (or mirror) the AnswerEnvelope smart constructors so the six invariants hold
AT EVERY LEAF (contract §invariant preservation I1-I6). The downgrade-triggers leaf and the current-state
posture leaf are the two that fold MANY sub-facts into one leaf posture; those internal folds are MEETs and
MUST be monotone (they can only LOWER class/freshness/completeness).
```

### 3b. Root — `CoherenceEnvelope<CoherentTrustReport>` (per command)

```text
trust does NOT reuse CoherentOrientResult (it returns TrustReport, not OrientResult — §1d). Its coherent
container is a NEW `CoherentTrustReport` (D-TRUST-1), the trust analogue of orient's D7 CoherentOrientResult:

  CoherentTrustReport {
    // identity / operational meta — kept as container fields (source described per §2), pristine:
    snapshot_uid, display_name, basis_commit, toolchain,
    // HALF B — the v1 report axes, EACH re-typed to a residual leaf (source=sqlite), payloads pristine:
    diagnostics:       CoherenceEnvelope<DiagnosticsMeta>,     // diagnostics_version/available/edges/unresolved_total
    resolution:        CoherenceEnvelope<ResolutionCounts>,
    reliability:       CoherenceEnvelope<TrustReliabilitySerialized>,  // 3 axes (dead_code stays internal)
    triggered_downgrades: CoherenceEnvelope<TrustDowngrades>,  // source={sqlite, declaration} (D8)
    categories:        CoherenceEnvelope<Vec<TrustCategoryRow>>,
    classifications:   CoherenceEnvelope<Vec<TrustClassificationRow>>,
    unknown_calls_blast_radius: CoherenceEnvelope<Option<UnknownCallsBlastRadiusBreakdown>>,
    enrichment_status: CoherenceEnvelope<Option<EnrichmentStatus>>,
    modules:           CoherenceEnvelope<Vec<ModuleTrustRow>>,
    caveats:           CoherenceEnvelope<Vec<String>>,
    // HALF A — the NEW current-state posture leaf (source=livegraph):
    current_state_posture: CoherenceEnvelope<LiveGraphPosture>,
  }

  root.value      = CoherentTrustReport (above).
  root.provenance = { source: SET-UNION of all leaf sources = {livegraph, sqlite, declaration} (monotone union
                      fold, root ⊇ every leaf — D8); basis aggregated; fallback_reason: only Half-B leaves can
                      carry a cert fallback IF a future slice makes a Half-B axis LG-first (none today, so
                      fallback_reason is null today); missing_partitions aggregated from the Half-A posture leaf }
  root.trust      = the MEET fold of ALL leaf TrustPostures — BOTH halves (contract D3, monotone GLB).
  root.freshness  = the MEET of (the Half-A LiveGraph current freshness) AND (the Half-B snapshot freshness).
                    => a report whose LiveGraph half is Fresh but whose SQLite snapshot half is Stale reports
                    root.freshness = Stale (RISK-T-A epoch skew handled by the MEET; cannot read Fresh-overall).

  The pristine v1 axis VALUES (Half B) stay byte-identical to today's TrustReport (the hybrid RETAINS them); the
  CONTAINER shape changes (axes re-typed to leaves + the Half-A leaf added) and the wrapper gains the honest
  source/freshness labels by design (NOT a byte-identity goal — contract RISK-F).

  NO ZERO-LEAF ROOT. trust ALWAYS emits at least the full Half-B leaf set (the report is always computable from
  SQLite once a ready snapshot exists; the snapshot gate guarantees this — §D-TRUST-6) PLUS the Half-A posture
  leaf (Unavailable-labelled if the LiveGraph is cold). orient's empty-fold-to-TOP hazard does not arise.
```

### 3c. The two-axis hybrid model (current-state posture × residual diagnostics)

```text
trust today has ONE axis: the v1 repo-wide reliability assessment (a snapshot-scoped, outgoing-extractor
artifact). The coherence hybrid ADDS an ORTHOGONAL current-state posture axis (LiveGraph-derived), so trust now
reports BOTH, each with its own source + freshness, NEVER conflated:

  freshness mapping:
    - Half B (residual diagnostics):  Fresh when the snapshot is the current index (no stale files); Stale when
                                      get_stale_files-equivalent staleness applies / the snapshot lags; Unavailable
                                      / Unknown when diagnostics_available=false (the blob is absent).
    - Half A (current-state posture): the MEET of resident-partition freshness; PrecisionPending while a
                                      refresh is mid-flight; RefreshFailed / Unavailable when the producer is
                                      absent or the LiveGraph is cold for the repo's partitions.
    - root: the MEET of the two (RISK-T-A).

  WHY independent (the contract's "trust must itself state which axes are LiveGraph-current vs snapshot-scoped
  extraction artifacts", coherence-layer-1.md Q3): the v1 reliability describes the OUTGOING extractor's
  unresolved edges on a snapshot; the current-state posture describes the INCOMING LiveGraph's per-answer
  reliability right now. They can disagree (a Fresh LiveGraph posture over a Stale v1 snapshot, or vice-versa).
  Labelling them separately is the WHOLE of trust's coherence contribution and the explicit defence against F5
  (the trust-specific false-completeness: presenting the v1 SQLite report as if it described the current-state
  LiveGraph resolution). No fastpath, no cert toggle on the v1 axes — just honest two-source labelling.
```

### 3d. Reconciliation points implied by adopting the wrapper (recorded; resolution belongs to COHERENCE-ENVELOPE-1 / the impl)

```text
RP-T1 (the LiveGraph posture read API). Half A reads the LiveGraph's CURRENT runtime state (residency / epochs /
  languages / producer flag / cert readiness). The LiveGraph today serves PER-QUERY AnswerEnvelopes; a repo-wide
  "posture" read aggregates state it ALREADY holds. That aggregation method is a LOCAL mechanism (a read over
  existing in-memory state, no new persisted fact, no new producer — contract D6). It belongs to
  COHERENCE-ENVELOPE-1 / the trust impl (support module → feature, architecture.md §Build Order); it is NOT a
  new architectural boundary (it crosses no new module edge, inverts no dependency). Recorded, not re-decided.
RP-T2 (FreshnessInfo reconciliation, = contract RISK-G). The shared Signal DTO carries
  `freshness: Option<FreshnessInfo>` (artifact_contracts vocabulary) — but trust returns TrustReport, NOT Signal,
  so the Signal.freshness reconciliation does NOT directly apply to trust's leaves. trust's leaves carry the
  trust-model FreshnessState directly. The single FreshnessInfo→FreshnessState mapping remains COHERENCE-ENVELOPE-1's
  (orient D-ORIENT-7); trust's only dependency on it is via COHERENCE-ENVELOPE-1 owning the FreshnessState enum.
  Recorded, contract-deferred, not re-decided here.
RP-T3 (the home crate for CoherentTrustReport + LiveGraphPosture). Extend repo-graph-trust-model vs a new
  repo-graph-coherence crate is the SAME small boundary call the contract DEFERRED to COHERENCE-ENVELOPE-1
  (coherence-layer-1.md envelope spec). Not re-opened here. NOTE: LiveGraphPosture must NOT live in
  repo-graph-ir (the contract's "NO serde in repo-graph-ir" + the dep-direction rule); it is a coherence-layer
  DTO. Recorded.
```

### 3e. CLI-wrapper human-render remap under the wrapper (INFERRED, forced by the ratified shape — milder than check)

```text
The wrapper is daemon-INTERNAL (it re-shapes what handle_trust serializes). run_trust reads that serialized
shape in TWO places (§1c). Adopting the wrapper FORCES a mechanical remap — NOT a new decision (the ratified
`value`-nesting, contract D7-analogue). UNLIKE check, trust derives NO exit code from the body, so there is NO
silent-CI-breakage hazard; the only forced change is the human deserialization.

  --json path (trust.rs:90-96): prints `to_string_pretty(&result)` VERBATIM → automatically prints the full
    `CoherenceEnvelope<CoherentTrustReport>`. No code change needed beyond the daemon emitting the wrapper.
  human path (trust.rs:104-108): today `from_value::<TrustResponse>(result)` over the bare TrustReport. After:
    EITHER TrustResponse deserializes the CoherentTrustReport carried under `value` (reading each axis leaf's
    inner `.value` for the section payloads), OR run_trust unwraps `result["value"]` before from_value. Either
    realization is acceptable (local detail — decide-and-record). The render CONTENT stays byte-identical for the
    Half-B sections (§1b) and GAINS: (a) per-section source/freshness labels, and (b) a NEW "Current-State
    Posture" section rendering the Half-A leaf (residency / freshness / languages / producer / capability),
    clearly distinguished from the "(snapshot-scoped, outgoing extractor)"-labelled Half-B sections. The exit
    codes are UNCHANGED (success 0 / usage 1 / runtime 2 — §1c); a missed remap fails LOUD (human parse error →
    exit 2), not silently.
```

---

## 4. Degradation / safe-fallback behaviour for trust (honest labelling, no false completeness)

```text
trust's degradation is about honest two-source labelling, NOT a cert ladder on its v1 axes (those have no
LiveGraph alternative to fall back FROM — they ARE the SQLite source of truth). Classes:

HALF B (residual diagnostics) — the v1 report is ALWAYS computable from a ready snapshot, so it is always
  PRESENT, but honestly labelled:
  - DIAGNOSTICS BLOB ABSENT (diagnostics_available=false, OBSERVED service.rs:211/454): today the code emits
    edges_total=0 / unresolved_total=0 + the caveat "Extraction diagnostics unavailable for this snapshot.
    Re-index to populate." (service.rs:163-167). Under the wrapper the diagnostics-meta + dependent residual
    leaves are labelled Unavailable / completeness=Unknown (NOT Fresh zeros) — the F3 guard ("null=unknown,
    empty=known-zero"; architecture.md Rule 6). The zero is "unknown", not "known-zero edges".
  - STALE SNAPSHOT: the Half-B leaves carry freshness=Stale (the snapshot lags the worktree). The v1 reliability
    is reported as a Stale assessment, never a Fresh current-state claim.
  - F5 (the trust-specific false-completeness, contract): every Half-B leaf is source=sqlite and LABELLED as the
    OUTGOING-extractor snapshot-scoped unresolved-edge model. It is NEVER presented as describing the
    current-state LiveGraph resolution. This is the load-bearing defence and the reason the hybrid was ratified.

HALF A (current-state posture) — degrades INDEPENDENTLY of Half B:
  - LIVEGRAPH COLD / NON-RESIDENT for the repo's partitions: the posture leaf is Unavailable / Partial with a
    DegradationReason + missing_partitions; freshness=Unavailable. It does NOT report an empty/Exact posture
    ("Unavailable is not empty", contract F3 / Q8b). It NEVER claims a current-state Exact when no partition is
    resident.
  - PRODUCER ABSENT (no SCIP producer, LIVEGRAPH-INTEGRATION-1C ProducerUnavailable): the posture leaf reports
    ProducerUnavailable / RefreshFailed — the current state cannot be (re)established — while Half B is still
    served (the snapshot diagnostics persist independently). The two halves degrade on independent epochs.
  - NON-TS partitions (D6: no non-TS LiveGraph support beyond the existing per-crate Rust beta): the posture
    leaf reports the contributing-language maturity honestly (Partial + LanguageSupport reason); it does NOT
    fabricate a TS-grade posture for non-TS code.
  - REFRESH MID-FLIGHT: PrecisionPending; the posture is served from the last-good epoch, labelled — never
    Exact while pending (invariant I6).

EPOCH SKEW (RISK-T-A): the root freshness is the MEET of the two halves. A Fresh LiveGraph posture over a Stale
  v1 snapshot → root Stale; a stale LiveGraph (cold) over a Fresh snapshot → root degraded. The MEET is monotone
  — it cannot manufacture a Fresh-overall report from a stale half.

MULTI-SOURCE AUTHORITY (D8): the downgrade-triggers leaf reads the declarations Authority table (entrypoint
  count) on every report; it carries source={sqlite, declaration}. The Authority read is NOT conditioned on the
  trigger firing — the `declaration` source is present even when missing_entrypoint_declarations.triggered=false
  (it was consulted and returned a count). This is the trust analogue of check's D-V1b.

TRANSPORT-LEVEL DEGRADATION (OBSERVED, first-hand, distinct from the envelope's internal labelling):
  [EXECUTED this turn: `rmap trust` with the daemon down → "error: daemon connection failed: socket does not
  exist: /Users/apple/Library/Application Support/repo-graph/daemon.sock".] When the socket is absent the CLI
  NEVER reaches handle_trust: it returns a CONNECTION ERROR and NO envelope at all (DaemonClient::new failure →
  exit 2, trust.rs:75-81). This is honest failure (a transport error, not a false-complete answer) and is
  OUTSIDE the CoherenceEnvelope's scope — the envelope models the daemon-INTERNAL source labelling, not
  client↔daemon transport. IMPLICATION FOR VALIDATION: trust's coherence degradation is exercised daemon-side
  (trust-crate / agent / daemon-runtime tests with a live RepoState + LiveGraph), NOT through a socketless CLI
  (identical posture to orient/check/explain §4).

NO FALSE-COMPLETENESS, enumerated against the contract's F-list:
  F1/F4 (Exact over non-resident / SCIP-refresh-pending): forbidden for the Half-A posture leaf (it is never
    Exact unless every contributing partition is resident + Fresh; I1/I6).
  F2 (confidence/freshness Fresh over a stale/pending input): forbidden — root freshness is the MEET (§3b).
  F3 (empty as known-zero): forbidden — diagnostics-absent → Unavailable/Unknown, cold-LiveGraph → Unavailable;
    neither is a Fresh zero.
  F5 (v1 report presented as current-state): forbidden — the disjoint source labels (Half B always sqlite,
    Half A always livegraph) + the explicit outgoing-extractor labelling. THE trust-specific guard.
  F6 (Authority overlay erasing computed fact): N/A in trust's direction — trust READS the Authority entrypoint
    COUNT as an input to a downgrade trigger; it does not overlay/suppress a computed structural fact. The
    count's Authority origin is preserved in the leaf source set {sqlite, declaration} (D8), never hidden.
```

---

## 5. Validation plan (for the eventual implementation)

```text
Off-target first (architecture.md §Off-Target Testability + §Build Order). The wrapper type, the MEET fold, the
BTreeSet<Source> provenance, and the FreshnessState live in COHERENCE-ENVELOPE-1 (pure, unit-tested there);
this slice validates the TRUST WIRING. The trust crate's existing matrices MUST stay green UNCHANGED (the v1
report logic is NOT touched — only the surrounding wrapper + the NEW Half-A posture are added): service.rs tests
(compute/assemble, ~30 tests), rules.rs tests (formulas/triggers/thresholds, ~40 tests), types.rs tests
(serde/skip-fields). The trust crate's PARITY tests (rust/crates/trust/tests/parity.rs) MUST stay green.

PARITY (no discovery loss vs today's SQLite trust):
  P1. The Half-B residual axis VALUE payloads (summary counts, the 3 reliability axes + reasons, the 4
      downgrade triggers + reasons, categories, classifications, blast radius, enrichment, modules, caveats,
      diagnostics meta) are BYTE-IDENTICAL to today's TrustReport — the hybrid RETAINS the v1 report. Only the
      surrounding wrapper gains labels + the Half-A leaf is added. (Reuse orient/check P1.)
  P2. The dead_code reliability axis stays INTERNAL (skip_serializing) — NOT a leaf, NOT in either half (D-TRUST-5).
      Pin that no `dead_code` key appears in the wrapped output.
  P3. enrichment_eligible_count stays serde(skip) — not in the wire contract.
  P4. Sort orders preserved (categories desc-then-asc service.rs:304-308; classifications service.rs:332-336;
      modules by qualified_name service.rs:386; enrichment top_types truncate(15) service.rs:561-566).

HYBRID-LABEL CORRECTNESS (the trust-specific core — the F5 guard made testable):
  H1. EVERY Half-B leaf carries provenance.source ⊆ {sqlite, declaration} and NEVER `livegraph`; its
      TrustPosture is LABELLED as the outgoing-extractor snapshot-scoped model.
  H2. The current_state_posture leaf carries provenance.source = {livegraph} and is the ONLY `livegraph`-sourced
      leaf.
  H3. The downgrade-triggers leaf carries the MULTI-SOURCE set {sqlite, declaration} (the entrypoint Authority
      read is present on EVERY report, even when missing_entrypoint is not triggered — assemble:640; the trust
      analogue of check D-V1b). Add a sibling case: a report where NO downgrade fires STILL has the
      downgrade-triggers leaf source = {sqlite, declaration}, not {sqlite}.
  H4. F5 REGRESSION PIN: assert no Half-B leaf is ever rendered/labelled "current-state" and the Half-A posture
      is never labelled with the snapshot epoch. A consumer reading source=sqlite must never conclude
      current-state LiveGraph resolution.

DEGRADATION:
  D-T1. Half A — LiveGraph COLD/non-resident: posture leaf Unavailable + missing_partitions + reason; freshness
        Unavailable; NOT empty-as-known-zero (F3). Half B still fully served.
  D-T2. Half A — PRODUCER ABSENT: posture leaf ProducerUnavailable/RefreshFailed; Half B unaffected.
  D-T3. Half A — non-TS partitions: posture leaf Partial + LanguageSupport reason (no TS-grade claim for non-TS).
  D-T4. Half B — diagnostics blob ABSENT (diagnostics_available=false): diagnostics-meta + dependent leaves
        Unavailable/Unknown (NOT Fresh zeros) + the existing "Extraction diagnostics unavailable" caveat (F3).
  D-T5. Half B — STALE snapshot: Half-B leaves freshness=Stale; the v1 reliability reported as a Stale assessment.
  D-T6. EPOCH SKEW: Fresh LiveGraph posture + Stale snapshot → root.freshness = Stale (MEET); cold LiveGraph +
        Fresh snapshot → root degraded. The MEET cannot read Fresh-overall over a stale half (RISK-T-A).
  D-T7. Snapshot gate PRESERVED: no-snapshot → daemon SnapshotNotFound "no snapshot found" → CLI exit 2, NO
        envelope; not-ready snapshot → SnapshotNotFound "not ready" → exit 2 (OBSERVED dispatch.rs:2846-2869;
        §D-TRUST-6). Assert UNCHANGED.
  D-T8. Transport: socket-absent → connection error, NO envelope, exit 2 (EXECUTED this turn; assert UNCHANGED).

ENVELOPE CORRECTNESS:
  E1. MEET monotonicity: root.trust/freshness ≤ every leaf; no fold yields an Exact/Fresh root from a
      non-Exact/Stale/Unavailable leaf (the formal anti-false-completeness guarantee).
  E2. Invariants I1-I6 hold at every leaf and survive the fold: Exact requires Fresh+Complete (I1); Partial
      justified by reason/missing_partition/non-Fresh (I2); Unavailable carries a reason (I3); Stale≠Fresh (I4);
      null≠empty (I5); the Half-A posture leaf, being SCIP-dependent, is never Exact under PrecisionPending
      without a non-SCIP basis (I6).
  E3. provenance: per-leaf source correct (H1-H3); root.provenance.source = exact SET-UNION
      {livegraph, sqlite, declaration}; fallback_reason null today (no Half-B axis is LG-first this slice);
      missing_partitions surfaces only from the Half-A posture leaf.

WIRE SHAPE / RENDERER / FIXTURES:
  W1. WIRE SHAPE: top-level JSON = `CoherenceEnvelope<CoherentTrustReport>`; `value` carries the identity meta +
      the Half-B residual leaves + the Half-A current_state_posture leaf; `root.trust` + `root.freshness`
      present. Half-B payloads byte-identical (P1); whole-output byte-identity is NOT a goal (RISK-F).
  W2. RENDERER (presentation/trust.rs): the 8 Half-B sections (§1b) render byte-identical text + per-section
      source/freshness labels; a NEW "Current-State Posture" section renders the Half-A leaf (residency /
      freshness / languages / producer / capability), explicitly distinguished from the
      "(snapshot-scoped, outgoing extractor)"-labelled Half-B sections. The dead_code axis stays absent (P2).
  W3. FIXTURES: JSON-contract fixtures in lockstep with COHERENCE-ENVELOPE-1 — a Fresh-both fixture, a
      Stale-snapshot fixture, a cold-LiveGraph (Half-A Unavailable) fixture, a producer-absent fixture, a
      diagnostics-absent fixture, and a no-downgrade fixture pinning the {sqlite, declaration} downgrade leaf
      (H3). Bump the schema id only if contract tests pin the top-level shape (shared one-time bump with the
      orient/check/explain wrapper — RISK-F).

CLI-WRAPPER (run_trust; commands/trust.rs:35-130 — the PROCESS contract, §1c/§3e). Driveable off-target with a
recorded daemon-response fixture (the wrapped envelope) for CW1/CW2; CW3 needs no daemon.
  CW1. `rmap trust --json` over a wrapped fixture → stdout is the FULL `CoherenceEnvelope<CoherentTrustReport>`
       (value + provenance + trust + freshness) AND exit 0 (trust success is always 0 — §1c).
  CW2. `rmap trust` (human) over the SAME fixture → render is byte-identical for the Half-B sections + the
       labels + the NEW posture section (W2) AND exit 0. Assert the human path reads the WRAPPED value
       (TrustResponse projects `value`, §3e); a missed remap fails LOUD (parse error → exit 2), NOT silently.
  CW3. usage errors UNCHANGED: unknown flag (`rmap trust --bogus`) → `error: unknown flag: --bogus` + usage,
       exit 1; unexpected positional (`rmap trust x`) → `error: unexpected argument: x` + usage, exit 1
       (OBSERVED trust.rs:44-53). socket-absent → daemon-connection error, exit 2 (= D-T8). RepoNotFound →
       `error: repo not indexed` + hint, exit 2 (OBSERVED trust.rs:116-124). SnapshotNotFound ("no snapshot
       found"/"not ready") → `error: {code}: {message}`, exit 2 (= D-T7).

LIVE (after off-target green; macOS, ./scripts/dev-install-local.sh):
  L1. `rmap trust` on a Fresh TS pilot snapshot with a warm LiveGraph → root Fresh; Half-A posture shows the
      resident TS partition (Fresh, LanguageSupport=TS-mature, producer present, migrated-answer capability
      Exact); Half-B shows the v1 report byte-identical to pre-wrapper + source=sqlite labels.
  L2. Mutate a tracked file (induce snapshot staleness) without re-index → `rmap trust` → Half-B freshness=Stale;
      assert the v1 axes report a Stale assessment and the root is Stale (MEET), while Half-A may still be Fresh
      (epoch skew, D-T6).
  L3. `rmap trust` with the SCIP producer absent → Half-A posture ProducerUnavailable; Half-B still served (D-T2).
  L4. `rmap trust` on a never-indexed repo → daemon error, no envelope, exit 2 (snapshot gate, D-T7).
  L5. `rmap trust; echo $?` → 0 on success in BOTH default and --json mode (trust derives NO verdict exit — the
      live seal that the wrapper did NOT introduce a non-zero exit).
```

---

## 6. Scope boundary

```text
IN SCOPE: `rmap trust` ONLY (repo-wide always). Wrap trust's answer in `CoherenceEnvelope<CoherentTrustReport>`
(a NEW container — trust does NOT reuse CoherentOrientResult, §1d/D-TRUST-1); RETAIN the full v1 report as Half-B
residual leaves (source=sqlite, LABELLED outgoing-extractor, payloads byte-identical); ADD the Half-A
current-state posture leaf (source=livegraph) as a PROJECTION of EXISTING LiveGraph runtime state (residency /
freshness / languages / producer / capability — NO new producer, NOT a recomputed reliability score); label the
downgrade-triggers leaf multi-source {sqlite, declaration} (D8); fold the root by MEET; remap the run_trust human
deserialization to `value` + add the two-halves render (§3e). NO cert/fastpath on the v1 axes (they have no LG
alternative). The dead_code axis stays internal (D-TRUST-5).

OUT OF SCOPE (separate later slices / explicitly deferred):
  - The FULL LiveGraph reliability rebase (TRUST-DISPOSITION Option B). The hybrid (C) is ratified; B is
    DEFERRED until a current-state reliability PRODUCER exists (coherence-layer-1.md D2). This slice MUST NOT
    recompute import/call/change reliability LEVELS over the LiveGraph — that is the deferred producer (D-TRUST-2).
  - NO new producer for measurements/boundary/inferences/unresolved-edges (contract D6). Half A READS existing
    LiveGraph runtime state; it produces no new persisted fact.
  - NO change to the v1 reliability formulas / downgrade triggers / classification / blast-radius / enrichment
    (rules.rs + service.rs compute logic UNTOUCHED — the coherence layer wraps + labels, it does not re-judge).
  - NO change to declarations / Authority semantics (the entrypoint count read is unchanged; only its source is
    now labelled in the leaf set).
  - NO snapshot-gate relaxation (no-snapshot/not-ready → daemon error, as today — D-TRUST-6). Relaxing trust to
    serve a LiveGraph-only posture without a snapshot is a future option, not this slice.
  - NO non-TS LiveGraph support beyond the existing per-crate Rust beta (the posture leaf reports the language
    maturity honestly; D6).
  - ORIENT/CHECK/EXPLAIN-LIVEGRAPH (done / earlier in the sequence). COHERENCE-ENVELOPE-1 (the support module —
    this slice DEPENDS on it; not built here).
  - SQLITE-RAW-DECOMMISSION-1: trust still reads SQLite (snapshots/edges/unresolved_edges/module_candidates) +
    declarations (Authority) for the RETAINED Half-B diagnostics. NO table is decommissioned here. The hybrid
    RETAINS the unresolved-edge / extraction-diagnostics tables (it still reports them, labelled), so it does
    NOT by itself unblock their decommission — COHERENCE-READINESS-RECOMPUTE-1 MUST record them as still
    load-bearing (contract slice-sequence note).

HARD GUARDRAILS (this slice's out-of-scope, mirroring the contract):
  NO source code (spec-first). NO table deletion, NO schema/data migration, NO default flip beyond specifying it.
  NO new producer. NO change to the v1 trust computation. NO raw nodes/edges/unresolved_edges/snapshots
  decommission. NO edit to docs/ROADMAP.md or CURRENT_SLICE.md. NO live daemon run / index / refresh.
```

---

## Forced decisions — every cell filled

### D-TRUST-1 — coherent container = NEW `CoherentTrustReport` (NOT CoherentOrientResult) (DECIDED, recorded — within contract)

```text
QUESTION: trust returns the bespoke `TrustReport`, not the shared `OrientResult` (§1d, OBSERVED first-hand). How
does the ratified wrapper's `value: T` instantiate for trust?

| Option | Container | Reuses CoherentOrientResult | Half-B retention | Half-A placement | Contract fit | Verdict |
|---|---|---|---|---|---|---|
| A — NEW CoherentTrustReport: TrustReport axes re-typed to leaves + a current_state_posture leaf | trust-specific | NO (trust has no OrientResult) | each axis a leaf, payload pristine | a co-located leaf in the same tree | EXACT — contract "the full TrustReport retained, EACH axis a CoherenceEnvelope leaf" | **DECIDED** |
| B — coerce trust into CoherentOrientResult (signals = trust axes) | shared | YES | axes flattened into signals[] | a signal | MISFIT — trust is not a signal-list; forcing it loses the report shape | rejected |
| C — parallel structures: keep bare TrustReport + a sibling Vec of CoherenceEnvelope leaves | two structures | n/a | bare report + parallel leaves | a parallel array | DRIFT HAZARD — the same parallel-structure pattern orient D7 rejected (Q7-4) | rejected |

DECIDED: **Option A.** It is the literal realization of the contract's trust envelope spec ("the full
TrustReport is RETAINED, and EACH reported axis is wrapped as a CoherenceEnvelope leaf"). It mirrors orient's D7
(CoherentOrientResult re-types the signals slot) but for trust's distinct DTO: CoherentTrustReport re-types the
report axes to leaves and ADDS the Half-A posture leaf, ONE self-describing tree (leaf provenance co-located
with the axis it labels). Decide-and-record (CLAUDE.md §Decision Autonomy: "choices a ratified decision already
imply"). B/C are named rejected so the gap is closed at authoring. NOT a re-escalation: the wrapper SHAPE
{value, provenance, trust, freshness} is unchanged; D-TRUST-1 only pins how `T` instantiates for trust — a local
realization the ratified wrapper + the contract's trust envelope spec already imply. D-ORIENT-6 /
D-EXPLAIN-TRUST-BRIEFING (the trust_briefing field) are STRUCTURALLY ABSENT: trust injects no overlay and IS the
report (§1d), so CoherentTrustReport has no trust_briefing.
```

### D-TRUST-2 — Half A = posture PROJECTION of existing LiveGraph state, NOT a reliability recomputation (DECIDED, recorded — the anti-Option-B guard)

```text
QUESTION: the ratified hybrid (D2) says trust reports "the current per-answer posture (LiveGraph-derived)". What
is the SUBSTANCE of Half A — and where is the line against Option B (the deferred full rebase)?

| Option | Half-A substance | New producer? | Re-opens TRUST-DISPOSITION? | Honesty | Verdict |
|---|---|---|---|---|---|
| A — posture PROJECTION: residency / per-partition freshness / contributing-language maturity / producer availability / migrated-answer capability, in the AnswerEnvelope vocabulary | reads EXISTING runtime state | NO | NO | maximal (reports only what the LiveGraph already knows; never a fabricated score) | **DECIDED** |
| B — recompute repo-wide reliability LEVELS (import/call/change) over the LiveGraph | YES (a current-state reliability roll-up) | YES | YES — this IS the deferred Option B | premature (no current-state reliability producer exists yet) | rejected (deferred) |
| C — omit Half A; keep only the v1 report | none | NO | YES — this is Option A freeze-v1 | violates the ratified hybrid (no current-state half) | rejected |

DECIDED: **Option A.** The ratified hybrid (D2) reuses the AnswerEnvelope per-answer vocabulary and adds NO new
producer; it explicitly DEFERS the repo-wide reliability rebase to Option B "once a current-state reliability
producer exists". So Half A is a PROJECTION of state the LiveGraph runtime ALREADY holds (LIVEGRAPH-RUNTIME-1
residency/epochs; QUERY-MIGRATION-1 contributing_languages; LIVEGRAPH-INTEGRATION-1C producer availability;
livegraph_feed cert) into the AnswerEnvelope vocabulary — NOT a new reliability score. Decide-and-record WITHIN
the ratified hybrid; the matrix is provided because Half A's substance is load-bearing for honesty (it is
exactly where a false current-state claim could be minted). HARD GUARD: choosing B here would re-open the
ratified TRUST-DISPOSITION and invent the deferred producer — FORBIDDEN (§6). The line is bright: Half A reports
a POSTURE (class/freshness/languages/capability), never a reliability LEVEL.
```

### D-TRUST-3 — combine = MEET over BOTH halves (DECIDED, recorded)

```text
The root trust/freshness fold is the MEET (greatest-lower-bound) over ALL leaves of BOTH halves (contract D3).
DECIDED, not asked: it is the only fold consistent with the AnswerEnvelope invariants (monotone, cannot raise
class) and with "never collapse unknown/inferred/extracted into one certainty class". The root freshness = the
MEET of the Half-A LiveGraph current freshness AND the Half-B snapshot freshness — so epoch skew (one half fresh,
the other stale) yields a Stale-overall root (RISK-T-A). Local mechanism implied by a ratified invariant.
```

### D-TRUST-4 — downgrade-triggers leaf is MULTI-SOURCE {sqlite, declaration} (DECIDED, recorded — contract D8)

```text
DECIDED (contract D8 multi-source LEAF provenance, RATIFIED): the downgrade-triggers leaf carries
provenance.source = {sqlite, declaration} because missing_entrypoint_declarations reads the `declarations`
Authority table via count_active_declarations(entrypoint) on EVERY report (assemble:640), UNCONDITIONALLY — the
`declaration` source is present even when the trigger is not fired (it was consulted). The trust analogue of
check's verdict leaf (D-CHECK-1/D-V1b). The `Source` axis is unchanged {livegraph, sqlite, filesystem,
declaration}; the intra-sqlite distinctions ride in `basis`, not a new variant. Decide-and-record (the ratified
D8 set-typed `Provenance.source` already grants this; COHERENCE-ENVELOPE-1 BUILDS the field, does not decide it).
```

### D-TRUST-5 — dead_code reliability axis stays INTERNAL (DECIDED, recorded — within contract + VISION)

```text
DECIDED: the dead_code reliability axis (computed internally, rules.rs:238) stays NON-serialized
(skip_serializing, types.rs:140-141) — it is NOT a leaf and appears in NEITHER half. Surfacing it would
re-introduce the withdrawn `rmap dead` vocabulary (VISION §"Dead-code surface withdrawal"; the public dead-code
surface was removed). Decide-and-record: preserve existing behaviour; do not add a withdrawn surface (do not get
ahead of scope; do not add functionality). The internal computation is retained for future coverage-backed
reintroduction, gated elsewhere.
```

### D-TRUST-6 — snapshot gate PRESERVED (DECIDED, recorded — scope)

```text
DECIDED: trust's handler errors on no-snapshot ("no snapshot found", dispatch.rs:2846-2851) and not-ready
("latest snapshot is not ready", :2861-2869). This slice PRESERVES that gate: trust requires a ready SQLite
snapshot to assemble the report, because Half B (the v1 diagnostics) is fundamentally snapshot-scoped. The
Half-A LiveGraph posture is conceptually snapshot-independent, but the hybrid is ADDITIVE to the snapshot-scoped
report — so with no ready snapshot, trust errors as today (no envelope; honest unavailability at the handler
level, NOT a false-complete answer). Relaxing the gate to serve a LiveGraph-only posture without a snapshot is a
FUTURE option, recorded, not built here. Decide-and-record (conservative; changing handler behaviour is a
behaviour change beyond this spec, and the current behaviour mints no false claim).
```

### D-TRUST-7 — CLI human-render remap to `value`; exit codes UNCHANGED (DECIDED, recorded — forced by the wrapper)

```text
DECIDED: run_trust's human deserialization (from_value::<TrustResponse>, trust.rs:104) must read the wrapped
`value` (or unwrap result["value"] first) so the render projects CoherentTrustReport; the --json path prints the
full wrapper verbatim (no code change beyond the daemon emitting it). The render gains per-section source/
freshness labels + the NEW Current-State Posture section (§3e/W2). EXIT CODES are UNCHANGED (success 0 always /
usage 1 / runtime+transport+daemon 2 — §1c): trust derives NO exit code from the report body, so — UNLIKE check
— there is NO silent-CI-breakage hazard; a missed remap fails LOUD (human parse error → exit 2). Decide-and-record:
a mechanical consequence of the ratified `value`-nesting, not a new decision; introduces no new exit codes.
```

### D-TRUST-8 — scope (DECIDED, recorded)

```text
This slice SPECIFIES trust ONLY. NO command implementation here. NO Option-B reliability rebase (deferred). NO
new producer. NO change to the v1 trust computation (rules.rs/service.rs untouched). NO change to declarations/
Authority. NO raw nodes/edges/unresolved_edges/snapshots decommission (all retained, now honestly labelled — the
hybrid keeps them load-bearing; COHERENCE-READINESS-RECOMPUTE-1 records this). Mirrors contract D6.
```

---

## Risks (trust-specific projections of the contract risks; each the implementation must address)

```text
RISK-T-A — EPOCH SKEW FALSE FRESHNESS (= contract RISK-A). The Half-A LiveGraph posture and the Half-B snapshot
  have INDEPENDENT epochs; a blended report could read Fresh-overall. MITIGATION: the root freshness is the MEET
  of the two halves (D-TRUST-3); the fold is monotone and cannot raise to Fresh.
RISK-T-B — V1 REPORT MISREAD AS CURRENT-STATE (THE trust-specific false-completeness, = contract RISK-C/F5). The
  v1 reliability is an OUTGOING-extractor snapshot-scoped artifact; presenting it as the current-state LiveGraph
  resolution is the exact failure the Fact Certainty Model forbids. MITIGATION: disjoint source labels (Half B
  always source=sqlite + LABELLED outgoing-extractor; Half A always source=livegraph); the two halves are never
  conflated (§3c, H1-H4). This is the reason the hybrid was ratified over freeze-v1.
RISK-T-C — HALF-A SCOPE CREEP TOWARD OPTION B. An implementer could be tempted to compute LiveGraph reliability
  LEVELS (a repo-wide roll-up), which IS the deferred Option B and a new producer. MITIGATION: the bright-line
  anti-B guard (D-TRUST-2) — Half A is a POSTURE projection (class/freshness/languages/capability), never a
  reliability score; producing levels re-opens the ratified TRUST-DISPOSITION.
RISK-T-D — "UNRESOLVED EDGE" SEMANTIC DRIFT UNDER SCIP. The v1 unresolved-edge model is an artifact of the
  homegrown extractor; SCIP is compiler-grade, so "unresolved edge" changes meaning. MITIGATION: Half B is
  LABELLED as the outgoing model; the eventual Option B rebase (deferred) replaces it. Recorded so the conceptual
  shift is explicit, not silent.
RISK-T-E — ENVELOPE SHAPE CHURN (= contract RISK-F, shared with orient/check/explain). The wrapper changes
  trust's JSON wire shape (top level becomes CoherenceEnvelope; value = CoherentTrustReport). MITIGATION: land
  the wrapper ONCE in COHERENCE-ENVELOPE-1; keep Half-B values byte-identical (P1); update the trust renderer +
  fixtures in lockstep; one shared schema bump with the other three commands (not a per-command bump).
RISK-T-F — `edges_resolved == edges_total` QUIRK (OBSERVED, first-hand; recorded, NOT reconciled). service.rs:420-424
  assigns edges_resolved = diagnostics.edges_total, so the human "Edges: N% resolved" is always 100%. This is a
  Half-B (v1) datum; the hybrid RETAINS it byte-identical. Reconciling it is OUT OF SCOPE (pre-existing behaviour;
  the trust computation is untouched here). Flagged per the evidence law so a later slice can address it.
RISK-T-G — HALF-A POSTURE READ API + FRESHNESS RECONCILIATION (= §3d RP-T1/RP-T2). Half A needs a LiveGraph
  posture read (aggregating existing runtime state) and the FreshnessState enum. MITIGATION: the posture read is
  a LOCAL mechanism reading existing state (NO new producer, no new boundary — D-TRUST-2); the FreshnessState +
  any FreshnessInfo mapping is COHERENCE-ENVELOPE-1's (orient D-ORIENT-7). Recorded as realization points, not
  open decisions.
RISK-T-H — MODULE-IDENTITY DIVERGENCE (= contract RISK-E, inherited). trust's module rows use the v1 framing
  (SQLite module_candidates / compute_module_stats); the LiveGraph aggregates modules by dirname. The two module
  identity sets may differ. MITIGATION: trust's modules leaf stays Half-B (source=sqlite, v1 framing); this slice
  does NOT swap it to a LiveGraph module count (a naive swap would diverge — RISK-E). Recorded.
RISK-T-I — DECOMMISSION STILL BLOCKED (recorded, cross-slice). The hybrid RETAINS the unresolved_edges +
  extraction_diagnostics tables (Half B still reports them, labelled), so TRUST-LIVEGRAPH-1 does NOT unblock
  their decommission. MITIGATION: COHERENCE-READINESS-RECOMPUTE-1 must record trust's retained eager SQLite reads
  (the 8 Half-B reads, §1a) as STILL load-bearing for SQLITE-RAW-DECOMMISSION-1.
```

---

## References
```text
GOVERNANCE / MODEL:
- docs/VISION.md §Fact Certainty Model / §Product Layer Model / §Agent Priorities (#2 preserve computed truth) /
  §"Dead-code surface withdrawal" (the dead_code-axis internal-only basis, D-TRUST-5).
- agent_docs/architecture.md §Mandatory Rules (rule 6 "null=unknown, empty=known-zero"; rule 2 support-module-first;
  rule 4/§Layer-Rules Layer-4-overlays-never-erases) / §Product Layer Stack (Layer 1 lists "trust") / §Build Order
  (support module → storage → feature → tests → docs) / §Persistence Completeness Checklist.
- CLAUDE.md §Fact Certainty Model / §Decision Autonomy (decide-and-record vs stop-and-ask) / §Evidence Law.
- agent_docs/validation.md (the EXECUTED/OBSERVED/INFERRED/NOT RUN labels; the standard validation sequence).
- agent_docs/storage-architecture-v2.md (Tier A1 Authority — declarations never in Tier B/C; the table→tier map).

AUTHORITATIVE CONTRACT + PRECEDENT:
- docs/slices/coherence-layer-1.md — RATIFIED + AMENDED (2026-06-09, D8). Cited by SECTION / DECISION ID (stable;
  the D8 amendment shifted line numbers): §"Per-command source map" (the TRUST row — the source postures this
  doc refines first-hand); Q3 (trust "must itself state which axes are LiveGraph-current vs snapshot-scoped");
  Q6 ("trust returns the separate full TrustReport"); Q7 (combine/MEET/DISJOINT); Q8 / §"Safe-fallback contract"
  (the TRUST row — hybrid is the safe-fallback); §"The shared coherence answer-envelope" (the trust envelope spec
  — "full TrustReport retained, EACH axis a CoherenceEnvelope leaf, source=livegraph posture / source=sqlite
  residual"); D2 (TRUST-DISPOSITION = hybrid, RATIFIED — Option C; A=freeze-v1, B=full-rebase deferred); D1
  (wrapper); D3 (MEET); D5 (overlay-preserves-computed); D8 (multi-source LEAF provenance — Provenance.source =
  BTreeSet<Source>); §"Proposed follow-up slice sequence" (TRUST depends on EXPLAIN + COHERENCE-ENVELOPE-1; the
  hybrid RETAINS the diagnostics tables → COHERENCE-READINESS-RECOMPUTE-1); RISK-C (trust conceptual drift),
  RISK-E (module identity), RISK-F (envelope churn), RISK-G (Signal freshness reconciliation).
- docs/slices/orient-livegraph-1.md — the first per-command application (SHAPE precedent): the D7
  CoherentOrientResult container (the trust analogue is D-TRUST-1 CoherentTrustReport); the root MEET D-ORIENT-4;
  transport degradation §4.
- docs/slices/check-livegraph-1.md — the first MULTI-SOURCE composite leaf (D-CHECK-1 + D8 {sqlite, declaration};
  the trust analogue is the downgrade-triggers leaf D-TRUST-4); the CLI process-wrapper enumeration §1f (the
  trust analogue is §1c, MILDER — no verdict-derived exit); the MEET-freshness-not-fastpath posture.
- docs/slices/explain-livegraph-1.md — the heaviest aggregator; the D8 multi-source leaves; the EXPLAIN_TRUST
  v1-trust-core section + the trust-overlay/trust_briefing correction (trust the COMMAND has NEITHER — §1d).

TRUST IMPLEMENTATION TODAY (all SQLite/Authority; LiveGraph=NONE) [OBSERVED, first-hand this turn]:
- rust/crates/trust/src/types.rs — TrustReport:240-300 (display_name skip_if_none:248-249; toolchain:251;
  enrichment_eligible_count serde(skip):298-299); TrustSummary:149-161; TrustReliability:135-143 (dead_code
  skip_serializing:140-141); TrustDowngrades:113-119; TrustCategoryRow:167-172; TrustClassificationRow:176-180;
  UnknownCallsBlastRadiusBreakdown:186-191; EnrichmentStatus:207-212 + EnrichmentTopType:197-203;
  ModuleTrustRow:218-227; ReliabilityLevel:74-79; ReliabilityAxisScore:83-87; DowngradeTrigger:105-109;
  ExtractionDiagnostics:51-68.
- rust/crates/trust/src/service.rs — assemble_trust_report:591-705 (the 8 reads: diagnostics:600,
  file_paths:628, module_stats:632, path_prefix_cycles:636, entrypoint count:640, CALLS:644, classification
  calls-filter:655 / all:665, unknown samples:677); compute_trust_report:210-457 (8 phases);
  edges_resolved==edges_total:420-424; build_caveats:155-199 (diagnostics-unavailable:163-167; permanent cycle
  caveat:193-197); call_resolution_rate:398-405; compute_blast_radius_and_enrichment:468-577.
- rust/crates/trust/src/rules.rs — detect_framework_heavy_suspicion:77; detect_registry_pattern_suspicion:98;
  detect_missing_entrypoint_declarations:132; detect_alias_resolution_suspicion:149;
  compute_import_graph_reliability:171; compute_call_graph_reliability:204 (<0.5 LOW / <0.85 MEDIUM:217-232);
  compute_dead_code_reliability:238; compute_change_impact_reliability:275.
- rust/crates/trust/src/storage_port.rs — TrustStorageRead trait:142-190 (the 8 narrow read methods).
- rust/crates/daemon-runtime/src/dispatch.rs — handle_trust:2825-2920 (resolve+display_name:2830; read lock:2839;
  get_latest_snapshot:2844; no-snapshot SnapshotNotFound:2846-2851; not-ready:2861-2869; assemble:2881-2887;
  assembly-error InternalError:2889-2894; display_name inject:2899; serialize:2913-2919; NO LiveGraph branch, NO
  overlay injection — contrast orient :2647 / explain :2811).
- rust/crates/rgr/src/presentation/trust.rs — TrustResponse (deserialized subset):46-62; render_human (8
  sections):139-198; render_resolution:200-229; render_reliability (3 axes, NO dead_code):231-240;
  render_unresolved_breakdown:242-259; render_classification:261-278; render_suspicious_modules (take 10):280-299;
  render_downgrades (4 triggers):301-351; format_axis:354-361; humanize_reason:364-405; truncate_uid:407-413.
- rust/crates/rgr/src/commands/trust.rs — run_trust:35-130 (args:39-55; cwd/canonicalize:58-72; DaemonClient::new
  :75-81; request:88; --json stdout SUCCESS:90-96; human from_value::<TrustResponse>+render SUCCESS:104-108;
  RepoNotFound+hint:116-124; other/transport Err exit 2:121/125-128; SUCCESS ALWAYS 0).

ANSWER-ENVELOPE VOCABULARY + LIVEGRAPH SURFACE [OBSERVED via contract/precedent]:
- rust/crates/repo-graph-trust-model/src/lib.rs — AnswerClass / FreshnessState / DegradationReason /
  LanguageSupport / QueryCompleteness / ProvenanceBasis + the 6 invariants (the vocabulary Half A projects).
- rust/crates/repo-graph-livegraph/src/lib.rs — the runtime state Half A reads (partition residency +
  missing_partitions; per-partition epoch/freshness; contributing_languages union); livegraph_feed.rs (the cert
  fingerprint + FallbackReason); LIVEGRAPH-INTEGRATION-1C (the ProducerUnavailable failure class).

EVIDENCE LOG:
- [EXECUTED, this turn] `rmap trust` → "error: daemon connection failed: socket does not exist:
  /Users/apple/Library/Application Support/repo-graph/daemon.sock" (transport degradation path, §4). Exit code
  NOT captured at runtime (pipe-masked); source maps it to exit 2 (commands/trust.rs:78-80, OBSERVED).
- [OBSERVED, first-hand, this turn] trust/src/{types,service,rules,storage_port}.rs; dispatch.rs
  handle_trust:2825-2920; rgr/src/presentation/trust.rs; rgr/src/commands/trust.rs. The COMPLETE three-surface
  output enumeration (§1a DTO, §1b human render, §1c CLI wrapper, §1d negative space) is grounded in these reads.
- [OBSERVED, via contract/precedent] coherence-layer-1.md (full, incl. D2 hybrid + D8); orient/check/
  explain-livegraph-1.md (structure + container + multi-source-leaf + CLI-wrapper precedents); architecture.md
  §§Mandatory Rules/Product Layer Stack/Build Order; storage-architecture-v2 Tier model (cited, not re-read).
- [INFERRED] the CoherenceEnvelope<CoherentTrustReport> wiring (§3), the Half-A posture field set (§3a/§D-TRUST-2),
  the per-leaf source mapping (§2), the MEET freshness rules (§3c), the degradation mapping (§4), the validation
  plan (§5), the forced-decision verdicts (D-TRUST-1..8) — grounded in the ratified contract + hybrid (D2) + the
  three precedents. No realization here re-opens a ratified decision; each is decide-and-record per CLAUDE.md
  §Decision Autonomy, with an exhaustive matrix where a genuine choice existed (D-TRUST-1, D-TRUST-2).
- [NOT RUN] live `rmap` orientation / index / trust over a populated graph (daemon socket absent; spec-only slice
  does not mutate state). The §5 LIVE plan (L1-L5) is specified for the eventual implementation, NOT executed here.
```
