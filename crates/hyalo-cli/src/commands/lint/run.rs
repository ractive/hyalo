//! The `hyalo lint` dispatch arm.
//!
//! Split out of the single 4,005-line `commands/lint.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `commands::lint::...` paths and behaviour are
//! unchanged.

use super::{ExtLintOptions, lint_files_extended, validate_schema_config, validate_views};
use crate::output::CommandOutcome;
use anyhow::Result;
use hyalo_mdlint::schema::{FileLintResult, FixMode, RULE_ID_BROKEN_LINK, Severity};

/// ARCH-1 (iter-225): the `hyalo lint` dispatch arm, extracted verbatim from
/// `dispatch.rs` so its warning policy (`[lint] ignore` notices), profile
/// selection and exit-code logic are unit-testable in-process.
///
/// `profile`, `files_from` and `index_flags` were consumed earlier in
/// `run.rs`/dispatch (profile overlay into `ctx.lint_profiles`, snapshot
/// loading), so they never reach here.
use crate::dispatch::{
    CommandContext, adapt_view_result_to_ext, inject_ext_file_result, maybe_case_index,
};
use hyalo_core::mode_enabled;

#[allow(clippy::too_many_arguments)]
#[allow(clippy::fn_params_excessive_bools)] // moved verbatim from the dispatch arm
#[allow(clippy::needless_pass_by_value)] // args moved verbatim from the clap variant
pub(crate) fn run(
    ctx: &mut CommandContext<'_>,
    file_positional: Vec<String>,
    file: Vec<String>,
    glob: Vec<String>,
    lint_type: Option<String>,
    fix: bool,
    dry_run: bool,
    cli_limit: Option<usize>,
    detailed: bool,
    rule: Option<String>,
    rule_prefix: Option<String>,
    max_per_rule: Option<usize>,
    fix_rule: Vec<String>,
    lint_strict_flag: bool,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let snapshot_index = &mut *ctx.snapshot_index;
    let index_path = ctx.index_path;
    let profile_active = |name: &str| ctx.lint_profiles.iter().any(|p| p == name);
    let okf_profile_active = profile_active("okf");
    let madr_profile_active = profile_active("madr");
    let skills_profile_active = profile_active("skills");
    let changelog_profile_active = profile_active("changelog");

    {
        // --strict flag wins over config value; config value is the fallback.
        let effective_strict = lint_strict_flag || ctx.lint_strict;
        // Resolve --type to a glob pattern from its filename_template.
        let type_glob: Option<String> = if let Some(type_name) = lint_type {
            use hyalo_core::filename_template::FilenameTemplate;
            match ctx.schema.types.get(&type_name) {
                Some(ts) => match &ts.filename_template {
                    Some(template_str) => match FilenameTemplate::parse(template_str) {
                        Ok(tpl) => Some(tpl.to_glob()),
                        Err(e) => {
                            return Ok(crate::output::CommandOutcome::UserError(
                                crate::output::format_error(
                                    ctx.user_format,
                                    &format!(
                                        "invalid filename_template for type '{type_name}': {e}"
                                    ),
                                    None,
                                    None,
                                    None,
                                ),
                            ));
                        }
                    },
                    None => {
                        return Ok(crate::output::CommandOutcome::UserError(
                            crate::output::format_error(
                                ctx.user_format,
                                &format!("type '{type_name}' has no filename_template defined"),
                                None,
                                Some(
                                    "set one with: hyalo types set <name> --filename-template <pattern>",
                                ),
                                None,
                            ),
                        ));
                    }
                },
                None => {
                    return Ok(crate::output::CommandOutcome::UserError(
                        crate::output::format_error(
                            ctx.user_format,
                            &format!("unknown type '{type_name}'"),
                            None,
                            Some("run `hyalo types list` to see available types"),
                            None,
                        ),
                    ));
                }
            }
        } else {
            None
        };

        // Build the file list. Positional args are treated as --file
        // targets (repeatable), preserving their command-line order ahead
        // of any --file values.
        let mut files_arg: Vec<String> = file_positional;
        files_arg.extend(file);
        // --type expands to a glob that overrides file/glob args.
        let effective_glob: Vec<String> = if let Some(g) = type_glob {
            vec![g]
        } else {
            glob
        };

        let mut file_pairs = match crate::commands::collect_files(
            dir,
            &files_arg,
            &effective_glob,
            ctx.user_format,
        )? {
            crate::commands::FilesOrOutcome::Files(f) => f,
            crate::commands::FilesOrOutcome::Outcome(o) => return Ok(o),
        };

        // Reach a repo-root CHANGELOG.md that lives *outside* the vault dir.
        // When the changelog profile is active and `[changelog] path`
        // resolves to a file the vault walk can't see (the common
        // docs-subdir layout), add it to the lint set so
        // `lint --profile changelog` validates the real file without
        // `--dir .` gymnastics. Only for an unscoped run (no explicit
        // `--file`/`--glob`), so a targeted lint is never surprised by an
        // extra file.
        if files_arg.is_empty()
            && effective_glob.is_empty()
            && ctx.lint_profiles.iter().any(|p| p == "changelog")
            && let Ok(changelog_file) = crate::commands::changelog::resolve_changelog_file(
                dir,
                ctx.config_dir,
                ctx.changelog_path,
            )
            && changelog_file.is_file()
        {
            let rel = hyalo_core::discovery::relative_path(dir, &changelog_file);
            // Only inject when the file is outside the vault dir (a relative
            // path that climbs out, or an absolute one) — an in-vault
            // CHANGELOG.md was already discovered by the walk.
            let outside = rel.starts_with("..") || std::path::Path::new(&rel).is_absolute();
            let already = file_pairs.iter().any(|(p, _)| p == &changelog_file);
            if outside && !already {
                let display = changelog_file
                    .file_name()
                    .map_or_else(|| rel.clone(), |n| n.to_string_lossy().into_owned());
                file_pairs.push((changelog_file, display));
            }
        }

        let fix_mode = if fix {
            if dry_run {
                self::FixMode::DryRun
            } else {
                self::FixMode::Apply
            }
        } else {
            self::FixMode::Off
        };

        // Filter out files matching `[lint] ignore` entries.
        //
        // Each entry is matched against the vault-relative path (with `/`
        // separators) as a glob: `vendor/**/*.md`, `legacy/known-bad.md`,
        // `templates/*.md`. An entry without glob meta-characters is matched
        // literally (exact path equality on the normalized path).
        // `--file`/positional args are "explicit": the user named them
        // directly rather than sweeping a glob. When an explicit file is
        // excluded by `[lint] ignore` it must produce a visible notice, not
        // a silent `0 files checked` (df-scale silent-drop family).
        let explicit_named = !files_arg.is_empty();
        // UX-1 (dogfood pre3): how many files `[lint] ignore` dropped from
        // this run, regardless of scope. Zero when there is no `[lint]
        // ignore` at all (the branch below never runs).
        let mut lint_ignored_count: usize = 0;
        let filtered_pairs: Vec<_> = if ctx.lint_ignore.is_empty() {
            file_pairs
        } else {
            use globset::{GlobBuilder, GlobSetBuilder};
            let mut builder = GlobSetBuilder::new();
            let mut build_failed = false;
            for pat in ctx.lint_ignore {
                match GlobBuilder::new(pat)
                    .literal_separator(true)
                    .backslash_escape(true)
                    .build()
                {
                    Ok(g) => {
                        builder.add(g);
                    }
                    Err(e) => {
                        crate::warn::warn(format!("invalid [lint] ignore pattern {pat:?}: {e}"));
                        build_failed = true;
                    }
                }
            }
            let set = if build_failed {
                None
            } else {
                builder.build().ok()
            };
            // UX-1 (dogfood pre3): a `--glob` sweep is just as "explicit"
            // as `--file` when it comes to the *all-matches-ignored* case
            // — the user asked for a specific set of files by name or by
            // pattern either way, and getting back an empty, vacuously
            // green lint run with no explanation is the same silent-drop
            // trap regardless of which form they used.
            let explicit_glob = !effective_glob.is_empty();
            match set {
                Some(set) => {
                    let mut ignored_named: Vec<String> = Vec::new();
                    let before = file_pairs.len();
                    let kept: Vec<_> = file_pairs
                        .into_iter()
                        .filter(|(_, rel)| {
                            let norm = rel.replace('\\', "/");
                            let matched = set.is_match(&norm);
                            if matched && (explicit_named || explicit_glob) {
                                ignored_named.push(norm);
                            }
                            !matched
                        })
                        .collect();
                    // UX-1: unconditional count, regardless of how the
                    // file set was scoped — feeds the bare-sweep summary
                    // line ("N files checked (M ignored by [lint]
                    // ignore)") so a full-vault run stops hiding how much
                    // of the vault it silently skipped.
                    lint_ignored_count = before - kept.len();
                    // Notice for an explicit scope (named files, or a
                    // --glob whose matches are *entirely* ignored)
                    // silently excluded by `[lint] ignore` — otherwise the
                    // run reports `0 files checked, no issues` with no
                    // hint why. A --glob that only partially matches the
                    // ignore list stays quiet here: the bare-sweep count
                    // above already makes the exclusion visible without
                    // the noise of naming every match.
                    let glob_all_ignored = explicit_glob && !explicit_named && kept.is_empty();
                    if !ignored_named.is_empty() && (explicit_named || glob_all_ignored) {
                        let list = ignored_named.join(", ");
                        let plural = if ignored_named.len() == 1 {
                            "file"
                        } else {
                            "files"
                        };
                        crate::warn::warn(format!(
                            "{} named {plural} excluded by [lint] ignore (not linted): {list}",
                            ignored_named.len()
                        ));
                    }
                    kept
                }
                // If building the set failed (warning already emitted above),
                // fall back to no filtering rather than silently ignoring
                // potentially relevant files.
                None => file_pairs,
            }
        };

        // Decide which lint path to use: extended (body+frontmatter) or legacy.
        // The extended path is used whenever the new flags are active OR when the
        // engine is available.  We always use the extended path now.
        let md_engine = hyalo_mdlint::HyaloLintEngine::create()
            .map_err(|e| anyhow::anyhow!("failed to create lint engine: {e}"))?;

        // M-10: `--rule` names a rule id, so validate it the way
        // `lint-rules show` does instead of silently linting with a filter
        // that matches nothing (an unknown id used to exit 0 with "no
        // issues found", which reads as "clean" in CI). The match is
        // case-insensitive and canonicalizes to the catalog spelling so
        // `--rule hyalo006` behaves exactly like `--rule HYALO006`.
        let rule = match rule {
            Some(raw) => match md_engine.rule_entry_ci(&raw) {
                Some(entry) => Some(entry.id.clone()),
                None => {
                    return Ok(crate::output::CommandOutcome::UserError(
                        crate::output::format_error(
                            ctx.user_format,
                            &format!("no such rule: {raw}"),
                            None,
                            Some("run `hyalo lint-rules list` to see available rules"),
                            None,
                        ),
                    ));
                }
            },
            None => None,
        };
        // iter-210 BUG-5: a prefix that selects no rule is as much a typo
        // as an unknown `--rule` id, and it used to be *worse* than one:
        // the empty filter fell through to "no filtering", so `--rule-prefix
        // nope` warned and then ran every MD rule anyway at exit 0. Fail it
        // with the same error shape as `--rule`. The match is
        // case-insensitive, so a matching family is unaffected.
        if let Some(prefix) = rule_prefix.as_deref()
            && md_engine.rules_matching_prefix_ci(prefix).is_empty()
        {
            return Ok(crate::output::CommandOutcome::UserError(
                crate::output::format_error(
                    ctx.user_format,
                    &format!("no rule matches prefix: {prefix}"),
                    None,
                    Some("run `hyalo lint-rules list` to see available rules"),
                    None,
                ),
            ));
        }

        // `--format github` emits one annotation per violation, so every
        // finding must be materialized: force `detailed` and lift the
        // per-rule / per-file caps. Otherwise the summary-mode truncation
        // would silently drop annotations from the PR check.
        let github_output = ctx.user_format == crate::output::Format::Github;

        let max_per_rule_eff = if github_output {
            // `usize::MAX`, not `0` — the fix-mode output path uses
            // `.take(n)`/`.min(n)` to cap violations shown per rule, so `0`
            // would truncate every rule group to zero shown violations and
            // silently drop annotations. The non-fix path separately bypasses
            // this cap via `opts.detailed`, but fix-mode does not, so the
            // sentinel must mean "unlimited" in both paths.
            usize::MAX
        } else {
            max_per_rule.unwrap_or_else(|| ctx.md_lint.max_violations_per_rule())
        };
        // CLI --limit overrides the config max_files when provided.
        // `--limit 0` is documented as "unlimited" (matches `--count
        // --limit 0`): map it to `usize::MAX` so it lifts the file cap
        // instead of truncating the list to zero (ff-rdp B5, mapl BUG-4).
        let max_files_eff = match cli_limit {
            Some(0) => usize::MAX,
            Some(n) => n,
            None if github_output => usize::MAX,
            None => ctx.md_lint.max_files(),
        };

        // HYALO006 (broken-link): build the vault-wide resolution context
        // ONCE per invocation, but only when the rule will actually run —
        // i.e. it is enabled (no `[lint.rules.HYALO006] enabled = false`)
        // AND selected by any `--rule` / `--rule-prefix` filter. Building it
        // means seeding a case/stem index (from the snapshot when `--index`
        // is active, else a single disk walk) — never per file.
        let hyalo006_enabled = ctx
            .md_lint
            .rules
            .get(self::RULE_ID_BROKEN_LINK)
            .and_then(hyalo_mdlint::RuleOverride::enabled)
            .unwrap_or(true);
        let hyalo006_selected = match (&rule, &rule_prefix) {
            (Some(r), _) => r == self::RULE_ID_BROKEN_LINK,
            (None, Some(p)) => self::RULE_ID_BROKEN_LINK
                .to_ascii_uppercase()
                .starts_with(&p.to_ascii_uppercase()),
            (None, None) => true,
        };
        let link_lint_ctx = if hyalo006_enabled && hyalo006_selected {
            let case_index = maybe_case_index(
                ctx.case_insensitive_mode,
                dir,
                true,
                (*snapshot_index).as_ref(),
            )
            .unwrap_or_default();
            hyalo_mdlint::profiles::link::LinkLintContext::new(
                dir,
                site_prefix.map(str::to_owned),
                case_index,
                ctx.frontmatter_link_props.map(<[String]>::to_vec),
            )
        } else {
            None
        };

        let mut ext_opts = self::ExtLintOptions {
            fix: fix_mode,
            detailed: detailed || github_output,
            rule_filter: rule.as_deref(),
            rule_prefix: rule_prefix.as_deref(),
            max_per_rule: max_per_rule_eff,
            max_files: max_files_eff,
            fix_rules: &fix_rule,
            snapshot_index,
            index_path,
            vault_dir: dir,
            strict: effective_strict,
            // The active profile is resolved in run.rs (CLI `--profile`
            // overlay OR `[lint] profile` in config); captured above.
            okf_profile: okf_profile_active,
            madr_profile: madr_profile_active,
            skills_profile: skills_profile_active,
            changelog_profile: changelog_profile_active,
            // Resolved once per `hyalo lint` invocation (not re-probed
            // per file) so `[schema] exempt` globs fold case the same way
            // `hyalo okf index` treats `INDEX.md` on case-insensitive
            // filesystems (macOS/Windows default).
            case_insensitive: mode_enabled(ctx.case_insensitive_mode, dir),
            link_lint_ctx,
            files_ignored: lint_ignored_count,
        };

        let (outcome, mut counts) = self::lint_files_extended(
            &filtered_pairs,
            ctx.schema,
            &md_engine,
            ctx.md_lint,
            &mut ext_opts,
        )?;

        // Additional config-level lint: check view definitions AND that
        // [schema] itself parses (review round finding 2 — a malformed
        // [schema] block used to be only a stderr warning, so `lint
        // --strict` could exit 0 on a vault whose schema validation was
        // silently disabled). Merged into one `.hyalo.toml` pseudo-file
        // result so both kinds of config-level problem show up together
        // rather than as two separate file entries.
        let mut config_result: Option<self::FileLintResult> = self::validate_views(ctx.config_dir);
        if let Some(schema_result) = self::validate_schema_config(ctx.config_dir, effective_strict)
        {
            match &mut config_result {
                Some(existing) => existing.violations.extend(schema_result.violations),
                None => config_result = Some(schema_result),
            }
        }
        let outcome = if let Some(config_result) = config_result {
            for v in &config_result.violations {
                match v.severity {
                    self::Severity::Error => counts.errors += 1,
                    self::Severity::Warn => counts.warnings += 1,
                }
            }
            counts.files_with_issues += 1;
            // Adapt into the new shape — inject as a file with a SCHEMA group.
            let adapted = adapt_view_result_to_ext(&config_result);
            inject_ext_file_result(outcome, &adapted)?
        } else {
            outcome
        };

        // Signal exit code 1 when errors remain after fixes (set before returning).
        if counts.errors > 0 {
            ctx.exit_code_override = Some(1);
        }

        Ok(outcome)
    }
}
