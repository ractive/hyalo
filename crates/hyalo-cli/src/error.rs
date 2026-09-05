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
/// iter-274 (BUG-25 / UX-1). hyalo's exit-code taxonomy is: 0 ok, 1 every
/// hyalo-own user error, 2 clap usage errors and internal errors. Several user
/// errors reached the top of `run` as plain `anyhow` values, so they were
/// reported as internal — bare text on stderr and exit 2 even under
/// `--format json`, which a scripted caller cannot parse and cannot
/// distinguish from a crash. Those sites now wrap their message in this marker:
/// it survives `?` through the intermediate layers, and the handler in `run`
/// re-renders it through [`crate::output::format_error`] at the effective
/// format and exits 1.
#[derive(Debug)]
pub(crate) struct UserFacing {
    pub message: String,
    pub hint: Option<String>,
    pub cause: Option<String>,
}

impl std::fmt::Display for UserFacing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserFacing {}

/// Build a user-facing `anyhow::Error` that exits 1 and renders as an envelope
/// under `--format json`.
pub(crate) fn user_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(UserFacing {
        message: message.into(),
        hint: None,
        cause: None,
    })
}

/// Like [`user_error`] but carries the `hint` and `cause` fields the JSON
/// envelope reports alongside `error`.
pub(crate) fn user_error_with(
    message: impl Into<String>,
    hint: Option<String>,
    cause: Option<String>,
) -> anyhow::Error {
    anyhow::Error::new(UserFacing {
        message: message.into(),
        hint,
        cause,
    })
}

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
    fn user_facing_survives_anyhow_context() {
        let err: anyhow::Error = user_error("bad glob");
        let wrapped = err.context("while scanning");
        let found = wrapped.downcast_ref::<UserFacing>();
        assert!(found.is_some(), "the marker must survive added context");
        assert_eq!(found.map(|u| u.message.as_str()), Some("bad glob"));
    }

    #[test]
    fn user_error_with_carries_hint_and_cause() {
        let err = user_error_with("nope", Some("try x".to_owned()), Some("io".to_owned()));
        let u = err.downcast_ref::<UserFacing>().expect("marker");
        assert_eq!(u.hint.as_deref(), Some("try x"));
        assert_eq!(u.cause.as_deref(), Some("io"));
    }

    #[test]
    fn error_format_round_trips() {
        set_error_format(Format::Json);
        assert_eq!(error_format(), Format::Json);
        set_error_format(Format::Text);
        assert_eq!(error_format(), Format::Text);
    }
}
