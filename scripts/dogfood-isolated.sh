#!/usr/bin/env bash
#
# dogfood-isolated.sh — run `rmap` against an indexed fixture in a FULLY ISOLATED
# daemon state root, WITHOUT touching the operator's real daemon or registry.
#
# ── Why this exists ──────────────────────────────────────────────────────────
# Relay/review agents repeatedly hit `error: repo not indexed`: the agent sandbox
# has no indexed repo, and an agent must NOT pollute the operator's real registry
# (`~/Library/Application Support/repo-graph/`) just to exercise the CLI. This
# harness closes that gap: it stands up an ephemeral, throwaway daemon state root,
# indexes a tiny self-contained TypeScript fixture, and runs the live agent
# orientation commands (`orient`, `explain`, `check`) against it — provably
# isolated from the operator's installed daemon.
#
# Canonical reference: docs/testing/end-of-slice-procedure.md  (§ Isolated dogfood).
#
# ── Isolation mechanism (two independent levers, both load-bearing) ───────────
#   RMAP_TRANSPORT=stdio   → `rmap` spawns its OWN `rmapd --stdio` subprocess and
#                            talks to it over stdin/stdout. NO Unix socket is
#                            opened, so the operator's resident launchd daemon
#                            (`com.repo-graph.rmapd`) is never contacted. The
#                            subprocess exits on EOF when each `rmap` call ends.
#   RMAP_STATE_ROOT=<tmp>  → the spawned daemon writes `registry.json` and
#                            `databases/` under <tmp> instead of the operator's
#                            data dir. <tmp> lives under /private/tmp, which puts
#                            the daemon in SandboxLocal mode (STATE-ROOT-
#                            SEPARATION-1): A1 authority writes (alias/baseline/
#                            declaration) are blocked, but A2/B (index, query) are
#                            allowed — exactly what a dogfood needs.
# The subprocess inherits the parent environment, so both env vars flow into the
# `rmapd --stdio` child automatically.
#
# State persists across the per-call subprocesses via on-disk registry.json +
# the per-repo SQLite DB under RMAP_STATE_ROOT (handle_index calls registry.save()),
# so `index` in one call is visible to `orient`/`explain`/`check` in later calls.
#
# ── No SCIP / no scip-typescript required ────────────────────────────────────
# `rmap index` uses the homegrown tree-sitter extractor (SQLite-served), and
# orient/check/explain serve from that SQLite base. The SCIP/LiveGraph path is
# opt-in (`rmap dev livegraph-refresh`, `--engine livegraph`) and is NOT what
# these default commands use, so this harness needs no scip-typescript provision.
#
# ── Usage ────────────────────────────────────────────────────────────────────
#   ./scripts/dogfood-isolated.sh                 # run, then clean up the state root
#   ./scripts/dogfood-isolated.sh --keep          # run, retain artifacts for inspection
#   RMAP_BIN=/path/to/rmap ./scripts/dogfood-isolated.sh   # override binary (rmapd must be its sibling)
#
# Exit 0 on success (all commands ran AND operator registry confirmed untouched);
# nonzero on any failure.
#
# PLATFORM: macOS (uses /private/tmp + the launchd state-root convention). The
# mechanism is platform-portable; only the operator-registry path probe is macOS.

set -euo pipefail

# ── Args ─────────────────────────────────────────────────────────────────────
KEEP=false
for arg in "$@"; do
    case "$arg" in
        --keep) KEEP=true ;;
        -h|--help)
            sed -n '2,60p' "$0"
            exit 0
            ;;
        *) echo "error: unknown argument '$arg' (try --help)" >&2; exit 2 ;;
    esac
done

# ── Resolve binaries (rmapd MUST be the sibling of rmap for stdio spawn) ──────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

RMAP_BIN="${RMAP_BIN:-${REPO_ROOT}/rust/target/release/rmap}"

if [[ ! -x "${RMAP_BIN}" ]]; then
    echo "error: rmap binary not found/executable: ${RMAP_BIN}" >&2
    echo "       build it first:  (cd rust && cargo build --release --bin rmap --bin rmapd)" >&2
    exit 1
fi

BIN_DIR="$(cd "$(dirname "${RMAP_BIN}")" && pwd)"
RMAPD_BIN="${BIN_DIR}/rmapd"
if [[ ! -x "${RMAPD_BIN}" ]]; then
    echo "error: rmapd not found beside rmap: ${RMAPD_BIN}" >&2
    echo "       stdio transport spawns rmapd as a sibling of rmap; both must co-locate." >&2
    exit 1
fi
# Belt-and-suspenders: also expose rmapd via PATH for find_rmapd's PATH fallback.
export PATH="${BIN_DIR}:${PATH}"

# ── Isolated state root (ephemeral, under /private/tmp → SandboxLocal mode) ────
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
STATE_ROOT="/private/tmp/repo-graph-dogfood/${RUN_ID}"
FIXTURE="${STATE_ROOT}/fixture"
OUT_DIR="${STATE_ROOT}/out"
mkdir -p "${FIXTURE}/src" "${OUT_DIR}"

# The operator's REAL state root — we only READ it (to prove non-pollution), never write.
OPERATOR_STATE_ROOT="${HOME}/Library/Application Support/repo-graph"
OPERATOR_REGISTRY="${OPERATOR_STATE_ROOT}/registry.json"

# The isolation environment, applied per-call (kept explicit so the doc can quote it).
export RMAP_TRANSPORT="stdio"
export RMAP_STATE_ROOT="${STATE_ROOT}"

echo "=================================================================="
echo "rmap ISOLATED dogfood harness"
echo "=================================================================="
echo "rmap            : ${RMAP_BIN}"
echo "rmapd (sibling) : ${RMAPD_BIN}"
echo "RMAP_TRANSPORT  : ${RMAP_TRANSPORT}   (no Unix socket; operator daemon untouched)"
echo "RMAP_STATE_ROOT : ${RMAP_STATE_ROOT}  (ephemeral; SandboxLocal)"
echo "fixture         : ${FIXTURE}"
echo "binary version  : $("${RMAP_BIN}" --version 2>&1 || true)"
echo "------------------------------------------------------------------"

# ── Tiny self-contained TypeScript fixture (no node_modules, no network) ──────
# 3 source files. util.ts + main.ts give a clear import + call graph so
# orient/explain/check have real structure to report (IMPORTS edge; CALLS
# computeScore→{square,clamp}, main→computeScore, main→console.log[unresolved
# builtin]). many.ts holds 20 symbols — OVER the default explain item cap (15 at
# `medium`) — so the TRUNCATION-AUDIT-1 `--full` demonstration below can show the
# cap BITE by default and VANISH under `--full` on live output.
cat > "${FIXTURE}/package.json" <<'JSON'
{
  "name": "rmap-dogfood-fixture",
  "version": "0.0.0",
  "private": true
}
JSON

cat > "${FIXTURE}/tsconfig.json" <<'JSON'
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "strict": true
  },
  "include": ["src"]
}
JSON

cat > "${FIXTURE}/src/util.ts" <<'TS'
export function square(n: number): number {
  return n * n;
}

export function clamp(n: number, lo: number, hi: number): number {
  if (n < lo) return lo;
  if (n > hi) return hi;
  return n;
}
TS

cat > "${FIXTURE}/src/main.ts" <<'TS'
import { square, clamp } from "./util";

export function computeScore(values: number[]): number {
  let total = 0;
  for (const v of values) {
    total += square(clamp(v, 0, 100));
  }
  return total;
}

export function main(): void {
  const score = computeScore([10, 200, -5]);
  console.log(score);
}
TS

# many.ts: 20 exported functions (manyFn00..manyFn19), one per line so the
# extractor records 20 symbols with ascending line_start. 20 > the default explain
# EXPLAIN_SYMBOLS item cap (15 at `medium`), so the list truncates by default and is
# uncapped under `--full`. `sort_explain_symbols` orders by line_start ASC, so the
# surviving default top-15 is manyFn00..manyFn14 and manyFn19 is the dropped tail —
# the assertion below keys off manyFn19's presence/absence.
{
    for i in $(seq -w 0 19); do
        printf 'export function manyFn%s(): number { return %d; }\n' "$i" "$((10#$i))"
    done
} > "${FIXTURE}/src/many.ts"

# ── run(): echo the EXACT command, execute it, capture stdout to a file ────────
# Commands that resolve the repo "from cwd" must run with cwd = the fixture dir.
run() {
    local label="$1"; shift
    local outfile="$1"; shift
    local errfile="${outfile%.txt}.stderr"
    echo ""
    echo ">>> [${label}] (cwd=$(pwd))"
    echo "    \$ RMAP_TRANSPORT=stdio RMAP_STATE_ROOT=<state-root> $*"
    # Capture stdout and stderr into SEPARATE per-command files (<cmd>.txt /
    # <cmd>.stderr). Per-command (not one shared interleaved log) so each command's
    # output is fully attributable and the procedure doc §4.2 can quote these files
    # BYTE-FOR-BYTE — no hand-transcription. `index` reports its "indexed N files…"
    # summary, the daemon sandbox notes, and the `--stdio` warning to stderr.
    if "$@" >"${outfile}" 2>"${errfile}"; then
        if [[ -s "${outfile}" ]]; then
            sed 's/^/    /' "${outfile}"
        else
            echo "    (no stdout — this command reports to stderr; see $(basename "${errfile}"))"
        fi
        return 0
    else
        local rc=$?
        echo "    !! command failed (exit ${rc}); stderr tail:" >&2
        tail -n 20 "${errfile}" | sed 's/^/    /' >&2
        return ${rc}
    fi
}

# ── Phase 1: index the fixture into the ISOLATED state root ───────────────────
# `index` accepts an explicit repo_path; no --alias (alias is an A1 write,
# blocked in SandboxLocal — we resolve by path/cwd instead).
run "index" "${OUT_DIR}/index.txt" "${RMAP_BIN}" index "${FIXTURE}"

# All subsequent commands resolve the repo FROM CWD, so move into the fixture.
cd "${FIXTURE}"

# ── Phase 2: the live agent-orientation surface ───────────────────────────────
run "orient"  "${OUT_DIR}/orient.txt"  "${RMAP_BIN}" orient
run "explain" "${OUT_DIR}/explain.txt" "${RMAP_BIN}" explain "src/main.ts"
run "check"   "${OUT_DIR}/check.txt"   "${RMAP_BIN}" check

# ── Phase 2b: TRUNCATION-AUDIT-1 — live `--full` capture + cap-bite proof ──────
# many.ts has 20 symbols > the default explain item cap (15 at `medium`), so the
# EXPLAIN_SYMBOLS list TRUNCATES by default and is UNCAPPED under `--full`. These
# runs capture that end-to-end (the operator-required `--full` dogfood), and the
# assertion proves `--full` truly uncaps: the 20th symbol (manyFn19) is OMITTED by
# default and PRESENT under `--full`, and the `items_truncated` flag tracks the cut.
run "explain many (default cap)"  "${OUT_DIR}/explain-many.txt"       "${RMAP_BIN}" explain "src/many.ts"
run "explain many --full"         "${OUT_DIR}/explain-many-full.txt"  "${RMAP_BIN}" explain "src/many.ts" --full
run "explain many --json"         "${OUT_DIR}/explain-many.json"      "${RMAP_BIN}" explain "src/many.ts" --json
run "explain many --full --json"  "${OUT_DIR}/explain-many-full.json" "${RMAP_BIN}" explain "src/many.ts" --full --json
run "orient --full"               "${OUT_DIR}/orient-full.txt"        "${RMAP_BIN}" orient --full
run "check --full (no-op)"        "${OUT_DIR}/check-full.txt"         "${RMAP_BIN}" check --full

echo ""
echo "------------------------------------------------------------------"
echo "TRUNCATION-AUDIT-1 --full proof (EXPLAIN_SYMBOLS: 20 symbols vs default cap 15):"
MANY_DEF_JSON="${OUT_DIR}/explain-many.json"
MANY_FULL_JSON="${OUT_DIR}/explain-many-full.json"

# (1) --full emits the COMPLETE list: the 20th symbol survives.
if grep -q "manyFn19" "${MANY_FULL_JSON}"; then
    echo "  PASS: --full output includes manyFn19 (20th symbol) — the list is UNCAPPED"
else
    echo "  FAIL: --full output is missing manyFn19 — --full did NOT uncap" >&2
    exit 1
fi
# (2) the default cut is LOAD-BEARING: the 20th symbol is omitted at cap 15.
if grep -q "manyFn19" "${MANY_DEF_JSON}"; then
    echo "  FAIL: default (cap 15) output unexpectedly includes manyFn19 — cap did not bite" >&2
    exit 1
else
    echo "  PASS: default output OMITS manyFn19 — the 15-item cap truncated the list"
fi
# (3) the truncation flag tracks the cut honestly: present by default, absent under --full.
if grep -q "items_truncated" "${MANY_DEF_JSON}"; then
    echo "  PASS: default --json carries items_truncated (the cut is reported, not hidden)"
else
    echo "  FAIL: default --json lacks items_truncated despite a 20>15 cut" >&2
    exit 1
fi
if grep -q "items_truncated" "${MANY_FULL_JSON}"; then
    echo "  FAIL: --full --json still carries items_truncated — something truncated under --full" >&2
    exit 1
else
    echo "  PASS: --full --json has NO items_truncated flag — nothing was cut"
fi

# ── HUMAN grep-path proof (TRUNCATION-AUDIT-1 review-1 #1/#4) ──────────────────
# The JSON proof above only exercises the DATA layer (Budget::Full). The acceptance use case is
# `rmap explain <target> --full | grep <x>` on the HUMAN render, which has a SECOND, independent
# presentation cap (EXPLAIN_SYMBOLS: 15). review-1 OBSERVED `--full` human output still stopping at
# manyFn14 with "... (5 more)". These assertions key off the HUMAN .txt files (not JSON): under
# `--full` the 20th symbol must be present; by default the presentation cap must still bite.
echo ""
echo "TRUNCATION-AUDIT-1 --full HUMAN grep-path proof (presentation cap, not just JSON):"
MANY_DEF_TXT="${OUT_DIR}/explain-many.txt"
MANY_FULL_TXT="${OUT_DIR}/explain-many-full.txt"

# (4) the HUMAN render under --full is UNCAPPED — grep on stdout sees the 20th symbol.
if grep -q "manyFn19" "${MANY_FULL_TXT}"; then
    echo "  PASS: --full HUMAN output includes manyFn19 — grep sees the COMPLETE list"
else
    echo "  FAIL: --full HUMAN output is missing manyFn19 — the presentation cap still truncates" >&2
    exit 1
fi
# (5) the default HUMAN render is still capped (presentation cap bites without --full).
if grep -q "manyFn19" "${MANY_DEF_TXT}"; then
    echo "  FAIL: default HUMAN output unexpectedly includes manyFn19 — presentation cap did not bite" >&2
    exit 1
else
    echo "  PASS: default HUMAN output OMITS manyFn19 — the presentation cap truncates by default"
fi

# ── Phase 3: prove the fixture lives in the ISOLATED registry ─────────────────
run "repo list (isolated)" "${OUT_DIR}/repo-list-isolated.txt" "${RMAP_BIN}" repo list

# ── Non-pollution proof: the operator's REAL registry must NOT know this fixture ─
echo ""
echo "------------------------------------------------------------------"
echo "Non-pollution check (operator state root is READ-ONLY here):"
FIXTURE_CANON="$(cd "${FIXTURE}" && pwd -P)"
if [[ -f "${OPERATOR_REGISTRY}" ]]; then
    if grep -qF "${FIXTURE_CANON}" "${OPERATOR_REGISTRY}"; then
        echo "  FAIL: operator registry mentions the fixture path — ISOLATION BREACHED" >&2
        echo "        ${OPERATOR_REGISTRY}" >&2
        exit 1
    fi
    echo "  PASS: ${OPERATOR_REGISTRY}"
    echo "        does NOT contain ${FIXTURE_CANON}"
else
    echo "  note: operator registry not present (${OPERATOR_REGISTRY}); nothing to pollute."
fi

# The isolated registry SHOULD contain it (proves the write landed in the sandbox).
ISOLATED_REGISTRY="${STATE_ROOT}/registry.json"
if [[ -f "${ISOLATED_REGISTRY}" ]] && grep -qF "${FIXTURE_CANON}" "${ISOLATED_REGISTRY}"; then
    echo "  PASS: isolated registry recorded the fixture:"
    echo "        ${ISOLATED_REGISTRY}"
else
    echo "  FAIL: isolated registry missing the fixture entry: ${ISOLATED_REGISTRY}" >&2
    exit 1
fi

# ── Summary + cleanup ─────────────────────────────────────────────────────────
echo ""
echo "=================================================================="
echo "OK — isolated dogfood complete."
echo "  state root : ${STATE_ROOT}"
echo "  outputs    : ${OUT_DIR}/<cmd>.{txt,stderr}  (cmd: index orient explain check repo-list-isolated)"
echo "  --full cap : ${OUT_DIR}/explain-many{,-full}.{txt,json}, orient-full.txt, check-full.txt"
echo "=================================================================="

if [[ "${KEEP}" == "true" ]]; then
    echo "Retained (--keep). Remove with:  rm -rf '${STATE_ROOT}'"
else
    rm -rf "${STATE_ROOT}"
    echo "Cleaned up ${STATE_ROOT} (pass --keep to retain)."
fi
