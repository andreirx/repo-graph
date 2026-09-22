# Per-command usefulness audit — rmap v0.19.0 (audit round seven), 2026-09-23

Artifact (house format): see ROADMAP link. Root causes: `docs/audits/2026-09-23-root-causes-v0.19.0.md` (RC-1 and RC-2 verified from source by the operator; RC-3 … RC-9 grader-observed with the cited mechanism, to be established at code level before any packet).

## Corpus and method
- Release: v0.19.0 (`05a68eb`, tag pushed; installed locally with `scripts/dev-install-local.sh`). Eleven slices shipped since v0.18.0 under the requirements-assurance paradigm (TRUST-MODULE-EDGES-1, CALL-BINDING-RECEIVER-1, CPP-INCLUDE-ROOTS-1, EXPLAIN-CYCLES-HONEST-1, DEPS-ECOSYSTEM-PARTITION-1, EXPLAIN-TYPE-SECTIONS-1 inc 1, COMPLEXITY-SCOPE-1, DOCS-DISCOVERY-1, PYTHON-SELF-BINDING-1, ALIAS-SUSPICION-1, SEED-DOCUMENT-1).
- Smoke: `SMOKE_SKIP=linux ./scripts/smoke-validation-repos.sh --retain audit-v0.19.0` → run `smoke-runs/2026-09-22T22-39-58Z`: 26 passed / 3 failed / 1 skipped (linux, env). Failures: repo-graph `check --full` exit 1 = the documented Fail verdict (CALL_GRAPH_RELIABILITY 24 % resolved — truthful); repo-graph `assess` and django `cycles` = transient Busy, retested clean uncontended; poco `docs list` exit 2 in both modes = RC-1 (a real defect); `dead` exit 4 = the documented verdict. Accounting gap: kafka `check --full` was also Busy in the batch and not counted (N9). Retained root relocated to `~/repo-graph-retained/audit-v0.19.0` (16 GB, `db_path` rewritten).
- Supplementals: `agent-manager/scripts/audit19-supplemental.sh`, 61 probes re-checking every shipped slice's recorded AFTER literal (prefixes tme/as/cbr/cir/ech/dep/ets/cs/dd/psb/sd/ho), read-only over stdio on the retained root with enrich/retention off.
- Gate: matrix grader (rubric HIT/EVIDENCE/HONESTY/ECONOMY, ground-truthed against the checkouts) + judge (no rubric, 61 per-capture verdicts) + Codex gpt-5.6-terra standalone adjudication (read-only, evidence inlined, zero reconnects). Operator verification: RC-1 (poco) and RC-2 (`callers ListMixin.extend` = 271 callers on the retained root; the dotted-name fallback at `indexer/src/resolver.rs:1012-1050`).

## Dimension movement (v0.18.0 → v0.19.0)
HIT B → B+ · EVIDENCE B+ → A- · HONESTY B- → B · ECONOMY B- → B- (flat). Every shipped slice's recorded literal was found in the field: 11 FIXED, 0 PARTIAL, 0 DORMANT, 1 REGRESSED side effect (DOCS-DISCOVERY-1's widening exposed the poco decode crash — counted inside its FIXED row). The adjudicator qualifies: CALL-BINDING-RECEIVER-1 fixed for the C++ receiver case only; EXPLAIN-TYPE-SECTIONS-1's `Referenced by` undercounts on Python (N4); SEED-DOCUMENT-1 is literal compliance (rank 8 of 10 at 0.33), not a strong product gain.

## Grade matrix (HIT/EVIDENCE/HONESTY/ECONOMY)
| Command | Rust/TS-int | Java | C/C++ | Python | TS-legacy |
|---|---|---|---|---|---|
| orient small/med/large | A-/A-/A-/A | A-/A-/A-/B+ | B+/A-/A-/A- | B+/A-/A-/A- | B+/A-/B+/B+ |
| orient --full | C+/B+/B-/D+ | C+/B+/B-/D+ | C+/B+/B-/D | C+/B+/B-/D+ | C+/B+/B-/D+ |
| check --full | B/A-/B+/A | B/A-/B+/A | B/A-/A-/A | B/A-/A-/A | B/A-/B+/A |
| trust | A-/A/B+/B+ | A-/A/A-/B+ | B+/A/B+/B+ | B+/A/B+/B+ | A-/A/B+/B+ |
| modules list | A-/B+/A-/B | A-/B+/A-/B- | B/A-/A-/B | C+/B+/A-/A- | C+/B+/B+/B |
| stats | B/B-/B+/C+ | B-/B-/B+/C | C+/B-/B+/C+ | B/B-/B+/C+ | B/B-/B+/C |
| cycles | A-/A-/A-/A | A-/A-/A-/A- | A-/A-/A-/A | A/A-/A-/A | A/A-/A-/B |
| churn / hotspots | B/B+/A-/A | D/B+/A-/A | D/B+/A-/A | D/B+/A-/A | C/B+/A-/A |
| risk | F/A/A/A | F/A/A/A | F/A/A/A | F/A/A/A | F/A/A/A |
| dead (refuses) | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A |
| surfaces list | A/A/A-/B- | A/A/A-/A- | F/—/A/— | F/—/A/— | A-/A/A-/A- |
| boundaries list/summary | C+/A-/A-/B | C+/A-/A-/B | C+/A-/A-/B | —/—/A/— | C+/A-/A-/B |
| resource list | D/A/A/A | D/A/A/A | D/A/B+/A | D/A/A/A | D/A/A/A |
| docs list | B+/B+/B+/A | A-/A-/A-/A | C-/B+/B/A | B+/A-/A-/A | B+/B+/B+/A |
| inferences list | B+/A-/A-/A | B/A-/A-/A | —/—/A/— | —/—/A/— | A/A-/A-/A |
| deps list | A-/A-/A-/A | C+/A-/A-/A | B-/A-/A/A | B+/A-/A-/A | A-/A-/A-/A- |
| map --dry-run | C+/A/A/C | C+/A/A/C | C+/A/A/C | C+/A/A/C | C+/A/A/C |
| doctor | B/A-/B+/C+ | B/A-/B+/C+ | B/A-/B+/C+ | B/A-/B+/C+ | B/A-/B+/C+ |
| gate / assess / violations | F/—/A/A | F/—/A/A | F/—/A/A | F/—/A/A | F/—/A/A |
| find exact-symbol | A-/B+/A-/A- | n/p [B+/B+/A-/A-] | B+/A-/B/A- | B/B+/A-/A- | n/p [B+/B+/A-/A-] |
| find concept-seeds | B-/B+/B+/A- | n/p [B-/B+/B+/A-] | B/B+/B+/A- | n/p [B-/B+/B+/A-] | n/p |
| find --text | not probed | — | — | — | — |
| explain / callers / callees | B-/B+/B+/B | n/p [D/C+/D/B-] | B+/A-/B+/B | B-/B+/C/B | n/p [D/C+/D/B-] |

(`n/p` = not probed this round; the bracketed grade is carried from v0.18.0.) Movements: explain/callers/callees C/C++ D+ → B+, Python D → B- (HONESTY C from RC-2); trust HONESTY C/C-/C+ → B+/A-/B+; modules list C/C++ C+ → B; docs list Java B → A-, Python C+ → B+, C/C++ C+ → C- (the poco crash); deps Python HONESTY C+ → A-; find exact-symbol C/C++ HONESTY A- → B (phantom macro methods).

## Shipped slices — in the field (code under analysis)
- CALL-BINDING-RECEIVER-1: `callers leveldb::DBImpl::Recover` = `leveldb::DB::Open db/db_impl.cc:1503` (call at :1511); the fabricated self-row is gone; trust 40 % (3488 of 8827).
- CPP-INCLUDE-ROOTS-1: poco `79 cross-module dependencies (3810 imports unresolved)`, `Net → Foundation (616 file-level imports)`, a real cycle `Foundation/include/Poco -> Foundation/include/Poco/Dynamic`.
- EXPLAIN-CYCLES-HONEST-1: leveldb explain's ring == cycles' ring on the same index (`db -> table -> db (+ 2 more members)`); vcmi 55-module SCC as `members (unordered)`.
- DEPS-ECOSYSTEM-PARTITION-1: django `deps list --ecosystem npm` `undeclared 0` with `13956 of 13967 … (13956 python)`; gstreamer names the 13 foreign manifests parsed.
- EXPLAIN-TYPE-SECTIONS-1: `explain CGHeroInstance` `Members (135)`, `Referenced by (178 files)`; `explain BaseHandler` `Members (10)` `load_middleware base.py:27`.
- COMPLEXITY-SCOPE-1: poco `Windows1251Encoding.cpp — convert (cx 345)` first, `218 … excluded — --include-all`.
- DOCS-DISCOVERY-1: hadoop `556 documents` (559 entries = 556 + 3 vendored — the two graders' counts reconciled), `read: README.txt`; django `read: CONTRIBUTING.rst, README.rst, docs/`; buildroot 310.
- PYTHON-SELF-BINDING-1: `callers BaseHandler.get_response` = `WSGIHandler.__call__ wsgi.py:120`, `ClientHandler.__call__ client.py:169`; trust 46 %.
- ALIAS-SUSPICION-1: FRAKTAG `Alias resolution suspected — packages/ui (55 imports through a project alias did not resolve)`; kafka's false reason and its deps footnote gone.
- SEED-DOCUMENT-1: FRAKTAG `find "where are conversations persisted to disk"` → `ConversationManager.createSession (0.33)` rank 8, class rank 3; controls first; leveldb identical.
- TRUST-MODULE-EDGES-1: repo-graph Suspicious Modules = the six genuinely isolated candidates; kafka's five non-code directories.

## Where the fabrication class went
Round six's symbol-layer class (receiver self-loops, invented cycle arrows, empty explain on types) is dead at every audited site — and disproved as a universal invariant by two remnants: RC-2 (Python `obj.method()` bound by unique method name with the receiver ignored — `dependencies.extend(` → GIS `ListMixin.extend`, 271 false callers) and RC-4 (a trailing attribute macro on a C++ field extracted as a METHOD — `leveldb::DBImpl::GUARDED_BY` ×13). The new dominant class is SCOPE MISATTRIBUTION: true facts from test, build-script, vendored or first-party scope labeled as the repository's production story — poco `Foundation → CppUnit (415)` from `Foundation/testsuite/` only; leveldb's `table -> db` cycle via `table/table_test.cc`; a root buildscript classpath attributed to all 61 kafka modules; a nested Font Awesome license standing in for django's; FRAKTAG's `@fraktag/engine` workspace import seen by `deps`, invisible to `modules`/`trust` ("isolated"). Every number is real and the label is wrong, so nothing contradicts the source until the agent asks "which files?".

## Fix queue — cut along root causes (adjudicator's order; the human may reorder)
1. **Certainty gate for symbol and call facts**: PYTHON-RECEIVER-BINDING-1 (RC-2, verified; a 2-part `obj.m` with an untyped bare-name receiver never binds by unique method name — stays unresolved with a named basis and persisted candidates; resolved % will drop and trust says why) → CPP-ATTRIBUTE-MACRO-1 (RC-4; `field MACRO(args);` is a data member, never a METHOD).
2. **Source-scope propagation for module edges**: TEST-EDGE-SCOPE-1 + IS-TEST-CPPUNIT-1 (RC-3; module edges / cycles / orient / explain-cycles follow the importing file's stored `is_test`; largest cross-surface blast radius — six misleading captures).
3. **Import-target resolution, per language, no shared framework**: CPP-INCLUDE-BASENAME-1 (RC-5; nginx's 1,142 single-segment `<ngx_*.h>` includes — a DECISION for the human: unique-basename binding is the guessing RG-REQ-006-L03 forbids unless bound to a uniqueness proof), TS-WORKSPACE-RESOLUTION-1 (RC-6; npm workspace package names — FRAKTAG, amodx, glamCRM), PYTHON-SUBMODULE-IMPORT-1 (RC-7; `from pkg import module` → `pkg/module.py`, not `__init__.py`).
4. **DOCS-UNREADABLE-DECODE-1** (RC-1, verified; one DTO field to `Option`, fixture, poco proof) — executed FIRST in practice: a crash, no decision, an hour.
5. **DEPS-GRADLE-CATALOG-1** (RC-8; kafka's root buildscript classpath on 61 modules; `libs.*` catalog declarations unparsed; own package flagged undeclared).
6. Document scope: django's root `LICENSE`/`LICENSE.python` absent — BY THE HUMAN'S RULING (no `license` stem, 2026-09-22); recorded, not queued. N7 (`nginx/auto/install` as a doc — the ratified bare-stem rule's known residual class), N12 (generated directories in docs) only if shown to mislead.
7. Cross-surface identity (N10: codegraph's two `codegraph-kernel` modules; storybook directory ids vs npm names) after the structural facts are trustworthy; then N8 (`resource list` ignores the vendored predicate), N9 (smoke accounting), N11 (vscode orient 24 s at every budget).
Carried follow-ups unchanged: CYCLES-WALK-DETERMINISM-1 (this index's leveldb ring `db -> table -> db` vs the ship block's `util -> helpers/memenv -> util`, both real), EXPLAIN-BASES-1, DOWNGRADE-LABELS-1, TRUST-CEILING-WORDING-1, CALLERS-ANCHOR-1, SEED-LOGTURN-1, SEED-CPP-CLASS-DOC-1, DOC-RELEVANCE-STEMS-1, CLAIM-INVARIANT-1; CONCERN-HINTS-1 shows no evidence in any capture.

## Keep and imitate
trust's basis line and per-module alias row; `79 cross-module dependencies (3810 imports unresolved)`; the type zero-line `Callers (0) — a type is not called; see Members / Referenced by`; `members (unordered)`; the deps partition sentence with the flag to run; `218 … excluded — --include-all`; `+N … not scanned (--json for the rule)`; nginx cycles' honest EMPTY-graph zero-state; doctor's retention standing line; codegraph/vscode self-reported non-reconciliation.

## Verdict
The queue did what it was cut to do: every one of the eleven recorded outcomes holds in the field, the symbol-layer fabrications of round six are gone where they were found, and HIT/EVIDENCE/HONESTY each moved a step. What remains is honest about the index and wrong about the repository — scope misattribution and two remnants of name-only binding — plus one crash that turned an honesty rule (unreadable documents are counted) into a failure. The adjudicator's unsupported-claim list (both graders over-generalized "dead in the field"; the matrix's "270 false callers" and "~315 edges" are extrapolations; the judge's nginx remedy names the wrong follow-up) is recorded in `/private/tmp/audit19/codex-verdict.txt` and applied here.

## Could not ground-truth
`find --text`; Java and TS-legacy explain/find (grades carried); C++ IMPLEMENTS from a class focus; vcmi rings' edge sets; the exact `excluded_generated` counts (hadoop 6813 / grpc 2911 vs `MAP.md` files on disk) — re-measure at the next round.
