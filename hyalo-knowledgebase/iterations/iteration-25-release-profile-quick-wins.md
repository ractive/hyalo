---
branch: iter-25/release-profile-quick-wins
date: 2026-03-23
status: completed
tags:
- iteration
- performance
- binary-size
- release
title: 'Iteration 25: Release Profile Quick Wins'
type: iteration
---

# Iteration 25: Release Profile Quick Wins

The release binary is 5.8 MB with no `[profile.release]` optimizations. The bench profile already uses `lto = true` + `codegen-units = 1`, proving these work. Apply the same (plus strip and panic=abort) to the release profile for a smaller, faster binary.

## Tasks

- [x] Add `[profile.release]` to Cargo.toml with: `strip = true`, `lto = true`, `codegen-units = 1`, `panic = "abort"`
- [x] Measure binary size reduction (before vs after)
- [x] Verify all cross-platform CI targets still build (review release.yml for compatibility)
- [x] Run e2e tests to confirm no behavioral regressions
- [x] Update release.yml if any changes needed for new profile
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Build release and dogfood
