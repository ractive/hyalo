---
branch: iter-53/saphyr-hardening
date: 2026-03-27
status: completed
tags:
- security
- parser
- iteration
title: Leverage serde-saphyr Options for parsing hardening
type: iteration
---

# Leverage serde-saphyr Options for parsing hardening

## Motivation

Iteration 51 replaced `serde_yaml_ng` with `serde-saphyr` but used bare `from_str()` calls
with default options. `serde-saphyr` ships a rich `Options` + `Budget` API that can enforce
parser-level limits, strict YAML 1.2 booleans, and duplicate-key detection — all things hyalo
currently either doesn't do or does with manual pre-read caps. Wiring these in hardens the
parser with minimal code change.

See: [[iterations/done/iteration-51-safe-yaml-parser]] for the migration that made this possible.

## Tasks

### Budget-based parsing limits
- [x] Create a shared `hyalo_options()` helper in `frontmatter.rs` that returns `serde_saphyr::Options` with a tight budget (e.g. `max_events: 10_000`, `max_depth: 20`, `max_aliases: 0`, `max_nodes: 5_000`, `max_total_scalar_bytes: 8192`)
- [x] Replace all `serde_saphyr::from_str()` calls with `serde_saphyr::from_str_with_options()` using the shared options (4 call sites: `frontmatter.rs` ×2, `scanner.rs` ×1, `tags.rs` test helper ×1)
- [x] Evaluate whether the manual 200-line / 8 KB pre-read cap in `read_frontmatter_from_reader` can be relaxed now that the parser itself enforces limits — keep the pre-read cap as defense-in-depth but document why both exist
- [x] Unit test: YAML with deeply nested structures (depth > 20) is rejected
- [x] Unit test: YAML with excessive aliases/anchors is rejected

### Compact snapshot serialization
- [x] ~~Switch `rmp_serde::to_vec_named` → `rmp_serde::to_vec` in `index.rs`~~ — **Dropped**: compact (positional) encoding is incompatible with `#[serde(skip_serializing_if)]` on nested types like `OutlineSection::tasks`. Would require a separate snapshot-specific struct hierarchy — not worth the complexity for an ephemeral format.

### Duplicate key detection
- [x] Set `duplicate_keys: DuplicateKeyPolicy::Error` in the shared options
- [x] E2E test: frontmatter with duplicate keys produces a clear error message
- [x] Verify existing vaults (own knowledgebase) don't have duplicate keys — fix any found

### Strict booleans (YAML 1.2)
- [x] Add `strict_booleans: true` to the shared options so `yes`/`no`/`on`/`off` are parsed as strings, not booleans
- [x] Audit existing knowledgebase for frontmatter using `yes`/`no`/`on`/`off` as boolean values — migrate any found to `true`/`false`
- [x] Unit test: `yes` parses as `Value::String("yes")`, not `Value::Bool(true)`
- [x] Document the behavioral change in the iteration file and consider a note in `--help` or changelog

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Risk

- **Strict booleans is a behavioral change**: users with `flag: yes` in frontmatter will see it become a string instead of a boolean. The `infer_type` function will report "text" instead of "checkbox". This is the correct YAML 1.2 behavior but may surprise users. Mitigation: document it clearly.
- **Budget limits too tight**: if any real-world frontmatter exceeds the budget (e.g. very large tag lists), parsing will fail. Mitigation: test against the own knowledgebase and the vscode-docs vault before merging.
- **Duplicate key error on existing files**: some users may have frontmatter with accidental duplicate keys that currently silently resolve via last-key-wins. Mitigation: scan known vaults first.

## Behavioral changes

- **Strict booleans (YAML 1.2)**: `yes`, `no`, `on`, `off` in frontmatter are now parsed as strings instead of booleans. `infer_type` will report "text" instead of "checkbox" for these values. Use `true`/`false` for boolean properties. This is the correct YAML 1.2 behavior.
- **Duplicate keys rejected**: Frontmatter with duplicate keys (e.g. two `title:` lines) now produces an error instead of silently using last-key-wins.
- **Budget limits**: Frontmatter with >20 nesting depth, >5000 nodes, >8192 scalar bytes, or any aliases/anchors is now rejected.
