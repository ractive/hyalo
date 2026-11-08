//! Signal handling for a bulk write phase and the `--jq` worker (DEC-409).
//!
//! A bulk phase keeps its staged temporary files (and the scratch copies of
//! the notes it read) in a [`Registry`]. While that registry is not empty,
//! SIGINT, SIGTERM and SIGHUP are hooked, each one only if its disposition is
//! the default: an ignored signal (`nohup`, a shell background job, DEC-369)
//! stays ignored and the phase runs without cleanup. On a signal the files
//! not yet renamed are removed, what was written is reported on stderr and the
//! process then **dies by that signal** with the default action restored, so a
//! calling shell sees WIFSIGNALED and stops its script. Once the registry is
//! empty the hooks are removed, so a signal outside a phase behaves exactly as
//! without this module. The `--jq` worker (DEC-369) polls [`flag`]; it hooks
//! all three signals, only if all three are default, for the rest of the run,
//! and a signal then ends in the worker's cancellation, not in death.
//!
//! On Unix a minimal handler writes the signal number to a pipe and a thread
//! does the work, because removing files is not async-signal-safe. Windows has
//! no signals to re-raise: `ctrlc` hooks Ctrl-C and the process exits with
//! `STATUS_CONTROL_C_EXIT` (0xC000013A), what the default action gives.
//! SIGKILL and power loss cannot be handled: their temp files stay, named
//! `.hyalo-tmp-*` (see [`TEMP_PREFIX`]).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

/// Every temporary file a write creates beside a note starts with this, so a
/// leftover of a killed run is recognisable: `find <vault> -name '.hyalo-tmp-*'`.
pub const TEMP_PREFIX: &str = ".hyalo-tmp-";

static CANCELLED: AtomicBool = AtomicBool::new(false);
/// A `--jq` worker polls [`CANCELLED`], so a signal must not end the process.
static POLLED: AtomicBool = AtomicBool::new(false);
/// A signal is about to end the process: `main` must not return first.
static DYING: AtomicBool = AtomicBool::new(false);
static GLOBAL: Registry = Registry::new();
/// The longest a signal waits for workers that are creating a temp file.
const IN_FLIGHT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
/// Serialises hooking against unhooking.
static HOOKS: Mutex<()> = Mutex::new(());

/// The flag a signal raises; hooks the signals for the rest of the run, if all
/// of them are at their default.
pub fn flag() -> &'static AtomicBool {
    POLLED.store(true, Ordering::Relaxed);
    let _guard = HOOKS.lock().unwrap_or_else(PoisonError::into_inner);
    sys::hook_all_or_none();
    &CANCELLED
}

/// A phase has files to protect: hook each default signal.
fn ensure_hooked() {
    let _guard = HOOKS.lock().unwrap_or_else(PoisonError::into_inner);
    // Once a signal is ending the process, a worker must not hook it again.
    if !DYING.load(Ordering::SeqCst) {
        sys::hook_each_default();
    }
}

/// Nothing left to protect: give the signals back, unless a worker polls.
fn release_hooks_if_idle() {
    let _guard = HOOKS.lock().unwrap_or_else(PoisonError::into_inner);
    if !POLLED.load(Ordering::Relaxed) && GLOBAL.is_empty() {
        sys::unhook_all();
    }
}

/// Runs on the handler thread (Unix) or the Ctrl-C thread (Windows).
fn on_signal(signal: i32) {
    CANCELLED.store(true, Ordering::Relaxed);
    if !POLLED.load(Ordering::Relaxed) {
        // This signal ends the process. The cleanup below pulls files from
        // under the workers, which may then fail and let `main` return first;
        // an exit handler holds the process until the signal kills it.
        DYING.store(true, Ordering::SeqCst);
    }
    if let Some((removed, renamed)) = GLOBAL.interrupt(!POLLED.load(Ordering::Relaxed)) {
        eprintln!(
            "hyalo: interrupted: {renamed} note(s) were already written, {removed} staged note(s) \
             were not and their temporary files are removed"
        );
        DYING.store(true, Ordering::SeqCst);
        sys::die(signal);
    }
    // Outside a phase: the default action, unless a `--jq` worker polls.
    if !POLLED.load(Ordering::Relaxed) {
        sys::die(signal);
    }
}

#[cfg(unix)]
mod sys {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};

    const SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];
    static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);
    /// Bit `i` set: `SIGNALS[i]` carries our handler.
    static HOOKED: AtomicU8 = AtomicU8::new(0);
    static THREAD: OnceLock<bool> = OnceLock::new();

    extern "C" fn handler(signal: libc::c_int) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // a signal number is below 128
        let byte = signal as u8;
        // SAFETY: `write` is async-signal-safe; the descriptor is the pipe
        // made in `start_thread`, never closed; one byte from a live local.
        unsafe {
            libc::write(
                PIPE_WRITE.load(Ordering::Relaxed),
                (&raw const byte).cast(),
                1,
            );
        }
    }

    extern "C" fn hold_exit_while_dying() {
        while super::DYING.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn start_thread() -> bool {
        *THREAD.get_or_init(|| {
            // SAFETY: registers a plain `extern "C"` function that takes no
            // argument and only sleeps while a signal is ending the process.
            unsafe { libc::atexit(hold_exit_while_dying) };
            let mut fds = [0 as libc::c_int; 2];
            // SAFETY: `fds` has room for the two descriptors `pipe` writes.
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return false;
            }
            for fd in fds {
                // SAFETY: `fd` was just created; setting FD_CLOEXEC is harmless.
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
            }
            PIPE_WRITE.store(fds[1], Ordering::Relaxed);
            let read_end = fds[0];
            std::thread::Builder::new()
                .name("hyalo-signal".into())
                .spawn(move || {
                    loop {
                        let mut byte = 0u8;
                        // SAFETY: reads one byte into a live local from the pipe.
                        let n = unsafe { libc::read(read_end, (&raw mut byte).cast(), 1) };
                        if n == 1 {
                            super::on_signal(i32::from(byte));
                        } else if n == 0 {
                            return;
                        }
                    }
                })
                .is_ok()
        })
    }

    fn disposition(signal: libc::c_int) -> Option<libc::sigaction> {
        // SAFETY: a zeroed `sigaction` is a valid out-parameter; a null `act`
        // only queries the current disposition.
        unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            (libc::sigaction(signal, std::ptr::null(), &raw mut current) == 0).then_some(current)
        }
    }

    fn is_default(signal: libc::c_int) -> bool {
        disposition(signal).is_some_and(|d| d.sa_sigaction == libc::SIG_DFL)
    }

    fn set(signal: libc::c_int, action: libc::sighandler_t) -> bool {
        // SAFETY: a zeroed `sigaction` with an empty mask, SA_RESTART and
        // either SIG_DFL or our async-signal-safe `handler`.
        unsafe {
            let mut new: libc::sigaction = std::mem::zeroed();
            new.sa_sigaction = action;
            new.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&raw mut new.sa_mask);
            libc::sigaction(signal, &raw const new, std::ptr::null_mut()) == 0
        }
    }

    fn hook(index: usize) {
        let signal = SIGNALS[index];
        let bit = 1u8 << index;
        if HOOKED.load(Ordering::SeqCst) & bit == 0
            && is_default(signal)
            && set(signal, handler as *const () as libc::sighandler_t)
        {
            HOOKED.fetch_or(bit, Ordering::SeqCst);
        }
    }

    pub fn hook_each_default() {
        if start_thread() {
            (0..SIGNALS.len()).for_each(hook);
        }
    }

    /// DEC-369: all three at their default, or none hooked.
    pub fn hook_all_or_none() {
        if SIGNALS.iter().all(|&s| is_default(s) || is_ours(s)) && start_thread() {
            (0..SIGNALS.len()).for_each(hook);
        }
    }

    fn is_ours(signal: libc::c_int) -> bool {
        disposition(signal)
            .is_some_and(|d| d.sa_sigaction == handler as *const () as libc::sighandler_t)
    }

    pub fn unhook_all() {
        for (index, &signal) in SIGNALS.iter().enumerate() {
            let bit = 1u8 << index;
            if HOOKED.load(Ordering::SeqCst) & bit != 0 {
                if is_ours(signal) {
                    set(signal, libc::SIG_DFL);
                }
                HOOKED.fetch_and(!bit, Ordering::SeqCst);
            }
        }
    }

    /// Die by `signal` with its default action, as if it had never been hooked.
    pub fn die(signal: i32) {
        // The caller set `DYING`, so nothing hooks the signal again; try a few
        // times anyway, in case the signal was delivered before the restore.
        for _ in 0..40 {
            set(signal, libc::SIG_DFL);
            // SAFETY: signalling our own process with a valid signal number.
            unsafe { libc::kill(libc::getpid(), signal) };
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // SAFETY: `_exit` ends the process at once, skipping the exit handler
        // that is holding `main` back.
        unsafe { libc::_exit(128 + signal) };
    }
}

#[cfg(not(unix))]
mod sys {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();

    fn install() {
        INSTALLED.get_or_init(|| {
            // Ignoring the error is deliberate: an unavailable handler means
            // the phase runs without cleanup.
            let _ = ctrlc::try_set_handler(|| super::on_signal(0));
        });
    }
    pub fn hook_each_default() {
        install();
    }
    pub fn hook_all_or_none() {
        install();
    }
    pub fn unhook_all() {}
    /// `STATUS_CONTROL_C_EXIT`, the status the default action ends with.
    pub fn die(_signal: i32) {
        #[allow(clippy::cast_possible_wrap)]
        std::process::exit(0xC000_013Au32 as i32);
    }
}

/// The temporary files of the bulk phase in flight.
#[derive(Debug)]
pub struct Registry {
    state: Mutex<State>,
    renamed: AtomicUsize,
    /// Temp files being created: a signal waits for them to register.
    in_flight: AtomicUsize,
    stopped: AtomicBool,
}

#[derive(Debug)]
struct State {
    temps: BTreeSet<PathBuf>,
    /// Copies of notes in `$TMPDIR`: removed on a signal, never counted.
    scratch: BTreeSet<PathBuf>,
    interrupted: bool,
    active: bool,
}

impl Registry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                temps: BTreeSet::new(),
                scratch: BTreeSet::new(),
                interrupted: false,
                active: false,
            }),
            renamed: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
        }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
    pub fn register(&self, path: &Path) {
        let mut state = self.lock();
        state.active = true;
        if state.interrupted {
            // Created after the signal: nobody will claim it.
            let _ = std::fs::remove_file(path);
        } else {
            state.temps.insert(path.to_path_buf());
        }
    }
    pub fn register_scratch(&self, path: &Path) {
        let mut state = self.lock();
        if state.interrupted {
            let _ = std::fs::remove_file(path);
        } else {
            state.scratch.insert(path.to_path_buf());
        }
    }
    pub fn unregister_scratch(&self, path: &Path) {
        self.lock().scratch.remove(path);
    }
    /// Nothing registered: no signal needs hooking.
    pub fn is_empty(&self) -> bool {
        let state = self.lock();
        state.temps.is_empty() && state.scratch.is_empty()
    }
    /// Mark a temp file as about to be created and registered.
    ///
    /// After a signal a worker that has not started yet stops here for good
    /// (the process is about to exit), so the count can reach zero.
    pub fn flight(&self) -> Flight<'_> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        if self.stopped.load(Ordering::SeqCst) {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            loop {
                std::thread::park();
            }
        }
        Flight(&self.in_flight)
    }
    pub fn unregister(&self, path: &Path) {
        self.lock().temps.remove(path);
    }
    /// Take `path` out of the registry right before its rename. `false` after
    /// an interrupt: the signal handler owns the file and removes it.
    pub fn claim(&self, path: &Path) -> bool {
        let mut state = self.lock();
        if state.interrupted {
            return false;
        }
        state.temps.remove(path);
        true
    }
    pub fn renamed_one(&self) {
        self.renamed.fetch_add(1, Ordering::Relaxed);
    }
    /// The phase ended normally.
    pub fn done(&self) {
        let mut state = self.lock();
        state.active = false;
        state.temps.clear();
        self.renamed.store(0, Ordering::Relaxed);
    }
    /// Remove every unclaimed temporary file and refuse further claims.
    /// `dying`: the process ends after this, so later registrations remove
    /// their file at once. `Some((removed, renamed))` when a phase was in flight.
    pub fn interrupt(&self, dying: bool) -> Option<(usize, usize)> {
        let (removed, active) = {
            let mut state = self.lock();
            let active = state.active;
            if active || dying {
                state.interrupted = true;
                self.stopped.store(true, Ordering::SeqCst);
            }
            for copy in std::mem::take(&mut state.scratch) {
                let _ = std::fs::remove_file(copy);
            }
            let temps = std::mem::take(&mut state.temps);
            if active {
                for temp in &temps {
                    let _ = std::fs::remove_file(temp);
                }
            }
            (temps.len(), active)
        };
        if !active && !dying {
            return None;
        }
        // A worker between creating a file and registering it removes it at
        // registration now; wait for it to get there. It does one file
        // creation, which on a loaded disk (a CI runner) can take well over
        // half a second: a shorter wait let the process die first and left the
        // copy behind (DEC-409). Workers that have not started park in
        // `flight`, so the count only falls.
        let deadline = std::time::Instant::now() + IN_FLIGHT_WAIT;
        while self.in_flight.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        active.then(|| (removed, self.renamed.load(Ordering::Relaxed)))
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// See [`Registry::flight`].
#[derive(Debug)]
pub struct Flight<'a>(&'a AtomicUsize);

impl Drop for Flight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Hold while creating and tracking a temporary file of a bulk phase.
#[must_use]
pub fn flight() -> Flight<'static> {
    GLOBAL.flight()
}

/// A temporary file registered with the process registry until dropped.
#[derive(Debug)]
pub struct Tracked(Option<PathBuf>);

impl Tracked {
    /// Track `path` for a bulk phase.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        GLOBAL.register(path);
        ensure_hooked();
        Self(Some(path.to_path_buf()))
    }
    /// Not tracked (a single-note write).
    #[must_use]
    pub fn none() -> Self {
        Self(None)
    }
    /// Right before the rename: `false` once a signal arrived.
    #[must_use]
    pub fn claim(&self) -> bool {
        let claimed = self.0.as_deref().is_none_or(|path| GLOBAL.claim(path));
        if claimed {
            release_hooks_if_idle();
        }
        claimed
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            GLOBAL.unregister(path);
            release_hooks_if_idle();
        }
    }
}

/// A copy of a note in `$TMPDIR`, registered until dropped so a signal
/// removes it (it is not a staged note and is never counted).
#[derive(Debug)]
pub struct Scratch(PathBuf);

impl Scratch {
    #[must_use]
    pub fn new(path: &Path) -> Self {
        GLOBAL.register_scratch(path);
        ensure_hooked();
        Self(path.to_path_buf())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        GLOBAL.unregister_scratch(&self.0);
        release_hooks_if_idle();
    }
}

/// A note was renamed by the phase.
pub fn renamed_one() {
    GLOBAL.renamed_one();
}

/// The phase finished; a later signal is not about it.
pub fn phase_done() {
    GLOBAL.done();
    release_hooks_if_idle();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signal_removes_every_unclaimed_temp_and_refuses_later_claims() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let paths: Vec<_> = (0..5)
            .map(|i| {
                let path = dir.path().join(format!("{TEMP_PREFIX}{i}"));
                std::fs::write(&path, b"x").unwrap();
                registry.register(&path);
                path
            })
            .collect();
        assert!(registry.claim(&paths[0]));
        registry.renamed_one();
        assert_eq!(registry.interrupt(true), Some((4, 1)));
        assert!(paths[0].exists(), "a claimed temp belongs to its rename");
        assert!(paths[1..].iter().all(|p| !p.exists()));
        assert!(!registry.claim(&paths[0]));
    }

    #[test]
    fn a_signal_outside_a_phase_is_not_ours() {
        let registry = Registry::new();
        assert_eq!(registry.interrupt(true), None);
        registry.register(Path::new("x"));
        registry.done();
        assert_eq!(registry.interrupt(true), None);
    }

    #[test]
    fn a_signal_that_does_not_end_the_process_leaves_the_registry_usable() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let copy = dir.path().join("copy");
        std::fs::write(&copy, b"x").unwrap();
        registry.register_scratch(&copy);
        // A polled `--jq` signal outside a phase: the scratch copy goes, the
        // registry keeps accepting work.
        assert_eq!(registry.interrupt(false), None);
        assert!(!copy.exists());
        let temp = dir.path().join("temp");
        std::fs::write(&temp, b"x").unwrap();
        registry.register(&temp);
        assert!(registry.claim(&temp));
        assert!(temp.exists());
    }

    #[test]
    fn a_file_registered_after_a_fatal_signal_is_removed_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        assert_eq!(registry.interrupt(true), None);
        let late = dir.path().join("late");
        std::fs::write(&late, b"x").unwrap();
        registry.register_scratch(&late);
        assert!(!late.exists());
    }

    /// A worker that is slow to create its copy (a loaded disk) is still waited
    /// for: the copy must not survive the signal.
    #[test]
    fn a_signal_waits_for_a_worker_that_is_slow_to_register_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let copy = dir.path().join("copy");
        std::fs::write(&copy, b"x").unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let _flight = registry.flight();
                // Slower than the old half-second wait.
                std::thread::sleep(std::time::Duration::from_millis(900));
                registry.register_scratch(&copy);
            });
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert_eq!(registry.interrupt(true), None);
            assert!(!copy.exists(), "the copy survived the signal");
        });
    }
}
