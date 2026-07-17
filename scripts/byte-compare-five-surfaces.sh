#!/usr/bin/env bash
#
# byte-compare-five-surfaces.sh — EC-M3B-TRUST-AGG-1 §4 binding validation.
#
# Byte-compares the FIVE trust-core surfaces (trust, check, orient, explain,
# stats; human AND --json) between a BASELINE (pre-M-3b) rmap binary and the
# CANDIDATE (working-tree) binary, on the same fixture, in fully isolated
# state roots (the dogfood-isolated.sh isolation levers: RMAP_TRANSPORT=stdio
# + RMAP_STATE_ROOT under /private/tmp — the operator's daemon/registry is
# never touched).
#
# Three comparisons:
#   A vs B  — baseline binary on a baseline-indexed root VS candidate binary
#             on a candidate-indexed root (write path + read path together).
#             Volatile identity tokens (snapshot uid embeds the index-run
#             wall-clock; "indexed …" freshness phrasing) are normalized on
#             BOTH sides before diffing — every normalization is explicit in
#             normalize() below; everything else must be byte-identical.
#   A vs C  — baseline outputs VS candidate binary serving a COPY of the
#             baseline-indexed root (a REAL pre-migration DB: migration 030
#             runs on open, the aggregate columns stay NULL, the labeled
#             live-COUNT fallback serves). SAME snapshot, SAME rows — diffed
#             RAW, no normalization: any byte difference is attributable to
#             the code change.
#   exit    — nonzero on ANY unexplained difference; diffs retained.
#
# Usage:
#   BASELINE_RMAP=/path/to/old/rmap CANDIDATE_RMAP=/path/to/new/rmap \
#     ./scripts/byte-compare-five-surfaces.sh
# (rmapd must sit beside each rmap; that sibling is what stdio spawns.)

set -euo pipefail

BASELINE_RMAP="${BASELINE_RMAP:?set BASELINE_RMAP to the pre-slice rmap binary}"
CANDIDATE_RMAP="${CANDIDATE_RMAP:?set CANDIDATE_RMAP to the working-tree rmap binary}"

for bin in "$BASELINE_RMAP" "$CANDIDATE_RMAP"; do
    [[ -x "$bin" ]] || { echo "error: not executable: $bin" >&2; exit 1; }
    [[ -x "$(dirname "$bin")/rmapd" ]] || { echo "error: rmapd missing beside $bin" >&2; exit 1; }
done

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
WORK="/private/tmp/rg-m3b-bytecompare/${RUN_ID}"
FIXTURE="${WORK}/fixture"
mkdir -p "${FIXTURE}/src" "${WORK}/out-A" "${WORK}/out-B" "${WORK}/out-C" "${WORK}/diffs"

echo "== byte-compare-five-surfaces =="
echo "baseline  : ${BASELINE_RMAP} ($("${BASELINE_RMAP}" --version 2>&1 || true))"
echo "candidate : ${CANDIDATE_RMAP} ($("${CANDIDATE_RMAP}" --version 2>&1 || true))"
echo "workdir   : ${WORK}"

# ── Fixture: import + call graph, same shape as the dogfood fixture ──────────
cat > "${FIXTURE}/package.json" <<'JSON'
{ "name": "m3b-bytecompare-fixture", "version": "0.0.0", "private": true }
JSON
cat > "${FIXTURE}/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2020", "module": "commonjs", "strict": true }, "include": ["src"] }
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

# ── Surface runner: index once, then the 5 surfaces, human + --json ──────────
run_surfaces() { # $1=rmap bin  $2=state root  $3=out dir  $4=index? (yes|no)
    local bin="$1" root="$2" out="$3" do_index="$4"
    local bindir; bindir="$(cd "$(dirname "$bin")" && pwd)"
    (
        export RMAP_TRANSPORT="stdio"
        export RMAP_STATE_ROOT="$root"
        export PATH="${bindir}:${PATH}"
        if [[ "$do_index" == "yes" ]]; then
            "$bin" index "$FIXTURE" > "${out}/index.txt" 2> "${out}/index.stderr"
        fi
        cd "$FIXTURE"
        "$bin" trust                 > "${out}/trust.txt"        2> "${out}/trust.stderr"
        "$bin" trust --json          > "${out}/trust.json"       2> "${out}/trust-json.stderr"
        "$bin" check                 > "${out}/check.txt"        2> "${out}/check.stderr"
        "$bin" check --json          > "${out}/check.json"       2> "${out}/check-json.stderr"
        "$bin" orient                > "${out}/orient.txt"       2> "${out}/orient.stderr"
        "$bin" orient --json         > "${out}/orient.json"      2> "${out}/orient-json.stderr"
        "$bin" explain src/main.ts   > "${out}/explain.txt"      2> "${out}/explain.stderr"
        "$bin" explain src/main.ts --json > "${out}/explain.json" 2> "${out}/explain-json.stderr"
        "$bin" stats                 > "${out}/stats.txt"        2> "${out}/stats.stderr"
        "$bin" stats --json          > "${out}/stats.json"       2> "${out}/stats-json.stderr"
    )
}

echo ""
echo "-- Phase A: BASELINE binary, baseline-indexed root"
run_surfaces "$BASELINE_RMAP" "${WORK}/state-A" "${WORK}/out-A" yes

echo "-- Phase C prep: copy state-A (pre-migration DB) BEFORE any candidate open"
cp -R "${WORK}/state-A" "${WORK}/state-C"
# The registry must not point back into state-A (that would mutate the
# original). Fail loudly if it embeds the state-A path.
if grep -q "state-A" "${WORK}/state-C/registry.json" 2>/dev/null; then
    sed -i '' "s|${WORK}/state-A|${WORK}/state-C|g" "${WORK}/state-C/registry.json"
    echo "   (registry.json contained absolute state paths — rewritten to state-C)"
fi

echo "-- Phase B: CANDIDATE binary, candidate-indexed root"
run_surfaces "$CANDIDATE_RMAP" "${WORK}/state-B" "${WORK}/out-B" yes

echo "-- Phase C: CANDIDATE binary serving the PRE-MIGRATION copy (fallback path)"
run_surfaces "$CANDIDATE_RMAP" "${WORK}/state-C" "${WORK}/out-C" no

# ── Normalization for A-vs-B (two DIFFERENT index runs) ──────────────────────
# ONLY provably run-volatile identity tokens; each rule justified:
#  1. snapshot uids embed the index-run ISO timestamp + a random uuid slice
#     (crud/snapshots.rs create_snapshot: "<repo>/<ISO>/<uuid8>").
#  2. ISO timestamps (created_at etc.) differ between the two index runs.
#  3. "Xs ago" / "Xm ago"-style freshness phrasing tracks wall clock.
#  4. repo uids are per-`rmap index` random ULIDs ("repo_" + 26 chars;
#     the human trust header renders a 12-char truncation + "...").
#  5. the ellipsized snapshot-uid tail ("Snapshot …<uuid8>") renders the
#     same random uuid slice as rule 1 in truncated human form.
normalize() { # $1=in  $2=out
    sed -E \
        -e 's|[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z?|<TS>|g' \
        -e 's|<TS>/[0-9a-f]{8}|<TS>/<UID8>|g' \
        -e 's|repo_[0-9a-z]{26}|<REPO>|g' \
        -e 's|repo_[0-9a-z]{12}\.\.\.|<REPO>...|g' \
        -e 's|…[0-9a-f]{8}|…<UID8>|g' \
        -e 's|[0-9]+(\.[0-9]+)?(ms\|s\|m\|h)( ago)?|<DUR>\3|g' \
        "$1" > "$2"
}

FAIL=0
compare() { # $1=left dir  $2=right dir  $3=label  $4=normalize? (yes|no)
    local left="$1" right="$2" label="$3" norm="$4" f base l r
    for f in trust.txt trust.json check.txt check.json orient.txt orient.json \
             explain.txt explain.json stats.txt stats.json; do
        base="${f}"
        l="${left}/${f}"; r="${right}/${f}"
        if [[ "$norm" == "yes" ]]; then
            normalize "$l" "${WORK}/diffs/${label}-L-${base}"
            normalize "$r" "${WORK}/diffs/${label}-R-${base}"
            l="${WORK}/diffs/${label}-L-${base}"; r="${WORK}/diffs/${label}-R-${base}"
        fi
        if diff -u "$l" "$r" > "${WORK}/diffs/${label}-${base}.diff" 2>&1; then
            rm -f "${WORK}/diffs/${label}-${base}.diff"
            echo "   PASS ${label} ${base}"
        else
            echo "   FAIL ${label} ${base} — see ${WORK}/diffs/${label}-${base}.diff"
            FAIL=1
        fi
    done
}

echo ""
echo "-- Compare A vs B (baseline-indexed/baseline-served vs candidate-indexed/candidate-served; normalized)"
compare "${WORK}/out-A" "${WORK}/out-B" "AvsB" yes

echo ""
echo "-- Compare A vs C (SAME pre-migration DB: baseline binary vs candidate binary; RAW bytes)"
compare "${WORK}/out-A" "${WORK}/out-C" "AvsC" no

echo ""
if [[ "$FAIL" == "0" ]]; then
    echo "OK — all five surfaces byte-identical (A vs C raw; A vs B modulo the explicit volatile-token rules in normalize())."
    echo "outputs: ${WORK}/out-{A,B,C}"
else
    echo "FAILED — differences retained under ${WORK}/diffs/" >&2
    exit 1
fi
