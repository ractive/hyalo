//! The marker that separates "the caller got it wrong" from "hyalo broke".
//!
//! iter-274 (BUG-25 / UX-1, DEC-307). hyalo's exit-code taxonomy is
//! **0** ok, **1** every hyalo-own user error, **2** clap usage errors and
//! internal errors. Several genuine user errors — an unparseable `--glob`, an
//! unreadable `--files-from` list, a `create-index --output` into a directory
//! that does not exist, an unknown `init --profile` — travelled to the top of
//! the process as plain `anyhow` values and were reported as internal: bare
//! text on stderr and exit 2 even under `--format json`. A scripted caller
//! could neither parse the message nor tell the mistake apart from a crash.
//!
//! Wrapping the message in [`UserFacingError`] fixes both: the marker survives
//! `?` and added `.context(...)` through every intermediate layer, and the CLI's
//! top-level handler re-renders it through the standard error envelope at the
//! effective `--format` and exits 1.
//!
//! It lives in `hyalo-core` rather than the CLI because the layers that detect
//! these mistakes — glob compilation, path resolution — are here.

/// An error caused by the caller's input rather than by a hyalo failure.
///
/// Construct with [`user_error`] or [`user_error_with`]; recognise it with
/// `anyhow::Error::downcast_ref::<UserFacingError>()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFacingError {
    /// The `error` field of the rendered envelope.
    pub message: String,
    /// The `hint` field: what to do instead.
    pub hint: Option<String>,
    /// The `cause` field: the underlying diagnostic, when there is one.
    pub cause: Option<String>,
}

impl std::fmt::Display for UserFacingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserFacingError {}

/// A user-facing `anyhow::Error` carrying only a message.
#[must_use]
pub fn user_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(UserFacingError {
        message: message.into(),
        hint: None,
        cause: None,
    })
}

/// A user-facing `anyhow::Error` carrying the envelope's `hint` and `cause`.
#[must_use]
pub fn user_error_with(
    message: impl Into<String>,
    hint: Option<String>,
    cause: Option<String>,
) -> anyhow::Error {
    anyhow::Error::new(UserFacingError {
        message: message.into(),
        hint,
        cause,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_survives_added_context() {
        let err = user_error("invalid glob pattern").context_note();
        assert_eq!(
            err.downcast_ref::<UserFacingError>()
                .map(|u| u.message.as_str()),
            Some("invalid glob pattern")
        );
    }

    /// Small helper so the test above reads as the real call sites do.
    trait ContextNote {
        fn context_note(self) -> anyhow::Error;
    }
    impl ContextNote for anyhow::Error {
        fn context_note(self) -> anyhow::Error {
            use anyhow::Context as _;
            Err::<(), _>(self)
                .context("while scanning the vault")
                .unwrap_err()
        }
    }

    #[test]
    fn display_is_the_message_alone() {
        let err = user_error_with("nope", Some("try x".to_owned()), Some("io".to_owned()));
        assert_eq!(err.to_string(), "nope");
        let u = err.downcast_ref::<UserFacingError>().expect("marker");
        assert_eq!(u.hint.as_deref(), Some("try x"));
        assert_eq!(u.cause.as_deref(), Some("io"));
    }
}
