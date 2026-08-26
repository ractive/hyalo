//! Conformance-profile lint rules (ARCH-2, iter-226).
//!
//! Historically these lived in `hyalo-cli/src/commands/` as a hidden lint
//! subsystem; iter-226 moved them into the `hyalo-mdlint` crate so lint
//! logic is reusable by library consumers of hyalo-core and unit-testable
//! in-process. The CLI keeps only flag parsing and output formatting.
//!
//! Profiles compose: `hyalo lint --profile okf --profile madr` runs every
//! listed profile's advisory rules in one pass. Each module exposes a
//! `run_*_rules` entry point returning plain finding structs; the CLI's
//! lint pipeline converts them into its output shape.
//!
//! - [`changelog`] — Keep a Changelog conformance (`--profile changelog`)
//! - [`madr`] — Nygard/MADR ADR conformance (`--profile madr`)
//! - [`okf`] — Open Knowledge Format advisory rules (`--profile okf`)
//! - [`skills`] — SKILL.md frontmatter conformance (`--profile skills`)
//! - [`github`] — GitHub Actions annotation rendering for `--format github`
//! - [`link`] — HYALO006 broken-link detection context
//! - [`heading_grammar`] — declarative ATX-heading-shape engine (shared by
//!   the changelog profile)
//! - [`section_scanner`] — fenced-code-aware outline/task scanner used by
//!   schema linting and `hyalo task`

pub mod changelog;
pub mod github;
pub mod heading_grammar;
pub mod link;
pub mod madr;
pub mod okf;
pub mod section_scanner;
pub mod skills;
