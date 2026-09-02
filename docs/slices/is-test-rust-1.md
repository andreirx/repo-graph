# IS-TEST-RUST-1 — a file included under #[cfg(test)] is a test file

Status: SPECIFIED (2026-09-02) · Track: queue tail (verified fact gap, 2026-09-02). CODE
slice, indexer. Maturity: MATURE (is_test feeds demotion, ranking, counts).

## 1. Problem (VERIFIED — self-index DB query)

Rust in-crate test modules carry `is_test = 0`: `src/**/tests.rs`,
`src/explain_serve_tests/*`, `src/callgraph_cert/tests.rs` — all production-classified.
Consequences already measured: find ranks test symbols in the non-test partition
(`witness_epoch` from a tests module at rank 2); FIXTURE-POLLUTION demotion cannot see
them; module test counts undercount.

## 2. Contract

1. **Basis: the #[cfg(test)] inclusion chain — structural evidence, never filenames.** A
   Rust file is test iff its module inclusion is gated by `#[cfg(test)]` (the `mod x;`
   declaration in the including file carries the attribute, directly or via an enclosing
   `#[cfg(test)] mod`), transitively for nested inclusions. `tests.rs`/`*_tests` as NAMES
   are NOT evidence (a production module may be named tests-adjacent; a cfg(test) module
   may be named anything). Existing bases for other languages unchanged.
2. **Files unreachable from any inclusion** (not declared by any `mod`): keep current
   classification (no basis → no change), never guess.
3. **Expected downstream movement is the point** (deep-vertical): repo-graph self-index —
   find's symbol ranking demotes in-crate test symbols; test counts rise; test-only
   demotion sees them. Record before/after counts in the report. Other-language repos
   byte-stable.
4. Refresh copy-forward preserves the classification consistently (same as TS-LINGUIST-1's
   handling); JSON unchanged (the fact's VALUES change — that is the fix); exit codes
   unchanged.

## 3. Stop conditions

Frozen: storage schema (values change, shape does not), exit codes, trust computation
semantics. STANDING HONESTY RULES. If the inclusion-chain resolution needs whole-crate mod
graph machinery the indexer lacks, STOP + DECISION_REQUIRED with the smallest-mechanism
options (do not build a resolver speculatively). Unmet DoD → STOP + DECISION_REQUIRED.
Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing test FIRST: fixture crate with (a) `#[cfg(test)] mod tests;` file, (b) a
  nested include under it, (c) a PRODUCTION module named `tests_util.rs` included without
  cfg(test), (d) an undeclared stray file → pre-fix (a)(b) misclassify (FAILS); post-fix
  (a)(b) test, (c) production (the name-trap witness), (d) unchanged.
- Live proof (isolated state root, registry sha unchanged): repo-graph self-index —
  `src/**/tests.rs` rows is_test=1 in the DB; `find witness` no longer ranks
  `witness_epoch` in the non-test partition; before/after test-file counts. leveldb/django
  byte-parity spot-check.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Rust test-module files carry the true fact via structural evidence; the name-trap witness
proves no filename classification; downstream surfaces move as measured; other languages
untouched; gates green.
