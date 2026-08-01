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
LOAD_REQUESTS="${LOAD_REQUESTS:-10000}"
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

    # Locate brew's curl (has HTTP/3 support; fall back to system curl)
    BREW_CURL="curl"
    for candidate in "/opt/homebrew/opt/curl/bin/curl" "/usr/local/opt/curl/bin/curl"; do
        if [ -x "$candidate" ]; then BREW_CURL="$candidate"; break; fi
    done

    # ── Protocol verification (single request per protocol) ────────────
    echo ""
    info "Protocol verification (via $BREW_CURL):"

    H1_VER=$("$BREW_CURL" -sk --http1.1 -o /dev/null -w "%{http_version}" "$URL" 2>/dev/null || echo "0")
    if [ "$H1_VER" != "0" ]; then
        ok "HTTP/1.1 → v$H1_VER"
        H1_OK=true
    else
        warn "HTTP/1.1 not available"
        H1_OK=false
    fi

    H2_VER=$("$BREW_CURL" -sk --http2 -o /dev/null -w "%{http_version}" "$URL" 2>/dev/null || echo "0")
    if [ "$H2_VER" != "0" ]; then
        ok "HTTP/2   → v$H2_VER"
        H2_OK=true
    else
        warn "HTTP/2 not available"
        H2_OK=false
    fi

    H3_VER=$("$BREW_CURL" -sk --http3 -o /dev/null -w "%{http_version}" "$URL" 2>/dev/null || echo "0")
    if [ "$H3_VER" != "0" ]; then
        ok "HTTP/3   → v$H3_VER"
        H3_OK=true
    else
        warn "HTTP/3 not available (need brew curl with HTTP/3 support)"
        H3_OK=false
    fi

    # ── Warm-up ───────────────────────────────────────────────────────
    info "Warming up..."
    for _ in $(seq 1 10); do
        curl -sk -o /dev/null "$URL" 2>/dev/null || true
    done

    # ── Helper: parse h2load latency + throughput output ───────────────
    # h2load latency columns:  min  max  median  p95  p99  mean  sd  +/-sd
    #   request     :          $3   $4   $5      $6   $7   $8    $9  $10
    # Aggregate throughput comes from the "finished in" line:
    #   finished in X.XX<unit>, YY req/s, ZZ bytes/s  → total=$3, rps=$4
    parse_h2load() {
        echo "$1" | awk '
        /^finished in/ {
            val = $3; gsub(/,/, "", val)
            if (val ~ /us$/)      { gsub(/us$/, "", val); total = val / 1000000 }
            else if (val ~ /ms$/) { gsub(/ms$/, "", val); total = val / 1000 }
            else if (val ~ /s$/)  { gsub(/s$/, "", val);  total = val }
            rps = $4 + 0   # aggregate req/s (not per-connection)
        }
        /^request[[:space:]]+:/ && !/req\// {
            lat_min = conv_ms($3)
            lat_max = conv_ms($4)
            lat_avg = conv_ms($8)
        }
        END {
            printf "%.1f|%.1f|%.1f|%.1f|%.3f\n", \
                lat_min + 0, lat_max + 0, lat_avg + 0, rps + 0, total + 0
        }
        function conv_ms(v) {
            if (v == "") return 0
            if (v ~ /us$/) { gsub(/us$/, "", v); return v / 1000 }
            if (v ~ /ms$/) { gsub(/ms$/, "", v); return v }
            if (v ~ /s$/)  { gsub(/s$/, "", v);  return v * 1000 }
            return v + 0
        }'
    }

    # ── HTTP/1.1 benchmark (h2load --h1, 50 concurrent connections) ────
    echo ""
    if $H1_OK && command -v h2load &>/dev/null; then
        info "Benchmarking HTTP/1.1: $N requests via h2load (50 clients)..."
        H1_OUT=$(h2load -n "$N" -c 50 -m 1 --h1 "$URL" 2>&1) || true
        IFS='|' read -r h1_min h1_max h1_avg h1_rps h1_total <<< "$(parse_h2load "$H1_OUT")"
        ok "HTTP/1.1  avg=${h1_avg}ms  rps=${h1_rps}  total=${h1_total}s"
    else
        h1_min=0; h1_max=0; h1_avg=0; h1_rps=0; h1_total=0
    fi

    # ── HTTP/2 benchmark (h2load, 10 clients × 10 streams each) ────────
    echo ""
    if $H2_OK && command -v h2load &>/dev/null; then
        info "Benchmarking HTTP/2: $N requests via h2load (10 clients × 10 streams)..."
        H2_OUT=$(h2load -n "$N" -c 10 -m 10 "$URL" 2>&1) || true
        IFS='|' read -r h2_min h2_max h2_avg h2_rps h2_total <<< "$(parse_h2load "$H2_OUT")"
        ok "HTTP/2    avg=${h2_avg}ms  rps=${h2_rps}  total=${h2_total}s"
    else
        h2_min=0; h2_max=0; h2_avg=0; h2_rps=0; h2_total=0
    fi

    # ── HTTP/3 benchmark (parallel curl --http3, 10-way concurrency) ───
    echo ""
    if $H3_OK; then
        info "Benchmarking HTTP/3: $N requests via parallel curl (10 concurrent)..."
        TMPDIR=$(mktemp -d)

        # Warm up QUIC connection
        "$BREW_CURL" -sk --http3 -o /dev/null "$URL" 2>/dev/null || true
        sleep 1

        # Run N requests with xargs -P for 10-way parallelism
        seq 1 "$N" | xargs -P 10 -I {} "$BREW_CURL" -sk --http3 -o /dev/null -w "%{time_total}\n" "$URL" >> "$TMPDIR/h3.txt" 2>/dev/null

        H3_STATS=$(awk '
        {
            total += $1
            count++
            if (NR == 1 || $1 < min) min = $1
            if (NR == 1 || $1 > max) max = $1
        }
        END {
            if (count > 0) {
                avg_ms = (total / count) * 1000
                min_ms = min * 1000
                max_ms = max * 1000
                rps    = count / total
                printf "%.1f|%.1f|%.1f|%.1f|%.3f\n", min_ms, max_ms, avg_ms, rps, total
            } else {
                printf "0|0|0|0|0\n"
            }
        }' "$TMPDIR/h3.txt")
        IFS='|' read -r h3_min h3_max h3_avg h3_rps h3_total <<< "$H3_STATS"

        rm -rf "$TMPDIR"
        ok "HTTP/3    avg=${h3_avg}ms  rps=${h3_rps}  total=${h3_total}s"
    else
        h3_min=0; h3_max=0; h3_avg=0; h3_rps=0; h3_total=0
    fi

    echo ""

    # ── Comparison table ──────────────────────────────────────────────
    echo -e "${BOLD}╔═════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║                HTTP Protocol Comparison — $N requests each                 ║${NC}"
    echo -e "${BOLD}╠═══════╦══════════╦═══════════╦═══════════╦═══════════╦═══════════╦══════════╣${NC}"
    echo -e "${BOLD}║ Proto ║ Requests ║  Total(s) ║  Avg(ms)  ║  Min(ms)  ║  Max(ms)  ║ Req/sec  ║${NC}"
    echo -e "${BOLD}╠═══════╬══════════╬═══════════╬═══════════╬═══════════╬═══════════╬══════════╣${NC}"
    if $H1_OK; then
        printf "${CYAN}║ h1.1  ${NC}║ %8s ║ %9s ║ %9s ║ %9s ║ %9s ║ %8s ║\n" "$N" "$h1_total" "$h1_avg" "$h1_min" "$h1_max" "$h1_rps"
    else
        echo -e "${YELLOW}║ h1.1  ║    —     ║     —     ║     —     ║     —     ║     —     ║    —     ║${NC}"
    fi
    if $H2_OK; then
        printf "${CYAN}║ h2    ${NC}║ %8s ║ %9s ║ %9s ║ %9s ║ %9s ║ %8s ║\n" "$N" "$h2_total" "$h2_avg" "$h2_min" "$h2_max" "$h2_rps"
    else
        echo -e "${YELLOW}║ h2    ║    —     ║     —     ║     —     ║     —     ║     —     ║    —     ║${NC}"
    fi
    if $H3_OK; then
        printf "${CYAN}║ h3    ${NC}║ %8s ║ %9s ║ %9s ║ %9s ║ %9s ║ %8s ║\n" "$N" "$h3_total" "$h3_avg" "$h3_min" "$h3_max" "$h3_rps"
    else
        echo -e "${YELLOW}║ h3    ║    —     ║     —     ║     —     ║     —     ║     —     ║    —     ║${NC}"
    fi
    echo -e "${BOLD}╚═══════╩══════════╩═══════════╩═══════════╩═══════════╩═══════════╩══════════╝${NC}"
    echo ""

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
