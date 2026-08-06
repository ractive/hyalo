---
title: Iteration 193 — vault side effects and dependency diet
type: iteration
date: 2026-08-06
status: planned
branch: iter-193/vault-side-effects-dep-diet
tags: [iteration, performance, dependencies, supply-chain]
related:
  - "[[reviews/codebase-review-2026-08-06]]"
  - "[[dogfood-results/dogfood-v0200-opus5-review-round]]"
---

# Iteration 193 — vault side effects and dependency diet

## Goal

Two independent cleanups that share a theme: hyalo doing expensive,
surprising things the user never asked for. Read-only commands write files
into the vault, and half the dependency tree exists to satisfy an upstream
crate's unused import.

Sequenced **after** iter-191 and iter-192 because neither item is a
correctness bug and the upstream half has a lead time outside our control.

**Do NOT release; release is a separate user-gated step.**

## Context

Verified at `c42fa6f`:

- `hyalo find --dir <fresh-vault> --count` bumps the vault directory's
  mtime. `CaseInsensitiveMode::Auto` is the default; `mode_enabled` probes by
  creating and deleting a file in the vault root, uncached, at seven call
  sites.
- `mdbook` v0.4.52 is **82 of 168** transitive crates. `mdbook-lint-core`
  declares it as a hard dependency; grepping that crate's entire source for
  any reference to it returns nothing.

## Tasks

### Part A — stop writing to the vault on read-only commands

- [ ] Cache the resolved case-sensitivity per run — `OnceLock` keyed by
      canonical vault dir, or thread it through the existing command context
      so the seven `mode_enabled` call sites resolve it once.
- [ ] Replace the write-based probe with a stat-only probe: stat the vault
      directory itself under a case-flipped final component, or stat an
      already-discovered file under a flipped name. Falls back to the current
      probe only if no candidate exists (genuinely empty vault).
- [ ] If the write-based probe survives as a fallback, add
      `.hyalo-case-probe-*` cleanup to the stale-file sweep that
      `find_stale_indexes` already performs — today an orphaned probe has no
      cleanup path and is invisible to `hyalo find` because it is dot-prefixed.
- [ ] Document the read-only-mount behaviour: when the probe cannot run,
      case-insensitive link resolution silently turns **off**. That is a
      semantic change, not a perf detail, and it is currently undocumented.

### Part B — dependency diet

- [ ] File an upstream issue on `joshrotenberg/mdbook-lint` showing that
      `mdbook-lint-core` never references the `mdbook` crate, with the
      82-of-168 number.
- [ ] Open the upstream PR making the dependency optional (feature-gated
      behind whatever `MdBookRuleProvider` actually needs) or removing it.
      hyalo uses `StandardRuleProvider`, not `MdBookRuleProvider`.
- [ ] Once upstream lands, bump `mdbook-lint-core` / `mdbook-lint-rulesets`,
      drop the scoped MPL-2.0 exception at `deny.toml:33-38`, and re-run
      `cargo deny check`.
- [ ] Re-check the two RUSTSEC ignores (`bincode` 1.x RUSTSEC-2025-0141,
      `yaml-rust` RUSTSEC-2024-0320). Both arrive via `comrak -> syntect`;
      confirm whether the bump clears them or whether they still need ignoring.
- [ ] Record the resulting crate count and clean-build time in the iteration
      so the win is measured, not asserted.

### Part C — small consistency debts

- [ ] Replace the ~40 `writeln!(summary, ...).unwrap()` calls in
      `commands/init.rs` with `let _ = writeln!(...)`. `fmt::Write` into a
      `String` is infallible, but these are the bulk of the project's
      no-unwrap-outside-tests violations and they make the rule
      un-greppable.
- [ ] Consider a distinct `out_of_vault` bucket for link targets that resolve
      outside the scanned directory (dogfood UX-1: GitHub Docs reports 6,568
      broken of 14,167, mostly `/src/...` and `../contributing/...` paths that
      are legitimately out of scope). Same treatment iter-184 gave broken
      anchors — keep them out of the headline `broken` count.

## Acceptance criteria

- [ ] e2e: vault directory mtime is unchanged after `hyalo find`,
      `hyalo summary`, and `hyalo tags summary` — test name
      `read_only_commands_do_not_touch_vault_dir`
- [ ] e2e: `hyalo find` succeeds on a read-only vault directory and the
      resulting case-sensitivity mode is documented in the test's assertion
- [ ] `mode_enabled` resolves at most once per invocation — asserted via a
      counter in a unit test or by construction (single call site after the
      refactor)
- [ ] upstream issue and PR links recorded in this file
- [ ] `cargo tree -p hyalo-cli --edges normal --prefix none | sort -u | wc -l`
      recorded before and after; MPL-2.0 exception removed from `deny.toml`
      if upstream landed
- [ ] `grep -c "unwrap()" crates/hyalo-cli/src/commands/init.rs` returns 0
      outside `#[cfg(test)]`
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `cargo deny check` all clean

## Non-goals

- Part B's upstream work must not become a vendored fork. If upstream is
  unresponsive, record that and close the iteration with Part A and Part C
  landed — do not vendor `mdbook-lint-core` to force the win.
