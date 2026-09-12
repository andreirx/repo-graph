<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-014",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "distribution-principles" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "protocol-surface-standard" },
    { "kind": "document-section", "path": "docs/slices/dist-1-distribution-install-contract.md", "fragment": "core-decisions" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-014-L01", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L02", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L03", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L04", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L05", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L06", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L07", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L08", "parentId": "RG-REQ-014" },
    { "id": "RG-REQ-014-L09", "parentId": "RG-REQ-014" }
  ]
}
-->
# RG-REQ-014 — Installing, running and integrating rmap is binary-first, reversible and honest about its platforms

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Distribution Principles](../VISION.md#distribution-principles) (daemon as first-class runtime; binary-first; host integrations deterministic, reviewable, reversible; lifecycle policy in `rmap hook`; macOS then Linux, Windows deferred); [VISION — Protocol Surface Standard](../VISION.md#protocol-surface-standard) layer 3 (external workflow instructions); the ratified contracts of DIST-1, MAC-1, INSTALL-ROBUSTNESS-2, HOST-1, CLAUDE-1, CODEX-1 (the 2026-05-14 integration doc supersedes the earlier planned one), HOOK-1/1A, REL-1, REL-SUPPORT-1, RGISTR-1, LINUX-1; deferred: MAC-2 signing, UPDATE-1, WIN-1, CURSOR-1.

## High-level requirement

A developer shall be able to install rmap without a Rust toolchain or sudo, have the daemon start and stay running under the platform's service manager, integrate an agent host with an explicit, backed-up, surgically removable patch whose policy lives in `rmap hook`, and know exactly which platforms and hosts are supported — with one version asserted everywhere.

**Scope:** installers, service registration, host integrations, hook CLI, versioning and release pipeline. Exit codes are RG-REQ-012.

**High-level acceptance:** all L entries hold on the install harness and the host-integration tests; the hook CLI has end-to-end coverage before any L that depends on it is reported satisfied.

## Low-level requirements

### RG-REQ-014-L01 — Install needs no Rust toolchain and no sudo

The default install shall download a pre-built archive and place `rmap`, `rmapd` and `rgistr` in `~/.local/bin` (user-local, platform-native paths — LOCKED); building from source is an opt-in fallback; toolchain detection reports found/not-found/unusable and absence is not fatal.

**Verification criterion:** `scripts/install.template.sh`, `scripts/dist/install.sh`, `scripts/lib/{macos,linux}.sh` (shell; no automated toolchain-detection test exists); `rgr/src/cli/paths.rs` as the path authority.

**Evidence (v0.18.0):** OBSERVED MET (v0.18.0 released d7522f9; daemon running under launchd).

### RG-REQ-014-L02 — The daemon starts itself and "started" means it answered

On macOS the installer shall register `com.repo-graph.rmapd` with launchd (`RunAtLoad`, `KeepAlive.SuccessfulExit=false`, throttle 10 s, log at `~/Library/Logs/repo-graph/daemon.log`), and install/upgrade/uninstall shall `bootout` then `bootstrap gui/$(id -u)`; install success is decided by the socket answering, not the launcher's exit status; a launcher failure warns rather than fails the install. Linux registers `rmapd.service` under systemd.

**Verification criterion:** `rgr/src/platform/{macos,linux,manifest}.rs` state parsers; `scripts/test-install-robustness-2.sh` (already-running, slow-start, launcher-failure, honest-failure scenarios).

**Evidence (v0.18.0):** OBSERVED MET (INSTALL-ROBUSTNESS-2, 19/19 harness scenarios). `rmapd --config` is reserved and ignored — not to be documented as working.

### RG-REQ-014-L03 — A host integration is explicit, backed up, surgically removable

No host automation file shall be written without an explicit user action (`rmap integrate <host>` plus confirmation); the existing file is backed up to `{original}.rmap-backup` and recorded in the install manifest; installing preserves hooks the product did not add; removal deletes only repo-graph's own entries. Restore-from-backup is never the default removal.

**Verification criterion:** `rgr/src/commands/integrate/config.rs` tests (mixed hooks, existing-requires-force, preserves-other-hooks, remove-cleans-empty, backup path); `rgr/tests/integrate_codex_command.rs`; a Claude Code end-to-end test (to be added — only arg-parsing units exist).

**Evidence (v0.18.0):** OBSERVED MET for the shared engine and Codex; UNKNOWN for Claude Code end-to-end.

### RG-REQ-014-L04 — Orientation policy lives in `rmap hook`, not in host files

Host files shall be thin shims invoking `rmap hook session-start | prompt-submit | post-edit | pre-compact | stop | status`; the hook reads context from stdin (`--from-stdin`, the standard for Claude Code and Codex), `--from-env` is legacy-only, and context resolves explicit args > env > stdin > env-file > discovery. `hook` is a verdict-family command (0 success / 1 non-fatal warning / 2 fatal) — a warning never exits 2.

**Verification criterion:** `rgr/src/commands/hook/env.rs` context tests; end-to-end tests per subcommand with a real stdin payload (to be added — NONE exist; the largest verification gap in this H).

**Evidence (v0.18.0):** UNKNOWN — implemented, thinly pinned.

### RG-REQ-014-L05 — Each supported host gets the minimum hooks that make orientation work

Claude Code's default integration registers SessionStart + Stop, with `--full` adding UserPromptSubmit/PostToolUse/PreCompact; Codex's targets `hooks.json` with a PostToolUse matcher `Edit|Write|apply_patch`; both are idempotent and reversible. Cursor (MCP + rules) is PLANNED and not claimed.

**Verification criterion:** `integrate/codex.rs` and `integrate/claude_code.rs` unit tests; `integrate_codex_command.rs`.

**Evidence (v0.18.0):** OBSERVED MET for Claude Code and Codex; Cursor has no code. The superseded `codex-1-codex-integration.md` carries retracted env-var assumptions and shall not be cited.

### RG-REQ-014-L06 — One version, asserted everywhere

`[workspace.package] version` in `rust/Cargo.toml` shall be the single canonical version; a release requires tag == manifest == `rmap --version` == `rmapd --version` == archive filename == release title == `rgistr --version`; `rmap --version` answers without a daemon; client/daemon mismatch follows the DIST-1 policy (major → `--force`; minor → warn; patch → silent).

**Verification criterion:** `rgr/tests/daemon_integration.rs::version_succeeds_without_daemon`; `tools/rgistr/src/cli.test.ts` version case; the equality chain is enforced by `scripts/bump_version_*.sh` / `scripts/cut_release_*.sh` and the release workflow (no Rust test); a client↔daemon runtime version check test (to be added).

**Evidence (v0.18.0):** OBSERVED MET operationally; the runtime cross-check has no verification. Doc defect to fix: DIST-1 has two headings numbered D4.

### RG-REQ-014-L07 — A release is reproducible from a tag and reviewable before publish

A tag push shall build `rmap-{version}-{platform}-{arch}.tar.gz` + `.sha256` containing `rmap`, `rmapd`, `rgistr`, LICENSE, README, CHANGELOG for `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`; releases come from tags only; the GitHub release is created as a draft. Binaries are unsigned and un-notarized until MAC-2 ships, and the install docs say so with the quarantine workaround.

**Verification criterion:** the release workflow and `scripts/build-installer.sh` (no automated verification of the workflow itself); the operator's release-cut checklist (`agent-manager/scripts/repo-graph-release-cut.sh`).

**Evidence (v0.18.0):** OBSERVED MET for the pipeline; signing DEFERRED (stated).

### RG-REQ-014-L08 — Upgrading is one documented action that does not strand the daemon

Re-running the installer shall replace the binaries and re-register the service (`bootout` then `bootstrap`); there is no auto-updater and the product does not claim one; `doctor`'s post-install readiness check stays light (< 5 s; the heavy probe lives in `rmap perf`).

**Verification criterion:** `scripts/test-install-robustness-2.sh`; DEV-INSTALL-DOCTOR-WAIT-1's `storage_health` method; no update/rollback test exists (feature deferred).

**Evidence (v0.18.0):** OBSERVED — manual update works; UPDATE-1 DEFERRED by decision.

### RG-REQ-014-L09 — The product states the platforms and languages it supports and does not pretend elsewhere

macOS ARM64 is primary; Linux with systemd is code-complete, validation pending; ARM64 Linux, musl and non-systemd init are out of scope; Windows is deferred with no code. `README.md`'s shipped-language list names exactly the six extracted languages; roadmap languages are labeled roadmap.

**Verification criterion:** `rgr/src/platform/linux.rs` state parsers; absence of Windows code; README inspection at each release; the audit's "could not ground-truth" list names Linux end-to-end install as pending.

**Evidence (v0.18.0):** OBSERVED MET as a stated matrix; Linux end-to-end UNKNOWN.

## Preservation obligations named by the ratifying specifications

- DIST-1 D2 (user-local, no sudo) and D3 (platform-native paths) — LOCKED; the install-manifest schema (D6); the D8 mismatch policy.
- Binary names and locations; launchd label and plist keys; log path; Linux unit name.
- Release artifact naming and contents; tag-only trigger; draft release.
- `[workspace.package] version` as sole authority and the equality chain.
- The host-integration four steps (explicit → back up → patch only selected → surgical removal) and the `.rmap-backup` naming.
- `rmap hook` subcommand names as the stable policy surface; `--from-stdin` as the host transport.
- `rgr/src/cli/paths.rs` as the single path authority.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
