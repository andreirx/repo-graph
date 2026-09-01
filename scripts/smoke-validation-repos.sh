#!/usr/bin/env bash
# smoke-validation-repos.sh — smoke test rmap on all validation repos with daemon-based execution
#
# Version: 3 (REG-1; full command surface at full output)
#
# Capture harness for docs/testing/end-to-end-usefulness-protocol.md: indexes the
# validation repos and runs the COMPREHENSIVE repo-wide command set at FULL output
# (orient --full, all repo-wide commands). An agent then evaluates the captures in
# smoke-runs/<ts>/ against known ground truth (the usefulness judgment is not a
# script assertion — see the protocol's rubric).
#
# Usage:
#   ./scripts/smoke-validation-repos.sh <task> [commands...]
#
# Examples:
#   ./scripts/smoke-validation-repos.sh slice-12                    # default commands
#   ./scripts/smoke-validation-repos.sh pf-3 trust modules          # specific commands
#   ./scripts/smoke-validation-repos.sh quality trust check orient  # multiple commands
#
# Flags:
#   --retain    Keep state after run (default: delete on success)
#   --adhoc     Skip smoke-runs/ logging (for quick exploration only,
#               NOT for slice verification or production-fix validation)
#
# Default commands: trust, modules list, check
#
# Repo inventory model:
#   - Internal repos: explicitly listed (repo-graph, amodx, glamCRM, hexmanos, zap-engine)
#   - Legacy repos: discovered dynamically from ../legacy-codebases/
#
# Discovery rules for legacy bucket:
#   - directories only
#   - hidden entries skipped
#   - sorted lexicographically
#
# State isolation (REG-1 daemon model):
#   RMAP_STATE_ROOT=/private/tmp/repo-graph-tests/<task>
#   RMAP_SOCKET_PATH=/private/tmp/repo-graph-tests/<task>/daemon.sock
#
# Run logging (per protocol):
#   smoke-runs/<timestamp>/00-meta.json           — batch summary
#   smoke-runs/<timestamp>/<repo>-meta.json       — per-repo traceability
#   smoke-runs/<timestamp>/<repo>-<command>.txt   — per-command output (human mode)
#   smoke-runs/<timestamp>/92-tool-latency.json   — all timings
#
# Protocol requirement:
#   All slice verification, production-fix validation, and validation-repo
#   smoke runs MUST be logged. Use --adhoc only for non-protocol exploratory
#   runs that do not constitute verification evidence.

set -euo pipefail

SCRIPT_VERSION="2"
GENERATOR="smoke-validation-repos.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/rust/Cargo.toml"
PACKAGE_RGR="repo-graph-rgr"
PACKAGE_RMAPD="rmapd"
TEST_ROOT="/private/tmp/repo-graph-tests"
PARENT_DIR="$(cd "$REPO_ROOT/.." && pwd)"
LEGACY_BUCKET="$PARENT_DIR/legacy-codebases"

RETAIN=false
ADHOC=false
DAEMON_PID=""

# Internal repos — explicitly listed
INTERNAL_NAMES=(
    "repo-graph"
    "amodx"
    "glamCRM"
    "hexmanos"
    "zap-engine"
    "zap-squad"
    "FRAKTAG"
)
INTERNAL_PATHS=(
    "$REPO_ROOT"
    "$PARENT_DIR/amodx"
    "$PARENT_DIR/glamCRM"
    "$PARENT_DIR/hexmanos"
    "$PARENT_DIR/zap-engine"
    "$PARENT_DIR/zap-squad"
    "$PARENT_DIR/FRAKTAG"
)

# Giant repos to run LAST so a huge index does not head-of-line-block the batch on
# the serial daemon (e.g. the Linux kernel; the vscode/storybook TS giants).
# Override: SMOKE_DEFER_LAST="a b".
DEFER_LAST=(${SMOKE_DEFER_LAST:-vscode storybook linux})
# Optional: run ONLY these repos (space-separated names), e.g.
#   SMOKE_ONLY="mempalace nginx" ./scripts/smoke-validation-repos.sh <task> ...
SMOKE_ONLY="${SMOKE_ONLY:-}"
# Optional: SKIP these repos (space-separated names) — the complement filter, e.g.
#   SMOKE_SKIP="linux" for a batch without the kernel-scale index.
SMOKE_SKIP="${SMOKE_SKIP:-}"

usage() {
    echo "usage: $0 [--retain] [--adhoc] <task> [commands...]" >&2
    echo "" >&2
    echo "  --retain   — keep state after run (default: delete on success)" >&2
    echo "  --adhoc    — skip logging (exploration only, NOT for verification)" >&2
    echo "  task       — task identifier (e.g., slice-12, validation-run)" >&2
    echo "  commands   — rmap commands to run (default: trust, modules, check)" >&2
    echo "" >&2
    echo "Internal repos: ${INTERNAL_NAMES[*]}" >&2
    echo "Legacy bucket: $LEGACY_BUCKET" >&2
    exit 1
}

# Discover legacy repos from bucket directory
# Populates LEGACY_NAMES and LEGACY_PATHS arrays
discover_legacy_repos() {
    LEGACY_NAMES=()
    LEGACY_PATHS=()

    if [[ ! -d "$LEGACY_BUCKET" ]]; then
        echo "NOTE: Legacy bucket not found: $LEGACY_BUCKET" >&2
        return
    fi

    # Find directories, skip hidden, sort lexicographically. Defer DEFER_LAST
    # giants (e.g. linux) to the end so they don't head-of-line-block the batch.
    local deferred_names=() deferred_paths=()
    while IFS= read -r dir; do
        local name
        name=$(basename "$dir")
        if [[ " ${DEFER_LAST[*]} " == *" $name "* ]]; then
            deferred_names+=("$name")
            deferred_paths+=("$dir")
        else
            LEGACY_NAMES+=("$name")
            LEGACY_PATHS+=("$dir")
        fi
    done < <(find "$LEGACY_BUCKET" -mindepth 1 -maxdepth 1 -type d -not -name '.*' | sort)
    if [[ ${#deferred_names[@]} -gt 0 ]]; then
        LEGACY_NAMES+=("${deferred_names[@]}")
        LEGACY_PATHS+=("${deferred_paths[@]}")
    fi
}

# Run a command and capture output without head truncation issues
run_and_capture() {
    local output_file="$1"
    shift
    local exit_code=0
    "$@" > "$output_file" 2>&1 || exit_code=$?
    return $exit_code
}

# Display truncated output from a file
display_truncated() {
    local file="$1"
    local lines="${2:-50}"
    head -n "$lines" "$file"
    local total
    total=$(wc -l < "$file" | tr -d ' ')
    if [[ "$total" -gt "$lines" ]]; then
        echo "... ($((total - lines)) more lines)"
    fi
}

# Start daemon in background with isolated state
start_daemon() {
    local state_root="$1"
    local socket_path="$2"

    echo "Starting daemon..."
    echo "  State root: $state_root"
    echo "  Socket: $socket_path"

    RMAP_STATE_ROOT="$state_root" RMAP_SOCKET_PATH="$socket_path" \
        cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE_RMAPD" --release &
    DAEMON_PID=$!

    # Wait for socket to appear (up to 120 seconds for initial compilation)
    local waited=0
    while [[ ! -S "$socket_path" && $waited -lt 1200 ]]; do
        sleep 0.1
        waited=$((waited + 1))
        if (( waited % 100 == 0 )); then
            echo "  Waiting for daemon... (${waited}0ms)"
        fi
    done

    if [[ ! -S "$socket_path" ]]; then
        echo "ERROR: Daemon failed to start (socket not created after 120s)" >&2
        kill $DAEMON_PID 2>/dev/null || true
        exit 2
    fi

    echo "  Daemon started (PID: $DAEMON_PID)"
}

# Stop daemon gracefully
stop_daemon() {
    if [[ -n "${DAEMON_PID:-}" ]]; then
        echo "Stopping daemon (PID: $DAEMON_PID)..."
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
        DAEMON_PID=""
    fi
}

# Cleanup handler
cleanup() {
    stop_daemon
}

trap cleanup EXIT

# Parse flags
while [[ $# -gt 0 ]]; do
    case "$1" in
        --retain)
            RETAIN=true
            shift
            ;;
        --adhoc)
            ADHOC=true
            shift
            ;;
        -*)
            echo "error: unknown flag $1" >&2
            usage
            ;;
        *)
            break
            ;;
    esac
done

if [[ $# -lt 1 ]]; then
    usage
fi

TASK="$1"
shift

# Default commands: the comprehensive repo-wide command surface at FULL output
# (the End-to-End Usefulness Protocol capture set), not a historical few. Each may
# be multi-word (subcommand + flags). Target-requiring commands (explain/callers/
# path/imports/modules show) need a per-repo symbol/file — pass them explicitly.
if [[ $# -eq 0 ]]; then
    COMMANDS=(
        "orient --budget small" "orient --budget medium" "orient --budget large" "orient --full"
        "check --full" "trust"
        "modules list" "modules violations" "stats" "cycles"
        "churn" "hotspots" "risk"
        "dead" "violations" "gate" "assess"
        "surfaces list" "boundaries list" "boundaries summary" "resource list"
        "docs list" "inferences list" "deps list"
        # "map --dry-run", NEVER bare "map": bare map WRITES MAP.md sidecars into the target
        # tree (bitten 2026-09-01: 79,325 generated files across the whole validation corpus,
        # including overwriting FRAKTAG's own TRACKED MAP.md — the smoke's read-only posture
        # was violated by its own default command set on every prior run).
        "map --dry-run" "doctor"
    )
else
    COMMANDS=("$@")
fi

# Discover legacy repos
discover_legacy_repos

# Build combined repo lists
ALL_NAMES=("${INTERNAL_NAMES[@]}" "${LEGACY_NAMES[@]}")
ALL_PATHS=("${INTERNAL_PATHS[@]}" "${LEGACY_PATHS[@]}")

# Build category lookup (parallel array)
ALL_CATEGORIES=()
for _ in "${INTERNAL_NAMES[@]}"; do
    ALL_CATEGORIES+=("internal")
done
for _ in "${LEGACY_NAMES[@]}"; do
    ALL_CATEGORIES+=("legacy")
done

# Optional subset: SMOKE_ONLY="n1 n2 ..." runs only those repos (order preserved;
# DEFER_LAST giants stay last). Non-matching names are skipped.
if [[ -n "$SMOKE_ONLY" ]]; then
    only_names=(); only_paths=(); only_cats=()
    for idx in "${!ALL_NAMES[@]}"; do
        if [[ " $SMOKE_ONLY " == *" ${ALL_NAMES[$idx]} "* ]]; then
            only_names+=("${ALL_NAMES[$idx]}")
            only_paths+=("${ALL_PATHS[$idx]}")
            only_cats+=("${ALL_CATEGORIES[$idx]}")
        fi
    done
    if [[ ${#only_names[@]} -eq 0 ]]; then
        echo "error: SMOKE_ONLY matched no known repos: '$SMOKE_ONLY'" >&2
        exit 1
    fi
    ALL_NAMES=("${only_names[@]}")
    ALL_PATHS=("${only_paths[@]}")
    ALL_CATEGORIES=("${only_cats[@]}")
fi

# Optional complement: SMOKE_SKIP="n1 n2 ..." drops those repos from the batch.
if [[ -n "$SMOKE_SKIP" ]]; then
    keep_names=(); keep_paths=(); keep_cats=()
    for idx in "${!ALL_NAMES[@]}"; do
        if [[ " $SMOKE_SKIP " != *" ${ALL_NAMES[$idx]} "* ]]; then
            keep_names+=("${ALL_NAMES[$idx]}")
            keep_paths+=("${ALL_PATHS[$idx]}")
            keep_cats+=("${ALL_CATEGORIES[$idx]}")
        fi
    done
    ALL_NAMES=("${keep_names[@]}")
    ALL_PATHS=("${keep_paths[@]}")
    ALL_CATEGORIES=("${keep_cats[@]}")
fi

# State isolation paths (REG-1 daemon model)
STATE_ROOT="$TEST_ROOT/$TASK"
SOCKET_PATH="$STATE_ROOT/daemon.sock"
mkdir -p "$STATE_ROOT"

# Run logging setup
TIMESTAMP="$(date -u +%Y-%m-%dT%H-%M-%SZ)"
RUN_DIR="$REPO_ROOT/smoke-runs/$TIMESTAMP"
if [[ "$ADHOC" == "false" ]]; then
    mkdir -p "$RUN_DIR"
else
    echo "WARNING: --adhoc mode, no run logging (not valid for verification)" >&2
fi

echo "=============================================="
echo "Validation smoke run: $TASK"
echo "Commands: ${COMMANDS[*]}"
echo "State root: $STATE_ROOT"
if [[ "$ADHOC" == "false" ]]; then
    echo "Run log: $RUN_DIR"
fi
echo ""
echo "Internal repos: ${INTERNAL_NAMES[*]}"
echo "Legacy repos: ${LEGACY_NAMES[*]:-none discovered}"
echo "=============================================="
echo ""

# Start isolated daemon
start_daemon "$STATE_ROOT" "$SOCKET_PATH"

# Export daemon environment for rmap commands
export RMAP_STATE_ROOT="$STATE_ROOT"
export RMAP_SOCKET_PATH="$SOCKET_PATH"

FAILED_REPOS=()
PASSED_REPOS=()
SKIPPED_REPOS=()

# Timing log accumulator
TIMING_FILE=$(mktemp)
echo "{" > "$TIMING_FILE"
FIRST_TIMING=true

for i in "${!ALL_NAMES[@]}"; do
    REPO_NAME="${ALL_NAMES[$i]}"
    REPO_PATH="${ALL_PATHS[$i]}"
    REPO_CATEGORY="${ALL_CATEGORIES[$i]}"

    echo "----------------------------------------------"
    echo "Repo: $REPO_NAME ($REPO_CATEGORY)"
    echo "Path: $REPO_PATH"
    echo "----------------------------------------------"

    if [[ ! -d "$REPO_PATH" ]]; then
        echo "SKIP: repo path does not exist"
        SKIPPED_REPOS+=("$REPO_NAME")
        echo ""
        continue
    fi

    # Per-repo meta accumulator
    REPO_META_COMMANDS=""

    # Index the repo (daemon allocates db internally)
    INDEX_EXIT=0
    INDEX_TIME=0
    echo "Indexing..."
    INDEX_OUTPUT=$(mktemp)
    START_TIME=$(date +%s)
    if ! run_and_capture "$INDEX_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE_RGR" --release -- \
        index "$REPO_PATH"; then
        INDEX_EXIT=1
        echo "FAIL: indexing failed"
        display_truncated "$INDEX_OUTPUT" 20
        FAILED_REPOS+=("$REPO_NAME:index")
        rm -f "$INDEX_OUTPUT"

        # Write per-repo meta even on index failure
        if [[ "$ADHOC" == "false" ]]; then
            cat > "$RUN_DIR/${REPO_NAME}-meta.json" << EOF
{
  "repo_uid": "$REPO_NAME",
  "repo_path": "$REPO_PATH",
  "category": "$REPO_CATEGORY",
  "baseline_shape_version": 4,
  "cli_model": "REG-1",
  "timestamp": "$TIMESTAMP",
  "index_failed": true,
  "commands": {}
}
EOF
        fi
        echo ""
        continue
    fi
    END_TIME=$(date +%s)
    INDEX_TIME=$((END_TIME - START_TIME))

    # Add to timing log
    if [[ "$FIRST_TIMING" == "true" ]]; then
        FIRST_TIMING=false
    else
        echo "," >> "$TIMING_FILE"
    fi
    echo "  \"${REPO_NAME}_index_seconds\": $INDEX_TIME" >> "$TIMING_FILE"

    display_truncated "$INDEX_OUTPUT" 5
    rm -f "$INDEX_OUTPUT"

    # Run each command from the repo directory (CWD-based resolution)
    REPO_FAILED=false
    # C7: commands whose non-zero exit is a designed VERDICT/status, NOT a harness error
    # (check = reliability/gate verdict; dead = intentionally disabled; gate = gate pass/fail).
    # A non-zero exit for these is still recorded in per-repo meta, but does not fail the repo.
    VERDICT_COMMANDS=" check dead gate "
    for CMD in "${COMMANDS[@]}"; do
        echo ""
        echo "Command: $CMD"

        CMD_OUTPUT=$(mktemp)
        CMD_EXIT=0
        START_TIME=$(date +%s)

        # Run command from repo directory. CMD may be multi-word (e.g.
        # "orient --full", "modules list", "surfaces list") — split into argv so
        # subcommands + flags pass through. The FULL command surface, not a
        # hardcoded few (End-to-End Usefulness Protocol).
        read -ra CMD_ARGS <<< "$CMD"
        pushd "$REPO_PATH" > /dev/null
        run_and_capture "$CMD_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE_RGR" --release -- \
            "${CMD_ARGS[@]}" || CMD_EXIT=$?
        popd > /dev/null

        END_TIME=$(date +%s)
        CMD_TIME=$((END_TIME - START_TIME))

        # Add to timing log
        if [[ "$FIRST_TIMING" == "true" ]]; then
            FIRST_TIMING=false
        else
            echo "," >> "$TIMING_FILE"
        fi
        echo "  \"${REPO_NAME}_${CMD}_seconds\": $CMD_TIME" >> "$TIMING_FILE"

        # Add to per-repo meta
        if [[ -n "$REPO_META_COMMANDS" ]]; then
            REPO_META_COMMANDS+=","
        fi
        REPO_META_COMMANDS+="\"$CMD\":{\"exit_code\":$CMD_EXIT,\"seconds\":$CMD_TIME}"

        if [[ "$CMD_EXIT" -ne 0 ]]; then
            if [[ "$VERDICT_COMMANDS" == *" ${CMD_ARGS[0]} "* ]]; then
                # non-zero is a designed verdict/status (check FAIL, dead disabled, gate fail) — not a harness error
                echo "NOTE: $CMD exit $CMD_EXIT (command verdict/status — not a harness error)"
                display_truncated "$CMD_OUTPUT" 30
            else
                echo "FAIL: $CMD (exit $CMD_EXIT)"
                display_truncated "$CMD_OUTPUT" 20
                REPO_FAILED=true
            fi
        else
            display_truncated "$CMD_OUTPUT" 30
        fi

        # Log output
        if [[ "$ADHOC" == "false" ]]; then
            CMD_FILENAME=$(echo "$CMD" | tr ' /' '-')
            cp "$CMD_OUTPUT" "$RUN_DIR/${REPO_NAME}-${CMD_FILENAME}.txt"
        fi

        rm -f "$CMD_OUTPUT"
    done

    # Write per-repo meta
    if [[ "$ADHOC" == "false" ]]; then
        cat > "$RUN_DIR/${REPO_NAME}-meta.json" << EOF
{
  "repo_uid": "$REPO_NAME",
  "repo_path": "$REPO_PATH",
  "category": "$REPO_CATEGORY",
  "baseline_shape_version": 4,
  "cli_model": "REG-1",
  "timestamp": "$TIMESTAMP",
  "commands": {$REPO_META_COMMANDS}
}
EOF
    fi

    if [[ "$REPO_FAILED" == "true" ]]; then
        FAILED_REPOS+=("$REPO_NAME")
    else
        PASSED_REPOS+=("$REPO_NAME")
    fi

    echo ""
done

# Close timing log
echo "" >> "$TIMING_FILE"
echo "}" >> "$TIMING_FILE"

# Write batch meta and timing logs
if [[ "$ADHOC" == "false" ]]; then
    # Build JSON arrays manually (jq may not be installed)
    INTERNAL_JSON="[\"$(echo "${INTERNAL_NAMES[*]}" | sed 's/ /","/g')\"]"

    LEGACY_JSON="[]"
    if [[ ${#LEGACY_NAMES[@]} -gt 0 ]]; then
        LEGACY_JSON="[\"$(echo "${LEGACY_NAMES[*]}" | sed 's/ /","/g')\"]"
    fi

    CMDS_JSON="[\"$(echo "${COMMANDS[*]}" | sed 's/ /","/g')\"]"

    PASSED_JSON="[]"
    if [[ ${#PASSED_REPOS[@]} -gt 0 ]]; then
        PASSED_JSON="[\"$(echo "${PASSED_REPOS[*]}" | sed 's/ /","/g')\"]"
    fi

    FAILED_JSON="[]"
    if [[ ${#FAILED_REPOS[@]} -gt 0 ]]; then
        FAILED_JSON="[\"$(echo "${FAILED_REPOS[*]}" | sed 's/ /","/g')\"]"
    fi

    SKIPPED_JSON="[]"
    if [[ ${#SKIPPED_REPOS[@]} -gt 0 ]]; then
        SKIPPED_JSON="[\"$(echo "${SKIPPED_REPOS[*]}" | sed 's/ /","/g')\"]"
    fi

    # 00-meta.json — batch summary
    cat > "$RUN_DIR/00-meta.json" << EOF
{
  "generator": "$GENERATOR",
  "generator_version": "$SCRIPT_VERSION",
  "type": "batch_validation",
  "task": "$TASK",
  "state_root": "$STATE_ROOT",
  "baseline_shape_version": 4,
  "cli_model": "REG-1",
  "timestamp": "$TIMESTAMP",
  "inventory_model": "hybrid",
  "internal_repos": $INTERNAL_JSON,
  "legacy_repos": $LEGACY_JSON,
  "legacy_bucket": "$LEGACY_BUCKET",
  "commands": $CMDS_JSON,
  "passed": $PASSED_JSON,
  "failed": $FAILED_JSON,
  "skipped": $SKIPPED_JSON,
  "per_repo_meta": "See <repo>-meta.json for per-repo category, exit codes, timing; <repo>-<cmd>.txt for output"
}
EOF

    # 92-tool-latency.json
    cp "$TIMING_FILE" "$RUN_DIR/92-tool-latency.json"
fi

rm -f "$TIMING_FILE"

# Stop daemon before summary
stop_daemon

echo "=============================================="
echo "Summary"
echo "=============================================="
echo "Passed: ${#PASSED_REPOS[@]} (${PASSED_REPOS[*]:-none})"
echo "Failed: ${#FAILED_REPOS[@]} (${FAILED_REPOS[*]:-none})"
echo "Skipped: ${#SKIPPED_REPOS[@]} (${SKIPPED_REPOS[*]:-none})"
echo ""
echo "State root: $STATE_ROOT"

if [[ "$ADHOC" == "false" ]]; then
    echo "Run log: $RUN_DIR"
fi

# Cleanup or retain
if [[ ${#FAILED_REPOS[@]} -eq 0 && "$RETAIN" == "false" ]]; then
    rm -rf "$STATE_ROOT"
    echo "Disposal: deleted (all passed, default lifecycle)"
elif [[ "$RETAIN" == "true" ]]; then
    echo "Disposal: RETAINED (--retain flag)"
else
    echo "Disposal: RETAINED (failures detected)"
fi

if [[ ${#FAILED_REPOS[@]} -gt 0 ]]; then
    exit 1
fi
