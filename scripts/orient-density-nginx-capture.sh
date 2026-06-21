#!/usr/bin/env bash
# ORIENT-DENSITY-1 usefulness proof: index nginx in a FULLY ISOLATED state root
# (stdio transport + throwaway RMAP_STATE_ROOT — the operator's daemon/registry is
# never contacted) and capture `rmap orient --budget small` + `rmap orient --full`.
# Throwaway; not part of the shipped harness. Mirrors scripts/dogfood-isolated.sh.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RMAP_BIN="${REPO_ROOT}/rust/target/release/rmap"
NGINX="$(cd "${REPO_ROOT}/../legacy-codebases/nginx" && pwd -P)"

RUN_ID="orient-density-$$"
STATE_ROOT="/private/tmp/repo-graph-density/${RUN_ID}"
OUT="${STATE_ROOT}/out"
mkdir -p "${OUT}"

export RMAP_TRANSPORT="stdio"
export RMAP_STATE_ROOT="${STATE_ROOT}"
export PATH="$(dirname "${RMAP_BIN}"):${PATH}"

echo "rmap     : ${RMAP_BIN}"
echo "nginx    : ${NGINX}"
echo "state    : ${STATE_ROOT}  (stdio; isolated; operator daemon untouched)"
echo "----------------------------------------------------------------------"

echo ">>> indexing nginx (isolated) ..."
"${RMAP_BIN}" index "${NGINX}" >"${OUT}/index.txt" 2>"${OUT}/index.stderr" || {
    echo "INDEX FAILED"; tail -n 25 "${OUT}/index.stderr"; exit 1; }
grep -iE "indexed|files|symbols|nodes|edges" "${OUT}/index.txt" "${OUT}/index.stderr" | head -8 || true

cd "${NGINX}"

echo ""
echo ">>> rmap orient --budget small"
echo "======================================================================"
"${RMAP_BIN}" orient --budget small | tee "${OUT}/orient-small.txt"

echo ""
echo ">>> rmap orient --full"
echo "======================================================================"
"${RMAP_BIN}" orient --full | tee "${OUT}/orient-full.txt"

echo ""
echo "----------------------------------------------------------------------"
echo "captures in: ${OUT}"
echo "STATE_ROOT (remove when done): ${STATE_ROOT}"
