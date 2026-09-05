//! Building the `hyalo ...` command string carried by a [`Hint`].
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{FindIndexHint, HintContext};

// ---------------------------------------------------------------------------
// Command builder
// ---------------------------------------------------------------------------

/// Push the global flags that were explicitly passed on the CLI.
///
/// Always the **last** thing a hint builder pushes (iter-213, dogfood UX-5).
/// Several builders used to push globals mid-command and then append
/// `--glob`/file targets after them, so the same `--format text` landed in a
/// different position depending on which hint you were reading — the block of
/// hints under one result set did not look like variations of one command.
pub(super) fn push_global_flags(parts: &mut Vec<String>, ctx: &HintContext) {
    if let Some(dir) = &ctx.dir {
        parts.push("--dir".to_owned());
        parts.push(shell_quote(dir));
    }
    if let Some(fmt) = &ctx.format {
        parts.push("--format".to_owned());
        parts.push(shell_quote(fmt));
    }
    if ctx.hints {
        parts.push("--hints".to_owned());
    }
    // BUG-15 (iter-277): `--site-prefix` decides which site-absolute links
    // resolve, so a hint that drops it reports a different set than the
    // command that printed it. Threaded on the same terms as `--dir` and
    // `--format`, and only onto commands that declare the flag.
    if let Some(prefix) = &ctx.site_prefix
        && command_accepts_site_prefix(parts)
    {
        parts.push("--site-prefix".to_owned());
        parts.push(shell_quote(prefix));
    }
    // iter-274 (UX-2): an indexed run's hints must stay indexed. `--index` /
    // `--index-file` is threaded exactly like `--dir` and `--format` — a hint
    // that silently drops it answers a different (and, on a large vault,
    // vastly slower) question than the command that produced it: 0.14 s vs
    // 1.2-1.4 s per follow-up on MDN. Only appended when the hinted command
    // actually accepts the flag, so `create-index`-style suggestions (which
    // build a snapshot rather than read one) are left alone.
    if !matches!(ctx.find_index, FindIndexHint::None) && command_accepts_index(parts) {
        push_find_index_file(parts, ctx);
    }
}

/// Whether the `hyalo` (sub)command spelled by `parts` declares an `--index`
/// flag, decided by walking clap's own command tree rather than a hand-kept
/// list that would drift the moment a command gains or loses the flag.
///
/// The leading tokens after `hyalo` are consumed only while they name a real
/// subcommand, so `hyalo find 'pattern'` resolves to `find` and stops.
fn command_accepts_index(parts: &[String]) -> bool {
    use clap::CommandFactory;

    let mut cmd = crate::cli::args::Cli::command();
    for token in parts.iter().skip(1) {
        if token.starts_with('-') {
            break;
        }
        let Some(sub) = cmd
            .get_subcommands()
            .find(|s| s.get_name() == token || s.get_all_aliases().any(|a| a == token))
            .cloned()
        else {
            break;
        };
        cmd = sub;
    }
    cmd.get_arguments().any(|a| a.get_long() == Some("index"))
}

/// Whether the `hyalo` (sub)command spelled by `parts` accepts
/// `--site-prefix`.
///
/// The flag is `global = true`, so clap attaches it to every subcommand and
/// this is effectively always true — the check is kept so a future
/// de-globalisation cannot silently produce hints that fail to parse.
fn command_accepts_site_prefix(parts: &[String]) -> bool {
    use clap::CommandFactory;

    let mut cmd = crate::cli::args::Cli::command();
    for token in parts.iter().skip(1) {
        if token.starts_with('-') {
            break;
        }
        let Some(sub) = cmd
            .get_subcommands()
            .find(|s| s.get_name() == token || s.get_all_aliases().any(|a| a == token))
            .cloned()
        else {
            break;
        };
        cmd = sub;
    }
    cmd.get_arguments()
        .any(|a| a.get_long() == Some("site-prefix"))
}

/// Push the graph/title filters that scope a `find` query (`--broken-links`,
/// `--orphan`, `--dead-end`, `--title`) so derived hints reproduce the same
/// filtered set. Kept separate from `push_global_flags` because these are
/// `find`-specific, not global, flags. `--sort`/`--reverse` are pushed
/// separately by [`push_find_sort`] since a couple of derived hints (e.g. the
/// literal "Sort by most recently modified" suggestion) intentionally
/// override them rather than preserve the active query's ordering.
pub(super) fn push_find_graph_filters(parts: &mut Vec<String>, ctx: &HintContext) {
    if ctx.broken_links_filter {
        parts.push("--broken-links".to_owned());
    }
    if ctx.orphan_filter {
        parts.push("--orphan".to_owned());
    }
    if ctx.dead_end_filter {
        parts.push("--dead-end".to_owned());
    }
    if let Some(title) = &ctx.title_filter {
        parts.push("--title".to_owned());
        parts.push(shell_quote(title));
    }
}

/// Push the active `--sort`/`--reverse` so derived hints (show-all, narrow-by-tag,
/// filter-by-status) reproduce the same ordering as the query that produced
/// them — otherwise a truncated, sorted result set's "Show all" hint would
/// silently revert to default ordering, changing which rows lead the output
/// even though the *count* still matches (a milder variant of BUG-8).
pub(super) fn push_find_sort(parts: &mut Vec<String>, ctx: &HintContext) {
    if let Some(sort) = &ctx.sort {
        parts.push("--sort".to_owned());
        parts.push(shell_quote(sort));
        if ctx.reverse {
            parts.push("--reverse".to_owned());
        }
    }
}

// ---------------------------------------------------------------------------
// HintBuilder (ARCH-4, iter-225)
// ---------------------------------------------------------------------------

/// Argv-based builder for the `cmd` string of a [`Hint`].
///
/// Before iter-225, hint commands were hand-assembled `String`s — parallel
/// copies of the CLI surface that could (and once did) reference flags the
/// real command does not accept (`tags --limit 0`), which is exactly why
/// `tests/e2e/hint_execution.rs` exists. `HintBuilder` is the single path
/// forward: the command is assembled as an *argv vector* and serialized
/// through [`shell_quote`], and [`HintBuilder::argv`] exposes that vector so
/// tests can feed it straight back into the real clap parser
/// (`crate::cli::args::Cli::try_parse_from`) — a hinted command that would
/// not run is now a unit-test failure, not an e2e-spawn discovery.
///
/// All new/edited hints must go through this API; the guard test
/// `no_raw_hyalo_command_literals` in this file fails the suite when a new
/// hand-written `"hyalo …"` string literal appears in non-test source.
#[derive(Debug, Clone)]
pub struct HintBuilder {
    parts: Vec<String>,
}

impl HintBuilder {
    /// Start a command: `hyalo <subcommand>` (or a subcommand group like
    /// `hyalo task toggle` — pass the words one by one via [`Self::raw`]).
    #[must_use]
    pub fn cmd(subcommand: &str) -> Self {
        let mut parts = vec!["hyalo".to_owned()];
        parts.extend(subcommand.split_whitespace().map(str::to_owned));
        Self { parts }
    }

    /// Append one shell-quoted argument (paths, patterns, values).
    #[must_use]
    pub fn arg(mut self, arg: &str) -> Self {
        self.parts.push(shell_quote(arg));
        self
    }

    /// Append pre-formed, unquoted tokens (flags like `--dry-run`, already
    /// validated enum values). Use sparingly — prefer [`Self::flag`] and
    /// [`Self::flag_value`].
    #[must_use]
    pub fn raw(mut self, token: &str) -> Self {
        self.parts.push(token.to_owned());
        self
    }

    /// Append a bare flag, e.g. `--apply`.
    #[must_use]
    pub fn flag(self, flag: &str) -> Self {
        self.raw(flag)
    }

    /// Append a flag and its shell-quoted value, e.g. `--rule HYALO006`.
    #[must_use]
    pub fn flag_value(self, flag: &str, value: &str) -> Self {
        self.raw(flag).arg(value)
    }

    /// Append multiple shell-quoted arguments at once.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for a in args {
            self.parts.push(shell_quote(a.as_ref()));
        }
        self
    }

    /// The raw argv vector (including the leading `"hyalo"`), for feeding
    /// into the real clap parser in tests.
    #[must_use]
    pub fn argv(&self) -> &[String] {
        &self.parts
    }

    /// Serialize to the display string: space-joined, each value already
    /// shell-quoted by [`shell_quote`].
    #[must_use]
    pub fn build(&self) -> String {
        self.parts.join(" ")
    }

    // --- crate-internal plumbing used by the `build_command_*` family ---
    // The family predates the builder and composes shared `push_*` helpers
    // over a `Vec<String>`; these accessors let it migrate without
    // duplicating the argv/serialisation rules (ARCH-4, iter-225).

    /// A builder holding only the `hyalo` program token.
    pub(crate) fn empty() -> Self {
        Self {
            parts: vec!["hyalo".to_owned()],
        }
    }

    /// Append one raw (unquoted) token in place.
    pub(crate) fn push_raw(&mut self, token: &str) {
        self.parts.push(token.to_owned());
    }

    /// Append one shell-quoted token in place.
    pub(crate) fn push_quoted(&mut self, token: &str) {
        self.parts.push(shell_quote(token));
    }

    /// Append the context's global flags (`--dir`, `--format`, `--hints`) and
    /// serialize. The last thing every derived hint does (iter-213, UX-5).
    pub(crate) fn finish(mut self, ctx: &HintContext) -> String {
        push_global_flags(&mut self.parts, ctx);
        self.build()
    }
}

/// Build a command that intentionally omits `--glob` (for file-specific hints).
pub(super) fn build_command_no_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut b = HintBuilder::empty();
    for arg in args {
        b.push_quoted(arg);
    }
    b.finish(ctx)
}

/// Build a command where `file_arg` is a positional file path following `subcommand_args`.
///
/// If `file_arg` starts with `-`, emits `--file <path>` instead of the bare positional
/// to prevent clap from interpreting the filename as a flag.
pub(super) fn build_command_with_file(
    ctx: &HintContext,
    subcommand_args: &[&str],
    file_arg: &str,
    trailing_args: &[&str],
) -> String {
    let mut b = HintBuilder::empty();
    for arg in subcommand_args {
        b.push_quoted(arg);
    }
    push_file_positional(&mut b.parts, file_arg);
    for arg in trailing_args {
        b.push_quoted(arg);
    }
    b.finish(ctx)
}

/// Build a command that propagates `--glob` when present.
pub(super) fn build_command_with_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut b = HintBuilder::empty();
    for arg in args {
        b.push_quoted(arg);
    }
    for glob in &ctx.glob {
        b.push_raw("--glob");
        b.push_quoted(glob);
    }
    b.finish(ctx)
}

/// Like `build_command_with_glob` but also preserves `--file` / positional file
/// targets so that lint hints don't widen scope from a single file to the whole
/// vault.
pub(super) fn build_command_with_glob_and_files(ctx: &HintContext, args: &[&str]) -> String {
    let mut b = HintBuilder::empty();
    for arg in args {
        b.push_quoted(arg);
    }
    for glob in &ctx.glob {
        b.push_raw("--glob");
        b.push_quoted(glob);
    }
    for ft in &ctx.file_targets {
        b.push_quoted(ft);
    }
    b.finish(ctx)
}

/// Build a `find` command that preserves the caller's existing filters (property,
/// tag, task, file targets) plus `--glob`, then appends `extra_args`.  Use this for
/// hints like sort and limit that refine the current query without changing its scope.
pub(super) fn build_find_command_preserving_filters(
    ctx: &HintContext,
    extra_args: &[&str],
) -> String {
    let mut b = HintBuilder::cmd("find");
    for pf in &ctx.property_filters {
        b.push_raw("--property");
        b.push_quoted(pf);
    }
    for tf in &ctx.tag_filters {
        b.push_raw("--tag");
        b.push_quoted(tf);
    }
    if let Some(task) = &ctx.task_filter {
        b.push_raw("--task");
        b.push_quoted(task);
    }
    for ft in &ctx.file_targets {
        b.push_raw("--file");
        b.push_quoted(ft);
    }
    push_find_graph_filters(&mut b.parts, ctx);
    push_find_sort(&mut b.parts, ctx);
    for arg in extra_args {
        b.push_quoted(arg);
    }
    for glob in &ctx.glob {
        b.push_raw("--glob");
        b.push_quoted(glob);
    }
    b.finish(ctx)
}

/// Render a snapshot-index path for a hint command, preferring the shortest
/// spelling that still runs verbatim from the user's working directory.
///
/// A snapshot path is the longest single token any hint carries, and a `find`
/// listing repeats the *same* one on every derived query — four or five copies
/// of one absolute path in a five-line block (iter-208a / UX-5). Eliding it
/// from the repeats is not an option: hints have to stay copy-pasteable, and a
/// `find` hint that quietly loses `--index-file` rescans the vault and answers
/// a different question. Shortening the path to its working-directory-relative
/// form keeps every hint runnable while removing most of the bulk, since an
/// index almost always lives inside the project it indexes.
#[must_use]
pub fn shorten_index_path_for_hint(path: &std::path::Path) -> String {
    let absolute = path.display().to_string();
    let Ok(cwd) = std::env::current_dir() else {
        return absolute;
    };
    match path.strip_prefix(&cwd) {
        Ok(rel) if !rel.as_os_str().is_empty() => {
            let relative = rel.to_string_lossy().replace('\\', "/");
            if relative.len() < absolute.len() {
                relative
            } else {
                absolute
            }
        }
        _ => absolute,
    }
}

/// Push `--index-file <path>` when the query ran against an explicit non-default
/// snapshot index. Derived `find` hints must query the same index or they would
/// silently rescan the vault (BUG-7 audit: `--index-file` was a dropped flag).
pub(super) fn push_find_index_file(parts: &mut Vec<String>, ctx: &HintContext) {
    match &ctx.find_index {
        FindIndexHint::None => {}
        FindIndexHint::Default => parts.push("--index".to_owned()),
        FindIndexHint::File(path) => {
            parts.push("--index-file".to_owned());
            parts.push(shell_quote(path));
        }
    }
}

/// Build a `find` command that preserves every active filter (property, tag,
/// task, file, graph, title, glob, index-file) *and* appends the caller's
/// existing body-search pattern as the leading positional argument. Used by
/// derived hints (narrow-by-tag / filter-by-status) that must compose with the
/// current query rather than replace it.
pub(super) fn build_find_command_composing(ctx: &HintContext, extra_args: &[&str]) -> String {
    let mut b = HintBuilder::cmd("find");
    if let Some(pat) = &ctx.body_pattern {
        b.push_quoted(pat);
    }
    for pf in &ctx.property_filters {
        b.push_raw("--property");
        b.push_quoted(pf);
    }
    for tf in &ctx.tag_filters {
        b.push_raw("--tag");
        b.push_quoted(tf);
    }
    if let Some(task) = &ctx.task_filter {
        b.push_raw("--task");
        b.push_quoted(task);
    }
    for ft in &ctx.file_targets {
        b.push_raw("--file");
        b.push_quoted(ft);
    }
    push_find_graph_filters(&mut b.parts, ctx);
    push_find_sort(&mut b.parts, ctx);
    for arg in extra_args {
        b.push_quoted(arg);
    }
    for glob in &ctx.glob {
        b.push_raw("--glob");
        b.push_quoted(glob);
    }
    b.finish(ctx)
}

/// Build a `find` command that replaces the body search pattern with `new_pattern`
/// while preserving all other existing filters (property, tag, task, file targets,
/// glob). The pattern is inserted as a positional argument immediately after `find`.
pub(super) fn build_find_command_with_pattern(ctx: &HintContext, new_pattern: &str) -> String {
    let mut b = HintBuilder::cmd("find");
    b.push_quoted(new_pattern);
    for pf in &ctx.property_filters {
        b.push_raw("--property");
        b.push_quoted(pf);
    }
    for tf in &ctx.tag_filters {
        b.push_raw("--tag");
        b.push_quoted(tf);
    }
    if let Some(task) = &ctx.task_filter {
        b.push_raw("--task");
        b.push_quoted(task);
    }
    for ft in &ctx.file_targets {
        b.push_raw("--file");
        b.push_quoted(ft);
    }
    for glob in &ctx.glob {
        b.push_raw("--glob");
        b.push_quoted(glob);
    }
    b.finish(ctx)
}

/// Push a file argument that is safe as a positional arg.
///
/// If the filename starts with `-`, clap would interpret it as a flag.
/// In that case, emit `--file <path>` (flag form) instead of the bare positional.
pub(super) fn push_file_positional(parts: &mut Vec<String>, file: &str) {
    if file.starts_with('-') {
        parts.push("--file".to_owned());
        parts.push(shell_quote(file));
    } else {
        parts.push(shell_quote(file));
    }
}

/// Wrap a string in single-quotes if it contains any shell-special characters.
///
/// Uses an allowlist of safe characters — anything not in the list triggers quoting.
/// Single-quoting avoids variable expansion and is safer than double-quoting.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty()
        || s.chars().any(|c| {
            !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '/' | ':' | '@' | '=' | ',' | '+')
        })
    {
        // In single-quoted strings, the only character that needs escaping is '
        // which is done by ending the quote, adding an escaped quote, and reopening.
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_owned()
    }
}
