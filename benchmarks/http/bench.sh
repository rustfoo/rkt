#!/usr/bin/env bash
# HTTP benchmark harness — rkt vs axum vs actix-web
#
# Sequential mode (default): benchmarks each framework one at a time.
# Gives the fairest absolute req/s numbers since servers don't share CPU.
#
# Parallel mode (--parallel): runs oha against all three servers simultaneously
# per scenario. ~3x faster wall-clock; relative rankings stay valid on
# multi-core machines but absolute numbers will be lower.
#
# Requires: oha (cargo install oha), jq
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/results"
STATIC_DIR="$SCRIPT_DIR/static"
BIN_DIR="$SCRIPT_DIR/target/release"

RKT_PORT=8000
AXUM_PORT=8001
ACTIX_PORT=8002

DURATION=30
CONCURRENCY=100
PARALLEL=false
SCENARIOS="ping,hello,state,query,headers,headers-heavy,file-small,file-large"

usage() {
    echo "Usage: $0 [--parallel] [-d seconds] [-c connections] [-s scenario,...]"
    echo
    echo "  --parallel    Run all three servers under load simultaneously"
    echo "  -d N          Duration per scenario in seconds (default: 30)"
    echo "  -c N          Concurrent connections (default: 100)"
    echo "  -s LIST       Comma-separated scenarios to run (default: all)"
    echo
    echo "Scenarios: ping, hello, state, query, headers, headers-heavy, file-small, file-large"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --parallel) PARALLEL=true; shift ;;
        -d) DURATION="$2"; shift 2 ;;
        -c) CONCURRENCY="$2"; shift 2 ;;
        -s) SCENARIOS="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

# ── Prerequisites ────────────────────────────────────────────────────────────

check_tool() {
    if ! command -v "$1" &>/dev/null; then
        echo "Error: '$1' not found."
        case $1 in
            oha) echo "  Install with: cargo install oha" ;;
            jq)  echo "  Install with: sudo dnf install jq  (or apt/brew)" ;;
        esac
        exit 1
    fi
}

check_tool oha
check_tool jq

# ── Static files ─────────────────────────────────────────────────────────────

mkdir -p "$STATIC_DIR"
if [[ ! -f "$STATIC_DIR/small.txt" ]]; then
    dd if=/dev/urandom bs=1024 count=1 2>/dev/null | base64 | head -c 1024 > "$STATIC_DIR/small.txt"
    echo "Generated static/small.txt (1 KB)"
fi
if [[ ! -f "$STATIC_DIR/large.bin" ]]; then
    dd if=/dev/urandom bs=1024 count=1024 of="$STATIC_DIR/large.bin" 2>/dev/null
    echo "Generated static/large.bin (1 MB)"
fi

# ── Build ─────────────────────────────────────────────────────────────────────

echo "Building all binaries (release)..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" 2>&1 | tail -5
echo

# ── Server management ────────────────────────────────────────────────────────

PIDS=()

start_server() {
    local name=$1 bin=$2 port=$3
    "$BIN_DIR/$bin" "$port" &
    PIDS+=($!)
    local pid=${PIDS[-1]}

    local tries=0
    until curl -sf "http://localhost:$port/ping" >/dev/null 2>&1; do
        tries=$((tries + 1))
        if [[ $tries -ge 50 ]]; then
            echo "Error: $name failed to start on port $port"
            kill "${PIDS[@]}" 2>/dev/null || true
            exit 1
        fi
        sleep 0.1
    done
    echo "  $name ready on :$port (pid $pid)"
}

stop_servers() {
    [[ ${#PIDS[@]} -eq 0 ]] && return
    kill "${PIDS[@]}" 2>/dev/null || true
    PIDS=()
}

trap stop_servers EXIT

echo "Starting servers..."
cd "$SCRIPT_DIR"
start_server "rkt"   "rkt-bench"   "$RKT_PORT"
start_server "axum"  "axum-bench"  "$AXUM_PORT"
start_server "actix" "actix-bench" "$ACTIX_PORT"
echo

# ── Benchmark runner ─────────────────────────────────────────────────────────

mkdir -p "$RESULTS_DIR"

scenario_url() {
    local scenario=$1 port=$2
    case $scenario in
        ping)       echo "http://localhost:$port/ping" ;;
        hello)      echo "http://localhost:$port/hello" ;;
        state)      echo "http://localhost:$port/state/key-42" ;;
        query)      echo "http://localhost:$port/query?msg=hello&n=42" ;;
        headers)    echo "http://localhost:$port/headers" ;;
        # headers-heavy hits /ping (a no-op route that never reads headers) but
        # sends a large, realistic header set. Same route/response as `ping`, so
        # the delta between the two scenarios isolates pure per-request header
        # handling cost — the path lazy-header materialization aims to avoid.
        headers-heavy) echo "http://localhost:$port/ping" ;;
        file-small) echo "http://localhost:$port/files/small.txt" ;;
        file-large) echo "http://localhost:$port/files/large.bin" ;;
        *) echo "Unknown scenario: $scenario" >&2; exit 1 ;;
    esac
}

run_oha() {
    local framework=$1 scenario=$2 port=$3
    local url extra_flags=()
    url=$(scenario_url "$scenario" "$port")

    if [[ $scenario == "headers" ]]; then
        extra_flags+=(-H "X-Bench-Id: bench-run-001")
    fi

    # A realistic browser/proxy header set (~16 headers on top of Host, which
    # oha sends automatically). The route ignores all of these; they exist only
    # to make per-request header handling a measurable share of the work.
    if [[ $scenario == "headers-heavy" ]]; then
        extra_flags+=(
            -H "User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"
            -H "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
            -H "Accept-Language: en-GB,en-US;q=0.9,en;q=0.8"
            -H "Accept-Encoding: gzip, deflate, br"
            -H "Cache-Control: no-cache"
            -H "Pragma: no-cache"
            -H "Referer: https://example.com/some/previous/page?with=query"
            -H "Origin: https://example.com"
            -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJiZW5jaCJ9.signature-placeholder-value"
            -H "Cookie: session=8f3a2b1c9d4e5f60718293a4b5c6d7e8; csrftoken=abc123def456ghi789; theme=dark; locale=en-GB; consent=1"
            -H "X-Forwarded-For: 203.0.113.7, 198.51.100.42"
            -H "X-Forwarded-Proto: https"
            -H "X-Forwarded-Host: example.com"
            -H "X-Request-Id: 9b2c4d6e-8f01-4a23-b567-89abcdef0123"
            -H "DNT: 1"
            -H "Upgrade-Insecure-Requests: 1"
        )
    fi

    oha -z "${DURATION}s" -c "$CONCURRENCY" --no-tui --output-format json \
        "${extra_flags[@]+"${extra_flags[@]}"}" \
        "$url" > "$RESULTS_DIR/${framework}-${scenario}.json" 2>/dev/null
}

run_scenario() {
    local scenario=$1
    echo "  [$scenario]"
    if [[ $PARALLEL == true ]]; then
        run_oha rkt   "$scenario" "$RKT_PORT"   &
        run_oha axum  "$scenario" "$AXUM_PORT"  &
        run_oha actix "$scenario" "$ACTIX_PORT" &
        wait
    else
        run_oha rkt   "$scenario" "$RKT_PORT"
        run_oha axum  "$scenario" "$AXUM_PORT"
        run_oha actix "$scenario" "$ACTIX_PORT"
    fi
}

echo "Running benchmarks (${DURATION}s × ${CONCURRENCY} connections, $([ $PARALLEL == true ] && echo parallel || echo sequential))..."
IFS=',' read -ra SCENARIO_LIST <<< "$SCENARIOS"
for scenario in "${SCENARIO_LIST[@]}"; do
    run_scenario "$scenario"
done
echo

# ── Results table ─────────────────────────────────────────────────────────────

print_results() {
    local frameworks=("rkt" "axum" "actix")

    printf "%-12s" "scenario"
    for fw in "${frameworks[@]}"; do
        printf " %-12s %-10s %-10s" "$fw req/s" "p50(ms)" "p99(ms)"
    done
    printf "\n"

    printf "%-12s" "--------"
    for fw in "${frameworks[@]}"; do
        printf " %-12s %-10s %-10s" "----------" "-------" "-------"
    done
    printf "\n"

    for scenario in "${SCENARIO_LIST[@]}"; do
        printf "%-12s" "$scenario"
        for fw in "${frameworks[@]}"; do
            local result_file="$RESULTS_DIR/${fw}-${scenario}.json"
            if [[ -f $result_file ]]; then
                local rps p50 p99
                rps=$(jq -r '.summary.requestsPerSec | . * 10 | round / 10' "$result_file")
                p50=$(jq -r '(.latencyPercentiles.p50 // 0) * 1000 | . * 100 | round / 100' "$result_file")
                p99=$(jq -r '(.latencyPercentiles.p99 // 0) * 1000 | . * 100 | round / 100' "$result_file")
                printf " %-12s %-10s %-10s" "$rps" "$p50" "$p99"
            else
                printf " %-12s %-10s %-10s" "n/a" "n/a" "n/a"
            fi
        done
        printf "\n"
    done
}

echo "Results (req/s and latency in ms):"
echo
print_results
echo
echo "Raw JSON results saved to: $RESULTS_DIR/"
