#!/usr/bin/env bash
#
# byte-compare-map-modules-surfaces.sh — EC-M3A-AGG-REHOME-1 §4 binding
# validation (sibling of byte-compare-five-surfaces.sh, M-3b's harness —
# same isolation + comparison pattern, this slice's three surfaces).
#
# Byte-compares the THREE surfaces the M-3a read swaps touch —
# modules list (dead rollup), modules show (dead rollup), map (dep
# sketch) — human AND --json — between a BASELINE (pre-M-3a) rmap binary
# and the CANDIDATE (working-tree) binary, on the same fixture, in fully
# isolated state roots (RMAP_TRANSPORT=stdio + RMAP_STATE_ROOT under
# /private/tmp — the operator's daemon/registry is never touched).
#
# The fixture is deliberately g2/g3-discriminating (review-0 item 1):
#
#   g2 — `helper` and `fmt` have CALLS fan-in as their ONLY liveness
#        evidence (IMPORTS edges target FILE nodes, never symbols): a
#        broken degree swap flips them dead and changes both dead
#        rollups (dead_symbol_count 3 → 5).
#   g3 — the (src/main.ts → src/util.ts) file pair is CALLS-ONLY:
#        main.ts and util.ts are SCRIPT-context TS (no import/export —
#        the legacy global-scope shape), so NO IMPORTS edge exists for
#        that pair anywhere; the call resolves cross-file by unambiguous
#        name (resolver.rs bare-name lookup). The rendered dependency
#        sketch (`## Dependencies` in src/MAP.md) is the ONLY surface
#        that renders CALLS targets, so its `- src/util.ts` line exists
#        IF AND ONLY IF the CALLS share serves — a broken pair swap
#        deletes the line (per-file `## Imports` lists render only the
#        IMPORTS share and cannot resupply it; asserted below by
#        occurrence count). The (src/extra.ts → src/format.ts) pair
#        keeps both shares live (IMPORTS owner-read + CALLS), covering
#        the union branch's two sub-selects and their total order.
#
# Three comparisons:
#   A vs B  — baseline binary on a baseline-indexed root VS candidate
#             binary on a candidate-indexed root (write path + read path
#             together). Volatile identity tokens normalized on BOTH
#             sides; every normalization is explicit in normalize().
#   A vs C  — baseline outputs VS candidate binary serving a COPY of the
#             baseline-indexed root (a REAL pre-migration DB: migration
#             031 runs on open, the family tables stay empty, the
#             markers stay NULL, the labeled live-derived fallback
#             serves). SAME snapshot, SAME rows — diffed RAW, no
#             normalization: any byte difference is attributable to the
#             code change.
#   exit    — nonzero on ANY unexplained difference; diffs retained.
#
# Usage:
#   BASELINE_RMAP=/path/to/old/rmap CANDIDATE_RMAP=/path/to/new/rmap \
#     ./scripts/byte-compare-map-modules-surfaces.sh
# (rmapd must sit beside each rmap; that sibling is what stdio spawns.)

set -euo pipefail

BASELINE_RMAP="${BASELINE_RMAP:?set BASELINE_RMAP to the pre-slice rmap binary}"
CANDIDATE_RMAP="${CANDIDATE_RMAP:?set CANDIDATE_RMAP to the working-tree rmap binary}"

for bin in "$BASELINE_RMAP" "$CANDIDATE_RMAP"; do
    [[ -x "$bin" ]] || { echo "error: not executable: $bin" >&2; exit 1; }
    [[ -x "$(dirname "$bin")/rmapd" ]] || { echo "error: rmapd missing beside $bin" >&2; exit 1; }
done

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
WORK="/private/tmp/rg-m3a-bytecompare/${RUN_ID}"
FIXTURE="${WORK}/fixture"
mkdir -p "${FIXTURE}/src" "${WORK}/out-A" "${WORK}/out-B" "${WORK}/out-C" "${WORK}/diffs"

echo "== byte-compare-map-modules-surfaces =="
echo "baseline  : ${BASELINE_RMAP} ($("${BASELINE_RMAP}" --version 2>&1 || true))"
echo "candidate : ${CANDIDATE_RMAP} ($("${CANDIDATE_RMAP}" --version 2>&1 || true))"
echo "workdir   : ${WORK}"

# ── Fixture: cross-file call graph, g2/g3-discriminating ─────────────────────
cat > "${FIXTURE}/package.json" <<'JSON'
{ "name": "m3a-bytecompare-fixture", "version": "0.0.0", "private": true }
JSON
cat > "${FIXTURE}/tsconfig.json" <<'JSON'
{ "compilerOptions": { "target": "ES2020", "module": "commonjs", "strict": true }, "include": ["src"] }
JSON
# SCRIPT-context pair (no import/export): the CALLS-only file pair.
cat > "${FIXTURE}/src/util.ts" <<'TS'
function helper(): number {
  return 1;
}

function lonely(): number {
  return helper() ? 2 : 3;
}
TS
cat > "${FIXTURE}/src/main.ts" <<'TS'
function main(): number {
  return helper() + helper();
}
TS
# MODULE-context pair: IMPORTS owner-read + CALLS share on the SAME pair.
cat > "${FIXTURE}/src/format.ts" <<'TS'
export function fmt(): string {
  return "x";
}
TS
cat > "${FIXTURE}/src/extra.ts" <<'TS'
import { fmt } from "./format";

export function extra(): string {
  return fmt();
}
TS

# ── Surface runner: index once, then the 3 surfaces, human + --json ──────────
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
        "$bin" modules list                > "${out}/modules-list.txt"  2> "${out}/modules-list.stderr"
        "$bin" modules list --json         > "${out}/modules-list.json" 2> "${out}/modules-list-json.stderr"
        "$bin" modules show .              > "${out}/modules-show.txt"  2> "${out}/modules-show.stderr"
        "$bin" modules show . --json       > "${out}/modules-show.json" 2> "${out}/modules-show-json.stderr"
        # --dry-run: maps print to stdout; the fixture tree is never written.
        "$bin" map src --dry-run           > "${out}/map.txt"           2> "${out}/map.stderr"
        "$bin" map src --dry-run --json    > "${out}/map.json"          2> "${out}/map-json.stderr"
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

# ── Non-vacuity guards (review-0 item 1) ─────────────────────────────────────
# Run against BOTH sides: out-A proves the fixture GENERATES the
# discriminating facts under the baseline's live row-derived reads;
# out-B proves the candidate's PERSISTED families actually serve them
# (a broken g2 swap changes the dead rollups; a broken g3 swap deletes
# the sketch line — either fails here, before any diff runs).
guard() { # $1=out dir  $2=side label
    local out="$1" side="$2"

    # g2 — dead rollup: exactly lonely/main/extra are dead (3). helper
    # and fmt are alive ONLY through CALLS fan-in (IMPORTS edges target
    # FILE nodes, never symbols; fan-out confers no liveness) — a broken
    # degree swap flips them dead (5) or resurrects the dead (<3).
    if ! grep -q '"dead_symbol_count": 3' "${out}/modules-list.json"; then
        echo "FIXTURE GUARD FAILED (${side}): expected dead_symbol_count 3 (lonely+main+extra; helper/fmt alive via CALLS only)" >&2
        grep -o '"dead_symbol_count": [0-9]*' "${out}/modules-list.json" >&2 || true
        exit 1
    fi

    # g3 — the CALLS-only pair REACHES RENDERED MAP OUTPUT: src/MAP.md's
    # dependency sketch must carry `- src/util.ts`, and that exact line
    # must occur EXACTLY ONCE in the whole rendered stream. No per-file
    # `## Imports` list can resupply it (main.ts/util.ts are script
    # files — no IMPORTS edge for the pair exists), so occurrence 1 is
    # attributable to the CALLS share alone; a broken pair swap drops it
    # to 0. This is the assertion that the CALLS-only dependency affects
    # rendered map bytes.
    local util_lines
    util_lines="$(grep -c '^- src/util\.ts$' "${out}/map.txt" || true)"
    if [[ "${util_lines}" != "1" ]]; then
        echo "FIXTURE GUARD FAILED (${side}): expected exactly 1 '- src/util.ts' sketch line in map.txt, got ${util_lines} — the CALLS-only pair is not reaching rendered output" >&2
        exit 1
    fi

    # IMPORTS share — the owner-read still serves beside the persisted
    # CALLS share: src/format.ts renders in BOTH the sketch union and
    # extra.ts's per-file Imports list (2 occurrences).
    local fmt_lines
    fmt_lines="$(grep -c '^- src/format\.ts$' "${out}/map.txt" || true)"
    if [[ "${fmt_lines}" != "2" ]]; then
        echo "FIXTURE GUARD FAILED (${side}): expected 2 '- src/format.ts' lines (sketch + per-file Imports), got ${fmt_lines}" >&2
        exit 1
    fi
}
guard "${WORK}/out-A" "A/baseline"
guard "${WORK}/out-B" "B/candidate"

# ── Normalization for A-vs-B (two DIFFERENT index runs) ──────────────────────
# ONLY provably run-volatile identity tokens; each rule justified:
#  1. snapshot uids embed the index-run ISO timestamp + a random uuid slice
#     (crud/snapshots.rs create_snapshot: "<repo>/<ISO>/<uuid8>").
#  2. ISO timestamps (created_at etc.) differ between the two index runs.
#  3. "Xs ago" / "Xm ago"-style freshness phrasing tracks wall clock.
#  4. repo uids are per-`rmap index` random ULIDs ("repo_" + 26 chars).
#  5. the ellipsized snapshot-uid tail ("…<uuid8>") renders the same random
#     uuid slice as rule 1 in truncated human form.
#  6. map artifacts stamp "snapshot <uuid8>" (bare 8-hex slice of rule 1's
#     uuid — map.rs banner + modules_show snapshot line).
#  7. module-candidate uids are SHA256 slices computed OVER the per-run
#     repo ULID ("<kind>-mod-" + 16 hex; generate_module_uid — OBSERVED:
#     package_json.rs:222-235, inferred_modules.rs:842-853) — rule 4's
#     volatility in hashed form, rendered by the modules JSON surfaces.
normalize() { # $1=in  $2=out
    sed -E \
        -e 's|[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z?|<TS>|g' \
        -e 's|<TS>/[0-9a-f]{8}|<TS>/<UID8>|g' \
        -e 's|repo_[0-9a-z]{26}|<REPO>|g' \
        -e 's|repo_[0-9a-z]{12}\.\.\.|<REPO>...|g' \
        -e 's|…[0-9a-f]{8}|…<UID8>|g' \
        -e 's|snapshot [0-9a-f]{8}|snapshot <UID8>|g' \
        -e 's|[0-9]+(\.[0-9]+)?(ms\|s\|m\|h)( ago)?|<DUR>\3|g' \
        -e 's|[a-z0-9]+-mod-[0-9a-f]{16}|<MODUID>|g' \
        "$1" > "$2"
}

FAIL=0
compare() { # $1=left dir  $2=right dir  $3=label  $4=normalize? (yes|no)
    local left="$1" right="$2" label="$3" norm="$4" f base l r
    for f in modules-list.txt modules-list.json modules-show.txt modules-show.json \
             map.txt map.json; do
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
    echo "OK — all three surfaces byte-identical (A vs C raw; A vs B modulo the explicit volatile-token rules in normalize())."
    echo "outputs: ${WORK}/out-{A,B,C}"
else
    echo "FAILED — differences retained under ${WORK}/diffs/" >&2
    exit 1
fi
