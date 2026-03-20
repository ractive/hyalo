---
name: review-rust
description: >
  Perform critical Rust code reviews covering correctness, edition 2024 compliance, error
  handling, API design, async pitfalls, and dependency hygiene. ALWAYS use this skill when the
  user wants to review, audit, or critically evaluate Rust code — whether that's a PR diff,
  a specific crate or module, a cross-cutting concern like error handling, or the whole
  codebase. This includes requests to "review my changes", "check this code", "audit for best
  practices", "look for issues", "what's wrong with this", or "flag anything that could bite
  us". Use it even when the user doesn't say "review" explicitly but is asking you to find
  problems, inconsistencies, or anti-patterns in existing Rust code. Do NOT use for requests
  to write, fix, refactor, test, explain, or implement code — only for evaluating existing code.
---

# Rust Code Review (Edition 2024)

You are performing a critical code review of Rust code targeting edition 2024. Your job is to
find real problems — not to rubber-stamp or nitpick formatting. Focus on correctness, safety,
idiomatic patterns, and maintainability in that order.

## Determine Review Scope

First, figure out what you're reviewing:

- **PR review**: Use `git diff` against the base branch to identify changed files. Focus review
  on the diff, but read surrounding context to understand impact.
- **Crate/module review**: Read the specified crate or module top-down. Start with `lib.rs` or
  `mod.rs` to understand the public API surface, then drill into implementation.
- **Full codebase review**: Start with `Cargo.toml` (workspace layout, edition, dependencies),
  then review each crate systematically. Prioritize public API surfaces and core logic.

For PR reviews, also check:
- Does the PR title/description match what the code actually does?
- Are there unrelated changes mixed in?
- Is the diff a reasonable size, or should it be split?

## Review Process

Work through these categories in order. Not every category applies to every review — skip
sections that aren't relevant to the code at hand. Spend your time proportional to risk.

### 1. Correctness

This is the most important category. Bugs ship when reviewers focus on style instead of logic.

- **Logic errors**: Trace the happy path and key error paths mentally. Look for off-by-one,
  wrong comparison operators, swapped arguments, incorrect boolean logic.
- **Edge cases**: What happens with empty collections, zero values, `None`, maximum values,
  concurrent access? Does the code handle them or silently produce wrong results?
- **Error handling**: Are errors propagated correctly? Watch for `unwrap()` and `expect()` on
  values that can legitimately fail at runtime. Using `?` is good — but is the error type
  meaningful to the caller, or does it erase important context?
- **Resource management**: Are files, connections, locks released properly? Rust's RAII helps,
  but watch for `std::mem::forget`, leaked `Box::into_raw`, or holding locks across `.await`.
- **Integer overflow**: Debug builds panic, release builds wrap. If the code does arithmetic on
  user-provided values, are `checked_*` or `saturating_*` methods used?

### 2. Rust 2024 Edition Compliance

The 2024 edition (Rust 1.85+) introduced breaking changes. Check that the code is compatible:

- **`unsafe_op_in_unsafe_fn`**: Unsafe operations inside `unsafe fn` now require an explicit
  `unsafe {}` block. The entire body is no longer implicitly unsafe. Each block should have a
  `// SAFETY:` comment.
- **`unsafe` attributes**: `#[no_mangle]`, `#[export_name]`, and `#[link_section]` must be
  written as `unsafe(...)` — e.g. `#[unsafe(no_mangle)]`. Flag bare uses.
- **`unsafe extern` blocks**: `extern` blocks now require `unsafe extern`. Each item inside
  can be individually marked `safe` if appropriate.
- **`static mut` references denied**: Taking a reference to a `static mut` is a hard error.
  Use `Mutex`, `OnceLock`, atomics, or raw pointers instead.
- **RPIT lifetime capture**: Return-position `impl Trait` now captures ALL in-scope lifetime
  parameters (not just type params as in 2021). This is the most impactful change. If the
  hidden type doesn't live long enough, compilation fails. To opt out: `impl Trait + use<>`.
  Review any `-> impl Trait` signatures for accidental captures.
- **Newly unsafe functions**: `std::env::set_var`, `std::env::remove_var`, and
  `CommandExt::before_exec` are now unsafe. Flag bare calls.
- **Macro fragment specifiers**: `expr` now matches `const {}` and `_` expressions. If a macro
  relies on the old behavior, it should use `expr_2021`. Missing fragment specifiers are a hard
  error.
- **`dyn Trait` required**: Bare trait objects (without `dyn`) are a hard error.
- **Reserved keywords**: `gen` is reserved. Identifiers using it need `r#gen`.
  `#"..."#` guarded strings and `##` tokens are also reserved.
- **Prelude additions**: `Future` and `IntoFuture` are in the 2024 prelude. Check for name
  conflicts with local traits.
- **Let chains**: `if let Some(x) = a && let Some(y) = b { ... }` is now valid. Prefer this
  over nested `if let` when it improves readability.
- **Match ergonomics**: `ref`, `mut`, `ref mut` capture modifiers are disallowed in patterns
  that use auto-dereferencing (match ergonomics). Flag if encountered.
- **Temporary scope changes**: Temporaries from tail expressions are now dropped before local
  variables. Review code that relies on temporaries living until end of block.
- **Never type coercion**: Changes to how `!` coerces may affect match arms and closures that
  diverge.

### 3. Ownership, Borrowing, and Lifetimes

These are where Rust-specific bugs hide that the compiler doesn't always catch at a design level.

- **Unnecessary cloning**: `.clone()` to satisfy the borrow checker is sometimes needed, but
  often signals a design issue. Can the function take a reference instead? Can the data be
  restructured to avoid shared ownership?
- **Lifetime over-constraint**: Are lifetime parameters more restrictive than necessary? A
  function taking `&'a str` when it could take `&str` (elided) adds complexity without benefit.
- **`Arc<Mutex<T>>` smell**: Sometimes necessary, but often indicates the data ownership model
  should be rethought. Consider channels, actor patterns, or restructuring.
- **Returning references to temporaries**: The compiler catches the obvious cases, but complex
  chains of method calls can obscure them.
- **`Cow<'_, T>` opportunities**: If code clones conditionally (clone on some paths, borrow on
  others), `Cow` is the idiomatic pattern.

### 4. API Design

Apply the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html)
where relevant:

- **Naming**: Follow Rust conventions — `new()` for constructors, `into_*()` for consuming
  conversions, `as_*()` for cheap reference conversions, `to_*()` for expensive conversions,
  `is_*()` / `has_*()` for boolean queries. Iterator-producing methods: `iter()`, `iter_mut()`,
  `into_iter()`.
- **Common trait implementations**: Types should derive or implement traits that users expect:
  `Debug` (almost always), `Clone`, `PartialEq`, `Eq`, `Hash`, `Display` (for user-facing
  types), `Default` (when a sensible default exists), `Send + Sync` (when possible).
- **Error types**: Custom errors should implement `std::error::Error`, `Display`, and `Debug`.
  Use `thiserror` for library errors, `anyhow` for application errors. Error variants should
  carry enough context to be actionable.
- **Builder pattern**: For structs with many optional fields, a builder is more ergonomic than
  a constructor with many parameters. Check that builders validate at build time, not use time.
- **Sealed traits**: If a trait is not meant to be implemented outside the crate, seal it.
- **`#[must_use]`**: Functions that return a value where ignoring it is almost certainly a bug
  (like `Result`) should be `#[must_use]`.
- **`#[non_exhaustive]`**: Public enums and structs that may gain variants/fields in future
  versions should use this to preserve semver compatibility.
- **Type conversions**: Prefer `From`/`Into` over custom conversion methods. `TryFrom`/`TryInto`
  for fallible conversions.

### 5. Error Handling Patterns

Poor error handling is the #1 source of production incidents in Rust services.

- **`unwrap()` / `expect()` audit**: Grep the code under review for `.unwrap()` and `.expect()`
  calls. Every call site should be justified. In library code, they're almost never acceptable.
  In application code, `expect()` with a descriptive message is tolerable only when the
  invariant is truly guaranteed. Report each unjustified instance with file and line.
- **Error context**: Bare `?` loses context. Prefer `.map_err(|e| ...)` or use `anyhow::Context`
  / `thiserror` to add "what was being attempted" to the error chain.
- **Panicking in libraries**: Libraries should never panic on bad input. Return `Result` or
  `Option`. Document when a function can panic (e.g., index out of bounds).
- **Error granularity**: A single catch-all error enum with 20 variants is a smell. Group errors
  by operation. Callers should be able to match on the variants they care about.
- **`Box<dyn Error>`**: Fine for quick prototypes, but in published APIs it prevents callers
  from matching on specific error types. Prefer concrete error types.

### 6. Async Code

Async Rust has unique pitfalls beyond what the compiler catches.

- **Holding locks across `.await`**: A `MutexGuard` held across an await point blocks the
  executor. Use `tokio::sync::Mutex` if you must hold across awaits, or restructure to drop
  the guard before awaiting.
- **Blocking in async context**: `std::fs`, `std::thread::sleep`, CPU-heavy computation in
  async tasks starve the executor. Use `tokio::fs`, `tokio::time::sleep`,
  `tokio::task::spawn_blocking`.
- **`Send` bounds**: If the future needs to be `Send` (most executors require this), all types
  held across await points must be `Send`. Watch for `Rc`, `Cell`, `RefCell`.
- **Cancellation safety**: When a future is dropped (e.g., in `tokio::select!`), is the
  operation left in a consistent state? Partial writes, half-sent messages, leaked resources.
- **Unnecessary `async`**: If a function doesn't actually await anything, it shouldn't be async.
  The async machinery adds overhead.
- **Spawning too many tasks**: `tokio::spawn` per request is fine; `tokio::spawn` per item in
  a large collection might exhaust memory. Consider `FuturesUnordered` or `buffer_unordered`.

### 7. Performance

Only flag performance issues that are likely to matter. Micro-optimizations in cold paths are
noise.

- **Unnecessary allocations**: `String` where `&str` suffices, `Vec<u8>` where `&[u8]` works,
  `format!()` in a hot loop.
- **Iterator misuse**: Collecting into a `Vec` just to iterate again. Prefer chaining iterators.
  Use `Iterator::size_hint()` and `Vec::with_capacity()` when the size is known.
- **Large types on the stack**: Structs over ~1KB on the stack can blow it in deep recursion.
  Box large types.
- **`to_string()` / `format!()` in Display impls**: Can cause infinite recursion or needless
  allocation. Write directly to the formatter.
- **Serialization overhead**: For APIs, check that serde attributes are used correctly —
  `#[serde(rename_all = "camelCase")]`, skip unnecessary fields, use `#[serde(default)]`
  judiciously.

### 8. Dependencies and Cargo.toml

- **Edition field**: Must be `edition = "2024"` in `Cargo.toml`. Flag if missing or outdated.
- **Dependency versions**: Are versions pinned appropriately? Workspace dependencies should use
  `workspace = true`. Watch for duplicated dependency specifications across workspace members.
- **Feature flags**: Are default features disabled for dependencies that don't need them?
  (`default-features = false`). Are feature flags additive (they should be — a feature should
  never remove functionality)?
- **Dev vs runtime dependencies**: Test-only dependencies belong in `[dev-dependencies]`. Build
  tools in `[build-dependencies]`. Check that heavy deps like `tokio` with `full` feature aren't
  pulled in unnecessarily.
- **Duplicate dependencies**: Multiple versions of the same crate inflate compile times and
  binary size. Flag when visible in `Cargo.lock`.
- **Crate audit**: Are any dependencies unmaintained, known-vulnerable, or from untrusted
  sources? Consider `cargo audit` and `cargo deny` checks.

### 9. Testing

- **Coverage of critical paths**: Are the happy path, error paths, and edge cases tested?
  Untested error handling is as bad as no error handling.
- **Test isolation**: Do tests depend on external state (network, filesystem, environment
  variables)? They should be hermetic or clearly marked as integration tests.
- **Assertion quality**: `assert!(result.is_ok())` loses the error message on failure. Prefer
  `result.unwrap()` in tests, or pattern match with a descriptive panic message.
- **Mock boundaries**: If the code uses trait objects or generics for testability, is the
  boundary at the right level? Too fine-grained mocking tests implementation details, not
  behavior.

### 10. Documentation

Only flag missing documentation that would actually help someone. Don't demand docs on every
private helper.

- **Public API**: All `pub` items in library crates should have doc comments explaining what
  they do, not how they're implemented.
- **Examples**: Complex APIs benefit from `/// # Examples` blocks that compile and run as tests.
- **Safety docs**: Every `unsafe` block must have a `// SAFETY:` comment explaining why the
  invariants are upheld. Every `unsafe fn` must document what the caller must guarantee.
- **Panics section**: If a public function can panic, document when with `/// # Panics`.
- **Errors section**: If a function returns `Result`, document the error conditions with
  `/// # Errors`.

### 11. Tooling Checks

As part of the review, run these tools and incorporate their output:

- **`cargo check`**: Verify the code compiles. Report any errors or warnings.
- **`cargo clippy -- -W clippy::pedantic`**: Run clippy with pedantic lints. Don't blindly
  report every lint — assess which ones point to real issues vs noise. Key lints to watch:
  `needless_collect`, `large_enum_variant`, `redundant_allocation`, `manual_map`,
  `missing_errors_doc`, `missing_panics_doc`.
- **`cargo test`**: Run the test suite. Report failures. Note if test coverage seems thin for
  the code under review.
- **rust-analyzer**: Use the LSP for go-to-definition, find-references, and type information
  when you need to trace how a type or function is used across the codebase.

## Output Format

Structure your review as follows:

### Summary

One paragraph: what the code does, overall quality assessment, whether it's ready to ship.

### Critical Issues

Problems that must be fixed before merging. These are bugs, safety issues, or correctness
problems. Each item should include:
- File and line reference
- What the problem is
- Why it matters
- Suggested fix

### Improvements

Things that should be addressed but aren't blocking. Design issues, missing error context,
suboptimal patterns. Same format as critical issues.

### Observations

Minor notes, style suggestions, questions for the author. Keep this section short — if you
have more than 5 items here, some of them probably aren't worth mentioning.

### Edition 2024 Compliance

Only include this section if there are actual edition-related findings. Don't include it just
to say "everything looks fine."

---

Keep the review proportional to the code size. A 50-line change doesn't need a 500-line review.
Focus on what matters most and skip the rest. If the code is genuinely good, say so briefly and
move on — don't manufacture feedback.
