/// `hyalo config` — print effective configuration to stdout.
///
/// Reads the `.hyalo.toml` in the CWD (same resolution as the normal config
/// loader) and prints:
///
/// - The resolved config file path (or `(none)` if absent).
/// - The raw file contents (when present), prefixed with a separator line.
/// - All effective values: `dir`, `cwd`, `format`, `hints`, `site_prefix`
///   (with the source it was resolved from).
///
/// Supports both text and JSON output via the standard `--format` flag.
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde_json::json;

use crate::output::{CommandOutcome, Format, format_success};

/// Data collected for the config report.
pub(crate) struct ConfigReport {
    /// Absolute path to the `.hyalo.toml` that was found, or `None`.
    pub config_path: Option<PathBuf>,
    /// Raw text of `.hyalo.toml` — only when the caller passed `--raw`.
    ///
    /// Opt-in since iter-213 (dogfood UX-2): a real `.hyalo.toml` is several
    /// kilobytes, and as a single JSON string it dwarfed every resolved value
    /// in `results` — the part of the output people actually came for.
    pub raw_contents: Option<String>,
    /// The parse/read diagnostic when a `.hyalo.toml` exists but could not be
    /// used, `None` when the config loaded (or when there is no config file).
    ///
    /// When this is `Some`, every other value in the report is a built-in
    /// default rather than something the file asked for. Reported in both
    /// renderings so a JSON consumer can detect the state without scraping
    /// stderr — before iter-213 `hyalo config` exited 0 with populated defaults
    /// and said nothing (dogfood UX-2).
    pub malformed: Option<String>,
    /// `true` when [`Self::dir`] was recovered from the malformed file rather
    /// than defaulted, so the "every value below is a built-in default" note
    /// can say `dir` is the exception (NEW-17, dogfood pre3).
    pub dir_salvaged: bool,
    /// Diagnostic when this config's `dir` resolves outside its own config
    /// directory (H-1, iter-221) — absolute, or a net `..` escape. `None` in
    /// the ordinary case. Distinct from [`Self::malformed`]: the file parsed
    /// fine, but its `dir` value is refused as a scope-widening attempt.
    /// Every other hyalo command refuses to run while this is `Some`;
    /// `hyalo config` is the one place it is safe to show and continue,
    /// because showing it is the whole point.
    pub dir_out_of_bounds: Option<String>,
    /// Current working directory.
    pub cwd: PathBuf,
    /// Resolved vault directory: the effective directory the CLI would use —
    /// a `--dir` override wins over the config's own `dir`.
    pub dir: PathBuf,
    /// `true` when a `--dir` override replaced the config's `dir` value.
    pub dir_overridden: bool,
    /// Resolved output format (from config or `None`).
    pub format: Option<String>,
    /// Whether hints are enabled.
    pub hints: bool,
    /// The **effective** site prefix — what site-absolute links like `/foo`
    /// actually resolve against, including the value hyalo auto-derives from
    /// the vault directory name. Before iter-203 this only reported the
    /// `.hyalo.toml` value and printed `(none)` while a derived prefix was
    /// silently in effect (dogfood UX-4).
    pub site_prefix: Option<String>,
    /// Where [`Self::site_prefix`] came from — flag, config, derived, or
    /// explicitly disabled.
    pub site_prefix_source: crate::config::SitePrefixSource,
    /// Vault-relative exempt globs from `[schema] exempt` (files bound to no schema).
    pub exempt: Vec<String>,
    /// Effective `[links.auto]` settings (iter-195a).
    pub links_auto: LinksAutoReport,
    /// Effective confidence floor for `links fix --apply-fuzzy` (iter-212):
    /// `[links] fuzzy_min_confidence` when set, otherwise the built-in
    /// [`hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE`]. Reported so a
    /// surprisingly small `--apply-fuzzy` run is one command away from an
    /// explanation.
    pub fuzzy_min_confidence: f64,
    /// Effective `[pi] session_summary` (opt-in): when `true`, the pi
    /// extension injects a `hyalo summary` snapshot into the LLM context at
    /// session start. `false` when unset.
    pub pi_session_summary: bool,
}

/// Effective `[links.auto]` settings, as `hyalo config` reports them.
///
/// These are the persisted `hyalo links auto` preferences; CLI flags extend the
/// two lists per-run and `--first-only` can turn `first_only` on for a run, so
/// what is reported here is the *baseline* every `links auto` invocation starts
/// from.
#[derive(Debug)]
pub(crate) struct LinksAutoReport {
    /// `[links.auto] exclude_titles`.
    pub exclude_titles: Vec<String>,
    /// `[links.auto] exclude_target_globs`.
    pub exclude_target_globs: Vec<String>,
    /// `[links.auto] first_only`.
    pub first_only: bool,
    /// `[links.auto] warn_common_titles` — `true` (the default) means `links
    /// auto` may print the advisory noisy-candidate-title note on stderr.
    pub warn_common_titles: bool,
}

impl Default for LinksAutoReport {
    /// Mirrors the resolved config defaults, where `warn_common_titles` is on.
    fn default() -> Self {
        Self {
            exclude_titles: Vec::new(),
            exclude_target_globs: Vec::new(),
            first_only: false,
            warn_common_titles: true,
        }
    }
}

/// Build and return the config report for `cwd`.
///
/// `effective` comes from [`crate::config::resolve_effective`] — the same
/// resolution every other command goes through — so `hyalo config` reports what
/// the CLI would actually use. It used to answer the `--dir` question on its
/// own by reloading `.hyalo.toml` from the target directory, which reported
/// `config_path: null` while a config *was* in effect (iter-201, H-4).
pub(crate) fn collect_config_report(
    cwd: &Path,
    effective: crate::config::EffectiveConfig,
    dir_overridden: bool,
    cli_site_prefix: Option<&str>,
    raw: bool,
) -> anyhow::Result<ConfigReport> {
    let crate::config::EffectiveConfig {
        config: resolved,
        dir,
        config_path,
        ..
    } = effective;

    // Report the prefix the rest of the CLI would use, derived through the
    // shared resolver rather than reading the raw config value (iter-203).
    let (site_prefix, site_prefix_source) =
        crate::config::resolve_site_prefix(cli_site_prefix, resolved.site_prefix.as_deref(), &dir);

    let raw_contents = match (raw, &config_path) {
        (true, Some(path)) => Some(
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
        ),
        _ => None,
    };

    Ok(ConfigReport {
        config_path,
        raw_contents,
        malformed: resolved.malformed,
        dir_salvaged: resolved.dir_salvaged,
        dir_out_of_bounds: resolved.dir_out_of_bounds,
        cwd: cwd.to_path_buf(),
        dir,
        dir_overridden,
        format: resolved.format,
        hints: resolved.hints,
        site_prefix,
        site_prefix_source,
        exempt: resolved.schema.exempt.patterns().to_vec(),
        links_auto: LinksAutoReport {
            exclude_titles: resolved.auto_link_exclude_titles,
            exclude_target_globs: resolved.auto_link_exclude_target_globs,
            first_only: resolved.auto_link_first_only,
            warn_common_titles: resolved.auto_link_warn_common_titles,
        },
        fuzzy_min_confidence: resolved
            .fuzzy_min_confidence
            .unwrap_or(hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE),
        pi_session_summary: resolved.pi_session_summary,
    })
}

/// Drill-down hints emitted by `hyalo config`.
///
/// `config` answers "what settings are in effect?"; the natural next questions
/// are "what is in the vault those settings point at?" and "what schema rules
/// will lint apply?". Both hints are plain, always-valid commands so the
/// execution-based hint gate (`tests/e2e/hint_execution.rs`) can run them.
pub(crate) fn config_hints(report: &ConfigReport) -> Vec<crate::hints::Hint> {
    let dir = report.dir.display().to_string();
    // Only pass --dir when the caller passed one. When `dir` came from the
    // config file, a bare `hyalo summary` run from the same CWD reads that very
    // file — re-emitting `--dir <configured>` adds nothing and, before
    // iter-201, actively changed which config applied (H-4).
    // ARCH-4 (iter-225): built through `HintBuilder` so these hints can no
    // longer drift from the real CLI surface — the argv is validated against
    // the actual clap parser in `hints::tests::hint_builder_commands_parse`.
    let summary = crate::hints::HintBuilder::cmd("summary");
    let types_list = crate::hints::HintBuilder::cmd("types list");
    let (summary, types_list) = if report.dir_overridden && dir != "." {
        (
            summary.flag_value("--dir", &dir),
            types_list.flag_value("--dir", &dir),
        )
    } else {
        (summary, types_list)
    };
    vec![
        crate::hints::Hint::new(
            "Overview of the vault this config points at".to_owned(),
            summary.build(),
        ),
        crate::hints::Hint::new(
            "Schema types lint will enforce".to_owned(),
            types_list.build(),
        ),
    ]
}

/// Build the JSON envelope for `hyalo config`.
///
/// Wrapped in the standard `{"results": ..., "hints": [...]}` envelope
/// (iter-192) so `--jq` addresses it exactly like every other command.
///
/// Two deliberate shape decisions:
/// - The config's own on/off switch is reported as `results.hints_enabled`,
///   not `results.hints`. The envelope's `hints` is an array of drill-down
///   commands; a boolean under the same name in the same document made
///   `.hints` mean two different things depending on nesting depth.
/// - `dir` appears both at `results.dir` and hoisted to the envelope root, the
///   latter for compatibility with pre-192 consumers of `hyalo config
///   --format json | jq .dir`.
pub(crate) fn config_envelope(report: &ConfigReport) -> serde_json::Value {
    let hints: Vec<serde_json::Value> = config_hints(report)
        .iter()
        .map(|h| json!({"description": &h.description, "cmd": &h.cmd, "writes": h.writes}))
        .collect();
    json!({
        "results": {
            "config_path": report.config_path.as_ref().map(|p| p.display().to_string()),
            // Always present (iter-213, UX-2). `malformed: true` means the
            // config file exists but could not be parsed, so every sibling
            // value below is a built-in default; `parse_error` carries the
            // diagnostic that was previously stderr-only.
            "malformed": report.malformed.is_some(),
            "parse_error": report.malformed,
            // `true` when `dir` below was salvaged from an otherwise
            // unusable file rather than defaulted (NEW-17, dogfood pre3) —
            // meaningful only alongside `malformed: true`.
            "dir_salvaged": report.dir_salvaged,
            // `true` when `dir` was refused for resolving outside its own
            // config directory (H-1, iter-221); `dir_out_of_bounds_reason`
            // carries the diagnostic. Every other command refuses to run
            // while this is `true` — `hyalo config` is the exception.
            "dir_out_of_bounds": report.dir_out_of_bounds.is_some(),
            "dir_out_of_bounds_reason": report.dir_out_of_bounds,
            // Only present with --raw; `null` otherwise so the key's shape
            // never changes between invocations.
            "raw_contents": report.raw_contents,
            "cwd": report.cwd.display().to_string(),
            "dir": report.dir.display().to_string(),
            "dir_overridden": report.dir_overridden,
            "format": report.format,
            "hints_enabled": report.hints,
            "site_prefix": report.site_prefix,
            "site_prefix_source": report.site_prefix_source.as_str(),
            "exempt": report.exempt,
            // Effective `[links.auto]` baseline for `hyalo links auto`
            // (iter-195a). Always present, empty lists / false when unset, so
            // consumers never have to distinguish "absent" from "off".
            "links_auto": {
                "exclude_titles": report.links_auto.exclude_titles,
                "exclude_target_globs": report.links_auto.exclude_target_globs,
                "first_only": report.links_auto.first_only,
                "warn_common_titles": report.links_auto.warn_common_titles,
            },
            // Effective `links fix --apply-fuzzy` confidence floor (iter-212).
            // Always a number — the built-in default when the key is unset.
            "links_fuzzy_min_confidence": report.fuzzy_min_confidence,
            // Effective `[pi]` agent-integration settings (iter-230).
            // Always present, `false` when unset, so consumers never have to
            // distinguish "absent" from "off".
            "pi": {
                "session_summary": report.pi_session_summary,
            },
        },
        "hints": hints,
        "dir": report.dir.display().to_string(),
    })
}

/// Run `hyalo config` and return a `CommandOutcome` ready for the output pipeline.
///
/// `show_hints` controls whether the text rendering appends the `-> hyalo …`
/// drill-down lines; the JSON envelope always carries a `hints` array (empty
/// when suppressed), matching every other command.
pub(crate) fn run_config(
    report: &ConfigReport,
    format: Format,
    show_hints: bool,
) -> CommandOutcome {
    match format {
        // `github` is rejected for non-lint commands upstream; treat as JSON here.
        Format::Json | Format::Github => run_config_json(report, show_hints),
        Format::Text => run_config_text(report, show_hints),
    }
}

fn run_config_json(report: &ConfigReport, show_hints: bool) -> CommandOutcome {
    let mut envelope = config_envelope(report);
    if !show_hints {
        envelope["hints"] = json!([]);
    }
    CommandOutcome::success(format_success(Format::Json, &envelope))
}

/// Render a list-valued config setting for the text report: comma-joined, or
/// `(none)` when empty (the convention already used for `exempt`).
fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_owned()
    } else {
        values.join(", ")
    }
}

fn run_config_text(report: &ConfigReport, show_hints: bool) -> CommandOutcome {
    let config_path_str = report
        .config_path
        .as_ref()
        .map_or_else(|| "(none)".to_owned(), |p| p.display().to_string());

    let format_str = report.format.as_deref().unwrap_or("(none)");
    // Always say where the prefix came from: `(none)` alone hid the fact that
    // an auto-derived prefix was deciding what `/foo` means (iter-203, UX-4).
    let mut site_prefix_str = format!(
        "{} ({})",
        report.site_prefix.as_deref().unwrap_or("(none)"),
        report.site_prefix_source.as_str()
    );
    // iter-204: auto-derivation can only ever produce ONE path segment — the
    // vault directory's name — while real corpora publish multi-segment URL
    // prefixes (MDN checked out into `en-us/` writes `/en-US/docs/...`).
    // Matching is case-insensitive, so casing is no longer the problem; the
    // missing second segment still is, and nothing detects it automatically.
    // Say so where the value is shown, since a wrong prefix silently breaks
    // every site-absolute link rather than erroring.
    if report.site_prefix_source == crate::config::SitePrefixSource::Derived {
        site_prefix_str.push_str(
            "\n  note: derived prefixes are a single path segment, matched case-insensitively; \
             if links are written with a multi-segment prefix (e.g. /en-US/docs/...), \
             pass --site-prefix 'en-US/docs' or set site_prefix in .hyalo.toml",
        );
    }
    let exempt_str = list_or_none(&report.exempt);

    // Lead with the integrity problem: when the config did not parse, every
    // line below it is a built-in default rather than a configured value, and
    // reading them as "the effective configuration" is the mistake to prevent.
    //
    // NEW-17 (dogfood pre3): `dir` is the one exception — a lenient re-read
    // salvages it from the broken file when possible (see `salvage_dir`), so
    // claiming it is a "built-in default" alongside everything else
    // contradicts the `dir: <value>` line printed right below. Say so only
    // when a value was actually salvaged *and* the printed `dir` is that
    // salvaged value: an unclosed table or invalid UTF-8 fails even the
    // lenient re-read (`dir` really is defaulted then), and a `--dir`
    // override replaces the printed `dir` with the flag's own text — the
    // salvaged value is no longer what is on screen, so the note would be
    // pointing at the wrong line.
    let malformed_str = match report.malformed.as_deref() {
        Some(diagnostic) => format!(
            "malformed: true\n  {}\n  note: every value below is a built-in default, \
             not what the file asked for{}\n",
            diagnostic.trim_end().replace('\n', "\n  "),
            if report.dir_salvaged && !report.dir_overridden {
                " — except dir, which was salvaged from the file despite the rest failing to parse"
            } else {
                ""
            }
        ),
        None => String::new(),
    };

    // H-1 (iter-221): a `dir` refused for resolving outside its own config
    // directory. Distinct from `malformed_str` above — the file parsed fine,
    // but this specific value was refused as a scope-widening attempt, and
    // `dir` below is the hardcoded "." default, not the offending value.
    let dir_out_of_bounds_str = match report.dir_out_of_bounds.as_deref() {
        Some(diagnostic) => format!(
            "dir_out_of_bounds: true\n  {}\n  note: dir below is the built-in default, not the \
             value the config asked for — every other hyalo command refuses to run until this \
             is fixed\n",
            diagnostic.trim_end().replace('\n', "\n  "),
        ),
        None => String::new(),
    };

    // Annotate the dir line when a `--dir` override is in effect, so the report
    // makes the shadow explicit rather than silently reporting the override.
    let dir_suffix = if report.dir_overridden {
        "  (--dir override)"
    } else {
        ""
    };

    let mut out = format!(
        "{dir_out_of_bounds_str}{malformed_str}config: {config_path_str}\ncwd: {cwd}\ndir: {dir}{dir_suffix}\nformat: {format_str}\nhints: {hints}\nsite_prefix: {site_prefix_str}\nexempt: {exempt_str}\n\
         links.auto.exclude_titles: {auto_titles}\nlinks.auto.exclude_target_globs: {auto_globs}\nlinks.auto.first_only: {auto_first_only}\nlinks.auto.warn_common_titles: {auto_warn_common}\nlinks.fuzzy_min_confidence: {fuzzy_floor}\npi.session_summary: {pi_session_summary}\n",
        cwd = report.cwd.display(),
        dir = report.dir.display(),
        hints = report.hints,
        auto_titles = list_or_none(&report.links_auto.exclude_titles),
        auto_globs = list_or_none(&report.links_auto.exclude_target_globs),
        auto_first_only = report.links_auto.first_only,
        auto_warn_common = report.links_auto.warn_common_titles,
        fuzzy_floor = report.fuzzy_min_confidence,
        pi_session_summary = report.pi_session_summary,
    );

    if let Some(ref contents) = report.raw_contents {
        out.push('\n');
        out.push_str("--- .hyalo.toml ---\n");
        out.push_str(contents);
        if !contents.ends_with('\n') {
            out.push('\n');
        }
    }

    // Drill-down hints, rendered with the same `  -> cmd  # description` layout
    // the JSON pipeline's text mode uses (crate::output::format_envelope), so
    // `config` reads like every other command (iter-192).
    if show_hints {
        for hint in config_hints(report) {
            out.push_str("\n  -> ");
            out.push_str(&hint.cmd);
            out.push_str("  # ");
            out.push_str(&hint.description);
        }
        out.push('\n');
    }

    CommandOutcome::RawOutput(out)
}
