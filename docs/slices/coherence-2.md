# COHERENCE-2 — adjacent commands never disagree about one fact

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #6 (human-ratified; codex
adjudication elevated cross-surface inconsistency to its own defect class: "inconsistent
answers from adjacent commands destroy calibration"). CODE slice, presentation +
shared-computation. Maturity: MATURE.

## 1. Problem (VERIFIED — docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md)

1. **orient asserts cycle walks that `cycles` refuses**: django orient prints `8 import
   cycles (apps -> backends -> backends -> … -> apps)` (a self-edge nonsense) while
   `cycles` renders the same SCC as `35 modules, members (unordered)`. Same on FRAKTAG and
   OpenXcom. ORIENT-CYCLES-DISAGREE-1 unified the COUNTS; the WALK text is still a second
   derivation.
2. **type-only predicate ALL→ANY**: a cycle vanishes at runtime if ANY edge is erased;
   the TYPE-ONLY-IMPORTS-1 contract labeled only all-type-only cycles (2/118 labeled; 5
   verified false negatives, e.g. FRAKTAG's sole cycle with one `import type` edge).
3. **Test-only exclusion reached `cycles` but not `orient`/`surfaces`**: vscode orient
   says `69 import cycles (+5 test-only excluded)` yet headlines `99 HTTP surfaces (9
   providers)` where 6 of 9 providers are test fixtures; storybook's sole provider is
   `test-storybooks/…/server.js`, unlabeled.
4. **`(N test)` means opposite things**: `stats` renders a SUBSET (`files=3014 (2000
   test)`), `modules list` an ADDEND (`907 files (1997 test)` = 2904) — same module, two
   sizes across commands; vscode `extensions 0 files (107 test)`.
5. **Five unreconciled file totals**: orient / check / stats / modules-list sum / map
   (grpc-java 1909/1627/1627/1627/1821; zvec-grep 239/235/234/133) — no output states the
   convention.

## 2. Contract

1. **One walk derivation.** orient's cycle parenthetical renders EXACTLY what `cycles`
   would: a real walk only when the shared cycle computation yields an ordered walk;
   otherwise the unordered form ("largest: 35 modules — rmap cycles"). No second
   derivation; a seam test makes disagreement unrepresentable (ORIENT-CYCLES-DISAGREE-1
   precedent — extend it from counts to walks).
2. **Type-only becomes three states** (additive, TYPE-ONLY-IMPORTS-1's `CycleTypeOnly`
   extended): `TypeOnly` (all edges — "vanishes at runtime"), `BreaksAtRuntime{type_only:
   k, of: n}` (any edge — "broken at runtime: k of n edges are type-only; residual one-way
   coupling remains"), `HasRuntimeEdges`, `Unknown{reason}`. Exhaustive matches; both
   cycles and orient render the same state. Labels state the residual truthfully — a
   one-erased-edge cycle is not "gone", it is a chain.
3. **Test-only partition on every headline that counts**: surfaces/boundaries provider
   and consumer headlines use the stored `is_test` fact exactly as cycles does
   (`N surfaces (+M test-fixture excluded)`), fixtures labeled in lists; unknown never
   invisible.
4. **One `(N test)` semantics, product-wide**: SUBSET everywhere (`files=3014 (2000
   test)` means 2000 of 3014). Every renderer that prints the form is enumerated and
   converted; a renderer test pins the meaning; `modules list`'s addend rendering is a
   defect and its sums must reconcile with `check`.
5. **File totals state their basis.** Each command that prints a file total names its
   basis in one clause ("977 files indexed" / "926 with symbols" / "1821 mapped —
   excludes generated") so five numbers read as five bases, not five errors; where two
   commands claim the same basis they must agree (seam test).
6. JSON additive; exit codes unchanged; certificates (CYCLES-B) untouched — labels and
   walks are decorations on the shared computation, per the route-conditional precedent.

## 3. Stop conditions

Frozen: cycle computation and exclusion semantics, the CYCLES-B certificate, storage
schema, exit codes, ranking. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED.
Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Seam tests: orient-walk == cycles-walk (ordered and unordered cases); three-state
  type-only on fixtures (all/any/none); surfaces headline partition; `(N test)` subset
  pinned per renderer; same-basis totals agree.
- Live proof (isolated state root, registry sha unchanged): django + FRAKTAG orient vs
  cycles walks; FRAKTAG's cycle labeled "broken at runtime (1 of 2)"; vscode surfaces
  headline with fixture exclusion; django modules list vs stats vs check totals
  reconciled with stated bases. Before/after verbatim.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No two commands state different facts about one cycle, one module size, one surface
count, or one file total without naming a different basis; type-only renders the runtime
truth in three states; gates green.

CORPUS PATHS: django at ../legacy-codebases/django; FRAKTAG at ../FRAKTAG; vscode at
../legacy-codebases/vscode; grpc-java at ../legacy-codebases/grpc-java; zvec-grep at
../legacy-codebases/zvec-grep; repo-graph is THIS repo.
