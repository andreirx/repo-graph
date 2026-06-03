# MODULE-AGGREGATION-1: derive MODULE import cycles from the FILE import graph (equivalence analysis)

Slice ID: MODULE-AGGREGATION-1
Status: **RATIFIED (2026-06-03). Implementation in progress.** Ratified: D1=A (`module(file)=dirname(repo-
relative path)`), D2=skip-self + dedup, D3=A (headless + compare harness; no CLI), D4=inherit
file-import completeness + module-aggregation caveat, D5=fixture EXACT / real-repo subset with classed
divergences / no default migration. Key-parse: REUSE the resolver's proven `file_key_path` (first-colon =
repo boundary; `repo_uid` has no colon), NOT new string-slicing (stop condition satisfied). A SEMANTIC
BRIDGE slice: aggregate the
LiveGraph FILE->FILE import graph (resident `AstImport` edges UNION the cross-partition overlay) up to
MODULE->MODULE cycles, for EQUIVALENCE ANALYSIS against the SQLite `rmap cycles` default — NOT a default
migration, NOT decommission, NOT deletion.
Depends: IMPORTS-XPART-WIRING-1 + IMPORTS-XPART-ENUMERATION-1 (the live FILE import graph + overlay),
CYCLES-LIVEGRAPH-1 (`file_import_cycles` + the SCC reuse), XPART-FIXTURE-STANDALONE-1 (the comparison
fixture), `repo-graph-algorithms` (Tarjan SCC). Baseline audit: SQLITE-RAW-DECOMMISSION-READINESS-3.
Track: Stage D. NO raw decommission. NO default flip. NO deletion. NO package/path-alias/dynamic expansion.

## Goal
```text
Produce a HEADLESS LiveGraph MODULE-import cycle answer derived PURELY from the FILE import graph, and PROVE
(or honestly bound) its equivalence to the SQLite `rmap cycles` MODULE default. This is the bridge the
READINESS audits gate `rmap cycles` migration on — but this slice does ONLY the derivation + the
equivalence harness; it does not migrate the default or retire anything.
```

## Grounding (EXECUTED 2026-06-03) — the SQLite module-cycle model + the fixture side-by-side
```text
SQLite MODULE identity (what `rmap cycles` ACTUALLY uses) is PURE PATH = dirname(file):
  - indexer/orchestrator.rs get_module_path(file) = everything before the last `/` (the immediate parent
    dir). file_to_module[file] = `{repo}:{dirname}:MODULE`. NO config/manifest read on this path.
  - orchestrator materializes MODULE->MODULE IMPORTS edges from file-level imports, SKIPPING same-module
    (`if src_key == tgt_key { continue }`). A MODULE node is created for every ancestor dir of every file.
  - storage/queries.rs find_cycles runs Tarjan (`repo_graph_algorithms::find_sccs`, size > 1) over the
    PRE-MATERIALIZED MODULE->MODULE IMPORTS edges (SELECT DISTINCT). It reads ONLY edges + nodes; it does
    NOT read module_file_ownership / package.json / tsconfig at query time.
  - There IS a separate manifest-driven ownership (compose.rs module_file_ownership, longest-prefix vs
    package.json/Cargo.toml roots), but `rmap cycles` does NOT use it. (Confirm via the compare harness on a
    package.json repo — if a manifest repo's module cycles differ, the cycle path's identity is still
    dirname and the divergence is a DATA-completeness gap, not an identity mismatch.)
Fixture side-by-side (OBSERVED on xpart-monorepo):
  - SQLite `rmap cycles`           -> 1 module-level cycle, 2 modules BOTH named "src"
                                      (= packages/a/src <-> packages/b/src; keys differ).
  - LiveGraph file-import cycles    -> 1 FILE cycle: packages/a/src/main.ts <-> packages/b/src/foo.ts.
  - dirname aggregation of the FILE cycle: module(packages/a/src/main.ts)=packages/a/src;
    module(packages/b/src/foo.ts)=packages/b/src; cross-module (a/src != b/src) -> module cycle
    {packages/a/src, packages/b/src} == the SQLite result. EQUIVALENT on the fixture by PURE derivation.
Conclusion: MODULE cycles are DERIVABLE from the LiveGraph FILE import graph with NO extra metadata, using
dirname identity. The only equivalence GAP on real repos is the FILE-graph completeness gap (the captured
graph is relative + ext/index ONLY; SQLite FILE imports include package/dynamic forms) -> LiveGraph module
cycles will be a SUBSET of SQLite's where package/dynamic imports close a module ring.
```

## Ratified decisions (2026-06-03) — every cell filled

### D1 — MODULE identity (how a FILE maps to a MODULE)
```text
A. file's immediate PARENT DIRECTORY = dirname(repo-relative path); key `{repo}:{dir}:MODULE`;
   name = last component.                                                              [RECOMMENDED]
B. package root (walk up to package.json).
C. tsconfig root.
D. "existing SQLite module key rules" — for the CYCLE path this IS dirname (== A).
RECOMMENDATION: A. Grounding shows `rmap cycles` uses dirname identity (orchestrator get_module_path), and
the fixture confirms it (modules == the files' parent `src` dirs). B/C would DIVERGE from the SQLite cycle
default (they match the manifest-driven module_file_ownership, which find_cycles does NOT use). Matching A
is the only choice consistent with the equivalence goal.
TRADE-OFF: dirname makes EVERY directory a module (fine — same as SQLite). If a manifest repo's `rmap
cycles` ever turns out to use ownership identity, the compare harness (D5) catches it and we revisit; until
then dirname is the evidenced rule.
```

### D2 — Edge aggregation (FILE edges -> MODULE edges)
```text
FILE import A->B becomes MODULE(dirname A) -> MODULE(dirname B); SKIP self-module (src_mod == dst_mod);
DEDUP (set of distinct module pairs). MATCHES SQLite exactly (orchestrator skips `src_key == tgt_key`;
classification/module_edges.rs skips `source_module == target_module`; find_cycles SELECT DISTINCT).
  - include self-module edges instead?  REJECTED — SQLite skips them; including them would diverge and
    could fabricate single-module self-cycles SQLite never reports.
RECOMMENDATION: aggregate + skip-self + dedup (match SQLite). The FILE graph aggregated is the SAME union
`file_import_cycles` uses: resident `AstImport` edges UNION the cross-partition overlay.
```

### D3 — Surface
```text
A. HEADLESS `LiveGraph::module_import_cycles()` ONLY + a COMPARE harness vs SQLite.        [RECOMMENDED now]
B. + `rmap cycles --engine livegraph --kind module-import` (EXPLICIT, never default).      [follow-up]
C. a cycles `--engine compare` mode (SQLite answer + LiveGraph module-cycle diff).
RECOMMENDATION: A this slice (headless derivation + an equivalence harness that diffs LiveGraph module
cycles against SQLite `rmap cycles` on the fixture AND a real repo). B is the thin follow-up
(MODULE-CYCLES-CLI-1) once D5 equivalence is ratified — an EXPLICIT surface, NEVER a default flip. C is a
nice diagnostic but larger; defer. Rationale: equivalence is the deliverable; a CLI before equivalence is
proven would invite a premature default migration.
TRADE-OFF: headless-only means the new answer isn't user-visible yet — acceptable; this is an analysis
slice, and the harness is the artifact.
```

### D4 — Trust / scope honesty
```text
`module_import_cycles()` is EXACT only WITHIN (the captured FILE-import scope) AND (the module-aggregation
scope). It REUSES the file_import_cycles completeness model: all contributing partitions resident + Fresh +
TS -> Exact WITHIN SCOPE; a non-resident/stale/non-TS partition -> Partial/Stale + missing. The answer
NEVER claims "all module cycles" — the FILE-graph completeness caveat (package / path-alias / dynamic /
re-export NOT captured) propagates to the module level. The scope descriptor extends the D5 flag set with a
`module_aggregated: true` marker (and records the FILE-graph scope it aggregated).            [RECOMMENDED]
RECOMMENDATION: do not invent a new trust class; inherit file_import_cycles'. The honest claim is "module
cycles over the CAPTURED resolved-relative FILE import graph, aggregated by directory" — a strict subset of
"all module cycles".
```

### D5 — Equivalence gate (the heart of the slice)
```text
EQUIVALENCE CRITERION (to ratify): compare LiveGraph `module_import_cycles()` against SQLite `rmap cycles`
as SETS of cycles, each cycle a SET of module qualified-names (dir paths), order-independent.
  - FIXTURE (xpart-monorepo, pure relative imports): MUST be EXACT (same cycle set, same member module
    paths). A mismatch here is a derivation BUG -> stop.
  - REAL repo with module cycles: LiveGraph is expected to be a SUBSET of SQLite (the FILE-graph
    completeness gap: package/dynamic/path-alias imports that close a SQLite module ring are not in the
    captured graph). EACH divergence must be EXPLAINED (which module ring, which missing import form), NOT
    hand-waved. An UNEXPLAINED divergence (a LiveGraph cycle SQLite lacks, or a missing cycle with no
    completeness explanation) -> stop + report.
NO DEFAULT MIGRATION of `rmap cycles` until these criteria are RATIFIED and met (a separate slice). This
slice PRODUCES the evidence; it does not act on it.                                            [RECOMMENDED]
```

## Out of scope (hard guardrails)
```text
No `rmap cycles` DEFAULT flip. No raw `nodes`/`edges` decommission. No table/anything deletion. No
package-name / tsconfig-path-alias / dynamic-import / re-export expansion (the captured FILE graph stays
relative + ext/index only — closing that gap is IMPORTS-PACKAGE-RESOLUTION-1, a prerequisite for FULL
module-cycle parity). No new trust class. No module MEASUREMENTS (degree/complexity) — cycles only.
```

## Acceptance (EXECUTED later)
```text
1. headless `LiveGraph::module_import_cycles()` derives module cycles from the FILE import graph (resident
   AstImport UNION overlay) via dirname identity + skip-self + dedup + Tarjan; unit tests incl. a
   hand-built 2-module cross-partition cycle and an intra-module (no-edge) case.
2. the answer is trust-labelled like file_import_cycles (Exact within scope; non-resident -> Partial +
   missing) and is honestly scoped (module_aggregated; never "all module cycles").
3. an equivalence harness diffs LiveGraph module cycles vs SQLite `rmap cycles`:
   - fixture xpart-monorepo: EXACT set-equality (1 cycle, {packages/a/src, packages/b/src}).
   - an existing repo with SQLite module cycles: LiveGraph subset-of SQLite, every divergence explained.
4. full gate (workspace test, clippy -D warnings, fmt) + the harness EXECUTED + recorded.
5. NO default flip, NO decommission, NO deletion (guardrails verified).
```

## Build contract (PROPOSED — gated on ratification)
```text
1. repo-graph-livegraph: `module_import_cycles()` — build the FILE-edge union (the same source as
   file_import_cycles), map each endpoint via dirname identity, skip self-module, dedup, Tarjan SCC over
   the module pairs; trust-label by reusing the file_import_cycles completeness fold; a
   `ModuleImportCyclesAnswer` (+ scope `module_aggregated`). Unit tests.
2. equivalence harness: extend scripts/validate-xpart-fixture.sh (or a sibling) to also run SQLite `rmap
   cycles --json` + a headless/CLI module-cycle dump and assert set-equality on the fixture; a documented
   procedure to compare on a real repo with module cycles.
3. docs: completion + the equivalence EVIDENCE (fixture exact; real-repo divergences explained) + the
   ratified equivalence criteria for any future default migration.
```

## Follow-up slices
```text
- MODULE-CYCLES-CLI-1 : EXPLICIT `rmap cycles --engine livegraph --kind module-import` (never default), once
  D5 equivalence is ratified.
- IMPORTS-PACKAGE-RESOLUTION-1 : package-name + tsconfig path-alias resolution — the prerequisite for FULL
  module-cycle parity (closes the subset gap), required before any `rmap cycles` DEFAULT migration.
- CYCLES-DEFAULT-MIGRATION-1 : (much later) migrate the `rmap cycles` default — only after parity proven.
```

## References
- `rust/crates/indexer/src/orchestrator.rs` (`get_module_path` dirname identity; MODULE node + MODULE->MODULE edge materialization; skip same-module)
- `rust/crates/storage/src/queries.rs` (`find_cycles` — Tarjan over materialized MODULE IMPORTS edges)
- `rust/crates/classification/src/module_edges.rs` (`derive_module_dependency_edges` — skip intra-module, aggregate)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`file_import_cycles` + the overlay — the FILE graph to aggregate)
- `docs/slices/sqlite-raw-decommission-readiness-3.md` (the audit that gates `rmap cycles` on this)
- `rust/crates/repo-graph-scip-ingest/tests/fixtures/xpart-monorepo/` + `scripts/validate-xpart-fixture.sh` (the comparison fixture/harness)
