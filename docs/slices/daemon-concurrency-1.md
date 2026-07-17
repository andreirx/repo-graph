# DAEMON-CONCURRENCY-1 — the serial daemon becomes concurrent (many readers, coordinated writers)

Status: SPECIFIED (2026-07-17) · Track: Operational architecture (VISION daemon promise) ·
Priority: P1 (TECH-DEBT #1 — the last known P1 on the operational layer)
Origin: `run_socket` is a single accept loop calling `handle_connection` INLINE
(daemon-transport/src/socket.rs:257-288) — one heavy request (a tens-of-minutes index)
blocks `listener.accept()` and every other client waits behind it. The VISION commits to
"concurrent queries … many readers, fewer writers"; the multi-agent world this serves is
the product's target user.

## 1. Contract

1. **Concurrent connection handling:** each accepted connection is served on its own
   thread (thread-per-connection is the smallest fit for a Unix-socket daemon with tens
   of clients; an async runtime is NOT required — justify if chosen anyway). The accept
   loop never blocks on request execution; shutdown stays prompt (the existing flag
   semantics preserved).
2. **Reader/writer coordination, explicit:** many concurrent READ requests execute in
   parallel; WRITE operations (index/refresh/enrich/retention) remain **single-writer**
   (the existing activity-registry + W-B epoch/coordinator invariants are the law — this
   slice must NOT weaken them; a second write request during an active write behaves
   exactly as today: honest busy/queued response, never a second concurrent write).
   SQLite access follows its existing connection/locking discipline (WAL readers during
   a write per current design); LiveGraph reads bind to epochs exactly as today.
3. **Orientation stays in milliseconds under load:** a read request issued DURING a
   long-running index completes without waiting for the index (the head-of-line proof).
   Progress/doctor queries during writes keep working (they already read the activity
   registry).
4. **Bounded concurrency, honest overload:** a connection cap (sensible default, env
   override) with an honest at-capacity response — never unbounded thread growth, never
   silent queueing without a message.

## 2. Stop conditions

- The W-B epoch/coordinator invariants, activity registry semantics, single-writer rule,
  and detached-completion behavior (INDEX-DISCONNECT-1) are FROZEN — coordination wraps
  around them, never rewrites them. Crash-reconciliation (DAEMON-CRASH-RECOVERY-1)
  semantics unchanged.
- Protocol/wire format unchanged (clients need no update).
- If any shared daemon state proves not-thread-safe in a way that requires more than
  guard-with-a-lock (a design change to a frozen area) → STOP + DECISION_REQUIRED.
- Do NOT commit.

## 3. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Cargo gates from `rust/` (fmt / clippy / affected crates / full suite CHUNKED — the
  standing pattern).
- Named tests: concurrent readers proceed during a long write (the head-of-line proof —
  a slow request + a fast request on separate connections, fast completes first);
  single-writer exclusion honest-busy; connection-cap overload response; prompt shutdown
  with connections open; no cross-connection response interleaving (each connection's
  responses ordered).
- Isolated live proof (/private/tmp + stdio is SINGLE-connection by design — use a
  private SOCKET daemon in the isolated state root, the EY-1 harness pattern; registry
  checksum before/after): start a real index of a large legacy repo (e.g.
  ../legacy-codebases/gstreamer), issue orient/doctor DURING it from separate
  connections, show sub-second answers while the index runs; raw transcript.

## 4. Definition of done

A long index no longer blocks anyone: concurrent reads are proven live during a real
index; single-writer + frozen invariants intact; overload is bounded and honest; full
gates green.

---

## 5. Delivery record (2026-07-17)

**DELIVERED** (2 relay cycles, fable-5). The premise was stale: concurrency shipped
2026-06-24 (`10493e8`) — TECH-DEBT #1 was never updated (now closed with the staleness
lesson recorded). This slice delivered the missing VALIDATION surface: prompt-shutdown and
no-interleaving tests added; head-of-line / honest-busy-serialization / cap-overload
re-verified; the live proof ran a real gstreamer index (~122s) with concurrent reads
served during it. The VISION's "concurrent queries, many readers" promise is now both
implemented AND proven. Note from the live run: first orient on a cold 100k-node snapshot
took 9.8s (cold-read cost, not concurrency) — a scale-chain datum.
