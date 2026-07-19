#!/usr/bin/env bash
#
# validate-layer2-fixture.sh — LIVE end-to-end proof of the RECON-M-R4 Layer-2 landing
# (recon-design-1 §5.5): a call the tree-sitter pipeline leaves UNRESOLVED but the compiler
# (scip-typescript) RESOLVES becomes the labeled "likely resolves to X" annotation on the
# attribution/trust surface AND explain SYMBOL focus. This is the binding slice §4 live E2E for
# `docs/slices/recon-m-r4-layer2-attribution-1.md` — the "amodx Toolbar → cn(...)" class, minimized.
#
# ── Why a persistent SOCKET daemon (not the stdio dogfood) ────────────────────────────────────
# The Layer-2 block is read from the IN-MEMORY LiveGraph + witness ledger. `livegraph-refresh`
# populates them; the ledger warms lazily on the first serve (orient/callers → callgraph_is_green).
# A stdio `rmap` call is a fresh short-lived daemon, so the refresh-populated graph would be gone by
# the `trust` call. This harness stands up ONE isolated, throwaway `rmapd` so index → refresh →
# orient → trust → explain all hit the SAME resident process — fully isolated from the operator's
# daemon (its own socket + state root under /private/tmp; a safety gate aborts if the registry is
# not empty). No committed fixture files: the tiny TypeScript project is generated inline.
#
# ── The fixture, and why P fails but S resolves (indexer/src/resolver.rs) ──────────────────────
#   src/lib/utils.ts     export function cn(...)          ← the `@/lib/utils` alias target
#   src/other/helpers.ts export function cn(...)          ← a 2nd same-named export (repo-wide ambiguity)
#   src/Toolbar.ts       import { cn } from "@/lib/utils"; ... cn(...)
#   tsconfig.json        "paths": { "@/*": ["./src/*"] }
# P: bare name `cn` is ambiguous (2 defs) → the singleton lookup refuses (resolver.rs:463); named-
# import narrowing finds the binding but bails on the NON-RELATIVE `@/lib/utils` (resolver.rs:512)
# → the call lands in `unresolved_edges` (type='CALLS'). scip-typescript honours tsconfig `paths`
# and resolves the SAME call to `src/lib/utils.ts#cn` → ONE `semantic`/`new_pair` edge → M-R4 lands
# a single-candidate "likely" hint (never ambiguous — nothing calls the 2nd `cn`).
#
# ── Usage ──────────────────────────────────────────────────────────────────────────────────────
#   ./scripts/validate-layer2-fixture.sh              # build not required if release binaries exist
#   RMAP_BIN=rust/target/debug/rmap ./scripts/validate-layer2-fixture.sh   # use an existing debug build
#   ./scripts/validate-layer2-fixture.sh --keep       # retain the isolated state root for inspection
#
# Exit 0 = the annotation rendered on BOTH surfaces (PASS). Exit 1 = a surface did not render it
# (FAIL). Exit 4 = scip-typescript not found, so the compiler half cannot run (SKIP — not a pass,
# not a code fault). PLATFORM: macOS (/private/tmp + the isolated-daemon convention).

set -uo pipefail

KEEP=false
for a in "$@"; do case "$a" in
    --keep) KEEP=true ;;
    -h|--help) sed -n '2,40p' "$0"; exit 0 ;;
    *) echo "error: unknown argument '$a' (try --help)" >&2; exit 2 ;;
esac; done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ── Binaries (rmapd must be the sibling of rmap for the socket daemon) ──────────────────────────
RMAP_BIN="${RMAP_BIN:-${REPO_ROOT}/rust/target/release/rmap}"
if [[ ! -x "${RMAP_BIN}" ]]; then
    echo "error: rmap not found: ${RMAP_BIN}" >&2
    echo "       build it:  (cd rust && cargo build --release --bin rmap --bin rmapd)" >&2
    echo "       or override: RMAP_BIN=rust/target/debug/rmap $0" >&2
    exit 2
fi
BIN_DIR="$(cd "$(dirname "${RMAP_BIN}")" && pwd)"
# Canonicalize to ABSOLUTE — every command below runs after `cd` into the fixture, so a relative
# override (e.g. RMAP_BIN=rust/target/debug/rmap) would fail to resolve post-cd.
RMAP_BIN="${BIN_DIR}/$(basename "${RMAP_BIN}")"
RMAPD_BIN="${BIN_DIR}/rmapd"
[[ -x "${RMAPD_BIN}" ]] || { echo "error: rmapd not beside rmap: ${RMAPD_BIN}" >&2; exit 2; }

# ── Discover scip-typescript (the compiler witness). Absent ⇒ SKIP, never a false pass. ─────────
if [[ -z "${RMAP_SCIP_TYPESCRIPT:-}" ]]; then
    for cand in \
        "${HOME}/.local/share/repo-graph-tools/scip-typescript-0.4.0/bin/scip-typescript-node18" \
        "$(command -v scip-typescript 2>/dev/null || true)"; do
        [[ -n "${cand}" && -x "${cand}" ]] && { export RMAP_SCIP_TYPESCRIPT="${cand}"; break; }
    done
fi
if [[ -z "${RMAP_SCIP_TYPESCRIPT:-}" || ! -x "${RMAP_SCIP_TYPESCRIPT}" ]]; then
    echo "SKIP: scip-typescript not found (set RMAP_SCIP_TYPESCRIPT). The compiler witness cannot" >&2
    echo "      run, so the Layer-2 hit cannot be reproduced on this machine." >&2
    exit 4
fi

# ── Isolated, throwaway daemon (own socket + state root; /private/tmp ⇒ SandboxLocal) ───────────
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
ISO="/private/tmp/repo-graph-m-r4-e2e/${RUN_ID}"
STATE="${ISO}/state"; SOCK="${ISO}/daemon.sock"; FIXTURE="${ISO}/fixture"
mkdir -p "${STATE}" "${FIXTURE}/src/lib" "${FIXTURE}/src/other"

cleanup() {
    [[ -n "${DAEMON_PID:-}" ]] && { kill "${DAEMON_PID}" 2>/dev/null; wait "${DAEMON_PID}" 2>/dev/null; }
    ${KEEP} || rm -rf "${ISO}"
}
trap cleanup EXIT

echo "=================================================================="
echo "RECON-M-R4 Layer-2 landing — LIVE isolated E2E"
echo "  rmap    : ${RMAP_BIN}"
echo "  scip    : ${RMAP_SCIP_TYPESCRIPT}"
echo "  state   : ${STATE}   (isolated; SandboxLocal)"
echo "=================================================================="

# ── Generate the fixture inline ────────────────────────────────────────────────────────────────
cat > "${FIXTURE}/package.json" <<'JSON'
{ "name": "layer2-alias-fixture", "version": "0.0.0", "private": true }
JSON
cat > "${FIXTURE}/tsconfig.json" <<'JSON'
{
  "compilerOptions": {
    "target": "ES2020", "module": "commonjs", "strict": true,
    "baseUrl": ".", "paths": { "@/*": ["./src/*"] }
  },
  "include": ["src"]
}
JSON
cat > "${FIXTURE}/src/lib/utils.ts" <<'TS'
// The alias target: `@/lib/utils` resolves here via tsconfig `paths`.
export function cn(...classes: string[]): string {
  return classes.filter(Boolean).join(" ");
}
TS
cat > "${FIXTURE}/src/other/helpers.ts" <<'TS'
// A SECOND value-space `cn` — makes the bare name ambiguous repo-wide so the pipeline refuses to
// resolve Toolbar's call. Nothing calls this one, so the compiler mints no semantic edge to it.
export function cn(...parts: string[]): string {
  return parts.join("-");
}
TS
cat > "${FIXTURE}/src/Toolbar.ts" <<'TS'
import { cn } from "@/lib/utils";

// The genuine pipeline-unresolved / compiler-resolved site the M-R4 Layer-2 landing annotates.
export function Toolbar(active: boolean): string {
  return cn("toolbar", active ? "is-active" : "");
}
TS

# ── Start the isolated daemon, wait for its socket ─────────────────────────────────────────────
RMAP_STATE_ROOT="${STATE}" RMAP_SOCKET_PATH="${SOCK}" "${RMAPD_BIN}" >"${ISO}/daemon.log" 2>&1 &
DAEMON_PID=$!
for _ in $(seq 1 600); do [[ -S "${SOCK}" ]] && break; sleep 0.1; done
[[ -S "${SOCK}" ]] || { echo "FAIL: daemon socket never appeared" >&2; cat "${ISO}/daemon.log" >&2; exit 1; }
export RMAP_STATE_ROOT="${STATE}" RMAP_SOCKET_PATH="${SOCK}"

# ── SAFETY GATE: prove this is the isolated daemon, not the operator's ──────────────────────────
LIST="$("${RMAP_BIN}" repo list 2>&1 || true)"
if echo "${LIST}" | grep -qE "/repo-graph$|Users/[^/]+/Documents"; then
    echo "FAIL: registry not empty — refusing to run against a non-isolated daemon:" >&2
    echo "${LIST}" >&2; exit 2
fi

FAIL=0
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1" >&2; FAIL=1; }

echo "--- index (pipeline) ---";                 "${RMAP_BIN}" index "${FIXTURE}" 2>&1 | tail -1
echo "--- livegraph-refresh (scip-typescript) ---"; "${RMAP_BIN}" dev livegraph-refresh --repo "${FIXTURE}" 2>&1 | grep -E '"status"|"producer_unavailable"' || true
cd "${FIXTURE}"
"${RMAP_BIN}" orient  >/dev/null 2>&1 || true    # warm the ledger (callgraph_is_green)
"${RMAP_BIN}" callers "src/lib/utils.ts" >/dev/null 2>&1 || true

LIKELY_LINE="in Toolbar, cn(…) likely resolves to cn (src/lib/utils.ts) — a same-named call the compiler resolved; syntax did not confirm it"
HEADING="Compiler Evidence for Unresolved Calls"

echo "--- [A] attribution/trust (human) ---"
TRUST="$("${RMAP_BIN}" trust 2>/dev/null)"
echo "${TRUST}" | grep -qF "${HEADING}"      && pass "trust shows the Layer-2 heading (coverage-labeled)" || fail "trust missing the Layer-2 heading"
echo "${TRUST}" | grep -qF "${LIKELY_LINE}"  && pass "trust shows the 'likely resolves' annotation (basis + no-syntax-confirm)" || fail "trust missing the likely annotation"

echo "--- [B] explain Toolbar SYMBOL focus (human) ---"
EXP="$("${RMAP_BIN}" explain "Toolbar" 2>/dev/null)"
echo "${EXP}" | grep -qF "${LIKELY_LINE}"     && pass "explain SYMBOL shows the 'likely resolves' annotation" || fail "explain SYMBOL missing the likely annotation"

echo "--- [C] explain Toolbar SYMBOL focus (json) ---"
if "${RMAP_BIN}" explain "Toolbar" --json 2>/dev/null | python3 -c '
import sys, json
v = json.load(sys.stdin).get("value", {})
l = v.get("layer2_resolution") or {}
likely = l.get("likely") or [{}]
ok = (v.get("focus", {}).get("resolved_kind") == "symbol"
      and l.get("accounting") == "layer2"
      and likely[0].get("call") == "cn"
      and likely[0].get("resolves_to", {}).get("file") == "src/lib/utils.ts")
sys.exit(0 if ok else 1)
'; then pass "explain --json: symbol focus + layer2 likely cn->src/lib/utils.ts"
else fail "explain --json missing/incorrect layer2 block"; fi

echo "=================================================================="
if [[ ${FAIL} -eq 0 ]]; then echo "RESULT: PASS — the Layer-2 landing rendered on trust + explain SYMBOL."; exit 0
else echo "RESULT: FAIL — see the missing surfaces above." >&2; exit 1; fi
