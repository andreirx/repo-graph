# RMAPD-1: Daemon Binary Target

Status: IMPLEMENTED
Depends: None (unblocks REL-1)
Track: Distribution / Install / Host Integration

## Objective

Create the `rmapd` binary target to complete the two-binary distribution model
with correct architectural boundaries.

## Context

The distribution track (REL-1) established a two-binary architecture:
- `rmap` — CLI binary (user/admin/operator interface)
- `rmapd` — daemon binary (long-lived service)

REL-1 implementation (workflows, installer) expects both binaries to exist.

## Architecture Decision

### Option A: Second [[bin]] in repo-graph-rgr

**Rejected** — would couple CLI and daemon in same crate.

### Option B: Separate crate with shared runtime

**Chosen** — daemon runtime extracted to dedicated crate.

## Implementation

### Crate Structure

```
rust/crates/
├── daemon-runtime/          # NEW: shared daemon runtime
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           # run_daemon(), exports
│       ├── dispatch.rs      # ServiceDispatcher
│       ├── state.rs         # DaemonState, RepoState, RepoKey
│       └── util/
│           ├── mod.rs
│           ├── context.rs   # compute_storage_root_path
│           ├── time.rs      # utc_now_iso8601
│           └── trust.rs     # compute_trust_overlay_for_snapshot
├── rmapd/                   # Daemon binary
│   ├── Cargo.toml           # depends on daemon-runtime
│   └── src/main.rs          # --version, --help, --config, run_daemon()
└── rgr/                     # CLI binary
    ├── Cargo.toml           # depends on daemon-runtime
    └── src/
        ├── main.rs          # CLI commands + deprecated `daemon` shim
        └── lib.rs           # no daemon module (removed)
```

### Dependency Graph

```
                ┌──────────────────────────────────┐
                │       daemon-runtime              │
                │  (state, dispatch, run_daemon)    │
                └──────────────────────────────────┘
                         ▲              ▲
                         │              │
                   ┌─────┴─────┐  ┌─────┴─────┐
                   │   rmap    │  │   rmapd   │
                   │   (CLI)   │  │  (daemon) │
                   └───────────┘  └───────────┘
```

Both binaries depend on `daemon-runtime`. Neither depends on the other.

### `rmap daemon` Deprecation

The `rmap daemon` subcommand is preserved as a **compatibility shim** with
explicit deprecation warning:

```
$ rmap daemon
warning: 'rmap daemon' is deprecated. Use 'rmapd' instead.
         The rmapd binary is the dedicated daemon executable.
```

The shim calls `repo_graph_daemon_runtime::run_daemon()` and will be removed
in a future release.

## Responsibility Boundary

**rmap (CLI):**
- User commands (index, refresh, callers, callees, etc.)
- One-shot execution
- Human-readable output
- Exit codes for scripting
- **Deprecated:** `daemon` subcommand (compatibility shim only)

**rmapd (daemon):**
- Long-lived service process
- NDJSON stdin/stdout protocol
- Multi-repo coordination
- Session management
- No CLI command surface (except --version, --help, --config)

**daemon-runtime:**
- `DaemonState` — per-repo state management
- `ServiceDispatcher` — request routing
- `run_daemon()` — main loop entry point
- Utility functions for storage paths, time, trust overlays

## Definition of Done

1. ✅ `rmapd` binary target exists
2. ✅ `rmapd --version` outputs version and exits 0
3. ✅ `rmapd --help` outputs usage and exits 0
4. ✅ `rmapd` (no args) starts daemon and accepts NDJSON on stdin
5. ✅ `rmap daemon` shows deprecation warning
6. ✅ Daemon runtime extracted to `daemon-runtime` crate
7. ✅ `rmapd` depends on `daemon-runtime`, NOT on `repo-graph-rgr`
8. ✅ `.github/workflows/ci.yml` builds both `-p repo-graph-rgr` and `-p rmapd`
9. ✅ `.github/workflows/release.yml` packages both binaries
10. ✅ All daemon dispatch tests pass (36 tests)

## Verification (Executed)

```
$ cargo build -p repo-graph-daemon-runtime -p rmapd -p repo-graph-rgr
   Finished `dev` profile [unoptimized + debuginfo] target(s)

$ ./target/debug/rmap --version
rmap 0.1.0

$ ./target/debug/rmapd --version
rmapd 0.1.0

$ echo "" | ./target/debug/rmap daemon
warning: 'rmap daemon' is deprecated. Use 'rmapd' instead.
         The rmapd binary is the dedicated daemon executable.

$ cargo test -p repo-graph-daemon-runtime
running 13 tests ... ok

$ cargo test -p repo-graph-rgr --test daemon_dispatch
running 36 tests ... ok
```

## Out of Scope

- Daemon functionality changes
- Service registration (MAC-1, LINUX-1)
- Protocol changes
- Configuration schema changes
- Removal of `rmap daemon` shim (future cleanup slice)

## Deliverables

1. ✅ `rust/crates/daemon-runtime/` — shared daemon runtime crate
2. ✅ `rust/crates/rmapd/` — dedicated daemon binary crate
3. ✅ Updated `rgr` to use daemon-runtime and deprecate `daemon` subcommand
4. ✅ Updated CI workflow to build both packages
5. ✅ Updated release workflow to build both packages
