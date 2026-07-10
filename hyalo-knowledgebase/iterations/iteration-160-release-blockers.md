---
type: iteration
title: "Iteration 160 — Release blockers: lint-rules panic + links fix frontmatter no-op"
date: 2026-07-10
status: completed
branch: iter-160/release-blockers
tags:
  - iteration
  - correctness
  - links
  - lint
---

# Iteration 160 — Release blockers

Fix the two v0.16.0 release blockers from
[[reviews/codebase-review-2026-07-10]] and
[[dogfood-results/dogfood-v0160-iter157-159-pr186]].

## Blocker A (CRITICAL) — `lint-rules set` panics on malformed `.hyalo.toml`

`crates/hyalo-cli/src/commands/lint_rules.rs:222-226` and `:263-267`: the
guard `if doc.get("lint").is_none()` only covers "key absent". With a scalar
(`lint = "oops"`), `doc["lint"]["rules"]` blind-indexes via `toml_edit`'s
`IndexMut` and panics ("index not found", exit 134). `types.rs` handles the
identical scenario via `.as_table_mut().context(...)?` — port that pattern:
promote/replace `lint` when present-but-not-a-table at both call sites.

## Blocker B (HIGH) — `links fix --apply` reports frontmatter fixes as applied but never writes them

`build_replacements_for_file` (`crates/hyalo-core/src/link_fix.rs:919`) walks
the body only, so frontmatter FixPlans yield no Replacement, while the CLI
reports the pre-write FixPlan list as `Applied: yes`. 13/15 "applied" fixes
on the own KB were silent no-ops; agent fix-loops never converge. `mv`
already rewrites frontmatter wikilinks (`plan_frontmatter_wikilink_rewrites`
in `link_rewrite.rs`) — `links fix` needs the same coverage, and the report
must derive from what was actually written.

## Tasks

- [x] A: guard `lint` scalar→table promotion at both `lint_rules.rs` call sites
- [x] A: regression test seeded with `lint = "oops"` (--severity and --enabled paths)
- [x] B: extend `links fix --apply` to rewrite frontmatter wikilinks (mirror mv's frontmatter scanning)
- [x] B: report applied fixes from actual written Replacements, not FixPlans
- [x] B: e2e test — frontmatter `related:` + body link both fixed; second run reports 0 broken
- [x] Quality gates: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [x] Dogfood: re-run the BUG-1 minimal repro and the own-KB `links fix --apply --threshold 0.95`

## Acceptance criteria

- [x] `hyalo lint-rules set MD013 --severity error` on `lint = "oops"` exits 1 with a clean error (no panic), or repairs the key
- [x] BUG-1 minimal repro: both frontmatter and body link rewritten; re-run shows 0 fixable
- [x] Own KB: the 13 remaining frontmatter fixes actually apply; `hyalo links` then reports 13 fewer broken links
- [x] `Applied: yes` output lists only fixes that were durably written