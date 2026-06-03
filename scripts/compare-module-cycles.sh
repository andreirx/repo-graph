#!/usr/bin/env bash
#
# MODULE-AGGREGATION-1 (D5): LIVE equivalence cross-check of LiveGraph-derived MODULE cycles vs SQLite
# `rmap cycles` on the xpart-monorepo fixture.
#
# There is intentionally NO CLI for LiveGraph module cycles yet (the slice is headless). So this harness
# corroborates the equivalence live WITHOUT a new surface: it dirname-aggregates the LiveGraph FILE-import
# cycle (existing `--engine livegraph --kind file-import`) and asserts the resulting module paths match
# SQLite's MODULE cycle. The AUTHORITATIVE equivalence is the Rust unit test
# (`module_cycle_uses_xpart_overlay`, which asserts the LiveGraph module cycle == {packages/a/src,
# packages/b/src}); this script is the live regression guard. The real-repo divergence comparison is
# deferred to MODULE-CYCLES-CLI-1 (it needs a surface to dump LiveGraph module cycles for an arbitrary repo).
#
# Prerequisites: a running daemon with the scip-typescript producer (./scripts/dev-install-local.sh).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE="$REPO_ROOT/rust/crates/repo-graph-scip-ingest/tests/fixtures/xpart-monorepo"
ALIAS="xpart-monorepo"

fail() { echo "FAIL: $1" >&2; exit 1; }
ok()   { echo "  ok: $1"; }

# Extract the cycle-node "name" values from a cycles JSON (one per line). Matches only `"name":` (NOT
# `"display_name":`, whose preceding char is `_`, not `"`).
names() { grep -o '"name": "[^"]*"' | sed 's/.*: "//; s/"$//'; }

[ -d "$FIXTURE/packages/a" ] || fail "fixture not found at $FIXTURE"

echo "==> 1/3 register + refresh the fixture (its own repo identity)"
rmap index "$FIXTURE" >/dev/null || fail "rmap index <fixture> failed"
rmap repo alias "$FIXTURE" "$ALIAS" >/dev/null 2>&1 || true
rmap dev livegraph-refresh --repo "$FIXTURE" \
  --source-root "$FIXTURE/packages/a" --source-root "$FIXTURE/packages/b" \
  | grep -q '"status": "AllRefreshed"' || fail "refresh not AllRefreshed"
ok "fixture registered + both partitions refreshed"

echo "==> 2/3 SQLite MODULE cycle (rmap cycles)"
SQ="$(cd "$FIXTURE" && rmap cycles --json)"
echo "$SQ" | grep -q '"count": 1' || fail "SQLite module cycle count != 1:
$SQ"
SQ_NAMES="$(printf '%s' "$SQ" | names | sort | tr '\n' ',')"
[ "$SQ_NAMES" = "src,src," ] || fail "SQLite module members != {src,src}: '$SQ_NAMES'"
ok "SQLite: 1 module cycle of 2 modules (both short-name 'src')"

echo "==> 3/3 dirname-aggregate the LiveGraph FILE cycle -> compare module paths"
FI="$(cd "$FIXTURE" && rmap cycles --engine livegraph --kind file-import --json)"
echo "$FI" | grep -q '"count": 1' || fail "LiveGraph file cycle count != 1:
$FI"
# Each FILE cycle member name is a repo-relative path; dirname (strip /lastcomponent) -> module path; dedup.
LG_MODS="$(printf '%s' "$FI" | names | sed 's:/[^/]*$::' | sort -u | tr '\n' ',')"
[ "$LG_MODS" = "packages/a/src,packages/b/src," ] \
  || fail "dirname-aggregated FILE modules != {packages/a/src,packages/b/src}: '$LG_MODS'"
# Short-name equivalence with SQLite (both 'src'): the module SHORT name is the dirname's last component.
LG_SHORT="$(printf '%s' "$FI" | names | sed 's:/[^/]*$::; s:.*/::' | sort | tr '\n' ',')"
[ "$LG_SHORT" = "$SQ_NAMES" ] \
  || fail "module short-names differ: LiveGraph='$LG_SHORT' SQLite='$SQ_NAMES'"
ok "LiveGraph FILE cycle dirname-aggregates to {packages/a/src, packages/b/src}; short-names match SQLite"

echo
echo "PASS: on the fixture, the dirname-aggregated LiveGraph FILE cycle == the SQLite MODULE cycle."
echo "      (authoritative equivalence: Rust module_cycle_uses_xpart_overlay; real-repo compare: MODULE-CYCLES-CLI-1)"
