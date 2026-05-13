---
title: Iteration 122 — Security Audit Findings
type: iteration
date: 2026-04-16
tags:
  - iteration
  - security
  - hardening
status: completed
branch: iter-122/security-audit-findings
---

# Iteration 122 — Security Audit Findings

## Goal

Address findings from the comprehensive security audit performed on 2026-04-16. The audit covered secrets/sensitive data, path traversal & injection, unsafe code & dependencies, and CLI misuse vectors. Overall posture is strong — this iteration handles the actionable items.

## Medium Severity

### MED-1: BM25 `doc_id` out-of-bounds panic from crafted snapshot

**File:** `crates/hyalo-core/src/bm25.rs:615,672`

`doc_lengths[p.doc_id as usize]` and `doc_paths[doc_id as usize]` use `doc_id` from deserialized posting lists without bounds checking. A crafted `.hyalo-index` committed to a shared repo crashes every `hyalo find` with a text query (process abort).

- [x] Add validation in `SnapshotIndex::load_inner` after deserializing BM25 index: verify all `doc_id` values in posting lists are `< doc_paths.len()`
- [x] Reject the snapshot (fall back to disk scan) if any `doc_id` is out of range
- [x] Add test: crafted snapshot with out-of-range `doc_id` triggers fallback, not panic

### MED-2: Private filesystem paths in committed KB files

Six knowledgebase files contain absolute user-specific paths (e.g. `/Users/<name>/...`) that expose machine-specific directory layout. Committed KB content should use stable placeholders instead.

- [x] Replace local `/Users/...` paths with `<MDN_DIR>` in `dogfood-results/dogfood-v0120-iter115-followup.md`
- [x] Replace local filesystem paths with placeholders in `dogfood-results/dogfood-v0120-multi-kb.md`
- [x] Replace local filesystem paths with placeholders in `dogfood-results/hyalo-run.md`, `hyalo-run2.md`, `hyalo-run3.md`
- [x] Replace local filesystem paths with placeholders in `iterations/iteration-117-case-insensitive-link-resolution.md`

## Low Severity

### LOW-1: Terminal escape sequence injection in text output

**File:** `crates/hyalo-cli/src/output.rs`

Frontmatter values and filenames are printed without stripping ANSI escape sequences. A crafted `.md` with `title: "\x1b[31mRED"` injects terminal escapes into text output. JSON output is safe.

- [x] Sanitize control bytes (0x00-0x1F except `\n`/`\t`, 0x7F, 0x9B-0x9F) from text-format output before printing
- [x] Add test with embedded escape sequences in frontmatter values

### LOW-2: MessagePack deserialization memory amplification

**File:** `crates/hyalo-core/src/index.rs:468`

Deeply nested link graphs or BM25 indices within the 512 MiB file-size cap could allocate significantly more memory than the file size during deserialization.

- [x] Add post-deserialization size/count checks for `graph` and `bm25_index` fields (e.g., total edges, total postings)
- [x] Reject snapshot and fall back to disk scan if limits exceeded

### LOW-3: No per-file size limit on full file reads

**Files:** `crates/hyalo-core/src/scanner/mod.rs:46`, `crates/hyalo-core/src/link_rewrite.rs:125,159`

`scan_file_multi` and `link_rewrite` read entire files with no size cap. A 2+ GiB `.md` causes OOM.

- [x] Add a per-file size limit (e.g., 100 MiB) before full reads; skip oversized files with a warning

### LOW-4: No timeout or output-size cap on jq filters

**File:** `crates/hyalo-cli/src/output.rs:488`

User-supplied `--jq` filters via `jaq` have no timeout or output-size limit. Pathological filters can cause exponential output growth.

- [x] Add an output-size cap (e.g., 10 MiB) for jq filter results
- [x] Consider a wall-clock timeout for filter execution

### LOW-5: TOCTOU in read-modify-write mutations

**Files:** `set.rs`, `remove.rs`, `append.rs`, `tasks.rs`, `mv.rs`

All mutation commands have a read-modify-write window where concurrent modifications can be silently lost. The `mv` command's `exists()` → `rename()` has a similar race.

- [x] Evaluate adding advisory file locking (`flock` on Unix) for mutation commands
- [x] Alternatively, check mtime before write and fail if file changed since read

## Quality Gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance Criteria

- [x] MED-1: Crafted snapshot with bad `doc_id` causes graceful fallback, not panic
- [x] MED-2: No private filesystem paths remain in committed KB files
- [x] LOW-1: Terminal escape sequences in frontmatter are sanitized in text output
- [x] LOW-2: Oversized deserialized index structures are rejected
- [x] LOW-3: Oversized `.md` files are skipped with a warning
- [x] All existing tests pass
