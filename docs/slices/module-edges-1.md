# MODULE-EDGES-1 — the dependency story gets told, not counted

Status: SPECIFIED (2026-09-02) · Track: diverse-verification queue (the VISION acid-test
gap). CODE slice, presentation. Maturity: MATURE (modules).

## 1. Problem (measured — VCMI)

`modules list` says "14 cross-module dependencies detected" — a bare count. The verified
facts behind it (client→lib, server→lib — VCMI's real architecture) render NOWHERE by
default: `boundaries` returns zero (different fact class), and the story survives only in
stats' fan_in/instability numbers an agent must decode. The VISION's primary use case —
"how modules relate to each other / where the boundaries and seams are" — is the one story
the default surfaces never tell directly.

## 2. Contract

1. **modules-list renders the edge list** under the count line: one row per cross-module
   edge, `client → lib (N file-level imports)`, sorted by reference count DESC then names
   ASC, with the house budget (cap + explicit "(+N more — --full)"). The data is the SAME
   module dependency graph the count already comes from (module-queries) — no new
   computation, no new fact class; the count and the listed edges must come from ONE read
   (never a count that disagrees with its own list).
2. **Zero-state unchanged** (modules-list's existing honest zero handling stays; repo-graph's
   "no cross-module dependencies" pre-enrich thinness is DEPS-SELF-1/structural territory —
   out of scope here).
3. **orient's module section gets the top edges** (the first-60-seconds surface): the top 3
   cross-module edges by reference count, one line, only when they exist — budget-honest,
   nothing on repos without them. (Deep-vertical rule: the capability must be visible where
   agents actually look.)
4. JSON additive (edges array on the existing response); exit codes unchanged.

## 3. Stop conditions

Frozen: module identity/ownership/dependency computation (render what is computed), storage
schema, exit codes, trust. STANDING HONESTY RULES. New public APIs beyond additive DTO
fields → DECISION_REQUIRED (read-only accessor precedent chain citable). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: edge rows from the same read as the count (a disagreement is impossible by
  construction — test the single-read seam); sort determinism; budget remainder; orient
  top-3 only-when-present.
- Live proof (isolated state root, registry sha unchanged): VCMI — modules list shows the
  named edges (client → lib, server → lib among them; spot-verify one against real
  includes); orient carries the top-3 line; glamCRM/leveldb spot-checks (their cross-module
  edges render; no regression elsewhere). Captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

An agent reading `modules list` (or just orient) on VCMI sees client→lib and server→lib as
named, counted edges — the architecture story told directly; the count can never disagree
with its list; budgets honest; gates green.
