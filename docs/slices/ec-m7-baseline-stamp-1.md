# EC-M7-BASELINE-STAMP-1 — baseline marks become provenance stamps; row retention is an explicit opt-in (EC-1 milestone M-7)

Status: SPECIFIED (2026-07-18) · Track: Consolidation milestones (EC-1 §5.2 M-7; predicate C-8)
Ratified basis: D-EC-8 = option D (baseline axis) + REP-1 (representation axis), ratified as
written in EC-1 §8. REP-2 is explicitly OUT (queue-4 delta-indexing territory, its own future
ratification).

## 1. Problem

Today `mark_baseline` retains full graph-row families per mark — multi-GB pinned copies at scale
(87k-file evidence: 3 retained snapshots = 11 GB, EC-1 §4.5) — and the cost is invisible at mark
time. The ratified D semantics: a baseline mark is a provenance STAMP by default; full-row
retention remains available but becomes an explicit, cost-surfaced choice.

## 2. Contract (EC-1 §5.2 M-7 row + D-EC-8-D/REP-1, as ratified)

1. **Stamp default:** `mark_baseline` marks STAMPS — comparability metadata + the (small) FC4
   measurement rows retained per mark; graph-family rows NOT retained by default.
2. **Explicit opt-in:** `mark_baseline` gains a stated row-retention flag; the storage cost is
   SURFACED AT MARK TIME (the response names what is being retained and its measured/estimated
   size — honest numbers, no fabrication). Retention reporting shows per-mark cost either way.
3. **Honest degradation:** graph-row baseline comparisons against a stamp-only mark degrade to
   NOT_COMPARABLE with the concrete remediation named (re-mark with row retention) — VISION rule
   3, never fake numbers; measurement-level comparison keeps working.
4. **Handlers keep their surface:** `classify_retention`/`mark_baseline` continue operating on
   retention_class; existing callers keep working against the stamp.
5. **INVARIANTS PRESERVED UNCHANGED (C-8; frozen):** the W-B transient window (refresh publishes
   N+1 without deleting N; pinned readers; prune as exclusive writer) and the ratified keep-set
   copy-COUNT (current + delta-base + baseline marks; the §4.5 two-regime transient model). M-7
   changes what a baseline MARK retains + blob width — never the keep-set count or the
   refresh-window invariant.
6. **REP-1:** the snapshot-scoped row-copy representation is untouched — no reader-query,
   copy-forward, CASCADE, or pin-mechanics changes.
7. **Migration/back-compat:** existing `baseline_user` marks (row-retaining) keep their rows and
   their comparability — no silent data loss on upgrade; their class/report labels them as
   row-retaining marks.
8. **The M-1 witness stays green**; manifest edits explicit + reviewed.

## 3. Stop conditions

Frozen: W-B epoch/coordinator invariants, prune's exclusive-writer/reader-drain mechanics, the
keep-set count semantics, enrich_pass, postpass/extractor walks. If preserving an existing
capability conflicts with the stamp default in a way the ratified text does not resolve, that is
a FINDING (DECISION_REQUIRED), not an improvisation. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Retention tests (stamp default; opt-in row retention; existing-mark back-compat; NOT_COMPARABLE
degradation with remediation text; per-mark cost reporting) + refresh smoke on fresh index AND
delta refresh (Persistence Completeness — the keep-set and W-B window proven undisturbed);
chunked cargo gates (standing pattern); witness 15/15; isolated dogfood (RMAP_BIN override
sanctioned).

## 5. Definition of done

A default mark retains stamp + measurements only and says so; row retention is an explicit flag
with its cost surfaced at mark time and in retention reporting; stamp-only comparisons degrade
honestly with remediation; pre-existing marks unaffected; keep-set count and W-B window
byte-for-byte semantics preserved; gates + witness green.
