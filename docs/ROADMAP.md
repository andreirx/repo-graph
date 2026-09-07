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
   — human-ratified Option A (2026-09-04) and, same day, retention fix (a) [FK-index premise
   RETRACTED 2026-09-05 — indexes exist since 001-initial.sql; operator grep missed the .sql
   bootstrap; mechanism to be reproduced+measured] + chunking AND (b) rebuild-instead-of-delete when prunable dominates, plus a prevention
   set (snapshot hard cap with prune-on-commit, time budget → rebuild, cache sizing, doctor
   visibility, benchmark gate). PRODUCTION REBUILD DONE same day: 40 s reindex replaced a
   5h+ zero-progress prune; store 4,802 → 253 MB, snapshots 29 → 1. Diagnose the assess hang under reproduced
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
regressed: a NEW FABRICATION CLASS (verified: ~~vscode .env resource~~ [RETRACTED 2026-09-04 — real, 4 literal sites;
grader error], hadoop phantom
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
[HONESTY-GATE-1 SHIPPED — see below] ANCHORS-EVERYWHERE-1 (human addition: line numbers on every surface citing symbols/files,
not only find — sequenced AFTER span fidelity so C++ anchors land on correct spans) · 5
SEED-CHUNK-2 · 6 COHERENCE-2 (split 2026-09-05: part 1 = type-only SCC-wide semantics + (N test) subset; part 2 = COHERENCE-3: walks, headline partitions, totals) · 7 DAEMON-CONCURRENCY-1 (human addition; SPEC slice first —
foundational, decisions to ratify) · 8 ECONOMY-2 · carried minors.
SHIPPED: HONESTY-GATE-1 9ea23df (2026-09-04): deps list never renders "unused" without an
established basis — "no static import found" + a TRUE caveat (dynamic imports are
extracted but not attributed to a declared package); counts reconcile to the parsed
manifests (storybook 124 across 12); Maven absence named as a capability limit; node:
builtins bucketed; basis is an exhaustive sum type with the Established arm pinned by
test as the re-enable point. Four verified false claims gone in live proofs. Operator
close-out after two reviewer timeouts at the verdict step (codex high effort ≈2h/diff).
NAMED FOLLOW-UP (2026-09-04, HONESTY-GATE-2 ruling HG2-FAM3-MAP): **JAVA-RESOLVER-IDENTITY-1**
— the Java resolver emits intra-repo CALLS/INSTANTIATES edges on bare-name collisions with
JDK classes (hadoop MavenWrapperDownloader: 3 "resolved" edges for a JDK-only file).
Requiring qualified identity is a Layer-0 contract change across callers/callees/dead/
trust — its own slice with a whole-graph before/after parity budget. Deferred by ruling;
HONESTY-GATE-2 renders the resolution basis on the edge if the resolver records one.
## rmap-on-rmap modeling experiment (2026-09-04, human-directed) — docs/audits/2026-09-04-self-model.md
Artifact: https://claude.ai/code/artifact/a7630d1b-6c7c-41a6-868e-4536a545b471 · CURSOR-ROUNDTRIP-1
(spec b3417df) slotted immediately after HONESTY-GATE-2 — small, user-visible garbage on a common path.

Question: can rmap model its OWN modules as algorithms + data structures + data flows +
access patterns? Method: 24 captures + ~20 live queries on the v0.16.0 isolated index;
every model statement labeled [rmap:…] or [source:…]. RESULT: ~30% from rmap, all in the
topology/attention band (59 modules, hotspots, complexity centers, the type-only cycle,
and — best surface — `find --text` enumerating all 61 lock-acquisition sites with
enclosing symbols); ~70% from source: every algorithm identity, every struct shape, every
lock SEMANTIC, every DTO crossing, and the ENTIRE crate dependency graph — rmap resolves
NO cross-module dependency on its own Rust workspace (7,490 unresolved imports; 47
modules "zero connectivity"; the Unix-socket NDJSON protocol — the system's real IPC
boundary — undetected; the 48-table SQLite store invisible to `resource list`; no
trait/port surface). Verdict: "a triage instrument that tells you where to read, not yet
a modeling instrument you can build an architecture from."
THE FIVE MODELING SURFACES (proposed track MODELING-SURFACES, awaiting human ordering):
1 Rust import/module-edge resolution (gates all others) · 2 `rmap shape <symbol>` —
struct fields/enum variants/trait methods · 3 store/schema surface for Rust (tables from
migrations; per-symbol readers/writers) · 4 concurrency/lock surface (guard-typed values,
where acquired, held across what) · 5 trait/port surface (implementors, dyn/generic
dispatch edges).
LIVE DEFECTS FOUND (v0.16.0 regressions, small): CURSOR-ROUNDTRIP-1 — find's printed
short cursors are accepted by `explain` only; `callers`/`callees` reject them ("symbol
not found") — extend the FIND-EVIDENCE-1 alias; SEED-FALLBACK-DTO-1 — the not-found
fallback renders seed candidates via the pre-SEED-CHUNK-1 DTO shape, printing
"(malformed candidate: missing file/stable_key/score/model_id/source)" ×N in user output.
Probe corrections: `--text` regex alternation WORKS (201 hits) — the initial "no
alternation" claim was an operator quoting error.
SHIPPED: HONESTY-GATE-2 (2026-09-04): resources — `Direction::Unknown` → ACCESSES edge,
stored/counted separately, rendered "access (mode unknown)"; C/C++ fopen/fstream modes
accept only demonstrated-valid forms (dynamic, malformed `"r++"`, near-name tokens →
unknown); `.ok()?` collapses swept from the classified-read paths of three extractors;
trust's Next.js detection requires app-router convention AND `next.config.*`, basis
rendered (name-only → honest abstention); map JDK-collision edges DEFERRED to
JAVA-RESOLVER-IDENTITY-1 by ruling. The audit's vscode `.env` fabrication claim was
RETRACTED mid-slice (four literal sites exist — product right, grader wrong). 6 cycles,
closed-list steering converged it. Gates ALL GREEN.
SHIPPED: CURSOR-ROUNDTRIP-1 (2026-09-04): every printed short cursor resolves in
callers/callees/path via ONE syntax-gated, storage-free helper (nodes-free green path
kept); the not-found fallback shares find's current seed renderer (invalid candidates →
one counted "unreadable" line, never a malformed placeholder); additive `cursor_raw`
JSON field (verb-less, unquoted, uid-stripped). Live proof on the retained seeded root:
`callers` on a short cursor → 65 real callers (was "symbol not found"); nonexistent
symbol → real candidates, zero "malformed". Gates ALL GREEN; approved review-1.
SHIPPED: CPP-SPAN-FIDELITY-1 (2026-09-05): macro-decorated C++ types carry their real
names and kinds (name = last identifier before base-clause/body; kind from the keyword;
macros kept as metadata) and spans never take an ERROR extent (balanced-brace recovery,
raw-string-aware; unrecoverable → no span; parser-swallowed siblings re-walked at true
scope). Mechanisms proven: tree-sitter error recovery (names) + preprocessor-confused
brace matching (spans). Live: env_posix.cc 13 → 74 symbols, Limiter 73–130, Posix*File
recovered; vcmi CGHeroInstance → SYMBOL:CLASS (DLL_LINKAGE no longer a name); leveldb
DB → SYMBOL:CLASS; `--text fsync` → [method PosixWritableFile::SyncFd]. One reindex key
transition (~813 vcmi, 23 leveldb defs); openxcom byte-stable. Single file +766/−83; 80
crate tests; gates ALL GREEN; approved review-2.
SHIPPED: ANCHORS-EVERYWHERE-1 (2026-09-05, human directive "line numbers everywhere"):
one shared `path:line` formatter; Tier 0 (explain header + Symbols, line already on the
wire); Tier 1 (explain callers/callees/candidates + orient complexity centers via
SQLite-paired file+line reads; boundaries rows through four structs, grouped headlines
never anchored); Tier 2 (surfaces show renders `payload_json.lineStart` — it WAS stored;
malformed payload surfaced, never silently bare). LiveGraph rebuild keeps SQLite
file+line together while swapping only the name (single-source invariant asserted).
Growth 0.8–3.4%, anchor suffixes only. Reviewer spot-checked leveldb/FRAKTAG anchors
against source. 42 files +1,015/−87; gates ALL GREEN; approved review-4.
NAMED FOLLOW-UP (2026-09-05, SEED-CHUNK-2 ruling SC2-FRAKTAG-DOD): **SEED-CHUNK-3** —
short-chunk / property demotion: one-line interface properties (FRAKTAG: 6 of 10 slots for
"where are conversations persisted") have NO implementation counterpart, so decl demotion
cannot reach them; a chunk-length/kind-aware ranking policy needs its own measured slice.
SHIPPED: SEED-CHUNK-2 (2026-09-05): per-chunk is_test from structural evidence (Rust
`#[test]`/`#[cfg(test)]` on the item or an enclosing mod with real predicate evaluation —
`cfg(not(test))` is production; TS/JS enclosing describe/it/test); migration 034 adds
per-chunk `is_test`/`is_decl` (nullable = pre-034, REFUSED as classified and SELF-HEALED:
the daemon schedules a re-seed via the SeedCoordinator latch and renders the pending
reason); partition FIRST, then declaration-below-ANY-implementation within it, `(decl)`
labeled; `--text` referral always beside seeds, shell-quoted. Live: repo-graph fresh
isolated index → 16,980 chunks, 5,329 in the test partition (≥ the 4,578 literal floor);
leveldb impl above its higher-scoring decl; FRAKTAG re-scoped to referral evidence
(one-line PROPERTIES have no impl counterpart → SEED-CHUNK-3). Strain: two 120-min proof
kills + a stash lost to a timeout; converged after closed-list steering. 19+4 files;
gates ALL GREEN; approved review-4.
SHIPPED: COHERENCE-2 part 1 (2026-09-05): type-only cycle verdicts are SCC-wide —
`BreaksAtRuntime{k of n}` only when the runtime-only subgraph of the SCC is acyclic
(Option A; the spec's literal any-edge rule would have fabricated "broken" claims on
multi-cycle SCCs) — and rendered by one shared label function on cycles AND orient;
`(N test)` is a subset on every renderer, modules-list sums reconcile with check, absent
counts render "(test count unavailable)". Live: FRAKTAG's 5-module SCC → BreaksAtRuntime
(3 of 10 edges type-only, no runtime cycle remains); rgistr stays TypeOnly. Split:
walks / headline partitions / totals → COHERENCE-3 (spec + packet staged). 16 files
+1,070/−102; gates ALL GREEN; approved review-3.
SHIPPED: COHERENCE-3 (2026-09-05, part 2): one cycle-walk derivation (`agent::cycle_walk`,
additive `walk` field) — orient renders a walk only when cycles would, else the unordered
form; malformed walk evidence renders no walk; surfaces/boundaries headlines partition
test fixtures on the stored is_test fact (vscode 99/9 → `54 (3 providers / 51 consumers)
(+45 test-fixture excluded)`); every file total names its basis (same-basis totals agree:
orient == check `1909 files indexed`). Operator close-out after a 3-cycle checkpoint with
the diff reviewed sound and only evidence unrecorded; operator gates + live proof on the
retained root. Minor carried: `1 owned files` plural.
NAMED FOLLOW-UP (2026-09-05, DAEMON-RESIDUALS-1 ruling D1): **ENRICH-GUARD-1** — the enrich
pass holds the DB write mutex AND the repo coordinator's refresh guard for its entire
duration (rust-analyzer resolution included), so every same-DB foreground request waits
unboundedly (the assess 301 s hang: assess takes the write mutex FIRST). D1-A (bounded
patience + named Busy on both layers) ships in DAEMON-RESIDUALS-1; the deeper fix —
resolve without holding either exclusive guard, acquire both only for a verified atomic
promotion — needs its own slice because it touches promotion atomicity.
SPLIT (2026-09-05, ruling split-daemon-residuals-1 = A): DAEMON-RESIDUALS-1 → the shipped
D1-A increment (bounded patience + named Busy on BOTH the DB write mutex and the
coordinator guards for assess/coverage/…); DAEMON-RESIDUALS-2 = retention (reproduce →
measured remedy → rebuild path → prevention set); DAEMON-RESIDUALS-3 = seed-publish
chunking + #2b LiveGraph coordinator guard + VACUUM guard. Order after ECONOMY-2 and the
v0.17.0 release/audit: -2 then -3.
SHIPPED: DAEMON-RESIDUALS-1 = D1-A (2026-09-05): foreground handlers (assess, coverage, …)
no longer wait unboundedly — bounded patience + NAMED typed Busy on BOTH the DB write
mutex and the coordinator guards, holder class + elapsed from the activity registry.
Diagnosis evidenced (lock order + 1.30/1.44 s measured blocks), not reproduced at 301 s;
enrich holding both exclusive guards for its whole run → ENRICH-GUARD-1. Rescoped by
ruling A; -2 (retention) and -3 (seed chunking/#2b/VACUUM) staged.
SHIPPED: ECONOMY-2 (2026-09-06): seed cursor lines collapse to one header pattern (rows
already carry the short cursor's identity) — literal whole-cursor-line share on a
seed-bearing find 53.05% → 6.69%; `orient --full` caps long tails at a STATED 200 rows
with truthful elision lines and "output complete" only when nothing (docs included) was
elided (gstreamer 314,730 → 38,987 B; zvec-grep identical-to-large notice); `map --dry-run`
gets a source/sidecar count header + hard line cap (gstreamer 12.99 MB → 24.6 KB, hadoop
32.2 MB → 31.4 KB); public `--full` help corrected. 5 cycles + operator close-out on a
single stale comment. THE v0.16.0 AUDIT QUEUE IS COMPLETE — next: release v0.17.0 + audit
round five; DAEMON-RESIDUALS-2/-3 staged after.
Infra: serial-daemon contention MEASURED under batch (301s assess hang; Busy bounce; the
chunk seed pass is a new long writer) — DAEMON-CONCURRENCY-1 price rising.

RELEASED v0.17.0 (2026-09-06, 28b2ff6). AUDIT ROUND FIVE (2026-09-06, `docs/audits/
2026-09-06-per-command-usefulness-v0.17.0.md`; smoke 2026-09-06T03-20-35Z, 26 passed, two
typed NAMED-Busy bounces retested clean; 32 supplemental probes; three-perspective gate,
ground-truth-verified with stated searches). Whole product HIT C+→B−, EVIDENCE C+→B,
HONESTY C−→C+, ECONOMY C→B−; `find` vs zg now B−/B+/B/A− (level HIT, leads the rest).
Every TARGETED v0.16.0 fabrication verified dead (phantom writers → mode unknown; Next.js
two-signal gate with amodx's true basis kept; storybook 13+111=124; JDK-only edges →
"External / unresolved (11)"; vscode .env retraction confirmed; C++ --text 6/6 exact;
map --dry-run reconciles). VERDICT (codex-adjudicated, adopting the matrix grader over the
judge's "new absence class"): THE CLASS RELOCATED to the command-summary layer — the product
now fabricates authoritative absence/architecture claims from low-trust graphs: `modules
list` says "all imports are intra-module" on 18/28 repos INCLUDING repo-graph's own 59-crate
workspace while `trust` on the SAME snapshot lists 2,821 internal references and every module
zero-connectivity-suspicious (D1, CRITICAL — stronger than any v0.16.0 instance); django deps
"no static import: asgiref" against 42 static sites (D2, CRITICAL — HONESTY-GATE-1's own
invariant violated: the caveat's escape hatches do not apply); `cycles`/`explain` bare zeros
where trust says LOW (D8/D10); check PASS at 27% vs FAIL at 24% (D15). The repair was applied
PER SURFACE, not as an invariant. Also: C/C++ macro-wrapped FUNCTION names still erased (D3,
26 rows); C++ forward decls rendered as definitions and outranking them (D4, vcmi 7/8);
three irreconcilable file totals per snapshot (D5 — COHERENCE-3 labels landed, reconciliation
did not); surfaces "0 project surfaces" beside 235 real routes + test rows first (D6); zvec
--full marker false (D7); fixture App.java → repo-level Spring fact (D9); FRAKTAG seeds return
decls not the write path (D11); complexity headline = symbol cx under a file label (D12);
boundaries 0/772 anchors (D14); `dead` exit 2 + "error:" on a correct refusal.
WITHDRAWN QUEUE (human directive 2026-09-06: root-cause before packeting): the first proposed
queue (CLAIM-INVARIANT-1 → …) was symptom-labelled. ROOT CAUSES RECORDED
(`docs/audits/2026-09-06-root-causes-v0.17.0.md`, five read-only investigations, code-cited):
A. D1+D10 = the resolver has NO stage mapping Rust `use <crate>::` / Java FQN specifiers to files
(never worked; June smoke already 0; MODULE-EDGES-1 verified on C++ only) + grpc-java
`projectDir` unparsed; renderers' zero branches consult only the HTTP link count. B. D2 = the
index-time classifier exact-matches the FULL dotted specifier (`asgiref.sync` ≠ `asgiref`;
`import sqlparse` matches) → asgiref's edges sit in `unknown`, a fourth bucket NO headline counts
(the "419 could contain it" premise was false); reconcile emits the negative while the bindings
are in memory; HONESTY-GATE-1 relabeled and canonized the false row as a test fixture. C. D3 =
both C/C++ `extract_function_name`s return the macro token for `M(name)(args)` and ignore the
ERROR child for attribute-macro shapes; D4 = bodiless `class X;` emits an unflagged symbol, no
decl column on nodes, and `pick_unambiguous` DROPS every inheritance edge to a 70×-declared
class. D. D5 = three file-count bases (file_versions all rows / OWNS on FILE nodes with a "/" /
manifest ownership incl. root "."; grpc-java 290 = 96 config + 194 proto, exact); D12 dedup-by-
file; D7 = ECONOMY-2 REGRESSION (byte-equality with large while the group fallback is capped at
a fixed 12) — D7 and D12 are PINNED BY TESTS asserting the defect; D6 "project surfaces" = the
catalog after HTTP is lifted out; D9 inference rows never join files.is_test; COH-2 orient gate.
E. D11 = property seed documents are ~90% their qualified name (doc_comment: None). F. isolated
one-file causes (docs kind order/"license"=has-header; boundaries groups never read their
line; not-found path passes repo_uid None; doctor seed probe always passed; SMOKE_SKIP never
appends to SKIPPED_REPOS). G. by-design renderings dropping the qualifier (check `ceiling`
marker dropped at the CLI DTO; trust root posture MEET over a dev-only LiveGraph; dead exit 2
frozen). Cause-cut slices proposed to the human — see the audit doc.
SHIPPED: IMPORT-RESOLUTION-RUST-1 (2026-09-06, 6 cycles): the declared Cargo catalog crosses
the compose→indexer boundary as a raw DTO; a pure resolver stage maps `use other_crate::…` to
the defining file (crate root + `<segs>.rs` / `mod.rs` / shorten / `lib.rs` / `main.rs`), gated
on Rust-extractor provenance; one `_`→`-` canonicalisation replaces three copies (the fourth,
config.rs, is the OPPOSITE transform — name-vs-semantics catch by the builder); modules-list
and cycles zero-states print resolved AND unresolved counts (cycles names its population
"directory groups" — the two-crate fixture has four; cycles-over-directories vs modules-list-
over-declared-modules recorded as a FOLLOW-UP). repo-graph: 0 → 129 cross-crate edges,
unresolved imports 7,528 → 4,789, trust's first-party lines shrink by exactly the resolved
count; `map --dry-run` moves a cross-crate import to `indexer/src/storage_port.rs`; leveldb
byte-stable bar the permitted clause. Cycle 3 fixed a builder-caught honesty defect (a storage
read failure rendered as "older daemon — reindex"). Four additive wire fields.
SHIPPED: DAEMON-RESIDUALS-2 increment 1 (2026-09-06, 255b315, 4 cycles; DR-1 = B by human):
migration 035 adds the four single-column FK-child indexes with a schema-derived completeness
gate; the EXPLAIN harness now ASSERTS SCAN-before/SEARCH-after; chunked per-snapshot prune
with the write slot released between chunks (per-chunk atomicity documented truthfully and
trigger-tested — the old doc claimed single-transaction all-or-nothing) and the maintenance
cache_size (restore failure surfaced, not swallowed); retention BENCHMARK GATE (3×1,200-node
snapshots pruned in 0.108 s under a 10 s bound; the same shape took 374 s before). MEASURED
insert-path cost on a real repo-graph index: +0.46 s (+5.1%, single run, noisy; synthetic
+14.3%), store +7.0 MiB (+2.7%). Increment 2 = DAEMON-RESIDUALS-2B (rebuild path +
`maintenance rebuild`, snapshot cap + prune-on-commit, time budget → rebuild, doctor fields)
launched next; JAVA-1 follows.
DAEMON-RESIDUALS-2B (increment 2) cycle 1 (2026-09-06): SHIPPED the observability part
(600da87 — doctor renders the prunable share with its basis and the last-pass duration);
the builder STOPPED on the rest, correctly: (i) the snapshot cap + prune-on-commit ALREADY
EXIST — every index/refresh commit chains enrich → seed → retention asynchronously
(dispatch.rs finish_write_with_maintenance) and the keep-set is already current + parent
(retention/classify.rs) — production reached 29 snapshots because the prune never FINISHED,
which 035 fixed; a synchronous prune at commit would break the frozen "never on the
foreground path" invariant for no outward gain; (ii) the rebuild path + time-budget → rebuild
is a safety net whose premise (a prune that cannot keep up) the diagnosis removed, and it
costs a ~40-table filtered copy + atomic file swap under the writer guard — a new corruption
surface. DECISION surfaced to the human: A keep the rebuild machinery (own slice, swap
discipline ratified first) vs B drop it — prove the async cap on a real multi-snapshot store
with 035, doctor names the manual rebuild when a pass exceeds its budget. Operator
recommendation B. IMPORT-RESOLUTION-JAVA-1 launched meanwhile (tree free).
HUMAN RULING (2026-09-06): **B**, plus "keep the option of fully wiping and reindexing in the
daemon — separate verb" → `rmap repo rebuild <path>` (spec §7): explicit `--yes`/confirm naming
what is discarded, the daemon owns the coordination (FIFO Writing guard, NAMED Busy when a
reader or a detached index holds it — the 2026-09-04 hazard), drops the repo's store
atomically, indexes from scratch, additive wire method. DAEMON-RESIDUALS-2C = proof that the
existing async cap holds at current+parent with 035 (leveldb multi-snapshot, concurrent read
loop under the patience) + the verb + a REPORTING-ONLY retention budget (doctor names the
overrun and the verb; "under 1s" for sub-second passes). Queued after JAVA-1.
SHIPPED: IMPORT-RESOLUTION-JAVA-1 (2026-09-06, dc07527, 3 cycles): a Java `import a.b.C`
resolves to the file defining `C` via a once-per-index suffix index over the file list
(nested-class / static-member imports shorten until a file matches); wildcard and
ambiguous-suffix imports stay unresolved with NAMED, COUNTED bases (max wildcard 0.52% hadoop,
max ambiguous 0.10% kafka — the 5 grpc-java ambiguities are exactly the `netty/shaded`
copies); `settings.gradle` `projectDir` relocations honoured with a strict grammar (unhandled
forms counted and surfaced on modules list, never prefix-guessed); the modules-list unresolved
count covers all three import categories through one shared constant with a regression test.
Movement (isolated before/after): kafka module edges 1 → 5,602 (42,482 file-imports resolved;
67 modules; 13 module cycles), grpc-java 0 → 814 (43 modules OWN their files; root `grpc` owns
111, not 1,624; 4 real cycles), hadoop 51 → 16,780 (Maven still unparsed — inferred dir
modules), langchain4j 0 → 3,029, spring-petclinic 0 → 8; leveldb and repo-graph (129)
byte-stable.
SHIPPED: DAEMON-RESIDUALS-2C (2026-09-07, 1f77e36, 9 cycles + operator close-out on one
comment word): `rmap repo rebuild <path>` — the wipe-and-reindex verb the human asked to keep:
explicit `--yes` naming what is discarded; the daemon owns the coordination (FIFO Writing guard;
NAMED Busy — "a concurrent READ is holding this repo's store" for readers, holder + age for
writers); retire-by-rename with rollback/restore; the sentinel `<store>.rebuilding` = "never
serve a partial store" (human ruling detect-and-name), enforced at the ONE gated store open
(three per-caller bypasses found by review before the seam moved — load_repo, boot reconcile,
enrich pass — now a call-site enumeration test fails CI on the next one); the remedy works on
any partial state incl. an absent `.db`; a failed sentinel unlink is a named non-success;
`symbols_total` additive; the no-op `--progress` flag removed. Reporting-only retention
budget: aborts at the chunk boundary, doctor names the overrun with its basis and the verb;
sub-second passes render "under 1s". The existing async cap PROVEN on isolated leveldb: 7
re-indexes → 1 snapshot after each; concurrent reads 57–68 ms vs the 450 ms patience. The
reader-held live race could not be provoked (reads are sub-second) — proven at the dispatch
layer by ruling. DAEMON-RESIDUALS-2 is COMPLETE (increments 1, 2B observability, 2C);
DAEMON-RESIDUALS-3 remains staged. CPP-DECLARATORS-1 launched next.
SHIPPED: CPP-DECLARATORS-1 (2026-09-07, 5c3ec2d, 7 cycles + operator close-out): macro-wrapped
C/C++ function names recovered (`M(name)(args)` in both extractors; the attribute-macro shape is
C++-only — the pinned tree-sitter-c never yields it, so the C branch was removed as code for an
imagined variation); a declaration is a STORED fact (`forward_decl` on bodiless types AND in-class
method prototypes; a corrupt carrier is a named Unreadable state, excluded from the decl tier and
counted — never a `(decl)` fact); definition-first in find, seeds and resolution; C++ `Implements`
affinity admits CLASS/STRUCT under extractor provenance (the root-cause report's one UNDETERMINED:
inheritance edges were dropped by affinity BEFORE ambiguity); enclosing-class call preference;
receiver text kept in metadata; a bare type name beats its own constructors with a rendered
pointer to the constructor. MOVEMENT (clean worktree baselines): vcmi resolved IMPLEMENTS edges
2 → 852; `find CGHeroInstance` definition first above `(decl)` rows; `explain CGHeroInstance`
resolves; leveldb `callers leveldb::DBImpl::Recover` ambiguous → 1 real caller, callees 16 → 21
(`NewDB`, `RecoverLogFile` restored); macro-named complexity rows → 0 on the hadoop uriparser /
duckdb zstd / poco pcre2+expat subtrees; leveldb key churn 0; FRAKTAG byte-stable. Strain: three
cycles lost to an over-heavy proof protocol (operator lesson recorded); `explain` has NO
inheritance section — filed EXPLAIN-BASES-1. SYMBOL-IDENTITY-1 launched next.
SHIPPED: SYMBOL-IDENTITY-1 (2026-09-07, f5cfe1e; reviewed by claude-opus-4-6 — Codex quota interim
by human directive): what `find` prints, `explain`/`callers`/`callees` accept. ONE shared resolver:
exact stable_key → qualified_name → name → a qualified-SUFFIX step (`qualified_name` ends with
`::`/`.` + query; one hit resolves, >1 lists candidates with files, 0 not-found) with the
CPP-DECLARATORS-1 decl/def filter; measured 29–31 ms warm on django's 78k symbols (index search on
the snapshot's SYMBOL rows, not a scan). `explain` routes through a new `AgentStorageRead::
resolve_symbol` port method returning a raw `Resolved | Ambiguous | NotFound` DTO, delegated to
SQLite by the daemon decorator (ruling EXPLAIN-RESOLVER-ROUTING = B: the dev-only LiveGraph
"nodes-free on green" invariant amended for explain's resolution step; `resolve_symbol_name` stays
name-only for orient + the parity cert, its comment de-lied); type-beats-constructor collapse on
the ambiguous set; a miss or ambiguity renders `Confidence: low`, never the old static `high`.
Movement: `explain DBImpl::Recover` no_match/high → resolved (ambiguous(2) listed at low; the
definition after the CPP flag); `callers OwnerController.processCreationForm` "symbol not found" →
`OwnerController.java:77`. Process: the Codex reviewer failed 4×/cycle on a usage-limit lockout
(until Sep 10); the interim Claude reviewer's approval was mis-parsed as `unknown` by the relay
because of a stray preamble (agent-manager TD-017). DEPS-CLASSIFIER-1 launched next.
SHIPPED: DEPS-CLASSIFIER-1 increment 1 (2026-09-07, 09071cf; reviewed by claude-opus-4-6 — Codex
quota interim): a declared dependency is "used" when ANY of its modules is imported. ONE shared head
reduction (`classification::dep_reduce`: Python head + PEP 503, npm scope/subpath, Rust `::`) now
serves the index-time classifier (import path AND call-path Rule 5b) and the query-time
normalizers — the demonstrated duplication that earned the module; Python import edges read the
dotted specifier from metadata (the slash/dot mismatch); per-package computed basis `used (N
import sites, M call sites)`; the canonized false fixture replaced by the true row; the "also
present" ecosystem line already existed (DEPS-ATTRIB-2) and is wired. django before: asgiref's
153 edges all `unknown`, trust never attributing a library call to it → after: `used: asgiref (51
import sites, 131 call sites), sqlparse (4, 8) · no static import: tzdata` (truly unused). Rust
byte-stable (retained copy, no reindex); kafka byte-stability not run (kafka is not in the operator
registry — the Java branch is untouched and unit-covered). Sequencing ruling: one relay run could
not carry the TS substrate half + four reindexes + the §3 STOP trigger → increment 2
(DEPS-CLASSIFIER-1B: TS bare-import IMPORTS edge, `require` binding, `import type` surviving the
storage read, the two-root-manifests collision) launched next with the STOP measurement on
storybook + FRAKTAG quarantined there.
QUEUE RATIFIED (human 2026-09-06, "ok with proposed queue"): DAEMON-RESIDUALS-2 increment 1
(in flight) → IMPORT-RESOLUTION-JAVA-1 → CPP-DECLARATORS-1 → SYMBOL-IDENTITY-1 (new, from the
codegraph round) → DEPS-CLASSIFIER-1 → HEADLINE-TRUTH-1 → SEED-CHUNK-3 → AUDIT5-MINORS-1 →
EXIT-CODES-1; DAEMON-RESIDUALS-2 increment 2 and DAEMON-RESIDUALS-3 interleave after -2
increment 1 per the earlier ratification (-2 then -3).
RATIFIED (human 2026-09-06): fix at the cause, never a query-time gate or zero-state wording
over a known index-time defect ("no reindex is not a reward"). ORDER: IMPORT-RESOLUTION-RUST-1
→ IMPORT-RESOLUTION-JAVA-1 → CPP-DECLARATORS-1 → DEPS-CLASSIFIER-1 → HEADLINE-TRUTH-1 →
SEED-CHUNK-3 → AUDIT5-MINORS-1 → EXIT-CODES-1 (specs `docs/slices/<name>.md`; the two
IMPORT-RESOLUTION specs follow a seam investigation). G1 (check verdict qualifier) and G2
(LiveGraph residency) DROPPED as instrument navel-gazing — "the map is not the territory";
the constant LiveGraph posture lines leave the human trust output (AUDIT5-MINORS-1 F4).
DAEMON-RESIDUALS-2 cycle 1 (2026-09-06): the builder MEASURED the retention mechanism — the
FK cascade's per-node child lookups (edges.source/target_node_uid, unresolved_edges.
source_node_uid, nodes.parent_node_uid) are full SCANs because every index is composite
with snapshot_uid leading → O(nodes × child rows) per snapshot (68 MB toy: 374 s; 512 MB
cache 265 s; FK-OFF explicit deletes 0.52 s; single-column FK indexes 0.84 s). This
reconciles the 2026-09-05 retraction: the indexes EXIST but do not serve the cascade.
DR-1 RULED BY HUMAN 2026-09-06: **B** — additive single-column FK indexes, FK-ON cascade
kept, SQLite keeps enforcing integrity; the insert-path cost is MEASURED and reported (spec
§6). Relaunch queued behind IMPORT-RESOLUTION-RUST-1 (one relay per tree). Operator rulings: the diagnosis harness asserts its plans; the slice splits into
increment 1 (mechanism + guard/index + benchmark gate) and increment 2 (rebuild path +
prevention set); DAEMON-RESIDUALS-3 follows. Next horizon (human): how a diff maps to
deltas in the in-memory representation and in SQLite — CHARACTERISED 2026-09-06
(`docs/audits/2026-09-06-change-path-characterisation.md`): `rmap refresh` is per-file at
EXTRACTION only (content-hash plan; changed files re-extracted, unchanged ROW-COPIED forward);
resolution and every aggregate re-run whole-snapshot; snapshots are full row copies (a no-change
django refresh ≈ 84–102 s ≈ a full index); identity is path-based with no rename tracking; no
reverse-dependency map exists; the LiveGraph is a separate TS-only whole-partition store.
CODEGRAPH COMPARISON (human request 2026-09-06; `docs/audits/2026-09-06-codegraph-vs-rmap.md`;
codegraph 1.6.0 b9ca4b7, same corpus/metric as the zg round): rmap wins OVERVIEW 5–0 (module
model, edges, cycles, surfaces, reliability floors; codegraph has no overview verb) and HONESTY
(B+ vs C−: codegraph binds virtual/duck-typed calls to whichever same-named definition — a test
mock, a GIS mixin — and prints a 20-row default as a total; rmap invented no edge); codegraph
wins SYMBOL+CALLERS 3–1, CONCEPT 4–1 (verbatim bodies land the right file), IMPACT 3–0 (rmap
has no file-level dependents / symbol impact), CHANGE HANDLING (one-file sync 0.5 s vs ~100 s
no-change refresh). rmap economy 171 KB vs 483 KB for the same questions. NEW DEFECTS SURFACED
(to ROOT-CAUSE before packeting, per practice): the find → explain/callers HAND-OFF is broken —
`explain DBImpl::Recover` says "unresolved: no_match / Confidence: high" for the string `find`
just resolved; `callers leveldb::DBImpl::Recover` ambiguous decl-vs-def; `callers
OwnerController.processCreationForm` "symbol not found" while its own hint lists the FQN; only
the full stable key works and then reports 0 callers where 1–3 exist (C++/Python); TS provider
rows carry no line anchors. IMITATE candidates (mapped to outward surfaces): route-as-caller
(rmap has the fact in find's http-surface rows), resolve-what-find-printed, bounded verbatim
source on explain, file dependents + symbol impact with the LOW banner, second-scale
incremental update (codegraph's edge-snapshot/re-attach/resurrect + replay benchmark is a
measured, cheaper alternative to snapshot-stable identity), provider line anchors. codegraph
STRUCTURE (same doc): TS engine 109k LOC owns resolution/storage/MCP; a 25k-LOC Rust kernel does
parse+extract only (napi, optional, ABI-checked, per-file fallback); one mutable SQLite per
repo, no snapshot history; id = sha256(path:kind:name:LINE); no LSP/SCIP; change tracking =
fs.watch + content hash + changed-files-only re-extract + name-keyed edge re-attach; reverse
index used for reporting only; one default MCP tool with a CHARACTER budget; never isError for
expected conditions; README "full support" labels exceed its resolver for C#/Ruby/Swift/Dart/
Scala. KEEP: dead's refusal, gate's vacuous-pass, inferences' three-way cause
discrimination, django boundaries zero-state (what cycles should print), map --dry-run cap
discipline, trust basis lines, named-holder Busy (D1-A visible in the field: "started 43s
ago"), find --text enclosing symbol, glamCRM surfaces (best single output).

Open follow-ups awaiting ratification/scheduling: EXPLAIN-BASES-1 (no surface renders a type's
base classes / implementors; the edges now exist — CPP-DECLARATORS-1, 2026-09-07), CPP-FNPTR-RETURN-1 (C extractor emits no node
for a function returning a function pointer — `parenthesized_declarator` never handled; found by
CPP-DECLARATORS-1's negative fixture 2026-09-07), CYCLES-POPULATION-1 (cycles runs over directory
groups while modules list uses declared modules — found by IMPORT-RESOLUTION-RUST-1),
RESOURCE-DYNAMIC-PATH-1, docs
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
