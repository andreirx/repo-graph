# DAEMON-W-B-EPOCH-1: request-level cross-store epoch → re-enable W-B (serve last-good during refresh) — SPEC

Slice: DAEMON-W-B-EPOCH-1
Status: **SPEC** (design; the IMPL is the relay slice(s) that follow — see §12 slice-split)
Track: Daemon robustness (`docs/ROADMAP.md` → Current priority, P1 "daemon concurrency" → the deferred
`DAEMON-W-B-EPOCH-1` bullet). Re-opens the **D-W = W-B** + **D-E = E-A** cells that DAEMON-CONCURRENCY-1 §14
**withdrew** after the two-agent decision review found a cross-store split-brain. Closes **TECH-DEBT #2b**.
Grounded in: `docs/slices/daemon-concurrency-1.md` (§2 substrate, §6 reader/writer safety + the §6 correction,
§8 D-S/D-W/D-E matrices, §14 ratification + the "Deferred (not lost)" note); the Codex W-B split-brain finding
§14 records; `docs/VISION.md` (Operational Architecture — the daemon as multi-agent authority; "git owns
history, repo-graph owns current-state"). Confirmed against the daemon source (every claim cited by `path:line`;
lines are working-tree at spec time — the IMPL re-confirms before editing).
Model: this doc follows `docs/slices/daemon-concurrency-1.md` (problem+evidence → root cause confirmed against
code → principle → desired behavior → design → decisions-to-surface → per-choice VISION defense →
§6 amendment → validation → smallest-design/STOP assessment).
Prior art reused (not reinvented): the shared SQLite-free cert fingerprint
(`import_cert_fingerprint` — `daemon-runtime/src/livegraph_feed.rs:1668-1693`) that ALREADY binds
`(snapshot_uid, LiveGraph partition epochs)` into one digest; the six fingerprint-keyed no-loss certs
(`state.rs:195-246`); the `OrientServeDecorator` serve-then-fallback shape (`orient_serve/mod.rs`,
`orient_serve/storage_port_impl.rs`); the `RepoCoordinator` reader/writer FIFO + its `Refreshing` state
(`daemon-policy/src/coordinator.rs`, `daemon-policy/src/state.rs`); the B1 LiveGraph-writer coordination fix
(`dispatch.rs:801,844`). This slice **captures that fingerprint pair ONCE per request and threads it**; it does
**not** invent a new storage or LiveGraph subsystem.

### Revision log

- **review-0 (iteration 0): `STATUS: revise`** — overall shape accepted; three surgical buildability/honesty
  fixes required. This revision (iteration 1) addresses each, in place (no structural rewrite):
  1. *Epoch representation must keep `orient_repo` buildable* — `orient_repo` feeds the **full** `AgentSnapshot`
     to `aggregators::snapshot::aggregate(&snapshot)` (`agent/src/orient/repo.rs:113`), so passing only
     `snapshot_uid` is non-buildable. **Fixed:** `RequestEpoch` now carries the whole pinned `AgentSnapshot`
     (`snapshot_uid` is a derived accessor); `orient_repo` receives `&AgentSnapshot` and uses it for BOTH the
     aggregator AND the uid (§5.2, §5.3, §8 D-EP, §12).
  2. *Do not call a cross-instant SQLite+LG pair "coherent by construction"* — capture reads the two stores under
     different mechanisms and W-B admits a racing publish between them. **Fixed:** the `snapshot_uid` is the
     **atomic** SQLite pin; the fingerprint is reframed as a **green-validated LG-serve eligibility witness**
     (the green-check's own validated value), with a coherence proof for the captured-across-a-publish case — no
     atomicity claimed, the cross-instant case is *detected and kept coherent* (downgraded to SQLite at the pin, or
     served only as a cert-proven-equal set), never a mix (§5.1, §6.2; the lazy-rebuild correction is the review-2
     entry below).
  3. *Validation must cover full-snapshot threading* — **Fixed:** §11 tests now assert `get_latest_snapshot == 1`,
     that `snapshot::aggregate`'s SNAPSHOT_INFO uses the **captured** snapshot's metadata (no fallback
     re-resolve/fetch-latest), and a dedicated cross-instant-capture → eager-SQLite-fallback case (§11 tests 1, 3, 3b).

- **review-1 (iteration 1): `STATUS: revise`** — the orient/explain **decorator** mechanization was accepted; the
  spec was found **incomplete for its own recommended SC-A scope** (`orient, explain, path, callers, callees`):
  `path`/`callers`/`callees` had the hazards named but **no buildable per-handler epoch validation/fallback**, the §6
  proof covered only the orient decorator, validation lacked non-orient race tests, and the eligibility-helper
  semantics under lazy cert rebuilds were unclarified. This revision (iteration 2) addresses each, **additively**
  (the accepted orient design is preserved; `RequestEpoch`'s shape is unchanged):
  1. *Buildable design for `path`/`callers`/`callees`* (review-1 #1) — **Added §5.4 (capture) + §6.3 (serve
     validation).** Confirmed first-hand: these three resolve the snapshot **once** and thread `snapshot.snapshot_uid`
     to every SQLite read (`dispatch.rs:1022/1115/1847` → `:1040/1133/1866,1889` + the lazy fallback closures + the
     `path` stamp `:1949-1960`) — they have **no** orient-style double-resolve, so their SQLite side is **already
     epoch-pinned**. Their **only** gap is the LG serve: today they serve on a **per-call `Exact + Fresh + TS-only`
     envelope** with **no cert and no `snapshot_uid` binding** (`livegraph_feed.rs:231-246,355-392,684-755`). The fix:
     capture `epoch.fingerprint` from the **CALLGRAPH** no-loss cert (the oracle that proves the resident partitions
     are no-loss-coherent with the pinned snapshot — `callgraph_cert/mod.rs:220-267`; callers/callees serve CALLS
     rows, `path` BFS traverses CALLS edges), validate `current_fp == epoch.fingerprint` under the **same** read guard
     as the LG data read, else fall back to SQLite at `epoch.snapshot_uid()`. This is a **behavior change** (today they
     serve LG even when the repo-wide callgraph cert is RED) → surfaced as **D-CC** (§8).
  2. *§6 proof per SC-A handler* (review-1 #2) — **Extended §6.2** with a per-handler coherence clause: orient/explain
     (decorator), `callers`/`callees` (`target` ⋈ edges), `path` (`from`/`to` ⋈ BFS ⋈ stamp).
  3. *Non-orient race tests* (review-1 #3) — **Added §11 tests 7–9**: `path` BFS + stamp under a mid-request swap;
     `callers`/`callees` LG-first serve + pinned-SQLite fallback; the capture-straddle for all three.
  4. *Eligibility-helper semantics under lazy rebuilds* (review-1 #4) — **Clarified §5.1 + new §6.4.** The
     `*_cert_eligibility` witness is captured **build-then-peek**: warm the cert (lazy build), then re-peek a cached
     GREEN cert at the **current resident fingerprint** under one read guard, returning `Some(current_fp)` **iff** a
     GREEN cert exists at exactly the resident state, else **`None`** ⇒ pinned-SQLite. So the returned fingerprint **is**
     the exact resident-and-validated state, or `None` — review-1 #4 in its own terms. Needs a `focus_resolution_cached_green`
     peek (mirror of the existing `callgraph_cached_green`) for the bounded peek, and a `callgraph_cert_eligibility`
     for the three non-orient handlers. The lazy-build mislabel is **harmless** (monotonic partition epochs ⇒ a
     mislabeled cert is never validly re-peeked); an optional honest-build hardening is noted.

- **review-2 (iteration 2): `STATUS: revise`** — scope, decisions, and source-confirmation all accepted; **one
  correctness inconsistency** in the cert/fingerprint semantics required reconciliation (the cross-instant capture
  vs. the lazy cert rebuild). review-2 observed — re-confirmed first-hand this iteration against
  `callgraph_cert/mod.rs:220-348` + `livegraph_feed.rs:1668-1693` — that `callgraph_is_green` /
  `focus_resolution_is_green` **lazily (re)build** a cert keyed at `import_cert_fingerprint(current partitions,
  snapshot_uid)` **against whatever partitions are resident at build time** (`build_and_store_callgraph_cert`
  `:272-285` runs the field-exact compare, then stores the verdict under the passed fingerprint). So a stored GREEN
  cert at `snapshot_uid` is an **equivalence proof** (those partitions ≡ SQLite@`snapshot_uid`), **not** a token
  "produced by that snapshot's publish," and a capture that **straddles** a publish can **validly** return `Some(fp)`
  when the new resident partitions are proven equal to SQLite@`snapshot_uid_N` — the exact case §6.4's build-then-peek
  and §11 test 9b already described. This revision (iteration 3) reconciles the four flagged spots to **one** rule —
  *the pinned SQLite `snapshot_uid_N` is the immutable anchor; the fingerprint is an equivalence witness; the
  LiveGraph serves only a partition-set the no-loss cert proves equal to SQLite@`snapshot_uid_N`; coherence comes from
  that proven equality, not from epoch-identity* — and changes **only the description of why the design is coherent**
  (no structural change, no new decision, no scope change; capture-once + thread + serve-iff-cert-proven-equal is
  unchanged):
  1. *Remove the false claim* (review-2 #2) — §5.1 point 2 no longer asserts a GREEN cert is "only produced by that
     snapshot's publish"; it now states the lazy-on-demand equivalence-proof semantics (cited to
     `callgraph_cert/mod.rs:220-348`, `livegraph_feed.rs:1668-1693`).
  2. *Reconcile §5.1 / §6.2 / §6.4* (review-2 #1) — §5.1's straddle paragraph now gives **two** coherent outcomes
     (a: data-mutating straddle ⇒ `None` ⇒ eager SQLite@N; b: content-preserving straddle ⇒ `Some(fp)` ⇒ LG proven
     equal to SQLite@N), matching §6.4's build-then-peek and §11 test 9b — not the earlier "straddle ⇒ always `None`."
  3. *Fix the proof wording* (review-2 #4) — §6.1 / §6.2 no longer say `current_fp == fingerprint_N` ⟺ "resident LG
     is still epoch N"; they say it ⟺ the resident partition-set is the *same cert-validated partitions proven
     no-loss-equal to SQLite@`snapshot_uid_N`* (possibly a later partition-set proven equivalent — §5.1 case b), so the
     serve is substitutable for SQLite@N **by the cert's equality**. The §10 binding-contract restatement is reworded
     to match.
  4. *Make test 3b conditional* (review-2 #3) — §11 test 3b now splits into 3b(a) (data-mutating straddle ⇒ assert
     `None`) and 3b(b) (content-preserving straddle ⇒ assert `Some(fp)` + LG-equals-SQLite@N); it no longer asserts an
     unconditional `epoch.fingerprint == None`.

---

## 0. Evidence law (how to read the claims below)

- `OBSERVED` — read directly from source at spec time, cited by `path:line`.
- `INFERRED` — concluded from cited code, not executed.
- This slice is a **design doc**: it runs **no** code (VALIDATION below is "the doc exists and is buildable").
  Every `OBSERVED` line cites the file; the IMPL re-confirms before editing (lines drift) and produces the
  EXECUTED-class evidence in §11.

---

## 1. The problem (OBSERVED, cited to source) — the cross-store split-brain

DAEMON-CONCURRENCY-1 §14 **withdrew W-B** ("readers serve the last-good READY snapshot during a concurrent
refresh") because the two-agent decision review proved W-B reintroduces a **cross-store split-brain**: the SQLite
READY snapshot and the in-memory LiveGraph are two **independently-versioned stores**, and a single request reads
**"latest" from each store separately**, with no captured per-request epoch binding them. Under W-A (the shipped
behavior) this is **latent** — the coordinator's `Refreshing` read-block (`daemon-policy/src/state.rs:80-84`)
excludes any refresh for the whole duration of a reader's `acquire_read` guard, so no store can swap mid-request.
W-B removes that exclusion (its entire point), which makes the latent split-brain **live**.

### 1a. There is no single "epoch" — SQLite and the LiveGraph version independently

- **SQLite "latest" = the `status='ready'` row.** `get_latest_snapshot` filters `status = 'ready'` ORDER BY
  `created_at DESC` (`storage/src/crud/snapshots.rs:121-134`). An index/refresh creates a **BUILDING** snapshot,
  works, then flips it READY with a one-row UPDATE (`update_snapshot_status`, `snapshots.rs:142-155`); old epochs
  are later **marked prunable** (`retention/classify.rs:261 mark_stale_epochs_prunable`), then deleted only by a
  separate prune. So the SQLite "current snapshot pointer" moves on the BUILDING→READY flip.
- **LiveGraph "latest" = the resident partition epoch.** A refresh feeds the partition under
  `repo_state.livegraph.write()`, bumping the partition epoch **in place** (`livegraph_refresh.rs:354-364`,
  `feed_partition` into `get_or_insert_with`; swap sites also `:278`, `:539`). The producer runs **lock-free**;
  the write lock is held **only for the swap** (`livegraph_refresh.rs:116`).
- **The two flips are NOT atomic with each other and are driven by SEPARATE daemon commands.** The SQLite
  snapshot flip happens in the SQLite index/refresh pipeline (`handle_refresh` → `index_path_with_progress`); the
  LiveGraph epoch bump happens in `handle_livegraph_refresh` / `handle_livegraph_preload`
  (`dispatch.rs:821,766`). Nothing serializes "SQLite is now epoch N+1" with "LiveGraph is now epoch N+1." They
  are kept in agreement only by the **no-loss cert** being GREEN at a matching fingerprint — which is precisely a
  detector that they CAN disagree.

### 1b. A single request reads "latest" THREE times, across both stores (the double-resolve + the LG serve)

Trace the **headline** mixed-read handler, `handle_orient` (`dispatch.rs:2831`):

1. `let _read_guard = repo_state.coordinator.acquire_read();` (`dispatch.rs:2870`).
2. **Resolve #1 (SQLite):** `storage.get_latest_snapshot(&repo_uid)` → `snapshot_uid_A`, used to decide
   `serve_from_lg = orient_bounded_cert_is_green(&repo_state, snapshot_uid_A)` (`dispatch.rs:2915-2922`). The
   green-check recomputes `import_cert_fingerprint(live_partitions(), snapshot_uid_A)` and compares it to the
   stored cert (`orient_serve/mod.rs:89-92`; `callgraph_cert/mod.rs:297-309`) — reading `live_partitions()` at
   instant **T1**.
3. Build the decorator over `&repo_state.livegraph` (`OrientServeDecorator::new`, `dispatch.rs:2939-2940`) and
   call `orient_cancellable(&decorator, …)`.
4. **Resolve #2 (SQLite) — the double-resolve:** inside the agent use case, `orient_repo` resolves
   `storage.get_latest_snapshot(repo_uid)` **AGAIN** (`agent/src/orient/repo.rs:99-106`) → `snapshot_uid_B`, and
   threads `snapshot_uid_B` into **every** aggregator (trust, cycles, boundary, dead-code, MODULE_SUMMARY —
   `orient/repo.rs:117-151`). The decorator delegates `get_latest_snapshot` straight to SQLite
   (`orient_serve/storage_port_impl.rs:151-156`), so resolve #2 is a fresh "latest" read.
5. **LiveGraph serve (store #2):** the decorator's `find_symbol_callers` / focus methods read
   `self.livegraph.read()` directly (`orient_serve/storage_port_impl.rs:33-50,111-126`); `lg_caller_rows(lg, …)`
   serves whatever epoch is resident at instant **T2** — it takes **no** fingerprint and does **no** re-validation
   (`callgraph_cert/mod.rs:117-160`).

Under W-B, a refresh that commits between any of T1 / resolve #1 / resolve #2 / T2 yields **three potentially
different epochs in one answer**: the cert was proven GREEN at `(snapshot_uid_A, LG@T1)`, the trust/module
counts are read at `snapshot_uid_B`, and the callers are served from `LG@T2`. The agent receives a **Frankenstein
orientation** — MODULE_SUMMARY from one epoch, callers from another, a no-loss guarantee proven for a third — a
repo state that **never existed**. This violates the Fact-Certainty model: a Layer-1 answer presented as coherent
current-state truth that is actually a cross-epoch mix.

### 1c. The same shape in `path` (and every mixed-read handler)

`handle_path` (`dispatch.rs:1818`): acquires the read guard (`:1835`), resolves `get_latest_snapshot` →
`snapshot.snapshot_uid` (`:1847`), runs the **LiveGraph BFS** (`path_engine_response`, `:1943`), then **stamps
the SQLite `snapshot_uid` onto the LiveGraph-served answer** (`:1949-1960`, `"snapshot_uid": snapshot.snapshot_uid`).
If the LiveGraph swapped between `:1847` and `:1943`, the path is as-of the LiveGraph's newer epoch but is **labelled**
with the older SQLite snapshot — a false freshness claim. `explain` shares orient's decorator (`dispatch.rs:3274`),
so it has orient's exact double-resolve; `callers`/`callees` resolve `get_latest_snapshot` then serve LiveGraph-first
with a lazy SQLite fallback (`dispatch.rs:986,1040,1077` / `:1133,1170`).

**Root-cause summary**

| Symptom (OBSERVED) | Mechanism (cited) | What it needs |
|---|---|---|
| Two SQLite "latest" resolves per orient request | `dispatch.rs:2916` (serve-decision) + `agent/src/orient/repo.rs:99` (aggregators) | resolve `snapshot_uid` **once**, thread it |
| LiveGraph served at a different instant than the SQLite reads | decorator reads `self.livegraph.read()` with no epoch pin (`storage_port_impl.rs:33-50,111-126`; `callgraph_cert/mod.rs:117-160`) | **validate** the captured LG fingerprint on each serve; fall back to the pinned snapshot on mismatch |
| LG-served answer stamped with a SQLite snapshot_uid from a different epoch | `path` stamp `dispatch.rs:1949-1960`; orient/explain envelope stamp | stamp the **captured** epoch, not a re-resolved "latest" |
| The whole hazard is invisible today | `acquire_read` excludes `Refreshing` (`daemon-policy/src/state.rs:80-84`) — W-A | W-B relaxes that block → the capture must make the request self-coherent |

---

## 2. The substrate already exists — confirmed against code (the load-bearing correction)

The central finding, confirmed by reading the daemon: **the `(ready_snapshot_uid, livegraph_fingerprint)` pair the
epoch needs is ALREADY a computed value** — it is the cert invalidation key, computed on every fastpath call. The
slice's job is to **capture it ONCE per request and thread it**, not to build an epoch subsystem.

### 2a. `import_cert_fingerprint` IS the cross-store epoch pair

- `import_cert_fingerprint(partitions, snapshot_uid)` (`livegraph_feed.rs:1668-1693`) returns a string digest over
  **both** stores: per-partition `id@epoch:fresh:ts:source_inputs_hash:producer_fingerprint` (the LiveGraph side)
  **and** the `snapshot_uid` (the SQLite index epoch) and the import-completeness policy version. Its own doc:
  "Any import-relevant change (a refresh / swap / re-index / policy bump) yields a different fingerprint"
  (`:1666-1667`). This is exactly `(ready_snapshot_uid, livegraph_fingerprint)` collapsed into one comparable key.
- It is the **shared** key for all six no-loss certs (import / cycles / stats / complexity / focus-resolution /
  callgraph), each `parking_lot::RwLock<Option<…Cert>>` on `RepoState` carrying a stored `fingerprint`
  (`state.rs:195-246`; e.g. `import_cert` `:201`, `callgraph_cert` `:246`). A cert is "GREEN at the current
  fingerprint" iff its stored fingerprint equals `import_cert_fingerprint(current partitions, snapshot_uid)`
  (`callgraph_cert/mod.rs:297-309`; `livegraph_feed.rs:1797-1806`).

⇒ The epoch is **not a new type with new semantics**; it is "capture this already-computed pair at request start,
then compare against it instead of recomputing 'latest' each time." (Abstraction ledger, §12.)

### 2b. The decorator is already a serve-then-fallback seam — it just needs the pin threaded in

- `OrientServeDecorator { livegraph, inner }` (`orient_serve/mod.rs:105-116`) already wraps the SQLite port and,
  per method, serves the LiveGraph **only when the answer is `Exact`**, else delegates to `inner` (SQLite)
  (`storage_port_impl.rs:33-50,111-126`). Today the `Exact` check is the only guard; the captured-fingerprint
  validation is the **one new condition** the epoch adds to that existing branch — not a new code path.
- The decorator is constructed **only on a GREEN bounded cert** (`dispatch.rs:2938-2940`), and the green-check
  already takes a `snapshot_uid` argument (`orient_bounded_cert_is_green(repo_state, snapshot_uid)`,
  `orient_serve/mod.rs:89`). Threading the **captured** `snapshot_uid` (not a re-resolved one) into that call +
  the decorator + `orient_repo` is the bulk of the change.

### 2c. The SQLite snapshot is retained across a refresh — pinning by `snapshot_uid` is robust

- Refresh creates a **new** snapshot and flips it READY; it does **not** delete the prior READY snapshot. Old
  epochs are only **marked prunable** (`retention/classify.rs:261`), and deletion is a **separate** prune
  operation (`storage/src/retention/prune.rs`). So during a refresh **both** the old and new snapshot rows exist;
  a request pinned to `snapshot_uid_A` reads A's rows by uid (queries take a `snapshot_uid`, not "latest"
  + status filter) regardless of the READY pointer moving to B.
- The **only** deleter of A's rows is prune. Prune is rare/operator-triggered (RETENTION-POLICY-1,
  MAINTENANCE-CLI-1) and is a coordinated writer. The retention sub-decision (§8 D-RET) keeps prune an
  **exclusive** writer so a pinned reader is never pruned mid-request.

### 2d. The B1 coordination fix is shipped — W-B builds directly on it

- `livegraph_refresh` / `livegraph_preload` now acquire the repo coordinator as writers
  (`dispatch.rs:801` preload `acquire_refresh`, `:844` refresh) — the §14 #2b fix. So under W-A every store-swap
  (SQLite **and** LiveGraph) is already excluded by a reader's `acquire_read`. W-B's relaxation (§7) is therefore
  a **single, well-scoped** change to the coordinator's `Refreshing`→reader rule, layered on a model where the
  writers are already coordinated.

---

## 3. Principle (what "one coherent cross-store epoch" means here)

- **A request pins an epoch once and never reads "latest" again.** After a request begins serving, the captured
  epoch — the **atomically-pinned** READY snapshot plus its **green-validated** LG-serve eligibility fingerprint
  (§5.1) — is fixed. Every SQLite read uses the pinned `snapshot_uid`; every LiveGraph read is validated against the
  pinned fingerprint. The double-resolve (`orient/repo.rs:99`) and the per-method LG reads collapse to **one** epoch.
- **Coherence is whole-request, not per-store.** §6 of DAEMON-CONCURRENCY-1 proved per-store atomicity (WAL/READY
  for SQLite; the RwLock swap for the LiveGraph). The §14 §6 correction is binding: that is **necessary but
  insufficient**. This slice proves the **join** — trust@N ⋈ module@N ⋈ callers@N — is coherent because every
  contributor resolves to the **same** captured N.
- **Eviction degrades to the pinned SQLite snapshot, never to a mixed epoch.** The LiveGraph holds one epoch per
  partition; a refresh evicts the old one in place (§2a). A request pinned to an evicted LG epoch does **not**
  re-pin (that would mix stores) and does **not** serve the new epoch — it serves the **pinned SQLite snapshot**,
  which is the same epoch N the fingerprint was captured at. Worst case the request is W-A-equivalent (eager
  SQLite at the pinned snapshot) — coherent and correct, just without the LG fastpath for that leaf.
- **Honesty (Fact-Certainty / Layer model):** a request must never present a cross-epoch mix as coherent
  current-state truth, and must never stamp an answer with a `snapshot_uid` from a different epoch than the data.
  The captured epoch is the enforcement; the design preserves the no-loss certs' GREEN guarantee by serving the
  LiveGraph **only** while the captured fingerprint still holds.
- **Smallest design earned by demonstrated need:** the demonstrated variation is "a writer publishes epoch N+1
  while a reader is mid-request at epoch N" (the W-B race). The fingerprint pair already exists (§2a); the epoch
  is that pair captured-once + threaded. Any new abstraction beyond that must name its concrete current callers or
  be dropped (§12 ledger).

---

## 4. Desired behavior (the IMPL must deliver — concrete + checkable on a headless Test API)

1. **One captured epoch per request.** A mixed-read request resolves the snapshot exactly once; the orient
   double-resolve is gone (`orient/repo.rs` no longer calls `get_latest_snapshot` — it receives the pinned
   `&AgentSnapshot`, used for **both** `snapshot::aggregate` and `snapshot_uid`). Provable by a storage spy that
   counts `get_latest_snapshot` calls per request (== 1), and that `snapshot::aggregate`'s SNAPSHOT_INFO is the
   **captured** snapshot's metadata (files/nodes/edges totals), with no fallback re-resolve.
2. **Every SQLite read pins the captured `snapshot_uid`.** No aggregator re-resolves "latest." Provable: a spy
   whose `get_latest_snapshot` returns a DIFFERENT uid on the 2nd call cannot change the answer (it is never
   called twice).
3. **Every LiveGraph read validates the captured `fingerprint`; a swapped LG falls back to the pinned snapshot.**
   With a writer that bumps the LiveGraph epoch mid-request (so the current fingerprint ≠ captured), the serve site —
   the decorator for orient/explain (§6.1), the `*_engine_response` Auto arm for callers/callees/path (§6.3) — serves
   the pinned SQLite snapshot for that leaf, and the **whole** answer is still epoch N (the captured snapshot).
   Provable headless: capture epoch N, swap the LiveGraph to N+1 between the green-check and the serve, assert the
   served callers/path + the stamped `snapshot_uid` + the SQLite-read trust/module are **all** epoch N (no field is
   from N+1). §11 tests 1 (orient), 7 (path), 8 (callers/callees).
4. **W-B: a read completes (does not block) during a concurrent refresh, at the pinned epoch.** With the
   `Refreshing` read-block relaxed (§7), a reader pinned to N returns a fully-coherent N **while** a refresh builds
   N+1; it neither blocks for the refresh nor observes N+1 in any field. §11 test 1.
5. **E-A: a background enrich publishing N+1 is invisible to a reader pinned to N.** Same mechanism as #3/#4; the
   enrich is just another `Refreshing` writer. §11 test 2.

Non-goals for this slice's *behavior* (kept honest): cross-store **atomic commit** (making the SQLite flip and the
LiveGraph swap a single transaction — out of scope; the epoch makes that unnecessary by pinning, not by
synchronizing the writers); pinning across a **prune** that deletes the pinned snapshot (handled by keeping prune
exclusive, §8 D-RET — not by a snapshot refcount subsystem); epoch coherence for handlers that read only **one**
store (imports/cycles/stats single-leaf fastpaths already re-validate the fingerprint per call — §8 D-SCOPE).

---

## 5. Design A — epoch capture (where, when, representation)

One change, threaded through the mixed-read handlers.

### 5.1 Where + when — capture once, under the read guard, before any serve decision

In each mixed-read handler, immediately after `acquire_read()`, capture the epoch in one step that **reuses the
snapshot the serve-decision already resolves**. Today `handle_orient` resolves the full `AgentSnapshot` at
`dispatch.rs:2915-2922` only to read `s.snapshot_uid` for the green-check and **discard the rest** (`.map(|s| …
&s.snapshot_uid …)` drops `s`); the capture keeps it:

```
let _read_guard = repo_state.coordinator.acquire_read();
let storage = repo_state.storage()?;                       // S-A per-op connection (state.rs:332)

// (1) THE one snapshot resolve — the request's ATOMIC SQLite pin. One READY row,
//     captured WHOLE (not just its uid). This is the resolve dispatch.rs:2915 already
//     performs-and-discards today; it also REPLACES the second resolve inside
//     orient_repo (repo.rs:99). No-READY-snapshot returns the existing error response.
let snapshot = match storage.get_latest_snapshot(&repo_uid)? {
    Some(s) => s,
    None => return /* the existing no-READY-snapshot response, as callers/path handlers already do */,
};

// (2) The LG-serve ELIGIBILITY fingerprint — a green-validated witness, captured BUILD-THEN-PEEK
//     (§6.4): Some(fp) iff a no-loss cert is GREEN at EXACTLY the resident fingerprint for the pinned
//     snapshot_uid; None ⇒ eager SQLite. orient/explain use the BOUNDED cert (focus ∧ callgraph);
//     callers/callees/path use the CALLGRAPH cert (§5.4 / D-CC) — same helper shape, different cert.
let fingerprint = orient_bounded_cert_eligibility(&repo_state, &snapshot.snapshot_uid);

let epoch = RequestEpoch { snapshot, fingerprint };
let serve_from_lg = epoch.fingerprint.is_some();           // identical decision to today's serve_from_lg
```

**The `snapshot_uid` is the atomic pin; the fingerprint is NOT a second atomically-captured store-version — it is a
green-validated *eligibility witness*.** This is the correction review-0 required: capture reads SQLite "latest" and
the LiveGraph under different mechanisms, and W-B explicitly admits a refresh racing between them, so the pair is
**not** "coherent by construction." Two facts make it safe *without* atomic dual-capture:

1. **The `snapshot_uid` pin is genuinely atomic and load-bearing.** `get_latest_snapshot` returns one READY row at
   one instant; that `snapshot_uid` is the request's SQLite identity, its rows are retained until prune (prune
   excluded — §8 D-RET), and the request **never re-resolves "latest"**, so a concurrent flip to N+1 cannot move it.
   Every SQLite read uses this one uid.
2. **The fingerprint is an EQUIVALENCE WITNESS — what the green-check validated, not a separately-timed
   store-version read.** `orient_bounded_cert_eligibility` returns `Some(import_cert_fingerprint(current partitions,
   snapshot_uid))` **iff**, under one read guard, a stored cert exists at exactly that fingerprint with verdict GREEN
   (`orient_serve/mod.rs:89-92`; `callgraph_cert/mod.rs:297-348`; `livegraph_feed.rs:1797-1806`). A GREEN cert is built
   **lazily, on demand, against whatever partitions are resident at build time**: `callgraph_is_green` computes the
   current fingerprint and, on `StaleOrMissing`, calls `build_and_store_callgraph_cert`, which runs the field-exact
   no-loss compare of **those resident partitions** against SQLite@`snapshot_uid` and stores the verdict keyed by that
   fingerprint (`callgraph_cert/mod.rs:220-267,272-326`). So GREEN at `(fp, snapshot_uid)` proves **the partition-set
   digested into `fp` is no-loss-equal to SQLite@`snapshot_uid`** — an *equivalence established by the cert's own
   compare*, **not** a token tied to "the publish that produced snapshot N" (any caller can rebuild the cert against the
   current partitions). Therefore `Some(fp)` witnesses that the resident partitions are **cert-proven substitutable for
   SQLite@`snapshot_uid`**; serving them is as-of `snapshot_uid`, regardless of which publish produced them. (`None` =
   no green cert at the resident fingerprint ⇒ eager SQLite, the same routing W-A uses.)

**Coherence when capture straddles a publish (the proof review-0 asked for — corrected for the lazy cert rebuild,
review-2).** Suppose a refresh commits between the snapshot read and the eligibility read — the exact race W-B admits.
The eligibility read then runs **build-then-peek** (§6.4) against the *new* resident partitions, paired with the
*pinned* `snapshot_uid_N`. Because the cert is rebuilt lazily against those resident partitions, there are **two**
coherent outcomes — not the single unconditional `None` an earlier draft claimed:

   - **(a) The racing publish changed import/CALLS-relevant content**, so the new partitions are **not** no-loss-equal
     to SQLite@`snapshot_uid_N`. The lazy compare yields RED ⇒ no GREEN cert at the resident fingerprint ⇒
     `orient_bounded_cert_eligibility` returns **None** ⇒ `serve_from_lg = false` ⇒ the request runs the eager SQLite
     path **at `snapshot_uid_N`**. All-SQLite at N.
   - **(b) The racing publish left the import/CALLS-relevant content equal** to SQLite@`snapshot_uid_N` (a refresh that
     re-fed identical CALLS, or touched only unrelated partitions). The lazy compare yields GREEN ⇒ build-then-peek
     returns **`Some(fp)`** at the *new* resident fingerprint ⇒ the request may serve those LG partitions — **which are
     cert-proven no-loss-equal to SQLite@`snapshot_uid_N`**, hence as-of N.

   Either way the request is coherent at `snapshot_uid_N`: every SQLite read is at N, and any LG serve is cert-proven
   equal to SQLite@N. The straddle is **never served as a mix** — it is downgraded to SQLite@N (a) or served as an LG
   partition-set proven equal to SQLite@N (b). ∎ (The per-leaf extension — a swap *after* a green capture — is
   §6.1/§6.2; the executable form is §11 tests 3b + 9.)

(Local mechanism, decided-and-recorded: `orient_bounded_cert_eligibility` is `orient_bounded_cert_is_green`
evolved to return its validated fingerprint as `Option<String>` instead of `bool` — a return-type change to a
`daemon-runtime`-internal helper, not a boundary; `serve_from_lg` is then `epoch.fingerprint.is_some()`. The
non-orient SC-A handlers use a sibling `callgraph_cert_eligibility` of the **same** shape over the callgraph cert,
§5.4. **Both are captured BUILD-THEN-PEEK so the returned fingerprint is the EXACT resident-and-validated state, or
`None` — the precise semantics, and why a lazy cert rebuild cannot make the witness lie, are §6.4** (review-1 #4).)

### 5.2 Representation — a request-scoped `RequestEpoch` value (the recommendation; D-EP)

```
pub struct RequestEpoch {
    /// The pinned READY snapshot, resolved ONCE — the ATOMIC SQLite pin. Carried WHOLE
    /// (not just its uid) so orient_repo keeps `snapshot::aggregate(&snapshot)` (repo.rs:113)
    /// without a second resolve. Every SQLite read uses `snapshot.snapshot_uid`.
    pub snapshot: AgentSnapshot,
    /// The LG-serve eligibility witness (§5.1): Some(fp) = the green-check validated
    /// `import_cert_fingerprint(partitions, snapshot.snapshot_uid)` against a stored GREEN
    /// cert at capture; None = no green cert ⇒ eager SQLite, no LiveGraph serve.
    pub fingerprint: Option<String>,
}

impl RequestEpoch {
    /// The pinned SQLite identity — every SQLite read and the response stamp use THIS.
    pub fn snapshot_uid(&self) -> &str { &self.snapshot.snapshot_uid }
}
```

A plain value, captured once, passed by reference into: the `OrientServeDecorator`, `orient_repo` /
`orient_cancellable` (replacing their internal `get_latest_snapshot`), and the response-stamping site. It is **not**
a guard with Drop semantics and **not** a new storage port. **Why carry the whole `AgentSnapshot`, not just the uid
(the review-0 #1 buildability fix):** `orient_repo` feeds the full snapshot to `aggregators::snapshot::aggregate(&snapshot)`
(`repo.rs:113`) for the SNAPSHOT_INFO signal; passing only the uid would force `orient_repo` to re-resolve the very
snapshot it was meant to stop resolving (or invent an extra fetch). Carrying the whole snapshot lets it **both** skip
the resolve **and** keep the aggregator; `snapshot_uid()` is a derived accessor — one source of truth, no second
string to drift. `AgentSnapshot` is the **agent crate's own DTO** (`agent/src/storage_port.rs:65`, `#[derive(Clone)]`),
already returned by `get_latest_snapshot` and already imported by `daemon-runtime` (`storage_port_impl.rs:15`), so
carrying it introduces **no new cross-boundary data shape** — `RequestEpoch` is a daemon-local value holding a type
that already crosses the port. The exact representation (this value vs. a pinned pair threaded through a new
storage-port method) is **D-EP** (§8); recommendation **the value**, because the snapshot is already resolved (and
discarded) at the serve-decision and needs no port surface.

### 5.3 Threading — eliminate the double-resolve (carry the pinned snapshot, not just the uid)

- `agent/src/orient/mod.rs:82` (`orient_cancellable`) + `agent/src/orient/repo.rs:84-106` (`orient_repo`): add a
  `snapshot: &AgentSnapshot` parameter and **delete** the internal `get_latest_snapshot` (`repo.rs:99-106`).
  `orient_repo` uses the passed snapshot for **both** `aggregators::snapshot::aggregate(&snapshot)` (`repo.rs:113`,
  unchanged) **and** `snapshot_uid = &snapshot.snapshot_uid` (`repo.rs:106`, now derived from the param). This is the
  **named** double-resolve elimination (§14 "eliminate the double snapshot-resolve in `orient/repo.rs`"); the
  **buildable** form carries the whole snapshot (review-0 #1), because the aggregator needs the full DTO, not the uid.
  The focused pipelines already take `&snapshot` (`orient_focused` resolves at `mod.rs:118-123` then passes
  `&snapshot` into `orient_file`/`orient_symbol`/`orient_path` — `mod.rs:131-139`), so only that one top-level resolve
  moves out; the pipelines are unchanged.
- The daemon (`dispatch.rs:2941/2950`) passes `&epoch.snapshot` into `orient_cancellable`; the CLI wrapper
  (`orient`, `mod.rs:59-69`) resolves the snapshot once and passes it — the same injection pattern as `now` (the use
  case never reads "latest" itself). `OrientError::NoSnapshot` (today raised inside `orient_repo`/`orient_focused`)
  moves to the two callers, which already resolve the snapshot before calling — the daemon's sibling handlers already
  `match get_latest_snapshot { None => <error> }` (`dispatch.rs:968,1022,1115,…`), so `orient_repo` becomes total
  over a provided snapshot with no new error surface.
- `dispatch.rs:2915-2922`: the serve-decision **is** the capture now —
  `orient_bounded_cert_eligibility(&repo_state, epoch.snapshot_uid())` yields the witness and
  `serve_from_lg = epoch.fingerprint.is_some()` (the same boolean the old `orient_bounded_cert_is_green(...).unwrap_or(false)`
  produced).
- `dispatch.rs:1949-1960` (`path`) + the orient/explain envelope stamp: stamp `epoch.snapshot_uid()` (identical
  value today, but now provably the captured one, not a re-resolve).

The agent crate stays pure: `orient_repo` already took its snapshot from a resolve it owned; this moves the resolve
**out** to the daemon (the composition root) and passes the DTO in — a dependency-direction improvement (matching
`now`), not a new seam. No new trait method, no new cross-crate type — `AgentSnapshot` is the agent crate's own DTO.

### 5.4 Capture for `path` / `callers` / `callees` (review-1 #1) — no double-resolve to remove; pin the LG serve

The three non-orient SC-A handlers are **structurally simpler** than orient, confirmed against code: each already
resolves the snapshot **exactly once** under its read guard and threads `snapshot.snapshot_uid` to every SQLite read,
so **there is no double-resolve to eliminate** and their **SQLite side is already epoch-pinned**:

| Handler | One snapshot resolve | SQLite reads pinned to that uid | LG serve today (the gap) |
|---|---|---|---|
| `callers` | `dispatch.rs:1022` | `resolve_symbol` `:1040`, `find_direct_callers` `:1077` | `callers_engine_response` Auto → `livegraph_callers_auto` (per-call `Exact+Fresh+TS-only`, **no cert**) `livegraph_feed.rs:485-491,355-372,231-246` |
| `callees` | `dispatch.rs:1115` | `resolve_symbol` `:1133`, `find_direct_callees` `:1170` | `callees_engine_response` Auto → `livegraph_callees_auto` (same) `livegraph_feed.rs:581-587,375-392` |
| `path` | `dispatch.rs:1847` | `resolve_symbol` `:1866,1889`, `find_shortest_path` `:1951`, **stamp** `:1960` | `path_engine_response` Auto → `livegraph_path_cancellable` (per-call `Exact+Fresh+TS-only`, **no cert**) `livegraph_feed.rs:875-882,684-702,729-755` |

So the capture is the **same one-step shape as §5.1**, dropped in right after each handler's existing
`get_latest_snapshot`, with the **callgraph** cert as the eligibility basis (not the bounded cert — these handlers do
no focus resolution; they serve / traverse **CALLS** rows, whose no-loss proof is the callgraph cert
`callgraph_cert/mod.rs:220-267`):

```
// callers/callees/path — immediately after the existing single `get_latest_snapshot`:
let snapshot = /* the handler's existing one resolve — dispatch.rs:1022 / 1115 / 1847 */;
let fingerprint = callgraph_cert_eligibility(&repo_state, &snapshot.snapshot_uid);   // §6.4 build-then-peek
let epoch = RequestEpoch { snapshot, fingerprint };
```

`callgraph_cert_eligibility` is the §6.4 build-then-peek helper over the **callgraph** cert (the sibling of
`orient_bounded_cert_eligibility`): `Some(fp)` iff a GREEN callgraph cert exists at exactly the resident fingerprint
for the pinned `snapshot_uid`, else `None`. The handler then threads `&epoch` into its `*_engine_response` (§6.3 adds
the one serve-time gate inside those functions). **No agent-crate change** for these three — `callers_engine_response`
/ `callees_engine_response` / `path_engine_response` live in `daemon-runtime` (`livegraph_feed.rs`), so the epoch is
threaded daemon-locally; the blast radius is strictly smaller than orient's (which touches the agent use-case
signature). The single snapshot resolve each already performs **is** the atomic SQLite pin — nothing moves.

**Why the CALLGRAPH cert is the right basis (and a behavior change → D-CC).** Today these handlers serve the LG on a
**per-call** `Exact+Fresh+TS-only` envelope that is computed purely from the resident partitions (`auto_outcome`
`livegraph_feed.rs:231-246` / `path_auto_outcome` `:729-755`) and **carries no `snapshot_uid` binding**. Under W-A
that is safe only because the `Refreshing` read-block guarantees the resident LG and the pinned snapshot are a
**steady-state (coherent) pair** for the request's whole duration. Under W-B that guarantee is gone: a reader admitted
mid-refresh can see a resident LG from a **different publish** than `snapshot_uid` — a per-call `Exact` answer that is
internally consistent but **not** proven equal to SQLite@`snapshot_uid`. The **only** existing oracle that proves
"resident partitions are no-loss-coherent with this `snapshot_uid`" is the **callgraph no-loss cert** (it field-exact
multiset-compares every corpus symbol's callers/callees, LG vs SQLite@uid — `callgraph_cert/mod.rs:220-267`). So
gating the LG serve on that cert is what makes these handlers coherent under W-B. The cost — they fall back to SQLite
when the **repo-wide** callgraph cert is RED, where today they serve per-call — is a genuine behavior change, surfaced
as **D-CC** (§8) with the cheaper-but-per-call alternative (a per-symbol compare) and the SQLite-only alternative.

---

## 6. Design B — the consistency rule (SQLite pins the uid; the LiveGraph validates the fingerprint)

| Store | Read rule under the captured epoch | Mechanism (already exists) |
|---|---|---|
| **SQLite** | every read uses `epoch.snapshot_uid()`; **never** re-resolve "latest" | queries are `snapshot_uid`-parameterized (`snapshots.rs:121`); the pinned snapshot is retained across refresh (§2c) |
| **LiveGraph** | serve **only** while `import_cert_fingerprint(current partitions, epoch.snapshot_uid()) == epoch.fingerprint` under the **same** read guard as the data read; else delegate to SQLite at `epoch.snapshot_uid()` | orient/explain: the decorator's `Exact`→serve / else→delegate branch gains the check (§6.1, `storage_port_impl.rs:33-50,111-126`); callers/callees/path: the `*_engine_response` Auto arm gains the check (§6.3) |

**One rule, two serve sites.** The LiveGraph rule is **identical** for all five SC-A handlers — *serve LG iff the
captured green-validated fingerprint still equals the resident one, else SQLite at the pin* — but it lands at two
sites: the **decorator** for orient/explain (§6.1, the accepted iteration-1 mechanism, unchanged) and the
**`*_engine_response` Auto arm** for callers/callees/path (§6.3, new). §6.2 proves the join coherent per handler;
§6.4 specifies the build-then-peek capture that makes `epoch.fingerprint` an honest witness (review-1 #4).

### 6.1 The LiveGraph validation for orient/explain — the decorator (the one new condition)

The decorator gains an `epoch: &'a RequestEpoch` field (added to `OrientServeDecorator` + its `new`,
`orient_serve/mod.rs:105-115`; constructed at `dispatch.rs:2940` from the captured epoch). On each LG serve method,
before returning LG rows, it confirms the LiveGraph has **not** swapped out from under the captured epoch:

```
let guard = self.livegraph.read();
if let Some(lg) = guard.as_ref() {
    let current_fp = import_cert_fingerprint(&lg.live_partitions(), self.epoch.snapshot_uid());
    if Some(&current_fp) == self.epoch.fingerprint.as_ref() {
        // captured epoch still resident → serve LiveGraph (Exact, as today)
        if let Some(rows) = lg_caller_rows(lg, symbol_stable_key) { return Ok(rows); }
    }
}
// captured LG epoch evicted/swapped, OR not Exact → delegate to SQLite at the PINNED snapshot
self.inner.find_symbol_callers(self.epoch.snapshot_uid(), symbol_stable_key)
```

`epoch.fingerprint` is the **equivalence witness** (§5.1, captured build-then-peek so it is the *exact*
resident-and-validated state or `None` — §6.4 closes review-1 #4): it was `Some` only because, at capture, a stored
GREEN cert existed at exactly the resident fingerprint for `snapshot_uid_N` — **proving those partitions no-loss-equal
to SQLite@`snapshot_uid_N`**. At serve, `current_fp = import_cert_fingerprint(resident partitions, snapshot_uid_N)`;
because that digest embeds each partition's epoch + content hashes (`livegraph_feed.rs:1672-1685`) and partition epochs
are **monotonic** (a refresh bumps the epoch in place — `livegraph_refresh.rs:354-364` — and never restores an old one,
so a fingerprint never recurs), `current_fp == epoch.fingerprint` holds **iff the resident partition-set is
byte-identical to the one captured-and-proven** — the same cert-validated partitions, still resident. Serving them is
therefore **substitutable for SQLite@`snapshot_uid_N`** (the cert proved the equality), so the serve is as-of N. This
captured partition-set need **not** be "snapshot N's own publish": under a capture straddle (§5.1 case b) it may be a
*later* partition-set proven equivalent to SQLite@`snapshot_uid_N` — **coherence comes from the proven equality, not
from epoch-identity**. Once a writer swaps to partitions not equal to the captured set, `current_fp` differs and the
equality can never spuriously re-appear. Because the decorator is constructed only on a green capture, the match branch
is the steady state and serves byte-identically to today. The `else` branch fires **only** after such a swap — and it
delegates to SQLite **at the pinned `snapshot_uid_N`**. Either branch keeps the request at N. (Note: `lg_caller_rows`
itself stays fingerprint-free — `callgraph_cert/mod.rs:117-160` — the validation lives in the decorator, where the
captured epoch is in scope; no producer-crate change.)

### 6.2 §6 amendment (binding) — whole-request join coherence

DAEMON-CONCURRENCY-1 §6 proves **per-store** atomicity. This slice adds the **whole-request** proof the §14 §6
correction requires. Let N = the captured epoch: `snapshot_uid_N` (the atomically-pinned READY snapshot — the
request's **immutable anchor**) and `fingerprint_N` (the **equivalence witness**, §5.1 — `Some(fp)` where a GREEN
no-loss cert proves the partition-set digested into `fp` is no-loss-equal to SQLite@`snapshot_uid_N`, or `None`).

1. **SQLite contributors are all at N.** `snapshot_uid_N` is captured by one atomic READY-row resolve (§5.1) and
   threaded to every SQLite read (§5.3); the rows for `snapshot_uid_N` are retained until prune, and prune is
   excluded (§8 D-RET). ⇒ trust, cycles, MODULE_SUMMARY, boundary, dead-code, gate, **and `snapshot::aggregate`'s
   SNAPSHOT_INFO** are **all** read at N (the last from the captured `&AgentSnapshot` itself — no re-resolve).
2. **LiveGraph contributors are proven equal to SQLite@N, or fall back to SQLite@N.** Each LG serve validates
   `current_fp == fingerprint_N` under the data read guard — at the **decorator** for orient/explain (§6.1) and at the
   **`*_engine_response` Auto arm** for callers/callees/path (§6.3). `fingerprint_N` is `Some` only because,
   build-then-peek (§6.4), a GREEN cert existed at exactly the resident fingerprint for `snapshot_uid_N` — **proving
   those partitions no-loss-equal to SQLite@`snapshot_uid_N`** — and partition epochs are monotonic (fingerprints never
   recur), so `current_fp == fingerprint_N` holds **iff** the resident partition-set is the *same cert-validated
   partitions* captured for `snapshot_uid_N` (which may be a partition-set published after N, proven equivalent — §5.1
   case b — not necessarily N's own publish). Serving them is **substitutable for SQLite@`snapshot_uid_N`** ⇒ as-of N.
   If the equality does not hold (a mid-request swap to partitions not proven equal), the leaf delegates to SQLite at
   `snapshot_uid_N` ⇒ as-of N. ⇒ no LG contributor is ever at an epoch ≠ N; every served LG value is proven equal to
   SQLite@N.
3. **The stamp is N.** The response `snapshot_uid` is `epoch.snapshot_uid()` = `snapshot_uid_N` (§5.3) ⇒ the
   freshness label matches the data.
4. **Therefore the join is coherent.** Every field of the answer resolves to the same N; no cross-epoch mix
   survives. ∎

The proof does **not** depend on the SQLite flip and the LiveGraph swap being mutually atomic (they are not, §1a),
**nor on the snapshot read and the eligibility read inside capture being atomic** (they are not, and W-B admits a
publish between them, §5.1). It depends only on (a) the request **pinning** one `snapshot_uid` atomically and never
re-resolving "latest," and (b) serving the LiveGraph **only** while the green-validated witness still holds (i.e. the
served partitions are cert-proven equal to SQLite@`snapshot_uid_N`), else **degrading to the pinned SQLite snapshot**.
A capture that straddled a publish yields either `fingerprint_N = None` (the whole request goes eager-SQLite at the
pin — §5.1 case a) or `Some(fp)` for a partition-set the cert proves equal to SQLite@`snapshot_uid_N` (served,
equivalently — §5.1 case b); and post-capture, a per-leaf fingerprint mismatch sends that leaf to eager-SQLite at the
pin — every path resolves to epoch N, never a mix. That is the load-bearing difference from §6's per-store argument.

#### 6.2a Per-handler instantiation (review-1 #2 — the proof for EACH SC-A handler, not only orient)

The §6.2 argument is generic; here is its instantiation for every SC-A handler, naming the contributors each joins
and where the LG gate lands. In every row, **every** named contributor resolves to N, so the handler's answer is a
coherent N (or it fails-soft to the existing no-snapshot error).

| Handler | SQLite contributors @ `snapshot_uid_N` | LG-served contributors (gated @ `fingerprint_N`, else SQLite@N) | Stamp | Cross-epoch mix it closes |
|---|---|---|---|---|
| `orient` | trust, cycles, MODULE_SUMMARY, boundary, dead-code, gate, SNAPSHOT_INFO (`repo.rs:113-151`) | focus-resolution + callers/callees rows via the decorator (§6.1) | envelope `snapshot_uid` | §1b: MODULE_SUMMARY@N ⋈ callers@N+1 ⋈ cert-proven-for-a-third |
| `explain` | trust, cycles, `compute_*_summary` (delegated) | focus-resolution + callers/callees rows via the **same** decorator (`dispatch.rs:3274`) | envelope `snapshot_uid` | same as orient (shared decorator double-resolve) |
| `callers` | `resolve_symbol` → `target` (`:1040`); fallback `find_direct_callers` (`:1077`) | the callers key-set via `callers_engine_response` Auto (§6.3) | (no `snapshot_uid` stamp; `target` IS the SQLite-pinned identity) | `target`@N ⋈ callers@N+1 (a renamed/moved symbol's edges from a newer epoch beside an N-resolved target) |
| `callees` | `resolve_symbol` → `target` (`:1133`); fallback `find_direct_callees` (`:1170`) | the callees key-set via `callees_engine_response` Auto (§6.3) | (as callers) | `target`@N ⋈ callees@N+1 |
| `path` | `resolve_symbol` → `from`/`to` (`:1866,1889`); fallback `find_shortest_path` (`:1951`) | the BFS node path via `path_engine_response` Auto (§6.3) | `snapshot_uid` (`:1960`) | §1c: a BFS as-of LG@N+1 **stamped** with `snapshot_uid_N` — a false-freshness label |

**`callers`/`callees` clause.** The `target` (its `stable_key`, `name`, `file`, `line`) is resolved from SQLite at
`snapshot_uid_N` (`:1040`/`:1133`); the served caller/callee key-set is gated @ `fingerprint_N` (§6.3) ⇒ either the
LG set (cert-proven equal to SQLite@`snapshot_uid_N` — the green callgraph cert IS that multiset equality,
`callgraph_cert/mod.rs:191-212`) or, on mismatch/`None`, the SQLite set via the lazy closure at `snapshot_uid_N`
(`:1077`/`:1170`). Both the target and the edge-set are therefore at N — no `target`@N ⋈ edges@N+1 split. (There is
no `snapshot_uid` stamp on these two; the `target` is the pinned identity, so there is no separate freshness label to
falsify.)

**`path` clause.** `from`/`to` resolve at `snapshot_uid_N`; the served BFS is gated @ `fingerprint_N` (§6.3) ⇒ either
the LG BFS (the green callgraph cert proves the LG CALLS edges are no-loss equal to SQLite@`snapshot_uid_N`, so the
BFS is as-of N) or the SQLite `find_shortest_path` at `snapshot_uid_N`. **Either way the stamped
`"snapshot_uid": snapshot_uid_N` (`:1960`) labels data that is genuinely as-of N** — closing the §1c false-freshness
stamp (the LG BFS can no longer be as-of N+1 while wearing an N label).

### 6.3 The LiveGraph validation for `callers`/`callees`/`path` — the `*_engine_response` Auto arm (review-1 #1)

These three do not use the decorator; their LG serve is the `Engine::Auto` arm of `callers_engine_response` /
`callees_engine_response` / `path_engine_response` (`livegraph_feed.rs:485-491,581-587,875-882`). The **one new
condition** lands there, mirroring §6.1: the epoch is threaded in, and the Auto arm gates its existing per-call serve
on `current_fp == epoch.fingerprint`, computed **under the same read guard** that reads the LG envelope/BFS (so the
gate and the data read are atomic w.r.t. a swap — the swap takes `livegraph.write()`). On mismatch/`None`, it returns
the existing lazy SQLite fallback at `epoch.snapshot_uid()` (the closure the handler already binds to
`snapshot.snapshot_uid`). For `callers` (callees/path symmetric):

```
// inside livegraph_callers_auto (livegraph_feed.rs:355) — the SAME guard that builds the envelope:
let guard = repo_state.livegraph.read();
let lg = guard.as_ref()?;                                   // None ⇒ SQLite fallback (existing)
// NEW epoch gate: the resident LG must still be the captured green-validated epoch.
let current_fp = import_cert_fingerprint(&lg.live_partitions(), epoch.snapshot_uid());
if Some(&current_fp) != epoch.fingerprint.as_ref() {
    return None;                                            // swapped / straddled / no green cert ⇒ SQLite@pin
}
let env = lg.callers(target, Granularity::CallerDetail);   // unchanged Exact+Fresh+TS-only reduction follows
...
```

`return None` routes through the unchanged `auto_outcome` → `callers_auto_or_sqlite` fallback, which calls
`sqlite_fetch()` — already bound to `find_direct_callers(&snapshot.snapshot_uid, …)` (`dispatch.rs:1077`) — i.e.
SQLite **at the pin**. The served `backend_used` already records `livegraph` vs `sqlite`; the downgrade is observable
without new fields (a dedicated `fallback_reason::EpochSuperseded` is an optional honesty nicety, not required —
**decided-and-recorded**, not a surfaced decision). `path` is identical inside `livegraph_path_cancellable`
(`:684`), guarded once before the BFS so a swapped epoch falls to `find_shortest_path` at the pinned uid and the
`:1960` stamp stays truthful (§6.2a path clause).

Because `epoch.fingerprint` for these handlers is the **callgraph**-cert witness (§5.4), `current_fp ==
epoch.fingerprint` holds **iff** the resident partitions are still the *exact cert-validated set* proven green for
`snapshot_uid` — which proves (callgraph green ⇒ multiset parity, `callgraph_cert/mod.rs:191-212`) that the LG
callers/callees/CALLS-edges are no-loss equal to SQLite@`snapshot_uid`. So the served LG answer is substitutable for
SQLite@`snapshot_uid` — coherent with every SQLite contributor (§6.2a), **by the cert's proven equality, not by
epoch-identity** (the resident set may be a later partition-set proven equivalent — §5.1 case b). The existing per-call `Exact+Fresh+TS-only`
check (`auto_outcome`/`path_auto_outcome`) becomes **subsumed** by the stronger cert gate but is kept in place
(belt-and-suspenders; no engine_response logic deleted).

### 6.4 Eligibility-helper semantics under lazy cert rebuilds (review-1 #4) — build-then-peek

Review-1 #4: *the returned fingerprint must be the exact resident LiveGraph state validated for the pinned snapshot,
or the helper must return `None` and force the pinned-SQLite fallback.* The hazard is real and W-B-specific:
`callgraph_is_green` and `focus_resolution_is_green` both compute `current_fp` under one read guard, **drop it**, then
lazily (re)build the cert under a **re-locked** guard (`callgraph_cert/mod.rs:300-323`; `focus_resolution_cert/mod.rs:392-422`
— the drop-relock is deliberate, parking_lot is non-reentrant). Under W-A the read-block makes that window swap-free;
under W-B a publish can land in it, so the rebuilt cert can be **keyed at the pre-swap fingerprint but its verdict
computed over post-swap partitions** — a mislabel. A captured witness taken naïvely from such a helper could claim
"GREEN at fp X" while the verdict was actually over fp Y.

The eligibility helpers close this by **build-then-peek**, returning an honest witness or `None`:

```
fn callgraph_cert_eligibility(repo_state, snapshot_uid) -> Option<String> {
    let _ = callgraph_is_green(repo_state, snapshot_uid);          // 1. WARM: lazy (re)build if stale/missing
    let guard = repo_state.livegraph.read();                      // 2. one guard for BOTH reads below:
    let current_fp = import_cert_fingerprint(&guard.as_ref()?.live_partitions(), snapshot_uid);
    let cached = repo_state.callgraph_cert.read();
    match cached.as_ref() {                                        // 3. PEEK a GREEN cert at EXACTLY current_fp
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN" => Some(current_fp),
        _ => None,                                                 //    else ⇒ pinned-SQLite
    }
}
// orient_bounded_cert_eligibility is identical over the BOUNDED cert: warm orient_bounded_cert_is_green, then peek
// BOTH focus_resolution_cached_green AND callgraph_cached_green at current_fp (Some(current_fp) iff both GREEN).
```

The peek (step 3) reads the cached cert and `current_fp` under **one** read guard, so the "(is there a GREEN cert) at
(this exact resident fingerprint)" question is answered atomically w.r.t. any swap. Therefore `Some(fp)` ⇒ a GREEN
cert exists at exactly the partitions resident *now*, paired with `snapshot_uid` — **the returned fingerprint IS the
exact resident-and-validated state**, review-1 #4 satisfied verbatim. If the warm-build straddled a swap (its cert
keyed at the old fp), step 3's peek at the new `current_fp` finds no GREEN cert there ⇒ `None` ⇒ pinned-SQLite.

**Why the mislabel is otherwise harmless (the monotonic-epoch argument).** A refresh bumps the partition epoch **in
place and never restores a prior epoch** (`livegraph_refresh.rs:354-364`), and `import_cert_fingerprint` embeds
`p.epoch` (`livegraph_feed.rs:1676-1684`), so each epoch's fingerprint is **unique and never recurs**. A mislabeled
cert keyed at fp X (verdict over fp Y, Y a later epoch) can only be peek-matched when the resident fingerprint equals
fp X again — which monotonicity forbids once the epoch has moved to Y. So a stray mislabeled cert is never validly
served regardless. Two consequences: (a) the build-then-peek above is **sufficient** on its own; (b) an **optional
honest-build hardening** — have `build_and_store_*_cert` key the stored cert at the fingerprint of the partitions it
**actually compared** (computed under the compare's own guard, `callgraph_cert/mod.rs:221-266`), so no mislabeled cert
is ever stored — is **defense-in-depth, decided-and-recorded** (a local change to the build helpers; no boundary, no
decision). The new peek `focus_resolution_cached_green` is a verbatim mirror of the existing `callgraph_cached_green`
(`callgraph_cert/mod.rs:338-348`) — a trivial addition, named here so the IMPL adds it rather than inventing one.

---

## 7. Design C — W-B re-enablement (relax the `Refreshing` read-block) + E-A composition

### 7.1 The coordinator change (W-B)

Today `try_acquire_read` returns `Blocked` for `Writing | Refreshing` (`daemon-policy/src/state.rs:80-84`). W-B
relaxes **only** the `Refreshing` arm so readers proceed while a refresh/enrich builds the next epoch; `Writing`
(index, prune) **stays** read-excluding. Concretely the coordinator gains a state in which a refresh is in flight
**and** readers are admitted — readers pinned to their captured epoch, the refresh building N+1 under its own LG
write lock + SQLite connection. The exact representation (a `RefreshingWithReaders(n)` state vs. a flag on
`Refreshing`) is local mechanism, **decided-and-recorded by the IMPL**, not a surfaced decision — it changes no
data shape crossing a boundary; the **contract** change (readers no longer excluded by `Refreshing`) is the
ratifiable part and is **D-WB** (§8).

This is safe **because of the captured epoch**, not on its own: §6.2 proves a reader admitted during `Refreshing`
sees a coherent N. Without the epoch, this relaxation is exactly the split-brain §14 withdrew — so D-WB is
**ratifiable only jointly with D-EP + D-EV** (the epoch + its eviction rule). The conservative fallback is W-A
(keep the block) — already shipped; this slice's entire purpose is to earn the relaxation.

### 7.2 E-A composition (no enrich-specific mechanism)

A background enrich (ENRICH-LIFECYCLE-1) is, in the existing model, **just another `Refreshing` writer**: it takes
the DB write lock + coordinator refresh state and swaps the LiveGraph under `livegraph.write()`, flipping the
snapshot BUILDING→READY (DAEMON-CONCURRENCY-1 §12.3). Under the captured epoch, an enrich publishing N+1 is
identical to a refresh publishing N+1 (§6.2): a reader pinned to N reads SQLite at `snapshot_uid_N` and validates
LG against `fingerprint_N`, serving N or falling back to SQLite N. The enrich's N+1 is **invisible** to the pinned
reader. ⇒ E-A composes via the **same** epoch + W-B seam — **zero** enrich-specific code in this slice (D-EA, §8;
re-ratifies DAEMON-CONCURRENCY-1 §8 D-E = E-A, which §14 deferred to here).

---

## 8. Decisions to surface (DECISION_REQUIRED — operator ratifies; the IMPL does NOT re-decide)

Each is an exhaustive matrix + a defensible recommendation + a blocking reason. The IMPL executes only the
ratified cells. This spec runs through the relay's decision-review phase; the matrices are the options audit trail.

DECISION_REQUIRED:
- ID: D-EP-CAPTURE-REPRESENTATION
  QUESTION: Where is the epoch captured, and what is its representation?
  OPTIONS:
  - EP-A Request-scoped `RequestEpoch { snapshot: AgentSnapshot, fingerprint: Option<String> }` value (with a
    `snapshot_uid()` accessor), captured once in each mixed-read handler right after `acquire_read`, threaded by
    reference into the decorator (orient/explain) or the `*_engine_response` (callers/callees/path) + `orient_repo`/
    `orient_cancellable` + the stamp (RECOMMENDED). Smallest: the snapshot is **already resolved** at each handler's
    serve-decision (`dispatch.rs:2915` orient; the single resolve at `:1022/1115/1847` for callers/callees/path) and
    the `fingerprint` is the green-check's own output, captured **build-then-peek** (§6.4) via
    `orient_bounded_cert_eligibility` (orient/explain — bounded cert) or `callgraph_cert_eligibility`
    (callers/callees/path — callgraph cert, D-CC); no storage-port surface, no Drop/guard semantics, no new crate, no
    new cross-boundary type (`AgentSnapshot` is the agent crate's own DTO, already crossing the port). Consequence: one
    small `pub struct` + a `capture` step in `daemon-runtime`; `orient_repo` gains a `snapshot: &AgentSnapshot` param
    (the **buildable** double-resolve elimination — carries the whole snapshot so `snapshot::aggregate(&snapshot)`
    `repo.rs:113` is preserved; review-0 #1); callers/callees/path thread `&epoch` daemon-locally into their
    `*_engine_response` (no agent-crate change, §5.4). Passing only `snapshot_uid` is **rejected as non-buildable**: it
    would force `orient_repo` to re-resolve the snapshot the aggregator needs.
  - EP-B A pinned pair threaded through a NEW `AgentStorageRead` method (e.g. `pinned_epoch()`), so the agent crate
    asks the port for "the epoch" instead of receiving it. Consequence: a new trait method on the boundary the
    daemon and tests both implement — an architecture-boundary surface change for no gain over passing a value;
    couples the agent crate to an epoch concept it does not need.
  - EP-C An `EpochGuard` RAII type that also holds the coordinator read guard. Consequence: conflates "the captured
    facts" with "the lock lifetime"; the read guard is already an explicit local in each handler — bundling them
    obscures the lock and adds Drop semantics the captured value (a snapshot + a fingerprint) does not need.
  RECOMMENDED: **EP-A** (the value, captured in the handler, threaded by reference). Reject EP-B (unearned boundary
    method) and EP-C (conflated lifetimes).
  BLOCKING_REASON: Determines whether a new **data shape crosses the storage-port boundary** (EP-B) or stays a
    daemon-local value (EP-A) — an architecture-boundary choice (CLAUDE.md "data shape crossing a boundary"). The
    IMPL cannot thread the epoch without it.

- ID: D-EV-EVICTION-POLICY
  QUESTION: When the captured LiveGraph epoch has been swapped/evicted mid-request (current fingerprint ≠ captured),
    what does a LiveGraph leaf do?
  OPTIONS:
  - EV-A Serve the **pinned SQLite snapshot** (`epoch.snapshot_uid()`) for that leaf — per-leaf fail-soft to the
    eager path (RECOMMENDED). The whole request stays at epoch N (the captured snapshot is N). Requires **no** LG
    retention (we never read the old LG epoch — we detect it is gone and use SQLite). Strictly coherent.
    Consequence: a refresh mid-request makes some leaves serve from SQLite instead of the LG fastpath (slightly
    slower, still correct + coherent) — a rare race window, not the steady state.
  - EV-B **Re-pin** to the new epoch (N+1) and continue. REJECTED: the SQLite reads already done at
    `snapshot_uid_N` are now inconsistent with an LG read at N+1 — this **reintroduces** the split-brain mid-request.
    Listed to be explicit it is unsafe.
  - EV-C **Fail-soft to W-A for the whole request** — on the first eviction, abandon the LG path and re-run the
    request eagerly from SQLite at the pinned snapshot. Coherent, but strictly worse than EV-A (discards the leaves
    already served correctly from the LG; an all-or-nothing fallback where per-leaf suffices). Acceptable only if
    per-leaf mixing of LG-served and SQLite-served leaves within one (coherent) epoch is judged undesirable for
    output-uniformity reasons.
  RECOMMENDED: **EV-A** (per-leaf fall back to the pinned SQLite snapshot). It is the smallest, needs no LG
    retention, and keeps the request coherent at N.
  BLOCKING_REASON: Sets the request's correctness behavior under the exact W-B race the slice exists to make safe,
    and (with D-RET) determines whether any LiveGraph-retention machinery is needed. Invariant-affecting: the wrong
    cell (EV-B) is a false-coherence claim. Must be ratified before the IMPL writes the decorator validation.

- ID: D-RET-RETENTION
  QUESTION: How long must an old epoch be kept resident for a pinned reader, and what protects the pinned SQLite
    snapshot from deletion mid-request?
  OPTIONS:
  - RET-A **No LiveGraph retention; keep prune exclusive** (RECOMMENDED, pairs with EV-A). The LiveGraph keeps
    only the current epoch (unchanged — `livegraph_refresh.rs:354-364` swaps in place); a pinned reader whose LG
    epoch was evicted falls back to the pinned SQLite snapshot (EV-A). The SQLite snapshot is retained by the
    existing model (refresh adds, doesn't delete — §2c); the only deleter, prune, **stays a `Writing`/exclusive
    writer** (W-B relaxes only `Refreshing`, §7.1), so a reader holding `acquire_read` excludes prune and its
    pinned snapshot cannot be deleted mid-request. Consequence: **zero** new retention machinery; the warm-cache
    eviction model is untouched (STOP_CONDITION #2 not triggered).
  - RET-B **Retain the old LiveGraph epoch** (keep N resident until the last reader pinned to N drops) via a
    per-epoch refcount/pin set in `RepoState`. Serves the LG fastpath even during a refresh, at the cost of holding
    ≥2 epochs resident for the refresh window + a refcount subsystem + a contract change to the in-place swap
    (`feed_partition` would need to preserve the prior epoch). Consequence: more machinery, more memory, a
    warm-cache-eviction contract change — earns nothing EV-A does not already deliver coherently. This is the cell
    the STOP_CONDITION names; recommended **against**.
  - RET-C **Snapshot pin-set** — a refcount on SQLite snapshots so prune skips pinned ones, allowing prune to run
    concurrently with readers (relax prune to non-exclusive). Consequence: lets prune be concurrent, but adds a pin
    table + prune-side checks for a maintenance op that is rare and already fast when exclusive. Unearned unless
    prune-during-read latency is shown to matter.
  RECOMMENDED: **RET-A** (no LG retention; prune stays exclusive). Pairs with EV-A. RET-B/RET-C are the named
    upgrade levers if profiling later shows LG-fastpath-during-refresh or concurrent-prune is worth the machinery.
  BLOCKING_REASON: Determines whether this slice touches the warm-cache eviction / swap contract (RET-B) or the
    prune coordination contract (RET-C) — both architecture-boundary changes — or neither (RET-A). STOP_CONDITION
    #2 in the packet routes here.

- ID: D-WB-COORDINATOR-CONTRACT
  QUESTION: Re-enable W-B by relaxing the coordinator's `Refreshing` read-block?
  OPTIONS:
  - WB-A **Relax `Refreshing`→reader** (RECOMMENDED) — readers are admitted during a refresh/enrich and proceed
    against their captured epoch; `Writing` (index/prune) stays read-excluding. Safe **only** jointly with the
    captured epoch (D-EP) + its eviction rule (D-EV) — §6.2 is the proof. Consequence: the VISION's "orientation
    in milliseconds even during a background refresh/enrich"; the deferred §14 W-B, now earned.
  - WB-B **Keep W-A** (block) — ship the epoch capture (D-EP) for its own sake (it removes the double-resolve and
    makes every request self-coherent even under W-A) but do NOT relax the block. Consequence: no per-repo
    read-during-refresh; the slice's headline win (and E-A) is not delivered. Defensible only as a staged first
    cut: land D-EP under W-A, flip D-WB to WB-A in a fast follow once the epoch is proven.
  RECOMMENDED: **WB-A**, ratified **jointly** with D-EP = EP-A + D-EV = EV-A + D-RET = RET-A. Fallback WB-B if the
    operator wants the epoch landed and proven before relaxing the contract.
  BLOCKING_REASON: Changes the ratified `RepoCoordinator` reader/writer **contract** (the §14 STOP_CONDITION #2
    surface) and is the decision the whole slice exists to enable. Must be ratified before the IMPL touches the
    coordinator state machine, and only jointly with the epoch decisions that make it safe.

- ID: D-EA-ENRICH-COMPOSITION
  QUESTION: Does a background enrich (ENRICH-LIFECYCLE-1) compose via this epoch + W-B seam, or carry its own?
  OPTIONS:
  - EA-A **Compose via the shared epoch + W-B seam; add nothing enrich-specific** (RECOMMENDED) — an enrich is
    another `Refreshing` writer publishing N+1; a reader pinned to N is unaffected (§7.2). Re-ratifies
    DAEMON-CONCURRENCY-1 §8 D-E = E-A, which §14 deferred to this slice. Consequence: one seam serves refresh +
    enrich; ENRICH-LIFECYCLE-1 inherits a ready epoch + relaxed coordinator and never re-opens the contract.
  - EA-B **Keep enrich independent** (DAEMON-CONCURRENCY-1 §14 E-B, the current state) — ENRICH-LIFECYCLE-1
    re-opens W-B/epoch itself later. Consequence: duplicated decision; risk of a divergent enrich relax. Acceptable
    only if D-WB ships WB-B (no relax) for independent reasons.
  RECOMMENDED: **EA-A** — ratifying WB-A + the epoch makes refresh and enrich share one seam. (Tightly coupled to
    D-WB; ratify together.)
  BLOCKING_REASON: A cross-slice architecture-boundary coupling: EA-A binds ENRICH-LIFECYCLE-1's
    read-during-enrich correctness to this slice's epoch; EA-B leaves it to re-open the coordinator contract.

- ID: D-SCOPE-HANDLER-SET
  QUESTION: Apply the captured epoch to all mixed-read handlers at once, or the §14-cited subset first?
  OPTIONS:
  - SC-A **The §14 subset first: orient, explain, path, callers, callees** (RECOMMENDED) — the handlers that
    COMBINE an LG-served value with SQLite reads/stamps in one answer (orient/explain via the decorator double-
    resolve `dispatch.rs:2916`+`repo.rs:99`; path via the BFS+stamp `dispatch.rs:1949-1960`; callers/callees via
    LG-serve+resolve `dispatch.rs:986,1040/1133`). These are where a cross-epoch mix is observable. Consequence:
    the highest-risk surface closed first; one capture pattern proven on the headline before fan-out.
  - SC-B **All mixed-read handlers in one slice** — add the single-leaf fastpaths (imports/cycles/stats). But those
    already re-validate the shared fingerprint **per call** (`livegraph_feed.rs:1797-1806`; `callgraph_cert/mod.rs`
    pattern) and read essentially one store, so their only residual is the `snapshot_uid` **stamp** coherence —
    lower-risk. Consequence: larger diff, most of it on already-fingerprint-gated paths; dilutes the review of the
    headline subset.
  - SC-C **orient only** — the single headline handler. Consequence: leaves path's false-freshness stamp and
    callers/callees mixing open under W-B — an incomplete fix that cannot ship WB-A safely.
  RECOMMENDED: **SC-A** (the §14 subset: orient/explain/path/callers/callees). The single-leaf fastpaths' stamp
    coherence is a named fast-follow, not a blocker for WB-A (their data is already fingerprint-gated).
  BLOCKING_REASON: Sets the IMPL's blast radius and which handlers are coherent before WB-A flips. SC-C cannot ship
    W-B safely; the choice between SC-A and SC-B determines the review surface.

- ID: D-CC-ELIGIBILITY-BASIS
  QUESTION: `callers`/`callees`/`path` serve the LiveGraph today on a per-call `Exact+Fresh+TS-only` envelope with NO
    cert and NO `snapshot_uid` binding (`livegraph_feed.rs:231-246,729-755`). Under W-B that is not coherent with the
    pinned snapshot (§5.4). What coherence basis licenses their LiveGraph serve? (Surfaced in iteration 2 — review-1
    #1 asked "what cert/eligibility basis licenses their LiveGraph serve"; this is a behavior change, so it is the
    operator's, not the IMPL's.)
  OPTIONS:
  - CC-A **Gate on the CALLGRAPH no-loss cert** (RECOMMENDED) — capture `epoch.fingerprint = callgraph_cert_eligibility(uid)`
    (§5.4/§6.4) and serve LG iff `current_fp == epoch.fingerprint` under the data guard (§6.3), else SQLite at the pin.
    The callgraph cert is the existing oracle that proves LG callers/callees/CALLS-edges are no-loss equal to
    SQLite@`snapshot_uid` (`callgraph_cert/mod.rs:191-212,220-267`); `path`'s BFS traverses those same CALLS edges.
    Reuses existing machinery (`callgraph_is_green`/`callgraph_cached_green`), uniform with orient, keeps the
    zero-SQLite green fastpath. Consequence (the behavior change): when the **repo-wide** callgraph cert is RED, these
    handlers fall back to SQLite where today they serve a per-call `Exact` answer — i.e. they stop serving an
    LG answer that was never proven no-loss against SQLite. Defensible as a HONESTY improvement (it makes the standalone
    `callers`/`callees` as rigorous as orient's already-cert-gated callers serve) and as REQUIRED for W-B coherence;
    the first call after a fingerprint change pays the (amortized) callgraph cert build orient already pays.
  - CC-B **Per-call no-loss compare at the pin** — at serve, read SQLite for the ONE queried symbol/path at
    `snapshot_uid` and serve LG iff it equals SQLite@`snapshot_uid` (this is `Engine::Compare`'s logic, served).
    Precise (per-symbol, no repo-wide gate; serves LG even when the repo cert is RED-elsewhere) and coherent.
    Consequence: reads SQLite on EVERY served call (reverts QUERY-AUTO-LAZY-SQLITE-1's zero-read fastpath for these
    handlers under W-B); cheap for `callers`/`callees` (one point query) but ~2× cost for `path` (two BFS). The named
    lever if the CC-A repo-wide RED-cert coverage loss is shown to matter for `callers`/`callees`.
  - CC-C **Serve SQLite-only for these handlers under W-B** — do not serve the LiveGraph at all when a refresh could be
    racing; always eager SQLite at the pin. Simplest + trivially coherent. Consequence: loses the LG fastpath for
    `callers`/`callees`/`path` entirely under W-B — strictly worse than CC-A in the common (green) case for no safety
    gain. Listed as the floor.
  RECOMMENDED: **CC-A** (callgraph-cert gate). Uniform with orient, reuses existing machinery, preserves the green
    fastpath; the RED-cert fallback is a defensible honesty improvement, not a regression. CC-B is the named precision
    lever; CC-C the floor.
  BLOCKING_REASON: Changes the **observable serve behavior** (`backend_used`) of three shipped handlers and is an
    invariant-affecting choice (the wrong basis — keeping today's cert-free per-call serve under W-B — is exactly the
    §5.4 cross-store mix). Must be ratified before the IMPL writes the §6.3 gate. Tightly coupled to D-SCOPE (it only
    bites for the non-orient SC-A handlers) and to D-WB (it is only REQUIRED once `Refreshing` admits readers).

---

## 9. Per-choice VISION defense (every choice defended; none contradicts the cited VISION)

- **Multi-agent coordination authority / "git owns history, repo-graph owns current-state"** (`docs/VISION.md`
  Operational Architecture). The captured epoch makes "current-state" a **coherent** noun across both stores: a
  request sees one current-state, not a mix of two stores' currents. W-B (WB-A) delivers the concurrent-readers
  daemon during a refresh; the epoch is what makes that concurrency **honest**.
- **"Orientation in milliseconds"** (`docs/VISION.md` Primary Use Case). WB-A removes the last per-repo
  head-of-line case — a reader no longer blocks for its repo's whole refresh. EV-A keeps the answer fast in the
  steady state (LG fastpath) and merely coherent (SQLite at the pinned snapshot) in the rare swap-mid-request race.
- **Fact-Certainty / Layer model / honest degradation** (`docs/VISION.md` Fact-Certainty; `CLAUDE.md` Mission,
  Layer 0). The split-brain is precisely a Layer-1 answer that is secretly a cross-epoch mix presented as coherent
  truth, and a `snapshot_uid` stamp that lies about freshness (§1). The epoch + §6.2 proof eliminate both: every
  field is epoch N, the stamp is N. EV-A degrades to a coherent SQLite answer, never to a mixed one — honest
  degradation, not a false certainty claim.
- **CC-A (callgraph-cert basis for `callers`/`callees`/`path`) is the Fact-Certainty-honest choice** (`docs/VISION.md`
  Fact-Certainty; Layer 0). Today these handlers serve an LG answer that is internally `Exact` but **never proven
  no-loss against SQLite** — under W-B that answer can silently be from a different epoch than its pinned `target`/uid.
  CC-A serves the LG only when the callgraph cert proves it equal to SQLite@`snapshot_uid`, else the labelled SQLite
  fallback — turning an unverified Layer-1 serve into a proven one and making the standalone `callers`/`callees` as
  rigorous as orient's already-cert-gated serve. The RED-cert coverage loss is honest degradation, not a regression.
- **Smallest design / earn abstractions** (`CLAUDE.md` Structural Guardrails; operating rules). The recommended
  path (EP-A + EV-A + RET-A + CC-A) reuses `import_cert_fingerprint`, the six fingerprint-keyed certs (incl. the
  callgraph cert + its `is_green`/`cached_green`), the decorator's and the `*_engine_response`'s serve-then-fallback
  branches, the retained-snapshot model, and the B1 coordination fix. The only new artifacts are a `RequestEpoch`
  value (the pinned `AgentSnapshot` + the eligibility fingerprint) + a `capture` step + one fingerprint comparison at
  each of the two existing serve sites + the build-then-peek helpers (whose only genuinely new code is the one-line
  `focus_resolution_cached_green` mirror). No new subsystem (STOP_CONDITION #1); no warm-cache/retention contract
  change (STOP_CONDITION #2 — RET-A).
- **`main.rs` is wiring only** (`CLAUDE.md`). The epoch capture lives in `daemon-runtime` handlers + the agent
  use-case signature; `rmapd/src/main.rs` stays wiring-only.

---

## 10. The §6 amendment, restated as the binding contract (for DAEMON-CONCURRENCY-1 cross-reference)

This slice **amends** DAEMON-CONCURRENCY-1 §6 (without editing that doc — it is read-only per the packet; the
amendment lands when this slice's IMPL ships and is cross-linked from §14's "Deferred (not lost)" note). The
binding statement:

> §6's table proves **per-store** atomicity (WAL + READY filter for SQLite; the `RwLock<Option<LiveGraph>>` swap
> for the in-memory graph). Under W-B that is **necessary but insufficient**. Whole-request cross-store coherence
> is established by the **captured request epoch** (§5–§6 of DAEMON-W-B-EPOCH-1): a request **atomically pins one
> READY snapshot** and a **green-validated LG-serve eligibility fingerprint**, the latter captured **build-then-peek**
> so it is the *exact* resident-and-validated state or `None` (the capture is NOT itself a cross-store atomic read — a
> straddling publish or a lazy-rebuild straddle is detected and either downgraded to SQLite@`snapshot_uid` or, when the
> resident partitions are cert-proven no-loss-equal to SQLite@`snapshot_uid`, served as that proven-equal set — never
> as a mix, §5.1/§6.4). Every SQLite read pins `epoch.snapshot_uid()`; every LiveGraph read — at the **decorator**
> (orient/explain, §6.1) or the **`*_engine_response` Auto arm** (callers/callees/path, §6.3) — serves only while
> `import_cert_fingerprint(current partitions, epoch.snapshot_uid()) == epoch.fingerprint` under the data read guard
> (so the served partitions are the cert-proven-equal set), else delegates to SQLite at `epoch.snapshot_uid()`.
> Therefore every contributor to a request's answer resolves to `snapshot_uid_N` — read directly from SQLite@N, or via
> an LG partition-set the no-loss cert proves equal to SQLite@N; no cross-epoch join survives. W-B is safe **iff** the
> epoch is captured; W-A (the §14 ratification) did not require it because the `Refreshing` read-block excluded every
> concurrent store-swap.

---

## 11. Validation plan (for the IMPL — EXECUTED-class, headless, no wall-clock flakiness)

All behaviors are provable on the headless Test API the daemon already has: a custom `Dispatcher` + the
`RepoState` LiveGraph/cert RwLocks + a storage spy, plus the isolated `rmap` dogfood
(`./scripts/dogfood-isolated.sh`, `docs/testing/end-of-slice-procedure.md`). **No GUI, no wall-clock sleeps for
correctness** — use a barrier so the writer's swap is deterministically interleaved mid-request.

1. **Pinned reader sees a coherent N while a writer publishes N+1 (the headline).** Capture epoch N (a resident
   GREEN-cert LiveGraph + a READY snapshot N). Drive an orient request to a barrier placed **between** the
   green-check and the first LG serve; release a writer that (a) flips a new READY snapshot N+1 in SQLite and (b)
   swaps the LiveGraph to N+1 (`feed_partition`, bumping the epoch); then release the request. Assert: the served
   callers, the SQLite-read trust/MODULE_SUMMARY, the SNAPSHOT_INFO totals (`files/nodes/edges` from
   `snapshot::aggregate` over the **captured** `&AgentSnapshot`), **and** the stamped `snapshot_uid` are **all** N
   (no field from N+1). With EV-A, the LG-served leaf falls back to SQLite at `snapshot_uid_N` (its fingerprint no
   longer matches) — assert it returns the N rows, not N+1. Fails today (the decorator would serve N+1 callers beside
   N trust).
2. **E-A: a background enrich publishing N+1 is invisible to the pinned reader.** Same harness, the writer is an
   enrich (LiveGraph swap + snapshot flip) — assert identical coherence (the enrich path is just another
   `Refreshing` writer).
3. **The double-resolve is gone, and `snapshot::aggregate` rides the captured snapshot (review-0 #3).** A storage
   spy counts `get_latest_snapshot` per orient request; assert **== 1** (today == 2: `dispatch.rs:2916` +
   `orient/repo.rs:99`). Drive the spy so its **2nd** `get_latest_snapshot` would return a DIFFERENT
   `snapshot_uid` AND different `files/nodes/edges` totals; assert the answer is unchanged (it is never called
   twice) and that SNAPSHOT_INFO equals the **captured** (1st-resolve) snapshot's totals — proving no fallback
   re-resolve / fetch-latest was introduced and that `orient_repo` aggregates the injected `&AgentSnapshot`.
3b. **Cross-instant capture stays coherent at the pin — downgrade-to-SQLite XOR proven-equal-LG, never a mix (the
   §5.1 straddle proof, made executable; corrected for the lazy cert rebuild — review-2).** Place a barrier
   **between** the snapshot resolve and the eligibility read in `capture`; release a writer that swaps the LiveGraph
   to N+1 in that window; then read the eligibility. **Two sub-cases — because the cert is rebuilt lazily against the
   new resident partitions (mirrors tests 9a/9b):**
   - **3b(a) data-mutating straddle** — the writer's swap changes import/CALLS-relevant content so the new partitions
     are **not** no-loss-equal to SQLite@`snapshot_uid_N`. Assert `epoch.fingerprint == None` ⇒ `serve_from_lg ==
     false` ⇒ the request serves the eager SQLite path at the pinned `snapshot_uid_N` (a coherent all-SQLite N).
   - **3b(b) content-preserving straddle** — the writer's swap leaves the import/CALLS-relevant content equal to
     SQLite@`snapshot_uid_N`. Assert build-then-peek (§6.4) returns `Some(fp)` at the *new* resident fingerprint and
     the served LG leaf equals SQLite@`snapshot_uid_N` (the GREEN cert IS that equality) ⇒ coherent at N.
   Either way no mixed answer is served. The test must **not** assert an unconditional `epoch.fingerprint == None` — it
   asserts `None` only in 3b(a), where the harness mutates data so no GREEN cert can exist at `(new partitions,
   snapshot_uid_N)`. This demonstrates the capture is NOT claimed atomic, and that a straddle is downgraded to SQLite
   **when** the resident partitions are not proven equal at the pin, served (equivalently) otherwise — never a mix.
4. **W-B: a read completes (does not block) during a refresh.** With the coordinator relaxed (WB-A), assert a
   reader admitted while a `Refreshing` writer is in flight **returns** (not blocks) — extend the coordinator
   thread tests (`coordinator.rs` suite) with a `Refreshing`-admits-reader case; assert the answer is coherent N
   (compose with test 1).
5. **No regression / contracts.** `cargo build/fmt/clippy/test` green in `rust/`; the daemon transport +
   coordinator + orient_serve + agent suites pass; orient/explain/path byte-output unchanged in the **no-writer**
   steady state (the epoch is captured but no swap occurs ⇒ identical bytes); smoke protocol
   (`docs/testing/rmap-test-protocol.md`) + isolated dogfood (`orient`/`explain`/`check`) unchanged.
6. **Multi-client live (OBSERVED, isolated).** Via the isolated dogfood: a slow `livegraph-refresh` on a repo and a
   concurrent `orient` on the same repo — the `orient` returns a coherent answer (not blocked, not mixed). A
   presence-of-coherence check, not a wall-clock assertion.

**Non-orient SC-A handler race tests (review-1 #3) — the same barrier harness, driven through `path`/`callers`/`callees`.**

7. **`path` BFS + stamp stay at N under a mid-request swap (closes §1c).** Capture epoch N (resident GREEN callgraph
   cert + READY snapshot N). Drive a `path` request to a barrier placed **between** the epoch capture and the LG BFS
   (`path_engine_response` Auto → `livegraph_path_cancellable`); release a writer that swaps the LiveGraph to N+1
   (`feed_partition`) and flips a READY snapshot N+1; release the request. Assert: `current_fp != epoch.fingerprint`
   ⇒ the BFS falls back to `find_shortest_path` at `snapshot_uid_N`, `backend_used == "sqlite"`, **and the stamped
   `snapshot_uid == N`** (not N+1). Plus the **no-swap** control: same request, no writer ⇒ `backend_used ==
   "livegraph"`, BFS is the N path, stamp == N. The mid-swap case **fails today** (the LG BFS would serve the N+1
   path stamped with `snapshot_uid_N` — the false-freshness label).
8. **`callers`/`callees` fall back to pinned SQLite on a mid-request swap (target ⋈ edges both N).** Capture epoch N;
   barrier between the epoch capture and `livegraph_callers_auto`'s envelope read; release a writer that swaps the LG
   to N+1; release. Assert: `current_fp != epoch.fingerprint` ⇒ `callers_auto_or_sqlite` calls the lazy
   `find_direct_callers(&snapshot_uid_N, …)` ⇒ `backend_used == "sqlite"`, the `target` is the `snapshot_uid_N`
   resolution, and **no caller row is from N+1** (the served set equals SQLite@`snapshot_uid_N`). No-swap control:
   `backend_used == "livegraph"`, the served set equals SQLite@`snapshot_uid_N` (callgraph-green parity). Symmetric
   for `callees`. Fails today (the per-call envelope would serve N+1 callers beside an N-resolved target).
9. **Capture-straddle for `callers`/`callees`/`path` (the §5.4/§6.4 build-then-peek, made executable).** Place a
   barrier **between** the snapshot resolve and the `callgraph_cert_eligibility` read; release a writer that swaps the
   LG to N+1 in that window; then read eligibility. Two sub-cases: (9a) the swap changed CALLS edges ⇒ no GREEN cert
   exists at `import_cert_fingerprint(N+1 partitions, snapshot_uid_N)` ⇒ `epoch.fingerprint == None` ⇒ the whole
   request serves eager SQLite at `snapshot_uid_N` (coherent, never a mix); (9b) the swap left CALLS edges unchanged
   ⇒ the build-then-peek finds GREEN at exactly the resident fingerprint ⇒ `epoch.fingerprint == Some` and the served
   LG answer equals SQLite@`snapshot_uid_N` (the green callgraph cert IS that equality) ⇒ coherent. Either way no
   cross-epoch mix is served.
9b. **Eligibility-helper honesty under a lazy rebuild (review-1 #4, made executable).** Force the callgraph cert
   `StaleOrMissing` (clear it), then drive `callgraph_cert_eligibility` with a barrier **between** its warm-build step
   and its peek step (§6.4); release a writer that swaps the LG to N+1 in that window; release. Assert the peek at the
   **new** `current_fp` finds no GREEN cert there ⇒ the helper returns `None` ⇒ the request serves eager SQLite at
   `snapshot_uid_N` — proving the returned witness is the exact resident-and-validated state or `None`, never a
   mislabeled fingerprint. (Mirror for `orient_bounded_cert_eligibility`'s two-cert peek.)

---

## 12. Smallest-design statement, abstraction ledger & STOP-condition assessment

**Smallest design.** The recommended path (EP-A + EV-A + RET-A + WB-A + EA-A + SC-A + **CC-A**) introduces **no new
storage or LiveGraph subsystem**. It reuses: `import_cert_fingerprint` (the pair already exists), the six
fingerprint-keyed no-loss certs (incl. the callgraph cert + its `callgraph_is_green`/`callgraph_cached_green`), the
`OrientServeDecorator` serve-then-fallback branch, the `*_engine_response` Auto/SQLite-fallback branch, the
retained-snapshot model, and the B1 coordination fix. New work: (a) a `RequestEpoch` value (the pinned
`AgentSnapshot` + the eligibility fingerprint) + a `capture` step that reuses each handler's already-resolved
snapshot; (b) thread the captured `&AgentSnapshot` into the decorator + `orient_repo`/`orient_cancellable`
(eliminating the orient double-resolve while preserving `snapshot::aggregate`), and `&epoch` daemon-locally into
`callers`/`callees`/`path`'s `*_engine_response`; (c) one fingerprint comparison at each serve site — the decorator's
LG-serve branch (orient/explain) and the `*_engine_response` Auto arm (callers/callees/path); (d) the build-then-peek
eligibility helpers — `callgraph_cert_eligibility` (new, composes the existing `callgraph_is_green` +
`callgraph_cached_green`) and `orient_bounded_cert_eligibility`, plus a `focus_resolution_cached_green` peek that is a
verbatim mirror of the existing `callgraph_cached_green`; (e) relax the coordinator's `Refreshing`→reader arm.
The larger alternatives (a retained-epoch refcount RET-B, a snapshot pin-set RET-C, a storage-port epoch method
EP-B, a per-call compare CC-B / SQLite-only CC-C for the non-orient handlers) are surfaced and recommended **against**.

**Abstraction ledger** (per the operating rule — name it, its concrete current users, its axis of variation, the
rejected simpler alternative):

- *`RequestEpoch` value (D-EP = EP-A).* **What:** a request-scoped `{ snapshot: AgentSnapshot, fingerprint }`
  captured once (the pinned snapshot + the green-validated eligibility witness). **Current users:** the five SC-A
  mixed-read handlers (orient, explain, path, callers, callees) + `orient_repo`/`orient_cancellable`'s new
  `snapshot: &AgentSnapshot` param + the decorator's `epoch` field + the `callers`/`callees`/`path`
  `*_engine_response` `&epoch` param. **Axis of variation:** none beyond "one pinned snapshot + one eligibility
  fingerprint per request" — the snapshot is already resolved at each handler's serve-decision, the fingerprint is the
  green-check's own output. **Rejected simpler:** keep re-resolving per read
  (the status quo) — that IS the split-brain; *and* the sub-variant "carry only `snapshot_uid`" — **non-buildable**,
  it strands `snapshot::aggregate` (review-0 #1). **Rejected fancier:** a storage-port `pinned_epoch()` method
  (EP-B) — unearned boundary surface. Justified: it removes a demonstrated cross-epoch hazard and the double-resolve,
  carrying a DTO (`AgentSnapshot`) that already crosses the port — no new boundary type.
- *The serve-time fingerprint gate (D-EV = EV-A).* **What:** one `import_cert_fingerprint(current, pinned_uid) ==
  captured_fp` comparison guarding the existing LG-serve branch, applied at **both** serve sites — the decorator's
  LG-served methods (orient/explain, §6.1) and the `*_engine_response` Auto arm (callers/callees/path, §6.3).
  **Current users:** those two sites. **Axis of variation:** "is the captured LG epoch still resident?" **Rejected
  simpler:** none — without it the serve uses a swapped epoch (the §1b/§1c bug). **Rejected fancier:** retain the old
  epoch (RET-B) — recommended against (STOP_CONDITION #2). Justified: it is the EV-A eviction rule, a comparison not a
  subsystem.
- *The build-then-peek eligibility helpers (D-EP capture / D-CC basis; review-1 #4).* **What:**
  `orient_bounded_cert_eligibility` (bounded cert) + `callgraph_cert_eligibility` (callgraph cert) — each warms the
  existing `*_is_green` then peeks `*_cached_green` at the current resident fingerprint under one guard, returning
  `Some(current_fp)` or `None`; plus `focus_resolution_cached_green`, a mirror of `callgraph_cached_green`. **Current
  users:** the §5.1/§5.4 capture in the five SC-A handlers. **Axis of variation:** which cert gates the serve (bounded
  for orient/explain; callgraph for callers/callees/path — D-CC). **Rejected simpler:** read the helper's verdict
  without re-peeking the resident fingerprint (iteration-1's naïve witness) — the lazy-rebuild TOCTOU makes it lie
  (review-1 #4). **Rejected fancier:** a new per-symbol coherence cert (CC-B's per-call compare is the lighter form of
  this, surfaced not built). Justified: it reuses the existing cert accessors; the only genuinely new code is the
  one-line `focus_resolution_cached_green` mirror.
- *Coordinator `Refreshing`→reader relaxation (D-WB = WB-A).* **What:** admit readers during `Refreshing`.
  **Current users:** every read handler's `acquire_read` during a concurrent refresh/enrich. **Axis of variation:**
  the refresh-vs-reader policy (the §14 D-W axis, now re-opened). **Rejected simpler:** W-A keep-block (WB-B) — the
  shipped fallback; recommended against because it is the entire deferred win. Justified by §6.2 + the demonstrated
  W-B race.
- *Retained LiveGraph epoch (RET-B) / snapshot pin-set (RET-C).* **Rejected** for this slice in favor of RET-A
  (no retention; prune stays exclusive). Listed so the IMPL does not add a refcount/pin subsystem "for
  flexibility"; they earn their place only if LG-fastpath-during-refresh or concurrent-prune is later shown to matter.

**STOP-condition assessment (packet):**

- *"If making the epoch coherent requires a new storage/LiveGraph subsystem (not just capture-once + thread the
  existing fingerprint pair) → STOP + DECISION_REQUIRED."* → **Assessed: it does NOT, including for the non-orient
  handlers added in iteration 2.** The pair is `import_cert_fingerprint`'s existing output (§2a); the design is
  capture-once (§5) + thread + one comparison at each of the two existing serve sites (§6.1 decorator, §6.3
  `*_engine_response`). The callers/callees/path eligibility reuses the **existing** callgraph cert
  (`callgraph_is_green`/`callgraph_cached_green`); the only genuinely new code is the one-line
  `focus_resolution_cached_green` mirror and the build-then-peek composition — neither a subsystem. No new subsystem.
  The candidate subsystems (RET-B/RET-C/EP-B, and the per-call-compare CC-B) are surfaced as decisions and recommended
  against. The one **behavior change** — gating callers/callees/path on the callgraph cert (D-CC) — is surfaced as a
  DECISION, not silently taken. Surfaced, not assumed. ✔
- *"If pinning an old LiveGraph epoch conflicts with the warm-cache eviction model in a way that needs a contract
  change → surface it as DECISION_REQUIRED."* → The **recommended** path (EV-A + RET-A) does **not** pin/retain an
  old LiveGraph epoch at all — it falls back to the pinned SQLite snapshot — so it does **not** touch the
  warm-cache eviction/swap contract. The retain-the-epoch option that *would* touch it (RET-B) is surfaced as
  **D-RET** with the conflict named and recommended against. ✔

---

## 13. Out of scope

- Cross-store **atomic commit** (a single transaction spanning the SQLite snapshot flip and the LiveGraph swap) —
  the epoch makes it unnecessary; not built here.
- A snapshot **refcount / pin-set** (RET-C) and a retained-LiveGraph-epoch **refcount** (RET-B) — surfaced as
  D-RET, recommended against; the named upgrade levers, not this slice.
- ENRICH-LIFECYCLE-1's own background-scheduling design (separate queued slice) — this spec only re-enables the
  shared epoch + W-B seam it composes with (§7.2).
- The K-B fd-watcher cancellation upgrade and any further B2 cancellation work (DAEMON-CANCEL arc — complete).
- Single-leaf fastpath stamp coherence (imports/cycles/stats) — named SC-A fast-follow, not a WB-A blocker.
- Any production code in THIS slice (design only; the IMPL slice executes it).
- `docs/ROADMAP.md` / `docs/TECH-DEBT.md` / `CURRENT_SLICE.md` / `docs/VISION.md` /
  `docs/slices/daemon-concurrency-1.md` edits (cross-reference, do not edit — per the packet).

---

## 14. Ratification (operator — pending)

This SPEC enters the relay's decision-review phase with the §8 slate recommended as a **joint** ratification:
**D-EP = EP-A, D-EV = EV-A, D-RET = RET-A, D-WB = WB-A, D-EA = EA-A, D-SCOPE = SC-A, D-CC = CC-A.** The seven are
coupled: WB-A (re-enable W-B) is safe **only** with EP-A + EV-A + RET-A (the captured epoch + its eviction/retention
rules); EA-A rides on WB-A; and **CC-A** (the callgraph-cert basis for the non-orient SC-A handlers) is what makes
`callers`/`callees`/`path` coherent once WB-A admits readers (D-CC bites only for the SC-A subset, only under WB-A).
The conservative fallback is **WB-B** (land the epoch under W-A as a staged first cut, flip to WB-A once proven) —
strictly better than today either way, because EP-A removes the orient double-resolve and makes every request
self-coherent even under W-A.

**On ratification, the IMPL** (a single relay slice unless the reviewer splits it): captures `RequestEpoch` (the
pinned `AgentSnapshot` + the build-then-peek green-validated eligibility fingerprint) in the SC-A handlers; threads
the captured `&AgentSnapshot` through the decorator + `orient_repo`/`orient_cancellable` (deleting the
`orient/repo.rs:99` re-resolve while preserving `snapshot::aggregate`) and `&epoch` daemon-locally into
`callers`/`callees`/`path`'s `*_engine_response`; adds the serve-time fingerprint comparison at **both** serve sites —
the decorator (orient/explain) and the `*_engine_response` Auto arm (callers/callees/path, EV-A + CC-A); adds the
build-then-peek eligibility helpers (`callgraph_cert_eligibility` + the `focus_resolution_cached_green` mirror, §6.4);
relaxes the coordinator `Refreshing`→reader arm (WB-A); and lands the §11 headless coherence tests (incl. tests 7–9b
for the non-orient handlers) + the §10 §6 amendment cross-link. If the IMPL discovers that any of these requires a
NEW crate/module boundary or a data shape crossing a boundary beyond the `RequestEpoch` value (e.g. EV-A turns out to
need RET-B's retained epoch, or CC-A's coherence cannot be met by the existing callgraph cert), it **STOPs +
surfaces** — it does not restructure a boundary unilaterally (CLAUDE.md Decision Autonomy; the packet
STOP_CONDITIONs).
