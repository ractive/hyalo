use clap::{Parser, Subcommand};

mod artifact;
mod behavioral_contracts;
mod bench_scale;
mod bundled_skills;
mod codex_package;
mod command_reference;
mod feature_fanout;
mod help_drift;
mod jq_recipes;
mod mutation_journal;
mod npm_package;
mod pi_package_sync;
mod pi_runtime;
mod stubs;
mod ts_types;
mod typed_output;
mod workspace;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "CI quality-gate tasks for the hyalo workspace"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::enum_variant_names)]
enum Commands {
    /// Gate 2: verify cross-command flag consistency per feature-matrix.toml.
    CheckFeatureFanout,
    /// Gate 3: verify help text has EXAMPLES blocks and no stale wording.
    CheckHelpDrift,
    /// Gate: verify every clap subcommand appears in the COMMAND REFERENCE block.
    CheckCommandReference,
    /// Gate: verify every bundled skill template passes the skills profile.
    CheckBundledSkills,
    /// Gate: verify the vendored `crates/hyalo-cli/templates/pi/` copies
    /// match the canonical `pi-package/` files byte-for-byte.
    CheckPiPackageSync,
    /// Rebuild the self-contained Pi API runtime in isolation and reject drift.
    CheckPiRuntime,
    /// Generate or verify npm launcher/platform package metadata.
    GenerateNpmPackages(npm_package::NpmArgs),
    /// Plan an immutable npm publication from npm pack integrity metadata.
    PlanNpmPublication(npm_package::NpmPublishPlanArgs),
    /// Reject ad-hoc JSON macros in production command output.
    CheckTypedOutput,
    /// Regenerate Rust-derived TypeScript declarations in a temporary directory and reject drift.
    CheckTsTypes,
    /// Refresh committed Rust-derived TypeScript declarations and generated barrels.
    GenerateTsTypes,
    /// Verify Codex plugin assets, metadata, and embedded-copy parity.
    CheckCodexPackage,
    /// Refresh the embedded Codex skills from plugins/hyalo/skills.
    SyncCodexPackage,
    /// Gate (iter-274, BUG-29): every `--jq` recipe in a shipped document
    /// executes against this repo's own knowledgebase without a jq error.
    CheckJqRecipes(artifact::ArtifactArgs),
    /// Unsupported legacy placeholder. Exits non-zero and is not a quality gate.
    CheckDeadPrimitives(stubs::LegacyGateArgs),
    /// Unsupported legacy placeholder. Exits non-zero and is not a quality gate.
    CheckTodoAnnotations(stubs::LegacyGateArgs),
    /// Run the focused cross-iteration behavioral safety contract suite.
    CheckBehavioralContracts,
    /// Gate (ARCH-3, iter-226): every mutating command records index
    /// maintenance through MutationJournal; no direct index persistence.
    CheckMutationJournal,
    /// Scale regression gate for a generated ~14k-file synthetic vault.
    /// CI runs it with an explicit native target and isolated Cargo target
    /// directory on Linux, macOS and Windows; it is also available on demand.
    BenchScale(artifact::ArtifactArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::CheckFeatureFanout => feature_fanout::run(),
        Commands::CheckHelpDrift => help_drift::run(),
        Commands::CheckCommandReference => command_reference::run(),
        Commands::CheckBundledSkills => bundled_skills::run(),
        Commands::CheckPiPackageSync => pi_package_sync::run(),
        Commands::CheckPiRuntime => pi_runtime::run(),
        Commands::GenerateNpmPackages(args) => npm_package::run(args),
        Commands::PlanNpmPublication(args) => npm_package::run_publication_plan(args),
        Commands::CheckTypedOutput => typed_output::run(),
        Commands::CheckTsTypes => ts_types::check(),
        Commands::GenerateTsTypes => ts_types::generate(),
        Commands::CheckCodexPackage => codex_package::run(false),
        Commands::SyncCodexPackage => codex_package::run(true),
        Commands::CheckJqRecipes(args) => jq_recipes::run(&args),
        Commands::CheckDeadPrimitives(_) => stubs::check_dead_primitives(),
        Commands::CheckTodoAnnotations(_) => stubs::check_todo_annotations(),
        Commands::CheckBehavioralContracts => behavioral_contracts::run(),
        Commands::CheckMutationJournal => mutation_journal::run(),
        Commands::BenchScale(args) => bench_scale::run(&args),
    };
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(2);
        }
    }
}
