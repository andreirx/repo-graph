# DAEMON-CONCURRENCY-1: concurrent connection handling + query-path cancellation — SPEC

Slice: DAEMON-CONCURRENCY-1
Status: **SPEC** (design; the IMPL is the relay slice(s) that follow — see §12 slice-split)
Track: Product-surface honesty / daemon robustness (`docs/ROADMAP.md` → Current priority, P1
"daemon concurrency" → P2 "query-path cancellation"). Closes **TECH-DEBT #1** (serial daemon /
head-of-line blocking) and **TECH-DEBT #2** (no cancellation on query paths) as one coherent design.
Grounded in: `docs/TECH-DEBT.md` §1, §2, and §"Daemon — Progress abort checkpoint granularity"
(D5b); the VISION's Operational Architecture + "Daemon purpose clarified" notes; confirmed against
the daemon source (every claim cited by `path:line` in §1–§2; lines are working-tree at spec time —
the IMPL re-confirms before editing).
Model: this doc follows `docs/slices/module-model-1.md` (problem+evidence → root cause confirmed
against code → principle → desired behavior → design → decisions-to-surface → per-choice VISION
defense → validation → smallest-design/STOP assessment).
Prior art reused (not reinvented): the `RepoCoordinator` reader/writer FIFO model
(`daemon-policy/src/coordinator.rs`), the `DatabaseState` write lock (`daemon-runtime/src/state.rs`),
the SQLite WAL + READY-snapshot model (`storage/src/...`), the LiveGraph atomic-swap
(`daemon-runtime/src/livegraph_refresh.rs`), and the D5b `ControlFlow`/emitter abort seam
(`daemon-runtime/src/dispatch.rs:1902`). This slice **wires** that existing substrate into a
concurrent accept loop and **extends** the abort seam to the query paths. It does **not** invent a
new concurrency subsystem.

> **Model-doc note (OBSERVED deviation).** The selection packet named
> `docs/slices/enrich-lifecycle-1.md` as a second structural model. That file does **not exist** in
> the working tree (`ENRICH-LIFECYCLE-1` is a *queued, unwritten* slice per `docs/ROADMAP.md`
> §"Resolution & attribution"). This spec therefore models on `module-model-1.md` alone (the
> ratified, existing SPEC model) and treats ENRICH-LIFECYCLE-1 as a **forward dependency to compose
> with**, addressed in §12.3 — not as a structural template.

---

## 0. Evidence law (how to read the claims below)

- `OBSERVED` — read directly from source at spec time, cited by `path:line`.
- `INFERRED` — concluded from cited code, not executed.
- This slice is a **design doc**: it runs **no** code (VALIDATION below is "the doc exists and is
  buildable"). Every `OBSERVED` line cites the file; the IMPL re-confirms before editing (lines drift)
  and produces the EXECUTED-class evidence in §10.

---

## 1. The problem (OBSERVED, cited to source)

The daemon the VISION commits to is a "long-lived daemon ... [that] enables **concurrent queries**"
and "the future **multi-agent coordination authority** ... with many readers, fewer writers"
(`docs/VISION.md` Operational Architecture; Agent-facing daemon notes). The **shipped** daemon is the
direct contradiction: a strictly serial accept loop with no query cancellation.

### 1a. The accept loop is strictly serial — head-of-line blocking (TECH-DEBT #1)

- `run_socket` (`daemon-transport/src/socket.rs:257`) is a single-threaded accept loop. On each
  accepted connection it calls `handle_connection(stream, dispatcher)` **inline** — `socket.rs:274`
  — with **no** `thread::spawn`, no worker pool, no async task. The listener is non-blocking only so
  the loop can poll the shutdown flag (`socket.rs:262-285`); the accepted stream is set **back to
  blocking** at `socket.rs:271`.
- `handle_connection` (`socket.rs:147-195`) reads line-delimited NDJSON requests in a `for` loop
  (`socket.rs:164`) and `parse_and_dispatch`es each **synchronously** (`socket.rs:182`), writing only
  the **final** response per request (`socket.rs:185`).
- **Consequence (INFERRED from the above):** one in-flight request occupies the *entire* daemon for
  its full duration. A heavy `index`/`refresh` (Linux kernel: ~77 min per `docs/VISION.md` "Just
  shipped"; tens of minutes) or any slow query blocks `listener.accept()` — so **every other agent's
  fast `orient` waits behind it**. "Orientation in milliseconds" becomes unbounded behind any
  concurrent heavy request. The only `thread::spawn` on this path is test harness code spawning the
  whole server (`socket.rs:417`, `:466`).

### 1b. No cancellation on the query paths (TECH-DEBT #2)

- Cancellation today is **coupled to progress emission**. The D5b seam (`docs/TECH-DEBT.md:2653-2699`)
  makes a transport-write failure during a progress callback an abort checkpoint: the callback maps
  `emitter.emit(...) → ControlFlow::Break` (`daemon-runtime/src/dispatch.rs:1902-1911`), which
  propagates `IndexError::Aborted` / `ComposeError::Aborted` back to the daemon.
- But a **query** writes only its final response (`socket.rs:185`) and emits **nothing**
  mid-computation. Most read handlers take no emitter at all (`callers`, `callees`, `imports`,
  `explain`, `path`, `modules_*`, `deps_*`, `boundaries_*`, … — `dispatch.rs:263-359`); only
  `stats`/`cycles`/`orient`/`check`/`trust` even receive one (`dispatch.rs:270-282`) and use it for
  heartbeats, not cancellation. So a peer disconnect is **invisible** until the query has already run
  to completion.
- **Consequence (INFERRED):** a disconnected or timed-out client (e.g. the relay's `--timeout`)
  leaves a heavy query running to completion with **no consumer**. Combined with #1, that abandoned
  query keeps the serial loop blocked for its full duration. RMAPD-PERF-2's "Daemon continued
  processing after client disconnect" is the same shape.

---

## 2. The substrate already exists — confirmed against code (the load-bearing correction)

The central finding, confirmed by reading the daemon: **the reader/writer coordination, the
write serialization, the atomic snapshot hand-off, and the abort seam are ALL already built and
wired into every handler.** They are inert today *only* because the accept loop is serial and the
shared state is `!Sync`. This is what makes the slice small: the work is concurrent **dispatch** +
making the shared state **thread-safe** + a query **cancel** seam — not a new concurrency engine.

### 2a. `RepoCoordinator` is already a complete, thread-safe reader/writer FIFO

- `daemon-policy/src/coordinator.rs:37-46` — `RepoCoordinator` is `parking_lot::Mutex<CoordinatorInner>`
  + `Condvar` + an atomic ticket counter. It already provides: **concurrent readers**
  (`acquire_read`, state `Reading(n)`, `:110-133`), **exclusive writers** (`acquire_write`/
  `acquire_refresh`, `:214-263`), a **FIFO writer queue** so readers can't starve writers
  (`:48-55`, `:113-118`), and **timeout** variants (`acquire_read_timeout` `:161-208`,
  `acquire_write_timeout` `:310-397`). RAII guards release on drop (`:57-78`, `:399-432`).
- It is **already exercised under real concurrency**: its own test suite shares it across threads via
  `Arc` and asserts concurrent readers + FIFO writers + writer-no-starvation
  (`coordinator.rs:644-714`, e.g. `writers_are_fifo`, `concurrent_readers_with_writer_waiting`).
- It is **already wired into the handlers**: every read handler acquires a read guard
  (`dispatch.rs:964`, `:1057`, `:1151`, `:1316`, e.g. `handle_callers` at `:964`); refresh/enrich
  acquire a refresh guard (`dispatch.rs:2047`, `:2270`, `:3271`).

⇒ **The coordinator needs no new capability for concurrent reads** (STOP_CONDITION #2 is *not*
triggered for the read path). The one place it is *stricter than necessary* — `Refreshing` blocks
readers on that repo — is surfaced as a **decision** in §8 (D-W), because for a long refresh it
re-creates per-repo head-of-line blocking.

### 2b. Writes are already serialized and use their own connection

- `DatabaseState` (`state.rs:104-146`) holds a `parking_lot::Mutex<()>` write lock; `acquire_write`
  (`:134`) serializes all DB-file mutations. `handle_index` (`dispatch.rs:1870`), `handle_refresh`
  (`:2044`), `handle_enrich` (`:2267`) all take it first.
- The write path **opens its own connection**: `index_path_with_progress(&canonical_path, &db_path,
  &repo_uid, …)` (`dispatch.rs:1914`, signature `repo-index/src/compose.rs:3003`) takes a `db_path`
  and opens the DB internally — it does **not** write through `repo_state.storage`. So **readers and
  writers already use separate SQLite connections.**

### 2c. SQLite is already WAL; readers already see only the last-good (READY) snapshot

- `StorageConnection::open` applies **WAL** + `foreign_keys` pragmas on open
  (`storage/src/connection.rs:38`, `:133-137`). WAL's model is *exactly* "many concurrent readers + one
  writer."
- `get_latest_snapshot` filters `status = 'ready'` (`storage/src/crud/snapshots.rs:121-134`,
  WHERE clause `:124-126`). BUILDING / STALE / FAILED snapshots are **invisible** to readers (test
  `get_latest_snapshot_excludes_building_snapshots`, `snapshots.rs:322`). An index/refresh creates a
  **BUILDING** snapshot, does all its work, then flips it to **READY** with a single-row UPDATE at the
  end (`update_snapshot_status`, `snapshots.rs:142-155`).

⇒ **The "current snapshot pointer" is the `status='ready'` row, and the swap is the atomic
BUILDING→READY UPDATE.** Under WAL, a reader on its own connection sees a consistent committed view
and never observes the writer's partial BUILDING snapshot. This is the SQLite analogue of the
in-memory swap below. (D5b adds: on any pipeline error the snapshot goes FAILED and is excluded —
`docs/TECH-DEBT.md:2684`.)

### 2d. The in-memory LiveGraph + certs already swap atomically under a RwLock

- `RepoState` holds the LiveGraph and five derived NO-LOSS certs each behind
  `parking_lot::RwLock<Option<T>>` (`state.rs:199`, `:205-250`). The doc-comments state the intended
  contract: "shared as `Arc<RepoState>` … read-locks for serve, write-locks for build" (`state.rs:196-250`).
- The refresh/preload path holds the LiveGraph write lock **only for the swap**; the heavy producer
  runs unlocked: `livegraph_refresh.rs:116` ("LOCK-FREE; the LiveGraph write lock is acquired ONLY
  for the swap; on any failure the last-good epoch is untouched"), swap sites `:278`, `:355`, `:539`.
  Certs are fingerprint-keyed: a stale reader detects a fingerprint mismatch and rebuilds
  (`state.rs:201-212`).

⇒ The in-memory graph already has atomic, last-good-preserving hand-off — the same pattern
ENRICH-LIFECYCLE-1 relies on (§12.3).

### 2e. The ONE blocker: the shared state is `!Send + !Sync` by current design

- `run_daemon` builds `Arc::new(DaemonState::new())` under `#[allow(clippy::arc_with_non_send_sync)]`
  with the comment **"DaemonState is !Send/!Sync due to interior mutability. Arc is used for shared
  ownership, not cross-thread access. The daemon is single-threaded."** (`lib.rs:243-246`), then
  `ServiceDispatcher::new(state)` and `run_socket_transport(&config, &dispatcher)` (`lib.rs:249`,
  `:273`). `ServiceDispatcher` holds `state: Arc<DaemonState>` and dispatches via `&self`
  (`dispatch.rs:63-65`, `:235-236`).
- Two concrete `!Sync` sources:
  1. `DaemonState.registry: RefCell<RepoRegistry>` (`state.rs:336`, comment "daemon is
     single-threaded"). `RefCell` is `!Sync`.
  2. `RepoState.storage: StorageConnection` wraps a single `rusqlite::Connection`
     (`connection.rs:97-109`). `rusqlite::Connection` is `Send` but **`!Sync`** — it cannot be shared
     by `&` across threads. `state.rs:518` records "RepoState is !Sync".
- The transport trait already permits concurrency: `Dispatcher::dispatch(&self, …)`
  (`daemon-transport/src/dispatch.rs:151`). The blocker is purely that `&ServiceDispatcher` is not
  `Sync` to hand to multiple threads, because the state it borrows is not `Sync`.

**Root-cause summary**

| Symptom (OBSERVED) | Mechanism (cited) | What it needs |
|---|---|---|
| serial accept loop, head-of-line (#1) | `handle_connection` inline, no spawn — `socket.rs:274` | concurrent dispatch in `run_socket` |
| cannot share dispatcher across threads | `DaemonState.registry: RefCell` `state.rs:336`; `RepoState.storage` single `!Sync` `Connection` `connection.rs:109` | make shared state `Send+Sync` (registry lock; per-thread/pool read connections) |
| no query cancel (#2) | queries write only final response `socket.rs:185`; no emitter checkpoints `dispatch.rs:263-359` | extend the D5b `ControlFlow`/emitter abort seam to queries + disconnect detection |
| (latent) reads on a repo block during its refresh | coordinator `Refreshing` blocks `acquire_read` `coordinator.rs` state machine | a coordinator-policy **decision** (D-W): keep-block vs serve-last-good |

---

## 3. Principle (what "concurrent, safe, honest" means here)

- **Remove head-of-line blocking without inventing a concurrency engine.** The coordinator + WAL +
  READY-snapshot + RwLock swap already encode "many readers, one writer, last-good during writes."
  The slice's job is to *let* that substrate run concurrently (spawn per connection, make state
  `Sync`), not to replace it.
- **Writers stay serialized; readers never corrupt.** One writer per repo (the existing
  `DatabaseState` write lock + coordinator refresh state). Readers see only `status='ready'`
  snapshots and atomically-swapped in-memory graphs. No reader ever observes a partial write.
- **Cancellation is cooperative and discards cleanly.** Queries are **read-only**, so a cancelled
  query has **no storage state to roll back** (the key simplification vs. index abort, which D5b
  already handles): detect peer-gone at a checkpoint, stop, return a "cancelled" outcome, drop the
  connection. Nothing to undo.
- **Honesty (Fact-Certainty / Layer model):** concurrency must not let a reader serve a partial or
  uncommitted snapshot as if it were current-state truth. The READY filter + atomic swap are the
  enforcement; the design must preserve them, not bypass them for speed.
- **Smallest design earned by demonstrated need:** the *demonstrated* variation is "≥2 concurrent
  clients" (the whole point). Any new abstraction (a pool type, a cancel-token type) must name its
  concrete current callers or be dropped (§11 ledger).

---

## 4. Desired behavior (the IMPL must deliver — concrete + checkable on a headless Test API)

1. **Concurrent reads, no head-of-line blocking.** With a slow request in flight (a writer holding a
   refresh, or a deliberately-blocked slow query), a second client's fast read **returns promptly**,
   not after the slow request finishes. Provable deterministically (a blocked-on-a-channel slow
   handler, not a wall-clock sleep) — §10 test 1.
2. **Writes remain correctly serialized — one writer per repo.** Two concurrent `index`/`refresh`
   requests to the same DB are serialized by the existing `DatabaseState` write lock + coordinator;
   no interleaving, no corruption; the loser waits (FIFO) or is admitted after the winner commits.
   Readers concurrent with the writer see the **last-good READY** snapshot until the atomic flip —
   §10 test 2.
3. **A disconnected/timed-out client's in-flight heavy QUERY is cancelled (cooperative).** When the
   peer disconnects, the query stops at the next checkpoint and abandons its work; it does **not** run
   to completion with no consumer. Provable headless: a query whose checkpoint fires on a closed
   transport returns/stops at a cancel point — §10 test 3.

Non-goals for this slice's *behavior* (kept honest): instruction-granular cancellation (D5b is and
stays checkpoint-granular, `docs/TECH-DEBT.md:2666-2699`); cancelling an in-flight **write**
mid-transaction beyond the existing D5b index/refresh abort.

---

## 5. Design A — concurrency model (the dispatch change + making state `Sync`)

Two coupled changes. Neither is async.

### 5.1 Concurrent dispatch in `run_socket`

Replace the inline `handle_connection(stream, dispatcher)` (`socket.rs:274`) with **concurrent**
handling: hand each accepted stream to a worker so `accept()` returns immediately to take the next
client. The dispatcher is shared as `Arc<D>` (today it is borrowed `&D`). Each connection still
processes its *own* requests serially (NDJSON request pipelining per connection is unchanged); it is
*across connections* that we gain concurrency — which is the multi-agent case.

The concrete model (thread-per-connection vs bounded pool vs async) + the **concurrency cap /
backpressure** is **DECISION_REQUIRED D-C** (§8). Recommendation preview: **thread-per-connection
bounded by a counting semaphore cap** — smallest change that also bounds resource use; async is
rejected (§8 D-C, satisfies STOP_CONDITION #1).

### 5.2 Make the shared state `Send + Sync` (the load-bearing change)

For `Arc<ServiceDispatcher>` to cross threads, `DaemonState` and `RepoState` must be `Sync`. Two
sub-changes, both mechanical given the substrate:

- **Registry:** replace `DaemonState.registry: RefCell<RepoRegistry>` (`state.rs:336`) with a
  `parking_lot::Mutex<RepoRegistry>` (or `RwLock`). The registry is already accessed through narrow
  `registry()` / `registry_mut()` accessors (`state.rs:563-571`) — the change is contained to those
  accessors + their few callers. The `repos` and `db_runtimes` maps are **already** `std::sync::RwLock`
  (`state.rs:324`, `:330`) and need no change.
- **Read connection:** `RepoState.storage` is a single `!Sync` `Connection` shared by `&` across
  reader threads — illegal once threads are real. The fix is the **SQLite-under-concurrency decision
  D-S** (§8): connection-per-operation, a per-repo pool, or a single `Mutex<Connection>`. (Writers are
  unaffected — they already open their own connection, §2b.)

Everything else `RepoState` holds is already `Sync`: `RepoCoordinator` (Mutex+Condvar), the
`parking_lot::RwLock<Option<…>>` LiveGraph + certs. The only `!Sync` field is `storage`.

> **The `arc_with_non_send_sync` allow disappears.** After D-S + the registry lock, the
> `#[allow(clippy::arc_with_non_send_sync)]` at `lib.rs:245` and `state.rs:521` are removed — their
> removal compiling is itself a proof the state became `Send+Sync` (the IMPL cites this).

---

## 6. Design B — reader/writer safety under concurrency (reuse, do not reinvent)

The safety argument is a composition of four mechanisms that **already exist** (§2); the design's job
is to keep them intact while the accept loop goes concurrent.

| Concern | Mechanism (already built) | Cited |
|---|---|---|
| One writer per DB file | `DatabaseState` `Mutex<()>` write lock | `state.rs:104-146`; taken at `dispatch.rs:1870/2044/2267` |
| One writer vs readers per repo | `RepoCoordinator` refresh state (FIFO) | `coordinator.rs:214-263`; taken at `dispatch.rs:2047/2270` |
| Readers never see partial SQLite | WAL + `get_latest_snapshot status='ready'` | `connection.rs:38`; `snapshots.rs:121-134`, test `:322` |
| Readers never see partial in-memory graph | `RwLock<Option<LiveGraph>>` atomic swap; fingerprint-keyed certs | `livegraph_refresh.rs:116/278/355/539`; `state.rs:199-250` |
| Failed write never served | snapshot → FAILED, excluded by READY filter | `docs/TECH-DEBT.md:2684`; `snapshots.rs:124` |

**SQLite access under concurrency.** WAL is on (§2c). With **separate connections per reader**
(D-S = connection-per-op or pool), WAL gives true concurrent reads + one writer with no extra locking
— the readers-writer model is the database's, not ours. The coordinator then governs *policy* (FIFO
fairness, refresh exclusivity) on top, not correctness of concurrent SQLite access. A single shared
`Mutex<Connection>` (D-S option C) is also safe but **serializes same-repo reads** — it removes the
*accept-loop* and *cross-repo* head-of-line (the bulk of #1) but not *same-repo* read concurrency;
this trade-off is the body of D-S.

**The coordinator's refresh-vs-reader policy (D-W).** Today `Refreshing` blocks `acquire_read` on
that repo (`coordinator.rs` state machine; mirrored in `state.rs:18-27` doc table). That is **safe**
but **stricter than WAL requires**: because readers see only the READY snapshot (§2c) and the writer
commits the new snapshot atomically, readers *could* serve last-good throughout a refresh without ever
seeing partial state. Keeping the block means a long refresh on repo A blocks reads on repo A for its
whole duration — re-introducing per-repo head-of-line for the *one* repo being refreshed. Relaxing it
delivers the full VISION ("orientation in milliseconds" even during a background refresh/enrich) and
is what ENRICH-LIFECYCLE-1 needs (§12.3). Relaxing is a **coordinator contract change** → surfaced as
**D-W** (§8), recommended **relax**, with the safety proof above; the conservative default (keep
block) is still a strict improvement over the serial loop and is the fallback.

---

## 7. Design C — query-path cancellation (extend the D5b seam, don't build a new one)

Three pieces: a cancel signal, disconnect detection, and checkpoints.

### 7.1 The cancel signal — reuse `ControlFlow` / emitter-`Err`

D5b already defines the abort signal: a callback returns `ControlFlow::Break` when `emitter.emit()`
returns `Err` (transport gone) — `dispatch.rs:1902-1911`; `ProgressEmitter::emit -> Result<(),
EmitError>` is `daemon-transport/src/dispatch.rs:107-114`. The query seam reuses this shape: a query
checkpoint attempts a cheap signal and, on transport failure, returns a **Cancelled** outcome.
The **response codes already exist** (OBSERVED): `ErrorCode::Cancelled` ("Operation was cancelled")
and `ErrorCode::ProgressDeliveryFailed` ("aborted at a progress checkpoint because the transport
channel failed") are defined at `envelope.rs:133-141`. So a cancelled query already has a protocol
code — the cancel seam adds **checkpoints**, not a new error kind (unlike BP-BUSY's one new code, D-C).

### 7.2 Disconnect detection for a *computing* request

A query that is computing is **not** reading the socket, so disconnect is not seen passively. Two
mechanisms (DECISION_REQUIRED **D-K**, §8):

- **(K-A) Heartbeat-write probe (smallest; pure D5b reuse).** At each checkpoint the query writes a
  heartbeat/progress line via the emitter. Writing to a peer-closed socket fails (EPIPE) → `Err` →
  `Break` → Cancelled. Already true for `stats`/`cycles`/`orient`/`check`/`trust` which hold an
  emitter (`dispatch.rs:270-282`); the non-emitting queries (`callers`/`callees`/`imports`/`explain`/
  `path`/`modules_*`/…) would be given an emitter + checkpoints. Cost: emits heartbeat traffic; cancel
  latency = checkpoint spacing.
- **(K-B) Explicit cancel token + disconnect watcher (more general; new machinery).** A per-request
  `Arc<AtomicBool>` cancel flag, checked at checkpoints. With thread-per-connection, the connection's
  reader side (or a lightweight `poll`/`MSG_PEEK` for `POLLHUP`/EOF on the stream fd) sets the flag on
  peer close even while the worker computes. Cost: a watcher mechanism + threading the token through
  query signatures — a new cross-cutting parameter.

Recommendation preview: **K-A** (reuse the existing emitter seam; it requires no new type and no fd
polling), accepting that cancel latency is bounded by checkpoint spacing. K-B is the upgrade path if
non-emitting silent cancellation is later required.

### 7.3 Checkpoint placement (granularity)

Cancellation is cooperative: the long query algorithms must check the signal at bounded intervals.
Candidate checkpoints (the heavy read paths named in TECH-DEBT #2): the per-symbol/per-node loops in
`cycles` (graph traversal), `stats` (aggregation), `orient`/`check`/`trust` (multi-signal assembly).
Exact placement + interval is **DECISION_REQUIRED D-K** (granularity sub-point), mirroring D5b's
"checkpoint-granular, not instruction-granular" honesty (`docs/TECH-DEBT.md:2666`). Because queries
are read-only, a checkpoint that fires simply returns Cancelled and drops work — **no partial state,
no rollback** (contrast index abort, D5b §"Residual limitation").

---

## 8. Decisions to surface (DECISION_REQUIRED — operator ratifies; the IMPL does NOT re-decide)

Each is an exhaustive matrix + a defensible recommendation. The IMPL executes only the ratified cells.

DECISION_REQUIRED:
- ID: D-C-CONCURRENCY-MODEL
  QUESTION: What concurrency model handles connections, what is the cap, and what is the precise
    over-cap (backpressure) policy?
  OPTIONS (concurrency model):
  - C-A Thread-per-connection, **unbounded** — spawn a thread per accepted stream. Smallest diff to
    `run_socket` (`socket.rs:274`). Risk: unbounded threads/memory under a connection storm; no
    backpressure. Consequence: simplest, least safe at scale.
  - C-B Thread-per-connection bounded by a **counting-semaphore cap** (RECOMMENDED) — spawn per
    connection; a process-wide semaphore of **`RMAP_DAEMON_MAX_CONNS` permits (default 64)** caps the
    number of concurrent connection-handler threads (one permit ≈ one connection; for the
    connect→query→close agent workload one connection ≈ one in-flight request). Smallest change that
    **bounds** resource use. Robust to long writes (a 77-min index ties up one thread, not a pool
    slot). The behavior when all permits are held is the explicit **over-cap policy** sub-decision
    below (BP-WAIT vs BP-BUSY) — no longer left fuzzy. Consequence: bounds memory/threads; one
    constant + a `Semaphore`, no pool type.
  - C-C Fixed bounded **worker pool** + bounded work queue — M worker threads pull accepted streams
    from a channel. Classic; bounds threads precisely. But a long `index`/`refresh` occupying a pool
    worker for tens of minutes burns a fixed fraction of capacity (a 4-worker pool loses 25% to one
    kernel index); needs a pool abstraction (§11 ledger). Consequence: tightest thread bound, worst
    behavior under long writes unless the pool is large.
  - C-D **Async runtime** (tokio) — rewrite handlers to `async`. REJECTED: the entire storage/index
    stack is synchronous, `rusqlite::Connection` is `!Sync` (`connection.rs:109`), `DaemonState` is
    `!Send` (`lib.rs:243`); this is a full daemon rewrite with a large blast radius for no benefit a
    thread model can't deliver. Listed to satisfy STOP_CONDITION #1.
  OVER-CAP POLICY (sub-decision; applies to any bounded model — C-B/C-C; **pick exactly one** — this
    is a client-visible protocol behavior, not a runtime tunable):
  - BP-WAIT **Bounded wait** — the accept loop acquires a permit *before* spawning; when all permits
    are held it blocks, so further `connect()`s queue in the `UnixListener` accept backlog and, once
    that fills, are refused at the socket layer. **No new protocol surface.** Cost: an over-cap client
    receives no response until a permit frees — it experiences a hang resolved only by its own
    `--timeout`. That is **opaque** degradation (the client cannot distinguish "busy" from "stuck").
  - BP-BUSY **Explicit busy rejection** (RECOMMENDED) — the accept loop keeps draining `accept()`;
    when no permit is available it writes a typed error on the **existing** `ErrorResponse`/`ErrorDetail`
    envelope (`socket.rs:204-241`, `envelope.rs:166-193`) and closes the connection. This needs **one
    new `ErrorCode::Busy` variant** alongside the existing `Cancelled`/`StateUnavailable`/`Timeout`
    codes (`envelope.rs:108-145`) — there is no busy/capacity code today (OBSERVED). The client gets an
    **immediate, honest "at capacity"** signal it can back off / retry on. Cost: one enum variant; no
    new machinery (reuses the error-response path).
  RECOMMENDED: **C-B** (thread-per-connection + counting-semaphore cap), **default cap 64**, over-cap
    policy **BP-BUSY**. Reject C-D explicitly. Rationale for BP-BUSY over BP-WAIT: the Mission's
    **"honest degradation reporting"** (`CLAUDE.md`) — at saturation an explicit `Busy` is honest
    degradation, whereas a silent wait that surfaces only as the client's timeout is precisely the
    opaque failure the VISION rejects. BP-WAIT is the documented fallback if the operator prefers zero
    new protocol codes.
  CONFIGURABILITY (IMPL scope — explicit, so the IMPL does not re-decide): the **cap** is
    env-overridable via `RMAP_DAEMON_MAX_CONNS` (default 64) and **is in scope for the IMPL** (matches
    the `RMAP_` env convention, `registry.rs:553`). The **over-cap policy** (BP-WAIT vs BP-BUSY) is
    **compile-fixed to the ratified choice — NOT a runtime toggle**: no config surface is earned for
    switching it (smallest design; one behavior, ratified once, no dual-path to test).
  BLOCKING_REASON: Sets the daemon's runtime threading model, its resource bound, AND a client-visible
    over-cap protocol behavior — a foundational architecture-boundary choice (CLAUDE.md "new
    module/crate boundary, dependency edge"). The IMPL cannot write `run_socket` concurrency, or know
    whether to add `ErrorCode::Busy`, without it.

- ID: D-S-SQLITE-CONCURRENCY
  QUESTION: How do concurrent readers access SQLite, given `RepoState.storage` is one `!Sync`
    `Connection` (`connection.rs:109`) and WAL is already on (`connection.rs:38`)?
  OPTIONS:
  - S-A Connection-per-operation — each read handler opens its own `StorageConnection` from the
    repo's `db_path`; WAL gives true concurrent reads. `RepoState` stops holding a shared read
    connection (becomes `Sync`). Cost: `StorageConnection::open` **re-runs the migration check every
    open** (`connection.rs:133-137`) — adds a per-request open + `schema_migrations` scan; mitigate
    with a migrations-already-applied fast-open constructor (small storage addition). Consequence:
    full concurrency, simplest ownership; pay a per-call open.
  - S-B Per-repo connection **pool** (RECOMMENDED if same-repo read concurrency is required) — a
    bounded pool of N reader connections per `RepoState` (hand-rolled `Mutex<Vec<StorageConnection>>`
    or `r2d2`/`r2d2_sqlite`), checked out per read; WAL concurrent. Amortizes the open cost. Cost: a
    pool abstraction + sizing + (if r2d2) a dependency (§11 ledger). Consequence: full concurrency,
    no per-call open, more machinery.
  - S-C Single `Mutex<StorageConnection>` on `RepoState` — wrap the existing one connection; readers
    lock it. Smallest type change; makes `RepoState` `Sync`. **Serializes same-repo reads** (only
    cross-repo + accept-loop concurrency gained). Removes most of #1 (a heavy op on repo A no longer
    blocks reads on repo B or the accept loop) but two agents reading repo A concurrently serialize.
    **Incompatible with D-W relax** (the writer would contend the readers' connection). Consequence:
    smallest, but caps the VISION's "many readers" to "many readers across repos."
  RECOMMENDED: **S-A** (connection-per-operation + a fast-open that skips the migration scan) as the
    smallest path to *true* concurrent reads; **S-B** only if profiling shows the per-call open is a
    real cost or same-repo read concurrency at high fan-out is needed. **S-C** only if the operator
    wants the minimal diff and accepts same-repo read serialization (then D-W must = keep-block).
  BLOCKING_REASON: Determines `RepoState`'s shape and whether reads are truly concurrent or
    cross-repo-only — a data-shape/ownership boundary, and it gates D-W. Architecture-boundary
    decision; the IMPL cannot make `RepoState` `Sync` without it.

- ID: D-W-REFRESH-READER-POLICY
  QUESTION: During a repo's refresh/enrich, do readers on that repo **block** (current behavior) or
    **serve the last-good READY snapshot**?
  OPTIONS:
  - W-A Keep blocking (conservative; no contract change) — `Refreshing` continues to block
    `acquire_read` (`coordinator.rs` state machine). Reads on the repo being refreshed wait for the
    whole refresh (Linux: tens of minutes). Cross-repo + accept-loop head-of-line still eliminated by
    Design A. Consequence: simplest/safest; per-repo head-of-line persists during that repo's refresh.
  - W-B Serve last-good during refresh (RECOMMENDED) — relax the coordinator so readers proceed
    against the `status='ready'` snapshot while a refresh builds the BUILDING snapshot; the atomic
    BUILDING→READY flip (`snapshots.rs:142-155`) + WAL + the RwLock LiveGraph swap guarantee no reader
    sees partial state (safety proof §6). Requires a `RepoCoordinator` **contract change** (readers no
    longer excluded by `Refreshing`; a brief exclusive window only for any non-atomic final swap) and
    **D-S ∈ {S-A, S-B}** (separate reader connections). Consequence: full VISION behavior; composes
    with ENRICH-LIFECYCLE-1; load-bearing seam change.
  RECOMMENDED: W-B (relax), paired with D-S = S-A or S-B. Fallback W-A if the operator wants the
    minimal-risk first cut (then ship W-B as a fast follow).
  BLOCKING_REASON: Changes the ratified `RepoCoordinator` reader/writer contract (STOP_CONDITION #2)
    and couples to D-S. Must be ratified before the IMPL touches the coordinator state machine.

- ID: D-K-CANCELLATION
  QUESTION: How is a computing query's peer-disconnect detected, and at what checkpoint granularity is
    it cancelled?
  OPTIONS:
  - K-A Heartbeat-write probe + reuse D5b `ControlFlow` (RECOMMENDED) — long queries emit a heartbeat
    at each checkpoint; emit-`Err` (peer gone) → `Break` → Cancelled, exactly the D5b seam
    (`dispatch.rs:1902-1911`). Give the non-emitting queries an emitter + checkpoints. No new type, no
    fd polling. Cost: heartbeat traffic; cancel latency = checkpoint spacing.
  - K-B Explicit cancel token + disconnect watcher — per-request `Arc<AtomicBool>`, checked at
    checkpoints; the connection reader / a `poll(POLLHUP)` watcher sets it on peer close. More general
    (silent cancel for non-emitting queries) but adds a cancel-token parameter threaded through query
    signatures + a watcher (new cross-cutting machinery, §11 ledger). Consequence: detects disconnect
    without emitting; larger surface.
  - Granularity sub-choice (applies to both): checkpoint placement = the per-symbol/per-node loops in
    `cycles`/`stats`/`orient`/`check`/`trust` (the heavy read paths, TECH-DEBT #2). Bounded-interval,
    not instruction-granular (mirrors D5b honesty, `docs/TECH-DEBT.md:2666`). Read-only ⇒ Cancelled
    discards work with no rollback.
  RECOMMENDED: K-A, checkpoints at the named query loops, interval tuned so cancel latency is "small"
    (sub-second on the heavy paths). K-B deferred unless silent cancellation of non-emitting queries
    is later required.
  BLOCKING_REASON: Sets whether a new cancel-token abstraction enters the codebase and how queries
    learn they are abandoned — affects query handler signatures (a boundary the IMPL must not invent
    unilaterally).

- ID: D-SPLIT-SLICE
  QUESTION: Ship #1 (concurrency) and #2 (cancellation) as ONE IMPL slice or split B1 → B2?
  OPTIONS:
  - Split B1 (concurrency: Design A + B) → B2 (cancellation: Design C) (RECOMMENDED) — B1 is the
    larger architectural change (state `Send+Sync`, concurrent dispatch, D-S/D-W); B2 layers the
    cancel seam on top and *depends on* B1 (disconnect-via-heartbeat-write is cleanest once
    thread-per-connection owns the stream). Smaller, independently reviewable diffs; B1 delivers the
    headline #1 win on its own. Consequence: two relay cycles; intermediate state (B1 shipped, B2
    pending) is coherent — concurrency without query-cancel is strictly better than today.
  - One slice — concurrency + cancellation together. Fewer cycles; one larger diff spanning the
    runtime threading model AND the query handler signatures. Consequence: bigger review surface,
    higher blast radius in one step.
  RECOMMENDED: Split (B1 → B2). The roadmap already orders them "daemon concurrency → query-path
    cancellation" (`docs/ROADMAP.md` Dependency notes).
  BLOCKING_REASON: Sequencing/scope decision with blast-radius implications; sets how many IMPL
    relay slices follow this SPEC.

- ID: D-E-ENRICH-COMPOSITION
  QUESTION: How does this slice compose with ENRICH-LIFECYCLE-1 (a queued slice that runs enrichment
    as a daemon **background task** after index/refresh, with atomic snapshot hand-off)? Both add
    non-serial work; what, if anything, does THIS slice owe that composition?
  OPTIONS:
  - E-A Compose via the shared D-W seam; add nothing enrich-specific here (RECOMMENDED) — a background
    enrich is **just another writer** in the existing model: it takes the DB write lock + coordinator
    refresh state (`dispatch.rs:2267/2270`) and swaps the LiveGraph under `livegraph.write()`
    (`livegraph_refresh.rs`), flipping the snapshot BUILDING→READY atomically (`snapshots.rs:142-155`).
    The *only* thing it needs from this slice is **D-W = W-B** ("readers serve last-good while a writer
    runs"). If D-W ships as W-B, ENRICH-LIFECYCLE-1's "index returns fast syntax; enrich upgrades the
    graph behind it; readers never blocked" is satisfied by the *same* relaxed-coordinator seam — no
    new mechanism in this slice. Consequence: one seam serves both features; this slice adds zero
    enrich-specific code; ENRICH-LIFECYCLE-1 does not re-open the coordinator contract.
  - E-B Keep them independent; ship D-W = W-A (keep block) and let ENRICH-LIFECYCLE-1 carry its own
    relax later — this slice does the minimum and stays silent on enrich. Consequence: ENRICH-LIFECYCLE-1
    must **re-open D-W** (re-litigate the `RepoCoordinator` reader/writer contract) when it lands, OR
    run enrich without holding the repo read-exclusive (its own bespoke relax) — a duplicated decision
    and the risk of two divergent relax mechanisms. Acceptable only if D-W is ratified W-A for
    independent reasons.
  - E-C Pre-build enrich background-task scheduling in THIS slice — REJECTED: ENRICH-LIFECYCLE-1 is its
    own queued slice with no current caller here; building its scheduler now is unearned scope
    (smallest-design violation; packet FILES_OUT_OF_SCOPE = other slices). Listed to be explicit it is
    *not* being done in this slice.
  RECOMMENDED: **E-A** — ratifying **D-W = W-B** makes the two features share one seam and avoids
    ENRICH-LIFECYCLE-1 re-litigating the coordinator contract. They do not otherwise contend
    (concurrency = concurrent client access; enrich = autonomous background work) — confirmed against
    `docs/ROADMAP.md`'s "Not blocked on DAEMON-CONCURRENCY-1 … shares only state-safety, which the
    snapshot model makes an atomic pointer swap." (Tightly coupled to D-W; ratify the two together.)
  BLOCKING_REASON: A cross-slice architecture-boundary coupling: E-A binds ENRICH-LIFECYCLE-1's
    correctness to D-W = W-B; E-B leaves the coordinator contract for ENRICH-LIFECYCLE-1 to re-open.
    The operator must choose deliberately — it determines whether a *future* slice inherits a ready
    seam or must re-decide a ratified contract.

---

## 9. Per-choice VISION defense (every choice defended; none contradicts the cited VISION)

- **Concurrent queries / multi-agent coordination authority / many readers, fewer writers**
  (`docs/VISION.md` Operational Architecture; "Daemon purpose clarified"). Design A makes the daemon
  actually concurrent; Design B keeps writers serialized (DB write lock + coordinator) and readers
  consistent (READY filter + atomic swap). D-S = S-A/S-B realizes "many readers" literally (WAL
  concurrent reads); D-W = W-B realizes "readers proceed during writes." The recommended path *is* the
  daemon the VISION commits to, not a serial loop with a concurrency label.
- **"Orientation in milliseconds"** (`docs/VISION.md` Primary Use Case). #1's fix is exactly this: no
  client's fast `orient` waits unboundedly behind another's heavy `index`/refresh. D-W = W-B closes
  the last gap (reads during the *same* repo's refresh). The semaphore cap (D-C = C-B) keeps the
  daemon from collapsing under load, preserving low latency for admitted requests; over-cap clients get
  an immediate `Busy` (BP-BUSY) instead of inflating everyone's latency by queueing.
- **Honesty / no corruption / Fact-Certainty layer model** (`docs/VISION.md` Fact-Certainty;
  `CLAUDE.md` Layer 0). Readers never see a BUILDING/FAILED snapshot (`snapshots.rs:124`) or a
  half-swapped LiveGraph (`livegraph_refresh.rs:116`). A cancelled query is read-only and leaves no
  partial state. Concurrency adds *no* new path by which Layer-2–4 or partial data could be served as
  Layer-0 truth — the existing certainty gates are preserved, not bypassed. Under saturation the daemon
  reports `Busy` (D-C = BP-BUSY) rather than hanging — **honest degradation** (`CLAUDE.md` Mission),
  not an opaque stall the client can only resolve by timing out.
- **Smallest design / earn abstractions** (`CLAUDE.md` Structural Guardrails; operating rules).
  Recommended path reuses the coordinator, DB write lock, WAL/READY model, RwLock swap, and D5b seam;
  the only candidate new abstractions (a connection pool, a cancel token) are gated behind D-S/D-K and
  recommended *against* unless their concrete need is demonstrated (§11).
- **`main.rs` is wiring only** (`CLAUDE.md`). The concurrency lives in `daemon-transport::run_socket`
  + `daemon-runtime` state; `rmapd/src/main.rs` (`rmapd/src/main.rs:59-104`) stays wiring-only.

---

## 10. Validation plan (for the IMPL — EXECUTED-class, headless, no wall-clock flakiness)

All three behaviors are provable on the **headless Test API** the daemon already has: `run_socket`
with a custom `Dispatcher` (the transport tests spawn a server thread + connect real clients —
`socket.rs:401-511`), the `RepoCoordinator` thread tests (`coordinator.rs:644-714`), and the isolated
`rmap` dogfood (`./scripts/dogfood-isolated.sh`, `docs/testing/end-of-slice-procedure.md`). **No GUI,
no wall-clock sleeps for correctness** — use a channel/barrier so the "slow" request is deterministically
in-flight.

1. **Concurrency / no head-of-line (behavior 1).** Test `Dispatcher` whose `slow` method blocks on a
   `crossbeam`/`mpsc` rendezvous channel (held open by the test) and whose `fast` method returns
   immediately. Spawn the server; client A sends `slow` (now provably parked in the handler); client B
   sends `fast` and **receives its response before** the test releases A. Deterministic: B's response
   arrival is gated on the channel, not a timer. Fails today (serial loop: B's response can't arrive
   until A returns).
2. **Writer serialization + last-good reads (behavior 2).** Two threads call `index`/`refresh` on the
   same temp DB; assert exactly one holds the DB write lock at a time (instrument the test dispatcher,
   or assert via `DatabaseState::try_acquire_write` returning `None` while held — pattern at
   `state.rs:811-824`) and the second is admitted only after the first commits. Concurrently, a reader
   asserts `get_latest_snapshot` returns the **old READY** snapshot mid-build and the **new** one only
   after the flip (extends `get_latest_snapshot_excludes_building_snapshots`, `snapshots.rs:322`). If
   D-W = W-B: assert a read **completes** (not blocks) while a refresh holds the repo.
3. **Query cancellation (behavior 3).** Server + a `cancellable` test query with a checkpoint that
   emits a heartbeat (K-A). Client connects, sends the query, then **drops the socket**; assert the
   handler's next checkpoint observes emit-`Err` and returns `Cancelled` (instrument a counter that
   would keep incrementing if the query ran to completion, and assert it stops). Deterministic via a
   barrier that releases the checkpoint only after the client has closed. Mirror the D5b
   `FailingEmitter` pattern already in `daemon-transport/src/dispatch.rs:366-412`.
4. **No regression / contracts.** `cargo build/fmt/clippy/test` green in `rust/`; the daemon transport
   + coordinator + state suites pass (incl. the existing `socket_*`, `writers_are_fifo`,
   `concurrent_readers_with_writer_waiting`); the `#[allow(clippy::arc_with_non_send_sync)]` removals
   (`lib.rs:245`, `state.rs:521`) compile — proving the state is now `Send+Sync`; smoke protocol
   (`docs/testing/rmap-test-protocol.md`) + isolated dogfood (`orient`/`explain`/`check`) unchanged in
   output.
5. **Multi-client live (OBSERVED, isolated).** Via the isolated dogfood: two concurrent `rmap` clients
   against one daemon — a slow op on one repo and a fast `orient` on another return independently. Not
   a wall-clock assertion; a presence-of-concurrency check (both complete; the fast one need not wait
   for the slow one's full duration).

---

## 11. Smallest-design statement, abstraction ledger & STOP-condition assessment

**Smallest design.** The recommended path (D-C=C-B + BP-BUSY, D-S=S-A, D-W=W-B, D-K=K-A,
D-SPLIT=B1→B2, D-E=E-A) introduces **no new concurrency subsystem**. It reuses: the `RepoCoordinator` (already thread-safe,
already wired), the `DatabaseState` write lock, SQLite WAL + the READY-snapshot filter, the LiveGraph
`RwLock<Option>` swap, and the D5b `ControlFlow`/emitter abort seam. The new work is: (a) concurrent
dispatch in `run_socket` + a semaphore cap; (b) registry `RefCell→Mutex` and reader-connection
ownership so the state is `Send+Sync`; (c) query checkpoints reusing the existing emitter abort. The
larger alternatives (async rewrite; a bespoke pool) are surfaced and recommended **against**.

**Abstraction ledger** (per the operating rule — name it, its concrete current users, its axis of
variation, the rejected simpler alternative):

- *Counting semaphore connection cap (D-C = C-B).* **What:** a process-wide in-flight-handler limit.
  **Current users:** the one accept loop in `run_socket`. **Axis of variation:** max concurrency
  (one tunable). **Rejected simpler:** unbounded thread-per-connection (C-A) — dropped because it is
  unbounded under load. Justified: it is a constant + a `Semaphore`, not a new module.
- *`ErrorCode::Busy` variant (D-C = BP-BUSY).* **What:** one enum variant on the existing
  `ErrorCode`/`ErrorResponse` envelope (`envelope.rs:108-145`) signalling "at capacity." **Current
  users:** the same `run_socket` accept loop, on over-cap rejection. **Axis of variation:** the
  daemon's over-cap response. **Rejected simpler:** BP-WAIT (no code) — dropped for honest degradation
  over an opaque hang. Justified: a single enum variant reusing the error path, not new machinery.
- *Reader-connection strategy (D-S).* Recommended **S-A = no new abstraction** (open per op; one
  small `open`-without-migrations storage fn — earned by every read handler as a concrete caller).
  A connection **pool** (S-B) *would* be a new abstraction — **only earned** if profiling shows the
  per-call open is a real cost; otherwise dropped. Stated so the IMPL doesn't add a pool "for
  flexibility."
- *Cancel token (D-K = K-B).* A new `Arc<AtomicBool>` cancel-token threaded through query signatures —
  **rejected** for this slice in favor of K-A (reuse the emitter seam). Listed so it is not added
  pre-emptively; it earns its place only when silent cancellation of non-emitting queries is required.

**STOP-condition assessment (packet):**

- *"If safe concurrency genuinely requires a full async runtime rewrite → STOP + DECISION_REQUIRED
  (sync-thread/pool vs async)."* → **Assessed: it does NOT.** The coordinator + WAL + RwLock swap are
  all synchronous and already concurrency-capable; a thread model over them suffices. Async (C-D) is
  presented in the D-C matrix and recommended **against**. Surfaced, not assumed. ✔
- *"If the RepoCoordinator cannot support concurrent readers without a contract change → surface it."*
  → The coordinator **already** supports concurrent reads (no change needed for the read path, §2a).
  The *only* coordinator contract question is the refresh-vs-reader **policy** (block vs serve
  last-good) — surfaced as **D-W**, with the safety proof. ✔
- *"If a new subsystem seems needed beyond concurrent-dispatch + reader/writer-safety + the cancel
  seam → STOP + DECISION_REQUIRED."* → **No new subsystem is needed.** The only candidate abstractions
  (pool, cancel token) are gated behind D-S/D-K and recommended against unless demonstrated. No hard
  global stop is warranted; the boundaries are recorded and ratifiable. ✔

---

## 12. Slice split, dependencies & ENRICH-LIFECYCLE-1 composition

### 12.1 Split (per D-SPLIT)
- **B1 — concurrent connection handling** (Design A + B): state `Send+Sync`, concurrent dispatch in
  `run_socket`, D-C/D-S/D-W. Delivers behaviors 1 + 2. The headline #1 fix.
- **B2 — query-path cancellation** (Design C): D-K, the checkpoint seam. Delivers behavior 3. Depends
  on B1.

### 12.2 Dependency notes
- `docs/ROADMAP.md` Dependency notes already orders "daemon concurrency → query-path cancellation."
- B2's recommended detection (K-A heartbeat-write) is cleanest once B1's thread-per-connection owns the
  stream — confirming the order.

### 12.3 ENRICH-LIFECYCLE-1 composition (the shared seam)
`docs/ROADMAP.md` states ENRICH-LIFECYCLE-1 (run enrichment as a daemon **background task** after
index/refresh, atomic snapshot hand-off) is "**Not blocked on DAEMON-CONCURRENCY-1** … shares only
state-safety, which the snapshot model makes an atomic pointer swap." The **ratifiable decision** for
this composition is **D-E-ENRICH-COMPOSITION** (§8); this section is its evidence/analysis. This spec
confirms they **compose** via one seam:
- A background enrich is just **another writer** in the existing model: it takes the DB write lock +
  coordinator refresh state (`dispatch.rs:2267/2270`) and swaps the LiveGraph under
  `livegraph.write()` (`livegraph_refresh.rs`), flipping the snapshot BUILDING→READY atomically.
- **The thing it needs from this slice is exactly D-W = W-B:** "readers serve last-good while a
  background writer runs." If D-W ships as W-B, ENRICH-LIFECYCLE-1's "index returns fast syntax;
  enrich upgrades the graph behind it, readers never blocked" is *already* satisfied by the same
  reader/writer-during-write decision (§6 D-W). If D-W = W-A (keep block), ENRICH-LIFECYCLE-1 must either run
  enrich without holding the repo read-exclusive or carry its own relax — i.e. it would re-open D-W.
- **Decision (ratifiable):** see **D-E-ENRICH-COMPOSITION** (§8) — recommended **E-A**: ratify
  **D-W = W-B** so the two features share one seam and ENRICH-LIFECYCLE-1 never re-litigates the
  coordinator contract. They do not otherwise contend (concurrency = concurrent client access;
  enrich = autonomous background work).

---

## 13. Out of scope

- Instruction-granular cancellation / SQLite-savepoint rollback for writes — D5b stays
  checkpoint-granular (`docs/TECH-DEBT.md:2691-2699`); not reopened here.
- Cross-DB / global write parallelism beyond the existing per-DB write lock.
- ENRICH-LIFECYCLE-1's own background-scheduling design (separate queued slice) — this spec only
  defines the shared reader/writer-during-write seam it composes with (§12.3).
- Any production code in THIS slice (design only; B1/B2 IMPL slices execute it).
- `docs/ROADMAP.md` / `docs/TECH-DEBT.md` / `CURRENT_SLICE.md` / `docs/VISION.md` edits (out of scope
  per the selection packet).

---

## 14. Ratification (operator — 2026-06-23)

Ratified after a **two-agent adversarial decision review** (the first use of that mode): the builder
recommended the §8 slate → Codex (reviewer) adversarially challenged the recommendations against
source → the builder rebutted/conceded → the two **converged**. The review caught a real blocker the
relay's artifact-review had approved: **W-B reintroduces a cross-store split-brain** (SQLite snapshot
and the in-memory LiveGraph swap independently, with no captured per-request epoch — `dispatch.rs:2677-2685`
+ `agent/src/orient/repo.rs:98-105` + `orient_serve/storage_port_impl.rs:38-49,111-126`; `path` stamps a
SQLite `snapshot_uid` onto a LiveGraph-served answer, `dispatch.rs:1760-1766`). It also surfaced a latent
hole the spec missed: `livegraph_refresh`/`livegraph_preload` acquire **no** coordinator
(`dispatch.rs:764-880`) — harmless serially, unsafe under concurrent accept regardless of W-A/W-B.

**The §8 RECOMMENDED cells for D-S, D-W, D-E are SUPERSEDED by this section** (the matrices remain as the
options audit trail). Binding slate the IMPL executes without re-opening:

| Decision | RATIFIED | Change from §8 |
|---|---|---|
| **D-C** | **C-B** — thread-per-connection + counting-semaphore cap, **BP-BUSY** over-cap. Cap default 64 is an **arbitrary policy** knob (`RMAP_DAEMON_MAX_CONNS`), not source-derived. | unchanged (number labeled policy) |
| **D-S** | **S-A** — connection-per-operation using the **normal `StorageConnection::open`**. **Fast-open WITHDRAWN** (skipping `run_migrations` could serve an unmigrated schema — a Layer-0 honesty violation). A fast-open is admissible only behind daemon-start schema validation + a cheap schema-marker recheck, or via S-B if profiling earns it. | fast-open dropped |
| **D-W** | **W-A** (keep the refresh read-block) **+ bring `livegraph_refresh`/`livegraph_preload` (and any future background enrich writer) under the repo coordinator.** W-A is NOT safe without this fix — the uncoordinated LiveGraph writers defeat the read-guard. **W-B WITHDRAWN**, deferred to `DAEMON-W-B-EPOCH-1`. | W-B → W-A + coordination fix |
| **D-K** | **K-A** — heartbeat-write cancel via the D5b seam. **Documented limit:** a connected-but-not-reading peer can block the heartbeat write (cancel latency unbounded in that case); K-B is the upgrade trigger. | unchanged + documented caveat |
| **D-SPLIT** | **B1 → B2.** B1 = concurrent dispatch + state `Send+Sync` + **W-A** + LiveGraph-writer coordination + **S-A normal-open**. B2 = query cancellation (D-K). | B1 scope corrected (W-A, +coordination) |
| **D-E** | **E-B** — keep ENRICH-LIFECYCLE-1 independent for now. Under the corrected model a background enrich IS just an uncoordinated LiveGraph writer, so B1's coordination fix is exactly what enrich reuses later. E-A revisited with W-B. | E-A → E-B |

**Deferred (not lost):** `DAEMON-W-B-EPOCH-1` — capture a request-level `(ready_snapshot_uid,
livegraph_fingerprint)` epoch once and thread it through **every** read in a request (eliminate the
double snapshot-resolve in `orient/repo.rs` + the per-method decorator reads), then amend §6 to prove
**whole-request join coherence** (not just per-store atomicity) and re-enable W-B + E-A. Queued in `docs/ROADMAP.md`.

**§6 correction (binding):** the §6 safety argument proves per-store atomicity, which is correct but
**insufficient** — it must prove whole-request cross-store coherence. The IMPL of B1 relies on W-A's
read-guard exclusion (valid only after the coordination fix), NOT on the §6 cross-store claim.

**D-K granularity resolution (2026-06-25, B2-CHECKPOINT-GRANULARITY → A).** During the first B2 IMPL
attempt the builder delivered handler-boundary checkpoints only (cancel *before* heavy work) and
escalated, because the heavy loops sit inside opaque storage/agent calls and reaching them needs a
cross-cutting cancel seam. A two-agent decision review surfaced **Option A (keep §7.3: in-loop
checkpoints, full)** vs **Option B (handler-boundary only, reduced guarantee)**. **Operator ratified
A.** So B2 implements cancellation that fires *inside* the heavy loops: a checkpoint/`CancelToken`
threaded through the Rust-side loops (cycles/path/orient/check/trust/explain) reusing the D5b
emitter-`Err` seam, **plus** `sqlite3_interrupt` (rusqlite interrupt handle) for the SQL-bound
`compute_module_stats` path (which cannot be checkpointed mid-statement). The cross-cutting cancel
seam is accepted by this ratification; §7.3 stands unchanged. If the seam would require a NEW crate
boundary or a breaking trait-contract change (beyond threading an optional cancel param), the IMPL
STOPs + surfaces (it does not restructure a boundary unilaterally).

**B2 COMPLETE (2026-06-26).** D-K shipped in three reviewable slices after the single mega-slice
blocked: **DAEMON-CANCEL-1** (cancel seam + `run_interruptible` panic≠Cancelled fix + cycles
Tarjan + path BFS), **DAEMON-CANCEL-2** (stats `sqlite3_interrupt`), **DAEMON-CANCEL-3**
(orient/check/trust/explain — cycle Tarjan, complexity `FETCH_ALL`, trust `compute_module_stats`
SQL + sample loop). A Codex adversarial decision review on CANCEL-3 scope **refuted** (with cites)
a hypothesis that orient/check/trust/explain were light — they reach heavy uncancellable storage
work — confirming the operator's Option A. Every heavy query path now cancels mid-flight; honest
large-fixture in-flight tests throughout. The B1 concurrency + B2 cancellation arc is fully shipped.
**Still deferred:** `DAEMON-W-B-EPOCH-1` (the cross-store epoch → re-enable W-B + E-A) and the K-B
fd-watcher (the K-A limitation's upgrade).
