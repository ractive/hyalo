/// Top-level application error.
///
/// Each variant maps to a specific exit code so that `run` can convert the
/// error into the correct process exit without `process::exit` being called
/// from deep inside the call stack.
pub(crate) enum AppError {
    /// User-facing error (invalid arguments, file not found, etc.) — exit 1.
    User(String),
    /// Internal / system error (I/O failure, parse error, etc.) — exit 2.
    Internal(anyhow::Error),
    /// Clap parse or help/version error — exit with clap's own code.
    Clap(clap::Error),
    /// Error already printed by the output pipeline — just set exit code.
    Exit(i32),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}
