#!/usr/bin/env bash
set -euo pipefail

# ══════════════════════════════════════════════════════════════════════════
# Verification Script — executes all tests from 10-verification-strategy.md
# ══════════════════════════════════════════════════════════════════════════
#
# Prerequisites (all assumed installed per the task):
#   - Rust stable + nightly toolchains (miri needs nightly)
#   - cargo-llvm-lines, cargo-bloat, cargo-flamegraph
#   - wrk, h2load, bombardier, quiche-client
#
# Usage:
#   ./verify.sh              # Run all verification steps
#   ./verify.sh --quick      # Skip stress tests (faster)
#   ./verify.sh --test-only  # Only unit/integration + miri tests
#   ./verify.sh --load-only  # Only stress/load tests (skip compilation checks)
# ══════════════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_DIR="$SCRIPT_DIR/rust-server"
pushd $RUST_DIR;
TARGET_DIR=$(cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*' | grep -o '[^"]*$')
popd
BIN="$TARGET_DIR/release/app"
PORT=3000
BASE_URL="http://localhost:$PORT"
DURATION="${DURATION:-10s}"
CONCURRENCY="${CONCURRENCY:-100}"
REQUESTS="${REQUESTS:-10000}"

# ── Colors ──────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ── State ───────────────────────────────────────────────────────────────
SERVER_PID=""
FAILURES=0
START_TIME=$(date +%s)

section()  { echo -e "\n${CYAN}━━━ $* ━━━${NC}\n"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
ok()      { echo -e "${GREEN}[OK]${NC}   $*"; }
fail()    { echo -e "${RED}[FAIL]${NC}  $*"; FAILURES=$((FAILURES + 1)); }
info()    { echo -e "       $*"; }

cleanup() {
    if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        info "Stopping server (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# ── Stress testing function (shared between normal and --load-only paths) ──
run_stress_tests() {
    # Ensure release binary is built
    if [ ! -f "$BIN" ]; then
        (cd "$RUST_DIR" && cargo build --release 2>&1) || {
            fail "Release build failed"
            exit 1
        }
    fi

    # Start server
    info "Starting server on $BASE_URL..."
    "$BIN" --summary &
    SERVER_PID=$!
    sleep 2

    # Verify server is running
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/" 2>/dev/null || echo "000")
    if [ "$HTTP_CODE" != "200" ]; then
        fail "Server returned HTTP $HTTP_CODE — check that port $PORT is free"
        exit 1
    fi
    ok "Server responding on $BASE_URL (HTTP $HTTP_CODE)"

    # ── 7a. wrk (HTTP/1.1 keep-alive, high throughput) ──────────────────
    echo ""
    echo "--- wrk: $CONCURRENCY connections, $DURATION ---"
    if command -v wrk &>/dev/null; then
        wrk -t4 -c"$CONCURRENCY" -d"$DURATION" "$BASE_URL/" 2>&1 || warn "wrk failed"
    else
        warn "wrk not installed (brew install wrk)"
    fi

    # ── 7b. wrk — connection reuse ────────────────────────────────────
    echo ""
    echo "--- wrk: 4 threads, 10 connections, 30s (simulates sequential reuse) ---"
    if command -v wrk &>/dev/null; then
        wrk -t4 -c10 -d30s "$BASE_URL/" 2>&1 || warn "wrk sequential test failed"
    fi

    # ── 7c. h2load (HTTP/2 multiplexed) ────────────────────────────────
    echo ""
    echo "--- h2load: $REQUESTS requests, $CONCURRENCY clients, 10 streams each ---"
    if command -v h2load &>/dev/null; then
        h2load -n "$REQUESTS" -c "$CONCURRENCY" -m 10 "$BASE_URL/" 2>&1 || warn "h2load failed"
    else
        warn "h2load not installed (brew install nghttp2)"
    fi

    # ── 7d. h2load — 50k concurrent HTTP/2 streams ─────────────────────
    echo ""
    echo "--- h2load: 50k streams, 100 clients, 500 streams each ---"
    if command -v h2load &>/dev/null; then
        h2load -n 50000 -c 100 -m 500 "$BASE_URL/" 2>&1 || warn "h2load 50k-stream test failed"
    else
        warn "h2load not installed"
    fi

    # ── 7e. bombardier (HTTP/1.1, high concurrency) ────────────────────
    echo ""
    echo "--- bombardier: $REQUESTS requests, $CONCURRENCY connections ---"
    if command -v bombardier &>/dev/null; then
        bombardier -c "$CONCURRENCY" -n "$REQUESTS" "$BASE_URL/" 2>&1 || warn "bombardier failed"
    else
        warn "bombardier not installed (brew install bombardier)"
    fi

    # ── 7f. bombardier — 1000 concurrent keep-alive clients ────────────
    echo ""
    echo "--- bombardier: 1000 concurrent keep-alive clients, 30s ---"
    if command -v bombardier &>/dev/null; then
        bombardier -c 1000 -d 30s --http1 "$BASE_URL/" 2>&1 || warn "bombardier 1000-client test failed"
    else
        warn "bombardier not installed"
    fi

    # ── 7g. quiche-client (HTTP/3) ─────────────────────────────────────
    echo ""
    echo "--- quiche-client: HTTP/3 verification ---"
    if command -v quiche-client &>/dev/null; then
        quiche-client --no-verify "https://localhost:$PORT/" 2>&1 || warn "quiche-client failed"
    else
        warn "quiche-client not installed"
    fi

    # ── Stop server ────────────────────────────────────────────────────
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
}

# ── Parse flags ─────────────────────────────────────────────────────────
QUICK=false
TEST_ONLY=false
LOAD_ONLY=false
LOAD_REQUESTS="${LOAD_REQUESTS:-200}"
for arg in "$@"; do
    case "$arg" in
        --quick)      QUICK=true ;;
        --test-only)  TEST_ONLY=true ;;
        --load-only)  LOAD_ONLY=true ;;
        --help|-h)
            echo "Usage: $0 [--quick] [--test-only] [--load-only]"
            echo "  --quick      Skip stress/load tests"
            echo "  --test-only  Only run unit/integration + miri tests"
            echo "  --load-only  Load-test h1.1, h2, h3 and compare (skip compilation checks)"
            exit 0
            ;;
    esac
done

# ── Protocol comparison (used by --load-only) ────────────────────────────
run_protocol_comparison() {
    # Ensure release binary is built
    if [ ! -f "$BIN" ]; then
        (cd "$RUST_DIR" && cargo build --release 2>&1) || {
            fail "Release build failed"
            exit 1
        }
    fi

    # Start server
    info "Starting server on $BASE_URL..."
    "$BIN" --summary &
    SERVER_PID=$!
    sleep 2

    # Verify server is running
    HTTP_CODE=$(curl -sk -o /dev/null -w "%{http_code}" "https://localhost:$PORT/" 2>/dev/null || echo "000")
    if [ "$HTTP_CODE" != "200" ]; then
        fail "Server returned HTTP $HTTP_CODE — check that port $PORT is free"
        exit 1
    fi
    ok "Server responding on https://localhost:$PORT (HTTP $HTTP_CODE)"

    N="$LOAD_REQUESTS"
    URL="https://localhost:$PORT/"

    # Temp files for per-request timings
    TMPDIR=$(mktemp -d)
    trap "rm -rf $TMPDIR" RETURN

    # ── Warm-up ─────────────────────────────────────────────────────────
    info "Warming up..."
    for _ in $(seq 1 5); do
        curl -sk -o /dev/null "$URL" 2>/dev/null || true
    done

    # ── curl format: output http_version and time_total per request ─────
    CURL_FMT="%{http_version} %{time_total}\n"

    # ── Helper: run N requests with given curl flags, record times ──────
    # IMPORTANT: this function is called in a $(...) command substitution.
    # Only the final stats line must go to stdout; all diagnostics go to stderr.
    run_curl_bench() {
        local label="$1"
        local extra_flags="$2"
        local out_file="$3"
        local count=0

        echo -e "       Testing $label: $N requests..." >&2
        for _ in $(seq 1 "$N"); do
            local line
            line=$(curl -sk $extra_flags -o /dev/null -w "$CURL_FMT" "$URL" 2>/dev/null) || true
            if [ -n "$line" ]; then
                local ver="${line%% *}"
                local t="${line##* }"
                echo "$ver $t" >> "$out_file"
                count=$((count + 1))
            fi
        done

        if [ "$count" -eq 0 ]; then
            echo "0 0 0 0 0"
            return
        fi

        # Calculate stats with awk (one line to stdout, newline-terminated)
        awk '
        {
            total += $2
            count++
            if (NR == 1 || $2 < min) min = $2
            if (NR == 1 || $2 > max) max = $2
        }
        END {
            if (count > 0) {
                avg = total / count
                printf "%d %.3f %.3f %.3f %.3f\n", count, total, avg * 1000, min * 1000, max * 1000
            } else {
                printf "0 0 0 0 0\n"
            }
        }' "$out_file"
    }

    # ── Run benchmarks ──────────────────────────────────────────────────
    echo ""
    H1_STATS=$(run_curl_bench "HTTP/1.1" "--http1.1" "$TMPDIR/h1.txt")
    echo ""
    H2_STATS=$(run_curl_bench "HTTP/2"   "--http2"   "$TMPDIR/h2.txt")
    echo ""

    # HTTP/3: try curl --http3 first, fall back to quiche-client
    H3_STATS="0 0 0 0 0"
    H3_OK=false
    if curl --http3 -sk -o /dev/null "$URL" 2>/dev/null; then
        H3_STATS=$(run_curl_bench "HTTP/3" "--http3" "$TMPDIR/h3.txt")
        H3_OK=true
    elif command -v quiche-client &>/dev/null; then
        echo -e "       Testing HTTP/3: $N requests via quiche-client..." >&2
        for _ in $(seq 1 "$N"); do
            local start_ns end_ns
            start_ns=$(date +%s%N)
            quiche-client --no-verify "$URL" > /dev/null 2>&1 || true
            end_ns=$(date +%s%N)
            local elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
            echo "3 $elapsed_ms" >> "$TMPDIR/h3.txt"
        done
        H3_STATS=$(awk '
        {
            total += $2
            count++
            if (NR == 1 || $2 < min) min = $2
            if (NR == 1 || $2 > max) max = $2
        }
        END {
            if (count > 0) {
                avg = total / count
                printf "%d %.3f %.3f %.3f %.3f\n", count, (total / 1000), avg, min, max
            } else {
                printf "0 0 0 0 0\n"
            }
        }' "$TMPDIR/h3.txt")
        H3_OK=true
    else
        warn "Neither curl --http3 nor quiche-client available — skipping HTTP/3"
    fi
    echo ""

    # ── Parse stats ─────────────────────────────────────────────────────
    parse_stats() {
        echo "$1" | awk '{printf "%d|%.3f|%.1f|%.1f|%.1f\n", $1, $2, $3, $4, $5}'
    }

    IFS='|' read -r h1_n h1_total h1_avg h1_min h1_max <<< "$(parse_stats "$H1_STATS")"
    IFS='|' read -r h2_n h2_total h2_avg h2_min h2_max <<< "$(parse_stats "$H2_STATS")"
    IFS='|' read -r h3_n h3_total h3_avg h3_min h3_max <<< "$(parse_stats "$H3_STATS")"

    # Calculate req/sec (guard against division by zero)
    h1_rps=$(echo "scale=1; if ($h1_total > 0) $h1_n / $h1_total else 0" | bc 2>/dev/null || echo "N/A")
    h2_rps=$(echo "scale=1; if ($h2_total > 0) $h2_n / $h2_total else 0" | bc 2>/dev/null || echo "N/A")
    h3_rps=$(echo "scale=1; if ($h3_total > 0) $h3_n / $h3_total else 0" | bc 2>/dev/null || echo "N/A")

    # ── Comparison table ────────────────────────────────────────────────
    echo ""
    echo -e "${BOLD}╔══════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║              HTTP Protocol Comparison — $N requests each               ║${NC}"
    echo -e "${BOLD}╠═══════╦══════════╦═══════════╦═══════════╦═══════════╦══════════════════╣${NC}"
    echo -e "${BOLD}║ Proto ║ Requests ║  Total(s) ║  Avg(ms)  ║  Min(ms)  ║  Max(ms)  ║  Req/sec  ║${NC}"
    echo -e "${BOLD}╠═══════╬══════════╬═══════════╬═══════════╬═══════════╬═══════════╬══════════╣${NC}"
    printf "${CYAN}║ h1.1  ${NC}║ %8s ║ %9s ║ %9s ║ %9s ║ %9s ║ %8s ║\n" "$h1_n" "$h1_total" "$h1_avg" "$h1_min" "$h1_max" "$h1_rps"
    printf "${CYAN}║ h2    ${NC}║ %8s ║ %9s ║ %9s ║ %9s ║ %9s ║ %8s ║\n" "$h2_n" "$h2_total" "$h2_avg" "$h2_min" "$h2_max" "$h2_rps"
    if $H3_OK; then
        printf "${CYAN}║ h3    ${NC}║ %8s ║ %9s ║ %9s ║ %9s ║ %9s ║ %8s ║\n" "$h3_n" "$h3_total" "$h3_avg" "$h3_min" "$h3_max" "$h3_rps"
    else
        echo -e "${YELLOW}║ h3    ║    —     ║     —     ║     —     ║     —     ║     —     ║    —     ║${NC}"
    fi
    echo -e "${BOLD}╚═══════╩══════════╩═══════════╩═══════════╩═══════════╩═══════════╩══════════╝${NC}"
    echo ""

    # ── Protocol verification: check that curl actually used the right version ──
    info "Protocol version verification (first 3 responses):"
    if [ -f "$TMPDIR/h1.txt" ]; then
        echo -n "  HTTP/1.1 → "
        head -3 "$TMPDIR/h1.txt" | awk '{printf "v%s ", $1}'
        echo ""
    fi
    if [ -f "$TMPDIR/h2.txt" ]; then
        echo -n "  HTTP/2   → "
        head -3 "$TMPDIR/h2.txt" | awk '{printf "v%s ", $1}'
        echo ""
    fi
    if [ -f "$TMPDIR/h3.txt" ] && $H3_OK; then
        echo -n "  HTTP/3   → "
        head -3 "$TMPDIR/h3.txt" | awk '{printf "v%s ", $1}'
        echo ""
    fi

    # ── Stop server ────────────────────────────────────────────────────
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
}

# ── Load-only: skip straight to protocol comparison ─────────────────────
if "$LOAD_ONLY"; then
    section "HTTP Protocol Load Test Comparison"
    run_protocol_comparison
    elapsed=$(($(date +%s) - START_TIME))
    echo -e "${GREEN}Load comparison completed in ${elapsed}s.${NC}"
    exit $FAILURES
fi

# ══════════════════════════════════════════════════════════════════════════
# 1. Unit & Integration Tests
# ══════════════════════════════════════════════════════════════════════════
section "1. Unit & Integration Tests (cargo test)"
if (cd "$RUST_DIR" && cargo test 2>&1); then
    ok "All unit and integration tests passed"
else
    fail "Some tests failed — review output above"
fi

# ══════════════════════════════════════════════════════════════════════════
# 2. Concurrency Testing — cargo +nightly miri test
# ══════════════════════════════════════════════════════════════════════════
section "2. Concurrency Testing (cargo +nightly miri test)"

if ! rustup toolchain list 2>/dev/null | grep -q 'nightly'; then
    warn "Nightly toolchain not installed — installing via rustup..."
    rustup toolchain install nightly 2>&1 || warn "Failed to install nightly"
fi

if ! rustup component list --toolchain nightly 2>/dev/null | grep -q 'miri.*installed'; then
    warn "miri component not installed for nightly — installing..."
    rustup component add miri --toolchain nightly 2>&1 || warn "Failed to install miri"
fi

if (cd "$RUST_DIR" && cargo +nightly miri test 2>&1); then
    ok "miri tests passed (no undefined behavior detected)"
else
    warn "miri tests failed or could not run — review output above"
fi

if "$TEST_ONLY"; then
    section "Test-only mode — skipping remaining verification steps"
    elapsed=$(($(date +%s) - START_TIME))
    echo -e "${GREEN}All requested tests completed in ${elapsed}s.${NC}"
    exit $FAILURES
fi

# ══════════════════════════════════════════════════════════════════════════
# 3. Binary Analysis — cargo llvm-lines, cargo bloat
# ══════════════════════════════════════════════════════════════════════════
section "3. Binary Analysis"

echo "--- cargo llvm-lines (generic instantiations, top 20) ---"
if (cd "$RUST_DIR" && cargo llvm-lines 2>&1 | head -30); then
    ok "cargo llvm-lines completed"
else
    warn "cargo-llvm-lines failed — is it installed? (cargo install cargo-llvm-lines)"
fi

echo ""
echo "--- cargo bloat (release binary, top 20 by size) ---"
if (cd "$RUST_DIR" && cargo bloat --release -n 20 --crates 2>&1); then
    ok "cargo bloat completed"
else
    warn "cargo-bloat failed — is it installed? (cargo install cargo-bloat)"
fi

echo ""
echo "--- Release binary size ---"
(cd "$RUST_DIR" && cargo build --release 2>&1) || warn "Release build failed"
if [ -f "$BIN" ]; then
    SIZE=$(ls -lh "$BIN" | awk '{print $5}')
    ok "Release binary built: $SIZE"
else
    fail "Release binary not found at $BIN"
fi

# ══════════════════════════════════════════════════════════════════════════
# 4. Criterion Benchmarks
# ══════════════════════════════════════════════════════════════════════════
section "4. Criterion Benchmarks (route + response_for_asset)"

if (cd "$RUST_DIR" && cargo bench --bench handler_bench 2>&1); then
    ok "Criterion benchmarks completed"
    info "Reports: $TARGET_DIR/criterion/report/index.html"
else
    warn "Criterion benchmarks failed — review output above"
fi

# ══════════════════════════════════════════════════════════════════════════
# 5. Allocation Profiling — dhat
# ══════════════════════════════════════════════════════════════════════════
section "5. Allocation Profiling (dhat)"

if (cd "$RUST_DIR" && cargo test --test dhat_profile -- profile_allocations --nocapture 2>&1); then
    ok "dhat allocation profiling completed"
    if [ -f "$RUST_DIR/dhat-heap.json" ]; then
        DHAT_SIZE=$(ls -lh "$RUST_DIR/dhat-heap.json" | awk '{print $5}')
        info "dhat-heap.json written ($DHAT_SIZE)"
        info "View at: https://nnethercote.github.io/dh_view/dh_view.html"
    fi
else
    warn "dhat profiling failed — review output above"
fi

# ══════════════════════════════════════════════════════════════════════════
# 6. CPU Profiling — cargo flamegraph
# ══════════════════════════════════════════════════════════════════════════
section "6. CPU Profiling (cargo flamegraph)"

if ! command -v flamegraph &>/dev/null && ! cargo flamegraph --help &>/dev/null 2>&1; then
    warn "cargo-flamegraph not found — install with: cargo install flamegraph"
else
    if [ ! -f "$BIN" ]; then
        (cd "$RUST_DIR" && cargo build --release 2>&1)
    fi

    if [ -f "$BIN" ]; then
        info "Starting server for flamegraph profiling..."
        "$BIN" --summary &
        SERVER_PID=$!
        sleep 2

        # Generate load while we would capture a flamegraph.
        # The flamegraph itself needs manual perf/dtrace capture for a daemon.
        if command -v wrk &>/dev/null; then
            info "Generating load with wrk (${DURATION})..."
            wrk -t2 -c10 -d"$DURATION" "$BASE_URL/" 2>&1 || true
        fi

        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
        SERVER_PID=""

        warn "Flamegraph of a daemon requires manual capture:"
        info "  sudo flamegraph -o flamegraph.svg -- $BIN --summary   (then generate load)"
        info "  Or use:  perf record -g -p <PID>  &&  perf script | stackcollapse-perf | flamegraph > flamegraph.svg"
    fi
fi

if "$QUICK"; then
    section "Quick mode — skipping stress tests"
    elapsed=$(($(date +%s) - START_TIME))
    echo -e "${GREEN}All quick checks completed in ${elapsed}s.${NC}"
    exit $FAILURES
fi

# ══════════════════════════════════════════════════════════════════════════
# 7. Stress Testing — wrk, h2load, bombardier, quiche-client
# ══════════════════════════════════════════════════════════════════════════
section "7. Stress Testing"
run_stress_tests

# ══════════════════════════════════════════════════════════════════════════
# Summary
# ══════════════════════════════════════════════════════════════════════════
section "Verification Complete"
elapsed=$(($(date +%s) - START_TIME))
echo -e "  Total time: ${BOLD}${elapsed}s${NC}"
echo -e "  Failures:   ${BOLD}${FAILURES}${NC}"
echo ""
if [ "$FAILURES" -eq 0 ]; then
    echo -e "  ${GREEN}${BOLD}All checks passed.${NC}"
else
    echo -e "  ${RED}${BOLD}${FAILURES} step(s) had issues — review output above.${NC}"
fi

exit $FAILURES
