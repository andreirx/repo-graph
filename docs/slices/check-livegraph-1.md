# CHECK-LIVEGRAPH-1: apply the coherence contract to `rmap check`

Slice ID: CHECK-LIVEGRAPH-1
Status: **DESIGN / SPEC-FIRST — NOT IMPLEMENTED — DECISION-COMPLETE.** This document SPECIFIES the SECOND
per-command application of the ratified COHERENCE-LAYER-1 contract (orient was first). It produces NO source
code, NO table deletion, NO schema/data migration, NO default flip. The implementation is a LATER slice and
depends on COHERENCE-ENVELOPE-1 (the support module) landing first. **No open DECISION_REQUIRED remains.**
The two boundary decisions the contract left open for orient are STRUCTURALLY ABSENT for check (see the
banner finding below); the one check-specific data-shape point — the MULTI-SOURCE verdict leaf — was escalated
at iteration 1 and is now RATIFIED.

**COMPLETE OUTPUT ENUMERATION (review-2 closure, iteration 3):** check produces THREE distinct output surfaces,
all enumerated FIRST-HAND below: (1) the daemon/API envelope — signals, conditions, envelope fields (§1a-1d);
(2) the human-render content (§1e); (3) the CLI PROCESS wrapper — stdout/stderr + the 0/1/2 EXIT CODE that CI
observes (§1f). check is CI-FACING (its exit code is DERIVED from the verdict signal, unlike orient's always-
SUCCESS), so adopting the ratified `value`-nesting wrapper FORCES a CLI exit-code + human-render remap (§3e) —
a parity obligation pinned in validation (§5 CLI-WRAPPER + L5). This is a mechanical consequence of the
already-ratified wrapper shape, NOT a new boundary decision.

**RATIFIED DECISION (operator sign-off 2026-06-09 — CHECK-PROVENANCE-LEAF-SHAPE = multi-source LEAF
provenance):** A `CoherenceEnvelope` LEAF may carry provenance from MULTIPLE contributing sources. The
COHERENCE-LAYER-1 contract is AMENDED (its D8) so `Provenance.source` is a `BTreeSet<Source>` at BOTH leaf
and root — resolving the prior leaf-single (coherence-layer-1.md ~:397) / root-set (~:433) inconsistency.
check keeps its SINGLE composite verdict leaf (D-CHECK-1) and labels that leaf's provenance as the honest
multi-source set `{sqlite, declaration}` (snapshot-present) / `{sqlite}` (no-snapshot). The leaf's source set
is now a RATIFIED shape, not an invented or deferred one; this SUPERSEDES the iteration-1 draft's
self-"DECIDED" leaf-shape text (§3d RP-1, D-CHECK-1, D-CHECK-5 carry the ratified record). **No open
DECISION_REQUIRED remains.**

**BANNER FINDING (the load-bearing difference from orient, OBSERVED first-hand this turn):** check is the
SIMPLEST of the four coherence commands. It has **ZERO LiveGraph-first leaves** — none of its inputs is a
migrated SQLite-free surface — so it gets **no cert-gated fastpath**. Its entire coherence contribution is to
**wrap the existing SQLite/Authority answer in `CoherenceEnvelope<T>` and label the verdict with honest
MEET-freshness**, exactly as the contract's check row prescribes: "check is a thin 3-phase reducer over the
SAME ports orient uses; it gains no NEW LiveGraph source of its own. Its coherence work is the MEET-freshness
verdict label, not a fastpath." [OBSERVED: coherence-layer-1.md:334-335.] check ALSO has **no daemon trust
overlay** (the orient/explain post-serialize `trust` key) and **no focus dispatch** (always repo focus) — so
orient's two escalated decisions (D-ORIENT-6 trust_briefing; D-ORIENT-SYMBOL-CALLGRAPH) have NO check
analogue. This is verified, not assumed (§1d, D-CHECK-2).

Goal: specify how `rmap check` serves its pre-handoff readiness verdict from durable **SQLite/persisted
authority**, wrapped in `CoherenceEnvelope<T>`, with honest degradation (a PASS over a Stale snapshot is a
Stale PASS, never a Fresh PASS) and no false completeness — WITHOUT inventing a LiveGraph path the contract
does not grant it.

Track: Stage D, SQLITE-RAW-DECOMMISSION path — second per-command coherence build.

Authoritative contract (RATIFIED, read FIRST): `docs/slices/coherence-layer-1.md`. This slice REUSES that
contract's `CoherenceEnvelope<T> { value, provenance, trust, freshness }` wrapper, its check source map
(coherence-layer-1.md:325-335), its MEET fold (D3), its authority-overlay rule (D5), and its safe-fallback
ladder (the CHECK row). It does NOT re-open COHERENCE-ENVELOPE-SHAPE (RATIFIED = Option B wrapper) or
TRUST-DISPOSITION (RATIFIED = hybrid; trust is a separate later slice).

Precedent (followed for SHAPE, reused — NOT re-derived): `docs/slices/orient-livegraph-1.md` — the first
per-command application. This doc mirrors its structure (current-outputs enumeration → source map → envelope
wiring → degradation → validation → scope → forced-decision matrices → risks → evidence log) and reuses its
ratified container `CoherentOrientResult` (the shared OrientResult with its `signals` slot re-typed to leaf
envelopes, contract D7) verbatim. Where orient needed a decision, this doc records whether check needs the
same one — and for the two orient escalations, it does NOT (§1d, D-CHECK-2).

Depends (precedent, reused — NOT re-derived here):
- COHERENCE-LAYER-1 — the ratified mixed-source contract (envelope shape, check source map, MEET, fallback).
- COHERENCE-ENVELOPE-1 — the SUPPORT module that realizes `CoherenceEnvelope<T>` + `CoherentOrientResult` +
  the MEET fold + the FreshnessInfo→FreshnessState reconciliation. **MUST land before this slice's
  implementation** (architecture.md §Build Order: support module → feature). check is the FIRST command to
  EXERCISE a MULTI-SOURCE leaf (the verdict). The contract amendment (D8, RATIFIED 2026-06-09) makes
  `Provenance.source` a `BTreeSet<Source>` at leaf and root, so COHERENCE-ENVELOPE-1 BUILDS that ratified
  set-typed field — it no longer DECIDES the shape (§3d RP-1 / D-CHECK-5).
- ORIENT-LIVEGRAPH-1 — the first feature build, which de-risks the wrapper/provenance-tag pattern and
  ratifies `CoherentOrientResult`. The contract sequences CHECK after ORIENT precisely so check "reuses
  orient's provenance-aware trust summary" (coherence-layer-1.md:610-611). check MUST NOT land before orient.

## Spec-first note (read first)
```text
This is a SPECIFICATION. Per the repo evidence law (CLAUDE.md §Evidence Law), every claim is labelled
OBSERVED or INFERRED.
  OBSERVED [first-hand, this turn] = reads I performed this turn, with file:line:
      rust/crates/agent/src/check/mod.rs (run_check three-phase pipeline, end-to-end)
      rust/crates/agent/src/check/evaluate.rs (the 6 condition evaluations + the no-snapshot early return)
      rust/crates/agent/src/check/reduce.rs (the verdict precedence reducer + the 6-condition test oracle)
      rust/crates/agent/src/check/types.rs (CheckVerdict / ConditionCode / CheckInput / GateOutcomeForCheck)
      rust/crates/daemon-runtime/src/dispatch.rs:2672-2732 (handle_check — NO trust overlay) vs :2734-2819
        (handle_explain — DOES inject a post-serialize `trust` key: the contrast that proves check has none)
      rust/crates/agent/src/dto/signal.rs (grep: CheckPass/Fail/Incomplete + SnapshotInfo codes, as_str
        strings, descriptor severities, evidence structs, builders)
      rust/crates/agent/src/confidence.rs (derive_repo_confidence — SHARED with orient)
      rust/crates/agent/src/storage_port.rs:225-340,:438-520 (AgentReliabilityLevel / EnrichmentState /
        AgentTrustSummary + the get_repo/get_latest_snapshot/get_stale_files/get_trust_summary port reads)
      rust/crates/rgr/src/presentation/check.rs (CheckResponse human renderer — NO trust field)
      rust/crates/rgr/src/commands/orient.rs:222-331 (run_check_cmd — the CLI PROCESS wrapper: arg parse,
        cwd/canonicalize, daemon connect, the signal->exit-code 0/1/2 mapping, --json vs human render) vs
        :130-210 (run_orient_cmd — the CONTRAST: both success arms return ExitCode::SUCCESS, no signal-derived
        exit code)
      rust/crates/rgr/src/daemon_client/connection.rs:42-64 (DaemonClientError variants —
        ConnectionFailed/SendFailed/ReadFailed/InvalidResponse/DaemonError{code,message,data}/Timeout)
  OBSERVED [via contract / precedent, first-hand THERE] = facts the ratified contract or the orient slice
      read first-hand and cited with file:line; reused here without re-reading (e.g. agent_impl.rs concrete
      SQL; signal.rs:89-98,:958 Signal.freshness; storage-architecture-v2 Tier model). Labelled inline.
  INFERRED = my design judgment over those OBSERVED facts (the envelope wiring, the per-leaf provenance
      mapping, the MEET freshness rules, the validation plan), grounded in the ratified contract.
Spine claims I PERSONALLY verified this turn are marked [OBSERVED, first-hand].

NO live `rmap` graph orientation was run: the daemon socket is absent. [EXECUTED this turn: `rmap check` ->
"error: daemon connection failed: socket does not exist:
/Users/apple/Library/Application Support/repo-graph/daemon.sock".] A spec-only slice does not start the
daemon or run the index/refresh sequence (that mutates state). Orientation was grounded in first-hand source
reads — the stronger evidence basis for a contract about code structure. The socket-absent result is itself
recorded below as check's transport-level degradation path (§4 TRANSPORT-LEVEL DEGRADATION), identical to
orient's.
```

## Why now (priority path)
```text
[OBSERVED: docs/slices/coherence-layer-1.md §slice sequence :604-624 + CURRENT_SLICE.md STATUS banner.]
COHERENCE-LAYER-1 is RATIFIED (operator sign-off 2026-06-08). Its slice sequence is
ORIENT-LIVEGRAPH-1 → CHECK-LIVEGRAPH-1 → EXPLAIN-LIVEGRAPH-1 → TRUST-LIVEGRAPH-1. ORIENT-LIVEGRAPH-1 is
DECISION-COMPLETE (operator sign-off 2026-06-09), de-risking the wrapper. check is the contract's NEXT
per-command build: "verdict carries MEET freshness; gate stays Authority. Small; reuses orient's
provenance-aware trust summary" (coherence-layer-1.md:610-611).

[OBSERVED, first-hand: dispatch.rs handle_check:2672-2732; LiveGraph is wired into dispatch ONLY for
callers/callees/imports/stats/cycles/path/preload/refresh (all <=1701 per coherence-layer-1.md:59-62,
:747-749); the check handler body 2672-2732 contains NO LiveGraph branch.] => check today is 100% SQLite +
Authority with NO served LiveGraph path. It is one of the LAST four SQLite-eager defaults and a precondition
for SQLITE-RAW-DECOMMISSION-1: the raw `nodes`/`edges`/`snapshots`/`declarations` substrate cannot be
decommissioned while check reads it eagerly on every call. (check's contribution to that decommission is
SMALL — it reads no `nodes`/`edges` directly; its load-bearing reads are snapshots/files/file_versions
[operational], the v1 trust core, and `declarations` [Authority]. None of those leaves SQLite under this
slice; this slice makes them HONESTLY LABELLED, not decommissioned — see §6 + COHERENCE-READINESS-RECOMPUTE-1.)
```

---

## 1. What `rmap check` returns today (OBSERVED, first-hand)

check is a THREE-PHASE pipeline [OBSERVED: check/mod.rs:45-153 `run_check`], NOT an aggregator fan-out like
orient:
- **Phase 1 (Gather)** [mod.rs:50-104]: resolve repo (`get_repo`:53), fetch latest snapshot
  (`get_latest_snapshot`:60); if a snapshot exists, fetch stale files (`get_stale_files`:81), the trust
  summary (`get_trust_summary`:84), and the gate outcome (`gather_gate_outcome`:87). Build one `CheckInput`
  struct (the pure reducer's only input).
- **Phase 2 (Reduce)** [mod.rs:108; reduce.rs:44-51]: `check(&input)` = `evaluate_conditions` (pure, no I/O)
  then `reduce_verdict` (precedence Incomplete > Fail > Pass, reduce.rs:18-41).
- **Phase 3 (Format)** [mod.rs:110-152]: build the verdict signal, push SNAPSHOT_INFO when a snapshot exists,
  `sort_and_rank`, and return ONE shared `OrientResult` envelope with `command = CHECK_COMMAND`.

The daemon handler [OBSERVED, first-hand: dispatch.rs:2672-2732] resolves the repo, acquires a read lock,
stamps `now` (waiver expiry), calls `run_check`, sets `display_name` on the result (:2718), serializes, and
returns. **It injects NOTHING else** — no trust overlay, no extra keys (§1d).

### 1a. Signals emitted (always repo focus)

check emits AT MOST TWO signals, in this order before ranking [OBSERVED: check/mod.rs:112-126]:

| Signal (code) | When | Builder (OBSERVED file:line) | Severity (OBSERVED signal.rs) | Source today |
|---|---|---|---|---|
| `CHECK_PASS` \| `CHECK_FAIL` \| `CHECK_INCOMPLETE` (the verdict) | ALWAYS (exactly one of the three) | `build_verdict_signal` mod.rs:208-252; `Signal::check_pass`:1073 / `check_fail`:1088 / `check_incomplete`:1103 | `CheckPass`→(Check, **Low**) signal.rs:395; `CheckFail`→(Check, **High**):396; `CheckIncomplete`→(Check, **Medium**):397 | DERIVED from the conditions (multi-source — see §1b) |
| `SNAPSHOT_INFO` | ONLY when a snapshot exists | mod.rs:115-122; `Signal::snapshot_info`:1325 | `SnapshotInfo`→(Informational, **Low**):413 | SQLite `snapshots` (scope/basis_commit/created_at/uid) |

The verdict signal carries the conditions as nested evidence [OBSERVED build_verdict_signal mod.rs:208-252;
evidence structs signal.rs:615/:622/:630]:
- `CHECK_PASS` → `CheckPassEvidence { conditions: Vec<CheckConditionEvidence> }` (ALL conditions, all passing).
- `CHECK_FAIL` → `CheckFailEvidence { fail_conditions, passing }` (split by status).
- `CHECK_INCOMPLETE` → `CheckIncompleteEvidence { incomplete_conditions, fail_conditions, passing }`.
- Each `CheckConditionEvidence = { code, status, summary }` [OBSERVED condition_to_evidence mod.rs:255-265].

### 1b. The condition set (the verdict's sub-facts) + per-condition source — THE COMPLETENESS CORE

The verdict is a reduction over a `Vec<ConditionResult>`. The condition COUNT depends on snapshot existence
[OBSERVED evaluate.rs:19-186 + the test oracle reduce.rs:305-326 asserting EXACTLY 6 when a snapshot exists]:
- **No snapshot:** EXACTLY ONE condition — `SNAPSHOT_EXISTS` (status Incomplete); conditions 2-6 are NOT
  evaluated (early return evaluate.rs:36). [OBSERVED reduce.rs:288-301 `no_snapshot_only_evaluates_snapshot_exists`.]
- **Snapshot exists:** EXACTLY SIX conditions, in this fixed order [OBSERVED reduce.rs:314-325]:

| # | Condition code | Status logic (OBSERVED evaluate.rs) | Backing input (OBSERVED file:line) | Source today | Layer |
|---|---|---|---|---|---|
| 1 | `SNAPSHOT_EXISTS` | Pass if snapshot present; else Incomplete (early-return) | `get_latest_snapshot` mod.rs:60 | SQLite `snapshots` | A2 operational |
| 2 | `INDEX_NOT_EMPTY` | Pass if `files_total > 0`; else Incomplete | `snapshot.files_total` mod.rs:95 | SQLite `snapshots` | A2 operational |
| 3 | `STALE_FILES` | Pass if `stale_file_count == 0`; else **Fail** | `get_stale_files` mod.rs:81 | SQLite `files`/`file_versions` | A2 operational freshness |
| 4 | `CALL_GRAPH_RELIABILITY` | High/Medium→Pass (MEDIUM advisory, **check-specific** policy); Low→Fail; None→Incomplete | `get_trust_summary(...).call_graph_reliability.level` mod.rs:84/97 | SQLite trust-core (v1) | 1 |
| 5 | `ENRICHMENT_STATE` | Ran/NotApplicable→Pass; NotRun→Fail; None→Incomplete | `get_trust_summary(...).enrichment_state` mod.rs:84/98 | SQLite trust-core (v1) | 1 |
| 6 | `GATE_STATUS` | Pass/Fail/Incomplete per gate; NotConfigured→Pass (**check-specific** policy); None→Incomplete | `gather_gate_outcome` mod.rs:87 → `get_active_requirements` + `assemble_from_requirements` mod.rs:173/182 | SQLite `declarations` (**Authority**) | 4 |

POLICY NOTES (check-specific interpretations, OBSERVED in evaluate.rs, NOT inherited from the trust/gate
contracts — preserve verbatim): `CALL_GRAPH_RELIABILITY` MEDIUM → Pass ("safe enough to act on",
evaluate.rs:74-77); `GATE_STATUS` NotConfigured → Pass ("no policy = no violation", evaluate.rs:143-145).

PROVENANCE NOTE — GATE_STATUS is Authority-sourced for EVERY snapshot-present check, INCLUDING NotConfigured
[OBSERVED, first-hand: check/mod.rs:87 + gather_gate_outcome mod.rs:166-180]. When a snapshot exists,
`run_check` calls `gather_gate_outcome` UNCONDITIONALLY (mod.rs:87) and stores `gate_outcome: Some(...)`
(mod.rs:99), so the GATE_STATUS condition is ALWAYS evaluated. `gather_gate_outcome` reads
`get_active_requirements` (the `declarations` Authority table) at mod.rs:173 BEFORE the
`requirements.is_empty()` → `NotConfigured` early return (mod.rs:178-180). Therefore the declarations Authority
table is READ on every snapshot-present run — a NotConfigured outcome means "the declarations table was asked
and returned no active requirements", NOT "the declarations table was not consulted". CONSEQUENCE for the
source map (§2/§3a/§3b/D-V1): a snapshot-present verdict's `provenance.source` ALWAYS includes `declaration`,
never only when a requirement produced a non-default (Pass/Fail) outcome. The no-snapshot branch (mod.rs:62-76)
sets `gate_outcome: None` and NEVER calls `gather_gate_outcome`, so it reads no `declarations` →
`provenance.source = { sqlite }` only.

The verdict reduction [OBSERVED reduce.rs:18-41]: Incomplete if ANY condition Incomplete; else Fail if ANY
Fail; else Pass. So the verdict is a STRICT MEET over condition STATUS (Incomplete < Fail < Pass).

### 1c. Envelope-level fields (OBSERVED, first-hand: check/mod.rs:128-152)

| Field | Value today (OBSERVED) | Source class |
|---|---|---|
| `schema` | `ORIENT_SCHEMA` (compile-time const) | static |
| `command` | `CHECK_COMMAND` (compile-time const) | static |
| `repo` | `repo.name` (mod.rs:131) — the repo NAME, looked up by `get_repo(repo_uid)` | SQLite `repos` |
| `display_name` | `None` from the use case (mod.rs:132); daemon injects `Some(display_name)` (dispatch.rs:2718) | daemon operational metadata |
| `snapshot` | `snapshot.snapshot_uid` when present, else `""` (mod.rs:78/134; empty string on no-snapshot, mod.rs:75) | SQLite `snapshots` |
| `focus` | `Focus::repo()` ALWAYS (mod.rs:134) — check has NO focus dispatch | static (repo only) |
| `confidence` | snapshot present: `derive_repo_confidence(&trust, stale)` (mod.rs:91; confidence.rs:43-70); no snapshot: STATIC `Confidence::Low` (mod.rs:75) | SQLite trust-core (with-snapshot); static (no-snapshot) |
| `documentation` | `None` ALWAYS (mod.rs:137) — check builds NO documentation section (unlike orient) | static `None` |
| `signals[]` | the verdict signal (+ SNAPSHOT_INFO when snapshot present), after `sort_and_rank` (mod.rs:126) | derived (§1a) |
| `signals_truncated` / `signals_omitted_count` | `None` ALWAYS (mod.rs:140-141) — ≤2 signals never truncate | static `None` |
| `limits[]` | `Vec::new()` ALWAYS (mod.rs:143) — check emits NO limits (unlike orient's COMPLEXITY/MODULE_DATA_UNAVAILABLE) | static empty |
| `limits_truncated` / `limits_omitted_count` | `None` ALWAYS (mod.rs:144-145) | static `None` |
| `next[]` | `Vec::new()` ALWAYS (mod.rs:147) — check emits NO next-actions | static empty |
| `next_truncated` / `next_omitted_count` | `None` ALWAYS (mod.rs:148-149) | static `None` |
| `truncated` | `false` ALWAYS (mod.rs:151) | static |

### 1d. What `rmap check` does NOT emit (the negative space — load-bearing for completeness)

The orient draft was rejected for omitting signals; this section pins the OPPOSITE risk — falsely
attributing orient-only surfaces to check. ALL verified first-hand:
- **NO LiveGraph read.** check's handler has no LiveGraph branch; the use case takes only an `AgentStorageRead
  + GateStorageRead` handle (the SQLite `StorageConnection`, dispatch.rs:2701). check touches ZERO migrated
  SQLite-free surfaces (callers/callees/path/imports/cycles/stats) — none of its inputs IS one of them.
  Therefore check has ZERO LG-first leaves and gets NO cert-gated fastpath. [OBSERVED, first-hand.]
- **NO daemon trust overlay / NO `trust_briefing` analogue.** `handle_check` (dispatch.rs:2716-2725) sets
  `display_name`, serializes, returns. It does NOT call `compute_trust_overlay_for_snapshot` and does NOT
  inject a post-serialize `trust` key. CONTRAST `handle_explain` (dispatch.rs:2801-2816) which DOES inject
  `trust` when degraded, and orient which does the same (the D-ORIENT-6 subject). The `CheckResponse`
  renderer has NO `trust` field either (presentation/check.rs:32-45; full human surface enumerated §1e). =>
  check has NO analogue to orient's `trust_briefing`; D-ORIENT-6 is structurally ABSENT here (D-CHECK-2).
  [OBSERVED, first-hand.]
- **NO focus dispatch.** check is always `Focus::repo()` (mod.rs:134). There is no file/path/symbol pipeline,
  hence no callers/callees summary and no symbol-focus posture. => D-ORIENT-SYMBOL-CALLGRAPH is structurally
  ABSENT here. [OBSERVED, first-hand.]
- **NO limits, NO next-actions, NO documentation section** (mod.rs:137/143/147). These envelope slots are
  always empty/None for check.
- **NO measurements / boundary / module / cycle / dead-code signals.** check evaluates only the 6 readiness
  conditions; it surfaces none of orient's structural signals.

Net: **check's complete daemon/API output surface = the shared OrientResult envelope (CHECK_COMMAND) + daemon
display_name; signals ∈ {verdict, optional SNAPSHOT_INFO}; the verdict nests 1 or 6 conditions; everything
else is empty/None.** Nothing is LiveGraph-served; nothing is a fastpath candidate. The downstream HUMAN CLI
renderer surface (what a person sees) is enumerated separately in §1e, and the CLI PROCESS-wrapper surface
(stdout/stderr + EXIT CODES — what CI and shell callers observe) in §1f. All THREE surfaces — daemon/API
envelope (§1a-1d), human render content (§1e), process wrapper (§1f) — are check's complete current output.

### 1e. The human CLI renderer output surface (OBSERVED, first-hand: rgr/src/presentation/check.rs)

The daemon returns the OrientResult JSON (§1a-1d). The CLI human renderer is `CheckResponse::render_human`
[OBSERVED presentation/check.rs:79-98]. It deserializes a SUBSET of the envelope into `CheckResponse`
[check.rs:32-45] and emits exactly the lines below — this is the COMPLETE human surface `rmap check` produces
today. (Enumerated first-hand because the selection packet requires EVERY current output surface, and the
human renderer is a distinct surface from the API envelope.)

| Rendered surface (what the user sees) | Built from (OBSERVED file:line) | Source posture today → under this slice |
|---|---|---|
| `Repo: <name>` line | `display_name.as_deref().unwrap_or(&self.repo)` (check.rs:84-85) — prefer the daemon-injected `display_name`, else fall back to the `repo` field | daemon operational metadata, else SQLite `repos` (§1c). Unchanged by this slice. |
| `Verdict: <PASS\|FAIL\|INCOMPLETE>` line | `determine_verdict()` (check.rs:88-89/:100-110) — scans `signals[].code` for `CHECK_PASS`→PASS / `CHECK_FAIL`→FAIL / `CHECK_INCOMPLETE`→INCOMPLETE; `UNKNOWN` if none found | DERIVED from the verdict signal (§1a; multi-source SQLite+Authority, §1b). Under this slice GAINS the freshness suffix (PASS@Fresh / PASS@Stale / INCOMPLETE@Unavailable — §3c, §5 W2). |
| `Incomplete conditions` heading + `- CODE: summary` bullets | `render_conditions` from `evidence.incomplete_conditions`, emitted ONLY when non-empty (check.rs:130-136) | the verdict leaf's nested conditions (SQLite/Authority, §1b). Text byte-unchanged (P1). |
| `Failing conditions` heading + bullets | `evidence.fail_conditions`, ONLY when non-empty (check.rs:138-145) | same as above |
| `Passing conditions` heading + bullets | `evidence.conditions` for CHECK_PASS, else `evidence.passing`, ONLY when non-empty (check.rs:147-160) | same as above |

NOT rendered today (deserialized but suppressed — OBSERVED first-hand):
- `snapshot`: the field carries `#[allow(dead_code)]` (check.rs:40-41); it is DESERIALIZED but NEVER printed
  (the test `render_hides_internal_fields` check.rs:270-281 pins this).
- `confidence`: deserialized into `CheckResponse.confidence: String` (check.rs:42) but `render_human` emits
  ONLY Repo + Verdict + conditions (check.rs:79-98) — confidence is NEVER written to the human surface today.
- The `CheckResponse` struct has NO `trust` field at all (check.rs:32-45) — first-hand confirmation that check
  has no trust overlay to render (D-CHECK-2). CONTRAST orient's renderer, which carries
  `OrientResponse.trust: Option<TrustOverlay>` [OBSERVED via the orient slice: presentation/orient.rs:83].

CONSEQUENCE FOR THIS SLICE (INFERRED, grounded in the OBSERVED renderer): the renderer reads `signals[].code` +
`signals[].evidence` (check.rs:48-66); under the wrapper those move under `value.signals[*].value` (the re-typed
slot, §3b). The verdict + condition TEXT stays byte-identical (P1); the renderer gains ONLY the freshness suffix
on the Verdict line (§5 W2). `confidence` and `snapshot` remain JSON-only — this slice does NOT start rendering
them (no scope creep; surfacing confidence would be a render-only choice, not a contract change, §5 W2). The
absent `trust` field stays absent (validation E5). No other human-surface line changes.

### 1f. The CLI process-wrapper surface (`run_check_cmd`) — stdout/stderr + EXIT CODES (OBSERVED, first-hand: rgr/src/commands/orient.rs:222-331)

§1a-1e enumerate the daemon/API envelope and the human-render CONTENT. But `rmap check` is also a PROCESS: the
binary entry `run_check_cmd` [OBSERVED orient.rs:222-331] parses args, resolves cwd, connects to the daemon,
calls `client.request("check", ...)`, and maps the outcome to STDOUT/STDERR + an EXIT CODE. This wrapper is a
DISTINCT output surface from the envelope content — it is the surface CI and shell callers observe — and check
is **CI-FACING**: it DERIVES a non-trivial 0/1/2 exit code from the verdict signal [OBSERVED orient.rs:278-290].
This is the load-bearing contrast with orient, whose `run_orient_cmd` returns `ExitCode::SUCCESS` in BOTH
success arms [OBSERVED orient.rs:175,:187] and so derives NO signal-dependent exit code — which is why orient's
slice needed no exit-code section and check's does. Enumerated first-hand because the selection packet requires
EVERY current output/signal and review-2 flagged this surface as omitted.

| Wrapper output (OBSERVED orient.rs:line) | Channel | Exit | Source posture today → effect of this slice |
|---|---|---|---|
| `--json` sets json_mode; only `--json` accepted (:226-242) | — | — | static CLI arg parser (local process). Unchanged. |
| unknown `--flag` → `error: unknown flag: {flag}` + `usage: rmap check [--json]` (:231-235) | stderr | **1** | static arg parser; NO daemon contacted. Unchanged. |
| unexpected positional arg → `error: unexpected argument: {other}` + usage (:236-240) | stderr | **1** | static arg parser; NO daemon contacted. Unchanged. |
| `current_dir()` fails → `error: cannot get current directory: {e}` (:245-250) | stderr | **2** | process environment (local); NO daemon. Unchanged. |
| `canonicalize()` fails → `error: cannot canonicalize current directory: {e}` (:253-258) | stderr | **2** | filesystem (local); NO daemon. Unchanged. |
| `DaemonClient::new()` fails → `error: {e}` (:262-267) | stderr | **2** | transport/client connect (`DaemonClientError::ConnectionFailed` — the socket-absent path EXECUTED this turn, §4). Unchanged. |
| request `Err(DaemonError{code="RepoNotFound"})` → `error: repo not indexed` + `hint: run 'rmap index .' to index this repo` (:318-321) | stderr | **2** | daemon / repo registry (RepoNotFound from the daemon registry resolve). Unchanged. |
| request `Err(DaemonError{code,message})` (other) → `error: {code}: {message}` (:322-323) | stderr | **2** | daemon error. Unchanged. |
| request `Err(e)` (Send/Read/InvalidResponse/Timeout — connection.rs:42-64) → `error: {e}` (:327-329) | stderr | **2** | transport/client (post-connect failures). Unchanged. |
| `--json` success → `to_string_pretty(&result)` to stdout (:294-297) | stdout | derived (↓) | the daemon envelope VERBATIM. Under this slice prints the FULL `CoherenceEnvelope<CoherentOrientResult>` (was the bare OrientResult). |
| `--json` serialize error → `error: {e}` (:299-301) | stderr | **2** | local serializer over the daemon envelope. Unchanged. |
| human success → `from_value::<CheckResponse>(result)` then `render_human()` to stdout (:306-308) | stdout | derived (↓) | renderer projection of the daemon envelope (§1e). Under this slice CheckResponse projects `value` (§3e). |
| human parse/render error → `error: failed to parse check response: {e}` (:311-312) | stderr | **2** | local deserializer/renderer over the daemon envelope. Unchanged. |
| SUCCESS EXIT CODE (both modes), mapped from the verdict signal code (:278-290): `CHECK_PASS`→**0**, `CHECK_FAIL`→**1**, `CHECK_INCOMPLETE`→**2**, none-found→`.unwrap_or(2)` (:290) | exit | **0/1/2** | DERIVED from the daemon envelope's verdict signal `code`. EXTRACTION PATH CHANGES under the wrapper (§3e); value mapping PRESERVED. |

EXIT-CODE SEMANTICS (check's CI contract, OBSERVED orient.rs:278-290 — preserve verbatim): `CHECK_PASS`=0,
`CHECK_FAIL`=1, `CHECK_INCOMPLETE`=2, fallback (no verdict signal/code found) = 2 (`.unwrap_or(2)`). INCOMPLETE
and fallback are BOTH 2; parse/serialize/transport/daemon errors are ALSO 2; only usage errors are 1. This
slice changes the EXTRACTION PATH (where the code is read in the JSON), NOT the value mapping (§3e; validation
§5 CLI-WRAPPER).

CRITICAL (the load-bearing review-2 finding): the exit code is read from the TOP-LEVEL `result["signals"][*]["code"]`
(:278-282) and the human render deserializes the TOP-LEVEL `result` into `CheckResponse` (:306). The ratified
wrapper moves `signals` UNDER `value` and gives each signal leaf a `.value` (contract D7 / §3b). If the
implementation wraps the daemon envelope but does NOT update these two CLI reads, `result["signals"]` resolves
to null → `.unwrap_or(2)` silently returns exit 2 for EVERY check, INCLUDING a PASS — a SILENT CI BREAKAGE (a
green repo reports failure/incomplete). The exit-code index and the CheckResponse deserialization are SEPARATE
code paths inside `run_check_cmd`; BOTH must shift to `value` in lockstep with the daemon change (§3e), and
validation pins BOTH (§5 CLI-WRAPPER).

---

## 2. Per-output source map (the field-level boundary)

Legend (per COHERENCE-LAYER-1 §source map): **SQLite-first** = SQLite is source of truth (Q5). **Authority** =
Tier-A1 `declarations`, permanent SQLite, overlays-never-erases. **A2** = operational SQLite (snapshots/files
— not rebuildable structure). Layer = Fact Certainty Model layer. **There is NO `LG-first` row for check** —
that absence IS the finding. This table REFINES the contract's check row (coherence-layer-1.md:325-335) with
first-hand per-condition granularity; no posture here contradicts the contract.

| Output / sub-fact | Layer | Target posture | LiveGraph surface | Notes |
|---|---|---|---|---|
| `repo` / `snapshot` identity, `SNAPSHOT_INFO`, `INDEX_NOT_EMPTY` | A2 | **SQLite-first** | — (none) | Operational identity + index size; not rebuildable structure. `snapshot=""` on no-snapshot. |
| `STALE_FILES` (snapshot-vs-worktree drift) | A2 | **SQLite-first** | — | The freshness DRIVER: `get_stale_files` non-empty ⇒ snapshot Stale (§3c). |
| `CALL_GRAPH_RELIABILITY`, `ENRICHMENT_STATE`, `confidence` | 1 | **SQLite-first** (trust-core v1) | — | The outgoing-extractor reliability core. The hybrid rebase is TRUST-LIVEGRAPH-1, NOT here; if that slice later changes what `get_trust_summary` returns, check inherits it through the port with no edit (D-CHECK-2 note). |
| `GATE_STATUS` | 4 | **Authority — SQLite-first** | — | Requirement/obligation/waiver evaluation via `declarations`; the `get_active_requirements` read makes this Authority-sourced on EVERY snapshot-present check, INCLUDING NotConfigured (§1b PROVENANCE NOTE; mod.rs:173). No LiveGraph home by construction (contract Q2a). Overlays-never-erases — reuses contract D5 (no dedicated check decision); see RISK-C-C + validation D-V6. |
| verdict (`CHECK_PASS`/`FAIL`/`INCOMPLETE`) | — | **DERIVED; MULTI-SOURCE; carries MEET freshness** | — | A reduction over the rows above; `provenance.source` = the RATIFIED set `BTreeSet<Source>`{sqlite, declaration} for a snapshot-present verdict (GATE_STATUS always reads `declarations` — §1b PROVENANCE NOTE; multi-source leaf provenance ratified — contract D8), {sqlite} on no-snapshot; freshness = MEET (§3a/§3c). check's single check-specific envelope design point: ONE composite leaf (D-CHECK-1) carrying the multi-source set (D-CHECK-5 / D8). |
| `documentation` | — | **n/a** | — | check emits none (mod.rs:137); orient's FS doc-scan is NOT a check surface. |

**Net for check: ZERO LG-first outputs. Every output is SQLite-first, A2-operational, or Authority.** This is
the contract's check row made first-hand-precise (coherence-layer-1.md:325-335). [INFERRED mapping from the
OBSERVED §1 outputs onto the OBSERVED contract postures; no posture diverges from the contract.]

This table maps the daemon/API FIELD-level provenance. The CLI PROCESS-wrapper outputs (usage / cwd /
canonicalize / transport / daemon errors + the derived exit code) have their OWN source postures — static
parser, process environment, filesystem, transport/client, daemon/registry, and the verdict-signal-DERIVED
exit code — enumerated with file:line in §1f. They are process-level, not field-level, so they are NOT
LiveGraph-vs-SQLite postures and do not affect the ZERO-LG-first finding.

---

## 3. CoherenceEnvelope<T> wiring for check (INFERRED, grounded in the RATIFIED contract)

Per COHERENCE-LAYER-1 §"The shared coherence answer-envelope" (RATIFIED Option B), the wrapper is applied
COMPOSITIONALLY at two granularities. check is the SECOND command to instantiate it and the FIRST to wrap a
command with NO LG-first leaf — so for check the wrapper carries ONLY honest labels, never a fastpath toggle.

### 3a. Leaf — `CoherenceEnvelope<Signal>` (one per emitted signal)

```text
check emits at most two signals, so at most two leaves. The inner `Signal` payload stays PRISTINE (the
contract's Option-B principle); provenance/trust/freshness ride in the wrapper SIBLING fields.

LEAF 1 — the VERDICT signal (CHECK_PASS | CHECK_FAIL | CHECK_INCOMPLETE). This is a MULTI-SOURCE COMPOSITE
  leaf (D-CHECK-1 = Option A: ONE leaf for the whole verdict, NOT one leaf per condition):
    value             = the CHECK_* Signal, conditions nested in its evidence, UN-widened (D-CHECK-1).
    provenance.source = the contributing-source SET (a `BTreeSet<Source>`; multi-source LEAF provenance is
                        RATIFIED — contract D8 / D-CHECK-5), i.e. the condition sources actually READ:
                          { sqlite }                (SNAPSHOT_*/INDEX/STALE_FILES/CALL_GRAPH/ENRICHMENT)
                          ∪ { declaration }          (GATE_STATUS — present for EVERY snapshot-present verdict:
                                                      gather_gate_outcome reads get_active_requirements, the
                                                      declarations Authority table, on every snapshot-present
                                                      run, EVEN when it returns empty → NotConfigured; the
                                                      declaration source is NOT conditioned on a Pass/Fail gate
                                                      outcome. OBSERVED mod.rs:87/173/178, §1b PROVENANCE NOTE)
                        On no-snapshot the gate is NOT evaluated (gate_outcome=None, mod.rs:74; gather_gate_outcome
                        never called) and the only condition is SNAPSHOT_EXISTS → source = { sqlite }.
    trust (TrustPosture) = the MEET of the contributing conditions' postures. completeness = Complete when
                        all applicable conditions were EVALUABLE (no None/Incomplete-from-missing-data),
                        else Degraded; class = Exact only when Complete AND Fresh (invariant I1), else
                        Partial/Unavailable. An INCOMPLETE verdict (a condition could not be evaluated) is
                        NEVER Exact (it is Partial/Degraded with a DegradationReason).
    freshness (FreshnessState) = the MEET of the contributing inputs' freshness = the SNAPSHOT freshness
                        (§3c): Fresh (snapshot present, no stale files) | Stale (stale files present) |
                        Unavailable (no snapshot). Computed INDEPENDENTLY of the Pass/Fail/Incomplete verdict
                        (the 2-axis model, §3c).

LEAF 2 — SNAPSHOT_INFO (present only when a snapshot exists):
    value             = the SnapshotInfo Signal (uid/scope/basis_commit/created_at), pristine.
    provenance.source = { sqlite } (the `snapshots` row).
    trust             = Exact/Complete (pure identity metadata for THIS snapshot — no derivation, no
                        cross-source fold).
    freshness         = the snapshot freshness (Fresh/Stale per §3c). Identity is reported for the snapshot
                        as-recorded; staleness rides on the freshness label, not on the identity value.

Leaf construction MUST delegate to (or mirror) the AnswerEnvelope smart constructors so the six invariants
hold AT THE LEAF (contract §invariant preservation I1-I6). The verdict leaf is the first to fold MANY
sub-fact postures into ONE leaf posture; that internal fold is the MEET and MUST be monotone (it can only
LOWER class/freshness/completeness — it can never manufacture an Exact from a non-Exact/Stale condition).
```

### 3b. Root — `CoherenceEnvelope<CoherentOrientResult>` (per command)

```text
The root `value` is the SAME `CoherentOrientResult` container orient ratified (contract D7) = `OrientResult`
with its `signals` slot re-typed `Vec<Signal>` -> `Vec<CoherenceEnvelope<Signal>>`. check reuses it
UNCHANGED. The orient-added `trust_briefing: Option<TrustOverlaySummary>` field exists on the shared
container, and for check it is ALWAYS `None` (check's handler never injects a trust overlay, §1d / D-CHECK-2)
— matching orient's W3 "a check/explain coherent response serializes with NO `trust_briefing` key"
(orient-livegraph-1.md:762-764). check adds NO new field of its own.

  root.value      = CoherentOrientResult {
                      ... ,                                   // all OrientResult fields verbatim (§1c)
                      signals: Vec<CoherenceEnvelope<Signal>>,    // the verdict leaf (+ SNAPSHOT_INFO leaf)
                      trust_briefing: None,                       // ALWAYS None for check (D-CHECK-2)
                    }
  root.provenance = { source: SET union of leaf sources = { sqlite } on no-snapshot; { sqlite, declaration }
                      for EVERY snapshot-present verdict (GATE_STATUS always reads the declarations Authority
                      table via get_active_requirements, even on NotConfigured — §1b PROVENANCE NOTE /
                      mod.rs:87/173); basis/fallback_reason: there is NO LG fallback for check, so
                      fallback_reason is ALWAYS null and missing_partitions ALWAYS empty (no LiveGraph read) }
  root.trust      = the MEET fold of the leaf TrustPostures (contract D3 — greatest-lower-bound, monotone).
  root.freshness  = the MEET fold of the leaf freshness states (= the snapshot freshness; both leaves share
                    it).
  CoherentOrientResult.confidence is DERIVED from the root MEET and NEVER exceeds the weakest contributor
  (D-CHECK-3, mirroring orient D-ORIENT-4). The legacy derive_repo_confidence(trust, stale) result
  (mod.rs:91) becomes ONE input to the MEET, not the sole confidence source; on no-snapshot the static
  Confidence::Low (mod.rs:75) is preserved and is already the lattice bottom for confidence.

  NO ZERO-LEAF ROOT CASE. Unlike orient (whose ambiguous/no-match builders emit zero signals), check ALWAYS
  emits at least the verdict signal — even on no-snapshot it emits exactly the CHECK_INCOMPLETE verdict
  (mod.rs:112 runs before the snapshot check; SNAPSHOT_INFO is the only conditional one). So the root MEET
  ALWAYS has >= 1 leaf; orient's empty-fold-to-TOP hazard (orient D-ORIENT-4 zero-signal branch) does NOT
  arise for check. [OBSERVED: mod.rs:112 builds the verdict signal unconditionally; SNAPSHOT_INFO at :115 is
  the only snapshot-gated signal.]
```

### 3c. The 2-axis model (verdict × freshness) — check's coherence enrichment

```text
check today has ONE axis: the verdict ∈ {Pass, Fail, Incomplete} (a MEET over condition STATUS, reduce.rs:18-41).
The coherence wrapper ADDS an ORTHOGONAL freshness axis ∈ {Fresh, Stale, Unavailable}, so a verdict is now
reported as (verdict, freshness):

  freshness mapping (the contract's "verdict carries MEET freshness of the above", coherence-layer-1.md:332):
    - snapshot present, get_stale_files EMPTY        -> Fresh
    - snapshot present, get_stale_files NON-EMPTY    -> Stale   (the STALE_FILES condition's input)
    - no snapshot                                    -> Unavailable
  The MEET freshness equals the SNAPSHOT freshness because every contributing input (snapshot identity,
  stale-files, trust-core, gate) is computed over the SAME snapshot_uid (mod.rs:78-99); the snapshot is the
  single freshness unit in check's SQLite-only world. [INFERRED from the OBSERVED single-snapshot gather.]

  WHY independent of the verdict: the contract example "a PASS over a Stale trust-summary is a Stale PASS,
  never a Fresh PASS" (coherence-layer-1.md:153) demands the freshness label be computed from the INPUTS, not
  the verdict. OBSERVED COUPLING (recorded honestly): in check's CURRENT condition logic a non-empty
  get_stale_files makes STALE_FILES Fail (evaluate.rs:61-69), which forces the verdict to Fail
  (reduce.rs:36-38). So today a PASS is ALWAYS Fresh and "PASS@Stale" is not reachable from stale FILES. The
  coherence layer STILL computes freshness independently so that (a) the contract's MEET rule holds literally,
  (b) the label stays honest if the verdict logic or input sources ever change (e.g. a future trust-core
  staleness not captured by per-file staleness), and (c) FAIL@Stale vs FAIL@Fresh is distinguishable (a FAIL
  caused by stale files is Stale; a FAIL caused only by a gate violation over a Fresh snapshot is Fresh).

  This 2-axis model is the WHOLE of check's coherence contribution: no fastpath, no LiveGraph — just honest
  labelling so a consumer never reads check's snapshot-scoped PASS as a current-state claim (RISK-C-A).
```

### 3d. Reconciliation points implied by adopting the wrapper (RP-1 RESOLVED/ratified; RP-2 contract-deferred)

```text
RP-1 (the MULTI-SOURCE LEAF — check is the first command to EXERCISE it) — RESOLVED, not deferred. check's
  verdict leaf is the FIRST place a LEAF (not just the root) holds a source SET. The iteration-1 draft DEFERRED
  this shape to COHERENCE-ENVELOPE-1 and self-"DECIDED" it; the reviewer ESCALATED it as a boundary data-shape
  decision (CHECK-PROVENANCE-LEAF-SHAPE, review-1.json). The operator RATIFIED multi-source LEAF provenance
  (2026-06-09) and the COHERENCE-LAYER-1 contract is AMENDED (its D8) so `Provenance.source` is a
  `BTreeSet<Source>` at BOTH leaf and root — eliminating the prior leaf-single (coherence-layer-1.md ~:397) /
  root-set (~:433) tension at the source. check's verdict leaf therefore carries the RATIFIED set
  `{sqlite, declaration}` (snapshot-present) / `{sqlite}` (no-snapshot). COHERENCE-ENVELOPE-1 now BUILDS that
  ratified field; it no longer DECIDES its shape. See D-CHECK-5.
RP-2 (Signal.freshness reconciliation, = contract RISK-G). The shared Signal DTO already carries
  `freshness: Option<FreshnessInfo>` (Current/Impacted/Unknown from artifact_contracts) [OBSERVED via
  contract RISK-G citing signal.rs:89-98,:958] — a different vocabulary from the leaf's FreshnessState. check's
  signals (verdict, SNAPSHOT_INFO) are NOT among the artifact-contract-tracked ones today, so check is
  unlikely to populate the inner field; regardless, the OUTER leaf freshness is authoritative. The single
  FreshnessInfo->FreshnessState mapping is COHERENCE-ENVELOPE-1's (same as orient D-ORIENT-7); RECORDED,
  contract-deferred, not re-decided here.
```

### 3e. CLI-wrapper exit-code + human-render remap under the wrapper (INFERRED, forced by the ratified shape)

```text
The wrapper is daemon-INTERNAL (it re-shapes what handle_check serializes). But run_check_cmd reads that
serialized shape in TWO places (§1f), so adopting the wrapper FORCES a mechanical remap in the CLI — NOT a new
decision, a direct consequence of the ratified `value`-nesting (contract D7 / §3a/§3b). Both reads must move in
lockstep with the daemon change, or check's PROCESS contract (its exit code / human render) silently breaks.

EXIT-CODE EXTRACTION (OBSERVED orient.rs:278-290):
  today:  result["signals"][ N ]["code"]                       (top-level signals; bare OrientResult)
  after:  result["value"]["signals"][ N ]["value"]["code"]     (signals under `value`; each leaf carries `value`)
          — or an EQUIVALENT TYPED parse of CoherenceEnvelope<CoherentOrientResult> that reads the verdict
            leaf's inner Signal.code. Either realization is acceptable (a local implementation detail —
            decide-and-record, CLAUDE.md §Decision Autonomy). The REQUIREMENT is that the VALUE MAPPING is
            preserved verbatim: CHECK_PASS->0, CHECK_FAIL->1, CHECK_INCOMPLETE->2, and signal/code-not-found->2
            (the existing `.unwrap_or(2)` fallback at :290). This slice does NOT introduce new exit codes or
            re-map existing ones — that would change check's CI contract (a boundary change) and is out of scope.

HUMAN-RENDER DESERIALIZATION (OBSERVED orient.rs:306):
  today:  serde_json::from_value::<CheckResponse>(result)      (CheckResponse over the bare OrientResult)
  after:  the wrapper's `value` projects into CheckResponse — CheckResponse deserializes the CoherentOrientResult
          carried under `value`, reading each signal leaf's inner `.value` for code + evidence (§1e / §5 W2) —
          OR run_check_cmd unwraps `result["value"]` before from_value. Render CONTENT stays byte-identical
          except the added freshness suffix on the Verdict line (§5 W2); the absent `trust` field stays absent
          (§5 E5).

ORDER (OBSERVED orient.rs:276-316): the exit code is computed FIRST, INDEPENDENT of json/human mode, and only
THEN the mode branch prints. So the exit-code index (:278) and the CheckResponse parse (:306) are SEPARATE,
independent reads of the same `result`; updating one without the other leaves a latent break. A correct
implementation changes the daemon serializer (handle_check emits the wrapper) AND both CLI reads together; a
wire-shape fixture/contract test (§5 W1/W3) pins the shape so the two reads cannot silently drift from the
daemon output. The exit-code remap and both validation cases are PARITY obligations, not enhancements: a green
repo must still exit 0 after the wrapper lands.
```

---

## 4. Degradation / safe-fallback behaviour for check (honest labelling, no false completeness)

```text
check has NO LG-first leaf, so it has NO cert ladder and NO LiveGraph->SQLite fallback. Its degradation is
ENTIRELY about honest freshness/completeness labelling of the SQLite/Authority answer. Three classes:

MEET-FRESHNESS VERDICT (the core, contract CHECK row, coherence-layer-1.md:332 + safe-fallback :485-487):
  - PASS/FAIL/INCOMPLETE inherit the MEET freshness of (snapshot, stale-files, trust-core, gate). A PASS over
    a Stale snapshot is reported PASS@Stale; a PASS over a Fresh snapshot is PASS@Fresh. Exit code (if any)
    unchanged; the freshness label is explicit and additive. NEVER report PASS@Fresh over a non-Fresh input
    (forbids contract F2).
  - INCOMPLETE remains the honest verdict when a required input is Unavailable or unevaluable. REACHABLE
    triggers in the current wiring: no snapshot (-> root.freshness Unavailable, §4 NO-SNAPSHOT); an EMPTY index
    (files_total==0 -> INDEX_NOT_EMPTY Incomplete, evaluate.rs:46-51); or a gate that returns
    GateOutcomeForCheck::Incomplete (storage error / missing evidence, mod.rs:175/191/202 -> GATE_STATUS
    Incomplete, evaluate.rs:162-167). INCOMPLETE is NEVER dressed as PASS/FAIL. (HONESTY NOTE, OBSERVED
    first-hand: the call_graph/enrichment/gate `None` arms at evaluate.rs:100/:132/:176 are DEFENSIVE — run_check
    always supplies `Some(...)` for these three when a snapshot exists, mod.rs:97-99 — so a None-input INCOMPLETE
    is NOT reachable through the current gather; it is a forward-looking guard, not a present degradation path.)

NO-SNAPSHOT (the principal degradation, OBSERVED mod.rs:62-76 + evaluate.rs:29-37):
  - verdict = CHECK_INCOMPLETE with EXACTLY one condition (SNAPSHOT_EXISTS, Incomplete). NO SNAPSHOT_INFO
    leaf. root.freshness = Unavailable; root.trust = Partial/Unavailable + a DegradationReason
    ("no READY snapshot"); confidence = the static Low (mod.rs:75). provenance.source = { sqlite }.
    This is "Unavailable is not empty" (contract F3 / architecture.md Rule 6 "null=unknown, empty=known-zero"):
    check does NOT serve an empty/PASS answer when there is nothing to judge — it serves INCOMPLETE@Unavailable.

GATE AUTHORITY OVERLAY NEVER ERASES (contract D5, reused): a waiver suppressing a gate FAIL produces
  GATE_STATUS = Pass at the EFFECTIVE layer, but the computed gate verdict is preserved upstream in the gate
  report (VISION §Agent Priorities #2). check consumes only the projected GateOutcomeForCheck
  (Pass/Fail/Incomplete/NotConfigured, types.rs:97-107), which is the EFFECTIVE (waiver-overlaid) outcome by
  the gate contract; the COMPUTED verdict remains queryable via `rmap gate`, not erased. check must keep
  labelling GATE_STATUS source=declaration so a consumer knows the verdict rode an Authority overlay.

TRANSPORT-LEVEL DEGRADATION (OBSERVED, first-hand, distinct from the envelope's internal labelling):
  [EXECUTED this turn: `rmap check` with the daemon down -> "error: daemon connection failed: socket does not
  exist: /Users/apple/Library/Application Support/repo-graph/daemon.sock".] When the daemon socket is absent
  the CLI NEVER reaches handle_check: it returns a CONNECTION ERROR and NO envelope at all. This is honest
  failure (a transport error, not a false-complete answer) and is OUTSIDE the CoherenceEnvelope's scope — the
  envelope models the daemon-INTERNAL source labelling, not client<->daemon transport. IMPLICATION FOR
  VALIDATION: check's coherence degradation is exercised daemon-side (agent unit/integration tests with a
  live RepoState), NOT through a socketless CLI. The socketless path is a separate, already-correct transport
  behaviour; this slice neither changes it nor depends on it (identical to orient §4 transport degradation).

NO FALSE-COMPLETENESS, enumerated against the contract's F-list:
  F2 (confidence HIGH over a stale/pending input): forbidden — confidence is MEET-capped (D-CHECK-3).
  F3 (empty as known-zero): forbidden — no-snapshot is INCOMPLETE@Unavailable, never an empty PASS.
  F5/F6 do not arise for check (no v1-trust report surface of its own; the gate overlay is preserved, not
    erased). F1/F4 do not arise (no LiveGraph structural section, no SCIP-dependent leaf).
```

---

## 5. Validation plan (for the eventual implementation)

```text
Off-target first (architecture.md §Off-Target Testability + §Build Order). The wrapper type + MEET fold live
in COHERENCE-ENVELOPE-1 (pure, unit-tested there); this slice validates the CHECK WIRING. check's pure
reducer already has a strong matrix (reduce.rs:55-327, 18 tests) — those MUST stay green unchanged (the
verdict logic is NOT touched; only the surrounding envelope gains labels).

PARITY (no discovery loss vs today's SQLite check):
  P1. The verdict + conditions VALUE payloads (CHECK_* signal, CheckPassEvidence/CheckFailEvidence/
      CheckIncompleteEvidence, the 6 condition codes + summaries, SNAPSHOT_INFO) are byte-identical to today's
      OrientResult value; only the surrounding wrapper gains labels. [Reuse the orient P1/P2 precedent.]
  P2. Signal ordering/ranking is unchanged (sort_and_rank runs post-build, mod.rs:126; wrapping is
      post-aggregation). limits[]/next[] stay empty; truncation flags stay None (§1c).
  P3. Condition-COUNT parity vs the OBSERVED oracle: snapshot present -> EXACTLY the 6 codes in order
      (reduce.rs:314-325); no snapshot -> EXACTLY SNAPSHOT_EXISTS (reduce.rs:288-301). The wrapper MUST NOT
      add/drop a condition.
  P4. The 3 verdict variants + their evidence splits (pass=conditions; fail=fail_conditions+passing;
      incomplete=incomplete+fail+passing, mod.rs:208-252) are unchanged.

DEGRADATION:
  D-V1. PASS over a FRESH snapshot (get_stale_files empty) -> verdict leaf + root freshness = Fresh;
        confidence = derive_repo_confidence unchanged; provenance.source = { sqlite, declaration }. The
        declaration source is present because a snapshot-present verdict ALWAYS reads the declarations Authority
        table via get_active_requirements (even when the gate is NotConfigured — §1b PROVENANCE NOTE /
        mod.rs:87/173/178); a PASS does NOT drop the declaration source. Add a sibling case D-V1b: PASS over a
        gate that is NotConfigured -> assert provenance.source STILL = { sqlite, declaration } (not { sqlite }).
  D-V2. FAIL caused by stale files (get_stale_files non-empty) -> STALE_FILES Fails AND freshness = Stale;
        assert FAIL@Stale (the freshness label present, distinct from a gate-only FAIL@Fresh).
  D-V3. FAIL caused ONLY by a gate violation over a FRESH snapshot (no stale files, GATE_STATUS Fail) ->
        verdict Fail, freshness Fresh; assert FAIL@Fresh — proving the freshness axis is INDEPENDENT of the
        verdict (§3c).
  D-V4. INCOMPLETE from no snapshot -> exactly one condition (SNAPSHOT_EXISTS Incomplete), NO SNAPSHOT_INFO
        leaf, root.freshness = Unavailable, confidence = static Low; assert "Unavailable != empty" (a reason
        is carried; not an empty PASS).
  D-V5. INCOMPLETE over an EXISTING snapshot (distinct from the no-snapshot D-V4). REACHABLE triggers in the
        current wiring: an EMPTY index (files_total==0 -> INDEX_NOT_EMPTY Incomplete, evaluate.rs:46-51) or a
        gate returning GateOutcomeForCheck::Incomplete (storage error / missing evidence, mod.rs:175/191/202 ->
        GATE_STATUS Incomplete, evaluate.rs:162-167). Assert: verdict Incomplete; freshness STILL reflects the
        snapshot (Fresh/Stale, NOT Unavailable — the snapshot exists); completeness = Degraded; the
        DegradationReason names the specific incomplete input, not a blanket "incomplete". (DEFENSIVE-ARM NOTE,
        OBSERVED: the call_graph/enrichment/gate `None` arms at evaluate.rs:100/:132/:176 are NOT reached by the
        current gather — mod.rs:97-99 always supplies `Some(...)` when a snapshot exists — so a None-input
        INCOMPLETE is covered here only as a forward-looking guard, e.g. a future trust-core returning an absent
        axis under TRUST-LIVEGRAPH-1.)
  D-V6. WAIVED gate (effective Pass over a computed Fail) -> GATE_STATUS leaf provenance.source = declaration;
        assert the computed gate verdict remains queryable via `rmap gate` (overlay-preserves-computed, D5).
  D-V7. Transport: socket-absent -> connection error, NO envelope (OBSERVED this turn; assert UNCHANGED).

ENVELOPE CORRECTNESS:
  E1. MEET monotonicity: root confidence <= derive_repo_confidence on identical input; the verdict leaf's
      internal MEET (over conditions) and the root MEET (over leaves) can only LOWER class/freshness — no fold
      yields an Exact root from a non-Exact/Stale leaf.
  E2. Invariants I1-I6 hold at every leaf and survive the fold (Exact requires Fresh+Complete; Partial
      justified; Unavailable carries a reason; Stale != Fresh; null != empty; no SCIP-dependent leaf so I6 is
      vacuous for check).
  E3. provenance.source is correct per leaf and the root provenance.source is the exact SET union; assert
      fallback_reason is ALWAYS null and missing_partitions ALWAYS empty (check makes no LiveGraph read).
  E4. Authority overlay preserves computed fact (D5): see D-V6.
  E5. trust_briefing is ALWAYS absent from check's wire shape (None; skip_serializing_if) — the sibling-
      non-crossing guard (orient W3). Pin this so orient's added field never leaks into check's output.

WIRE SHAPE / RENDERER / FIXTURES:
  W1. WIRE SHAPE: the top-level JSON is `CoherenceEnvelope<CoherentOrientResult>`; `value` carries the
      CHECK_COMMAND envelope with the re-typed signals slot; `value.trust_briefing` is ABSENT; `root.trust`
      (TrustPosture) + `root.freshness` (FreshnessState) are PRESENT. The reused signal VALUE payloads stay
      byte-identical (P1); byte-identity of the whole command output is NOT a goal (the wrapper adds labels by
      design, contract RISK-F).
  W2. RENDERER: presentation/check.rs renders the verdict + conditions from `value.signals[*].value`
      (today it reads `signals[].evidence`, presentation/check.rs:121-163). Add the freshness label to the
      human verdict line (PASS@Fresh / PASS@Stale / INCOMPLETE@Unavailable) so the human surface shows the new
      axis; assert NO double-trust rendering (check has no trust_briefing) and the verdict/condition text is
      otherwise unchanged. confidence stays JSON-only (today's renderer does not print it, presentation/
      check.rs:79-98) unless the implementation chooses to surface it — a render-only choice, not a contract
      change.
  W3. FIXTURES: update JSON-contract fixtures in lockstep with COHERENCE-ENVELOPE-1 — a Fresh-PASS fixture, a
      Stale-FAIL fixture, an Unavailable-INCOMPLETE (no-snapshot) fixture, and a check fixture asserting NO
      `trust_briefing` key. Bump the schema id only if the contract tests pin the top-level shape (shared with
      orient — one bump for the wrapper, RISK-F).

CLI-WRAPPER (run_check_cmd; orient.rs:222-331 — the PROCESS contract: stdout/stderr + exit code, §1f/§3e).
  These guard the exit-code remap so the wrapper does NOT silently break check's CI status. Driveable off-target
  with a recorded daemon-response fixture (the wrapped envelope) for CW1/CW2/CW5/CW6; CW3/CW4 need no daemon.
  CW1. `rmap check --json` over a PASS fixture -> stdout is the FULL wrapped `CoherenceEnvelope<CoherentOrientResult>`
       (value + provenance + trust + freshness present; value.trust_briefing ABSENT) AND exit code = 0,
       extracted from result["value"]["signals"][*]["value"]["code"] (§3e), NOT the dead top-level path.
  CW2. `rmap check` (human) over the SAME PASS fixture -> render is byte-identical to today + the freshness
       suffix (§5 W2) AND exit code = 0. Assert the human path reads the WRAPPED signal values
       (CheckResponse projects `value`, §3e) and the exit code is computed BEFORE the render branch (orient.rs:276).
  CW3. unknown flag (`rmap check --bogus`) -> stderr `error: unknown flag: --bogus` + `usage: rmap check [--json]`,
       exit 1, NO daemon contacted; unexpected positional (`rmap check x`) -> `error: unexpected argument: x` +
       usage, exit 1 (OBSERVED orient.rs:231-240). Unchanged by this slice.
  CW4. socket-absent -> stderr daemon-connection error (DaemonClientError::ConnectionFailed), NO envelope on
       stdout, exit 2 (= D-V7; EXECUTED this turn). Unchanged by this slice.
  CW5. EXIT-CODE PARITY MATRIX after the wrapper (the anti-silent-break guard): PASS fixture -> 0; FAIL fixture
       -> 1; INCOMPLETE (incl. no-snapshot) fixture -> 2; a malformed/missing-verdict-signal fixture -> 2 (the
       `.unwrap_or(2)` fallback, orient.rs:290). A REGRESSION fixture pins that the OLD path
       result["signals"][*]["code"] (now null under the wrapper) is NO LONGER used — proving a PASS does not
       silently degrade to exit 2.
  CW6. `RepoNotFound` daemon error -> stderr `error: repo not indexed` + `hint: run 'rmap index .' to index this
       repo`, exit 2 (OBSERVED orient.rs:318-321); a generic DaemonError -> `error: {code}: {message}`, exit 2.
       Unchanged by this slice.

LIVE (after off-target green; macOS, ./scripts/dev-install-local.sh):
  L1. `rmap check` on a Fresh TS pilot snapshot -> PASS@Fresh (or the true verdict), root.freshness=Fresh,
      no trust_briefing, human render shows the verdict line + conditions as today + the freshness label.
  L2. Mutate a tracked file (induce stale files) without re-index -> `rmap check` -> STALE_FILES Fails ->
      FAIL@Stale; assert the freshness label and the STALE_FILES condition agree.
  L3. `rmap check` on a never-indexed repo -> INCOMPLETE@Unavailable, exactly the SNAPSHOT_EXISTS condition.
  L4. Declare a failing requirement, no stale files -> `rmap check` -> FAIL@Fresh (gate-only fail over a fresh
      snapshot, D-V3); add a waiver -> PASS, GATE_STATUS source=declaration, computed fail still in `rmap gate`.
  L5. EXIT-CODE end-to-end (the live seal for CW5): `rmap check; echo $?` on a Fresh PASS pilot -> 0; over a
      gate-failing repo (L4) -> 1; on a never-indexed repo -> 2 (INCOMPLETE@Unavailable). Run in BOTH default
      and `--json` mode — same exit code. This confirms the §3e remap end-to-end through the real daemon, not
      just the fixtures.
```

---

## 6. Scope boundary

```text
IN SCOPE: `rmap check` ONLY (always repo focus). Wrap check's answer in
`CoherenceEnvelope<CoherentOrientResult>`; reuse orient's ratified container UNCHANGED with
`trust_briefing = None`; model the verdict as ONE multi-source composite leaf (D-CHECK-1) + the optional
SNAPSHOT_INFO leaf; label each leaf source (sqlite/declaration) + freshness (Fresh/Stale/Unavailable);
fold the root by MEET; derive confidence from the MEET (D-CHECK-3); add the freshness axis to the human
verdict line (§5 W2). ALSO IN SCOPE (a PARITY obligation forced by the wrapper, §1f/§3e): the run_check_cmd
CLI exit-code extraction and human-render deserialization MUST be remapped to the `value`-nested shape so
check's CI exit codes (PASS=0/FAIL=1/INCOMPLETE=2/fallback=2) are PRESERVED, not silently broken. NO LiveGraph
read, NO cert, NO fastpath (check has zero LG-first leaves, §1d/§2).

OUT OF SCOPE (separate later slices, per the contract slice sequence):
  - ORIENT-LIVEGRAPH-1 (DONE/decision-complete; this slice DEPENDS on it for the wrapper pattern + container).
  - EXPLAIN-LIVEGRAPH-1, TRUST-LIVEGRAPH-1 — the other two coherence commands.
  - COHERENCE-ENVELOPE-1 — the support module (wrapper type, MEET fold, the BUILD of the RATIFIED multi-source
    `Provenance` set-typed field per contract D8 / RP-1 / D-CHECK-5, the FreshnessInfo reconciliation RP-2).
    This slice DEPENDS on it; not built here.
  - The hybrid trust rebase (TRUST-DISPOSITION) — realized in TRUST-LIVEGRAPH-1, not check. If it later
    rebases the trust core, check's CALL_GRAPH_RELIABILITY/ENRICHMENT freshness updates THROUGH the existing
    port with no check edit (§2 trust-core row).
  - SQLITE-RAW-DECOMMISSION-1 — check still reads SQLite (snapshots/files/trust-core) + `declarations`
    (Authority) to compute its verdict; NO table is decommissioned here. COHERENCE-READINESS-RECOMPUTE-1 must
    record check's retained eager SQLite reads as still load-bearing.

HARD GUARDRAILS (this slice's out-of-scope, mirroring the contract):
  NO source code (spec-first). NO table deletion, NO schema/data migration, NO default flip beyond specifying
  it. NO new producer. NO change to declarations/gate/authority semantics. NO change to check's verdict logic
  (reduce.rs/evaluate.rs are untouched — the coherence layer wraps, it does not re-judge). NO raw nodes/edges
  decommission. NO non-TS concern (check reads no language-partitioned structure). NO edit to docs/ROADMAP.md
  or CURRENT_SLICE.md. NO live daemon run / index / refresh.
```

---

## Forced decisions — every cell filled

### D-CHECK-1 — verdict-leaf granularity = ONE multi-source composite leaf (DECIDED, recorded — within contract)

```text
QUESTION: At what granularity does check apply the per-leaf CoherenceEnvelope to its DERIVED, MULTI-SOURCE
verdict (conditions drawn from sqlite operational + sqlite trust-core + declaration authority)? orient never
faced this — orient's signals are each atomic and single-source; check's verdict folds 1-6 conditions of
mixed source into one signal.

| Option | Per-condition source visible | Value/evidence shape | Wire churn | Contract fit | Verdict |
|---|---|---|---|---|---|
| A — ONE composite verdict leaf; conditions stay nested in evidence (un-widened); leaf provenance = source SET, freshness/trust = internal MEET over conditions | via condition summaries + the leaf source SET (not as separate envelopes) | UNCHANGED (verdict Signal pristine, contract Option-B principle) | minimal (only the wrapper added) | EXACT match — the contract's "MEET-freshness verdict label, not a fastpath" (coherence-layer-1.md:334-335) | **DECIDED** |
| B — one leaf PER condition (re-type the verdict evidence so each condition is a CoherenceEnvelope<ConditionEvidence>) | maximal (each condition its own envelope) | CHANGED (verdict evidence re-typed; conditions promoted from sub-facts to leaves) | larger (new nested-leaf shape beyond orient) | OVER-decomposes — conditions are sub-facts of one signal, not top-level signals; goes BEYOND the contract | rejected |
| C — composite verdict leaf BUT widen CheckConditionEvidence with a per-condition source/freshness tag | per-condition tag inline | CHANGED (widens the evidence DTO) | medium | DIVERGES from Option-B "value payload un-widened"; widens check's evidence the contract kept pristine | rejected |

DECIDED: **Option A.** It is the literal realization of the ratified contract's check row ("the MEET-freshness
verdict LABEL"): ONE verdict, one MEET freshness, conditions pristine. Decide-and-record (CLAUDE.md §Decision
Autonomy: "choices a ratified decision already imply -> decide and record") — A is WITHIN the contract, so it
is not a boundary decision "beyond the ratified contract". B and C go BEYOND the contract and are named here
as rejected so the gap is closed at authoring, not a later correction. NO false-completeness from A: the
per-condition detail the agent needs is ALREADY in the condition summaries (which name the failing/incomplete
input, evaluate.rs), and the machine-readable aggregate is the leaf MEET — degradation cannot be hidden.
NOTE ON THE ENABLING DATA-SHAPE: Option A relies on a source SET on the leaf provenance. At iteration 1 that
shape was not yet ratified at the contract level; the reviewer ESCALATED it (CHECK-PROVENANCE-LEAF-SHAPE,
review-1.json). It is now RATIFIED (operator sign-off 2026-06-09) and the contract is AMENDED (D8) so
`Provenance.source` is a `BTreeSet<Source>` at leaf and root — so Option A's leaf set `{sqlite, declaration}`
is a RATIFIED shape, not an invented or deferred one (D-CHECK-5). The granularity choice recorded HERE (ONE
composite leaf vs per-condition leaves) is the residual check-LOCAL decide-and-record WITHIN that ratified
shape. Cheap to unwind: B/C remain addable later if per-condition provenance leaves are ever wanted, without
re-judging the verdict — the full matrix is provided.
```

### D-CHECK-2 — NO trust overlay / NO `trust_briefing` for check (DECIDED, recorded — resolves the packet's "analogous overlay?" question)

```text
The packet asks whether check has "an analogous overlay" to orient's trust_briefing (D-ORIENT-6). ANSWER,
OBSERVED first-hand: NO. handle_check (dispatch.rs:2716-2725) injects NO trust key; the CheckResponse renderer
has NO trust field (presentation/check.rs:32-45; full human surface §1e). CONTRAST handle_explain (dispatch.rs:2801-2816) + orient,
which DO inject a degraded-state `trust` key. => D-ORIENT-6 is STRUCTURALLY ABSENT for check.

DECIDED (decide-and-record, no new surface): check's `CoherentOrientResult.trust_briefing` is ALWAYS `None`.
The field EXISTS on the shared container (orient added it, contract D7 + D-ORIENT-6), and check leaves it None
— exactly the sibling-non-crossing guard orient already specified (W3, orient-livegraph-1.md:762-764). check
does NOT GAIN a trust briefing: that would be a NEW surface beyond the contract and beyond check's current
behaviour (do not get ahead of scope; do not add functionality). check's degradation is ALREADY surfaced via
its conditions (CALL_GRAPH_RELIABILITY/ENRICHMENT/STALE_FILES) + the verdict + the new MEET freshness label —
a separate human briefing is unnecessary and is not added. check's machine certainty lives in `root.trust`
(always present, the MEET).
```

### D-CHECK-3 — confidence becomes one contributor to the root MEET (DECIDED, recorded)

```text
check computes `confidence` via the SAME derive_repo_confidence orient uses (mod.rs:91; confidence.rs:43-70),
and a static Confidence::Low on no-snapshot (mod.rs:75). DECIDED (mirrors orient D-ORIENT-4, implied by
contract D3 MEET): the coherent root confidence is DERIVED from the root MEET and NEVER exceeds the weakest
contributor; the legacy derive_repo_confidence result is ONE input to that MEET, not the sole source. The
no-snapshot static Low is preserved (it is already confidence-lattice bottom). Not a boundary decision — a
local mechanism implied by the ratified MEET invariant. No zero-leaf hazard for check (§3b: check always emits
>= the verdict leaf).
```

### D-CHECK-4 — ZERO LG-first leaves; NO cert / NO fastpath (DECIDED, recorded — within contract)

```text
DECIDED (the contract's check row, made explicit): check serves entirely from SQLite + Authority; it has NO
LG-first leaf, builds NO cert, and runs NO cert-gated fastpath. This is NOT a posture choice this slice is
free to make — it is the ratified contract ("gains no NEW LiveGraph source of its own ... not a fastpath",
coherence-layer-1.md:334-335) confirmed against first-hand source (check touches none of the migrated
SQLite-free surfaces, §1d/§2). Inventing an LG-first posture for check (e.g. serving INDEX_NOT_EMPTY or
SNAPSHOT counts from the LiveGraph) would RE-OPEN the ratified contract and is explicitly forbidden here.
Consequence: check's `provenance.fallback_reason` is ALWAYS null and `missing_partitions` ALWAYS empty
(no LiveGraph read can fail or be partial) — pinned in validation E3.
```

### D-CHECK-5 — multi-source LEAF provenance = RATIFIED (CHECK-PROVENANCE-LEAF-SHAPE; contract amended, D8)

```text
STATUS: RATIFIED (operator sign-off 2026-06-09). This REPLACES the iteration-1 draft's self-"DECIDED"
leaf-shape text, which the reviewer correctly ESCALATED (review-1.json) as a boundary data-shape decision the
contract had not settled for LEAVES — the contract declared `Provenance.source: Source` (single, ~:397) while
requiring the ROOT to carry "the SET of contributing sources" (~:433), an unresolved tension.

THE DECISION (CHECK-PROVENANCE-LEAF-SHAPE = multi-source LEAF provenance): a `CoherenceEnvelope` LEAF may carry
provenance from MULTIPLE contributing sources. The COHERENCE-LAYER-1 contract is AMENDED (its D8) so
`Provenance.source` is a `BTreeSet<Source>` at BOTH leaf and root, resolving the leaf-single/root-set
inconsistency. check's verdict leaf — the FIRST LEAF (not just the root) to hold a source SET — therefore
carries the honest multi-source set `{sqlite, declaration}` (snapshot-present: sqlite-operational +
sqlite-trust-core + declaration-authority/gate) / `{sqlite}` (no-snapshot). [OBSERVED, first-hand:
check/mod.rs:81/84/87 three inputs; gather_gate_outcome reads `declarations` at mod.rs:173 before the
NotConfigured early-return :178-180 → `declaration` contributes on every snapshot-present verdict.]

The full option matrix (A = source set; B = per-condition leaves; C = single coarse source) is preserved in
the contract's D8 for the decision audit trail; Option A was RATIFIED. This introduces NO new architectural
boundary beyond the ratified amendment: the `Signal` evidence stays un-widened (the set rides in the wrapper
SIBLING `provenance`, Option B / contract D1); the `Source` axis is unchanged {livegraph, sqlite, filesystem,
declaration} (the intra-sqlite operational-vs-trust-core distinction rides in `basis`, not a new variant).

WHAT REMAINS FOR COHERENCE-ENVELOPE-1: purely BUILDING the ratified set-typed field (support module → feature,
architecture.md §Build Order). COHERENCE-ENVELOPE-1 has NO latitude to make `Provenance.source` single-valued;
the shape is ratified, not deferred. check's implementation is BLOCKED on that BUILD, not on any open decision.
```

### D-CHECK-6 — scope (DECIDED, recorded)

```text
This slice SPECIFIES check ONLY. NO command implementation here. NO LiveGraph support added to check (it has
none and gets none). NO change to declarations/gate/measurements/trust producers. NO change to check's verdict
logic. NO raw nodes/edges/snapshots/declarations decommission (all retained, now honestly labelled). Mirrors
contract D6.
```

---

## Risks (check-specific projections of the contract risks; each the implementation must address)

```text
RISK-C-A — SNAPSHOT-SCOPED VERDICT MISREAD AS CURRENT-STATE (the check analogue of contract RISK-C/F5).
  check's verdict describes the SQLite SNAPSHOT's readiness, NOT the LiveGraph current state. In a world where
  sibling commands serve current-state LiveGraph facts, a consumer could misread check's PASS as "the
  current-state graph is ready". MITIGATION: the MEET-freshness label (§3c) makes the snapshot scope explicit
  (PASS@Stale when the snapshot lags the worktree); provenance.source = sqlite (never livegraph) states the
  origin. The verdict is NEVER labelled with a LiveGraph epoch (check reads none).
RISK-C-B — EPOCH-SKEW FALSE FRESHNESS (= contract RISK-A, attenuated). check reads ONE snapshot, so there is
  no LiveGraph-vs-SQLite epoch skew WITHIN check (no LiveGraph read). The only freshness skew is
  snapshot-vs-worktree, already detected by get_stale_files and mapped to Stale (§3c). MITIGATION: the MEET
  freshness = snapshot freshness; monotone, cannot read Fresh over stale files.
RISK-C-C — AUTHORITY/STRUCTURE SEAM ERASING COMPUTED FACT (= contract RISK-B/D5). A careless impl could let
  the gate WAIVED overlay hide the computed gate verdict. MITIGATION: GATE_STATUS leaf labelled
  source=declaration; the computed verdict stays queryable via `rmap gate` (D-V6); check consumes only the
  effective projection, it does not erase the computed one.
RISK-C-D — ENVELOPE SHAPE CHURN (= contract RISK-F, shared with orient). The wrapper changes check's JSON wire
  shape (top level becomes CoherenceEnvelope; value = CoherentOrientResult). MITIGATION: land the wrapper ONCE
  in COHERENCE-ENVELOPE-1; keep the reused signal VALUE payloads byte-identical (P1); update the check renderer
  + fixtures in lockstep; one shared schema bump with orient (not a per-command bump).
RISK-C-E — MULTI-SOURCE LEAF REPRESENTATION (= §3d RP-1 / D-CHECK-5) — RESOLVED at the contract level. The
  hazard was that a strictly single-valued `Provenance.source` would prevent check's verdict leaf from
  honestly stating its mixed source. RESOLUTION: the contract is AMENDED (D8, RATIFIED 2026-06-09) so
  `Provenance.source` is a `BTreeSet<Source>` at leaf and root; COHERENCE-ENVELOPE-1 has NO latitude to make
  it single-valued. The residual is purely an IMPLEMENTATION obligation — BUILD the ratified set-typed field
  before check's feature build (support module -> feature) — not an open design risk.
RISK-C-F — TRUST-CORE DRIFT UNDER TRUST-LIVEGRAPH-1 (recorded, cross-slice). check's CALL_GRAPH_RELIABILITY /
  ENRICHMENT read the v1 SQLite trust core via get_trust_summary; TRUST-LIVEGRAPH-1 may rebase that core to a
  hybrid. MITIGATION: check depends only on the AgentTrustSummary PORT (storage_port.rs:317), not on trust
  internals; a hybrid rebase flows through the port. check's freshness labelling reflects whatever the port
  returns. Recorded so the two slices stay decoupled; no check edit anticipated.
```

---

## References
```text
GOVERNANCE / MODEL:
- docs/VISION.md §Fact Certainty Model / §Product Layer Model / §Agent Priorities (#2 preserve computed
  truth) / §"The discovery-first agent loop" (orient -> check handoff role of `check`).
- agent_docs/architecture.md §Product Layer Stack (Layer 0-4) / Rule 6 "null=unknown, empty=known-zero".
- CLAUDE.md §Fact Certainty Model / §Decision Autonomy / §Evidence Law.

AUTHORITATIVE CONTRACT + PRECEDENT:
- docs/slices/coherence-layer-1.md — RATIFIED + AMENDED (2026-06-09, D8 multi-source LEAF provenance). Cited
  by SECTION / DECISION ID, which are stable (the D8 amendment shifted line numbers): §"Per-command source
  map" (check row); Q3 (check freshness — "a PASS over a Stale trust-summary is a Stale PASS"); §"Safe-fallback
  contract" (CHECK row); §"The shared coherence answer-envelope" (envelope spec incl. the AMENDED `Provenance`
  struct — `source: BTreeSet<Source>`); D7 (CoherentOrientResult); **D8 (multi-source LEAF provenance —
  `Provenance.source` = `BTreeSet<Source>` at leaf and root; the RATIFIED basis for check's verdict leaf)**;
  §"Proposed follow-up slice sequence" (CHECK depends on ORIENT); RISK-F (envelope churn); RISK-G (Signal
  freshness reconciliation).
  CAVEAT (evidence-law honesty): the inline `coherence-layer-1.md:NNN` citations elsewhere in THIS doc predate
  the D8 amendment and are now approximate (off by the amendment's line delta, ~+5 to ~+62 depending on
  position). Re-verify against SECTION NAMES and DECISION IDs (D1–D8), which did not move.
- docs/slices/orient-livegraph-1.md — the first per-command application (SHAPE precedent). CoherentOrientResult
  reuse; D-ORIENT-4 confidence MEET; D-ORIENT-6 trust_briefing (ABSENT for check); W3 sibling-non-crossing
  :762-764; transport degradation §4.

CHECK IMPLEMENTATION TODAY (all SQLite/Authority; LiveGraph=NONE) [OBSERVED, first-hand this turn]:
- rust/crates/agent/src/check/mod.rs — run_check:45; gather (get_repo:53 / get_latest_snapshot:60 /
  get_stale_files:81 / get_trust_summary:84 / gather_gate_outcome:87 / derive_repo_confidence:91); no-snapshot
  branch:62-76; OrientResult build:128-152; build_verdict_signal:208-252; condition_to_evidence:255-265;
  gather_gate_outcome:166-205.
- rust/crates/agent/src/check/evaluate.rs — evaluate_conditions:19; SNAPSHOT_EXISTS early-return:29-37; the 6
  conditions:22-185; MEDIUM->pass policy:74-77; NotConfigured->pass policy:143-145.
- rust/crates/agent/src/check/reduce.rs — reduce_verdict (Incomplete>Fail>Pass):18-41; check():44-51; oracle
  tests (6 conditions:305-326; no-snapshot 1 condition:288-301).
- rust/crates/agent/src/check/types.rs — CheckVerdict:18; ConditionCode (6):38-59; CheckInput:76-91;
  GateOutcomeForCheck:97-107; CheckResult:112-116.
- rust/crates/daemon-runtime/src/dispatch.rs — handle_check:2672-2732 (NO trust overlay; display_name:2718;
  run_check:2701); handle_explain:2734-2819 (trust injection :2801-2816 — the contrast).
- rust/crates/agent/src/dto/signal.rs — codes CheckPass/Fail/Incomplete:258-260 + SnapshotInfo:282; strings
  :300-302/:318; severities CheckPass(Check,Low):395 / CheckFail(Check,High):396 / CheckIncomplete(Check,
  Medium):397 / SnapshotInfo(Informational,Low):413; evidence structs :578/:615/:622/:630; builders
  check_pass:1073 / check_fail:1088 / check_incomplete:1103 / snapshot_info:1325.
- rust/crates/agent/src/confidence.rs — derive_repo_confidence:43-70 (Low<0.20; Medium<=0.50 or stale or
  NotRun; else High).
- rust/crates/agent/src/storage_port.rs — AgentReliabilityLevel:231; EnrichmentState:282; AgentTrustSummary:317;
  get_repo:438 / get_latest_snapshot:444 / get_stale_files:451 / get_trust_summary:503.
- rust/crates/rgr/src/presentation/check.rs (full human surface, §1e) — CheckResponse (no trust field;
  snapshot #[allow(dead_code)]; confidence deserialized-not-rendered):32-45; render_human (Repo + Verdict +
  conditions ONLY):79-98; determine_verdict:100-110; render_conditions:121-163; render_hides_internal_fields
  test (snapshot suppressed):270-281.
- rust/crates/rgr/src/commands/orient.rs (the CLI PROCESS wrapper, §1f/§3e) — run_check_cmd:222-331 (arg
  parse:226-242; cwd/canonicalize:245-259; DaemonClient::new:262-268; verdict->exit-code map:278-290;
  --json stdout:294-297; human from_value::<CheckResponse>+render:306-308; RepoNotFound+hint:318-321;
  generic/catch-all daemon error:322-330); run_orient_cmd:130-210 (CONTRAST: ExitCode::SUCCESS:175,:187).
- rust/crates/rgr/src/daemon_client/connection.rs:42-64 — DaemonClientError variants (ConnectionFailed /
  SendFailed / ReadFailed / InvalidResponse / DaemonError{code,message,data} / Timeout) + Display strings.

ANSWER-ENVELOPE VOCABULARY [OBSERVED via contract]:
- rust/crates/repo-graph-trust-model/src/lib.rs — AnswerClass / FreshnessState / DegradationReason /
  QueryCompleteness / ProvenanceBasis + the 6 invariants (cited coherence-layer-1.md:757-762).

EVIDENCE LOG:
- [EXECUTED, this turn] `rmap check` -> "error: daemon connection failed: socket does not exist:
  /Users/apple/Library/Application Support/repo-graph/daemon.sock" (transport degradation path, §4).
- [OBSERVED, first-hand, this turn] check/{mod,evaluate,reduce,types}.rs; dispatch.rs:2672-2819;
  signal.rs (grep: codes/severities/builders); confidence.rs; storage_port.rs:225-340/:438-520;
  presentation/check.rs.
- [OBSERVED, via contract/precedent] coherence-layer-1.md (full); orient-livegraph-1.md (structure +
  D-ORIENT-4/6 + W3 + §4); agent_impl.rs concrete SQL + Tier model (cited, not re-read).
- [INFERRED] the CoherenceEnvelope wiring (§3), the MEET freshness rules (§3c), the degradation mapping (§4),
  the validation plan (§5), the forced-decision verdicts (D-CHECK-1..4,6; D-CHECK-5 is now RATIFIED, below).
- [OBSERVED, first-hand, iteration 2] re-verified the verdict's MULTI-SOURCE nature grounding the amendment:
  check/mod.rs:81 (get_stale_files — SQLite operational), :84 (get_trust_summary — SQLite trust-core), :87
  (gather_gate_outcome — Authority); gather_gate_outcome reads get_active_requirements (the `declarations`
  table) at mod.rs:173 BEFORE the NotConfigured early-return :178-180; evaluate.rs the 6 conditions + the
  no-snapshot early return :36. CONFIRMS the verdict leaf's honest source set = {sqlite, declaration}
  (snapshot-present) / {sqlite} (no-snapshot).
- [RATIFIED, operator sign-off 2026-06-09] CHECK-PROVENANCE-LEAF-SHAPE = multi-source LEAF provenance.
  coherence-layer-1.md AMENDED (D8): `Provenance.source` = `BTreeSet<Source>` at leaf and root, resolving the
  leaf-single (~:397) / root-set (~:433) inconsistency. This doc FINALIZED against the amendment (status block,
  §3a, §3d RP-1, §2, D-CHECK-1, D-CHECK-5, RISK-C-E); the iteration-1 self-"DECIDED" leaf-shape text REPLACED.
  No open DECISION_REQUIRED remains.
- [OBSERVED, first-hand, iteration 3] CLOSING review-2's CLI-PROCESS-WRAPPER completeness gap. Read
  rgr/src/commands/orient.rs:222-331 (run_check_cmd) end-to-end: arg parse (unknown flag / unexpected arg ->
  usage + exit 1, :231-240), cwd/canonicalize (-> exit 2, :245-258), DaemonClient::new failure (-> exit 2,
  :262-267), the verdict->exit-code map (CHECK_PASS->0 / CHECK_FAIL->1 / CHECK_INCOMPLETE->2 / .unwrap_or(2),
  :278-290), --json full-envelope stdout (:294-297) + serialize-error exit 2 (:299-301), human
  from_value::<CheckResponse> + render (:306-308) + parse-error exit 2 (:311-312), DaemonError RepoNotFound +
  hint -> exit 2 (:318-321), generic DaemonError / catch-all Err -> exit 2 (:322-330). CONTRAST run_orient_cmd
  (:130-210): both success arms return ExitCode::SUCCESS (:175,:187) — orient derives NO signal exit code, so
  orient's slice had no exit-code section; check's needs one. Read daemon_client/connection.rs:42-64 for the
  DaemonClientError variants. ADDED §1f (wrapper surface + per-output source posture), §3e (the today->after
  exit-code + human-render remap: result["signals"][*]["code"] -> result["value"]["signals"][*]["value"]["code"],
  value mapping preserved), §5 CLI-WRAPPER (CW1-CW6) + L5, and the §1d/§2/§6 cross-references. NO new boundary
  decision: the remap is mechanically forced by the already-ratified wrapper shape (decide-and-record). No open
  DECISION_REQUIRED remains.
```
