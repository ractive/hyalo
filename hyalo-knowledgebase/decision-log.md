---
title: Decision Log
type: decisions
date: 2026-03-20
tags:
  - decisions
  - architecture
status: reference
---

# Decision Log

## DEC-001: CLI Flag Style (2026-03-20)

**Decision:** Use idiomatic `--flag` style with clap subcommands, not Obsidian's `key=value` style.

**Why:** AI agents generate CLI calls — standard flag syntax is universally supported across all agent frameworks and shell environments.

## DEC-002: `--dir` Instead of Vault Concept (2026-03-20)

**Decision:** Accept `--dir <path>` global option (defaults to `.`) to specify the working directory. No vault registry, no vault names.

**Why:** Self-contained tool. No application state, no config files to manage. Just point at a directory.

## DEC-003: ~~`--path` for File Targeting~~ (2026-03-20) — SUPERSEDED by DEC-018

**Decision:** ~~Single `--path` flag for all file targeting.~~ Replaced by `--file` (single file) and `--glob` (pattern). See [[decision-log]].

The following still applies: always relative to `--dir`, always requires `.md` extension, no fuzzy wikilink-style name resolution. Leading `./` is tolerated and normalized. Missing `.md` triggers a helpful error with a hint.

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

**Superseded by [[decision-log#DEC-080]] (iter-214):** "hand-edited YAML preservation"
did become important — a one-key change rewrote 116 of 198 lines on a real
GitHub Docs file. Formatting is now preserved by splicing per-key line spans
in the write path; the parser choice in this decision is unaffected.

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

## DEC-014: Simple Direct Link Resolution (2026-03-20)

**Decision:** `[[foo]]` resolves via direct filesystem probes: check `foo` then `foo.md` relative to the vault root. No shortest-path search, no case-insensitive matching. Path-qualified links (`[[sub/foo]]`) use exact match.

**Why:** Keeps resolution simple and predictable for the initial implementation. Full Obsidian-style shortest-path and case-insensitive resolution is deferred to the indexing iteration.

## DEC-015: %%comments%% Deferred as Known Limitation (2026-03-20)

**Decision:** Obsidian `%%comment%%` blocks are not yet handled by the scanner. Links inside comments will be incorrectly extracted.

**Why:** Adding comment tracking is straightforward (similar to fenced code block tracking) but wasn't needed for the initial link implementation. Documented as a known limitation. Can be added to the scanner in a future iteration since we control all the code.

**Update (2026-03-21):** Resolved in iteration 10. Both block (`%%...%%`) and inline (`%%text%%`) comments are now tracked by the scanner.

## DEC-016: Single-File Only for `links` and `unresolved` Commands (2026-03-20)

**Decision:** Both `links` and `unresolved` require exactly one file via `--file`. No vault-wide mode, no glob support.

**Why:** AI agents work on one file at a time. Vault-wide link dumps are expensive (full directory walk + every file read) and produce bulk data that's hard to act on. If the agent needs links from multiple files, it calls the command per file. Bulk graph operations (backlinks, orphans) belong in a future indexed command.

## DEC-017: Minimal Link Object — target, path, label (2026-03-20)

**Decision:** The link output object contains only three fields: `target` (raw text as written), `path` (resolved file path or null), `label` (display text or null).

**Why:** Fields like `style`, `line`, `is_embed`, `heading`, `block_ref` are parser internals. An AI agent needs to know where a link points and what it's called, not how the syntax was written. Start minimal, add fields later only when a concrete use case emerges.

## DEC-018: `--file` and `--glob` as the Two File-Targeting Flags (2026-03-20)

**Decision:** All commands use exactly one of two flags for file targeting:
- `--file` — exactly one file (e.g. `property read --file note.md`, `links --file note.md`)
- `--glob` — a glob pattern matching multiple files (e.g. `properties --glob "research/*.md"`)

The old `--path` flag is retired. Both flags are always relative to `--dir` and require `.md` extension.

**Why:** `--path` was ambiguous — it could mean a single file, a directory, or a glob pattern depending on context. `--file` signals "exactly one file" and `--glob` signals "pattern matching multiple files". This is self-documenting and consistent with conventions in tools like ripgrep and fd. Supersedes the original DEC-003 `--path` convention.

## DEC-019: Link Targets Must Be Resolved Paths (2026-03-20)

**Decision:** The link object includes `path` — the file path relative to `--dir` that the link resolves to, or `null` for broken links. The raw `target` field preserves the original text as written.

**Why:** AI agents work with file paths, not Obsidian note names. `[[My Note]]` is meaningless to an agent — it needs `notes/my-note.md` to open the file. Both fields are needed: `path` for navigation, `target` for display and search/replace in the source file.

## DEC-020: Frontmatter-Only Tags — No Inline `#tag` Support (2026-03-20)

**Decision:** Tag commands only read and write the `tags` property in YAML frontmatter. Inline `#tags` in the markdown body are not extracted, searched, or modified.

**Why:** Frontmatter tags are structured data — a YAML list that can be reliably parsed, added to, and removed from. Inline `#tags` are embedded in prose, making extraction ambiguous (code blocks, URLs, headings with `#`) and modification risky (could corrupt surrounding text). Frontmatter tags are also what Obsidian uses for programmatic tag management. If inline tag extraction is needed later, it can be added as a separate read-only feature.

## DEC-021: Tasks Are File-Scoped Only (2026-03-20) — SUPERSEDED

**Original decision:** All task commands require `--file`. No vault-wide task listing.

**Why (original):** Tasks live in the markdown body, so vault-wide task search requires reading the full content of every file — not just frontmatter. Without an index, this is O(n) full-file reads per invocation.

**Superseded by:** Iteration 9 introduced vault-wide and glob-scoped task support (`--file`, `--glob`, or no scope flag). The multi-visitor scanner ([[decision-log#DEC-028: Multi-Visitor Scanner Architecture (2026-03-21)]]) made this feasible — each file is opened exactly once regardless of how many data dimensions are collected. Vault-wide tasks are now consistent with the tags API and give LLM agents a single-call way to find all open work.

## DEC-022: Tags Support Vault-Wide Operations Without Index (2026-03-20)

**Decision:** Tag commands (`tags`, `tag find`, `tag add`, `tag remove`) support vault-wide and glob-scoped operations without requiring an index. They scan all matching files on each invocation.

**Why:** Tags live in frontmatter, which is at most ~8KB per file and can be read without buffering the body. The existing `read_frontmatter` streaming reader stops at the closing `---`. For a 1000-file vault, this means reading ~8MB of data at most — well within acceptable latency. Pre-filtering optimizations (byte-level `tags:` search before YAML parse) can be explored if benchmarks show need. This is fundamentally different from vault-wide task search (DEC-021), which requires reading entire file contents.

## DEC-023: Split `properties`/`tags` into `summary` + `list` Subcommands (2026-03-21)

**Context:** The `properties` and `tags` commands each produced a single aggregate output (unique names with counts). There was no way to get per-file detail — which file has which properties or tags. Adding `--file`/`--glob` to the top-level commands overloaded a single output shape, making it unclear whether the output was aggregate or per-file.

**Decision:** Split both commands into two subcommands:
- `summary` (default) — aggregate unique names with types/counts, same as the original output
- `list` — per-file detail, each file with its property key/value pairs or tags array

The `summary` subcommand is the default, so `hyalo properties` and `hyalo tags` without a subcommand still produce the same aggregate output as before. The `--file`/`--glob` flags move to the subcommand level.

**Consequences:**
- No breaking change for callers that used `hyalo properties` or `hyalo tags` without flags — they get `summary` by default
- Callers that used `--file`/`--glob` at the top level must now place them after the subcommand name (e.g. `hyalo properties list --glob '*.md'`)
- Consistent CLI model: both `properties` and `tags` follow the same `summary`/`list` pattern
- Shared helpers extracted to avoid duplicating file-discovery logic between the two command groups

## DEC-024: Outline Command — Section-Aware Structural Extraction (2026-03-21)

**Context:** An LLM needs to understand a document's structure without reading it in full. Existing commands answer narrow questions (`properties` → metadata, `tags` → categorization, `links` → flat reference list), but none give the structural skeleton: what sections exist, what each section references, and whether work is complete.

**Decision:** Add an `outline` command that extracts per-section structure:
- **Headings** with level, text, and line number — the document skeleton
- **Frontmatter properties with names, types, and values** — matching the `properties list` shape
- **Tags** — list of tag strings from frontmatter
- **Wikilinks per section** — which section references what (not just "file has links")
- **Task counts per section** — `total`/`done` per section; `tasks` field omitted (not null) when section has no tasks
- **Code block languages per section** — content type hints

Content before the first heading gets a synthetic `level: 0` section (only if non-empty). ATX headings only — no setext.

Supports `--file`, `--glob`, and vault-wide mode (unlike `links` which is single-file per DEC-016) because outline output is lightweight.

**Why:** This gives an LLM a "table of contents with context" — enough to navigate, decide where to edit, and assess completeness without reading the full body. Each piece of enrichment (links, tasks, code blocks) answers a question an LLM would otherwise need a separate command call or full file read for.

**Consequences:**
- Scanner gains heading extraction capability (ATX headings outside code blocks)
- New section-aware accumulator pattern for attributing links/tasks/code blocks to their enclosing section
- Multi-file outline produces an array — consistent with `properties list` and `tags list` output shape

## DEC-025: Typed Structs for JSON Output (2026-03-21)

**Context:** All commands built JSON output dynamically using `serde_json::json!()` macros and manual `serde_json::Map` construction. Shapes were implicit — defined only by the code that constructed them and the tests that parsed them. The outline command needed to reuse the same property and tag shapes as existing commands.

**Decision:** Introduce `crates/hyalo-core/src/types.rs` with `#[derive(Serialize)]` structs for all JSON output shapes. Refactor all existing commands to construct typed structs instead of ad-hoc `json!()` values. Add `format_output<T: Serialize>()` to `output.rs` as the standard serialization path.

**Types introduced:** `PropertyInfo`, `FileProperties`, `PropertySummaryEntry`, `PropertyRemoved`, `PropertyFindResult`, `PropertyMutationResult`, `FileTags`, `TagSummary`, `TagSummaryEntry`, `TagFindResult`, `TagMutationResult`, `LinkInfo`, `FileLinks`, `FileOutline`, `OutlineSection`, `TaskCount`.

**Why:** Typed structs guarantee that the outline command's `properties` and `tags` fields are structurally identical to what `properties list` and `tags list` produce — the compiler enforces it. Also removes the `build_find_json` / `build_list_mutation_json` generic helpers that used dynamic key names, replacing them with specific structs per command.

**Consequences:**
- JSON output is now compiler-verified — shape mismatches are caught at build time
- New commands can reuse existing types instead of guessing the right `json!()` shape
- Removed ~50 lines of generic JSON-building helpers from `commands/mod.rs`

## DEC-026: Glob `*` Must Not Cross Path Separators (2026-03-21)

**Context:** The `globset` crate's `Glob::new()` defaults to letting `*` match across `/` path separators. This means `*.md` matched `sub/nested.md`, which contradicts standard shell glob semantics and surprises users expecting `*` to match within a single directory only.

**Decision:** Use `GlobBuilder::literal_separator(true)` when compiling glob patterns in `match_glob()`. This makes `*` match only within a single directory component. Use `**` for recursive matching across directories.

**Why:** Standard shell behavior — `*.md` should match `note.md` but not `sub/note.md`. Users familiar with any shell, ripgrep, fd, or .gitignore expect this. The previous behavior made `--glob "*.md"` equivalent to `--glob "**/*.md"`, removing the ability to scope to a single directory level.

## DEC-027: jq Filters via `jaq` for Text Output (2026-03-21)

**Context:** All commands support `--format text` but prior to iteration 7, text output was produced by a generic key=value formatter that was unreadable for nested/typed data (e.g. `properties: [{name=title, type=text, value=My Note}]`).

**Decision:** Use the `jaq` crate (pure-Rust jq interpreter) to transform `serde_json::Value` to human-readable text. Each output type gets a `&'static str` jq filter constant. The filter is looked up by sorting the JSON object's top-level keys into a comma-joined "key signature". Unknown shapes fall back to generic key: value formatting.

**Why jaq:**
- jq is purpose-built for JSON→text transformation with string interpolation, conditionals, and array iteration
- Pure Rust — no C deps, no subprocess, fast startup
- Filters are `&'static str` — changing text format = editing one string constant, no Rust recompile of business logic needed
- Standard language — no custom DSL to learn or maintain

**Tradeoffs:**
- Filter re-compilation on every call (acceptable for a CLI tool; no daemon/server use case)
- Raw string delimiter collision: `"#" * .level` in jq requires `r##"..."##` instead of `r#"..."#` in Rust 2024 edition
- Filter strings must be tested carefully — jq syntax errors produce `None` and fall back to generic format silently

**Stable versions used:** `jaq-core = "2.2.1"`, `jaq-json = "1.1.3"` (with `serde_json` feature), `jaq-std = "2.1.2"`.

## DEC-028: Multi-Visitor Scanner Architecture (2026-03-21)

**Context:** The outline command opened each file twice (once for frontmatter, once for body scanning). The summary command would need even more passes per file (frontmatter + task counting + metadata). For a vault with hundreds of files this becomes a bottleneck.

**Decision:** Introduce a `FileVisitor` trait in `scanner.rs` with callbacks for `on_frontmatter`, `on_body_line`, `on_code_fence_open`, and `on_code_fence_close`. A new `scan_file_multi` function drives multiple visitors in a single pass per file, tracking which visitors are still active. Visitors can signal `ScanAction::Stop` to opt out early.

**Key optimization:** If all registered visitors only need frontmatter (i.e. they all return `Stop` from `on_frontmatter` or have `needs_body() == false`), the file body is never read. This makes frontmatter-only queries (like `properties summary`) pay zero cost for body scanning.

**Concrete visitors:**
- `FrontmatterCollector` — captures parsed YAML `BTreeMap<String, Value>`
- `TaskCollector` / `TaskCounter` — collect or count task checkboxes
- `SectionScanner` — builds outline sections with headings, links, tasks, code blocks

**Tradeoffs:**
- Small overhead of `active: Vec<bool>` per scan call — negligible vs I/O
- Visitors receive raw body lines (not inline-code-stripped) — callers that need cleaned text call `strip_inline_code` themselves
- Frontmatter is always parsed with `serde_yaml_ng` even if no visitor needs it — overhead is negligible vs the file open syscall

## DEC-029: Summary Command — Single-Call Vault Overview (2026-03-21)

**Decision:** Add `hyalo summary [--glob G] [--recent N]` that returns a `VaultSummary` aggregating file counts (by directory), property summary, tag summary, status grouping, task counts, and recently modified files — all in one pass per file using the multi-visitor scanner.

**Why:** Agents and users need a quick orientation command before drilling down. A single `summary` call replaces what would otherwise require 4-5 separate commands (`properties summary`, `tags summary`, `tasks`, file listing, outline).

## DEC-030: Glob UX Fix on Bare `properties`/`tags` (2026-03-21)

**Decision:** Add `--file`/`--glob` args to the top-level `Commands::Properties` and `Commands::Tags` enum variants, forwarded to the default summary action. This means `hyalo properties --glob 'backlog/*.md'` works without needing the explicit `hyalo properties summary --glob ...`.

**Why:** In dogfooding, typing `hyalo properties --glob ...` felt natural but previously required `hyalo properties summary --glob ...`. The extra `summary` subcommand was friction for the most common use case.

## DEC-031: Discoverable Drill-Down Hints Architecture (2026-03-22)

**Context:** After building summary, outline, and tags commands, dogfooding revealed that LLM agents (and humans) had no way to discover follow-up commands from output alone. An agent seeing "rust: 7 files" in tags summary had to already know `hyalo tag find --name rust` exists. This is the CLI equivalent of the HATEOAS problem in REST APIs. See [[backlog/done/discoverable-drill-down-commands]] for the original backlog item.

**Decision:** Add a hint system with these architectural choices:

1. **Concrete-only hints (no templates):** Every hint is a fully executable command string. No `<placeholder>` syntax. An LLM agent can execute hints verbatim without interpolation, eliminating hallucination risk from template filling.

2. **Opt-in `--hints` flag:** Hints are off by default. This keeps default output backward-compatible and clean for scripting. Chosen over `--no-hints` (opt-out) because hints add noise for most programmatic consumers.

3. **JSON envelope `{"data": ..., "hints": [...]}`:** When `--hints` is active, the original output is wrapped in an envelope. The `data` field contains the unmodified original output; `hints` is a flat string array of commands. This avoids polluting existing output types with hint fields.

4. **Suppress when `--jq` is active:** If the user passes `--jq`, they are doing custom extraction and the envelope would break their filter. Hints are silently suppressed.

5. **State-aware hint generation:** `generate_hints()` inspects the actual serialized output data — which tags appear, which properties exist, what counts are highest — and produces relevant commands. This is not a static lookup table; hints adapt to the data.

**Why these tradeoffs:**
- Concrete hints are safer than templates for automated agents but cannot cover every possible drill-down (only the most useful ones). Acceptable because the goal is discoverability, not exhaustive API documentation.
- The JSON envelope adds one level of nesting but keeps the `data` field structurally identical to non-hint output. Callers that don't use `--hints` see zero change.
- Flag propagation (`--dir`, `--glob`) in hints ensures suggested commands work in the caller's current context without manual flag copying.

**Consequences:**
- New `crates/hyalo-cli/src/hints.rs` module with `HintSource`, `HintContext`, and `generate_hints()` function
- New `format_with_hints()` in `crates/hyalo-cli/src/output.rs` alongside existing `format_output()`
- 37 unit tests + 14 e2e tests covering hint generation and flag interactions
- Found and fixed tags summary sort bug during dogfooding (hints were showing alphabetically-first tags instead of most-used)

## DEC-032: ~~YAML Parse Errors Are Hard Errors~~ (2026-03-23) — UPDATED iter-35: read-only commands skip malformed files

**Context:** The codebase had two scan paths for reading markdown files: `read_frontmatter_from_reader` (used by `properties`, `tags`, mutation commands) and `scan_reader_multi` (used by `find`, `summary`, task extraction). The former propagated YAML parse errors via `?`; the latter silently swallowed them with `unwrap_or_default()`, returning an empty property map for malformed frontmatter. This inconsistency meant `hyalo find` would silently skip broken files while `hyalo properties` would warn about them.

**Decision:** Both paths now treat malformed YAML frontmatter as a hard error, propagating via `anyhow::Context("failed to parse YAML frontmatter")`. Commands that want graceful degradation (like `properties summary`) catch the error at the command level using `is_parse_error()` and emit a warning — but the scanner itself always surfaces the error.

**Why:** Silent data loss is worse than a noisy error. A user with a broken frontmatter file should learn about it immediately, not wonder why `find --property status=planned` returns fewer results than expected. Commands that aggregate across many files can opt into graceful skip at their own level.

**Consequences:**
- `scan_reader_multi` now returns `Err` on malformed YAML (was `Ok` with empty props)
- `scan_reader` / `scan_file` (closure-based API) now delegates to `scan_reader_multi` via a `ClosureVisitor` wrapper, unifying the two code paths
- The old 80-line `scan_reader` implementation and its dependency on `frontmatter::skip_frontmatter` are removed
- All read-only commands (`find`, `summary`, `properties`, `tags`) gracefully skip files with malformed YAML: emit a warning to stderr and continue (iter-35 extended this from `properties`/`tags` to `find`/`summary`)
- Mutation commands (`set`, `remove`, `append`) still fail hard on malformed YAML — safe, since silent corruption would be worse

## DEC-033: Advanced Filter Syntax for iter-36 (2026-03-25)

**Context:** `hyalo find` supported existence checks (`--property K`) and comparison operators but had no way to express absence, substring/regex matches on property values, or dynamic section headings. `--section` required exact whole-string match, making it brittle for headings with date/counter suffixes (e.g. `## DEC-031: ... (2026-03-22)`).

**Decisions:**

1. **Property absence: `--property '!K'`** — Chosen over a separate `--no-property` flag because it is composable with the existing `--property` repetition and consistent with the `!=` operator. The `!` prefix is unambiguous since property names cannot start with `!` in YAML.

2. **Property value regex: `--property 'K~=pattern'` / `'K~=/pattern/flags'`** — The `~=` operator was chosen to parallel CSS attribute selector and existing tool conventions while being visually distinct from `=` and `!=`. Bare `K~=foo` is unanchored regex (contains semantics). For list properties, matches if any element matches. The `i` flag enables case-insensitive matching, consistent with how `--regexp` works on body content. Substring match was not added as a separate operator since `~=foo` already provides it.

3. **Section substring default** — `--section` changed from exact whole-string to case-insensitive substring (contains) matching. This is backwards-compatible in practice: any query that previously matched will still match (exact match is a subset of substring match). Power users can use `--section '~=/regex/'` for regex. Level pinning (`## Foo`) continues to work with substring.

4. **Glob negation: `!pattern`** — Follows ripgrep convention (`--glob '!pattern'`). Simpler than adding a separate `--exclude` flag; consistent with rg muscle memory.

**Consequences:**
- `PropertyFilter` gained `Absent` and `Regex` variants
- `SectionFilter` gained a `Regex` variant and changed its default match mode from `Exact` to `Contains`
- `match_glob()` checks for `!` prefix and inverts the match result
- Help text and COOKBOOK updated with examples for all four operators

## DEC-034: Subcommand Groups for `properties` and `tags` (2026-03-25)

**Context:** `hyalo properties` and `hyalo tags` were leaf commands that only showed aggregate summaries. Iteration 37 adds bulk rename operations (`properties rename`, `tags rename`). Rather than adding top-level `rename-property` / `rename-tag` commands, the existing commands were restructured as subcommand groups.

**Decisions:**

1. **Explicit `summary` subcommand** — `hyalo properties summary` / `hyalo tags summary` replace the implicit summary behavior. Bare `hyalo properties` / `hyalo tags` now show help text listing available subcommands.

2. **No backward compatibility shim** — Breaking change accepted since hyalo has no external users yet. The `summary` subcommand makes the CLI more discoverable (you can see all available operations under `hyalo properties help`).

3. **Rename uses `--from`/`--to` flags** — `hyalo properties rename --from old --to new` rather than positional arguments. Flags are more explicit and harder to get wrong.

4. **Property rename skips conflicts** — If the target key already exists on a file, the file is skipped and reported in a `conflicts` array. This prevents silent data loss.

5. **Tag rename is atomic per-file** — If the new tag already exists on a file, only the old tag is removed (no duplicate). This ensures idempotent behavior.

**Consequences:**
- All existing `hyalo properties` / `hyalo tags` calls (e2e tests, SKILL.md, CLAUDE.md examples, hint generation) updated to `properties summary` / `tags summary`
- `PropertiesAction` and `TagsAction` subcommand enums added to the CLI
- Rename results include `modified`, `skipped`, and (for properties) `conflicts` arrays

## DEC-035: No LLM Prompt Injection Mitigation in CLI Output (2026-03-27)

**Context:** During the security hardening phase (iter-50), we evaluated whether hyalo's CLI output could be exploited for LLM prompt injection — e.g., an attacker embedding malicious instructions in YAML frontmatter values, markdown body content, filenames, or section headings that Claude would then follow when consuming hyalo's output.

**Decision:** No action taken. This is not a hyalo-specific problem and no hyalo-specific mitigation is warranted.

**Why:**

1. **Not tool-specific.** Every tool that feeds file content into an LLM context has the identical attack surface — `cat`, `grep`, `git diff`, the built-in `Read` tool, etc. Hyalo is no different.

2. **Sanitization would be counterproductive.** Stripping patterns like "ignore previous instructions" from legitimate documentation would degrade the tool's core purpose (making vault content available to the user and their LLM).

3. **Sanitization would be fragile.** Any blocklist approach is an arms race against creative prompt formulations. It provides a false sense of security while breaking valid content.

4. **Hyalo's JSON output is a partial natural defense.** Structured JSON with named fields makes it harder for an LLM to confuse data with instructions compared to raw freeform text output.

5. **The problem belongs to the LLM layer.** Distinguishing instructions from data is the LLM's responsibility, not the tool's. Claude already has system-level instructions and tool-result tagging to help with this.

**Consequences:**
- No output sanitization, escaping, or filtering added to hyalo
- If hyalo ever adds a server mode (MCP server, HTTP API) serving vault content to remote/untrusted clients, this decision should be revisited — trust boundaries change in that scenario

## DEC-036: Orphan vs Dead-End Terminology (2026-03-29)

**Context:** During v0.6.0 dogfood tidy across 3 external repos, the `summary` command's orphan count (25 in vscode-docs) diverged from a `find --fields backlinks` query filtering for zero backlinks (56 files). Investigation revealed `summary` defines orphans as files with **no inbound AND no outbound links** (fully isolated), matching Obsidian Graph View and Foam. The backlinks-based query finds files with **no inbound links** (unreachable), matching the Wikipedia/SEO definition.

Research across tools:
- **Wikipedia/SEO** (older, broader): orphan = no inbound links; dead-end = no outbound links
- **Obsidian Graph View / Foam / Logseq**: orphan = no links in either direction (isolated)

**Decision:** Keep hyalo's orphan definition as-is (no inbound AND no outbound = fully isolated, consistent with Obsidian). Add a new **dead-end** concept: files that have inbound links but no outbound links (orphans are excluded and reported separately).

**Why:** Both definitions are useful. Orphans (isolated files) are clearly disconnected. Dead-ends (inbound links but no outbound links, excluding orphans) flag navigation dead-ends where users arrive but have nowhere to go. Note: many dead-ends are not actionable — top-level files in root or well-known directories (e.g. `/iterations/`) are easily accessible by browsing and don't need outbound links.

**Consequences:**
- `summary` gains a `dead_ends` section alongside `orphans`
- No change to existing orphan behavior
- See [[iterations/done/iteration-67-summary-enhancements]]

## DEC-037: Won't Fix — False-Positive Links from Square Brackets in Body Text (2026-03-29)

**Context:** Dogfood on legalize-es (8,643 files) found 3 broken links that were false positives: markdown reference-style links like `[Opcion][1]` (where `[1]` is a ref label) and math expressions in square brackets like `[0,35 * kms.recorridos]`. This was flagged in v0.5.0 round 2 and again in v0.6.0.

**Decision:** Won't fix.

**Why:**

1. **Negligible rate.** 3 false positives in 8,643 files (0.03%). The other two repos (3,520 and 339 files) had zero false positives of this type.

2. **Reference-style links are real markdown syntax.** `[text][ref]` IS a valid markdown link — the issue is that hyalo doesn't resolve reference-link definitions (`[1]: http://...`). Adding a full reference-link resolver is significant parser work for near-zero practical impact.

3. **Square brackets in prose are ambiguous.** Distinguishing `[math expression]` from `[link text]` in raw markdown is impossible without full rendering context. Any heuristic would be fragile.

**Consequences:**
- No changes to link parsing
- Accept occasional false positives on repos with heavy use of reference-style links or mathematical notation
- If this becomes a real problem for a specific repo, `--quiet` suppresses warnings

## DEC-038: Won't Fix — Template/Liquid Syntax in Links (2026-03-29)

**Context:** Dogfood on docs/content (Hugo, 3,520 files) flagged thousands of "broken" links that contain Liquid template syntax like `{% ifversion ghes %}...{% endif %}`. These links are dynamically expanded at Hugo build time and work correctly on the live site.

**Decision:** Won't fix. Hyalo is a static analysis tool operating on raw markdown files — it cannot and should not evaluate template engines.

**Why:**

1. **Unbounded scope.** Hugo uses Go templates, Jekyll uses Liquid, Docusaurus uses JSX — supporting any template engine means supporting all of them.

2. **Correct behavior.** Reporting these as broken is technically accurate: the raw link target does not resolve to a file. The user knows their build pipeline expands these.

3. **Workaround exists.** Users can filter template-heavy files with `--glob '!**/template-dir/**'` or pipe through `--jq` to exclude links matching template patterns.

**Consequences:**
- No template-aware link resolution
- Documentation/SKILL.md could note this as expected behavior for SSG repos

## DEC-039: Won't Fix — `children` Frontmatter as Implicit Links (2026-03-29)

**Context:** Hugo's docs/content repo uses a `children` frontmatter property (list of page paths) to define navigation hierarchy. Since hyalo only counts `[[wikilinks]]` and `[markdown](links)` in the body as links, 52% of files appeared as orphans despite being reachable via the `children` navigation tree.

**Decision:** Won't fix. Frontmatter properties are data, not links.

**Why:**

1. **Convention-specific.** `children` is a Hugo convention. Other SSGs use `sidebar`, `nav`, `menu`, `weight`, or directory structure. There's no universal standard.

2. **Semantic ambiguity.** A frontmatter list of paths could be references, related content, aliases, or data — hyalo can't infer intent from the key name alone.

3. **Workaround exists.** Users can exclude known navigation-structured directories from orphan analysis, or use `--jq` to subtract files listed in `children` properties from the orphan set.

**Consequences:**
- Orphan counts will be inflated for SSG repos that use frontmatter-based navigation
- This is expected and documented behavior, not a bug

## DEC-040: Context-Aware Hints with Descriptions (2026-03-30)

**Context:** [[iterations/done/iteration-80-smarter-hints]] evolved the hint system introduced in [[backlog/done/discoverable-drill-down-commands]] (DEC-031). Two changes: (1) hints now include a human-readable description alongside the command, and (2) hints are generated for all commands — not just the original four (find, summary, properties summary, tags summary).

**Decision:** Change the hint format from a flat string array to an array of `{"description": "...", "cmd": "..."}` objects. Extend hint generation to all 15 command variants including mutations, read, backlinks, mv, task operations, links fix, create-index, and drop-index.

**Why these tradeoffs:**

1. **Descriptions make hints self-documenting.** An LLM seeing `{"description": "Find files with open tasks", "cmd": "hyalo find --task todo"}` understands intent without parsing the command. Humans scanning text output benefit from the `# description` suffix too.

2. **Breaking JSON change is acceptable.** The `--hints` envelope is a UX feature, not a stable API contract. Consumers using `--jq` never see hints (they are suppressed). The `data` field remains structurally identical.

3. **All-command coverage teaches the full CLI.** Mutation hints suggest verification commands (`hyalo find --file X`), dry-run hints suggest `--apply`, and create-index suggests drop-index. This turns every command into a learning opportunity.

4. **Performance constraint preserved.** All hint generation operates on the already-computed JSON output — no additional file I/O. Hints are O(n) on result count with a hard cap of ~5 hints per command.

**Consequences:**
- JSON envelope: `{"data": ..., "hints": [{"description": "...", "cmd": "..."}]}`
- Text format: `  -> hyalo cmd  # description`
- Updates DEC-031 point 3 (envelope format) — string array → object array
- `HintSource` enum expanded from 4 to 15 variants
- 12 generator functions in `hints.rs` covering all command families

## DEC-041: Markdown Linter — Embed mdbook-lint-core + HYALO Native Rules (2026-05-04)

**Context:** [[iterations/done/iteration-126-markdown-linter]] extends `hyalo lint` from frontmatter-only validation into a full markdown rule engine. Two design framings were considered: (A) hand-roll a small set of HYALO-specific rules only, or (B) embed `mdbook-lint-core` for stock markdownlint coverage (MD001..MD059) and add HYALO native rules on top.

**Decision:** Adopt framing (B). Bundle `mdbook-lint-core` + `mdbook-lint-rulesets` via a new `crates/hyalo-mdlint` crate, and add three HYALO native cross-cutting rules — HYALO001 (bare `[]` checkbox), HYALO002 (frontmatter `title` ↔ first H1 agreement), HYALO003 (`status: completed` requires all task checkboxes ticked). Severity is hyalo-controlled via a static override table; user overrides land last. Curate a default-on set (~14 stock rules) and default-off set (noisy/stylistic). Output is shaped for AI agents — per-rule caps, summary mode, hint chains.

**Why these tradeoffs:**

1. **Stock coverage is a freebie.** Embedding mdbook-lint-core gives ~59 markdownlint rules at the cost of ~3 MB binary growth and ~24 transitive crates. Hand-rolling parity would burn weeks for no incremental UX value.

2. **Cross-cutting rules are the headline.** No other linter has hyalo's parsed model in hand. HYALO001/002/003 enforce invariants that span frontmatter and body — these are rules nobody else can offer, and they justify the crate-organization overhead.

3. **Severity belongs to hyalo.** mdbook-lint-core has no config-level severity override. We post-process violations after collection: a static `HashMap<&str, Severity>` rewrites severity per rule, then user overrides from `[lint.rules]` win. This keeps the user model coherent (one place to tune severity) regardless of upstream defaults.

4. **Curated default-on set is opinionated but recoverable.** v1 ships a guess based on cheap-autofixable-structural heuristics. Worst case: flip 1–2 rules in v0.15.x after dogfooding feedback. Users can always override via `hyalo lint-rules set <ID> --enabled true|false`.

5. **JSON envelope break is acceptable.** The previous flat `violations: [...]` shape becomes `rule_groups: [...]`. Small installed base, and the new shape is what AI agents actually want — grouped, capped, with explicit `truncated` flags.

6. **Per-rule arg pass-through deferred.** mdbook-lint-core uses toml 0.5 while hyalo uses toml 1.x. A translation layer is non-trivial. v1 uses upstream defaults; we revisit if a user actually asks (e.g., MD013 `line_length=120`).

**Consequences:**
- New crate `crates/hyalo-mdlint` owns the engine factory, severity table, and HYALO rule provider
- New `[lint]` and `[lint.rules]` sections in `.hyalo.toml`
- New `hyalo lint-rules` command mirrors `hyalo types` / `hyalo views` shape
- New flags on `hyalo lint`: `--detailed`, `--rule`, `--rule-prefix`, `--max-per-rule`, `--fix-rule`
- Body autofix runs after frontmatter autofix; conflicts deferred and reported
- Snapshot index does not accelerate body lint (body bytes aren't indexed) — documented in `lint --help`

## DEC-042: Remove `unsafe` UTF-8 Shortcuts; Gate Parallelism for Miri (2026-05-23)

**Context:** hyalo had four `unsafe` blocks — three `String::from_utf8_unchecked` / `str::from_utf8_unchecked` in the scanner hot path, and one `libc::kill(pid, 0)` for PID liveness. The UTF-8 unchecks dated from when the scanner was written to maximise throughput on large vaults (MDN-scale, 250 MB). They were safe by inspection (ASCII-only mutations) but fragile across refactors. See [[research/miri-unsafe-audit]] for the full audit.

**Decision:** Remove the three UTF-8 `unsafe` blocks. Keep `libc::kill`. Add Miri as a manual-only gate via `justfile` recipes.

**Why these tradeoffs:**

1. **Perf cost is invisible.** Microbench shows +5 ns per call when backticks/comments are present in a line, +0 ns on the fast path. MDN 250 MB end-to-end: ~1.1 s before and after — change is lost in measurement noise.

2. **Safety burden was real.** Each `unsafe { from_utf8_unchecked }` carried a multi-paragraph SAFETY block establishing an invariant about ASCII byte substitution. Any future refactor that touched the strip logic had to re-prove the invariant or risk UB. Re-validation is one line and obvious.

3. **`scanner/mod.rs` was a free win.** That call site was `is_ok()` + `from_utf8_unchecked` on the same bytes — a redundant validation. Refactored to reuse the original `Result::Ok(s)`. Zero re-validation, zero perf cost.

4. **`libc::kill` stays.** No portable std equivalent for "is PID alive?"; the `sysinfo` crate is a heavy dep for one check. The call is one line with a documented SAFETY block.

5. **Miri is a manual gate.** Consistent with the existing convention that Miri + cargo-fuzz run manually rather than in CI (their interpreter overhead would push CI runtime past acceptable thresholds, and the modules that bring in `regex`/`aho-corasick` are pathologically slow under interpretation).

6. **`rayon::par_iter` doesn't run under Miri.** Gated with `#[cfg(not(miri))]` + serial fallback in `index.rs` and `lint.rs` so the parsing modules can still be exercised. No effect on non-Miri builds.

**Consequences:**
- `unsafe` count: 4 → 1 (only `libc::kill`)
- ~30 lines of SAFETY documentation deleted
- New `justfile` with `miri`, `miri-filter`, `miri-all`, `check`, `fmt` recipes
- Nightly toolchain + miri component required for `just miri`
- Miri pass on `scanner::`, `bm25::`, `links::`, `heading::`, `frontmatter::` — 262 tests, no UB
- Pre-existing brittle test surfaced: `bm25::test_bm25_serde_round_trip` uses `f64::EPSILON` tolerance for summed scores; failing under Miri due to HashMap iteration order. Not UB; widen tolerance when convenient.

## DEC-043: Schema-as-Template; No Templating Engine for `hyalo new` (2026-05-24)

**Context:** Consumer repos (ff-rdp and similar) wanted a way to create new markdown files from schema, without manually copying frontmatter boilerplate. The tempting design would have been a templating mini-DSL (`{var}`, `{date}`, `{{ #if ... }}`). See [[research/ff-rdp-discipline-consumer-notes]] for the full wishlist and design dialogue.

**Decision:** No templating engine. Schema declarations ARE the template. `hyalo new --type <name> --file <path>` synthesises a skeleton file from the type schema: required frontmatter properties with type-appropriate placeholders, required body sections with `TBD` paragraphs. Zero `{var}` substitution. The only "smart default" is `date`-typed properties getting today's ISO date — and that is typed-default behaviour, not templating.

**Why these tradeoffs:**

1. **Schema is already the source of truth.** Adding a separate template file would split the authority for "what does a valid file look like" between the schema and the template. When they drift, the agent gets confused. One source, one place.

2. **Intentionally invalid output drives the lint loop.** `TBD` placeholders fail `hyalo lint`. This is the mechanism. The agent creates a file, runs lint, and reads the violations to know exactly what to fill in. Pre-validated output would defeat this.

3. **No `--force`, no `mkdir -p`.** These are rejected to keep the surface area small and the error messages clear. The agent handles file existence checks and directory creation explicitly, which surfaces intent.

4. **`required-sections` defers hierarchy correctness to markdownlint MD001.** We check presence and level, not level-skipping. One concern at a time.

5. **`dir` field on type schemas rejected.** Agent specifies `--file` explicitly. A `dir` field would add implicit location logic that is hard to explain and easy to misconfigure.

**Consequences:**
- `hyalo new` is stateless: no template files to manage, no migration path needed when schema changes
- Agents using `hyalo new` must handle `lint` output to know what to fill in — the feedback loop is explicit
- Bulk creation (`--batch`) deferred; single-file is the unit for now
- `item_pattern` and `required-sections` schema extensions ship in the same iteration, making the lint pass immediately useful after `hyalo new`

## DEC-044: VCS-Agnostic Scoping via `--files-from` (2026-05-24)

**Context:** Consumer repos (ff-rdp and similar, see [[research/ff-rdp-discipline-consumer-notes]]) wanted to scope `hyalo lint` to only the files touched on a branch — "diff-aware lint in CI". The first instinct was a `--since <git-ref>` flag that would shell out to `git diff --name-only`. See [[research/ff-rdp-discipline-consumer-notes]] for the full discussion.

**Decision:** No git integration. Add `--files-from <PATH>` (or `-` for stdin) instead. The caller supplies the file list via any tool that fits their VCS: `git diff --name-only`, `hg status -n`, `make .changed`, a script. Hyalo accepts a flat newline-separated list and operates on exactly that set.

**Why these tradeoffs:**

1. **VCS-agnostic by design.** A `--since` flag would make hyalo depend on `git` being available, on the vault being a git repository, and on a specific ref format. Callers using Mercurial, Jujutsu, or no VCS at all would be excluded. `--files-from` lets every caller provide the file set via whatever tool fits.

2. **No git coupling in the binary.** Adding `git` as a shell-out dependency is risky: it may not be on `$PATH` in CI containers, the output format varies by version, and error handling is fragile. We reject this complexity.

3. **Silent skip for non-.md and out-of-vault paths.** CI diff output includes build artifacts, source files, deleted files — everything. Requiring callers to pre-filter with `grep -E '\.md$'` and `--diff-filter=AMR` defeats the ergonomic goal. Silent skips with JSON envelope counters (`files_missing`, `files_skipped_non_md`, `files_skipped_outside_vault`) give callers visibility without forcing them to wrap hyalo in a shell pipeline.

4. **`--files-from` is strictly a source of paths**, equivalent to `--glob` and `--file`. All three feed the same downstream path-handling pipeline. Mutual exclusion keeps the source of truth obvious.

5. **`--index` semantics preserved.** When `--index` is given, the snapshot is the source of truth — `--files-from` filters into it, never past it. A path in `--files-from` not present in the index counts as `files_missing`. This matches what `--index` already means and avoids a hidden disk-rescan fallback.

**Rejected alternatives:**
- `--since <git-ref>` with built-in `git` integration — rejected because it ties hyalo to git specifically
- `hyalo diff <revA>..<revB>` as a first-class command — `git diff` + `--files-from` covers the case with no extra command surface
- `--files-from0` (NUL-separated) — deferred; newline covers 99% of cases
- Combining `--files-from` with `--glob` (intersection or union) — rejected as confusing; mutual exclusion is enforced

**Consequences:**
- `git diff --name-only origin/main | hyalo lint --files-from -` works out of the box
- Callers don't need to filter git diff output — non-.md paths are skipped silently
- No git binary required; works in any CI environment with hyalo on PATH
- `--files-from` is available on `find`, `lint`, `mv`, `set`, `remove`, and `append` — the commands that already accept `--glob`

## DEC-045: Wall-Clock Signal for Index-Suggestion Hints (2026-05-25)

**Context:** iter-144 — index-suggestion hint for slow queries and large vaults.

**Decision:** Use wall-clock elapsed time (not CPU time or file count alone) as
the primary signal for the slow-query hint, with a 500 ms threshold. Use file
count (>500 files) as the signal for the large-vault summary hint.

**Rationale:**

- **Wall-clock, not CPU time.** I/O is the dominant cost for hyalo vault scans;
  wall-clock matches what the user perceives as "slow". CPU time would exclude
  I/O wait and underreport the user-visible latency.
- **500 ms slow-query threshold.** Shorter than the human "wait, this is slow"
  threshold (~1 s) with margin; longer than typical scans on small vaults
  (~100 ms). Calibrated from MDN dogfooding where property-only queries on a
  14K-file vault took ~1.5 s without an index vs ~80 ms with one.
- **500 files large-vault threshold.** Vaults above this size show measurable
  benefit from a snapshot index. Below it, disk scan is fast enough not to
  warrant the hint.
- **Global threshold, not per-command.** A single threshold for all eligible
  commands is simpler than per-command tuning. Eligible commands are those that
  scan the vault: `find`, `lint`, `backlinks`, `properties summary`,
  `tags summary`, `summary`, `read`.
- **Suppress on --index / --index-file active.** If the user already requested
  a snapshot, suppress both hints — even if the index load failed and fell back
  to a disk scan. The intent to use an index is the suppression signal, not the
  outcome.
- **Suppress slow-query hint on --quiet.** `--quiet` is the user's explicit
  opt-out from advisory output; it should silence the hint.

**Alternatives rejected:**
- Per-command thresholds: premature tuning, adds complexity without data.
- Auto-index config (`auto_index = true`): hyalo shouldn't manage index
  lifecycle silently. Lint and hints surface the suggestion; the user runs
  `create-index`.
- CPU time: misses I/O wait, doesn't reflect user-perceived latency.

## DEC-046: One Shared Frontmatter Opening-Delimiter Policy (2026-07-03)

**Decision:** A single predicate (`opening_delimiter` in `frontmatter/parse.rs`)
decides whether a line opens a frontmatter block, and every parse path in the
workspace uses it: the streaming reader, `find_body_offset` (write path),
`extract_frontmatter`, `skip_frontmatter` (`read`/`task read`), both scanner
entry points (`find` etc.), and lint's body split. The policy: an optional
single UTF-8 BOM, then a line that is exactly `---` followed by a line
terminator or EOF. Leading whitespace never opens frontmatter. A BOM is
preserved byte-for-byte on rewrite. The frontmatter block is re-emitted with
the file's own line-ending style (CRLF stays CRLF).

**Why:** iter-158's two critical findings were both caused by parse paths
disagreeing on this check — the read path accepted what the write path
rejected (BOM, leading space), so `set`/`remove`/`append` prepended a second
frontmatter block and silently demoted the real one to body (data loss,
reported as success). Two follow-up rounds (dogfooding, then PR review) found
the same drift in the scanner and in `skip_frontmatter`, proving hand-rolled
copies of this check *will* drift. Matching Obsidian/Jekyll (no leading
whitespace) keeps the rule unambiguous.

**Rule for future code:** never hand-roll an opening-`---` check; call the
shared predicate (`is_opening_delimiter` is crate-visible in hyalo-core).

## DEC-047: Per-Rule Column Units for mdlint Fix Conversion (2026-07-03)

**Decision:** `line_col_to_byte` in hyalo-mdlint selects its column unit per
rule via an explicit allowlist (`rule_uses_byte_columns`): rules verified to
emit byte-based columns (MD009, HYALO001) get a byte-length walk; every other
rule gets a char-index walk. MD011 additionally gets a guarded +1 on its end
column (upstream emits the inclusive position of the closing `]`), applied
only when the byte at that offset really is `]`.

**Why:** upstream mdbook-lint rules are inconsistent about what a fix column
means — MD009 computes columns from `line.len()` (bytes) while MD034/MD011
index into a `Vec<char>` (chars). On any line containing multibyte UTF-8 the
two units diverge, and using the wrong one either drops the fix (byte target
unreachable by char walk — the pre-iter-158 bug) or lands on the wrong byte
and corrupts the file (char target overshot by byte walk — the regression the
iter-158 PR review caught). The failure modes are asymmetric, so the default
for unaudited rules is the char walk: its worst case is a dropped fix,
never corruption.

**Rule for future code:** before adding a rule to the byte-column allowlist,
verify its column math in the upstream source and add a multibyte-line
regression test (see `md034_fix_correct_on_line_with_multibyte_prefix`).

## DEC-048: Shared Release Pipeline in ractive/release-workflows (2026-07-10)

**Decision:** hyalo, hoppy, and ff-rdp release via one reusable GitHub
Actions workflow in [ractive/release-workflows](https://github.com/ractive/release-workflows),
pinned by tag (`@v0.1.3`). Each repo keeps only a thin caller
(`.github/workflows/release.yml`) with repo-specific inputs, plus a
`workflow_dispatch` trigger that runs the whole pipeline in dry-run mode.
The shared repo tests itself: actionlint + zizmor on every push, and an
end-to-end selftest that runs the real pipeline against a bundled fixture
crate on four targets.

**Why:** the three pipelines were copy-paste descendants that had already
drifted (only hoppy had deb/rpm and man pages; only ff-rdp had SBOM and
attestations; only hyalo/ff-rdp had winget) and fixes did not propagate.
A reusable workflow converges everyone on the union of features, keeps
battle-tested logic (crates.io retry, per-target cache keys, hermetic
GIT_COMMIT provenance), and gives uniform attestation identity (GitHub's
SLSA-L3-style trusted-builder pattern). GoReleaser was the runner-up but
its cargo-workspace support is weak and all three repos publish 2–3 crates
(see [[research/release-pipeline-unification]]).

**Rule for future changes:** never edit release logic in an app repo —
change release-workflows, let its selftest validate, tag a new version via
`gh release create`, then bump the `@vX.Y.Z` pin in the callers. Before
merging a caller change, run the dry-run dispatch on the branch
(`gh workflow run release.yml --ref <branch>`); it caught five real bugs
during the migration (multi-line pre-package-command flattening, cargo run
--bin ambiguity, linux-packages binary path, hoppy's Windows-only test
stack overflow, hoppy's debug xtask man-page generation overflowing the
Windows stack) that lint and the fixture selftest could not. All three
repos' dry-runs were green on v0.1.3 before merge.

## DEC-049: OKF Conformance as a Lint Profile Overlay, Not an `okf lint` Subcommand (2026-07-17)

**Decision:** Ship OKF §9 conformance validation as `hyalo lint --profile okf` — an
ephemeral overlay that merges the same `profile-okf.toml` fragment `init --profile okf`
materializes (via the shared `profiles::merge_into_config`) and re-parses it — rather than
a dedicated `okf lint` subcommand or a hard-coded ruleset. The profile fragment now also
stamps `[lint] profile = "okf"`, so on an initialized vault a plain `hyalo lint` runs the
same rules; `--profile okf` there is a no-op (idempotent).

**Why:**
- **One code path.** `--profile` composes with the whole existing lint surface (`--fix`,
  `--rule`, `--strict`, `--files-from`) with no forked logic — the overlay only re-derives
  `SchemaConfig`/`LintConfig` before dispatch.
- **Works config-free.** CI and cloned third-party bundles have no `.hyalo.toml`; the
  overlay merges the fragment onto an empty base, so validation Just Works.
- **DRY + idempotent by construction.** Reusing the init fragment-merge means the overlay
  and materialized config can never drift, and re-merging an already-okf config is a no-op.
- **No new noun.** An `okf lint` subcommand would duplicate lint's flags and diverge; the
  profile is data, added by one entry, not a parallel command (mirrors DEC on data-driven
  init profiles).

**Consequences:** OKF advisory rules (`OKF-INDEX-STRUCTURE`, `OKF-LOG-STRUCTURE`,
`OKF-CITATIONS-{PRESENT,WELL-FORMED,RESOLVE}`, `OKF-AUGMENTATION-GUARD`) live in
`crates/hyalo-cli/src/commands/okf_lint.rs`, run only when the profile is active
(gated by a runtime flag, not the mdlint engine), and carry `default_enabled = true` in
the catalog so `lint-rules set OKF-* --enabled false` writes a real override. Per the OKF
permissive-consumption model every OKF rule is **warn** — SPEC §9 errors come only from the
schema pass (missing frontmatter / empty-or-missing `type`); broken links, reserved-file
structure, and citation issues never reject.

## DEC-050: `hyalo lint --format github` for PR Annotations, as a Third Output Format (2026-07-17)

**Decision:** Add `github` to the `--format` value set as a **lint-only** output mode that
emits one GitHub Actions workflow command per violation
(`::error file=<path>,line=<line>,title=<RULE_ID>::<message>`, warnings → `::warning`),
followed by a one-line `N errors, M warnings in K files` summary. Every other subcommand
rejects `--format github` with a clear message listing the valid formats. Annotation paths
are emitted **relative to the repository root** — vault-relative paths are prefixed with the
vault dir's path relative to CWD — so CI must run from the repo root.

**Why:**
- **No polyglot glue.** Native workflow-command output means findings render as inline PR
  annotations without a `jq` transform, which the no-polyglot-tooling rule forbids anyway.
- **Reuses the existing lint payload.** The renderer walks the same
  `files[].rule_groups[].violations[]` shape the text/json formatters consume; only the
  presentation differs. `--strict`, `--rule`/`--rule-prefix`, `--limit`, and `[lint] ignore`
  compose unchanged; exit codes are unchanged.
- **Lint-only keeps the contract honest.** Workflow commands only make sense for
  file/line/message findings. Rejecting `github` elsewhere avoids meaningless output and a
  fake-general format. It is deliberately **not** accepted as a `.hyalo.toml` `format` value.

**Consequences:** Rendering lives in `crates/hyalo-cli/src/commands/lint_github.rs` (escaping
per the workflow-command spec: `%`→`%25`, `\r`→`%0D`, `\n`→`%0A`; properties also `:`→`%3A`,
`,`→`%2C`). `--format github` forces `detailed` and lifts the per-rule/per-file caps in
dispatch so no annotation is silently dropped, and is rejected together with `--count`/`--jq`.
The repo dogfoods this via a `lint-kb` CI job. Frozen historical trees
(`iterations/done/**`, `backlog/done/**`, `dogfood-results/**`, `reviews/**`, `research/**`,
`promotion-plan.md`) are added to `[lint] ignore`, and `HYALO002` is downgraded to **warn**
in this vault because completed iterations legitimately keep a trailing unchecked
housekeeping task — so the gate protects the live knowledgebase without churning history.

## DEC-051: `setup-hyalo` lives in a separate repo with a floating `@v1` tag (2026-07-17)

**Decision:** Ship the install-hyalo GitHub Action as its own repository
`ractive/setup-hyalo` (composite bash action), **not** as a folder inside the
hyalo repo, and give it an independent version line: a full `vMAJOR.MINOR.PATCH`
tag plus a moving `vMAJOR` tag that consumers reference as `ractive/setup-hyalo@v1`.

**Why:** This is the `dtolnay/rust-toolchain` pattern. A separate repo decouples
action versioning from binary versioning (the hyalo binary can release without
retagging the action, and vice versa), keeps the action's marketplace/`@v1`
surface clean, and lets the action be pinned by SHA independently of the
`version:` input that pins the binary. The action stays pure bash + `curl` (no
Node/Python — consistent with the no-polyglot rule); it resolves the runner
platform to a release target, downloads + caches the prebuilt archive, and adds
the binary to `PATH`. hyalo ships **no** `x86_64-apple-darwin` build, so the
action fails fast with a clear message on Intel macOS runners (use `macos-14`+ or
`cargo install`).

**Retag protocol:** cut `vX.Y.Z`, then `git tag -f v1 && git push -f origin v1`;
only move the floating major for backwards-compatible changes, bump to `v2` on a
breaking change. When hyalo cuts a release, run the action's `smoke` workflow
(`workflow_dispatch`, `version:` = new tag) to confirm the new archives install
on all three OSes — automating this into the hyalo release pipeline is deferred
(follow the `ractive/release-workflows` change protocol).

**Blocked (2026-07-17):** the automated iteration run could not `gh repo create`
the public `ractive/setup-hyalo` repo — creating a new public repository requires
human authorization in the web UI and was denied by the environment's safety
classifier. The full, platform-verified action tree (`action.yml`, matrix `smoke`
workflow, fixture vault, MIT `LICENSE`, README) is built and its install logic was
validated end-to-end on macOS bash 3.2 (latest + pinned + warm-cache + input
validation). It awaits a human to create the repo and push the tree, after which
hyalo's own `lint-kb` CI job switches from build-from-source to
`uses: ractive/setup-hyalo@v1` (deliberately **not** switched now — pointing live
CI at a not-yet-published action would break every PR check).

## DEC-052: Fix-wave design decisions for profile composition & generators (2026-07-17)

**Context:** the 7-agent pre-v0.18.0 dogfood
([[dogfood-results/dogfood-v0180-okf-profiles-pre-release]]) found five
release blockers. Four design decisions were taken with the user to shape the
fix wave (iterations 172–175):

1. **Smart merge, not layered fragments.** Profile composition is fixed
   inside the materialized `.hyalo.toml`: array keys union, `[lint]`
   gains a `profiles` list, scalar overwrites print `conflict:` lines, and
   comments/order are preserved (`toml_edit`). The cleaner layered-fragments
   model (config names active profiles, fragments composed at load) is
   deliberately deferred as a possible future major-version redesign — it
   solves refresh/uninstall exactly but changes the config model.
2. **`okf index --apply` auto-adopts marker-less files, preserving all
   content.** The managed region is appended to the existing body; dry-run
   announces adoption; destructive overwrite requires an explicit
   `--replace`.
3. **Dot-directory reach is a general walker include-list**
   (`[scan] include` globs, `.git` hard-excluded), shipped by the skills
   profile as `.claude/skills/**` — not a hard-coded special case.
4. **Full 4-iteration cut before release** (blockers + mediums), with the
   feature-gap items (config-editing commands, `set` for string-lists,
   `okf log` style matching, body-section append) deferred to a separate
   design pass.

Supporting calls baked into the plans: `[[schema.bind]]` satisfies the
`type` requirement (bind = typing); root changelogs are addressed via
`[changelog] path` resolved from the config dir; the OKF profile ships
vendor-neutral (no BigQuery example types); case handling for reserved files
reuses the `[links] case_insensitive` auto-detection approach.

## DEC-053: OKF lint rules do not honor `[okf] ignore` globs (2026-07-18)

**Decision:** The `okf` conformance lint rules (`OKF-*`) do **not** exempt files
matching an `[okf] ignore` generation glob (e.g. `_template/**`). This is
deferred, not planned for iter-176.

**Why:** `[okf] ignore` is a *generation* filter consumed only by `okf index`;
it is not threaded into the lint pipeline (`lint_files_extended` → per-file
loop). Wiring the ignore globset + vault-relative path through the whole lint
machinery is a cross-cutting change disproportionate to the iter-176
data-safety scope. The OKF advisory rules are warn-only (never fail CI), so a
`_template/**` file being both generation-excluded and lint-flagged is cosmetic,
not a gate. Users who want template files fully silent can add them to
`[lint] ignore` / `[schema] exempt`. Tracked for a future lint-scoping
iteration. See [[iterations/iteration-176-okf-generator-hardening]].

## DEC-054: No lint rule for extra frontmatter on reserved OKF files (2026-07-18)

**Decision:** hyalo does **not** add an `OKF-*` lint rule that flags extra
frontmatter keys on the bundle-root `index.md` (SPEC allows a lone `okf_version`
key "and nothing else") or *any* frontmatter on nested reserved `index.md` /
`log.md` files. The generator stays permissive: `okf index` preserves the
bundle-root `okf_version` key and never *adds* frontmatter to reserved files,
but it does not reject a reserved file an author hand-decorated with extra keys.
The README and the bundled `okf` skill describe these as SPEC requirements
("MAY carry … and nothing else", "frontmatter-free by design") rather than as
hyalo-enforced guarantees, so their wording already matches the permissive
implementation — no doc change was needed beyond confirming this.

**Why:** Reserved files are already `[schema] exempt`, so they are outside the
schema/undeclared-property machinery a new rule would have to re-plumb. OKF
advisory rules are warn-only (never fail CI), so the incremental value of
flagging a decorative extra key on a reserved file is low, while the cost —
threading a reserved-file frontmatter check into the lint pipeline — is a
cross-cutting change out of scope for a docs-truth iteration. Authors who want
strictness can hand-declare a schema binding for those paths. Revisit if a real
OKF consumer starts rejecting bundles over reserved-file frontmatter drift. See
[[iterations/iteration-177-okf-docs-truth]].

## DEC-055: Backslash escaping of links follows CommonMark odd-backslash rule (2026-07-18)

**Decision:** A link opener that is preceded by an **odd** number of backslashes
is treated as literal text and is not extracted (L-16). `\[[x]]` → literal;
`\\[[x]]` → a real link (the `\\` renders as one literal backslash, leaving the
`[` unescaped); `\\\[[x]]` → literal again. The escape is evaluated at the
*opener byte* the parser is about to consume: for a markdown link and a plain
`[[wikilink]]` that is the `[`; for an embed `![[…]]` the `!` and the `[[` are
independent — `\![[x]]` escapes only the `!` and still yields a normal
(non-embed) `[[x]]` wikilink, whereas `!\[[x]]` escapes the `[[` and suppresses
the whole embed. Implemented as `links::is_escaped(bytes, pos)` counting
preceding `\` bytes and applied in both `extract_links_from_text_with_original`
and `extract_link_spans_with_original`, so extraction and rewriting share one
rule and rewriters never touch an escaped link.

**Why:** Matches CommonMark's backslash-escape semantics (odd count escapes,
even count is a literal-backslash run) and Obsidian's behavior for `\[[…]]`.
Doing it at the shared span extractor means every consumer — `find
--broken-links`, `mv`, `links fix`, `auto`, and any future lint rule — inherits
the same escape handling for free, with no per-command special-casing. See
[[iterations/iteration-185-link-semantics]].

## DEC-056: Batch `mv` reports (does not roll back) completed link rewrites on mid-batch write failure (2026-07-19)

**Decision:** When batch `mv --apply` fails partway through writing inbound
link-rewrite plans, the completed `atomic_write`s are **kept and reported**, not
rolled back. The physical file *renames* are still rolled back (they are
cheaply reversible), but link-rewrite content changes are left on disk and the
error message names exactly which files were durably rewritten before the abort
plus which plan failed and why. Implemented by routing the batch through the new
`link_rewrite::execute_plans_partial` (L-11) instead of the all-or-nothing
`execute_plans`; `execute_batch_mv` inspects the `PartialExecuteReport` and, on
any failure, rolls back renames and returns an error enumerating the durably
rewritten files and the failures.

**Why:** A faithful content rollback would require capturing per-file pre-images
of every rewritten file before the batch and restoring them on failure — extra
memory, extra I/O, and its own partial-failure surface (a rollback write can
itself fail). The renames are trivially reversible (`fs::rename` back), so those
are undone to keep the directory layout consistent; the content writes are made
*honest* instead of *atomic*. This matches the L-11 principle applied to `links
fix`/`links auto`: never silently keep a write the caller can't see. Callers who
need all-or-nothing semantics still have `execute_plans`. Revisit if a user
reports a half-rewritten vault after a batch mv failure is materially harder to
recover than a clear report of which files changed. See
[[iterations/iteration-187-link-writer-unification]].

**Amendment (2026-07-19, PR #221 review):** The "keep and report" rationale
above only holds when a kept plan's `path` is untouched by the rename set —
i.e. a genuinely external linker file. It does not hold for "self-rewrite"
plans, whose `path` **is** one of the batch's own rename destinations (the
moved file's own inbound and/or outbound link rewrites, built by
`plan_batch_mv`). For those, content and location are coupled: rolling back
the rename while keeping the rewritten content strands the file at its old
path with content written for the new (post-rename) layout — a dangling link
that is strictly worse than doing nothing, and the original error message
compounded this by claiming such a file was "durably rewritten... and NOT
rolled back" while it was in fact physically back at its old path.

`RewritePlan` now carries an `original_content: Option<String>` field,
populated only for self-rewrite plans (`plan.path` equals a rename
destination). `execute_batch_mv` builds a map of rename destination → source
path from the batch's own `renames` list; on a mid-batch failure, after
`rollback_renames`, every successfully-applied plan whose `path` is a key in
that map has `original_content` written back (via `atomic_write`) to the
file's now-restored old path — undoing both the rename and the content
change together. Plans on files outside the rename set are still kept and
reported per the original decision. The error message now reports three
distinct buckets: failed writes, self-rewrites restored (rolled back with
their rename), and external files kept. See
[[iterations/iteration-187-link-writer-unification]] and
`.claude/agent-memory/rust-developer/pitfall_batch_mv_rename_rollback_dangling_link.md`
for the confirmed repro that motivated this amendment.

## DEC-057: Percent-decoding scope and malformed-escape policy for link resolution (2026-07-19)

**Decision:** `discovery::resolve_target` and the link graph
(`insert_file_links`) percent-decode the **path portion** of a link target
after the existing fragment/query strip, so `[x](my%20dest.md)` resolves to the
on-disk file `my dest.md`. Decoding is applied uniformly (resolve_target is
kind-agnostic; in practice only markdown destinations carry `%`-escapes —
wikilinks never do). A malformed escape (`%` not followed by two hex digits, e.g.
`%2`, `%zz`, or a stray `%` in `100%done.md`) or an escape sequence that decodes
to non-UTF-8 bytes (`%FF`) **preserves the literal input** — the decoder returns
`None` ("nothing safely decodable") rather than corrupt the path. Encoding is
kept as-written on rewrite (an `mv` of `my dest.md` preserves the `%20` form),
parity with the angle-bracket handling from PR #220.

**Why:** A tiny hand-rolled decoder (no new dependency, all-Rust per project
policy) covers the real case — CommonMark/Obsidian-emitted `%20` spaced
destinations — without pulling `percent-encoding`. Preserving the literal on any
malformed/non-UTF-8 escape means a filename that genuinely contains a `%` still
resolves as written, so decoding can never introduce a *new* broken link. See
[[iterations/iteration-188-link-semantics-completion]].

## DEC-058: HYALO006 broken-link rule — CLI-side, warn-by-default, error-under-strict; anchor validation deferred (2026-07-19)

**Decision:** The broken-link lint rule is **HYALO006** (`HYALO004`/`HYALO005`
are taken — datetime-format / frontmatter-parse-error). Its catalog entry
(severity/default-on/description) lives in `hyalo-mdlint`, but the resolution
logic lives **CLI-side** in `commands/link_lint.rs` because it needs vault-wide
context (the set of files that exist), which the stateless mdlint engine does
not have. The rule is **enabled + `warn` by default** and promoted to **error
under `--strict`** (mirroring the strict-promotion pattern of the other HYALO
rules), unless the user pins an explicit `[lint.rules.HYALO006] severity`. The
vault resolution context (`LinkLintContext`: canonical dir + site_prefix +
case/stem index) is built **once per invocation** in the lint dispatch arm —
from the `--index` snapshot when active, else a single vault walk — and shared
by reference across the rayon workers, so the rule never rebuilds the graph per
file. Broken **anchors** are NOT included in HYALO006 this iteration: anchor
validation (L-21) is deferred because it requires the `Link` index wire-shape
bump (a new `fragment` field) plus an anchor-heading matcher, which must land
together with an index-rebuild note rather than as a half-done shape change.

**Why:** Keeping the catalog entry in mdlint (so `lint-rules list/show`,
`--rule`/`--rule-prefix`, and `[lint.rules.HYALO006]` overrides all work
uniformly) while putting the vault-aware logic in the CLI matches the existing
HYALO005 split and avoids giving `hyalo-mdlint` a link-graph dependency.
Warn-by-default keeps a broken link from breaking every existing green vault on
upgrade, while `--strict` (or an explicit `severity = "error"`) lets CI gate on
it deliberately. Building the context once is essential: a per-file graph
rebuild would make lint O(files²). See
[[iterations/iteration-188-link-semantics-completion]].

## DEC-059: `.md` normalization stays split between construction and resolution, not a new as-written Link field (2026-07-19)

**Decision:** For L-19, `.md`-suffix handling is centralized in the two places
that already own it rather than by adding an as-written field to the serialized
`Link` type. Wikilink targets are normalized to the extension-less canonical
form at construction (`strip_wikilink_md_suffix` in `parse_wikilink`); markdown
targets keep their `.md` (required by the syntax) and the single `.md`-toggle in
`resolve_target` reconciles both kinds at lookup time. The originally-proposed
extra `Link` field (preserving the exact user-typed suffix) is **not** added:
the rewrite side already reconstructs the written form via `WrittenForm` /
`LinkWriter`, so a second as-written field would be redundant and would force the
`Link` index wire-shape bump (with old-snapshot fallback handling) for no
observable benefit.

**Why:** Avoiding the `Link` shape change keeps `.hyalo-index` snapshots
forward-compatible and sidesteps the whole-codebase update of every `Link {…}`
literal. The two existing normalization points (`strip_wikilink_md_suffix` at
construction, the `.md` toggle in `resolve_target`) already give a single,
audited canonical comparison across link kinds — which is the actual L-19 goal.
The anchor `fragment` field that L-21 would need is the only thing that truly
requires the shape bump, so it is deferred as one unit (see DEC-058). See
[[iterations/iteration-188-link-semantics-completion]].

## DEC-060: Anchor-match convention, fragment percent-decoding, and the backward-compatible `Link.fragment` shape (2026-07-19)

**Decision (anchor match — L-21):** A link `#fragment` matches a target
heading iff the **trimmed** heading text equals the **percent-decoded, trimmed**
fragment under an **ASCII case-insensitive** comparison. This mirrors Obsidian,
which resolves `[[Foo#tasks]]` against a `## Tasks` heading. Markdown fragments
may be percent-encoded (`foo.md#my%20heading`); the encoded form is preserved in
the written link (the rewrite span never covers the fragment) and decoded only
for matching. `^block-id` fragments are **skipped** — never reported broken —
because hyalo does not index block ids. Sections with `heading: None`
(pre-heading outline entries) never match a non-empty fragment; an empty or
whitespace-only fragment (`[[note#]]`) is treated as no anchor. The matcher
lives in `hyalo-core/src/anchor.rs`, deliberately **separate** from
`heading::SectionFilter` (the `--section` substring selector) — validation needs
exact existence, not substring selection.

**Decision (wire shape — deviation from the plan's premise):** `Link` gains an
additive `fragment: Option<String>` with `#[serde(default,
skip_serializing_if = "Option::is_none")]`. The iteration plan assumed this
would be a *hard* schema break (old `.hyalo-index` snapshots falling to the
disk-scan `Err` arm). **Empirically it is not:** the index serializes with
`rmp_serde::to_vec_named` (map framing, not array framing), so an old snapshot
decodes cleanly into the new `Link` with `fragment: None` — verified with a
probe against both named and array encodings. This is **still fail-safe**: stale
entries carry no fragment, so no false broken-anchor reports; anchor data is
picked up after a `hyalo create-index` rebuild. We therefore ship the
backward-compatible field (matching the precedent set by `IndexEntry.bm25_tokens`
in the same file) rather than deliberately engineering a hard break, which would
needlessly invalidate every user's index on upgrade for a purely additive field.

**Why:** Case-insensitive exact match is the least-surprising, Obsidian-aligned
convention; percent-decoding only for comparison keeps the written link
byte-stable through `mv` / `links fix`. Graceful snapshot degradation is
strictly better than a forced disk-scan fallback here — same safety, no
upgrade-day index churn. See
[[iterations/iteration-190-link-anchors]].

## DEC-061: `HYALO006` stays target-only; anchors surface in `find --broken-links` for one release first (2026-07-19)

**Decision:** Broken heading anchors are **not** added to the `HYALO006`
broken-link lint rule this iteration. `HYALO006` continues to flag broken
*targets* only. Broken *anchors* surface exclusively through `find
--broken-links` (as a distinct `broken_anchor` category). Whether to fold
anchors into HYALO006 — as a sub-severity of the same rule or as its own new
rule id — is an explicit follow-up decision, deferred so anchor semantics can
soak one release behind `find` before any `lint`/CI gate consumes them.

**Why:** Mirrors DEC-058's warn-first caution for HYALO006 itself: let a new
link-semantics feature prove itself in an opt-in query surface before it can
fail a CI gate. The HYALO006 rule description and the README lint section state
that anchors are not checked by the rule. See
[[iterations/iteration-190-link-anchors]].

## DEC-062: `atomic_write` follows intra-vault symlinks and writes the target (2026-08-06)

**Decision:** When the destination of a mutating write is a symlink,
`fs_util::atomic_write` **follows** the link and replaces the *target* file,
leaving the symlink itself intact. It does not refuse, and it no longer
replaces the link with a regular file. Resolution is a bounded `read_link`
loop (max 32 hops) rather than `fs::canonicalize`, because the destination of
a write may legitimately not exist yet — `canonicalize` fails on a
not-yet-created file, and it would also normalize away a dangling final
component. The temporary file is created in the *resolved* target's parent
directory so the `persist` rename stays on the same filesystem.

A companion `fs_util::atomic_write_within(vault_root, …)` re-applies the vault
boundary check to the resolved destination and is used by every CLI mutation
site that has the vault dir in scope (`lint --fix`, `okf`, `changelog`,
`links auto`/`mv` link rewrites, managed regions). A symlink whose target
escapes the vault is refused, so "follow" never becomes an escape hatch. The
boundary check runs only when resolution actually changed the path, so the
common non-symlink write pays no extra syscalls. The `Option`-free
`atomic_write` remains for core-internal callers (`tasks`, frontmatter write)
whose paths have already passed `discovery::resolve_file`'s canonicalizing
boundary check.

**Why:** *Follow*, not *refuse*. Vaults legitimately alias notes through
symlinks and Obsidian follows them; refusing would break those vaults for
every mutating command at once. Before this change every mutation path —
`set`, `remove`, `append`, `task`, `lint --fix`, `mv`, `okf`, `changelog` —
silently replaced the symlink with a regular file holding the new content,
which is *silent data loss*: the aliasing relationship disappeared and the
real target kept the stale content. See
[[iterations/iteration-191-write-path-integrity]].

## DEC-063: `atomic_write` fsyncs the temp file and the parent directory (2026-08-06)

**Decision:** `atomic_write` calls `sync_all()` on the temp file before
`persist`, and on Unix opens the destination's parent directory and `sync_all`s
it after the rename. The parent-directory fsync is **best-effort**: an error is
ignored rather than propagated, because some filesystems reject `fsync` on a
directory handle and failing the whole write there would be a regression.

**Why:** Without the pre-`persist` fsync the "atomic" guarantee in the doc
comment was false. A crash between rename and writeback could leave the new
directory entry pointing at a file whose data blocks were never flushed —
i.e. a zero-length or partially-written note *replacing* good content. Unlike
the snapshot index (`index.rs`), which detects a torn read via
`rmp_serde::from_slice` and falls back to a disk scan, user markdown has no
recovery path. The snapshot index therefore deliberately keeps its unsynced
write. See [[iterations/iteration-191-write-path-integrity]].

## DEC-064: `mv`'s dual default is kept, and made loud (2026-08-17)

**Decision:** `hyalo mv` keeps its asymmetric default — single-file mode writes
immediately, batch mode (`--glob`/`--property`/`--tag`/`--type`) defaults to
dry-run and requires `--apply` — but single-file mode now **rejects** `--apply`
with `single-file mv applies by default; use --dry-run to preview` instead of
accepting it as a silent no-op.

**Why:** The two defaults are each right for their mode. A single-file `mv` is
the same gesture as `mv(1)` and blocking it behind a flag would be gratuitous
ceremony; a batch `mv` can rewrite hundreds of files from one glob, where the
cost of an unintended run is high enough that preview-by-default earns its
keystroke. Collapsing them either way makes one of the two modes worse.

What could not stay was the *silence*. `--apply` in single-file mode did
nothing and reported nothing, so a user who learned the batch form first had no
way to tell whether the flag was doing the work or the default was — and would
carry that wrong model into the batch case, where it matters. An explicit
rejection converts an invisible asymmetry into a one-line lesson at the moment
the wrong mental model shows up. See
[[iterations/iteration-192-cli-surface-truth]].

## DEC-065: `links fix` and `links auto` keep their names; no alias (2026-08-17)

**Decision:** The `hyalo links` subcommand pair keeps the verb/adjective mix —
`links fix` (repair broken links) and `links auto` (auto-link unlinked
mentions). No `visible_alias` is added in either direction. The inconsistency
is documented here and nowhere else, because both `--help` texts already state
what each subcommand does in its first line.

**Why:** The Opus 5 review flagged the pair as grammatically inconsistent: one
name is an imperative verb, the other an adjective. That is true, and it is also
the smaller problem. An alias (`links autolink`, or `links fix` gaining
`links repair`) does not remove the inconsistency — it adds a second spelling
that must be kept in `--help`, the COMMAND REFERENCE, the feature matrix and
the completion scripts, while leaving the original name in place forever. Both
names are already short, unambiguous within `links`, and load-bearing in
existing scripts and skill files. Renaming would break them for a purely
cosmetic gain, so the choice is between "inconsistent" and "inconsistent plus
redundant". See [[iterations/iteration-195-review-drain-cleanups]].

## DEC-066: the `resolve_target` bare-stem guard tests only `/` (2026-08-17)

**Decision:** The bare-stem resolution guard in
`crates/hyalo-core/src/discovery.rs` stays `!target.contains('/')` and is NOT
extended to also test `'\\'`, unlike its three sibling separator checks in the
same file. A comment at the guard records why, and
`resolve_target_backslash_targets_are_normalized_before_stem_resolution` pins
the behaviour.

**Why:** The review reported this as a Windows-only correctness gap: a target
like `note.md\` would, on Windows, be seen by `Path` as having an `.md`
extension, get truncated to the mangled stem `note.`, and resolve to the wrong
file. Verification shows the scenario is unreachable. `resolve_target` begins
with an unconditional `target.replace('\\', "/")`, and everything between that
and the guard only truncates or trims the string — so `target` provably cannot
contain a backslash by the time the guard runs. `note.md\` has already become
`note.md` and resolves as the ordinary bare name. The three sibling guards do
need both separators because they inspect *raw* link targets that never pass
through that normalization. Adding `&& !target.contains('\\')` would have been
a permanently-false condition that reads like a real defence, so the finding is
dispositioned as won't-fix-with-evidence rather than landed as code. See
[[iterations/iteration-195-review-drain-cleanups]].

## DEC-067: `links fix --ignore-target` keeps its name; only `links auto` gains persistence (2026-08-18)

**Decision:** `links fix --ignore-target <substring>` is **not** given an
`--exclude-target-glob`-style alias and is **not** added to `[links.auto]`. It
stays a per-invocation flag with its current name, documented as-is in the
repository's `docs/configuration.md`. The `[links.auto]` section added by
[[iterations/iteration-195a-auto-link-config-exclusions]] persists exactly three
keys — `exclude_titles`, `exclude_target_globs`, `first_only` — all of them
`links auto` flags.

**Why:** The naming similarity is superficial. `--ignore-target` matches a
*substring of the link target as written* (`--ignore-target draft` drops
`[[drafts/x]]` and `[[x-draft]]` alike); `--exclude-target-glob` matches a
*vault-relative path glob against candidate target pages*. They take different
pattern languages and operate on different inputs, so an alias would promise
interchangeability that does not exist, and a shared config key would be worse
still. Adding `ignore_target` to a `[links.fix]` table is also not justified by
evidence: the external-user report behind this iteration was entirely about
`links auto` noise on a title-heavy vault, with no equivalent complaint about
`links fix`. Non-breaking was a hard requirement, and the cheapest non-breaking
option that adds no permanent second spelling is documentation — the
configuration reference now states plainly that the two filters are different
things and why `--ignore-target` is absent from `[links.auto]`.

**Also considered and deferred:** a `--no-first-only` counter-flag, so a vault
with `first_only = true` could get an all-mentions run without editing
`.hyalo.toml`. Out of scope here (the iteration's non-goals fix the surface at
three keys and no new flags), and the workaround — `hyalo links auto` on a
narrower `--file`/`--glob` scope, or a temporary config edit — is adequate.
File a backlog item if a real user hits it.

## DEC-068: `links auto --no-first-only` ships as a conflicting counter-flag (2026-08-18)

**Decision:** `hyalo links auto` gains `--no-first-only`. It forces
first-mention-only **off** for a single run, overriding
`[links.auto] first_only = true`, and clap rejects it alongside `--first-only`
(`conflicts_with`). `AutoFilters::effective_first_only` becomes
`if cli_no_first_only { false } else { cli_first_only || config_first_only }`.
No other `[links.auto]` key gets a counter-flag.

**Why:** [[iterations/iteration-195a-auto-link-config-exclusions]] made
`first_only` persistable, and [[decision-log#DEC-067]] deferred the counter-flag
"pending a real user hitting it". The re-evaluation this iteration opened with
found no external report — but it did find an internal inconsistency that is
evidence enough on its own: `warn_common_titles`, the other boolean key in the
section, already has `--no-warn-common-titles`. `first_only` was the only
`[links.auto]` setting a run could not opt out of, and the documented
workarounds are both bad — narrowing `--file`/`--glob` changes *what is
scanned*, not *how it is linked* (a different result, not the same result on a
smaller set), and editing `.hyalo.toml` for one run is a mutation of shared
vault state that a killed process leaves behind. The implementation is ~10
lines of merge logic on an existing flag surface, so the cost side of "defer
pending demand" was near zero.

**Why conflict rather than precedence:** `--first-only --no-first-only` on one
command line has no defensible meaning. Silently letting one win makes a typo
in a scripted invocation produce a quietly different vault; a clap error makes
it a build failure. `effective_first_only` still tie-breaks to *off* if the
pair somehow reaches it, so no non-CLI caller can get a surprise first-only
run.

**Not done:** counter-flags for `exclude_titles` / `exclude_target_globs`. They
are unioned lists, so "ignore the config's list for this run" is a different
and larger question (partial vs. total override) with no demand behind it.

## DEC-069: `--dir` selects a vault, not a config (2026-08-22)

**Decision:** `--dir <path>` names the *vault* to operate on and never decides,
on its own, which `.hyalo.toml` applies. Resolution is now a single function,
`config::resolve_effective`, used by every command including `hyalo config`:

1. **No `--dir`** — the CWD's `.hyalo.toml` applies; the vault is its `dir`.
2. **`--dir` canonicalizing to the vault that config already resolves to** — the
   CWD config *still applies*. The existing `note: --dir is redundant` stays,
   because the flag genuinely adds nothing.
3. **`--dir` naming a different directory** — that directory's own
   `.hyalo.toml` applies if present, else built-in defaults. Either way hyalo
   writes a `note:` on stderr naming the file that took over, because this is
   the case where the user's config really does stop applying.

`hyalo config --dir X` reports exactly this, so `config_path` is never `null`
while a config was read.

**Why:** [[dogfood-results/dogfood-v0210-pre-release-iters-191-198]] H-4. Case 2
is the overwhelmingly common layout — config at the repo root with
`dir = "hyalo-knowledgebase"` — and hyalo's behaviour there was to reload
`.hyalo.toml` from *inside* the vault, find nothing, and run on built-in
defaults: no schema, no views, no `[lint] ignore`, no severity overrides, no
`site_prefix`, no changelog path. Measured on this repo, `lint --dir
hyalo-knowledgebase --strict` reported 125 files/694 warnings against the
config-honoring run's 4 warnings — a CI gate written with the flag was
inspecting a different rule set than the one the project maintains, and hyalo's
own hint output emitted `--dir`-bearing commands that walked users into it. The
"redundant" note made it worse by asserting the two forms were equivalent.

**Why not "always load the target dir's config":** that is what the code did,
and it is only coherent if `--dir` is understood as "switch projects". But
`--dir` is documented as "root directory for resolving all file and --glob
paths" and is what `hyalo config`'s own hints emit — it reads as a path
argument, not a project switch. Making case 2 a no-op restores the property the
flag's name implies.

**Why not walk ancestors for a config in case 3:** hyalo has never merged or
inherited configs, and an ancestor walk would make `--dir /tmp/scratch` silently
pick up whatever config sits above `/tmp`. Case 3 keeps the existing rule (own
file or defaults) and only adds the stderr note.

**Consequence:** breaking for anyone who used `--dir <configured-vault>` as a
way to *ignore* the local config. Called out in the changelog with the
migration (point `--dir` outside the configured vault instead).

## DEC-070: unusable `.hyalo.toml` is fatal for writers, advisory for readers (2026-08-22)

**Decision:** A `.hyalo.toml` that exists but does not parse sets
`ResolvedDefaults::malformed`. Commands that would actually write — per
`Commands::writes()`, which excludes `--dry-run` and preview-only forms — exit 1
with the parse diagnostic and touch nothing. Read-only commands continue on
built-in defaults, except that `dir` is salvaged from the file when a lenient
re-read can still find it. The diagnostic is emitted through
`warn::warn_always`, which `--quiet` cannot suppress. `init`/`deinit` are never
blocked.

**Why:** dogfood M-2. One unknown key anywhere in the file discarded everything
including `dir`, so `hyalo links auto --apply -q` could rewrite a tree the
config never pointed at, with no output at all. The asymmetry is deliberate: a
read on the wrong defaults produces a confusing answer the user can discard; a
write produces edits they have to find and undo. Salvaging `dir` narrows even
the read case to "wrong rules" rather than "wrong tree".

**Why `-q` does not apply:** `--quiet` exists to suppress per-file chatter in
scripts. A config that stopped applying is not chatter — it changes which vault
and which rules the command used, which is precisely the thing a script author
must not miss.

## DEC-071: hints carry a `writes` flag, derived not declared (2026-08-22)

**Decision:** `Hint` gains a `writes: bool`, computed inside `Hint::new` from
the command string by `mutation::command_line_writes` rather than passed in by
each hint builder. Text output renders writing hints with a `=>` arrow and a
trailing `[writes]` tag; the JSON envelope always includes `"writes"`. An e2e
gate harvests every hint the CLI emits, runs the ones marked read-only, and
fails on any byte-level change to the vault or `.hyalo.toml`.

**Why:** dogfood M-7. `hyalo find` with two or more filters suggests
`hyalo views set …`, which rewrites `.hyalo.toml`, in the same `-> hyalo …`
list as read-only drill-downs. "Follow the hints" is standing instruction in
this repo's `CLAUDE.md`, so an unmarked mutation is a trap for exactly the
audience hints are written for.

**Why derived rather than declared:** a `Hint::writing(…)` constructor is one
that a future hint site can forget to use, and the failure mode is silent. A
classifier in `Hint::new` cannot be bypassed. Its risk — misclassification —
is covered two ways: `mutation::tests` runs a command corpus through both the
string classifier and the typed `Commands::writes()` and asserts they agree, and
the e2e gate independently proves the read-only marker by executing the hints.

## DEC-072: `links` text puts the fuzzy listing last, and caps every bucket at 20 (2026-08-23)

**Decision:** The `links fix` text report is ordered: counts (now including a
`Fuzzy matches` line), then the fixes that would be — or were — written, then
the actionable buckets (unfixable, out-of-vault, case mismatches, ambiguous,
templated), and *finally* the fuzzy proposals. Each bucket listing is capped at
20 entries with an `… and N more (use --format json for the full list)` footer.
JSON is never capped.

**Why:** dogfood UX-3/UX-4 against GitHub Docs. Fuzzy is the largest listing by
an order of magnitude — thousands of low-confidence guesses that plain `--apply`
will not even write — and it sat directly above the buckets that actually need a
human. The two lists have opposite value density: unfixable and out-of-vault are
short and require judgement, fuzzy is long and is skimmed at most. Ordering by
"how likely is the reader to act on this" puts the short lists where a terminal
still shows them.

**Why a cap rather than a `--verbose` split:** the counts are what make the
report honest, and they are never capped. Capping only the *listings* keeps a
single default output that both reconciles arithmetically and fits on a screen,
without adding a flag whose absence produces a misleading report. A script that
wants everything already has `--format json`, which this iteration also made
worth using (per-fix detail is pinned by e2e).

## DEC-073: `links auto` reports `col` in Unicode scalars, decided by release status (2026-08-23)

**Decision:** `links auto` JSON `col` counts Unicode scalar values, 1-based —
the same convention as `lint`'s `column`. The byte offset the rewriter needs is
retained as a non-serialized `byte_col` field on `AutoLinkMatch`.

**Why now rather than "document the bytes":** iteration 210's plan made this
conditional on release status — switch if the iteration lands before v0.21.0 is
cut, document byte semantics if after. The workspace was still at 0.20.0 with
v0.21.0 on hold for this integrity wave, so the switch rides along with
iter-204's already-documented breaking change instead of costing a second one.
A byte column that is silently *called* a column is the kind of output-truth
defect this whole iteration exists to remove; documenting it would have locked
the wrong semantics in for a release cycle.

## DEC-074: repeated `--index-file` hint paths are shortened, never elided (2026-08-23)

**Decision:** A snapshot-index path in a hint renders in its
working-directory-relative form when that is shorter than the absolute path, and
absolute otherwise. The flag is *not* factored out of the repeated hints into a
one-line preamble.

**Why:** dogfood UX-5 asked for de-duplication of a path repeated four or five
times in one hint block. Every derived `find` hint must carry `--index-file` or
it silently rescans the vault and answers a different question, so eliding it
from the repeats would produce hints that run but lie — the exact failure class
iteration 210 is closing, and a direct violation of its own acceptance criterion
that every emitted hint execute correctly verbatim. Shortening removes most of
the bulk (an index almost always lives inside the project it indexes) while
keeping each line independently copy-pasteable.

## DEC-075: anchors match the rendered GitHub slug as well as the raw heading text (2026-08-23)

**Decision:** A link fragment resolves against a target file's headings when
*either* DEC-060's raw-text rule holds (trimmed, percent-decoded,
ASCII-case-insensitive equality) *or* the GitHub-style slug of the fragment
equals the GitHub slug of a heading. Slugs are lowercased Unicode-aware, keep
alphanumerics plus `-` and `_`, drop all other punctuation, map each whitespace
character to `-`, and carry the renderer's `-1`, `-2`, … duplicate suffixes in
document order. Both sides are slugified, which makes the comparison idempotent.

**Why:** DEC-060 alone accepted only the spelling *no renderer ever emits*.
Every mainstream markdown renderer turns `### Sub Section` into `#sub-section`,
so `[c](t.md#sub-section)` — the form authors actually write — was reported
broken while `#Sub Section` passed. Measured on the GitHub Docs corpus
(`~/devel/docs/content`, 3,710 files): **947 of 2,071 checkable anchors matched
only through the slug rule**, i.e. were false positives before this change. The
remaining 1,048 are genuinely absent from the source markdown (generated REST
reference anchors and Liquid-templated headings), and are still caught — the
check did not become a rubber stamp.

**Why the union rather than a replacement:** Obsidian resolves `[[Foo#tasks]]`
against `## Tasks`, and hyalo's own knowledgebase is an Obsidian vault. Keeping
both conventions costs one extra comparison and serves both audiences; the check
exists to catch dead anchors, and a false positive costs a user far more than a
missed exotic spelling.

**Also decided:** same-file fragments (`[b](#nope)`, `[[#nope]]`) are validated
against the containing file's own headings. They carry no target path, so they
were dropped at parse time and never checked at all. They are collected into a
separate `IndexEntry.self_anchors` list rather than being forced into the link
list — they are not graph edges, and making them edges would silently change
every `--orphan` / `--dead-end` verdict. Only the *broken* ones surface in
`find --broken-links` output.

## DEC-076: a basename-only rescue is gated on whether the author wrote a directory (2026-08-23)

**Decision:** When a broken link matches a vault file by its last path segment
alone, the verdict depends on what the author wrote, not on the leading
character:

- **No directory component** (`[[actions]]`, `[x](actions.md)`) → the target
  asserts no location, so resolving it by stem is the documented Obsidian
  short-form rule. Strategy `ShortestPath`, confidence 0.95, written by plain
  `--apply`.
- **Any directory component** (`[x](guides/actions)`, `[x](/guides/actions)`,
  `[[sub/actions]]`) → the author asserted a location and matching by basename
  discards it. Strategy `BasenameFallback`, reduced confidence, written only
  under `--apply-fuzzy` / `--min-confidence`.

**Why:** iter-200 gated only the *site-absolute* spelling, so
`[x](guides/actions)` was rewritten to `reference/actions.md` by a plain
`--apply` while the byte-identical `/guides/actions` required an opt-in. Same
guess, two gates, and no way to explain the difference to a user — the
2026-08-23 dogfood called it indefensible. A leading `/` is a *syntax* fact; the
presence of a directory is the *semantic* one, and it is the only thing that
distinguishes a location claim from a bare name.

**Cost, measured:** on GitHub Docs `links fix` now reports `fixable: 0,
fuzzy: 4659` where the path-asserting half of those 4,659 used to be written by
a plain `--apply`. That is the same corruption class iter-200's M-1 finding was
about, applied consistently.

**Also decided:** the read-side stem rescue is labelled honestly. A link whose
exact path fails but whose bare stem resolves elsewhere used to be reported as
`link-case-mismatch` with `confidence: 1.0` — a *relocation* dressed as a casing
fix, printed next to an old and new target differing by a whole directory. It is
now `LinkResolution::StemRelocation` → `FixStrategy::ShortestPath` at 0.95, and
`links fix` text output prints each fix's own rule code instead of a hard-coded
`[link-case-mismatch]`.

## DEC-077: a trailing slash prefers the directory index but still falls back to the file (2026-08-23)

**Decision:** `foo/` resolves to `foo/index.md` when that exists and to `foo.md`
otherwise — the permissive rule already implemented in
`discovery::resolve_target`. Iteration 203's plan text ("`foo/` is unambiguously
a directory reference", implying no `.md` fallback) is superseded: the trailing
slash changes the *precedence*, not the candidate set. `foo` prefers `foo.md`
then `foo/index.md`; `foo/` prefers `foo/index.md` then `foo.md`.

**Why permissive rather than strict:** every pretty-URL static site generator
(Jekyll, Hugo, Docusaurus, Next.js) serves the page authored in `baz.md` at the
URL `/baz/`. Making `/baz/` broken whenever `baz/index.md` is absent would
manufacture false positives on exactly the corpora the directory-index work of
iteration 203 exists to support, for no integrity gain — the fallback can only
ever resolve to a file that exists.

**What actually had to change** was not the rule but its *uniform application*.
Three surfaces disagreed:

1. `normalize_link_target` ran relative targets through
   `normalize_path_components`, which drops a trailing slash — so `[a](foo/)`
   resolved to `foo.md` while `[b](/foo/)`, which skips normalization, resolved
   to `foo/index.md`. The slash is now re-attached after normalization.
2. The link graph keyed `/baz/` under the literal key `baz/`, which
   `backlinks baz.md` (which probes `baz.md` and `baz`) can never hit. Trailing
   slashes are now stripped from graph keys after the spelling has been recorded.
3. The graph registered **both** the written key and the resolved key for one
   occurrence, so a single `[b](foo/)` was a backlink of `foo.md` *and*
   `foo/index.md` while `links` reported `ambiguous: 0`. One occurrence now
   produces exactly one key — the resolved one when resolution succeeded, the
   written one otherwise. This generalizes the narrower L-1 rule it replaces.

`find --broken-links`, `backlinks`, `links fix` and HYALO006 now agree on all
eight dogfood spellings, verified corpus-wide against GitHub Docs.

## DEC-078: fix confidence is basename-dominant, and `--apply-fuzzy` has a floor (2026-08-23)

**Decision:** the confidence attached to a `links fix` proposal is
`0.7 · basename_similarity + 0.3 · directory_similarity`, and `--apply-fuzzy`
writes only proposals at or above **0.8** unless the user says otherwise
(`--min-confidence`, or `[links] fuzzy_min_confidence` in `.hyalo.toml`).

**Why the old score had to go:** it was `strsim::jaro_winkler` over the two
filename stems. Jaro-Winkler adds a bonus for a shared prefix, which is exactly
the wrong bias for documentation slugs, and it has no view of the directory at
all. Meanwhile `BasenameFallback` — the strategy that fires when the basename
matches *exactly* — reported a flat constant of 0.6. The result on GitHub Docs
was an inverted ordering:

| proposal | old | new |
| --- | --- | --- |
| `/actions/reference/actions-limits` → `graphql/reference/actions.md` (wrong) | 0.9 | 0.504 |
| `/billing/reference/actions-minute-multipliers` → `…/actions-built-in-queries.md` (wrong) | 0.889 | 0.533 |
| `…/scan-code-for-vulnerabilities/configuring-larger-runners-for-default-setup` → `…/find-and-fix-code-vulnerabilities/…` (correct) | 0.6 | 0.87 |

**The model** (`hyalo-core/src/link_score.rs`):

- *Basename* — a soft token F1 over the slug tokens. Each token is paired with
  its best partner in the other slug, but the pairing only counts above a
  Jaro-Winkler floor of 0.85. Without that floor every unrelated English word
  pair contributes 0.5–0.65 of partial credit and long slugs score high against
  short ones; with it, `actions-limits` vs `actions` drops to 0.667 while the
  typo `configuraton` vs `configuration` stays at 0.96.
- *Directory* — three quarters shared **leading** components, one quarter
  unordered token overlap. Pure membership was tried first and failed: generic
  levels (`how-tos`, `reference`, `guides`) are shared by thousands of
  unrelated pages, so `actions/how-tos/x` scored 0.67 against `billing/how-tos/y`
  and every cross-product substitution cleared the floor.
- *No location claim, no directory term.* A target written with no separator
  (`[[targt]]`) asserts nothing about where the file lives (DEC-076), so
  scoring it against the candidate's directory would penalise it for a claim it
  never made. Site-absolute targets are the opposite case: `/actions` strips to
  a bare `actions` but still claims the site root, so the claim is passed in
  explicitly rather than inferred from the stripped text.
- Relative targets are resolved against the source directory before scoring —
  `../c/target.md` in `a/b/page.md` is a claim about `a/c/`, not about a
  directory named `..`.

**Why 0.8:** measured on the GitHub Docs corpus (3,710 files, 6,099 broken
links), classifying every proposal against the `redirect_from:` frontmatter
GitHub maintains as ground truth (9,467 redirects indexed):

| floor | applied | wrong | unknown | correct (of known) |
| --- | --- | --- | --- | --- |
| none (v0.20.0 behaviour) | 4,659 | 804 | 144 | 82.2% |
| 0.75 | 3,111 | 39 | 17 | 98.7% |
| **0.8 (chosen)** | **2,253** | **15** | **3** | **99.3%** |
| 0.85 | 596 | 11 | 0 | 98.2% |
| 0.9 | 312 | 0 | 0 | 100% |

Accuracy is monotone in the score across the whole range (bands below 0.45 are
under 30% correct, bands above 0.8 are over 99%), which is the real result: the
number is now a usable signal rather than decoration. 0.8 was chosen over 0.75
because it is the same number as the existing `--threshold` default — one fewer
magic constant to explain — and because the iteration's purpose is trust, which
argues for the precision-favouring end of a range where both options clear the
≥90% target. Users who want the extra 858 fixes pass `--min-confidence 0.75`.

**Kept separate:** `--threshold` still means "minimum Jaro-Winkler stem score
for a file to be a fuzzy *candidate* at all". It admits candidates; the
confidence floor decides which admitted candidates get written. Reusing the
stem gate also keeps the composite scorer off the ~99% of the vault that could
never win, so scoring stayed free (13.6s on 3,710 files, unchanged).

**Side effect, accepted:** ranking by the composite instead of the raw stem
score breaks ties the old code declined. Two same-stem candidates in different
directories used to score identically and be rejected as ambiguous; now the
directory-nearer one wins. Fuzzy proposals rose 4,659 → 5,506 and `unfixable`
fell 1,377 → 530, but applied rewrites fell 4,659 → 2,253 because the floor
does the real filtering. Reporting more candidates while writing fewer is the
intended shape.

## DEC-079: `.hyalo.toml` is discovered from ancestors, not just the working directory (2026-08-23)

**Decision:** when the working directory has no `.hyalo.toml`, hyalo walks up to
the **nearest** ancestor that has one and adopts it — but only when that
config's resolved vault contains the working directory. A nearer config that
points somewhere else is not adopted, and the walk does not continue past it.

**Why:** the config was read from CWD and nowhere else, so `cd
hyalo-knowledgebase && hyalo lint` re-rooted on built-in defaults: no schema, no
`[lint] ignore`, no views, no `site_prefix`, `dir = "."`. Nothing was printed.
The `--dir` spelling of the same mistake has warned loudly since iter-201
(DEC-070), and the `cd` spelling is the one people actually make — it is what an
agent does the moment it wants a shorter path. Adoption is the fix rather than a
warning, because the invocation is not wrong: the user is standing inside the
vault their config describes, and every file path they can name is still
vault-relative.

**Nearest wins, containment gates:** walking past a non-matching config to a
further ancestor would make "which file applies" depend on the contents of files
in between — two vaults side by side under one repo could each capture the
other's subdirectories. Stopping at the first `.hyalo.toml` and requiring
containment keeps the answer decidable from the directory chain alone. An
ancestor config that cannot be parsed is still adopted (its vault is taken to be
its own directory), because the alternative is to skip it silently — which is
the failure mode this decision exists to remove.

**Announced only when it widens scope:** in the common case (`cd <vault>`) the
adopted vault *is* the working directory, so nothing about the run changes
except that the settings apply, and hyalo says nothing. From a deeper
subdirectory the vault is genuinely wider than where the user is standing, so a
stderr note names the config and points at `--dir .`.

**Retired:** the "hyalo is configured with dir = X — do not cd into X" warning
(iter-128). It taught users to avoid a workflow that now works. The
absolute-`--file` half of that warning stays: passing `/abs/path/to/note.md`
still resolves, still warns, and is still worth discouraging.

**Side effect, accepted:** `views set`, `types set` and `lint-rules set` run
from inside the vault now write to the ancestor `.hyalo.toml` instead of
creating a new one in the working directory. That is the desired outcome —
nested configs are shadowed and already warned about — but it does mean a run
that previously created a config file now edits one.

## DEC-080: frontmatter writes splice per-key line spans instead of re-serializing (2026-08-23)

**Decision:** every frontmatter mutation re-emits the **original bytes** of every
top-level key whose value did not change, and serializes only the keys that were
added, changed, or reordered. The raw YAML block is segmented into one line span
per top-level key; unchanged spans are copied verbatim, changed spans are
replaced with freshly serialized YAML for that key alone, removed spans (and the
comment block directly above them) are dropped.

**Why:** the writer parsed the whole block into `serde_yaml`/`serde_saphyr`
values and re-serialized everything, so a one-key change rewrote every line the
serializer chose to format differently. On a real GitHub Docs `index.md` that
was **116 of 198 frontmatter lines** for a single added property — long list
items refolded into `>-` block scalars, `'` quote style flipped to `"`. The
round trip was semantically lossless, so nothing was *lost*; but a 116-line diff
for a one-key change is unreviewable, and it makes hyalo unusable in any repo
where frontmatter is under code review. Diff churn is an adoption blocker
independent of correctness.

**Supersedes DEC-006**, which accepted full-block rewrites because the YAML
library could not preserve formatting. The fix does not need the library to:
the unchanged text is never handed to the serializer at all.

**Splice, not a format-preserving YAML crate:** the alternative was to swap the
parser for a CST-preserving YAML library (the `toml_edit` equivalent, which
hyalo already uses for `.hyalo.toml` — that path never churned). No maintained
Rust YAML crate offers `toml_edit`'s fidelity, and swapping the parser would put
every read path, the hardened `Budget`, strict-boolean handling, and duplicate-key
policy back on the table for a formatting problem. Line-span splicing is
confined to the write path and leaves the read model untouched.

**Comments belong to the key below them:** a contiguous run of blank/comment
lines immediately above a key travels with that key — including into oblivion
when the key is removed. Trivia before the *first* key is document-level and
stays at the top; trivia after the last key is a footer and stays at the bottom.
This is a convention, not a YAML rule, but it matches how frontmatter comments
are actually written (`# explains the field below`).

**Verified, not trusted:** span mapping is a heuristic, so the spliced text is
re-parsed and compared against the exact property map the caller asked to write
— same keys, same order, same values — before it is returned. Verification uses
budgets scaled to the frontmatter size limit rather than the tighter read-path
defaults, because a caller is allowed to *write* a value larger than the
read-path scalar budget (the size-budget pre-flight is what rejects those) and
verification must not mistake that for a splicing failure.

## DEC-081: the full-rewrite fallback stays, but is never silent (2026-08-23)

**Decision:** when a frontmatter block cannot be span-mapped, hyalo still
performs the write — by re-serializing the whole block — and emits a
`warning:` on stderr naming the file and the reason. It does **not** refuse.

**Why not refuse:** the fallback triggers on YAML hyalo can read but this
splicer deliberately does not model. Refusing would turn a *cosmetic*
limitation into a hard failure on files that `set` has always handled
correctly, and would leave the user with no in-tool way to change a
property. The prior behavior (full rewrite) is still correct; it is only noisy.

**Corrected fallback-trigger list (iter-219):** the real set that reaches
this fallback is explicit `? key` syntax, top-level flow mappings/sequences,
invalid UTF-8, and mixed line endings (DEC-087). The original list above
also named "directives" and, implicitly, anchors/aliases — both are wrong:
a `%directive` or `&anchor`/`*alias` inside the frontmatter block either
fails the *baseline* parse outright (`Unparseable`, same family as any other
malformed YAML) or — for anchors/aliases specifically — is rejected before
splicing is ever reached, by the parser's own `max_anchors: 0` /
`max_aliases: 0` budget (see DEC-088). Neither reaches this graceful,
whole-block-reserialize fallback; both hard-error, and `set`/`remove`/
`append` skip the file as unparseable rather than silently reformatting it.

**Why not silent:** the entire point of DEC-080 is that unexplained diff churn
destroys trust in the tool. A user who sees 116 changed lines must be able to
tell "hyalo could not do better here, and said so" from "hyalo has a bug".
The warning carries the reason string, so the fallback class is identifiable
from the message alone.

**Not warned:** creating frontmatter on a file that had none, and writing into
an empty `---\n---` block. Neither has any formatting to preserve, so neither is
churn.

## DEC-082: `links auto` emits an alias, not a silent prose rewrite, when matched text differs from the target (2026-08-23)

**Decision:** `links auto --apply` writes `[[link_target|matched_text]]`
whenever the matched surface text is not byte-identical to the emitted
target — including a bare case difference (`Pulls` vs `pulls`) — and only
writes the plain `[[link_target]]` form when the two are identical. The
comparison is exact string equality; there is no separate case-insensitive
carve-out; any difference at all routes through the alias branch.

**Why:** before this decision, `--apply` always substituted the target stem
for the matched text, so `pull requests` became `[[pulls]]` and `PULLS`
became `[[pulls]]` — silently changing what the page renders. On a GitHub
Docs dogfood corpus, 22.2% of proposed insertions (7,968 of 35,860) altered
rendered prose this way; 5,178 of those were pure case differences. No prior
decision documented prose substitution as intended, and the read side
already supported the alias form — `AutoLinkMatch` carried `matched_text`
and `link_target` as two separate fields specifically so the write side
could tell them apart. The fix is confined to the replacement-text
construction (`wikilink_replacement_text` in `auto_link.rs`); the scan and
match logic are unchanged.

**Why exact equality, not case-insensitive-equal-then-plain:** a title
mention that differs only by case is still a case difference the author
chose to write, and `[[target]]` alone would flip it to whatever case the
target's stem happens to use. Preserving the exact surface form via an
alias is strictly safer than guessing which case differences are "close
enough" to normalize away.

**Text-format dry-run output mirrors this:** the `links auto` text renderer
shows `"<matched_text>" → [[target|matched_text]]` (or plain `[[target]]`
when they match) so a dry-run preview shows exactly what `--apply` will
write, not a simplified approximation of it.

## DEC-083: `links auto` ambiguity is checked in the emitted namespace, not just the title namespace (2026-08-23)

**Decision:** a candidate title is excluded from the auto-link inventory
when **either** of two conditions holds: two different source files produce
the identical (lowercased) title-map key (the pre-existing check), **or**
two different source files would each emit the same `link_target` — the
filename stem actually written into `[[…]]` — even though their title-map
keys differ. Both conditions add every implicated title to
`ambiguous_titles`.

**Why the second condition is necessary:** ambiguity has to be checked
against what gets **written**, not against the human-readable title used to
find the candidate. `graphql/reference/pulls.md` (title "Pull requests")
and `rest/pulls/pulls.md` (title "REST API endpoints for pull requests")
have distinct titles — so a title-only ambiguity check sees no conflict —
but both files are literally named `pulls.md`, so both resolve to the same
emitted stem `pulls`. Checking titles alone let `links auto --apply` write
`[[pulls]]` for either mention, a link hyalo's own resolver then reports as
ambiguous (`links` on a real GitHub Docs corpus: `ambiguous` count 0 → 1,492
after such a run).

**Implementation is a second pass over `entries`, not over the survivors of
the first:** a second pass builds `target_sources` — `stem → {source
files}` — directly from the raw index `entries` (minus `--exclude-target-glob`
matches, mirroring the first pass's own exclusion), and removes any
`title_map` entry whose target has more than one distinct source, adding the
target to `ambiguous_titles` if not already present.

The first implementation tried instead grouped `title_map.values()` — the
entries that *survived* the identical-key pass — and passed the
`graphql/reference/pulls.md` / `rest/pulls/pulls.md` synthetic test built to
verify it. It failed on the real GitHub Docs corpus: `rest/pulls/pulls.md`
and `rest/pulls/index.md` happen to share the exact title "REST API
endpoints for pull requests", an *unrelated* first-pass key collision that
removed `rest/pulls/pulls.md`'s own title-map entry before the
target-collision pass ever ran. With that file's contribution gone from
`title_map`, the survivors-only pass saw only `graphql/reference/pulls.md`'s
"Pull requests" entry targeting `pulls` — a lone source, not ambiguous — and
`links auto --apply` wrote `[[pulls]]` 1,429 times before this was caught by
running the fix against real data rather than a 2-file unit test. The lesson
generalizes: an ambiguity check must be computed from ground truth (every
file's own stem, from `entries`), never from a structure that a different,
unrelated collision can have already thinned out. Regression coverage:
`same_stem_different_dirs_and_titles_is_ambiguous`,
`stem_collision_survives_even_when_one_side_title_collides_elsewhere`, and
`same_stem_different_dirs_ambiguity_blocks_actual_writes` in `auto_link.rs`
— the middle test is the real-corpus shape and is the one that would have
failed under the survivors-only implementation.

**AC:** `hyalo links` (the independent resolver) reports the same
`ambiguous` count before and after `links auto --apply` on any corpus —
nothing written by `--apply` may ever appear in that count afterward.
Verified on live GitHub Docs and vscode-docs scratch copies, not only unit
tests, precisely because the unit-test-only version of this fix shipped a
real gap.

## DEC-084: any well-formed `[...]` bracket span is inert, not only ones that resolve to a real link or reference (2026-08-23)

**Decision:** `links auto`'s zone scan treats **every** syntactically
well-formed `[...]` span — an unescaped `[` matched by a later unescaped
`]` within the same paragraph block — as inert, regardless of what is
inside it. This subsumes CommonMark reference-link usages (`[label][ref]`,
`[ref][]`, shortcut `[ref]`, `![ref][ref]`) as a special case, but is not
conditioned on a matching `[ref]: url` definition existing anywhere in the
document.

**Supersedes the iter-217 plan as written.** The iteration's own task list
said: "Shortcut-form detection must not blanket-ban all bracketed text:
only labels that match a definition in the same document are reference
links" — i.e. `[Gamma]` should stay a linkable candidate when no
`[Gamma]: url` definition exists, since otherwise it is just prose in
square brackets. A first implementation did exactly that (gated shortcut
zoning on a document-wide `definitions: HashSet<String>`), passed every
unit test built against that premise, and still corrupted real corpora.

**Why the premise was wrong:** GitHub Docs and vscode-docs both use plain,
*undefined* `[...]` bracket conventions for things that were never links —
GitHub's style guide writes permission-statement placeholders as
`[ACCOUNT ROLE]`; vscode's release notes prefix each entry with a PR area
tag like `[typescript-language-features]`. Neither has a corresponding
`[ref]: url` definition; under the definition-gated design both were
correctly judged "not a reference link" and left open to auto-linking.
Auto-linking "ACCOUNT" or "typescript" inside them spliced `[[…]]` markup
directly against the pre-existing bracket, producing
`[[[account|ACCOUNT]] ROLE]` and `[[[typescript]]-language-features]` —
syntactically valid nowhere, and specifically misread by hyalo's own
wikilink parser as a link with a mangled target (`"[account"`,
`"[typescript"`). Verification against live corpora (not just the 2-3-file
unit tests the definition-gated version passed) caught this: GH Docs
`broken` count rose 6,099 → 6,107, vscode-docs 330 → 371, even after NEW-1
(defined reference links), NEW-2 (wrapped links), NEW-4 (emitted-namespace
ambiguity) were all independently fixed and verified.

**Why blanket-inert is the right trade, not a workaround:** the codebase's
standing zone-scan philosophy — stated for Liquid/HTML in iter-207 and
reused throughout `inert_link_zones` — is "an unterminated marker makes
the rest of the line inert… a missed auto-link candidate costs nothing, a
corrupted file does." A bracket span that isn't a real link is exactly
this shape: ambiguous intent, cheap to skip, expensive to guess wrong on.
Treating every bracket span uniformly (rather than trying to distinguish
"this bracket looks decorative" from "this bracket might resolve someday")
also keeps the rule simple: one fallback arm in `inert_link_zones`'s
existing `[` handling, no new state, no interaction with the definition
collector. The recall this costs — a title mention that happens to sit
inside unrelated brackets never gets linked — was already the accepted
trade-off for every other zone in this scanner.

**What still needs the definition collector:** a `[ref]: url "title"`
*definition line itself*. The `[ref]` part is already covered by the
generic bracket rule, but the destination and title after the colon sit
outside any bracket and would otherwise be ordinary, linkable prose —
`parse_reference_definition_label` and the per-line `is_definition` flag in
`auto_link.rs`'s block builder exist specifically to blank the whole line.

**Regression coverage:** `bracketed_mention_without_a_reference_definition_stays_inert`
and `placeholder_style_bracket_text_is_never_corrupted` in `auto_link.rs`
lock in the corrected behavior; the former replaced a same-named-in-spirit
test that asserted the opposite under the superseded design.

## DEC-085: `lint --fix` JSON renames `errors`/`warnings` to `remaining_errors`/`remaining_warnings` (2026-08-23)

**Decision:** rename, don't unify. Plain `lint`'s `errors`/`warnings` keep
meaning whole-run severity counts, unchanged. `lint --fix`'s JSON — which
used to carry the same key names for a different quantity (violations left
after fixing) — now carries `remaining_errors`/`remaining_warnings`
instead. `dispatch.rs`'s `inject_ext_file_result` (which patches injected
`views`-sourced violations into either shape) now bumps whichever key pair
the payload it is patching actually carries.

**Why not unify the meaning instead:** the two numbers answer genuinely
different questions and both are needed. A CI gate wants "did anything I
couldn't autofix stay broken" (remaining, fix-mode's meaning); an audit
wants "how much is wrong with this vault, before any fix" (whole-run,
plain lint's meaning). Making fix-mode also report whole-run counts would
have made the exit code (already driven by the remaining-after-fix count,
correctly — a fully-autofixed vault should exit 0) disagree with the
`errors`/`warnings` in the same payload it drives. Making plain `lint`
report a "remaining" count makes no sense — nothing was fixed. There was
no single meaning that served both callers; only a key name pretended
there was.

**Why this is worth a breaking JSON-shape change instead of leaving it**:
dogfood NEW-6b — a script reading `.errors` off both `lint` and
`lint --fix` output got answers to two different questions under one key
name, with no signal in the JSON that the meaning had shifted. That is
exactly the class of output-truth defect iter-210/iter-218 exist to
remove elsewhere in this same command's counters (see NEW-6 above, and
BUG-6 in iter-210). Landing it in the same iteration as NEW-6 means one
CHANGELOG entry and one migration note instead of two.

**Blast radius checked:** the other producers/consumers of the old
`errors`/`warnings` keys on the fix-mode shape were `dispatch.rs`'s view-
violation injection path and `run.rs`'s `empty_result_for_command` (the
`--files-from`-resolves-to-zero-files shortcut, which hand-built the
fix-mode JSON shape rather than reusing `ExtLintFixOutput` — review finding
#5 on the first review round switched it to serializing
`ExtLintFixOutput::default()` so this rename, and any future one, cannot
drift out of sync there again). The text renderer
(`format_lint_fix_output_text`) never read those keys at all (the footer is
built entirely from `total_fixed`/`total_remaining`/`total_conflicts`), so
no text-output change was needed.

## DEC-086: the splice engine extends inside a key's own span — one appended or removed list item is a line-level edit, not a whole-value re-serialize (2026-08-23)

**Decision:** when a top-level key's *value* changes, and both the old and
new values are arrays of plain scalars differing by exactly one item
(append at the end, or removal of one item anywhere), `splice_frontmatter`
edits only that item's line(s) instead of re-serializing the whole list.
Block-style lists (`key:\n  - a\n  - b\n`, indented or compact-flush) get
one inserted or deleted dash line; flow-style lists (`key: [a, b]`) stay
flow, rebuilding just the bracket interior. Anything else about the change
— replacement, reorder, multiple items, a non-scalar item anywhere in the
list — falls through to the existing whole-key re-serialize from DEC-080;
this is strictly additive to that path, not a replacement for it.

**Why:** DEC-080 fixed cross-key diff churn (`set` on one key no longer
rewrites every other key), but `append`/`remove <key>=<value>` — which only
ever change a list by exactly one item — still fell into the *within-key*
"Changed" branch and re-serialized the whole list's value. On a real
GitHub Docs corpus, one appended `redirect_from` entry churned more than
one line in 361 of 406 files (worst case 118 lines, `admin/index.md`) —
DEC-080's own bar ("touch only what changed") applies just as much inside a
key's span as across keys, and this was the same defect relocated rather
than fixed.

**Detected structurally, not by caller intent:** `splice_frontmatter` has no
way to know whether a value change came from `append`, `remove`, or a
`set --property tags=[...]` full replacement — all three funnel through the
same parse → mutate → write cycle and hand the splicer only the before/after
property maps. So the append-one/remove-one shape is classified purely from
comparing the two array values (`classify_list_delta`): this is what makes
the fix apply uniformly to `append`, `remove --property k=v`, and
`remove --tag t` without any CLI-layer changes — all three already produce
exactly this "array shrinks/grows by one scalar" shape in memory.

**Verification gate covers this too:** like every other heuristic in this
module, a list splice that doesn't match reality (wrong line count, tabs,
an item that turns out to be multi-line) is caught by re-parsing the
spliced output and comparing it against the requested property map before
returning `Spliced`; anything that doesn't verify was never at risk of
reaching disk wrong, only of missing the minimal-diff optimization.

**A list that "looks like" flow or block but doesn't fit the model still
falls back, and still warns — for both append and remove:** the first
implementation checked whether the *new item* could be inlined before
checking whether the body even *was* a flow list, so a non-inlineable item
appended to a flow list took the `NotApplicable` path and silently
re-serialized flow to block with no warning — precisely the DEC-081
violation this decision exists to prevent (caught in PR #250 review, M1).
The fix reorders the checks: `is_single_line_flow(body)` is evaluated
first, so once a body is recognized as flow-shaped, *any* reason splicing
fails inside it — an unrenderable new item, or (M2/M3) the existing list
itself having a trailing `#`-comment the tokenizer can't represent —
routes to the explicit `FlowListNotModellable` fallback, symmetrically for
`ListDelta::Append` and `ListDelta::Remove`. The same logic applies to
block lists: a `#`-comment interleaved between item lines used to silently
fall through to a whole-key re-serialize that discarded the comment with
no explanation (M6); `is_unmodellable_block_list` now detects "this is
block-sequence-shaped but doesn't split cleanly" and routes it to a
dedicated `BlockListNotModellable` fallback instead. Both are a wider
hammer than strictly necessary for a one-key problem, but reuse DEC-081's
existing "full rewrite, always announced" machinery rather than inventing
a narrower warning channel for shapes expected to be rare in practice
(short redirect/alias/tag strings without comments, not multi-paragraph
values or heavily annotated lists).

## DEC-087: mixed line endings get an honest full-rewrite fallback, not a silent per-line-preserving splice (2026-08-23)

**Decision:** when a frontmatter block's lines don't all share the same
line-ending style as its opening `---` line, the write path treats this the
same as any other DEC-081 fallback trigger: skip the minimal-diff splice
entirely, re-serialize the whole block, and warn on stderr that the
frontmatter mixed line endings. The block (and thus the file) still ends up
on one consistent style afterward — that part doesn't change — but it is no
longer silent.

**The bug this replaces:** `find_body_offset` recorded only the *opening*
delimiter's line-ending style; the write path re-expanded every line to
that one style unconditionally. A file with `\r\n` on some lines and `\n`
on others round-tripped through `set` with those `\r`s dropped (if the
opening line was `\n`-terminated) or spuriously added to originally-`\n`
lines (if the opening line was `\r\n`-terminated) — with an empty stderr.
`splice_frontmatter`'s own mixed-endings guard didn't catch this either: it
only rejects a *lone* `\r` not paired with `\n` (embedded literal `\r`
inside a value), which is a different, narrower concern from *which* lines
use which paired terminator.

**Why the honest-warning option, not per-line preservation:** the
alternative — tracking each line's original terminator through the splice
and re-emitting it verbatim — would mean every span boundary, every
`serialize_one` call, and the whole-document fallback's own re-serialize
step would all need to carry and restore individual line endings, not just
one block-wide style. That is real engineering weight for a shape hyalo has
never claimed to preserve losslessly (mixed EOL within one file is already
unusual enough that Git itself normally flags it). DEC-081 already has a
"fall back and say so" contract for exactly this kind of edge case;
reusing it is far cheaper than building a second preservation mechanism,
and satisfies the actual complaint (no *silent* churn) without it.

**Detection is free:** `find_body_offset` already reads every line of the
block to find the closing delimiter; checking each line's terminator
against the opening line's while it does so costs nothing extra. Only a
newline-terminated line can mismatch — the file's very last line, if it
lacks a terminator entirely (see DEC-089), is not treated as a mismatch;
that is NEW-16a's concern, not this one's.

## DEC-088: the frontmatter parser's scalar-content budget matches the documented 64 KiB limit, and its errors don't leak internals (2026-08-23)

**Decision:** `hyalo_options()`'s `Budget.max_total_scalar_bytes` is set to
`MAX_FRONTMATTER_BYTES` (64 KiB) instead of the prior 8192. Every
`serde_saphyr` parse error that reaches a user (four call sites: the two
production frontmatter-parse paths plus the two scanner fast-paths) is now
passed through `friendly_parse_error`, which intercepts the two variants
whose `Display` renders Rust struct/Debug syntax — a budget breach
(`budget breached: ScalarBytes { total_scalar_bytes: 8205 }`) and a
duplicate key (`..., set DuplicateKeyPolicy in Options if acceptable`, a
hint about hyalo's own internal `Options` type) — and replaces them with
plain, actionable text naming what happened. Every other `serde_saphyr`
error variant's own `Display` is already clean and passes through
unmodified.

**Why the budget was wrong:** `MAX_FRONTMATTER_BYTES` (the documented "64
KiB / 2000 lines" limit in `set`/`remove`/`append --help`, and the
pre-flight check every write already enforces) is a limit on the *whole
frontmatter block*. The parser's own internal `max_total_scalar_bytes`
budget — meant as defense-in-depth against pathological inputs, not a
user-facing limit at all — was independently set to 8192 bytes, a fraction
of the documented ceiling. A real GitHub Docs `admin/index.md` at 7,961
bytes of frontmatter was about 40 redirect entries from becoming unreadable
by hyalo, well inside its own documented budget. Scalar content is a
subset of total block bytes, which every caller already caps at
`MAX_FRONTMATTER_BYTES` before the parser budget is even consulted, so
raising this to match can never make the *effective* ceiling looser than
what was already documented and enforced elsewhere.

**Why interception, not a new error type:** `serde_saphyr::Error` is
`#[non_exhaustive]` and its message formatting is a library concern hyalo
doesn't control. Rather than growing a parallel error hierarchy,
`friendly_parse_error` *matches on* `unwrap_snippet(err)` — walking past
`Error::WithSnippet` wrappers only to reach the two offending shapes
(`Error::Budget`, `Error::DuplicateMappingKey`) for pattern-matching, not
by string-matching the rendered message, which would be fragile against
upstream wording changes.

**Every other variant returns the original, still-wrapped error (PR #250
review, M4):** the first implementation's catch-all arm called
`.to_string()` on the *unwrapped* inner error returned by
`unwrap_snippet`, which discards `Error::WithSnippet`'s source-context
caret/window — for the common cases this function was never meant to
rewrite (bad indentation, an unexpected token), this made error quality
strictly worse than before the fix, the opposite of the goal. The catch-all
now returns `err.to_string()` — the original, outer error — so `unwrap_snippet`
is used only to decide *which* branch to take, never to build the returned
message for anything but the two variants actually being rewritten.

**Location is preserved, not dropped, on the rewritten variants:** both
`Error::Budget` and `Error::DuplicateMappingKey` carry a `location` field;
the first cut of the rewrite discarded it, which broke an existing CLI-side
test (`terse_root_cause_strips_duplicate_key_policy_advice`) that depended
on line/column surviving into the terse message. `friendly_parse_error`
now appends `" at line X, column Y"` — the exact phrasing
`serde_saphyr`'s own localizer uses elsewhere, matched deliberately rather
than invented — and omits it entirely when the location is
`serde_saphyr::Location::UNKNOWN`, rather than printing "at line 0, column 0".

**The scalar-byte limit named in the `ScalarBytes` message is passed in,
not hardcoded (L13):** `splice_frontmatter`'s own verification pass parses
with a 2x budget (`MAX_FRONTMATTER_BYTES * 2` — see DEC-086's
verification-gate note), specifically so a caller-supplied value larger
than the read-path budget isn't mistaken for a splicing failure. That path
doesn't currently route its errors through `friendly_parse_error` (they're
discarded and turned into a generic `VerificationFailed` fallback instead),
but `describe_budget_breach` takes `scalar_byte_limit` as an explicit
parameter rather than reading the `MAX_FRONTMATTER_BYTES` constant
directly, so a future caller wiring that path through here cannot silently
get a message naming the wrong limit.

## DEC-089: NEW-16 write-path residue — no invented trailing newline, a narrow dotted-key guard, and a retype advisory (2026-08-23)

**No invented trailing newline (NEW-16a):** a file whose last three bytes
are literally `---` — no body, no trailing newline anywhere after the
closing delimiter — must round-trip that way. `find_body_offset` now
records whether the closing `---` line itself was newline-terminated; the
write path only appends the delimiter's line-ending after the closing
`---` when there is a body to separate from, or the original already had
one there. Creating brand-new frontmatter (no prior close to inspect)
always gets the separator, matching existing behavior for that case.

**Dotted-key collision guard, not path syntax (NEW-16b):** `hyalo` has
never supported dotted paths in `--property`; `set --property a.b=x`
always writes (or overwrites) a literal top-level key named `"a.b"`. That
is silently wrong specifically when `a` already exists as a mapping — the
GitHub Docs repro is `--property versions.fpt=X` against a file with an
existing `versions:` map, which used to create a `versions.fpt` key
sitting right next to the map it looked like it should have nested into.
`set`/`append` now reject that one collision with an error naming the
file and the colliding key — and the error's hint spells out what to do
instead (edit the file directly to change a value inside the map, or
choose a non-colliding key name) rather than only explaining what's
unsupported. A dotted key with no colliding map is unchanged — still a
literal flat key — because adding real nested-path support is explicitly
out of scope for this iteration; only the confusing collision is guarded
against, not the general case. `remove` is not guarded: removing a
nonexistent literal dotted key is already a harmless no-op (reported as
skipped), not a data-corruption risk.

**Runs as a whole-batch pre-pass, not a mid-loop check (PR #250 review,
M5):** the first implementation checked for the collision *inside* the
per-file read-modify-write loop and `return`ed immediately on a hit. On a
50-file batch, hitting the collision on file 7 left files 1-6 already
written to disk, and skipped the end-of-loop `save_index_if_dirty` call
entirely — a partial write plus a stale on-disk snapshot index, exactly
the kind of half-applied batch mutation the rest of this codebase goes out
of its way to avoid (see the existing BUG-D pre-validation pass in both
`set` and `append`, which this guard now sits next to). The fix moves the
check into its own read-only pass over every filtered file, run before any
mutation and before the (also pre-existing) `--validate` pass — reject the
whole batch, or don't touch anything.

**Retype advisory reuses the existing advisory mechanism, scoped to avoid
noise (NEW-16c):** `set`'s CLI argument is always a bare string, so type
inference has no way to know a property was deliberately quoted to stay
text — `code: '42'` (string) silently becoming `code: 42` (number) via
`set --property code=42` is inherent to how the CLI parses values, not a
bug to fix. What was missing was visibility: `advisory_note` (which already
carries the BUG-B date advisory and the iter-181 enum/pattern advisory)
gained a third branch that fires when the *first mutated file's*
pre-existing value for that property was a string and the newly inferred
value is a number or boolean — a third branch in the same function, not a
new mechanism. Scoped to "was previously a string, now isn't" rather than
"is a number/boolean at all" specifically to avoid noise on properties
that are numeric by design (`priority=3` on a file where `priority` never
existed, or was already numeric, does not fire this advisory) — the
surprising case is specifically the type *changing* under an existing
value, mirroring how the date advisory only fires when a date-typed
property's *new* value fails to look like a date, not on every write to
that property.

**Wording hedges the batch-vs-sample gap (PR #250 review, L11):** the
sampled value comes from exactly one representative file — the same
"first mutated file" sampling `batch_type_from_file` already uses for
schema resolution — not from every file the batch touches. The first cut
of the message ("was previously stored as a string") stated this as fact
about the whole batch; it now reads "at least one matched file previously
stored this property as a string ... (other matched files may differ)" so
the advisory doesn't imply a guarantee about files it never actually
inspected.

## DEC-090: CI gate for broken anchors is `find --strict`, not a new lint rule (2026-08-23)

**Decision:** UX-2's CI-gate finding ("a vault whose only defect is a dead
heading anchor exits 0") is closed with a general `--strict` flag on `find`
— exit 1 when the query returns any results, 0 when empty — rather than a
new HYALO00N lint rule alongside HYALO006 (broken-link, target only).
`find --broken-links --strict` is the anchor-gating command; the flag also
composes with any other `find` filter (`find --property status=draft
--strict`, `find --orphan --strict`, ...), so the same primitive covers
every "fail CI if this query finds anything" use case, not just anchors.

**Why not a new lint rule:** HYALO006's vault-wide context
(`LinkLintContext`, built once per `hyalo lint` invocation) tracks only a
case/stem index for target *existence* — it has no per-file heading data at
all. Anchor checking needs the target file's headings, which `find`
already has for free (`IndexEntry.sections`, populated for every file by
the same scan that builds the index) but `hyalo lint`'s vault-wide context
does not. Adding it would mean either a second full disk scan inside
`hyalo lint` just for this one rule, or threading a shared sections cache
through the lint dispatch/rayon-worker plumbing HYALO006 was carefully
built around (see the `LinkLintContext` doc comments) — real, but
disproportionate infrastructure work for a small-batch item, when `find`
already does the exact check for `--broken-links`/`--fields links` and
only needed an exit-code path bolted on.

**`links fix` gets a narrower, budget-conscious version instead of the same
machinery:** `links` (dogfood NEW-15) still needed *some* anchor signal —
"Broken links: 0" was misleading a reader into trusting a vault that
actually had dead anchors. `hyalo_core::link_fix::count_broken_anchors`
mirrors `find`'s own anchor check (`resolve_link_from_source` +
`anchor::fragment_matches_headings`) but is gated to run only when
`broken.is_empty()` — the note it feeds ("N broken anchors — see `find
--broken-links`") is only meaningful when targets are otherwise clean, and
gating on that condition means the extra resolution pass never runs on a
vault that already has broken targets, the common case on a large,
imperfect corpus (GH Docs, MDN) where the perf cost would have mattered
most. `summary`'s own `broken_anchors` figure (NEW-15) is NOT gated the
same way — `summary` is a deliberate full vault-health report, not a
fix-loop's advisory note, so it always pays the cost for an accurate
number.

**Numbers are not expected to match 1:1 across commands:** `summary`'s
`broken_anchors` counts *links*; `find --broken-links`'s `total` counts
*files*. A file with two dead anchors contributes 1 to the file count and
2 to the link count. This mirrors how `broken`/`out_of_vault` already
differ in unit from `find`'s own counts — the fix for NEW-15 is that
neither figure may claim *zero* while the other reports something, not
that the two numbers must be numerically identical.

## DEC-091: `--dir` ancestor-config discovery extends config trust to a second entry point, boundary deferred to iteration 221 (2026-08-24)

**Decision:** NEW-17's iter-220 fix (`load_config_for_dir`) gives `--dir
<foreign-tree>` the same ancestor-discovery fallback `cd <foreign-tree> &&
hyalo …` already had since iter-213 (UX-1): when `<foreign-tree>` has no
`.hyalo.toml` of its own, hyalo walks up looking for the nearest ancestor
`.hyalo.toml` whose configured vault contains `<foreign-tree>`, and — if
found — that file's `[lint]`, `[scan]`, `site_prefix`, and rule settings
govern the run. This is a deliberate widening of *where* an ancestor config
can be discovered from (a second entry point, `--dir`, not just the real
process CWD), not a new trust *rule* — the discovery logic itself
(`discover_ancestor_config`) is unchanged and iter-213's own containment
check (the ancestor's configured vault must actually contain the target
directory) still applies identically on both paths.

**Why this matters and why it is not fixed here (PR #251 review L10):** the
practical effect is that a `.hyalo.toml` living in *any* ancestor of a
`--dir` target — not just an ancestor of the real CWD — can now govern a
run. On a shared machine or a checkout with an untrusted or unexpected
ancestor directory (e.g. a `/tmp/.hyalo.toml` left by another process or
user), `--dir /tmp/some/deep/path` could silently inherit `[lint] ignore`,
`[scan]`, `site_prefix`, or rule overrides from a file the invoking user
never asked for and may not even know exists. This is the same class of
concern the `cd`-based path already had since iter-213 — NEW-17 did not
introduce the *risk*, only a second code path that reaches it — and drawing
a trust boundary around config discovery (e.g. refusing to adopt an
ancestor config outside some allowed root, or requiring an explicit opt-in)
is real design work with its own trade-offs against the discoverability
`cd <vault> && hyalo …` depends on. That is exactly the subject of
[[iterations/iteration-221-config-dir-boundary]], already planned before
this review; iter-220 does not attempt it. This DEC exists so the extension
is documented and traceable to where the boundary is meant to land, not
silently widened and forgotten.

## DEC-092: a project-local `dir` outside its own config directory is a hard refusal, not a clamp (2026-08-24)

**Decision:** [[iterations/iteration-221-config-dir-boundary]] closes H-1
(re-confirmed as F-6 in the deep-analysis-2 review): a project-local
`.hyalo.toml`'s `dir` — absolute, or netting above the config directory
after resolving `..` — now refuses the run outright rather than being
honored or silently clamped. `load_config_from` validates `dir` immediately
after a successful TOML parse, before any other field is even looked at; a
violation short-circuits to `ResolvedDefaults::dir_out_of_bounds_for`, which
leaves `dir` at the hardcoded `"."` default (never the offending value) and
records a diagnostic in a new `dir_out_of_bounds: Option<String>` field,
kept deliberately distinct from `malformed` (the TOML parsed fine; this is a
policy refusal, not a parse failure). The diagnostic names both the
offending `.hyalo.toml` and the exact `dir = "…"` value, and points at
`--dir` as the escape hatch. It is emitted through `warn::warn_always`
(survives `-q`, same as `malformed`), and every command that can touch the
filesystem refuses to run while it is set — **reads included, not just
writers** — since the whole point is that even a read must not operate
against a boundary the config was never entitled to set for itself. The one
exception is `hyalo config` itself, which reports `dir_out_of_bounds: true`
plus the reason in both JSON and text and keeps working, because surfacing
exactly this is its job.

The gate only fires when no `--dir` was given
(`crates/hyalo-cli/src/run.rs`, gated on `!dir_from_cli`): `--dir` is the
user's own explicit choice, and `EffectiveConfig::dir` is always the literal
`--dir` value in every branch of `resolve_effective` — never a discovered
config's own `dir` field — so a run with `--dir` given is safe regardless of
what an ancestor `.hyalo.toml` (adopted via DEC-091's second discovery
entry point) wrote. A `dir` that stays at-or-below the config directory,
including a bounded round-trip like `sub/../kb`, is unaffected and is now
lexically normalized (`lexically_normalize_relative`) so the round-trip
behaves like writing `kb` directly rather than requiring the phantom `sub/`
to exist on disk. Symlinks are checked too: when the resolved path already
exists, both sides are canonicalized and compared for real containment, so
a `dir` that is lexically bounded but physically escapes through a symlink
is still refused.

**Why refuse instead of clamp:** hyalo is agent-driven — CLAUDE.md instructs
agents to run its hints verbatim — so a hostile cloned repo whose
`.hyalo.toml` widens `dir` is a plausible write-scope-escape primitive
against exactly the audience most likely to run it unattended. Silently
clamping to the config directory would fix the escape but would still let
the config decide, unannounced, that the user's intended scope was wrong;
DEC-070 already establishes that a config-integrity problem this large must
be loud, not quietly worked around. Refusing everything (not just writers,
unlike DEC-070's malformed-config split) is the stricter twin of that
stance: DEC-070's read/write split exists because a *read* on a wrong-but-
bounded default vault is merely confusing, while a read that followed an
attacker-chosen `dir` could itself be the information disclosure — the
asymmetry that justifies leniency for readers there does not hold here.

**Relationship to DEC-069/070/071 (iter-201) and DEC-091 (iter-220):**
DEC-069/070/071's throughline is "no silent config discard" — a config that
stops applying, or applies with missing pieces, must say so loudly rather
than let a command run quietly degraded. DEC-092 is the same stance applied
to the opposite direction: a config that tries to *apply more than it
should* must refuse loudly rather than be silently honored or silently
narrowed. DEC-091 documented that ancestor-config discovery (DEC-069's case
1, extended by iter-220 to a second entry point) had no boundary check on
what an adopted config's `dir` could say, and named this iteration as where
that boundary would land — DEC-092 is that boundary, and it protects both
discovery entry points identically since the gate lives in
`load_config_from` itself, not in either caller.

**Non-goals, deferred to [[iterations/iteration-222-security-robustness-batch]]:**
alternate data streams are a known gap (M-2), not addressed here. Windows
drive-relative paths (`C:foo` without a root, distinct from the also-rejected
`C:\foo`) turned out to be in scope, not out of it: `Path::is_absolute()`
returns `false` for `C:foo` on Windows, so it is not caught by the
absolute-path check, but `validate_project_local_dir`'s component walk still
refuses it — a `C:foo` value lexes to a single `Prefix` component, which the
walk's catch-all arm treats as an escape (PR #253 review, finding 3). The
broader Windows/ADS hardening iteration 222 was scoped for remains open;
this DEC only corrects an inaccurate claim that `C:foo` specifically passed
through unrefused. Sandboxing hyalo against a fully hostile repo beyond the
write-scope root remains out of scope for a local single-user CLI.

## DEC-093: `--jq` is bounded by a worker-thread wall-clock deadline, not a cooperative step check, because jaq exposes no interruption hook (2026-08-24)

**Decision:** [[iterations/iteration-222-security-robustness-batch]] closes
F3-1: `apply_jq_filter_result` (`crates/hyalo-cli/src/output.rs`) now
compiles and executes a user-supplied `--jq` filter on its own thread and
waits on `JQ_TIME_LIMIT` (3 seconds) via `mpsc::Receiver::recv_timeout`.
Only `filter_code: String`, `value: serde_json::Value`, and the final
`Result<String, String>` cross the thread boundary — a compiled
`jaq_core::compile::Filter` and `jaq_json::Val` both hold `Rc` internally
(`Val::Arr(Rc<Vec<Val>>)`, `Val::Obj(Rc<Map<..>>)`, plus `Rc` inside
jaq-core's own list types) and so are not `Send`; compiling *inside* the
worker thread instead of passing a pre-compiled filter across the channel
avoids ever needing them to be. On timeout the function returns an error
without joining the worker: both call sites (`output_pipeline.rs`, `run.rs`'s
`hyalo config --jq`) format that error and return almost immediately, so the
whole process — worker thread included — is torn down by the OS shortly
after, which is what actually bounds the abandoned thread's resource use,
not anything inside the thread itself. A second, cheap, in-loop guard —
`JQ_MAX_OUTPUT_VALUES` (1,000,000) — caps total emitted *value count*
alongside the pre-existing `JQ_OUTPUT_CAP` (10 MiB emitted *bytes*), so a
filter producing millions of tiny values can't outlast the byte cap.

**Why a thread instead of a step counter:** the two reviewed repros
(`hyalo find --jq 'def f: f; f'` — infinite recursion, never emits a value,
~1.6 MB RSS, pure CPU spin; `hyalo find --jq '[range(3e8)] | length'` —
verified 8.7s / 4.8 GB peak RSS to print one number) both do their unbounded
work *inside a single opaque jaq evaluation step* — `[range(3e8)]` builds
the whole 300M-element array before the interpreter ever yields a value back
to Rust, and `def f: f; f` never yields at all. jaq-core 3.0.0's public API
(checked: no `fuel`/`budget`/`step_limit` type or method in the crate)
offers no way to interrupt mid-step, so a "check the clock between values
pulled from the output iterator" approach — which would be cheaper and finer
grained — cannot catch either repro: the for-loop body in
[`execute_jq_filter`] never runs even once for the first case, and only runs
after the full 8.7s block for the second. A wall-clock deadline on a
separate thread, relying on process teardown to reclaim whatever the
abandoned thread had allocated, was the only mechanism available without
forking jaq or wiring a non-portable OS memory rlimit. Verified empirically
(release build, `[range(3e8)] | length`): unmitigated the query alone costs
8.7s/4.8 GB; with the fix, `hyalo find --jq '[range(3e8)] | length'` returns
a clean error at ~3s with peak RSS around 1.7–2.3 GB (bounded by the
deadline window, not the full computation).

**Deadline choice:** 3 seconds. Measured against real corpora (GitHub Docs,
3,710 files; MDN, 14,394 files) a realistic heavy filter (`map`/`select`/
`sort_by`/`group_by` over every result) added well under half a second of
jq-only time on top of the disk-scan/envelope-build baseline — comfortable
headroom under the deadline. `--jq --help` documents all three limits (time,
value count, byte size) so an agent hitting one gets an actionable message
rather than rediscovering the ceiling by trial and error.

**Non-goals:** the internal, hyalo-authored filter templates used for
`--format text` rendering (`FILE_OBJECT_FILTER` and friends, dispatched via
`lookup_filter`/`build_file_object_filter`) are trusted, reviewed strings —
not attacker/user input — and are called once per rendered item, so they
were deliberately left on the existing cached, unthreaded path
(`apply_jq_filter`/`run_jq_filter_cached`); wrapping every one of those in a
thread spawn would be a real per-item perf cost for no security benefit.

**Follow-up from the PR #254 review round (2026-08-24):** two gaps in the
first cut, now closed or documented:

- A single pathological *value* — `"x" * 2000000000`, ~4.0 GB peak RSS in
  ~1.5s — finished well inside the 3s deadline while defeating both output
  caps, because `execute_jq_filter` only measured a value's size *after*
  `.to_owned()`/`from_utf8_lossy()` had already duplicated it into a second
  multi-GB buffer. Fixed by checking a `Val::TStr`/`Val::BStr`'s raw byte
  length against `JQ_OUTPUT_CAP` before any copy — the value still borrows
  from the interpreter's own `Val` at that point, so the check is free.
  Measured: peak RSS for the same repro dropped from ~4.0 GB to ~2.0 GB
  (jaq's own internal string-repeat allocation is the residual cost; nothing
  on our side can intercept construction *inside* a single interpreter
  step, per the reasoning above). No equivalent pre-check exists for a
  large *non-string* value (`Display`'s `to_string()` gives no length
  preview before building the whole formatted string) — that residual case
  is bounded only by `JQ_TIME_LIMIT`, documented as such rather than
  silently claimed to be covered.
- **Known, accepted, unfixed gap:** `def f: [f]; f` overflows the native
  thread stack and hits Rust's abort-on-stack-overflow guard (`exit 134`),
  killing the whole process before the deadline (or anything else) ever
  gets a chance to run. This is not a regression introduced by running the
  filter on a separate thread — the identical filter overflows any thread's
  stack, worker or main, since jaq's recursive evaluation has no depth
  limit either — and there is no user-space hook that intercepts a stack
  overflow before the OS/runtime aborts the process. Documented on
  `JQ_TIME_LIMIT`'s doc comment rather than left as a silent gap; no fix is
  planned (would require jaq upstream to add its own recursion-depth guard,
  or evaluating on a growable/guarded stack, both out of scope here).

## DEC-094: `task toggle --section` refuses an ambiguous multi-heading match instead of silently applying to all of them (2026-08-24)

**Decision:** [[iterations/iteration-223-query-output-correctness]] closes
F-1: `resolve_task_lines` (`crates/hyalo-cli/src/commands/tasks.rs`) now
scans the target file's outline via `SectionScanner` +
`hyalo_core::heading::build_section_scope` *before* resolving `--section` to
task lines. When the filter matches more than one distinct heading instance
(e.g. two `## Tasks` headings under different ADRs), the command bails with
an error naming every matched heading's line number and suggesting `--line`,
rather than toggling every task under every match. A single matching
heading — even one with several tasks under it — is unaffected. This
applies to all three `--section` consumers that share `resolve_task_lines`
(`task read`, `task toggle`, `task set`), so the read path also refuses an
ambiguous selector rather than silently reading from an arbitrary one of the
matches.

**Why refuse rather than an opt-in flag:** the review offered two options —
refuse by default, or require an explicit `--all-sections`/`--nth` flag. The
`links` command already establishes the vault's precedent for a selector
matching more than one thing: it reports `ambiguous: N` rather than picking
one silently. `task toggle` writes immediately with no dry-run gate by
default, so an over-broad `--section` match is not just a wrong *read* but
an actual mutation of tasks the user never intended to touch — the same
asymmetry DEC-092 used to justify a stricter stance for writers than
readers. An opt-in `--all-sections` flag would still require the same
detection logic to decide when to suggest it, so refusing by default is not
meaningfully more expensive to implement, and it fails safe: a vault that
happens to reuse a heading name never has its tasks silently over-toggled,
even before its owner learns the flag exists.

**Why detect via `build_section_scope` instead of comparing task-section
text:** the naive fix — count distinct `t.section` strings among matched
tasks — cannot distinguish two *different* headings that happen to share
text (the exact case this fix targets) from one heading with many tasks
under it, since both produce one distinct string. Reusing
`build_section_scope` (already the mechanism `find --section` uses to turn a
filter into heading-anchored line ranges) counts *heading occurrences*
directly — one `SectionRange` per matching heading in the outline,
regardless of how many or how few tasks sit under each — which is the
correct unit of ambiguity here.

**Asymmetry with `find --section` (review round, 2026-08-24):** `find
--section` uses the same `build_section_scope` primitive as this fix but
does the opposite thing on an ambiguous multi-heading match: it unions every
matched heading's scope within a file, rather than refusing. This is
deliberate, not an oversight. `find` is a vault-wide, read-only query —
different files legitimately have different heading structures, and there
is no single "the" match to disambiguate against the way there is for a
selector scoped to one file's mutation. Refusing per-file would mean one
file with a duplicate heading silently drops out of a `find --section`
result set entirely (or, worse, aborts a vault-wide query over one
unrelated file), which is a worse failure mode for a search command than
the union it already produced correctly before this iteration. The
asymmetry is intentional: mutation commands (`task toggle`/`read`/`set`)
refuse because an over-broad match risks writing to (or reading from) the
wrong place with no dry-run gate; `find` unions because there is no "wrong
place" for a read spanning the whole vault, only a broader result. A
one-line stderr summary ("`--section` matched more than one heading in N
file(s)") was added so the union isn't silent, without a per-file note that
would spam a large result set. See `find --help` (`FindFilters::sections`
doc comment and the `FILTERS` section of `find`'s `long_about`) for the
user-facing statement of this asymmetry.

## DEC-095: BM25 CJK tokenization uses overlapping character bigrams, with a per-index `tokenizer_version` so a stale persisted index falls back to live re-tokenization instead of serving unmatchable results (2026-08-24)

**Decision:** [[iterations/iteration-223-query-output-correctness]] closes
F-2: `bm25::tokenize` (`crates/hyalo-core/src/bm25.rs`) now detects
scriptio-continua runs — CJK ideographs (including compatibility/extension
blocks), Hiragana, Katakana, Hangul syllables, by codepoint range — and
tokenizes them as overlapping character bigrams instead of one whole-run
token. Previously `text.split(|c| !c.is_alphanumeric())` treated an entire
whitespace-free CJK run as a single token, so `hyalo find 日本語` returned
zero hits on a file containing the query verbatim: the module claimed
"Unicode-aware" tokenization but was Unicode-*safe*, not CJK-*aware*. The
same `tokenize()` function tokenizes both documents and queries, so the fix
is symmetric with no separate query-side change needed; a plain (unquoted)
multi-bigram CJK query becomes several `Must` clauses ANDed together
(bag-of-bigrams, not phrase-adjacent), which is looser than true
segmentation but sufficient to make substring queries match — the
documented, accepted tradeoff (see Non-goals below).

**Tokenizer versioning:** `bm25::TOKENIZER_VERSION` (currently 2) is a new
constant bumped whenever `tokenize()`'s output for the same input changes.
Both `IndexEntry.bm25_tokenizer_version` (per-file pre-tokenized data,
`crates/hyalo-core/src/index.rs`) and `Bm25InvertedIndex`'s own
`tokenizer_version` field (the persisted, pre-built inverted index) carry
this version, defaulted via `#[serde(default)]` so a MessagePack snapshot
written before either field existed deserializes as version `0` — which
never equals the current version. `find`'s corpus-building code
(`crates/hyalo-cli/src/commands/find/mod.rs`) checks both: the "fastest
path" (`index.bm25_index()`) is only used when its `tokenizer_version()`
matches current, and the per-entry pre-tokenized fast path is only used
when `entry.bm25_tokenizer_version` matches current — both gates mirror the
pre-existing language-mismatch gate that already falls through to a live
disk read + fresh `tokenize()` call. A snapshot built by a pre-F-2 hyalo
binary therefore does not need to be manually rebuilt for correctness: BM25
search transparently degrades to live-scan speed on that snapshot (until
the next `create-index`), rather than silently continuing to serve results
computed by a tokenizer that can never match a CJK query. No hard version
check or forced rebuild was added — the existing snapshot-tolerance
philosophy (structural fields use `#[serde(default)]`, never a hard load
failure) extends naturally to a *semantic* version mismatch the same way.

**Why bigrams over the alternatives:** the review's fix ladder was (a)
bigram indexing, (b) substring fallback when BM25 returns nothing for a CJK
query, (c) documentation only. Bigram indexing was chosen as the "cheapest
sufficient" option per the plan: it fixes the index itself rather than
papering over empty results with a second search pass, needs no additional
runtime cost on the query path (same `tokenize()` call), and needs no new
dependency (no CJK segmentation library). Full morphological segmentation
(MeCab/Jieba-class tooling) was explicitly out of scope (see Non-goals).

**Performance:** the ASCII fast path (`text.is_ascii()` short-circuit,
checked before any per-run CJK classification) is byte-identical to the
pre-fix pipeline — verified both by a dedicated unit test
(`test_tokenize_ascii_fast_path_unaffected_by_cjk_handling`) and by an A/B
timing run of `hyalo find` against the GitHub Docs corpus (3,710 English
`.md` files, live body scan, 3 runs each): baseline (pre-fix, via `git
stash`) 0.94–1.43s real / ~1.0–1.06s user; post-fix 0.99–1.45s real /
~1.02–1.06s user — within noise, no regression. This matters because
`bm25_tokenize` was previously a scan-perf hotspot (iter-158's H-8 fix); F-2
does not reopen that regression for the common (ASCII) case.

**Non-goals (explicit, matching the iteration plan):** full CJK
morphological segmentation is out of scope — bigrams are an approximation
that can over-match (two bigrams from unrelated parts of a longer run can
both appear in an unrelated document) but never under-match a real
substring query, which is the correct direction to err for a search tool.
BM25 ranking-math correctness beyond tokenization (IDF/length normalization)
is unchanged and out of scope, per `deep-analysis-2`'s own scope note that
the ranking math was separately verified sound. A single-character CJK
query (e.g. searching for one kanji) cannot match a longer run: bigram
indexing has no unigram entries except for a run that is itself exactly one
character, so a one-character query tokenizes to a unigram that was never
indexed for any longer run containing it. This is a pre-existing limitation
of bigram-only indexing (not introduced or worsened by this fix — the
pre-F-2 whole-run-token approach couldn't match single-character queries
either, just for a different reason) and a real gap since single-kanji
search is a common query shape in practice; accepted for this iteration,
not fixed (would require unigram indexing alongside bigrams, which roughly
doubles the CJK posting-list size for a query shape that's a minority of
real CJK searches).

**Segmentation-boundary rule and mixed-script fix (review round,
2026-08-24):** the first cut of this fix classified an entire alphanumeric
*run* as bigram-mode or word-mode by "does the run contain any
scriptio-continua character" — checked once per run, not per character.
That broke on a no-separator mixed run, the ordinary shape of real CJK
technical writing (`日本語Docker入門ガイドです`, Japanese prose with an
inline Latin product name and no space around it): the whole run, Latin
substring included, was forced into character bigrams, fragmenting `Docker`
into unmatchable pieces (`語D`, `ck`, `er入`) — worse than before the CJK
fix, which at least kept the whole run as one exact (if CJK-unmatchable)
token, so `Docker` was findable pre-fix and unfindable immediately after
it. Fixed by segmenting each run at every scriptio-continua /
non-scriptio-continua character boundary *before* choosing a tokenization
strategy, then applying bigrams or the word pipeline to each segment
independently (`tokenize_run` in `bm25.rs`). The segmentation rule is
narrow and deliberate: a boundary is drawn only between a scriptio-continua
character and a non-scriptio-continua one — never between two different
scriptio-continua scripts (Han vs. Hiragana vs. Katakana vs. Hangul). A
Kanji run immediately followed by Hiragana (`日本語です`, the ordinary
shape of a Japanese sentence, since Japanese freely interleaves Kanji and
Hiragana within one semantic word-run with no internal separator) stays in
one bigram segment, producing a bigram that straddles the Han/Hiragana
boundary (`語で`) rather than artificially cutting the segment there. Both
directions are now tested: `Docker` and `日本語` are each independently
searchable in the mixed run, and a Han/Hiragana-straddling bigram is
asserted directly.

## DEC-096: schema `RawPropertyConstraint` denies unknown TOML keys AND implements `minimum`/`maximum` — both, not either/or (BREAKING) (2026-08-24)

**Decision:** [[iterations/iteration-223-query-output-correctness]] closes
F3-3: `RawPropertyConstraint` (`crates/hyalo-core/src/schema.rs`) now carries
`#[serde(deny_unknown_fields)]`, and gains `minimum: Option<f64>` /
`maximum: Option<f64>` fields, restricted to `type = "number"` properties
(the same rejection pattern already used for `min-length`/`max-length` on
`string`). `PropertyConstraint::Number` changed from a unit variant to
`Number { minimum: Option<f64>, maximum: Option<f64> }`; `hyalo lint`'s
`validate_constraint` enforces both bounds (inclusive) when configured.
Previously `type = "number"` with `minimum = 1` / `maximum = 5` in
`.hyalo.toml` silently validated a `priority: 99` file as clean — the keys
were parsed into nothing, since `RawPropertyConstraint` only captured the
six fields it happened to define and dropped everything else via ordinary
serde struct deserialization.

**Why both fixes, not one:** the review offered "implement minimum/maximum"
OR "deny unknown fields" as alternatives, preferring both. Implementing only
`minimum`/`maximum` would still silently drop the *next* plausible-but-
unsupported key (a typo like `patterns` for `pattern`, or `default` inside a
constraint block instead of the type-level `[schema.types.*.defaults]`).
Denying unknown fields without implementing `minimum`/`maximum` would turn
today's silent no-op into a loud error, which is strictly better, but would
leave the two JSON-Schema-shaped names a config author reaches for first
still unimplemented — the module's own doc comment already states the
opposite philosophy ("misconfigured TOML surfaces as an error rather than
silently discarding the configured values"), so an unenforceable but
accepted key would be exactly the failure mode being fixed. Doing both
closes the hole from two directions: real constraints work, and anything
still unrecognized is a hard error instead of a silent drop.

**Breaking-change handling:** `deny_unknown_fields` rejects any
`.hyalo.toml` that happened to carry an unused key in a
`[schema.types.*.properties.*]` block — intentional or accidental. This is
not a hard failure at the CLI level: `parse_schema_from_toml`
(`crates/hyalo-cli/src/config.rs`) already treats any `[schema]`
deserialization error as "malformed config" — it emits a `warn::warn` and
falls back to an empty `SchemaConfig` (no validation for that run) rather
than aborting the command, consistent with the existing malformed-schema
handling for every other kind of TOML mistake in this block (DEC-070's "no
silent config discard" stance: loud warning, graceful degradation, never a
silent no-op and never a hard abort for a single misconfigured block). A
vault carrying a stray key therefore loses schema *validation* on upgrade,
with a warning naming the problem, rather than losing the command entirely
— but that is still a real behavior change worth flagging as BREAKING in
the changelog, since a vault that depended on schema enforcement silently
loses it until the key is fixed.

**Follow-up: `lint` must surface a malformed schema as a lint-level
problem, not just a stderr warning (review round, 2026-08-24, finding 2):**
the graceful-degradation stance above (loud warning, empty `SchemaConfig`,
command proceeds) had a real gap on `hyalo lint` specifically: the warning
is `-q`-suppressible, and — worse — `lint --strict` printed a clean "no
issues" and exited 0 on a file carrying a genuine schema violation the
(silently disabled) schema would have caught, because *every* type's
schema is disabled by one bad key anywhere in the `[schema]` block. A CI
gate relying on `lint --strict` would pass while validation was silently
off. Fixed by extracting the parse-or-diagnose logic into
`config::try_parse_schema_from_toml` (returns `Result` instead of
warning-and-defaulting) and adding `lint::validate_schema_config`, which
calls it independently and — mirroring the existing `validate_views`
pattern of representing a config-level problem as a violation on a
`.hyalo.toml` pseudo-file — turns a parse failure into a visible SCHEMA
violation in the lint results: `Warn` severity by default (so `--format
json` shows it in `results` without failing plain `lint`), promoted to
`Error` directly (not via the later strict-promotion pass the per-file
`schema/*` violation kinds use) so `lint --strict` exits non-zero and
names the bad key. This is the review's option (a), the stated minimum
bar. Option (b) — scoping the degradation to only the offending
type/property block, so *unaffected* types keep validating — was not
implemented: the current `RawSchemaConfig`/`TryFrom` architecture
deserializes and validates the whole `[schema]` block as one atomic step
(a single `val.clone().try_into::<RawSchemaConfig>()` covering every
nested type/property), so isolating one bad block would need restructuring
that deserialization to catch errors per `[schema.types.*]` entry
independently and keep the successfully-parsed ones — a moderate
refactor, not the "cheap, do in addition" case the review allowed for.
Left as a documented follow-up, not attempted here.

## DEC-097: the lexical no-`..` rejection gets an honest, distinct message and error variant instead of reusing "outside vault boundary" (2026-08-24)

**Decision:** [[iterations/iteration-223-query-output-correctness]] closes
F3-4: `resolve_file_ci` (`crates/hyalo-core/src/discovery.rs`) now checks
`has_parent_traversal` *separately* from the other lexical escape checks
(absolute path, `is_absolute()`, Windows drive-relative / NTFS-ADS colon),
and reports it via a new `FileResolveError::ParentTraversal` variant instead
of folding it into `OutsideVault`. Previously, from a vault subdirectory,
`hyalo read ../broken.md` — naming a file squarely *inside* the vault —
returned "file resolves outside vault boundary", which is false: the path
was never resolved before being rejected, so the claim about where it
"resolves" to was never checked. `ParentTraversal`'s `Display` says
"paths must be vault-relative without '..' components", and
`resolve_error_to_outcome` (`crates/hyalo-cli/src/commands/mod.rs`) attaches
a matching hint. The other three lexical conditions, and the genuine
symlink-escape case (`ensure_within_vault` returning `false` after actually
canonicalizing), keep `OutsideVault` — those really are "the path resolves
(or is) outside the vault," an accurate claim in every one of those cases.

**Why a new variant instead of resolve-then-check:** the review offered two
options — accept in-vault `..` paths by resolving before checking
(`fs_util::escaping_write_target`-style canonicalize-then-compare), or keep
the lexical rule and fix the wording. Resolve-then-check was rejected: the
lexical no-`..` gate is a foundational assumption several rounds of prior
security hardening were built on top of (iter-202's write-boundary unifier,
iter-221's ancestor-config `dir` boundary, iter-222's Windows drive-relative
/ NTFS-ADS closes) — auditing every one of those call sites to confirm none
of them implicitly depend on "a `..`-bearing path is *always* rejected
before any resolution happens" was out of scope for a LOW-severity message-
wording bug, and getting that audit wrong would trade a cosmetic false
positive for a real boundary weakening. The plan's own guidance named this
exact tradeoff and preferred the rewording unless resolve-then-check could
be shown airtight everywhere `resolve_file` is used — it wasn't attempted,
so rewording is the fix. The *policy* is unchanged: `..` is still never
accepted, in-vault destination or not; only the explanation of *why* is now
accurate.

## DEC-098: scale regression gate is an on-demand `xtask bench-scale`, not a per-PR CI check (2026-08-24)

**Decision:** [[iterations/iteration-224-test-quality-hardening]] T-6 adds
`cargo run -p xtask -- bench-scale` (`crates/xtask/src/bench_scale.rs`): it
generates a deterministic ~14,000-file synthetic vault (every file's content
is a pure function of its index — no RNG seed to manage, byte-identical
across runs) and times `hyalo find --format json` and `hyalo links fix
--format json` against it, three repetitions each, comparing the median to a
fixed wall-time budget. `find`: 3s budget (measured baseline ~0.44s on Apple
Silicon, ~7x headroom). `links fix`: 15s budget (measured baseline ~3.45s,
~4x headroom — lower multiple because `links fix` does real cross-file
resolution work that scales with corpus size, so its baseline itself already
reflects the thing being gated, unlike `find`'s largely I/O-bound walk).
This is deliberately **not** wired into `ci.yml`: it runs on demand only.

**Why on-demand, not CI:** two costs, neither of which buys much given what
the gate can and can't catch. Runtime cost — generating and linting a
14k-file vault adds real wall-clock (~15-20s total) to every PR for a class
of regression (gross O(n²) blowups, a dropped index fast-path) that's rare
between PRs and easy to catch in a manual pass before a release. Flake
risk — shared CI runners have enough single-core variance that a budget
tight enough to catch a real regression would also intermittently fail on
runner noise, and a budget loose enough to never flake would be too loose to
catch anything but a catastrophic (10x+) regression anyway — at which point
an on-demand run before cutting a release catches it just as well without
spending everyone's PR-cycle budget on it. `bench-e2e.sh` (hyperfine-based,
needs an external `HYALO_BENCH_VAULT`) already established the project's
existing convention of keeping detailed perf work manual; this follows it.

**What this gate does not cover** (documented in `bench_scale.rs`'s own
module doc so it isn't mistaken for full perf coverage): the fuzzy-candidate
matching perf debt tracked separately in
[[iterations/iteration-206-links-perf-profiling]]; sub-command timing
breakdowns or memory usage; anything below an order-of-magnitude regression,
by design (the headroom that keeps this flake-free on a slow runner also
means it can't catch a 2x slowdown). `bench-e2e.sh` remains the tool for
detailed A/B comparisons against a real-world vault.

## DEC-099: a templated heading makes every anchor into that file unknowable, not broken (2026-08-24)

**Decision:** [[iterations/iteration-215-anchor-and-broken-links-followups]]
extends `anchor::fragment_matches_headings` with a final escape hatch: when
neither DEC-060 (raw heading text) nor DEC-075 (rendered GitHub slug) matches
a fragment, and *either* side of the comparison carries a template marker
(`{%`, `{{`, `${`), the fragment is treated as matching rather than reported
as a dead anchor. "Either side" means the fragment itself
(`[x](f.md#{{anchor}})`) **or any heading in the target file**. The marker
test is `anchor::is_templated_heading`, a delegating wrapper over iter-207's
`link_fix::is_templated_target`, so the heading-side and destination-side
rules cannot drift apart.

**Why:** a Liquid heading — `## {% data variables.product.prodname_pro %}` —
renders to `## GitHub Pro` and anchors as `#github-pro`. hyalo only ever sees
the pre-render source, so `github_slug` produces `-data-variables…`: a slug no
author ever writes and no fragment can ever match. Every *correct* anchor into
such a heading was therefore reported broken. iter-211 fixed the same class of
false positive for slug spellings (DEC-075) and recorded this remainder as a
known limitation; on the GitHub Docs corpus it is the residual noise that
survived DEC-075.

**Why file-wide, not per-heading:** a templated heading's rendered slug is
unknowable, so *any* templated heading in the target file could be the one a
given fragment names — there is no way to pair a specific fragment with a
specific templated heading. Scoping the skip to the file is the only sound
option short of rendering Liquid, which hyalo will not do. The cost is a
missed dead anchor in files that use templating; the alternative cost is every
anchor into such a file reported broken, which is what made hyalo unusable on
templated corpora. This follows the module's stated bias, unchanged since
DEC-060: a false positive costs a user far more than a missed exotic spelling.
A file with no templated heading is completely unaffected.

**Blast radius:** every caller routes through the one matcher, so
`find --broken-links`, `summary`'s `links.broken_anchors`
(`link_fix::count_broken_anchors`) and the `links fix` broken-anchor note move
together — the NEW-15/UX-2 agreement between those numbers is preserved by
construction.

## DEC-100: `LinkInfo` carries `line`, and links are emitted in document order (2026-08-24)

**Decision:** [[iterations/iteration-215-anchor-and-broken-links-followups]]
adds an always-present `line: usize` (1-based) to `LinkInfo`, the shape behind
`find --fields links` and `find --broken-links`. The field is named `line` —
the same name and the same meaning every other line-bearing shape in
`.results` already uses (`BacklinkInfo`, `OutlineSection`, `ContentMatch`,
`TaskInfo`): a 1-based source line, never an index or a byte offset. The
per-file link list is sorted by line, so same-file anchors (chained from
`IndexEntry::self_anchors`, previously always appended last) now interleave in
document order.

**Why:** dogfood UX-6 — `find --broken-links` listed every link of a matching
file with no location, so locating the one that was actually broken meant
grepping the file. `hyalo lint` (HYALO006) and `backlinks` already reported a
line for the same links; `find` was the outlier. The value is taken from
`IndexEntry::links` / `::self_anchors`, which already stored it, so there is
no extra file read and the `--index` and disk paths agree by construction.

**Why not a new name (`source_line`, `at`):** the plan explicitly required not
silently reusing a name already used differently elsewhere in `.results` —
`line` is used identically everywhere it appears, so reuse is the consistent
choice rather than the risky one.

**Shape impact:** this is an additive JSON change on a field that is always
present, which shifts `LinkInfo`'s key signature and therefore its text-output
filter dispatch in `output.rs`. Both the pre- and post-215 signatures are
listed there, and the filters fall back to `line 0` (no real line is 0), so a
`LinkInfo` deserialized from an older snapshot still renders instead of
dropping to generic key/value formatting. Text output gained a `line N:`
prefix on each link.

## DEC-101: pi integration distributed as a git-installable pi package, template copy eliminated (2026-08-25)

**Decision:** the pi extension and skills live in a top-level `pi-package/`
directory that is a valid pi package — `pi install git:github.com/ractive/hyalo`
— and simultaneously the single source of truth for the hyalo binary: the
`include_str!` constants in `init.rs` point directly at the `pi-package/`
files. The duplicate copies under `crates/hyalo-cli/templates/` were removed.

**Why:** the 2026-08-24 dogfood showed the failure mode of the old model —
users of a broken extension stayed broken until they upgraded hyalo *and*
re-ran `hyalo init --pi`. A git package source lets `pi update --extensions`
deliver extension fixes independently of hyalo releases. Keeping two copies
(template + package) would invite drift, so the binary embeds the package
files directly; drift is impossible by construction.

**Vendored fallback:** `hyalo init --pi` still writes the embedded copies
into `.pi/` for users who don't want a git dependency, and now prints a
one-time hint recommending the package install. It writes byte-identical
content because it is the same file at compile time.

**Versioning policy:** the package carries its own `version` in
`pi-package/package.json` (0.1.0), bumped with a CHANGELOG entry on every
extension/skill change. Git refs are pinned by pi at install time; moving to
a new ref is an explicit `pi install git:...@new-ref` (verified against the
installed pi docs — `pi update` reconciles but never moves pinned refs).
Main-branch HEAD is acceptable as the ref for now; a tag strategy is a
carry-over. Minimum-hyalo-version requirements are documented in
`pi-package/README.md` (typed tools need ≥ 0.21).

## DEC-102: `--filenames0` emits NUL-terminated bytes via a `RawBytes` outcome; `--iteration` extends the shared input resolver (2026-08-25)

**Decision:** two iter-238 ergonomics follow-ups:

1. **`find --filenames0` writes NUL-terminated paths byte-exactly.** The
   projection produces a new [`CommandOutcome::RawBytes`] variant that
   bypasses both the JSON pipeline and the control-character sanitizer
   (`sanitize_control_chars` would strip NUL, defeating the whole flag), and
   the output pipeline writes it verbatim with no appended newline — GNU
   `find -print0` precedent. Only hyalo-generated content (vault-relative
   paths) may use `RawBytes`; raw file *body* text stays on `RawOutput`,
   which keeps its ANSI-stripping sanitization. Each path is terminated
   (not just separated) so the last entry is complete for `xargs -0`.

2. **`--iteration <ID>` joins `InputSelection`, resolved at dispatch time**
   via `iteration::selection_with_iteration_resolved`, which rewrites the
   selection to carry the matched file as an ordinary `--file` value after
   enforcing the exactly-one-match contract (same errors as
   `set --iteration`). Every single-file command built on the shared input
   resolver gets natural-key addressing in one place — `read`, `backlinks`,
   and all three `task` actions — instead of five hand-rolled copies.

**Why:** the ralph-loop workflow reads iteration plans and ticks their tasks
every run, so natural-key addressing belongs on `read`/`task`, not only on
`find`/`set`. Centralizing resolution in one helper means the next command
that adopts `InputSelection` inherits `--iteration` for free rather than
re-implementing (or forgetting) the exactly-one-match error contract.

**Alternatives rejected:** plumbing the schema into `resolve_inputs`
(signature churn across ~20 test call sites for no behavioral gain) and
keeping NUL output inside `RawOutput` with an exemption carved into
`sanitize_control_chars` (would let NUL through every text-mode consumer,
including `read` of files whose bodies contain NULs).

## DEC-225: thin dispatch, argv-based `HintBuilder`, hyalo-core façade (2026-08-25)

**Decision:** three architecture cleanups from
[[reviews/deep-analysis-2-2026-08-23]], all pure structure — no behavior change
(the e2e suite, 1737 tests, is the guard):

1. **ARCH-1 — one handler per command.** Every `dispatch.rs` match arm with
   business logic now delegates to `commands::<cmd>::run(ctx, args)` in the
   command's own module (`find::run`, `lint::run`, `task::run`, `views::run`,
   `mv::run`, `set::run`, plus read/properties/tags/summary/backlinks/
   remove/append/links/changelog/okf/madr/types/lint-rules). `dispatch.rs`
   shrank from **3024 to 876 lines**; the `dispatch` match itself only
   destructures clap variants and forwards. Shared helpers (`resolve_index`,
   `maybe_case_index`, `patch_index_for_modified_files`, …) stay in
   `dispatch.rs` as `pub(crate)` and are re-exported to handlers. The win is
   testability: e.g. `find`'s `--strict` exit-code policy is now the pure
   function `find::run::strict_exit_code`, unit-tested in-process
   (`strict_exit_code_policy`) where it previously required an e2e process
   spawn to observe.

2. **ARCH-4 — hints are argv, not strings.** New
   `hints::HintBuilder::cmd("task toggle").flag_value("--status", "?")` builds
   the command as an argv vector serialized through the existing
   `shell_quote`, with `argv()` exposing the vector so tests can feed it to
   the real clap parser (`hint_builder_commands_parse`). All `build_command_*`
   family members, the config hints, and `profile_lint_hint` now route
   through it. A drift guard, `no_raw_hyalo_command_literals`, fails the
   suite when a new hand-written `"hyalo …"` string literal appears in
   non-test source — the `tags --limit 0` class of bug (a hint that reads
   fine but doesn't run) can no longer be introduced.

3. **ARCH-5 — hyalo-core is a curated façade.** Plumbing modules (`util`,
   `common_words`, `case_index`, `fs_util`; `warn` already was) are now
   `pub(crate)`, with the specific items the CLI/mdlint consume re-exported
   at the crate root (`CaseInsensitiveIndex`, `atomic_write_within`,
   `is_common_word`, `levenshtein`, …). Internal refactors of those modules
   are no longer semver-relevant; the supported surface is documented in the
   crate root doc. Call sites were rewritten from
   `hyalo_core::case_index::X` to `hyalo_core::X`.

**Where a new command goes now:** add the clap variant in
`cli/args.rs`, then implement `commands/<cmd>.rs::run(ctx, args) -> Result<CommandOutcome>`
and make the `dispatch.rs` arm a one-line forward. Hint commands are built
with `HintBuilder`, never `format!("hyalo …")`.

## DEC-226: lint subsystem lives in hyalo-mdlint; index maintenance goes through one MutationJournal (2026-08-25)

Two structural decisions from the 2026-08-23 deep analysis
([[reviews/deep-analysis-2-2026-08-23]]), implemented in
[[iterations/iteration-226-arch-lint-crate-index-journal]]:

**ARCH-2 — lint crate boundary.** The hidden lint subsystem that lived in
`hyalo-cli/src/commands/` moved into `hyalo-mdlint`:

- the five profile linters (`changelog_lint.rs`/`madr_lint.rs`/`okf_lint.rs`/
  `skills_lint.rs`/`lint_github.rs`) are now
  `hyalo_mdlint::profiles::{changelog, madr, okf, skills, github}`;
- their shared engines (`heading_grammar.rs`, `link_lint.rs` =
  HYALO006's context, `section_scanner.rs`) moved with them;
- the schema-validation core of the 5,100-line `commands/lint.rs` (types,
  violation kinds, `lint_file`/`lint_file_with_fix`, auto-fix computation,
  the `validate_constraint` family) is now `hyalo_mdlint::schema`, exposing
  an **in-process API** (`lint_file`, `lint_counts_only`,
  `validate_constraint_simple`, …) that library consumers and the test suite
  can drive without spawning a CLI process. `commands::lint` re-exports the
  items so existing call sites are unchanged, and CLI output stays
  byte-identical. The CLI keeps flag parsing, profile selection and output
  formatting only.

**ARCH-3 — index refresh is a property of the write path.** The three
coexisting index-refresh mechanisms (`mutation::save_index_if_dirty` with 8
call sites, `tasks.rs`'s local `patch_index`, and
`patch_index_for_modified_files`) are gone, folded into one
`commands::journal::MutationJournal`. It borrows the command context's
snapshot index for the duration of a mutating command, tracks dirtiness
itself, always refreshes the entry *and* the persisted link graph, and is
flushed exactly once at the end. Every mutating command
(`set`/`remove`/`append`/`new`/`mv`/`task toggle|set`/`properties rename`/
`tags rename`/`links fix|auto --apply`/`lint --fix`) goes through it. Two
bonus fixes of the stale-graph bug class fell out:
`properties rename` and `tags rename` previously patched only entries,
never the link graph — frontmatter link properties (`related`,
`depends-on`) renamed under `--index` used to leave the persisted graph
stale.

**Guard:** the `xtask check-mutation-journal` gate fails CI when (a) a
pre-journal persistence token reappears outside the sanctioned files, or
(b) a mutating command module stops referencing `MutationJournal`. The
stale-link-graph regression (`index.rs:439`'s recorded bug class) is pinned
by e2e tests in `tests/e2e/index_journal.rs` that mutate via each path and
assert the persisted graph is current.

## DEC-240: MutationJournal upserts unknown files; JSON `applied` keeps its "apply mode" meaning (2026-08-27)

Two follow-ups from the independent review of iter-225/226 recorded in
[[iterations/iteration-240-review-followups-bugfixes]] (code: PR #275).

**Journal refresh is an upsert, not a patch.** DEC-226 made every mutating
command go through `MutationJournal`, but its `update_entry`/`update_task`/
`rescan_modified` still guarded on "entry already present" — a file created
after `create-index` (or by anything other than `hyalo new --index`) was
written to disk and then silently dropped from the persisted index, so
`find --file <it> --index` returned nothing. The `--index` help promises the
index is "patched in-place, keeping it current"; a write path that can leave
the index missing the file it just wrote violates that. Decision: when the
mutated file is not in the index, the journal inserts it from a fresh disk
scan **and** registers its outbound edges in the persisted link graph
(`SnapshotIndex::insert_or_replace_entry_with_links`). The older
`insert_or_replace_entry` deliberately leaves the graph alone (it serves
`hyalo new`, whose body has no links yet) and stays as-is. Body text still
enters the BM25 index only on the next full `create-index`, unchanged from
DEC-226.

Rejected alternative: refusing the mutation with "file not indexed, run
create-index" — safe but hostile; the journal already has everything it needs
to do the right thing in one scan.

**`applied` in `links fix` JSON stays "apply mode was used".** The dogfood
session read `applied: true` with `fixes: 0` as a false success. The iter-216
D-4 contract defines `applied` as the mode flag, and consumers key off the
`fixes`/`applied_fixes` counts for "did anything land". Changing the boolean's
meaning would break that contract for no information gain; instead the *text*
summary — the only place the two were indistinguishable — now prints
`Applied: yes (N fixes)`. **Still open:** the journal trusts the loaded index
as ground truth, so `links fix --apply --index` on a vault edited externally
since `create-index` reports `broken: 0` without warning. Detection (mtime
comparison vs. warning vs. refuse) is a separate decision, carried over in
iteration 240's out-of-scope list.

## DEC-241: stale-index detection for `links fix`/`links auto` is a per-entry mtime check + rescan, not a refusal (2026-08-27)

BUG-2's detection half from
[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]],
implemented in [[iterations/iteration-241-stale-index-detection-and-ux-fixes]].

**The problem.** The load-time staleness probe
(`newest_shallow_dir_mtime`, M-6) only sees directory mtimes, which move on
file create/rename/delete — an *in-place edit* of an indexed note leaves
every directory mtime untouched, so a `links fix --apply --index` run could
report `broken: 0` for a link added seconds earlier and exit 0 with no
warning. DEC-240 closed the same bug's `applied`-semantics half; this is
the detection half.

**The decision.** Before `links fix`/`links auto` run their discovery pass
against a loaded snapshot, compare every indexed entry's stored `modified`
(ISO 8601, written at scan time) with the file's current disk mtime — one
`stat` per entry, no content read, with the same 1-second tolerance as the
shallow probe. Drifted files are refreshed from disk through the existing
`MutationJournal::rescan_modified` (entry + link graph, DEC-226's path),
then discovery sees current bodies. A warning names the drift ("index is
stale: N file(s) changed on disk since create-index"). The refresh is
persisted under `--apply` (the run already writes) and in-memory only for a
dry run, so a preview never mutates the snapshot file.

**Why not refuse.** Refusing with "re-run create-index" is safe but hostile
to the agent-CLI use case — the journal already has everything needed to
do the correct thing in one per-file scan, and the run remains correct
without a full rebuild. A warning-only alternative (no rescan) was rejected
because it leaves the actual failure intact: `broken: 0` with a warning is
still the wrong answer.

**Scope.** Only the `links` discovery pass gets the mtime check — it is the
one read path whose *results* are computed from indexed link data. Files
created or deleted after `create-index` remain the shallow probe's job
(warn at load); a full-walk reconciliation is `create-index`. Other
mutating commands (`set`/`append`/…) write the file they mutate, so their
journal refresh already sees current disk state — no check needed there.
