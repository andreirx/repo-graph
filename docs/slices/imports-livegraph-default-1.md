# IMPORTS-LIVEGRAPH-DEFAULT-1: flip the default `rmap imports <file>` to LiveGraph-first

Slice ID: IMPORTS-LIVEGRAPH-DEFAULT-1
Status: **RATIFIED (D1/D3/D4/D5/D6 as written; D2=B COMPARE-ON-CALL — 2026-06-07). BUILD IN PROGRESS.** The
first imports default migration gets a HARD per-call no-loss guarantee (SQLite still read as the safety net; no
decommission yet -- that is the follow-up IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1). Flip the DEFAULT
single-file `rmap imports <file>` (SQLite today) to LiveGraph-first with a LABELLED SQLite fallback, mirroring
the ratified QUERY-MIGRATION-CLI-1 pattern (callers/callees/path). NO raw decommission, NO SQLite deletion, NO
resolver changes, NO repo-wide default surface. The explicit `--engine sqlite|livegraph|compare` overrides are
UNCHANGED.
Depends: IMPORTS-LIVEGRAPH-CLI-1 (the read-model), -DEFAULT-READINESS-1 (the per-file gate), -REPOWIDE-READINESS-1
(GREEN-SAFE: 0 regression / 0 unknown over 1303 files). Mirrors QUERY-MIGRATION-CLI-1 (the callers/callees
`Engine::Auto` + `FallbackReason` + the human-strip). Track: Stage D, QUERY-MIGRATION-1.

## Why now (priority path)
```text
Per-file + repo-wide readiness are GREEN-SAFE: ZERO regression + ZERO unknown across 1303 files; LiveGraph is a
SUPERSET for TS files; non-TS files fall back by precondition. The default flip is now EVIDENCE-BACKED. The
established QUERY-MIGRATION-CLI-1 pattern (callers/callees/path serve LiveGraph-first with a labelled SQLite
fallback, backend_used/fallback_reason JSON-only stripped in human) is the template -- imports is the last
graph-drilldown surface to migrate.
```

## Grounding (EXECUTED 2026-06-07)

### The QUERY-MIGRATION-CLI-1 precedent (callers/callees/path)
```text
DAEMON (livegraph_feed.rs): `Engine::Auto` (the DEFAULT). `auto_outcome(livegraph_query)`: None (no LiveGraph)
  -> LiveGraphUnavailable ; freshness != Fresh -> LiveGraphStale ; class != Exact -> LiveGraphPartial ; !ts_only
  -> LiveGraphUnsupportedLanguage ; else serve LiveGraph. PRECONDITION-ONLY TRUST -- NO per-call no-loss check.
FallbackReason (7 variants; 4 active): LiveGraphUnavailable / LiveGraphPartial / LiveGraphStale /
  LiveGraphUnsupportedLanguage (+ path's LiveGraphDisplayMetadataUnavailable; 2 reserved).
RESPONSE: callers_value(target, results, backend_used, fallback_reason) -> backend_used ALWAYS present,
  fallback_reason null when livegraph.
HUMAN STRIP (rgr graph.rs): inline `obj.remove("backend_used"); obj.remove("fallback_reason")` BEFORE the human
  render -> the sqlite-compatible human output is byte-unchanged.
```

### The imports-specific facts (from the readiness slices)
```text
The per-file gate exists: `imports_compare_sidecar` (directional no-loss) + `file_partition_status` (the
  precondition) + `live_import_view` (the edges, a proven superset). REPOWIDE GREEN-SAFE: 0 regression / 0
  unknown / 1303 files.
SQLite TS imports = resolved-relative FILE/SOURCE/static ONLY (the homegrown `ts-core` extractor's limit;
  EXECUTED: amodx rows are all kind=FILE/subtype=SOURCE/resolution=static -- NO external / unresolved rows).
  => the LiveGraph EDGE set is a proven superset of the SQLite OUTPUT for TS files (not just the resolved-local
  subset). Serving LiveGraph edges loses NOTHING SQLite showed; it ADDS the alias/dynamic edges SQLite missed.
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Default predicate (the structure)
```text
The DEFAULT serves LiveGraph for a file IFF: PRECONDITION (the file's partition resident + Fresh + TS-primary,
via `file_partition_status`) AND a SAFETY signal (per D2: a per-call no-loss compare passes, OR -- long-term --
the precondition alone, trusting the measured safety). ELSE a LABELLED SQLite fallback. NEVER serve a LiveGraph
answer that fails the precondition or (D2=compare) the per-call no-loss.
RECOMMENDATION: as written -- precondition + (D2 safety) + labelled fallback. Mirrors the callers/callees
`Engine::Auto` STRUCTURE, with imports adding the (optional) per-call no-loss (D2).
```

### D2 — Runtime cost / safety model (THE fork — force)
```text
                       | mechanism                          | safety                         | cost / decommission
A. PRECONDITION-ONLY   | serve LiveGraph iff resident+Fresh | trusts the MEASURED no-loss    | FAST; SKIPS SQLite ->
   (the precedent)     | +TS (mirror callers/callees Auto)  | (0 regression/1303 files); a   | real decommission. RISK:
                       |                                    | NEW file is not re-checked     | a post-measurement file
B. COMPARE-ON-CALL     | every default call runs SQLite +   | MAXIMAL: per-call no-loss       | SQLite read EVERY call (==
   [LEAN for the FIRST | LiveGraph + the directional no-    | guarantee; reuses imports_     | today's default cost) -> NO
   migration]          | loss; serve LiveGraph iff pass     | compare_sidecar                | decommission YET; the WIN is
                       |                                    |                                | the IMPROVED answer (extras)
C. CACHE               | a precomputed per-file readiness   | as good as the cache's          | FAST; needs invalidation on
                       | verdict consulted per call         | freshness                      | re-index/refresh (staleness)
RECOMMENDATION: B for THIS slice. imports is unique: the default ALREADY reads SQLite, so compare-on-call adds
  NO cost over today -- it serves the IMPROVED LiveGraph answer (the alias/dynamic extras) VERIFIED no-loss per
  call, with SQLite as the safety net. No decommission yet (that is a SEPARATE follow-up: A precondition-only or
  C cache, once trusted). This matches the user's "first migration compare-on-call is safest; do not compare
  every call long-term." (A is the callers/callees precedent + evidence-backed, but for imports it changes the
  default to TRUST without the cheap per-call check -- defer A to the decommission slice.)
```

### D3 — Output compatibility (mirror the precedent)
```text
HUMAN: byte-COMPATIBLE with today. The LiveGraph-served listing maps each edge -> the existing ImportEntry
  shape { symbol = dst_file, file = dst_file, kind = "FILE", subtype = "SOURCE", resolution = "static",
  edge_type = "IMPORTS", evidence = [basis], depth = 1 } so `render_human` is unchanged. backend_used /
  fallback_reason are JSON-ONLY and STRIPPED in human (the inline `obj.remove` precedent) -> the human render
  is byte-unchanged whichever backend served.
JSON: adds `backend_used` ("livegraph"|"sqlite") ALWAYS + `fallback_reason` (null on livegraph). Existing
  consumers read {file, imports, count}; the extra fields are additive (forward-compatible) -- the established
  callers/callees convention.
RECOMMENDATION: as written. Mirror QUERY-MIGRATION-CLI-1 exactly.
```

### D4 — Extra edges (the value)
```text
A. SHOW EXTRAS [LEAN]: the LiveGraph-served listing IS the LiveGraph edge set -- including the alias/dynamic
   edges SQLite missed (421 extra over the measured set). The default answer IMPROVES (more complete imports).
   This is the PRODUCT VALUE (agents get the resolved alias/dynamic imports SQLite never had). Proven no-loss:
   LiveGraph edges are a superset of the SQLite TS output (grounding).
B. SUPPRESS to a SQLite-compatible subset (filter LiveGraph edges to SQLite's targets). == SQLite -> the
   migration has NO value (the user's own caveat). REJECT.
C. EXTRAS in JSON/metadata only; human = SQLite-compatible subset. Half-measure; the human user gets no
   improvement. Weak.
RECOMMENDATION: A. The whole point of the flip is the IMPROVED answer; the readiness proved it loses nothing.
  (The benign external / workspace-local / unresolved OBSERVATIONS are NOT in the default edge listing -- they
  are not in the SQLite TS output either, so omitting them loses nothing; they remain in `--engine livegraph`.)
```

### D5 — Fallback labels (extend the precedent's FallbackReason)
```text
Reuse the precedent's variants where they map, + the imports-specific ones:
- precondition unmet: file NOT in a resident TS partition -> `LiveGraphUnavailable` (no resident TS partition
  owns the file); a found-but-STALE partition -> `LiveGraphStale`; a found-but-non-TS partition ->
  `LiveGraphUnsupportedLanguage`.
- (D2=B only) per-call no-loss FAILS -> a NEW `LiveGraphImportRegression` (a SQLite resolved-local import the
  LiveGraph lost -- the dangerous case; never observed in the readiness but the safety net).
- (D2=B only) an UNKNOWN ambiguous SQLite import -> `LiveGraphImportUnknown` (conservative fallback).
RECOMMENDATION: as written. The precondition reasons mirror callers/callees; the regression/unknown reasons are
  imports-specific (only if D2=B). backend_used = sqlite whenever any reason is set.
```

### D6 — Overrides (unchanged)
```text
`--engine sqlite` -> the SQLite single-file listing, UNCHANGED (the escape hatch). `--engine livegraph` -> the
full evidence view (edges + observations + the named module-cycle trust), UNCHANGED. `--engine compare <file>`
+ `--engine compare` (no file, repo-wide) -> UNCHANGED. ONLY the DEFAULT (no --engine) changes: SQLite-only ->
the LiveGraph-first auto. file STILL REQUIRED for the default (single-file; no repo-wide default surface).
RECOMMENDATION: as written.
```

## Acceptance (to verify post-build, EXECUTED)
```text
1. xpart / amodx / repo-graph TS files: the DEFAULT serves LiveGraph (backend_used=livegraph) with NO missing
   SQLite import (D2=B: the per-call no-loss passes) -- and shows the alias/dynamic extras.
2. OpenXcom / non-TS files: the DEFAULT falls back to SQLite (backend_used=sqlite, fallback_reason a precondition
   reason). The existing C++ listing is byte-unchanged.
3. The HUMAN `rmap imports <file>` output is byte-compatible with today (backend_used/fallback_reason stripped).
4. No default call silently loses imports (D2=B: the per-call no-loss; D5: a regression -> labelled SQLite
   fallback, never a silent loss).
5. `--engine sqlite|livegraph|compare` unchanged.
Gate: cargo test --workspace ; clippy --workspace --all-targets -- -D warnings ; cargo fmt --all -- --check.
```

## Out of scope (hard guardrails)
```text
NO raw decommission (SQLite still read -- D2=B; the SKIP-SQLite decommission is a SEPARATE follow-up) ; NO SQLite
deletion ; NO resolver changes ; NO repo-wide DEFAULT surface (the default stays single-file) ; NO change to the
explicit engines (D6) ; NO new import classes. The benign/blocking OBSERVATIONS stay in `--engine livegraph`,
not the default listing.
```

## Build contract (PROPOSED — gated on D1–D6 ratification)
```text
1. daemon: handle_imports DEFAULT (no engine / engine="auto") -> an `imports_auto_response`: compute the
   precondition (file_partition_status) ; (D2=B) run the per-file directional no-loss (reuse imports_compare_
   sidecar's verdict) ; serve the LiveGraph edges mapped to the ImportEntry shape (D3/D4) with backend_used=
   livegraph IFF precondition met AND no-loss ; ELSE find_imports with backend_used=sqlite + fallback_reason.
2. daemon: a FallbackReason for imports (D5) + the edge->ImportEntry mapping (D3). Pure; unit-tested.
3. cli: the DEFAULT render strips backend_used/fallback_reason (the precedent's inline `obj.remove`) -> human
   byte-compatible ; --json keeps them. --engine routing unchanged.
4. live: the acceptance set (xpart/amodx/repo-graph TS -> livegraph+extras ; OpenXcom -> sqlite fallback ; human
   byte-compatible) ; gate ; completion doc.
Stop if: any default call would serve a LiveGraph answer missing a SQLite resolved-local import (a regression)
  -> the fallback MUST fire (D2=B catches it). Stop if the edge->ImportEntry mapping changes the human bytes for
  a fallback (SQLite) answer -> the SQLite path must stay identical.
```

## After this slice
```text
IMPORTS-LIVEGRAPH-DECOMMISSION-1 (separate): once the default is LiveGraph-first + trusted, move D2 B -> A
(precondition-only) or C (cache) to SKIP the SQLite read for served files -- the real decommission. Gated on
this slice being stable in production.
```

## References
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`callers_value` / `auto_outcome` / `FallbackReason` — the QUERY-MIGRATION-CLI-1 precedent; `imports_compare_sidecar` — the per-file no-loss to reuse)
- `rust/crates/rgr/src/commands/graph.rs` (the human-strip of backend_used/fallback_reason — the precedent)
- `rust/crates/rgr/src/presentation/imports.rs` (`ImportsResponse` / `ImportEntry` — the byte-compatible human shape)
- `docs/slices/imports-livegraph-repowide-readiness-1.md` (the GREEN-SAFE evidence backing the flip)
