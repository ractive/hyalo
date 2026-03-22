#!/usr/bin/env bash
set -euo pipefail

VAULT="${HYALO_BENCH_VAULT:-../obsidian-hub}"
HYALO="${1:-target/release/hyalo}"
HYALO_B="${2:-}"  # optional second binary for A/B comparison

if [[ ! -d "$VAULT" ]]; then
    echo "ERROR: Vault not found at $VAULT. Set HYALO_BENCH_VAULT." >&2
    exit 1
fi

if [[ ! -x "$HYALO" ]]; then
    echo "Building release binary..."
    cargo build --release
    HYALO="target/release/hyalo"
fi

WARMUP=3
RUNS=10
OUTDIR="bench-results"
mkdir -p "$OUTDIR"

run_bench() {
    local name="$1"; shift

    echo "  $name ..."

    local current_cmd=( "$HYALO" --dir "$VAULT" "$@" )
    local current_cmd_str
    printf -v current_cmd_str '%q ' "${current_cmd[@]}"
    current_cmd_str=${current_cmd_str% }

    if [[ -n "$HYALO_B" ]]; then
        local baseline_cmd=( "$HYALO_B" --dir "$VAULT" "$@" )
        local baseline_cmd_str
        printf -v baseline_cmd_str '%q ' "${baseline_cmd[@]}"
        baseline_cmd_str=${baseline_cmd_str% }

        hyperfine \
            --warmup "$WARMUP" \
            --runs "$RUNS" \
            --ignore-failure \
            --export-markdown "$OUTDIR/${name}.md" \
            -n "current" "$current_cmd_str" \
            -n "baseline" "$baseline_cmd_str"
    else
        hyperfine \
            --warmup "$WARMUP" \
            --runs "$RUNS" \
            --ignore-failure \
            --export-markdown "$OUTDIR/${name}.md" \
            -n "$name" "$current_cmd_str"
    fi
}

echo "=== E2E Benchmarks against $VAULT ==="
echo ""

run_bench "find"              find
run_bench "find-pattern"      find obsidian
run_bench "find-property"     find --property title
run_bench "find-task"         find --task todo
run_bench "properties"        properties
run_bench "tags"              tags
run_bench "summary"           summary
run_bench "find-json"         find --format json
run_bench "find-text"         find --format text

echo ""
echo "Results saved to $OUTDIR/"
