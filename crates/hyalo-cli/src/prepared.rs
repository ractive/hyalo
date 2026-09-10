//! Application validation authority. Clap remains a transport DTO; only this
//! module constructs execution requests and output plans.
mod selection;
pub(crate) use selection::PreparedSelection;

use crate::cli::args::{
    ChangelogAction, Cli, Commands, FindArgs, FindFilters, LinksAction, LintRulesAction,
    MadrAction, OkfAction, PropertiesAction, ReadArgs, TagsAction, TaskAction, TypesAction,
    ViewsAction,
};
use crate::commands::files_from::FilesFromCounters;
use crate::output::{Format, UserDiagnostic};
use anyhow::{Result, bail};
use clap::CommandFactory;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TargetCardinality {
    None,
    Single,
    Batch,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum IndexCapability {
    None,
    Read,
    CreateDestination,
    DropDestination,
}
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum EmptyInput {
    NotApplicable,
    CardinalityError,
    EmptyResult,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct Capabilities {
    targets: TargetCardinality,
    count: bool,
    github: bool,
    projection: bool,
    writes: bool,
    schema: bool,
    profile: bool,
    index: IndexCapability,
    empty_input: EmptyInput,
    output_modes: Vec<&'static str>,
}

impl Capabilities {
    pub(crate) fn of(command: &Commands) -> Self {
        use TargetCardinality::{Batch, None as NoTarget, Single};
        // Exhaustive on the actual parsed hierarchy, including default actions.
        let (targets, count, projection, schema, profile) = match command {
            Commands::Find(_) => (Batch, true, true, false, false),
            Commands::Read(_) => (Single, false, false, false, false),
            Commands::Backlinks { .. } => (Single, true, false, false, false),
            Commands::Summary(_) | Commands::Set { .. } | Commands::Append { .. } => {
                (Batch, false, false, true, false)
            }
            Commands::Remove { .. }
            | Commands::Mv { .. }
            | Commands::Madr {
                action: MadrAction::Toc { .. },
            } => (Batch, false, false, false, false),
            Commands::Lint { .. } => (Batch, true, false, true, true),
            Commands::New { .. } => (Single, false, false, true, false),
            Commands::Task { action } => match action {
                TaskAction::Read { .. } => (Single, false, false, false, false),
                TaskAction::Toggle { .. } | TaskAction::Set { .. } => {
                    (Batch, false, false, false, false)
                }
            },
            Commands::Properties { action, .. } => match action {
                None | Some(PropertiesAction::Summary { .. }) => (Batch, true, false, false, false),
                Some(PropertiesAction::Rename { .. }) => (Batch, false, false, false, false),
            },
            Commands::Tags { action, .. } => match action {
                None | Some(TagsAction::Summary { .. }) => (Batch, true, false, false, false),
                Some(TagsAction::Rename { .. }) => (Batch, false, false, false, false),
            },
            Commands::Views { action } => match action {
                None | Some(ViewsAction::List) => (NoTarget, true, false, false, false),
                Some(ViewsAction::Run { .. }) => (Batch, true, true, false, false),
                Some(ViewsAction::Set { .. } | ViewsAction::Remove { .. }) => {
                    (NoTarget, false, false, false, false)
                }
            },
            Commands::Types { action } => match action {
                None | Some(TypesAction::List) => (NoTarget, true, false, true, false),
                Some(TypesAction::Show { .. } | TypesAction::Remove { .. }) => {
                    (NoTarget, false, false, true, false)
                }
                Some(TypesAction::Set { .. }) => (Batch, false, false, true, false),
            },
            Commands::LintRules { action } => match action {
                None | Some(LintRulesAction::List { .. }) => (NoTarget, true, false, false, false),
                Some(
                    LintRulesAction::Show { .. }
                    | LintRulesAction::Set { .. }
                    | LintRulesAction::Remove { .. },
                ) => (NoTarget, false, false, false, false),
            },
            Commands::Links { action } => match action {
                None | Some(LinksAction::Fix { .. } | LinksAction::Auto { .. }) => {
                    (Batch, false, false, false, false)
                }
            },
            Commands::Okf { action } => match action {
                OkfAction::Index { .. } | OkfAction::Log { .. } => {
                    (Batch, false, false, false, false)
                }
            },
            Commands::Changelog { action } => match action {
                ChangelogAction::Release { .. } | ChangelogAction::Add { .. } => {
                    (Single, false, false, false, false)
                }
            },
            Commands::Init { .. } => (NoTarget, false, false, true, true),
            Commands::Deinit
            | Commands::Completion { .. }
            | Commands::Help { .. }
            | Commands::Config { .. }
            | Commands::CreateIndex { .. }
            | Commands::DropIndex { .. } => (NoTarget, false, false, false, false),
        };
        let index = match command {
            Commands::CreateIndex { .. } => IndexCapability::CreateDestination,
            Commands::DropIndex { .. } => IndexCapability::DropDestination,
            Commands::Find(_)
            | Commands::Read(_)
            | Commands::Summary(_)
            | Commands::Backlinks { .. }
            | Commands::Set { .. }
            | Commands::Append { .. }
            | Commands::Remove { .. }
            | Commands::Mv { .. }
            | Commands::Lint { .. }
            | Commands::New { .. }
            | Commands::Task { .. }
            | Commands::Properties { .. }
            | Commands::Tags { .. }
            | Commands::Links { .. } => IndexCapability::Read,
            Commands::Views { action } => {
                if matches!(action, Some(ViewsAction::Run { .. })) {
                    IndexCapability::Read
                } else {
                    IndexCapability::None
                }
            }
            Commands::Types { .. }
            | Commands::LintRules { .. }
            | Commands::Okf { .. }
            | Commands::Madr { .. }
            | Commands::Changelog { .. }
            | Commands::Init { .. }
            | Commands::Deinit
            | Commands::Completion { .. }
            | Commands::Help { .. }
            | Commands::Config { .. } => IndexCapability::None,
        };
        let empty_input = match targets {
            Single => EmptyInput::CardinalityError,
            Batch => EmptyInput::EmptyResult,
            NoTarget => EmptyInput::NotApplicable,
        };
        let output_modes = if matches!(command, Commands::Completion { .. }) {
            vec!["text"]
        } else if matches!(command, Commands::Lint { .. }) {
            vec!["text", "json", "github"]
        } else {
            vec!["text", "json"]
        };
        Self {
            targets,
            count,
            projection,
            schema,
            profile,
            index,
            empty_input,
            output_modes,
            github: matches!(command, Commands::Lint { .. }),
            writes: command.writes() || matches!(command, Commands::Init { .. } | Commands::Deinit),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HintDemand {
    None,
    Requested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Projection {
    Standard,
    Filenames,
    Filenames0,
}

pub(crate) struct OutputPreflight {
    jq: Option<String>,
    count: bool,
    internal_report: bool,
}
impl OutputPreflight {
    /// Policy tests start after jq compilation, whose worker protocol is covered by e2e tests.
    #[cfg(test)]
    pub(crate) fn validated_for_test(cli: &Cli) -> Self {
        Self {
            jq: cli.jq.clone(),
            count: cli.count,
            internal_report: cli.internal_mutation_report,
        }
    }

    pub(crate) fn new(cli: &Cli) -> Result<Self> {
        let capabilities = Capabilities::of(&cli.command);
        if cli.internal_mutation_report
            && (!matches!(
                cli.command,
                Commands::Set { .. }
                    | Commands::Append { .. }
                    | Commands::Task {
                        action: TaskAction::Toggle { .. } | TaskAction::Set { .. }
                    }
            ) || cli.format != Some(Format::Json)
                || !cli.no_hints
                || cli.hints
                || cli.jq.is_some()
                || cli.count)
        {
            bail!(crate::error::user_error(
                "internal mutation reports require a supported mutation with JSON output, --no-hints and no output transforms"
            ));
        }
        if matches!(cli.command, Commands::Completion { .. })
            && (cli.jq.is_some() || cli.format.is_some_and(|format| format != Format::Text))
        {
            bail!(crate::error::user_error(
                "completions supports only text output without jq"
            ));
        }
        if cli.count && !capabilities.count {
            bail!(crate::error::user_error(
                crate::list_commands::count_unsupported_error()
            ));
        }
        if cli.count && cli.jq.is_some() {
            bail!(crate::error::user_error(
                "--count cannot be combined with --jq"
            ));
        }
        if cli.jq.is_some() && cli.format.is_some_and(|format| format != Format::Json) {
            bail!(crate::error::user_error(
                "--jq cannot be combined with --format text or github"
            ));
        }
        if cli.format == Some(Format::Github) {
            if !capabilities.github {
                bail!(crate::error::user_error(
                    "--format github is only supported by `hyalo lint`"
                ));
            }
            if cli.count {
                bail!(crate::error::user_error(
                    "--format github cannot be combined with --count"
                ));
            }
        }
        if let Some(source) = &cli.jq {
            crate::output::preflight_jq(source).map_err(crate::error::user_error)?;
        }
        Ok(Self {
            jq: cli.jq.clone(),
            count: cli.count,
            internal_report: cli.internal_mutation_report,
        })
    }
}

pub(crate) struct OutputPlan {
    format: Format,
    error_format: Format,
    jq: Option<String>,
    count: bool,
    hints: HintDemand,
    projection: Projection,
    counters: Option<FilesFromCounters>,
    internal_report: bool,
}

impl OutputPlan {
    pub(crate) fn new(
        preflight: OutputPreflight,
        command: &Commands,
        format: Format,
        error_format: Format,
        explicit_format: bool,
        hints: bool,
    ) -> Result<Self> {
        let projection = match command {
            Commands::Find(FindArgs { filters, .. })
            | Commands::Views {
                action: Some(ViewsAction::Run { filters, .. }),
            } => {
                if filters.filenames0 {
                    Projection::Filenames0
                } else if filters.filenames_only {
                    Projection::Filenames
                } else {
                    Projection::Standard
                }
            }
            _ => Projection::Standard,
        };
        if preflight.jq.is_some() && format != Format::Json {
            bail!(crate::error::user_error("--jq requires JSON output"));
        }
        if projection != Projection::Standard
            && (preflight.jq.is_some()
                || preflight.count
                || explicit_format && format != Format::Text)
        {
            let flag = if projection == Projection::Filenames0 {
                "--filenames0"
            } else {
                "--filenames-only"
            };
            let incompatible = if explicit_format && format != Format::Text {
                if format == Format::Json {
                    "--format json"
                } else {
                    "--format github"
                }
            } else if preflight.jq.is_some() {
                "--jq"
            } else {
                "--count"
            };
            bail!(crate::error::user_error(format!(
                "{flag} cannot be combined with {incompatible}"
            )));
        }
        let demand = if hints && preflight.jq.is_none() {
            HintDemand::Requested
        } else {
            HintDemand::None
        };
        Ok(Self {
            format,
            error_format,
            jq: preflight.jq,
            count: preflight.count,
            hints: demand,
            projection,
            counters: None,
            internal_report: preflight.internal_report,
        })
    }
    pub(crate) fn internal_report(&self) -> bool {
        self.internal_report
    }
    pub(crate) fn format(&self) -> Format {
        self.format
    }
    pub(crate) fn error_format(&self) -> Format {
        self.error_format
    }
    pub(crate) fn jq(&self) -> Option<&str> {
        self.jq.as_deref()
    }
    pub(crate) fn count(&self) -> bool {
        self.count
    }
    pub(crate) fn hint_demand(&self) -> HintDemand {
        self.hints
    }
    pub(crate) fn projection(&self) -> Projection {
        self.projection
    }
    pub(crate) fn counters(&self) -> Option<&FilesFromCounters> {
        self.counters.as_ref()
    }
    pub(crate) fn set_counters(&mut self, counters: Option<FilesFromCounters>) {
        self.counters = counters;
    }
}

pub(crate) enum IndexIntent {
    None,
    Read { path: PathBuf, explicit: bool },
    Create { path: PathBuf },
    Drop { path: PathBuf },
}

pub(crate) fn normalize_index_alias(
    command: &mut Commands,
    global: Option<&Path>,
    dir: &Path,
) -> Result<Option<IndexIntent>> {
    let (local, create) = match command {
        Commands::CreateIndex { output, .. } => (output, true),
        Commands::DropIndex { path, .. } => (path, false),
        _ => return Ok(None),
    };
    if let Some(global) = global {
        if local.as_ref().is_some_and(|local| local != global) {
            bail!(crate::error::user_error(
                "conflicting index destination paths"
            ));
        }
        *local = Some(global.to_path_buf());
    }
    let path = match local.as_ref() {
        Some(path) if path.is_relative() => std::env::current_dir()?.join(path),
        Some(path) => path.clone(),
        None => dir.join(".hyalo-index"),
    };
    *local = Some(path.clone());
    Ok(Some(if create {
        IndexIntent::Create { path }
    } else {
        IndexIntent::Drop { path }
    }))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SelectionProvenance {
    Explicit,
    FilesFrom,
    Unrestricted,
}

pub(crate) struct SingleTargetRequest {
    file: (PathBuf, String),
}
impl SingleTargetRequest {
    pub(crate) fn checked(files: Vec<(PathBuf, String)>) -> Result<Self> {
        let mut files = files.into_iter();
        let file = files.next().ok_or_else(|| {
            crate::error::user_error(
                "this command requires exactly one resolved file; selection is empty",
            )
        })?;
        if files.next().is_some() {
            bail!(crate::error::user_error(
                "this command requires exactly one resolved file; multiple files selected"
            ));
        }
        Ok(Self { file })
    }
    pub(crate) fn into_file(self) -> (PathBuf, String) {
        self.file
    }
}
pub(crate) struct BatchTargetRequest {
    files: Vec<(PathBuf, String)>,
    provenance: SelectionProvenance,
}
impl BatchTargetRequest {
    pub(crate) fn new(files: Vec<(PathBuf, String)>, provenance: SelectionProvenance) -> Self {
        Self { files, provenance }
    }
    pub(crate) fn into_files(self) -> Vec<(PathBuf, String)> {
        self.files
    }
    pub(crate) fn provenance(&self) -> SelectionProvenance {
        self.provenance
    }
}
pub(crate) struct EffectiveQuery {
    selection: PreparedSelection,
    pattern: Option<String>,
    filters: FindFilters,
    provenance: SelectionProvenance,
    sections: Vec<hyalo_core::heading::SectionFilter>,
}
impl EffectiveQuery {
    pub(crate) fn from_find(
        args: FindArgs,
        config_language: Option<&str>,
        provenance: SelectionProvenance,
        dir: &Path,
    ) -> Result<Self> {
        let mut filters = args.filters;
        if !args.file_positional.is_empty() {
            if !filters.glob.is_empty() {
                crate::warn::warn(
                    "positional file arguments override the view's --glob; glob filter has been ignored",
                );
            }
            filters.glob.clear();
            filters.file = args.file_positional;
        }
        if filters.language.is_none() {
            filters.language = config_language.map(str::to_owned);
        }
        if let Some(language) = &filters.language {
            hyalo_core::bm25::parse_language(language).map_err(|error| {
                crate::error::user_error(format!("invalid language {language:?}: {error}"))
            })?;
        }
        let sections = filters
            .sections
            .iter()
            .map(|section| {
                hyalo_core::heading::SectionFilter::parse(section).map_err(crate::error::user_error)
            })
            .collect::<Result<Vec<_>>>()?;
        let selection = if matches!(provenance, SelectionProvenance::FilesFrom) {
            PreparedSelection::resolved(&filters.file)?
        } else {
            PreparedSelection::explicit(dir, &filters.file)?
        };
        filters.file.clear();
        Ok(Self {
            selection,
            pattern: args.pattern,
            filters,
            provenance,
            sections,
        })
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        FindFilters,
        SelectionProvenance,
        Vec<hyalo_core::heading::SectionFilter>,
        PreparedSelection,
    ) {
        (
            self.pattern,
            self.filters,
            self.provenance,
            self.sections,
            self.selection,
        )
    }
}

pub(crate) enum PreparedCommand {
    Read {
        target: SingleTargetRequest,
        section: Option<String>,
        lines: Option<String>,
        frontmatter: bool,
    },
    Backlinks {
        target: SingleTargetRequest,
        limit: Option<usize>,
    },
    TaskRead {
        target: SingleTargetRequest,
        lines: Vec<usize>,
        section: Option<String>,
        all: bool,
    },
    TaskMutation {
        targets: BatchTargetRequest,
        lines: Vec<usize>,
        section: Option<String>,
        all: bool,
        status: Option<char>,
        dry_run: bool,
    },
    Find(EffectiveQuery),
    Legacy(Commands),
}

pub(crate) struct PreparedInvocation {
    command: PreparedCommand,
    output: OutputPlan,
    index: IndexIntent,
}
impl PreparedInvocation {
    pub(crate) fn new(
        command: Commands,
        mut output: OutputPlan,
        index: IndexIntent,
        ctx: &crate::dispatch::CommandContext<'_>,
    ) -> Result<Self> {
        use crate::commands::inputs::{ResolutionPolicy, ResolvedInputsOrOutcome, resolve_inputs};
        let mut resolve =
            |selection: &crate::cli::inputs::InputSelection, single: bool, ci: bool| -> Result<_> {
                let policy = if single {
                    ResolutionPolicy::Single { allow_glob: false }
                } else {
                    ResolutionPolicy::SingleOrMany
                };
                match resolve_inputs(
                    selection,
                    ctx.dir,
                    ctx.configured_dir_str,
                    ctx.snapshot_index.as_ref(),
                    &policy,
                    ctx.effective_format,
                    ci,
                )? {
                    ResolvedInputsOrOutcome::Resolved(inputs) => {
                        output.set_counters(inputs.counters);
                        Ok(inputs.files)
                    }
                    ResolvedInputsOrOutcome::Outcome(crate::output::CommandOutcome::UserError(
                        diagnostic,
                    )) => Err(anyhow::Error::new(diagnostic)),
                    ResolvedInputsOrOutcome::Outcome(_) => {
                        bail!("input resolver produced an invalid outcome")
                    }
                }
            };
        let command = match command {
            Commands::Read(ReadArgs {
                selection,
                section,
                lines,
                frontmatter,
                ..
            }) => PreparedCommand::Read {
                target: SingleTargetRequest::checked(resolve(&selection, true, false)?)?,
                section,
                lines,
                frontmatter,
            },
            Commands::Backlinks {
                selection, limit, ..
            } => PreparedCommand::Backlinks {
                target: SingleTargetRequest::checked(resolve(
                    &selection,
                    true,
                    hyalo_core::mode_enabled(ctx.case_insensitive_mode, ctx.dir),
                )?)?,
                limit,
            },
            Commands::Task {
                action:
                    TaskAction::Read {
                        selection,
                        line,
                        section,
                        all,
                        ..
                    },
            } => PreparedCommand::TaskRead {
                target: SingleTargetRequest::checked(resolve(&selection, true, false)?)?,
                lines: line,
                section,
                all,
            },
            Commands::Task { action } => {
                let (selection, lines, section, all, status, dry_run) = match action {
                    TaskAction::Toggle {
                        selection,
                        line,
                        section,
                        all,
                        dry_run,
                        ..
                    } => (selection, line, section, all, None, dry_run),
                    TaskAction::Set {
                        selection,
                        line,
                        section,
                        all,
                        status,
                        dry_run,
                        ..
                    } => {
                        let mut chars = status.chars();
                        let ch = chars.next();
                        if ch.is_none() || chars.next().is_some() {
                            bail!(crate::error::user_error(
                                "--status must be a single character"
                            ));
                        }
                        (selection, line, section, all, ch, dry_run)
                    }
                    TaskAction::Read { .. } => unreachable!("read handled above"),
                };
                if selection.files_from.is_some() && !all && section.is_none() {
                    bail!(crate::error::user_error(
                        "--files-from requires --all or --section"
                    ));
                }
                let provenance = if selection.files_from.is_some() {
                    SelectionProvenance::FilesFrom
                } else {
                    SelectionProvenance::Explicit
                };
                PreparedCommand::TaskMutation {
                    targets: BatchTargetRequest::new(
                        resolve(&selection, false, false)?,
                        provenance,
                    ),
                    lines,
                    section,
                    all,
                    status,
                    dry_run,
                }
            }
            Commands::Find(args) => {
                let provenance = if ctx.file_list_from_files_from {
                    SelectionProvenance::FilesFrom
                } else if args.filters.file.is_empty()
                    && args.file_positional.is_empty()
                    && args.filters.glob.is_empty()
                {
                    SelectionProvenance::Unrestricted
                } else {
                    SelectionProvenance::Explicit
                };
                PreparedCommand::Find(EffectiveQuery::from_find(
                    args,
                    ctx.config_language,
                    provenance,
                    ctx.dir,
                )?)
            }
            other => PreparedCommand::Legacy(other),
        };
        Ok(Self {
            command,
            output,
            index,
        })
    }
    /// Refresh only after the invocation owns checked cardinality/effective inputs.
    pub(crate) fn refresh_index(
        &self,
        ctx: &mut crate::dispatch::CommandContext<'_>,
    ) -> Result<()> {
        let read_fallback = match &self.command {
            PreparedCommand::Legacy(command) => !command.writes(),
            PreparedCommand::TaskMutation { .. } => false,
            _ => true,
        };
        let Some(index) = ctx.snapshot_index.as_mut() else {
            return Ok(());
        };
        let (selection, repairs, insert) = match &self.command {
            PreparedCommand::Read { target, .. }
            | PreparedCommand::Backlinks { target, .. }
            | PreparedCommand::TaskRead { target, .. } => (
                PreparedSelection::resolved(std::slice::from_ref(&target.file.1))?,
                false,
                false,
            ),
            PreparedCommand::TaskMutation { .. } => {
                (PreparedSelection::resolved(&[])?, true, false)
            }
            PreparedCommand::Find(query) => (
                query.selection.clone(),
                false,
                !matches!(query.provenance, SelectionProvenance::FilesFrom),
            ),
            PreparedCommand::Legacy(command) => {
                let raw = command.explicit_file_targets();
                let selection = if ctx.file_list_from_files_from {
                    PreparedSelection::resolved(&raw)?
                } else {
                    PreparedSelection::explicit(ctx.dir, &raw)?
                };
                (selection, command.write_repairs_named_targets(), false)
            }
        };
        let root = hyalo_core::rooted::VaultRoot::new(ctx.dir)?;
        let refreshed = selection::refresh_named_selection(&root, &selection, index, insert)?;
        if !refreshed.all_current(&selection) && !repairs {
            warn_stale_index(index, ctx.dir);
            if read_fallback && (refreshed.failed > 0 || refreshed.missing > 0) {
                // A failed named scan cannot leave old fields/postings available.
                // Reads use the bounded disk path and its parse/skip diagnostics.
                *ctx.snapshot_index = None;
            }
        }
        Ok(())
    }

    pub(crate) fn into_parts(self) -> (PreparedCommand, OutputPlan, IndexIntent) {
        (self.command, self.output, self.index)
    }
}

impl IndexIntent {
    pub(crate) fn read_target(&self) -> Option<(&Path, bool)> {
        match self {
            Self::Read { path, explicit } => Some((path, *explicit)),
            _ => None,
        }
    }
    pub(crate) fn destination(&self) -> Option<&Path> {
        match self {
            Self::Create { path } | Self::Drop { path } => Some(path),
            _ => None,
        }
    }
}

/// Test/adapter producer derived from the actual Clap tree and the exhaustive
/// runtime metadata. It describes one parsed invocation, including aliases and
/// canonical option spellings; no parallel public schema or command registry.
pub fn describe_invocation(args: &[String]) -> Result<serde_json::Value> {
    use clap::FromArgMatches;
    #[derive(serde::Serialize)]
    struct OptionDescriptor<'a> {
        long: Option<&'a str>,
        short: Option<char>,
        aliases: Vec<&'a str>,
        takes_value: bool,
        value_names: Vec<String>,
        equals_form: bool,
    }
    #[derive(serde::Serialize)]
    struct Descriptor<'a> {
        command: Vec<&'a str>,
        capabilities: Capabilities,
        options: Vec<OptionDescriptor<'a>>,
    }
    let mut root = Cli::command();
    root.build();
    let matches = root.clone().try_get_matches_from(args)?;
    let cli = Cli::from_arg_matches(&matches)?;
    let mut node = &root;
    let mut current = &matches;
    let mut path = Vec::new();
    while let Some((name, child)) = current.subcommand() {
        node = node
            .find_subcommand(name)
            .ok_or_else(|| anyhow::anyhow!("Clap hierarchy changed"))?;
        path.push(node.get_name());
        current = child;
    }
    let options = node
        .get_arguments()
        .filter(|arg| !arg.is_hide_set())
        .map(|arg| OptionDescriptor {
            long: arg.get_long(),
            short: arg.get_short(),
            aliases: arg.get_all_aliases().unwrap_or_default(),
            takes_value: arg.get_action().takes_values(),
            value_names: arg
                .get_value_names()
                .unwrap_or_default()
                .iter()
                .map(ToString::to_string)
                .collect(),
            equals_form: arg.get_long().is_some() && arg.get_action().takes_values(),
        })
        .collect();
    Ok(serde_json::to_value(Descriptor {
        command: path,
        capabilities: Capabilities::of(&cli.command),
        options,
    })?)
}

pub(crate) fn cardinality_diagnostic(
    message: &str,
    counters: Option<&FilesFromCounters>,
) -> UserDiagnostic {
    let mut diagnostic = UserDiagnostic::new(message);
    if let Some(counters) = counters {
        for (key, value) in [
            ("files_missing", counters.files_missing),
            ("files_skipped_non_md", counters.files_skipped_non_md),
            (
                "files_skipped_outside_vault",
                counters.files_skipped_outside_vault,
            ),
        ] {
            diagnostic.details.insert(key.into(), value.into());
        }
    }
    diagnostic
}

fn warn_stale_index(idx: &hyalo_core::index::SnapshotIndex, dir: &Path) {
    let (_, _, created_at, _) = idx.header_info();
    let dirs_moved = hyalo_core::index::newest_dir_mtime(dir).is_some_and(|newest| {
        newest > created_at.saturating_add(hyalo_core::index::STALENESS_TOLERANCE_SECS)
    });
    if dirs_moved {
        // UX-8 (iter-277): name the probe that fired. Two
        // different checks produce this warning and only
        // one of them names a witness file, so the same
        // vault appeared to report a filename on one run
        // and nothing on the next, with no way to tell
        // that a different check had spoken.
        crate::warn::warn(
            "index older than vault (a directory's mtime moved since the \
                                 index was built); results may be stale — re-run create-index",
        );
    } else if let Some(rel) =
        // INDEX-1 (iter-273, BUG-12): the directory probe
        // above sees notes added and removed, but an
        // in-place overwrite moves no directory mtime — so
        // a rewritten note was served from the snapshot
        // silently. Fall through to the per-entry mtime
        // comparison only when the cheap probe found
        // nothing, so the extra `stat`s are paid once, on
        // the vault that looked clean.
        hyalo_core::index::first_file_modified_since_snapshot(idx, dir)
    {
        crate::warn::warn(format!(
            "index older than vault (file {rel} changed on disk since the \
                                 index was built); results may be stale — re-run create-index"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(input: &[&str]) -> serde_json::Value {
        let mut argv = vec!["hyalo".to_owned()];
        argv.extend(input.iter().map(|arg| (*arg).to_owned()));
        describe_invocation(&argv).unwrap()
    }
    #[test]
    fn descriptor_uses_canonical_clap_aliases_and_capabilities() {
        for (args, canonical, count) in [
            (vec!["show", "a.md"], "read", false),
            (vec!["completion", "bash"], "completions", false),
            (vec!["tags", "list"], "summary", true),
            (vec!["views", "run", "saved"], "run", true),
            (
                vec!["task", "set-status", "a.md", "--all", "--status", "x"],
                "set",
                false,
            ),
            (vec!["lint", "--fix"], "lint", true),
        ] {
            let descriptor = descriptor(&args);
            assert_eq!(
                descriptor["command"].as_array().unwrap().last().unwrap(),
                canonical
            );
            assert_eq!(descriptor["capabilities"]["count"], count);
        }
        let read = descriptor(&["read", "a.md"]);
        assert_eq!(read["capabilities"]["targets"], "single");
        assert!(
            read["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option["long"] == "file" && option["equals_form"] == true)
        );
    }
}
