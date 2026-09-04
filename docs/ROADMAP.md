# Roadmap

**Forward-only.** Completed work lives in git history, `docs/shipped/slices/`, and
the per-slice docs. **This file is the priority index; the _context_ for each item
lives in `docs/TECH-DEBT.md`** (limitations + findings) and the linked slice/design
docs. Direction and horizons: `docs/VISION.md`. Substrate-arc ledger: `CURRENT_SLICE.md`.

> The prior, history-laden roadmap is preserved in git (the commit before this
> rewrite). Nothing was lost — the past was retired, not deleted.

## How we got here — `arch/scip-substrate-pivot` (v0.3.0)

The branch began as one infrastructure question: _pivot L0/L1 facts to SCIP and
retire the homegrown raw `nodes`/`edges` SQLite substrate._ The answer was a
**principled boundary** — SCIP carries no unresolved-call disposition
(`SCIP-UNRESOLVED-CALL-PROBE-1`, NO-GO), so the trust unresolved-call fields have
**no current-state source: RED by design.** A full decommission is impossible;
Option A (bounded partial) was ratified.

That single discipline — _labeling what the substrate cannot source_ — generalized
from substrate to surface and became the branch's real achievement: **repo-graph's
honesty reckoning.** `orient` was rebuilt to be densely honest (budget = depth,
reliability caveats), output words were audited against ground truth
(`dead` → `unref?`), the daemon's serial reality was caught, and the discipline was
institutionalized as the **End-to-End Usefulness Protocol**
(`docs/testing/end-to-end-usefulness-protocol.md`). The lesson that shapes this
roadmap: _a green test suite and a passing smoke coexist with a surface that lies._
So the next track centers on the surface the agent actually consumes.

## Strategic center (unchanged)

Legacy-code **relationship discovery an agent can trust**: seams, boundaries,
module/ownership structure, state/resource touchpoints, policy-propagation paths,
testability and migration relationships. Multi-language extraction feeds this
substrate; it is not a set of unrelated per-language extractors. See `docs/VISION.md`.

---

## NOW — post-v0.7.0 (reconciled 2026-07-16)

Three releases shipped since this file's last major update, all field-report- or
vision-driven (delivery records live in each `docs/slices/*.md`):

- **v0.5.0** — installer truth (INSTALL-ROBUSTNESS-2), detached index completion
  (INDEX-DISCONNECT-1), daemon visibility (DAEMON-VISIBILITY-1), auto snapshot retention
  (SNAPSHOT-RETENTION-1), Rust metrics + coverage honesty (METRIC-LANG-COVERAGE-1).
- **v0.6.0** — postpass recursion fix at kernel scale (PERSIST-RECURSION-1), crash
  reconciliation (DAEMON-CRASH-RECOVERY-1), auto-enrichment (ENRICH-LIFECYCLE-1), quiet
  CLI (INDEX-QUIET-1), rgistr header strip.
- **v0.7.0** — module model at monorepo scale (MODULE-MODEL-2 + CARGO-WORKSPACE-
  INHERITANCE-1), scanner honesty — no silently dropped source (SCANNER-GITIGNORE-1),
  the ENRICH-YIELD arc (funnel instrumentation → ratified levers → Rust receiver
  locator), the reader's coverage map (RELIABILITY-REFRAME-1), named attribution
  (ATTRIBUTION-1), deterministic maps — LLM out of the map path (MAP-FROM-INDEX-1).

**The current gate is FIELD VALIDATION:** v0.7.0 on the second machine against the
160k-file polyglot monorepo (the deployment target), scored with the two-agent
usefulness protocol against the operator's independent indexer. Priorities below are
hypotheses until that run speaks.

**The active work queue (operator-ratified 2026-07-16):**

1. ~~TS-PROTOTYPE-RETIREMENT-1~~ **SHIPPED** (800d78e — tree verified clean of src/;
   this block was stale from 2026-07-16 until 2026-09-02).
2. ~~ENGINE-CONSOLIDATION-1 (SPEC)~~ **RATIFIED 2026-07-16** (spec doc §8: six D-EC
   decisions as written; D-EC-1/7 superseded by reconciliation-over-adjudication;
   RECON-DESIGN-1 §8 ratified 2026-07-17). Remaining: the M-1..M-6 milestone EXECUTION
   (status audit vs shipped code opened 2026-09-03 — this block was stale since July).
3. ~~DAEMON-CONCURRENCY-1 — serial → concurrent~~ **SHIPPED** (10493e8 2026-06-24,
   thread-per-connection + 64-conn cap + typed Busy; DELIVERED 2026-07-17 per
   docs/slices/daemon-concurrency-1.md; TECH-DEBT #1 closed 2026-07-17 — this block was
   stale until 2026-09-04). REMAINING concurrency residuals (see 2026-09-04 audit +
   survey): TECH-DEBT #2b (LiveGraph preload/refresh bypass the repo coordinator);
   seed-publish write window vs foreground open patience (django Busy bounce, measured);
   assess 301s hang under batch load (UNDIAGNOSED — not write-mutex contention, different
   DB); retention's VACUUM journal_mode=DELETE window. Re-scoped as DAEMON-RESIDUALS-1
   — human-ratified Option A (2026-09-04): diagnose the assess hang under reproduced
   load first, then fix the three known residuals under the frozen invariants
   (docs/slices/daemon-residuals-1.md). Queue slot #7.
4. **Scale chain** — POSTPASS-PROFILE-1 → delta-indexing completion → sharding
   (monorepo reindex cost).
5. Housekeeping: PROTOCOL-HELP-TRUTH-1 (#7+#9), F6 (`rmap check` F2 residual),
   F14 (extractor deep-file guard), GRADLE-DEP-READER-1 (Java attribution prereq),
   ENRICH-YIELD residuals (dependency-version capture; caller-level version resolution).

## Usefulness audit v0.9.0 — the fix queue — COMPLETE (2026-08-30)

All ten shipped: #1-5 pre-queue (SELF-POLLUTION-1, DEPS-LIST-REWRITE-1, HTTP-SURFACE-COHERENCE-1,
INFERENCES-SURFACE-1, ORIENT-SEGMENT-2), #6 CONTRADICTION-SWEEP-1 b66391c, #7 CYCLE-HONESTY-1
4574fd0 (CYCLE-FACTS-2 follow-up awaits human ratification), #8 DEAD-CAUSES-1 e909190,
#9 GOV-ARMED-1 be7fa5b, #10 QUANT-MECH-1 1fe015a. (2026-08-26)

Per-command QUALITY+QUANTITY audit across six repos (protocol run
`smoke-runs/2026-08-25T22-41-37Z`; full report `docs/audits/2026-08-26-per-command-usefulness-v0.9.0.md`,
local). Verdict: facts mostly true and honestly labeled where the honesty machinery exists
(trust/check/doctor/orient); failures cluster in dumps-not-presentations (4 commands ≈ 90% of all
output lines), detectors blaming the codebase for their own absence, SELF-POLLUTION (rmap's own
`*_MAP.md` counted by drift/docs/corpus — django check goes INCOMPLETE on 3,721 of its own
sidecars), and cross-command contradictions. Ranked queue (operator, pending human reorder):

1. **SELF-POLLUTION-1** — exclude self-generated artifacts from INDEX_DRIFT/docs/corpus; K=0
   drift severity → Pass. 2. **DEPS-LIST-REWRITE-1** — specifier-only undeclared bucket, manifest
   by dominant language, loud unattributed failure, ≤20-line table. 3. **HTTP-SURFACE-COHERENCE-1**
   — Spring `@Controller` + Next.js App Router providers, footer/headline contradictions, merge
   `boundaries list` into `surfaces list`, dual-implementation annotation. 4. **INFERENCES-SURFACE-1**
   — grouped default, `--limit`+truncated flag (silent 752 cap = honesty violation). 5.
   **ORIENT-SEGMENT-2** — promote directory groups when package topology collapses (django `.`);
   budget never changes facts; `--full` false promise; REST count in headline; no `.env` in Docs.
   6. **CONTRADICTION-SWEEP-1** — doctor-vs-check, UNPARSED-vs-map, trust-vs-stats, enrich-CTA on
   Python. 7. cycles fake paths / type-only / test-only. 8. dead's static causes. 9. governance
   quartet collapse. 10. churn sort + budgets, stats de-dup. Seeding corpus levers (boilerplate
   skip, symbol names, config/test exclusion) fold into the symbol-granularity milestone.

## Diverse-codebase verification — OpenXcom + VCMI (2026-09-01)

Operator-directed spot audit on two game engines (smoke `2026-09-01T15-01-04Z`, --retain;
grading agents with tree-verification). VERDICT: rmap genuinely orients on both — OpenXcom's
structure story verifies true edge-by-edge (its strongest legacy-C++ honesty showing); VCMI's
16 inferred modules ARE the real top level, every spot-checked cycle arrow is a real #include
(including a true lib→AI seam violation via AIFactory.cpp), and the C++ ceiling posture holds
at 2000-file scale. The seams-and-history rim does not: the client↔server↔lib DEPENDENCY
STORY is never told directly (boundaries returns zero; "14 cross-module dependencies" is a
bare count; the story survives only in stats' fan_in/I numbers).

NEW findings (merge into the active queue):
- **SMOKE-MAP-WRITE — FIXED same day (be5459d):** bare `map` in the smoke wrote 79,325 MAP.md
  files across the corpus incl. FRAKTAG's tracked ones (restored). Script now uses --dry-run.
- **TS-LINGUIST-1** (high): Qt Linguist `.ts` (XML, `<!DOCTYPE TS>`) classified as TypeScript
  — cascades into churn's whole top-25, dead/inferences applicability, deps, stats. Content
  sniffing, not extension.
- **CHURN-SHALLOW-1** (high): depth-1 clone → churn asserts "2072 files changed (90 days)"
  from one commit; hotspots ranks on it. The degenerate-history case MISLEADS (vs the known
  zero-state hedge which under-informs): single-commit/shallow history must be stated.
- **MODULE-EDGES-1** (the acid-test gap): "N cross-module dependencies detected" renders as a
  bare count — no default surface shows client→lib/server→lib. The VISION's "where the
  boundaries and seams are" needs the edge list rendered (modules deps rollup or boundaries).
- **RESOURCE-CPP-INERT-1**: resource coverage line claims C/C++ while the detector sees only
  fopen-style calls — std::ofstream/ifstream invisible → "0 reads" on file-driven engines
  (OpenXcom saves, VCMI lib/filesystem). Narrow the claim or teach the detector streams.
- **TRUST-CEILING-1**: trust renders "below 50% target" with no ceiling statement where check
  says the ceiling is reached; also "C" vs "C/C++" naming drift on ALL orient tiers + stats.
- Minors: trust edges-100% vs 944-unresolved-imports juxtaposition; stats I/D/A prints
  confident nonsense on C++ (Menu A=1.00); docs kinds (CMakeLists as architecture, CHANGELOG
  missed); three unreconciled file totals (2072/2069/2065); vendored code blends into
  complexity centers.

## Usefulness audit v0.15.0 — round three (2026-09-02)

All six tail fixes field-verified (CHECK-LANG-SPLIT's one grader FAIL refuted by the
materiality-gate measurement: repo-graph TS+JS 5.3% < 10%); nothing regressed; honesty
architecture holds under vcmi/OpenXcom stress. Artifact + `docs/audits/2026-09-02-…v0.15.0.md`.
New queue: 1. ORIENT-CYCLES-DISAGREE (verified) 2. RESOURCE-RECALL-1 (zero-states cite the
arg0 literal-path gate) 3. FIND-KIND-MISLABEL (C++ kinds) 4. MODULE-VOCAB-DRIFT 5.
NODE-BUILTIN-AS-UNDECLARED 6. SURFACES-HEADLINE-BLEND + minors (TRUST-ROUND-1,
DEAD-FIXTURE-WEIGHT, skip-line stacking) + carried protocol (firing boundary, risk coverage
import, linux timeout, capture-after-settle).

## Queue tail COMPLETE (2026-09-02) — the v0.13.0 + diverse queues fully cleared

ZEROSTATE-SCOPE-1 62995e6+4e3eb2c (per-repo gap sentences; any-TS cycle caveat; + the
committed-past-red-gate repair) · CHECK-LANG-SPLIT-1 6ed99ec (per-language confidence under
the blended figure) · COHERENCE-POLISH-1 3d2d6c9 (funnel gate-anchored reasons; trust
ceiling echo — 9th two-computations kill; gate arming scope) · FINAL-POLISH-1 d9ff6b7
(surfaces ×N dedup; deps self-import first-party; resource claim measured true — BOTH
premises refuted: fstream delivered for literal paths only; RESOURCE-DYNAMIC-PATH-1 is the
re-scoped follow-up: the extractor's arg0 string-literal gate drops every computed path,
cross-language) · IS-TEST-RUST-1 ba68535 (cfg(test) inclusion-chain fact; find/demotion/
counts heal).

CYCLE-FACTS-2 RESOLVED (human ruling 2026-09-03, re-presented in product-impact terms per
the 2026-09-02 obligation, after ENGINE-CONSOLIDATION-1 was confirmed ratified+shipped):
the trio is UNBUNDLED. Part (c) — type-only import extraction with per-cycle
"type-only (vanishes at runtime)" labels — is RATIFIED as its own slice,
**TYPE-ONLY-IMPORTS-1** (spec: docs/slices/type-only-imports-1.md). Parts (a)
edge-set certification on the LiveGraph cycle route and (b) is_test in the LiveGraph IR
are OPTIONAL-UNSCHEDULED (like M-5 was): honest asymmetry notes stay; revivable if M-R2
union-flip work ever opens those seams. Rationale: (c) carries the agent-visible value
(runtime coupling vs compile-time phantom) with zero certification-surface risk; (a)+(b)
buy route symmetry in transient residency windows at the highest-risk machinery.
SHIPPED: TYPE-ONLY-IMPORTS-1 bd12f0a (2026-09-03, 2 relay cycles + 1 operator ruling —
write-port ratified, Unknown-reason honesty enforced end-to-end).
v0.15.0 queue: RESOURCE-RECALL-1 68a8b08 (zero-state + coverage header cite the arg0
literal-path gate) · FIND-KIND-MISLABEL-1 5387c6d (root cause: .h routed to the C
extractor, error-recovered C++ classes stamped FUNCTION; content-evidenced .h promotion
per TS-LINGUIST-1 mechanism; one-reindex key transition, churn measured).

## zg (zvec-grep) head-to-head — 2026-09-03 (docs/audits/2026-09-03-zg-vs-rmap-find.md)

Human-requested comparison vs Qwen's zvec-grep 0.2.1, 12 ground-truthed tasks × 3 repos.
rmap wins exact-symbol 3–0 and honesty (B+ vs C-); zg wins literal/regex 3–0 (absent
capability) and evidence anchors (line spans, enclosing-symbol annotation). Both fail
dispatch/algorithm concept questions. HONESTY DEFECT FOUND IN FIND: "may not have a
distinct home in this repo" rendered where the true cause is capability absence (3/3
literal probes, all false). Proposed queue (awaiting scheduling): FIND-GREP-1 (ratified
grep direction + false-sentence retirement + enclosing-symbol annotation), FIND-ANCHOR-1
(line numbers + snippet on fact rows), FIND-ECONOMY-1 (cursor boilerplate 39–52% of
bytes), SEED-CHUNK-1 (candidate: sub-file embedding granularity).
SEED-CHUNK-SPIKE-1 MEASURED (2026-09-03, docs/audits/2026-09-03-seed-chunk-spike-1.md):
chunk granularity fixes the dilution class (4/4 queries absent→top-3-or-better);
potion-code-16M-v2 static (model2vec-rs) wins 3/4 vs nomic at equal granularity, <1min
vs ~20min, offline; is_test demotion compounds (obsolete-files → rank 1) — facts×seeds
is the differentiator. FACT GAP: is_test=0 for all C++ (IS-TEST-CPP-1 candidate, gtest
structural basis). SEED-CHUNK-1 RATIFIED Option 1 (human 2026-09-03: full swap — model2vec-rs +
potion-code-16M-v2 in-process, per-symbol chunks from the nodes table, is_test-partitioned
seeds, lmstudio retired from find), sequenced AFTER FIND-EVIDENCE-1 and FIND-GREP-1;
IS-TEST-CPP-1 slotted before it (small; demotion consumer measured). Then release+audit.
SHIPPED: FIND-EVIDENCE-1 42f1b8f (2026-09-03: line anchors + stored evidence line per
fact row, cursor diet −12.3% boilerplate, syntax-gated short-cursor alias with the
nodes-free green path preserved; economy clause amended to signal-per-byte — operator
drafting defect owned; survived an Anthropic 529 incident + codex cache corruption +
an OpenAI Codex backend incident across its three relay runs).
NEW DEFECT (verified 2026-09-03, via FIND-GREP-1 review): **CPP-SPAN-FIDELITY-1** —
leveldb util/env_posix.cc: `class Limiter` stored span 73–806 swallows most of the file;
the four Posix*File classes + all methods (~130–800) UNEXTRACTED (namespace{} wrapper
suspected; diagnosis in the slice-to-be). Every C++ fact consumer inherits this
(explains part of the C++ audit weakness). Queue candidate after SEED-CHUNK-1.

## Usefulness audit v0.16.0 — round four (2026-09-04)

docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md · artifact
581da1fc-faa1-4972-ae77-2d27579b0493 · three-perspective gate (matrix grader + judge +
codex standalone). VERDICT: more useful (all four zg F's dead — --text exact parity;
anchors everywhere; spike seed ranks reproduced, lmstudio retired) BUT the trust contract
regressed: a NEW FABRICATION CLASS (verified: vscode .env resource, hadoop phantom
writers, nextjs-on-React-Router, invented map edges, deps "unused" false positives incl.
django asgiref vs 31 import sites). "v0.15.0 failed by silence (safe); v0.16.0 fails by
confident invention (indefensible)." Trust repair dominates the next round.
Shipped-fix verdicts: --text FIXED-recall/BROKEN-C++-evidence · anchors PARTIAL · false
sentence FIXED · seeds PARTIAL (is_test partition inert on Rust; decl-chunk dominance) ·
resource caveat FIXED-disclosure/NEW-fabrication · .h routing REGRESSED (macro-name
erasure 770/770 vcmi) · type-only NEAR-DORMANT (ALL-edge predicate; should be ANY).
QUEUE RATIFIED (human 2026-09-04, with two additions): 1 HONESTY-GATE-1 (deps: the
destructive-edit family) · 2 HONESTY-GATE-2 (resources/trust/map fabrications; same
invariant) · 3 CPP-SPAN-FIDELITY-1 (macro classes + span containment) · 4
ANCHORS-EVERYWHERE-1 (human addition: line numbers on every surface citing symbols/files,
not only find — sequenced AFTER span fidelity so C++ anchors land on correct spans) · 5
SEED-CHUNK-2 · 6 COHERENCE-2 · 7 DAEMON-CONCURRENCY-1 (human addition; SPEC slice first —
foundational, decisions to ratify) · 8 ECONOMY-2 · carried minors.
Infra: serial-daemon contention MEASURED under batch (301s assess hang; Busy bounce; the
chunk seed pass is a new long writer) — DAEMON-CONCURRENCY-1 price rising.

Open follow-ups awaiting ratification/scheduling: RESOURCE-DYNAMIC-PATH-1, docs
residual-bucket taxonomy, ARMED-POSITIVE firing-boundary smoke protocol, linux per-repo
timeout override.

## v0.13.0 + diverse-verification merged queue — high tier COMPLETE (2026-09-02)

Shipped: ORIENT-SMALL-ENRICH-1 c56d298 (in-flight promise gates on per-repo applicability;
C/C++ naming unified) · TS-LINGUIST-1 b716f77 (content-sniffed .ts; Qt Linguist never
TypeScript; churn keeps real translation activity by ruling) · flake family root-caused +
closed test-side 5535092 (retention pass vs zero-patience opens) · FOREGROUND-LOCK-1 795f82d
(half-second open patience; Busy + holder-named message; 20 raced calls clean; flaky binaries
0/10) · CHURN-SHALLOW-1 f7c57aa (shallow/single-commit history stated; the either/or hedge
dead) · MODULE-EDGES-1 2db7335 (the acid test: cross-module edges rendered on modules-list +
orient line 2; one-read count — exposed the old count as wrong, 14 vs 29 true).

Remaining tail: ZEROSTATE-SCOPE-1, FUNNEL-VOCAB-1, CHECK-LANG-SPLIT-1, SURFACES-DEDUP-1,
IS-TEST-RUST-1 (VERIFIED 2026-09-02: src/**/tests.rs & *_tests/ carry is_test=0 — an INDEXER gap; honest basis is the #[cfg(test)] inclusion chain, never the filename; supersedes FIND-TEST-RANK-2's ranking premise), DEPS-SELF-1,
RESOURCE-CPP-INERT-1, TRUST-CEILING-1, governance polish, docs residual-bucket taxonomy.

## Usefulness audit v0.13.0 — the fix queue (2026-09-01)

Post-release audit of v0.13.0 (all nine v0.11.0 fixes field-verified, no regressions; matrix
mostly A-band; zero new zero-collapse instances — the class is dead; Codex second opinion
adopted: remaining defects are narrower but several still honesty-class). Artifact + local
doc `docs/audits/2026-09-01-per-command-usefulness-v0.13.0.md`. Ranked queue: 1.
**ORIENT-SMALL-ENRICH-1** (small tier renders daemon-global in-flight on no-path repos).
2. **FOREGROUND-LOCK-1** (foreground open lock patience + honest lock message). 3.
**ZEROSTATE-SCOPE-1** (per-repo-scoped detector rosters; boundaries zero-state; any-TS cycle
caveat). 4. **FUNNEL-VOCAB-1**. 5. **CHURN-DIAGNOSE-1**. 6. **CHECK-LANG-SPLIT-1**. 7.
**SURFACES-DEDUP-1**. 8. **FIND-TEST-RANK-2** (verify the is_test fact on in-crate test
modules first). 9. **DEPS-SELF-1**. 10. governance polish + protocol items (post-enrichment
re-capture; firing-boundary arm; linux timeout override).

## Usefulness audit v0.11.0 — the fix queue (2026-08-31)

**Top tier COMPLETE (2026-08-31/09-01):** #1 ORIENT-FACT-COHERENCE-1 5b0a6b2 (re-scoped:
temporal race, in-flight rendering) · #2 DEPS-ATTRIB-2 a79fc19 · ENRICH-ROOT-1 2817ef6
(field P1: cwd-independent enrichment; + launchd PATH 744141d; + busy-retry passes) ·
#3 FIXTURE-POLLUTION-1 8e3445b · #4 CHECK-SIGNAL-1 b1c499f · #5 FIND-RANK-1 71ace5a.
v0.12.0 released mid-queue (the two field P1s). Tail remaining: RESOURCE-HONESTY-1,
TRUST-FIRSTPARTY-1, MODULES-IDENTITY-2, DOCS-LIST-2 → then release + re-audit.

Post-release audit of v0.11.0 (all ten v0.9.0 fixes verified live in the field; grade matrix
artifact + `docs/audits/2026-08-31-per-command-usefulness-v0.11.0.md`; standalone Codex second
opinion concurring). Ranked queue: 1. **ORIENT-FACT-COHERENCE-1** (budgeted orient serves
pre-enrichment cached facts + stale CTA — one snapshot fact source, freshness-labeled).
2. ~~INFERENCE-CAP-1~~ **REFUTED 2026-08-31**: glamCRM=amodx=752 is coincidence — same-run
storybook holds 1902 and zap-squad 757 (no cap; write path code-audited unbounded); the
"complete (true total)" label is TRUE. Audit-of-the-audit lesson recorded. 3. **DEPS-ATTRIB-2** (glamCRM zero content, false
manifest diagnosis, silent Gradle/Maven absence). 4. **FIXTURE-POLLUTION-1** (own test fixtures
dominate boundaries/cycles/docs). 5. **CHECK-SIGNAL-1** (always-LOW verdict; no-path language
blamed). 6. **FIND-RANK-1** (kind-weighted non-test-first ranking under the cap). 7-10:
RESOURCE-HONESTY-1, TRUST-FIRSTPARTY-1, MODULES-IDENTITY-2, DOCS-LIST-2. Protocol gaps: arm one
boundary next smoke; capture explain + amodx find; linux client-timeout override.

## Semantic seeding — ratified track (2026-08-24)

Operator-ratified (VISION § Semantic Seeding; spike evidence
`docs/spikes/2026-08-23-embed-seed-spike-1.md`). Queue: **EMBED-SEED-1** (SPEC via relay,
decision-review) → IMPL. → **FIND-FACTS-1** (human-ratified 2026-08-30: facts tier
from the fact tables renders ABOVE demoted embedding seeds; `docs/slices/find-facts-1.md`). Layer-3, opt-in verb, local model, pinned vectors, fixed-formula
ranking — the bounds are in the VISION section and are not re-litigated per slice.

## Product-surface honesty — track ledger (largely DELIVERED)

The branch proved the gaps are on the agent-facing surface and that they pass CI.
This track closes them. Context for every item: `docs/TECH-DEBT.md`
§ _Pre-Merge Hardening + E2E Usefulness Findings_ (numbered #1–#9).

**SHIPPED (2026-07-01, since v0.3.1) — the honesty core.** A second E2E smoke (nginx) + the two-agent
usefulness gate drove the ratified **HONEST-DEGRADATION-1** contract (D1-D5): `stats` now renders
unknowns as `unknown`/`null` not known-zero + one canonical symbol count (**#5/#6/C1/C4**); `deps` no
longer mislabels a C repo `npm` (**C2**); `orient`'s footer is a scoped `Serving:` not a global "exact"
(**C3**); every posture-bearing surface carries a toolchain-aware honest next-action; and `orient` gained
a progressive budget ladder (**C5**), the smoke harness stopped miscounting verdict exits (**C7**). See
`docs/slices/honest-degradation-1.md` §12 + TECH-DEBT § _Checkpoint Smoke … Resolution_. **Still open
below:** orient under-segmentation (#3/#4), the reliability reframe, and capability (enrichment /
manifest readers) — no longer honesty, but depth.

**Focus corrections — fresh-eyes review (2026-07-02, v0.4.0).** The v0.4.0 review (VISION distilled —
speculative directions moved to `docs/FUTURE-ITERATIONS.md`; governance surface frozen) + a
self-dogfood (rmap on repo-graph) added three slices. Context: TECH-DEBT § _Fresh-Eyes Review_ (F1–F4).

- **METRIC-LANG-COVERAGE-1** (P1 — honesty class) — only the Rust extractor lacked
  complexity emission (premise corrected 2026-07-02: Java/Python already emit); on a Rust
  repo `orient`/`hotspots`/`metrics` ranked ONLY the legacy TS and said nothing. Fix:
  data-driven per-language measurement-coverage caveat (general honesty infrastructure) +
  Rust cyclomatic emission. `docs/slices/metric-lang-coverage-1.md`. _(F2)_
- **TS-PROTOTYPE-RETIREMENT-1** (P1 — focus) — bury the ~90k-LOC TS prototype
  (`src/`, `test/`, `parity-fixtures/`): it dominates every self-index signal
  (all complexity centers/hotspots, 4/6 cycles). Verify-then-delete; git history is the
  archive. `docs/slices/ts-prototype-retirement-1.md`. _(F4)_
- **ENGINE-CONSOLIDATION-1** (P2 — SPEC) — name the end-state for the two coexisting
  read engines (SQLite pipeline vs LiveGraph stack): read-path inventory → fact-class
  ownership (honoring the RED floor) → checkable "consolidated" definition + milestones →
  DECISION_REQUIRED list for ratification. `docs/slices/engine-consolidation-1.md`.

**Field findings — first real install on a second Mac (2026-07-02).** A fresh install +
first index of a 160k-file repo surfaced four truth/visibility failures on the
distribution surface, sliced as:

- **INSTALL-ROBUSTNESS-2** (P1 — installer truth) — version resolution dies on the
  GitHub API rate limit (403; resolve via `github.com` redirect + `GITHUB_TOKEN`
  fallback), and the daemon-start retry loop reports "failed" while the daemon is
  actually running (socket liveness must be the predicate).
  `docs/slices/install-robustness-2.md`.
- **INDEX-DISCONNECT-1** (P0 — HOTFIX, gates the next release; ratified: detached completion)
  — the client's 300s timeout ABORTS the in-flight index (progress-emit failure returns
  `ControlFlow::Break`): hours of work die on a broken pipe, snapshots stay `building`
  forever, registration is never persisted (TECH-DEBT F5, root-caused from the second
  machine's daemon.log). Fix: best-effort emission, up-front registration persistence, no
  `building` limbo, explicit cancel unchanged. `docs/slices/index-disconnect-1.md`.
- **DAEMON-VISIBILITY-1** (P1 — long-op honesty) — `rmap index` reported a live 160k-file
  index as "timed out after 300s" (it completed ~10 min later); `doctor` showed no
  in-progress operation, no enrichment status, and reported the daemon-held database as
  "error opening database". Day-2 (2026-07-03): a 4 GB **non-READY** snapshot invisible
  everywhere while `orient` says "index the repo first" — snapshot STATE + last-index
  OUTCOME become first-class facts on doctor/repo-info/orient errors, prunable when
  partial. Exposure of coordinator state + honest timeout behavior + doctor contention
  truth. `docs/slices/daemon-visibility-1.md`.
- **SNAPSHOT-RETENTION-1** (P1 — RATIFIED 2026-07-04: current-state only; "git has
  history — I want DISCOVERY") — the retention model is shipped but never runs; every
  index adds multi-GB snapshots forever. Auto background retention pass after every
  successful write op: keep current + delta-parent (+ user-marked baselines), prune the
  rest incl. auto-baselines, threshold-gated VACUUM, doctor-visible, reclaim reported.
  Steady state ≤2 snapshots/repo. Queued after INDEX-DISCONNECT-1 (small, fast-converge),
  before ENRICH-LIFECYCLE-1. `docs/slices/snapshot-retention-1.md`.
- **ENRICH-YIELD-1** (P2 — follow-up filed 2026-07-07 at ENRICH-LIFECYCLE-1 delivery) —
  the auto-pass resolves 74% of unknown receiver types but promotion passes only ~3.5% of
  resolutions (261/7435 on self-index; +0.3pp call resolution). Investigate promotion
  criteria (confidence/ambiguity) + whether resolved-but-unpromoted types can land as
  Layer-2 inferences with basis, per the certainty model. Call-graph reliability stays
  LOW until yield improves.
- **DAEMON-CRASH-RECOVERY-1** (P0 — gates the release after v0.5.0) — a daemon crash
  mid-index leaves wreckage no shipped tool can see or reclaim (F7-F12: 3 orphaned
  `building` snapshots/11 GB invisible to every retention class; prune says "no prunable";
  orient's bare error bypasses F2; the daemon log is mute on operations). Startup
  reconciliation + op lifecycle in the LOG + F2-bypass audit + stats naming + lock-race
  probe case. `docs/slices/daemon-crash-recovery-1.md`.
- **ENRICH-LIFECYCLE-1** (P1 — RATIFIED 2026-07-04: auto-enrich after every index/refresh,
  toolchain-aware honest skips, opt-out; queued next after INDEX-DISCONNECT-1; headlines
  the release AFTER the field-fix release) — enrichment is the largest available
  resolution win (~81% of unknown receivers; self-index sits at 21% call resolution
  without it) yet never runs: opt-in AND invocable only via the positional
  `<db_path> <repo_uid>` identifiers REG-1 deliberately hides. Auto background pass as a
  normal write op (activity-stamped, cancellable, detached-completion), registry-resolved
  manual form, full lifecycle truth on doctor. `docs/slices/enrich-lifecycle-1.md`.
  Interim: `rmap repo info <repo> --json` exposes the identifiers.

**P1 — headline**

- **orient module under-segmentation** — `orient` reports "1 module" on deeply-nested
  layouts (spring-petclinic: 1 vs `stats`' 11); a structurally wrong model on the
  _primary_ surface. Root cause is the unmigrated **dual-path** (orient/modules read
  `module_candidates`; `stats` reads `nodes` kind='MODULE') — **not** the umbrella
  heuristic (disproven; that's a secondary manifest-less variant). Fix: name the
  package topology + unify the "module" notion. Spec'd in
  `docs/slices/module-model-1.md` (6 decisions await ratification). _(TECH-DEBT #3;
  pairs the module-model P2)_
- **daemon concurrency** — `run_socket` handled connections inline (serial; head-of-line
  blocking), contradicting the VISION's concurrent-readers daemon. Spec'd + **decision-
  reviewed + ratified** (`docs/slices/daemon-concurrency-1.md` §14). **B1 SHIPPED**
  (`DAEMON-CONCURRENCY-IMPL-1`: concurrent dispatch + state `Send+Sync` + W-A +
  `livegraph_refresh`/`preload` under the coordinator + S-A normal-open). The two-agent
  review withdrew W-B (cross-store split-brain) → deferred to **`DAEMON-W-B-EPOCH-1`**.
  _(TECH-DEBT #1/#2/#2b)_
  - **B2 (query-path cancellation, in-loop / Option A) — COMPLETE** (decomposed; the mega-slice
    blocked, too big). Shipped as **`DAEMON-CANCEL-1`** (cancel seam + `run_interruptible` fix
    [panic ≠ Cancelled] + cycles Tarjan + path BFS) → **`DAEMON-CANCEL-2`** (stats SQL
    `sqlite3_interrupt`) → **`DAEMON-CANCEL-3`** (orient/check/trust/explain: cycle Tarjan,
    complexity `FETCH_ALL`, trust `compute_module_stats` SQL + 100k sample loop). A Codex
    decision review refuted a "these paths are light" hypothesis (cited) → confirmed Option A.
    Every heavy query path now cancels mid-flight on peer-disconnect; honest large-fixture
    in-flight tests throughout.
  - **`DAEMON-W-B-EPOCH-1` — SHIPPED** (2026-06-29; decision-reviewed + ratified §14). A
    request-level `(ready_snapshot_uid, livegraph_fingerprint)` epoch captured once and threaded
    through every SQLite + LiveGraph read; whole-request join coherence proven with a real SQLite
    N+1 publish mid-request. W-B re-enabled: **read-during-refresh** (`Refreshing` admits readers
    via `RefreshingWithReaders(n)`; `Writing` still serializes). Delivered as IMPL-1 → 2A → 2B →
    2C (trust) → 2D (cycle_completeness_audit) → 3 (flip) — the "all mixed-read handlers" scope
    grew from an assumed 8 to the authoritative **10** as the per-slice pre-flip enumeration
    (re-verified by the reviewer) caught two missed handlers; §7.3 records the closed set. E-A
    (enrich shares the seam) documented in the coordinator contract; ENRICH-LIFECYCLE-1 is the
    remaining consumer. **The daemon robustness arc — serial → B1 → B2 → W-B — is COMPLETE.**
  - **`WORKTREE-SUPPORT-1`** (spec — ties to the multi-agent daemon) — does repo-graph serve
    git **worktrees** correctly? Agents increasingly work in worktrees (parallel/isolated
    work; the relay's own worktree isolation), and B1 just made the daemon concurrent for
    exactly that multi-agent case. Open questions the spec must resolve against code: does the
    daemon registry key a repo by **canonicalized working-dir path** (→ each worktree is a
    distinct current-state, served correctly but with no shared-extraction reuse) or by **.git
    identity** (→ two worktrees collide on one entry and the daemon serves the wrong branch's
    state — a correctness bug)? `.rgr/` warm cache is per-working-dir (good); the global
    registry (`daemon-runtime/src/registry.rs`, `state_root/registry.json` + `databases/`) is
    the risk surface. Detect a worktree via `git rev-parse --git-common-dir`. Per the VISION
    ("git owns history, repo-graph owns current-state"), each worktree's current-state is
    distinct and must be served as such; shared-history extraction reuse is a nice-to-have.

**P2**

- **query-path cancellation** — extend the D5b abort seam to the non-progress query
  paths. _(TECH-DEBT #2; depends on daemon concurrency)_
- **"module" model unification** — `orient`/`modules` (inferred modules) and `stats`
  (directory groups) disagree on what "module" means; pick one canonical notion (or
  self-label each). _(TECH-DEBT #4; pairs orient under-segmentation)_
- **stats false-zero** — `total_symbols: 0` on rmap-indexed repos; populate it, or
  render "not measured" — never `0`. _(TECH-DEBT #5)_
- **stats reliability marker** — fan-in/out and distance-from-main-sequence carry no
  import-resolution caveat (overclaim on syntax-only C/C++); mirror `orient`'s
  reliability line. _(TECH-DEBT #6)_
- **REG-1 `--help` truth** — help is stale for the still-positional governance/write
  commands; make it reflect the actual mixed contract. _(TECH-DEBT #7)_

**P3**

- **peripheral output-words audit** — extend the OUTPUT-DOC-TRUTH-AUDIT rubric across
  the command tail. _(TECH-DEBT #9)_
- **postpass optimization** — profile the ≈50%-of-index postpass phase (the
  `RMAP_PERF` markers exist now), then scope / batch / fold into extraction.
  _(TECH-DEBT #8)_

**Dedicated slices — queued for agents to write** (relay: builder writes the spec → Codex
reviews → IMPL slice; operator triggers each). Paired findings bunched into one slice:

- `MODULE-MODEL-1` (#3 + #4) — **DELIVERED** (IMPL `17dbe93` 2026-06-23; scale +
  polyglot follow-up MODULE-MODEL-2 `170be30` 2026-07-12; workspace-inheritance
  `958a6bb`; #3/#4 closed).
- `DAEMON-CONCURRENCY-1` (#1) — **OPEN** (P1; #2 query-path cancellation already
  resolved 2026-06-26 — the remaining work is concurrent connection handling).
- `STATS-HONESTY-1` (#5 + #6) — **SHIPPED via HONEST-DEGRADATION-1** (2026-07-01:
  unknowns render as unknown, one canonical symbol count) + RELIABILITY-REFRAME-1
  (axis-framed caveats). No separate slice needed.
- `PROTOCOL-HELP-TRUTH-1` (#7 + #9) — **OPEN** (housekeeping tier).
- `POSTPASS-PROFILE-1` (#8) — **OPEN**; entry point of the scale chain.

## Resolution & attribution — resolve what we can, label what we can't (reader-context)

*Pervasive primary-surface honesty: today's "unresolved / reliability LOW" tells the agent
about repo-graph's own pipeline, not about their code — meaningless on **every** repo.
Context: `docs/TECH-DEBT.md` § Resolution (R1–R4). Labels follow the VISION's "labels speak
the reader's language" principle. Runs **parallel to** the product-surface honesty track
(no contention).*

- **Run enrichment automatically (P1)** — wire the LSP enrichment pass into the daemon as a
  **background task after index/refresh**, with **atomic snapshot hand-off** (index returns
  fast syntax; enrich upgrades the graph behind it); toolchain-aware (auto-run when present;
  reader-context message when absent — "semantic resolution unavailable; install
  rust-analyzer", not "enrichment phase did not run"). Closes the in-scope resolution gap
  with no agent babysitting. **Not blocked on DAEMON-CONCURRENCY-1** — different capability
  (autonomous background work, not concurrent client access); shares only state-safety, which
  the snapshot model makes an atomic pointer swap. _(TECH-DEBT R4)_
- **Attribute the unresolved set, in the reader's terms (P1)** — label each reference
  `library call → serde` / `stdlib → std::…` / `system call → …` / `native/DLL call` /
  `dynamic dispatch` / `(unknown — couldn't attribute)`, with provenance (which dep + version;
  resolve `#include` via include paths). The basis codes are already computed — surface them
  as reader-context labels + named attribution. _(TECH-DEBT R2)_
- **Reframe reliability as a coverage map (P1)** — stop grading ourselves ("reliability LOW /
  22% / below 50%"); show the agent **where their calls go**: "N% into external libraries
  (serde, tokio, …) — follow to their crates/docs; your own code's calls M% resolved." Exclude
  out-of-scope refs from the in-scope rate; flag only genuine in-scope failures, in their
  terms. _(TECH-DEBT R1)_

**Dedicated slices — queued for agents to write** (relay: builder writes the spec → Codex
reviews → a follow-on IMPL slice; operator triggers each):

- `ENRICH-LIFECYCLE-1` (R4) — **DELIVERED** (`1fe33b7`, v0.6.0): auto-enrichment after
  every index/refresh, batch-boundary cancellation, toolchain-honest skips. Follow-ups
  delivered: ENRICH-YIELD-1/2/3 (funnel accounting on the product surface; Layer-2
  likely-external projection; promotion-neutral primitive reattribution; enum widening;
  Rust receiver locator + safe self.field.method).
- `RELIABILITY-REFRAME-1` (R1) — **DELIVERED** (`285e62e`, 2026-07-14): one shared
  CallReliabilityView across orient/trust/check/explain; in-scope-or-unclassified
  conservative rate; named external coverage map; honest zero states. R1 CLOSED.
- `ATTRIBUTION-1` (R2) — **DELIVERED** (`adfd0cf`, 2026-07-15): "Unresolved references —
  where they go" with declared base-dependency names via the three-path storage join;
  internal vocabulary off all reader surfaces. R2 CLOSED.
- `GRADLE-DEP-READER-1` (R3) — **OPEN**; prerequisite for Java attribution (Java renders
  the honest degraded path meanwhile).

Residuals recorded in the slice docs: dependency VERSIONS are not captured anywhere
(disclosed honestly on the surface; capture is future work), caller-level workspace
version resolution (CARGO-WORKSPACE-INHERITANCE-1 §6).

## Quality trend discovery — PARKED (2026-07-11)

Snapshot-to-snapshot quality diff, "what got worse" delta surfacing, and
risk-ranking-over-time are **not** a discovery priority: the operator derives
trends by other means and will not rely on repo-graph for them. Moved with
full rationale to `docs/FUTURE-ITERATIONS.md` (§ Quality Trend Discovery);
VISION amended same date. Current-state quality signals (complexity, hotspots,
cycles — coverage-labelled) remain in scope as orientation aids.

## Candidate track — take JavaScript seriously (operator-surfaced 2026-07-20)

Today JS/JSX gets pipeline extraction (structure, calls, modules) but NO semantic
second witness: the reconciliation union is TS-only (`scip-typescript`), and there is
no JS resolver equivalent to tsserver/rust-analyzer. Real JS frontends (e.g. glamCRM's
React-in-JSX web app) therefore read lower on call resolution and stay single-witness —
correct today, but a coverage ceiling the operator wants lifted, not just tolerated.
**Scope deliberately UNSET** — to be shaped against the concrete glamCRM index evidence
(the isolated verify run after GRADLE-DEP-READER-1), not in the abstract. Candidate
axes to weigh then: whether `scip-typescript`'s JS mode (`allowJs`/`checkJs`) yields a
usable second witness on JSX; a tsserver-on-JS enrichment path; and the honest-labeling
work so JS coverage reads as a distinct, named posture rather than "low reliability."
Not ratified scope; a named direction on the queue.

## Parallel strategic bet — non-TS LiveGraph coverage

The structural ceiling of the SCIP pivot: non-TS repos (Rust / C / C++ / Java /
Python) fall back to SQLite-labelled serving. Extending per-language SCIP producers
(`scip-clang`, `scip-rust`, …) + ingest + LiveGraph is the one substrate item with
real strategic payoff. High cost; the viability spikes were GO-_with-caveats_ (Rust
per-crate dedup, C++ TU multiplication). Does **not** fix the unresolved-call gap.
_(slice docs: `scip-*-spike-1`; `CURRENT_SLICE.md`)_

## On deck — coverage-backed liveness

The highest-leverage breadth chain: import **`llvm-cov`** coverage to satisfy the
hard prerequisite for **reintroducing the withdrawn dead-code public surface**.
Dead-code from structural heuristics alone stays blocked. _(TECH-DEBT §Dead-code
surface withdrawal; Deferred §Dead-code public reintro, below)_

---

## Substrate decommission — bounded, at its floor

`SQLITE-RAW-DECOMMISSION-1` is RATIFIED as a bounded partial (Option A,
`docs/slices/sqlite-raw-decommission-1.md`). The trust unresolved-call fields are
RED-by-design → a **permanent SQLite floor**; a full
`nodes`/`edges`/`unresolved_edges` drop is impossible. PREREQ-1 (focus-resolution)
shipped + closed. Remaining work is **diminishing-returns** and deferred:

- **C1** marginal partial fastpaths — flips no deletion gate.
- **C3** bounded retirement IMPL — deferred on PREREQ-2.

Full arc ledger: `CURRENT_SLICE.md`; probe record:
`docs/slices/scip-unresolved-call-probe-1.md`.

## Demand-gated breadth

Extraction/detection breadth. The gate (the roadmap's own rule): **pursue only when
real-repo navigation proves the current surface insufficient.** Context per item in
`docs/TECH-DEBT.md` (subsystem sections) + slice/design docs.

- **C/C++ clangd enrichment** — receiver-type resolution for the strategic legacy
  C/C++ center (ties to the LLVM track below). _(TECH-DEBT §Extraction—C/C++,
  §Enrichment)_
- **Java enrichment operationalization** — jdtls reliability/determinism (hardening,
  not breadth). _(TECH-DEBT §Extraction—Java)_
- **State-boundary expansion** — queue/event boundaries, config/env seam,
  SQL-string / ORM inference; Rust blocked on extractor `ResolvedCallsite`.
  _(milestone: `rmap-state-boundaries-v1` §Deferred)_
- **Docs inventory remaining** — `explain` doc integration, persisted inventory,
  document-backed authored relationship items (anchored seams/migrations readable
  outside the DB). _(VISION value frontier #4)_
- **Rust framework detectors** (Actix/Axum/Rocket/Warp) · **policy-facts PF-4+**
  (BRANCH_OUTCOME, DEFAULT_PROVENANCE) · **multi-track boundary depth** (gRPC
  endpoint/method linking, message-broker cloud/language coverage) · **CLI boundary
  expansion** (CI/Docker/frameworks, barrel-cycle normalization) · **PY-EXT-2-PERF**
  (needs a benchmark harness) · **rgistr remaining** (e2e + CLI tests).

## Performance & scale

- **postpass optimization** (Current priority, P3) → **delta-indexing completion**
  (scoped postpasses, large-repo validation) → **sharded indexing** (Linux-scale;
  build-aware C/C++ partitioning). Architecture-grade at the sharding tier.
- **CLI progress rendering** — render the index callback to stderr (small).

## Strategic-later

- **LLVM/Clang ecosystem** — ordered: `llvm-cov` (unblocks dead-code + risk evidence)
  → `compile_commands.json` for C++ → ASan/UBSan/TSan import → clang-tidy →
  libclang/clangd enrichment.
- **Mobile/native track** — Objective-C/C++ (the C/C++ bridge-layer leverage) →
  Kotlin → Swift → Dart/Flutter. New relationship classes: lifecycle entrypoints,
  navigation, DI, persistence, FFI/interop seams.
- **Python semantic enrichment** (pyright/mypy) · **Go extractor** (gopls) · **Full
  TS semantic / Path C** (TypeChecker replacement — largest investment, only after
  the syntax-first ceiling) · **Scala** (after Java + mobile + daemon).

## Parked

Halstead metrics (don't expand the metric set without a concrete consumer) ·
entrypoint-declaration adoption (operational, not a code change) · trust-score
reweighting (recalibrate after enrichment stabilizes) · D2c field-type binding ·
tsconfig package-name `extends`.

## Platform / distribution (deferred)

Cursor integration (CURSOR-1, queued) · Windows (WIN-1) · macOS
signing/notarization (MAC-2) · updater/repair channel (UPDATE-1).

---

## Dependency notes (sequencing constraints)

- daemon concurrency → query-path cancellation
- orient under-segmentation ↔ "module" model unification (decide the notion once)
- `llvm-cov` → dead-code reintro **and** → C/C++ clangd enrichment
- postpass profiling → delta-indexing completion / sharded indexing
- substrate C1/C2 → C3 retirement IMPL
- Rust state-boundaries blocked on Rust `ResolvedCallsite` emission
- Quality discovery surface depends on comparability (toolchain provenance;
  `docs/architecture/versioning-model.txt`)
