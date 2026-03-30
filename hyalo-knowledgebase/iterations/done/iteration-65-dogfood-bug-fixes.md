---
title: "Iteration 65: Dogfood bug fixes"
type: iteration
date: 2026-03-28
tags:
  - iteration
  - dogfooding
  - bug-fix
status: completed
branch: iter-65/dogfood-bug-fixes
---

# Iteration 65: Dogfood bug fixes

## Context

Dogfooding v0.5.0 on `../docs/content` (3520 files) and `../vscode-docs/docs` (339 files) found 7 bugs ranging from MEDIUM to COSMETIC. No crashes, but silent wrong results (Bug 1), inconsistent validation (Bug 2), missing feedback (Bugs 3, 4, 7), and cosmetic issues (Bugs 5, 6). This iteration fixes all of them.

## Bug list

| # | Sev | Summary |
|---|-----|---------|
| 1 | MED | `--property 'title!!!broken'` silently returns `[]` instead of erroring |
| 2 | LOW | `--tag ''` returns `[]` silently (inconsistent with `--property ''`) |
| 3 | LOW | `--sort property:nonexistent` gives no feedback when key missing from all files |
| 4 | LOW | Date property sort is lexicographic (`02/04/2026` < `1/18/2023`) |
| 5 | LOW | `--limit 0` error shows raw u64 range |
| 6 | COS | YAML `null` typed as `"text"` in properties output |
| 7 | COS | `hyalo versio` gets no typo suggestion (unlike `summry`) |

## Tasks

### Phase 1: Core library (`hyalo-core`)

- [x] Bug 1: Reject operator-like chars (`!`, `~`) in existence-check fallback in `filter.rs:232-240`
- [x] Bug 1: Add unit tests for rejected and valid property names
- [x] Bug 4: Add `try_as_iso_date()` helper and date-aware string comparison in `compare_property_values`
- [x] Bug 4: Add unit tests for date sorting
- [x] Bug 6: Split `Value::Null | Value::Object(_)` match arm in `frontmatter.rs:504` — null → `"null"`
- [x] Bug 6: Add unit test for `infer_type(&Value::Null)`

### Phase 2: CLI (`hyalo-cli`)

- [x] Bug 2: Add `validate_tag()` loop for find's `--tag` in `main.rs:1370` (mirrors pattern at line 1025)
- [x] Bug 2: Add e2e test: `hyalo find --tag ''` → error
- [x] Bug 5: Replace `RangedU64ValueParser` with custom `parse_limit` function in `main.rs:530`
- [x] Bug 5: Fix all other `--limit` args if they use the same pattern (only one occurrence found)
- [x] Bug 5: Add e2e test: `--limit 0` → "limit must be at least 1"
- [x] Bug 3: Add post-sort warning when property key is missing from all files in `find.rs`
- [x] Bug 3: Add e2e test: `--sort property:nonexistent` → warning on stderr
- [x] Bug 7: Add `strsim` to `hyalo-cli/Cargo.toml`
- [x] Bug 7: Suggest `--version`/`--help` for typos in `main.rs:1112`
- [x] Bug 7: Add e2e test: `hyalo versio` → suggests `--version`

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Manual smoke tests on both doc directories

## Design details

### Bug 1 — Reject operator-like chars in existence check fallback
**File:** `crates/hyalo-core/src/filter.rs:232-240`

Before the existence-check fallback, reject names containing `!` or `~`:
```rust
if input.contains('!') || input.contains('~') {
    bail!(
        "invalid property filter {input:?}: contains operator-like characters; \
         supported operators: =, !=, >=, <=, >, <, ~=, =~, ! (absence)"
    );
}
```
`>`, `<`, `=` are already consumed by earlier branches so they can't reach the fallback.

### Bug 4 — Date-aware string comparison in sort
**File:** `crates/hyalo-core/src/filter.rs:667` (String,String arm of `compare_property_values`)

Add a `try_as_iso_date(s) -> Option<&str>` helper that extracts the `YYYY-MM-DD` prefix from ISO date/datetime strings. In the String arm, try date comparison first, fall back to lexicographic. Only handle ISO format — `MM/DD/YYYY` is ambiguous across locales. No new dependency needed.

### Bug 6 — Null type inference
**File:** `crates/hyalo-core/src/frontmatter.rs:504`

```rust
Value::Null => "null",
Value::Object(_) => "text",
```

### Bug 2 — Validate `--tag` in find command
**File:** `crates/hyalo-cli/src/main.rs:1370`

Add tag validation loop after prop_filters parsing (mirrors existing pattern at line 1025-1030):
```rust
for t in tag {
    if let Err(msg) = hyalo_cli::commands::tags::validate_tag(t) {
        eprintln!("Error: {msg}");
        die(1);
    }
}
```

### Bug 5 — Friendly `--limit 0` error
**File:** `crates/hyalo-cli/src/main.rs:530`

Replace `RangedU64ValueParser` with custom `parse_limit`:
```rust
fn parse_limit(s: &str) -> Result<usize, String> {
    let n: usize = s.parse().map_err(|_| format!("'{s}' is not a valid number"))?;
    if n == 0 { return Err("limit must be at least 1".to_owned()); }
    Ok(n)
}
```
Check and fix all other `--limit` args across subcommands.

### Bug 3 — Warning when sort property missing from all files
**File:** `crates/hyalo-cli/src/commands/find.rs` (after `apply_sort` call)

After `apply_sort` returns, if sort is `Property(key)` and `results.len() > 1` and all files lack that property, emit warning: `"no files have property '{key}' — sort has no effect"`.

### Bug 7 — Suggest `--version`/`--help` for typos
**File:** `crates/hyalo-cli/src/main.rs:1112`

For `InvalidSubcommand` errors, check if the invalid token is within edit distance 2 of "version" or "help" using `strsim::damerau_levenshtein`, and suggest `--version`/`--help`.

## Files to modify

| File | Bugs |
|------|------|
| `crates/hyalo-core/src/filter.rs` | 1, 4 |
| `crates/hyalo-core/src/frontmatter.rs` | 6 |
| `crates/hyalo-cli/Cargo.toml` | 7 (add strsim) |
| `crates/hyalo-cli/src/main.rs` | 2, 5, 7 |
| `crates/hyalo-cli/src/commands/find.rs` | 3 |
