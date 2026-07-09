# PERSIST-RECURSION-1 — Postpass AST walks must not scale stack with tree depth

Status: **DELIVERED (2026-07-09)** · Track: Daemon stability — P0

## 0. Delivery record (2026-07-09)

Shipped via target-owned relay (5 review cycles) + operator close-out on the
frozen tree (later rounds had degenerated into an evidence treadmill — each
code tweak invalidated "exact tree" evidence; operator froze and produced the
final set once). What shipped: iterative work-stack walkers replacing
recursion-on-input-depth in the C boundary detector (the proven killer), all
policy-facts extractors, TS boundary detectors, and the express/react
walkers; per-file depth guard with honest skip + recorded diagnostics;
reader-facing degradation surface (doctor/repo-info render deep-nesting
skips); postpass failure isolation, with the honesty machinery itself made
FALLIBLE (ratified: a snapshot whose degradation records cannot persist is
demoted out of READY — Fact-Certainty applied to our own bookkeeping).
Review catches: missing degradation surface (r0), React skip double-count
(r2), fallibility contract (r3). Evidence (operator, frozen tree): build/
fmt/clippy clean, **4908/0 workspace**, dogfood green,
`iterative_walk_is_byte_identical_to_recursive_reference` ok (named; zero
pre-existing assertion lines modified across converted extractors), and the
acceptance transcript: **`rmap index legacy-codebases/linux` → INDEX_EXIT=0,
snapshot READY, daemon clean** — the identical run that produced `fatal
runtime error: stack overflow, aborting` at persisting 5/8 on 2026-07-06.
Residual filed: TECH-DEBT F14 (≤1MB pathologically-deep file could overflow
a MAIN extractor — postpasses now guarded, extractors not; follow-up slice).
Origin: TECH-DEBT F13 root cause — REPRODUCED with a profile transcript

## 1. Problem — a deterministic daemon-killer in persist step 5

Both the operator's kernel-scale profile run (legacy-codebases/linux, release
v0.5.0 binaries, isolated) and the second machine's 151k-file monorepo die
identically: `fatal runtime error: stack overflow, aborting` at
`persisting: 5/8` — `persist_boundary_interactions` (BI-1A), a re-parse
postpass whose AST descent recurses with tree depth. Deep/generated files at
scale overflow the thread stack; the WHOLE DAEMON aborts (uncatchable),
orphaning the snapshot (see DAEMON-CRASH-RECOVERY-1). Memory was a red
herring (RSS peaked 10 GB in extraction, then FELL before the crash).
Sibling suspect with the same shape: step 4 `persist_policy_facts` (PF-1,
also a re-parse postpass); audit all re-parse postpasses and any other
recursive graph/AST walks on the index path.

## 2. Contract

1. Convert the recursive AST walks in the re-parse postpasses
   (boundary-interaction extractor; policy-facts extractor; any sibling
   found by audit) to ITERATIVE traversal (explicit work stack) — depth
   becomes heap-bounded. Prefer this over stacker/segmented stacks (no new
   dependency; deterministic).
2. Defense in depth: a per-file depth guard — beyond a generous bound
   (e.g. 10k), skip THAT FILE's facts for THAT postpass with an extraction
   diagnostic recorded and a reader-frame line in the report/doctor
   ("boundary facts skipped for 1 file (pathological nesting)") — honest
   degradation, never process death.
3. A postpass failure must never abort the index: fallible postpasses record
   their diagnostic and the index completes without their facts (audit which
   already behave this way; the stack overflow bypassed all of it by killing
   the process — item 1 is the real fix, this is the contract statement).

## 3. Stop conditions

- No behavior change to emitted facts on non-pathological input (byte-equal
  outputs on existing fixtures).
- No new dependencies; no schema changes. Do NOT commit.

## 4. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/`, inlined.
- **Regression fixture (named test):** a generated deeply-nested source file
  (e.g. 50k-deep nested blocks/expressions) through the previously-recursive
  postpasses → completes (or honestly skips per the guard); no overflow.
  Byte-equality proof on existing fixtures.
- **Kernel-scale proof (operator will co-verify):** full index of
  legacy-codebases/linux in isolated state COMPLETES to a READY snapshot —
  the exact transcript that today ends in "stack overflow, aborting".
- `./scripts/dogfood-isolated.sh` green.

## 5. Definition of done

`rmap index` on the Linux kernel completes to READY; the 151k-file monorepo
class of repos indexes instead of killing the daemon; pathological files
degrade honestly instead of fatally; proven by the regression fixture + the
kernel transcript.
