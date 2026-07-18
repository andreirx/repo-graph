# RECON-M-R2-UNION-SERVING-1 — union serving for callers/callees in W-BOTH, flag-gated (reconciliation IMPL milestone M-R2)

Status: SPECIFIED (2026-07-18) · Track: Reconciliation IMPL (recon-design-1 §6.1, ratified §8)
Depends: M-R1 (done, c0e1dad — the ledger + amendment bad69da). Ordering: M-R2 ∥ M-R3a after M-R1.

## 1. Contract — the recon-design-1 §6.1 **M-R2 row IS the binding contract**, verbatim

Union serving for callers/callees in W-BOTH: the CAPTURE-CONTRACT flip
(ledger-validity-gated, verdict-independent — §4.2/§5.1; the named movement
`fallback_reason`; the flip RIDES THE SAME FLAG as union serving — the default path's
capture stays GREEN-gated byte-exact until the recorded default flip), the LG kind filter
(§3.4-3), union rows + `witness` fields (dual-measured only; `mixed` +
`occurrences: {confirmed, total}` on P-excess delta pairs; S-excess instances MINT
`semantic` rows — §3.3, iteration 6) + `witness_counts` incl. `unmeasured` (1:1 with rows,
§5.2), MAX multiplicity = row count (the preserved `count == rows.len()` contract), null-
not-zero locations (§3.7-4; §3.3a definition-location semantics), presentation accepts
unknown; replace the §3.7-3 row builder; ADD the pipeline-only test fixture (informed by
the amodx boundary + uncorroborated shapes).

**SHIPS FLAG-GATED, NON-DEFAULT, until S-1..S-3 (§6.2 — the monorepo field gates). The
default flip is its own recorded step, NOT this slice.** With the flag OFF (the default),
every served byte everywhere is byte-identical to today.

## 2. Gate — the M-R2 row's gate column, verbatim (highlights)

union ⊇ P verbatim (named test); R-0 + R-1 byte-parity (nginx/petclinic + zap-engine
mixed); count/MAX + ROW/COUNT INVARIANT (`count == rows.len()` across every fixture
class); DIVERGENT-CAPTURE (divergent fixture CAPTURES a fingerprint at M-R2 under the
flag and serves union in W-BOTH — the twin of M-R1's opposite test); EPOCH-MOVED
(fingerprint moved between capture and read → pipeline bytes at the pinned snapshot, NO
witness fields, the movement `fallback_reason`); CAPTURE-FAILED (ledger build error →
pipeline serve + doctor-reportable reason); DELTA-PAIR row tests (P=2/S=1 → both rows
`mixed` + {confirmed:1, total:2}, never `both`; P=1/S=2 → count 2, two rows: one P `both`
+ one S-minted `semantic`/`multiplicity`, closure + row multiset 1:1); STALE-serving
(pipeline bytes, no union fields — W-ONE); collision-withheld pairs NEVER serve (M-R1's
guard fixture through serving); W-B epoch tests (pin + eviction unchanged).

## 3. Stop conditions

Frozen: W-B epoch/coordinator invariants, activity-registry, enrich_pass, postpass/
extractor walks, the M-3a/M-3b persisted pipeline accounting, trust ratio denominator
(remains the pipeline-only floor — never inflated by union counts). The DEFAULT FLIP is
out of scope — flag-off byte-parity everywhere is part of the definition of done. A
baseline/parity mismatch is a FINDING. Do NOT commit. Consolidation witness green;
manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate column in full; flag-OFF byte-parity proven on R-0 dogfood (nginx +
spring-petclinic) AND the self-index; chunked cargo gates; witness 15/15; isolated
dogfood.

## 5. Definition of done

Behind the flag: callers/callees in W-BOTH serve the union with instance-granular witness
provenance, honest unknowns, and the capture-contract flip; flag off: byte-identical
serving everywhere. All gate tests green. The default flip remains a separate recorded
step gated on S-1..S-3.
