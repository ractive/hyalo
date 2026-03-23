# Benchmarks

On-demand performance benchmarks for hyalo. Two types: Criterion micro-benchmarks
(Rust) and Hyperfine end-to-end CLI benchmarks (shell).

## Quick Start

Step-by-step guide to running benchmarks from scratch.

### 1. Install tools

```bash
cargo install critcmp          # compare Criterion baselines
brew install hyperfine          # macOS — or see https://github.com/sharkdp/hyperfine
```

### 2. Get a test vault

Clone [obsidian-hub](https://github.com/obsidian-community/obsidian-hub) next to
the hyalo repo (or anywhere you like):

```bash
cd ..
git clone https://github.com/obsidian-community/obsidian-hub.git
cd hyalo
```

If you place it somewhere else, tell the benchmarks where it is:

```bash
export HYALO_BENCH_VAULT=/path/to/obsidian-hub
```

### 3. Run micro-benchmarks

These test pure functions (string parsing, link extraction, etc.) and don't need
the vault:

```bash
cargo bench --bench micro
```

Open `target/criterion/report/index.html` in a browser to see the HTML reports.

### 4. Run vault benchmarks

These test file discovery, frontmatter reading, and full-vault scanning:

```bash
cargo bench --bench vault
```

If the vault is not found, these benchmarks print a warning and skip gracefully.

### 5. Run end-to-end CLI benchmarks

Build a release binary first, then run all CLI commands through hyperfine:

```bash
cargo build --release
./bench-e2e.sh
```

Results are saved as markdown tables in `bench-results/`. Each command gets its own
file (e.g., `bench-results/find.md`, `bench-results/find-text.md`).

### 6. Compare two versions (A/B test)

This is the most useful workflow — measure the impact of an optimization.

**Option A: Hyperfine side-by-side** (quick, compares CLI wall-clock time)

```bash
# 1. Save the current binary as the baseline
cargo build --release
cp target/release/hyalo /tmp/hyalo-before

# 2. Make your changes, then rebuild
#    ... edit code ...
cargo build --release

# 3. Race both binaries against each other
./bench-e2e.sh target/release/hyalo /tmp/hyalo-before
```

The script runs every command with both binaries and reports which is faster.

**Option B: Criterion baselines** (precise, with statistical analysis)

```bash
# 1. On main (or before your change): save a named baseline
git checkout main
cargo bench -- --save-baseline before

# 2. On your feature branch: save another baseline
git checkout iter-16/some-optimization
cargo bench -- --save-baseline after

# 3. Compare — shows % change with confidence intervals
critcmp before after
```

**Option C: Git worktrees** (avoids stashing and switching branches)

```bash
# 1. Create a worktree for the baseline
git worktree add /tmp/hyalo-main main
(cd /tmp/hyalo-main && cargo build --release)

# 2. Build the current branch
cargo build --release

# 3. Race them
./bench-e2e.sh target/release/hyalo /tmp/hyalo-main/target/release/hyalo

# 4. Clean up
git worktree remove /tmp/hyalo-main
```

### 7. Profile with samply (flamegraphs)

[samply](https://github.com/mstange/samply) records a CPU profile and opens it in the
Firefox Profiler UI — interactive flamegraphs, call trees, and timeline views.

```bash
cargo install samply

# Record a profile (writes a JSON file, then serves the UI)
cargo build --release
samply record target/release/hyalo --dir ../obsidian-hub find --format text
```

This opens the Firefox Profiler in your browser automatically. Use the tabs:
- **Flammendiagramm / Flame Graph** — visual breakdown of where time is spent
- **Aufrufbaum / Call Tree** — hierarchical view with exact sample counts
- **Stack-Diagramm / Stack Chart** — timeline of stack frames over time

To save a profile for later viewing:

```bash
# Record without opening the browser
samply record --save-only -o /tmp/profile.json target/release/hyalo --dir ../obsidian-hub find --format text

# Load it later
samply load --port 9876 /tmp/profile.json
# Then open http://localhost:9876 and click "Open the profile in the profiler UI"
```

### 8. Measure memory usage

No special tooling needed — use the OS time command:

```bash
# macOS
command time -l target/release/hyalo --dir ../obsidian-hub find 2>&1 | grep 'maximum resident'

# Linux
/usr/bin/time -v target/release/hyalo --dir ../obsidian-hub find 2>&1 | grep 'Maximum resident'
```

## Reference

### What each benchmark file covers

| File | Needs vault? | What it measures |
|------|:---:|---|
| `benches/micro.rs` | No | `strip_inline_code`, `strip_inline_comments`, `detect_task_checkbox`, `extract_links_from_text`, `parse_property_filter` |
| `benches/vault.rs` | Yes | `discover_files`, `read_frontmatter` (sample + all), `scan_file_multi` with multiple visitors |
| `bench-e2e.sh` | Yes | All CLI commands: `find`, `find <pattern>`, `find --property`, `find --task`, `properties`, `tags`, `summary`, `--format json` vs `--format text` |

### bench-e2e.sh usage

```bash
./bench-e2e.sh                                    # benchmark current release binary
./bench-e2e.sh path/to/hyalo                      # benchmark a specific binary
./bench-e2e.sh path/to/new path/to/old            # A/B comparison
```

The script builds a release binary automatically if the given path is not executable.

### Tuning

If vault benchmarks are too slow, adjust in the source:

- `sample_size(N)` — number of iterations (default 100, vault benchmarks use 10)
- `measurement_time(Duration)` — time budget per benchmark (vault benchmarks use 30s)
