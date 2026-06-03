# MODULE-CYCLES-CLI-1: explicit CLI surface for LiveGraph MODULE import cycles + compare

Slice ID: MODULE-CYCLES-CLI-1
Status: **RATIFIED (2026-06-03). Implementation in progress.** Ratified: D1 CyclesRoute enum; D2 LiveGraph
module response (member name = module PATH; scope `aggregation_basis:"dirname"`); D3 dedicated MODULE-import
human renderer; **D4=A** (structural compare + sidecar: missing -> UnknownDivergence, extra ->
UnexpectedExtraInLiveGraph; NO auto package/dynamic attribution this slice -> MODULE-CYCLES-COMPARE-CLASSIFY-1);
D5 node_uid->qualified_name lookup (compare-only); D6 `--engine sqlite --kind module-import` == SQLite
default. Default `rmap cycles` UNCHANGED; no flip/decommission/deletion. Expose the headless
`LiveGraph::module_import_cycles()` (MODULE-AGGREGATION-1) as an EXPLICIT CLI surface + a compare-vs-SQLite
mode that runs the real-repo divergence classification. NO default migration, NO raw decommission, NO
deletion.
Depends: MODULE-AGGREGATION-1 (`module_import_cycles` + `module_cycle_compare`), CYCLES-LIVEGRAPH-CLI-1 (the
cycles engine/kind surface), CYCLES-FILE-IMPORT-RENDER-1 (the FILE renderer to mirror), the existing
`--engine compare` convention (callers/callees/path). Track: Stage D. CLI + daemon wiring.

## Goal
```text
Make LiveGraph MODULE cycles invokable + comparable to SQLite `rmap cycles` from the CLI, so the
MODULE-AGGREGATION-1 equivalence can be RUN on real repos (not just the fixture) and divergences CLASSED.
The `rmap cycles` DEFAULT stays SQLite, untouched. This slice produces the comparison EVIDENCE; it does not
act on it (no default flip).
```

## Grounding (EXECUTED 2026-06-03)
```text
CLI run_cycles (graph.rs:772) is a (engine,kind) match -> a `livegraph: bool`. Today: ("sqlite","")=SQLite
  default; ("livegraph","file-import")=LiveGraph file cycles; ("compare",_) REJECTED; non-"file-import"
  kinds REJECTED. Params sent: livegraph -> {engine:"livegraph",kind:"file-import"}; else {} (SQLite).
Daemon handle_cycles (dispatch.rs:1206) mirrors the match: (livegraph,file-import) ->
  file_import_cycles_response; (compare,_) REJECTED; fall-through -> storage.find_cycles(snapshot,"module").
SQLite MODULE cycles: storage.find_cycles(snapshot,"module") -> Tarjan over MODULE->MODULE IMPORTS edges;
  CycleResult.nodes carry {node_id (uuid), name (SHORT, e.g. "src"), file:null}. The QUALIFIED module path
  (packages/a/src) is NOT in the cycles JSON -> a compare by module-path needs a node_uid -> qualified_name
  lookup (the nodes table). The DEFAULT cycles JSON must stay unchanged (short `name`).
Compare convention (callers/callees/path, livegraph_feed.rs): PRIMARY = the SQLite answer; + a
  `*_compare` struct inline; + a SIDECAR written to `<repo_root>/.rgr/livegraph-compare/{ts}.json`
  (best-effort; never fails the query). file_import_cycles_response is the JSON template to mirror
  (cycles/count/backend_used/kind/scope/answer_class/freshness/missing_partitions/degradation_reasons).
No `module-import` CLI kind exists yet (only "file-import" + ""=SQLite-module).
```

## Required surface (from the brief)
```text
rmap cycles                                          -> SQLite MODULE default (UNCHANGED).
rmap cycles --engine livegraph --kind module-import  -> LiveGraph module_import_cycles().
rmap cycles --engine compare   --kind module-import  -> SQLite primary + classified LiveGraph-vs-SQLite diff.
rmap cycles --engine sqlite    --kind module-import  -> the SQLite MODULE default (treat as equivalent).
rmap cycles --engine livegraph --kind file-import    -> unchanged (LiveGraph file cycles).
REJECT: --engine livegraph WITHOUT --kind; --engine compare --kind file-import; --engine sqlite --kind
  file-import; unknown engine/kind.
```

## Ratified decisions (2026-06-03) — every cell filled

### D1 — CLI route representation
```text
The `livegraph: bool` cannot express 4 live routes. Replace with a closed enum
`CyclesRoute { SqliteModule, LivegraphFile, LivegraphModule, CompareModule }` from the (engine,kind) match;
each maps to the daemon params. Reject arms unchanged + the new rejects.            [RECOMMENDED]
TRADE-OFF: a small mechanical refactor of the match; clearer than nested bools.
```

### D2 — LiveGraph `--kind module-import` response (the new daemon path)
```text
A new `module_import_cycles_response()` mirrors `file_import_cycles_response()`: calls
`LiveGraph::module_import_cycles()`, maps cycles to the {nodes:[{node_id,name,file:null}]} shape (node_id =
the module path key; name = the module path, e.g. "packages/a/src"), and emits
backend_used:"livegraph", kind:"module-import", answer_class/freshness/missing_partitions/
degradation_reasons, and a `scope` object = the ModuleImportCycleScope: { file_scope:{...}, module_aggregated:true,
aggregation_basis:"dirname" }.                                                       [RECOMMENDED]
DECISION sub-point — member `name`: use the MODULE PATH ("packages/a/src"), not the short name ("src"), so
the human + compare are unambiguous (SQLite's short-name collision "src"/"src" is exactly what we avoid).
```

### D3 — human render for `--kind module-import`
```text
A `render_human_module_import` (mirror CYCLES-FILE-IMPORT-RENDER-1's FILE renderer) with MODULE vocabulary:
"N MODULE import cycle(s) found", "Cycle i (N modules):", members = module paths; NO "rmap modules deps"
hint (the SQLite generic renderer is for the SQLite default only). The compare human render = the SQLite
primary (unchanged SQLite render) + one summary line ("LiveGraph module-cycle compare: X matched, Y missing
[classed], Z extra; sidecar=<path>").                                                [RECOMMENDED]
```

### D4 — compare DIVERGENCE-CLASSIFICATION DEPTH (the genuine decision)
```text
The brief wants "no unexplained divergence" on real repos. Classifying WHY a SQLite module cycle is MISSING
from the LiveGraph needs cause data. Two depths:

A. STRUCTURAL + Unknown default: the daemon emits compare_module_cycles() (matched / missing_in_livegraph /
   extra_in_livegraph, by module-path sets) + counts; every missing -> UnknownDivergence; every extra ->
   UnexpectedExtraInLiveGraph. "No unexplained divergence" is a MANUAL review gate (an analyst classes each
   missing using the sidecar). Smaller; the fixture (EXACT, no divergence) fully validates the surface.

B. OBSERVATION-BASED auto cause-resolver: for each missing module cycle {M..}, inspect the LiveGraph IR
   import OBSERVATIONS between those modules' files and class by resolution:
     PackageExternal / DynamicUnsupported -> MissingInLiveGraphDueToPackageOrDynamicImport
     StaticUnresolved (no overlay match)  -> MissingInLiveGraphDueToUnresolvedImport
     a module-path that has NO SQLite-equivalent identity in the LiveGraph -> ModuleIdentityMismatch
     none of the above explains it        -> UnknownDivergence (the signal to STOP)
   Automates "no unexplained divergence" where the captured observations explain it; Unknown becomes the
   true stop signal.

RECOMMENDATION: A this slice (surface + structural compare + sidecar; the fixture EXACTLY validates it), B
as the immediate follow-up (MODULE-CYCLES-COMPARE-CLASSIFY-1). Rationale: the fixture has NO divergence, so
B is unexercised until a real repo is staged; shipping the surface + structural diff unblocks RUNNING the
comparison, and the auto-classifier is a focused, separately-testable add. BUT if you want the real-repo
"no unexplained divergence" gate to be AUTOMATED in one slice, choose B now (larger).
TRADE-OFF: A risks a pile of "Unknown" on a real repo (manual triage); B front-loads the correlation logic
(LiveGraph observation lookup per missing cycle) before we have a real-repo divergence to test it against.
```

### D5 — SQLite qualified module paths for compare (impl detail)
```text
find_cycles returns SHORT names; compare needs QUALIFIED module paths. Add a node_uid -> qualified_name
lookup (MODULE nodes for the snapshot) used ONLY by the compare path; the DEFAULT SQLite cycles output is
UNCHANGED (still short `name`). No new SQLite write, no schema change.                [RECOMMENDED]
```

### D6 — `--engine sqlite --kind module-import` (brief-dictated)
```text
Treat as the current SQLite MODULE default (NOT reject): module-import IS the SQLite default graph, so
`--engine sqlite --kind module-import` is an explicit spelling of it. Same output as `rmap cycles`. [RATIFIED by brief]
```

## Validation (EXECUTED later)
```text
1. fixture: `--engine livegraph --kind module-import` -> 1 MODULE cycle {packages/a/src, packages/b/src},
   MODULE vocabulary; `--engine compare --kind module-import` -> compare report EMPTY (exact); sidecar
   written. Extend scripts/compare-module-cycles.sh to drive the CLI directly (replacing the dirname-
   aggregation shim now that a real surface exists).
2. real repo with module cycles: `--engine compare --kind module-import` -> divergences CLASSED; NO
   unexplained divergence (D4: A=manual review of the sidecar / B=auto). If an UNEXPLAINED (Unknown) or an
   EXTRA divergence appears -> STOP, do not claim readiness.
3. `rmap cycles` (default) + `--engine sqlite` outputs BYTE-UNCHANGED. `--kind file-import` unchanged.
4. reject matrix holds (livegraph-no-kind; compare+file-import; sqlite+file-import; unknown).
5. full gate (workspace test, clippy -D warnings, fmt).
```

## Out of scope (hard guardrails)
```text
NO `rmap cycles` default flip (the default stays SQLite until the compare evidence is reviewed + a separate
migration slice is ratified). NO raw nodes/edges decommission. NO deletion. NO package/path-alias/dynamic
expansion (the LiveGraph module graph stays a subset; that is what the compare MEASURES). NO module
measurements. The compare is DIAGNOSTIC; it changes no default answer.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. CLI (rgr/graph.rs): CyclesRoute enum (D1); the 4 live routes + the reject matrix; params per route;
   render_human_module_import (D3) + the compare summary line.
2. daemon (dispatch.rs + livegraph_feed.rs): handle_cycles routes (livegraph,module-import) ->
   module_import_cycles_response (D2); (compare,module-import) -> SQLite find_cycles("module") +
   module_import_cycles() + compare_module_cycles + classify (D4) + sidecar (the existing convention); the
   node_uid->qualified_name lookup (D5). (sqlite,module-import) -> the SQLite default.
3. validation: drive scripts/compare-module-cycles.sh through the new CLI (fixture exact, compare empty);
   document the real-repo procedure + the stop-on-unexplained gate.
4. docs: completion + the fixture compare evidence + (if a real repo is staged) the classed divergences.
```

## Follow-up slices
```text
- MODULE-CYCLES-COMPARE-CLASSIFY-1 : (if D4=A) the observation-based auto cause-resolver.
- IMPORTS-PACKAGE-RESOLUTION-1 : close the subset gap (package/path-alias) — prerequisite for FULL parity.
- CYCLES-DEFAULT-MIGRATION-1 : (much later) flip `rmap cycles` default — only after the compare evidence
  proves parity / acceptable bounded divergence.
```

## References
- `rust/crates/rgr/src/commands/graph.rs` (`run_cycles` engine/kind match; the FILE renderer to mirror)
- `rust/crates/daemon-runtime/src/dispatch.rs` (`handle_cycles` routing)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`file_import_cycles_response`; the compare convention + `write_compare_sidecar`)
- `rust/crates/repo-graph-livegraph/src/module_cycle_compare.rs` (`compare_module_cycles` + the divergence vocabulary)
- `rust/crates/storage/src/queries.rs` (`find_cycles` "module"; the node qualified_name source for D5)
- `docs/slices/module-aggregation-1.md` (the headless API + the equivalence model this exposes)
