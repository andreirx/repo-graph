# EC-M2-LEAF-SERVE-1 — finish the (b)-leaves: MODULE_SUMMARY + cycle VALUES LG-serve on GREEN (EC-1 milestone M-2)

Status: SPECIFIED (2026-07-17) · Track: Consolidation milestones (EC-1 §5.2 M-2)
Depends: M-0 (done). Supersedes the parked `*-SQLITE-FREE-1` spec-first plans (D-EC-4 ratified) —
those docs stay valid as ANALYSIS (leaf inventories, cert designs), retired as plans.

## 1. Problem

orient/explain still serve MODULE_SUMMARY (+ trust + cycle values) from eager SQLite even on
GREEN with LG (b)-leaves live (EC-1 §3.3 orient row). The deferred P1 remainder: DR-2/DR-E3 —
the `module_stats` identity reconciliation for MODULE_SUMMARY structural counts (LG-derivable,
was ratified-SQLite pending exactly this cert) — and CYCLES-B (cycle VALUES LG-serve). The
ratified M-2 row decides the direction: Cat-2(ii) cache serves over SQLite-owned classes,
cert-witnessed, on GREEN only.

## 2. Contract (EC-1 §5.2 M-2 row, as ratified)

1. **orient + explain MODULE_SUMMARY LG-serve on GREEN:** structural counts (file/symbol/
   languages) serve from the LiveGraph cache when the coherence envelope is GREEN, with the
   DR-2/DR-E3 `module_stats` identity reconciliation cert proving LG-derived counts ==
   SQLite-derived counts per module (the stats `module_stats` compare pattern —
   orient-sqlite-free-1 §cert-4). Identity divergence = cert RED = SQLite serve (no silent
   drift; the RISK-E divergence concern is answered by the cert, not assumed away).
2. **Cycle VALUES LG-serve on GREEN (CYCLES-B):** same Cat-2(ii) posture, cert-witnessed.
3. **Envelope discipline:** GREEN-only; RED/YELLOW serve SQLite exactly as today. The
   OrientServeDecorator/CoherenceEnvelope mechanism is the existing seam — extend it; do not
   invent a parallel one.
4. **Explicitly NOT here (ratified):** a `resolved_calls` LG-serve — that leaf's terminal
   source is the M-3b persisted aggregate (ae6e7f8); the trust leaf stays SQLite-labeled.
   No FC ownership changes; this is serving-path only (Cat-2(ii) cache over SQLite-owned
   classes) — the ownership table (C-1) is untouched.
5. **The M-1 witness stays green**; any manifest edits explicit + reviewed.

## 3. Stop conditions

Frozen areas: W-B epoch/coordinator invariants, activity-registry semantics, enrich_pass
semantics, postpass/extractor walks. If the identity-reconciliation cert finds a REAL
divergence between LG and SQLite module counts on fixtures, that is a FINDING (evidence +
DECISION_REQUIRED) — do not paper over, do not "fix" the divergence inside this slice. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Per-leaf no-loss certs GREEN + byte-compare on the smoke fixtures (GREEN serve == SQLite serve
byte-identical; RED path unchanged) + `dogfood-isolated.sh` (RMAP_BIN override sanctioned);
chunked cargo gates (standing pattern); witness 15/15.

## 5. Definition of done

On GREEN, orient/explain MODULE_SUMMARY and cycle VALUES serve from the LG cache,
cert-witnessed byte-identical to the SQLite serve; on non-GREEN the SQLite path is untouched;
certs + byte-compare + dogfood + gates green. The EC-1 §3.3 orient/explain rows' "always
SQLite" caveat shrinks to the trust leaf (M-3b-persisted) alone.
