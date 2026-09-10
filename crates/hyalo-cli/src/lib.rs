mod broken_pipe;
mod cli;
pub mod commands;
pub mod config;
pub mod diagnostic;
mod dispatch;
mod error;
pub mod hints;
mod list_commands;
mod mutation;
pub mod output;
mod output_pipeline;
mod prepared;
pub use prepared::describe_invocation;
mod run;
pub mod suggest;
pub mod warn;

pub use run::run;
