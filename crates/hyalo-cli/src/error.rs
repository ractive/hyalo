use std::sync::atomic::{AtomicU8, Ordering};

use crate::output::Format;

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

/// Marker attached to an `anyhow::Error` that is the *caller's* fault.
///
/// iter-274 (BUG-25 / UX-1, DEC-307). Defined in `hyalo-core` because the
/// layers that detect these mistakes (glob compilation, path resolution) live
/// there; re-exported here so `run.rs` reads naturally.
pub(crate) use hyalo_core::{UserFacingError as UserFacing, user_error, user_error_with};

/// The format errors are rendered in, published for the top-level handler.
///
/// `run_inner` resolves the effective error format long after `run` has taken
/// its `match`, and threading it back through every `?` would mean changing
/// every fallible signature in the crate. One atomic, written once per process
/// exactly like [`crate::warn::init`]'s quiet flag, keeps the envelope
/// consistent with the rest of the run.
static ERROR_FORMAT: AtomicU8 = AtomicU8::new(FORMAT_TEXT);

const FORMAT_TEXT: u8 = 0;
const FORMAT_JSON: u8 = 1;
const FORMAT_GITHUB: u8 = 2;

/// Record the effective error format for the top-level handler.
pub(crate) fn set_error_format(format: Format) {
    let code = match format {
        Format::Text => FORMAT_TEXT,
        Format::Json => FORMAT_JSON,
        Format::Github => FORMAT_GITHUB,
    };
    ERROR_FORMAT.store(code, Ordering::Relaxed);
}

/// The format recorded by [`set_error_format`]; text until one is set.
pub(crate) fn error_format() -> Format {
    match ERROR_FORMAT.load(Ordering::Relaxed) {
        FORMAT_JSON => Format::Json,
        FORMAT_GITHUB => Format::Github,
        _ => Format::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_format_round_trips() {
        set_error_format(Format::Json);
        assert_eq!(error_format(), Format::Json);
        set_error_format(Format::Text);
        assert_eq!(error_format(), Format::Text);
    }
}
