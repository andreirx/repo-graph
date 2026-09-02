# ORIENT-CYCLES-DISAGREE-1 — one cycle count, everywhere

Status: SPECIFIED (2026-09-02) · Track: v0.15.0 audit queue #1 (verified post-settle). CODE
slice, small. Maturity: MATURE.

## 1. Problem (VERIFIED post-settle, repo-graph self-index)

orient headlines "2 import cycles (core -> graph -> core)" while `cycles` says "1
module-level cycle found (+1 test-only cycle (excluded from the headline))". orient's
figure predates FIXTURE-POLLUTION-1's test-only exclusion — two commands, two headline
counts for one fact. (orient's parenthetical also names a walk; verify it comes from a real
edge source, not a stale summary string.)

## 2. Contract

1. orient's cycle figure comes from the SAME exclusion-aware computation cycles' headline
   uses (one read/derivation — cite where; never a second count): production count
   headline, "+N test-only excluded" appended when nonzero (orient's compact form may
   abbreviate, but the NUMBERS must be the same numbers).
2. Any walk/parenthetical orient renders obeys CYCLE-HONESTY-1 (real edges or nothing).
3. JSON additive if a field is needed; exit codes unchanged.

## 3. Stop conditions

Frozen: cycle computation, the exclusion semantics (FIXTURE-POLLUTION-1), exit codes,
storage schema. STANDING HONESTY RULES. New public APIs beyond additive DTO fields →
DECISION_REQUIRED (precedent chain citable). Unmet DoD → STOP + DECISION_REQUIRED. Never
touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: orient and cycles headline figures from one source (seam test making disagreement
  unrepresentable); test-only-present and none-present shapes.
- Live proof (isolated state root, registry sha unchanged): repo-graph self-index — orient
  and cycles agree ("1 … +1 test-only"); a no-test-cycle repo (leveldb) unchanged.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No two surfaces state different cycle counts for one snapshot; the shared source is cited;
gates green.

CORPUS PATHS: repo-graph is THIS repo; leveldb at ../legacy-codebases/leveldb.
