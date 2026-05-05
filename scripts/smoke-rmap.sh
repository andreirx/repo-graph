#!/usr/bin/env bash
# smoke-rmap.sh — smoke test rmap commands with canonical DB paths and run logging
#
# Version: 2
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
#   --retain    Keep DB after run (default: delete on success)
#   --adhoc     Skip smoke-runs/ logging (for quick exploration only,
#               NOT for slice verification or production-fix validation)
#
# DB path follows protocol:
#   /private/tmp/repo-graph-tests/<task>/<repo-name>.db
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

SCRIPT_VERSION="3"
GENERATOR="smoke-rmap.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/rust/Cargo.toml"
PACKAGE="repo-graph-rgr"
TEST_ROOT="/private/tmp/repo-graph-tests"

RETAIN=false
ADHOC=false

usage() {
    echo "usage: $0 [--retain] [--adhoc] <task> <repo-path> <command> [args...]" >&2
    echo "" >&2
    echo "  --retain   — keep DB after run (default: delete on success)" >&2
    echo "  --adhoc    — skip logging (exploration only, NOT for verification)" >&2
    echo "  task       — task identifier (e.g., slice-12, pf-2, bi-em-1)" >&2
    echo "  repo-path  — path to repo (relative to APLICATII BIJUTERIE or absolute)" >&2
    echo "  command    — rmap command, may include subcommand (e.g., \"boundaries list\")" >&2
    echo "  args       — additional arguments to the command" >&2
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
COMMAND="$3"
shift 3

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

# Extract repo name from path
REPO_NAME="$(basename "$REPO_PATH")"
REPO_UID="$REPO_NAME"

# Canonical DB path
DB_DIR="$TEST_ROOT/$TASK"
DB_PATH="$DB_DIR/$REPO_NAME.db"

# Run logging setup
TIMESTAMP="$(date -u +%Y-%m-%dT%H-%M-%SZ)"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RUN_DIR="$REPO_ROOT/smoke-runs/$TIMESTAMP"

if [[ "$ADHOC" == "false" ]]; then
    mkdir -p "$RUN_DIR"
else
    echo "WARNING: --adhoc mode, no run logging (not valid for verification)" >&2
fi

# Create DB directory
mkdir -p "$DB_DIR"

# Index if DB does not exist
INDEX_TIME=0
if [[ ! -f "$DB_PATH" ]]; then
    echo "Indexing $REPO_NAME..."
    START_TIME=$(date +%s)
    cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
        index "$REPO_PATH" "$DB_PATH"
    END_TIME=$(date +%s)
    INDEX_TIME=$((END_TIME - START_TIME))
    echo ""
fi

# Split COMMAND into array to handle subcommands like "boundaries list"
# shellcheck disable=SC2206
CMD_WORDS=($COMMAND)

# Build full argv for provenance
FULL_ARGV=("rmap" "${CMD_WORDS[@]}" "$DB_PATH" "$REPO_UID" "$@")

echo "Running: ${FULL_ARGV[*]}"
echo "DB: $DB_PATH"
echo ""

CMD_OUTPUT_FILE=$(mktemp)
CMD_STDERR_FILE=$(mktemp)
CMD_EXIT_CODE=0
START_TIME=$(date +%s)

# Capture stdout (JSON) separately from stderr (cargo warnings, status)
# Only stdout goes to the artifact file; stderr passes through to terminal
cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
    "${CMD_WORDS[@]}" "$DB_PATH" "$REPO_UID" "$@" > "$CMD_OUTPUT_FILE" 2> >(tee "$CMD_STDERR_FILE" >&2) || CMD_EXIT_CODE=$?

END_TIME=$(date +%s)
CMD_TIME=$((END_TIME - START_TIME))
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Display command output (JSON only - stderr already shown via tee)
cat "$CMD_OUTPUT_FILE"

# Determine status
if [[ "$CMD_EXIT_CODE" -eq 0 ]]; then
    STATUS="success"
elif [[ "$CMD_EXIT_CODE" -eq 1 ]]; then
    STATUS="no_results"
else
    STATUS="error"
fi

# Write run logs (non-adhoc only)
if [[ "$ADHOC" == "false" ]]; then
    # Build command_argv JSON array
    ARGV_JSON=$(json_array "${FULL_ARGV[@]}")

    # 00-meta.json with generator provenance
    cat > "$RUN_DIR/00-meta.json" << EOF
{
  "generator": "$GENERATOR",
  "generator_version": "$SCRIPT_VERSION",
  "baseline_shape_version": 3,
  "timestamp": "$TIMESTAMP",
  "task": "$TASK",
  "db_path": "$DB_PATH",
  "repo_uid": "$REPO_UID",
  "repo_path": "$REPO_PATH",
  "command": "$COMMAND",
  "command_argv": $ARGV_JSON,
  "started_at": "$STARTED_AT",
  "finished_at": "$FINISHED_AT",
  "exit_code": $CMD_EXIT_CODE,
  "status": "$STATUS"
}
EOF

    # Command output (sanitize command name for filename)
    CMD_FILENAME=$(echo "$COMMAND" | tr ' /' '-')
    cp "$CMD_OUTPUT_FILE" "$RUN_DIR/$CMD_FILENAME.json"

    # 92-tool-latency.json
    CMD_KEY=$(echo "$COMMAND" | tr ' ' '_')
    cat > "$RUN_DIR/92-tool-latency.json" << EOF
{
  "index_seconds": $INDEX_TIME,
  "${CMD_KEY}_seconds": $CMD_TIME
}
EOF

    # ════════════════════════════════════════════════════════════════════
    # SELF-VALIDATION: verify all required artifacts exist and are valid
    # ════════════════════════════════════════════════════════════════════
    VALIDATION_FAILED=false

    # Check 00-meta.json exists
    if [[ ! -f "$RUN_DIR/00-meta.json" ]]; then
        echo "VALIDATION ERROR: 00-meta.json not created" >&2
        VALIDATION_FAILED=true
    fi

    # Check command output exists
    if [[ ! -f "$RUN_DIR/$CMD_FILENAME.json" ]]; then
        echo "VALIDATION ERROR: $CMD_FILENAME.json not created" >&2
        VALIDATION_FAILED=true
    fi

    # Check 92-tool-latency.json exists
    if [[ ! -f "$RUN_DIR/92-tool-latency.json" ]]; then
        echo "VALIDATION ERROR: 92-tool-latency.json not created" >&2
        VALIDATION_FAILED=true
    fi

    # Verify generator field in 00-meta.json (anti-forgery check)
    if [[ -f "$RUN_DIR/00-meta.json" ]]; then
        if ! grep -q "\"generator\": \"$GENERATOR\"" "$RUN_DIR/00-meta.json"; then
            echo "VALIDATION ERROR: 00-meta.json missing generator field" >&2
            VALIDATION_FAILED=true
        fi
        if ! grep -q "\"generator_version\": \"$SCRIPT_VERSION\"" "$RUN_DIR/00-meta.json"; then
            echo "VALIDATION ERROR: 00-meta.json missing generator_version field" >&2
            VALIDATION_FAILED=true
        fi
        if ! grep -q "\"command_argv\":" "$RUN_DIR/00-meta.json"; then
            echo "VALIDATION ERROR: 00-meta.json missing command_argv field" >&2
            VALIDATION_FAILED=true
        fi
    fi

    if [[ "$VALIDATION_FAILED" == "true" ]]; then
        echo "" >&2
        echo "FATAL: Artifact validation failed. Run is invalid." >&2
        echo "Run directory: $RUN_DIR" >&2
        rm -f "$CMD_OUTPUT_FILE" "$CMD_STDERR_FILE"
        exit 3
    fi

    echo ""
    echo "Run logged: $RUN_DIR"
    echo "Artifacts validated: 00-meta.json, $CMD_FILENAME.json, 92-tool-latency.json"
fi

rm -f "$CMD_OUTPUT_FILE" "$CMD_STDERR_FILE"

# Cleanup or retain
echo ""
echo "---"
echo "DB path: $DB_PATH"

if [[ "$CMD_EXIT_CODE" -eq 0 && "$RETAIN" == "false" ]]; then
    rm -rf "$DB_DIR"
    echo "Disposal: deleted (success, default lifecycle)"
elif [[ "$RETAIN" == "true" ]]; then
    echo "Disposal: RETAINED (--retain flag)"
else
    echo "Disposal: RETAINED (command failed, exit code $CMD_EXIT_CODE)"
fi

exit $CMD_EXIT_CODE
