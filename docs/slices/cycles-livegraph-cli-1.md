# CYCLES-LIVEGRAPH-CLI-1: CLI surface for FILE import-cycle detection (Stage D)

Slice ID: CYCLES-LIVEGRAPH-CLI-1
Status: **DESIGN — Surface A + rules ratified (2026-06-02). Implementation NOT started.**
Depends: CYCLES-LIVEGRAPH-1 (`LiveGraph::file_import_cycles()` + `CapturedResolvedRelativeIntraPartition`),
QUERY-MIGRATION-CLI-1 / PATH-LIVEGRAPH-DEFAULT-1 (the `--engine` + `backend_used`/`fallback_reason` pattern).
Track: Stage D. Adds a CLI surface only. NOT a default migration. NOT raw decommission.

## Framing (hard constraints)
```text
- NOT a migration of existing `rmap cycles`: that stays SQLite MODULE-import-cycle authority, default output.
- Adds an EXPLICIT file-import captured-graph cycle surface (a DIFFERENT question).
- No compare against SQLite MODULE cycles (different graphs — invalid equivalence).
- No raw nodes/edges decommission credit beyond "a new LiveGraph cycle surface exists".
- No daemon default flip. No MODULE aggregation. No decommission.
```

## Purpose
```text
Expose the headless `file_import_cycles()` via the CLI as an EXPLICIT, clearly-labelled file-import
cycle query, without changing `rmap cycles`' default SQLite MODULE-import behavior or letting users
confuse the two graphs.
```

## Grounding (EXECUTED 2026-06-02)
```text
- CLI: rgr `run_cycles` (graph.rs:709) sends params {repo} only — no --engine, no --kind. Human render via
  CyclesResponse{cycles:[{nodes:[{name}]}], count} (presentation/cycles.rs) shows member-name chains.
- Daemon: handle_cycles (dispatch.rs:1129) hardcodes find_cycles(snapshot,"module") (SQLite IMPORTS/MODULE
  SCC); returns {repo_uid, display_name, snapshot_uid, cycles, count}.
- Headless: LiveGraph::file_import_cycles() -> AnswerEnvelope<FileImportCyclesAnswer{cycles:[{members:[FILE
  key]}], scope, contributing_epochs}> (CYCLES-LIVEGRAPH-1). Maps to the CyclesResponse shape via
  members -> nodes[{name: file path/key}] + the trust metadata on the envelope.
```

## Ratified decisions (2026-06-02)

### D1 — Surface: A (RATIFIED)
```text
A  `rmap cycles --engine livegraph --kind file-import`  (RATIFIED)
   - explicit; less discoverable; preserves the `cycles` command family.
B  new command `rmap file-import-cycles`                (NOT chosen)
   - clearer standalone semantics, but a larger CLI surface + a second cycle command to maintain.
Chosen A: keep one `cycles` family; the file-import graph is reached only via EXPLICIT flags, never a
default. B remains a possible future ergonomic alias, not this slice.
```

### D2 — `--kind` is MANDATORY with `--engine livegraph`
```text
`--engine livegraph` WITHOUT `--kind file-import` MUST reject with a clear error — never silently compute a
different graph. (The only supported kind this slice is `file-import`; the error names it.) This prevents a
user from reading file-import cycles as MODULE cycles.
```

### D3 — default unchanged
```text
`rmap cycles` with no flags -> the current SQLite MODULE-import-cycle output, byte-for-byte. No auto, no
livegraph default for cycles.
```

### D4 — human output labels scope
```text
LiveGraph file-import cycle human output MUST label the scope, e.g.:
  "captured resolved-relative intra-partition FILE import cycles"
so it is never mistaken for MODULE-import cycles. (SQLite output is unchanged.)
```

### D5 — JSON metadata
```text
The `--engine livegraph --kind file-import --json` output MUST include:
  scope (CapturedResolvedRelativeIntraPartition), backend_used ("livegraph"), answer_class
  (Exact/Partial/Stale/Unavailable), freshness, and fallback/degradation metadata
  (missing_partitions / degradation_reasons). The human render strips these (format unchanged otherwise).
```

### D6 — compare
```text
`--engine compare` is INVALID for `--kind file-import` unless comparing the SAME kind. NEVER compare to
SQLite MODULE cycles (different graphs). This slice does NOT add a compare mode for file-import; an attempt
rejects (or is simply unsupported) — it must not silently diff against the module graph.
```

### D7 — Partial/Stale: show the class, do NOT fallback to SQLite
```text
If the LiveGraph answer is Partial/Stale/Unavailable, SHOW that class + scope. Do NOT fall back to SQLite
MODULE cycles as if same-semantics (unlike callers/callees/path `auto`, which fall back within the SAME
question). For cycles the two engines answer DIFFERENT questions, so a cross-engine fallback would be a
false-equivalence. `--engine livegraph` is explicit and stays on LiveGraph (degraded if need be).
```

## Out of scope (hard guardrails)
```text
No `rmap cycles` default change. No `auto` for cycles. No cross-engine fallback (D7). No compare to SQLite
MODULE cycles (D6). No MODULE aggregation. No raw decommission credit. No new daemon default.
```

## Acceptance (EXECUTED later)
```text
1. `rmap cycles` (no flags) -> unchanged SQLite MODULE-import output (human + --json).
2. `rmap cycles --engine livegraph --kind file-import` -> LiveGraph file-import cycles; human output labels
   the captured scope; finds a known import cycle on a fixture with one.
3. `--engine livegraph` WITHOUT `--kind file-import` -> clear REJECT error (names file-import), no compute.
4. `--json` includes scope, backend_used=livegraph, answer_class, freshness, degradation/missing metadata.
5. a Partial/Stale LiveGraph answer shows the class + scope; NO SQLite fallback.
6. `--engine compare` with `--kind file-import` does NOT diff against SQLite MODULE cycles (rejected/unsupported).
7. `rmap cycles --kind file-import` without `--engine livegraph` -> documented behavior (reject or require
   engine; decide at build, record).
```

## Open sign-off items (decide at build, record)
```text
- Daemon wiring: a `cycles` engine/kind branch (mirror path/callers/callees `*_engine_response`), mapping
  FileImportCyclesAnswer -> CyclesResponse-shaped JSON + the envelope metadata.
- Exact error text + exit codes for D2/D6/D7 rejects.
- Whether `--kind` defaults are needed (only `file-import` exists; `module` is implicit-SQLite).
```

## Follow-up slices
```text
- IMPORTS-EXTRACT-COMPLETENESS-1 / IMPORTS-XPART-RESOLUTION-1: widen the captured scope toward complete TS
  import-cycle semantics (then the scope label can broaden).
- MODULE-AGGREGATION-1 -> a MODULE-import LiveGraph cycle path (the actual `rmap cycles` parity question).
```

## References
- `docs/slices/cycles-livegraph-1.md` (the headless API + CapturedImportGraphScope semantics)
- `docs/slices/path-cycles-livegraph-2.md` (why `rmap cycles` is a DIFFERENT graph; no compare)
- `docs/slices/query-migration-cli-1.md`, `docs/slices/path-livegraph-default-1.md` (--engine + JSON metadata pattern)
- `rust/crates/rgr/src/commands/graph.rs:709` (`run_cycles`), `rust/crates/rgr/src/presentation/cycles.rs` (CyclesResponse)
- `rust/crates/daemon-runtime/src/dispatch.rs:1129` (`handle_cycles`), `livegraph_feed.rs` (engine-response pattern)
