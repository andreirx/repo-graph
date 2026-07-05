#!/usr/bin/env bash
#
# dv1-inflight-e2e.sh — DAEMON-VISIBILITY-1 real-transport self-dogfood.
#
# Exercises the visibility contracts against REAL binaries over a REAL Unix socket, using an
# isolated `rmapd` (its own socket + state root under /private/tmp). The operator's resident
# launchd daemon (~/.local/bin/rmapd, ~/Library/Application Support/repo-graph) is NEVER touched:
# RMAP_SOCKET_PATH + RMAP_STATE_ROOT relocate BOTH the daemon it binds and the client it talks to.
#
# Proofs (all on a real in-flight index of a generated large TS fixture — the homegrown extractor is
# JS/TS, so a large TS fixture gives a multi-second index with real structure, deterministically):
#   D0    register   : a first index of repoA to READY registers it (precondition for E: `storage_health`
#                     resolves the repo from the registry, which `record_index` only writes on success).
#   D/E  mid-reindex: while a RE-index of the registered repoA holds the DB, `rmap doctor` shows
#                     "indexing <repo> …" (activity, D) AND the storage line shows "in use by daemon"
#                     (contention truth, E) — NEVER "error opening database".
#   D3   post-index : `rmap doctor` shows idle + the LAST SNAPSHOT (repo @ time) + the enrichment line.
#   C    still-run  : DETERMINISTIC still-running proof. A background "holder" re-index holds repoB's
#                     DB write lock; a short-timeout "prober" index of the SAME repo blocks on that lock
#                     BEFORE any progress frame (total silence), so its 1s read ALWAYS times out while
#                     the op is live → it probes the daemon and reports STILL RUNNING with the distinct
#                     exit status 3. STRICT: exit 0 and exit 2 both FAIL; only exit 3 + "STILL RUNNING"
#                     passes (no race, no exit-0 escape hatch). The pure classifier is also unit-tested
#                     in rgr; the real in-flight status surface is integration-tested in daemon-runtime.
#   ID1  disconnect : INDEX-DISCONNECT-1 real-transport proof. Kill the CLIENT (SIGKILL, NOT the daemon)
#                     mid-index of a FRESH repoD → the daemon finishes the index DETACHED: repoD becomes
#                     registered + READY (orient answers) and the daemon logs a detached-continuation
#                     line — the F5 fix (a dead client never aborts a durable write). The strict
#                     "exactly one detached line per op" is unit-tested in daemon-runtime
#                     (tests/index_disconnect.rs); here we assert the end-to-end socket behaviour.
#   F1/F3 field-repro: kill the daemon mid-reindex (the day-2 restart/sleep scenario) → an interrupted
#                     snapshot survives → after restart `doctor`/`repo info` NAME it, and
#                     `rmap maintenance prune` deletes + reclaims it (READY snapshots untouched).
#
# All D/E/F observations are HARD assertions: a required observation that cannot be made FAILS the run
# (it does not `note`-and-pass). Exit 0 iff every REQUIRED assertion holds AND the operator registry is
# confirmed untouched.
# PLATFORM: macOS/Linux (bash + a POSIX socket). Cleanup is via an EXIT trap.

set -u

# ── Resolve binaries (rmapd MUST be the sibling of rmap) ──────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RMAP_BIN="${RMAP_BIN:-${REPO_ROOT}/rust/target/release/rmap}"
RMAPD_BIN="$(cd "$(dirname "${RMAP_BIN}")" && pwd)/rmapd"

for b in "${RMAP_BIN}" "${RMAPD_BIN}"; do
    if [[ ! -x "${b}" ]]; then
        echo "error: binary not found/executable: ${b}" >&2
        echo "       build first: (cd rust && cargo build --release --bin rmap --bin rmapd)" >&2
        exit 1
    fi
done

# ── Isolated state root + socket (ephemeral, under /private/tmp → SandboxLocal) ─
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
STATE_ROOT="/private/tmp/repo-graph-dv1-e2e/${RUN_ID}"
SOCKET_PATH="${STATE_ROOT}/daemon.sock"
FIXROOT="${STATE_ROOT}/fixtures"
OUT="${STATE_ROOT}/out"
mkdir -p "${STATE_ROOT}" "${FIXROOT}" "${OUT}"

OPERATOR_REGISTRY="${HOME}/Library/Application Support/repo-graph/registry.json"

export RMAP_SOCKET_PATH="${SOCKET_PATH}"
export RMAP_STATE_ROOT="${STATE_ROOT}"
# NOT stdio: we want the real socket transport (progress frames + read timeout are socket features).
unset RMAP_TRANSPORT 2>/dev/null || true

FIXTURE_FILES="${FIXTURE_FILES:-800}"   # ~800 TS files × FNS_PER_FILE → a multi-second index (mid-index + kill window)
FAILURES=0
DAEMON_PID=""

note()  { echo "    $*"; }
pass()  { echo "  PASS: $*"; }
fail()  { echo "  FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }
hr()    { echo "------------------------------------------------------------------"; }

cleanup() {
    if [[ -n "${DAEMON_PID}" ]] && kill -0 "${DAEMON_PID}" 2>/dev/null; then
        kill "${DAEMON_PID}" 2>/dev/null || true
        wait "${DAEMON_PID}" 2>/dev/null || true
    fi
    # Belt-and-suspenders: any rmapd bound to OUR socket (never the operator's).
    pkill -f "rmapd.*${SOCKET_PATH}" 2>/dev/null || true
    if [[ "${KEEP:-false}" == "true" ]]; then
        echo "Retained (KEEP=true): ${STATE_ROOT}"
    else
        rm -rf "${STATE_ROOT}"
    fi
}
trap cleanup EXIT

# ── Daemon lifecycle helpers ──────────────────────────────────────────────────
start_daemon() {
    "${RMAPD_BIN}" >"${OUT}/rmapd.log" 2>&1 &
    DAEMON_PID=$!
    # Wait up to ~20s for the socket to bind AND doctor to answer.
    for _ in $(seq 1 40); do
        if [[ -S "${SOCKET_PATH}" ]] && "${RMAP_BIN}" doctor >/dev/null 2>&1; then
            return 0
        fi
        if ! kill -0 "${DAEMON_PID}" 2>/dev/null; then
            echo "error: rmapd exited during startup; log:" >&2
            cat "${OUT}/rmapd.log" >&2
            return 1
        fi
        sleep 0.5
    done
    echo "error: isolated rmapd did not become ready within 20s" >&2
    return 1
}
stop_daemon() {
    if [[ -n "${DAEMON_PID}" ]] && kill -0 "${DAEMON_PID}" 2>/dev/null; then
        kill "${DAEMON_PID}" 2>/dev/null || true
        wait "${DAEMON_PID}" 2>/dev/null || true
    fi
    DAEMON_PID=""
}
hard_kill_daemon() {  # the field scenario: an abrupt stop mid-index (restart / machine sleep)
    if [[ -n "${DAEMON_PID}" ]]; then
        kill -9 "${DAEMON_PID}" 2>/dev/null || true
        wait "${DAEMON_PID}" 2>/dev/null || true
    fi
    DAEMON_PID=""
    rm -f "${SOCKET_PATH}"   # a -9'd daemon leaves a stale socket file
}

# Generate a fixture of N TS files (each with FNS_PER_FILE cross-referencing functions) via python3,
# so the index has real structure AND takes multiple seconds (enough to observe mid-index, to trip a
# 1s stall timeout during the silent extraction phase, and to kill mid-index). python3 is used for
# speed — a bash printf loop over thousands of files is far too slow.
FNS_PER_FILE="${FNS_PER_FILE:-120}"
gen_fixture() {
    local dir="$1" n="$2"
    mkdir -p "${dir}/src"
    printf '{ "name": "dv1-fixture", "version": "0.0.0", "private": true }\n' > "${dir}/package.json"
    python3 - "${dir}/src" "${n}" "${FNS_PER_FILE}" <<'PY'
import os, sys
src, n, m = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
os.makedirs(src, exist_ok=True)
with open(os.path.join(src, "base.ts"), "w") as f:
    f.write("export function base(n: number): number { return n + 1; }\n")
for i in range(1, n + 1):
    lines = ['import { base } from "./base";\n']
    for j in range(m):
        if j == 0:
            lines.append(f"export function fn{i}_{j}(x: number): number {{ return base(x) * 2; }}\n")
        else:
            lines.append(f"export function fn{i}_{j}(x: number): number {{ return fn{i}_{j-1}(x) + base(x); }}\n")
    with open(os.path.join(src, f"mod{i}.ts"), "w") as f:
        f.writelines(lines)
PY
}

# Poll `rmap doctor` (from a repo cwd) until its activity line reports an in-flight op, up to ~timeout.
wait_until_indexing() {
    local cwd="$1" tries="${2:-40}"
    local i
    for i in $(seq 1 "${tries}"); do
        if (cd "${cwd}" && "${RMAP_BIN}" doctor 2>/dev/null) | grep -qiE "indexing|in use by daemon"; then
            return 0
        fi
        sleep 0.25
    done
    return 1
}

echo "=================================================================="
echo "DAEMON-VISIBILITY-1 real-transport E2E"
echo "=================================================================="
echo "rmap            : ${RMAP_BIN}"
echo "rmapd           : ${RMAPD_BIN}"
echo "RMAP_SOCKET_PATH: ${SOCKET_PATH}   (isolated; operator daemon untouched)"
echo "RMAP_STATE_ROOT : ${STATE_ROOT}"
echo "fixture files   : ${FIXTURE_FILES}"
echo "binary version  : $("${RMAP_BIN}" --version 2>&1 || true)"
hr

echo ">>> generating fixtures (${FIXTURE_FILES} TS files each)…"
gen_fixture "${FIXROOT}/repoA" "${FIXTURE_FILES}"
gen_fixture "${FIXROOT}/repoB" "${FIXTURE_FILES}"
gen_fixture "${FIXROOT}/repoC" "${FIXTURE_FILES}"

echo ">>> starting isolated rmapd…"
start_daemon || exit 1
pass "isolated rmapd is up on ${SOCKET_PATH}"

# ── Proof D0: first index registers repoA (precondition for the E contention proof) ──
# E's "in use by daemon" is served by `storage_health`, which resolves the repo from the registry —
# and `record_index` only registers a repo on a SUCCESSFUL index. So E can only be observed during a
# RE-index of an already-registered repo (a first index of a fresh repo is not yet resolvable). This
# is also the day-2 field shape: the repo was indexed before, then re-indexed under contention.
hr
echo ">>> [D0] index repoA to READY — registers it so the E contention proof can resolve it"
"${RMAP_BIN}" index "${FIXROOT}/repoA" >"${OUT}/indexA-ready.txt" 2>&1
A_EXIT=$?
echo "    --- rmap index repoA (first) exit=${A_EXIT} ---"
sed 's/^/    /' "${OUT}/indexA-ready.txt"
if [[ "${A_EXIT}" -ne 0 ]]; then
    fail "D0: first index of repoA did not succeed (exit ${A_EXIT})"
fi
# D3 (enrichment honesty) on the completion report itself.
if grep -qiE "enrichment: not run" "${OUT}/indexA-ready.txt"; then
    pass "D3: the index completion report states enrichment did not run (with the next action)"
else
    fail "D3: the index completion report did not state enrichment status"
fi

# ── Proof D + E: RE-INDEX repoA, observe doctor mid-flight (status + contention) ──
hr
echo ">>> [D/E] re-index repoA in the background; observe doctor while the daemon holds the DB"
( "${RMAP_BIN}" index "${FIXROOT}/repoA" >"${OUT}/indexA-reindex.txt" 2>&1 ) &
IDX_A=$!
if wait_until_indexing "${FIXROOT}/repoA" 80; then
    (cd "${FIXROOT}/repoA" && "${RMAP_BIN}" doctor) >"${OUT}/doctor-midindex.txt" 2>&1 || true
    echo "    --- doctor (mid-reindex) ---"
    sed 's/^/    /' "${OUT}/doctor-midindex.txt"
    if grep -qiE "indexing" "${OUT}/doctor-midindex.txt"; then
        pass "D: doctor activity line reports the in-flight index"
    else
        fail "D: doctor did not report the in-flight index"
    fi
    # E: repoA is REGISTERED (D0), so storage_health resolves it and reports the daemon's OWN lock as
    # healthy 'in use by daemon' — the fix for the field bug where it cried 'error opening database'.
    if grep -qi "in use by daemon" "${OUT}/doctor-midindex.txt"; then
        pass "E: doctor storage line reports 'in use by daemon' (contention truth)"
    else
        fail "E: doctor did NOT report 'in use by daemon' for a live daemon's own lock"
    fi
    if grep -qi "error opening database" "${OUT}/doctor-midindex.txt"; then
        fail "E: doctor cried 'error opening database' for a live daemon's own lock"
    else
        pass "E: doctor never said 'error opening database' during the live index"
    fi
else
    fail "D/E: could not observe repoA mid-reindex within ~20s — the fixture must index slowly enough to catch (raise FIXTURE_FILES/FNS_PER_FILE)"
fi
wait "${IDX_A}" 2>/dev/null || true

# ── Proof D3: post-index doctor (idle + last snapshot + enrichment line) ──────
hr
echo ">>> [D3] post-index doctor — repoA is done; doctor shows idle + last snapshot + enrichment"
(cd "${FIXROOT}/repoA" && "${RMAP_BIN}" doctor) >"${OUT}/doctor-postindex.txt" 2>&1 || true
echo "    --- doctor (post-index) ---"
sed 's/^/    /' "${OUT}/doctor-postindex.txt"
if grep -qi "idle" "${OUT}/doctor-postindex.txt"; then
    pass "D: doctor activity line reports idle after completion"
else
    fail "D: doctor did not report idle after the index completed"
fi
# D2: the idle line NAMES the last completed snapshot (repo @ time) — completion is observable to a
# reader who "indexed 15 minutes ago"; a bare "idle" reads like "nothing ever happened".
if grep -qi "last snapshot" "${OUT}/doctor-postindex.txt"; then
    pass "D2: doctor idle line names the last snapshot (repo @ time)"
else
    fail "D2: doctor idle line did NOT name the last snapshot"
fi
if grep -qiE "enrichment:" "${OUT}/doctor-postindex.txt"; then
    pass "D3: doctor shows the enrichment status line"
else
    fail "D3: doctor did not show the enrichment status line"
fi

# ── Proof C: still-running client — DETERMINISTIC via the DB write lock ───────
# A lone short-timeout index is a RACE: it can complete inside the 1s window (exit 0), proving nothing
# about the still-running path. Force that path deterministically instead. `handle_index` takes a
# BLOCKING DB write lock (`acquire_write`, state.rs) at the top, BEFORE it emits any progress frame —
# so a SECOND index of a repo already being indexed blocks there in total silence (no frame resets the
# client's read deadline). We: (1) index repoB to READY, (2) start a background "holder" re-index that
# grabs+holds repoB's write lock for seconds, (3) confirm it is genuinely in flight (doctor), then
# (4) attach a "prober" index of the SAME repo with a 1s read timeout. The prober blocks on the held
# lock → its read times out with the daemon silent → it probes `daemon_info` on a FRESH connection
# (served concurrently on another daemon thread), sees repoB's live index, and reports STILL RUNNING
# with the distinct exit status 3. STRICT: exit 0 (completed) and exit 2 (failure) BOTH fail the proof
# — only exit 3 + "STILL RUNNING" passes. (The pure timeout→probe→classify logic is also unit-tested in
# rgr: `still_running_timeout_yields_distinct_non_failure_exit_status`.)
hr
echo ">>> [C] still-running (deterministic) — hold repoB's DB write lock, attach a 1s-timeout prober"
"${RMAP_BIN}" index "${FIXROOT}/repoB" >"${OUT}/indexB-ready.txt" 2>&1
B_READY_EXIT=$?
if [[ "${B_READY_EXIT}" -ne 0 ]]; then
    fail "C: precondition — first index of repoB did not succeed (exit ${B_READY_EXIT})"
fi
( "${RMAP_BIN}" index "${FIXROOT}/repoB" >"${OUT}/indexB-holder.txt" 2>&1 ) &
HOLD_B=$!
if wait_until_indexing "${FIXROOT}/repoB" 80; then
    RMAP_LONG_OP_READ_TIMEOUT_SECS=1 "${RMAP_BIN}" index "${FIXROOT}/repoB" >"${OUT}/indexB-prober.txt" 2>&1
    C_EXIT=$?
    echo "    --- rmap index prober (timeout=1s, write lock held by holder) exit=${C_EXIT} ---"
    sed 's/^/    /' "${OUT}/indexB-prober.txt"
    # STRICT: only exit 3 (EXIT_STILL_RUNNING) + the STILL RUNNING line passes. Exit 0 proves nothing
    # (the prober was not blocked); exit 2 is the failure code (the exact field bug).
    if [[ "${C_EXIT}" -eq 2 ]]; then
        fail "C: prober reported the FAILURE exit code (2) — the exact field bug"
    elif [[ "${C_EXIT}" -eq 3 ]] && grep -qi "STILL RUNNING" "${OUT}/indexB-prober.txt"; then
        pass "C: prober blocked on the live index, timed out, and reported STILL RUNNING (exit 3, distinct non-failure)"
    else
        fail "C: expected deterministic exit 3 + 'STILL RUNNING'; got exit ${C_EXIT} (a race or a lost op)"
    fi
else
    fail "C: could not get repoB in flight to hold the write lock (raise FIXTURE_FILES/FNS_PER_FILE)"
fi
wait "${HOLD_B}" 2>/dev/null || true
# The prober's own index (queued behind the holder, then detached when its client timed out) finishes
# in the background (INDEX-DISCONNECT-1) — let it settle before the next proof.
sleep 1

# ── Proof ID1 (INDEX-DISCONNECT-1): kill the CLIENT mid-index → daemon completes DETACHED to READY ──
# The F5 field bug: a client disconnect (read timeout / closed terminal / machine sleep) made the
# daemon's next progress emit fail (EPIPE) and ABORT the in-flight index — hours of work lost, the repo
# left unregistered, the snapshot stuck non-READY. Fix: progress emission is BEST-EFFORT, so a dead
# client never aborts a durable write. Here we index a FRESH repoD over the real socket, SIGKILL the
# CLIENT (not the daemon) mid-index, and prove the daemon finishes the index DETACHED: repoD reaches
# READY (orient answers) and stays registered, and the daemon logs the detached continuation.
hr
echo ">>> [ID1] kill the CLIENT mid-index — the daemon must finish the index detached to READY"
gen_fixture "${FIXROOT}/repoD" "${FIXTURE_FILES}"
# Count prior detached-continuation lines (proof C's timed-out prober also detaches) so we can assert
# repoD adds at least one more. The per-op "exactly one" is unit-tested (tests/index_disconnect.rs).
DETACHED_BEFORE="$(grep -c "continues detached" "${OUT}/rmapd.log" 2>/dev/null || true)"
# Launch the index client DIRECTLY (no subshell) so $! is the rmap CLIENT pid we can SIGKILL.
"${RMAP_BIN}" index "${FIXROOT}/repoD" >"${OUT}/indexD-client.txt" 2>&1 &
CLIENT_D=$!
if wait_until_indexing "${FIXROOT}/repoD" 80; then
    note "repoD index is in flight — SIGKILL the CLIENT (pid ${CLIENT_D}); the daemon keeps the work"
    kill -9 "${CLIENT_D}" 2>/dev/null || true
    wait "${CLIENT_D}" 2>/dev/null || true
    # Poll until the DETACHED index finishes: `orient` exits 0 only when a READY snapshot exists (it
    # resolves the repo from cwd). A regression (disconnect aborts the index) never reaches READY here,
    # so this stays non-zero → the loop times out → FAIL.
    D_READY=false
    for _ in $(seq 1 120); do
        if (cd "${FIXROOT}/repoD" && "${RMAP_BIN}" orient) >"${OUT}/orient-D.txt" 2>&1; then
            D_READY=true
            break
        fi
        sleep 0.5
    done
    if [[ "${D_READY}" == "true" ]]; then
        pass "ID1: daemon finished the index DETACHED after the client was killed — repoD orient answers (READY)"
    else
        fail "ID1: repoD never reached READY after the client was killed (the disconnect aborted the work — F5 regression)"
        echo "    --- orient repoD (last attempt) ---"; sed 's/^/    /' "${OUT}/orient-D.txt"
    fi
    # Registration persisted over the real transport: repo info resolves repoD (never 'not indexed').
    (cd "${FIXROOT}/repoD" && "${RMAP_BIN}" repo info) >"${OUT}/repo-info-D.txt" 2>&1 || true
    echo "    --- rmap repo info (repoD, after client kill) ---"
    sed 's/^/    /' "${OUT}/repo-info-D.txt"
    if grep -qi "not indexed" "${OUT}/repo-info-D.txt"; then
        fail "ID1: repoD is 'not indexed' — the up-front registration did not persist"
    else
        pass "ID1: repoD is registered (repo info resolves it, never 'not indexed')"
    fi
    # The detached-continuation path executed: at least one NEW 'continues detached' line for repoD.
    DETACHED_AFTER="$(grep -c "continues detached" "${OUT}/rmapd.log" 2>/dev/null || true)"
    if [[ "$(( DETACHED_AFTER - DETACHED_BEFORE ))" -ge 1 ]]; then
        pass "ID1: the daemon logged the detached continuation (${DETACHED_BEFORE} → ${DETACHED_AFTER})"
    else
        fail "ID1: no detached-continuation log line for the killed-client index (${DETACHED_BEFORE} → ${DETACHED_AFTER})"
    fi
else
    fail "ID1: could not catch repoD mid-index within ~20s — raise FIXTURE_FILES/FNS_PER_FILE"
    kill -9 "${CLIENT_D}" 2>/dev/null || true
    wait "${CLIENT_D}" 2>/dev/null || true
fi

# ── Proof F1/F3: kill daemon mid-index → interrupted snapshot → prune reclaims ─
hr
echo ">>> [F1/F3] field repro — index repoC (READY), then kill the daemon during a RE-index, restart, prune"
# Step 1: a clean first index so repoC is REGISTERED (registry.save runs only on success). This is the
# field precondition: a repo that indexed fine before the interruption.
if ! "${RMAP_BIN}" index "${FIXROOT}/repoC" >"${OUT}/indexC-ready.txt" 2>&1; then
    fail "F1/F3: could not index repoC to READY (precondition)"
fi
note "repoC indexed to READY (registered). Now starting a RE-index to interrupt mid-flight…"
# Step 2: re-index in the background and hard-kill the daemon while it runs. The re-index's `building`
# snapshot is created (and WAL-durable) EARLY, before extraction; the ~1s settle guarantees it landed.
# It is then orphaned beside the still-present READY snapshot after the -9 + restart.
( "${RMAP_BIN}" index "${FIXROOT}/repoC" >"${OUT}/indexC-reindex.txt" 2>&1 ) &
IDX_C=$!
if wait_until_indexing "${FIXROOT}/repoC" 80; then
    note "repoC RE-index is in flight — simulating an abrupt daemon stop (restart / machine sleep)…"
    sleep 1.0   # let create_snapshot + some extraction land so the partial is WAL-durable and holds bytes
    hard_kill_daemon
    wait "${IDX_C}" 2>/dev/null || true
    note "daemon killed mid-reindex; restarting…"
    if start_daemon; then
        # F1: repo info NAMES the interrupted snapshot beside the surviving READY one.
        (cd "${FIXROOT}/repoC" && "${RMAP_BIN}" repo info) >"${OUT}/repo-info-interrupted.txt" 2>&1 || true
        echo "    --- rmap repo info (repoC: READY + interrupted) ---"
        sed 's/^/    /' "${OUT}/repo-info-interrupted.txt"
        if grep -qiE "interrupted|in progress" "${OUT}/repo-info-interrupted.txt"; then
            pass "F1: repo info names the interrupted (non-READY) snapshot"
        else
            fail "F1: repo info did NOT name the interrupted snapshot left by the kill"
        fi
        # F3: prune deletes + reclaims the orphaned partial, keeping the READY snapshot. No live op holds
        # the DB (the daemon was killed + restarted), so the orphaned partial MUST be reclaimed.
        (cd "${FIXROOT}/repoC" && "${RMAP_BIN}" maintenance prune) >"${OUT}/prune-repoC.txt" 2>&1
        echo "    --- rmap maintenance prune (repoC) ---"
        sed 's/^/    /' "${OUT}/prune-repoC.txt"
        if grep -qiE "reclaimed [1-9][0-9]* interrupted snapshot|freed" "${OUT}/prune-repoC.txt"; then
            pass "F3: prune reclaimed the orphaned interrupted snapshot (freed disk)"
        else
            fail "F3: prune did NOT reclaim the orphaned interrupted snapshot (no live op — it should have)"
        fi
        # Confirm the READY snapshot survived.
        (cd "${FIXROOT}/repoC" && "${RMAP_BIN}" repo info) >"${OUT}/repo-info-afterprune.txt" 2>&1 || true
        if grep -qi "ready" "${OUT}/repo-info-afterprune.txt"; then
            pass "F3: a READY snapshot survives the reclaim (READY untouched)"
        else
            fail "F3: the READY snapshot did NOT survive the reclaim (READY must be untouched)"
        fi
    else
        fail "F3: daemon did not restart after the hard kill"
    fi
else
    fail "F1/F3: could not catch repoC mid-reindex within ~20s — raise FIXTURE_FILES/FNS_PER_FILE"
    wait "${IDX_C}" 2>/dev/null || true
fi

# ── Non-pollution proof ───────────────────────────────────────────────────────
hr
echo ">>> non-pollution — the operator registry must NOT mention our isolated fixtures"
if [[ -f "${OPERATOR_REGISTRY}" ]]; then
    if grep -qF "${FIXROOT}" "${OPERATOR_REGISTRY}" 2>/dev/null; then
        fail "operator registry mentions an isolated fixture — ISOLATION BREACHED"
    else
        pass "operator registry does NOT contain our fixtures (${OPERATOR_REGISTRY})"
    fi
else
    note "operator registry not present; nothing to pollute."
fi

stop_daemon
hr
if [[ "${FAILURES}" -eq 0 ]]; then
    echo "OK — DAEMON-VISIBILITY-1 real-transport E2E: all required assertions passed."
    echo "  outputs: ${OUT}/  (KEEP=true to retain)"
    exit 0
else
    echo "FAILED — ${FAILURES} required assertion(s) failed. See ${OUT}/ for transcripts." >&2
    KEEP=true   # retain artifacts on failure for inspection
    exit 1
fi
