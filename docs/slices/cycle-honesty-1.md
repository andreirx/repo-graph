# CYCLE-HONESTY-1 — cycles render real edges or say "members (unordered)"; test-only cycles labeled

Status: SPECIFIED (2026-08-28) · Track: Usefulness audit v0.9.0 fix queue, item #7. CODE slice.
Maturity: MATURE (cycles output contract shipped as CYCLES-OUTPUT-CONTRACT-1).

## 1. Problem (measured — audit `docs/audits/2026-08-26-per-command-usefulness-v0.9.0.md` §7)

1. **Fake paths.** `render_cycle_chain` (`rgr/src/presentation/cycles.rs:189`) draws
   `A -> B -> C -> A` from `Cycle.nodes` — but the DTO carries **no edges** (verified: `Cycle`
   is `{nodes}` only) and the node order is the SCC member enumeration, not a walk. The audit
   grep-verified on leveldb and django that rendered arrows include **nonexistent import edges**.
   An arrow claims "X imports Y"; drawing one that does not exist is a fabricated fact.
2. **Test-only cycles unlabeled.** A cycle entirely inside test modules reads with the same
   severity as a production cycle; the reader cannot tell without leaving the tool.
3. **Type-only phantom cycles (TS).** `import type` edges compile away yet create cycles.
   VERIFIED 2026-08-28: no `type_only` fact exists anywhere in storage or repo-index — the
   extractor does not record it. Labeling per-cycle is therefore NOT possible in this slice.

## 2. Contract

1. **Additive `edges` on the cycle DTO.** The daemon's cycles computation (Tarjan — the SCC
   already knows its intra-SCC edges) returns, per cycle, the **real intra-SCC directed edges**
   (`from_node_id`, `to_node_id`), additively (new optional field; `nodes`, `count`, JSON shape
   otherwise unchanged — CI-facing consumers keep working). Cap edges per cycle at a sane bound
   (e.g. 200) with an explicit `edges_truncated: true` marker — never silent truncation.
2. **The human render draws only real arrows.** With edges present: render an actual cycle walk
   found by DFS over the real edges (every SCC contains one); members not on the displayed walk
   render as `+ N more members in this cycle`. Without edges (older daemon reply, or truncated):
   render `members (unordered): A, B, C` — NO arrows. An arrow may only ever appear between
   nodes with a verified edge.
3. **Test-only labeling.** A cycle whose member modules are ALL test modules renders with a
   `(test-only)` label; a mixed cycle names its test members (`includes test modules: X, Y`).
   Basis: the files' existing `is_test` fact aggregated per member module — a module is a test
   module iff ALL its files are `is_test` (conservative; a module with any production file is
   production). If per-module test composition is not cheaply queryable at the cycles
   computation site, STOP + DECISION_REQUIRED (do not proxy from path names — the standing
   no-name-classification rule).
4. **Type-only honesty by caveat, not by data we don't have.** On repos where TS/JS files
   participate in any rendered cycle, one footer line: "import edges do not distinguish
   `import type` (type-only imports may create cycles that vanish at runtime)". No per-cycle
   claim. Recording type-only at extraction is a NAMED FOLLOW-UP (extractor + storage fact —
   its own slice, human-ratified), not this slice.
5. JSON: new fields additive only; human render changes are the point. Contract doc
   (`docs/architecture/` cycles section / CYCLES-OUTPUT-CONTRACT-1 doc) updated in-slice.

## 3. Stop conditions

Frozen: Tarjan/SCC computation semantics (which cycles exist — this slice changes what is
*said*, not what is *found*), exit codes, storage schema, trust, LiveGraph/witness. New PUBLIC
API surfaces beyond the additive DTO field → DECISION_REQUIRED. STANDING HONESTY RULES apply
(unknown-with-reason on every fallible read that renders; no classification from names).
Unmet DoD → STOP + DECISION_REQUIRED, never a debt note. Never touch the operator's real state
root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: DFS walk renders only real edges (fixture SCC where alphabetical order is NOT a walk —
  the old renderer's output would be provably fake); no-edges fallback renders "members
  (unordered)" with zero arrows; truncation marker; test-only and mixed labeling; TS caveat
  line appears iff TS/JS members are present in rendered cycles.
- Live proof (isolated state roots, registry sha256 unchanged): leveldb and django — the
  audit's grep check re-run on the new output: **every rendered arrow corresponds to a real
  import** (grep/`rmap imports` cross-check in the report); glamCRM or FRAKTAG for the TS
  caveat; a test-only cycle capture if one exists in the corpus (else say so — do not
  manufacture one).
- Chunked cargo gates; consolidation witness 15/15 (+ new arm declared if a dispatch arm is
  added); `./scripts/dogfood-isolated.sh` green.

## 5. Definition of done

No rendered arrow without a verified edge anywhere in cycles output; unordered fallback is
explicit; test-only cycles are labeled from the is_test fact (or the slice STOPPED with the
decision); TS repos carry the type-only caveat; the type-only *fact* is a recorded follow-up;
gates green.
