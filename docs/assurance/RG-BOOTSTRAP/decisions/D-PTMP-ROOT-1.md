# D-PTMP-ROOT-1 — The sandbox state root is one fixed directory per OS, owned by `platform-paths`

Raised: 2026-09-27 by the manager, preparing PORTABLE-TMP-1 (queued by the human 2026-09-27: "queue portable tmp 1 now after the current", after an agent on a Linux machine found repo-graph hard-codes `/private/tmp`).

## The problem
When a client cannot reach the daemon socket (EPERM/EACCES, e.g. a Codex sandbox), it runs the daemon as a stdio subprocess with a sandbox-local state root (RG-REQ-011-L07). A sandbox root blocks user-authority writes (RG-REQ-011-L06). Three hand-written copies of one literal decide where that root is and whether a root is one (at 15fa3304):

- `rgr/src/daemon_client/stdio_transport.rs:183`: the client creates `/private/tmp/repo-graph-agent/<uid>` (mode 0700) and injects it.
- `daemon-runtime/src/lib.rs:269`: the socket daemon clears that directory at startup (sandbox state is ephemeral).
- `daemon-runtime/src/state.rs:876`: the daemon treats any state root starting with `/private/tmp/` as `SandboxLocal`.
- `rgr/src/platform/mod.rs:292` renders the same literal in `doctor`.

`/private/tmp` exists only on macOS. On Linux the fallback cannot create its root (an ordinary user cannot create `/private`), and sandbox detection never fires. RG-REQ-014-L09 states "Linux with systemd is code-complete", and that is false for this path.

Resolved: 2026-09-27 by the OPERATOR (in-place-manager): option A. The human may override.

## Options
- **A (ratified): a fixed root per OS, in `platform-paths`.** `/private/tmp` on macOS, unchanged; `/tmp` on Linux. `platform-paths` exposes the sandbox root for a uid and the "is this state root a sandbox root" predicate. Client, daemon clearing, daemon detection and `doctor` all call it.
  - Reward: macOS behaviour is byte-identical; Linux gets a root every user can create; one definition replaces three copies.
  - It follows the crate's stated principle: paths shared by client and daemon do not come from session environment, the same reason socket paths use the passwd home, not `$HOME`.
  - Risk: a Linux sandbox that forbids `/tmp` would still fail. The fallback reports that as a named error, as it does today for any uncreatable root.
- **B: `std::env::temp_dir()` / `$TMPDIR`.** Risk:
  - On macOS this changes today's root to `/var/folders/…/T/`, a long path that pressures the 104-byte Unix socket limit under isolated roots.
  - `$TMPDIR` differs between processes: a launchd/systemd socket daemon and an agent shell. The socket daemon would then look for, and clear, a different directory from the one the agent created, so the predicate and the root disagree.
- **C: a configurable root (`RMAP_SANDBOX_ROOT`).** Risk: a new environment contract for a case nobody has asked to configure; `RMAP_STATE_ROOT` already overrides the state root entirely.

## Consequences
- Scripts in `scripts/` that hard-code `/private/tmp` for isolated roots use the same per-OS rule (`/private/tmp` on Darwin, `/tmp` elsewhere). Script runs on macOS keep their current paths, and with them their current sandbox classification. They do not switch to `$TMPDIR`, for the reasons in B.
- The macOS-only inventory tests (`daemon-runtime/src/handlers/inventory/tests.rs`, `#[cfg(target_os = "macos")]`) run on both OSes through the shared predicate.
- Test data that merely spells a path (`reclaim.rs:1565`, `doctor/daemon_info.rs:989`) and doc comments that cite a past calibration path are not affected.
