# D-IUR-FASTPATH — the default `imports <file>` answer reads SQLite for the rows without a confirmed target, on every route including a GREEN-certified LiveGraph serve, and names that source

Raised: 2026-10-03 by the document review of IMPORTS-UNRESOLVED-REMAINDER-1 (INPUT-1, cycle 0; reviewer codex gpt-6-sol; `.agent-manager/slices/IMPORTS-UNRESOLVED-REMAINDER-1-PREP/review-0.json`, local).

**The problem in plain words.** `imports <file>` is "LiveGraph-first" by default: on a GREEN import no-loss certificate the answer is served from the in-memory graph with no SQLite read (`daemon-runtime/src/livegraph_feed.rs:1966-1985`; `docs/cli/rmap-contracts.md:110-118`, "SQLite-free migrated default paths"), and the JSON says `backend_used: "livegraph"`. The rows this slice adds — the file's imports that never bound to an indexed file — exist only in SQLite (`unresolved_edges`). A complete default answer therefore needs one per-file SQLite read even on a GREEN serve, and the backend claim must stop saying the whole answer came from LiveGraph. The allocation had not named this loss.

**Options presented to the human (reward / risk).**
- A — accept one per-file SQLite read on every default `imports <file>` call and qualify the backend claim: the smallest complete answer, the rows live where SQLite keeps them, one indexed lookup per call, no cache to keep coherent; `imports` leaves the SQLite-free default set and a failed store read fails the request (the honest outcome) — a new failure path on the fastpath.
- B — carry the unresolved facts in the resident LiveGraph answer: the GREEN path stays SQLite-free; a new IR datum with invalidation on index/refresh/epoch and a per-file parity proof between LiveGraph's observation classes and SQLite's categories — a much larger slice.
- C — defer: nothing changes; agents keep reading `0 imports` beside known rows; RG-REQ-006-L12 stays NOT MET.

Resolved: 2026-10-03 by the HUMAN: **"Ok a"** — option A.

## What this record authorizes (encoded in IMPORTS-UNRESOLVED-REMAINDER-1 §2.1 item 3, §2.4, IUR-C03, IUR-C06)
- The one attach site reads `find_unresolved_file_imports` on BOTH default routes — the GREEN-cert LiveGraph serve and the SQLite fallback — and on `--engine sqlite`. A storage error fails the request by name (never an empty array).
- `backend_used` keeps its meaning for the resolved rows (so every existing consumer and test of that key is unchanged); the answer ADDS `unresolved_source: "sqlite"` on every route that carries `unresolved`, so no answer claims a LiveGraph-only origin for facts read from SQLite. The human renderer strips both routing fields as it does today.
- The CLI contract's "SQLite-free migrated default paths" statement is amended for `imports`: its default answer reads SQLite for the rows without a confirmed target; the other five migrated defaults are unchanged.
- It does NOT authorize caching the unresolved rows in the LiveGraph (option B) nor changing the GREEN serve of the resolved rows.
