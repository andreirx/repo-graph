# RECON-M-R1-WITNESS-LEDGER-1 — the witness ledger (reconciliation IMPL milestone M-R1)

Status: SPECIFIED (2026-07-18) · Track: Reconciliation IMPL (recon-design-1 §6.1, ratified §8
all seven decisions, commit 8241ff5). Ordering: M-R1 ≺ everything in the arc.

## 1. Contract — the recon-design-1 §6.1 **M-R1 row IS the binding contract**, verbatim

Generalize the callgraph-cert compare into the full-walk witness ledger. The row specifies:
divergence classes (§3.1/§3.3/§3.6, DUAL-MEASURED only), kind-alignment rule (c), instance-level
rule (d) with the multiplicity sub-classes, W-BOTH-eligibility scoping rule (e) keyed by the
fingerprint, per-language×partition rollups, the identity_suspect guard, the R-RAT-4 collision
guard (key→sources SET semantics), the GREEN/RED verdict DERIVED from the ledger (behavior
byte-unchanged), the §3.7-2/§3.7-5 doc fixes, and the recorded per-kind classification of the
fixture's SCIP-only edges. In-memory only — NO persisted family (D-R8; Persistence Completeness
N/A by design).

## 2. Gate — the M-R1 row's gate column, verbatim (highlights)

Ledger reproduces the spike's 7/0/2/9 canonical classification on the committed fixture AND the
amodx retained-artifact classification kind-aligned (both 494 / syntactic 13 / unmeasured 24 /
semantic_only_calls 48 / union 579 / agreement 97.4% / S kinds 542 Calls + 12,189 References /
suspects 0); the INSTANCE fixtures (P=2/S=1 and P=1/S=2 with exact closure); the REGIME tests
(exclusive AND exhaustive over the §4.2 matrix; three W-ONE reasons deterministic; stale
partition serves byte-identical pipeline with NO ledger rows); CAPTURE-CONTRACT byte-parity (a
divergent fixture captures NO fingerprint at M-R1 — the GREEN gate preserved until M-R2); the
iteration-4 exact collision baseline (identity_collision = ∅ with the 280-key fallback
population); the hand-built-PartitionIr COLLISION-GUARD test; zap-engine mixed-repo scoping
(1,585 = 29 + 1,556); GREEN/RED byte-unchanged on faithful-mirror/drop-calls/degenerate; R-0
byte-parity dogfood on nginx + spring-petclinic; full cargo gates. Retained artifacts:
`runs/amodx/*`, `runs/ANALYSIS.md` (referenced by the gate; read them, do not regenerate).

## 3. Stop conditions

Frozen: W-B epoch/coordinator invariants, activity-registry semantics, enrich_pass semantics,
postpass/extractor walks, capture-contract GREEN gating (M-R2 flips it, not this slice), the
M-3a/M-3b persisted pipeline accounting (never touched by the union accounting). Any gate
reproduction mismatch vs the retained artifacts is a FINDING (evidence + DECISION_REQUIRED) —
the measured numbers are ratified facts; do not adjust either side to force agreement. Do NOT
commit. The M-1 consolidation witness stays green; manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate column in full; chunked cargo gates (standing pattern); witness 15/15; isolated
dogfood. Fixture-scale runs (nginx/petclinic/zap-engine/amodx artifacts) per the gate.

## 5. Definition of done

The ledger exists in-memory with the full classification taxonomy, reproduces every ratified
measured baseline exactly, derives GREEN/RED byte-unchanged, and changes NO served bytes
anywhere (M-R1 is measurement infrastructure; serving flips are M-R2+).
