#!/usr/bin/env bash
# test-smoke-rmap.sh — regression tests for smoke-rmap.sh
#
# Verifies:
#   - Nested subcommand handling (e.g., "boundaries list")
#   - Generator provenance fields in 00-meta.json
#   - command_argv contains correct command order
#   - All required artifacts are created
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
# Use absolute path for fixture - script expects absolute or relative to parent of repo
FIXTURE_DIR="$REPO_ROOT/test/fixtures/semaphores"

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
echo "Test 1: Nested subcommand 'boundaries list' produces correct argv"
((++TESTS_RUN))

# Run the script with a nested subcommand
TEST_RUN_DIR=""
OUTPUT=$("$SCRIPT" --retain "$TEST_TASK" "$FIXTURE_DIR" "boundaries list" --kind semaphore 2>&1) || true

# Find the run directory from output
TEST_RUN_DIR=$(echo "$OUTPUT" | grep "Run logged:" | sed 's/.*Run logged: //' | xargs -I{} basename "{}" 2>/dev/null || echo "")

if [[ -z "$TEST_RUN_DIR" || ! -d "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" ]]; then
    fail "nested subcommand argv" "Run directory not found in output"
else
    META_FILE="$REPO_ROOT/smoke-runs/$TEST_RUN_DIR/00-meta.json"
    if [[ ! -f "$META_FILE" ]]; then
        fail "nested subcommand argv" "00-meta.json not created"
    else
        # Check command_argv contains correct order: rmap, boundaries, list, <db>, <repo>, --kind, semaphore
        # The key check: "boundaries" and "list" must appear BEFORE the db path
        ARGV=$(grep -o '"command_argv":.*' "$META_FILE" | head -1)

        # Extract the array portion
        if echo "$ARGV" | grep -q '"rmap","boundaries","list"'; then
            pass "nested subcommand argv"
        else
            fail "nested subcommand argv" "command_argv has wrong order: $ARGV"
        fi
    fi
    # Cleanup this specific run
    rm -rf "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR"
fi

# ════════════════════════════════════════════════════════════════════════════
# Test 2: Generator provenance fields present
# ════════════════════════════════════════════════════════════════════════════
echo "Test 2: Generator provenance fields in 00-meta.json"
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

    if ! grep -q '"command_argv":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS command_argv"
    fi

    if ! grep -q '"started_at":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS started_at"
    fi

    if ! grep -q '"finished_at":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS finished_at"
    fi

    if ! grep -q '"status":' "$META_FILE"; then
        MISSING_FIELDS="$MISSING_FIELDS status"
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
# Test 3: All required artifacts created
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
    [[ ! -f "$RUN_PATH/trust.json" ]] && MISSING="$MISSING trust.json"
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
# Test 6: Command output is valid JSON (no cargo warnings mixed in)
# ════════════════════════════════════════════════════════════════════════════
echo "Test 6: Command output is valid JSON (no stderr pollution)"
((++TESTS_RUN))

OUTPUT=$("$SCRIPT" --retain "$TEST_TASK-json" "$FIXTURE_DIR" trust 2>&1) || true
TEST_RUN_DIR=$(echo "$OUTPUT" | grep "Run logged:" | sed 's/.*Run logged: //' | xargs -I{} basename "{}" 2>/dev/null || echo "")

if [[ -z "$TEST_RUN_DIR" || ! -d "$REPO_ROOT/smoke-runs/$TEST_RUN_DIR" ]]; then
    fail "json output" "Run directory not found"
else
    JSON_FILE="$REPO_ROOT/smoke-runs/$TEST_RUN_DIR/trust.json"
    if [[ ! -f "$JSON_FILE" ]]; then
        fail "json output" "trust.json not found"
    else
        # Check file starts with { (JSON object) not 'warning:' or 'Compiling' etc
        FIRST_CHAR=$(head -c 1 "$JSON_FILE")
        if [[ "$FIRST_CHAR" == "{" || "$FIRST_CHAR" == "[" ]]; then
            pass "json output"
        else
            FIRST_LINE=$(head -1 "$JSON_FILE")
            fail "json output" "File does not start with JSON: $FIRST_LINE"
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
