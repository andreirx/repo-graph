# IS-TEST-CPP-1 — a gtest file is a test file

Status: SPECIFIED (2026-09-03) · Track: zg-derived queue #3 (human-ratified sequence
2026-09-03). CODE slice, indexer, SMALL. Maturity: MATURE (is_test feeds demotion,
ranking, counts — and SEED-CHUNK-1's seed partition next).

## 1. Problem (MEASURED — docs/audits/2026-09-03-seed-chunk-spike-1.md §4)

C++ has NO is_test basis: leveldb carries `is_test=0` on all 128 files including
`recovery_test.cc` et al. Consequences measured: concept-seed queries drown in test
symbols (both embedding models ranked RecoveryTest 1–4 for "crash recovery");
FIXTURE-POLLUTION demotion is blind on C++; module test counts undercount. The simulated
fix (gtest structural markers) took obsolete-files from rank 2 to rank 1 and recovery
from 49 to 10 in the spike.

## 2. Contract

1. **Basis: structural evidence PROMOTES; absence preserves** (AMENDED 2026-09-03,
   operator ruling ISTESTCPP1-STRUCTURAL-SEMANTICS — the original "iff" wording
   contradicted §2.3 and would have minted false is_test=false facts). A gtest/gmock
   structural marker — include (`<gtest/...>`, `"gtest/...`, `<gmock/...>`) or top-level
   `TEST(`/`TEST_F(`/`TEST_P(`/`TYPED_TEST(` macro — is STRONG positive evidence: the
   file is test. Absence of a marker is WEAK negative evidence and never overrides an
   existing classification (path-convention or otherwise): no marker → keep current
   value. A NAME alone still never promotes (a production `src/parser_test.cc` without a
   marker stays production; the name-trap witness is mandatory). The report must COUNT
   the residual per corpus repo: path-classified test files carrying no marker (known
   residual, recorded honestly, not silently demoted).
2. **Framework scope:** gtest/gmock family only in this slice — the demonstrated corpus
   evidence (leveldb, openxcom, vcmi use it or nothing). Catch2/doctest/CppUnit are named
   FOLLOW-UP candidates in the report if observed in corpus, not built speculatively.
3. Files with no marker keep current classification (no basis → no change); existing
   bases for other languages unchanged; refresh copy-forward preserves the fact.
4. **Downstream movement measured (deep-vertical):** leveldb reindex — `*_test.cc` rows
   is_test=1 in the DB via the marker (name-trap witness proves the name alone does
   nothing); before/after test-file counts for leveldb/openxcom/vcmi; a find run showing
   test symbols now demoted where the demotion already exists. Non-C/C++ repos
   byte-stable.

## 3. Stop conditions

Frozen: storage schema (values change, shape does not), exit codes, demotion semantics
(this slice feeds the existing fact; it does not change consumers). STANDING HONESTY
RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root.
Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing test FIRST: fixture with (a) a gtest-include file, (b) a TEST_F-only file
  (no include in-file), (c) a PRODUCTION file named `foo_test.cc` with no marker, (d) a
  production file mentioning gtest only in a comment/string → pre-fix (a)(b) misclassify;
  post-fix (a)(b) test, (c)(d) production.
- Live proof (isolated state root, registry sha unchanged): leveldb reindex with
  before/after counts; openxcom/vcmi spot-checks; leveldb `find` demotion movement.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

C/C++ files with structural test markers carry is_test=1; unmarked files keep their
existing basis (residual counted and reported, never silently demoted); the name-trap
witness proves a name alone never promotes; measured downstream movement reported; other
languages untouched; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; openxcom at
../legacy-codebases/openxcom; vcmi at ../legacy-codebases/vcmi; repo-graph is THIS repo.
