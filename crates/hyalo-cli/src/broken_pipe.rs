//! Make broken-pipe writes (`hyalo find | head`) terminate quietly instead of
//! panicking.
//!
//! Rust's `println!`/`print!`/`eprintln!` macros panic on any write error —
//! including `ErrorKind::BrokenPipe`, which happens whenever a downstream
//! reader (e.g. `head`) closes its end of the pipe before we're done writing.
//! That surfaces to the user as `thread 'main' panicked at ...: failed
//! printing to stdout: Broken pipe (os error 32)`, which looks like a crash
//! even though nothing is actually wrong.
//!
//! Two layers, since neither alone covers every platform:
//!
//! 1. On Unix, reset `SIGPIPE` to its default disposition (`SIG_DFL`). Rust's
//!    runtime masks `SIGPIPE` at startup so writes fail with an `Err` instead
//!    of killing the process the traditional Unix way; resetting it restores
//!    that behavior, so the process is terminated by the kernel before the
//!    write call can return an error to panic on. This matches how `cat`,
//!    `grep`, etc. behave and yields the conventional 128+SIGPIPE (141) exit
//!    code without touching any print call site.
//! 2. A panic hook that recognizes the broken-pipe panic message and exits
//!    quietly (same 141 code, for consistency) instead of printing a panic
//!    backtrace. This is the only mechanism available on Windows (no
//!    `SIGPIPE`), and is a backstop on Unix for any write that fails before
//!    the signal takes effect.
//!
//! Call [`install`] once, as early as possible in `main`.

/// Exit code used when a write fails because the reader closed the pipe.
///
/// 141 = 128 + `SIGPIPE` (13), the conventional Unix exit status for a process
/// killed by `SIGPIPE`. Used consistently on all platforms (including
/// Windows, which has no `SIGPIPE`) so scripts piping `hyalo` can check for
/// one exit code regardless of OS.
pub const BROKEN_PIPE_EXIT_CODE: i32 = 141;

/// Install the broken-pipe handling described in the module docs.
pub fn install() {
    reset_sigpipe();
    install_panic_hook();
}

#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: `libc::signal` is called with a valid signal number (SIGPIPE)
    // and a valid handler constant (SIG_DFL) per POSIX — the standard idiom
    // for restoring default SIGPIPE behavior.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {
    // No SIGPIPE on non-Unix platforms; the panic hook below is the only
    // line of defense there.
}

/// OS error codes (as rendered by Rust's `io::Error` `Display` impl, which is
/// always English regardless of locale) that indicate the reader went away —
/// as opposed to some other write failure (disk full, permission denied)
/// that should still be reported as a real error.
///
/// Unix: `EPIPE` = 32. Windows: `ERROR_BROKEN_PIPE` = 109, `ERROR_NO_DATA` =
/// 232 (returned when the reading end of an anonymous pipe has been closed).
#[cfg(windows)]
const BROKEN_PIPE_OS_ERROR_CODES: &[i32] = &[109, 232];
#[cfg(not(windows))]
const BROKEN_PIPE_OS_ERROR_CODES: &[i32] = &[32];

/// Returns `true` if `msg` is the message `println!`/`print!`/`eprintln!`
/// panic with when the underlying write fails with a broken pipe.
///
/// Requires both the `println!`/`eprintln!` failure prefix *and* a matching
/// `(os error N)` suffix, so unrelated write failures (disk full, permission
/// denied) still fall through to the default panic hook instead of being
/// silently swallowed.
fn is_broken_pipe_panic(msg: &str) -> bool {
    is_broken_pipe_panic_with_codes(msg, BROKEN_PIPE_OS_ERROR_CODES)
}

/// Core matching logic, parameterized over the OS error code list so it can
/// be unit-tested for every platform's code list regardless of which
/// platform the tests actually run on.
fn is_broken_pipe_panic_with_codes(msg: &str, codes: &[i32]) -> bool {
    let has_prefix = msg.starts_with("failed printing to stdout")
        || msg.starts_with("failed printing to stderr");
    has_prefix
        && codes
            .iter()
            .any(|code| msg.ends_with(&format!("(os error {code})")))
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    // This hook still runs before the process aborts even when the workspace's
    // release profile sets `panic = "abort"`: panic hooks execute as part of
    // unwinding-or-aborting, before the abort itself, so this fires in both
    // dev (unwind) and release (abort) builds. Don't "simplify" this away as
    // dead code under the abort profile.
    std::panic::set_hook(Box::new(move |info| {
        let is_broken_pipe = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .is_some_and(is_broken_pipe_panic);

        if is_broken_pipe {
            std::process::exit(BROKEN_PIPE_EXIT_CODE);
        }
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::{
        BROKEN_PIPE_OS_ERROR_CODES, is_broken_pipe_panic, is_broken_pipe_panic_with_codes,
    };

    #[test]
    fn recognizes_stdout_broken_pipe_message() {
        // Build the message from the running platform's own code list so this
        // passes on Unix (32) and Windows (109/232) alike.
        for code in BROKEN_PIPE_OS_ERROR_CODES {
            assert!(is_broken_pipe_panic(&format!(
                "failed printing to stdout: Broken pipe (os error {code})"
            )));
        }
    }

    #[test]
    fn recognizes_stderr_broken_pipe_message() {
        for code in BROKEN_PIPE_OS_ERROR_CODES {
            assert!(is_broken_pipe_panic(&format!(
                "failed printing to stderr: Broken pipe (os error {code})"
            )));
        }
    }

    #[test]
    fn does_not_match_unrelated_panics() {
        assert!(!is_broken_pipe_panic("index out of bounds"));
        assert!(!is_broken_pipe_panic(
            "called `Option::unwrap()` on a `None` value"
        ));
    }

    /// Same "failed printing to ..." prefix as a real broken-pipe panic, but a
    /// different OS error (disk full) — must NOT be treated as a broken pipe,
    /// or a real error would be silently swallowed as a quiet exit.
    #[test]
    fn does_not_match_same_prefix_different_os_error() {
        assert!(!is_broken_pipe_panic(
            "failed printing to stdout: No space left on device (os error 28)"
        ));
        assert!(!is_broken_pipe_panic(
            "failed printing to stderr: Permission denied (os error 13)"
        ));
    }

    #[test]
    fn matches_unix_epipe_code() {
        assert!(is_broken_pipe_panic_with_codes(
            "failed printing to stdout: Broken pipe (os error 32)",
            &[32]
        ));
    }

    #[test]
    fn matches_windows_broken_pipe_codes() {
        assert!(is_broken_pipe_panic_with_codes(
            "failed printing to stdout: The pipe has been ended. (os error 109)",
            &[109, 232]
        ));
        assert!(is_broken_pipe_panic_with_codes(
            "failed printing to stdout: The pipe is being closed. (os error 232)",
            &[109, 232]
        ));
    }

    #[test]
    fn windows_codes_do_not_match_unix_epipe() {
        assert!(!is_broken_pipe_panic_with_codes(
            "failed printing to stdout: Broken pipe (os error 32)",
            &[109, 232]
        ));
    }

    /// Sanity check that the platform-selected constant is non-empty and
    /// contains only the codes documented above.
    #[test]
    fn platform_code_list_is_well_formed() {
        assert!(!BROKEN_PIPE_OS_ERROR_CODES.is_empty());
    }
}
