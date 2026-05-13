---
title: "Dogfood v0.12.0 — Post Iteration 122 (Security Hardening)"
type: research
date: 2026-04-16
status: active
tags:
  - dogfooding
  - security
related:
  - "[[dogfood-results/dogfood-v0120-post-iter120]]"
  - "[[iterations/done/iteration-122-security-audit-findings]]"
---

# Dogfood v0.12.0 — Post Iteration 122 (Security Hardening)

Binary: `hyalo 0.12.0` (built from source, main branch post-iter-122 merge).
KBs tested: Own KB (248 files), MDN Web Docs (14,245 files, indexed), GitHub Docs (3,520 files), plus a purpose-built adversarial test vault.

## New Feature Verification (iter-122)

### Terminal escape sequence sanitization — WORKING

Created files with real ANSI escape bytes (`\x1b[31;1m`, `\x1b[2J\x1b[H`) in frontmatter titles, status values, and tags. Text output strips the ESC byte (0x1B) so the CSI parameters appear as harmless visible text like `[31;1mRED[0m`. Verified via `xxd` — no ESC bytes in text output.

JSON output correctly preserves the raw bytes as `\u001b` escapes (serde_json). This is correct: JSON is machine-readable and should preserve data faithfully; text output is terminal-bound and must be sanitized.

### jq filter output cap (10 MiB) — WORKING

Tested with nested object explosion: `{a: ., b: ., c: .} | {a: ., b: ., c: .} | ...` (9 levels deep). The cap triggers cleanly with a descriptive error:

```json
{"cause": "jq filter output exceeds 10 MiB limit", "error": "jq filter failed"}
```

Smaller explosions (2 levels, 93 KB for 5 entries) pass through fine. The cap is precise — it stops serialization when the limit is hit rather than OOM'ing.

### Per-file size limit (100 MiB) — WORKING

Created a 110 MiB `.md` file. hyalo emits `warning: skipping ... (110 MiB exceeds 100 MiB limit)` and continues processing other files. The file still appears in results (frontmatter-only, since the small frontmatter header is read before the size check on the body), but body content is not scanned. BM25 search correctly excludes the oversized file.

### BM25 index doc_id validation — WORKING (inferred)

Corrupted a `.hyalo-index` by flipping 50 bytes in the middle. hyalo fell back gracefully to disk scan and returned correct BM25 results. No crash, no panic. The validation code (tested via unit tests in iter-122) rejects bad `doc_id` values before they can reach the indexing hot path.

### YAML bomb defense — WORKING

Created a file with YAML anchor/alias bomb (`&a [...], *a, *a, ...`). Immediately rejected: `budget breached: Anchors { anchors: 1 }` with a clear source-location error. Zero aliases permitted by the budget.

### Private filesystem path scrubbing — VERIFIED

`hyalo find -e '/Users/james'` returns no results. The six dogfood/iteration files identified in the audit now use `<MDN_DIR>` and `<PROJECT_DIR>` placeholders. Only reference to `/Users/` is in iter-50's task description (describing the original PII scrub), which is metadata about the scrub itself, not leaked paths.

### Path traversal defenses — VERIFIED

All vectors blocked with clear errors:
- `set ../../etc/passwd` → `"file resolves outside vault boundary"`
- `set /etc/passwd` → `"file resolves outside vault boundary"`
- Symlink to `/etc/passwd` → `"symlink target resolves outside vault"`
- Null byte in path → silently filtered (no crash)

### Schema validation on write — WORKING

`set --property status=banana --validate --dry-run` on an iteration file: `"banana" not in [planned, in-progress, completed, ...] (did you mean "planned"?)`. Clear error with did-you-mean suggestion and actionable hint.

## Bug Regression Testing

### UX-1 (prior): `properties rename --dry-run` verbose skipped list — UNCHANGED

Not retested (no changes in iter-122 to this area).

### MDN index load time — STABLE

MDN indexed BM25 search: 0.66s (previous: 0.67s). No regression from security hardening.

### Boolean operator warning — STILL FIXED

Not explicitly retested but BM25 searches ran clean throughout.

## Bugs Found

No new bugs found.

## UX Observations

### UX-1: Warning duplication for persistent-skip files (LOW)

When a vault contains files that are always skipped (symlinks outside vault, YAML bomb, broken frontmatter), the warnings appear on EVERY command invocation. In the adversarial test vault with 3 such files, every `find` command printed 6+ lines of warnings before results. For a real user with one or two known-broken files, this becomes noise.

Possible improvement: a `--quiet` flag to suppress warnings, or a per-vault skip-list config (`.hyalo-ignore`?) for known-bad files.

### UX-2: Oversized file appears in results without properties (LOW)

A file exceeding 100 MiB still appears in `find` results (with empty properties/sections) because its frontmatter was parsed before the body size check. This could confuse users — the file shows up but with no searchable content. Consider either fully excluding oversized files from results, or adding a visual indicator like `(body skipped: oversized)`.

### UX-3: CJK text not searchable via BM25 (KNOWN LIMITATION)

BM25 search for Japanese (`日本語`) returns no results because the tokenizer splits on whitespace/punctuation and CJK characters have no word boundaries. Regex search (`-e '日本語'`) and property regex (`--property 'title~=日本語'`) both work. This is a known limitation of whitespace-based tokenization — fixing it properly requires a CJK-aware tokenizer (e.g., character n-grams or a segmentation library like `lindera`).

## What Worked Well

### Security hardening is invisible to normal use

All the new guards (file size limits, jq output cap, TOCTOU mtime checks, escape sanitization) add zero perceptible overhead. Own KB commands run at the same speed (18-69ms range). The guards only activate on adversarial or edge-case inputs.

### Error messages are excellent throughout

Every security rejection includes: what went wrong, the offending path/value, and often a hint for what to do instead. The YAML budget errors even include source-location pointers with line/column numbers and code snippets. This is genuinely better UX than most CLI tools.

### Nested task toggle works perfectly

Tested with 3 levels of nested task indentation — all toggled correctly, preserving indentation. Task toggle also handles edge cases: empty task text, tasks with code/bold/links/emoji, extra whitespace.

### `set` creates frontmatter on bare files

Running `set --property title="Added Title"` on a file with no frontmatter block creates the `---` delimiters and adds the property. Smooth UX — no need to manually add frontmatter first.

### Corrupted index falls back gracefully

A corrupted `.hyalo-index` (50 bytes flipped) doesn't crash — it silently falls back to disk scan and returns correct results. Users would never know the index was corrupt except for slightly slower performance.

## Performance

| Command | Own KB (248) | Own KB (indexed) | MDN (14,245) | MDN (indexed) | GH Docs (3,520) |
|---|---|---|---|---|---|
| `find --limit 1` | 27ms | — | — | — | — |
| BM25 search | 69ms | 20ms | 3.50s | 660ms | 908ms |
| `summary` | 30ms | — | 956ms | 782ms | 324ms |
| `property filter` | 18ms | — | 749ms | — | 106ms |

No regressions from iter-122 security hardening. All timings within noise of the previous dogfood report.
