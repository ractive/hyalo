/**
 * A single drill-down hint: a concrete command plus a short human-readable description.
 */
type Hint = {
    /**
     * Human-readable purpose of the suggested command.
     */
    description: string;
    /**
     * Concrete command to run, or empty for advice-only hints.
     */
    cmd: string;
    /**
     * `true` when running `cmd` would modify the vault or `.hyalo.toml`.
     *
     * Derived from the command text by
     * [`crate::mutation::command_line_writes`] rather than set by each hint
     * builder, so a new hint cannot be added *without* being classified. The
     * `views set …` suggestion sat unmarked among read-only drill-downs until
     * iter-201; renderers now separate the two (see
     * [`crate::output::format_envelope`]).
     */
    writes: boolean;
};

/**
 * Successful output contract. Optional metadata is omitted, including all
 * counters when no file list was supplied; hints are always an array.
 */
type Envelope<T> = {
    /**
     * Optional vault directory hoisted from the command result.
     */
    dir?: string;
    /**
     * Missing paths from an explicitly supplied file list, including zero.
     */
    files_missing?: number;
    /**
     * Non-Markdown paths skipped from the supplied file list.
     */
    files_skipped_non_md?: number;
    /**
     * Paths outside the vault skipped from the supplied file list.
     */
    files_skipped_outside_vault?: number;
    /**
     * Read-only suggestions and explicitly marked mutation suggestions.
     */
    hints: Array<Hint>;
    /**
     * Named command output; arrays contain named result items.
     */
    results: T;
    /**
     * Total matching items before pagination, omitted for non-list commands.
     */
    total?: number;
};

type IndexDisposition = "not_used" | "updated" | "invalidated" | "update_failed";

type EffectFailure = "source_conflict" | "io" | "finalization";

type EffectState = "unchanged" | "committed" | "not_attempted" | "failed_before_commit" | "committed_with_finalization_error" | "restored" | "restore_failed" | "kept";

type PathEffect = {
    file: string;
    state: EffectState;
    error?: string;
    category?: EffectFailure;
};

type ApplyReport = {
    paths: Array<PathEffect>;
    index: IndexDisposition;
    index_error?: string;
};

/**
 * Internal adapter response; ordinary success envelopes do not gain effects.
 */
type MutationReportEnvelope<T> = {
    effects: ApplyReport;
    /**
     * Optional vault directory hoisted from the command result.
     */
    dir?: string;
    /**
     * Missing paths from an explicitly supplied file list, including zero.
     */
    files_missing?: number;
    /**
     * Non-Markdown paths skipped from the supplied file list.
     */
    files_skipped_non_md?: number;
    /**
     * Paths outside the vault skipped from the supplied file list.
     */
    files_skipped_outside_vault?: number;
    /**
     * Read-only suggestions and explicitly marked mutation suggestions.
     */
    hints: Array<Hint>;
    /**
     * Named command output; arrays contain named result items.
     */
    results: T;
    /**
     * Total matching items before pagination, omitted for non-list commands.
     */
    total?: number;
};

/**
 * Structured failure contract; the singular `hint` key is intentional.
 */
type ErrorEnvelope = {
    /**
     * Effects remain present if rendering fails after publication.
     */
    effects?: ApplyReport;
    /**
     * Stable failure classification when available.
     */
    category?: string;
    /**
     * Underlying diagnostic, omitted when there is no additional cause.
     */
    cause?: string;
    /**
     * User-facing description of the failure.
     */
    error: string;
    /**
     * Suggested recovery action, omitted when unavailable.
     */
    hint?: string;
    /**
     * Relevant path, omitted for failures unrelated to a specific path.
     */
    path?: string;
};

/**
 * A single backlink: another file that links to this one.
 * Used by `find` (backlinks field).
 */
type BacklinkInfo = {
    /**
     * Vault-relative file containing the authored link.
     */
    source: string;
    /**
     * One-based source line number.
     */
    line: number;
    /**
     * Authored link display label, when present.
     */
    label?: string;
};

/**
 * Serialized ConfigLinksResult command contract.
 */
type ConfigLinksResult = {
    /**
     * Whether all frontmatter values contribute graph edges.
     */
    frontmatter: boolean;
    /**
     * Explicit property allow-list, or null.
     */
    frontmatter_properties: Array<string> | null;
    /**
     * Whether authored aliases resolve links.
     */
    aliases: boolean;
    /**
     * Effective case resolution mode.
     */
    case_insensitive: string;
};

/**
 * Effective pi integration settings.
 */
type ConfigPiResult = {
    /**
     * Whether session summaries are enabled.
     */
    session_summary: boolean;
};

/**
 * Effective `[links.auto]` settings, as `hyalo config` reports them.
 *
 * These are the persisted `hyalo links auto` preferences; CLI flags extend the
 * two lists per-run and `--first-only` can turn `first_only` on for a run, so
 * what is reported here is the *baseline* every `links auto` invocation starts
 * from.
 */
type LinksAutoReport = {
    /**
     * `[links.auto] exclude_titles`.
     */
    exclude_titles: Array<string>;
    /**
     * `[links.auto] exclude_target_globs`.
     */
    exclude_target_globs: Array<string>;
    /**
     * `[links.auto] first_only`.
     */
    first_only: boolean;
    /**
     * `[links.auto] warn_common_titles` — `true` (the default) means `links
     * auto` may print the advisory noisy-candidate-title note on stderr.
     */
    warn_common_titles: boolean;
};

/**
 * Effective `[scan]` settings, as `hyalo config` reports them (iter-265).
 */
type ScanReport = {
    /**
     * `[scan] include` — hidden dot-subtrees the walker descends into.
     */
    include: Array<string>;
    /**
     * `[scan] exclude` — vault-relative globs no command sees.
     */
    exclude: Array<string>;
    /**
     * `[scan] verbose_skips` — stream per-file skip diagnostics instead of
     * collapsing them into one end-of-run summary line.
     */
    verbose_skips: boolean;
};

/**
 * Serialized ConfigResult command contract.
 */
type ConfigResult = {
    /**
     * Discovered config path, or null when absent.
     */
    config_path: string | null;
    /**
     * Whether config or schema parsing failed.
     */
    malformed: boolean;
    /**
     * Primary parsing diagnostic, or null.
     */
    parse_error: string | null;
    /**
     * Schema parsing diagnostic, or null.
     */
    schema_error: string | null;
    /**
     * Snapshot format written and accepted by this binary.
     */
    snapshot_format_version: number;
    /**
     * Effective dir salvaged setting.
     */
    dir_salvaged: boolean;
    /**
     * Effective dir out of bounds setting.
     */
    dir_out_of_bounds: boolean;
    /**
     * Effective dir out of bounds reason setting.
     */
    dir_out_of_bounds_reason: string | null;
    /**
     * Original config text with --raw; null otherwise.
     */
    raw_contents: string | null;
    /**
     * Effective cwd setting.
     */
    cwd: string;
    /**
     * Resolved vault directory, intentionally retained alongside envelope.dir.
     */
    dir: string;
    /**
     * Effective dir overridden setting.
     */
    dir_overridden: boolean;
    /**
     * Effective format setting.
     */
    format: string;
    /**
     * Effective format source setting.
     */
    format_source: string;
    /**
     * Configured format, or null when unset.
     */
    format_configured: string | null;
    /**
     * Effective hints setting.
     */
    hints: boolean;
    /**
     * Compatibility alias of the effective hints setting.
     */
    hints_enabled: boolean;
    /**
     * Effective site prefix setting.
     */
    site_prefix: string | null;
    /**
     * Effective site prefix source setting.
     */
    site_prefix_source: string;
    /**
     * Effective exempt setting.
     */
    exempt: Array<string>;
    /**
     * Effective links setting.
     */
    links: ConfigLinksResult;
    /**
     * Effective scan setting.
     */
    scan: ScanReport;
    /**
     * Effective links auto setting.
     */
    links_auto: LinksAutoReport;
    /**
     * Effective links fuzzy min confidence setting.
     */
    links_fuzzy_min_confidence: number;
    /**
     * Effective pi setting.
     */
    pi: ConfigPiResult;
};

/**
 * A content search match within a file body.
 */
type ContentMatch = {
    /**
     * One-based source line number.
     */
    line: number;
    /**
     * Containing section heading, including its ATX prefix.
     */
    section: string;
    /**
     * Matched source line or ranked snippet text.
     */
    text: string;
};

/**
 * Count of files in a directory.
 */
type DirectoryCount = {
    /**
     * Vault-relative directory path.
     */
    directory: string;
    /**
     * Number of matching occurrences.
     */
    count: number;
    /**
     * Files under this directory skipped for unparsable frontmatter.
     * Omitted from JSON when zero, so an all-clean vault's per-directory rows
     * stay as compact as they were.
     */
    skipped?: number;
};

/**
 * File counts by directory.
 */
type FileCounts = {
    /**
     * Total number of considered items.
     */
    total: number;
    /**
     * Files the scan found but could not use because their frontmatter would
     * not parse (iter-265). `summary` reported only `total` before, so a vault
     * of 103 notes with 28 unparsable Templater templates said `Files: 75` and
     * never accounted for the missing 28. Run `hyalo lint --rule HYALO005` to
     * see them individually.
     */
    skipped: number;
    /**
     * Files dropped before the scan by `[scan] exclude` in `.hyalo.toml`.
     * Zero unless the vault configures exclusions.
     */
    excluded: number;
    /**
     * File counts grouped by directory.
     */
    directories: Array<DirectoryCount>;
};

/**
 * A single task with section context, used by the `find` command.
 * Extends `TaskInfo` with section heading information.
 */
type FindTaskInfo = {
    /**
     * One-based source line number.
     */
    line: number;
    /**
     * Containing section heading, including its ATX prefix.
     */
    section: string;
    /**
     * Task checkbox marker or grouped status value.
     */
    status: string;
    /**
     * Authored task text without its checkbox marker.
     */
    text: string;
    /**
     * Whether the task is checked.
     */
    done: boolean;
};

/**
 * What a link in the `--fields links` inventory *is* — the reported `kind`
 * (iter-261, dogfood UX-6).
 *
 * Distinct from [`crate::links::LinkKind`], which is the two-valued *syntax*
 * the resolver branches on. This is the user-facing bucket, and it mixes
 * syntax (`embed`, `markdown`) with verdict (`external`, `attachment`)
 * because that is what a reader triaging a link report needs: without it,
 * telling `![[img.png]]` from `[[note]]` from `<obsidian://…>` meant going
 * back to the file.
 *
 * Precedence when several could apply — `external` beats `attachment` beats
 * `embed` beats the syntax kinds — so exactly one label is reported per link.
 */
type LinkKindLabel = "wikilink" | "embed" | "markdown" | "external" | "attachment" | "frontmatter";

/**
 * A single link with its resolution status.
 * Used by `find` (links field).
 */
type LinkInfo = {
    /**
     * Resolved or authored link target, according to the command.
     */
    target: string;
    /**
     * Path associated with this result.
     */
    path: string | null;
    /**
     * Authored link display label, when present.
     */
    label: string | null;
    /**
     * What this link is: `wikilink` | `embed` | `markdown` | `external` |
     * `attachment` (iter-261, dogfood UX-6). Always serialized — a link
     * always has a kind — and defaulted to `wikilink` when reading JSON
     * written by an older hyalo.
     */
    kind: LinkKindLabel;
    /**
     * The frontmatter key this link was written under, for a link with
     * `kind: "frontmatter"` — the dotted key path for a nested map
     * (`meta.source`), the plain key otherwise (iter-262). Absent for body
     * links, so the shape of an existing report is unchanged.
     */
    property?: string;
    /**
     * 1-based source line the link was written on (iter-215, dogfood UX-6).
     *
     * `find --broken-links` used to list every link of a matching file with no
     * location, so finding the reported broken link meant grepping the file.
     * The line is the same one `hyalo lint` (HYALO006) and `backlinks` report
     * for the same link, and comes straight from the index
     * (`IndexEntry::links` / `IndexEntry::self_anchors` already store it), so
     * no extra file read is involved.
     *
     * Named `line` to match every other line-bearing shape in `.results`
     * (`BacklinkInfo`, `OutlineSection`, `ContentMatch`, `TaskInfo`) — always
     * a 1-based source line, never an index or an offset. Always serialized:
     * unlike `fragment` / `broken_anchor` / `out_of_vault` this is not a
     * verdict that may be absent, it is a location every link has.
     * `#[serde(default)]` only covers deserializing JSON written by an older
     * hyalo, where it reads back as `0`.
     */
    line: number;
    /**
     * The `#fragment` (heading anchor) the link carried, without the leading
     * `#`. `None` for links with no fragment. Skipped from JSON when absent so
     * non-anchored links keep today's shape (L-21, iter-190).
     */
    fragment?: string;
    /**
     * `true` when the link's target file resolved (`path` is `Some`) but the
     * `#fragment` does not name any heading in that file — a *broken anchor*.
     * Distinct from a broken target (`path: None`); the two are never both set
     * on one link. Skipped from JSON when `false` so non-anchored / valid
     * links keep today's shape.
     */
    broken_anchor?: boolean;
    /**
     * The full heading text to write instead, when this link's dead fragment
     * is the prefix of exactly one heading in the target file (iter-261 /
     * DEC-268): `[[decision-log#DEC-068]]` → `DEC-068: Snapshot index format`.
     *
     * Only ever set alongside `broken_anchor`, and only when the prefix is
     * unambiguous — two matching headings yield no suggestion. It is a
     * suggestion, never an automatic rewrite: a silent prefix match would hide
     * the typos this rule exists to surface. Skipped from JSON when absent.
     */
    suggested_fragment?: string;
    /**
     * `true` when the link's target normalizes to a path *above* the vault
     * root, so it can never resolve to a scanned file. Implies `path: None`,
     * but is deliberately distinguished from a broken target: the file is out
     * of scope, not missing (iter-193). Skipped from JSON when `false`.
     */
    out_of_vault?: boolean;
    /**
     * How the target resolved when a plain path or filename lookup was not
     * what answered — currently only `"alias"`, for a target matched against
     * a note's frontmatter `aliases:` (iter-272 Part B, DEC-296).
     *
     * Absent for every link that resolves by path or filename, so a report of
     * an alias-free vault keeps today's shape byte for byte. `kind` stays
     * `wikilink`: an alias changes *what the target names*, not the syntax it
     * was written in.
     */
    via?: string;
};

/**
 * Task checkbox counts within a section.
 */
type TaskCount = {
    /**
     * Total number of considered items.
     */
    total: number;
    /**
     * Number of checked tasks.
     */
    done: number;
};

/**
 * A single section in the document outline.
 * Used by `find` (sections field).
 */
type OutlineSection = {
    /**
     * ATX heading level (one through six).
     */
    level: number;
    /**
     * Heading text, or null for the preamble.
     */
    heading: string | null;
    /**
     * One-based source line number.
     */
    line: number;
    /**
     * Link targets occurring within this section.
     */
    links: Array<string>;
    /**
     * Task checkbox counts.
     */
    tasks?: TaskCount;
    /**
     * Languages of fenced code blocks in the section.
     */
    code_blocks: Array<string>;
};

/**
 * A single frontmatter property with its inferred type and value.
 * Used by `properties` (aggregate summary).
 */
type PropertyInfo = {
    /**
     * Name of this entry.
     */
    name: string;
    /**
     * Inferred property type name.
     */
    type: string;
    /**
     * User-authored property value; the value shape is genuinely dynamic.
     */
    value: unknown;
};

/**
 * The unified file object returned by the `find` command.
 * Always returned in an array. Optional fields are controlled by `--fields`.
 */
type FileObject = {
    /**
     * The only unconditional key (iteration 254, DEC-254): it names the
     * result, so no projection may drop it.
     */
    file: string;
    /**
     * Last-modified timestamp. In the *default* field set — an agent picks
     * its next call by recency — but an explicit `--fields` that does not
     * name `modified` drops it.
     */
    modified?: string;
    /**
     * File size in bytes (iteration 252), so an agent can budget a `read`
     * before issuing it. Default field set; droppable via `--fields`.
     */
    size?: number;
    /**
     * Line count (see [`crate::scanner::ScanStats`]) — the unit
     * `read --lines A:B` takes. Default field set; droppable via `--fields`.
     */
    lines?: number;
    /**
     * Title extracted from frontmatter `title` property or first H1 heading.
     * - `None`: field not requested (omitted from JSON output)
     * - `Some(Value::String(...))`: title found
     * - `Some(Value::Null)`: title requested but not found
     */
    title?: unknown;
    /**
     * Where [`Self::title`] came from: `"property"` (a scalar frontmatter
     * `title`), `"h1"` (the first H1 heading) or `"filename"` (the filename
     * stem — Obsidian's own fallback, added in iteration 267 / DEC-283).
     *
     * Present exactly when `title` is; `None` when the title field was not
     * requested, or when even the filename stem was empty.
     */
    title_source?: string;
    /**
     * User-authored frontmatter properties with genuinely dynamic values.
     */
    properties?: Record<string, unknown>;
    /**
     * Frontmatter properties paired with their inferred types.
     */
    properties_typed?: Array<PropertyInfo>;
    /**
     * Tags authored on this file.
     */
    tags?: Array<string>;
    /**
     * Document outline with section-level metadata.
     */
    sections?: Array<OutlineSection>;
    /**
     * Tasks in the selected file or sections.
     */
    tasks?: Array<FindTaskInfo>;
    /**
     * Outbound authored links.
     */
    links?: Array<LinkInfo>;
    /**
     * Inbound links from other vault files.
     */
    backlinks?: Array<BacklinkInfo>;
    /**
     * Body matches or ranked snippets; present and empty for title-only ranked hits.
     */
    matches?: Array<ContentMatch>;
    /**
     * BM25 relevance score for positional ranked searches.
     */
    score?: number;
};

/**
 * Arguments accepted by `hyalo find`.
 *
 * The generated TypeScript declaration is the source for the public API's
 * ergonomic `FindOptions` alias. Clap remains the authority for conflicts
 * and validation.
 */
type FindArgs = {
    /**
     * BM25 ranked full-text body search (stemmed; sorted by relevance)
     *
     * BM25 ranked body text search with stemming (e.g. "running" matches "run", "ran");
     * results sorted by relevance.
     */
    pattern?: string;
    /**
     * Target file(s), positional form of --file
     */
    file_positional: Array<string>;
    /**
     * Start from a saved view; CLI filters merge on top
     *
     * Use a saved view (named filter set from .hyalo.toml). Additional CLI filters
     * are merged on top: list filters (--property, --tag, --section, --glob) extend
     * the view; scalar filters (--sort, --limit, --regexp, --title, --task) override it.
     */
    view?: string;
    /**
     * Regex body search, case-insensitive (excludes PATTERN)
     *
     * Regex body text search (case-insensitive by default; use (?-i) to override).
     * Mutually exclusive with PATTERN.
     */
    regexp?: string;
    /**
     * K=V|K!=V|K>V|K>=V|K<V|K<=V|K|!K|K~=/re/i|K=null; AND; K may be a dot-path
     *
     * Property filter: K=V (eq), K!=V (neq), K>=V, K<=V, K>V, K<V, K (exists), !K (absent),
     * K~=pat or K~=/pat/i (regex). Repeatable (AND). K may be a dot-path into nested maps and
     * sequences (contact.email, contacts.0.email, contacts.email = any element).
     *
     * Value syntax: K=null matches a property present with a YAML null (`~`, `null`, or an
     * empty value) and K!=null a present non-null one; K=[] matches an empty list, K!=[] a
     * non-empty one. A list *containing* a null does not match K=null.
     * Ordering ops (>, >=, <, <=) compare numerically when both sides are numbers, by date
     * when both are ISO dates, and as text only when both are plain strings — a value of a
     * different kind never matches, so `last>=2023-09-01` skips `last: "[[2022-04]]"`.
     * The regex operator is ~= (not =~, which is rejected), and its pattern must not be empty.
     * K=V tests the RAW frontmatter value, while type binding normalises: a file whose
     * `type: ["[[Iteration]]"]` binds to the `Iteration` schema is not matched by
     * `--property type=Iteration`. Use `--property 'type~=Iteration'` to span every spelling.
     */
    properties: Array<string>;
    /**
     * Tag, exact or prefix ('project' matches 'project/backend'); repeatable (AND)
     *
     * Tag filter: exact or prefix match (e.g. 'project' matches 'project/backend' but not
     * 'projects'). Repeatable (AND).
     */
    tag: Array<string>;
    /**
     * Task presence: 'todo', 'done', 'any', or a single status character
     *
     * A FILE filter, not a task projection: it selects files that contain at least one
     * matching task, and `--fields tasks` then returns every task in those files, not only
     * the matching ones.
     */
    task?: string;
    /**
     * Heading substring, '##' pins the level, or /regex/; repeatable (OR)
     *
     * Section heading filter: case-insensitive substring match (e.g. 'Tasks' matches 'Tasks [4/4]');
     * prefix '##' to pin heading level; use '/regex/' for regex (e.g. '/DEC-03[12]/'). Repeatable (OR).
     * A file with more than one matching heading unions all of them (unlike `task --section`, which refuses)
     */
    sections: Array<string>;
    file: Array<string>;
    /**
     * Glob patterns relative to `--dir`, repeatable; `!` negates a pattern.
     *
     * Glob pattern(s) to select files, relative to --dir (repeatable); prefix '!' to negate
     * recursive matches.
     */
    glob: Array<string>;
    /**
     * Read paths from PATH, one per line ('-' = stdin)
     *
     * Read file paths from PATH (one per line); use '-' to read from stdin.
     * Mutually exclusive with --file and --glob.
     * Non-.md paths and paths outside the vault are silently skipped (counters appear in JSON envelope).
     * Repo-relative paths with the configured vault dir prefix (e.g. files/en-us/x.md with --dir files/en-us)
     * are resolved by trying vault-relative first, then stripping the full dir prefix and retrying.
     * Input is deduplicated; results follow first-seen order.
     * CHANGED-FILES RECIPE: `git diff --name-only origin/main | hyalo <cmd> --files-from -`
     * restricts a run to what a branch touched (any VCS or `find`/`fd`/`rg -l` works the same way;
     * hyalo shells out to nothing and has no VCS-specific flag).
     * MISSING PATHS: a path named with --file or positionally must exist (exit 1); a --files-from
     * list keeps batch semantics and merely counts a missing entry under files_missing, exit 0.
     * An EMPTY list examines nothing and still exits 0, so it is reported as a warning that -q
     * does not silence — in a gate, "no input" and "no findings" must not look alike.
     */
    files_from?: string;
    /**
     * all|file|modified|size|lines|title|properties|properties-typed|tags|sections|tasks|links|backlinks — exact projection
     *
     * Without --fields: file, modified, size, lines, title, properties, tags. With --fields:
     * exactly the named fields plus file (filters add what they need).
     *
     * `file` is the only unconditional key — it names the result — so `--fields title` returns
     * {file, title} and `--fields size,lines` returns {file, size, lines}. `modified`, `size`
     * and `lines` are ordinary members of the default set: cheap enough to always pay for, and
     * the inputs an agent uses to choose its next call (`read --lines`, recency), but dropped
     * when an explicit --fields does not name them. `--fields file` is accepted and means
     * {file}; `--fields all` selects everything. A saved view's pinned `fields` behaves exactly
     * like an explicit --fields; a CLI --fields on top replaces the pin rather than adding to it.
     *
     * A filter that implies a field still returns it, on top of whatever set is in force:
     * --section adds sections, --task adds tasks, --broken-links adds links,
     * --orphan/--dead-end add links and backlinks, and --sort links_count/backlinks_count add
     * the field they rank on.
     *
     * 'properties' is a {key: value} map WITHOUT the promoted 'title' property (which has its
     * own field whenever 'title' is included, and stays in the map when the frontmatter value
     * is a list or a map and so cannot be promoted); 'properties-typed' is a
     * [{name, type, value}] array; 'backlinks' requires scanning all files; 'title' is the
     * frontmatter title property — any scalar, stringified as written — or the first H1
     * heading (null if neither found). 'outline' is an alias for 'sections'. Note: in JSON
     * output, `properties-typed` is serialized as `properties_typed` (underscore).
     */
    fields: Array<string>;
    /**
     * file (default)|modified|backlinks_count|links_count|title|date|score|property:K
     *
     * Sort order: 'file' / 'path' (default), 'modified', 'backlinks_count', 'links_count',
     * 'title', 'date', 'score', or 'property:<KEY>' for any frontmatter property.
     *
     * DIRECTION: every key sorts ascending and --reverse inverts it, so
     * `--sort backlinks_count --reverse` is "most linked first" exactly as
     * `--sort modified --reverse` is "newest first". 'score' is the one exception: it ranks
     * best-match-first (descending relevance), and --reverse puts the weakest match first.
     * Files whose sort property is missing or null always sort last, in both directions.
     *
     * For 'property:<KEY>',
     * values of different JSON types (e.g. some files have a string, others a number) compare by
     * raw JSON text -- grouped by type but not sensibly ordered within a numeric group -- and a
     * stderr warning names the property when this happens; use a consistent type in frontmatter
     * for a meaningful sort.
     */
    sort?: string;
    /**
     * Reverse the sort order [alias: --desc]
     *
     * Reverse the sort order (ascending becomes descending and vice versa). Alias: --desc.
     */
    reverse: boolean;
    /**
     * Max results, 0 = unlimited (default cap: 50)
     *
     * Maximum number of results to return (0 = unlimited).
     * Default cap is bypassed when --jq or --count is used.
     */
    limit?: number;
    /**
     * Only files with an unresolved link or dead heading anchor
     *
     * Only return files with at least one unresolved link or dead heading anchor
     * (auto-includes links field).
     * Targets that resolve above the vault root are out of scope, not broken: they are
     * flagged `out_of_vault` on the link and do not qualify a file here.
     * A `#fragment` matches either the raw heading text or the rendered GitHub slug
     * (`#sub-section` for `### Sub Section`); same-file fragments (`[b](#nope)`) are
     * checked against the file's own headings and reported with an empty target.
     * A heading carrying a template expression (`## {% data variables.x %}`, `{{ y }}`)
     * renders to an anchor hyalo cannot compute, so anchors into such a file are never
     * reported broken.
     * Every listed link carries its 1-based source `line`, the same one `lint` (HYALO006)
     * reports, and links are listed in document order.
     * An external URI (`obsidian://`, `mailto:`, `https:`) and a link that resolves to a
     * non-`.md` vault file (an image, a `.base`) are never broken — they are reported with
     * `kind` `external` / `attachment` and never qualify a file here.
     */
    broken_links: boolean;
    /**
     * Exit 1 if any results, 0 if empty — a CI gate
     *
     * A CI gate for any find query, most commonly `find --broken-links --strict` to fail a build
     * on a dead heading anchor. Before this, `find --broken-links` always exited 0 even when it
     * reported findings, so a vault whose only defect was a dead anchor passed CI silently.
     */
    strict: boolean;
    /**
     * Only orphan files: no inbound and no outbound links (auto-includes links and backlinks)
     *
     * Deciding orphanhood needs both directions of the graph, so both fields come back
     * whether or not --fields names them.
     */
    orphan: boolean;
    /**
     * Only dead-end files: inbound links but no outbound links (auto-includes links and backlinks)
     *
     * Deciding dead-endedness needs both directions of the graph, so both fields come back
     * whether or not --fields names them.
     */
    dead_end: boolean;
    /**
     * Title substring (case-insensitive) or /regex/[i]
     *
     * Filter by title: case-insensitive substring match against the displayed title
     * (frontmatter 'title' property or first H1 heading). Use /regex/ for regex
     * (e.g. '/^The/' or '/^The/i').
     */
    title?: string;
    /**
     * BM25 Snowball stemmer language (default: english) [alias: --stemmer]
     *
     * Stemmer language for BM25 body search (also --stemmer). Selects Snowball stemmer for BM25
     * tokenization — NOT markdown code-block language.
     * Default: english. Accepts full names (english, german, …) or ISO 639-1 codes (en, de, …).
     * Supported: arabic (ar), danish (da), dutch (nl), english (en), finnish (fi), french (fr),
     * german (de), greek (el), hungarian (hu), italian (it), norwegian (no, nb, nn),
     * portuguese (pt), romanian (ro), russian (ru), spanish (es), swedish (sv), tamil (ta),
     * turkish (tr).
     */
    language?: string;
    /**
     * Print matching paths only, one per line — no envelope, no hints
     *
     * Print only the file path of each matching entry, one per line — no JSON,
     * no envelope, no count, no hints. grep `-l` precedent: the agent/
     * pipeline projection of a find result set, usable in `sort`, `xargs`,
     * and `while read` loops. Zero results → empty output, exit 0.
     *
     * Conflicts with `--jq`, `--count`, and an explicit `--format json`
     * (mutually exclusive projections — pick one). `--strict` still flips
     * the exit code (1 when results exist), so `find --property status=planned
     * --filenames-only --strict` is a CI gate that lists the offenders and
     * fails. Combines with every other filter (`--property`, `--tag`,
     * `--glob`, `--broken-links`, …) exactly as `find`
     * normally does.
     */
    filenames_only: boolean;
    /**
     * Like --filenames-only but NUL-separated, for `xargs -0`
     *
     * NUL-delimited sibling of `--filenames-only` (iter-238): each matching
     * file path is printed terminated by a NUL byte instead of a newline,
     * exactly like GNU `find -print0`. Safe for filenames that contain
     * newlines (which are legal in POSIX filenames, though not on Windows),
     * and composes
     * with `xargs -0` / `while IFS= read -r -d ''`. Same semantics as
     * `--filenames-only` otherwise: no JSON, no envelope, no count, no hints;
     * zero results → empty output, exit 0; `--strict` still flips the exit
     * code when results exist.
     *
     * Mutually exclusive with `--filenames-only`, `--jq`, `--count`, and an
     * explicit `--format json` (pick one projection).
     */
    filenames0: boolean;
    /**
     * Use the `.hyalo-index` snapshot in the vault dir
     *
     * Read-only commands (find, summary, tags, properties, backlinks) skip
     * the disk scan entirely when the index is present. On `tags` and
     * `properties` the flag is accepted on the bare command as well as on
     * the `summary`/`rename` subcommand (iter-266).
     *
     * Mutation commands (set, remove, append, task, mv, tags rename,
     * properties rename, links fix) still read/write individual files on disk
     * but also patch the index in-place after each mutation — keeping
     * the index current for subsequent queries. A file the index has never
     * seen (created by an editor or Obsidian since the last create-index)
     * is *upserted*: its full entry and outgoing links are inserted, not
     * dropped, so indexed reads match a disk scan after the mutation.
     * `set`/`append`/`remove` go further: every file they read whose
     * `(mtime, size)` no longer matches the snapshot is rescanned, even when
     * the mutation itself changes nothing (`0 modified`) — so a body edited
     * by hand between `create-index` and the mutation cannot leave the entry
     * describing bytes that are gone. `--dry-run` writes nothing, stale
     * entry or not.
     * `links fix`/`links auto` additionally mtime-check every indexed entry
     * before their discovery pass, rescan files that changed on disk since
     * create-index, and upsert files the index does not know yet (with a
     * warning), so an externally edited vault is not silently trusted.
     *
     * If the index file is incompatible (e.g. after a hyalo upgrade) hyalo
     * falls back to a full disk scan automatically.
     *
     * STALENESS PROBE: on load, hyalo compares directory mtimes in the
     * vault (the root and every directory up to 3 levels below it — cheap,
     * directory-only stats, no file reads) against the snapshot's creation
     * time and warns `index older than vault` when one postdates it. When
     * that probe finds nothing, a second pass compares each indexed file's
     * recorded mtime against disk and stops at the first drift, so an
     * in-place overwrite (which moves no directory mtime) is named in the
     * warning rather than silently served. Remaining blind spot: **up to
     * about two seconds** — mtimes are compared as whole seconds and a
     * one-second tolerance is applied on top, so an edit made within the
     * snapshot's own second or the one after it is invisible (BUG-30,
     * iter-276: the old wording said "the same whole second", which
     * understated it by half). The warning never stops the run: stale
     * results are still served.
     */
    index: boolean;
    /**
     * Use the snapshot index at PATH instead of `.hyalo-index`
     *
     * Implies `--index`. Relative paths are resolved against the current
     * working directory (not the vault dir); absolute paths are used as-is.
     *
     * Reading a snapshot from anywhere on disk is allowed. *Writing* one is
     * not: on `create-index` / `drop-index` this flag is an alias for the
     * output path, and a path outside the vault is refused unless
     * `--allow-outside-vault` is also passed.
     *
     * Read-only commands skip the disk scan entirely. Mutation commands
     * patch the index in-place after each write — see `--index` for details.
     *
     * If the index file is incompatible hyalo falls back to a disk scan.
     */
    index_file?: string;
};

type GlobalArgs = {
    /**
     * Vault root for file and glob paths (default: .)
     *
     * Root directory for resolving all file and --glob paths.
     * Default: "." (Override via .hyalo.toml)
     */
    dir?: string;
    /**
     * Output format (default: text on a terminal, json when piped)
     *
     * Output format: "json" or "text".
     * Default: "text" when stdout is a terminal, "json" when piped.
     * Override for a session via .hyalo.toml: format = "text"
     */
    format?: "json" | "text" | "github";
    /**
     * jq filter over the JSON envelope
     *
     * Apply a jq filter expression to the JSON output of any command.
     * Operates on the full JSON envelope: {"results": ..., "total": N, "hints": [...]}.
     * The filtered result is printed as plain text. Incompatible with --format text
     * (combining them is a user error and exits 1).
     * Example: --jq '.results[].file' or --jq '.results | map(.properties.status) | unique'.
     * HINTS: --jq computes none, so `.hints` is always [] under a filter (DEC-313) — it is
     * the machine path, and hint generation is a second pass over the results. Read hints
     * from plain --format json instead.
     * LIMITS: a filter is given 3 seconds of wall-clock time (a pathological filter —
     * infinite recursion with no output, or building a huge intermediate array before
     * ever yielding a value, e.g. '[range(3e8)]' — errors out instead of hanging or
     * exhausting memory), may emit at most 1,000,000 output values, and the total
     * emitted text is capped at 10 MiB. Any limit breach exits 1 with a clean error,
     * never a hang or an OOM.
     */
    jq?: string;
    /**
     * Print just the total as a bare integer (list commands)
     *
     * Print only the total count as a bare integer for list commands.
     * Shortcut for --jq '.total'. Incompatible with --jq.
     */
    count: boolean;
    /**
     * Force hints on (already the default)
     *
     * Force hints on (already the default).
     * Text mode: '-> hyalo ...  # description' lines — concrete, copy-pasteable commands with descriptions.
     * JSON mode: populates the "hints" array in the envelope (always present, empty when suppressed).
     * Suppressed when --jq is active.
     */
    hints: boolean;
    /**
     * Disable drill-down hints (on by default)
     *
     * Disable drill-down command hints (enabled by default).
     * Override via .hyalo.toml: hints = false
     * When both --hints and --no-hints are present, --hints takes precedence.
     */
    no_hints: boolean;
    /**
     * Prefix stripped from /root-absolute/links
     *
     * Site prefix for resolving root-absolute links like `/docs/page.md`.
     * When a markdown file contains a link like `/docs/guides/setup.md`, hyalo strips the
     * leading `/<prefix>/` to get the vault-relative path `guides/setup.md`. This is how
     * documentation sites (GitHub Pages, VuePress, Docusaurus) map URL paths to file paths.
     *
     * By default, hyalo auto-derives the prefix from --dir's last path component:
     *   --dir ../vscode-docs/docs  →  prefix = "docs"
     *   --dir /home/me/wiki        →  prefix = "wiki"
     *   --dir .                    →  prefix = name of the current directory
     *
     * Use --site-prefix to override when the directory name doesn't match the URL prefix,
     * or pass --site-prefix "" to resolve absolute links from the vault/bundle root:
     * only the leading `/` is stripped, so `/guides/setup.md` → `guides/setup.md`.
     *
     * Also settable via `site_prefix = "docs"` in .hyalo.toml.
     * Precedence: --site-prefix flag > .hyalo.toml > auto-derived from --dir.
     * Run `hyalo config` to see the effective value and where it came from.
     * `hyalo links fix` warns on stderr when the effective prefix stripped
     * 0 of N site-absolute links to a plausible vault path — a real MDN
     * checkout with the auto-derived one-segment prefix left every
     * `/en-US/docs/...` link unresolved, since `docs` names no real
     * top-level entry once `en-US` alone is stripped.
     */
    site_prefix?: string;
    /**
     * Suppress warnings on stderr
     *
     * Suppress all warnings printed to stderr.
     * Useful in scripts or CI pipelines where warning noise is undesirable.
     * Identical warnings are always deduplicated regardless of this flag;
     * use `--quiet` to suppress them entirely.
     *
     * One exception: a `.hyalo.toml` that could not be parsed is always
     * reported. That warning means the run is using a different vault and a
     * different rule set than the config asked for, which is not noise.
     */
    quiet: boolean;
    /**
     * Snapshot index path (alias for the subcommand flag)
     *
     * Use the snapshot index at PATH (global alias for the per-subcommand `--index-file`).
     * Equivalent to passing `--index-file PATH` after the subcommand.
     * When both the global flag and the subcommand flag are provided, the
     * subcommand value takes precedence.
     *
     * Relative paths are resolved against the current working directory.
     * Reading is unrestricted; writing one outside the vault (`create-index`,
     * `drop-index`) additionally needs `--allow-outside-vault`.
     */
    index_file?: string;
};

/**
 * Vault-wide link health: total links and broken count.
 */
type LinkHealthSummary = {
    /**
     * Total number of considered items.
     */
    total: number;
    /**
     * Number of unresolved links.
     */
    broken: number;
    /**
     * Links pointing above the scanned vault root (`../..` escapes). Kept out
     * of `broken` because the target is out of scope rather than missing
     * (iter-193). Omitted from JSON when zero so vaults with no such links
     * keep the previous output shape.
     */
    out_of_vault?: number;
    /**
     * Links whose target resolves but whose `#fragment` names no heading
     * there — a dead anchor, distinct from `broken` (a missing target).
     *
     * NEW-15 (dogfood pre3): `summary` used to say "0 broken" on a vault
     * `find --broken-links` reported 3 files for, because this count never
     * looked at anchors. Kept as its own field rather than folded into
     * `broken` since the two are different failure modes with different
     * fixes. Omitted from JSON when zero, same convention as `out_of_vault`.
     *
     * PR #251 review M3: only computed when `broken == 0` — checking it
     * unconditionally would mean a second full link-resolution pass
     * (re-hitting the filesystem for every fragment-bearing link) right
     * after the first one `summary` already runs, doubling summary's own
     * cost on a fragment-heavy corpus. A vault with both broken targets and
     * broken anchors reports `Some(0)` here until the targets are fixed;
     * `find --broken-links` is the always-accurate source of truth.
     *
     * PR #251 review L6: `None` when the vault directory itself could not
     * be canonicalized. Deliberately serialized as JSON `null` — NOT
     * omitted like a computed/gated `Some(0)` — so a script reading this
     * field cannot mistake "could not check" for "checked and it's clean";
     * omitting both would make them indistinguishable, which is exactly the
     * false-clean-bill this finding exists to prevent.
     */
    broken_anchors?: number | null;
};

/**
 * Lint violation counts for the vault summary.
 */
type LintSummary = {
    /**
     * Number of error-severity violations.
     */
    errors: number;
    /**
     * Number of warning-severity violations.
     */
    warnings: number;
    /**
     * Number of files with at least one schema violation.
     *
     * iter-216 D-5: named `files_with_violations` to match the key `hyalo
     * lint` emits for the same quantity. `summary` used to call it
     * `files_with_issues`, so a script comparing the digest against a full
     * lint run had to know both spellings.
     */
    files_with_violations: number;
};

/**
 * One type variant in a mixed-type property summary.
 */
type MixedTypeEntry = {
    /**
     * Inferred property type name.
     */
    type: string;
    /**
     * Number of matching occurrences.
     */
    count: number;
};

/**
 * Aggregate property summary entry.
 * Used by `properties` command and `summary`.
 */
type PropertySummaryEntry = {
    /**
     * Name of this entry.
     */
    name: string;
    /**
     * Inferred property type name.
     */
    type: string;
    /**
     * Number of matching occurrences.
     */
    count: number;
    /**
     * Present only when the property has inconsistent types across files.
     * Each entry is `(type_name, file_count)` for that type variant.
     * When `None`, all occurrences share the same type.
     */
    mixed_types?: Array<MixedTypeEntry>;
};

/**
 * Arguments accepted by `hyalo read`.
 */
type ReadArgs = {
    /**
     * Heading substring, '##' pins the level, or /regex/ (nested subsections included)
     *
     * Extract section(s) by case-insensitive substring match (e.g. 'Tasks' matches
     * 'Tasks [4/4]'); prefix '##' to pin the heading level; use '/regex/' for a regex.
     * Nested subsections are included.
     */
    section?: string;
    /**
     * Slice by line range: 5:10, 5:, :10, or 5 (1-based, inclusive, relative to the body)
     *
     * The frontmatter block is not counted, so line 1 is the first line after it — even
     * with --frontmatter. Note that `task --line` counts differently: those numbers are
     * file-absolute, with the frontmatter included.
     */
    lines?: string;
    /**
     * Include the YAML frontmatter in output
     *
     * Text output echoes the block's own bytes between its `---` fences —
     * indentation, quote style and comments exactly as on disk; no YAML is
     * re-serialized on a read path. JSON keeps the parsed map under
     * `frontmatter` and adds the raw text as `frontmatter_raw` (null for a
     * file with no frontmatter block).
     */
    frontmatter: boolean;
    /**
     * Target file (relative to --dir) — positional form (single file)
     */
    file_positional?: string;
    file: Array<string>;
    glob: Array<string>;
    files_from?: string;
    /**
     * Use the `.hyalo-index` snapshot in the vault dir
     *
     * Read-only commands (find, summary, tags, properties, backlinks) skip
     * the disk scan entirely when the index is present. On `tags` and
     * `properties` the flag is accepted on the bare command as well as on
     * the `summary`/`rename` subcommand (iter-266).
     *
     * Mutation commands (set, remove, append, task, mv, tags rename,
     * properties rename, links fix) still read/write individual files on disk
     * but also patch the index in-place after each mutation — keeping
     * the index current for subsequent queries. A file the index has never
     * seen (created by an editor or Obsidian since the last create-index)
     * is *upserted*: its full entry and outgoing links are inserted, not
     * dropped, so indexed reads match a disk scan after the mutation.
     * `set`/`append`/`remove` go further: every file they read whose
     * `(mtime, size)` no longer matches the snapshot is rescanned, even when
     * the mutation itself changes nothing (`0 modified`) — so a body edited
     * by hand between `create-index` and the mutation cannot leave the entry
     * describing bytes that are gone. `--dry-run` writes nothing, stale
     * entry or not.
     * `links fix`/`links auto` additionally mtime-check every indexed entry
     * before their discovery pass, rescan files that changed on disk since
     * create-index, and upsert files the index does not know yet (with a
     * warning), so an externally edited vault is not silently trusted.
     *
     * If the index file is incompatible (e.g. after a hyalo upgrade) hyalo
     * falls back to a full disk scan automatically.
     *
     * STALENESS PROBE: on load, hyalo compares directory mtimes in the
     * vault (the root and every directory up to 3 levels below it — cheap,
     * directory-only stats, no file reads) against the snapshot's creation
     * time and warns `index older than vault` when one postdates it. When
     * that probe finds nothing, a second pass compares each indexed file's
     * recorded mtime against disk and stops at the first drift, so an
     * in-place overwrite (which moves no directory mtime) is named in the
     * warning rather than silently served. Remaining blind spot: **up to
     * about two seconds** — mtimes are compared as whole seconds and a
     * one-second tolerance is applied on top, so an edit made within the
     * snapshot's own second or the one after it is invisible (BUG-30,
     * iter-276: the old wording said "the same whole second", which
     * understated it by half). The warning never stops the run: stale
     * results are still served.
     */
    index: boolean;
    /**
     * Use the snapshot index at PATH instead of `.hyalo-index`
     *
     * Implies `--index`. Relative paths are resolved against the current
     * working directory (not the vault dir); absolute paths are used as-is.
     *
     * Reading a snapshot from anywhere on disk is allowed. *Writing* one is
     * not: on `create-index` / `drop-index` this flag is an alias for the
     * output path, and a path outside the vault is refused unless
     * `--allow-outside-vault` is also passed.
     *
     * Read-only commands skip the disk scan entirely. Mutation commands
     * patch the index in-place after each write — see `--index` for details.
     *
     * If the index file is incompatible hyalo falls back to a disk scan.
     */
    index_file?: string;
};

/**
 * Serialized ReadResult command contract.
 */
type ReadResult = {
    /**
     * Vault-relative file path.
     */
    file: string;
    /**
     * Size of the whole file in bytes.
     */
    size: number;
    /**
     * Line count of the whole file, before section selection.
     */
    lines: number;
    /**
     * Parsed user-authored frontmatter, whose keys and values are genuinely dynamic.
     */
    frontmatter?: unknown;
    /**
     * Exact frontmatter source; null when requested but no source block exists.
     */
    frontmatter_raw?: string | null;
    /**
     * Selected body text, omitted for frontmatter-only reads.
     */
    content?: string;
};

/**
 * A recently modified file.
 */
type RecentFile = {
    /**
     * Path associated with this result.
     */
    path: string;
    /**
     * File modification time.
     */
    modified: string;
};

/**
 * Files grouped by status property value (count only).
 */
type StatusGroup = {
    /**
     * Authored status property value.
     */
    value: string;
    /**
     * Number of matching occurrences.
     */
    count: number;
};

/**
 * Arguments accepted by `hyalo summary`.
 */
type SummaryArgs = {
    glob: Array<string>;
    /**
     * Number of recent files to show
     *
     * NOTE: on this command -n means --recent, not --limit as on find and backlinks —
     * it caps only the "recently modified" list, never the summary's stats.
     */
    recent: number;
    /**
     * Limit directory listing depth (0 = root only; stats are always full)
     */
    depth?: number;
    /**
     * Use the `.hyalo-index` snapshot in the vault dir
     *
     * Read-only commands (find, summary, tags, properties, backlinks) skip
     * the disk scan entirely when the index is present. On `tags` and
     * `properties` the flag is accepted on the bare command as well as on
     * the `summary`/`rename` subcommand (iter-266).
     *
     * Mutation commands (set, remove, append, task, mv, tags rename,
     * properties rename, links fix) still read/write individual files on disk
     * but also patch the index in-place after each mutation — keeping
     * the index current for subsequent queries. A file the index has never
     * seen (created by an editor or Obsidian since the last create-index)
     * is *upserted*: its full entry and outgoing links are inserted, not
     * dropped, so indexed reads match a disk scan after the mutation.
     * `set`/`append`/`remove` go further: every file they read whose
     * `(mtime, size)` no longer matches the snapshot is rescanned, even when
     * the mutation itself changes nothing (`0 modified`) — so a body edited
     * by hand between `create-index` and the mutation cannot leave the entry
     * describing bytes that are gone. `--dry-run` writes nothing, stale
     * entry or not.
     * `links fix`/`links auto` additionally mtime-check every indexed entry
     * before their discovery pass, rescan files that changed on disk since
     * create-index, and upsert files the index does not know yet (with a
     * warning), so an externally edited vault is not silently trusted.
     *
     * If the index file is incompatible (e.g. after a hyalo upgrade) hyalo
     * falls back to a full disk scan automatically.
     *
     * STALENESS PROBE: on load, hyalo compares directory mtimes in the
     * vault (the root and every directory up to 3 levels below it — cheap,
     * directory-only stats, no file reads) against the snapshot's creation
     * time and warns `index older than vault` when one postdates it. When
     * that probe finds nothing, a second pass compares each indexed file's
     * recorded mtime against disk and stops at the first drift, so an
     * in-place overwrite (which moves no directory mtime) is named in the
     * warning rather than silently served. Remaining blind spot: **up to
     * about two seconds** — mtimes are compared as whole seconds and a
     * one-second tolerance is applied on top, so an edit made within the
     * snapshot's own second or the one after it is invisible (BUG-30,
     * iter-276: the old wording said "the same whole second", which
     * understated it by half). The warning never stops the run: stale
     * results are still served.
     */
    index: boolean;
    /**
     * Use the snapshot index at PATH instead of `.hyalo-index`
     *
     * Implies `--index`. Relative paths are resolved against the current
     * working directory (not the vault dir); absolute paths are used as-is.
     *
     * Reading a snapshot from anywhere on disk is allowed. *Writing* one is
     * not: on `create-index` / `drop-index` this flag is an alias for the
     * output path, and a path outside the vault is refused unless
     * `--allow-outside-vault` is also passed.
     *
     * Read-only commands skip the disk scan entirely. Mutation commands
     * patch the index in-place after each write — see `--index` for details.
     *
     * If the index file is incompatible hyalo falls back to a disk scan.
     */
    index_file?: string;
};

/**
 * A single tag with its file count.
 */
type TagSummaryEntry = {
    /**
     * Name of this entry.
     */
    name: string;
    /**
     * Number of matching occurrences.
     */
    count: number;
};

/**
 * Aggregate tag summary.
 * Used by `tags` command and `summary`.
 */
type TagSummary = {
    /**
     * Tag occurrence summary.
     */
    tags: Array<TagSummaryEntry>;
    /**
     * Total number of considered items.
     */
    total: number;
};

/**
 * High-level vault summary.
 */
type VaultSummary = {
    /**
     * Resolved vault directory (display string).
     */
    dir: string;
    /**
     * File counts and directory breakdown.
     */
    files: FileCounts;
    /**
     * Files with neither inbound nor outbound links.
     */
    orphans: number;
    /**
     * Files with inbound links and no outbound links.
     */
    dead_ends: number;
    /**
     * Vault-wide link health counts.
     */
    links: LinkHealthSummary;
    /**
     * Property usage and value summaries.
     */
    properties: Array<PropertySummaryEntry>;
    /**
     * Tag occurrence summary.
     */
    tags: TagSummary;
    /**
     * Task checkbox marker or grouped status value.
     */
    status: Array<StatusGroup>;
    /**
     * Task checkbox counts.
     */
    tasks: TaskCount;
    /**
     * Files ordered by modification time.
     */
    recent_files: Array<RecentFile>;
    /**
     * Schema lint counts — `None` when no `[schema]` block is configured.
     */
    schema?: LintSummary;
    /**
     * Format version of the snapshot this summary was computed from
     * (G4 / BUG-12, iter-276), or `None` for a disk scan.
     *
     * An agent comparing it against `hyalo config`'s
     * `snapshot_format_version` can tell an index this binary would refuse
     * from a fresh one *before* the numbers disagree.
     */
    index_format_version?: number;
};

type ApiGlobals = Partial<Omit<GlobalArgs, "format" | "jq" | "count" | "hints" | "no_hints">>;
type FindOptions = ApiGlobals & Partial<Omit<FindArgs, "filenames_only" | "filenames0" | "strict">>;
type ReadOptions = ApiGlobals & Partial<ReadArgs>;
type SummaryOptions = ApiGlobals & Partial<SummaryArgs>;
type ConfigOptions = ApiGlobals;
type FindResult = Array<FileObject>;
type SummaryResult = Omit<VaultSummary, "dir">;

interface ProcessResult {
    stdout: string;
    stderr: string;
    code: number;
}
interface TransportOptions {
    cwd?: string;
    timeoutMs: number;
    signal?: AbortSignal;
    stdin?: string | Uint8Array;
}
type HyaloTransport = (argv: readonly string[], options: TransportOptions) => Promise<ProcessResult>;
/** Receives the original successful stderr once; returned promises are awaited. */
type DiagnosticsCallback = (stderr: string) => void | Promise<void>;
interface ExecutionOptions {
    /**
     * Successful typed-call stderr. Defaults to process.stderr.write.
     * Empty stderr is ignored. Callback throws/rejections reject the call unchanged.
     * Failed calls retain diagnostics in their error; raw()/execute() retain streams only.
     */
    onDiagnostics?: DiagnosticsCallback;
    /** Explicit native binary. Useful for Cargo/Homebrew installations and tests. */
    binaryPath?: string;
    /** Process transport override. Pi uses this to retain its `pi.exec("hyalo", ...)` path. */
    transport?: HyaloTransport;
    /** Child working directory. Defaults to the caller's current directory. */
    cwd?: string;
    /** Wall-clock timeout in milliseconds. Defaults to 60 seconds. */
    timeoutMs?: number;
    /** Cancels the running child and rejects with `HyaloAbortError`. */
    signal?: AbortSignal;
    /** Standard input for commands such as `--files-from -`. */
    stdin?: string | Uint8Array;
}
type FindCallOptions = FindOptions & ExecutionOptions;
type ReadCallOptions = ReadOptions & ExecutionOptions;
type SummaryCallOptions = SummaryOptions & ExecutionOptions;
type ConfigCallOptions = ConfigOptions & ExecutionOptions;
declare class HyaloError extends Error {
    readonly exitCode: number;
    readonly stdout: string;
    readonly stderr: string;
    readonly envelope?: ErrorEnvelope;
    /** Committed paths and index disposition, retained even after output failure. */
    readonly effects?: ErrorEnvelope["effects"];
    readonly category?: ErrorEnvelope["category"];
    constructor(result: ProcessResult, envelope?: ErrorEnvelope);
}
declare class HyaloSpawnError extends Error {
    readonly cause: unknown;
    constructor(cause: unknown);
}
declare class HyaloParseError extends Error {
    readonly stdout: string;
    readonly stderr: string;
    readonly cause?: unknown;
    constructor(message: string, result: ProcessResult, cause?: unknown);
}
declare class HyaloTimeoutError extends Error {
    readonly timeoutMs: number;
    constructor(timeoutMs: number);
}
declare class HyaloAbortError extends Error {
    constructor();
}
declare class HyaloTransportError extends Error {
    constructor(message: string);
}
declare function createPiTransport(pi: {
    exec(command: string, args: string[], options: {
        cwd?: string;
        signal?: AbortSignal;
        timeout: number;
    }): Promise<ProcessResult & {
        killed: boolean;
    }>;
}): HyaloTransport;
/** @internal JSON mutation accessor for adapters; not exported by the package barrel.
 * Public set/task keep their successful ProcessResult streams. This accessor
 * executes exactly once and retains structured effects on HyaloError.
 */
declare function mutationReport<T>(argv: readonly string[], options?: ExecutionOptions): Promise<MutationReportEnvelope<T>>;
declare function find(options?: FindCallOptions): Promise<Envelope<FindResult>>;
declare function read(options?: ReadCallOptions): Promise<Envelope<ReadResult>>;
declare function summary(options?: SummaryCallOptions): Promise<Envelope<SummaryResult>>;
declare function config(options?: ConfigCallOptions): Promise<Envelope<ConfigResult>>;
interface SetOptions extends ExecutionOptions {
    file: string;
    property: string;
    tag?: string;
}
declare function set(options: SetOptions): Promise<ProcessResult>;
interface TaskOptions extends ExecutionOptions {
    file: string;
    mode: "all" | "section" | "line";
    section?: string;
    lines?: number[];
}
declare function task(options: TaskOptions): Promise<ProcessResult>;
declare function lint(file: string | undefined, options?: ExecutionOptions): Promise<ProcessResult>;
/** Run an arbitrary command without forcing JSON parsing or interpreting exit codes. */
declare function raw(argv: readonly string[], options?: ExecutionOptions): Promise<ProcessResult>;

/** The only configuration fields consumed by the Pi extension. */
interface PiConfigInfo {
    vaultDir: string | null;
    sessionSummary: boolean;
}
/**
 * Read the current config while retaining compatibility with pre-`[pi]`
 * releases and the flat config JSON shape accepted by the original extension.
 * The public `config()` contract remains the strict Rust-derived modern shape.
 */
declare function configForPi(options?: ExecutionOptions): Promise<PiConfigInfo>;

export { HyaloAbortError, HyaloError, HyaloParseError, HyaloSpawnError, HyaloTimeoutError, HyaloTransportError, config, configForPi, createPiTransport, find, lint, mutationReport, raw, read, set, summary, task };
export type { PiConfigInfo };
