#!/usr/bin/env bash
# Reproducible HTTP/1.1 loopback benchmark harness for rkt, Axum, and Actix.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_ROOT="$SCRIPT_DIR/results"
STATIC_DIR="$SCRIPT_DIR/static"
BIN_DIR="$SCRIPT_DIR/target/release"

RKT_PORT=8000
AXUM_PORT=8001
ACTIX_PORT=8002
EXPECTED_OHA_VERSION="1.14.0"

DURATION=10
WARMUP=2
REPETITIONS=5
CONCURRENCIES="1,32,100"
SCENARIOS="ping,hello,state,query-borrowed,query-owned,headers-wire,cookies-wire,headers-sparse,headers-full,memory-1k,memory-64k,memory-1m,file-1k,file-64k,file-1m,stream-slow"
OUTPUT_DIR=""
BUILD=true
ALLOW_OHA_VERSION=false

usage() {
    cat <<EOF
Usage: $0 [options]

  -d, --duration N       Measured seconds per run (default: $DURATION)
  -w, --warmup N         Warm-up seconds before each run (default: $WARMUP)
  -r, --repetitions N    Measured repetitions (default: $REPETITIONS)
  -c, --concurrency LIST Comma-separated concurrency sweep (default: $CONCURRENCIES)
  -s, --scenarios LIST   Comma-separated scenario list
  -o, --output-dir PATH  New directory for this run (must not exist)
      --no-build         Use existing release binaries
      --allow-oha-version
                         Record, but do not require, oha $EXPECTED_OHA_VERSION
  -h, --help             Show this help

Scenarios:
  ping, hello, state, query-borrowed, query-owned,
  headers-wire, cookies-wire, headers-sparse, headers-full,
  memory-1k, memory-64k, memory-1m, file-1k, file-64k, file-1m,
  stream-slow

The defaults are the diagnostic run. For a quick smoke test, use:
  $0 -d 1 -w 0 -r 1 -c 1 -s ping,headers-wire,cookies-wire
EOF
}

need_value() {
    if [[ $# -lt 2 ]]; then
        echo "Error: $1 requires a value" >&2
        exit 2
    fi
}

while [[ $# -gt 0 ]]; do
    case $1 in
        -d|--duration) need_value "$@"; DURATION=$2; shift 2 ;;
        -w|--warmup) need_value "$@"; WARMUP=$2; shift 2 ;;
        -r|--repetitions) need_value "$@"; REPETITIONS=$2; shift 2 ;;
        -c|--concurrency) need_value "$@"; CONCURRENCIES=$2; shift 2 ;;
        -s|--scenarios) need_value "$@"; SCENARIOS=$2; shift 2 ;;
        -o|--output-dir) need_value "$@"; OUTPUT_DIR=$2; shift 2 ;;
        --no-build) BUILD=false; shift ;;
        --allow-oha-version) ALLOW_OHA_VERSION=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Error: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

for value in "$DURATION" "$WARMUP" "$REPETITIONS"; do
    if [[ ! $value =~ ^[0-9]+$ ]]; then
        echo "Error: durations and repetitions must be non-negative integers" >&2
        exit 2
    fi
done
if (( DURATION == 0 || REPETITIONS == 0 )); then
    echo "Error: duration and repetitions must be greater than zero" >&2
    exit 2
fi

IFS=',' read -r -a CONCURRENCY_LIST <<< "$CONCURRENCIES"
IFS=',' read -r -a SCENARIO_LIST <<< "$SCENARIOS"
for concurrency in "${CONCURRENCY_LIST[@]}"; do
    if [[ ! $concurrency =~ ^[1-9][0-9]*$ ]]; then
        echo "Error: invalid concurrency: $concurrency" >&2
        exit 2
    fi
done

check_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Error: '$1' is required" >&2
        exit 1
    fi
}

for tool in cargo curl git jq oha python3 rustc; do
    check_tool "$tool"
done

OHA_VERSION="$(oha --version | awk '{print $2}')"
if [[ $OHA_VERSION != "$EXPECTED_OHA_VERSION" && $ALLOW_OHA_VERSION == false ]]; then
    echo "Error: expected oha $EXPECTED_OHA_VERSION, found $OHA_VERSION" >&2
    echo "Use --allow-oha-version for an explicitly non-comparable run." >&2
    exit 1
fi

scenario_url() {
    local scenario=$1 port=$2
    case $scenario in
        ping) echo "http://127.0.0.1:$port/ping" ;;
        hello) echo "http://127.0.0.1:$port/hello" ;;
        state) echo "http://127.0.0.1:$port/state/key-42" ;;
        query-borrowed) echo "http://127.0.0.1:$port/query?msg=hello&n=42" ;;
        query-owned) echo "http://127.0.0.1:$port/query-owned?msg=hello&n=42" ;;
        headers-wire|cookies-wire) echo "http://127.0.0.1:$port/ping" ;;
        headers-sparse) echo "http://127.0.0.1:$port/headers" ;;
        headers-full) echo "http://127.0.0.1:$port/headers-full" ;;
        memory-1k) echo "http://127.0.0.1:$port/memory/1k" ;;
        memory-64k) echo "http://127.0.0.1:$port/memory/64k" ;;
        memory-1m) echo "http://127.0.0.1:$port/memory/1m" ;;
        file-1k) echo "http://127.0.0.1:$port/files/1k.bin" ;;
        file-64k) echo "http://127.0.0.1:$port/files/64k.bin" ;;
        file-1m) echo "http://127.0.0.1:$port/files/1m.bin" ;;
        stream-slow) echo "http://127.0.0.1:$port/stream-slow" ;;
        *) echo "Error: unknown scenario: $scenario" >&2; return 2 ;;
    esac
}

for scenario in "${SCENARIO_LIST[@]}"; do
    scenario_url "$scenario" 0 >/dev/null
done

mkdir -p "$STATIC_DIR" "$RESULTS_ROOT"
make_static_file() {
    local name=$1 kib=$2
    if [[ ! -f "$STATIC_DIR/$name" ]] || [[ $(wc -c < "$STATIC_DIR/$name") -ne $((kib * 1024)) ]]; then
        dd if=/dev/zero of="$STATIC_DIR/$name" bs=1024 count="$kib" status=none
    fi
}
make_static_file 1k.bin 1
make_static_file 64k.bin 64
make_static_file 1m.bin 1024

if [[ $BUILD == true ]]; then
    echo "Building locked release binaries..."
    cargo build --locked --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
fi
for bin in rkt-bench axum-bench actix-bench; do
    if [[ ! -x "$BIN_DIR/$bin" ]]; then
        echo "Error: missing $BIN_DIR/$bin; rerun without --no-build" >&2
        exit 1
    fi
done

COMMIT="$(git -C "$SCRIPT_DIR" rev-parse HEAD)"
SHORT_COMMIT="$(git -C "$SCRIPT_DIR" rev-parse --short HEAD)"
if [[ -z $OUTPUT_DIR ]]; then
    OUTPUT_DIR="$RESULTS_ROOT/$(date -u +%Y%m%dT%H%M%SZ)-$SHORT_COMMIT"
elif [[ $OUTPUT_DIR != /* ]]; then
    OUTPUT_DIR="$PWD/$OUTPUT_DIR"
fi
if [[ -e $OUTPUT_DIR ]]; then
    echo "Error: output directory already exists: $OUTPUT_DIR" >&2
    exit 1
fi
mkdir -p "$OUTPUT_DIR/raw" "$OUTPUT_DIR/logs"

cargo metadata --locked --manifest-path "$SCRIPT_DIR/Cargo.toml" --format-version 1 \
    > "$OUTPUT_DIR/cargo-metadata.json"

DIRTY=false
if [[ -n $(git -C "$SCRIPT_DIR" status --porcelain) ]]; then
    DIRTY=true
fi
CPU_MODEL="$(awk -F ': ' '/model name/ {print $2; exit}' /proc/cpuinfo 2>/dev/null || true)"
KERNEL="$(uname -srmo)"
AFFINITY="$(taskset -pc $$ 2>/dev/null | sed 's/.*: //' || true)"
GOVERNOR=""
for governor_path in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
    if [[ -r $governor_path ]]; then
        GOVERNOR="$(< "$governor_path")"
        break
    fi
done

jq -n \
    --arg schemaVersion "1" \
    --arg startedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg commit "$COMMIT" \
    --argjson dirty "$DIRTY" \
    --arg rustc "$(rustc --version --verbose)" \
    --arg cargo "$(cargo --version)" \
    --arg oha "$OHA_VERSION" \
    --arg expectedOha "$EXPECTED_OHA_VERSION" \
    --arg kernel "$KERNEL" \
    --arg cpuModel "$CPU_MODEL" \
    --arg affinity "$AFFINITY" \
    --arg governor "$GOVERNOR" \
    --argjson duration "$DURATION" \
    --argjson warmup "$WARMUP" \
    --argjson repetitions "$REPETITIONS" \
    --arg concurrencies "$CONCURRENCIES" \
    --arg scenarios "$SCENARIOS" \
    '{schemaVersion: ($schemaVersion | tonumber), startedAt: $startedAt,
      source: {commit: $commit, dirty: $dirty},
      tools: {rustc: $rustc, cargo: $cargo, oha: $oha, expectedOha: $expectedOha},
      host: {kernel: $kernel, cpuModel: $cpuModel, affinity: $affinity, governor: $governor},
      protocol: "HTTP/1.1", mode: "sequential-isolated",
      config: {durationSeconds: $duration, warmupSeconds: $warmup,
               repetitions: $repetitions,
               concurrencies: ($concurrencies | split(",") | map(tonumber)),
               scenarios: ($scenarios | split(","))}}' \
    > "$OUTPUT_DIR/metadata.json"

CURRENT_PID=""
stop_server() {
    if [[ -n $CURRENT_PID ]]; then
        kill "$CURRENT_PID" 2>/dev/null || true
        wait "$CURRENT_PID" 2>/dev/null || true
        CURRENT_PID=""
    fi
}
trap stop_server EXIT INT TERM

framework_port() {
    case $1 in
        rkt) echo "$RKT_PORT" ;;
        axum) echo "$AXUM_PORT" ;;
        actix) echo "$ACTIX_PORT" ;;
    esac
}

framework_bin() {
    case $1 in
        rkt) echo rkt-bench ;;
        axum) echo axum-bench ;;
        actix) echo actix-bench ;;
    esac
}

start_server() {
    local framework=$1 log_file=$2 port bin tries=0
    port="$(framework_port "$framework")"
    bin="$(framework_bin "$framework")"
    (
        cd "$SCRIPT_DIR"
        exec "$BIN_DIR/$bin" "$port"
    ) > "$log_file" 2>&1 &
    CURRENT_PID=$!
    until curl --http1.1 -fsS "http://127.0.0.1:$port/ping" >/dev/null; do
        if ! kill -0 "$CURRENT_PID" 2>/dev/null || (( ++tries >= 100 )); then
            echo "Error: $framework failed to start; see $log_file" >&2
            return 1
        fi
        sleep 0.05
    done
}

SCENARIO_FLAGS=()
set_scenario_flags() {
    local scenario=$1
    SCENARIO_FLAGS=()
    case $scenario in
        headers-wire|headers-full)
            SCENARIO_FLAGS+=(
                -H "User-Agent: Mozilla/5.0 benchmark"
                -H "Accept: text/html,application/json;q=0.9,*/*;q=0.8"
                -H "Accept-Language: en-GB,en;q=0.8"
                -H "Accept-Encoding: identity"
                -H "Cache-Control: no-cache"
                -H "Pragma: no-cache"
                -H "Referer: https://example.test/previous?with=query"
                -H "Origin: https://example.test"
                -H "Authorization: Bearer benchmark-token"
                -H "X-Forwarded-For: 203.0.113.7, 198.51.100.42"
                -H "X-Forwarded-Proto: https"
                -H "X-Forwarded-Host: example.test"
                -H "X-Request-Id: 9b2c4d6e-8f01-4a23-b567-89abcdef0123"
                -H "DNT: 1"
                -H "Upgrade-Insecure-Requests: 1"
            )
            ;;
        cookies-wire)
            SCENARIO_FLAGS+=(
                -H "Cookie: session=8f3a2b1c9d4e5f60; csrftoken=abc123def456; theme=dark; locale=en-GB; consent=1"
            )
            ;;
        headers-sparse)
            SCENARIO_FLAGS+=(
                -H "X-Bench-Id: bench-run-001"
                -H "Accept: application/json"
            )
            ;;
    esac
    if [[ $scenario == headers-full ]]; then
        SCENARIO_FLAGS+=(
            -H "Cookie: session=8f3a2b1c9d4e5f60; csrftoken=abc123def456; theme=dark; locale=en-GB; consent=1"
        )
    fi
}

proc_ticks() {
    awk '{print $14 + $15}' "/proc/$1/stat"
}

proc_peak_rss_kib() {
    awk '/VmHWM:/ {print $2}' "/proc/$1/status"
}

run_oha() {
    local framework=$1 scenario=$2 concurrency=$3 repetition=$4 order=$5 sequence=$6
    local port url result_dir result_file temp_file log_file
    local ticks_before ticks_after ticks_delta peak_rss elapsed cpu_percent clock_ticks

    port="$(framework_port "$framework")"
    url="$(scenario_url "$scenario" "$port")"
    result_dir="$OUTPUT_DIR/raw/$scenario/c$concurrency"
    result_file="$result_dir/run-$repetition-$framework.json"
    temp_file="$result_file.tmp"
    log_file="$OUTPUT_DIR/logs/$sequence-$framework-$scenario-c$concurrency-r$repetition.log"
    mkdir -p "$result_dir"

    start_server "$framework" "$log_file"
    set_scenario_flags "$scenario"

    if (( WARMUP > 0 )); then
        env -u NO_COLOR oha -z "${WARMUP}s" -c "$concurrency" --http-version 1.1 \
            --wait-ongoing-requests-after-deadline --no-tui --no-color \
            --output-format quiet "${SCENARIO_FLAGS[@]}" "$url" >/dev/null 2>&1
    fi

    ticks_before="$(proc_ticks "$CURRENT_PID")"
    env -u NO_COLOR oha -z "${DURATION}s" -c "$concurrency" --http-version 1.1 \
        --wait-ongoing-requests-after-deadline --no-tui --no-color \
        --output-format json "${SCENARIO_FLAGS[@]}" "$url" > "$temp_file" 2>> "$log_file"
    ticks_after="$(proc_ticks "$CURRENT_PID")"
    peak_rss="$(proc_peak_rss_kib "$CURRENT_PID")"

    curl --http1.1 -fsS "http://127.0.0.1:$port/ping" >/dev/null
    if ! jq -e '
        .summary.successRate == 1 and
        ((.statusCodeDistribution | keys) == ["200"]) and
        ((.errorDistribution // {}) | length == 0)
    ' "$temp_file" >/dev/null; then
        echo "Error: unsuccessful responses in $temp_file" >&2
        return 1
    fi

    ticks_delta=$((ticks_after - ticks_before))
    clock_ticks="$(getconf CLK_TCK)"
    elapsed="$(jq -r '.summary.total' "$temp_file")"
    cpu_percent="$(awk -v ticks="$ticks_delta" -v hz="$clock_ticks" -v seconds="$elapsed" \
        'BEGIN { if (seconds > 0) printf "%.3f", 100 * ticks / hz / seconds; else print 0 }')"

    jq \
        --arg framework "$framework" \
        --arg scenario "$scenario" \
        --arg protocol "HTTP/1.1" \
        --argjson concurrency "$concurrency" \
        --argjson repetition "$repetition" \
        --argjson frameworkOrder "$order" \
        --argjson sequence "$sequence" \
        --argjson serverCpuPercent "$cpu_percent" \
        --argjson serverPeakRssKiB "${peak_rss:-0}" \
        '. + {benchmark: {
            framework: $framework, scenario: $scenario, protocol: $protocol,
            concurrency: $concurrency, repetition: $repetition,
            frameworkOrder: $frameworkOrder, sequence: $sequence,
            serverCpuPercent: $serverCpuPercent,
            serverPeakRssKiB: $serverPeakRssKiB
        }}' "$temp_file" > "$result_file"
    rm "$temp_file"
    stop_server
}

FRAMEWORKS=(rkt axum actix)
SEQUENCE=0
TOTAL=$((${#CONCURRENCY_LIST[@]} * ${#SCENARIO_LIST[@]} * REPETITIONS * ${#FRAMEWORKS[@]}))
echo "Writing run to $OUTPUT_DIR"
echo "Running $TOTAL isolated measurements after per-run warm-up..."

for concurrency_index in "${!CONCURRENCY_LIST[@]}"; do
    concurrency=${CONCURRENCY_LIST[$concurrency_index]}
    for scenario_index in "${!SCENARIO_LIST[@]}"; do
        scenario=${SCENARIO_LIST[$scenario_index]}
        for ((repetition = 1; repetition <= REPETITIONS; repetition++)); do
            rotation=$(((concurrency_index + scenario_index + repetition - 1) % ${#FRAMEWORKS[@]}))
            for framework_order in "${!FRAMEWORKS[@]}"; do
                framework=${FRAMEWORKS[$(((rotation + framework_order) % ${#FRAMEWORKS[@]}))]}
                SEQUENCE=$((SEQUENCE + 1))
                printf '[%d/%d] c=%s scenario=%s repetition=%s framework=%s\n' \
                    "$SEQUENCE" "$TOTAL" "$concurrency" "$scenario" "$repetition" "$framework"
                run_oha "$framework" "$scenario" "$concurrency" "$repetition" \
                    "$((framework_order + 1))" "$SEQUENCE"
            done
        done
    done
done

python3 "$SCRIPT_DIR/report.py" "$OUTPUT_DIR"
echo "Raw results, metadata, and aggregate report: $OUTPUT_DIR"
