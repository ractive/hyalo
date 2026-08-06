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
correctness bug. Both parts are now fully self-contained: the upstream
dependency fix already shipped in mdbook-lint-core 0.15.2, so Part B is a
version bump rather than a campaign with external lead time.

**Do NOT release; release is a separate user-gated step.**

## Context

Verified at `c42fa6f`:

- `hyalo find --dir <fresh-vault> --count` bumps the vault directory's
  mtime. `CaseInsensitiveMode::Auto` is the default; `mode_enabled` probes by
  creating and deleting a file in the vault root, uncached, at seven call
  sites.
- `mdbook` v0.4.52 is a hard, unused dependency of `mdbook-lint-core` 0.14.
  **Upstream already fixed this** — issue #457 closed as completed, and PR
  #472 ("refactor(core): remove unused mdbook dependency") merged 2026-08-04
  as commit `74827a7`. Shipped in **mdbook-lint-core 0.15.2**, published
  2026-08-04. hyalo pins `"0.14"`, so it does not pick it up.
- Measured on a scratch bump to `"0.15"` (reverted): dependency tree
  **168 → 135 crates** (-33 unique; the raw mdbook subtree is 82 but most of
  it is shared with hyalo's own deps), `mdbook` gone entirely,
  `cargo check -p hyalo-mdlint` clean with **zero source changes**, and
  `cargo test --workspace` fully green (3,288 tests, 0 failures).
- `toml` 0.5 survives the bump — it is a direct dep of `mdbook-lint-core`
  0.15.2, tracked upstream as open issue #459. The `toml` duplicate in
  hyalo's tree therefore does **not** clear here.

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

### Part B — dependency diet (upstream already landed; this is a bump)

- [ ] Bump `mdbook-lint-core` and `mdbook-lint-rulesets` from `"0.14"` to
      `"0.15"` in `crates/hyalo-mdlint/Cargo.toml`. Verified to need no
      source changes, but read the 0.15.0/0.15.1/0.15.2 release notes for
      rule-behaviour changes before assuming the green test run generalises
      (0.15.0 changed YAML-frontmatter handling in MD041/MD022/MD007 and
      snake_case config-key parsing).
- [ ] Remove the scoped MPL-2.0 exception at `deny.toml:33-38` — it exists
      only for `mdbook` — and re-run `cargo deny check`.
- [ ] Re-check the two RUSTSEC ignores (`bincode` 1.x RUSTSEC-2025-0141,
      `yaml-rust` RUSTSEC-2024-0320). Both arrive via `comrak -> syntect`,
      which the bump does **not** change, so expect them to stay. Confirm
      rather than assume, and leave them in place if still reachable.
- [ ] Record the before/after crate count and clean-build time in this file
      so the win is measured, not asserted.
- [ ] Comment on upstream issue **#456** ("Make autofix coordinates
      unambiguous and safe for library embedders", open, priority: high)
      with hyalo's concrete `convert_fix` workarounds — MD011's inclusive
      end column, MD034 swallowing Liquid `{%`/`{{`, MD009/HYALO001 needing
      byte columns while other rules use char columns, MD047's no-op range.
      hyalo is the best-documented embedder evidence available and those
      workarounds (`engine.rs:706-889`) are pure upstream-bug tax.

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
- [ ] `cargo tree -p hyalo-cli --edges normal --prefix none | sort -u | wc -l`
      recorded before and after (expect 168 -> 135); MPL-2.0 exception
      removed from `deny.toml` and `cargo deny check` clean
- [ ] upstream #456 comment link recorded in this file
- [ ] `grep -c "unwrap()" crates/hyalo-cli/src/commands/init.rs` returns 0
      outside `#[cfg(test)]`
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `cargo deny check` all clean

## Non-goals

- Do not vendor or fork `mdbook-lint-core`. The dependency win is a version
  bump; there is nothing left to force.
- Do not attempt to fix upstream #456 (autofix coordinates) here. Comment
  with evidence and stop — removing hyalo's `convert_fix` workarounds is a
  separate iteration gated on upstream shipping the fix.
