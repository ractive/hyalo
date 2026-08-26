//! hyalo-core — the library behind the `hyalo` CLI.
//!
//! ## Supported surface (ARCH-5 façade, iter-225)
//!
//! This crate deliberately exposes a **curated façade** rather than every
//! internal module:
//!
//! - **Domain modules** stay `pub` and are the supported surface:
//!   [`schema`], [`index`], [`filter`], [`frontmatter`], [`discovery`],
//!   [`types`], [`tasks`], [`heading`], [`anchor`], [`links`] (and the
//!   link-fixing family: [`auto_link`], [`link_fix`], [`link_graph`],
//!   [`link_rewrite`], [`link_score`]), [`bm25`], [`content_search`],
//!   [`filename_template`], [`iteration_id`], [`scanner`].
//! - **Plumbing modules** ([`case_index`], [`common_words`], [`fs_util`],
//!   `util`, `warn`) are `pub(crate)`; the handful of items the CLI needs
//!   cross-crate are re-exported at the root below. This keeps every
//!   internal refactor of those modules semver-cheap — before external
//!   consumers appear, demoting a module is a minor change; afterwards it
//!   would be breaking.
//!
//! Invariants that used to live only in doc comments are now structural: a
//! caller outside this crate literally cannot reach `hyalo_core::fs_util::*`
//! — only the re-exported boundary items.

pub mod anchor;
pub mod auto_link;
pub mod bm25;
pub(crate) mod case_index;
pub(crate) mod common_words;
pub mod content_search;
pub mod discovery;
pub mod filename_template;
pub mod filter;
pub mod frontmatter;
pub(crate) mod fs_util;
pub mod heading;
pub mod index;
pub mod iteration_id;
pub mod link_fix;
pub mod link_graph;
pub mod link_resolve;
pub mod link_rewrite;
pub mod link_score;
pub mod link_write;
pub mod links;
pub mod scanner;
pub mod schema;
pub mod tasks;
pub mod types;
pub(crate) mod util;
pub(crate) mod warn;

// ---------------------------------------------------------------------------
// Facade re-exports (ARCH-5, iter-225)
// ---------------------------------------------------------------------------
// The specific plumbing items the CLI and hyalo-mdlint consume. Add new
// ones here only when a CLI feature genuinely needs them — do not widen the
// module back to `pub` for convenience.

/// Case-insensitive link resolution (was `hyalo_core::case_index`).
pub use case_index::{
    CaseInsensitiveIndex, CaseInsensitiveMode, mode_enabled, sweep_stale_case_probes,
};
/// Common-word heuristic for `links auto` advisory notes.
pub use common_words::{MIN_COMMON_WORD_LEN, is_common_word};
/// Vault-boundary file writing and refusal messaging (was `hyalo_core::fs_util`).
pub use fs_util::{
    atomic_write_within, escaping_write_target, outside_vault_hint, outside_vault_message,
    outside_vault_message_with_dir,
};
/// Date-validation and string-distance helpers used by CLI lint rules.
pub use util::{is_iso8601_date, is_iso8601_datetime, is_iso8601_datetime_tz, levenshtein};
