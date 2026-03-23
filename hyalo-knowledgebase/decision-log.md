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

## DEC-003: ~~`--path` for File Targeting~~ (2026-03-20) — SUPERSEDED by DEC-018

**Decision:** ~~Single `--path` flag for all file targeting.~~ Replaced by `--file` (single file) and `--glob` (pattern). See [[decision-log#DEC-018]].

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

**Superseded by:** Iteration 9 introduced vault-wide and glob-scoped task support (`--file`, `--glob`, or no scope flag). The multi-visitor scanner ([[decision-log#DEC-028]]) made this feasible — each file is opened exactly once regardless of how many data dimensions are collected. Vault-wide tasks are now consistent with the tags API and give LLM agents a single-call way to find all open work.

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

**Context:** After building summary, outline, and tags commands, dogfooding revealed that LLM agents (and humans) had no way to discover follow-up commands from output alone. An agent seeing "rust: 7 files" in tags summary had to already know `hyalo tag find --name rust` exists. This is the CLI equivalent of the HATEOAS problem in REST APIs. See [[discoverable-drill-down-commands]] for the original backlog item.

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

## DEC-032: YAML Parse Errors Are Hard Errors (2026-03-23)

**Context:** The codebase had two scan paths for reading markdown files: `read_frontmatter_from_reader` (used by `properties`, `tags`, mutation commands) and `scan_reader_multi` (used by `find`, `summary`, task extraction). The former propagated YAML parse errors via `?`; the latter silently swallowed them with `unwrap_or_default()`, returning an empty property map for malformed frontmatter. This inconsistency meant `hyalo find` would silently skip broken files while `hyalo properties` would warn about them.

**Decision:** Both paths now treat malformed YAML frontmatter as a hard error, propagating via `anyhow::Context("failed to parse YAML frontmatter")`. Commands that want graceful degradation (like `properties summary`) catch the error at the command level using `is_parse_error()` and emit a warning — but the scanner itself always surfaces the error.

**Why:** Silent data loss is worse than a noisy error. A user with a broken frontmatter file should learn about it immediately, not wonder why `find --property status=planned` returns fewer results than expected. Commands that aggregate across many files can opt into graceful skip at their own level.

**Consequences:**
- `scan_reader_multi` now returns `Err` on malformed YAML (was `Ok` with empty props)
- `scan_reader` / `scan_file` (closure-based API) now delegates to `scan_reader_multi` via a `ClosureVisitor` wrapper, unifying the two code paths
- The old 80-line `scan_reader` implementation and its dependency on `frontmatter::skip_frontmatter` are removed
- `find` and `summary` commands now exit with an error on malformed YAML (previously silent)
- `properties` and `tags` commands continue to warn-and-skip via their existing `is_parse_error()` handling
