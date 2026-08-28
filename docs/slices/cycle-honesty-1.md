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

1. **Additive `edges` on the cycle DTO — only where honestly carried** (operator ruling
   module-cycle-edge-contract = A1, 2026-08-28: the LiveGraph fastpath's no-loss certificate
   proves member-set equality, NOT edge equality, so the default route must not claim edges).
   The SQLite serving path returns, per cycle, the **real intra-SCC directed edges**
   (`from_node_id`, `to_node_id`) as a new optional field; the LiveGraph route OMITS the field
   (an absent optional field is honest). `nodes`, `count`, JSON shape otherwise unchanged.
   Cap edges per cycle at a sane bound (e.g. 200) with an explicit `edges_truncated: true`
   marker — never silent truncation. No change to the certificate, witness, or
   backend-independence contract (extending certification to edge sets is the follow-up, §6).
2. **The human render draws only real arrows.** With verified edges present: render an actual
   cycle walk found by DFS over them (every SCC contains one); members not on the displayed
   walk render as `+ N more members in this cycle`. Without edges (LiveGraph route, older
   daemon reply, or truncated): render `members (unordered): A, B, C` — NO arrows. An arrow
   may only ever appear between nodes with a verified edge.
3. **Test-only labeling — DEFERRED (operator ruling test-label-data-path = B3, 2026-08-28).**
   The STOP path fired: `is_test` does not exist in the LiveGraph IR (VERIFIED review-0), so
   fact-based labels on the default route are impossible without projecting the fact into
   IR/warm-cache/witness (human-class blast radius), and SQLite-only labels would be a
   mostly-dormant, engine-asymmetric capability. Labels move to the follow-up (§6). Never
   label from names.
4. **Type-only honesty by caveat, not by data we don't have** (operator ruling ts-caveat-basis
   = C1 at REPO level, 2026-08-28). When the repo's stored language facts show material TS/JS
   presence AND at least one cycle renders, one footer line: "this repo contains
   TypeScript/JavaScript; import edges do not distinguish `import type` — some cycles may
   vanish at runtime". Repo-scoped truthful; no per-cycle claim, no new aggregation.
   Recording type-only at extraction is part of the follow-up (§6).
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
  (unordered)" with zero arrows; truncation marker; TS caveat line appears iff the repo's
  language facts show material TS/JS and cycles render.
- Live proof (isolated state roots, registry sha256 unchanged): leveldb and django — the
  audit's grep check re-run on the new output: **every rendered arrow corresponds to a real
  import** (grep/`rmap imports` cross-check in the report); glamCRM or FRAKTAG for the TS
  caveat.
- Chunked cargo gates; consolidation witness 15/15 (+ new arm declared if a dispatch arm is
  added); `./scripts/dogfood-isolated.sh` green.

## 5. Definition of done

No rendered arrow without a verified edge anywhere in cycles output; unordered fallback is
explicit; TS repos carry the type-only caveat; the follow-up (§6) is recorded; gates green.

## 6. Named follow-up — CYCLE-FACTS-2 (human ratification required; NOT this slice)

One slice, three coupled extensions the frozen surfaces blocked here (review-0 evidence):
(a) extend the LiveGraph no-loss certificate + witness to intra-SCC **edge-set equality** so
real walks render on every backend (was A2); (b) project `is_test` into the LiveGraph IR so
test-only/mixed cycle labels are fact-based and engine-symmetric (was B2/B3 deferral);
(c) record **type-only** import status at extraction (extractor + storage fact) so phantom TS
cycles are labeled per-cycle instead of caveated per-repo. All three touch witness/IR/schema —
human-class blast radius; surface as one DECISION_REQUIRED packet when queue #7-#10 are done.
