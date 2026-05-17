# CURRENT_SLICE.md

## Current Priority

**REG-1: Repo Registry + CWD Auto-Discovery** — IN PROGRESS

Slice doc: `docs/slices/reg-1-repo-registry.md`

### Goal

Eliminate `<db_path> <repo_uid>` from normal CLI workflows. Daemon resolves repos from cwd via registry.

### Target Contract

```bash
# Normal path (daemon resolves from cwd)
rmap index .
rmap orient
rmap modules list
rmap explain src/foo.ts

# Not required in normal use
rmap orient ./some.db some-repo-uid   # REMOVED
```

### Progress (2026-05-17)

**Infrastructure (IMPLEMENTED):**
- RepoRegistry in daemon-runtime
- Registry persistence (registry.json)
- `resolve_repo`, `list_repos`, `repo_info`, `repo_alias`, `repo_remove` handlers
- REG-1 repo resolution helper: `resolve_and_load_repo()`
- DaemonClient in CLI for daemon communication

**Command Families Migrated:**
| Family | Status |
|--------|--------|
| `surfaces` | list, show — REG-1 |
| `boundaries` | list, show, summary, links — REG-1 |
| `modules` | list, show, files, deps, violations, unowned — REG-1 |

**Also REG-1 (verified from code):**
| Family | Commands |
|--------|----------|
| orient family | orient, check, explain |
| graph family | callers, callees, path, imports, cycles, stats |
| other | trust, gate, deps, docs, contracts, inferences, resource |
| dead | Disabled (not legacy, just disabled) |

**Still Legacy (db_path + repo_uid required):**
| Command | Notes |
|---------|-------|
| `assess` | Write operation |
| `enrich` | Write operation |
| `policy` | Legacy |
| `modules boundary` | Write operation |
| `violations` (top-level) | Mixed-responsibility in violations.rs |
| `quality/churn` | Legacy |
| `quality/risk` | Legacy |
| `quality/hotspots` | Legacy |
| `quality/metrics` | Legacy |
| `quality/coverage` | Legacy |
| `declare/*` | All declaration commands — Legacy write |

**Test Migration:**
- daemon_dispatch.rs: 87 success-path tests using daemon infrastructure
- Ignored tests: 71 remaining (46 stubs deleted)
  - 43 in gate_command.rs — real implementations using old CLI contract, needs migration
  - 11 in dead_command.rs — command disabled (not REG-1 related)
  - 9 in index_contract_summary.rs — real implementations using old CLI contract
  - 8 in declare_* — legacy write operations

### Definition of Done

1. No normal documented workflow requires `<db_path> <repo_uid>`
2. Remaining legacy commands are either migrated or explicitly deferred in docs
3. Ignored REG-1 tests are reduced or accounted for
4. slice/roadmap/current-slice documents agree
5. Old positional syntax removed for migrated commands

### Blocked Slices

None. REG-1 is the current priority.

---

## Recently Implemented

**RMAPD-2: Unix Socket Transport** — IMPLEMENTED (2026-05-15)

Slice doc: `docs/slices/rmapd-2-socket-transport.md`

Daemon now runs as resident service with Unix socket transport.

### Delivered

- Unix domain socket transport (`/tmp/rmap-<uid>.sock`)
- Resident daemon lifecycle (stays alive without clients)
- NDJSON-over-socket protocol
- DaemonClient in CLI for socket communication
- Socket-based health checks in `rmap doctor`

### Validation Evidence

```
$ rmapd &
$ rmap doctor
Daemon: running (socket: /tmp/rmap-501.sock)
```

---

**LINUX-1: Linux Installer + Daemon Service** — IMPLEMENTED (2026-05-15)

Slice doc: `docs/slices/linux-1-linux-installer.md`

Validated with RMAPD-2 socket transport.

---

**CODEX-1: Codex CLI Integration** — IMPLEMENTED (2026-05-14)

Slice doc: `docs/slices/codex-1-codex-cli-integration.md`

---

**CLAUDE-1: Claude Code Integration** — IMPLEMENTED (2026-05-13)

Slice doc: `docs/slices/claude-1-claude-code-integration.md`

---

**HOOK-1A: Stdin JSON Transport Adapter** — IMPLEMENTED (2026-05-13)

Slice doc: `docs/slices/hook-1a-stdin-transport.md`

---

**MAC-1: macOS Installer + Daemon Service** — IMPLEMENTED (2026-05-13)

Slice doc: `docs/slices/mac-1-macos-installer.md`

---

**HOOK-1: rmap hook CLI Surface** — IMPLEMENTED (2026-05-13)

Slice doc: `docs/slices/hook-1-rmap-hook-cli.md`

---

## Execution Order (Distribution Track)

1. ~~DIST-1~~ — IMPLEMENTED
2. ~~HOST-1~~ — IMPLEMENTED
3. ~~REL-1~~ — IMPLEMENTED
4. ~~REL-SUPPORT-1~~ — IMPLEMENTED
5. ~~RGISTR-1~~ — IMPLEMENTED
6. ~~RMAPD-1~~ — IMPLEMENTED
7. ~~HOOK-1~~ — IMPLEMENTED
8. ~~MAC-1~~ — IMPLEMENTED
9. ~~HOOK-1A~~ — IMPLEMENTED
10. ~~CLAUDE-1~~ — IMPLEMENTED
11. ~~CODEX-1~~ — IMPLEMENTED
12. ~~LINUX-1~~ — IMPLEMENTED
13. ~~RMAPD-2~~ — IMPLEMENTED
14. **REG-1** — IN PROGRESS (current)
15. CURSOR-1 — PLANNED
