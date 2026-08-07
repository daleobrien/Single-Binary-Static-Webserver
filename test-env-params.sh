#!/usr/bin/env bash
# ══════════════════════════════════════════════════════════════════════════
# test-env-params.sh — Verify every compile-time ENV parameter works
# ══════════════════════════════════════════════════════════════════════════
#
# Each test case:
#   1. Sets the ENV var to a non-default value
#   2. Runs `cargo test -- config::tests`
#   3. The override test for that var asserts the constant matches the env
#   4. All default tests still pass (since unrelated vars are untouched)
#
# The `cargo:rerun-if-env-changed` directives in build.rs ensure cargo
# detects the env-var change and rebuilds as needed.
#
# Usage:
#   ./test-env-params.sh          # Run all override tests
#   ./test-env-params.sh --quick  # Test only the most critical params
# ══════════════════════════════════════════════════════════════════════════
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

PASS=0
FAIL=0
SKIP=0

# Guard against DISABLE_LOGGING leaking from the environment.
# If it's set (even to empty string), the default test fails.
if [ -n "${DISABLE_LOGGING+set}" ]; then
    SAVED_DISABLE_LOGGING="$DISABLE_LOGGING"
else
    SAVED_DISABLE_LOGGING="__UNSET__"
fi

run_test() {
    local label="$1"
    local env_var="$2"
    local env_value="$3"
    local extra_env="${4:-}"

    # Unset any leaked build-time vars so only the one under test is active.
    # We don't want cached env values from previous iterations leaking.
    unset HOSTNAME PORT WORKERS MAX_CONNS TCP_HANDLERS_PER_WORKER \
          H3_HANDLERS_PER_CONNECTION SHUTDOWN_TIMEOUT_SECS \
          H2_CONN_WINDOW H2_STREAM_WINDOW H2_MAX_FRAME_SIZE H2_MAX_SEND_BUF \
          DISABLE_LOGGING DISABLE_SRI ALLOW_INLINE_STYLES NOT_FOUND_FILENAME \
          2>/dev/null || true

    export "$env_var=$env_value"
    if [[ -n "$extra_env" ]]; then
        export "$extra_env"
    fi

    printf "${CYAN}Testing %-50s${NC} " "$label (export $env_var=$env_value)"

    if TEST_OUTPUT=$(cargo test -- config::tests 2>&1); then
        echo "$TEST_OUTPUT" | grep -qE '^test result: ok\.' && {
            printf "${GREEN}PASS${NC}\n"
            PASS=$((PASS + 1))
        } || {
            printf "${RED}FAIL${NC} (tests passed but unexpected output pattern)\n"
            FAIL=$((FAIL + 1))
        }
    else
        printf "${RED}FAIL${NC}\n"
        echo "  $TEST_OUTPUT"
        FAIL=$((FAIL + 1))
    fi
}

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Compile-time ENV Parameter Override Tests                 ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

cd "$(dirname "$0")/rust-server"

# ── Quick mode: test only critical params ──────────────────────────
if [[ "${1:-}" == "--quick" ]]; then
    run_test "HOSTNAME override"             HOSTNAME             "0.0.0.0"
    run_test "PORT override"                 PORT                 "9090"
    run_test "WORKERS override"              WORKERS              "4"
    run_test "DISABLE_LOGGING override"      DISABLE_LOGGING      "false"
    run_test "SHUTDOWN_TIMEOUT_SECS override" SHUTDOWN_TIMEOUT_SECS "10"

    echo ""
    echo "──────────────────────────────────────────────────────────────"
    echo -e "Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}, $SKIP skipped"
    echo "──────────────────────────────────────────────────────────────"
    exit $((FAIL > 0 ? 1 : 0))
fi

# ── Full suite — every parameter ───────────────────────────────────

# --- string params ---
run_test "HOSTNAME = 0.0.0.0"          HOSTNAME                   "0.0.0.0"
run_test "HOSTNAME = myserver.local"   HOSTNAME                   "myserver.local"

# --- u16 params ---
run_test "PORT = 9090"                 PORT                       "9090"
run_test "PORT = 443"                  PORT                       "443"

# --- usize params ---
run_test "WORKERS = 4"                 WORKERS                    "4"
run_test "WORKERS = 1"                 WORKERS                    "1"
run_test "MAX_CONNS = 2048"            MAX_CONNS                  "2048"
run_test "MAX_CONNS = 512"             MAX_CONNS                  "512"
run_test "TCP_HANDLERS_PER_WORKER = 128" TCP_HANDLERS_PER_WORKER  "128"
run_test "H3_HANDLERS_PER_CONNECTION = 4" H3_HANDLERS_PER_CONNECTION "4"
run_test "H3_HANDLERS_PER_CONNECTION = 16" H3_HANDLERS_PER_CONNECTION "16"

# --- u64 params ---
run_test "SHUTDOWN_TIMEOUT_SECS = 10"  SHUTDOWN_TIMEOUT_SECS      "10"
run_test "SHUTDOWN_TIMEOUT_SECS = 0"   SHUTDOWN_TIMEOUT_SECS      "0"

# --- u32 params ---
run_test "H2_CONN_WINDOW = 8MiB"       H2_CONN_WINDOW             "8388608"
run_test "H2_STREAM_WINDOW = 2MiB"     H2_STREAM_WINDOW           "2097152"
run_test "H2_MAX_FRAME_SIZE = 16384"   H2_MAX_FRAME_SIZE          "16384"
run_test "H2_MAX_FRAME_SIZE = 1048576" H2_MAX_FRAME_SIZE          "1048576"

# --- usize params (continued) ---
run_test "H2_MAX_SEND_BUF = 2MiB"      H2_MAX_SEND_BUF            "2097152"

# --- bool params ---
run_test "DISABLE_LOGGING = false"     DISABLE_LOGGING            "false"
run_test "DISABLE_LOGGING = true"      DISABLE_LOGGING            "true" \
                                        "DISABLE_LOGGING_EXPECTED=1"

# Restore original DISABLE_LOGGING
if [[ "$SAVED_DISABLE_LOGGING" == "__UNSET__" ]]; then
    unset DISABLE_LOGGING 2>/dev/null || true
else
    export DISABLE_LOGGING="$SAVED_DISABLE_LOGGING"
fi

echo ""
echo "══════════════════════════════════════════════════════════════"
echo -e "Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}, $SKIP skipped"
echo "══════════════════════════════════════════════════════════════"
exit $((FAIL > 0 ? 1 : 0))
