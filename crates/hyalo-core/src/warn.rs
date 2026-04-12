//! Lightweight warning helper for `hyalo-core`.
//!
//! Core-level code should call [`warn`] instead of `eprintln!` so that
//! the message is formatted consistently. The CLI layer (`hyalo-cli`)
//! provides its own richer warning system with quiet-mode suppression and
//! dedup tracking; this module is intentionally minimal — it just writes to
//! stderr with a standard `warning:` prefix.

/// Emit a warning message to stderr.
///
/// Formats the message with a `warning: ` prefix, matching the convention used
/// by the CLI layer.
pub fn warn(msg: impl AsRef<str>) {
    eprintln!("warning: {}", msg.as_ref());
}
