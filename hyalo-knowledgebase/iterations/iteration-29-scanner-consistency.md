---
branch: iter-29/scanner-consistency
date: 2026-03-23
status: completed
tags:
- iteration
- scanner
- error-handling
- architecture
title: 'Iteration 29: Scanner Consistency & Error Handling'
type: iteration
---

# Iteration 29: Scanner Consistency & Error Handling

## Goal

Fix the inconsistent YAML error handling between the two scan paths and unify the scanner APIs. The silent `unwrap_or_default()` in `scan_reader_multi` is the highest-priority bug risk from the code review.

## Tasks

### Fix YAML error consistency (priority)
- [x] Replace `unwrap_or_default()` in `scanner.rs:388` with proper error propagation — `scan_reader_multi` should return a parse error (or at minimum a warning) when frontmatter is malformed, matching the behavior of `read_frontmatter_from_reader`
- [x] Add e2e test: malformed YAML with multi-visitor path (task extraction, outline) should produce the same behavior as the read-frontmatter path
- [x] Decide on policy: either both paths error on bad YAML, or both silently skip — document the decision

### Unify scanner APIs (optional, lower priority)
- [x] Replace closure-based `scan_reader` with a thin `FileVisitor` wrapper
- [x] Update `links.rs` to use the visitor-based API
- [x] Remove the old closure-based `scan_reader` function
- [x] Verify inline code stripping behavior is consistent between old and new paths

### Clean up unreachable pattern
- [x] Flatten the nested match in `filter.rs:166` to remove `unreachable!()` — merge outer and inner matches into a single flat match

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria

- [x] Malformed YAML produces consistent behavior across all code paths
- [x] Scanner has a single API surface (visitor-based) — if the unification is done
- [x] No `unreachable!()` in match arms that can be structurally eliminated
- [x] All quality gates pass
