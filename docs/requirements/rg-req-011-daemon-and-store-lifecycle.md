<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-011",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "operational-architecture" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "the-core" },
    { "kind": "document-section", "path": "docs/slices/state-root-separation-1.md", "fragment": "policy" },
    { "kind": "document-section", "path": "docs/slices/snapshot-retention-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/daemon-residuals-2.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/self-pollution-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-011-L01", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L02", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L03", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L04", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L05", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L06", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L07", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L08", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L09", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L10", "parentId": "RG-REQ-011" },
    { "id": "RG-REQ-011-L11", "parentId": "RG-REQ-011" }
  ]
}
-->
# RG-REQ-011 — The daemon serves current state in milliseconds, never hangs, never serves a broken store, and never touches state it was not pointed at

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Operational Architecture](../VISION.md#operational-architecture) (a long-lived daemon holds current repo state; SQLite is persistence and fallback, not the conceptual centre; retention biases to latest full + minimal transient state); [VISION — The Core](../VISION.md#the-core) commitment 3 (current-state, in milliseconds — the economic claim); the ratified contracts of DAEMON-CONCURRENCY-1, DAEMON-W-B-EPOCH-1, FOREGROUND-LOCK-1, DAEMON-RESIDUALS-1/2/2B/2C, SNAPSHOT-RETENTION-1, RETENTION-POLICY-1, DAEMON-CRASH-RECOVERY-1, FORGET-REPO-1, STATE-ROOT-SEPARATION-1, STDIO-STATE-ROOT-1, STDIO-TRANSPORT-1, SOCKET-RENDEZVOUS-1, SELF-POLLUTION-1, ENRICH-LIFECYCLE-1; the 2026-09-04 production retention incident.

## High-level requirement

The `rmapd` daemon shall answer concurrent clients from the current snapshot without head-of-line blocking, report contention as a named, bounded `Busy`, keep the store bounded by a retention pass that always finishes, refuse to serve a corrupted or half-rebuilt store by name, recover from crashes without phantom work or unreclaimed bytes, and confine every read and write to the state root it was given — so that an isolated run can never touch the operator's real state.

**Scope:** daemon runtime, transport, store lifecycle, isolation and background passes. Not the content of any answer (other Hs).

**High-level acceptance:** all L entries hold under the daemon test suites and the isolated dogfood; L11's latency floor is ratified with a number before it is claimed.

## Low-level requirements

### RG-REQ-011-L01 — Concurrent clients without head-of-line blocking

Each accepted connection shall be served independently; the accept loop shall never block on request execution; a connection cap (default, env-overridable) returns an honest at-capacity response; responses never interleave across connections; a request pins one snapshot epoch and never re-reads "latest" mid-request; `Writing` excludes readers, `Refreshing` does not.

**Verification criterion:** `daemon-transport/tests/concurrency.rs` (`concurrent_no_head_of_line_blocking`, `over_cap_connection_gets_busy_then_closed`, `no_cross_connection_response_interleaving`); `daemon-runtime/tests/concurrency_dispatch.rs` (`reader_admitted_through_dispatch_while_refresh_in_flight`, `inflight_index_holds_write_lock_and_reader_sees_last_good_ready`).

**Evidence (v0.18.0):** OBSERVED MET (serial-daemon debt closed 2026-07-17).

### RG-REQ-011-L02 — Contention is a named Busy within a bounded wait, on both lock layers

When a store lock or the repo coordinator guard cannot be acquired, the client shall receive a typed `Busy` naming the holder class and how long it has held (`a background enrich pass (started 37s ago) is writing this repo's store`), within the foreground patience budget (4 × 150 ms) — never a hang, never an internal error, never a raw store path as the only identification. Every background operation registers its kind so its Busy is named.

**Verification criterion:** `daemon-runtime/tests/foreground_lock_seam.rs`; `daemon-runtime/src/foreground_open/tests.rs` (`busy_message_names_registered_holder_class`, `busy_message_is_honest_unknown_with_no_registered_op`); `storage/src/connection.rs` writer-past-timeout tests; smoke: every Busy in a batch run names its holder.

**Evidence (v0.18.0):** PARTIALLY MET — the bounded typed Busy holds on both layers (7c14033); the seed pass is unregistered so 2 of 5 batch-load Busy messages in audit round six were the generic form (DAEMON-RESIDUALS-3 `OpKind::Seed`), and every Busy leaks the hex store path as its only repo identification.

### RG-REQ-011-L03 — Retention is bounded and always finishes

Steady state shall hold at most the current snapshot and its parent (the delta-refresh base) plus user baselines; older snapshots are pruned at index/refresh commit; a prune runs in chunks inside a time budget and, on deadline, stops at a chunk boundary reporting what remains, leaving every pruned snapshot atomic. A retention pass that cannot finish within its budget is reported, never silently restarted from zero.

**Verification criterion:** `storage/src/retention/tests/{classify,prune,lifecycle,reproduce}.rs` (incl. `retention_prune_benchmark_gate`); `daemon-runtime/tests/snapshot_retention.rs::three_real_indexes_prune_to_current_only`; `retention_pass.rs::steady_state_keeps_current_and_parent_prunes_older`.

**Evidence (v0.18.0):** OBSERVED MET (migration 035; harness 374 s → 0.1 s; insert cost +5.1% wall reported). The 2026-09-04 incident (4.8 GB, 29 snapshots, prune never committing) is the baseline this closed.

### RG-REQ-011-L04 — Rebuild is explicit; a half-rebuilt store refuses to serve and names the rerun

`rmap repo rebuild <path>` shall wipe and reindex only on explicit intent (`--yes` or confirmation), take the writing guard fairly, bounce with a named Busy rather than deleting under a reader, write a synced `<store>.rebuilding` sentinel before the first destructive step and remove it after commit; any open of a sentinelled store — serving, enrichment, reconcile — refuses with "rebuild interrupted — run `rmap repo rebuild <path>` again", and `doctor` names it. There is no automatic recovery protocol.

**Verification criterion:** `daemon-runtime/tests/repo_rebuild.rs` (refuse-without-confirm; discard-and-keep-registry; bounce-on-writer; sentinel refuses reads; sentinel-unlink failure stays gated); `gated_open_enumeration.rs` (every open path goes through the gate).

**Evidence (v0.18.0):** OBSERVED MET (1f77e36; human ruling detect-and-name).

### RG-REQ-011-L05 — A crashed daemon leaves no phantom work and no unreclaimed bytes

On boot the daemon shall mark every `building` snapshot with no live operation as `interrupted (daemon restart)`, log start/phase/outcome per operation, and scan for orphans (DB files absent from the registry, registry entries with dead paths, sidecars without a base store, orphan seed vectors); `doctor` renders every orphan class with a next action; `rmap maintenance gc [--dry-run]` reclaims them reporting bytes; boot reconcile scans but never reclaims; a failed stat is unknown, not zero.

**Verification criterion:** `daemon-runtime/tests/daemon_crash_recovery.rs`, `forget_repo.rs`; `reclaim.rs` (`scan_classifies_all_three_orphan_classes`, `scan_stat_failure_on_an_orphan_is_unknown_not_zero`); `doctor/daemon_info.rs::orphan_probe_renders_all_three_classes_with_next_actions`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-011-L06 — A run is fully isolated by its state root

With `RMAP_STATE_ROOT` (and `RMAP_SOCKET_PATH`) set, every read and write shall be confined to that root; a sandbox root blocks writes to the user-authority class while allowing operational and derived-cache classes; the operator's registry and stores are never touched. A retained root served by a daemon is NOT read-only: a baseline is a copy served with `RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off`, or a `git worktree` before-binary on a fresh isolated index.

**Verification criterion:** `daemon-runtime/src/state.rs` mode tests; `scripts/dogfood-isolated.sh`; operator proof: registry sha256 of the real root identical before/after any isolated run.

**Evidence (v0.18.0):** OBSERVED MET mechanically; violated twice in practice at the protocol level (a builder read the operator's real root; a serving daemon mutated a retained "read-only" root via chained enrichment) — hence the baseline rule above is part of this L.

### RG-REQ-011-L07 — rmap does not index its own exhaust

The product's own generated artifacts (map sidecars carrying the rmap marker, the tool's state directory, OS noise, `.env*`) shall be excluded from the file inventory, the doc inventory and drift computation by ONE shared predicate; with zero indexed files changed, `INDEX_DRIFT` is Pass, never Incomplete.

**Verification criterion:** `doc-facts/src/self_generated.rs` tests; `daemon-runtime/src/index_drift_tests.rs::partition_excludes_only_proven_exhaust_and_counts_it`; a K=0 → Pass calibration test (to be added — none exists).

**Evidence (v0.18.0):** OBSERVED MET for the predicate; UNKNOWN for the K=0 calibration.

### RG-REQ-011-L08 — The CLI works where a Unix socket cannot be bound

On EPERM/EACCES connecting to the socket, the client shall fall back to running the daemon as a stdio subprocess (`RMAP_TRANSPORT=stdio` forces it); it shall NOT fall back on ECONNREFUSED, ETIMEDOUT or protocol errors; daemon-required commands then fail honestly rather than degrading silently.

**Verification criterion:** `rgr/src/daemon_client/transport.rs` permission-detection tests; `daemon_client/mod.rs::execute_or_fallback_*`; `stdio_transport.rs::stdio_transport_ping`.

**Evidence (v0.18.0):** OBSERVED MET (load-bearing for Codex-sandboxed builders, which cannot bind sockets).

### RG-REQ-011-L09 — Socket and store locations derive from the OS, not a mutable environment

The daemon socket path shall be computed from the passwd home at the platform-native location (macOS `~/Library/Application Support/repo-graph/daemon.sock`; Linux `~/.local/share/rmap/daemon.sock`), overridable only by `RMAP_SOCKET_PATH`; a stale socket file is removed on bind; installer, runtime and service agree on one path authority.

**Verification criterion:** `platform-paths/src/{home,socket}.rs` tests; `daemon-transport/src/socket.rs::bind_removes_stale_socket`; `rgr/src/cli/paths.rs` as the single authority.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-011-L10 — Background passes are optional, bounded, yield to writes, and are honest about being off

Enrichment and retention shall run after an index/refresh by default, register in the activity registry, yield to and cancel for explicit writes, and each have an env opt-out (`RMAP_AUTO_ENRICH`, `RMAP_AUTO_RETENTION`); when off or when a toolchain is absent, `doctor` states that with the reason. The retention line in `doctor` shall always state the standing (snapshots held / policy / last pass: never or its duration and outcome) so a fresh store still shows the mechanism exists.

**Verification criterion:** `daemon-runtime/tests/enrich_lifecycle.rs` (opt-out, absent toolchain, chained retention); `retention_pass.rs` toggle tests; `doctor/daemon_info.rs` probes; field: `rmap doctor` on a one-snapshot store renders the standing, not only "cleanup: none yet".

**Evidence (v0.18.0):** PARTIALLY MET — the toggles and skips hold; the always-rendered retention standing is NOT MET (DAEMON-RESIDUALS-2B's fields were DORMANT on both the audit root and production — the deep-vertical rule). Preservation: a chained pass must not hold the store across resolver work (ENRICH-GUARD-1 filed).

### RG-REQ-011-L11 — WITHDRAWN: warm answers have a measured latency floor

STATUS: WITHDRAWN 2026-09-14 (human, option C — no floor): "the vision promise is not about a benchmark on a particular hardware piece — it is about having good algorithms and data structures and good wiring between the modules — optimal solutions to known problems — good organization and not leaking stuff." The ID stays reserved; no numeric bound will be authored. Previously: no bound was ratified and no benchmark gate existed; this L becomes an obligation only when the human ratifies a bound with its measurement basis (machine, corpus, sample count, percentile, cache state). A warm `orient` (daemon resident, snapshot loaded) on a repository of the smoke corpus shall complete within a ratified bound, measured per release on the smoke corpus and published with the audit; a cold first answer may exceed it and shall say so. The bound is TO BE RATIFIED by the human — data points: django `stats` 760,594 ms → 2,981 ms after RMAPD-PERF-1; 9.8 s cold first `orient` on a 100k-node snapshot; the 300 s client read window is a symptom threshold, not a target.

**Verification criterion:** NO EXISTING VERIFICATION — no benchmark gate asserts a latency floor (the only gate is `retention_prune_benchmark_gate`). A per-release latency table is the smallest acceptable proof.

**Evidence (v0.18.0):** UNKNOWN — the economic claim of VISION commitment 3 has measurements but no asserted floor. Unresolved upstream decision: the numeric bound.

## Preservation obligations named by the ratifying specifications

- Wire protocol/envelope additive only; the protocol error-code SET is closed — a new code is a DECISION_REQUIRED.
- W-B epoch invariants: pin-once; `Writing` read-excluding; only `Refreshing` relaxed.
- The 300 s client read window with `RMAP_LONG_OP_READ_TIMEOUT_SECS` as the only override.
- Env var names: `RMAP_STATE_ROOT`, `RMAP_SOCKET_PATH`, `RMAP_TRANSPORT`, `RMAP_AUTO_ENRICH`, `RMAP_AUTO_RETENTION`, `RMAP_LONG_OP_READ_TIMEOUT_SECS`.
- Platform socket paths and passwd-home derivation; `rgr/src/cli/paths.rs` as the path authority.
- Retention's protected classes (current + parent + user baseline); per-snapshot prune atomicity; the stdio fallback error table.
- Store schema evolution is migration-only and forward; the `.rebuilding` sentinel is detect-and-name with no recovery protocol or layout migration.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING (L11's bound is an explicit open decision).
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
