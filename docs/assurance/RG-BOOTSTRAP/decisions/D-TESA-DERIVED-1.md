# D-TESA-DERIVED-1 — UNDETERMINED test status is computed when asked, from the path and the existing test flag; it is not stored

Raised: 2026-09-29 by the manager, after three document cycles of TEST-EDGE-SCOPE-1A-PREP-2 (reviewer codex gpt-6-sol).

The manager's packet required UNDETERMINED test status (RG-REQ-001-L07) to be a STORED fact, so the author recorded the undetermined set per snapshot (`snapshots.extraction_diagnostics_json`). But the flag it sits beside, `files.is_test`, is stored per repository and overwritten by each index (`storage/src/crud/files.rs:49–59`). Review-2 showed two consequences:
- **Stale counts.** An older snapshot, still served, can report a count over a universe the flag has since changed ("1 of 0"), or drop the file from its partition.
- **A transient unknown.** A snapshot is marked Ready (`indexer/src/orchestrator.rs:1234–1260`) before the recording postpass, so it can answer "not recorded".

Resolved: 2026-09-29 by the HUMAN, option "Compute when asked", chosen from three options presented self-contained: compute when asked; keep it stored and make the test flag snapshot-scoped; freeze with a caveat.

## Ruling
- **One shared function.** A tracked file is test status UNDETERMINED when all three hold:
  - its path contains one of L07's test words as a token (test, tests, testing, tester, testutil; case-insensitive; separated by `/`, `.`, `_`, `-` or a CamelCase boundary);
  - it matches none of L07's conventions (`__tests__`, `.test.`, `.spec.`, `test/`, `tests/`, `testsuite/`) and no marker;
  - its `is_test` is 0.
- Every surface that partitions by test status calls it on the same file list and `is_test` values it already reads, so a count can never disagree with its partition.
- Files with no FILE node are covered, because the function works on tracked paths.
- It survives a no-change refresh as L07 requires, because it is a pure function of the path and the flag.
- Nothing new is stored; there is no recording postpass and no Ready-to-recorded window.
- `testsuite/` is still a routing convention that sets `is_test` at index time. That part is unchanged.

## Consequences
- The document drops the stored carrier and its checks. The consistency tests become: for each partitioned surface, the count equals the number of files in its partition for which the function holds, for the snapshot served.
- The indexer version still moves, because `testsuite/` changes stored `is_test`. The undetermined status itself needs no re-index.
- A future change to the token rule re-derives old snapshots with the new rule. A version bump re-indexes them anyway.
