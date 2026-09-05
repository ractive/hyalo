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

**Correction + tag strategy (resolved 2026-08-27, iter-239):** the original
premise "a top-level `pi-package/` directory … is a valid pi package" was
half right: pi only reads the **clone root** for a `package.json` manifest
or convention directories, so the manifest at `pi-package/package.json` was
invisible to `pi install git:…` — the package registered but loaded zero
extensions/skills. Fixed with a root `package.json` manifest pointing into
`pi-package/` (single source of truth unchanged, verified live: all five
tools + skills load from the global git install, and a pushed change was
delivered by `pi update --extensions`).

**DEC-101 carry-over decision — tag pinning:** releases are tagged with the
hyalo release tags (`vX.Y.Z`), and the README recommends installing with a
tag ref (`pi install git:github.com/ractive/hyalo@v0.20.0`) over main HEAD.
Rationale: the extension is a thin CLI wrapper whose expected output shapes
track the binary, so pairing the package ref with the installed hyalo
version avoids extension/binary drift; main HEAD remains the living-edge
option. Because `pi update` reconciles but never moves pinned refs, moving
to a new release is an explicit `pi install git:…@vX.Y.Z` — that pins to
the release that matches the binary the user has, by design.

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

## DEC-242: `--iteration <ID>` natural-key addressing is removed, not extended (2026-08-27)

Implemented in [[iterations/iteration-242-remove-iteration-flag]], reversing
iter-235 (and its iter-238/iter-241 widenings).

**Why.** The owner's verdict on reviewing iter-241's UX-2 widening: the flag
was featureitis. It was a third file-addressing mechanism alongside
`--file`/`--glob`, and its matching rules — verbatim ID, zero-padding
fallback, letter-suffix separation, `**/` recursive fallback for archived
files — had grown to four interacting rules that are harder to predict than
the one glob the user could have written themselves. An agent CLI's surface
should be small and predictable; a convenience alias that requires a mental
model to use correctly is a net cost. The glob replacement is one sentence of
documentation: `find --glob '**/iteration-02-*.md'` reaches padded and
archived files alike.

**What stays.** The `{n}` filename_template feature itself (used by `new`,
type inference, and schema lint), `--filenames-only`/`--filenames0`, and the
iteration-*schema* ergonomics. Only the addressing flag is removed, along
with `hyalo-core::iteration_id` and the `FilenameTemplate` ID-glob helpers
that existed solely to serve it.

**Lesson recorded for future flag proposals:** when the motivating complaint
is "the agent doesn't know the vault layout", the fix is teaching (help text,
skills), not a new selector. Convenience flags for agents are a bad trade —
the agent doesn't mind typing 40 characters; it minds not being able to
predict behavior.

## DEC-243: dot-path property filters descend sequences by auto-descent, with numeric segments as an index (2026-08-28)

Implemented in [[iterations/iteration-245-deferral-carryovers]], closing the
UX-3 follow-up carried out of [[iterations/iteration-244-index-remaining-deferrals]].

**Decision.** `--property 'contacts.email=v'` traverses a frontmatter
*sequence* as well as a mapping. Both forms the plan offered are supported,
and they compose:

- a **numeric** segment indexes one element — `contacts.0.email`;
- **any other** segment auto-descends into *every* element and collects the
  hits into a sequence — `contacts.email` over
  `contacts: [{email: a}, {email: b}]` resolves to `[a, b]`.

**Why both.** Auto-descent alone cannot express "the first contact"; an
index alone forces the caller to know the list's shape, which is exactly
what a vault-wide query does not know. The precedence rule keeps them
unambiguous: a segment is only read as an index when the value at hand is a
sequence *and* the segment parses as an in-range `usize`. A mapping keyed
`"0"` is still a key lookup, and an out-of-range index falls through to
auto-descent (which then finds nothing, rather than silently matching a
neighbour).

**Why collect into a sequence.** Returning the hits as a list means the
established list semantics apply with no new operator rules to learn or
document: `=`/`~=` match when any element matches, `!=` when none does, a
bare key exists when at least one element yielded a value, and `!K` passes
when none did. A single hit is returned bare so the ordering operators
(`>`, `<`, …), which cannot compare a sequence, keep working. Nested
sequences flatten, so `groups.members.name` reaches leaves through two
levels.

**Cost.** `resolve_prop` now returns `Cow<'_, Value>`: the flat-key and
mapping paths stay borrowed, and only auto-descent through a sequence
allocates — a per-file cost paid solely by queries that actually use the
form. The literal-dotted-key-wins rule from iter-244 is unchanged.

## DEC-244: v0.21.0 is deferred; the release stays parked pending an explicit owner decision (2026-08-28)

Recorded in [[iterations/iteration-245-deferral-carryovers]] against that
plan's second task ("cut v0.21.0 … or record the owner's explicit decision
to stay on 0.20.x; owner call, do not tag without it").

**Decision.** No tag is cut. Releases are on hold by standing owner
decision, and the loop that implemented iteration 245 was explicitly barred
from tagging, running `/release`, or invoking `gh release create`. The
release question is therefore closed *as recorded*, not as executed: the
`Unreleased` section of the changelog keeps accumulating, and the workspace
stays on 0.20.0.

**What is queued for whenever the release is unparked.** The `Unreleased`
section already justifies a minor bump under DEC-101 version discipline —
BUG-4 post-mutation BM25 parity, UX-3 nested dot-path filters and this
iteration's sequence follow-up, UX-6 case-insensitive link resolution, and
the `new` link-graph upsert. None of it is a breaking change, so 0.21.0
remains the right number when the owner says go.

**How to unpark.** The owner runs `/release` (the `release` skill) and
picks the version; nothing in this decision constrains that choice beyond
recording that the accumulated work is minor-bump-shaped.

## DEC-245: stale snapshots stay warn-but-serve by default; `--strict-index` is the opt-in fallback (2026-08-28)

> **Superseded (implementation only) by [[decision-log#DEC-249|DEC-249]]
> (2026-08-28):** `--strict-index` itself was removed the same day, as an
> owner call unrelated to this decision's reasoning. The warn-but-serve
> default recorded below is still current and unaffected.

Closes the second task of [[iterations/iteration-247-carry-over-sweep]], which
carried finding S-2 of [[reviews/deep-review-2026-08-27]] forward from
[[iterations/iteration-246-help-coherence-review-followups]].

**Decision.** Both halves of the review's suggestion are taken, in the order
it framed them. The default is unchanged and now explicitly permanent: a
snapshot the staleness probe flags as older than the vault is still served,
the run still warns, and it still exits 0 (DEC-241). On top of that a global
`--strict-index` flag inverts the outcome for callers who ask for it — the
same detection then drops the snapshot and the query rescans disk.

**Why not make strictness the default.** The probe is a heuristic: it compares
shallow directory mtimes with one second of tolerance, so it cannot see an
in-place edit of an existing note, cannot see a change more than one level
deep, and can fire on a vault that only looks touched. Promoting a heuristic
to a hard refusal would make every indexed query hostage to filesystem mtime
granularity, and on a coarse-mtime filesystem it would silently un-index a
vault that was never stale. Warn-but-serve keeps the fast path honest about
its own limits instead of pretending to a precision it does not have.

**Why an opt-in is still worth having.** The review's real complaint is that
an agent that mutated files *without* `--index` and then queried *with* it
gets an answer contradicting disk, at exit 0, with only a warning to go on.
`--strict-index` gives that caller a way to say "I would rather pay for a
rescan than risk that", and the failure mode is benign in both directions: a
false positive costs a disk scan that was not needed, a false negative leaves
today's behaviour. It can never produce a wrong answer that the default would
have got right.

**Shape.** Global flag (it applies to every command that can load a snapshot),
inert when no index is in use, and irrelevant to a snapshot that fails the
vault/site-prefix check — that already falls back to disk. The fallback
warning names the flag and the remedy; `-q` suppresses it like any other
warning. `links fix` / `links auto` are unaffected: they already mtime-check
every entry and rescan what changed (DEC-241).

## DEC-246: no `find --changed-since <ref>`; hyalo stays VCS-agnostic and spawns nothing (2026-08-28)

Closes the fifth task of [[iterations/iteration-247-carry-over-sweep]] — the
minor feature request in [[reviews/deep-review-2026-08-27]]'s dogfood notes,
which asked whether the `--files-from <(git diff …)` cookbook pattern deserved
a friendlier built-in.

**Decision.** Rejected. `--files-from -` stays the supported way to scope a
run to changed files. The flag's `--help` now carries the recipe next to the
flag itself rather than only in `find`'s examples block, which is where the
discoverability gap actually was.

**Why.** hyalo executes no subprocess anywhere in the codebase today — grep
for `process::Command` and the workspace is empty. `--changed-since` cannot be
implemented without breaking that: either hyalo shells out to whatever `git`
is first on `PATH` (a new external runtime dependency, a new failure mode on
machines without git, and a new argument-injection surface, since a ref is
user input and refs may begin with `-`), or it takes a git library dependency
— `gix` and its tree, for a convenience flag — which is a large build-time and
audit-surface cost for sugar over one pipe. CLAUDE.md's "all code stays in
Rust, no polyglot tooling" points the same way.

**What is lost, honestly.** One pipe of typing, and the ability to write the
recipe as a single flag in a config file. **What is kept.** `--files-from -`
works with any producer: `git diff`, `jj diff`, `hg status`, `find -newer`,
`fd`, `rg -l`, a CI job's changed-files list, or a hand-written file. A
`--changed-since` flag would serve one of those and quietly privilege git as
*the* version control system, which is not a claim this tool needs to make.

## DEC-247: `summary --format text` announces the vault dir on stderr, not stdout (2026-08-28)

Closes the fourth task of [[iterations/iteration-247-carry-over-sweep]].

**Decision.** The `kb dir: <path>` line that led `hyalo summary`'s text report
moves to stderr as `note: kb dir: <path>`. Text stdout now starts at `Files:`,
`-q` suppresses the note, and the JSON envelope still carries `dir` unchanged.

**Why.** It was the only command that prefixed its stdout with resolution
context, and the content is cwd-dependent: the same vault prints a different
first line depending on where the command was run from, so every text-mode
script had to know to drop line one. stderr is already this CLI's channel for
"which vault/config did this run resolve" — a `--dir` that switches away from
the configured tree is announced there, and so is a `.hyalo.toml` that would
not parse — so the note is not a new convention, it is the existing one
applied consistently.

**Why not a flag.** A flag would have to default one way or the other and
would add a third thing to remember. `-q` already means "no stderr chatter",
which is exactly the control a script wants here, so the flag would have been
a synonym for something the CLI can already express.

**Where it lives.** Commands run with `effective_format = Json` internally and
the user's format is only known in the output pipeline, so the note is emitted
in `run.rs` after a successful dispatch rather than inside `commands::summary`
— documented at both ends so the split is not a surprise.

## DEC-248: `reviews/` is a live directory: it lints, and `review` is a declared type (2026-08-28)

Closes the third task of [[iterations/iteration-247-carry-over-sweep]].

**Decision.** `reviews/**` is removed from `[lint] ignore`, and a `review`
type is declared in `[schema.types.review]` (required `title`, `type`, `date`,
`status`; `status` an enum of active/resolved/completed/superseded/archived;
`date` typed; `tags` and `related` lists). The two files under `reviews/` that
still said `type: research` are migrated to `type: review`, and the body
warnings the directory had been hiding are fixed. `hyalo lint --strict` is
clean vault-wide.

**Why.** The ignore list exists for *frozen* records — completed iterations,
done backlog items, dogfood logs — whose old formatting is not worth
rewriting. Review notes are not frozen: iterations 246 and 247 are both
drawing their task lists straight out of `reviews/deep-review-2026-08-27.md`.
Excluding a directory that live work reads from meant a schema drift sat there
undetected: `type: review` was used by eight files and declared by none, and
`hyalo types show review` failed while nothing in CI noticed.

**Why declare rather than migrate to an existing type.** `review` is a real
kind in this vault with its own lifecycle (`resolved` — every finding
addressed — is a state `research` has no use for). Folding the files into
`research` would have made the type list lie about what the vault contains,
which is the same failure in the other direction.

## DEC-249: `--strict-index` is removed, not kept as a documented opt-in (2026-08-28)

Supersedes the implementation half of [[decision-log#DEC-245|DEC-245]] (the
warn-but-serve default it recorded as permanent is untouched). Closes
[[iterations/iteration-248-remove-strict-index]].

**Decision.** The global `--strict-index` flag added under DEC-245 /
[[iterations/iteration-247-carry-over-sweep]] is removed outright: the clap
field, its plumbing in `run.rs`, the `--help` line, the long-form
`create-index` help paragraph, the xtask `GLOBAL_FLAGS` command-reference
registration, and the four e2e tests that existed solely to exercise it. The
stale-index warning keeps firing exactly as before — `index older than
vault; results may be stale — re-run create-index` — just without the
`(or pass --strict-index to rescan disk instead)` suffix, since there is no
longer a flag to name.

**Why.** Owner call, made the same day the flag shipped, on three grounds:
the flag was redundant (a caller who wants a guaranteed disk scan already
gets one by not passing `--index` at all — that is not a new capability,
it is the pre-existing no-index path with an extra name on it); the name was
a misnomer ("strict" reads as "fails loudly," but the flag only ever
degrades to a slower, correct path, never refuses a run; DEC-245 itself
had to spend a paragraph explaining that the failure mode is benign in
both directions, which is a sign the name was fighting the behaviour it
named); and it grew the CLI's global-flag surface (one more line in every
`--help`, one more entry in `GLOBAL_FLAGS`, one more thing every future
flag audit has to reason about) for a need nobody had actually hit in
practice — DEC-245's own justification was a hypothetical caller, not a
reported bug.

**What stays.** Warn-but-serve as the sole behaviour on a stale snapshot is
unchanged and still governed by DEC-241/DEC-245: the run warns and exits 0.
A caller who wants disk truth over snapshot speed still has that lever —
omit `--index`/`--index-file` — it was just never a new one.

## DEC-250: pi-package stays canonical at the repo root; the crate embeds a vendored, gate-enforced copy (2026-08-29)

**Problem.** `crates/hyalo-cli/src/commands/init.rs` embedded the four pi
integration files (`hyalo`/`hyalo-tidy` `SKILL.md`, `hyalo.ts`, `package.json`)
via `include_str!("../../../../pi-package/...")` — reaching four directories
up, outside `crates/hyalo-cli/`. DEC-237 called this a "single source of
truth" with "drift impossible by construction." That held for a normal build,
but `cargo package`/`cargo publish` builds the verify tarball with only the
crate directory on disk, so the `include_str!` paths resolved to nothing and
the `hyalo-cli` 0.21.0 crates.io publish failed at the tarball verify step
(`hyalo-core` and `hyalo-mdlint` 0.21.0 had already published). `cargo
package -p hyalo-cli` reproduces the same failure locally.

**Decision.** Vendor byte-identical copies of the four files under
`crates/hyalo-cli/templates/pi/` (mirroring `pi-package/`'s own
`skills/<name>/SKILL.md` and `extensions/<name>.ts` layout) and point the
`include_str!`s there instead. The top-level `pi-package/` directory remains
canonical and unchanged — it is the exact layout `pi install
git:github.com/ractive/hyalo` consumes, and moving or symlinking it was ruled
out (symlinks don't survive a Windows checkout). A new `check-pi-package-sync`
xtask gate, wired into the `quality-gates` CI job alongside
`check-bundled-skills`, fails the build if any vendored file differs
byte-for-byte from its `pi-package/` counterpart, or if `pi-package/` gains a
skill/extension/`package.json` with no vendored counterpart. `just
sync-pi-package` copies `pi-package/` onto the vendored tree to fix drift.

**Alternatives considered.**
- *Publish `pi-package/` as its own crate/package and depend on it.* Rejected:
  `pi-package/` isn't Rust — it's a `pi` extension (TypeScript) plus
  Markdown skills consumed by `pi install`, not something Cargo can depend on
  without inventing a packaging format nobody else needs.
- *Move `pi-package/` inside `crates/hyalo-cli/` and symlink (or generate) the
  root-level path `pi install` expects.* Rejected on the same grounds
  DEC-237 already reflects in the README/justfile/e2e-script constraints
  this task inherited: `pi install git:...` needs `pi-package/` at the repo
  root, and symlinks are unusable in Windows checkouts.
- *Keep the single-source `include_str!` and special-case `cargo publish`
  (e.g. a build script that copies files in before packaging).* Rejected:
  `cargo package`'s verify step runs from the extracted tarball in a temp
  directory with no access to the original working tree, so a build script
  can't reach back out to `pi-package/` either — the constraint that broke
  `include_str!` breaks this too.

**Why a gate instead of trusting reviewers to keep both copies in sync.**
Manual sync is exactly the failure mode DEC-237 was trying to avoid by having
one source in the first place; a gate makes drift a CI failure instead of a
silent divergence that only surfaces (as this bug did) at publish time.

## DEC-251: axi.md agent-CLI principles — what hyalo adopts and what it rejects (2026-08-29)

**Context.** The owner asked what [axi.md](https://axi.md/) (AXI, "Agent
eXperience Interface" — a design spec for agent-facing CLIs with a
conformance catalog and LLM-judged benchmarks, not a knowledgebase tool)
teaches. A research pass compared its ten principles against
`target/release/hyalo` 0.21.0 on the own KB.

**Already at or above the bar.** Structured `hints[]` with a `writes`
flag (stricter than AXI's free-text `help[]`); `summary` as a pre-computed
aggregate; idempotent no-op mutations (`0/1 modified`, exit 0); structured
errors on stderr with exit codes.

**Measured gaps → adopted.**
- Concise per-command help: `hyalo -h` 7.7 KB, `find -h` 12.3 KB
  (`--help` 29 / 24 KB) because flag docs have no short/long split and the
  global-options block repeats on all 27 subcommands →
  [[iterations/iteration-251-agent-discoverability-help]].
- Definitive empty states: bare `No results`, `hints: []` on zero results →
  iteration 251.
- Minimal default list schema: `find --tag iteration --limit 20
  --format json` = 73 KB vs 8 KB with `--fields properties` →
  [[iterations/iteration-252-find-result-shape]] (breaking; minor bump).
- Size hints before reading: no `size`/`lines` on results → iteration 252.

**Rejected.**
- TOON as a third output format: breaks `--jq` and the envelope contract;
  the savings come from the default field set instead.
- Errors on stdout: stderr + exit code is the Unix contract CI relies on.
- Content-first bare invocation (`hyalo` alone printing live data): a KB
  has no single "most relevant" view; surprises humans and `hyalo | less`.
- `SessionStart` hook / ambient context: `summary` is one call away.
- npx/`skills add` distribution: hyalo has real packaging; no polyglot
  tooling.
- Benchmarks as marketing: a before/after token count of `find` on ten
  tasks is enough evidence for a solo tool.

**Constraint carried into both iterations.** No new CLI flags
([[decision-log#DEC-249]] discipline): every adoption is a default, a
layout, a hint, or metadata on existing output.

## DEC-252: `title` is promoted out of `properties`, and `--fields all` is hinted only where it is affordable (2026-08-30)

**Decision (a):** `find` promotes `title` to a top-level field in the default
result shape and stops repeating it inside the `properties` map. The removal
happens *after* sorting and filtering, so `--sort property:title` and
`--property title=…` compare exactly what they compared before; only the
serialized payload changes. `--fields properties` without `title` still
carries the raw `title` property, so no request can lose the value.

**Why:** `tags` has been promoted this way since the first `find`, so the rule
already existed — `title` joining the default field set (iteration 252) simply
made the duplicate visible: the same string, twice, in every result item. It
is also what closed the last kilobyte of the iteration's byte budget: the
20-file own-KB listing lands at 11.9 KB against a 12 KB acceptance criterion,
where keeping the duplicate put it at 13.3 KB. Callers reading
`.results[].properties.title` must read `.results[].title` — recorded as a
breaking change in the 0.22.0 changelog, and swept out of the bundled skills,
the knowledgebase rule file and the tests in the same PR.

**Decision (b):** the `--fields all` hint fires only for an untruncated result
set of five or fewer items, not on every `find` as
[[iterations/iteration-252-find-result-shape]] proposed.

**Why:** `--fields all` is roughly a 10x payload — precisely the cost this
iteration removed. Suggesting it under a 50-file listing would hand an agent
the bill it was just spared, and with `MAX_HINTS = 5` it would displace the
narrowing hints (`--limit`, narrow-by-tag) that actually help at that size.
Where the whole result set is a handful of files, expanding it is cheap and
the hint is the fastest way to the full shape. The general statement lives
where it costs nothing instead: the `--format text` `fields:` summary line
names what the payload carries and what `--fields all` would add, and
`find --help` says the same.

## DEC-253: `read` derives `lines` from the body pass it already makes; only `--frontmatter` still scans (2026-08-30)

**Decision:** `read` computes the whole-file `lines` it reports (DEC-252)
from the single body pass it already performs, not from a second
`scanner::count_file_lines` scan. `read_body_lines` now returns
`(Vec<String>, usize)` — the body lines plus the frontmatter's line count,
which `frontmatter::skip_frontmatter` already computed and simply was not
propagating — and `commands/read.rs::run` captures `fm_lines +
body_lines.len()` *before* `--section`/`--lines` narrow the vector.
`count_file_lines` survives for exactly one caller: the
`--frontmatter`-only path (`need_body == false`), which deliberately never
touches the body and so has nothing to derive from.

**Why:** DEC-252 made `lines` unconditional, which made the scan
unconditional too — every `read` that returned body content read the file's
bytes twice, once streamed through `read_line_capped` and once through
`memchr`. That doubles disk I/O for precisely the large-file case the
feature exists to help with, and it contradicts the scanner's standing
"frontmatter-only queries pay zero cost for body scanning" principle by
being the one path forced to eat a second full read *after* the body was
already in hand. Output is byte-identical; this is a pure how-computed
change, which is why it was not a blocker on PR #293 and landed as its own
iteration.

**The invariant it rests on:** `scanner::read_line_capped` consumes exactly
one logical line per call — an over-quota or invalid-UTF-8 line is drained
to the next `\n` and still occupies exactly one slot in the returned vector
(as a placeholder), and EOF after an unterminated final line yields that
line and then `n == 0`. That is the same rule `count_lines` applies (one
line per `\n`, plus a final unterminated one, zero for an empty file), so
`fm_lines + body.len()` is not an approximation of the scan — it is the same
count by construction. `skip_frontmatter` counts the opening and closing
`---` and everything between them, and bails with an error on unclosed or
oversized frontmatter, so there is no path where it under-reports a block it
actually consumed. Both halves are pinned by
`derived_line_count_matches_a_whole_file_scan` (unit, `read.rs`) and
`read_lines_match_disk_for_every_read_mode` (e2e, `find_result_shape.rs`),
which compare against an independent naive counter across CRLF, no-EOL,
multi-byte UTF-8, invalid-UTF-8 and over-cap-line inputs in every read mode.

**Not done:** removing the `--frontmatter`-only scan. Reporting `lines` there
requires reading the file's bytes no matter what — the DEC-252 contract says
every `read` result carries the whole file's `lines` — so that path reads
the file once, which is already the floor. Making `lines` conditional again
to avoid it would undo DEC-252, not improve it.

## DEC-254: an explicit `--fields` is an exact projection; only `file` survives it (2026-08-30)

**Decision:** `file` is the only unconditional key in a `find` result item —
it names the result, so no projection may drop it. `modified`, `size` and
`lines` stop being structural and become ordinary members of the *default*
field set: present when no `--fields` is given, absent when an explicit
`--fields` does not name them. So `--fields title` → `{file, title}`,
`--fields size,lines` → `{file, size, lines}`, `--fields file` → `{file}`,
`--fields all` unchanged. Filter auto-includes (`--section`→sections,
`--task`→tasks, `--broken-links`→links, `--orphan`/`--dead-end`→links +
backlinks, count sorts→the ranked field) still add to whatever set is in
force. A view's pinned `fields` behaves exactly like an explicit `--fields`,
and a CLI `--fields` **replaces** the pin rather than extending it.

**Why the three stay in the default set:** they are cheap — `modified` and
`size` come from the `stat` the walk already does, `lines` from the scan
already made — and they are precisely the inputs an agent uses to choose its
*next* call: `size`/`lines` to decide between `read` and `read --lines A:B`,
`modified` to decide what is worth looking at. Making them opt-in would mean
the common path has to name them, which is the cost DEC-252 set out to
remove.

**Why an explicit `--fields` drops them anyway:** `--fields` is the flag a
caller reaches for when it knows exactly what it wants, and before this it
could not express that — `--fields title` still paid for four keys, so the
narrowest possible request on this project's own knowledgebase was 8.6 KB for
50 items when the caller wanted 3.5 KB of paths and titles. "Exactly the
fields you named, plus the one that names the row" is also the only rule that
needs no list of exceptions, which matters more for a flag an agent composes
than saving anyone a keystroke. `--fields file` falls out of the rule for free
and means "just the paths" without a special case; it also makes a printed
field list round-trip back into `--fields`.

**Why CLI `--fields` replaces a view's pin:** the old `extend` meant a view
could only ever be widened. `hyalo find --view titles --fields tags` returned
`{file, tags, title}` — more than either the view or the flag asked for — and
there was no way to narrow a saved view at all. Replacement makes `--view`
plus `--fields` mean the same thing as `--fields` alone, which is what "the
CLI overrides the view" means for every other single-valued flag.

**Where it is implemented:** in the one projection point that already builds
the item — `Fields` in `hyalo-core::filter::fields` gained `modified`/`size`/
`lines` members, and `commands/find.rs` sets each key with
`fields.<name>.then(...)`. Not as a post-filter over the JSON: a post-filter
would have had to be repeated for the text renderer, the `fields:` summary
line and every future consumer. `--sort modified` under a projection that
drops `modified` follows the existing `--sort property:K`/`--sort title`
precedent: computed internally, stripped after sorting.

**One consequence worth naming:** the text renderer could no longer recognise
a `FileObject` by "has `file` and `modified`" — `--fields backlinks` yields
`{file, backlinks}`, byte-identical to a `backlinks` command result, and
rendered as one. Detection is now "has `file` and no key outside the
`FileObject` vocabulary", and a `find` listing renders its items directly
rather than dispatching each through the key-signature table.

## DEC-255: every scalar `title` promotes, stringified as written (2026-08-30) — amends DEC-252

**Decision:** the promoted `title` field takes any scalar frontmatter
`title`, stringified as it was written in the file: `title: 42` → `"42"`,
`title: 1.0` → `"1.0"`, `title: 2026-08-30` → `"2026-08-30"`, `title: true` →
`"true"`. The typed value stays available under `--fields properties-typed`.
`--property title=42`, `--title 42` and `--sort title` compare that string
like every other key. Null, empty and whitespace-only titles count as absent:
H1 fallback, and they stay in `properties` because nothing consumed them. A
collection — `title: [a, b]`, `title: {k: v}` — has no honest string form, so
it does not promote: `title` falls back to the H1, the raw value is **kept**
in `properties.title` (the promotion strips the property only when it actually
consumed a scalar), and the new default-on rule `HYALO007`
(`title-not-scalar`, warn; error under `--strict`) reports it.

**Why:** DEC-252 promoted `title` only when the YAML value was a string, and
stripped `properties.title` whenever `title` and `properties` were both
requested. Together those two rules made a non-string title *unreachable*:
`find --fields title` showed the H1, and `properties` no longer carried the
authored value either. YAML's type inference is an accident of the syntax —
someone who writes `title: 42` means the text `42`, and someone who writes
`title: 2026-08-30` means that date's text, not a date object. Stringifying
is what the author meant; keeping the typed value in `properties-typed` means
nothing is lost.

**Why a collection is different:** there is no string a reader would agree
`[a, b]` "means". Guessing one (`"a, b"`? `"[a, b]"`?) would put a fabricated
title into the field callers sort and filter on. In practice it is a quoting
typo — `title: [Draft] Notes` parses as a one-element list — which is why it
earns a lint rule rather than a silent coercion.

**Not done: a filename fallback.** The plan sketched "H1 fallback, then
filename". `title: null` stays the honest answer for a file with neither a
title nor an H1: `FileObject.title` is documented as `null` when the field was
requested and nothing was found, `--format text` marks it `(none)`, and
substituting a filename would make "this file has no title" unrepresentable
in either format — including for the lint and tidy recipes that look for
exactly that.

## DEC-256: `read --format text` prints the body and nothing else (2026-08-30)

**Decision:** `read`'s text output stays exactly the body bytes. The iteration
252 plan's note that "text mode shows size in the header line" is dropped, not
implemented. `size`/`lines` appear in `--format json`, in the over-8 KiB hint
(`Read only the first 80 of N lines (K KB file)`), and on the `find` item that
sent the caller here.

**Why:** `read` is the one command that deliberately defaults to plain text
rather than JSON, because its output is meant to be *the file* —
`hyalo read x.md > x.txt`, `hyalo read x.md | grep …`, a body pasted into a
prompt. A header line corrupts every one of those, and there is no flag-free
way for a consumer to strip it. The information is not lost: the number that
matters (this file is big, read less of it) is already delivered by the hint,
at exactly the moment it is actionable.

## DEC-257: `dry_run` is universal on object-shaped mutation results; `skipped_count` is bulk-family-only (2026-08-31)

**Decision:** the envelope claim "every mutating command reports `dry_run` and
`skipped_count`" was false for 18 of 23 mutating commands. Resolve it by
making the first half true and correcting the second, rather than by softening
both.

- `dry_run` is now emitted by every mutating command whose `results` is an
  object. Added in iteration 256 to batch `mv`, `new`, `types remove`,
  `madr toc`, `okf index`, `okf log`, `changelog add` and `changelog release`.
- `apply` (the generators) and `applied` (batch `mv`) are retained and are
  always the exact inverse of `dry_run`. They predate the convention and are
  load-bearing for the text formatters; a rename would be a breaking change
  bought for nothing.
- `skipped_count` stays with the bulk-mutation family only — `set`, `remove`,
  `append`, `properties rename`, `tags rename`.
- `task toggle` / `task set` are the documented exception: their `results` is
  an *array* of per-task records (collapsing to a bare object at length one),
  so there is no top-level object to carry the flag. `old_status` is present
  on dry-run records and absent on applied ones; that key is the
  discriminator.
- `init` / `deinit` and `create-index` / `drop-index` are outside the
  contract: they write config and index files, not notes.

**Why not add `skipped_count` everywhere:** a single-target command has no
scanned-but-unchanged set. A hard-coded `0` reads like a measurement and is
none — worse than an absent key, which at least makes the caller look.

**Why not just soften the sentence:** the `dry_run` half is the one a script
actually branches on ("did this write?"), and before this iteration answering
it needed per-command knowledge of four different spellings. That is exactly
the drift the results-shape inventory exists to prevent.

**Enforcement:** `crates/hyalo-cli/tests/e2e/results_shape.rs` walks both
families and asserts the flag's presence, its type, and its agreement with
`apply`/`applied`; a second test pins the `task` exception's discriminator.
Contract text updated in `cli/args.rs` (RESULTS CONVENTIONS),
`templates/rule-knowledgebase.md` and `templates/skill-hyalo.md`.

## DEC-258: `hyalo help <cmd>` forwards to the short `-h` page (2026-08-31)

**Decision:** `hyalo help <cmd>` renders the same page as `hyalo <cmd> -h`,
byte for byte. Measured on v0.22.0: `hyalo help find` was 28 701 bytes against
`hyalo find -h`'s 2 992 — a 9.6x tax on the phrasing agents reach for first.

**How, and why clap does not block it:** the iteration plan asked to confirm
clap-derive can intercept `Subcommand::Help` before committing. It can.
`disable_help_subcommand = true` on the root suppresses clap's generated
subcommand, and a `Commands::Help { command: Vec<String> }` variant reserves
the name (so it appears in shell completions and the COMMAND REFERENCE). The
forward itself is an argv rewrite in `run_inner`: `hyalo [globals] help
<path>...` becomes `hyalo <path>... -h` before clap parses.

**Why rewrite argv rather than render a page from the `Help` arm:** rendering
our own would produce a *similar* page, not the same one. The short page is
assembled at parse time — globals collapsed to a one-line pointer, `find`'s
composed examples, the `--help` footer — and all of that is inherited free by
rewriting. Two bonuses fall out: root globals are irrelevant to a help page so
dropping them costs nothing, and an unknown name now hits clap's ordinary
unknown-subcommand path, which restores the did-you-mean the generated `help`
subcommand never had (`hyalo help fnd` → "a similar subcommand exists:
'find'"). That closes HELP-13.

**Escape hatch:** every short page ends with a pointer at its long form, so
`hyalo find --help` is one line away from where the reader already is. Nested
group help (`hyalo task help toggle`) still uses clap's generated subcommand
and still renders the long page; that is a per-group `disable_help_subcommand`
with no replacement, which would remove the name rather than improve it.

## DEC-259: FIND-8's 20% was a quadratic stem dedupe, not lazy-field materialisation (2026-08-31)

**Decision:** fixed, not documented as inherent. The hypothesis on file — that
`--fields all` materialises heavy per-entry fields before `--limit` is applied
— was wrong, and the fix is in `hyalo-core`, not in `find`'s projection.

**Measurement** (release build, MDN vault, 14 399 files, 123 MB snapshot
index, best of 7):

| invocation | before | after |
| --- | --- | --- |
| `find --index --limit 1` | 0.371 s | 0.371 s |
| `find --index --limit 1 --fields all` | 0.448 s (+20.4%) | 0.368 s |
| `find --index --limit 1 --fields links` | 0.447 s | 0.381 s |
| `find --index --limit 1 --fields backlinks` | 0.380 s | 0.380 s |

Per-field timing localised the entire delta to `links`; `sections`, `tasks`,
`backlinks` and `properties-typed` were free. Instrumenting `maybe_case_index`
put 62.4 ms of the ~76 ms delta in one call: `build_case_index_from_snapshot`.

**Root cause:** `CaseInsensitiveIndex::insert` deduped by linear-scanning the
*stem* bucket. MDN names every page `index.md`, so all 14 399 paths shared one
bucket and the build was O(n²) — ~104 million string comparisons. Deduping
against the `map` bucket instead (which holds only the case-variants of one
path, and is written in the same call, so membership in one implies membership
in the other) makes it O(1) amortised: 62.4 ms → 4.2 ms, a 15x improvement.

**Why the cost could never have been fixed by limit-awareness:** resolving a
single file's outbound links needs a vault-wide stem map, because a link in
any one file can point at any file. `--limit 1` cannot shrink it. The
lazy-projection idea DEC-254 made available would have bought nothing here;
the win was an algorithmic bug that the `--fields all` measurement happened to
surface.

**Scope of the fix:** `build_case_index_from_dir` shares `insert`, so
unindexed vaults with a repeated basename — `index.md`, `README.md` — get the
same improvement across `links fix`, `--broken-links`, `--orphan` and
`--dead-end`. Three unit tests in `case_index.rs` pin what the removed scan
was there for: re-inserting a path is still a no-op in both maps, many paths
sharing one stem are all recorded and the stem stays ambiguous, and two paths
differing only in case stay separate candidates.

## DEC-260: the root `-h` grouping and examples stand; one label was factually wrong (2026-08-31)

**Decision:** iteration 254 deferred "revisit the top-level `-h` COMMANDS
grouping and the five EXAMPLES lines against a fresh dogfood pass". Revisited;
no reshuffle. The three-way read / write / setup split and the five composed
examples still read correctly, and the plan's own instruction was not to force
a change for its own sake.

One thing was not cosmetic and was fixed: the third group was labelled
`COMMANDS — config and scaffolds (write .hyalo.toml)`, but `create-index` and
`drop-index` write a snapshot index and `completions` writes nothing. The
label now reads `COMMANDS — setup (writes config/index, not your notes)`,
which is both accurate and the distinction that actually matters to a caller
deciding whether a command can touch their markdown.

`help` earns no COMMANDS row: the footer names it directly
(`hyalo <cmd> -h (== hyalo help <cmd>)`), and a row would cost a line of a
page held to a 2 560-byte ceiling to say what the footer already says.

## DEC-261: `init`/`deinit` follow `--dir`; a vault outside CWD moves the root (2026-08-31)

**Decision:** `--dir` names the *vault*, and the vault decides the *root* — the
directory that holds `.hyalo.toml` and the `.claude`/`.pi` integration files.
One rule, shared by both commands:

- vault at or below CWD (`--dir docs`, `--dir /repo/docs` run from `/repo`, no
  `--dir` at all) → root stays CWD; `init` records `dir` **relative** to it,
  `deinit` cleans CWD;
- vault outside CWD (`--dir /elsewhere/vault`, `--dir ../sibling`) → the root
  moves into that tree: `init` writes `.hyalo.toml` *there* with `dir = "."`,
  `deinit` removes *that* tree's files and never CWD's.

A run whose root is not CWD leads its summary with `target   <path>`.

**What was broken.** Both bugs were hit live during iteration 256's own
dogfooding pass, from inside this repo, and had to be repaired by hand
(commits `78fc09b5`, `0c96ec77`):

- `init --dir <other-tree>` wrote `.hyalo.toml` into **CWD** with `dir =
  "<absolute path>"`. A project-local config may not set an absolute `dir`
  (iter-221 H-1, tightened in iter-243/244), so the very next hyalo run refused
  the file it had just written — an `init` that produces a config `config`
  rejects.
- `deinit` ignored `--dir` outright and always targeted CWD, so
  `hyalo --dir <temp-vault> deinit` deleted this repo's `.hyalo.toml`,
  `.claude/CLAUDE.md` and three `.claude` symlinks while its summary looked, at
  a glance, like a no-op (a dozen `skipped … (not found)` lines around the real
  removals).

**Why this rule and not the alternatives.** Three were on the table for
`init`: write `dir` relative to the config file, write the config into the
other tree, or refuse the combination. Relative-to-config only works while the
target is *inside* the config's directory — `../..`-style values are refused by
the same boundary rule, so it fixes `--dir /repo/docs` and nothing else. It is
therefore kept, but only as the in-bounds half of the rule. Refusal was
rejected because `hyalo --dir /elsewhere/vault init` has an obvious intent and
no other spelling; making the user `cd` first is a worse CLI. Moving the root
is what the user asked for in every reading of the flag.

For `deinit` the choice was honour-or-refuse. Honouring wins for the same
reason, and it is strictly safer than the status quo either way: the failure
mode being fixed is *deleting the tree the user did not name*.

**Implementation note.** Containment is decided on canonicalized paths — the
deepest existing ancestor is canonicalized and the not-yet-created remainder
re-appended — so an absolute spelling of a subdirectory, a `sub/../kb`
round-trip, and macOS's `/tmp` → `/private/tmp` symlink all resolve to
"inside", while a symlink that genuinely leaves the tree resolves to "outside".
This mirrors what `validate_project_local_dir` already does for reads, so
`init` cannot write a `dir` that the reader would then refuse.

**Non-goal:** the rest of `init`/`deinit`'s UX. The interleaved
`skipped … (not found)` noise in `deinit`'s summary is unchanged apart from the
`target` header; it is now structured in JSON (DEC-262), which is the fix that
matters for agents.

## DEC-262: `init`/`deinit` answer `--format json`, but stay text when merely piped (2026-08-31)

**Decision:** both commands emit a minimal envelope on an explicit
`--format json` (or `--jq`, which implies it), and the text summary in every
other case — including when stdout is a pipe. `--format github` is refused with
the same message every non-lint command gives.

```json
{
  "results": {
    "command": "init",
    "root": "/abs/path/to/project",
    "actions": [
      {"action": "created", "target": ".hyalo.toml", "detail": "dir = \"docs\""},
      {"action": "skipped", "target": ".claude/CLAUDE.md", "detail": "not found"}
    ],
    "notes": ["…pi install hint…"]
  },
  "hints": [],
  "dir": "docs"
}
```

`action` is one of `created`, `updated`, `unchanged`, `removed`, `skipped`,
`warning`; `detail` is present only when the verb alone is ambiguous; `dir` is
hoisted to the top level by the shared envelope builder and is absent for
`deinit`, which has no vault value to report.

**Why not the full mutation-envelope contract.** DEC-257 put `init`/`deinit`
outside it: they write config and integration files, not notes, so
`dry_run`/`skipped_count`/per-file records have nothing to describe. That
stands. What did not stand was answering `--format json` with unparseable text.

**Why text still wins a bare pipe.** Every list/mutation command flips to JSON
when stdout is not a terminal, and `init`/`deinit` deliberately do not. Their
output is a progress report a human reads while setting a project up — the same
argument that keeps `read`'s results raw text under a pipe (DEC-256) — and
`hyalo init | tee setup.log` should not start emitting JSON. An agent that
wants structure asks for it, which is one flag, and the flag is now documented
in both help pages.

**Implementation note.** The summary is no longer built as a string. Both
commands accumulate a `Report { command, root, dir, actions, notes }`; the text
rendering is one line per action and the JSON body is `serde_json::to_value` of
the same struct, so the two can never drift. Text lines are now uniformly
`verb  target  (detail)` — previously some details were separated by one space
and some by two.

## DEC-263: a zero-result property-regex query probes the body before hinting (2026-08-31)

**Decision:** When a `find` returns nothing and carried a `--property K~=RE`
filter, hyalo probes whether the same regex matches body prose and — only if it
does — leads the zero-result hints with `hyalo find -e '<RE>'`. The probe is
bounded at 512 files / 8 MiB with a first-match early exit, runs on the
zero-result path only, and never on a query that already searched bodies
(`PATTERN` / `-e`). No new flag, no new config key.

**Why:** Iteration 256's dogfooding hit `hyalo find --property 'title~=/DEC-25/'`
against this decision log, whose `DEC-NNN` identifiers are `##` body headings.
The query is correct, the empty answer is correct, and the useful command
(`hyalo find -e 'DEC-25'`) is one flag away — but nothing on the zero-result
path said so. That is precisely the gap iteration 251's hint machinery exists to
close, so this is an addition to it, not a special case bolted on beside it.

**Why a probe rather than unconditional advice:** an unconditional "try body
search" line costs nothing but is wrong about as often as it is right, and a
hint that is sometimes wrong is a hint agents learn to skip (see
[[iterations/iteration-180-hint-trust]]). Confirming the match first makes the
hint a statement of fact: the command it prints is known to return something.
The probe compiles `(?i)<RE>` — exactly what `find -e` compiles — and matches
body and fenced-code lines, so the hint and the command it suggests cannot
disagree. For the same reason the probe ignores `--file`/`--glob` scoping and
every other filter, and the suggested command drops them: promising results
inside a narrower scope than was checked would be the lie this design is
avoiding.

**Why the budget is affordable:** the probe reads bodies, which a
frontmatter-only query otherwise never does — so it is bounded rather than
trusted to be small. Measured on this vault (437 files, 4 MB, release build) the
worst case, a regex that matches nothing anywhere, added ~10 ms to a ~20 ms
query; a match short-circuits far earlier. It fires only when a query already
returned zero *and* filtered on a property regex, so no query that found
anything pays for it. A vault above the ceiling simply does not get the hint,
which is the right trade against turning every empty query into a full body
scan.

**Rejected:** a `--search-bodies` / `--also-body` flag, and a config key for the
probe budget. Both grow the CLI surface under dogfood pressure, which
`feedback_no_cli_surface_growth` rules out; the hint text is the whole feature.

## DEC-264: the snapshot floor is BM25 traversal, and it is worth fixing (2026-09-01)

**Decision:** The ~0.37 s snapshot-load floor 256 flagged is **not** inherent.
76 % of a 116 MiB `.hyalo-index` is the BM25 inverted index, and a `find` with
no text query spends ~230 ms of its ~360 ms on that section without ever
reading it. The fix — stop the MessagePack parse at the `bm25_index` key and
decode it only on demand — is wire-compatible and measured at 240 ms → 61.5 ms
for the decode. It is scoped to
[[iterations/iteration-260-lazy-bm25-snapshot-load]] rather than landed in 259,
because making it *safe* is a design problem, not a patch. Full numbers:
[[research/snapshot-load-floor-2026-09-01]].

**Why the floor is not I/O:** reading the whole 116 MiB is 19 ms warm
(6.0 GiB/s) and 59 ms cold with `F_NOCACHE` (1.9 GiB/s) — 5–17 % of the
command. Every "read fewer bytes / stream the read / mmap it" idea is aimed at
the smallest term in the sum. mmap was already rejected on macOS in
[[research/performance-parallelization]]; nothing here reopens that.

**Why the floor is not the entries either:** the plan's stated suspects —
allocation shape and post-decode reconstruction — do not survive measurement.
All 14 375 `IndexEntry` values materialize from their own 19 MiB buffer in
41 ms. The re-sort, `path_index` rebuild and `rebuild_lower_index` together
cost 2 ms. DEC-259 already removed the one genuinely quadratic piece.

**Why `IgnoredAny` is not the answer:** the obvious lazy-field trick — mark
`bm25_index` as `IgnoredAny` — saves 35 ms of 240 ms. Decoding the whole
document while materializing *nothing at all* still costs 179 ms. Three
quarters of the decode is serde token-walking 87 MiB of `Posting` maps and
`positions: Vec<u32>` arrays; skipping construction while still walking the
bytes buys almost nothing. This is the finding that redirects the work: the
target is traversal, not materialization.

**Why early-stop rather than a format change:** `rmp_serde::to_vec_named`
writes the snapshot as a map with string keys and emits `bm25_index` last, so a
hand-written `Deserialize` that `break`s on that key skips all 87 MiB without
reading them, and `from_slice` accepts the unconsumed tail. The bytes on disk
do not change, every existing index stays readable, and the iteration's own
non-goal against uncovered wire-format changes is respected. An opaque
length-prefixed BM25 blob would be the more robust shape, but it breaks every
index in the field to buy the same 180 ms — so it stays on the shelf unless
early-stop proves unworkable.

**Why 260 and not 259:** the load-side change is small; the blast radius is
not. `write_snapshot` re-serializes `self.bm25_index`, so a snapshot loaded
lazily and then saved by `set` / `remove` / `append` / `task toggle` / `mv` /
`lint --fix --index` would silently drop the search index. SEC-3
(`total_postings`) and MED-1 (`validate_doc_ids`) currently run before the
index is exposed and reject the *whole* snapshot on failure — a contract that
does not compose with a section that fails at first use mid-query. And
early-stop is load-bearing on `bm25_index` being the last derive-emitted field,
an invariant no test pins today. Those are three decisions and a regression
test, which is an iteration, not a patch.

**Also recorded:** teardown is a real 59 ms of every indexed command — 43 ms of
it freeing the BM25 index's millions of small `Vec<u32>` allocations at process
exit. It is invisible to any measurement that stops when the command prints,
and it disappears for free alongside the decode saving.

**Rejected:** recording the floor as inherent and closing 259 with a "not worth
chasing" DEC. That was the plan's other permitted outcome and it would have
been wrong — a 2.4× win on the most common indexed command, at zero format
cost, is not noise.

## DEC-265: the deferred BM25 section keeps the snapshot bytes, and refuses at use (2026-09-01)

**Decision:** [[iterations/iteration-260-lazy-bm25-snapshot-load]] implements
DEC-264's early stop with three sub-decisions, recorded here because each had a
viable alternative the plan left open.

**1. Deferred, not `load_with(bm25: bool)`.** The plan offered a call-site
decision — every load site declares whether it will search text — or a lazy
re-read keyed off the index path. Both were rejected in favour of a third
shape: `SnapshotIndex` keeps the snapshot buffer it already read, plus the
offset of the `bm25_index` value, and decodes on first `bm25_index()` behind a
`OnceLock`. A `load_with` flag is the fastest possible answer but it is a new
correctness obligation at ~20 call sites, silently wrong when someone adds a
text path to a command that loads with `false`, and it leaks a storage detail
into every caller. The lazy re-read is worse still: it needs the index path
threaded through `SnapshotIndex`, and it re-opens a file that may have been
replaced since the load — a mutating command in another process rewrites it
atomically — so the section could disagree with the entries it was loaded
beside. Keeping the buffer costs the file's size in RSS until first use, which
is *less* than the decoded structure it stands in for, and the buffer is taken
and dropped by the decode, so a text query's steady state is unchanged.
Measured: the non-text path goes 396 ms → 151 ms and the text path 399 ms →
402 ms, i.e. deferring costs the text query nothing measurable.

**2. The save hazard is closed by forcing, not by carrying raw bytes.**
`save_to` calls `self.bm25.get()`, so a mutating command that never searched
text still decodes the section before re-serializing it. Splicing the raw
undecoded bytes into the new envelope would be faster and was the plan's other
option, but it means hand-assembling MessagePack around a `rmp_serde`-produced
document and keeping that splice correct against every future envelope change —
real risk of writing a subtly malformed index, for a saving on commands that
are already doing file I/O and a full re-scan. Mutating commands therefore pay
exactly what they paid before this change; nothing regresses, and the section
can never be silently dropped. Pinned by
`save_to_preserves_an_untouched_deferred_bm25_section` and by the e2e
`bm25_section_survives_a_mutating_command`.

**3. SEC-3 / MED-1 refuse the section, not the snapshot.** They used to run
before the index was exposed, and `load_inner` rejected the *whole* snapshot on
failure, falling back to a disk scan. A deferred section fails at first use,
mid-query, where that fallback no longer composes — the entries are already in
hand and the caller is inside `find`. The new contract: a section that fails
`validate_bm25` is dropped and the index reports "no BM25 section", which is a
state every caller already handles by live scanning. The crafted postings still
never reach `score()`, which is what SEC-3 and MED-1 exist to guarantee; only
the blast radius shrinks from "reject everything" to "reject the bad section".
The `load_inner_rejects_bm25_*` tests were adapted to the new control flow, not
weakened — they still assert the refusal, and additionally that it stays
refused on a second access rather than being retried.

**Also pinned:** `bm25_index_is_the_last_envelope_key` fails loudly if a future
field reorder moves the key, and the visitor itself degrades safely — it only
takes the early stop once `header`, `entries` and `graph` are all in hand, and
otherwise decodes the value in place (`envelope_with_bm25_not_last_falls_back_to_eager_decode`).
Slower, never wrong.

**Rejected:** the opaque length-prefixed BM25 blob, again — DEC-264 parked it
and early-stop worked, so it stays parked.

## DEC-266: an explicit non-`.md` extension is an attachment reference, and nothing crosses it (2026-09-03)

**Decision:** [[iterations/iteration-261-link-resolution-obsidian-compat]]
makes a link target that carries an explicit non-`.md` extension
(`img.png`, `Books.base`, `report.pdf`) a reference to a *vault file* rather
than to a note. Two halves:

**1. It resolves against every vault file, not just the notes.** The
case/stem index is now seeded with the vault's attachments as well as its
`.md` files, so `![[img.png]]` resolves by unique basename anywhere in the
vault, `![[sub/img.png]]` also resolves against the source folder, and
`[[Templates/Bases/Books.base]]` resolves by path — the same set of spellings
Obsidian's "shortest path when possible" setting accepts. A resolved one is
reported with `kind: "attachment"`: never broken, never in
`find --broken-links` / `summary.links.broken` / HYALO006, and never a graph
edge for `--orphan` / `--dead-end`. On kepano-obsidian 53 of 66 "broken" links
were `.base` embeds; on Obsidian Hub 83 were `.png` / `.gif` / `.jpg`.

Only files that *have* an extension are indexed as attachments. An
extension-less `LICENSE` would key on the same basename as `LICENSE.md` and
turn a link that resolves today into an ambiguous one; that trade is not worth
the handful of extension-less files a vault holds.

**2. `links fix` never matches across an explicit extension.** A broken
`Companies.base` may only be matched against a `*.base` file, and the fix
candidate set is the vault's notes, so it has no candidate at all and stays
honestly `unfixable`. Before this, `links fix` offered
`Companies.base → Templates/Company Template.md` (0.45) and
`Posts.base → Categories/Posts.md` (0.60), which
`--apply-fuzzy --min-confidence 0.5` would have written — turning a Bases embed
into a note link.

The same rule takes `[[beta.markdown]] → beta.md` off the table, which one
`index_journal` e2e relied on. That is the intended consequence, not
collateral: `.markdown` is not `.md`, Obsidian would look for a file literally
named `beta.markdown`, and the sanctioned extension repair (`foo` ↔ `foo.md`)
is a separate strategy that still runs.

**Also:** a fuzzy candidate whose composite confidence is `0.0` is no longer
reported at all. `[[lithou]]` was being listed against `lighthousedino.md` at
confidence 0.0 — a suggestion nobody would ever apply, and pure noise in the
report.

**Rejected:** a `[links] attachments = false` opt-out. Resolving an attachment
is what Obsidian does; a vault that wants its images reported as broken links
is not a vault hyalo needs to serve, and the project rule is no new surface
from dogfood pressure.

## DEC-267: link resolution folds case on every platform (2026-09-03)

**Decision:** case-insensitive *link resolution* is the default everywhere —
`find --broken-links`, `summary`, HYALO006, `backlinks`, `--orphan`,
`--dead-end`, `mv` — regardless of what the filesystem does. Opt out with
`[links] case_insensitive = "false"`. Exact-case still wins when both spellings
exist, because the literal path is tried before the folded lookup.

**Why not the filesystem probe.** `mode_enabled(Auto, dir)` asks "does this
filesystem distinguish `Foo.md` from `foo.md`?", which is the right question
for a `--file` argument naming a real path and the wrong one for a wikilink.
Obsidian resolves `[[AidenLx]]` to `People/aidenlx.md` on every platform, so
the same vault reported those 48 links as case-mismatched on macOS and as
broken on Linux. The probe still governs `--file` resolution and schema
exemption matching; only the link resolver was moved off it, via
`links_case_insensitive(mode)`.

**What `links fix --case-insensitive` now means.** It is *not* a no-op, and it
is not deprecated. Since resolution folds case, a case-only mismatch is never
broken and never counted under `broken`, so the flag no longer changes what
resolves — but it still suppresses the cosmetic `link-case-mismatch` rewrite
plans, which is a real thing to want on a vault (MDN's `en-US` / `en-us`) that
does not intend to normalise its link spellings. `case_mismatches` keeps
reporting them so an author who *does* want to normalise still can.

**Rejected:** removing the flag, and making it a warning-only no-op — the
plan's tentative proposal. Both throw away a control that still does something
useful; the narrower reading costs nothing and keeps `links fix --dry-run`
usable on a case-folded checkout.

## DEC-268: a dead anchor gets a suggestion, never a silent prefix match (2026-09-03)

**Decision:** when a link's `#fragment` names no heading in the target file but
is the **prefix of exactly one** heading there, the link carries
`suggested_fragment` with that heading's full text.
`[[decision-log#DEC-068]]` → `DEC-068: Snapshot index format`. Two or more
matching headings suggest nothing; so do zero. `find --broken-links` renders
it as `— did you mean "#DEC-068: Snapshot index format"?`, and the link stays
`broken_anchor: true` throughout.

This is option (a) from the plan — a suggestion — and it is deliberately a
*report*, not a rewrite. The own knowledgebase's 25 such anchors across 10
files are correctly broken per Obsidian, and the author is the one who knows
whether `#DEC-068` meant that heading or a heading that no longer exists.

**Rejected: (b), an opt-in `[links] anchor_prefix_match = true` that makes a
unique prefix *resolve*.** A silent prefix match hides typos — `#Task` would
quietly resolve against `## Tasks and open questions` — and it would make
`find --broken-links` answer differently for the same vault depending on a
config key, which is exactly the inconsistency DEC-267 just removed for case.

**Rejected: auto-applying the suggestion in `links fix`.** Same objection one
step further: an automatic rewrite of a fragment the author never wrote is a
guess written to disk, and the fix belongs behind human eyes. The suggestion
being visible in the report the `links fix` anchor note already points at
("N broken anchor(s) — see `find --broken-links`") is the whole affordance
needed.

## DEC-269: every frontmatter value is a link source, with a config opt-out (2026-09-04)

**Decision:** a `[[wikilink]]` written in **any** YAML frontmatter value is a
graph edge — a scalar (`type: "[[Author]]"`), a list item
(`categories: ["[[Books]]"]`), a value nested in a map, quoted or bare, at any
depth. It counts for `backlinks`, `find --orphan`/`--dead-end`/`--broken-links`,
`summary.links`, HYALO006 and the `--sort links_count|backlinks_count` keys,
`mv` rewrites it, and it is reported alongside body links under `--fields links`
with `kind: "frontmatter"`, the originating `property`, and the frontmatter line
it sits on. `related` is no longer special-cased.

`[links] frontmatter = false` in `.hyalo.toml` narrows the scan back to the four
legacy properties (`related`, `depends-on`, `supersedes`, `superseded-by`);
`[links] frontmatter_properties = [...]` names an explicit list and wins over
both. `hyalo config` reports the effective values under `links.frontmatter` and
`links.frontmatter_properties`.

**Why:** this is what Obsidian does, and the old four-property allow-list made
hyalo disagree with the vault in front of it. On kepano-obsidian,
`backlinks Categories/Books.md` was empty while `categories:` pointed at it from
two notes, `summary` reported 25 orphans that were all linked through
`categories:`/`type:`/`status:`, and `mv Categories/Books.md` reported
`total_links_updated: 0` while breaking those same links. The opt-out exists
because a vault may legitimately treat some frontmatter as metadata rather than
references; it degrades to the old behaviour rather than to *no* frontmatter
links, so no vault loses its `related:` graph by turning the new default off.

**Implementation note — the scan reads the raw frontmatter block, not the parsed
map.** Two things the parsed values cannot give: the source line every consumer
reports, and an unquoted `related: [[Books]]`, which YAML parses as a sequence
containing a sequence, brackets gone. So `frontmatter_links` walks the raw block
line by line with the same bracket scan `mv` and `links fix` already use to
*rewrite* frontmatter wikilinks — which is what keeps "hyalo counted this link"
and "hyalo rewrote this link" the same set — inferring the key path from
indentation and skipping `#` comments.

**Rejected: making it opt-in.** The default would then still disagree with
Obsidian, and the finding that motivated this was a silent `mv` corrupting
links hyalo itself had just counted.

## DEC-270: `set` on a list property writes the scalar and says so (2026-09-04)

**Decision:** `hyalo set K=<scalar>` on a property that currently holds a YAML
list replaces it with the scalar — option (a) from the plan. The files where the
type changed are listed under `list_collapsed` in JSON and named in a stderr
note that points at `hyalo append`. With `--validate` (or `validate_on_write`),
a schema declaring `K: list` rejects the scalar before anything is written, so
the enforcement path is unchanged.

**Why:** `set` means replace; that is the whole contract that separates it from
`append`, and silently preserving list shape would make `set` mean two different
things depending on what happened to be on disk. What was actually wrong was the
silence: Obsidian shows a property-type conflict across the vault afterwards, and
nothing in hyalo's output said the type had changed. Reporting it costs one
stderr line and one optional JSON key.

**Rejected: (b), preserving the list shape and writing a one-element list.** It
makes `set` behave differently on two files that differ only in prior state, and
it gives no way to *deliberately* turn a list into a scalar.

**Rejected: a `--keep-list` flag.** The project rule is no new CLI flags from
dogfood pressure, and `hyalo append` is already the command that keeps a list a
list.

## DEC-271: MD018 defers to Obsidian tag grammar, with a capitalization tiebreak (2026-09-04)

**Decision:** MD018 (`no-missing-space-atx`) does not fire on a line whose
single leading `#` is followed by a valid Obsidian tag token — letters, digits,
`_`, `-`, `/` or non-ASCII word characters, with at least one non-digit
character. Two or more hashes (`##todo`), a purely numeric token (`#1`,
`#2024`) and a punctuation token (`#!bang`) are not tags and keep firing.

Where the two grammars genuinely collide — `#Word more prose`, which is both a
tag followed by text and exactly what a heading missing its space looks like —
the tiebreak is capitalization: a **plain capitalized ASCII word** (initial
upper-case letter, then letters only: no digit, `-`, `_`, `/`, nothing
non-ASCII) followed by more text on the line stays flagged as a heading typo.
`#todo call the vet`, `#Project/alpha notes` and a bare `#Someday` are tags.

**Why:** the two errors are not symmetric. A missed heading typo leaves the file
byte-identical and costs a warning; a mis-"fixed" tag silently rewrites the
author's content and, on the Obsidian Hub vault, would have done so 162 times in
one `--fix` run. So the rule is biased toward exemption, and the one heuristic
that keeps the rule useful at all is the one that only affects the ambiguous
shape.

**Rejected: exempting only whole-line tags (`#todo` alone).** `#todo call the
vet` is the commonest daily-note shape in the corpus and is corrupted just as
badly.

**Rejected: dropping MD018 entirely, or default-disabling it.** It catches a
real typo class in prose-first vaults, and turning it off vault-wide is already
available as `hyalo lint-rules set MD018 --enabled false`.

**Where:** `crates/hyalo-mdlint/src/rules/obsidian.rs`
(`is_obsidian_tag_token` / `is_obsidian_tag_line`), applied as a post-filter in
`lint_body` next to the MD011 regex suppression. The token predicate is public
so a future tag rule reuses one definition of the grammar.

## DEC-272: MD001 is reported, never autofixed (2026-09-04)

**Decision:** MD001 (`heading-increment`) keeps warning about a skipped heading
level but carries no `fix`, so `lint --fix` never rewrites one. `hyalo
lint-rules list` reports `autofixable: false` for it, and the per-vault opt-out
for the warning itself stays `hyalo lint-rules set MD001 --enabled false`.
Implemented as a `NON_AUTOFIXABLE` table in `hyalo-mdlint`'s engine — a general
mechanism, not an MD001 special case — that both strips the fix and answers the
catalog.

**Why:** the fix renumbers a heading. On the Obsidian Hub vault it proposed 17
rewrites of deliberate `###### Caption` lines on CSS-snippet notes, turning them
into `##`. Correct per markdownlint, wrong for the author, and unlike a trailing
space there is no way to tell the two apart from the text. A skipped level is
still worth a warning, and the manual correction is a one-character edit.

**Rejected: default-disabling MD001 (option (b) in the plan).** That loses the
warning too, and the warning is the useful half.

**Rejected: a `--no-fix-rule` flag or an autofix allowlist in config.** No new
CLI surface from dogfood pressure; `--fix-rule` already restricts a run to named
rules, which covers the opposite need.

## DEC-273: one sort direction for every `find --sort` key (2026-09-04)

**Decision:** every `--sort` key orders **ascending** and `--reverse` inverts
it. `backlinks_count` and `links_count`, which used to sort descending, now
follow the rule, so `--sort backlinks_count --reverse` means "most linked
first" exactly as `--sort modified --reverse` means "newest first". `score` is
the one documented exception: it ranks best-match-first (descending relevance),
because "best first" is the only useful default for a relevance ranking, and
`--reverse score` is allowed and documented as "weakest match first".
`--reverse` is applied inside the comparator rather than by reversing the
sorted vector, so the `file` tiebreak stays ascending in both directions.

**Why:** `--reverse` meant the opposite thing depending on the key, with
nothing in `-h` or `--help` saying so. On the Obsidian Hub vault
`--sort backlinks_count --reverse --limit 3` returned 1-backlink files (and a
text-mode result whose `backlinks:` field was empty, because a file with no
backlinks prints none) while the bare sort returned the 2190-backlink hub — the
inverse of every other key. The two descending keys were almost certainly
copied from `score`'s comparator.

**Behaviour change.** A script that relied on `--sort backlinks_count` or
`--sort links_count` returning the most-linked file first must add `--reverse`;
one that passed `--reverse` to those keys must drop it. Flagged in the
changelog under Changed.

**Where:** `crates/hyalo-cli/src/commands/find/sort.rs` (`apply_sort` gained a
`reverse` parameter; `presort_index_entries` matches its ascending order), and
the `results.reverse()` call in `commands/find/mod.rs` is gone.

## DEC-274: null, empty-list and typed comparisons in `--property` (2026-09-04)

**Decision:** four value shapes and one comparison rule, all on the existing
`--property` flag (no new CLI surface):

- `K=null` matches a property **present** with a YAML null (`~`, `null`, or an
  empty value); `K!=null` matches present and non-null. A list *containing* a
  null (`aliases: [null]`) matches neither — the value's own type is tested, so
  `K=null` and `--fields properties-typed` (`type: "null"`) always agree.
- `K=[]` matches a present, empty list; `K!=[]` a present, non-empty one.
- The existing bare `K` / `!K` keep meaning present / absent.
- `<`, `<=`, `>`, `>=` classify both sides independently and compare only when
  the kinds agree: numeric when both parse as finite numbers (so `rating>=6`
  matches `rating: "7"`), by ISO date prefix when both parse as dates, textual
  only when both are plain strings. A value of any other kind (bool, null,
  list, map, or a string of the wrong kind) never matches.
- `--sort property:K` puts missing and null values last regardless of
  `--reverse`.

**Why:** `hyalo properties` reported `aliases: 2 null` on the Obsidian Hub
vault with no filter able to name those two files. And the old comparison fell
back to a lexicographic string compare across types, so `last>=2023-09-01`
matched the string `"[[2022-04]]"` and any date-shaped filter silently returned
wikilinks. Comparing text against a date is never what the caller meant, so the
right answer is "no match", not "an arbitrary total order".

**Rejected: a `--null` / `--empty` flag pair.** No new CLI flags from dogfood
pressure; the value slot of `--property` already reads as a value language.

**Where:** `crates/hyalo-core/src/filter/parse.rs` (four new `FilterOp`
variants), `filter/match_props.rs` (`CmpKind` classification in `yaml_cmp`),
`crates/hyalo-cli/src/commands/find/sort.rs` (`compare_nulls_last`).

## DEC-275: the typed-properties JSON key stays `properties_typed` (2026-09-04)

**Decision:** keep the snake_case JSON key `properties_typed` — every other
envelope key is snake_case — and accept **both** `--fields properties-typed`
and `--fields properties_typed` on the flag, so a printed field list round-trips
back into `--fields`. The mapping is stated in `find --help`'s RESULT SHAPE
section and in the `--fields` help.

**Why:** the flag value and the JSON key disagreed, so
`--jq '.results[0]["properties-typed"]'` returned null with no diagnostic.
Renaming the key would break every existing consumer and make one envelope key
kebab-case; accepting the second spelling costs one match arm.

**Where:** `crates/hyalo-core/src/filter/fields.rs`.

## DEC-276: `=~` is not an operator; empty patterns and empty `--fields` are errors (2026-09-04)

**Decision:** three rejections, all exit 1 (the exit code every other bad
argument in this CLI uses; exit 2 stays reserved for internal/system errors):

- `--property 'K=~/pat/'` → `unknown operator '=~' … use '~=' for a regex match
  (e.g. 'K~=/pat/')`. `=~` was never implemented as an operator; it "worked"
  only because `=` split first and `~/pat/` was then compared as a literal
  value — which is also why `--property 'aliases=~'` matched 5623 files on the
  Obsidian Hub vault: `~` is YAML null. When both spellings appear, whichever
  comes first wins, so `K~=a=~b` is still a regex whose pattern contains `=~`,
  and `K!=~foo` still compares against the literal `~foo`.
- `--property 'K~=//'`, `'K~=//i'` and `'K~='` → `empty regex in property
  filter …`. An empty pattern matches every value; bare `K` is the way to test
  presence.
- `--fields ''` and `--fields ,` → the same message the unknown-field path
  produces, listing the valid values. Silently yielding a `{file}`-only
  projection was a result nobody asked for.

The same parser backs `--where-property` on `set`/`remove`/`append` and
`--property` on `mv`, so all four get the identical errors.

**Breaking.** Anyone who relied on the `=~` accident must switch to `~=`, which
the help has always called the right spelling — and whose COMMON MISTAKES entry
contradicted the parser until now.

**Deviation from the iteration plan:** the plan's acceptance criteria asked for
exit code 2. Exit 2 is this CLI's internal-error code (iter-181); every
invalid-argument rejection — unknown field, unknown sort key, empty filter name
— exits 1, and these three belong to that class. Following the established
contract beats matching the number written in the plan.

**Where:** `crates/hyalo-core/src/filter/parse.rs`,
`crates/hyalo-core/src/filter/fields.rs`.

## DEC-277: `[scan] exclude` is the one vault-wide exclusion knob (2026-09-04)

**Decision:** a new `exclude` key in the existing `[scan]` section of
`.hyalo.toml` holds vault-relative globs whose files are dropped **at file
discovery**, which makes them invisible to every command without threading a
list through each one: `find`, `summary`, `tags`, `properties`, `lint`,
`links *`, `mv`'s link graph, `backlinks`, `create-index`, `views`, `types`,
`okf` and `madr`. It is hyalo's analogue of Obsidian's "Excluded files"; the
dogfood run on kepano-obsidian had no way to say "ignore `Templates/`" once
and be done, because the only knobs were per-feature (`[lint] ignore`,
`[okf] ignore`, `[schema] exempt`).

**Precedence.** Exclusion is the widest knob and runs first: a file it drops is
never seen, so the narrower per-feature lists only ever refine what survived.
They are kept, not folded in — `[lint] ignore` legitimately means "still part
of the vault, just not linted", which is a different claim.

**An explicitly named excluded file is refused, not skipped.** `--file
Templates/x.md` exits 1 with the matching glob quoted, rather than exiting 0
with an empty result. A script that asked for one specific file and got a
clean exit would read "excluded" as "nothing wrong here" — the same reasoning
iteration 204 used for `L-2` (naming one unparsable file is an error, not a
warning). Rejected: silently dropping it, and a `--force`-style override (the
project rule bars new CLI flags grown from dogfood pressure; editing the glob
is the intended escape).

**The exclusion also applies to `--index` reads**, filtered when the snapshot
loads rather than when it is built. Turning the knob therefore takes effect
immediately, with no `create-index` rebuild, and an index built by an older
version stays correct. Excluded sources are also dropped from the loaded link
graph, so a note only an excluded template linked to is still an orphan.

**Where:** `crates/hyalo-core/src/discovery.rs` (`set_scan_exclude`, applied in
the walker and in `resolve_file_ci`), `crates/hyalo-core/src/index.rs`
(`load_inner`), `crates/hyalo-cli/src/config.rs`, `crates/hyalo-cli/src/run.rs`.

## DEC-278: per-file skip diagnostics are collected, not streamed (2026-09-04)

**Decision:** a file a scan cannot use is recorded in a process-global
collector (`hyalo_core::warn::record_skip`) instead of being printed where it
was found. At the end of the run, stderr carries one line —
`warning: skipped N files with unparsable frontmatter (run hyalo lint --rule
HYALO005 for details)` — plus a second line of the same shape for files that
were unreadable for other reasons (invalid UTF-8, I/O). `-q` silences both.

The trigger: on kepano-obsidian, 28 Templater templates with `{{date}}` in
their frontmatter made `summary`, `find`, `tags`, `properties`, `lint`, `mv`
and `views` each print **251 stderr lines** of multi-line `serde_yaml`
excerpts. Nothing was wrong with any individual message; there were simply 28
of them, on every command, ahead of the answer the user actually ran.

**The detail is relocated, not removed.** `[scan] verbose_skips = true` in
`.hyalo.toml`, or `RUST_LOG=hyalo=debug` for one run, restores the per-file
excerpts verbatim. Rejected: a `--verbose` flag (project rule: no new CLI
surface from dogfood pressure — the existing `-q`, `RUST_LOG` and a config key
already cover the three audiences).

**`lint` keeps reporting each file individually** as `HYALO005`. Listing bad
frontmatter is what `lint` is for; the summary line is what points at it.

**Where:** `crates/hyalo-core/src/warn.rs`, `crates/hyalo-cli/src/warn.rs`
(`flush_skipped_files`, called from the existing `flush_summary`), and the
former `eprintln!`/`warn` call sites in `find`, `properties`, `tags`,
`link_rewrite`, `create_index`, `journal` and `commands::mod`.

## DEC-279: a malformed `.hyalo.toml` fails a gate, not just a write (2026-09-04)

**Decision:** the config-trust refusal from iteration 201 (DEC: a `.hyalo.toml`
that does not parse blocks mutating commands, reads continue on defaults with a
`-q`-proof warning) is widened to commands whose **exit code is a gate**:
`lint` in any form, `find --strict`, and `views run`. Those exit 1 with the
parse diagnostic. Every other read keeps warn-and-continue.

The reasoning is the same one that justified blocking writes, applied to a
different kind of damage. A broken config silently drops `[lint] ignore`, the
schemas and the saved views; `hyalo lint` then checked files the vault excluded,
validated against schemas it no longer had, and still exited 0. That is a green
CI build whose verdict was computed from rules nobody wrote — worse than a red
one, because nothing surfaces it. A command that answers *yes or no* has no way
to caveat its answer; a command that returns *data* does, and its warning is
already unsuppressible.

**"Gate" is defined by the exit code, not by read-vs-write.** `find` without
`--strict` is a report and keeps answering; `find --broken-links --strict` is
the documented CI gate and refuses. Rejected: making every read fail (it would
make a broken config unrecoverable without `--dir`), and leaving `lint` alone
with a louder warning (CI does not read stderr).

**Where:** `crates/hyalo-cli/src/mutation.rs` (`Commands::gates`),
`crates/hyalo-cli/src/run.rs`.

## DEC-280: `--index` refreshes what it was asked for, and warns only otherwise (2026-09-04)

**Decision:** one policy for stale-index handling, replacing three. A `--index`
run that **names** its files (`--file`, or a positional path, without `--glob`)
stat-refreshes exactly those entries — one `stat` each, re-scanning only when
mtime or size moved — and stays silent. A run that names nothing keeps the
existing whole-vault mtime heuristic and its
`index older than vault; results may be stale` warning.

Before this, `find --index` warned from a directory-mtime probe, `links auto
--index` did a per-file refresh, and `set --index` warned about staleness it
then fixed itself. The gap that mattered: `find --index --file just-appended.md`
answered from the snapshot, reporting the file's pre-append size and line count
while merely *warning* that something might be stale — a silently wrong answer
about a file the user had named.

**Rejected: an implicit full-vault refresh on every `--index` read.** That is
exactly the cost iteration 260 removed (396 ms → 151 ms on MDN), and it would
be paid by every query to fix a case that only arises for named targets.
Per-file stat is O(targets), not O(vault).

**Warn-but-serve stays the default** for unnamed runs: the directory-mtime probe
is a heuristic, and turning a heuristic into a refusal would make every indexed
query hostage to filesystem mtime granularity (iteration 247, S-2).

**Where:** `crates/hyalo-core/src/index.rs` (`refresh_if_changed_on_disk`),
`crates/hyalo-cli/src/mutation.rs` (`Commands::explicit_file_targets`),
`crates/hyalo-cli/src/run.rs`.

## DEC-281: a `type:` may be a string, a `[[Wikilink]]`, or a one-element list of either (2026-09-04)

**Decision:** schema type binding normalises the frontmatter `type:` value
before looking up `[schema.types.*]`. Three shapes bind:

- a plain string — `type: Author`
- a `[[Wikilink]]`, bare or quoted — `type: "[[Author]]"` → `Author`; an alias
  (`[[Author|writer]]`), an anchor and a directory prefix (`[[People/Author]]`)
  all resolve to the note name, the way a wikilink itself resolves
- a **one-element** list of either — `type: ["[[Author]]"]`, the shape
  Obsidian's own property editor writes for a link-typed property

A multi-element list, a map, a number or a bool names no type: it binds
nothing and `lint` reports it, now with a message that names the shapes that
work rather than the bare `expected string, got […]`.

The dogfood run on kepano-obsidian is the whole case. 15 of its notes write
`type: ["[[Authors]]"]`. `hyalo types set Authors --required categories`
reported success, wrote the schema, and then applied to nothing — `lint`
validated every one of those files against the default schema, and
`set --property rating=high --validate` accepted a value the schema forbade.
The command said yes and the vault behaved as if it had said no, which is worse
than refusing outright.

**Rejected: a `--match` flag or a per-type `match` config key.** The value
already names the type; needing a second declaration to say "and I mean it"
is CLI surface bought to work around a parser. If per-type *path* matching is
ever wanted, that is `[schema.types.X] match`, a config key, not a flag
(project rule: no new CLI flags from dogfood pressure).

**Rejected: binding a multi-element list to its first element.** `type: [a, b]`
is a genuine ambiguity — silently picking `a` would validate a file against a
schema its author did not choose. Reporting it is the honest answer.

**`types set '[[Authors]]'` is still an invalid type name.** Normalisation is a
*read* rule for what a vault already contains, not licence to write link syntax
into `.hyalo.toml`.

**Also decided: `types set --required K` infers the property type it
auto-declares.** `types set` has always auto-added a constraint for a required
field that has none, hardcoded to `type = "string"`. On a vault where `K` holds
lists that constraint is violated by every file the moment it is written — the
command creates the errors it then reports. The type is now inferred from the
values the vault already holds for `K` on files of this type (most common
inferred type wins; `string` when the vault has none), which is the same
information `hyalo properties` already surfaces.

**Where:** `crates/hyalo-core/src/schema.rs` (`normalize_type_value`),
`crates/hyalo-mdlint/src/schema.rs`, `crates/hyalo-cli/src/commands/types.rs`
(`infer_property_type_from_vault`), `set.rs`, `append.rs`, `lint/file.rs`.

## DEC-282: renaming a parent tag renames its whole subtree (2026-09-04)

**Decision:** `hyalo tags rename --from music --to audio` renames the tag
`music` **and** every nested `music/…` tag, matching Obsidian's own rename.
The match must land on a `/` boundary, so `music` never matches `musical`. The
parent need not itself occur: a vault holding only `music/genres` is renamed
rather than reported as `modified: (empty)`. JSON reports every tag actually
renamed under `renamed_tags: [{from, to, files}]` and the text output lists
them, so the expansion is never invisible.

Before this, the rename compared whole tags for equality. On kepano-obsidian
`tags rename --from music --to audio` printed `0 modified` while `music/genres`
sat in the vault — a no-op that looked like a successful run, and the shape a
user hits first, because a bare parent tag is precisely the tag one wants to
rename.

**Rejected: renaming only the exact tag and leaving children behind.** That
splits a hierarchy in half (`audio` beside `music/genres`) and is what Obsidian
users would call a bug in either direction; there is no reading of "rename the
`music` tag" that means "and orphan its children".

**Collision handling extends the existing per-file rule.** If a renamed tag
would duplicate one the file already carries, the duplicate is dropped rather
than written twice — the same "if the new tag already exists, only the old one
is removed" behaviour, now applied per renamed tag instead of once per file.
Only a duplicate the rename itself created is collapsed; a pre-existing
duplicate pair is left alone.

**Scope: frontmatter `tags:` only.** hyalo does not rewrite inline `#music`
body tags today, and this iteration does not add that — the rename is exactly
as wide as the data hyalo already owns.

**Where:** `crates/hyalo-cli/src/commands/tags.rs` (`tags_rename`,
`rename_nested_tag`), `crates/hyalo-cli/src/output/filters.rs`
(`TAG_RENAME_FILTER`).

## DEC-283: a title with no property and no H1 is the filename stem (2026-09-04)

**Decision:** the promoted `title` resolves in three steps, not two — a scalar
frontmatter `title`, else the first H1, else **the filename with `.md`
stripped**. JSON reports which step answered under `title_source`
(`"property" | "h1" | "filename"`), present exactly when `title` is.
`--title`, `--property 'title~=…'` and `--sort title` all read the promoted
value, so the three agree by construction. `HYALO007` (a `title` that cannot
promote) is unaffected: it is about the property, not about the fallback.

This supersedes DEC-255's "the honest answer is null, not a filename". The
honesty argument was right about one file and wrong about a vault: on the
Obsidian Hub, where notes carry neither a `title` property nor an H1,
`find --format text` printed `title: (none)` for every result and
`--sort title` collected the whole vault into one indistinguishable null
bucket (UX-5). Obsidian itself shows the stem in its file list and sidebar, so
the stem is not a fabrication — it is the name the author is already looking
at. What DEC-255 actually protected against was a consumer mistaking a derived
title for an authored one, and `title_source` answers that directly, which is
why the fallback is safe now and was not before.

**Rejected: a `--title-fallback` flag.** No new CLI surface from dogfood
pressure (standing project rule), and a per-run switch is the wrong shape for
a vault-wide property of how titles are read.

**Rejected: emitting `title_source` only under `--fields`.** It is meaningless
without `title` and would let an exact projection carry a provenance for a
value it dropped. It rides along with `title` and is never selectable alone —
so it is a companion key, absent from the `fields:` footer.

The stem strips a trailing `.md` and nothing else: `v0.22.0.md` promotes to
`v0.22.0`, not `v0.22` — `Path::file_stem` splits on the last dot and is
wrong here. A path with nothing left after stripping (`sub/.md`) keeps the
null.

**Where:** `crates/hyalo-cli/src/commands/find/build.rs`
(`extract_title_with_source`, `filename_stem`), `crates/hyalo-core/src/types.rs`
(`FileObject::title_source`), `crates/hyalo-cli/src/output.rs`
(`FIND_COMPANION_KEYS`), `crates/hyalo-cli/src/output/text.rs`
(`is_file_object_key`).

## DEC-284: naming a file overrides `[lint] ignore` (2026-09-04)

**Decision:** a path named explicitly — positionally, with `--file`, or through
`--files-from` — is linted even when `[lint] ignore` matches it. `--glob` and
the bare vault sweep keep honouring the list, and a `--glob` whose matches are
*entirely* ignored still explains itself on stderr.

Naming a file is a stronger, more recent and more specific signal than a glob
written once in `.hyalo.toml`. The previous behaviour — drop the file, then
warn that it was dropped — left no way to lint an ignored file at all, so the
answer to "why does this file have findings I cannot see?" was "edit the
config, run, edit it back".

**CI implication, and the reason this is the right default:**
`git diff --name-only | hyalo lint --files-from -` now lints changed files the
ignore list covers. For a diff gate that is the desired behaviour — the point
of a diff gate is that what you touched gets checked. A caller who wants the
ignore list applied to a set of paths selects them with `--glob` instead of
naming them; that is the documented opt-out.

**Rejected: a `--force` / `--no-ignore` flag.** This is a policy about what
naming a file *means*, not a new mode, and the project rule stands against
growing the flag surface from dogfood findings.

**Where:** `crates/hyalo-cli/src/commands/lint/run.rs` (target resolution),
`crates/hyalo-cli/src/cli/args.rs` (`lint --help`).

## DEC-285: `new` gains `--dry-run`, and un-fillable placeholders stay empty (2026-09-04)

**Decision, two halves.**

`hyalo new --dry-run` prints the scaffold and writes nothing — not the file,
not its parent directory — reporting `dry_run: true`, `created: false` and the
`content` it would have written. This is **parity, not new surface**: DEC-257
already makes `dry_run` a universal key on object-shaped mutation results, and
every other writing command has the flag. `new` was the only writer whose
preview you obtained by creating the file and deleting it again (UX-17).

Second half: a required `number`, `date`, `datetime` or `boolean` with no
schema `default` scaffolds as an **empty value** (`rating:`) rather than `0`,
today's date or `false`. Those three are values a reader takes at face value
and `lint` accepts — a scaffold that looks complete and is not. An empty value
is a schema error (`required property "rating" must not be empty`), so lint
names exactly the fields the scaffold could not know, which is what drives the
fill-in loop. A required `string` keeps `TBD` (it is visibly a placeholder and
already fails lint), an `enum` keeps its first declared value (schema-valid and
the only guess available), a list keeps `[]`, and a `default` declared in the
schema — including `$today` — is still emitted verbatim.

**Rejected: `TBD` for numbers and dates.** It violates the property's own
constraint, which iteration 181 already established is worse than omitting the
key.

**Rejected: omitting the key entirely** (the existing treatment for a
pattern-constrained string whose placeholder would be invalid). An empty key is
strictly more informative: the reader sees which fields are theirs to fill
without consulting the schema.

**Where:** `crates/hyalo-cli/src/commands/new.rs` (`create_new`,
`synthesise_content`, `PropValue::Null`), `crates/hyalo-cli/src/output/filters.rs`
(`NEW_RESULT_FILTER`).

## DEC-286: `links auto` holds back common-title candidates by default (2026-09-04)

**Decision:** the heuristic that has powered the noisy-title *warning* since
iteration 197 now powers an *exclusion*. Titles that are ordinary English
words, generic doc filenames or one of four platform/format names (`github`,
`gitlab`, `markdown`, `wiki`), plus any title that dominates the run (at least
25 proposed links and at least 2.5% of it), are held back. The report always
carries `default_excluded_titles` (the lowercased titles) and
`default_excluded_mentions`, and one stderr note names them with their counts
and the `--exclude-title` flags that reproduce the choice explicitly.

Warning about a list you have already handed over is the wrong shape. On the
Obsidian Hub a bare `links auto --dry-run` proposed 18,510 links, most of them
prose mentions of `github`, `links` and `Markdown`, and then advised excluding
them (UX-9) — a proposal nobody could review, plus homework. Holding them back
gives 7,014 reviewable proposals and a two-line account of what was left out.

**Two opt-outs, both existing config:** setting `[links.auto] exclude_titles`
hands the decision to your own list and the built-in stop-list steps aside
entirely (your judgment replaces it, it does not compose with it);
`warn_common_titles = false` / `--no-warn-common-titles` switches off both the
exclusion and the note, restoring the pre-iteration-267 all-candidates report.
No new flag.

**The word list stays narrow.** Product and domain nouns a vault genuinely
links to (`Obsidian`, `Dataview`, `Canvas`) and words that are plausible page
titles in their own right (`config`, `setup`, `template`) are deliberately
absent: the frequency trigger already catches the ones that actually flood a
run, and a false exclusion is invisible in a way a false proposal is not.

**Cost:** when the stop-list is active, a preview pass runs before the pass
that counts, because in `--apply` mode the pass that counts also writes and
must not write the held-back mentions. A run with nothing held back and no
write reuses the preview, so the common case stays at one pass.

**Supersedes** iteration 197's non-goal that "a vault that has not opted into
anything must see a byte-identical report". It does not: the report changes,
structurally and visibly, and the advisory prose still never reaches stdout.

**Where:** `crates/hyalo-cli/src/commands/links.rs` (`links_auto`,
`common_title_offenders`, `render_common_title_note`, `NoteMode`),
`crates/hyalo-core/src/common_words.rs`,
`crates/hyalo-cli/src/output/filters.rs` (`LINKS_AUTO_FILTER`).

## DEC-287: `object-list` is a flat, config-only schema type (2026-09-04)

**Decision:** the schema language gains an `object-list` property type for lists
whose items are maps, configured with three flat keys — `required-keys`
(present in every item), `allowed-keys` (the complete key set; omit it to allow
extras) and a `key-patterns` table mapping a key to a regex applied to that
key's scalar value. `list` and `string-list` could describe neither shape nor
per-key content, so a `sources:` list migrated from plain strings to
`- ref: … / commit: …` records had its contract living in a TOML comment.

Numbered 287, not the 286 the iteration-268 plan reserved: iteration 267 landed
first and took 286.

**The string item is the reason this exists.** `resolve_path` returns `None` for
a scalar with a remaining path, so a leftover plain-string entry in an otherwise
object-shaped list silently drops out of every `find --property sources.ref=…`
query — invisible, not merely wrong. Lint is now the thing that reports it, and
its message carries the fix-it text `- ref: <value>` so the repair is
mechanical. `dot_path_array_skips_scalar_items` pins the skip behaviour that
makes the pairing necessary; the skip itself is unchanged.

**Semantics.** Every item must be a map: a plain string is an error *with* the
fix-it hint, a number/bool/null/nested list an error without it. Keys in
`key-patterns` are optional unless also in `required-keys`; a non-scalar value
under a pattern key is an error, while numbers, bools and dates are matched
against their YAML text. Items are validated independently and **every**
violation is reported (no first-error cut-off, consistent with `item_pattern`),
each message naming the property, the 0-based item index and — where applicable
— the key. An empty list is vacuously valid; a non-list value is one error.

**Deliberately flat — this is not JSON Schema.** No nested maps or lists under a
key, no per-key types (`date`, `enum`), no cross-item uniqueness. *Rejected:
importing JSON Schema* — a whole second constraint language, its error messages
in someone else's vocabulary, for a need that is one level deep. *Rejected:
`list` plus per-key `pattern` sugar* — it reads as if the pattern applied to the
list, and gives nowhere to hang `required-keys`. *Rejected: nested types* —
every vault shape seen so far is flat records; nesting can be added later
without changing what is decided here.

**Config-only, no new CLI surface.** `types set --property-type` still rejects
`object-list`, exactly as it rejects `string-list` today, and gains no
`--required-keys` / `--allowed-keys` / `--key-pattern` flags; `types set --help`
says why. Authoring object *items* with `hyalo set` / `hyalo append` stays
unsupported and is an editor concern until a `set --property 'sources[]=ref=…'`
syntax is designed on its own merits (no backlog item filed). Write-time
*validation* falls out of the shared validator, so `--validate` already refuses
a scalar or string-item value for an `object-list` property.

**Regex compile-time asymmetry, accepted knowingly.** Every `key-patterns` regex
is compiled while `.hyalo.toml` is parsed, so an invalid one fails the schema
(`schema/malformed`, naming `property 'sources'`, `key-patterns.commit` and the
regex error) instead of being reported once per linted file. `item_pattern`
still surfaces an invalid regex per file at lint time and is **left as is**: the
load-time check is the better behaviour, but changing `item_pattern` now would
move an existing error from a per-file violation to a vault-wide config failure
for vaults that have one.

**Known gap, deliberately not closed here.** An invalid `key-patterns` regex
fails `SchemaConfig::try_from`, which `lint` reports as `schema/malformed` (an
error under `--strict`) — but a *write* does not gate on it: `set --validate`
prints the `-q`-proof `invalid [schema] in .hyalo.toml: …` warning, loads an
empty schema and writes anyway, so `--validate` is silently vacuous against a
broken schema. DEC-279 (iteration 265) scoped the broken-config gate to `lint`,
`find --strict` and `views run`, and writes are deliberately outside it; note
that this is about a `[schema]` that fails `TryFrom`, not a `.hyalo.toml` that
fails to parse at all, which does block every mutation. Making `--validate`
exit 1 on an unloadable schema would change behaviour for every schema error,
not just this one, so it belongs to its own decision. Pinned as-is by
`lint_reports_schema_malformed_for_an_invalid_key_pattern_regex`; the
iteration-268 plan's acceptance criterion asserting a refusal was mistaken
about today's behaviour.

**Fixed in passing: `autofixable` told the truth.** The SCHEMA group reported
`autofixable: true` for every violation except `missing-required-no-default`,
including `pattern` and `item_pattern` mismatches that `--fix` has no fixer for.
A new kind, `schema/constraint-violation`, now carries all three
(`pattern`, `item_pattern`, `object-list`) so their group reports
`autofixable: false`. No fixer is added: repairing a shape violation needs a
human decision about what the value should be.

**Note on key order.** `key-patterns` is stored in an `IndexMap`, but the config
passes through `toml::Value`, whose tables are sorted, so the keys arrive — and
`types show` renders them — alphabetically, not in file order.

**Where:** `crates/hyalo-core/src/schema.rs`
(`PropertyConstraint::ObjectList`, `RawPropertyConstraint`, `TryFrom`),
`crates/hyalo-mdlint/src/schema.rs` (`validate_object_list`,
`VIOLATION_KIND_CONSTRAINT_VIOLATION`),
`crates/hyalo-cli/src/commands/lint/file.rs` (the `autofixable` fix),
`crates/hyalo-cli/src/commands/types.rs` (`constraint_to_json`),
`crates/hyalo-cli/src/output/text_types.rs` (nested-map block),
`crates/hyalo-cli/src/commands/new.rs` (scaffolds as `[]`).

## DEC-288: `mv` looks past the backlinks graph, but only in single-file mode (2026-09-04)

**Decision:** `plan_mv` no longer scans only the files the backlinks graph
pointed at. Two shapes of reference are invisible to that graph and were
therefore silently missed, and both are now found by widening what `mv` looks
at rather than by widening what counts as a graph edge.

**A split frontmatter wikilink is not a backlink, and must not become one.**
`extract_frontmatter_links` deliberately refuses to emit a graph edge for a
`[[…]]` whose brackets straddle a line break (iteration 262, FM-1) — reading a
folded block scalar as a link would make `backlinks` / `summary` / `--orphan`
depend on YAML wrapping. That scope is unchanged here and is pinned by an e2e
test. What changed is that `mv`'s FM-2 *warning* used to inherit the same
blind spot: it only fired for a file the graph had already flagged for some
*other* link, so a file whose only reference to the moved target was the folded
link got neither a rewrite nor a warning — the exact silent-dangling-reference
case FM-2 exists to close.

**Mechanism: option (b), a marker on the build result — chosen on measurement,
not taste.** Option (a), the plan's simpler "sweep every file `plan_mv` has not
already opened and read just its frontmatter block", was implemented first and
measured on the Obsidian Hub vault (6,520 files, no split links present):
**0.25 s → 0.36 s** median of five, both directions, interleaved. That is a 44%
regression on a command whose whole cost is one graph build, and the sweep is
pure waste on the overwhelmingly common vault where no file has a split link at
all — 6,520 extra opens to find nothing.

So the flag rides along with the scan that already happened. `LinkGraphVisitor`
already receives each file's raw frontmatter text (`on_frontmatter_text`,
iter-262), and now records whether any line of it opens a `[[` that does not
close on that line — the exact opening test `split_frontmatter_wikilink` uses,
so a file the marker says no to provably cannot produce a report.
`LinkGraphBuild` carries the resulting paths as `split_frontmatter_candidates`
and `plan_mv` re-reads only those, which on a clean vault is none.

The coupling objection stands but lands softer than expected: the marker is on
`LinkGraphBuild`, the **build result**, not on `LinkGraph`. Nothing is
serialized into a snapshot, no query answers differently, `from_file_links`
(the `summary` path) leaves it empty, and the fact recorded — "this
frontmatter opens a bracket it does not close on the same line" — is a property
of the file, not of `mv`.

**NEW-3 folded into the same widening, one line lower.** An ambiguous bare
`[[b]]` cannot be resolved during the graph build, so it is indexed under the
*written* key `b` — a key `backlinks_ci("one/b.md")` never probes. The result
was that the ambiguity report only fired when one of the same-stemmed candidates
happened to sit at the vault root (whose `old_stem` *is* the bare stem), making
`mv`'s warning depend on which of two identically-named files was moved. `plan_mv`
now also probes the moved file's basename stem when it is nested. Every link that
extra key contributes is one the graph could not resolve — a resolvable stem was
already found under `old_rel` — so it reaches the unchanged ambiguity probe in
`plan_inbound_rewrites` and nothing new is rewritten by default.

**Batch `mv` is deliberately excluded.** `plan_batch_mv` returns bare
`RewritePlan`s and has never had a channel for `frontmatter_links_skipped` at
all, so giving it the sweep means changing its return type and the CLI's batch
output shape — a larger change than the three carry-over fixes this iteration
bundles, and one with no reported failure behind it. `mv --help` now states the
asymmetry instead of leaving it to be discovered.

**Where:** `crates/hyalo-core/src/link_graph.rs`
(`LinkGraphBuild::split_frontmatter_candidates`,
`frontmatter_opens_an_unclosed_wikilink`, the `LinkGraphVisitor` flag),
`crates/hyalo-core/src/link_rewrite.rs` (`plan_mv` steps 2/3b,
`scan_split_frontmatter_links`), `crates/hyalo-cli/src/cli/args.rs` (`mv` help).

## DEC-289: two lint rules narrowed at their boundaries, not suppressed (2026-09-04)

**Decision:** MD034 and MD047 each keep reporting; only the extent of what they
claim is corrected.

**MD034 stops the autolink at a `<`.** Upstream's end-of-URL scan does not treat
`<` as a boundary, so `https://…/Retroma<br>` (three occurrences on the Obsidian
Hub vault) fixed to `<https://…/Retroma<br>>` — the tag pulled inside the
autolink and the markup corrupted. hyalo narrows the fix's range and re-emitted
text instead of dropping the diagnostic: the URL really is bare and really
should be wrapped, just not that far. *Trimming at any `<` rather than only at a
tag-shaped `</?[A-Za-z]…` run:* RFC 3986 excludes `<` from the URI character set
outright, so a `<` inside a span claiming to be a bare URL is always adjacent
markup. *Rejected: suppressing the fix for this shape* (iteration 263's
under-fix bias) — the correct boundary is computable, so under-fixing would
leave a real finding unfixable for no gain. A URL followed by a bare `>` is
over-measured the same way but wraps to `<https://a.example/>>`, which still
renders as the autolink plus a literal `>`, so it is left alone and pinned by a
test.

**MD047 has nothing to check on an empty body.** A frontmatter-only file hands
the lint engine a zero-byte body and MD047 read "no bytes" as "no trailing
newline", reporting a file that plainly ends in one — a pre-existing bug, not
something iteration 267's `new --dry-run` work introduced. The rule is skipped
for a 0-byte body rather than given a fabricated diagnosis, extending the same
reasoning as the existing single-line-body guard one shape further down. A
non-empty body genuinely missing its terminator still fires and still fixes.

**Where:** `crates/hyalo-mdlint/src/rules/obsidian.rs`
(`bare_url_len_before_html`), `crates/hyalo-mdlint/src/engine.rs`
(`narrow_md034_autolink_fix`, the MD047 empty-body guard, `DESCRIPTION_SUFFIX`),
`hyalo-knowledgebase/docs/schema-and-lint.md` (Obsidian-grammar table).

## DEC-290: `--validate` refuses when the schema it names cannot be loaded (2026-09-04)

**Decision:** `set --validate` / `append --validate` — and a bare `set`/`append`
under `[schema] validate_on_write = true` — exit 1 and write nothing when a
`[schema]` section exists but fails `SchemaConfig::try_from`. `--dry-run` under
`--validate` refuses too. Every other command is unchanged: a `set`/`append`
*without* `--validate` still writes with the `-q`-proof
`invalid [schema] in .hyalo.toml: …` warning, and `mv`, `remove`,
`task toggle`, `links auto --apply` and all reads keep working on the empty
fallback schema.

This resolves DEC-287's "belongs to its own decision" note.

**The gate is scoped by the promise, not by the command.** DEC-279 defined its
gate by exit code — a command whose answer is *yes or no* cannot caveat that
answer, so `lint`, `find --strict` and `views run` refuse on a config that does
not parse. The same reasoning applied one level down picks out `--validate`
rather than "every write": the flag's entire content is "reject a value the
schema forbids before writing it". When `try_from` fails the run falls back to
an **empty** schema, which forbids nothing, so `--validate` returned 0 having
checked precisely nothing — the same vacuous-green failure DEC-279 was written
to stop, in the one place a user reaches for when they specifically do not
trust the value they are writing. A plain `set` makes no such claim and is not
lying when it writes.

**Rejected: gating every write on a broken `[schema]`.** That was the plan's
option 1 read literally, and it is materially different from the DEC-279 case
it looks like. A `.hyalo.toml` that does not parse loses `dir` itself — the
mutation would touch a tree the user never configured, which is why *every*
mutation refuses there. A rejected `[schema]` loses only the schema: `dir`,
`[lint] ignore` and the views are all intact, so `hyalo mv` still moves the
right file and `task toggle` still ticks the right box. Refusing those would
make a vault unusable for unrelated work while its schema is half-edited, and
`hyalo set` is one of the tools used to *do* that editing.

**Rejected: the plan's option 2 (keep writing, surface `schema_invalid: true`
in the result JSON).** It fixes detectability for a scripted caller while
leaving the default — a human at a terminal typing `--validate` — exactly as
wrong as before, and it asks every caller to add a check to get the guarantee
the flag already advertised. A result field is the right shape for information
the command *chose* not to act on; here there is a correct action available.

**Blast radius, checked rather than assumed.** Every `SchemaConfig::try_from`
rejection is a *config authoring* error, not a data error: mutually exclusive
`pattern`/`item_pattern`, `values` off an `enum`, `min-length`/`max-length` off
a `string` (or inverted), `minimum`/`maximum` off a `number`, the
`required-keys`/`allowed-keys`/`key-patterns` trio off an `object-list`, an
`allowed-keys` list that omits a name used elsewhere, an empty key name, an
uncompilable `pattern`/`item_pattern`/`key-patterns` regex, and an unknown type
name. None of them can be reached by editing a *note*: the vault always gets
into this state by editing `.hyalo.toml`, and gets out the same way. The
recovery paths are all short and none of them require a flag that does not
exist: fix the section, drop `--validate` for the one write, or `--dir` a vault
whose config is sound.

**`hyalo new` needed nothing.** It is the other command that reads the schema to
produce a write, and it already refuses: with the schema empty, `new --type X`
exits 1 with `type 'X' not found`. The message is indirect but the outcome is
right — no file is scaffolded from a schema that is not the vault's — so it is
left alone rather than given a second refusal path.

**No new CLI surface.** The behaviour rides entirely on the existing
`--validate` flag and the existing `validate_on_write` key; nothing is added to
the CLI.

**Where:** `crates/hyalo-cli/src/config.rs` (`parse_schema_from_toml` now
returns the diagnostic alongside the fallback schema;
`ResolvedDefaults::schema_invalid`), `crates/hyalo-cli/src/run.rs`,
`crates/hyalo-cli/src/dispatch.rs` (`CommandContext::schema_invalid`),
`crates/hyalo-cli/src/commands/mod.rs`
(`reject_write_with_unloadable_schema`), `crates/hyalo-cli/src/commands/set.rs`,
`crates/hyalo-cli/src/commands/append.rs`,
`crates/hyalo-cli/tests/e2e/config_trust.rs`,
`crates/hyalo-cli/tests/e2e/lint.rs`
(`lint_reports_schema_malformed_for_an_invalid_key_pattern_regex` re-pinned from
the vacuous write to the refusal).

## DEC-291: authoring `object-list` items stays an editor concern (2026-09-04)

**Decision:** `hyalo set` / `hyalo append` gain **no** syntax for writing a map
item into an `object-list` property. DEC-287's sketch
(`set --property 'sources[]=ref=…'`) is closed as won't-do, not deferred; no
backlog item is filed. Validation of object-list values on write is unchanged
and already works through the shared validator.

**No demand was found.** The plan made this conditional on evidence, and the
search came up empty: `object-list` appears in zero `.hyalo.toml` files across
this repo's own vault and both dogfooding testbeds (`../obsidian-hub`,
`../kepano-obsidian`), and no object-list property has been hand-edited in a
dogfooded vault since iteration 268 landed. The one real user — the
mapl-memory `sources:` migration that motivated the type — did the migration in
an editor and said so in its own request: *"`hyalo set --property` support for
appending object items is a separate concern; the type only needs lint +
`types show` output."* The feature request that produced `object-list` explicitly
did not ask for this.

**The symmetry argument fails on inspection.** The plan asked whether `find`'s
dot-path reads (`--property sources.ref=…`) could be reused for writes. They
cannot, and the reason is already settled in the code: `--property` on the write
side sets a *literal top-level key*, and `reject_dotted_property_collision`
exists precisely to refuse `set --property a.b=v` when `a` is a mapping, with a
hint saying hyalo does not support dotted paths for nested writes. Reads and
writes are not symmetric here because a read *addresses an existing value* while
a write must also say **which item** — the third one, the one whose `ref`
matches, or a new one — and that selector has no expression in the read syntax.
Reusing the syntax would either contradict an existing documented refusal or
silently pick an item for the user.

**The bespoke syntax is worse than the editor it replaces.** A flat
`sources[]=ref=…,commit=…` packs a second key/value mini-language into the value
half of a `K=V` argument that already infers types from `V`, and inherits its
own quoting problem: `key-patterns` regexes in the motivating example match
values containing `:`, `/` and `-`, and any item value containing a `,` or `=`
needs escaping rules that `set` has never had. That is a real parser and a real
error-message surface, added against the standing no-new-CLI-surface bar
(DEC-287, and the rule that already killed `--iteration` and `--strict-index`),
for a shape one `$EDITOR` invocation writes correctly the first time.

**What the user is left with is the half that catches mistakes.** `hyalo new`
scaffolds the property as `[]`, the editor writes the items, and `hyalo lint`
plus `set/append --validate` enforce `required-keys` / `allowed-keys` /
`key-patterns` afterwards — including the plain-string item that would otherwise
vanish from every `sources.ref=` query. Authoring was never the risky step;
drift was, and drift is covered. If a vault later shows repeated scripted
authoring of object items, this can be revisited with that evidence in hand —
which is exactly what it lacked here.

**Where:** no code. `hyalo-knowledgebase/docs/schema-and-lint.md` states the
outcome where it previously said "not supported" without saying why.

## DEC-292: concurrent writers to one file are not serialised — won't fix (2026-09-05)

**Decision:** hyalo does not serialise two of its own processes mutating the same
file at the same instant, and will not gain a lock to do so. Closed won't-fix by
the repo owner on the dogfood finding
[[dogfood-results/dogfood-v0220-post-batch-261-270]] BUG-1.

**The behaviour.** Every mutation is read → modify → write-temp → rename, guarded
by an `(mtime, size)` fingerprint taken at read time and re-checked before the
rename. Twenty parallel `hyalo set p.md --property kN=vN` processes all read the
same fingerprint before any renames, all pass the check, and the last rename
wins: 2–3 keys survive while 16–19 processes exit 0. A content hash would not
fix it — the window between check and rename stays open — only a lock would.

**Why won't-fix.** The race needs two hyalo processes writing *the same file* in
the same tens of milliseconds. None of the ways hyalo is actually used produce
that: a person on a PC in a repo runs one command at a time; a GitHub workflow
runs one job's steps sequentially; the iteration loop's agents never share a
file. Reaching it takes deliberate parallelism (`xargs -P`, `&` in a loop, two
agents pointed at one vault), and a caller who does that is outside the
contract. A lock would buy correctness for a case nobody has, at the cost of a
new failure mode everybody has: a stale lock after a crash, or a platform
dependency for the kernel-cleaned variant.

**Declined alternative, recorded so it is not re-proposed blind:** an
exclusive-create sidecar (`File::create_new` of `.<file>.hyalo-lock` around the
read-modify-rename) would turn the silent loss into a hard error for the losing
writer in ~20 lines with no global state, but needs a stale-lock rule of its own.
The `fd-lock` crate (`flock` / `LockFileEx`) avoids stale locks at the price of a
dependency. Either is the fix if a real workflow ever needs concurrent writers;
neither is worth carrying for a hypothetical one.

**Where:** no code. `hyalo --help` and the skill file may state "run mutations
sequentially; concurrent writers to one file are not serialised" if the
question comes up again; not added pre-emptively.

## DEC-293: the frontmatter closing fence is strict column-0, and hyalo never emits a block scalar that trips a lenient one (2026-09-05)

**Decision:** A frontmatter block closes on a line that is exactly `---` at
**column 0**, with trailing whitespace (a `\r`, spaces, tabs) allowed and
leading whitespace disqualifying — the same policy the opening delimiter has
always had. An indented `  ---` is content, not a delimiter.

Additionally, the frontmatter emitter never writes a **block scalar** whose
content carries a line that trims to `---` or `...`: such a value is written as
a double-quoted scalar (`k: "a\n---\nb"`) instead. The choice is made per
serialized value, so a document with no such string is emitted byte-identically
to before.

**Why the leniency existed.** iter-183 L-4 consolidated five parse paths
(`read_frontmatter_from_reader`, `find_body_offset`, `skip_frontmatter`, the
multi-visitor scanner, the body-scan loops) that had each independently closed
on `line.trim() == "---"`. Unifying them on one helper was right; keeping the
lenient predicate was the path of least resistance and was documented as
deliberate.

**Why it loses.** YAML and Obsidian both close only at column 0, so the
leniency was hyalo disagreeing with every other reader of the same bytes — and
disagreeing *destructively*. Given

```yaml
---
title: Ind
k: |-
  a
  ---
  b
after: 1
---
REALBODY
```

`read --frontmatter` returned `{"k": "a", "title": "Ind"}` — `after` gone — and
the next `set`/`append` spliced its new key over the block scalar's own text,
destroying the body. hyalo **produced this shape itself**: `set --property
"k=$(printf 'a\n---\nb')"` emitted exactly that block scalar, so the next
mutation corrupted a file hyalo had just written. `lint` reported nothing.

**Census (the cost of strictness).** Over five testbeds — the hyalo
knowledgebase (459 files with frontmatter), `obsidian-hub` (6509),
`kepano-obsidian` (98), `mdn/files/en-us` (14375) and `docs/content` (3707),
25148 files in total — the number of files whose frontmatter parses differently
under the lenient and the strict rule is **zero**. The leniency never rescued a
real file; it only mis-parsed the ones it broke. A file that closes only under
the lenient rule is malformed and now surfaces as "unclosed frontmatter" /
`HYALO005`, which is the honest verdict.

**Where:** `frontmatter::is_closing_delimiter` (the single canonical
predicate every path routes through), `hyalo_serializer_options_for` +
`has_document_marker_line` for the emitter guard. See
[[iterations/iteration-271-write-and-rewrite-safety]].

## DEC-294: `<!-- markdownlint-disable … -->` comments are honoured (2026-09-05)

**Decision:** `lint` and `lint --fix` honour markdownlint's own suppression
comments: `markdownlint-disable`, `-enable`, `-disable-line`,
`-disable-next-line`, `-disable-file` and `-enable-file`, each optionally
followed by whitespace- or comma-separated rule **ids** (`MD010`) or **aliases**
(`no-hard-tabs`), matched case-insensitively; with no ids the directive covers
every rule. It applies to the HYALO rules as well as the stock MD ones. A
directive inside a code fence is a sample, not a directive.

`markdownlint-capture` / `-restore` are **not** supported — they exist to save
and restore a configuration stack that hyalo does not model, and no corpus in
the testbeds uses them. MDN's `-nolint` info-string suffix
(```` ```html-nolint ````) is **not** supported either: it is an MDN build-system
convention, not markdownlint syntax, and reading fence info strings as lint
directives would silently change behaviour for every corpus that happens to use
a hyphenated language tag.

**Why.** MDN's whitespace guide wraps a deliberately tab-laden fence in
`<!-- markdownlint-disable no-hard-tabs -->`; hyalo recognised neither form and
`lint --fix` replaced the tabs in a page whose subject is tabs. Every markdown
corpus large enough to be worth linting has some region that is deliberately
"wrong", and the standard, portable way to say so is the comment markdownlint
itself defines. Inventing a hyalo-specific mechanism would have been a second
dialect for the same need.

It is cheap because iteration 271 Part D already computes HTML-comment spans
per line for the code-block exemption; the directive parser rides along on that
pass.

**Where:** `hyalo-mdlint`'s `rules::spans::BodySpans` (parsing and per-line
resolution) and the final `retain` in `HyaloLintEngine::lint_body`. See
[[iterations/iteration-271-write-and-rewrite-safety]].

## DEC-295: `links fix` does not report a case mismatch for a link that resolved through `site_prefix` (2026-09-05) — amends DEC-267

**Decision:** A `link-case-mismatch` plan is **not** produced for a link whose
target is site-absolute and carries the configured `site_prefix`
(`/en-US/docs/Web/CSS/Guides/Anchor_positioning` under
`site_prefix = "en-US/docs/Web/CSS"`). Such a link is correct *for the site*
and is left exactly as written. This is option (b) of the two the iteration
weighed; (a), "keep producing the plans with a corrected rewrite", is not taken.

Separately, and for every remaining strategy: **a rewrite keeps the incoming
form.** A directory link stays a directory link (trailing slash included), an
authored `.md` stays `.md`, and neither `/index` nor `.md` is ever appended to a
form that did not have it. The round-trip guard accepts the directory form of a
directory-index target, which is what lets the emitter keep it.

**Why.** DEC-267 folded case on every platform and called the resulting
`link-case-mismatch` rule cosmetic. On a copy of `mdn/files/en-us/web/css` the
cosmetic rule proposed **5096 rewrites across 1049 files** — a corpus whose URL
convention is deliberately Title-case over lowercase folders, so every one of
those "mismatches" was the site's own spelling. Worse, the written URL came out
as `/en-US/docs/Web/CSS/guides/anchor_positioning/index`: the directory-index
fallback's `/index` leaked into a published URL. A rule that is cosmetic by its
own description does not get to rewrite five thousand links, and a rewrite that
changes the *form* of a link is not a casing fix at all.

Option (a) was rejected because even a correctly-formed case rewrite of a
site-absolute link asserts that the on-disk folder casing is authoritative over
the site's URL convention. For a static-site corpus that is simply false, and
hyalo has no way to know which of the two the author meant. Doing nothing is
the only answer that cannot be wrong. Vaults that *do* want their site-absolute
links case-normalised can rename the files; `links fix` still fixes genuinely
broken ones.

**Where:** `link_fix::resolved_through_site_prefix` (the skip),
`emit_markdown_fix_target` + `raw_target_names_index` / `strip_directory_index`
(form preservation), `markdown_fix_round_trips` (the guard that accepts it). See
[[iterations/iteration-271-write-and-rewrite-safety]].

## DEC-296: a frontmatter `aliases:` value resolves a `[[wikilink]]` (2026-09-05)

**Decision:** `[[Leah]]` resolves to the note whose frontmatter declares
`aliases: [Leah]`. The rules:

- The property is **`aliases`** and nothing else — Obsidian's own spelling.
  Both shapes its property editor writes are accepted: a list, and a bare
  string (`aliases: Leah`). Non-string list items and blank entries are
  ignored. `alias:` (singular) and `title:` are **not** alias sources.
- **A filename or path match always beats an alias.** The alias map is
  consulted only after every path, `.md`-suffix, directory-index and bare-stem
  attempt has failed, so no vault can have an existing link repointed by
  someone else's frontmatter.
- **An alias claimed by two notes is ambiguous**, reported exactly like a
  colliding bare stem, never resolved to whichever note was scanned first.
- Matching **folds case**, like every other link lookup (DEC-267).
- `[[alias#Heading]]` and `[[alias|label]]` work: the fragment and the label
  are split off before resolution, as for any other target.
- The reported `kind` stays `wikilink` — an alias changes *what the target
  names*, not the syntax it was written in. The resolution is reported as
  `via: "alias"` on the link entry (absent for every other link, so an
  alias-free vault's output is unchanged byte for byte).
- An alias-resolved link is a **real graph edge**: `backlinks`, `--orphan`,
  `--dead-end`, `summary.links` and HYALO006 all agree with
  `find --fields links`.
- `links fix` never proposes a rewrite for a target that resolves through an
  alias, and never fuzzy-matches one.
- `mv` does not touch aliases and does not rewrite alias-written links: the
  alias travels with the note, so the link keeps resolving at the new path.
- **`[links] aliases = false`** turns the whole rule off and restores
  filename-only resolution. Default **on**, like DEC-267 case folding.
  `hyalo config --jq '.results.links.aliases'` reports the effective value.

**Index:** the alias map is built **at load, from the frontmatter the index
already carries** — no snapshot format change, so an index written by an older
hyalo resolves aliases identically. The disk path builds it with a
frontmatter-only scan (the visitor stops before the body), and the link-graph
build reads aliases in the pass that already enumerates every file.

**Why.** On the Obsidian Hub vault 7 of the 47 genuinely-broken targets (9
occurrences) are declared aliases of notes that exist, and the vault declares
5489 distinct aliases. Worse than the false "broken" verdict was what
`links fix --apply-fuzzy` did with them: `Leah → Lewuathe.md` at 0.87,
`Cat → CatMuse.md`, `jamesb → jamesgreenblue.md` — three confident rewrites of
links that were already correct. A resolver that does not know about aliases is
not merely incomplete on an Obsidian vault, it is dangerous on one.

**Where:** `case_index::CaseInsensitiveIndex::{insert_aliases, lookup_alias,
lookup_alias_all}`, `discovery::{resolve_target, resolves_via_alias,
populate_aliases_from_dir, read_aliases, set_link_aliases}`,
`discovery::classify_short_form_wikilink`, `link_graph::insert_file_links`,
`filter::extract_aliases`. See
[[iterations/iteration-272-resolution-completeness]].

## DEC-297: `![alt](img.png)` is inventoried as an embed (2026-09-05)

**Decision:** The markdown *image* form is extracted like every other markdown
link and flagged `embed`, so it is reported as `attachment` when it resolves to
a vault file and stays visible when it resolves to nothing. It is still never a
graph edge, never fuzzy-matched and never rewritten by a plain `links fix`.

**Why.** `![[img.png]]` and `[alt](img.png)` were both inventoried while
`![alt](img.png)` — the form every static-site corpus writes — was dropped at
extraction. MDN's whole-vault histogram therefore reported `attachment: 2`
against thousands of images, and a missing image surfaced nowhere: not in
`find --fields links`, not in `--broken-links`, not in `summary`.

**Where:** `links::extract_links_and_anchors`. See
[[iterations/iteration-272-resolution-completeness]].

## DEC-298: the `suggested_fragment` prefix test folds `-`, `_` and space (2026-09-05) — amends DEC-268

**Decision:** When deciding whether a dead fragment is the unique prefix of one
heading, `-`, `_` and a space are one character class. **Only the suggestion is
affected.** DEC-268's rule that a prefix match never silently *resolves* an
anchor is untouched: the suggestion is still printed, never applied, and still
requires a unique match.

**Why.** MDN slugs its headings with underscores (`#Browser_compatibility`)
while the heading itself is written with spaces, so a strict comparison found
no prefix for **1242 of the 1254** broken anchors on an MDN CSS copy — every
one of them a heading the reader could see two lines away. DEC-268 forbids
silent matching, not helpful suggesting.

**Where:** `anchor::{unique_heading_by_prefix, prefix_matches_fragment}`. See
[[iterations/iteration-272-resolution-completeness]].

## DEC-299: block-reference and slug anchors stay unchecked — backlog (2026-09-05)

**Decision:** `[[Target#^block-id]]` is **not** reported as a broken anchor,
and neither is a slug-shaped fragment that matches no heading beyond what
DEC-075 already accepts. Backlogged, not rejected.

**Why.** Obsidian does break `#^nope`, so hyalo is genuinely incomplete here —
but checking it needs a block-id scan (`^id` markers at the end of a block),
which is a new indexed field, a snapshot format addition and a new class of
false positive on every corpus that writes `^` for other reasons. That is its
own iteration, not a line in this one. Reporting them broken *without* the scan
would be strictly worse than saying nothing: every block reference in every
Obsidian vault would be a false positive.

**Where:** `anchor::is_block_ref` (the current skip, unchanged). See
[[iterations/iteration-272-resolution-completeness]] and the carry-over item
[[backlog/link-resolution-block-reference-and-slug-anchors]].

## DEC-300: no `[links] redirect_property` — backlog (2026-09-05)

**Decision:** A config key that reads a `redirect_from:` list as extra
resolution targets is **not** added in this iteration.

**Why.** It looked like a few lines on top of DEC-296's alias map, and it is
not: the alias map is keyed by *bare note name* and is consulted only for
targets with no path separator, because that is the only shape an Obsidian
alias can take. GitHub Docs' `redirect_from:` values are **site-absolute URL
paths** (`/actions/reference/workflow-syntax`), which resolve through
`strip_site_prefix` and the path lookup, never through the name map. Supporting
them means a second, path-keyed alias map with its own precedence against the
directory-index rule — a different feature wearing the same word. The 1569
"broken" GitHub Docs files stay reported; a vault that wants them resolved can
set `site_prefix` or fix the links.

**Where:** carried over as [[backlog/link-resolution-redirect-property]].

**Where:** nothing implemented; recorded so the option is not re-litigated. See
[[iterations/iteration-272-resolution-completeness]].

## DEC-301: a path the caller names is a promise, not a filter (2026-09-05) — amends DEC-278, DEC-280, DEC-284

**Decision:** For `find`, a path supplied as `--file` or positionally is
answered *about that path*, never quietly dropped:

- **Unparsable frontmatter is an error** (exit 1, the YAML diagnostic in
  `cause`, a `lint --rule HYALO005` hint), matching what `set`/`remove`/`append`
  have done since iteration 204's L-2. `--files-from` keeps DEC-284's batch
  semantics: the same file is counted as a skip and the run exits 0.
- **`--index` reads it from disk when the snapshot has never seen it.** One
  `is_file` plus one parse per named path, upserted into the in-memory snapshot
  (never written back), announced with a `note:`. A path in neither the
  snapshot nor the vault keeps `find`'s existing L-7 refusal (`file not found`,
  exit 1) rather than being downgraded to a `files_missing` counter — the
  refusal is strictly stronger and matches the non-`--index` path.
- **The broken-anchor verdict does not depend on how the file was selected.**
  `--file` / `--glob` narrow the scan, so the *target* of an anchored link was
  usually absent from the index and `broken_anchor` silently defaulted to
  `false`. The verdict now falls back to one memoized read of the target file's
  headings, so `--file`, `--glob`, positional and the vault sweep return
  identical link JSON, `suggested_fragment` included.
- **`lint --rule X` reports rule X.** A frontmatter parse error is HYALO005's
  finding; under a filter that does not name HYALO005 the file becomes a counted
  skip (DEC-278's one-line summary) instead of inflating `--count` for an
  unrelated rule.

**Why.** All four were the same failure: exit 0 with a clean-looking answer to a
question about a file the caller had named by hand. A script cannot tell that
from "the file matched no filter", which is exactly the ambiguity DEC-278's
skip-summary was allowed to create for *batch* scans and was never meant to
create for a named one. Batch and named are different contracts; the code had
lost the distinction because `--files-from` is flattened into the same `file`
list before dispatch (now tracked by `CommandContext::file_list_from_files_from`).

**Where:** `commands::NamedFilePolicy` + `build_scanned_index_named`,
`find::run::refresh_named_files_into_snapshot`, `find::anchor_verdict` +
`hyalo_core::index::scan_file_sections`, `lint::file`'s parse-error branch.
See [[iterations/iteration-273-index-and-named-file-honesty]].

## DEC-302: the stale-index probe falls through to per-file mtimes (2026-09-05) — amends DEC-280

**Decision:** When the DEC-280 directory-mtime probe finds nothing, a second
pass compares each indexed file's recorded mtime against disk and stops at the
first drift, naming that file in the `index older than vault` warning. It runs
only when the cheap probe was clean and the run did not already refresh every
file it named.

**Why.** DEC-280's probe watches directory mtimes, which move when an entry is
created, renamed or removed — and *not* when an existing file is overwritten in
place. On APFS, `printf … > n2.md` over an indexed note left the whole vault
looking untouched, so `find --index` served the pre-edit snapshot with no
warning and exit 0. Since the index already stores every file's mtime, the
detection is a comparison, not a scan.

**Cost.** One `stat` per indexed file in the worst case (a clean vault, where
the pass runs to completion). Measured on MDN's 14,375 files: `find --index
--limit 1` went from 0.12 s to 0.15 s — about 0.03 s, inside the 0.1 s budget,
and paid only by runs the cheap probe already declared clean. A dirty vault
short-circuits at the first drifted file.

**Residual blind spot.** An edit landing in the same whole second as the
snapshot: mtimes are compared in whole seconds with a one-second tolerance
(`STALENESS_TOLERANCE_SECS`), unchanged from DEC-280.

**Where:** `hyalo_core::index::first_file_modified_since_snapshot`, called from
`run.rs`'s snapshot-load path. See
[[iterations/iteration-273-index-and-named-file-honesty]].

## DEC-303: the snapshot records what `[scan] exclude` dropped (2026-09-05) — amends DEC-277

**Decision:** `SnapshotHeader` carries `scan_excluded` (the count of files
`[scan] exclude` dropped while the index was built) and `scan_exclude` (the
patterns that dropped them). On load, the stored count seeds
`summary`'s `excluded` figure **only** when the configured patterns still match
the recorded ones. Both fields default and are skipped when empty, so older
snapshots load unchanged and an unexcluded vault's bytes are identical.

**Why.** DEC-277 applies exclusions at snapshot *load* as well as on the disk
walk, which correctly handles a snapshot built before the exclusion existed. It
cannot handle the ordinary case: `create-index` run *with* the exclusion in
force never puts those files in the snapshot, so nothing remains at load to
count and `summary --index` reported `excluded: 0` where the disk scan reported
52. The build-time figure is the only witness, and it is one integer.

**Where:** `hyalo_core::index::SnapshotHeader`, `write_snapshot`,
`SnapshotIndex::load`, `discovery::scan_exclude_patterns`. See
[[iterations/iteration-273-index-and-named-file-honesty]].

## DEC-304: `mv` resolves the destination exactly like the source (2026-09-05)

**Decision:** Every `mv` destination — positional `DEST`, `--to <file>`,
`--to <dir>/`, and batch — goes through the same CWD-relative → vault-relative
normalisation `resolve_file` applies to the source, in one place in the dispatch
arm. A trailing slash survives the strip, because single-file mode reads it as
"this is a directory".

**Why.** With `dir = "kb"` in `.hyalo.toml`, `hyalo mv kb/a.md kb/sub/a.md` run
from the project root resolved the source to `a.md` and the destination to the
literal `kb/sub/a.md` — itself vault-relative — and created `kb/kb/sub/a.md`.
All four forms had it, because all four bypassed the normalisation the source
goes through. Normalising once, at the single point where the destination is
decided, is what stops the four drifting apart again.

**Also:** `--to dir/` naming a directory that does not exist used to be answered
with `did you mean dir/.md?`, a path nothing can name. It now reports the
missing directory and offers the explicit file destination.

**Where:** `commands::mv::strip_vault_prefix_from_destination`,
`validate_target_single`. Closes
[[backlog/done/mv-destination-path-resolved-vault-relative]]. See
[[iterations/iteration-273-index-and-named-file-honesty]].

## DEC-305: `--on-conflict` is a validated choice honoured in both `mv` modes (2026-09-05)

**Decision:** `mv --on-conflict` is a clap value enum (`error` | `skip`);
anything else is a usage error listing the real values. Single-file mode honours
`skip`: the source stays where it is, the destination is untouched, the path is
reported under `skipped`, and the run exits 0. The batch collision error
distinguishes "two sources map to one destination" from "a file already exists
at the destination" (and says so when a batch hits both).

**Why.** As a `String`, `--on-conflict overwrite` parsed cleanly and then behaved
as `error` — the worst possible answer from a flag whose only job is to say what
happens to your files. And `skip` was accepted in single-file mode and ignored,
so the run failed with `target file already exists`: the exact outcome the flag
exists to avoid. The two collision kinds have different fixes (rename a source
vs deal with the file already there), so one sentence could not serve both.

**Where:** `cli::args::ConflictPolicy`, `commands::mv::validate_target_single`,
`build_rename_map`. See
[[iterations/iteration-273-index-and-named-file-honesty]].

## DEC-306: batch `mv` sweeps for split frontmatter links, once per batch (2026-09-05) — amends DEC-296's scope note

**Decision:** Batch `mv` runs the same split-frontmatter-link sweep single-file
`mv` has run since iteration 269 (SCAN-1), reusing the candidate list the one
link-graph build already produces. Each candidate's frontmatter block is read
**once** and tested against every rename, so the sweep costs the same for one
move as for two hundred. Each move reports its own findings under
`moves[].frontmatter_links_skipped`.

**Why.** A frontmatter `[[…]]` that straddles a line break is not a graph edge,
so the file holding it is not a backlink source and the batch never opened it —
`mv --glob` could leave a dangling reference behind with nothing on stderr,
while `mv <file>` on the same target reported it. A guarantee that depends on
which spelling of the command you used is not a guarantee.

**Where:** `link_rewrite::scan_split_frontmatter_links_batch`,
`plan_batch_mv` → `BatchMvPlanResult`. Closes
[[backlog/done/mv-batch-frontmatter-link-scan-gap]]. See
[[iterations/iteration-273-index-and-named-file-honesty]].

## DEC-307: the exit-code taxonomy is 0 / 1 / 2, and every hyalo-own user error is 1 (2026-09-05) — amends DEC-276

**Decision:** hyalo has exactly three exit codes, and each means one thing:

- **0** — the command answered. A dry run has answered; so has a query that
  matched nothing. Drift, findings and "there is work to do" are reported in
  the payload, never in the exit code, unless a `--strict`-style gate flag was
  passed explicitly.
- **1** — a *hyalo-own user error*: the invocation was understood but wrong.
  A bad `--sort` key, an unparseable `--glob`, an unreadable `--files-from`
  list, an unknown `init --profile`, a `create-index --output` into a directory
  that does not exist, `find a b` (hyalo's own did-you-mean-quotes error),
  `deinit --dir <nonexistent>`, a broken `.hyalo.toml` under a gate command.
  Every one of these renders through the standard error envelope, so
  `--format json` stays parseable.
- **2** — clap usage errors (an unknown flag, a missing required argument) and
  internal errors (a panic-equivalent I/O or serialization failure).

**Why.** The line that matters to a caller is 1 vs 2: "I typed it wrong" versus
"hyalo broke". Several user errors reached the top of the process as plain
`anyhow` values and were therefore reported as internal — bare text on stderr
and exit 2 *even under `--format json`* — so a script could neither parse the
message nor tell a typo from a crash. Two commands went the other way and
signalled a *finding* through the exit code: `okf index`'s dry run exited 1 on
drift, which made the safe habit of previewing first look like a failure to
every wrapper that checks `$?`.

**Where:** `hyalo_core::user_error` / `user_error_with` build the
`UserFacingError` marker; it survives `?` and added `.context(...)`, and the
handlers in `run::run` and `output_pipeline` re-render it through
`format_error` at the effective `--format` and exit 1. `okf index --dry-run`
now exits 0 and reports drift as `results.changed` (amending the
iteration-176 choice). See
[[iterations/iteration-274-hints-help-and-contract-polish]].

## DEC-308: a bare `[[alias]]` is broken, and the alias map's job is to fix it (2026-09-05) — amends DEC-296

**Decision:** `[links] aliases` defaults to **false**. A hand-typed `[[Leah]]`
naming a note's frontmatter `aliases:` is reported **broken** — the way Obsidian
renders it — and counts in `summary.links.broken`, `find --broken-links` and
HYALO006. The alias map is still built in both modes, and does two other jobs:

- `links fix` plans the rewrite Obsidian's own link suggester writes —
  `[[Leah]]` → `[[Leah Ferguson|Leah]]` — in its own `alias_fixes` /
  `alias_fix_plans` bucket, strategy `Alias`, confidence 1.0, applied by plain
  `--apply` and **never** routed through the fuzzy matcher. An existing label
  survives (`[[Leah|boss]]` → `[[Leah Ferguson|boss]]`); an embed or a markdown
  destination rewrites the target only. The emitted target is the note's
  vault-relative path, exactly as every other wikilink fix emits it.
- `find` labels such a link `via: "alias"` whether or not it resolved, so text
  mode prints `(unresolved) (via alias)` — the marker `find --help` has
  promised since iteration 272 and never delivered (BUG-32).

`[links] aliases = true` restores iteration 272's behaviour verbatim, for vaults
running the Alias Linker community plugin.

**Why.** DEC-296's premise was wrong. Obsidian does **not** resolve a bare
`[[alias]]`, by design: aliases feed the link *suggester*, which inserts
`[[Artificial Intelligence|AI]]`, and a hand-typed `[[AI]]` is an unresolved
link whose click creates a new note. Verified against the Obsidian help page on
aliases and the forum thread *"Wikilink resolution does not honor frontmatter
aliases (1.12.7)"*, where a moderator states "This is not a bug … it's an
intentional design decision"; the community plugin **Alias Linker** exists
precisely to patch it in. On the Obsidian Hub, 51 links carried `via: "alias"`
and were excluded from every broken-link report although Obsidian shows them
dead — and `links fix` could not propose the one rewrite that is right for
them. Reporting a link as fine because *hyalo* can resolve it, when the editor
the vault is written for cannot, is the opposite of what a linter is for.

**Where:** `discovery::link_aliases_enabled` (default `false`),
`discovery::resolve_target`'s alias fallback, `stem_classification`,
`link_fix::alias_fix_target` + `FixStrategy::Alias`,
`commands::find`'s `via` label, `config::LinksConfig::aliases`. See
[[iterations/iteration-275-alias-semantics-and-mv-guards]].

### DEC-296 addendum

DEC-296 is **superseded in part by DEC-308**. Its mechanics stand — unique
alias, filename beats alias, shared alias ambiguous, case-folded matching, the
scalar form, `[[alias#h]]` / `[[alias|label]]`, frontmatter links with their
`property`, index parity, `mv` leaving alias links alone — and are exactly what
`[links] aliases = true` still does. Only its premise ("Obsidian resolves a bare
alias") was wrong, and with it the default. DEC-308 adds one rule DEC-296 got
wrong in *both* modes: an **ambiguous filename match is still a filename
match**, so an alias never breaks the tie (`[[avatar]]` with `Plugins/avatar.md`
aliased `Avatar` and `Themes/Avatar.md` present is `path: null` in `find`,
`backlinks`, `summary` and `mv` alike — previously `find` claimed the plugin
while `mv` called it ambiguous, so a rename silently repointed the links).

## DEC-309: `-`, `_` and a space are one word separator when *resolving* an anchor (2026-09-05) — amends DEC-268 and DEC-298

**Decision:** Anchor resolution folds the three interchangeable word separators,
not just the anchor *suggestion* DEC-298 folded them for. `#Browser_compatibility`
resolves to `## Browser compatibility` outright. The fold is applied to the
GitHub slug of both sides, so punctuation is still dropped the way DEC-075
drops it, and the duplicate-slug suffixes (`-1`, `-2`) still disambiguate.
`anchor::unique_heading_by_prefix` also stops excluding an *equal-length*
match (`<`, not `<=`), so the most useful suggestion there is — the whole
heading — is finally offered.

**Why.** MDN slugs its headings with underscores while writing the heading text
with spaces, so the two conventions disagree on exactly one byte. hyalo reported
**10 929** dead anchors on an MDN checkout with only 251 suggestions; every one
of the unsuggested ones named a heading the reader could see two lines away.
DEC-268 forbids *guessing* a heading, and this is not a guess: no two headings
in the wild differ only in which separator joins their words, and a renderer
that emits one form is read by an author writing the other. DEC-298's own
motivating example (`#Browser_compatibility`) was exactly this shape and still
did not resolve.

**Where:** `anchor::fold_separators`, `anchor::fragment_matches_headings`,
`anchor::unique_heading_by_prefix`. See
[[iterations/iteration-275-alias-semantics-and-mv-guards]].

## DEC-310: a wikilink target is trimmed before resolution, and `.` segments are dropped (2026-09-05)

**Decision:** `resolve_target` trims its input and drops `.` path segments, so
`[[ a ]]`, `[[a ]]`, `[[a #Heading]]` and `[[./a]]` all resolve to `a.md` and
report `path: "a.md"`. The link's `target` still carries the text the author
wrote — the trim is a resolution rule, not a rewrite — and `mv` rewrites the
trimmed form.

**Why.** Obsidian trims, so `[[ Leah Ferguson ]]` opens the note and hyalo
called it broken; HYALO006 fired on a link that works. `[[./a]]` was the mirror
image: it resolved, but reported `path: "./a.md"`, a spelling no other command
answers with, so `backlinks`, `find --broken-links` and any `--jq` grouping by
path saw two different files.

**Where:** `discovery::resolve_target`, `discovery::stem_classification`,
`discovery::resolves_via_alias`. See
[[iterations/iteration-275-alias-semantics-and-mv-guards]].

## DEC-311: `[[note#Heading One#Sub Two]]` is a heading *path*, resolved by walking the outline (2026-09-05) — sits beside DEC-299

**Decision:** A fragment containing an inner `#` is read as Obsidian's heading
path: each segment must match a heading nested inside the one before it, within
that heading's own subtree. `[[t#Heading One#Sub Two]]` resolves when `Sub Two`
sits under `Heading One`; `[[t#Heading One#Elsewhere]]` is a broken anchor even
though both headings exist, because `Elsewhere` is under `Other`. Separator
folding (DEC-309) applies per segment. A heading that genuinely contains a `#`
still matches through the ordinary literal checks, which run afterwards.

**Why.** Implemented rather than deferred because it is a resolver-only change:
`OutlineSection` already carries `level` and document order, which is everything
a nesting check needs. The alternative was to keep reporting a correct Obsidian
link as a dead anchor, and the fragment text (`Heading One#Sub Two`) made the
report unreadable as well as wrong.

**Where:** `anchor::heading_path_matches`, `anchor::segment_matches_heading`.
See [[iterations/iteration-275-alias-semantics-and-mv-guards]].
