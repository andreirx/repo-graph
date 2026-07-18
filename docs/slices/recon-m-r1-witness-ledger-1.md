# RECON-M-R1-WITNESS-LEDGER-1 — the witness ledger (reconciliation IMPL milestone M-R1)

Status: IMPLEMENTED (2026-07-18; working tree, uncommitted — reviewer gate pending) · Track:
Reconciliation IMPL (recon-design-1 §6.1, ratified §8 all seven decisions, commit 8241ff5).
Ordering: M-R1 ≺ everything in the arc. Implementation record: §6 below.

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

**Gate amendment (2026-07-18, operator-ratified — commit `bad69da`, resolving the review-0
`m-r1-rendering-scope` escalate):** the collision-guard item reads `identity_collision`
counted + retained + **debug-artifact-visible** at M-R1; trust-block + doctor RENDERING lands
with M-R3a's read surfaces per recon-design-1 §5.4 ownership. The original "counted + rendered
(trust block + doctor)" wording conflicted with this slice's zero-served-byte definition of
done; the guard's substance is unchanged.

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

## 6. Implementation record (M-R1 build, 2026-07-18)

### 6.1 Shape (least-new-surface, recorded per the packet)

- **The ledger module**: `daemon-runtime/src/callgraph_cert/ledger.rs` — a SUBMODULE of the cert
  it generalizes. No new crate, no new dependency edge, and NO new reader of the `RepoState`
  LiveGraph field (the sanctioned `mod.rs` reader locks once and passes `&LiveGraph` down; the
  EC-M1 reader-set witness is untouched by production code). The shared comparison primitives
  (`classify`, `diff_direction`, `EdgeViews`, `lg_side`, pair renderers) MOVED there from
  `diff.rs` verbatim — the §5.1 graduation: the spike collector now consumes the substrate it
  prototyped.
- **LiveGraph read-side walk**: one new accessor `LiveGraph::resident_irs()` returning
  `ResidentIr { id, language, fresh, ir }` borrow views (one consumer: the ledger build; simpler
  alternative rejected: exposing `Slot` internals).
- **Wiring**: `RepoState.witness_ledger: RwLock<Option<WitnessLedger>>` stored by
  `build_and_store_callgraph_cert` under the SAME fingerprint key + lifecycle as the cert; the
  stored GREEN/RED verdict is `WitnessLedger::derived_green()` (`GREEN ⟺ zero divergent symbols
  ∧ zero unanswerable projections ∧ zero field mismatches` on the measured path; degenerate
  paths RED; `None` only on a SQLite error, nothing stored — today's exact contracts).
- **Observation channel**: the env-gated debug artifact (`RMAP_CALLGRAPH_DIFF`) gains an ADDITIVE
  `witness_ledger` block; schema `callgraph-diff/v3` → `v4`. No served surface changes anywhere.
- **§3.7-2 doc fixes**: `lg_caller_rows`/`lg_callee_rows` doc-comments now state the KIND-BLIND
  truth (the M-R2 kind filter named). **§3.7-5**: `CANONICAL_EDGE_NOTE` carries the
  coverage-blend caveat; the ledger's `CanonicalSplit` separates `pipeline_only` into
  `dual_measured` vs `unmeasured` (the §3.6 rule).

### 6.2 Decide-and-record (one line each)

- **S-side dual-measured-by-construction**: an S strict-`Calls` pair from an eligible partition
  is dual-measured in any completed walk — the strict ingest guarantees the caller is an
  AST-adopted corpus symbol, so P's SQLite projection of the pair ran; testing S pairs by
  LG-ANSWER answerability would test the wrong witness (an envelope `Partial` from a
  fallback-identity endpoint is our honesty machinery, not P failing to measure). This is what
  makes the ratified amodx `semantic 48` (which includes `trackFBEvent → window.fbq`, a
  fallback-keyed ambient whose LG projections are unanswerable/unwalked) mechanically exact.
- **Exhaustive-walk panic policy**: per-symbol `catch_unwind` (the spike's mechanism) — a caught
  panic is an unanswerable projection ⇒ RED. Wherever today's short-circuit walk completes, the
  derived verdict is byte-identical; where today's walk would ABORT the daemon (a latent panic
  reached only exhaustively; measured incidence 0 since LIVEGRAPH-PARTIAL-FIX-1), the ledger
  yields RED fail-soft instead. Deviation surfaced, not silent.
- **SQLite-error-anywhere ⇒ `None`** (nothing stored): a superset of today's `None` (today a
  read error AFTER the first divergence never occurs — the walk stopped); on that corner today
  stores RED, the ledger stores nothing — served bytes identical (both fall back), only
  rebuild-caching differs on a failing-disk path.
- **Pair-level syntactic sub-class precedence**: `file_scope` (P caller node kind FILE) →
  `boundary` (both endpoints in S's node inventory, partition sets disjoint) →
  `uncorroborated` (incl. an endpoint absent from S entirely — no two-compiler-runs story
  exists for it). Reproduces the ratified amodx 11/1/1 exactly.
- **Rollup keying**: per `(language, partition)` for the S-witnessed facts (kind totals,
  adoption counts, colliding keys, `both`/`semantic` instance attribution by the witnessing IR
  edge's partition, min-then-excess filled in partition-id order). Classes defined by the
  ABSENCE of an S edge (`syntactic` pair-level, `unmeasured`) have no honest per-partition
  attribution and live global-only; the per-language RENDERING split is M-R3a's, deferred with
  its rendering contract.
- **Fallback-MIXED keys** (fallback + any adoption-compatible source on one key) are treated as
  COLLIDING (conservative, per §3.5's measured-observation rule; measured today: zero).
- **`s_calls_unmeasured` field REMOVED** (structurally zero under the S-side rule above; the
  invariant documented on `unmeasured_edges`).
- **clippy `enum_variant_names` allowed** on `StateClass` — the `W-*` prefixes are the
  OPERATOR-RATIFIED regime vocabulary (R-RAT-6); the lint yields to the ratified names.
- **Witness manifest edit (explicit)**: `callgraph_cert/ledger_tests.rs` added to
  `[test-scaffolding]` (it builds fixtures through the field; `#[cfg(test)]`-gated, verified by
  the witness's gating check). NO `[production]` change.
- **`agreement_pct` is PERCENTAGE POINTS** (97.4, never 0.974 — review-0 residue, fixed
  iteration 1): the build-0 method returned the 0–1 ratio, contradicting the `_pct` name and
  every ratified gate figure; the retained `iter4-recompute.py` prints the labeled ratio
  (`= 0.9744`) — same fact, different labeled unit, script stays frozen. Fixed at the method
  (single-rounding `100·both/dual_measured`), the artifact field, and the three test
  assertions; the artifact schema stays `callgraph-diff/v4` (v4 is THIS unmerged slice's
  addition — pre-merge shape refinement per `diff.rs`'s own schema-history rule, no shipped
  reader ever saw the ratio).

### 6.3 The committed fixture's per-kind RECORD (M-R1 gate item, measured 2026-07-18)

Canonical classification (LIVE ledger walk over the committed `index.scip` + the spike's 2-call
pipeline mirror): **scip_only 7 / pipeline_only 0 / shared 2 / union 9** — the spike's 7/0/2/9,
reproduced. Per-kind: **all 7 SCIP-only canonical instances are `References`-kind** (expected
per the measured ctor evidence — `is_call_at` does not cover new-expressions; CONFIRMED), so the
kind-aligned union call graph is exactly the 2 corroborated calls (`both` 2, `semantic` 0,
`agreement` 100%). The RAW IR holds **9** References instances beyond the call pairs = the 7
canonical + 2 whose projections are BOTH unanswerable (`main.ts:FILE → shapes.ts:FILE`, the
file-scope import reference; `shapes.ts:FILE → #label:SYMBOL:Term`, a REAL
`ScipSynthesizedFallback` ambient) — an unmeasured side never mints a phantom canonical edge
(§3.6). Fixture answerability: 4 unanswerable projections (the 2 FILE symbols × 2 directions),
0 panics, 0 field mismatches. Fixture guard population: **4 fallback keys** (`#Shape:SYMBOL:Type`,
`#label:SYMBOL:Term`, `#<constructor>:SYMBOL:Method`, `#size:SYMBOL:Method`), collisions ∅.
Pinned in `callgraph_cert/ledger_tests.rs::committed_fixture_reproduces_spike_7_0_2_9_and_records_all_references_kinds`.

### 6.4 New measurement recorded (not a baseline; labeled)

The zap-engine KIND-ALIGNED classification (never measured before — the baseline runs had no
kind harness for zap; the ratified zap gate is the kind-blind coverage split, reproduced
exactly): over the 3 eligible TS partitions — `both` 136 instances (126 identities),
`syntactic` 30 (28) = 0 boundary + 0 file_scope + 30 uncorroborated, `semantic` 26 (24) all
new_pair, `unmeasured` 1,556 (1,212). The kind-aligned `syntactic` 30 exceeds the kind-blind
dual-measured 29 exactly as the design predicts (kind alignment moves S-covers-pair-with-
non-Calls-kinds pairs from `shared` into `syntactic`).
