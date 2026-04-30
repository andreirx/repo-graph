#!/usr/bin/env bash
# smoke-validation-repos.sh — smoke test rmap on all validation repos with run logging
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
#   --retain    Keep DBs after run (default: delete on success)
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
# DB paths follow protocol:
#   /private/tmp/repo-graph-tests/<task>/<repo-name>.db
#
# Run logging (per protocol):
#   smoke-runs/<timestamp>/00-meta.json           — batch summary
#   smoke-runs/<timestamp>/<repo>-meta.json       — per-repo traceability
#   smoke-runs/<timestamp>/<repo>-<command>.json  — per-command output
#   smoke-runs/<timestamp>/92-tool-latency.json   — all timings
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
PARENT_DIR="$(cd "$REPO_ROOT/.." && pwd)"
LEGACY_BUCKET="$PARENT_DIR/legacy-codebases"

RETAIN=false
ADHOC=false

# Internal repos — explicitly listed
INTERNAL_NAMES=(
    "repo-graph"
    "amodx"
    "glamCRM"
    "hexmanos"
    "zap-engine"
)
INTERNAL_PATHS=(
    "$REPO_ROOT"
    "$PARENT_DIR/amodx"
    "$PARENT_DIR/glamCRM"
    "$PARENT_DIR/hexmanos"
    "$PARENT_DIR/zap-engine"
)

usage() {
    echo "usage: $0 [--retain] [--adhoc] <task> [commands...]" >&2
    echo "" >&2
    echo "  --retain   — keep DBs after run (default: delete on success)" >&2
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

    # Find directories, skip hidden, sort lexicographically
    while IFS= read -r dir; do
        local name
        name=$(basename "$dir")
        LEGACY_NAMES+=("$name")
        LEGACY_PATHS+=("$dir")
    done < <(find "$LEGACY_BUCKET" -mindepth 1 -maxdepth 1 -type d -not -name '.*' | sort)
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

# Default commands if none specified
if [[ $# -eq 0 ]]; then
    COMMANDS=("trust" "modules" "check")
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

DB_DIR="$TEST_ROOT/$TASK"
mkdir -p "$DB_DIR"

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
echo "DB directory: $DB_DIR"
if [[ "$ADHOC" == "false" ]]; then
    echo "Run log: $RUN_DIR"
fi
echo ""
echo "Internal repos: ${INTERNAL_NAMES[*]}"
echo "Legacy repos: ${LEGACY_NAMES[*]:-none discovered}"
echo "=============================================="
echo ""

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
    DB_PATH="$DB_DIR/$REPO_NAME.db"

    echo "----------------------------------------------"
    echo "Repo: $REPO_NAME ($REPO_CATEGORY)"
    echo "Path: $REPO_PATH"
    echo "DB: $DB_PATH"
    echo "----------------------------------------------"

    if [[ ! -d "$REPO_PATH" ]]; then
        echo "SKIP: repo path does not exist"
        SKIPPED_REPOS+=("$REPO_NAME")
        echo ""
        continue
    fi

    # Per-repo meta accumulator
    REPO_META_COMMANDS=""

    # Index if DB does not exist
    INDEX_EXIT=0
    INDEX_TIME=0
    if [[ ! -f "$DB_PATH" ]]; then
        echo "Indexing..."
        INDEX_OUTPUT=$(mktemp)
        START_TIME=$(date +%s)
        if ! run_and_capture "$INDEX_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
            index "$REPO_PATH" "$DB_PATH"; then
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
  "db_path": "$DB_PATH",
  "category": "$REPO_CATEGORY",
  "baseline_shape_version": 3,
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
    fi

    # Run each command
    REPO_FAILED=false
    for CMD in "${COMMANDS[@]}"; do
        echo ""
        echo "Command: $CMD"

        CMD_OUTPUT=$(mktemp)
        CMD_EXIT=0
        START_TIME=$(date +%s)

        case "$CMD" in
            trust)
                run_and_capture "$CMD_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
                    trust "$DB_PATH" "$REPO_NAME" || CMD_EXIT=$?
                ;;
            modules)
                run_and_capture "$CMD_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
                    modules list "$DB_PATH" "$REPO_NAME" || CMD_EXIT=$?
                ;;
            check)
                run_and_capture "$CMD_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
                    check "$DB_PATH" "$REPO_NAME" || CMD_EXIT=$?
                ;;
            orient)
                run_and_capture "$CMD_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
                    orient "$DB_PATH" "$REPO_NAME" --budget small || CMD_EXIT=$?
                ;;
            *)
                run_and_capture "$CMD_OUTPUT" cargo run --manifest-path "$MANIFEST_PATH" -p "$PACKAGE" --release -- \
                    "$CMD" "$DB_PATH" "$REPO_NAME" || CMD_EXIT=$?
                ;;
        esac

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
            echo "FAIL: $CMD (exit $CMD_EXIT)"
            display_truncated "$CMD_OUTPUT" 20
            REPO_FAILED=true
        else
            display_truncated "$CMD_OUTPUT" 30
        fi

        # Log output
        if [[ "$ADHOC" == "false" ]]; then
            CMD_FILENAME=$(echo "$CMD" | tr ' /' '-')
            cp "$CMD_OUTPUT" "$RUN_DIR/${REPO_NAME}-${CMD_FILENAME}.json"
        fi

        rm -f "$CMD_OUTPUT"
    done

    # Write per-repo meta
    if [[ "$ADHOC" == "false" ]]; then
        cat > "$RUN_DIR/${REPO_NAME}-meta.json" << EOF
{
  "repo_uid": "$REPO_NAME",
  "repo_path": "$REPO_PATH",
  "db_path": "$DB_PATH",
  "category": "$REPO_CATEGORY",
  "baseline_shape_version": 3,
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
  "type": "batch_validation",
  "task": "$TASK",
  "db_dir": "$DB_DIR",
  "baseline_shape_version": 3,
  "timestamp": "$TIMESTAMP",
  "inventory_model": "hybrid",
  "internal_repos": $INTERNAL_JSON,
  "legacy_repos": $LEGACY_JSON,
  "legacy_bucket": "$LEGACY_BUCKET",
  "commands": $CMDS_JSON,
  "passed": $PASSED_JSON,
  "failed": $FAILED_JSON,
  "skipped": $SKIPPED_JSON,
  "per_repo_meta": "See <repo>-meta.json files for per-repo db_path, category, exit codes, and timing"
}
EOF

    # 92-tool-latency.json
    cp "$TIMING_FILE" "$RUN_DIR/92-tool-latency.json"
fi

rm -f "$TIMING_FILE"

echo "=============================================="
echo "Summary"
echo "=============================================="
echo "Passed: ${#PASSED_REPOS[@]} (${PASSED_REPOS[*]:-none})"
echo "Failed: ${#FAILED_REPOS[@]} (${FAILED_REPOS[*]:-none})"
echo "Skipped: ${#SKIPPED_REPOS[@]} (${SKIPPED_REPOS[*]:-none})"
echo ""
echo "DB directory: $DB_DIR"

if [[ "$ADHOC" == "false" ]]; then
    echo "Run log: $RUN_DIR"
fi

# Cleanup or retain
if [[ ${#FAILED_REPOS[@]} -eq 0 && "$RETAIN" == "false" ]]; then
    rm -rf "$DB_DIR"
    echo "Disposal: deleted (all passed, default lifecycle)"
elif [[ "$RETAIN" == "true" ]]; then
    echo "Disposal: RETAINED (--retain flag)"
else
    echo "Disposal: RETAINED (failures detected)"
fi

if [[ ${#FAILED_REPOS[@]} -gt 0 ]]; then
    exit 1
fi
