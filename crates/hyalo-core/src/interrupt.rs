//! The one Ctrl-C / SIGTERM / SIGHUP handler of the process (DEC-409).
//!
//! `ctrlc` allows a single handler per process, and two things need one: the
//! `--jq` worker (DEC-369) polls [`flag`], and a bulk write phase keeps its
//! staged temporary files in a [`Registry`] so a signal can remove every one
//! that was not renamed yet. On a signal during a phase the process reports
//! what was written, leaves the untouched notes untouched and exits 130
//! (the Ctrl-C convention for every signal: `ctrlc` does not say which).
//! SIGKILL and power loss cannot be handled: their temp files stay, named
//! `.hyalo-tmp-*` (see [`TEMP_PREFIX`]).
//!
//! Installing the handler is best effort, as for `--jq`: `ctrlc` refuses when a
//! signal is ignored (a shell background job, `nohup`), and the phase then runs
//! without cleanup.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

/// Every temporary file a write creates beside a note starts with this, so a
/// leftover of a killed run is recognisable: `find <vault> -name '.hyalo-tmp-*'`.
pub const TEMP_PREFIX: &str = ".hyalo-tmp-";

static CANCELLED: AtomicBool = AtomicBool::new(false);
/// A `--jq` worker polls [`CANCELLED`], so a signal must not end the process.
static POLLED: AtomicBool = AtomicBool::new(false);
static INSTALLED: OnceLock<()> = OnceLock::new();
static GLOBAL: Registry = Registry::new();

/// The flag a signal raises; installs the handler on first use.
pub fn flag() -> &'static AtomicBool {
    POLLED.store(true, Ordering::Relaxed);
    install();
    &CANCELLED
}

/// Install the process handler (once, best effort).
pub fn install() {
    INSTALLED.get_or_init(|| {
        // Ignoring the error is deliberate: see the module docs.
        let _ = ctrlc::try_set_handler(on_signal);
    });
}

fn on_signal() {
    CANCELLED.store(true, Ordering::Relaxed);
    if let Some((removed, renamed)) = GLOBAL.interrupt() {
        eprintln!(
            "hyalo: interrupted: {renamed} note(s) were already written, {removed} staged note(s) \
             were not and their temporary files are removed"
        );
        std::process::exit(130);
    }
    // The handler replaced the default action: end the process like it unless
    // a `--jq` worker is polling the flag.
    if !POLLED.load(Ordering::Relaxed) {
        std::process::exit(130);
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
    interrupted: bool,
    active: bool,
}

impl Registry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                temps: BTreeSet::new(),
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
    /// `Some((removed, renamed))` when a phase was in flight.
    pub fn interrupt(&self) -> Option<(usize, usize)> {
        let removed = {
            let mut state = self.lock();
            if !state.active {
                return None;
            }
            state.interrupted = true;
            self.stopped.store(true, Ordering::SeqCst);
            let temps = std::mem::take(&mut state.temps);
            for temp in &temps {
                let _ = std::fs::remove_file(temp);
            }
            temps.len()
        };
        // A worker between creating a file and registering it removes it at
        // registration now; give it a moment to get there.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while self.in_flight.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        Some((removed, self.renamed.load(Ordering::Relaxed)))
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
        install();
        GLOBAL.register(path);
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
        self.0.as_deref().is_none_or(|path| GLOBAL.claim(path))
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            GLOBAL.unregister(path);
        }
    }
}

/// A note was renamed by the phase.
pub fn renamed_one() {
    GLOBAL.renamed_one();
}

/// The phase finished; a later signal is not about it.
pub fn phase_done() {
    GLOBAL.done();
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
        assert_eq!(registry.interrupt(), Some((4, 1)));
        assert!(paths[0].exists(), "a claimed temp belongs to its rename");
        assert!(paths[1..].iter().all(|p| !p.exists()));
        assert!(!registry.claim(&paths[0]));
    }

    #[test]
    fn a_signal_outside_a_phase_is_not_ours() {
        let registry = Registry::new();
        assert_eq!(registry.interrupt(), None);
        registry.register(Path::new("x"));
        registry.done();
        assert_eq!(registry.interrupt(), None);
    }
}
