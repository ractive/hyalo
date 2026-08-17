---
title: Iteration 193 — vault side effects and dependency diet
type: iteration
date: 2026-08-06
status: completed
branch: iter-193/vault-side-effects-dep-diet
tags:
  - iteration
  - performance
  - dependencies
  - supply-chain
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
  **Upstream already fixed this.** Issue #457 closed as completed; PR #472
  ("refactor(core): remove unused mdbook dependency") merged 2026-08-04 as
  commit `74827a7`. Shipped in **mdbook-lint-core 0.15.2**, published
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

### Part A — stop writing to the vault on read-only commands [4/4]

- [x] Cache the resolved case-sensitivity per run — `OnceLock` keyed by
      canonical vault dir, or thread it through the existing command context
      so the seven `mode_enabled` call sites resolve it once.
- [x] Replace the write-based probe with a stat-only probe: stat the vault
      directory itself under a case-flipped final component, or stat an
      already-discovered file under a flipped name. Falls back to the current
      probe only if no candidate exists (genuinely empty vault).
- [x] If the write-based probe survives as a fallback, add
      `.hyalo-case-probe-*` cleanup to the stale-file sweep that
      `find_stale_indexes` already performs — today an orphaned probe has no
      cleanup path and is invisible to `hyalo find` because it is dot-prefixed.
- [x] Document the read-only-mount behaviour: when the probe cannot run,
      case-insensitive link resolution silently turns **off**. That is a
      semantic change, not a perf detail, and it is currently undocumented.

### Part B — dependency diet (upstream already landed; this is a bump) [5/5]

- [x] Bump `mdbook-lint-core` and `mdbook-lint-rulesets` from `"0.14"` to
      `"0.15"` in `crates/hyalo-mdlint/Cargo.toml`. Release notes for
      0.15.0/0.15.1/0.15.2 were reviewed at plan time — **no API changes**,
      only rule-behaviour fixes, and the risk is low. Do not re-derive this;
      the audit is below.

  **0.15.x change audit (done 2026-08-06 — do not repeat):**

  - No-ops for hyalo, because it splits frontmatter itself and passes
    body-only to the engine: "Standard rules ignore YAML frontmatter
    (MD041, MD022, MD007)" and "md004: account for frontmatter line offset".
  - No-ops for hyalo, because it registers only `StandardRuleProvider`:
    every `MDBOOK*`, `CONTENT*`, and `ADR*` fix in all three releases.
  - Real but strictly noise-reducing: "md032: use node end lines so code
    fences in list items don't false-positive" and "md030: do not treat a
    long emphasis span as a list marker". Both can only *lower* violation
    counts, so neither can newly fail the KB lint gate.
  - Additive: `MD013 ignore_reference_definitions` option.
  - Changes fix output: 0.15.1 "preserve hashes in ATX heading content"
    (upstream #453) — MD018/MD019 fixes now keep a closing `##`.
  - **Not in the release notes, found by diffing `md018.rs`:** the violation
    column changed from `line.len()` (bytes) to `line.chars().count()`
    (chars). This silently fixes a latent hyalo bug — `rule_uses_byte_columns`
    (`engine.rs:702`) already classifies MD018 as char-columns, so on a
    multibyte line hyalo's char walk was being handed a byte column by 0.14.
    Worth citing verbatim on upstream #456; it is exactly the coordinate
    ambiguity that issue is about.
- [x] Remove the scoped MPL-2.0 exception at `deny.toml:33-38` — it exists
      only for `mdbook` — and re-run `cargo deny check`.
- [x] Re-check the two RUSTSEC ignores (`bincode` 1.x RUSTSEC-2025-0141,
      `yaml-rust` RUSTSEC-2024-0320). Both arrive via `comrak -> syntect`,
      which the bump does **not** change, so expect them to stay. Confirm
      rather than assume, and leave them in place if still reachable.
- [x] Record the before/after crate count and clean-build time in this file
      so the win is measured, not asserted.
- [x] Comment on upstream issue **#456** ("Make autofix coordinates
      unambiguous and safe for library embedders", open, priority: high)
      with hyalo's concrete `convert_fix` workarounds — MD011's inclusive
      end column, MD034 swallowing Liquid `{%`/`{{`, MD009/HYALO001 needing
      byte columns while other rules use char columns, MD047's no-op range.
      hyalo is the best-documented embedder evidence available and those
      workarounds (`engine.rs:706-889`) are pure upstream-bug tax.
      **[posted 2026-08-17 by the launching session on the user's explicit
      instruction (the unattended run was classifier-blocked), amended for
      upstream PR #486 which had already fixed items 2/3/5 on `main`:
      <https://github.com/joshrotenberg/mdbook-lint/issues/456#issuecomment-5319878913>]**

### Part C — small consistency debts [3/3]

- [x] Replace the ~40 `writeln!(summary, ...).unwrap()` calls in
      `commands/init.rs` with `let _ = writeln!(...)`. `fmt::Write` into a
      `String` is infallible, but these are the bulk of the project's
      no-unwrap-outside-tests violations and they make the rule
      un-greppable.
- [x] File an upstream issue: **MD018 fires on paragraph continuation
      lines.** A wrapped paragraph whose continuation line begins with `#`
      (e.g. a bare `#472` issue reference) is flagged "No space after hash on
      atx style heading" — it is paragraph text, not a heading, and `#472` is
      not a valid ATX heading under CommonMark anyway (no space after `#`).
      Verified still present in 0.15.2 by diffing `md018.rs`: only the fix
      generation and the column unit changed, the detection logic did not.
      Precedent for the shape: upstream #274 ("MD018: false positive on Rust
      attributes inside code blocks") was accepted and fixed. Reproduction —
      `#foo` alone between blank lines correctly fires; `#472` as a
      continuation line falsely fires; inline `PR #472` mid-line correctly
      does not. Upstream-only: hyalo can just disable MD018, which would lose
      the genuine `#Heading` typo detection.
      **[filed 2026-08-17 by the launching session on the user's explicit
      instruction, repro re-verified against 0.15.2 immediately before
      posting:
      <https://github.com/joshrotenberg/mdbook-lint/issues/491>]**
- [x] Consider a distinct `out_of_vault` bucket for link targets that resolve
      outside the scanned directory (dogfood UX-1: GitHub Docs reports 6,568
      broken of 14,167, mostly `/src/...` and `../contributing/...` paths that
      are legitimately out of scope). Same treatment iter-184 gave broken
      anchors — keep them out of the headline `broken` count.

## Acceptance criteria [7/7]

- [x] e2e: vault directory mtime is unchanged after `hyalo find`,
      `hyalo summary`, and `hyalo tags summary` — test name
      `read_only_commands_do_not_touch_vault_dir`
- [x] e2e: `hyalo find` succeeds on a read-only vault directory and the
      resulting case-sensitivity mode is documented in the test's assertion
- [x] `mode_enabled` resolves at most once per invocation — asserted via a
      counter in a unit test or by construction (single call site after the
      refactor)
- [x] `cargo tree -p hyalo-cli --edges normal --prefix none | sort -u | wc -l`
      recorded before and after (expect 168 -> 135); MPL-2.0 exception
      removed from `deny.toml` and `cargo deny check` clean
- [x] upstream #456 comment link recorded in this file
      **[posted 2026-08-17:
      <https://github.com/joshrotenberg/mdbook-lint/issues/456#issuecomment-5319878913>]**
- [x] `grep -c "unwrap()" crates/hyalo-cli/src/commands/init.rs` returns 0
      outside `#[cfg(test)]`
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `cargo deny check` all clean

## Non-goals

- Do not vendor or fork `mdbook-lint-core`. The dependency win is a version
  bump; there is nothing left to force.
- Do not attempt to fix upstream #456 (autofix coordinates) here. Comment
  with evidence and stop — removing hyalo's `convert_fix` workarounds is a
  separate iteration gated on upstream shipping the fix.

## Results (measured 2026-08-17)

### Part A — vault side effects

`mode_enabled` now routes through `probe_case_insensitive_cached`
(`crates/hyalo-core/src/case_index.rs`), which memoizes the answer in a
process-global map keyed by the canonical vault dir. The probe itself is
stat-only: flip the ASCII case of an existing vault entry's name (skipping any
orphaned probe file), stat it, and compare `dev`/`ino` — falling back to the
vault directory's own name, and only then to the historical write probe. A
read-only vault that contains any file therefore now resolves its case
behaviour correctly instead of silently disabling case-insensitive resolution.

- `probe_count()` asserts the "at most once per run" property in
  `cached_probe_runs_at_most_once_per_dir`.
- `sweep_stale_case_probes` removes orphaned `.hyalo-case-probe-*` files
  older than one minute; wired into `create-index`'s existing stale-file sweep.
- Behaviour documented in `docs/configuration.md` under
  "Case-insensitive link resolution", including the read-only-mount caveat.

### Part B — dependency diet

| Metric | Before (0.14) | After (0.15.2) |
| --- | --- | --- |
| `cargo tree -p hyalo-cli --edges normal` unique crates | 168 | **135** |
| Same command, raw line count (`sort -u \| wc -l`) | 205 | 166 |
| `mdbook` in the tree | v0.4.52 | **gone** |
| Clean release build of `hyalo-cli` (empty target dir) | 121 s | **112 s** |
| `deny.toml` license exceptions | 1 (`mdbook` MPL-2.0) | **0** |

Source changes required by the bump: **zero**. `cargo deny check` reports
`advisories ok, bans ok, licenses ok, sources ok`.

Both RUSTSEC ignores were re-checked and **stay**: `bincode` 1.3.3 and
`yaml-rust` 0.4.5 are still reachable via `comrak -> syntect`, which the bump
does not touch (`cargo tree -i` confirms; `yaml-rust` needs `--target all`).

### Part C — consistency debts

- 31 `writeln!(summary, ...).unwrap()` calls in
  `crates/hyalo-cli/src/commands/init.rs` became `let _ = writeln!(...)`;
  zero `unwrap()` remain outside `#[cfg(test)]` in that file.
- New `out_of_vault` bucket: a link whose target still starts with `..` after
  normalization walks above the vault root, so it is out of scope rather than
  broken. It is reported separately by `hyalo links`
  (`out_of_vault` / `out_of_vault_links`), counted separately by
  `hyalo summary` (`links.out_of_vault`, omitted when zero), flagged per link
  by `hyalo find` (`out_of_vault: true`), and excluded from
  `find --broken-links`. Site-absolute targets (`/src/...`) deliberately stay
  in `broken` — a vault that *is* the site root makes those genuine misses, and
  hiding them would be worse than the noise saved.

### Upstream items — both posted

Both upstream tasks were written up in full but could not be submitted by the
unattended run: writing to a third-party GitHub repository
(`joshrotenberg/mdbook-lint`) is blocked by the permission classifier. The
complete texts are in [[docs/upstream-mdbook-lint-reports]].

- The **#456 comment was posted 2026-08-17** by the launching session on the
  user's explicit instruction (with Claude Code attribution), amended to
  account for upstream PR #486 having fixed items 2/3/5 on `main`:
  <https://github.com/joshrotenberg/mdbook-lint/issues/456#issuecomment-5319878913>
- The **MD018 false-positive issue was filed 2026-08-17** the same way, with
  the reproduction re-verified against 0.15.2 immediately before posting:
  <https://github.com/joshrotenberg/mdbook-lint/issues/491>
