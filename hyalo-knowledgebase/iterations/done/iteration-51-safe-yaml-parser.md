---
branch: iter-51/safe-yaml-parser
date: 2026-03-27
status: completed
tags:
- security
- dependencies
- parser
title: Replace serde_yaml_ng with serde-saphyr (safe Rust YAML parser)
type: iteration
---

# Replace serde_yaml_ng with serde-saphyr

## Motivation

Pre-release security audit (iteration 50) identified `unsafe-libyaml` — the backend of
`serde_yaml_ng` — as the largest unsafe surface in hyalo's dependency tree: **14,491 unsafe
expressions** (a mechanical C-to-Rust transpilation of libyaml). While mitigated by the 8 KB
frontmatter cap, eliminating this dependency removes the entire attack surface.

`serde-saphyr` is a pure safe Rust YAML parser with `#![forbid(unsafe_code)]`, YAML 1.2
compliance, native serde support, and active maintenance (last release Mar 2026). It is built
on the saphyr parser (300 stars, 1M+ downloads).

## Research summary

There is no formal standard for YAML frontmatter in markdown — it is a de facto convention
from Jekyll (2008). All tools simply parse "valid YAML between `---` lines." Obsidian restricts
its Properties UI to flat maps with scalar values and simple lists, but the underlying files
can contain any YAML. Since hyalo processes arbitrary user vaults, we need a proper YAML parser
— not a restricted subset parser.

See: [[iteration-50-security-hardening]] for full audit findings.

## Migration plan

- [x] Add `serde-saphyr` to workspace dependencies
- [x] Audit API differences between `serde_yaml_ng` and `serde-saphyr` (`Value` type, `from_str`, `to_string`, error types)
- [x] Replace `serde_yaml_ng` with `serde-saphyr` in `hyalo-core/Cargo.toml`
- [x] Update `frontmatter.rs`: swap `serde_yaml_ng::Value` → `serde_json::Value` (serde-saphyr has no own Value type)
- [x] Update `filter.rs`: adapt property matching for new `Value` variants (`Array`/`Object` instead of `Sequence`/`Mapping`)
- [x] Update `scanner.rs`: swap YAML parsing calls
- [x] Update CLI commands (`set.rs`, `append.rs`, `remove.rs`, `tags.rs`, `read.rs`, `find.rs`) referencing `serde_yaml_ng::Value`
- [x] Simplify `yaml_to_json()` — now identity since internal representation is already `serde_json::Value`
- [x] Remove `serde_yaml_ng` and `unsafe-libyaml` from dependency tree
- [x] Verify roundtrip fidelity: parse → serialize → parse produces identical frontmatter
- [x] Test with real-world vaults (own knowledgebase — 125 files, all commands work)
- [x] Run benchmarks to confirm no performance regression

## Risk

- serde-saphyr is at version 0.0.22 with 271K downloads (vs serde_yaml_ng's 2.6M). The underlying saphyr parser is more mature.
- YAML 1.2 vs 1.1 behavioral differences: `yes`/`no`/`on`/`off` are no longer booleans in 1.2. This is actually a *fix* (fewer surprising type coercions), but could change behavior for users whose frontmatter relied on YAML 1.1 quirks.
- The `Value` type may have different variant names or serialization defaults — needs careful API audit.

## Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace` (437 tests pass)
- [x] `unsafe-libyaml` confirmed gone from `Cargo.lock`
- [x] `cargo deny check` still passes (advisories ok, bans ok, licenses ok, sources ok)
