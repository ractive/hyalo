use clap::{Parser, Subcommand};

mod ac_fidelity;
mod bundled_skills;
mod feature_fanout;
mod help_drift;
mod stubs;

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
    /// Gate 1: verify every ticked AC in an iteration plan has test evidence or a deferral.
    CheckAcFidelity(ac_fidelity::AcFidelityArgs),
    /// Gate 2: verify cross-command flag consistency per feature-matrix.toml.
    CheckFeatureFanout,
    /// Gate 3: verify help text has EXAMPLES blocks and no stale wording.
    CheckHelpDrift,
    /// Gate: verify every bundled skill template passes the skills profile.
    CheckBundledSkills,
    /// Stub — not yet implemented (iter-142b).
    CheckDeadPrimitives(stubs::StubArgs),
    /// Stub — not yet implemented (iter-142b).
    CheckTodoAnnotations(stubs::StubArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::CheckAcFidelity(args) => ac_fidelity::run(args),
        Commands::CheckFeatureFanout => feature_fanout::run(),
        Commands::CheckHelpDrift => help_drift::run(),
        Commands::CheckBundledSkills => bundled_skills::run(),
        Commands::CheckDeadPrimitives(_) => stubs::check_dead_primitives(),
        Commands::CheckTodoAnnotations(_) => stubs::check_todo_annotations(),
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
