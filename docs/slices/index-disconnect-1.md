# INDEX-DISCONNECT-1 — An index survives its client (detached completion)

Status: SPECIFIED (2026-07-03) · Track: Daemon correctness — HOTFIX, gates the
next release · Ratified: detached completion (operator, 2026-07-03)
Origin: TECH-DEBT F5 root cause (second-machine field failure, 160k-file repo)

## 1. Problem — the client's timeout kills the daemon's work

`handle_index`'s progress callback (`daemon-runtime/src/dispatch.rs`) returns
`ControlFlow::Break` when `emitter.emit()` fails. Consequence chain, observed
live twice: a >300s silent phase times out the CLI (default read timeout) →
client closes the socket → the daemon's next emit gets broken pipe → the
index **aborts mid-flight**. Hours of extraction persist as a forever-
`building` snapshot (4 GB); the ready-flip never runs; `record_index` +
`registry.save()` live only in the success branch, so `repo info` reports
"repo not indexed" — by path and by uid. The user's intent ("index this
repo") was made conditional on a socket staying open.

## 2. Ratified semantics

**An index is a durable mutation; client disconnect never aborts it.**
The daemon completes the work, persists everything, and the (dead) client's
absence affects nothing but progress delivery. Re-attachment/observability is
DAEMON-VISIBILITY-1's surface. Explicit cancellation (the shipped
DAEMON-CANCEL mechanism) remains the one deliberate way to stop a write op.

## 3. Contract

1. **Progress emission is best-effort.** Emit failure → log once per
   operation (reader-frame: "client disconnected; index continues detached")
   → `ControlFlow::Continue`. Never abort user-requested write work because
   reporting failed. Subsequent emits for that op may be skipped cheaply
   (the client is gone) but MUST NOT error the operation.
2. **Registration persists up-front.** `registry.register(...)` +
   `registry.save()` immediately at registration time, before indexing —
   the repo exists in the registry even if the index later fails. On
   success, `record_index` + save as today.
3. **No `building` limbo.** Every snapshot reaches a terminal state: on
   success the ready-flip runs unconditionally (independent of whether the
   response can be written); on a REAL failure (extraction error, panic
   caught at the boundary, explicit cancel) the snapshot is marked with its
   terminal state + reason (use the existing status vocabulary; add
   `failed`/reason only if the schema already supports it — otherwise
   surface DECISION_REQUIRED rather than migrating silently). Pairs with
   DAEMON-VISIBILITY-1's F contract (state + outcome exposure).
4. **Cancellation unchanged.** The explicit cancel path still cancels;
   query-path disconnect semantics (shipped, deliberate) are untouched —
   this slice changes WRITE-op disconnect semantics only (`index`,
   `refresh` if it shares the emitter pattern — verify and cover it).

## 4. Stop conditions

- Additive only; no daemon protocol breaking changes; no coordinator/epoch
  invariant changes (read the W-B epoch doc before touching anything near
  the write guard).
- If a terminal snapshot state requires a schema migration → STOP +
  DECISION_REQUIRED.
- Do NOT change explicit-cancel or query-path semantics. Do NOT commit.

## 5. Validation (end-of-slice — synchronous; TEST REPORT inlined)

- Cargo gates green from `rust/` (build / full test / fmt / clippy -D
  warnings), inlined.
- **F5 regression proof (named test):** an emitter that starts failing after
  N events → the index COMPLETES: snapshot `ready`, registry entry with
  `record_index` persisted, exactly one detached-continuation log line.
- **Up-front registration proof (named test):** an index that fails after
  registration → repo still present in the registry (queryable by path and
  uid); snapshot in a terminal non-`building` state with reason.
- **Cancel-still-cancels proof (named test):** explicit cancellation of an
  in-flight index still stops it and leaves a terminal-state snapshot.
- `./scripts/dogfood-isolated.sh` green; isolated self-index with a
  simulated client disconnect mid-index → completes to READY (transcript
  inlined).

## 6. Definition of done

A client disconnect during `rmap index` (timeout, closed terminal, sleep)
never costs the work: the daemon finishes, the snapshot is READY, the repo
is registered, and one honest log line records the detachment — proven by
the named tests + executed transcript; explicit cancel unchanged.
