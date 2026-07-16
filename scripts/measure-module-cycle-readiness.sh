#!/usr/bin/env bash
#
# MODULE-CYCLES-DEFAULT-READINESS-1: measure the module-cycle compare across a real repo set and emit a
# `rmap cycles` default-migration readiness verdict (GREEN / YELLOW / RED). MEASUREMENT ONLY -- no default
# flip, no resolver logic, no decommission. Evidence law: every repo is EXECUTED or NOT RUN (with reason);
# no inferred histograms.
#
# Prerequisites: a running daemon with the scip-typescript producer (./scripts/dev-install-local.sh).

set -o pipefail   # NOT -e: a per-repo failure must record NOT RUN, not abort the run. NOT -u: bash 3.2
                  # errors on empty-array expansion (a repo with no --source-root is intentional, e.g. B).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BASE="$(cd "$REPO_ROOT/.." && pwd)"
FIXTURE="$REPO_ROOT/rust/crates/repo-graph-scip-ingest/tests/fixtures/xpart-monorepo"

# Global accumulators for the verdict (over EXECUTED repos).
G_MISSING=0; G_UNKNOWN=0; G_EXTRA=0; G_IDENTITY=0; G_EXECUTED=0; A_OK=0

num() { grep -o "\"$2\": [0-9]*" <<<"$1" | grep -o '[0-9]*' | head -1; }
cls() { grep -c "\"divergence\": \"$2\"" <<<"$1"; }

# measure_repo <label> <repo_path> <is_control_A:0|1> <root...>
measure_repo() {
  local label="$1" repo="$2" isA="$3"; shift 3
  local roots=("$@")
  echo "── $label  ($repo)"
  if [ ! -d "$repo" ]; then echo "  NOT RUN: repo absent"; return; fi
  if ! rmap index "$repo" >/dev/null 2>&1; then echo "  NOT RUN: rmap index failed"; return; fi

  local sr=(); local r; for r in "${roots[@]}"; do sr+=(--source-root "$r"); done
  local refresh; refresh="$(rmap dev livegraph-refresh --repo "$repo" "${sr[@]}" 2>&1)"
  local total loaded
  total="$(num "$refresh" total)"; loaded="$(num "$refresh" succeeded)"
  [ -z "$total" ] && total=0; [ -z "$loaded" ] && loaded=0

  local cmp; cmp="$(cd "$repo" && rmap cycles --engine compare --kind module-import --json 2>&1)"
  if ! grep -q '"livegraph_module_compare"' <<<"$cmp"; then
    echo "  NOT RUN: compare produced no report (first 1 line): $(head -1 <<<"$cmp")"; return
  fi
  local sqlite lg matched subset pkg dyn static unloaded identity unknown extra
  sqlite="$(num "$cmp" sqlite_count)"; lg="$(num "$cmp" livegraph_count)"; matched="$(num "$cmp" matched)"
  subset="$(grep -o '"livegraph_subset": [a-z]*' <<<"$cmp" | awk '{print $2}' | head -1)"
  pkg=$(cls "$cmp" MissingDueToPackageExternal);  dyn=$(cls "$cmp" MissingDueToDynamicImport)
  static=$(cls "$cmp" MissingDueToStaticUnresolved); unloaded=$(cls "$cmp" MissingDueToUnloadedOrNonTsPartition)
  identity=$(cls "$cmp" ModuleIdentityMismatch); unknown=$(cls "$cmp" UnknownDivergence)
  extra=$(cls "$cmp" UnexpectedExtraInLiveGraph)
  local missing=$(( pkg + dyn + static + unloaded + identity + unknown ))

  local cov xpart
  cov="$(cd "$repo" && rmap cycles --engine livegraph --kind module-import --json 2>&1)"
  xpart="$(num "$cov" xpart_edge_count)"; [ -z "$xpart" ] && xpart=0
  local sidecar; sidecar="$(grep -o '"livegraph_module_compare_sidecar": "[^"]*"' <<<"$cmp" | sed 's/.*: "//; s/"$//')"

  echo "  EXECUTED: sqlite=$sqlite lg=$lg matched=$matched subset=$subset"
  echo "  missing=$missing  [pkg=$pkg dyn=$dyn static=$static unloaded=$unloaded identity=$identity unknown=$unknown]  extra=$extra"
  echo "  coverage: partitions=$total loaded=$loaded xpart_edges=$xpart"
  echo "  sidecar: ${sidecar:-<none>}"

  G_EXECUTED=$(( G_EXECUTED + 1 ))
  G_MISSING=$(( G_MISSING + missing )); G_UNKNOWN=$(( G_UNKNOWN + unknown ))
  G_EXTRA=$(( G_EXTRA + extra )); G_IDENTITY=$(( G_IDENTITY + identity ))
  if [ "$isA" = "1" ]; then
    if [ "$matched" -ge 1 ] && [ "$missing" -eq 0 ] && [ "$extra" -eq 0 ]; then A_OK=1
    else echo "  *** CONTROL A NOT EXACT -> run INVALIDATED (measurement bug)"; fi
  fi
}

echo "=== MODULE-CYCLES-DEFAULT-READINESS-1 measurement ==="
# A — hard control (must be exact). Both fixture partitions.
rmap repo alias "$FIXTURE" xpart-monorepo >/dev/null 2>&1 || true
measure_repo "A xpart-monorepo (HARD control)" "$FIXTURE" 1 "$FIXTURE/packages/a" "$FIXTURE/packages/b"

# B — HISTORICAL: when this readiness measurement was taken, repo-graph was a MIXED Rust+TS repo (it still
# carried the TypeScript prototype under src/, since retired by TS-PROTOTYPE-RETIREMENT-1). At that time B
# refreshed its DISCOVERED in-repo TS roots (excluding the xpart-monorepo fixture) -> the TS module cycles
# matched, the Rust/non-TS cycles showed as MissingDueToUnloadedOrNonTsPartition (the language gap); a
# boundary observation that did NOT invalidate the run. Post-retirement the src/ TS tree is gone, so a
# re-run discovers no in-repo TS roots here.
broots=()
while IFS= read -r d; do [ -n "$d" ] && broots+=("$d"); done < <(find "$REPO_ROOT" -maxdepth 4 -name tsconfig.json -not -path '*/node_modules/*' -not -path '*xpart-monorepo*' -exec dirname {} \; 2>/dev/null | sort -u)
measure_repo "B repo-graph (MIXED Rust+TS; TS portion)" "$REPO_ROOT" 0 "${broots[@]}"

# C — real TS repos: amodx, hexmanos, zap-engine (ALL viable run, for a fuller histogram).
for cand in amodx hexmanos zap-engine; do
  repo="$BASE/$cand"
  [ -d "$repo" ] || continue
  troots=() # bash 3.2 has no mapfile; read the discovered tsconfig dirs portably.
  while IFS= read -r d; do [ -n "$d" ] && troots+=("$d"); done < <(find "$repo" -maxdepth 3 -name tsconfig.json -not -path '*/node_modules/*' -exec dirname {} \; 2>/dev/null | sort -u)
  [ "${#troots[@]}" -eq 0 ] && { echo "── C $cand: NOT RUN (no tsconfig found)"; continue; }
  measure_repo "C $cand (REAL TS)" "$repo" 0 "${troots[@]}"
done

echo "── VERDICT (over $G_EXECUTED EXECUTED repos)"
echo "   totals: missing=$G_MISSING unknown=$G_UNKNOWN extra=$G_EXTRA identity=$G_IDENTITY  controlA_exact=$A_OK"
if [ "$A_OK" != "1" ]; then echo "   RESULT: INVALID (control A not exact)"; exit 2; fi
if [ "$G_UNKNOWN" -gt 0 ] || [ "$G_EXTRA" -gt 0 ] || [ "$G_IDENTITY" -gt 0 ]; then
  echo "   RESULT: RED (unknown/extra/identity present -> not migratable)"
elif [ "$G_MISSING" -eq 0 ]; then
  echo "   RESULT: GREEN (no divergence across the run set)"
else
  echo "   RESULT: YELLOW (missing only in explainable/degradable classes -> labeled-degradation candidate)"
fi
