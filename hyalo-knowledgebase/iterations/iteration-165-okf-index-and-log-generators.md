---
title: "Iteration 165 — hyalo okf index & log generators"
type: iteration
date: 2026-07-16
status: planned
branch: iter-165/okf-index-and-log-generators
tags: [iteration, okf, generators, index, log]
related: [research/okf-open-knowledge-format.md]
priority: 3
depends-on: iteration-164-okf-init-profile-and-skill
---

# Iteration 165 — `hyalo okf index` & `log` generators

The highest-leverage, ecosystem-unique features: deterministic (re)generation of the *derived* files OKF authors otherwise hand-maintain. Mirrors `reference_agent`'s `bundle/index.py::regenerate_indexes` — but with no LLM and no cloud. See [[okf-open-knowledge-format]].

## Goal

`hyalo okf index` regenerates every `index.md` from frontmatter; `hyalo okf log` prepends a dated entry to `log.md`. Both deterministic, streaming, cross-platform.

## Steps / Tasks

### 1. `hyalo okf index`

- [ ] New `hyalo okf` subcommand group; `index` regenerates `index.md` per directory
- [ ] For each dir: list child concepts + subdirs, group entries by frontmatter `type`, emit `* [title](relative-link) - description` (title falls back to filename; description optional)
- [ ] Preserve/emit the root-`index.md` `okf_version` line; never clobber non-generated prose above/below a generated marker (define a stable managed region)
- [ ] Flags: `--dry-run` (default), `--apply`, `--dir`/path scoping; exit non-zero on drift in dry-run for CI use
- [ ] Emit **relative** links in generated `index.md` entries (matches §6 examples and all official sample bundles). Note: SPEC §5 actually *recommends* bundle-absolute `/x.md` for concept cross-links — both forms must resolve (iter-163); the generator just follows the samples' de-facto style. Cross-platform paths, forward slashes only

### 2. `hyalo okf log`

Per SPEC §7 a `log.md` MAY appear at **any level** of the hierarchy and records the history of **that scope** (directory-local, not bundle-wide) — so the target level must be selectable.

- [ ] `hyalo okf log [TARGET] --message "..."` where `TARGET` selects which `log.md`:
  - a directory → writes/creates `TARGET/log.md`
  - a `log.md` file path → writes that file directly
  - omitted → defaults to the bundle-root `log.md` (`<dir>/log.md`)
- [ ] Validate `TARGET` is inside the vault/bundle; reject paths that escape it; cross-platform path handling
- [ ] Prepends under today's `YYYY-MM-DD` heading (newest first), leading bold action word optional via `--action Update` (convention, not required per §7)
- [ ] Create `log.md` if absent (no frontmatter — reserved file); append under existing date heading if present
- [ ] `--dry-run` (default) / `--apply`

### 3. Frontmatter hygiene helpers (support the producer story)

- [ ] Key-order normalization to `type, resource, title, description, tags, timestamp` (opt-in flag or part of `okf` lint --fix)
- [ ] tz-aware `timestamp` auto-stamp helper on write (reuse `datetime-tz` from iter-163)

### 4. Tests

- [ ] e2e: `okf index --apply` on a copied sample bundle reproduces the committed `index.md` files (modulo optional LLM directory summaries, which hyalo omits)
- [ ] e2e: `okf log` creates/updates `log.md` with correct date grouping and ordering
- [ ] Idempotency: running `okf index --apply` twice is a no-op
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR)

- [ ] `hyalo okf --help`, `hyalo okf index --help`, `hyalo okf log --help`
- [ ] README.md: generators section + CI usage (`okf index --dry-run`)
- [ ] Update the `okf` skill to prescribe `okf index`/`okf log` in the maintenance loop
- [ ] Update [[okf-open-knowledge-format]] gap #4 status

## Acceptance Criteria

- [ ] `hyalo okf index --apply` deterministically regenerates spec-shaped `index.md` files and is idempotent
- [ ] `hyalo okf log` maintains a spec-shaped `log.md`
- [ ] Generated index links are relative (sample-bundle style) and resolve; runs on Windows/macOS/Linux
- [ ] Quality gates pass; docs + skill updated in the same PR
