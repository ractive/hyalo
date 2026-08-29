//! Compiling and running jq/jaq filters, with the time, value-count and output-size budgets.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

use super::{D, JaqFilterCache};
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Native, Vars, load};
use jaq_json::Val;

/// Apply a jq filter string to a `serde_json::Value` and return the text output.
///
/// Looks up or compiles the filter in `cache`. Multiple outputs are joined with
/// newlines. On any error (parse or runtime), returns `None` (used internally
/// by the text formatter, which has its own fallbacks).
pub(super) fn apply_jq_filter(
    filter_code: &str,
    value: &serde_json::Value,
    cache: &mut JaqFilterCache,
) -> Option<String> {
    run_jq_filter_cached(filter_code, value, cache).ok()
}

/// Wall-clock deadline for evaluating a single user-supplied `--jq` filter
/// (F3-1, deep-analysis-3-2026-08-23.md).
///
/// `jaq`'s public API (checked jaq-core 3.0.0) has no step counter, fuel
/// limit, or other cooperative-cancellation hook — a pathological filter's
/// unbounded work can happen entirely *inside one* internal evaluation step,
/// with no opportunity for us to check anything in between. `[range(3e8)]`
/// builds its whole 300M-element intermediate array before the interpreter
/// ever yields a value back to Rust (verified: 8.7s / 4.8 GB peak RSS to
/// print a single number), and `def f: f; f` recurses forever without ever
/// producing a value at all — so neither is reachable by checking a clock
/// between values pulled from the output iterator inside
/// [`execute_jq_filter`]'s loop; that loop's body never even runs for the
/// second case.
///
/// The mitigation is therefore necessarily coarse: run the filter on its own
/// thread and bound how long the *caller* waits via `recv_timeout`. Both
/// call sites (`output_pipeline.rs`, `run.rs`'s `hyalo config --jq`) format
/// the returned error and return almost immediately afterward, so — since
/// the worker thread is deliberately never joined on timeout — the whole
/// process, worker included, is torn down by the OS shortly after this
/// function returns. That caps the *actual* resource exposure to roughly one
/// deadline's worth of runaway work rather than the filter's full
/// pathological cost: a 3s deadline turns `[range(3e8)]`'s real 8.7s/4.8 GB
/// into an error after 3s and however much the abandoned thread allocated in
/// that window, not the full amount.
///
/// A single pathological *value* (not a huge intermediate collection) is
/// additionally checked by raw byte length before it is ever copied out of
/// the interpreter — see the pre-check in [`execute_jq_filter`] — since that
/// case (`"x" * 2000000000`, ~4.0 GB peak RSS in ~1.5s, comfortably under
/// this deadline) would otherwise slip past both output caps by finishing
/// before either check could catch it, and used to be measured only *after*
/// being duplicated into a second multi-GB copy just to learn it was too
/// big.
///
/// **Known residual gap, not covered by anything here:** unbounded *native
/// stack* recursion — `def f: [f]; f` — overflows the OS thread stack and
/// hits Rust's SIGSEGV-to-`abort()` guard page, killing the whole process
/// immediately (verified: `exit 134`, well under this deadline) rather than
/// erroring cleanly through it. This is not a regression from this
/// mitigation — the same filter would abort a non-threaded evaluator's
/// stack identically on unpatched jaq — and there is no user-space hook to
/// catch a native stack overflow before it happens. Documented rather than
/// silently claimed to be covered; see DEC-093.
pub(super) const JQ_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Maximum number of output values a `--jq` filter may emit, independent of
/// [`JQ_OUTPUT_CAP`]'s byte total.
///
/// The byte cap alone only bounds emitted *bytes* — a filter producing
/// millions of tiny values (e.g. `range(1e9) | tostring[0:0]`, all empty
/// strings) would iterate far longer than intended without ever crossing
/// it. This is a cheap, exact check inside the loop that already tracks
/// `total_len`, catching that class before the byte cap would.
pub(super) const JQ_MAX_OUTPUT_VALUES: usize = 1_000_000;

/// Apply a user-supplied jq filter to a `serde_json::Value`.
///
/// Compiles the filter on every call. For repeated use across many values,
/// prefer the cached path via [`format_success`] / [`format_value_as_text`].
///
/// Bounded by [`JQ_TIME_LIMIT`]: compilation and execution both run on a
/// worker thread, and this function returns a clean timeout error if that
/// thread hasn't produced a result within the deadline (see the constant's
/// doc comment for why a thread is necessary rather than a cooperative
/// check). Only `filter_code`/`value` (cloned into the thread) and the final
/// `Result<String, String>` (sent back over a channel) ever cross the thread
/// boundary — never a compiled `Filter` or a `jaq_json::Val`, both of which
/// use `Rc` internally and so are not `Send`.
///
/// Returns `Ok(String)` with newline-joined output values on success, or
/// `Err(String)` with a human-readable description of the parse, runtime, or
/// timeout error.
pub fn apply_jq_filter_result(
    filter_code: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    apply_jq_filter_with_limit(filter_code, value, JQ_TIME_LIMIT)
}

/// [`apply_jq_filter_result`] with an explicit deadline. Production callers
/// always go through the constant-limit wrapper; tests that exercise the
/// output caps (value count, byte size) pass a generous deadline so the
/// *count* invariant is what fails on a slow or emulated CI runner (the
/// v0.21.0 release pipeline saw a 1,000,000-value cap test lose the race
/// against the 3 s limit under QEMU-emulated aarch64), never the clock.
pub(super) fn apply_jq_filter_with_limit(
    filter_code: &str,
    value: &serde_json::Value,
    time_limit: std::time::Duration,
) -> Result<String, String> {
    let filter_code = filter_code.to_owned();
    let value = value.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    let spawned = std::thread::Builder::new()
        .name("hyalo-jq-eval".to_owned())
        .spawn(move || {
            let result = compile_jq_filter(&filter_code)
                .and_then(|filter| execute_jq_filter(&filter, &value, &filter_code));
            // The receiver may already have timed out and moved on; a failed
            // send just means nobody is listening anymore, not an error here.
            let _ = tx.send(result);
        });
    let handle = match spawned {
        Ok(h) => h,
        Err(e) => return Err(format!("failed to start jq evaluation: {e}")),
    };

    match rx.recv_timeout(time_limit) {
        Ok(result) => {
            // Finished within the deadline — join is a formality (near-zero
            // wait, the thread already sent its result), just to avoid
            // leaking a completed-but-unjoined handle.
            let _ = handle.join();
            result
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // Deliberately not joined: the worker may still be spinning
            // (`def f: f; f`) or mid-allocation (`[range(3e8)]`). It is left
            // to run to completion or to be torn down by the OS when this
            // process exits shortly after returning this error — see
            // `JQ_TIME_LIMIT`'s doc comment.
            drop(handle);
            Err(format!(
                "jq filter exceeded the {}s time limit (see `hyalo find --help` for --jq's limits)",
                time_limit.as_secs()
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("jq evaluation thread panicked".to_owned())
        }
    }
}

/// Format a jaq load error (lex/parse/IO) into a human-readable string.
///
/// `load::Error<&str>` does not implement `Display`, so we extract the first
/// error's kind and the offending source snippet manually.
pub(super) fn format_load_errors(errs: &load::Errors<&str, ()>) -> String {
    // errs is Vec<(File<&str, ()>, load::Error<&str>)>
    // We take the first entry and describe its error kind.
    for (_file, err) in errs {
        match err {
            load::Error::Io(ios) => {
                if let Some((_path, msg)) = ios.first() {
                    return format!("jq filter error (IO): {msg}");
                }
            }
            load::Error::Lex(lex_errs) => {
                if let Some((expect, span)) = lex_errs.first() {
                    return format!(
                        "jq filter syntax error: expected {} near {:?}",
                        expect.as_str(),
                        span
                    );
                }
            }
            load::Error::Parse(parse_errs) => {
                if let Some((expect, _token)) = parse_errs.first() {
                    return format!("jq filter parse error: expected {}", expect.as_str());
                }
            }
        }
    }
    "jq filter error: invalid filter syntax".to_owned()
}

/// Compile a jq filter string into a reusable `Filter`.
///
/// The `Arena` used during loading is a temporary scratch pad and is dropped
/// after this function returns — the compiled `Filter` owns all its data.
pub(super) fn compile_jq_filter(
    filter_code: &str,
) -> Result<jaq_core::compile::Filter<Native<D>>, String> {
    let program = File {
        code: filter_code,
        path: (),
    };
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let loader = Loader::new(defs);
    let arena = Arena::default();

    let modules = loader
        .load(&arena, program)
        .map_err(|errs| format_load_errors(&errs))?;

    let funs = jaq_core::funs::<D>()
        .chain(jaq_std::funs::<D>())
        .chain(jaq_json::funs::<D>());
    Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| {
            // compile::Errors = Vec<(File<S,P>, Vec<(S, Undefined)>)>
            // Extract the first undefined symbol name for a useful message.
            let first = errs.iter().flat_map(|(_file, undefs)| undefs.iter()).next();
            if let Some((name, undef)) = first {
                format!("jq filter error: undefined {} {:?}", undef.as_str(), name)
            } else {
                "jq filter error: compilation failed".to_owned()
            }
        })
}

/// Maximum total output size for a jq filter to prevent pathological filters
/// from causing unbounded memory growth (e.g. exponential-expansion patterns).
pub(super) const JQ_OUTPUT_CAP: usize = 10 * 1024 * 1024; // 10 MiB

/// Maximum length (in `char`s) of a diagnostic string embedded in an error
/// envelope — the jq runtime error message and the filter code that produced
/// it (F-3, `deep-analysis-2-2026-08-23.md`). jaq's runtime `Display` embeds
/// the *value* it failed on (e.g. the whole `.results` array for `.results |
/// .file` on an array), which on a large vault can be megabytes; an
/// unbounded error also makes `--jq` a content-disclosure vector for any
/// consumer that logs error output. This is a `char` bound, not a byte
/// bound, so truncation never splits a multi-byte codepoint.
pub(super) const JQ_ERROR_DIAGNOSTIC_CHAR_CAP: usize = 200;

/// Truncate `s` to at most [`JQ_ERROR_DIAGNOSTIC_CHAR_CAP`] characters,
/// appending `…` when truncation occurred. Character-based (not byte-based)
/// so a multi-byte codepoint is never split.
pub(super) fn truncate_diagnostic(s: &str) -> String {
    if s.chars().count() <= JQ_ERROR_DIAGNOSTIC_CHAR_CAP {
        return s.to_owned();
    }
    let mut truncated: String = s.chars().take(JQ_ERROR_DIAGNOSTIC_CHAR_CAP).collect();
    truncated.push('…');
    truncated
}

/// Execute a pre-compiled jq filter against a JSON value and return the text output.
///
/// `filter_code` is used only to name the filter in error messages — the
/// filter itself has already been compiled into `filter`.
pub(super) fn execute_jq_filter(
    filter: &jaq_core::compile::Filter<Native<D>>,
    value: &serde_json::Value,
    filter_code: &str,
) -> Result<String, String> {
    let input: Val = serde_json::from_value(value.clone()).map_err(|e| {
        format!(
            "jq input conversion error in filter {}: {}",
            truncate_diagnostic(filter_code),
            truncate_diagnostic(&e.to_string())
        )
    })?;
    let ctx = Ctx::<D>::new(&filter.lut, Vars::new([]));

    let mut out = String::new();
    let mut total_len: usize = 0;
    let mut value_count: usize = 0;
    for result in filter.id.run((ctx, input)).map(jaq_core::unwrap_valr) {
        match result {
            Ok(val) => {
                value_count += 1;
                if value_count > JQ_MAX_OUTPUT_VALUES {
                    return Err(format!(
                        "jq filter output exceeds {JQ_MAX_OUTPUT_VALUES} values"
                    ));
                }
                // Check a string value's raw byte length BEFORE copying it
                // (Finding 2, review round on PR #254): a single pathological
                // value — `"x" * 2000000000` measured at ~4.0 GB peak RSS in
                // 1.49s, comfortably under JQ_TIME_LIMIT — used to be
                // measured only *after* `.to_owned()`/`from_utf8_lossy()`
                // duplicated it into a second multi-GB buffer just to learn
                // it was too big. `s` here still borrows from `val`, so this
                // costs nothing beyond the length check already available on
                // the byte slice jaq handed us.
                //
                // No equivalent pre-check exists for non-string values
                // (`other.to_string()` below): jaq's `Display` impl builds
                // the whole formatted string in one call with no length
                // preview, so a single huge *non-string* value (e.g. a large
                // array/object formatted directly, without a `length`/count
                // reduction) is bounded only by JQ_TIME_LIMIT, not by either
                // output cap — documented on `JQ_TIME_LIMIT`'s doc comment
                // and in DEC-093 rather than silently claimed to be covered.
                if let Val::TStr(ref s) | Val::BStr(ref s) = val
                    && s.len() > JQ_OUTPUT_CAP
                {
                    return Err(format!(
                        "jq filter output exceeds {} MiB limit",
                        JQ_OUTPUT_CAP / (1024 * 1024)
                    ));
                }
                let s = match val {
                    Val::TStr(ref s) | Val::BStr(ref s) => match std::str::from_utf8(s) {
                        Ok(valid) => valid.to_owned(),
                        Err(_) => String::from_utf8_lossy(s).into_owned(),
                    },
                    // For non-string values, `Display` produces valid JSON
                    // (numbers, booleans, null, arrays, objects).
                    other => other.to_string(),
                };
                // Account for the newline separator that will be prepended
                // between fragments when out is non-empty.
                total_len = total_len
                    .saturating_add(s.len())
                    .saturating_add(usize::from(!out.is_empty()));
                if total_len > JQ_OUTPUT_CAP {
                    return Err(format!(
                        "jq filter output exceeds {} MiB limit",
                        JQ_OUTPUT_CAP / (1024 * 1024)
                    ));
                }
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&s);
            }
            Err(e) => {
                // jaq's runtime `Display` embeds the value it failed on (e.g. the
                // whole input array for `.file` applied to an array) — truncate
                // both the value-bearing message and the filter code so a single
                // mistyped `--jq` filter on a large vault can't dump megabytes of
                // vault content into the error envelope (F-3, DEC-094).
                return Err(format!(
                    "jq runtime error in filter {}: {}",
                    truncate_diagnostic(filter_code),
                    truncate_diagnostic(&e.to_string())
                ));
            }
        }
    }

    Ok(out)
}

/// Look up or compile a jq filter from `cache`, then execute it against `value`.
pub(super) fn run_jq_filter_cached(
    filter_code: &str,
    value: &serde_json::Value,
    cache: &mut JaqFilterCache,
) -> Result<String, String> {
    if let Some(filter) = cache.get(filter_code) {
        return execute_jq_filter(filter, value, filter_code);
    }
    let compiled = compile_jq_filter(filter_code)?;
    let filter = cache.entry(filter_code.to_owned()).or_insert(compiled);
    execute_jq_filter(filter, value, filter_code)
}
