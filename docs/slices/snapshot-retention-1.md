# SNAPSHOT-RETENTION-1 — The database keeps current-state, not history

Status: **DELIVERED (2026-07-06)** · Track: Storage discipline (VISION: "git
owns history; latest full + minimal transient comparison state")

## 0. Delivery record (2026-07-06)

Shipped via target-owned relay + operator close-out (review-1 escalate
resolved here per the ratified corollary). What shipped: background retention
actor (`retention_pass.rs`, activity kind `retention`) queued after every
successful index/refresh; policy keeps current + valid delta-parent +
`baseline_user`, assigns no `baseline_auto`; threshold-gated WAL-aware
VACUUM; doctor renders the last retention outcome; two-gate contention
safety. **Contract amendment (escalation `completion-report-retention-result`,
ratified: accept):** the synchronous completion report says retention was
QUEUED (with candidate count); the pruned/reclaimed RESULT appears on
`rmap doctor` and in the daemon log — final numbers cannot exist
synchronously without violating the ratified never-on-foreground invariant;
the original DoD text double-promised (drafting error, reviewer-caught).
Evidence: operator-executed fmt/clippy clean, **4856/0** workspace tests,
dogfood green, and a live 3×-index isolated transcript ending `ready|1`
(steady state: ONE snapshot after three indexes; honest below-threshold
VACUUM deferral). Cycle-1 also root-caused the 1-in-4848 load flake (the
auto-pass contaminating unrelated dispatch shape-tests; disabled there under
the ratified irrelevant-to-assertion rule).
Ratified intent (operator, 2026-07-04): "I don't care about history — git has
history. I want DISCOVERY. Keep one snapshot; narrow to what changed."

## 1. Problem — retention exists but never runs

Every index/refresh adds a multi-GB snapshot; nothing removes the old ones.
The retention model is SHIPPED (`storage/src/retention/`: current / parent /
baseline / prunable classification, transactional cascade,
DAEMON-VISIBILITY-1's VACUUM reclaim) but has no lifecycle: the index path
deliberately skips pruning (REFRESH-HANG-1 — 60+s foreground) and manual
`maintenance prune` is nobody's habit. Field reality: one machine
accumulated 2 orphaned partials + will now add READY snapshots on every
reindex. The DB becomes a history store by accident — exactly what the
VISION forbids.

## 2. Contract

1. **Auto-retention pass after every successful index/refresh** (and after
   enrichment promotion once ENRICH-LIFECYCLE-1 lands): the daemon queues a
   BACKGROUND retention op (activity-stamped `retention`, doctor-visible,
   cancellable, detached-completion) — never on the foreground request path.
2. **Retention policy — current-state only (the ratified default):**
   - keep `current` (the latest READY snapshot);
   - keep `parent` ONLY as the delta-refresh base (this is mechanics, not
     history — it is what makes the next refresh cheap; if delta refresh is
     inapplicable, parent is prunable too);
   - keep `baseline_user` (explicitly marked by a human — explicit intent);
   - **everything else is prunable and gets pruned**, including
     `baseline_auto` (cross-snapshot "what changed" falls back to on-demand
     git-baseline recomputation per the VISION — the operator explicitly
     does not want retained comparison history);
   - orphaned non-READY snapshots: already handled (DAEMON-VISIBILITY-1),
     same pass re-runs it.
   Steady state: ≤2 snapshots per repo (current + delta base).
3. **Disk truth:** VACUUM runs threshold-gated inside the background pass
   (e.g. reclaimable ≥ 25% of file size or ≥ 1 GB — builder proposes the
   threshold with rationale) so the file actually shrinks without paying
   VACUUM on every small refresh. Doctor/report show reclaimed bytes.
4. **Honesty surface (amended 2026-07-06, ratified):** the retention pass
   reports like every op — the post-index completion report states that
   cleanup was QUEUED (with the candidate count when known); the RESULT
   ("pruned N, reclaimed X" / deferral reason) appears on `rmap doctor` and
   in the daemon log once the background pass completes. Opt-out switch
   consistent with ENRICH-LIFECYCLE-1's config precedent (default ON — the
   ratified posture is aggressive cleanup).

## 3. Out of scope / stop conditions

- No schema migration (the classification vocabulary exists); needed →
  STOP + DECISION_REQUIRED.
- No change to WHAT a snapshot contains (blob-narrowing is
  ENGINE-CONSOLIDATION-1 / delta-completion territory, not this slice).
- Explicit user ops win contention (same yield rule as enrichment);
  retention never runs while any write op is active on the DB (reuse the
  two-gate discipline from the orphan-prune handler).
- Do NOT commit.

## 4. Validation (end-of-slice — synchronous; TEST REPORT inlined)

- Cargo gates green (build / full test / fmt / clippy -D warnings), inlined.
- **Steady-state proof (named test):** three successive indexes of a fixture
  → exactly current+parent remain, older READY pruned, bytes reclaimed
  reported.
- **Baseline-user proof (named test):** a user-marked baseline survives the
  pass; an auto-baseline does not.
- **Contention proof (named test):** retention yields while an index runs;
  runs after.
- **Threshold proof (named test):** below-threshold reclaim skips VACUUM
  (rows gone, file unshrunk, honest report); above-threshold runs it.
- `./scripts/dogfood-isolated.sh` green; isolated self-dogfood: index
  repo-graph 3×, observe doctor during the retention pass, final DB holds
  ≤2 snapshots with reclaim reported (transcript inlined).

## 5. Definition of done

A repo indexed N times holds at most current + delta-base snapshots, disk
usage tracks the current codebase instead of accumulating history, the pass
runs itself in the background after every successful write op, and doctor +
completion reports state what was pruned and reclaimed — proven by the named
tests + the 3×-index self-dogfood transcript.
