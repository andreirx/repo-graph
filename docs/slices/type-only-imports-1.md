# TYPE-ONLY-IMPORTS-1 — "this cycle vanishes at runtime", per cycle

Status: SPECIFIED (2026-09-03) · Track: CYCLE-FACTS-2 part (c), ratified alone by human
ruling 2026-09-03 (parts a/b optional-unscheduled — see ROADMAP). CODE slice. Maturity:
MATURE (touches storage migrations + cycles serving).

## 1. Problem (VERIFIED — code trace 2026-09-03)

TS `import type` edges are extracted (`ts-extractor/src/extractor.rs:1350` sets
`is_type_only`), carried by the IR (`repo-graph-ir/src/lib.rs:176`), and mapped into
LiveGraph observations (`repo-graph-livegraph/src/lib.rs:1832`) — but the fact NEVER
reaches the SQLite store (zero `is_type_only` references in `crates/storage`), which per
ENGINE-CONSOLIDATION-1 FC2b is the OWNER of cycle facts. So `cycles` can only hedge with a
blanket repo-level caveat (`rgr/src/presentation/cycles/mod.rs:365`: "import edges do not
distinguish `import type` — some cycles may vanish at runtime") instead of telling the
agent WHICH cycle is a compile-time phantom vs a real runtime coupling. That distinction
flips the agent's decision: investigate/fix vs safely ignore.

## 2. Contract

1. **Plumb the existing fact to the owner store.** The per-import `is_type_only`
   observation reaches SQLite via an ADDITIVE migration (new column/fact on import-edge
   rows, established `migrations/` pattern; no reshaping of existing columns). No new
   extraction — the extractor fact at :1350 is the single source.
2. **Aggregation is conjunctive and explicit.** A module-level import edge is type-only
   iff EVERY contributing file-level import observation on that edge is type-only. A cycle
   is type-only iff EVERY edge in its walk is type-only. Per-cycle state is a sum type:
   `TypeOnly | HasRuntimeEdges | Unknown{reason}` (e.g. snapshot predates the fact) —
   unknown VISIBLE, never demoted, never silently treated as runtime.
3. **Output surface (deep-vertical):** `cycles` labels each type-only cycle
   "type-only (vanishes at runtime)". The blanket caveat at cycles/mod.rs:365 is
   NARROWED: rendered only when Unknown cycles exist (naming how many), retired when all
   cycles carry the fact. orient's cycle figure/parenthetical stays consistent via the
   shared computation (ORIENT-CYCLES-DISAGREE-1 precedent — one derivation, cite it).
4. **Route agreement per precedent.** Label computed at the shared serving computation
   where cycle_composition partitions live; if the LiveGraph cache route cannot evaluate
   it, it states the asymmetry honestly (route-conditional additive decoration,
   ORIENT-CYCLES-DISAGREE-1) — it never renders a different verdict.
5. **Scope: TS/JS only** — the only extractors emitting the fact. Other languages' import
   edges are runtime edges by definition (label absent, not Unknown). Refresh
   copy-forward preserves the fact; JSON additive fields only; exit codes unchanged.

## 3. Stop conditions

Frozen: cycle computation and exclusion semantics, the CYCLES-B byte-parity certificate
(the label is OUTSIDE it, per the route-conditional precedent), exit codes. Storage schema
change is RATIFIED but additive-migration only — any non-additive reshaping →
DECISION_REQUIRED. STANDING HONESTY RULES (no unwrap_or defaults on the fact read; a
lookup failure is Unknown{reason}, never HasRuntimeEdges). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing fixture FIRST: TS fixture with (a) a purely type-only cycle (A `import
  type` B, B `import type` A) → labeled type-only; (b) a mixed cycle (one runtime edge)
  → HasRuntimeEdges, NOT labeled; (c) assert the blanket caveat is absent when no
  Unknown exists.
- Live proof (isolated state root, registry sha unchanged): a TS corpus repo (amodx,
  glamCRM) — report cycle labels found (honest zero if the corpus has no type-only
  cycle; the fixture carries the rendering proof); repo-graph/leveldb (non-TS)
  byte-stable.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

An agent reading `cycles` on a TS repo sees per-cycle runtime-vs-type-only truth from the
extractor's existing fact, served from the owner store, agreeing across routes; the
blanket hedge survives only where genuine Unknown remains; gates green.

CORPUS PATHS: amodx at ../amodx; glamCRM at ../glamCRM; leveldb at
../legacy-codebases/leveldb; repo-graph is THIS repo.
