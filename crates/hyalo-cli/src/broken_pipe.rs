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
    // SAFETY: `signal` is called with a valid signal number (SIGPIPE) and a
    // valid handler constant (SIG_DFL) per POSIX; this is the standard
    // idiom for restoring default SIGPIPE behavior and has no preconditions
    // beyond being called before other threads rely on SIGPIPE being masked
    // (true here — this runs first thing in `main`).
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {
    // No SIGPIPE on non-Unix platforms; the panic hook below is the only
    // line of defense there.
}

#[cfg(unix)]
const SIGPIPE: i32 = 13;
#[cfg(unix)]
const SIG_DFL: usize = 0;

#[cfg(unix)]
unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

/// Returns `true` if `msg` is the message `println!`/`print!`/`eprintln!`
/// panic with when the underlying write fails with a broken pipe.
fn is_broken_pipe_panic(msg: &str) -> bool {
    msg.starts_with("failed printing to stdout") || msg.starts_with("failed printing to stderr")
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
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
    use super::is_broken_pipe_panic;

    #[test]
    fn recognizes_stdout_broken_pipe_message() {
        assert!(is_broken_pipe_panic(
            "failed printing to stdout: Broken pipe (os error 32)"
        ));
    }

    #[test]
    fn recognizes_stderr_broken_pipe_message() {
        assert!(is_broken_pipe_panic(
            "failed printing to stderr: Broken pipe (os error 32)"
        ));
    }

    #[test]
    fn does_not_match_unrelated_panics() {
        assert!(!is_broken_pipe_panic("index out of bounds"));
        assert!(!is_broken_pipe_panic(
            "called `Option::unwrap()` on a `None` value"
        ));
    }
}
