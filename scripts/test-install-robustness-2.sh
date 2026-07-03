#!/usr/bin/env bash
# test-install-robustness-2.sh — regression tests for INSTALL-ROBUSTNESS-2.
#
# Proves the two installer-truth fixes in scripts/install.template.sh +
# scripts/lib/macos.sh, using FUNCTION-LEVEL / COMMAND-LEVEL stubs in a bash
# harness (precedent: scripts/test-smoke-rmap.sh):
#
#   A — Version resolution off the API (redirect primary, API fallback):
#       (a) redirect-primary resolves the REAL current latest tag (live network);
#       (b) redirect stubbed to fail  → the api.github.com fallback resolves;
#       (c) BOTH stubbed to fail       → the error names the RMAP_VERSION escape hatch.
#
#   B0 — daemon_socket_answers() is a PURE socket_ping predicate (review-1 crux):
#       reads ONLY the socket_ping probe from `rmap doctor --json`, NOT doctor's exit
#       code. Driven against a FAKE rmap whose aggregate is unhealthy + exit is nonzero:
#       (a) socket_ping true  + doctor exit 1 → answers (rc 0)  — the field bug's inverse;
#       (b) socket_ping false + doctor exit 1 → not answering (rc nonzero);
#       (c) socket_ping true  + doctor exit 0 → answers (rc 0);
#       (d) [review-2 #1] EXPORTED RMAP_TRANSPORT=stdio (as dogfood-isolated sets) → the
#           probe FORCES RMAP_TRANSPORT=socket so socket_ping cannot false-pass via a
#           spawned stdio subprocess; asserts the observed transport was socket + no false pass.
#
#   B — Socket liveness is the daemon-start predicate:
#       (a)  daemon already answering  → "already running" + success, nothing started;
#       (b)  no daemon, slow start     → the probe loop flips to success once it answers;
#       (b2) [macOS] launchctl bootstrap returns NONZERO but the socket later answers
#            → the install does NOT abort on the launcher's exit status; it flips to
#            "started". This exercises the REAL start_launchd_service (launchctl stubbed
#            nonzero), proving the launcher-exit-status trust bug is fixed (error→warn);
#       (b3) [linux] systemctl --user start returns NONZERO but the socket later answers
#            → same invariant for the REAL start_systemd_service (systemctl stubbed nonzero);
#       (c)  socket never answers       → failure naming the socket path + daemon log;
#       (c2) [review-2 #2] rgr resolved a LEGACY socket (≠ canonical default) → the failure
#            names rgr's ACTUAL chosen path (parsed from the JSON), not a bash-reconstructed default.
#       (d)  [review-7] FINAL-WINDOW RACE, verify_daemon_health: socket false for every
#            scheduled probe, answers only AFTER the last delay → the loop re-probes past the
#            final wait and returns SUCCESS (never "failed" while the socket answers). FAILS
#            against the old slept-then-gave-up loop, which never re-probed after its last sleep;
#       (e)  [review-7] socket NEVER answers → verify_daemon_health fails only AFTER a final
#            probe (max_attempts+1 probes, max_attempts sleeps: the loop ENDS on a probe, so
#            there is no trailing dead sleep whose window a late-arriving daemon could slip through).
#
#   B (integration) — an ISOLATED real rmapd (RMAP_SOCKET_PATH/RMAP_STATE_ROOT under
#       /private/tmp, never the operator's daemon) proves daemon_socket_answers()
#       reflects real socket liveness end-to-end: 0 while up, nonzero once stopped.
#       [review-2 #3] REQUIRED, not optional: if release rmap/rmapd are absent this is a
#       LOUD, COUNTED failure (never a silent skip that green-washes an un-run proof).
#
#   C — main()'s FINAL SUMMARY states the daemon outcome and exits correctly (review-1 #4):
#       drives the REAL main() (heavy steps stubbed) for each DAEMON_OUTCOME —
#       (a) failed → "Service: daemon failed" + banner qualified + exit 1 (reached the
#           summary via `setup_daemon_service || true`, signal preserved);
#       (b) already running → "Service: daemon already running" + exit 0;
#       (c) started → "Service: daemon started" + exit 0.
#
# The functions under test are sourced from install.template.sh (with its trailing
# `main "$@"` stripped) + lib/macos.sh; lib/linux.sh is sourced INSIDE the B(b3)
# subshell only (so its is_daemon_running/get_daemon_pid do not shadow the macOS
# scenarios). Each scenario runs in a SUBSHELL under production `set -euo pipefail`,
# so the fixes are exercised under the real shell options while a failure never
# aborts the harness. B(b2)/B(b3) are the launcher-failure regression the reviewer
# required: they FAIL if start_{launchd,systemd}_service still `error`s (exit 1) on a
# nonzero launcher instead of deferring to the socket probe.
#
# Usage:   ./scripts/test-install-robustness-2.sh
# Exit:    0 — all tests passed;  1 — one or more failed.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="${REPO_ROOT}/scripts/install.template.sh"
MACOS_LIB="${REPO_ROOT}/scripts/lib/macos.sh"
LINUX_LIB="${REPO_ROOT}/scripts/lib/linux.sh"   # sourced only inside the B(b3) subshell

# Release binaries for the isolated integration probe (optional; skipped if absent).
RMAP_BIN="${RMAP_BIN:-${REPO_ROOT}/rust/target/release/rmap}"

if [[ -t 1 ]]; then RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; NC=$'\033[0m'; else RED=''; GREEN=''; NC=''; fi

TESTS_RUN=0; TESTS_PASSED=0; TESTS_FAILED=0
pass() { echo "${GREEN}PASS${NC}: $1"; TESTS_PASSED=$((TESTS_PASSED + 1)); }
fail() { echo "${RED}FAIL${NC}: $1"; [[ -n "${2:-}" ]] && echo "      $2"; TESTS_FAILED=$((TESTS_FAILED + 1)); }
check() { TESTS_RUN=$((TESTS_RUN + 1)); }
# Setup-level abort (review-4 #2). DISTINCT from fail(): fail() records a counted test
# failure and lets the run continue; fatal() means the harness setup itself is broken —
# no workspace, no functions under test — so every downstream result would be a noisy
# false-partial. Under `set -uo pipefail` (no `-e` — the scenario subshells need
# non-fatal failures) an un-guarded setup command would NOT abort; fatal() aborts loudly
# with a specific reason instead. Used only for the workspace/source setup below.
fatal() { echo "${RED}FATAL${NC}: $*" >&2; exit 1; }

# Create the temp workspace. review-4 #2: under `set -uo pipefail` (no `-e`), a failed
# mktemp would leave WORK empty and the harness would limp on — STRIPPED="/install.funcs.sh"
# (unwritable), source of a nonexistent file, every scenario failing against undefined
# functions — the "noisy false-partial results" the reviewer flagged (their read-only
# sandbox denied mktemp, which is exactly how this surfaced). Fail-fast instead.
WORK="$(mktemp -d "${TMPDIR:-/tmp}/inst-robust2.XXXXXX")" \
    || fatal "could not create a temp workspace via 'mktemp -d' under TMPDIR='${TMPDIR:-/tmp}' — need a writable temp dir"
[[ -n "${WORK}" && -d "${WORK}" ]] \
    || fatal "temp workspace was not created (WORK='${WORK}')"
cleanup() {
    [[ -n "${TEST_DAEMON_PID:-}" ]] && kill "${TEST_DAEMON_PID}" 2>/dev/null || true
    rm -rf "${WORK}" 2>/dev/null || true
    rm -rf "${TEST_STATE_ROOT:-/nonexistent-xyz}" 2>/dev/null || true
}
trap cleanup EXIT

# ── Build a sourceable copy of the template (drop the trailing `main "$@"`) ──────
# review-4 #2, "or sourced" half: each setup step below is guarded with a truthful,
# cause-specific fatal() so a broken workspace/template aborts loudly here instead of
# producing false-partial scenario results. Messages name the ACTUAL cause (missing
# template vs unwritable workspace vs unsourceable file) rather than a generic error.
[[ -f "${TEMPLATE}" ]] || fatal "installer template not found at ${TEMPLATE}"
STRIPPED="${WORK}/install.funcs.sh"
sed '/^main "\$@"$/d' "${TEMPLATE}" > "${STRIPPED}" \
    || fatal "could not write the stripped template to ${STRIPPED} (temp workspace unwritable?)"

# Sourcing turns on the template's `set -euo pipefail`; relax it for orchestration.
# The scenario subshells below re-assert `set -euo pipefail` to test under prod opts.
INSTALL_DIR="${WORK}/bin"          # referenced by daemon_socket_answers (unused when stubbed)
mkdir -p "${INSTALL_DIR}"
# shellcheck disable=SC1090
source "${STRIPPED}" || fatal "could not source the stripped template ${STRIPPED}"
# shellcheck disable=SC1090
source "${MACOS_LIB}" || fatal "could not source ${MACOS_LIB}"
set +e +u +o pipefail

echo "=== INSTALL-ROBUSTNESS-2 regression tests ==="
echo ""

# ════════════════════════════════════════════════════════════════════════════
# A — Version resolution
# ════════════════════════════════════════════════════════════════════════════
echo "── A. Version resolution (redirect primary, API fallback) ──"

# (a) redirect-primary resolves the REAL current latest tag (live github.com).
echo "A(a): redirect primary resolves the real current latest tag (network)"
check
GROUND_TRUTH_URL="$(curl -fsSI -o /dev/null -w '%{redirect_url}' --connect-timeout 10 --max-time 30 \
    "https://github.com/andreirx/repo-graph/releases/latest" 2>/dev/null || true)"
GROUND_TRUTH_TAG="${GROUND_TRUTH_URL##*/releases/tag/}"; GROUND_TRUTH_TAG="${GROUND_TRUTH_TAG%%[/?#]*}"
out_a="$(
    set -euo pipefail
    VERSION=latest; RESOLVED_TAG=""
    resolve_version
    printf 'RESULT_VERSION=%s\n' "${VERSION}"
)" 2>"${WORK}/a.err"
rc_a=$?
got_a="$(printf '%s\n' "${out_a}" | sed -n 's/^RESULT_VERSION=//p')"
if [[ ${rc_a} -eq 0 && "${got_a}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "v${got_a}" == "${GROUND_TRUTH_TAG}" ]]; then
    pass "A(a): redirect resolved ${got_a} (== live latest ${GROUND_TRUTH_TAG}, ≥ v0.4.0 required)"
else
    fail "A(a): redirect resolution" "rc=${rc_a} got='${got_a}' ground-truth='${GROUND_TRUTH_TAG}'; stderr: $(cat "${WORK}/a.err")"
fi

# (b) redirect stubbed to fail → api.github.com fallback resolves (offline, canned).
echo "A(b): redirect fails → API fallback resolves"
check
out_b="$(
    set -euo pipefail
    # Command-level curl stub: fail the github.com redirect, answer the API with a
    # canned tag. Branch on whether the URL is the api.github.com host. (Uses [[ ]]
    # not `case`: a case pattern's `)` confuses bash's $( ) substitution parser.)
    curl() {
        local a
        for a in "$@"; do
            if [[ "$a" == *api.github.com* ]]; then
                printf '%s' '{"tag_name": "v9.9.9"}'
                return 0
            fi
        done
        return 22   # redirect (github.com) probe fails
    }
    VERSION=latest; RESOLVED_TAG=""
    resolve_version
    printf 'RESULT_VERSION=%s\n' "${VERSION}"
)" 2>"${WORK}/b.err"
rc_b=$?
got_b="$(printf '%s\n' "${out_b}" | sed -n 's/^RESULT_VERSION=//p')"
if [[ ${rc_b} -eq 0 && "${got_b}" == "9.9.9" ]]; then
    pass "A(b): API fallback resolved ${got_b} after the redirect failed"
else
    fail "A(b): API fallback" "rc=${rc_b} got='${got_b}'; stderr: $(cat "${WORK}/b.err")"
fi

# (c) BOTH stubbed to fail → error message names the RMAP_VERSION escape hatch.
echo "A(c): both fail → error names RMAP_VERSION escape hatch"
check
(
    set -euo pipefail
    curl() { return 22; }   # both redirect and API fail
    VERSION=latest; RESOLVED_TAG=""
    resolve_version
) >"${WORK}/c.out" 2>"${WORK}/c.err"
rc_c=$?
if [[ ${rc_c} -ne 0 ]] && grep -q "RMAP_VERSION" "${WORK}/c.err"; then
    pass "A(c): exited ${rc_c} and the error names RMAP_VERSION (escape hatch preserved)"
else
    fail "A(c): escape-hatch error" "rc=${rc_c}; stderr: $(cat "${WORK}/c.err")"
fi

# ════════════════════════════════════════════════════════════════════════════
# B0 — daemon_socket_answers() is a PURE socket_ping predicate, NOT doctor's exit
# ════════════════════════════════════════════════════════════════════════════
# review-1 crux: the predicate MUST return success iff the socket answers, INDEPENDENT
# of the binary/dir/launchd-service/plist probes. We drive the REAL daemon_socket_answers
# against a FAKE `rmap` whose `doctor --json` AGGREGATE is unhealthy (plist probe fails)
# and which EXITS NONZERO — varying only the socket_ping probe. If the predicate keyed on
# doctor's exit code (the old bug) every case would read "not answering"; keying on
# socket_ping, only the socket_ping verdict decides. The fake reads FAKE_PING/FAKE_RC at
# runtime (no nested-heredoc expansion), so one fake serves every case.
echo ""
echo "── B0. daemon_socket_answers reads socket_ping, not doctor's exit code ──"

make_fake_rmap() {
    local dir="$1"
    mkdir -p "${dir}"
    cat > "${dir}/rmap" <<'FAKE'
#!/bin/bash
# Fake rmap: emit a doctor --json body whose AGGREGATE is unhealthy (plist fails) with
# socket_ping = ${FAKE_PING}, then exit ${FAKE_RC}. Mirrors serde pretty-print field
# order (name before passed) so the awk in daemon_socket_answers matches as in prod.
if [[ "$1" == "doctor" && "$2" == "--json" ]]; then
  printf '%s\n' \
    '{' \
    '  "platform": "macos",' \
    '  "probes": [' \
    '    { "name": "rmap", "passed": true, "message": "found" },' \
    '    { "name": "plist", "passed": false, "message": "not installed" },' \
    '    {' \
    '      "name": "socket_ping",' \
    '      "passed": '"${FAKE_PING:-true}"',' \
    '      "message": "pong received"' \
    '    }' \
    '  ],' \
    '  "summary": { "total": 3, "passed": 1, "failed": 2, "healthy": false }' \
    '}'
  exit "${FAKE_RC:-1}"
fi
exit 0
FAKE
    chmod +x "${dir}/rmap"
}

FAKE_BIN="${WORK}/fake-bin"
make_fake_rmap "${FAKE_BIN}"

# B0(a): socket_ping=true while the aggregate is UNHEALTHY and doctor exits 1 → predicate 0.
echo "B0(a): socket_ping true + doctor exit 1 (unhealthy aggregate) → answers (rc 0)"
check
(
    set -euo pipefail
    export FAKE_PING=true FAKE_RC=1 INSTALL_DIR="${FAKE_BIN}"
    daemon_socket_answers
)
rc_j0a=$?
if [[ ${rc_j0a} -eq 0 ]]; then
    pass "B0(a): socket answered despite unhealthy aggregate + nonzero doctor exit (predicate ignores exit code)"
else
    fail "B0(a): pure-socket_ping predicate" "daemon_socket_answers returned ${rc_j0a} (want 0)"
fi

# B0(b): socket_ping=false, doctor also exits 1 → predicate 1 (socket genuinely not answering).
echo "B0(b): socket_ping false + doctor exit 1 → not answering (rc nonzero)"
check
(
    set -euo pipefail
    export FAKE_PING=false FAKE_RC=1 INSTALL_DIR="${FAKE_BIN}"
    daemon_socket_answers
)
rc_j0b=$?
if [[ ${rc_j0b} -ne 0 ]]; then
    pass "B0(b): socket_ping false → predicate reported not-answering (rc ${rc_j0b})"
else
    fail "B0(b): pure-socket_ping predicate" "daemon_socket_answers returned 0 (want nonzero)"
fi

# B0(c): socket_ping=true and doctor exits 0 (healthy) → predicate 0 (sanity: same verdict).
echo "B0(c): socket_ping true + doctor exit 0 (healthy) → answers (rc 0)"
check
(
    set -euo pipefail
    export FAKE_PING=true FAKE_RC=0 INSTALL_DIR="${FAKE_BIN}"
    daemon_socket_answers
)
rc_j0c=$?
if [[ ${rc_j0c} -eq 0 ]]; then
    pass "B0(c): socket_ping true → answers regardless of exit code (rc 0)"
else
    fail "B0(c): pure-socket_ping predicate" "daemon_socket_answers returned ${rc_j0c} (want 0)"
fi

# B0(d): review-2 #1 — the probe FORCES RMAP_TRANSPORT=socket, so a polluted stdio
# environment cannot make socket_ping pass via a spawned rmapd SUBPROCESS while the socket
# daemon is down. The fake models "socket daemon DOWN; only a stdio subprocess would answer":
# socket_ping=true ONLY when invoked over stdio (the false-pass the slice forbids), false
# over socket (the truth). It also records the RMAP_TRANSPORT it observed. Under an EXPORTED
# RMAP_TRANSPORT=stdio (exactly what scripts/dogfood-isolated.sh sets), daemon_socket_answers
# must (1) invoke rmap with RMAP_TRANSPORT=socket and (2) NOT report a false pass. Remove the
# force and BOTH assertions flip (observed=stdio, predicate returns 0) → this test FAILS.
echo ""
echo "── B0(d). polluted RMAP_TRANSPORT=stdio → probe forces socket, no subprocess false-pass ──"
make_transport_probe_rmap() {
    local dir="$1"
    mkdir -p "${dir}"
    cat > "${dir}/rmap" <<'FAKE'
#!/bin/bash
if [[ "$1" == "doctor" && "$2" == "--json" ]]; then
  printf '%s' "${RMAP_TRANSPORT:-<unset>}" > "${OBSERVED_TRANSPORT_FILE}"
  # Model: socket DOWN; only a stdio subprocess would answer → ping=true over stdio (false
  # pass), ping=false over socket (truth).
  if [[ "${RMAP_TRANSPORT:-}" == "socket" ]]; then ping=false; else ping=true; fi
  printf '%s\n' \
    '{' \
    '  "probes": [' \
    '    { "name": "socket_ping", "passed": '"${ping}"', "message": "modelled" }' \
    '  ]' \
    '}'
  exit 1
fi
exit 0
FAKE
    chmod +x "${dir}/rmap"
}
echo "B0(d): exported RMAP_TRANSPORT=stdio → forced socket, predicate not fooled"
check
FAKE_TP_BIN="${WORK}/fake-transport"
make_transport_probe_rmap "${FAKE_TP_BIN}"
(
    set -euo pipefail
    export RMAP_TRANSPORT=stdio                                # polluted env (dogfood-isolated exports this)
    export OBSERVED_TRANSPORT_FILE="${WORK}/obs-transport.txt"
    export INSTALL_DIR="${FAKE_TP_BIN}"
    daemon_socket_answers
)
rc_j0d=$?
observed_tp="$(cat "${WORK}/obs-transport.txt" 2>/dev/null || echo MISSING)"
if [[ ${rc_j0d} -ne 0 && "${observed_tp}" == "socket" ]]; then
    pass "B0(d): probe ran rmap with RMAP_TRANSPORT=socket (not ambient stdio) → no subprocess false-pass (rc ${rc_j0d})"
else
    fail "B0(d): transport-pollution force" "rc=${rc_j0d} (want nonzero), observed RMAP_TRANSPORT='${observed_tp}' (want socket)"
fi

# ════════════════════════════════════════════════════════════════════════════
# B — Socket-liveness daemon-start predicate (stubbed)
# ════════════════════════════════════════════════════════════════════════════
echo ""
echo "── B. Daemon start = socket liveness (setup_macos_daemon_service) ──"

# (a) daemon already answering → "already running", success, nothing started.
echo "B(a): daemon already answering → already running, nothing started"
check
out_ba="$(
    set -euo pipefail
    MACOS_LOG_DIR="${WORK}/logs-ba"
    daemon_socket_answers() { return 0; }                       # socket answers
    daemon_pid_best_effort() { echo 4242; }
    install_launchd_plist()  { echo started >> "${WORK}/ba.started"; }
    start_launchd_service()  { echo started >> "${WORK}/ba.started"; }
    setup_macos_daemon_service && rc=0 || rc=$?
    printf 'OUTCOME=%s RC=%s\n' "${DAEMON_OUTCOME}" "${rc}"
)" 2>"${WORK}/ba.err"
if [[ "${out_ba}" == *"OUTCOME=already running RC=0"* ]] && [[ ! -f "${WORK}/ba.started" ]]; then
    pass "B(a): reported 'already running', exit 0, and NOTHING was started"
else
    fail "B(a): already-running path" "out='${out_ba}'; started-marker present=$([[ -f "${WORK}/ba.started" ]] && echo yes || echo no)"
fi

# (b) no daemon, slow start → probe loop flips to success once the socket answers.
echo "B(b): no daemon + slow start → probe loop flips to success"
check
out_bb="$(
    set -euo pipefail
    MACOS_LOG_DIR="${WORK}/logs-bb"
    # Socket answers only from the 4th probe onward (pre-check + 2 retries fail first).
    printf '0' > "${WORK}/bb.count"
    daemon_socket_answers() {
        local n; n="$(cat "${WORK}/bb.count")"; n=$((n + 1)); printf '%s' "${n}" > "${WORK}/bb.count"
        [[ ${n} -ge 4 ]]
    }
    daemon_pid_best_effort() { echo 5150; }
    install_launchd_plist()  { echo started >> "${WORK}/bb.started"; }
    start_launchd_service()  { echo started >> "${WORK}/bb.started"; }
    sleep() { :; }                                              # no real waiting
    setup_macos_daemon_service && rc=0 || rc=$?
    printf 'OUTCOME=%s RC=%s\n' "${DAEMON_OUTCOME}" "${rc}"
)" 2>"${WORK}/bb.err"
if [[ "${out_bb}" == *"OUTCOME=started RC=0"* ]] && [[ -f "${WORK}/bb.started" ]]; then
    pass "B(b): late-arriving daemon flipped the loop to 'started' (start WAS invoked)"
else
    fail "B(b): slow-start flip" "out='${out_bb}'; started-marker present=$([[ -f "${WORK}/bb.started" ]] && echo yes || echo no)"
fi

# (b2) [macOS] launcher (launchctl bootstrap) returns NONZERO, socket answers late.
# This is the reviewer-required regression: the REAL start_launchd_service runs with
# launchctl stubbed to fail, so it must NOT `error`/exit — the socket probe decides.
# With the fix it flips to "started"; without it (error→exit 1) the subshell dies
# before printing OUTCOME and this test FAILS.
echo "B(b2): [macOS] launchctl bootstrap NONZERO + late socket → started (no abort)"
check
out_bb2="$(
    set -euo pipefail
    MACOS_LOG_DIR="${WORK}/logs-bb2"
    MACOS_LAUNCHAGENTS_DIR="${WORK}/la-bb2"     # isolated temp; never the real ~/Library/LaunchAgents
    install_launchd_plist() { :; }              # not under test; skip the plist write
    launchctl() { return 1; }                   # bootout + bootstrap both nonzero → exercises error→warn
    printf '0' > "${WORK}/bb2.count"
    daemon_socket_answers() {                   # answers from the 3rd probe: pre-check + 1 retry fail first
        local n; n="$(cat "${WORK}/bb2.count")"; n=$((n + 1)); printf '%s' "${n}" > "${WORK}/bb2.count"
        [[ ${n} -ge 3 ]]
    }
    daemon_pid_best_effort() { echo 9001; }
    sleep() { :; }
    setup_macos_daemon_service && rc=0 || rc=$? # REAL start_launchd_service runs inside
    printf 'OUTCOME=%s RC=%s\n' "${DAEMON_OUTCOME}" "${rc}"
)" 2>"${WORK}/bb2.err"
if [[ "${out_bb2}" == *"OUTCOME=started RC=0"* ]]; then
    pass "B(b2): nonzero launchctl did NOT abort; late socket answer → 'started', exit 0"
else
    fail "B(b2): launcher-failure tolerance (macOS)" "out='${out_bb2}'; stderr: $(cat "${WORK}/bb2.err")"
fi

# (b3) [linux] launcher (systemctl --user start) returns NONZERO, socket answers late.
# Same invariant for the REAL start_systemd_service. lib/linux.sh is sourced in THIS
# subshell only (its is_daemon_running/get_daemon_pid stay out of the macOS scenarios);
# it reuses the template's daemon_socket_answers/verify_daemon_health from the top-level
# source. FAILS if start_systemd_service still `error`s on a nonzero start.
echo "B(b3): [linux] systemctl start NONZERO + late socket → started (no abort)"
check
out_bb3="$(
    set -euo pipefail
    # shellcheck disable=SC1090
    source "${LINUX_LIB}"
    LINUX_LOG_DIR="${WORK}/logs-bb3"            # isolated temp; never the real ~/.local/share/rmap/logs
    detect_systemd()       { LINUX_SERVICE_MODE=systemd; }   # force systemd path; no real systemctl probe
    install_systemd_unit() { :; }                            # not under test; skip the unit write
    systemctl()            { return 1; }                     # launcher start returns nonzero → exercises error→warn
    printf '0' > "${WORK}/bb3.count"
    daemon_socket_answers() {
        local n; n="$(cat "${WORK}/bb3.count")"; n=$((n + 1)); printf '%s' "${n}" > "${WORK}/bb3.count"
        [[ ${n} -ge 3 ]]
    }
    daemon_pid_best_effort() { echo 7007; }
    sleep() { :; }
    setup_linux_daemon_service && rc=0 || rc=$?  # REAL start_systemd_service runs inside
    printf 'OUTCOME=%s RC=%s\n' "${DAEMON_OUTCOME}" "${rc}"
)" 2>"${WORK}/bb3.err"
if [[ "${out_bb3}" == *"OUTCOME=started RC=0"* ]]; then
    pass "B(b3): nonzero systemctl start did NOT abort; late socket answer → 'started', exit 0"
else
    fail "B(b3): launcher-failure tolerance (linux)" "out='${out_bb3}'; stderr: $(cat "${WORK}/bb3.err")"
fi

# (c) socket never answers → failure naming the socket path + daemon log location.
echo "B(c): socket never answers → honest failure naming socket + log"
check
(
    set -euo pipefail
    MACOS_LOG_DIR="${WORK}/logs-bc"
    daemon_socket_answers() { return 1; }                       # never answers
    daemon_pid_best_effort() { echo ""; }
    install_launchd_plist()  { :; }
    start_launchd_service()  { :; }
    sleep() { :; }
    setup_macos_daemon_service && rc=0 || rc=$?
    printf 'OUTCOME=%s RC=%s\n' "${DAEMON_OUTCOME}" "${rc}"
) >"${WORK}/bc.out" 2>"${WORK}/bc.err"
out_bc="$(cat "${WORK}/bc.out")"
if [[ "${out_bc}" == *"OUTCOME=failed RC=1"* ]] \
   && grep -q "Socket probed:.*daemon.sock" "${WORK}/bc.err" \
   && grep -q "Daemon log:.*daemon.log" "${WORK}/bc.err"; then
    pass "B(c): reported 'failed' (exit 1) and named the socket path + daemon log"
else
    fail "B(c): honest-failure message" "out='${out_bc}'; stderr: $(cat "${WORK}/bc.err")"
fi

# (c2) review-2 #2 — the failure names the ACTUAL probed socket path, not a bash-reconstructed
# default. rgr's resolver can choose the LEGACY socket over canonical in the migration case
# (canonical unreachable, legacy reachable-but-ping-fails), so ${RMAP_SOCKET_PATH:-<canonical>}
# would name the WRONG path. A fake `rmap doctor --json` reports socket_path=<legacy sentinel>
# (distinct from the canonical MACOS_SOCKET_PATH) and socket_ping=false (never answers). With
# NO RMAP_SOCKET_PATH override, the failure message must name the LEGACY path resolved_socket_path
# parsed from the JSON — never the canonical default. FAILS on the old bash-reconstruction.
echo "B(c2): probed-path fidelity → failure names rgr's chosen path, not the canonical default"
check
make_pathreport_rmap() {
    local dir="$1"
    mkdir -p "${dir}"
    cat > "${dir}/rmap" <<'FAKE'
#!/bin/bash
# Fake rmap: socket_path=${FAKE_SOCKET_PATH} (rgr's chosen path) + socket_ping=false. Mirrors
# serde field order (name → passed → message) so resolved_socket_path's awk matches as in prod.
if [[ "$1" == "doctor" && "$2" == "--json" ]]; then
  printf '%s\n' \
    '{' \
    '  "platform": "macos",' \
    '  "probes": [' \
    '    {' \
    '      "name": "socket_path",' \
    '      "passed": false,' \
    '      "message": "'"${FAKE_SOCKET_PATH}"' (not found)"' \
    '    },' \
    '    {' \
    '      "name": "socket_ping",' \
    '      "passed": false,' \
    '      "message": "skipped (socket missing)"' \
    '    }' \
    '  ]' \
    '}'
  exit 1
fi
exit 0
FAKE
    chmod +x "${dir}/rmap"
}
FAKE_PR_BIN="${WORK}/fake-pathreport"
make_pathreport_rmap "${FAKE_PR_BIN}"
LEGACY_SENTINEL="/legacy-sentinel-xyz/daemon.sock"     # the path rgr actually resolved/probed
CANON_SENTINEL="/canonical-sentinel-xyz/daemon.sock"   # the canonical default (what old bash would print)
(
    set -euo pipefail
    unset RMAP_SOCKET_PATH 2>/dev/null || true            # no override → old code would print canonical
    export FAKE_SOCKET_PATH="${LEGACY_SENTINEL}"
    export INSTALL_DIR="${FAKE_PR_BIN}"
    MACOS_SOCKET_PATH="${CANON_SENTINEL}"                 # canonical default, distinct from the probed legacy path
    MACOS_LOG_DIR="${WORK}/logs-bc2"
    MACOS_LAUNCHAGENTS_DIR="${WORK}/la-bc2"               # isolated temp; never the real ~/Library/LaunchAgents
    install_launchd_plist() { :; }
    start_launchd_service()  { :; }
    sleep() { :; }
    setup_macos_daemon_service && rc=0 || rc=$?
    printf 'OUTCOME=%s RC=%s\n' "${DAEMON_OUTCOME}" "${rc}"
) >"${WORK}/bc2.out" 2>"${WORK}/bc2.err"
if grep -q "Socket probed:.*${LEGACY_SENTINEL}" "${WORK}/bc2.err" \
   && ! grep -q "${CANON_SENTINEL}" "${WORK}/bc2.err"; then
    pass "B(c2): failure named the ACTUAL probed path (${LEGACY_SENTINEL}), not the canonical default"
else
    fail "B(c2): probed-path fidelity" "stderr: $(cat "${WORK}/bc2.err")"
fi

# ════════════════════════════════════════════════════════════════════════════
# B(d)/B(e) — verify_daemon_health FINAL-WINDOW RACE (review-7)
# ════════════════════════════════════════════════════════════════════════════
# review-7 blocker: the old loop probed, then slept AFTER its last probe and returned
# failure with NO probe after that sleep — so a daemon that came up during the final
# ~delay-second window was misreported "failed" while the socket was answering. The fix
# makes the loop END on a probe: probe up front, then max_attempts waits each followed by a
# RE-PROBE (including a probe AFTER the last wait). These drive the REAL verify_daemon_health
# directly with a counting daemon_socket_answers stub + a counting sleep stub (no real
# waiting), so the probe/sleep SHAPE is observable. Both FAIL against the old slept-then-gave-
# up loop (which did max_attempts probes + max_attempts sleeps and never re-probed past the
# last sleep). max_attempts=3 keeps them fast.
echo ""
echo "── B(d)/B(e). verify_daemon_health re-probes after the final delay (no final-window race) ──"

# (d) THE CATCH: socket stays DOWN for every scheduled probe and only answers on the probe
# that FOLLOWS the last delay. With max_attempts=3 that is probe #4 (1 up-front + 3 retries).
# The fixed loop re-probes there and returns SUCCESS; the old loop stopped after probe #3
# (then a dead sleep) and returned failure → this is the regression that pins review-7.
echo "B(d): socket answers only AFTER the last delay → success, NOT 'failed'"
check
(
    set -euo pipefail
    printf '0' > "${WORK}/bd.probes"
    printf '0' > "${WORK}/bd.sleeps"
    daemon_socket_answers() {                         # DOWN for probes 1-3, UP from probe 4 (post-final-delay)
        local n; n="$(cat "${WORK}/bd.probes")"; n=$((n + 1)); printf '%s' "${n}" > "${WORK}/bd.probes"
        [[ ${n} -ge 4 ]]
    }
    sleep() {                                         # count waits; never actually sleep
        local s; s="$(cat "${WORK}/bd.sleeps")"; s=$((s + 1)); printf '%s' "${s}" > "${WORK}/bd.sleeps"
    }
    verify_daemon_health 3 1
) >"${WORK}/bd.out" 2>&1
rc_bd=$?
probes_bd="$(cat "${WORK}/bd.probes" 2>/dev/null || echo 0)"
sleeps_bd="$(cat "${WORK}/bd.sleeps" 2>/dev/null || echo 0)"
if [[ ${rc_bd} -eq 0 && ${probes_bd} -eq 4 && ${sleeps_bd} -eq 3 ]]; then
    pass "B(d): re-probed after the final delay (4 probes, 3 sleeps) → success, never 'failed' while the socket answers"
else
    fail "B(d): final-window race" "rc=${rc_bd} (want 0), probes=${probes_bd} (want 4), sleeps=${sleeps_bd} (want 3); out: $(cat "${WORK}/bd.out")"
fi

# (e) THE HONEST FAILURE SHAPE: socket NEVER answers → the loop still fails, but only AFTER a
# final probe. max_attempts+1 probes and max_attempts sleeps prove the loop ENDS on a probe
# (probe #N+1 runs AFTER the last sleep) — so failure is concluded only after a fresh
# liveness check and there is NO trailing dead sleep. Old loop: 3 probes + 3 sleeps → FAILS.
echo "B(e): socket never answers → 'failed' only after a final probe (no trailing dead sleep)"
check
(
    set -euo pipefail
    printf '0' > "${WORK}/be.probes"
    printf '0' > "${WORK}/be.sleeps"
    daemon_socket_answers() {                         # never answers
        local n; n="$(cat "${WORK}/be.probes")"; n=$((n + 1)); printf '%s' "${n}" > "${WORK}/be.probes"
        return 1
    }
    sleep() {
        local s; s="$(cat "${WORK}/be.sleeps")"; s=$((s + 1)); printf '%s' "${s}" > "${WORK}/be.sleeps"
    }
    verify_daemon_health 3 1
) >"${WORK}/be.out" 2>&1
rc_be=$?
probes_be="$(cat "${WORK}/be.probes" 2>/dev/null || echo 0)"
sleeps_be="$(cat "${WORK}/be.sleeps" 2>/dev/null || echo 0)"
if [[ ${rc_be} -ne 0 && ${probes_be} -eq 4 && ${sleeps_be} -eq 3 ]]; then
    pass "B(e): failed only after the final probe (4 probes, 3 sleeps; the loop's last action is a probe)"
else
    fail "B(e): no trailing dead sleep" "rc=${rc_be} (want nonzero), probes=${probes_be} (want 4), sleeps=${sleeps_be} (want 3); out: $(cat "${WORK}/be.out")"
fi

# ════════════════════════════════════════════════════════════════════════════
# B (integration) — ISOLATED real rmapd proves daemon_socket_answers() is truthful
# ════════════════════════════════════════════════════════════════════════════
echo ""
echo "── B (integration): daemon_socket_answers() vs a REAL isolated daemon ──"
check
if [[ ! -x "${RMAP_BIN}" ]] || [[ ! -x "$(dirname "${RMAP_BIN}")/rmapd" ]]; then
    # review-2 #3: the socket-truth integration proof is REQUIRED, not optional. A missing
    # binary must NOT silently un-count itself into a green run — it is a LOUD, COUNTED
    # failure that names how to satisfy it, so `all passed` can never hide an un-run required
    # proof. (Pure syntax iteration uses `bash -n` directly; it does not need this harness green.)
    fail "B(integration): NOT RUN — release rmap/rmapd are REQUIRED for the mandatory socket-truth proof" \
         "build them: (cd rust && cargo build --release --bin rmap --bin rmapd), or point RMAP_BIN=<path/to/rmap> at an existing pair"
else
    BIN_DIR="$(cd "$(dirname "${RMAP_BIN}")" && pwd)"
    TEST_STATE_ROOT="/private/tmp/inst-robust2-daemon-$$"
    TEST_SOCKET="${TEST_STATE_ROOT}/daemon.sock"
    mkdir -p "${TEST_STATE_ROOT}"
    # Start an ISOLATED daemon: RMAP_SOCKET_PATH + RMAP_STATE_ROOT under /private/tmp.
    # This NEVER binds the operator's canonical socket and writes only under the tmp
    # state root (SandboxLocal). We kill ONLY our own PID (never `pkill rmapd`).
    RMAP_SOCKET_PATH="${TEST_SOCKET}" RMAP_STATE_ROOT="${TEST_STATE_ROOT}" \
        "${BIN_DIR}/rmapd" >"${WORK}/daemon.log" 2>&1 &
    TEST_DAEMON_PID=$!
    for _ in $(seq 1 40); do [[ -S "${TEST_SOCKET}" ]] && break; sleep 0.5; done

    probe_up=1; probe_down=1
    if [[ -S "${TEST_SOCKET}" ]]; then
        # Probe UP: daemon_socket_answers() must succeed against the isolated socket.
        (
            set -euo pipefail
            export RMAP_SOCKET_PATH="${TEST_SOCKET}" RMAP_STATE_ROOT="${TEST_STATE_ROOT}"
            INSTALL_DIR="${BIN_DIR}"
            daemon_socket_answers
        ) >/dev/null 2>&1 && probe_up=0 || probe_up=$?

        # Stop the daemon, remove the socket, and re-probe: must now report DOWN.
        kill "${TEST_DAEMON_PID}" 2>/dev/null || true
        wait "${TEST_DAEMON_PID}" 2>/dev/null || true
        TEST_DAEMON_PID=""
        rm -f "${TEST_SOCKET}"
        (
            export RMAP_SOCKET_PATH="${TEST_SOCKET}" RMAP_STATE_ROOT="${TEST_STATE_ROOT}"
            INSTALL_DIR="${BIN_DIR}"
            daemon_socket_answers
        ) >/dev/null 2>&1 && probe_down=0 || probe_down=$?

        if [[ ${probe_up} -eq 0 && ${probe_down} -ne 0 ]]; then
            pass "B(integration): answered 0 while up, nonzero (${probe_down}) once stopped — socket-truthful"
        else
            fail "B(integration): socket truth" "probe_up=${probe_up} (want 0), probe_down=${probe_down} (want nonzero); daemon log: $(tail -5 "${WORK}/daemon.log" 2>/dev/null)"
        fi
    else
        fail "B(integration): isolated daemon setup" "socket never appeared at ${TEST_SOCKET}; daemon log: $(tail -20 "${WORK}/daemon.log" 2>/dev/null)"
    fi
fi

# ════════════════════════════════════════════════════════════════════════════
# C — main() renders the daemon-start verdict in the final summary + exit code
# ════════════════════════════════════════════════════════════════════════════
# review-1 #4: the FINAL SUMMARY must state started / already running / failed, and a
# genuine failure must REACH that summary (not set -e-abort before it) while preserving
# the pre-slice nonzero exit. We drive the REAL main() with every heavy step stubbed to a
# no-op EXCEPT setup_daemon_service, which sets DAEMON_OUTCOME to the case under test (as
# the real platform setup does) and, for the failed case, returns nonzero — so this also
# proves `setup_daemon_service || true` stops set -e from aborting before the summary.
echo ""
echo "── C. main() summary line + exit code per daemon outcome ──"

run_main_with_outcome() {
    # $1 = DAEMON_OUTCOME the stubbed setup will set. Emits main()'s stdout; the caller
    # reads $? for main's exit code. BINARY_ONLY=false so the daemon branch + summary run.
    local outcome="$1"
    (
        set -euo pipefail
        detect_platform()        { PLATFORM=darwin; ARCH=aarch64; }
        source_platform_module() { :; }
        detect_toolchains()      { :; }
        resolve_version()        { VERSION=9.9.9; }
        download_binary()        { :; }
        create_directories()     { :; }
        write_manifest()         { :; }
        detect_hosts()           { :; }
        setup_path()             { :; }
        setup_daemon_service()   {
            DAEMON_OUTCOME="${outcome}"
            [[ "${outcome}" == "failed" ]] && return 1 || return 0
        }
        RMAP_INSTALL_DIR="${WORK}/bin" main --non-interactive
    )
}

# C(a): failed → summary states "daemon failed", banner qualified, main exits 1.
echo "C(a): DAEMON_OUTCOME=failed → summary 'daemon failed' + exit 1"
check
out_ca="$(run_main_with_outcome failed)"; rc_ca=$?
if [[ ${rc_ca} -eq 1 ]] \
   && printf '%s\n' "${out_ca}" | grep -q "Service: daemon failed" \
   && printf '%s\n' "${out_ca}" | grep -q "daemon service NOT running"; then
    pass "C(a): reached the summary, stated 'daemon failed', qualified the banner, exited 1 (signal preserved)"
else
    fail "C(a): failed-summary + exit" "rc=${rc_ca} (want 1); out: ${out_ca}"
fi

# C(b): already running → summary states "daemon already running", exit 0.
echo "C(b): DAEMON_OUTCOME='already running' → summary + exit 0"
check
out_cb="$(run_main_with_outcome "already running")"; rc_cb=$?
if [[ ${rc_cb} -eq 0 ]] && printf '%s\n' "${out_cb}" | grep -q "Service: daemon already running"; then
    pass "C(b): stated 'daemon already running' and exited 0"
else
    fail "C(b): already-running summary + exit" "rc=${rc_cb} (want 0); out: ${out_cb}"
fi

# C(c): started → summary states "daemon started", exit 0.
echo "C(c): DAEMON_OUTCOME=started → summary + exit 0"
check
out_cc="$(run_main_with_outcome started)"; rc_cc=$?
if [[ ${rc_cc} -eq 0 ]] && printf '%s\n' "${out_cc}" | grep -q "Service: daemon started"; then
    pass "C(c): stated 'daemon started' and exited 0"
else
    fail "C(c): started summary + exit" "rc=${rc_cc} (want 0); out: ${out_cc}"
fi

# ════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Results ==="
echo "Tests run: ${TESTS_RUN}   Passed: ${TESTS_PASSED}   Failed: ${TESTS_FAILED}"
[[ ${TESTS_FAILED} -gt 0 ]] && exit 1
exit 0
