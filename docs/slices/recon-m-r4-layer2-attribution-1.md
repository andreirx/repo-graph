# RECON-M-R4-LAYER2-ATTRIBUTION-1 — Layer-2 landing on attribution/explain (reconciliation IMPL milestone M-R4)

Status: SPECIFIED (2026-07-19) · Track: Reconciliation IMPL (recon-design-1 §6.1, ratified §8) — the LAST arc milestone.
Depends: M-R1 (c0e1dad), M-R3a (109cf3b). Reads the ledger; independent of the M-R2 serving flag.

## 1. Contract — the recon-design-1 §6.1 M-R4 row + §5.5, binding

1. **Layer-2 annotation:** for an unresolved SITE (an `unresolved_edges` row — the ratified
   floor) where the ledger holds a `semantic` edge from the SAME caller key whose callee NAME
   corresponds to the site's target expression head (the §5.5 structurally-precise join —
   shared detection, not fuzzy correlation): attribution/explain land the labeled annotation
   "this call likely resolves to `X` (the compiler resolved a same-named call in this
   function; syntax resolution could not confirm)" — basis + provenance named (witness S +
   name correspondence). The NAME GUARD per §5.5: the correspondence is on the expression
   HEAD name, exact — no stemming/fuzzing.
2. **Contested-resolution signal:** the same join reversed — P resolved a site to project
   target A, the ledger holds a `semantic` edge binding a same-named site in the same caller
   to project target B → the attribution surface shows the labeled contradiction hint
   ("syntax and compiler resolutions disagree here"). Honest scope: fires only when S's
   competing binding is a PROJECT symbol (external bindings are dropped at ingest and
   surface via their divergence class instead).
3. **Denominator untouched:** Layer-2 annotations NEVER change the trust ratio, the
   unresolved count, or any resolution-rate input — they are labeled hints on the
   attribution/explain surfaces, additive only (§5.5 title: "denominator untouched").
4. **Labels rule:** every annotation's wording audited against the design's labels rule —
   basis named, certainty distinct ("likely resolves" ≠ "resolves"), never implying
   pipeline confirmation.

## 2. Gate — the M-R4 row's gate column

Denominator-invariance test (attribution/trust/check outputs' unresolved + ratio inputs
byte-identical with Layer-2 present); ambiguity-refusal test (MULTIPLE same-named semantic
candidates in the caller → NO annotation — refusal, not a guess); label wording audited
against the labels rule (recorded in the report); W-BOTH/ledger-eligibility scoping (no
annotations from stale/ineligible partitions); R-0 absence; canonical smoke + chunked cargo
gates + consolidation witness green.

## 3. Stop conditions

Frozen: the unresolved_edges floor semantics (FC3 — read-only), trust ratio inputs, union
serving, storage writes, livegraph feed/refresh, extractors/postpass. Ambiguity → refusal,
never a pick. Do NOT commit. Witness green; manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate; chunked cargo gates; witness 15/15; canonical smoke with provenance; isolated
dogfood; live E2E on a fixture with a genuine pipeline-unresolved/SCIP-resolved site (the
amodx `Toolbar → cn(...)` class informs the fixture) showing the annotation rendered with
basis + provenance.

## 5. Definition of done

Attribution/explain surface Layer-2 hints and contested-resolution signals per §5.5, name-
guarded, ambiguity-refusing, provenance-labeled; every denominator and ratio input
byte-unchanged; gates green. The reconciliation IMPL arc is code-complete (default flip
remains gated on S-1..S-3).
