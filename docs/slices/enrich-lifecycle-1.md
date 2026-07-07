# ENRICH-LIFECYCLE-1 — Enrichment runs itself (auto after index/refresh)

Status: **DELIVERED (2026-07-07)** · Track: Resolution capability (R4)

## 0. Delivery record (2026-07-07)

Shipped via target-owned relay (2 escalation-resolving ratifications:
batch-boundary cancel-of-running; see §log) + operator proof. Live isolated
transcript: index completes in 15s → "enrichment: queued" → "running —
resolving receiver types now" (~4 min, activity-stamped) → "resolved
7435/9985 receiver types, promoted 261; skipped java: jdtls not found — set
JDTLS_PATH to your jdtls launcher". Trust: 13,896 → 14,157 resolved calls
(+261, exactly the promoted count). Review-caught fixes: promoted=0 (Rust
unparented impl-method qualified-name matching), a lost-yield race in the
acquire→register window (pending-marker registry), doctor label dedupe.
Gates: 4879/0 workspace, fmt/clippy clean. FOLLOW-UP FILED (ENRICH-YIELD-1,
not this slice's regression): promotion criteria pass only 261 of 7435
resolutions (~3.5%) — investigate thresholds/ambiguity rules and whether
resolved-but-unpromoted types can land as Layer-2 inferences per the
certainty model; call-graph reliability stays LOW until the yield improves.
Ratified (operator, 2026-07-04): **auto-enrich after every index/refresh**,
toolchain-aware with honest skips, opt-out available; queued next after
INDEX-DISCONNECT-1.

## 1. Problem — the biggest resolution win requires a command nobody can run

Enrichment (LSP receiver-type resolution: rust-analyzer / tsserver / jdtls)
resolves the large majority of unknown receiver types when it runs (~81% in
the R4 measurement; repo-graph's own self-index sits at 21% call resolution
without it). But it (a) never runs unless invoked, (b) is invoked as
`rmap enrich <db_path> <repo_uid>` — the two identifiers the REG-1 registry
deliberately hides ("internal storage identifiers are hidden", repo.rs) — so
in practice it NEVER runs (the second-machine field sessions confirmed: the
user could not tell whether it ran, then could not run it). The daemon
finishing an index and then idling while every call-graph surface renders
LOW-reliability caveats is the product leaving its own value on the table.

## 2. Ratified semantics

After every completed index/refresh, the daemon queues a background
enrichment pass for that snapshot — when the language's resolver toolchain is
present. Missing toolchain → honest skip with a reader-frame next action.
Opt-out for constrained machines. Enrichment is a NORMAL write operation:
activity-registry stamped (doctor shows it), coordinated under the existing
write locks, cancellable via the existing cancel path, and it survives client
disconnect (INDEX-DISCONNECT-1 semantics apply to it as a write op).

## 3. Contract

1. **Auto-trigger:** on index/refresh completion (the same completion that
   flips the snapshot READY), enqueue enrichment for that snapshot. One
   background enrichment at a time per daemon; a newer trigger for the same
   repo supersedes a queued (not yet started) older one.
2. **Toolchain-aware:** per-language resolver detection (rust-analyzer,
   tsserver, jdtls — the shipped resolvers only). No toolchain for a
   language → that language is skipped with a doctor-visible reason
   ("enrichment skipped for Rust: rust-analyzer not found — install it for
   resolved call types"). Never an error; never silent.
3. **Opt-out:** a config/env switch (follow the existing configuration
   precedent in the codebase — additive; builder verifies the mechanism and
   names it in the report). Opted-out → doctor line says so ("enrichment:
   disabled (RMAP_AUTO_ENRICH=off)" or equivalent).
4. **Write-op discipline:** enrichment runs under the SAME coordination as
   other writes (activity registry stamp with op kind `enrich`, DB write
   coordination, explicit-cancel support, detached completion). It must not
   contend with an explicit user write: if an index/refresh arrives for the
   same repo, the running enrichment yields (cancel + requeue behind the new
   index) — the fresh index supersedes stale enrichment anyway.
5. **Atomicity/visibility of results:** readers never see a torn state.
   Builder inventories how the existing `enrich` writes results (in-place
   promote?) and integrates with the current snapshot/serving rules; if the
   integration requires anything beyond the existing write-lock + additive
   status exposure — e.g. new snapshot semantics or epoch changes — STOP +
   DECISION_REQUIRED (do not improvise around W-B invariants).
6. **Ergonomics (REG-1 closure):** `rmap enrich` resolves from cwd/alias
   like every other command (manual runs stay possible and now legal); the
   positional `<db_path> <repo_uid>` form keeps working for compatibility
   but disappears from help (help shows the registry-resolved form).
7. **Status truth:** the DAEMON-VISIBILITY enrichment line graduates from
   "not run automatically" to the full lifecycle: queued / running (with
   progress if the pipeline counts cheaply) / completed <time> (+ resolution
   delta if cheap) / skipped: <reason> / disabled. Doctor and the post-index
   completion report both carry it.

## 4. Stop conditions

- Additive protocol/config only; anything touching W-B epoch / snapshot
  serving semantics beyond existing write coordination → STOP +
  DECISION_REQUIRED.
- Do NOT add resolvers or languages (no clangd — that is its own tracked
  item). Do NOT auto-install toolchains.
- Explicit user operations always win contention. Do NOT commit.

## 5. Validation (end-of-slice — synchronous; TEST REPORT inlined)

- Cargo gates green from `rust/` (build / full test / fmt / clippy -D
  warnings), inlined.
- **Auto-trigger proof (named test):** completed index → enrichment op
  appears in the activity registry and runs; supersede rule proven (second
  index cancels/requeues a queued enrichment).
- **Toolchain-skip proof (named test):** resolver absent → honest skip
  line, no error, lifecycle state `skipped` with reason.
- **Opt-out proof (named test):** switch off → no enrichment queued, doctor
  says disabled.
- **Contention proof (named test):** explicit index during running
  enrichment → enrichment yields, index proceeds, enrichment requeued.
- **Ergonomics proof (named test):** `rmap enrich` from a repo cwd (no
  identifiers) enriches the registry-resolved repo.
- **Self-dogfood (isolated, /private/tmp, never the real registry):**
  self-index repo-graph with rust-analyzer available → auto-enrichment runs
  detached; record call-resolution BEFORE (~21%) and AFTER; doctor observed
  in queued/running/completed states; transcript inlined.
- `./scripts/dogfood-isolated.sh` green.

## 6. Definition of done

An index on a machine with the right toolchain produces, without any further
command, a snapshot whose call-graph resolution reflects enrichment — and
doctor tells the truth about that lifecycle at every stage (queued, running,
completed, skipped-with-reason, disabled). Manual `rmap enrich` works from
cwd like every other command. Proven by the named tests + the before/after
self-dogfood transcript.
