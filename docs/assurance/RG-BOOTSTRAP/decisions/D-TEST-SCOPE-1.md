# D-TEST-SCOPE-1 — Are module edges partitioned by the importing file's test status?

Raised: 2026-09-23 (audit round seven RC-3): poco's `Foundation → CppUnit (415)` comes solely from 281 files under `Foundation/testsuite/`; leveldb's `db → table → db` cycle is closed only by `table/table_test.cc:11-13`. `files.is_test` is a stored structural fact (RG-REQ-001-L07) that the module-edge derivation (`storage/src/trust_impl.rs:755-865`) never reads. RG-REQ-004 had no test-scope statement.
Resolved: 2026-09-23 by the HUMAN — option A.

## Options
- **A (ratified)** — partition by default; state the test-only remainder with a flag. Reward: the architecture picture is the library's; cycles that exist only through tests leave the default answer; nothing is hidden. Risk: six surfaces change at once; pinned numbers in older records go stale (history, stated in the ship block).
- **B** — label only. Reward: nothing disappears. Risk: `cycles` still reports the fake cycle; fan-in and connectivity stay inflated.
- **C** — leave it. Reward: no surface changes, no stale pinned numbers, no reindex. Risk: the audit judge's D4 — an agent keeps reading test scaffolding as architecture.

## Resolution
RG-REQ-004-L12. Queue: TEST-EDGE-SCOPE-1 (+ IS-TEST-CPPUNIT-1).
