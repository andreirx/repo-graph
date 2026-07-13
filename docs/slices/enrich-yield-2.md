# ENRICH-YIELD-2 — implement the ratified yield levers (A: Layer-2 projection, B: primitives, D: enums)

Status: SPECIFIED (2026-07-12) · Track: Resolution & attribution
Origin: ENRICH-YIELD-1 measured funnel (7,582 resolved → 251 promoted, 3.3%) + its
decision-review ratification packet; **EY1-A/B/D ratified by the operator 2026-07-12 as the
CORRECTED cells** (`.agent-manager/slices/ENRICH-YIELD-1/ratification-packet.md` — the
corrected cells are BINDING; the original recommendations are superseded). EY1-C (Rust
receiver-locator) is deliberately split out as ENRICH-YIELD-3 (new resolver seam).

## 1. Contract (three ratified items, each with its funnel-visible proof)

1. **EY1-A — Layer-2 read projection for likely-external receiver calls (~36% of
   rejections).** A read projection over the ALREADY-PERSISTED unresolved-edge
   `metadata_json` (receiver_type, is_external_type, origin) — NO new persisted inference
   shape. Reader label: "call on likely-external receiver `<T>`". Basis fields SEPARATE and
   heuristic-honest: receiver type = "LSP hover-derived (heuristic parse)"; external =
   "static name-set heuristic (STD_TYPES membership), not compiler-verified". Never a
   Layer-0 edge. Surface: wherever unresolved-call attribution renders (the callers/trust
   read paths that consume unresolved-edge classification today) — builder inventories the
   consuming surfaces and lands the projection on the natural one(s); the choice is
   recorded, least-new-surface wins.
2. **EY1-B (SAFE half only) — primitive reattribution.** Primitive receiver types (`str`,
   `usize`, …: the PRIMITIVES set) classify as external at gate 4 instead of passing to
   fail gate 5. DETERMINISTIC and promotion-neutral (a primitive is never an in-repo
   class). The dependency-name half stays BLOCKED (needs qualified-path provenance; the
   resolver discards it — do NOT attempt manifest-name guessing).
3. **EY1-D — Rust enum widening.** Preserve the Enum subtype across the extractor→enrichment
   boundary (today Enum collapses to Other in SymbolSubtype parsing) and widen gate 5's
   usable-type predicate to Class|Enum. Unambiguous enums with valid methods become REAL
   Layer-0 promotions (gates 6-8 still apply). Genuine ambiguity (multiple-class,
   overloaded-method) and method_not_found stay rejected — ratified.

## 2. Stop conditions

- EY1-B must be provably promotion-neutral: the funnel's promoted set BEFORE == AFTER on
  the self-index (only attribution moves between gates). If any promoted edge flips → STOP.
- EY1-D changes promotion behavior BY DESIGN (new enum promotions) — every NEW promoted
  edge must be a Rust enum method call; cite examples in the report.
- No dependency-name external classification (BLOCKED half). No `self.field.method`
  admission (ENRICH-YIELD-3). No schema migration; the Layer-2 projection is read-side.
- Do NOT commit.

## 3. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/` (build / workspace test with the documented environmental
  exclusion / fmt / clippy).
- Named tests per item: projection renders with both basis fields; primitives land at gate
  4 with conservation intact; Enum subtype survives the boundary; gate 5 accepts a
  fixture enum method (and still rejects ambiguous classes); funnel BEFORE/AFTER
  comparison test for EY1-B neutrality.
- Isolated self-dogfood (/private/tmp + stdio; NEVER the real registry): index repo-graph,
  auto-enrichment, inline the funnel BEFORE (from EY1's delivery record: gate-4 ~36%,
  gate-5 ~26%, 251 promoted) vs AFTER: primitives moved to gate 4; enum promotions
  appeared (cited); the Layer-2 projection rendering with real examples. THE FUNNEL IS THE
  PROOF SURFACE.

## 4. Definition of done

The three ratified levers are live and visible: funnel attribution is accurate for
primitives, Rust enum calls promote (cited), and the ~36% external class renders as honest
Layer-2 orientation — with the measured funnel delta inlined.

---

## 5. Delivery record (2026-07-13)

**DELIVERED** (`a5728f4`, relay-approved iteration 4 — 5 cycles, two mid-run operator
ratifications). EY1-A: Layer-2 likely-external projection on the trust surface with
separate heuristic-honest basis lines. EY1-B: primitives external at the resolver seam;
neutrality proven by deterministic replay of the REAL captured self-index corpus
(committed: `capture_ey1b_corpus.py` + `ey1b_selfindex_corpus.json`) — promoted sets
exactly equal, movement = gate 5-or-8 → 4 only for primitives. EY1-D: Enum preserved
across the boundary, gate 5 = Class|Enum via the shared predicate; 15 live enum-owned
promotions. Ratifications during the run: **EY2-B-PROOF** (deterministic identical-corpus
replay mandatory — a live-run comparison cannot isolate EY1-B), **EY2-D-CALLABLE**
("enum method call" = enum-owned callable incl. associated functions), **EY2-B-GATE8**
(the observed broader invariant: deep-chain primitives move 8 → 4, not only 5 → 4;
promoted-set equality is the binding fact). Review history: seam correction (review-0:
primitive list belonged in the Rust resolver, not language-agnostic promotion), synthetic
corpus rejected (review-2: the replay must use the REAL corpus), wording-vs-reality
escalation (review-3: resolved by ratifying what the proof measured).
Next: ENRICH-YIELD-3 (Rust receiver-expression locator; spec committed).
