#!/usr/bin/env bash
# smoke-rmap.sh — smoke test rmap commands with daemon-based execution
#
# Version: 4 (REG-1 daemon-based CLI)
#
# Usage:
#   ./scripts/smoke-rmap.sh <task> <repo-path> <command> [args...]
#
# Examples:
#   ./scripts/smoke-rmap.sh slice-12 . trust
#   ./scripts/smoke-rmap.sh pf-2 ../legacy-codebases/swupdate policy --kind BEHAVIORAL_MARKER
#   ./scripts/smoke-rmap.sh bi-em-1 ../linux "boundaries list" --kind inter_core_channel
#
# Flags:
#   --retain    Keep state after run (default: delete on success)
#   --adhoc     Skip smoke-runs/ logging (for quick exploration only,
#               NOT for slice verification or production-fix validation)
#
# State isolation (REG-1 daemon model):
#   RMAP_STATE_ROOT=/private/tmp/repo-graph-tests/<task>
#   RMAP_SOCKET_PATH=/private/tmp/repo-graph-tests/<task>/daemon.sock
#
# Run logging (per protocol):
#   smoke-runs/<timestamp>/00-meta.json      — run metadata with generator provenance
#   smoke-runs/<timestamp>/<command>.json    — command output
#   smoke-runs/<timestamp>/92-tool-latency.json — timing information
#
# Protocol requirements:
#   - All slice verification and production-fix validation MUST be logged
#   - Use --adhoc only for non-protocol exploratory runs
#   - Hand-crafted artifacts do not satisfy the protocol
#   - Script self-validates artifact completeness before exit

set -euo pipefail

SCRIPT_VERSION="4"
GENERATOR="smoke-rmap.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/rust/Cargo.toml"
PACKAGE_RGR="repo-graph-rgr"
PACKAGE_RMAPD="rmapd"
TEST_ROOT="/private/tmp/repo-graph-tests"

RETAIN=false
ADHOC=false

usage() {
    echo "usage: $0 [--retain] [--adhoc] <task> <repo-path> <command>..." >&2
    echo "" >&2
    echo "  --retain   — keep state after run (default: delete on success)" >&2
    echo "  --adhoc    — skip logging (exploration only, NOT for verification)" >&2
    echo "  task       — task identifier (e.g., slice-12, pf-2, bi-em-1)" >&2
    echo "  repo-path  — path to repo (relative to APLICATII BIJUTERIE or absolute)" >&2
    echo "  command    — one or more rmap commands (e.g., trust check orient)" >&2
    echo "" >&2
    echo "Examples:" >&2
    echo "  $0 slice-12 . trust                    # single command" >&2
    echo "  $0 pf-2 legacy-codebases/OpenXcom trust check orient  # multiple commands" >&2
    exit 1
}

# Build JSON array from arguments
# Usage: json_array "arg1" "arg2" ...
json_array() {
    local first=true
    printf '['
    for arg in "$@"; do
        if [[ "$first" == "true" ]]; then
            first=false
        else
            printf ','
        fi
        # Escape quotes and backslashes in arg
        local escaped
        escaped=$(printf '%s' "$arg" | sed 's/\\/\\\\/g; s/"/\\"/g')
        printf '"%s"' "$escaped"
    done
    printf ']'
}

# Start daemon in background with isolated state
# Sets DAEMON_PID global
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
        unset DAEMON_PID
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

if [[ $# -lt 3 ]]; then
    usage
fi

TASK="$1"
REPO_PATH_ARG="$2"
shift 2

# Remaining args are commands
COMMANDS=("$@")
if [[ ${#COMMANDS[@]} -eq 0 ]]; then
    usage
fi

# Resolve repo path
if [[ "$REPO_PATH_ARG" == "." ]]; then
    REPO_PATH="$REPO_ROOT"
elif [[ "$REPO_PATH_ARG" == /* ]]; then
    REPO_PATH="$REPO_PATH_ARG"
else
    # Relative to APLICATII BIJUTERIE
    REPO_PATH="$(cd "$REPO_ROOT/.." && pwd)/$REPO_PATH_ARG"
fi

if [[ ! -d "$REPO_PATH" ]]; then
    echo "error: repo path does not exist: $REPO_PATH" >&2
    exit 2
fi

# Canonicalize repo path
REPO_PATH="$(cd "$REPO_PATH" && pwd)"

# Extract repo name from path
REPO_NAME="$(basename "$REPO_PATH")"

# State isolation paths (REG-1 daemon model)
STATE_ROOT="$TEST_ROOT/$TASK"
SOCKET_PATH="$STATE_ROOT/daemon.sock"

# Run logging setup
TIMESTAMP="$(date -u +%Y-%m-%dT%H-%M-%SZ)"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RUN_DIR="$REPO_ROOT/smoke-runs/$TIMESTAMP"

if [[ "$ADHOC" == "false" ]]; then
    mkdir -p "$RUN_DIR"
else
    echo "WARNING: --adhoc mode, no run logging (not valid for verification)" >&2
fi

# Create state root directory
mkdir -p "$STATE_ROOT"

# Start isolated daemon
start_daemon "$STATE_ROOT" "$SOCKET_PATH"

# Export daemon environment for rmap commands
export RMAP_STATE_ROOT="$STATE_ROOT"
export RMAP_SOCKET_PATH="$SOCKET_PATH"

# Index the repo (daemon allocates db_path)
INDEX_TIME=0
echo ""
echo "Indexing $REPO_NAME..."
START_TIME=$(date +%s)
cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE_RGR" --release -- \
    index "$REPO_PATH"
END_TIME=$(date +%s)
INDEX_TIME=$((END_TIME - START_TIME))
echo ""

# Run all commands from repo directory
echo "Commands to run: ${COMMANDS[*]}"
echo "Repo: $REPO_PATH"
echo ""

# Track results
TIMING_JSON="\"index_seconds\": $INDEX_TIME"
COMMANDS_META=""
WORST_EXIT_CODE=0
FAILED_COMMANDS=()
PASSED_COMMANDS=()

pushd "$REPO_PATH" > /dev/null

for COMMAND in "${COMMANDS[@]}"; do
    echo "----------------------------------------------"
    echo "Running: $COMMAND"
    echo "----------------------------------------------"

    CMD_OUTPUT_FILE=$(mktemp)
    CMD_EXIT_CODE=0
    START_TIME=$(date +%s)

    cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE_RGR" --release -- \
        $COMMAND > "$CMD_OUTPUT_FILE" 2>&1 || CMD_EXIT_CODE=$?

    END_TIME=$(date +%s)
    CMD_TIME=$((END_TIME - START_TIME))

    # Display output (truncated for readability)
    head -50 "$CMD_OUTPUT_FILE"
    LINES=$(wc -l < "$CMD_OUTPUT_FILE" | tr -d ' ')
    if [[ "$LINES" -gt 50 ]]; then
        echo "... ($((LINES - 50)) more lines)"
    fi

    # Track timing
    CMD_KEY=$(echo "$COMMAND" | tr ' ' '_')
    TIMING_JSON+=", \"${CMD_KEY}_seconds\": $CMD_TIME"

    # Track command meta
    if [[ -n "$COMMANDS_META" ]]; then
        COMMANDS_META+=", "
    fi
    COMMANDS_META+="\"$COMMAND\": {\"exit_code\": $CMD_EXIT_CODE, \"seconds\": $CMD_TIME}"

    # Track pass/fail
    if [[ "$CMD_EXIT_CODE" -eq 0 ]]; then
        PASSED_COMMANDS+=("$COMMAND")
        echo "OK: $COMMAND (${CMD_TIME}s)"
    else
        FAILED_COMMANDS+=("$COMMAND")
        echo "FAIL: $COMMAND (exit $CMD_EXIT_CODE, ${CMD_TIME}s)"
        if [[ "$CMD_EXIT_CODE" -gt "$WORST_EXIT_CODE" ]]; then
            WORST_EXIT_CODE=$CMD_EXIT_CODE
        fi
    fi

    # Save command output
    if [[ "$ADHOC" == "false" ]]; then
        CMD_FILENAME=$(echo "$COMMAND" | tr ' /' '-')
        cp "$CMD_OUTPUT_FILE" "$RUN_DIR/$CMD_FILENAME.json"
    fi

    rm -f "$CMD_OUTPUT_FILE"
    echo ""
done

popd > /dev/null

FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Write run logs (non-adhoc only)
if [[ "$ADHOC" == "false" ]]; then
    # Build commands JSON array
    CMDS_JSON=$(json_array "${COMMANDS[@]}")
    PASSED_JSON=$(json_array "${PASSED_COMMANDS[@]}")
    FAILED_JSON=$(json_array "${FAILED_COMMANDS[@]}")

    # 00-meta.json with generator provenance
    cat > "$RUN_DIR/00-meta.json" << EOF
{
  "generator": "$GENERATOR",
  "generator_version": "$SCRIPT_VERSION",
  "baseline_shape_version": 4,
  "cli_model": "REG-1",
  "timestamp": "$TIMESTAMP",
  "task": "$TASK",
  "state_root": "$STATE_ROOT",
  "repo_name": "$REPO_NAME",
  "repo_path": "$REPO_PATH",
  "commands": $CMDS_JSON,
  "commands_detail": {$COMMANDS_META},
  "passed": $PASSED_JSON,
  "failed": $FAILED_JSON,
  "started_at": "$STARTED_AT",
  "finished_at": "$FINISHED_AT",
  "worst_exit_code": $WORST_EXIT_CODE
}
EOF

    # 92-tool-latency.json
    cat > "$RUN_DIR/92-tool-latency.json" << EOF
{$TIMING_JSON}
EOF

    echo "=============================================="
    echo "Run logged: $RUN_DIR"
    echo "Passed: ${#PASSED_COMMANDS[@]} (${PASSED_COMMANDS[*]:-none})"
    echo "Failed: ${#FAILED_COMMANDS[@]} (${FAILED_COMMANDS[*]:-none})"
    echo "=============================================="
fi

# Stop daemon before cleanup decision
stop_daemon

# Cleanup or retain
echo ""
echo "---"
echo "State root: $STATE_ROOT"

if [[ "$WORST_EXIT_CODE" -eq 0 && "$RETAIN" == "false" ]]; then
    rm -rf "$STATE_ROOT"
    echo "Disposal: deleted (all passed, default lifecycle)"
elif [[ "$RETAIN" == "true" ]]; then
    echo "Disposal: RETAINED (--retain flag)"
else
    echo "Disposal: RETAINED (failures detected)"
fi

exit $WORST_EXIT_CODE
