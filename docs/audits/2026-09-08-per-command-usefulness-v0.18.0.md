# Per-command usefulness audit — rmap v0.18.0 (audit round six), 2026-09-08

Artifact (house format): see ROADMAP link · local copy `~/Downloads/audit-v0180.html`.
Root causes: `docs/audits/2026-09-08-root-causes-v0.18.0.md` (RC-1 … RC-10 + operator field checks).

## Corpus and method
- Smoke `SMOKE_SKIP=linux ./scripts/smoke-validation-repos.sh --retain audit-v0.18.0` → run `smoke-runs/2026-09-08T17-59-32Z`: 24 passed / 5 failed / 1 skipped (linux, env). All five failures (repo-graph, codegraph, rabbitmq-tutorials `assess`; duckdb `orient --budget large`; gstreamer `deps list`) were typed `Busy` under batch load — 3 named the enrich pass + age, 2 the generic form — and all retested exit 0 uncontended. Retained root relocated to `~/repo-graph-retained/audit-v0.18.0` (registry rewritten). codegraph (TS+Rust) is new in the corpus.
- Supplementals: `agent-manager/scripts/audit18-supplemental.sh`, 45 probes against an isolated daemon on the retained root (`RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off`, killed by PID).
- Gate: matrix grader (rubric HIT/EVIDENCE/HONESTY/ECONOMY, ground-truthed) + judge (no rubric) + Codex gpt-5.6-terra standalone adjudication (read-only, evidence inlined, zero reconnects). Six read-only root-cause investigators over source and store copies.

## Dimension movement (v0.17.0 → v0.18.0)
HIT B− → B · EVIDENCE B → B+ · HONESTY C+ → B− · ECONOMY B− → B− (flat). 14 of 15 v0.17.0 defects FIXED or PARTIAL-by-design (D8 explain bare zeros carried by design — CLAIM-INVARIANT-1 not in the ratified order). "Largest single-round move" is the graders' judgement, not a defined cross-round metric; old-defect closure ≠ current product health (adjudicator).

## Grade matrix (HIT/EVIDENCE/HONESTY/ECONOMY)
| Command | Rust/TS-int | Java | C/C++ | Python | TS-legacy |
|---|---|---|---|---|---|
| orient small/med/large | A-/A-/B+/A | A-/A-/B+/B+ | B/A-/B/A- | B+/A-/B+/A- | B+/A-/B/B+ |
| orient --full | C+/B+/B-/D+ | C+/B+/B-/D+ | C+/B+/B-/D | C+/B+/B-/D+ | C+/B+/B-/D+ |
| check --full | B/A-/B+/A | B/A-/B+/A | B/A-/A-/A | B/A-/B+/A | B/A-/B+/A |
| trust | A-/A/C/B+ | A-/A/C-/B+ | B+/A/C+/B+ | B+/A/A-/B+ | A-/A/B+/B+ |
| modules list | A-/B+/A-/B | A-/B/A-/B- | C+/B+/A-/B | C/B+/A-/A- | C+/B+/A-/B |
| stats | B/B-/B+/C+ | B-/B-/B+/C+ | C+/B-/B+/C+ | B/B-/B+/C+ | B/B-/B+/C |
| cycles | A-/A-/A-/A | A-/A-/A-/A- | B+/A-/A-/A | A/A-/A-/A | A/A-/A-/B |
| churn / hotspots | B/B+/A-/A | D/B+/A-/A | D/B+/A-/A | D/B+/A-/A | C/B+/A-/A |
| risk | F/A/A/A | F/A/A/A | F/A/A/A | F/A/A/A | F/A/A/A |
| dead (refuses) | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A |
| surfaces list | A/A/A-/B- | A/A/A-/A- | F/—/A/— | F/—/A/— | A-/A/A-/A- |
| boundaries list/summary | C+/A-/A-/B | C+/A-/A-/B | C+/A-/A-/B | —/—/A/— | C+/A-/A-/B |
| resource list | D/A/A/A | D/A/A/A | D/A/A/A | D/A/A/A | D/A/A/A |
| docs list | B+/B+/B+/A | B/B+/B+/A | C+/B+/B/A | C+/B+/B/A | B+/B+/B+/A |
| inferences list | B+/A-/A-/A | B/A-/A-/A | —/—/A/— | —/—/A/— | A/A-/A-/A |
| deps list | A-/A-/A-/A | C+/A-/A-/A | —/A-/A/A | B+/A-/C+/A | A-/A-/A-/A- |
| map --dry-run | C+/A/A/C | C+/A/A/C | C+/A/A/C | C+/A/A/C | C+/A/A/C |
| doctor | B/A-/B/C+ | B/A-/B/C+ | B/A-/B/C+ | B/A-/B/C+ | B/A-/B/C+ |
| gate / assess / violations | F/—/A/A | F/—/A/A | F/—/A/A | F/—/A/A | F/—/A/A |
| find exact-symbol | A-/B+/A-/A- | B+/B+/A-/A- | B+/A-/A-/A- | B/B+/A-/A- | B+/B+/A-/A- |
| find concept-seeds | C+/B+/B+/A- | B-/B+/B+/A- | B/B+/B+/A- | B-/B+/B+/A- | B-/B+/B+/A- |
| find --text | not probed | — | — | — | — |
| explain / callers / callees | D+/C+/D+/B- | D/C+/D/B- | D+/C+/D/B- | D/C+/D/B- | D/C+/D/B- |

## Shipped slices — in the field
- IMPORT-RESOLUTION-RUST-1 FIXED (repo-graph 129 cross-crate edges; zero-states carry resolved+unresolved counts). IMPORT-RESOLUTION-JAVA-1 FIXED/partial (kafka 218, hadoop hdfs→common 11,874, grpc 193; Maven absence named; grpc edge list uses a different identifier space than its table — D-N10).
- DAEMON-RESIDUALS-2/2B/2C fixed where observable (worst query 25 s vs 379/1,001 s; `repo rebuild` usage-verified); retention observability DORMANT on the audit root AND the production daemon ("cleanup: none yet", 1 snapshot) — the deep-vertical rule.
- CPP-DECLARATORS-1 wins real (macro names exact; `(decl)` below definitions; type ahead of constructors) but NET-NEGATIVE for relationship discovery: its enclosing-class call preference fabricates self-loop CALLS edges on every C++ repo (RC-1). IMPLEMENTS 2→852 not exercised by any capture — provisional; RC-3 shows those edges anchored on the FILE node.
- SYMBOL-IDENTITY-1 mechanism FIXED (qualified suffix resolves; a miss never `Confidence: high`); the answer for the flagship symbol is inverted by RC-1.
- DEPS-CLASSIFIER-1/1B FIXED/partial (asgiref used 51/131; per-row basis; type-only; `--ecosystem npm` leaks Python names — RC-6). HEADLINE-TRUTH-1 FIXED (grpc 1917 = 1627+290; "0 project surfaces" gone corpus-wide; `--full` marker; inferences `[test]`+anchors). MODULES-METHOD-1 FIXED/partial (method line; docs recommendation correct 26/29, blind to root README.rst/.txt — RC-8). SEED-CHUNK-3 partial by design (0 `[field]` in FRAKTAG top 10; ConversationManager rank 2; SEED-DOCUMENT-1 filed). AUDIT5-MINORS-1 FIXED (architecture 577→28; boundaries line sets 26/26; LiveGraph posture lines 28/28→0; Express provider lines 47/47; smoke accounting; doctor `[note]` renders on the production daemon). EXIT-CODES-1 FIXED (contract doc; `dead` exit 4 on 29/29 graded repos; `check --full` 0/1/2 honoured).

## Where the fabrication class went
Judge: retreated into the symbol layer (worst output `callers leveldb::DBImpl::Recover`). Matrix: relocated into the reliability layer (trust zero-connectivity on ten repos); new dominant class = coherence under composition (trust vs modules, callers vs surfaces, table vs edge list, printed scores vs order, exclusion predicates applied by three surfaces and skipped by a fourth). Adjudicator: both right, "relocated" is plural; fix order RC-1 → trust → include roots → explain cycles → deps bleed → complexity scope → docs coverage → Python inherited calls; none is instrument navel-gazing.

## Fix queue — cut along root causes (human orders)
| # | Slice | Sev | Kind | Outward outcome | Root cause |
|---|---|---|---|---|---|
| Q1 | CALL-BINDING-RECEIVER-1 | critical | REGRESSION 5c3ec2d | callers/callees/explain on every C++ repo stop inventing self-callers and dropping real ones; `dead`/trust stop inheriting the fabricated fan-in (self-loops 0→155 leveldb/497 OpenXcom/836 vcmi/1098 poco/2522 duckdb/321 gstreamer) | RC-1 |
| Q2 | TRUST-MODULE-EDGES-1 | critical | REGRESSION 28126a2 | trust's zero-connectivity list and the alias-resolution downgrade agree with `modules list` (repo-graph 48→0); Import-graph stays LOW while unresolved>0 — the false REASON goes | RC-5 |
| Q3 | CPP-INCLUDE-ROOTS-1 | high | never worked | poco modules list gains ~50 cross-module pairs (Net→Foundation 616); unresolved 13,703→3,810; duckdb/OpenXcom gain; increment 2 = unique multi-segment suffix for gstreamer (+9,775) | RC-10 |
| Q4 | EXPLAIN-CYCLES-HONEST-1 | high | never worked | explain's "Import cycles" stops drawing arrows over a sorted member set (~15 lines, reuse cycles' render_unordered) | RC-4 |
| Q5 | DEPS-ECOSYSTEM-PARTITION-1 | high | never worked (visibility regressed d333214) | django `--ecosystem npm` undeclared 102→0 with the split named; gstreamer names only reader-less languages and lists its 13 parsed manifests | RC-6, RC-7 |
| Q6 | EXPLAIN-TYPE-SECTIONS-1 | high | never worked, language-wide | `explain <Type>` lists members and referencing files (render-only, every language), then C++ bases/derived (anchor fix + macro clause recovery) | RC-3 |
| Q7 | COMPLEXITY-SCOPE-1 | high | never worked | complexity centers exclude generated/vendored/test symbols and say so (poco top-5 → Foundation encodings; codegraph → handleExplore); vscode's weights table is an honest residual | RC-9 |
| Q8 | DOCS-DISCOVERY-1 | medium | never worked | hadoop docs 23→~545 + "read: README.txt"; django readme 4→6; buildroot 4→~77; one stem list read by discovery/classification/recommendation | RC-8 |
| Q9 | PYTHON-SELF-BINDING-1 | medium | never worked | `explain BaseHandler.get_response` Callers 0→2; ~+3,600 django edges via superclass metadata BFS; MRO collisions stay unresolved | RC-2 |

Carried/minor (surface, not packeted): CLAIM-INVARIANT-1 (D-N9 bare zeros beside a same-snapshot surface row / type 0-0); D-N10 modules identifier spaces; D-N7 find seed order after decl demotion; retention line always states standing; Busy messages leak the store path (2 of 5 generic — DAEMON-RESIDUALS-3); surfaces list / orient --full budgets.

## Keep and imitate
codegraph modules list self-reported non-reconciliation · cycles' empty-graph zero-state with denominators · modules list's resolution-gap note · check's ceiling-relative verdict · churn's clone-depth attribution · language-named blind spots (resource list Rust; vcmi boundaries C++) · map --dry-run cap + 11 unmapped named · dead's refusal at exit 4 with snapshot-derived causes · surfaces list [test] partition + dual-implementation finding · docs list excluded-and-counted pattern · deps "≠ unused" disclaimer with numbers.

## Verdict
At the sites v0.17.0 named the command-summary fabrication class is dead. CPP-DECLARATORS-1 shipped a fabrication into the C++ call graph and its ratifying review read the self-loop as a win — the missing packet invariant: an indirect receiver never binds to the enclosing class without type evidence. Everywhere else no output invents a row not in the store; the dominant class is coherence under composition. Q1, Q2 and Q4 come first because they are the only items where the product currently asserts a relationship that does not exist.

## Could not ground-truth
`find --text` (no capture); C++ IMPLEMENTS from a class focus; FK-index prune cost (nothing prunable on one-snapshot roots).
