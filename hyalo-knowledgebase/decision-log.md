---
title: "Decision Log"
type: decisions
date: 2026-03-20
tags:
  - decisions
  - architecture
---

# Decision Log

## DEC-001: CLI Flag Style (2026-03-20)

**Decision:** Use idiomatic `--flag` style with clap subcommands, not Obsidian's `key=value` style.

**Why:** AI agents generate CLI calls — standard flag syntax is universally supported across all agent frameworks and shell environments.

## DEC-002: `--dir` Instead of Vault Concept (2026-03-20)

**Decision:** Accept `--dir <path>` global option (defaults to `.`) to specify the working directory. No vault registry, no vault names.

**Why:** Self-contained tool. No application state, no config files to manage. Just point at a directory.

## DEC-003: `--path` for File Targeting (2026-03-20)

**Decision:** Single `--path` flag for all file targeting. Always relative to `--dir`. Always requires `.md` extension. Accepts globs for multi-file commands (`--path "research/*.md"`). No fuzzy wikilink-style name resolution.

**Why:** AI agents work with exact file paths. Fuzzy resolution adds complexity and ambiguity. Leading `./` is tolerated and normalized. Missing `.md` triggers a helpful error with a hint.

## DEC-004: Output Formats — JSON Default, Text for Humans (2026-03-20)

**Decision:** Global `--format` option on all commands. Two formats: `json` (default) and `text`. No YAML output format.

**Why:** JSON is what AI agents parse. Text is for human debugging. YAML adds complexity with little value — frontmatter is already readable via `text`. Can be added later if needed.

## DEC-005: Structured Error Output (2026-03-20)

**Decision:** Errors go to stderr, with non-zero exit code. Error format matches `--format`:
- JSON (default): `{"error": "...", "path": "...", "hint": "...", "cause": "..."}`
- Text: plain human-readable message

Fields (`path`, `hint`, `cause`) are omitted when not applicable. The `cause` field carries the underlying OS/library error (e.g. "permission denied", "disk full").

**Why:** AI agents need parseable errors to react programmatically. The `hint` field enables self-correction (e.g. suggesting `.md` extension). The `cause` field surfaces the actual system error without the agent needing to guess.

## DEC-006: Frontmatter Rewrite on Mutation (2026-03-20)

**Decision:** Use serde_yaml_ng for both reading and writing frontmatter. Full rewrite of the YAML block on `set`/`remove` — no formatting preservation.

**Why:** serde_yaml_ng cannot preserve formatting (comments, quoting style, blank lines). Obsidian itself rewrites frontmatter on save. The files are machine-managed. Keeps the implementation simple. Can revisit if hand-edited YAML preservation becomes important.

## DEC-007: serde_yaml_ng over serde_yaml (2026-03-20)

**Decision:** Use `serde_yaml_ng` 0.10 instead of the deprecated `serde_yaml` 0.9.

**Why:** dtolnay archived `serde_yaml` — no further fixes. `serde_yaml_ng` is the community-endorsed fork with active maintenance and a drop-in API. Avoid `serde_yml` (RUSTSEC-2025-0068: unsound, causes segfaults). `serde_norway` was considered but has less community endorsement.

## DEC-008: Sandbox --dir with Path Traversal Rejection (2026-03-20)

**Decision:** `resolve_file` rejects absolute paths, backslash-prefixed paths, and any path containing `..` segments. Operations are sandboxed to `--dir`.

**Why:** Without this, `property set --path ../../../etc/important.md` could write outside the intended directory. Since `property set`/`remove` are mutation commands, this is a security boundary.

## DEC-009: Unclosed Frontmatter is an Error (2026-03-20)

**Decision:** `Document::parse` returns an error when a file starts with `---` but has no closing `---` delimiter. The streaming `read_frontmatter` reader also enforces a 100-line / 8KB budget.

**Why:** Silently treating unclosed frontmatter as "no frontmatter" would cause `property set` to write a new `---` block on top, leaving the original opening `---` in the body — corrupting the file. Failing early is safer than silent corruption.

## DEC-010: Forward-Slash Path Normalization (2026-03-20)

**Decision:** All relative paths in output and glob matching use forward slashes (`/`), even on Windows.

**Why:** `std::path::Path::to_string_lossy()` uses `\` on Windows, which breaks glob patterns and produces inconsistent JSON output across platforms. Forward slashes work on all OSes.

## DEC-011: Custom Streaming Scanner over pulldown-cmark (2026-03-20)

**Decision:** Implement a custom line-by-line streaming scanner instead of using `pulldown-cmark` or another markdown parser.

**Why:** Streams line by line with only one line buffered at a time. Supports early abort via callback pattern (`ScanAction::Stop`). No full-body buffering. Reusable for links, tags, and tasks across iterations 2-4. We fully control Obsidian-specific syntax handling (`[[wikilinks]]`, `![[embeds]]`, `%%comments%%`). No external dependency.

## DEC-012: Callback-Based Scanner with ScanAction (2026-03-20)

**Decision:** The scanner uses a visitor/callback pattern where the caller provides a closure. The closure returns `ScanAction::Continue` or `ScanAction::Stop` to control flow.

**Why:** Keeps the scanner generic — different extraction tasks (links, tags, tasks) provide different visitors. Early abort is useful for queries like "find the first N matches" without scanning entire files.

## DEC-013: Defer backlinks/orphans/deadends to Indexing (2026-03-20)

**Decision:** `backlinks`, `orphans`, and `deadends` commands are deferred to the indexing iteration, not included in iteration 2.

**Why:** These commands require scanning all files in the vault per invocation. Without an index, they would be O(n²) — each call walks every file. The indexing iteration will provide SQLite-backed lookups that make these queries efficient.

## DEC-014: Obsidian Shortest-Path Resolution (2026-03-20)

**Decision:** `[[foo]]` resolves to the `.md` file named `foo` with the shortest relative path from the vault root.

**Why:** This matches Obsidian's default resolution behavior. Path-qualified links (`[[sub/foo]]`) use exact match. Case-insensitive to match Obsidian behavior.

## DEC-015: %%comments%% Deferred as Known Limitation (2026-03-20)

**Decision:** Obsidian `%%comment%%` blocks are not yet handled by the scanner. Links inside comments will be incorrectly extracted.

**Why:** Adding comment tracking is straightforward (similar to fenced code block tracking) but wasn't needed for the initial link implementation. Documented as a known limitation. Can be added to the scanner in a future iteration since we control all the code.

## DEC-016: Single-File Only for `links` and `unresolved` Commands (2026-03-20)

**Decision:** Both `links` and `unresolved` require exactly one file via `--file`. No vault-wide mode, no glob support.

**Why:** AI agents work on one file at a time. Vault-wide link dumps are expensive (full directory walk + every file read) and produce bulk data that's hard to act on. If the agent needs links from multiple files, it calls the command per file. Bulk graph operations (backlinks, orphans) belong in a future indexed command.

## DEC-017: Minimal Link Object — target, path, label (2026-03-20)

**Decision:** The link output object contains only three fields: `target` (raw text as written), `path` (resolved file path or null), `label` (display text or null).

**Why:** Fields like `style`, `line`, `is_embed`, `heading`, `block_ref` are parser internals. An AI agent needs to know where a link points and what it's called, not how the syntax was written. Start minimal, add fields later only when a concrete use case emerges.

## DEC-018: `--file` Instead of `--path` for Single-File Commands (2026-03-20)

**Decision:** Link commands use `--file` (required, exactly one file) instead of `--path` (optional, supports globs).

**Why:** `--path` on `properties` supports globs for multi-file queries, which makes sense there. For `links` and `unresolved`, multi-file output adds complexity without value. `--file` signals "exactly one file" and avoids confusion with the glob-capable `--path`.

## DEC-019: Link Targets Must Be Resolved Paths (2026-03-20)

**Decision:** The link object includes `path` — the file path relative to `--dir` that the link resolves to, or `null` for broken links. The raw `target` field preserves the original text as written.

**Why:** AI agents work with file paths, not Obsidian note names. `[[My Note]]` is meaningless to an agent — it needs `notes/my-note.md` to open the file. Both fields are needed: `path` for navigation, `target` for display and search/replace in the source file.
