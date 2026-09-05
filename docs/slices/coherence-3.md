# COHERENCE-3 — adjacent commands never disagree about one fact (part 2: walks, headlines, totals)

Status: SPECIFIED (2026-09-05) · Track: v0.16.0 audit queue #6b (split from COHERENCE-2 by
operator ruling 2026-09-05). CODE slice, presentation + shared-computation. Maturity: MATURE.

## 1. Problem (VERIFIED — docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md)

1. **orient asserts cycle walks that `cycles` refuses**: django orient prints `8 import
   cycles (apps -> backends -> backends -> … -> apps)` while `cycles` renders the same SCC
   as `35 modules, members (unordered)`; same on FRAKTAG and OpenXcom. Counts were unified
   (ORIENT-CYCLES-DISAGREE-1); the WALK text is still a second derivation.
2. **Test-only exclusion reached `cycles` but not `orient`/`surfaces`**: vscode orient
   says `69 import cycles (+5 test-only excluded)` yet headlines `99 HTTP surfaces (9
   providers)` where 6 of 9 providers are test fixtures; storybook's sole provider is a
   test fixture, unlabeled.
3. **Five unreconciled file totals**: orient / check / stats / modules-list sum / map
   (grpc-java 1909/1627/1627/1627/1821; zvec-grep 239/235/234/133) — no output states the
   convention.

## 2. Contract

1. **One walk derivation.** orient's cycle parenthetical renders EXACTLY what `cycles`
   would: a real walk only when the shared cycle computation yields an ordered walk;
   otherwise the unordered form ("largest: 35 modules — rmap cycles"). No second
   derivation; a seam test makes disagreement unrepresentable (extend the
   ORIENT-CYCLES-DISAGREE-1 seam from counts to walks).
2. **Test-only partition on every headline that counts**: surfaces/boundaries provider and
   consumer headlines use the stored `is_test` fact exactly as cycles does
   (`N surfaces (+M test-fixture excluded)`), fixtures labeled in lists; unknown never
   invisible.
3. **File totals state their basis.** Each command printing a file total names its basis
   in one clause ("977 files indexed" / "926 with symbols" / "1821 mapped — excludes
   generated"); where two commands claim the same basis they must agree (seam test).
4. JSON additive; exit codes unchanged; certificates (CYCLES-B) untouched.

## 3. Stop conditions

Frozen: cycle computation and exclusion semantics, the CYCLES-B certificate, storage
schema, exit codes, ranking. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED.
Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Seam tests: orient-walk == cycles-walk (ordered and unordered); surfaces headline
  partition; same-basis totals agree.
- Live proof (isolated state root, registry sha unchanged): django + FRAKTAG orient vs
  cycles walks; vscode surfaces headline with fixture exclusion (retained read-only root);
  grpc-java / zvec-grep totals with stated bases. Before/after verbatim. Gates first,
  proofs scoped small.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No two commands state a different cycle walk, surface count, or file total for one
snapshot without naming a different basis; gates green.

CORPUS PATHS: django at ../legacy-codebases/django; FRAKTAG at ../FRAKTAG; vscode at
../legacy-codebases/vscode; grpc-java at ../legacy-codebases/grpc-java; zvec-grep at
../legacy-codebases/zvec-grep; repo-graph is THIS repo.
