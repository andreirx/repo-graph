#!/usr/bin/env bash
#
# MODULE-CYCLES-CLI-1 / MODULE-AGGREGATION-1 (D5): LIVE equivalence check of LiveGraph-derived MODULE
# cycles vs SQLite `rmap cycles` on the xpart-monorepo fixture, driving the REAL CLI surface
# (`--engine livegraph|compare --kind module-import`).
#
# Fixture expectation: EXACT equivalence -> the compare report is empty (matched=1, missing=0, extra=0,
# livegraph_subset=true). On a real repo the LiveGraph is expected to be a SUBSET of SQLite (missing cycles
# classed UnknownDivergence this slice); an EXTRA LiveGraph cycle (livegraph_subset=false) is an overclaim
# and MUST fail. The authoritative unit proof is repo-graph-livegraph module_cycle_uses_xpart_overlay; this
# is the live regression guard.
#
# Prerequisites: a running daemon with the scip-typescript producer (./scripts/dev-install-local.sh).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE="$REPO_ROOT/rust/crates/repo-graph-scip-ingest/tests/fixtures/xpart-monorepo"
ALIAS="xpart-monorepo"

fail() { echo "FAIL: $1" >&2; exit 1; }
ok()   { echo "  ok: $1"; }
# Cycle-node "name" values from a cycles JSON (one per line; matches `"name":`, not `"display_name":`).
names() { grep -o '"name": "[^"]*"' | sed 's/.*: "//; s/"$//'; }

[ -d "$FIXTURE/packages/a" ] || fail "fixture not found at $FIXTURE"

echo "==> 1/4 register + refresh the fixture (its own repo identity)"
rmap index "$FIXTURE" >/dev/null || fail "rmap index <fixture> failed"
rmap repo alias "$FIXTURE" "$ALIAS" >/dev/null 2>&1 || true
rmap dev livegraph-refresh --repo "$FIXTURE" \
  --source-root "$FIXTURE/packages/a" --source-root "$FIXTURE/packages/b" \
  | grep -q '"status": "AllRefreshed"' || fail "refresh not AllRefreshed"
ok "fixture registered + both partitions refreshed"

echo "==> 2/4 LiveGraph MODULE cycles (--engine livegraph --kind module-import)"
LG="$(cd "$FIXTURE" && rmap cycles --engine livegraph --kind module-import --json)"
echo "$LG" | grep -q '"backend_used": "livegraph"' || fail "backend not livegraph:
$LG"
echo "$LG" | grep -q '"kind": "module-import"'     || fail "kind not module-import"
echo "$LG" | grep -q '"aggregation_basis": "dirname"' || fail "aggregation_basis not dirname"
LG_MODS="$(printf '%s' "$LG" | names | sort -u | tr '\n' ',')"
[ "$LG_MODS" = "packages/a/src,packages/b/src," ] \
  || fail "LiveGraph module cycle members != {packages/a/src,packages/b/src}: '$LG_MODS'"
ok "LiveGraph MODULE cycle = {packages/a/src, packages/b/src} (module paths; aggregation=dirname)"

echo "==> 3/4 compare vs SQLite (--engine compare --kind module-import): fixture EXACT"
CMP="$(cd "$FIXTURE" && rmap cycles --engine compare --kind module-import --json)"
echo "$CMP" | grep -q '"backend_used": "sqlite"' || fail "compare primary must be sqlite:
$CMP"
echo "$CMP" | grep -q '"livegraph_subset": true' || fail "livegraph_subset must be true (no extra cycle)"
echo "$CMP" | grep -q '"matched": 1'             || fail "matched must be 1 on the fixture"
echo "$CMP" | grep -q '"missing_in_livegraph": \[\]' || fail "missing_in_livegraph must be EMPTY (exact)"
echo "$CMP" | grep -q '"extra_in_livegraph": \[\]'   || fail "extra_in_livegraph must be EMPTY (no overclaim)"
echo "$CMP" | grep -q '"livegraph_module_compare_sidecar"' || fail "compare sidecar path missing"
ok "compare EXACT: 1 matched, 0 missing, 0 extra, livegraph_subset=true, sidecar written"

echo "==> 4/4 default SQLite + --engine sqlite --kind module-import UNCHANGED (D6)"
DEF="$(cd "$FIXTURE" && rmap cycles)"
echo "$DEF" | grep -q "module-level cycle"        || fail "default lost MODULE vocabulary:
$DEF"
EXP="$(cd "$FIXTURE" && rmap cycles --engine sqlite --kind module-import)"
[ "$DEF" = "$EXP" ] || fail "--engine sqlite --kind module-import differs from the default (D6)"
ok "default == --engine sqlite --kind module-import (SQLite MODULE; unchanged)"

echo
echo "PASS: LiveGraph MODULE cycles == SQLite on the fixture (compare empty); default SQLite unchanged."
