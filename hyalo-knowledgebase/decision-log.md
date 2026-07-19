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
