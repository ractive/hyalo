//! Complete resolved operations used by scope-preserving continuations.
//!
//! These family-specific specs are built after saved views, configuration
//! defaults and `--files-from` have been resolved. They deliberately retain
//! raw argv values; shell quoting belongs only to the display renderer.

use super::{HintBuilder, HintContext};
use crate::cli::args::{Commands, FindArgs, LinksAction};

const MAX_REPLAY_TARGETS: usize = 64;
const MAX_REPLAY_TARGET_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
enum ReplayFiles {
    Files(Vec<String>),
    Unreplayable(String),
}

impl ReplayFiles {
    fn bounded(files: &[String]) -> Self {
        let bytes = files.iter().map(String::len).sum::<usize>();
        if files.len() > MAX_REPLAY_TARGETS || bytes > MAX_REPLAY_TARGET_BYTES {
            Self::Unreplayable(format!(
                "the resolved selection has {} paths ({} bytes), which exceeds the safe hint replay budget; rerun the original selection without using a broader command",
                files.len(),
                bytes
            ))
        } else {
            Self::Files(files.to_vec())
        }
    }

    fn push(&self, mut builder: HintBuilder) -> Result<HintBuilder, &str> {
        match self {
            Self::Files(files) => {
                for file in files {
                    builder = builder.raw(format!("--file={file}"));
                }
                Ok(builder)
            }
            Self::Unreplayable(reason) => Err(reason),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FindHintSpec {
    pattern: Option<String>,
    regexp: Option<String>,
    properties: Vec<String>,
    tags: Vec<String>,
    task: Option<String>,
    sections: Vec<String>,
    files: ReplayFiles,
    globs: Vec<String>,
    fields: Vec<String>,
    sort: Option<String>,
    reverse: bool,
    limit: Option<usize>,
    broken_links: bool,
    orphan: bool,
    dead_end: bool,
    title: Option<String>,
    language: Option<String>,
    strict: bool,
}

impl FindHintSpec {
    fn from_args(args: &FindArgs, effective_language: Option<&str>) -> Self {
        let files = if args.file_positional.is_empty() {
            &args.filters.file
        } else {
            &args.file_positional
        };
        Self {
            pattern: args.pattern.clone(),
            regexp: args.filters.regexp.clone(),
            properties: args.filters.properties.clone(),
            tags: args.filters.tag.clone(),
            task: args.filters.task.clone(),
            sections: args.filters.sections.clone(),
            files: ReplayFiles::bounded(files),
            globs: args.filters.glob.clone(),
            fields: args.filters.fields.clone(),
            sort: args.filters.sort.clone(),
            reverse: args.filters.reverse,
            limit: args.filters.limit,
            broken_links: args.filters.broken_links,
            orphan: args.filters.orphan,
            dead_end: args.filters.dead_end,
            title: args.filters.title.clone(),
            language: args
                .filters
                .language
                .clone()
                .or_else(|| effective_language.map(str::to_owned)),
            strict: args.filters.strict,
        }
    }

    pub(crate) fn continuation(
        &self,
        ctx: &HintContext,
        extra: &[&str],
    ) -> Result<HintBuilder, String> {
        let overrides = |flag: &str| extra.contains(&flag);
        let mut builder = HintBuilder::cmd("find");
        if let Some(regexp) = &self.regexp {
            builder = builder.flag_value("--regexp", regexp);
        }
        for property in &self.properties {
            builder = builder.flag_value("--property", property);
        }
        for tag in &self.tags {
            builder = builder.flag_value("--tag", tag);
        }
        if let Some(task) = &self.task {
            builder = builder.flag_value("--task", task);
        }
        for section in &self.sections {
            builder = builder.flag_value("--section", section);
        }
        builder = self.files.push(builder).map_err(str::to_owned)?;
        for glob in &self.globs {
            builder = builder.flag_value("--glob", glob);
        }
        if !overrides("--fields") && !self.fields.is_empty() {
            builder = builder.flag_value("--fields", &self.fields.join(","));
        }
        if !overrides("--sort")
            && let Some(sort) = &self.sort
        {
            builder = builder.flag_value("--sort", sort);
            if self.reverse {
                builder = builder.flag("--reverse");
            }
        }
        if !overrides("--limit")
            && let Some(limit) = self.limit
        {
            builder = builder.flag_value("--limit", &limit.to_string());
        }
        if self.broken_links {
            builder = builder.flag("--broken-links");
        }
        if self.orphan {
            builder = builder.flag("--orphan");
        }
        if self.dead_end {
            builder = builder.flag("--dead-end");
        }
        if let Some(title) = &self.title {
            builder = builder.flag_value("--title", title);
        }
        if let Some(language) = &self.language {
            builder = builder.flag_value("--language", language);
        }
        if self.strict {
            builder = builder.flag("--strict");
        }
        for arg in extra {
            builder = builder.raw(*arg);
        }
        builder = builder.with_globals(ctx);
        if let Some(pattern) = &self.pattern {
            builder = builder.raw("--").arg(pattern);
        }
        Ok(builder)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LintHintSpec {
    files: ReplayFiles,
    globs: Vec<String>,
    file_type: Option<String>,
    fix: bool,
    dry_run: bool,
    limit: Option<usize>,
    detailed: bool,
    rule: Option<String>,
    rule_prefix: Option<String>,
    max_per_rule: Option<usize>,
    fix_rules: Vec<String>,
    strict: bool,
    profile: Option<String>,
}

impl LintHintSpec {
    pub(crate) fn continuation(
        &self,
        ctx: &HintContext,
        extra: &[&str],
    ) -> Result<HintBuilder, String> {
        let overrides = |flag: &str| extra.contains(&flag);
        let mut builder = HintBuilder::cmd("lint");
        builder = self.files.push(builder).map_err(str::to_owned)?;
        for glob in &self.globs {
            builder = builder.flag_value("--glob", glob);
        }
        if let Some(file_type) = &self.file_type {
            builder = builder.flag_value("--type", file_type);
        }
        let requested_fix = overrides("--fix");
        if requested_fix || self.fix {
            builder = builder.flag("--fix");
        }
        if overrides("--dry-run") || self.dry_run && !requested_fix {
            builder = builder.flag("--dry-run");
        }
        if !overrides("--limit")
            && let Some(limit) = self.limit
        {
            builder = builder.flag_value("--limit", &limit.to_string());
        }
        if self.detailed {
            builder = builder.flag("--detailed");
        }
        if !overrides("--rule")
            && let Some(rule) = &self.rule
        {
            builder = builder.flag_value("--rule", rule);
        }
        if let Some(prefix) = &self.rule_prefix {
            builder = builder.flag_value("--rule-prefix", prefix);
        }
        if let Some(max) = self.max_per_rule {
            builder = builder.flag_value("--max-per-rule", &max.to_string());
        }
        for rule in &self.fix_rules {
            builder = builder.flag_value("--fix-rule", rule);
        }
        if self.strict {
            builder = builder.flag("--strict");
        }
        if let Some(profile) = &self.profile {
            builder = builder.flag_value("--profile", profile);
        }
        for arg in extra {
            if !matches!(*arg, "lint" | "--fix" | "--dry-run") {
                builder = builder.raw(*arg);
            }
        }
        Ok(builder.with_globals(ctx))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LinkFixHintSpec {
    threshold: f64,
    apply_fuzzy: bool,
    min_confidence: Option<f64>,
    globs: Vec<String>,
    ignore_targets: Vec<String>,
    expand_short_form: bool,
    case_insensitive: bool,
}

impl LinkFixHintSpec {
    pub(crate) fn apply(&self, ctx: &HintContext, fuzzy_floor: Option<&str>) -> HintBuilder {
        let mut builder = HintBuilder::cmd("links fix").flag("--apply");
        if (self.threshold - 0.8).abs() > f64::EPSILON {
            builder = builder.flag_value("--threshold", &self.threshold.to_string());
        }
        if self.apply_fuzzy || fuzzy_floor.is_some() {
            builder = builder.flag("--apply-fuzzy");
        }
        if let Some(floor) = fuzzy_floor {
            builder = builder.flag_value("--min-confidence", floor);
        } else if let Some(floor) = self.min_confidence {
            builder = builder.flag_value("--min-confidence", &floor.to_string());
        }
        for glob in &self.globs {
            builder = builder.flag_value("--glob", glob);
        }
        for target in &self.ignore_targets {
            builder = builder.flag_value("--ignore-target", target);
        }
        if self.expand_short_form {
            builder = builder.flag("--expand-short-form");
        }
        if self.case_insensitive {
            builder = builder.flag("--case-insensitive");
        }
        builder.with_globals(ctx)
    }

    pub(crate) fn review_at_floor(&self, ctx: &HintContext, floor: &str) -> HintBuilder {
        let mut builder = HintBuilder::cmd("links fix").flag_value("--min-confidence", floor);
        if (self.threshold - 0.8).abs() > f64::EPSILON {
            builder = builder.flag_value("--threshold", &self.threshold.to_string());
        }
        for glob in &self.globs {
            builder = builder.flag_value("--glob", glob);
        }
        for target in &self.ignore_targets {
            builder = builder.flag_value("--ignore-target", target);
        }
        if self.expand_short_form {
            builder = builder.flag("--expand-short-form");
        }
        if self.case_insensitive {
            builder = builder.flag("--case-insensitive");
        }
        builder.with_globals(ctx)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AutoLinkHintSpec {
    min_length: usize,
    exclude_titles: Vec<String>,
    first_only: bool,
    exclude_target_globs: Vec<String>,
    no_warn_common_titles: bool,
    file: Option<String>,
    globs: Vec<String>,
}

impl AutoLinkHintSpec {
    pub(crate) fn apply(&self, ctx: &HintContext) -> HintBuilder {
        let mut builder = HintBuilder::cmd("links auto").flag("--apply");
        if self.min_length != 3 {
            builder = builder.flag_value("--min-length", &self.min_length.to_string());
        }
        for title in &self.exclude_titles {
            builder = builder.flag_value("--exclude-title", title);
        }
        builder = if self.first_only {
            builder.flag("--first-only")
        } else {
            builder.flag("--no-first-only")
        };
        for glob in &self.exclude_target_globs {
            builder = builder.flag_value("--exclude-target-glob", glob);
        }
        if self.no_warn_common_titles {
            builder = builder.flag("--no-warn-common-titles");
        }
        if let Some(file) = &self.file {
            builder = builder.raw(format!("--file={file}"));
        }
        for glob in &self.globs {
            builder = builder.flag_value("--glob", glob);
        }
        builder.with_globals(ctx)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolvedHintSpec {
    Find(FindHintSpec),
    Lint(LintHintSpec),
    LinksFix(LinkFixHintSpec),
    LinksAuto(AutoLinkHintSpec),
}

#[derive(Clone, Copy)]
pub(crate) struct HintResolutionConfig<'a> {
    pub(crate) language: Option<&'a str>,
    pub(crate) lint_strict: bool,
    pub(crate) auto_exclude_titles: &'a [String],
    pub(crate) auto_exclude_titles_set: bool,
    pub(crate) auto_exclude_target_globs: &'a [String],
    pub(crate) auto_first_only: bool,
    pub(crate) auto_warn_common_titles: bool,
}

pub(crate) fn resolve_hint_spec(
    command: &Commands,
    config: HintResolutionConfig<'_>,
) -> Option<ResolvedHintSpec> {
    match command {
        Commands::Find(args) => Some(ResolvedHintSpec::Find(FindHintSpec::from_args(
            args,
            config.language,
        ))),
        Commands::Lint {
            file_positional,
            file,
            glob,
            r#type,
            fix,
            dry_run,
            limit,
            detailed,
            rule,
            rule_prefix,
            max_per_rule,
            fix_rule,
            strict,
            profile,
            ..
        } => {
            let files = if file_positional.is_empty() {
                file
            } else {
                file_positional
            };
            Some(ResolvedHintSpec::Lint(LintHintSpec {
                files: ReplayFiles::bounded(files),
                globs: glob.clone(),
                file_type: r#type.clone(),
                fix: *fix,
                dry_run: *dry_run,
                limit: *limit,
                detailed: *detailed,
                rule: rule.clone(),
                rule_prefix: rule_prefix.clone(),
                max_per_rule: *max_per_rule,
                fix_rules: fix_rule.clone(),
                strict: *strict || config.lint_strict,
                profile: profile.clone(),
            }))
        }
        Commands::Links {
            action:
                Some(LinksAction::Fix {
                    threshold,
                    apply_fuzzy,
                    min_confidence,
                    glob,
                    ignore_target,
                    expand_short_form,
                    case_insensitive,
                    ..
                }),
        } => Some(ResolvedHintSpec::LinksFix(LinkFixHintSpec {
            threshold: *threshold,
            apply_fuzzy: *apply_fuzzy,
            min_confidence: *min_confidence,
            globs: glob.clone(),
            ignore_targets: ignore_target.clone(),
            expand_short_form: *expand_short_form,
            case_insensitive: *case_insensitive,
        })),
        Commands::Links {
            action:
                Some(LinksAction::Auto {
                    min_length,
                    exclude_title,
                    first_only,
                    no_first_only,
                    exclude_target_glob,
                    no_warn_common_titles,
                    file,
                    glob,
                    ..
                }),
        } => {
            let mut titles = config.auto_exclude_titles.to_vec();
            titles.extend(exclude_title.iter().cloned());
            titles.sort();
            titles.dedup();
            let mut target_globs = config.auto_exclude_target_globs.to_vec();
            target_globs.extend(exclude_target_glob.iter().cloned());
            target_globs.sort();
            target_globs.dedup();
            let effective_first_only = if *no_first_only {
                false
            } else {
                *first_only || config.auto_first_only
            };
            Some(ResolvedHintSpec::LinksAuto(AutoLinkHintSpec {
                min_length: *min_length,
                exclude_titles: titles,
                first_only: effective_first_only,
                exclude_target_globs: target_globs,
                no_warn_common_titles: *no_warn_common_titles
                    || !config.auto_warn_common_titles
                    || config.auto_exclude_titles_set,
                file: file.clone(),
                globs: glob.clone(),
            }))
        }
        Commands::Links { action: None } => Some(ResolvedHintSpec::LinksFix(LinkFixHintSpec {
            threshold: 0.8,
            apply_fuzzy: false,
            min_confidence: None,
            globs: Vec::new(),
            ignore_targets: Vec::new(),
            expand_short_form: false,
            case_insensitive: false,
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::Cli;
    use crate::hints::{FindIndexHint, HintSource};
    use clap::Parser as _;

    fn config<'a>() -> HintResolutionConfig<'a> {
        HintResolutionConfig {
            language: None,
            lint_strict: false,
            auto_exclude_titles: &[],
            auto_exclude_titles_set: false,
            auto_exclude_target_globs: &[],
            auto_first_only: false,
            auto_warn_common_titles: true,
        }
    }

    #[test]
    fn find_continuation_changes_only_limit_and_keeps_raw_paths() {
        let cli = Cli::try_parse_from([
            "hyalo",
            "find",
            "time out",
            "--section",
            "Wanted",
            "--file=-odd note 'é'.md",
            "--sort",
            "title",
            "--reverse",
            "--limit",
            "2",
            "--language",
            "english",
        ])
        .unwrap();
        let ResolvedHintSpec::Find(spec) = resolve_hint_spec(&cli.command, config()).unwrap()
        else {
            panic!("expected find spec")
        };
        let mut ctx = HintContext::new(HintSource::Find);
        ctx.site_prefix = Some("docs root".to_owned());
        ctx.find_index = FindIndexHint::File("indexes/current index".to_owned());
        let hint = spec.continuation(&ctx, &["--limit", "0"]).unwrap();
        let argv = hint.argv();
        assert!(argv.contains(&"--file=-odd note 'é'.md".to_owned()));
        assert_eq!(
            argv.iter().filter(|arg| arg.as_str() == "--limit").count(),
            1
        );
        assert_eq!(
            argv.iter().position(|arg| arg == "--").unwrap() + 1,
            argv.len() - 1
        );
        assert_eq!(argv.last().map(String::as_str), Some("time out"));
        assert!(
            Cli::try_parse_from(argv).is_ok(),
            "raw continuation must parse: {argv:?}"
        );
        assert!(
            hint.build()
                .contains(&crate::hints::shell_quote("--file=-odd note 'é'.md"))
        );
    }

    #[test]
    fn lint_apply_keeps_selection_profile_and_rules_but_drops_preview() {
        let cli = Cli::try_parse_from([
            "hyalo",
            "lint",
            "--file=a.md",
            "--profile",
            "skills",
            "--rule",
            "HYALO001",
            "--rule-prefix",
            "MD",
            "--fix-rule",
            "MD009",
            "--fix",
            "--dry-run",
        ])
        .unwrap();
        let ResolvedHintSpec::Lint(spec) = resolve_hint_spec(&cli.command, config()).unwrap()
        else {
            panic!("expected lint spec")
        };
        let hint = spec
            .continuation(&HintContext::new(HintSource::Lint), &["--fix"])
            .unwrap();
        let argv = hint.argv();
        for token in [
            "--file=a.md",
            "--profile",
            "skills",
            "--rule",
            "HYALO001",
            "--rule-prefix",
            "MD",
            "--fix-rule",
            "MD009",
            "--fix",
        ] {
            assert!(
                argv.iter().any(|arg| arg == token),
                "missing {token}: {argv:?}"
            );
        }
        assert!(!argv.iter().any(|arg| arg == "--dry-run"));
        assert!(
            Cli::try_parse_from(argv).is_ok(),
            "lint continuation must parse: {argv:?}"
        );

        let type_cli =
            Cli::try_parse_from(["hyalo", "lint", "--type", "note", "--fix", "--dry-run"]).unwrap();
        let ResolvedHintSpec::Lint(type_spec) =
            resolve_hint_spec(&type_cli.command, config()).unwrap()
        else {
            panic!("expected lint spec")
        };
        let type_argv = type_spec
            .continuation(&HintContext::new(HintSource::Lint), &["--fix"])
            .unwrap()
            .argv()
            .to_vec();
        assert!(type_argv.windows(2).any(|pair| pair == ["--type", "note"]));
        assert!(Cli::try_parse_from(&type_argv).is_ok());
    }

    #[test]
    fn links_review_changes_only_the_fuzzy_floor() {
        let cli = Cli::try_parse_from([
            "hyalo",
            "links",
            "fix",
            "--threshold",
            "0.42",
            "--min-confidence",
            "0.7",
            "--ignore-target",
            "draft",
            "--case-insensitive",
        ])
        .unwrap();
        let ResolvedHintSpec::LinksFix(spec) = resolve_hint_spec(&cli.command, config()).unwrap()
        else {
            panic!("expected links-fix spec")
        };
        let argv = spec
            .review_at_floor(&HintContext::new(HintSource::LinksFix), "0")
            .argv()
            .to_vec();
        assert!(argv.windows(2).any(|pair| pair == ["--threshold", "0.42"]));
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--min-confidence", "0"])
        );
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--ignore-target", "draft"])
        );
        assert!(argv.iter().any(|arg| arg == "--case-insensitive"));
        assert!(Cli::try_parse_from(&argv).is_ok());
    }

    #[test]
    fn auto_link_apply_emits_effective_counter_flags_and_exclusions() {
        let cli = Cli::try_parse_from([
            "hyalo",
            "links",
            "auto",
            "--no-first-only",
            "--exclude-target-glob",
            "drafts/*",
            "--file=-odd.md",
        ])
        .unwrap();
        let configured_titles = vec!["Home".to_owned()];
        let configured_globs = vec!["templates/*".to_owned()];
        let cfg = HintResolutionConfig {
            auto_exclude_titles: &configured_titles,
            auto_exclude_target_globs: &configured_globs,
            auto_first_only: true,
            ..config()
        };
        let ResolvedHintSpec::LinksAuto(spec) = resolve_hint_spec(&cli.command, cfg).unwrap()
        else {
            panic!("expected auto-link spec")
        };
        let argv = spec
            .apply(&HintContext::new(HintSource::LinksAuto))
            .argv()
            .to_vec();
        assert!(argv.iter().any(|arg| arg == "--no-first-only"));
        assert!(argv.iter().any(|arg| arg == "templates/*"));
        assert!(argv.iter().any(|arg| arg == "drafts/*"));
        assert!(argv.iter().any(|arg| arg == "--file=-odd.md"));
        assert!(
            Cli::try_parse_from(&argv).is_ok(),
            "auto-link continuation must parse: {argv:?}"
        );
    }

    #[test]
    fn oversized_resolved_selection_refuses_executable_replay() {
        let mut argv = vec!["hyalo".to_owned(), "lint".to_owned()];
        for index in 0..=MAX_REPLAY_TARGETS {
            argv.push(format!("--file=n{index}.md"));
        }
        argv.extend(["--fix".to_owned(), "--dry-run".to_owned()]);
        let cli = Cli::try_parse_from(argv).unwrap();
        let ResolvedHintSpec::Lint(spec) = resolve_hint_spec(&cli.command, config()).unwrap()
        else {
            panic!("expected lint spec")
        };
        let reason = spec
            .continuation(&HintContext::new(HintSource::Lint), &["--fix"])
            .unwrap_err();
        assert!(reason.contains("exceeds the safe hint replay budget"));
    }
}
