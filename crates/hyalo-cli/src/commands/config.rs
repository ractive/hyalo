/// `hyalo config` — print effective configuration to stdout.
///
/// Reads the `.hyalo.toml` in the CWD (same resolution as the normal config
/// loader) and prints:
///
/// - The resolved config file path (or `(none)` if absent).
/// - The raw file contents (when present), prefixed with a separator line.
/// - All effective values: `dir`, `cwd`, `format`, `hints`, `site_prefix`.
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
    /// Raw text of `.hyalo.toml` (when `config_path` is `Some`).
    pub raw_contents: Option<String>,
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
    /// Resolved site prefix (from config or `None`).
    pub site_prefix: Option<String>,
    /// Vault-relative exempt globs from `[schema] exempt` (files bound to no schema).
    pub exempt: Vec<String>,
    /// Effective `[links.auto]` settings (iter-195a).
    pub links_auto: LinksAutoReport,
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
    /// auto` may print the advisory common-English-word note on stderr.
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
) -> anyhow::Result<ConfigReport> {
    let crate::config::EffectiveConfig {
        config: resolved,
        dir,
        config_path,
        ..
    } = effective;

    let raw_contents = match &config_path {
        Some(path) => Some(
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
        ),
        None => None,
    };

    Ok(ConfigReport {
        config_path,
        raw_contents,
        cwd: cwd.to_path_buf(),
        dir,
        dir_overridden,
        format: resolved.format,
        hints: resolved.hints,
        site_prefix: resolved.site_prefix,
        exempt: resolved.schema.exempt.patterns().to_vec(),
        links_auto: LinksAutoReport {
            exclude_titles: resolved.auto_link_exclude_titles,
            exclude_target_globs: resolved.auto_link_exclude_target_globs,
            first_only: resolved.auto_link_first_only,
            warn_common_titles: resolved.auto_link_warn_common_titles,
        },
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
    let suffix = if report.dir_overridden && dir != "." {
        format!(" --dir {}", crate::hints::shell_quote(&dir))
    } else {
        String::new()
    };
    vec![
        crate::hints::Hint::new(
            "Overview of the vault this config points at".to_owned(),
            format!("hyalo summary{suffix}"),
        ),
        crate::hints::Hint::new(
            "Schema types lint will enforce".to_owned(),
            format!("hyalo types list{suffix}"),
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
            "raw_contents": report.raw_contents,
            "cwd": report.cwd.display().to_string(),
            "dir": report.dir.display().to_string(),
            "dir_overridden": report.dir_overridden,
            "format": report.format,
            "hints_enabled": report.hints,
            "site_prefix": report.site_prefix,
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
    let site_prefix_str = report.site_prefix.as_deref().unwrap_or("(none)");
    let exempt_str = list_or_none(&report.exempt);

    // Annotate the dir line when a `--dir` override is in effect, so the report
    // makes the shadow explicit rather than silently reporting the override.
    let dir_suffix = if report.dir_overridden {
        "  (--dir override)"
    } else {
        ""
    };

    let mut out = format!(
        "config: {config_path_str}\ncwd: {cwd}\ndir: {dir}{dir_suffix}\nformat: {format_str}\nhints: {hints}\nsite_prefix: {site_prefix_str}\nexempt: {exempt_str}\n\
         links.auto.exclude_titles: {auto_titles}\nlinks.auto.exclude_target_globs: {auto_globs}\nlinks.auto.first_only: {auto_first_only}\nlinks.auto.warn_common_titles: {auto_warn_common}\n",
        cwd = report.cwd.display(),
        dir = report.dir.display(),
        hints = report.hints,
        auto_titles = list_or_none(&report.links_auto.exclude_titles),
        auto_globs = list_or_none(&report.links_auto.exclude_target_globs),
        auto_first_only = report.links_auto.first_only,
        auto_warn_common = report.links_auto.warn_common_titles,
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
