#!/usr/bin/env bash
#
# XPART-FIXTURE-STANDALONE-1: validate the cross-partition import overlay LIVE against the two-package
# fixture AS ITS OWN REPO (not the enclosing repo-graph registration).
#
# Why this exists: the fixture is committed UNDER the repo-graph repo. Repo identity is a registry keyed by
# canonical path with LONGEST-ANCESTOR-PREFIX resolution (no .git walk). Until the fixture is registered,
# the only ancestor is repo-graph, so it resolved to repo-graph's repo_uid/display_name. Registering the
# fixture (`rmap index <fixture>`) makes it a longer prefix -> it wins resolution -> its own identity. This
# harness does that and asserts the live answer belongs to the fixture, with the cross-partition cycle.
#
# Prerequisites: a running daemon with the scip-typescript producer reachable (./scripts/dev-install-local.sh).
# Re-runnable: `rmap index` / refresh are idempotent per path.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE="$REPO_ROOT/rust/crates/repo-graph-scip-ingest/tests/fixtures/xpart-monorepo"
ALIAS="xpart-monorepo"

fail() { echo "FAIL: $1" >&2; exit 1; }
ok()   { echo "  ok: $1"; }

[ -d "$FIXTURE/packages/a" ] && [ -d "$FIXTURE/packages/b" ] || fail "fixture not found at $FIXTURE"

echo "==> 1/4 register the fixture as its OWN repo (alias '$ALIAS')"
# Register the path (idempotent per path), then set the readable alias separately + best-effort: `index
# --alias` REJECTS an already-used alias, so it is not re-runnable; `repo alias` is idempotent-tolerant and
# the display_name assertion in step 3 is the real gate on identity.
rmap index "$FIXTURE" >/dev/null || fail "rmap index <fixture> failed"
rmap repo alias "$FIXTURE" "$ALIAS" >/dev/null 2>&1 || true
ok "registered $FIXTURE as its own repo (alias '$ALIAS')"

echo "==> 2/4 multi-partition livegraph refresh (repeated --source-root, absolute)"
REFRESH="$(rmap dev livegraph-refresh --repo "$FIXTURE" \
  --source-root "$FIXTURE/packages/a" --source-root "$FIXTURE/packages/b")"
echo "$REFRESH" | grep -q '"status": "AllRefreshed"' || fail "refresh not AllRefreshed:
$REFRESH"
ok "both partitions refreshed (AllRefreshed)"

echo "==> 3/4 file-import cycles (JSON): fixture identity + cross-partition overlay"
JSON="$(cd "$FIXTURE" && rmap cycles --engine livegraph --kind file-import --json)"
echo "$JSON" | grep -q "\"display_name\": \"$ALIAS\"" \
  || fail "display_name is not '$ALIAS' (identity did not flip to the fixture):
$JSON"
if echo "$JSON" | grep -q '"display_name": "repo-graph"'; then
  fail "identity leaked to the enclosing repo-graph repo"
fi
ok "identity is the fixture ('$ALIAS'), NOT repo-graph"
echo "$JSON" | grep -q '"cross_partition": true'  || fail "cross_partition is not true"
echo "$JSON" | grep -q '"xpart_edge_count": 2'    || fail "xpart_edge_count is not 2"
echo "$JSON" | grep -q 'packages/a/src/main.ts:FILE' || fail "missing packages/a/src/main.ts FILE node"
echo "$JSON" | grep -q 'packages/b/src/foo.ts:FILE'  || fail "missing packages/b/src/foo.ts FILE node"
ok "cross-partition cycle preserved (a/main <-> b/foo; overlay edges=2; cross_partition=true)"

echo "==> 4/4 file-import cycles (human): FILE vocabulary, never 'module'"
HUMAN="$(cd "$FIXTURE" && rmap cycles --engine livegraph --kind file-import)"
echo "$HUMAN" | grep -q "FILE import cycle" || fail "human render is not FILE-import:
$HUMAN"
if echo "$HUMAN" | grep -qi "module"; then
  fail "human render says 'module' (must be file-import only):
$HUMAN"
fi
ok "human render says FILE import cycles, no 'module'"

echo
echo "PASS: the cross-partition fixture validates as a STANDALONE repo ('$ALIAS'); overlay cycle confirmed."
