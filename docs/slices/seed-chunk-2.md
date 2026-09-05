# SEED-CHUNK-2 — seeds rank what an agent wants: implementations, not declarations; production, not tests

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #5 (human-ratified). CODE
slice. Maturity: MATURE.

## 1. Problem (VERIFIED — docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md)

SEED-CHUNK-1 landed the engine; three relevance defects remain:
- **is_test partition INERT on Rust**: 4,578 repo-graph chunks (36.9%) containing a
  literal `#[test]` are stored is_test=0 — the fact is per-FILE (cfg(test) inclusion
  chain), chunks are per-SYMBOL, and in-file `mod tests { #[test] fn … }` lives in
  production files. Partition header never rendered in any capture.
- **Declaration chunks dominate**: ≤2-line chunks are 53% of FRAKTAG's seed corpus, 38%
  vcmi, 32% leveldb; `fr-persist` fills 6/10 slots with one-line interface properties
  while `JsonStorage.write` never appears; `db_impl.h:113` (decl, 2 lines) r2/0.45 beats
  `db_impl.cc:292` (91-line impl) r8/0.30 — the identifier-dominated geometry rewards
  short chunks whose text IS the name.
- **--text referral suppressed where seeds mislead**: `find fsync` returns
  `DBSynchronize` 0.56 and never mentions `--text`; the referral fires only when the seed
  tier is ALSO empty — exactly inverted from need.

## 2. Contract

1. **Per-chunk is_test.** A chunk's test-ness = its file's is_test OR structural
   per-symbol evidence: Rust `#[test]`/`#[cfg(test)]` attribute on the item or an
   enclosing `mod` (walk the parent chain — IS-TEST-RUST-1's inclusion logic applied
   in-file), TS/JS `describe(`/`it(`/`test(` enclosing call, Python `def test_` inside a
   `Test*` class or `test_*.py` — NO: names never (drop the last one; file fact only for
   Python unless a decorator/framework marker exists). Store per chunk (additive column on
   seed_vectors or a sibling fact); render the partition header when both partitions are
   non-empty; unknown never invisible.
2. **Declaration demotion, explicit and labeled** (PRECEDENCE, ratified 2026-09-05: the
   production/test PARTITION is applied first; decl-below-impl holds WITHIN a partition —
   a production declaration renders above a test-partition implementation of the same
   name, still labeled `(decl)`; the test impl is a double, not the answer). A chunk whose span is a declaration
   without a body (prototype, interface member, header decl) ranks BELOW any
   body-bearing chunk of the same qualified name when both are present (impl above decl,
   deterministic tie-break), and renders `(decl)` so the agent knows. No suppression —
   the decl still appears (it may be the only hit), just never above its own
   implementation. Cross-language: C/C++ .h vs .cc, TS `declare`/interface members,
   Rust trait method decls vs impls.
3. **The --text referral renders whenever seeds serve** ("for exact text/comments/
   expressions: `find --text <query>`"), one line, not only on empty tiers.
4. Floor/model/wall unchanged (SEEDCHUNK-FLOOR-2 ruling stands). JSON additive; exit
   codes unchanged. Pre-034 vectors (no per-chunk facts) are REFUSED as classified — and
   the daemon SCHEDULES a re-seed for that repo via the existing SeedCoordinator,
   rendering "seeds re-embedding for per-chunk facts (pending)" meanwhile: an upgrade must
   never leave a repo seedless without saying why and fixing itself.

## 3. Stop conditions

Frozen: the embedding engine/model, the 0.30 floor, the facts wall, storage schema
(additive only), exit codes. STANDING HONESTY RULES (no name-based test classification —
structural markers only; decl/impl determined by span structure, never by filename
extension alone). Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real
state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: in-file `mod tests` chunk → test partition; production chunk in a file with a
  test mod → production; decl-vs-impl ordering with the `(decl)` label; referral line
  present with non-empty seeds.
- Live proof (isolated state root, registry sha unchanged): repo-graph #[test]-chunk
  count in the test partition (expect ≈4,578 → all) — proven with the AFTER binary on ONE
  fresh isolated index + seed pass of repo-graph (~2 min; no before-binary for a count); `find "crash recovery"` on leveldb
  — `db_impl.cc:292` above `db_impl.h:113`; FRAKTAG "where are conversations persisted"
  — REFERRAL EVIDENCE ONLY (AMENDED 2026-09-05, ruling SC2-FRAKTAG-DOD: its dominant
  entries are one-line interface PROPERTIES with no implementation counterpart — outside
  decl demotion; measured unchanged, recorded honestly; short-chunk/property demotion →
  SEED-CHUNK-3); `find fsync` shows the --text
  referral. Before/after verbatim.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Test chunks partition by structural per-chunk evidence on every covered language;
implementations outrank their own declarations with the decl labeled; the --text referral
is always offered beside seeds; the leveldb and repo-graph captures improve as measured
and FRAKTAG's referral renders (its ranking recorded unchanged — SEED-CHUNK-3); pre-034
stores self-heal via a scheduled re-seed; gates green.

CORPUS PATHS: repo-graph is THIS repo; leveldb at ../legacy-codebases/leveldb; FRAKTAG at
../FRAKTAG; vcmi at ../legacy-codebases/vcmi.
