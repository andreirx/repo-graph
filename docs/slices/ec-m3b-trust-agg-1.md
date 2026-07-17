# EC-M3B-TRUST-AGG-1 — persist the g1 resolved-call aggregate; trust core reads it (EC-1 milestone M-3b)

Status: SPECIFIED (2026-07-17) · IMPLEMENTED (builder, 2026-07-17) · REVISED (builder revision 1, 2026-07-17 — review-0 items 1–5 addressed; §6 delivery record; awaiting review) · Track: Consolidation milestones (EC-1 §5.2 M-3b; predicate C-4's last eager FC2a scan)
Depends: M-0 (done), M-1 (done — the consolidation witness is live in CI; this slice pays its
integration there explicitly). HARD ordering: M-3b ≺ M-6's first drop.

## 1. Problem

Five surfaces (trust, check, orient, explain, stats) consume the snapshot resolved-call
count through ONE trust core, which today derives it by an eager read-time
`count_edges_by_type(CALLS)` scan [service.rs:875]. After any per-language CALLS-row drop
(M-6) that COUNT silently undercounts. The persisted full-stream aggregate is the only
honest source.

## 2. Contract (EC-1 §5.2 M-3b row, as ratified + amended)

1. **WRITE (g1):** index AND refresh persist the snapshot-level resolved-call count,
   computed from the FULL resolution stream (all languages — FC0 input, D-EC-2-A),
   including delta-refresh copy-forward. Full Persistence Completeness checklist: write
   path / read path / refresh behavior / trust impact / CLI visibility / validation.
   **Interim rule (ratified, D-EC-1/D-EC-7 supersession (c)):** the persisted value is
   PIPELINE-derived (one coherent accounting, matching the trust denominator) with an
   explicit provenance label; this accounting is EXPLICITLY TEMPORARY until the
   reconciliation layer ships (recon-design-1).
2. **READ:** the ONE shared trust core swaps `count_edges_by_type(CALLS)`
   [service.rs:875] for the persisted aggregate — trust, check, orient, explain, stats
   inherit through `assemble_trust_report`/`get_trust_summary` [agent_impl.rs:326-344];
   zero per-surface work. FC2b-derived module stats/cycles stay read-time owner reads
   (D-EC-5-A — NO migration here).
3. **Trust posture unchanged:** the two-source hybrid (Half-A/Half-B) untouched — only
   the v1 report's `resolved_calls` INPUT changes source. Honesty rules: a snapshot
   without the persisted aggregate (pre-migration snapshot) must be handled explicitly —
   fall back to the live COUNT while CALLS rows exist, labeled; never fabricate, never
   collapse unknown to zero.
4. **Scoping note (recorded deviation from the M-3b row's parenthetical):** the g2
   per-function degree family write MOVES to M-3a, where its consumers swap in the same
   slice — per the ratified deep-vertical directive (no dormant capability; a written
   family nothing reads is the field-bug factory). M-3a's own row already names its
   producer for that family; no ratified decision changes.
5. **The M-1 witness stays green** — any manifest change this slice needs (fact-class
   declarations for touched arms) is made explicitly and reviewed; that is the ONE
   integration by construction.

## 3. Stop conditions

- Frozen areas: W-B epoch/coordinator invariants, activity-registry semantics,
  enrich_pass semantics, postpass/extractor walks.
- If persisted-vs-live parity FAILS on any fixture (the self-validation), that is a
  FINDING (evidence + DECISION_REQUIRED) — do not paper over it.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- **Parity window (self-validating):** while CALLS rows exist, the persisted count MUST
  equal the live COUNT — asserted on fresh index AND refresh (delta copy-forward path
  exercised, not just fresh).
- Byte-compare trust/check/orient/explain/stats on fixtures (before/after — outputs
  identical while parity holds).
- Pre-migration-snapshot fallback path tested (old snapshot, no aggregate → labeled live
  COUNT, not a fabricated 0).
- Cargo gates chunked (standing pattern); consolidation witness 15/15; smoke via
  `dogfood-isolated.sh` if runnable in-sandbox, else recorded NOT RUN with reason.

## 5. Definition of done

The trust core's `resolved_calls` is served from the persisted, provenance-labeled,
full-stream g1 aggregate on both fresh and refreshed snapshots; parity green; the five
surfaces byte-identical; the eager read-time CALLS COUNT no longer on the serving path
(fallback only for pre-migration snapshots); witness green; gates green.

## 6. DELIVERY RECORD (builder, 2026-07-17; REVISED revision 1 — review-0 items 1–5)

**Shape (decide-and-record; D-EC-7-A left "column family or measurement kind" as an
M-3b implementation detail):** g1 = two nullable columns on `snapshots` —
`resolved_call_count INTEGER` (NULL = not persisted, ≠ 0) + `resolved_call_provenance
TEXT` (`'pipeline'`, the interim-rule label; migration 030). Precedent: the snapshot
row already carries the pipeline-written snapshot-level counters
(`files/nodes/edges_total`). REJECTED alternative: a `measurements` row (no
migration) — it would falsify the registered Measurements family contract
(file-local / DirectFromSourceFile / ReextractChangedInputs) and depend silently on
the file-anchored copy-forward's SQL patterns to avoid stale duplication. `snapshots`
is not a registered artifact family, so no contract amendment is needed.

**Write path — SUPPLIED stream-side count (review-0 item 1).** The persisted value
is NEVER derived from the `edges` table (post-M-6 a filtered subset — a
`COUNT(*)` recompute would bake the undercount in). Two writers, both in
crud/snapshots.rs (the auditable write census):

1. `persist_resolved_call_aggregate(snapshot_uid, count)` — stores the count the
   orchestrator TALLIED FROM THE RESOLVER'S OUTPUT STREAM (`run_pipeline` Phase 3:
   `resolved_calls_total`, counted per batch BEFORE `insert_resolved_edges`), called
   at Phase-5 finalization before the READY transition. Fresh index and delta
   refresh share the call; the delta value is full-stream by construction (the
   resolver re-runs over ALL extraction edges, copied-forward + fresh — FC0
   retention, D-EC-2-A). Discriminating test: `orchestrator::tests::
   aggregate_counts_full_resolver_output_when_a_calls_row_is_not_materialized`
   drives the REAL pipeline with a storage port that refuses to materialize one
   resolved CALLS result (the M-6 filter seam) — persisted 2, rows 1.
2. `adjust_resolved_call_aggregate(conn, snapshot_uid, delta)` — the enrichment
   promotion writer; see next paragraph.

**Promotion — atomic, coherent on every exit (review-0 item 2).** The three
separate port calls (delete/insert/persist) are REPLACED by one port method
`EnrichmentStoragePort::apply_promotion(snapshot_uid, promoted) -> inserted`:
delete-previously-promoted-uids + insert-new + aggregate delta adjustment run in
ONE SQLite transaction (`unchecked_transaction`, the prune.rs pattern). Forced
move, recorded: item 1 bans recompute, so promotion can only maintain the
aggregate by delta arithmetic, and delta arithmetic is only coherent if it commits
with the exact mutations it accounts for — atomicity is the unique design
satisfying both items simultaneously (invalidate-first was rejected: with
recompute banned there is no lawful re-seed, so every enriched snapshot would
degrade to fallback forever). Semantics preserved: promotion SELECTION
(`promote_edges`, candidates, gates) untouched; per-edge insert tolerance
("partial success acceptable") kept — the delta counts only rows that actually
landed, so it stays exact. On any hard failure the transaction rolls back: rows
AND aggregate revert together — never stale. NULL aggregate (pre-migration
snapshot) propagates through the delta as NULL: never seeded, never falsely
labeled; the fallback keeps serving. Failure-path coverage:
`apply_promotion_hard_failure_rolls_back_rows_and_aggregate` (101 uids = two
delete chunks; a trigger aborts chunk 2 AFTER chunk 1's 100 deletions executed —
the review's exact partial-mutation scenario — rollback verified, then recovery),
plus idempotent re-promotion and never-seeds tests.

**Read path (review-0 item 3 folded in):** `TrustStorageRead::
get_resolved_call_aggregate` returns the DTO only for a WELL-FORMED persisted
state; a negative count or an unlabeled/empty-labeled count is corrupt-by-
construction (no sanctioned writer produces it) and explicitly DEGRADES to
Ok(None) → the labeled live-COUNT fallback serves (`count.max(0)` clamp REMOVED —
it fabricated a measured zero). Degrade, not error: the trust report stays
reachable and the fallback IS the defined honest source for "no usable
aggregate". The ONE swap sits in `assemble_trust_report_cancellable`'s fetch
block; trust / check / orient / explain / stats inherit through
`assemble_trust_report`/`get_trust_summary` — zero per-surface work, per §2.2.
Trust hybrid posture (Half-A/Half-B) untouched. CLI visibility: the persisted
value is what `rmap trust` (and the four other surfaces) render as
`resolved_calls`.

**Abstraction record (review-0 item 5).**
- `ResolvedCallAggregate` (trust/storage_port.rs): boundary DTO for the persisted
  g1 aggregate. Concrete current users: the SQLite adapter constructs it
  (trust_impl); the trust core consumes `.count` in the source swap; trust
  service/parity tests model it. Named axis of variation: the ACCOUNTING — the
  interim rule makes today's `'pipeline'` label explicitly temporary; the
  reconciliation layer (recon-design-1) will write its own labeled accounting,
  and consumers match on the label value. Simpler rejected alternative: a bare
  `Option<u64>` return — rejected because it erases the label the ratified
  interim rule requires and cannot enforce "invalid states are not representable"
  (provenance is a non-optional `String`; validation lives at the single
  construction site).
- `apply_promotion` (enrichment port method): replaces THREE port methods
  (`delete_edges_by_uids`/`insert_promoted_edges`/iteration-0's
  `persist_resolved_call_aggregate`) — net −2 port surface. Concrete current
  users: `run_promotion` (sole production caller — verified by workspace grep,
  then by the compiler after removal); SQLite adapter + in-memory stub implement.
  Axis of variation: none claimed — it exists because the coherence invariant
  requires one transaction boundary, not for flexibility. Simpler rejected
  alternative: keep the three calls + begin/commit port methods — leaks
  transaction mechanics across the boundary and lets a forgotten commit corrupt
  silently.
- `adjust_resolved_call_aggregate` (crud/snapshots.rs, `pub(crate)`): free
  function over `&Connection` so the promotion transaction can pass its `tx`.
  Users: `apply_promotion` (production), crud tests. Rejected simpler
  alternative: inlining the UPDATE in enrichment_impl — rejected to keep BOTH
  aggregate writers in one file (the write census the parity story audits).

**Validation (§4) — all EXECUTED this revision:** parity fresh + refresh through
the REAL pipeline (repo-index/tests/resolved_call_aggregate.rs — now non-trivial:
supplied stream count vs independently-counted rows; non-vacuous count>0; delta
path proven via kind=refresh + parent link; parent aggregate re-checked);
promotion success/idempotency/failure/never-seeds at the adapter (4 tests);
invalid-state degrade tests (negative, unlabeled, empty label); pre-migration
fallback + core byte identity (storage/tests). **Five-surface byte-compare
EXECUTED** (review-0 item 4) via `scripts/byte-compare-five-surfaces.sh` — a
baseline binary built from HEAD `bfc83f0` (pre-M-3b; the slice-doc commit is
docs-only) vs the working-tree binary, isolated stdio state roots: (a) same
pre-migration DB served by both binaries → 10/10 files RAW byte-identical
(fallback proven on real pre-migration data: migration 30 applied, columns NULL,
live rows serve); (b) baseline-indexed/baseline-served vs candidate-indexed/
candidate-served → 10/10 identical modulo five EXPLICIT volatile-identity
normalizations (ISO timestamps, uuid8 slice, per-run repo ULID full + truncated,
durations — each justified in the script); candidate DB verified carrying
`3|pipeline|3` (persisted == live), so the identical bytes came from the NEW
source, not a silent fallback. Full gate outcomes: relay TEST REPORT.

**Known gaps (named, honest):** (1) no automated test drives
`run_promotion` → `apply_promotion` end-to-end through a live resolver toolchain
(needs LSP provisioning; covered at the adapter level + a 1-line reviewed call
site). (2) Pre-existing, NOT introduced here, surfaced for the ledger:
`snapshots.edges_total` (and siblings) go stale after enrichment promotion — same
staleness class M-3b fixes for the CALLS count; deliberately NOT folded in (would
change stats' summary bytes). (3) The persisted accounting is EXPLICITLY
TEMPORARY per the ratified interim rule — superseded when recon-design-1 ships
its own accounting. (4) Forward obligation, named for M-6: when M-6 introduces
storage-layer CALLS filtering, the PROMOTION insert path must keep its delta
stream-side (today insert-materialization == stream by construction; M-6 must
revisit `apply_promotion`'s counting the same way it revisits
`insert_resolved_edges`).

**Witness/manifests:** no dispatch arms added/removed, no fact-class change (the
five arms' FC2a-agg intake keeps its class; only the mechanism moved), no LiveGraph
reader-set change ⇒ zero manifest edits; witness stays 15/15 (gate-verified).
