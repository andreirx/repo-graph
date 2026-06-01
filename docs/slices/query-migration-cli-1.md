# QUERY-MIGRATION-CLI-1: callers/callees Default → LiveGraph-with-SQLite-Fallback (Stage D)

Slice ID: QUERY-MIGRATION-CLI-1
Status: **IMPLEMENTED + live-validated (2026-06-01).** Default callers/callees engine flipped to `auto`
(LiveGraph-when-complete, else labelled SQLite fallback). See "Completion".
Depends: SQLITE-RAW-DECOMMISSION-READINESS-1 (the §4 fallback decision), LIVEGRAPH-INTEGRATION-1B
(the `--engine` selector + `callers_engine_response`/`callees_engine_response`), WARM-CACHE-* (warm
LiveGraph population), the trust model (`AnswerClass`/`FreshnessState`).
Track: Stage D. The FIRST real default-path migration — treat as shipped behavior. Does NOT touch
path/cycles/imports/stats, does NOT delete `nodes`/`edges`.

## Purpose

```text
Make callers/callees DEFAULT path use LiveGraph where available, with a labelled SQLite fallback.
```

## Ratified fallback contract (readiness §4 → option C)

```text
Per-command migration with labelled SQLite fallback:
  For callers/callees:
    try LiveGraph first
    if available AND sufficiently complete (D3) -> serve LiveGraph
    if unavailable / incomplete / unsupported  -> fall back to SQLite
    human output compatibility preserved by default
    structured (JSON) metadata records backend_used and fallback_reason
  DO NOT remove the SQLite fallback. DO NOT migrate path/cycles. DO NOT delete nodes/edges.
```

## Ratified decisions (D1–D5, 2026-06-01)

### D1 — default engine = `auto`
```text
current default: sqlite        new default: auto        strict livegraph: still opt-in (--engine livegraph)
auto = LiveGraph when available + complete (D3), otherwise SQLite fallback.
```
Mechanism: add `Engine::Auto`; the no-flag default becomes `Auto` (today it is `Sqlite` — `Engine::parse`
default at `daemon-runtime/src/livegraph_feed.rs:82`; CLI default `"sqlite"` at `rgr graph.rs:151`).

### D2 — fallback visibility
```text
human output: FORMAT unchanged (same rendering shape as today). CONTENT may differ ONLY when the
              backend differs (LiveGraph vs SQLite edges) — and any such difference must be explainable
              by the JSON metadata (backend_used) and the compare sidecar. NOT byte-identical by claim.
JSON (--json): adds backend_used + fallback_reason
compare sidecar (--engine compare): remains the diagnostic parity harness (.rgr/livegraph-compare/<ms>.json)
```

### D3 — completeness threshold (conservative)
```text
auto serves LiveGraph ONLY when AnswerClass == Exact AND FreshnessState == Fresh.
Partial / Stale / RefreshFailed / Unavailable -> SQLite fallback.
```
Note (interaction with PRODUCER-ABSENT-1): a producer-absent warm load is `Stale + ProducerUnavailable`,
so `auto` will FALL BACK to SQLite for it — correct: `auto` serves LiveGraph only when fully fresh/exact.
A producer-absent LiveGraph is for `livegraph_refresh` resilience, NOT for being served as a default
answer over SQLite.

### D4 — language scope
```text
auto uses LiveGraph ONLY for partitions whose language is TypeScriptPrimary in this slice.
Any other language -> SQLite fallback (fallback_reason = language_unsupported).
```

### D5 — operator override (keep all three)
```text
--engine sqlite    -> force the SQLite path (the old default; an escape hatch)
--engine livegraph -> force LiveGraph explicitly (strict; serves even Partial/Stale, the existing 1B behavior)
--engine compare   -> compare sidecar (diagnostic)
(no flag)          -> auto (D1)
```

## The `auto` decision algorithm (precise)

```text
callers(target) / callees(target), engine = Auto:
  resolve the target's defining/contributing partition(s) in the repo LiveGraph
  if the LiveGraph has them AND language == TypeScriptPrimary (D4):
      ans = livegraph callers/callees (trust-labelled)
      if ans.class == Exact AND ans.freshness == Fresh (D3):
          serve LiveGraph;  backend_used = "livegraph";  fallback_reason = null
      else:
          serve SQLite;     backend_used = "sqlite";     fallback_reason = "livegraph_<class|freshness>"
                            (e.g. livegraph_partial, livegraph_stale, livegraph_unavailable)
  else:
      serve SQLite;         backend_used = "sqlite";     fallback_reason = "livegraph_absent" | "language_unsupported"
```

## Structured metadata (JSON only; human output unchanged)

```text
backend_used:    "livegraph" | "sqlite"     (which backend actually produced the served answer)
fallback_reason: null | "livegraph_absent" | "language_unsupported" | "livegraph_partial"
                 | "livegraph_stale" | "livegraph_unavailable"     (null iff backend_used == "livegraph")
```
For `--engine sqlite` -> backend_used="sqlite", fallback_reason=null. For `--engine livegraph` ->
backend_used="livegraph" (or an explicit error/empty if absent, the existing 1B behavior),
fallback_reason=null.

## Acceptance (shipped behavior — EXECUTED)

```text
1. default callers/callees on a preloaded/refreshed synthetic repo serves LiveGraph (or records
   backend_used=livegraph in JSON)
2. if the LiveGraph is absent, the default falls back to SQLite (backend_used=sqlite, fallback_reason set)
3. human (non-JSON) output FORMAT is unchanged (content may differ only when the backend differs,
   explainable by backend_used / compare; no new trust metadata in human output)
4. JSON (--json) exposes backend_used + fallback_reason
5. --engine sqlite forces the old SQLite path (backend_used=sqlite)
6. --engine livegraph remains explicit (strict LiveGraph)
7. --engine compare still writes the compare sidecar
```

## Out of scope (hard guardrails)

```text
No path/cycles/imports/stats migration (PATH-CYCLES-LIVEGRAPH-1 / later). No orient/explain/check
(ORIENT-EXPLAIN-TRUST-1). No nodes/edges deletion. SQLite fallback is PERMANENT here (never removed).
No multi-language LiveGraph (TS only). No change to the LiveGraph engine's own answer computation.
```

## Implementation notes (grounding; confirm during build)

```text
- Engine enum (livegraph_feed.rs) gains Auto; Engine::parse default -> Auto; rgr extract_engine_flag
  default "sqlite" -> "auto"; --engine sqlite|livegraph|compare still parse explicitly.
- handle_callers/handle_callees (dispatch.rs:767/862) route Auto: run the livegraph engine, inspect
  class/freshness (D3), serve or fall back; attach backend_used/fallback_reason to the response.
- The human renderer (rgr graph.rs) ignores backend_used/fallback_reason (output unchanged); the --json
  renderer includes them.
- The livegraph engine already falls back to SQLite when the partition is unavailable
  (livegraph_feed.rs:268) — Auto generalizes that to the D3 threshold + labels it.
```

## Proposed commit structure (confirm)
```text
1. Engine::Auto + the auto decision + backend_used/fallback_reason in the daemon (dispatch + livegraph_feed)
2. CLI default -> auto + JSON metadata surfacing (rgr); human output unchanged
   (combine if the default flip would otherwise leave a non-building / behavior-inconsistent step)
```

## Completion (implemented + live-validated 2026-06-01, EXECUTED)

Daemon (`livegraph_feed.rs`): `Engine::Auto` (new default) + `FallbackReason` enum + `auto_outcome`
(Stale-before-class so a producer-absent partition reports `LiveGraphStale`) + `backend_used`/
`fallback_reason` on every callers/callees response. CLI (`rgr graph.rs`): `--engine` default
`sqlite → auto`; human path strips `backend_used`/`fallback_reason` (format unchanged); `--json` surfaces
them. daemon-runtime 72 tests + clippy `-D warnings` (daemon + rgr) + `fmt --all` clean.

Live (synthetic fixture, daemon v0.2.1; after a refresh populated the LiveGraph Fresh):
```text
callers makeCircle (human)          -> 1 caller, format unchanged
callers makeCircle --json           -> backend_used=livegraph, fallback_reason=null        [#1,#4]
callers makeCircle --engine sqlite  -> backend_used=sqlite, fallback_reason=null (static)   [#5]
callers makeCircle --engine livegraph -> backend_used=livegraph                              [#6]
callers makeCircle --engine compare -> SQLite answer + compare sidecar                       [#7]
callees report (all five variants)  -> symmetric PASS
daemon restart (no refresh -> empty LiveGraph):
  callers makeCircle --json         -> backend_used=sqlite, fallback_reason=LiveGraphUnavailable [#2]
  callees report --json             -> backend_used=sqlite, fallback_reason=LiveGraphUnavailable [#2]
```
All 7 acceptance criteria PASS. The first default-path migration ships: default `auto` serves LiveGraph
when Exact+Fresh+TS-only, else a labelled SQLite fallback; SQLite fallback retained permanently.

## References
- `docs/slices/sqlite-raw-decommission-readiness-1.md` (§4 fallback contract; the command/backend map)
- `docs/slices/livegraph-integration-1b.md` (the `--engine` selector; shipped-flag history)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (Engine, `callers_engine_response`, the existing fallback)
- `rust/crates/rgr/src/commands/graph.rs` (callers/callees CLI; `extract_engine_flag`; JSON rendering)
- `docs/slices/warm-cache-producer-absent-1.md` (the Stale/ProducerUnavailable interaction with D3)
