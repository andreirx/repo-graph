#!/usr/bin/env bash
# test-smoke-rmap.sh — regression tests for smoke-rmap.sh
#
# Validates the smoke-rmap.sh v4 (REG-1 daemon) artifact contract:
#   - Nested subcommand handling (e.g., "boundaries list") — preserved as
#     a single command unit, dispatched in the correct order, exits 0
#   - Generator provenance fields in 00-meta.json (v4 shape:
#     `commands` / `commands_detail` / `worst_exit_code` — NOT the
#     removed `command_argv` / `status`)
#   - All required artifacts are created (v4 emits TEXT artifacts:
#     `<command>.txt`, not `<command>.json`)
#   - --adhoc skips smoke-runs/ logging
#   - Script version is set
#   - A `--json` command emits valid JSON (fully parsed, after the
#     `cargo run` banner lines that `2>&1` prepends to the text artifact)
#
# Usage:
#   ./scripts/test-smoke-rmap.sh
#
# Exit codes:
#   0 — all tests passed
#   1 — one or more tests failed

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/smoke-rmap.sh"
TEST_TASK="smoke-script-test-$$"
# Absolute path to the living integration corpus, relocated from the
# retired `test/fixtures/` into the repo-index crate by
# TS-PROTOTYPE-RETIREMENT-1.
FIXTURE_DIR="$REPO_ROOT/rust/crates/repo-index/tests/fixtures/semaphores"

# Colors for output (if terminal supports it)
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    NC='\033[0m' # No Color
else
    RED=''
    GREEN=''
    NC=''
fi

TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
TEST_RUN_DIR=""

pass() {
    echo -e "${GREEN}PASS${NC}: $1"
    ((++TESTS_PASSED))
}

fail() {
    echo -e "${RED}FAIL${NC}: $1"
    echo "  $2"
    ((++TESTS_FAILED))
}

cleanup() {
    # Clean up test run directories
    rm -rf "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" 2>/dev/null || true
    rm -rf "/private/tmp/repo-graph-tests/$TEST_TASK" 2>/dev/null || true
}

trap cleanup EXIT

echo "=== smoke-rmap.sh regression tests ==="
echo ""

# ════════════════════════════════════════════════════════════════════════════
# Test 1: Nested subcommand handling
# ════════════════════════════════════════════════════════════════════════════
# v4 records each command as one entry in `commands` / `commands_detail`.
# Passing the nested subcommand as a single argument ("boundaries list")
# proves it stays one unit; exit_code 0 proves rmap dispatched it as a
# nested subcommand (boundaries -> list) rather than mis-splitting it; and
# the artifact's `Running` banner line captures the invoked argv order
# (the direct v4 replacement for the removed `command_argv` order check).
echo "Test 1: Nested subcommand 'boundaries list' is dispatched correctly"
((++TESTS_RUN))

TEST_RUN_DIR=""
OUTPUT=$("$SCRIPT" --retain "$TEST_TASK" "$FIXTURE_DIR" "boundaries list" 2>&1) || true

# Find the run directory from output
TEST_RUN_DIR=$(echo "$OUTPUT" | grep "Run logged:" | sed 's/.*Run logged: //' | xargs -I{} basename "{}" 2>/dev/null || echo "")

if [[ -z "$TEST_RUN_DIR" || ! -d "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" ]]; then
    fail "nested subcommand" "Run directory not found in output"
else
    RUN_PATH="$REPO_ROOT/smoke-runs/$TEST_RUN_DIR"
    META_FILE="$RUN_PATH/00-meta.json"
    ART_FILE="$RUN_PATH/boundaries-list.txt"
    ERR=""

    if [[ ! -f "$META_FILE" ]]; then
        ERR="00-meta.json not created"
    elif ! grep -q '"boundaries list"' "$META_FILE"; then
        # Nested command must survive as ONE element of `commands`.
        ERR="nested 'boundaries list' unit missing from commands"
    elif ! grep -q '"boundaries list": {"exit_code": 0' "$META_FILE"; then
        # Nested command must have dispatched and exited 0.
        ERR="nested 'boundaries list' did not exit 0 in commands_detail"
    elif [[ ! -f "$ART_FILE" ]]; then
        # v4 writes a TEXT artifact named after the command.
        ERR="boundaries-list.txt artifact not created"
    elif ! grep -q 'rmap boundaries list' "$ART_FILE"; then
        # The cargo-run banner records the invoked argv; boundaries and
        # list appear in order (the v4 analogue of the old argv check).
        ERR="invoked argv 'rmap boundaries list' not found (nested order not preserved)"
    fi

    if [[ -z "$ERR" ]]; then
        pass "nested subcommand"
    else
        fail "nested subcommand" "$ERR"
    fi
    # Cleanup this specific run
    rm -rf "$RUN_PATH"
fi

rm -rf "/private/tmp/repo-graph-tests/$TEST_TASK"

# ════════════════════════════════════════════════════════════════════════════
# Test 2: Generator provenance fields present (v4 shape)
# ════════════════════════════════════════════════════════════════════════════
echo "Test 2: Generator provenance fields in 00-meta.json (v4 shape)"
((++TESTS_RUN))

OUTPUT=$("$SCRIPT" --retain "$TEST_TASK-gen" "$FIXTURE_DIR" trust 2>&1) || true
TEST_RUN_DIR=$(echo "$OUTPUT" | grep "Run logged:" | sed 's/.*Run logged: //' | xargs -I{} basename "{}" 2>/dev/null || echo "")

if [[ -z "$TEST_RUN_DIR" || ! -d "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" ]]; then
    fail "generator provenance" "Run directory not found"
else
    META_FILE="$REPO_ROOT/smoke-runs/$TEST_RUN_DIR/00-meta.json"

    MISSING_FIELDS=""

    if ! grep -q '"generator": "smoke-rmap.sh"' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS generator"
    fi

    if ! grep -q '"generator_version":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS generator_version"
    fi

    if ! grep -q '"baseline_shape_version":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS baseline_shape_version"
    fi

    if ! grep -q '"timestamp":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS timestamp"
    fi

    # v4 replaced the flat `command_argv` with the per-command
    # `commands` array + `commands_detail` map.
    if ! grep -q '"commands":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS commands"
    fi

    if ! grep -q '"commands_detail":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS commands_detail"
    fi

    if ! grep -q '"started_at":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS started_at"
    fi

    if ! grep -q '"finished_at":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS finished_at"
    fi

    # v4 replaced the string `status` with the numeric `worst_exit_code`.
    if ! grep -q '"worst_exit_code":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS worst_exit_code"
    fi

    if [[ -z "$MISSING_FIELDS" ]]; then
        pass "generator provenance"
    else
        fail "generator provenance" "Missing fields:$MISSING_FIELDS"
    fi

    rm -rf "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR"
fi

rm -rf "/private/tmp/repo-graph-tests/$TEST_TASK-gen"

# ════════════════════════════════════════════════════════════════════════════
# Test 3: All required artifacts created (v4 text artifacts)
# ════════════════════════════════════════════════════════════════════════════
echo "Test 3: All required artifacts created"
((++TESTS_RUN))

OUTPUT=$("$SCRIPT" --retain "$TEST_TASK-art" "$FIXTURE_DIR" trust 2>&1) || true
TEST_RUN_DIR=$(echo "$OUTPUT" | grep "Run logged:" | sed 's/.*Run logged: //' | xargs -I{} basename "{}" 2>/dev/null || echo "")

if [[ -z "$TEST_RUN_DIR" || ! -d "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" ]]; then
    fail "artifact creation" "Run directory not found"
else
    RUN_PATH="$REPO_ROOT/smoke-runs/$TEST_RUN_DIR"
    MISSING=""

    [[ ! -f "$RUN_PATH/00-meta.json" ]] && MISSING="$MISSING 00-meta.json"
    # v4 writes the command output as `<command>.txt` (human mode default),
    # not `<command>.json`.
    [[ ! -f "$RUN_PATH/trust.txt" ]] && MISSING="$MISSING trust.txt"
    [[ ! -f "$RUN_PATH/92-tool-latency.json" ]] && MISSING="$MISSING 92-tool-latency.json"

    if [[ -z "$MISSING" ]]; then
        pass "artifact creation"
    else
        fail "artifact creation" "Missing:$MISSING"
    fi

    rm -rf "$RUN_PATH"
fi

rm -rf "/private/tmp/repo-graph-tests/$TEST_TASK-art"

# ════════════════════════════════════════════════════════════════════════════
# Test 4: Adhoc mode skips logging
# ════════════════════════════════════════════════════════════════════════════
echo "Test 4: --adhoc mode skips smoke-runs logging"
((++TESTS_RUN))

BEFORE_COUNT=$(ls -1 "$REPO_ROOT/smoke-runs/" 2>/dev/null | wc -l)
OUTPUT=$("$SCRIPT" --adhoc --retain "$TEST_TASK-adhoc" "$FIXTURE_DIR" trust 2>&1) || true
AFTER_COUNT=$(ls -1 "$REPO_ROOT/smoke-runs/" 2>/dev/null | wc -l)

if [[ "$BEFORE_COUNT" -eq "$AFTER_COUNT" ]]; then
    pass "adhoc mode skips logging"
else
    fail "adhoc mode skips logging" "smoke-runs directory count changed"
fi

rm -rf "/private/tmp/repo-graph-tests/$TEST_TASK-adhoc"

# ════════════════════════════════════════════════════════════════════════════
# Test 5: Script version is set
# ════════════════════════════════════════════════════════════════════════════
echo "Test 5: Script has version number"
((++TESTS_RUN))

if grep -q '^SCRIPT_VERSION="[0-9]' "$SCRIPT"; then
    pass "script version set"
else
    fail "script version set" "SCRIPT_VERSION not found or invalid"
fi

# ════════════════════════════════════════════════════════════════════════════
# Test 6: --json command output is valid JSON
# ════════════════════════════════════════════════════════════════════════════
# v4 text artifacts carry the `cargo run` banner (Finished/Running lines,
# via 2>&1) BEFORE the command output, so the pre-v4 "first char is {"
# check no longer holds. Skip the banner (JSON begins at the first line
# starting with { or [) and fully parse the remainder — a strict parse is
# stronger than a first-character check, so the JSON-output assertion is
# preserved, not weakened.
echo "Test 6: --json command output is valid JSON (no stderr pollution)"
((++TESTS_RUN))

OUTPUT=$("$SCRIPT" --retain "$TEST_TASK-json" "$FIXTURE_DIR" "trust --json" 2>&1) || true
TEST_RUN_DIR=$(echo "$OUTPUT" | grep "Run logged:" | sed 's/.*Run logged: //' | xargs -I{} basename "{}" 2>/dev/null || echo "")

if [[ -z "$TEST_RUN_DIR" || ! -d "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" ]]; then
    fail "json output" "Run directory not found"
else
    # `trust --json` -> tr ' /' '-' -> trust---json.txt
    JSON_FILE="$REPO_ROOT/smoke-runs/$TEST_RUN_DIR/trust---json.txt"
    if [[ ! -f "$JSON_FILE" ]]; then
        fail "json output" "trust---json.txt not found"
    elif ! command -v jq >/dev/null 2>&1; then
        fail "json output" "jq is required to validate --json output but was not found on PATH"
    else
        if sed -n '/^[[{]/,$p' "$JSON_FILE" | jq empty >/dev/null 2>&1; then
            pass "json output"
        else
            FIRST_JSON_LINE=$(sed -n '/^[[{]/,$p' "$JSON_FILE" | head -1)
            fail "json output" "trust --json artifact is not valid JSON after the cargo banner (first JSON-looking line: ${FIRST_JSON_LINE:-<none>})"
        fi
    fi
    rm -rf "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR"
fi

rm -rf "/private/tmp/repo-graph-tests/$TEST_TASK-json"

# ════════════════════════════════════════════════════════════════════════════
# Summary
# ════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Results ==="
echo "Tests run: $TESTS_RUN"
echo "Passed: $TESTS_PASSED"
echo "Failed: $TESTS_FAILED"

if [[ "$TESTS_FAILED" -gt 0 ]]; then
    exit 1
fi

exit 0
