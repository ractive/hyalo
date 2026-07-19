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

# Benchmark a *mutating* command (e.g. `mv --apply`). Because apply rewrites the
# tree, each timed run must operate on a fresh throwaway copy of the vault: the
# copy is (re)generated in hyperfine's `--prepare` step so the timed command
# always sees the same pristine input. `$@` is the command run against the
# throwaway copy (which is passed via `--dir`).
run_bench_mv() {
    local name="$1"; shift

    echo "  $name ..."

    local work
    work="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$work'" RETURN

    local prepare_cmd="rm -rf '$work/vault' && cp -R '$VAULT' '$work/vault'"

    local current_cmd=( "$HYALO" --dir "$work/vault" "$@" )
    local current_cmd_str
    printf -v current_cmd_str '%q ' "${current_cmd[@]}"
    current_cmd_str=${current_cmd_str% }

    if [[ -n "$HYALO_B" ]]; then
        local baseline_cmd=( "$HYALO_B" --dir "$work/vault" "$@" )
        local baseline_cmd_str
        printf -v baseline_cmd_str '%q ' "${baseline_cmd[@]}"
        baseline_cmd_str=${baseline_cmd_str% }

        hyperfine \
            --warmup "$WARMUP" \
            --runs "$RUNS" \
            --ignore-failure \
            --prepare "$prepare_cmd" \
            --export-markdown "$OUTDIR/${name}.md" \
            -n "current" "$current_cmd_str" \
            -n "baseline" "$baseline_cmd_str"
    else
        hyperfine \
            --warmup "$WARMUP" \
            --runs "$RUNS" \
            --ignore-failure \
            --prepare "$prepare_cmd" \
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

# Link-resolution benches (iter-189): the three commands whose read/verdict
# resolution was re-routed onto the shared discovery entry points.
run_bench "links-fix-dry-run" links fix
run_bench "find-broken-links" find --broken-links
# Batch mv --apply mutates the tree, so it runs against a regenerated throwaway
# copy (see run_bench_mv). Renaming the whole vault into a subfolder exercises
# the batch link-rewrite path across every file.
run_bench_mv "mv-batch-apply" mv --glob '**/*.md' --to moved/ --apply --on-conflict=skip

echo ""
echo "Results saved to $OUTDIR/"
