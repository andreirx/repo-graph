# CURRENT_SLICE.md

## Current Priority

**CURSOR-1: Cursor Integration** — PLANNED

Slice doc: `docs/slices/cursor-1-cursor-integration.md` (to be created)

Different integration model from Claude Code / Codex. Needs investigation.

---

## Recently Implemented

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

Slice doc: `docs/slices/cli-out-1-presentation-layer.md`

Delivered:
- Human-default plain text output for `orient`, `check`, `explain`
- `--json` flag for machine mode (full daemon envelope)
- `presentation/` module with typed response structs and renderers
- 37 renderer unit tests + 7 flag parsing tests + 6 CLI success-path tests

**Validation evidence (2026-05-18):**
```
cargo test -p repo-graph-rgr --test cli_output_mode
test result: ok. 6 passed; 0 failed
```

**Known debt:** Test harness requires `cargo build -p rmapd` pre-step.
See `docs/TECH-DEBT.md` → "CLI-OUT-1 Test Harness Pre-Build Dependency".

---

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

Slice doc: `docs/slices/reg-1-repo-registry.md`

**Closure scope: Read-side contract migration complete.**
- Write/governance families intentionally deferred
- Presentation quality handed off to CLI-OUT-1

### Delivered

- Daemon-owned repo registry with persistence
- CWD-based resolution for normal read/query surface
- `db_path`/`repo_uid` removed from normal read workflows
- Modules read-side fully migrated
- Documentation updated to reflect reality

### Explicitly Deferred

| Category | Items |
|----------|-------|
| Write operations | `assess`, `enrich`, `modules boundary` |
| Governance | `declare/*`, `policy` |
| Quality commands | `quality/*` (churn, risk, hotspots, metrics, coverage) |
| Other legacy | top-level `violations` |
| Override flags | `--repo`, `--repo-path`, `--db`, `--repo-uid` |

### Ignored Test Debt (tracked)

- 43 in gate_command.rs — real implementations using old CLI contract
- 11 in dead_command.rs — command disabled (not REG-1 related)
- 9 in index_contract_summary.rs — real implementations using old CLI contract
- 8 in declare_* — legacy write operations

---

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
14. ~~REG-1~~ — IMPLEMENTED
15. ~~CLI-OUT-1~~ — IMPLEMENTED
16. **CURSOR-1** — CURRENT
