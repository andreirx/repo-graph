# HEADLINE-TRUTH-1 — a headline is a truthful projection of the rows beneath it

Status: SPECIFIED (2026-09-06) · Track: audit round five, group D (human-ratified 2026-09-06).
CODE slice, rgr presentation (orient, stats, modules, surfaces, inferences, dead) + agent
summary + storage inference query + spring classifier. Maturity: MATURE (orient/stats/
surfaces/inferences/dead are the product's first-read surfaces).

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §D)

Outward surface: the first lines an agent reads. Six headlines contradict their own detail:

- **D5 — three file totals per snapshot.** grpc-java orient "1917 files indexed" vs stats
  "total_files: 1627 (in directory groups)"; Σ modules-owned > stats total in 5 repos
  (django 3015 > 3014); FRAKTAG "92 files indexed" above rows summing to 85. Three bases:
  `file_versions` counts every tracked row incl. config / `.proto` / read-failed files
  (`agent_impl.rs:256-260`); "in directory groups" counts OWNS edges, written only for FILE
  nodes whose path contains a `/` (`resolver.rs:782-789` returns None for root-level files;
  config/proto/failed files have no FILE node); "owned" counts manifest ownership where the
  root manifest `.` matches root-level files too. Verified exact: grpc-java 290 = 96 config
  + 194 .proto; django +1 = `Gruntfile.js`; FRAKTAG 7 config. COHERENCE-3 named the bases
  but no surface states the inclusion rule (the stats.rs:219-222 comment admits it; never
  rendered).
- **D12 — a symbol's cx under a file label.** `complexity_line` (`orient_sections.rs:153-178`)
  iterates per-SYMBOL rows, dedups by file, and prints `db_bench.cc (cx 36)` — the FIRST
  symbol's cx as if it were the file's, dropping `Render (95)` on vcmi even though it outranks
  the next file's top. Pinned by `orient_density_tests.rs:410-450`.
- **D7 — "[--full identical to --budget large (nothing further to show)]" 55 lines below
  "… and 30 more groups".** ECONOMY-2 REGRESSION (8a6f1df): the predicate is byte-equality
  with large (`orient.rs:319-343`, precedence over completeness by comment), while the
  directory-group fallback is capped at a fixed `FALLBACK_TOP_N = 12`
  (`orient_topology_fallback.rs:61,209`) at every tier, so --full cannot expand it.
  `budget_saturated` knows it is an elision and is never consulted. Pinned by
  `orient_seg2_tests.rs:754,785`.
- **D6 — "0 project surfaces" above 235 real routes; test rows first.** `count` is the
  catalog AFTER http rows are lifted out (`dispatch.rs:7356-7361`) — structurally 0 on every
  HTTP repo; row sort key starts with direction (`http_boundary.rs:334-345`, "consumer" <
  "provider" lexically) and `is_test` is field 6, so `[test]` fixtures interleave above real
  routes; headline excludes them, rows include them (21 under "17").
- **D9 — a 10-line test fixture becomes a repo-level Spring fact.** Inference rows never join
  `files.is_test` (`queries.rs:2616-2619`); Spring omits `line_start` though the node has a
  location (`spring_liveness.rs:233-266`); `dead` counts every row (`dead_causes.rs:92-96`)
  and states "Framework liveness inferences exist (Spring: 1)" as a cause on repo-graph.
- **COHERENCE-2 gap — the type-only verdict vanishes whenever a test-only partition exists.**
  `orient_sections.rs:318-321` gates BOTH the anchor and the "type-only breaks the cycle at
  runtime" label on `test_only == 0`; vscode (+5) and repo-graph (+1) lose the verdict
  `cycles` prints. And "(N test)" (subset) vs "(+N test-only excluded)" (addend) share one
  parenthesis with opposite meanings and no legend.

## 2. Contract

1. **One file universe, stated once.** orient's header states the split it counts:
   `1917 files indexed (1627 source; 290 config/contract/unreadable, tracked only)` — one
   extra count in `compute_repo_summary` over `parse_status IN ('config','failed') OR
   extractor = 'contract-schema'`. stats's `total_files` line states its exclusion:
   `(in directory groups; N root-level or tracked-only files not grouped)` where N =
   indexed − Σ groups, computed not asserted. modules-list's Σ owned, when it exceeds the
   grouped total, says so in a footer naming the count of root-level files. The three
   numbers reconcile arithmetically on every smoke repo, by the stated formulas.
2. **Complexity headline names the symbol.** `db_bench.cc — Run (cx 36)`; no dedup by file —
   the headline is the top-N SYMBOLS (N unchanged), so a second center in the same file is
   listed if it ranks. Detail unchanged.
3. **The `--full` marker tells the truth about elision.** When `--full` equals `--budget
   large` AND the output still elided rows under any cap (`budget_saturated()` false), the
   line reads `[--full identical to --budget large — N group rows elided by the fixed cap;
   see stats]`; the "nothing further to show" wording is emitted ONLY when nothing was
   elided. (Making the fallback tier-aware is acceptable instead, if smaller; state which.)
4. **Surfaces headline and rows agree in count and order.** The "project surfaces" line is
   omitted when its count is 0 and HTTP surfaces are present (or names what it counts:
   `0 non-HTTP surfaces`). Rows order: providers, then consumers, then a
   `test fixtures (excluded from counts):` section — `is_test` leads the sort key.
5. **Inferences carry the test partition and an anchor.** The inference query projects
   `files.is_test`; rows render `[test]`; Spring rows carry `file:line` like React rows
   (`line_start` written by the classifier); `dead`'s framework line reports
   `Spring: 1 (1 in test fixtures)` and does not cite fixture-only inferences as a cause.
6. **The type-only verdict survives the test-only partition.** orient reads the SCC state
   from the first PRODUCTION cycle and renders the label independently of the anchor gate;
   vscode and repo-graph orient print the same verdict as `cycles`. One legend line
   distinguishes `(N test)` = of which, from `(+N test-only excluded)` = in addition.
7. **Tests that pinned the defects are flipped, not deleted**: `orient_density_tests.rs:412`,
   `orient_seg2_tests.rs:754/785`, the surfaces ordering test, `check_repo.rs:61` gains the
   reconciliation identity.

## 3. Stop conditions

Frozen: wire protocol (any new field is ADDITIVE), exit codes, storage schema shape (the
inference `is_test` comes from a join, not a column). If §2.1 reveals a FOURTH basis in the
field (a repo where indexed − Σ groups ≠ root-level + tracked-only by the stated formula),
record the repo and the residual and STOP + DECISION_REQUIRED — never fit the formula to the
number. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the
operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Flip the pinning tests FIRST (they must fail before the fix).
- Live proof (isolated; registry sha identical): grpc-java orient/stats/modules totals
  reconcile by the stated formulas (290 = 96 + 194); django Σ owned footer names 1; leveldb
  orient headline `db_bench.cc — Run (cx 36)` and vcmi lists `Render (95)`; zvec `orient
  --full` marker names the elided count; spring-petclinic / glamCRM surfaces: no "0 project
  surfaces", providers first, fixtures sectioned last, 17 = 17 rows before the section;
  repo-graph `inferences list` shows `App.java:… [test]` and `dead` says "(1 in test
  fixtures)"; vscode + repo-graph orient print the cycles' type-only verdict.
- Gates recorded FIRST; chunked cargo; witness; dogfood-isolated.

## 5. Definition of done

Every headline named above states its basis and reconciles with its rows by a rendered
formula; the complexity headline names symbols; the --full marker never denies an elision;
surfaces order providers → consumers → fixtures with matching counts; inferences and dead
partition fixtures; orient prints the cycles' type-only verdict on every repo; the pinning
tests assert the truthful behaviour; gates green.

CORPUS PATHS: grpc-java, django, leveldb, vcmi, zvec-grep, spring-petclinic, vscode at
../legacy-codebases/<name>; glamCRM and FRAKTAG under ../; repo-graph is THIS repo.
