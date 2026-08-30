#!/usr/bin/env bash
#
# find-facts-e2e.sh — ISOLATED end-to-end proof that EVERY next-command `rmap find`
# emits is RUNNABLE (exit 0), across all seven fact classes. Closes the FIND-FACTS-1
# review-1 defects at the live layer:
#   - item 1: a group header verb like `rmap explain` is NOT runnable on its own
#     (exits 1 with usage). Each HIT must render an executable invocation
#     (`explain <key>` / `map <path>` / the whole-listing command). This harness
#     PASTES each emitted next command into a shell and asserts exit 0.
#   - item 3: coverage must span all seven classes, not just symbol/file.
#
# It indexes a repo rich enough to populate the inferred/hint classes
# (module / http-surface / framework / boundary) into a THROWAWAY state root, runs
# `rmap find "<q>" --exact --json`, then executes EVERY DISTINCT emitted next command
# across ALL hits of every fact group (review-7 item 3: NOT just the first hit per
# class — every distinct `explain <key>` / `map --dry-run <path>` a reader could paste
# must run) — plus the class-constant `… list`/governance forms and a `map --dry-run`
# form unconditionally, so a class with zero hits in this repo still has its emitted
# COMMAND FORM proven runnable. Every emitted next command is NON-MUTATING
# (`explain`/`… list`/`violations`/`gate` read; `map --dry-run` writes nothing), so
# executing all of them never mutates ${TARGET_REPO}.
#
# ── Isolation (identical mechanism to dogfood-isolated.sh) ────────────────────
#   RMAP_TRANSPORT=stdio   → `rmap` spawns its OWN `rmapd --stdio`; no Unix socket,
#                            so the operator's resident daemon is never contacted.
#   RMAP_STATE_ROOT=<tmp>   → registry.json + databases/ under /private/tmp
#                            (SandboxLocal); the operator's real state root is only
#                            READ (its sha256 is checked identical before/after).
#
# ── Target repo ──────────────────────────────────────────────────────────────
# Must be rich enough for all seven classes. Default: the glamCRM checkout beside
# this repo; override with FIND_FACTS_E2E_REPO=/abs/path. If the repo is absent the
# harness SKIPS with a clear message (exit 0) rather than fabricating a pass — the
# in-process seam test `find_facts_all_seven_classes_produce_labeled_hits` covers the
# emitted-command SHAPE for all seven classes with no external repo.
#
# PLATFORM: macOS (mirrors dogfood-isolated.sh).

set -euo pipefail

# ── Resolve binaries (rmapd MUST be the sibling of rmap for the stdio spawn) ───
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
    exit 1
fi
export PATH="${BIN_DIR}:${PATH}"

# ── Target repo (rich → all seven classes) ────────────────────────────────────
DEFAULT_REPO="$(cd "${REPO_ROOT}/.." 2>/dev/null && pwd)/glamCRM"
TARGET_REPO="${FIND_FACTS_E2E_REPO:-${DEFAULT_REPO}}"
if [[ ! -d "${TARGET_REPO}" ]]; then
    echo "SKIP: target repo not found: ${TARGET_REPO}" >&2
    echo "      set FIND_FACTS_E2E_REPO=/abs/path to a repo rich in HTTP surfaces," >&2
    echo "      modules, dependencies, framework inferences, and governance declarations." >&2
    echo "      (the seam test covers all-seven emitted-command SHAPE without a repo)" >&2
    exit 0
fi

# ── Isolated state root (ephemeral, under /private/tmp → SandboxLocal) ─────────
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
STATE_ROOT="/private/tmp/repo-graph-find-facts-e2e/${RUN_ID}"
OUT_DIR="${STATE_ROOT}/out"
mkdir -p "${OUT_DIR}"
export RMAP_TRANSPORT="stdio"
export RMAP_STATE_ROOT="${STATE_ROOT}"
# Disable the daemon's BACKGROUND seed + enrich passes for the whole run. Both
# default ON; either racing `index` in this stdio-spawned daemon can take the
# write lock and flake the harness (the known lock-flake class — packet directive:
# every indexing test harness disables auto seed/enrich). LM Studio is live on
# this machine, so the seed pass WOULD fire otherwise. The facts tier reads the
# fact tables directly (never the vector store), so this does not weaken the proof;
# the endpoint-down check below still renders facts + seeds-unavailable-with-reason.
export RMAP_SEED_VECTORS="0"
export RMAP_AUTO_ENRICH="0"

OPERATOR_REGISTRY="${HOME}/Library/Application Support/repo-graph/registry.json"
OP_SHA_BEFORE=""
[[ -f "${OPERATOR_REGISTRY}" ]] && OP_SHA_BEFORE="$(shasum -a 256 "${OPERATOR_REGISTRY}" | awk '{print $1}')"

cleanup() { [[ "${KEEP:-false}" == "true" ]] || rm -rf "${STATE_ROOT}"; }
trap cleanup EXIT

echo "=================================================================="
echo "rmap find-facts E2E — every emitted next command must run (exit 0)"
echo "=================================================================="
echo "rmap        : ${RMAP_BIN}"
echo "target repo : ${TARGET_REPO}"
echo "state root  : ${STATE_ROOT}  (ephemeral; SandboxLocal)"
echo "------------------------------------------------------------------"

# ── Index the target into the isolated root ───────────────────────────────────
echo ">>> index"
"${RMAP_BIN}" index "${TARGET_REPO}" >"${OUT_DIR}/index.txt" 2>"${OUT_DIR}/index.stderr" \
    || { echo "FAIL: index exited nonzero"; tail -20 "${OUT_DIR}/index.stderr"; exit 1; }
cd "${TARGET_REPO}"

# All seven classes we expect to prove the emitted command for.
ALL_CLASSES=(symbol file module http-surface dependency framework boundary)

# A python filter: read a `find --json` payload on stdin, print one TSV line PER HIT —
# `class<TAB>certainty<TAB>next` for EVERY hit of every group with ≥1 hit (review-7
# item 3: not just hits[0]). Also fails hard if any group lacks a certainty tag in the
# allowed set, or any hit lacks a `next` (the review-1 honesty/actionability invariants).
read -r -d '' PYFILTER <<'PY' || true
import json, sys
doc = json.load(sys.stdin)
allowed = {"extracted", "inferred", "hint", "governance"}
for g in doc.get("facts", []):
    cls = g.get("fact_class")
    cert = g.get("certainty")
    if cert not in allowed:
        sys.stderr.write(f"INVARIANT FAIL: group {cls!r} certainty={cert!r} not in {allowed}\n")
        sys.exit(3)
    for h in g.get("hits", []):
        nxt = h.get("next")
        if not isinstance(nxt, str) or not nxt:
            sys.stderr.write(f"INVARIANT FAIL: group {cls!r} hit lacks a runnable next: {h}\n")
            sys.exit(3)
        print(f"{cls}\t{cert}\t{nxt}")
PY

# Queries chosen to spread hits across classes. Accumulate every emitted
# `class<TAB>certainty<TAB>next` line into one file (bash 3.2 on macOS has no
# associative arrays); the first line per class wins at execution time. Broad common
# substrings maximize the odds every class lands ≥1 hit in a real repo.
QUERIES=(a e i s api user service get post index config app route handler auth \
         aws lambda sdk jwt stack cdk type react node express main)
NEXT_TSV="${OUT_DIR}/emitted-next.tsv"
: >"${NEXT_TSV}"

for q in "${QUERIES[@]}"; do
    jf="${OUT_DIR}/find-${q}.json"
    if ! "${RMAP_BIN}" find "${q}" --exact --json >"${jf}" 2>"${OUT_DIR}/find-${q}.stderr"; then
        echo "FAIL: 'rmap find ${q} --exact --json' exited nonzero"; tail -20 "${OUT_DIR}/find-${q}.stderr"; exit 1
    fi
    python3 -c "${PYFILTER}" <"${jf}" >>"${NEXT_TSV}"
done

# ── Execute EVERY DISTINCT emitted next command from EVERY hit ─────────────────
# review-7 item 3: the prior harness ran only the first hit per class. Here we dedup
# the accumulated `class<TAB>cert<TAB>next` lines (identical commands prove nothing
# twice) and PASTE each distinct one into a shell exactly as a reader would — so every
# distinct `explain <key>` / `map --dry-run <path>` a real hit emitted is proven
# runnable, not just one representative. All forms are NON-MUTATING (see header).
DISTINCT_TSV="${OUT_DIR}/emitted-next.distinct.tsv"
sort -u "${NEXT_TSV}" >"${DISTINCT_TSV}"
DISTINCT_COUNT="$(wc -l <"${DISTINCT_TSV}" | tr -d ' ')"
echo ""
echo "Executing ${DISTINCT_COUNT} DISTINCT emitted next command(s) (pasted verbatim; each must exit 0):"
FAILED=0
COVERED=""
EXEC_N=0
while IFS=$'\t' read -r cls cert nxt; do
    [[ -z "${cls}" ]] && continue
    EXEC_N=$((EXEC_N + 1))
    # PASTE the emitted line into a shell exactly as a reader would (respects the
    # producer's shell-quoting). Output discarded; only the exit code is the assertion.
    if eval "\"${RMAP_BIN}\" ${nxt}" >/dev/null 2>"${OUT_DIR}/exec-${EXEC_N}.stderr"; then
        case " ${COVERED} " in *" ${cls} "*) : ;; *) COVERED="${COVERED}${cls} " ;; esac
    else
        printf '    FAIL: emitted next command exited nonzero (%s [%s]): rmap %s\n' "${cls}" "${cert}" "${nxt}" >&2
        tail -10 "${OUT_DIR}/exec-${EXEC_N}.stderr" | sed 's/^/      /' >&2
        FAILED=1
    fi
done <"${DISTINCT_TSV}"
# Report which classes contributed at least one executed per-hit next command, and
# name any class that produced NO hit in this repo (its FORM is proven below).
for cls in "${ALL_CLASSES[@]}"; do
    case " ${COVERED} " in
        *" ${cls} "*) printf '  · %-13s executed ≥1 per-hit next command\n' "${cls}" ;;
        *)            printf '  · %-13s no hits in this repo (command FORM proven below)\n' "${cls}" ;;
    esac
done

# ── Prove the class-constant COMMAND FORMS run, independent of hit presence ────
# The `… list` classes, the two boundary governance renderers (`violations`/`gate` —
# review-6 re-home), and `map --dry-run <dir>` emit a repo-agnostic form; run each once
# so a class that happened to match nothing in this repo still has its form verified.
# `map` is `--dry-run` (writes NOTHING): a discovery next-step must not mutate the target
# checkout on paste (review-2 item 1) — this harness therefore never writes into
# ${TARGET_REPO}, only READS it and writes to the isolated state root. NOTE: on an UNARMED
# repo (no governance declarations) `violations`/`gate` are a vacuous pass (exit 0); on an
# ARMED+failing repo `gate` may exit non-zero — a DOMAIN result, not a usage error (the
# review-0 defect this proves against). glamCRM (the default target) is unarmed → exit 0.
echo ""
echo "Class-constant command forms (runnable regardless of hits; non-mutating):"
for form in "boundaries list" "deps list" "inferences list" "violations" "gate" "map . --dry-run"; do
    printf '  $ rmap %s\n' "${form}"
    if ! eval "\"${RMAP_BIN}\" ${form}" >/dev/null 2>"${OUT_DIR}/form-${form// /_}.stderr"; then
        echo "    FAIL: 'rmap ${form}' exited nonzero" >&2
        tail -10 "${OUT_DIR}/form-${form// /_}.stderr" | sed 's/^/      /' >&2
        FAILED=1
    fi
done

# ── Endpoint-down proof: facts intact, seeds unavailable-with-reason ──────────
# A bogus endpoint makes the semantic tier unreachable; the facts tier must still
# render and the seed tier must say unavailable WITH a reason (verb survives the LM).
echo ""
echo "Endpoint-down proof (facts intact; seeds unavailable-with-reason):"
RMAP_SEED_ENDPOINT="http://127.0.0.1:9/v1/embeddings" \
    "${RMAP_BIN}" find "api" >"${OUT_DIR}/find-endpoint-down.txt" 2>/dev/null || true
if grep -q "Facts (deterministic lexical match" "${OUT_DIR}/find-endpoint-down.txt" \
   && grep -q "semantic seeds unavailable (" "${OUT_DIR}/find-endpoint-down.txt"; then
    echo "  PASS: facts rendered AND seeds unavailable-with-reason with the endpoint down"
else
    echo "  FAIL: endpoint-down output missing facts tier or seed-unavailable line" >&2
    sed 's/^/    /' "${OUT_DIR}/find-endpoint-down.txt" >&2
    FAILED=1
fi

# ── Isolation invariant: operator registry byte-identical ─────────────────────
if [[ -n "${OP_SHA_BEFORE}" ]]; then
    OP_SHA_AFTER="$(shasum -a 256 "${OPERATOR_REGISTRY}" | awk '{print $1}')"
    if [[ "${OP_SHA_BEFORE}" != "${OP_SHA_AFTER}" ]]; then
        echo "FAIL: operator registry.json changed (${OP_SHA_BEFORE} → ${OP_SHA_AFTER}) — ISOLATION BREACHED" >&2
        exit 1
    fi
    echo ""
    echo "Isolation: operator registry.json sha256 identical (${OP_SHA_AFTER})"
fi

echo ""
echo "------------------------------------------------------------------"
echo "classes with executed per-hit next commands: ${COVERED:-<none>}"
echo "distinct per-hit next commands executed: ${DISTINCT_COUNT}"
if [[ "${FAILED}" -ne 0 ]]; then
    echo "RESULT: FAIL — at least one emitted next command was not runnable." >&2
    exit 1
fi
echo "RESULT: PASS — every distinct emitted next command (all hits) ran exit 0."
