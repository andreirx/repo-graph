# INSTALL-ROBUSTNESS-2 — Installer tells the truth and survives GitHub's API

Status: **DELIVERED (2026-07-03)** · Track: Distribution robustness

## 0. Delivery record (2026-07-03)

Shipped via target-owned relay (builder claude, reviewer codex, approved
iteration 8) + operator verification. `resolve_version()` is redirect-first
(`github.com/…/releases/latest` Location header) with API fallback carrying
`GITHUB_TOKEN` when set; the escape-hatch error preserved. Daemon start truth:
`daemon_socket_answers` socket probe is the predicate (pre-start
already-running check; probe between retries; re-probe after the final sleep),
platform launchers warn-not-fail, summary states started / already running /
failed. Evidence: 19/19 harness scenarios (`scripts/
test-install-robustness-2.sh` — stubbed curl for redirect/API/double-failure;
socket fixtures for the three outcomes), reviewer-executed in-memory function
spot checks, `build-installer.sh` assembly + bundled-script syntax verified by
builder, reviewer, AND operator independently; dogfood-isolated green; the
operator's real daemon untouched throughout. NOTE: the fix reaches end users
only when `scripts/dist/install.sh` is uploaded to the next release. Process
note: this slice's review grind exposed the relay evidence-transport gap
(gitignored build reports invisible to git-based review) — fixed in
agent-manager (4bb03dc); the builder's tracked-report workaround
(`test-install-robustness-2.report.md`) is superseded and not committed.
Origin: real install failures on the operator's second Mac (2026-07-02)
Prior art: `dist-1-distribution-install-contract.md`, `mac-1-macos-installer.md`,
`dev-install-doctor-wait-1.md`, `daemon-socket-health-1.md`

## 1. Problems (both observed live on a fresh macOS install)

**A — Version resolution dies on GitHub's API rate limit.**
`resolve_version()` in `scripts/install.template.sh` resolves "latest" via
`api.github.com/repos/.../releases/latest`. Unauthenticated API calls are
capped at 60/hour/IP; on a shared IP the installer fails with
"could not reach api.github.com … 403". The binary download itself uses
`github.com` (not the API) and works — the API call is the only fragile hop,
for a single string.

**B — Daemon-start loop reports failure while the daemon is running.**
The installer printed "trying to start the daemon in 2s… failed", retried
5 times, and concluded failure — while the daemon was in fact up (the user
then ran it and found it already running). The start/retry loop trusts the
*start attempt's* result instead of the *socket's* liveness; "already
running" (e.g. launchd got there first, or a prior attempt succeeded late)
is misclassified as failure. This is an installer-honesty bug: the report
contradicts reality on the machine.

## 2. Contract

**A — Resolve latest without the API.**
1. Primary: resolve the latest tag from the redirect `Location` header of
   `https://github.com/<owner>/<repo>/releases/latest` (HEAD request,
   `--connect-timeout`/`--max-time` as today; parse the tag from the
   redirect URL). This hop is `github.com` — same host as the download,
   effectively not rate-limited.
2. Fallback: the existing `api.github.com` call, additionally sending
   `Authorization: Bearer ${GITHUB_TOKEN}` when `GITHUB_TOKEN` is set.
3. Error messages stay honest and actionable (keep the current
   `RMAP_VERSION=<ver>` pin escape hatch in the message; keep the split
   curl / observable-exit-code discipline this function already documents —
   its comment history records exactly why).

**B — Socket liveness is the source of truth for daemon start.**
1. Before the first start attempt: probe the socket (the health/handshake
   probe that `doctor`/`dev-install` already use — see
   `dev-install-doctor-wait-1.md`). If it answers → report "daemon already
   running (pid …)" and SUCCEED without starting anything.
2. After each start attempt / between retries: the retry loop's predicate is
   the socket probe, NOT the launcher's exit status. A late-arriving daemon
   flips the loop to success.
3. Only report failure when, after the retry budget, the socket does not
   answer — and then include the two facts the user needs: the socket path
   probed and where the daemon log lives.
4. Final summary line states which case occurred: started / already
   running / failed — never "failed" while the socket answers.

**Out of scope:** daemon-side changes (progress, status — that is
DAEMON-VISIBILITY-1); Linux installer parity beyond keeping shared template
code consistent; the updater; `rmap doctor` (separate slice).

## 3. Stop conditions

- If the redirect-based resolution cannot yield the tag reliably (GitHub
  changes redirect shape) → keep API-primary and STOP + DECISION_REQUIRED
  with the evidence, rather than shipping a flaky primary.
- Do NOT weaken the existing timeout discipline on any curl call.
- Do NOT report success without an answering socket.

## 4. Validation (end-of-slice, synchronous; TEST REPORT)

- Installer script syntax + `bash -n`; shellcheck if available in the repo's
  tooling; `./scripts/build-installer.sh` still assembles.
- **Resolution proof:** a test (or documented executed transcript) showing
  (a) redirect-primary resolves the current real latest tag; (b) with the
  redirect path stubbed to fail, the API fallback resolves; (c) with both
  stubbed to fail, the error message contains the `RMAP_VERSION` escape
  hatch. (Function-level stubs in a bash test harness; the repo has
  `scripts/test-smoke-rmap.sh` precedent.)
- **Start-truth proof:** with a daemon already running on the target socket,
  the install path reports "already running" and exits success (executed
  locally against a dev daemon); with no daemon and a start that succeeds
  slowly, the probe loop flips to success.
- `./scripts/dogfood-isolated.sh` green (installer changes must not affect
  the dev flow).

## 5. Definition of done

A fresh install on a rate-limited IP resolves the version via `github.com`
redirect (or authenticated fallback), and the daemon-start step's report
matches machine reality in all three cases (started / already running /
failed), verified by the named proofs.
