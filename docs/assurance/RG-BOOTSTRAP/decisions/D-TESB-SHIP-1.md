# D-TESB-SHIP-1 — TEST-EDGE-SCOPE-1B ships as built, with three named follow-ups; the implementation review's open decision is carried, not resolved

Raised: 2026-09-30 by the implementation review of TEST-EDGE-SCOPE-1B (INPUT-3, admission 3, cycle 1; reviewer codex gpt-6-sol; `.agent-manager/slices/TEST-EDGE-SCOPE-1B/review-0.json`, local).

**What that review established.**
- The candidate, 94 allocated paths at base `cabf3b86`, closed every earlier finding. The builder reports all 16 checks TESB-C01…C16 passed (BUILDER-EXECUTED).
- The reviewer verified the field witnesses:
  - leveldb `table/table_test.cc:11–13` (the test-only `db → table → db` cycle leaves the default view and is named as excluded);
  - poco `Foundation/testsuite/src/ArrayTest.cpp:12` `#include "CppUnit/TestCaller.h"` (`Foundation → CppUnit` leaves the default module view, with the test-import remainder stated);
  - leveldb's per-file imports unchanged at 14 rows.
- The verdict was **decision-required**, on three items:
  1. **TESB-GATE-UNMEASURED-ZERO.** For an `arch_violations` / `module_violations` evaluation with no usable method evidence or target (`gate/src/compute.rs:231–234`, `UnsupportedMethod`), `state_nothing_judged_when_absent` writes `inferred_imports_not_judged: {"count":0,…}` without any import read. A consumer can read "no inferred imports withheld" where the population was never measured (RG-REQ-002-L04/L11). The reviewer recommended reporting "not measured" with the existing reason.
  2. **Semantically contradictory carriers accepted as stated** (`rgr/src/presentation/import_partition.rs:298–326, 459–505`). Two examples: a `not_judged` block with `count: 0` beside a non-empty relation list, and an excluded cycle with an unknown flag string. Both decode as `stated`. The product's own daemon does not emit such payloads. This is the fourth appearance of the partition-evidence-status class.
  3. **A missing hand-off quote:** the kafka `trust` zero-connectivity row that gained the exclusion clause (slice §6). This is evidence, not code.

Resolved: 2026-09-30 by the HUMAN. The manager presented three options self-contained:
- fix 1 and 3 and defer 2 (the manager's recommendation);
- fix all three here;
- ship as is with follow-ups.

The human chose **"Ship as is + follow-ups"**, with the risk stated to them: the release carries a gate field that says "nothing withheld" when nothing was measured, a false-certainty claim this slice introduces, against RG-REQ-002-L04.

## What this record is and is not
- It is the human's authority to commit the TEST-EDGE-SCOPE-1B candidate and to treat the release gate D-PSI-RELEASE-GATE-1 as met by that commit.
- It does NOT say the implementation review accepted the candidate: it did not, and its verdict stands as written.
- The acceptance evidence is:
  - the builder's 16 executed checks;
  - the reviewer's verified witnesses;
  - the operator's gate suite on the unchanged candidate;
  - this ruling.

## Follow-ups (named, queued)
- **TESB-GATE-UNMEASURED-1:** an unevaluable import-judging obligation reports "not measured" with its reason, never `count: 0`. This is the reviewer's option A; tests cover the missing-target and absent-method-evidence cases in gate computation and in both render paths.
- **PARTITION-DECODE-HARDENING-1:** the shared partition decoders validate count/list consistency and the closed flag vocabulary. Contradictory carriers become `unreadable` with a reason; tests cover both counterexamples and their valid neighbours.
- **TESB-KAFKA-TRUST-WITNESS:** capture the kafka `trust` row with the exclusion clause on an isolated copy of the before-root, and add it to the ship record.

## Process note
Agent-manager `d210369` (2026-09-30 06:51, the human's other session) changed `SYSTEM.txt`'s first line after this admission pinned it. The build and review ran under the pinned version; the relay then refused to bind the review ("persisted reviewed-input instruction identities changed"). No further cycle runs on this admission, so the review stays as its raw record.

## Status at closeout (2026-10-02, in-place-manager) — what happened after the ruling
The ruling above was taken on admission 3's review. Before the commit, the operator's gate suite failed on daemon cancellation tests whose fixture was module-only; the HUMAN ruled "Fix the fixture first" (2026-10-02), which cost INPUT-4 (A-3, the fixture rebuilt, the whole daemon-runtime suite bound as TESB-C17), INPUT-5 (TESB-C14 fail-closed; the five before-roots re-captured after `/private/tmp` lost them) and INPUT-6 (C11 asserts the cycle's member set and edges, not a walk order; C13 accepts the documented `source.txt` suffix). Admission 6's review ACCEPTED the candidate (review-0 and review-1, codex gpt-6-sol; all 17 checks passed on cycle 2 with no code change), so the runtime's own acceptance record exists at the ship commit and the release-cut step 1 is met by it, not by this waiver. The product code is byte-identical to admission 3's.

Follow-ups, updated:
- **TESB-GATE-UNMEASURED-1** — still open (the candidate was not changed for it).
- **PARTITION-DECODE-HARDENING-1** — still open (same).
- **TESB-KAFKA-TRUST-WITNESS** — DONE in admission 6 cycle 2: the kafka `trust` capture is retained; zero-connectivity modules 5/65 → 34/65, `streams/integration-tests` fan-in/out `1/10` → `0/0` with 11 test-only excluded relations and the `--include-tests` action stated.
- **CYCLE-WALK-DETERMINISM-1** (new, from INPUT-6) — a cycle's rendered walk order follows random node UIDs, so two indexes of one commit can print different walks of the same cycle (RG-REQ-001-L09).
- **TS-TEST-RACE-1** (existing) — bit C17's first run in admission 6; operator ruling TESB-C17-PREEXISTING-FLAKE → A recorded in the slice packet.
