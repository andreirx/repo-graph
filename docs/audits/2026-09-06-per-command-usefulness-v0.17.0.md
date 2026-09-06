# Per-command usefulness audit — v0.17.0 (round five)

Date: 2026-09-06 · Build: v0.17.0 (28b2ff6) · Smoke: `smoke-runs/2026-09-06T03-20-35Z` (26 passed;
repo-graph `assess` + django `orient --full` bounced with a typed NAMED Busy under batch load, both
retested clean; linux silently dropped by the harness — in `legacy_repos`, in none of
passed/failed/skipped) · 32 supplemental probes (`/private/tmp/audit17/supplemental.sh`, captures
at `/private/tmp/audit17/captures/`) re-checking every slice shipped since v0.16.0 · state root
`/private/tmp/repo-graph-tests/audit-v0.17.0`.

Gate: three perspectives — matrix grader (28-repo sweep, 74 ground-truth checks), no-rubric judge
(supplementals + smoke selection), codex standalone adjudication (evidence inlined, tool-free,
`/private/tmp/audit17/codex-review.txt` → `codex-verdict.txt`). Artifact:
`/private/tmp/audit17/audit-v0170.html`.

## Verdict (codex-adjudicated)

v0.17.0 killed every fabrication it targeted and moved every dimension up (HIT C+→B−, EVIDENCE
C+→B, HONESTY C−→C+, ECONOMY C→B−; `find` vs zg: level on HIT, leads on EVIDENCE/HONESTY/ECONOMY).
Verified dead: hadoop phantom writers (→ `access (mode unknown)`), the Next.js name-only detection
(two-signal gate; amodx's true detection keeps `basis:`), storybook 13+111=124, JDK-only invented
edges (→ `External / unresolved (11)`); vscode `.env` retraction confirmed; C++ `--text` attribution
exact 6/6; `map --dry-run` reconciles to the file.

**The class did not die — it relocated to the command-summary layer.** The graders disagree on
taxonomy (judge: "absence stated as a finding", a new less-dishonest class; matrix: "not dead,
relocated, worst instance stronger than anything last round"); codex adopts the matrix framing:
"the product moved from fabricating concrete rows/edges to fabricating authoritative absence claims
from incomplete or low-trust graphs" — a positive architectural assertion contradicted by the
binary's own `trust` output on the same snapshot is a fabrication-class failure. The repair was
applied per surface, not as an invariant. Next round leads with a cross-command claim invariant.

## Shipped-fix verdicts

| Slice / check | Verdict | Evidence |
|---|---|---|
| HONESTY-GATE-1 counts reconcile | FIXED | storybook 13+111=124 across 12 manifests; zvec 8+14=22 |
| HONESTY-GATE-1 Maven absence named | FIXED | hadoop "Maven manifests are not parsed on this build (119 pom.xml present)" |
| HONESTY-GATE-1 no unsupported negatives | REGRESSED-IN-PLACE | django "no static import: asgiref" — 42 static imports, 31 in `django/`; caveat's escape hatches don't apply |
| HONESTY-GATE-2 unknown access mode | FIXED | hadoop "5 access (mode unknown)"; `/dev/loop-control` 1r/1w = `open(…,O_RDWR)` runc.c:339 |
| HONESTY-GATE-2 Next.js two-signal gate | FIXED (exemplary) | hadoop zero nextjs mentions; amodx `basis: renderer/next.config.ts present` |
| HONESTY-GATE-2 vscode .env retained | FIXED | 2r/2w = 2 readFileSync + 2 writeFileSync in getToken.mts/getEnv.mts |
| CURSOR-ROUNDTRIP-1 | PARTIAL | find 0 repo-uids; `callers` not-found 47% cursor bytes |
| CPP-SPAN-FIDELITY-1 macro names | PARTIAL | classes fixed; 26 function rows still URI_FUNC/PREFIX/ZSTD_ALLOW_POINTER_OVERFLOW_ATTR |
| CPP-SPAN-FIDELITY-1 --text attribution | FIXED (exact) | fsync 6/6; `[method leveldb::PosixWritableFile::SyncFd]` |
| ANCHORS-EVERYWHERE-1 | PARTIAL | orient detail 200/200; headline 0/130; boundaries 0/772; Express providers 0/57 |
| SEED-CHUNK-2 decl-below-impl | FIXED (C++) | db_impl.cc:292 Recover above db_impl.h:113 (decl) |
| SEED-CHUNK-2 referral always-on | FIXED | 4/4 |
| SEED-CHUNK-2 partition / decl demotion | PARTIAL | FRAKTAG "persisted to disk" → 6/10 property decls, zero write-path symbols |
| COHERENCE-2 SCC type-only identical | PARTIAL | dropped in orient for vscode + repo-graph (test-excluded branch) |
| COHERENCE-2 (N test) subset | PARTIAL | no legend; sibling "(+N test-only excluded)" is an addend |
| COHERENCE-3 walk agreement | FIXED | 146/146 cycles' member arithmetic clean |
| COHERENCE-3 surface headline partition | PARTIAL | arithmetic exact; excluded rows printed unsectioned, petclinic test rows first |
| COHERENCE-3 file totals basis | REGRESSED-IN-PLACE | grpc-java 1917 vs 1627 (identical symbol totals); Σ modules-owned > stats in 5 repos |
| DAEMON-RESIDUALS-1 named Busy | PARTIAL | layer 1 names holder+age; layer 2 generic (→ DR-3 #4 `OpKind::Seed`); store path leaks |
| ECONOMY-2 seed cursors / --full ladder | FIXED, one falsehood | zvec --full "identical to --budget large" 55 lines below "… and 30 more groups" |
| ECONOMY-2 bounded map dry-run | FIXED (model) | 3082+14052=17134; 14052+125=14177 |

## Grade matrix (HIT/EVIDENCE/HONESTY/ECONOMY)

| Command | RUST/TS-INT | JAVA | C/C++ | PY | TS-LEG |
|---|---|---|---|---|---|
| orient small/med/large | A-/B/C+/A | B/B/C+/B- | A-/B/C+/A | B+/B/C+/A- | B+/B/C+/B |
| orient --full | C/B/C/D | C/B/C/D | C/B/C/D | C/B/C/D | C/B/C/D |
| check --full | B/A-/C/A | B/A-/C/A | B/A-/C/A | B/A-/C/A | B/A-/C/A |
| trust | A-/A/A-/A- | A-/A/A-/A- | B+/A/A-/A- | B+/A/A-/A- | A-/A/A-/A- |
| modules list | C/C/F/B | C/C/F/B | C+/C/F/B | C/C/F/B | C/C/F/B |
| stats | B/C/C+/B | D/C/C+/C | C/C/C+/B | B/C/C+/B | B/C/C+/B |
| cycles | B/B+/C/A | F/B+/D/A | B+/B+/C/A | A/B+/C/A | A/B+/C/A |
| churn / hotspots | B/B+/B/A | D/B+/B/A | D/B+/B/A | D/B+/B/A | C/B+/B/A |
| risk | F/A/A/A | F/A/A/A | F/A/A/A | F/A/A/A | F/A/A/A |
| dead (refuses) | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A |
| surfaces list | A/A-/C+/A- | A/A-/C+/A- | F/—/A/— | F/—/A/— | B/A-/C+/A- |
| boundaries list/summary | C/F/C/B | C/F/C/B | C/F/C/B | —/—/A/— | C/F/C/B |
| resource list | D/A/A/A | D/A/A/A | D/A/A/A | D/A/A/A | D/A/A/A |
| docs list | B/B/D/A | B/B/D/A | B/B/D/A | B/B/C/A | B/B/D/A |
| inferences list | B+/A-/B-/A | B/A-/B/A | —/—/A/— | —/—/A/— | A/A-/B/A |
| deps list | B+/A-/B/A | C/A-/A/A | —/—/A/— | B+/A-/C+/A | B+/A-/A/A |
| map --dry-run | C/A/A/D | C/A/A/D | C/A/A/D | C/A/A/D | C/A/A/D |
| doctor | B/A-/C+/C | B/A-/C+/C | B/A-/C+/C | B/A-/C+/C | B/A-/C+/C |
| gate/assess/violations | F/—/A/A | F/—/A/A | F/—/A/A | F/—/A/A | F/—/A/A |
| find exact-symbol | A-/B+/A-/A- | B+/B+/A-/A- | B/B+/C/A- | B/B+/A-/A- | B+/B+/A-/A- |
| find concept-seeds | C+/B+/A-/A- | B/B+/A-/A- | B+/B+/A-/A- | B/B+/A-/A- | B/B+/A-/A- |
| find --text | A/A/A/A | A/A/A/A | A/A/A/A | A/A/A/A | A/A/A/A |
| explain | D/C/D/A | D/C/D/A | D/C/D/A | D/C/D/A | D/C/D/A |

find vs zg baseline (zg B−/B/C−/B−; v0.16.0 C−/D+/B+/C+): v0.17.0 **B−/B+/B/A−**.

## Defects (ground-truth verified; searches stated in the grader reports)

- **D1 CRITICAL** `modules list` "No cross-module dependencies detected / all imports are intra-module" on 18/28 repos incl. repo-graph (59 crates) — `trust` on the same snapshot lists 2,821 internal references + all 8 modules zero-connectivity-suspicious; hadoop 1,173 hdfs→common Configuration imports.
- **D2 CRITICAL** django `deps` "no static import: asgiref" — 42 static imports; the headline's 419 unattributed references could contain it.
- **D3 HIGH** C/C++ macro-wrapped function names erased (26 rows; hadoop #3 center "URI_FUNC (cx 102)" = ToStringEngine).
- **D4 HIGH** C++ forward declarations rendered as definitions and outranking them (vcmi CGHeroInstance 7/8 visible rows are `class CGHeroInstance;`).
- **D5 HIGH** three irreconcilable file totals per snapshot (grpc-java 1917 vs 1627; Σ modules-owned > stats in 5 repos).
- D6 surfaces "0 project surfaces" beside 235 real routes; test rows first. D7 zvec `--full` marker false. D8 `explain ServiceDispatcher` bare zeros (146 refs; construction at daemon-runtime/src/lib.rs:343, :404). D9 test-fixture App.java → repo-level Spring inference, propagates into `dead`'s cause. D10 Java `cycles` bare no-cycles where trust says LOW. D11 FRAKTAG seeds return decls, not the write path. D12 complexity headline = symbol cx under a file label. D13 docs kinds wrong. D14 boundaries 0/772 anchors. D15 check PASS at 27% vs FAIL at 24%.
- Minors: `dead` exit 2 + `error:` on a correct refusal (machine-consumer trust); LiveGraph Resident: no on 28/28; denominator drift 339374/339377; churn "(last 90 days)" titles with unavailable window; doctor green beside "vector store: unavailable"; raw enums as UI; find ranked block not score-ordered after decl pin; duckdb `dead` 379 s / zap-engine docs 1001 s under batch.

## Ranked fix queue (codex-consolidated)

1. **CLAIM-INVARIANT-1** — no absence/exclusivity/completeness/confidence claim rendered where a same-snapshot signal (LOW import graph, zero-connectivity suspicion, unattributed bucket, ceiling-relative verdict, lexical references present) disconfirms it; render as a resolution gap with cause. Sites: modules list (D1), cycles (D10), explain zeros (D8), deps per-package negatives (D2), check verdict token (D15).
2. **MODULE-EDGE-COHERENCE-1 + JAVA-MODULE-ATTRIBUTION-1** — consume the internal-reference evidence trust already holds (the repo-graph self-contradiction is not a Java bug); then package-root → Maven/Gradle module mapping.
3. **DEPS-NEGATIVE-1** — Python `from pkg.mod import` resolution, misattributing hedge, django's dropped package.json ecosystem, negative gate on unattributed buckets everywhere (storybook 111/124).
4. **CPP-FACTS-2** — macro-wrapped functions (D3); forward decls as `(decl)` in Facts, definition first (D4).
5. **TOTALS-1** — D5, D12, D6, D7, D9, COH-2 orient drop, (N test) legend.
6. **ECONOMY-3** — list budgets (repeated-row collapse, callers not-found cursor, model string, generated files out of complexity, not-found score floor, --full lines).
7. Machine-consumer: `dead` exit code; smoke manifest linux entry (harness).
8. Staged: DAEMON-RESIDUALS-2 (retention), DAEMON-RESIDUALS-3 (seed chunking, #2b, VACUUM guard, `OpKind::Seed`).

## Keep and imitate

`dead`'s refusal (fix only the exit code) · `gate` "pass (vacuous — nothing was evaluated)" ·
`inferences list` three-way cause discrimination with line-exact anchors · django `boundaries`
zero-state (detector inventory + specific gap — what `cycles` should print) · `map --dry-run` cap
discipline (template for TOTALS-1) · `deps` disclaimer + `resource list` caveat block (wording
right, gate missing) · `trust` basis/provenance + conservative denominators · named-holder Busy ·
`find --text` enclosing-symbol annotation · glamCRM `surfaces list` (235 routes with reasons and the
"also provided by serverless (dual implementation)" cross-store finding).

## Infra

D1-A visible in the field: both batch bounces named the holder with its age; no 301 s hangs.
Contention unchanged (379 s / 1001 s under batch). Harness dropped linux silently. `rust/target`
22→34 GB after the warm-up; free 128 GB.
