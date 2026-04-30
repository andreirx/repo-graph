#!/usr/bin/env bash
# smoke-rmap.sh — smoke test rmap commands with canonical DB paths and run logging
#
# Usage:
#   ./scripts/smoke-rmap.sh <task> <repo-path> <command> [args...]
#
# Examples:
#   ./scripts/smoke-rmap.sh slice-12 . trust
#   ./scripts/smoke-rmap.sh pf-2 ../legacy-codebases/swupdate policy --kind BEHAVIORAL_MARKER
#   ./scripts/smoke-rmap.sh modules-test . modules list
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
#   smoke-runs/<timestamp>/00-meta.json
#   smoke-runs/<timestamp>/<command>.json
#   smoke-runs/<timestamp>/92-tool-latency.json
#
# Protocol requirement:
#   All slice verification, production-fix validation, and validation-repo
#   smoke runs MUST be logged. Use --adhoc only for non-protocol exploratory
#   runs that do not constitute verification evidence.

set -euo pipefail

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
    echo "  task       — task identifier (e.g., slice-12, pf-2, modules-test)" >&2
    echo "  repo-path  — path to repo (relative to APLICATII BIJUTERIE or absolute)" >&2
    echo "  command    — rmap command (trust, modules, policy, etc.)" >&2
    echo "  args       — additional arguments to the command" >&2
    exit 1
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

# Run command and capture output
echo "Running: rmap $COMMAND $DB_PATH $REPO_UID $*"
echo "DB: $DB_PATH"
echo ""

CMD_OUTPUT_FILE=$(mktemp)
CMD_EXIT_CODE=0
START_TIME=$(date +%s)

cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
    "$COMMAND" "$DB_PATH" "$REPO_UID" "$@" > "$CMD_OUTPUT_FILE" 2>&1 || CMD_EXIT_CODE=$?

END_TIME=$(date +%s)
CMD_TIME=$((END_TIME - START_TIME))

# Display output
cat "$CMD_OUTPUT_FILE"

# Write run logs
if [[ "$ADHOC" == "false" ]]; then
    # 00-meta.json
    cat > "$RUN_DIR/00-meta.json" << EOF
{
  "task": "$TASK",
  "db_path": "$DB_PATH",
  "repo_uid": "$REPO_UID",
  "repo_path": "$REPO_PATH",
  "command": "$COMMAND",
  "args": "$*",
  "baseline_shape_version": 3,
  "timestamp": "$TIMESTAMP",
  "exit_code": $CMD_EXIT_CODE
}
EOF

    # Command output (sanitize command name for filename)
    CMD_FILENAME=$(echo "$COMMAND" | tr ' /' '-')
    cp "$CMD_OUTPUT_FILE" "$RUN_DIR/$CMD_FILENAME.json"

    # 92-tool-latency.json
    cat > "$RUN_DIR/92-tool-latency.json" << EOF
{
  "index_seconds": $INDEX_TIME,
  "${COMMAND}_seconds": $CMD_TIME
}
EOF

    echo ""
    echo "Run logged: $RUN_DIR"
fi

rm -f "$CMD_OUTPUT_FILE"

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
