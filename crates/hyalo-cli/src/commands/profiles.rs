#![allow(clippy::missing_errors_doc)]
//! Data-driven `hyalo init --profile <name>` support.
//!
//! A *profile* is a named, embedded declarative fragment that turns a plain
//! vault into a ready-to-use flavour (OKF today; `madr`/`skills`/`changelog`
//! queued behind it — see the knowledgebase `profile-candidates-beyond-okf`).
//!
//! Each profile contributes:
//! - a **TOML fragment** (schema + exemptions + `site_prefix` + type defs) that
//!   is *deep-merged* into `.hyalo.toml` — so multiple `--profile` runs coexist
//!   in one vault and re-running upserts rather than clobbers, and
//! - optional **skill files** installed under `.claude/skills/<name>/SKILL.md`
//!   when `--claude` is passed.
//!
//! Profiles are pure data (an embedded `.toml` string plus an optional skill
//! body), never per-profile Rust code paths — adding `madr`/`changelog` is a
//! matter of adding another [`Profile`] entry, not new branches.

use anyhow::{Context, Result};
use toml::Value as TomlValue;

/// The OKF `.hyalo.toml` fragment, embedded at compile time.
const OKF_PROFILE_TOML: &str = include_str!("../../templates/profile-okf.toml");

/// The bundled OKF skill body, embedded at compile time.
const OKF_SKILL_CONTENT: &str = include_str!("../../templates/skill-hyalo-okf.md");

/// The MADR `.hyalo.toml` fragment, embedded at compile time.
const MADR_PROFILE_TOML: &str = include_str!("../../templates/profile-madr.toml");

/// The bundled MADR skill body, embedded at compile time.
const MADR_SKILL_CONTENT: &str = include_str!("../../templates/skill-hyalo-madr.md");

/// A declarative init profile: a TOML fragment plus optional skill files.
#[derive(Debug)]
pub struct Profile {
    /// The canonical profile name accepted on the CLI (e.g. `"okf"`).
    pub name: &'static str,
    /// One-line description used in the "unknown profile" error listing.
    pub description: &'static str,
    /// TOML fragment deep-merged into `.hyalo.toml`.
    pub toml_fragment: &'static str,
    /// Skill files installed under `.claude/skills/<dir>/SKILL.md` with
    /// `--claude`. Each entry is `(skill_dir_name, skill_body)`.
    pub skills: &'static [(&'static str, &'static str)],
}

/// All profiles known to `hyalo init --profile`. Additive by design: new
/// profiles are new entries here, never new match arms elsewhere.
pub const PROFILES: &[Profile] = &[
    Profile {
        name: "okf",
        description: "Open Knowledge Format vault (schema, exemptions, bundle-root links)",
        toml_fragment: OKF_PROFILE_TOML,
        skills: &[("okf", OKF_SKILL_CONTENT)],
    },
    Profile {
        name: "madr",
        description: "Markdown Architecture Decision Records (MADR 4.0.0): path-bound `adr` \
                      schema, status lifecycle, auto-numbered `NNNN-slug.md`",
        toml_fragment: MADR_PROFILE_TOML,
        skills: &[("madr", MADR_SKILL_CONTENT)],
    },
];

/// Look up a profile by name, or return a helpful error listing the available
/// profiles.
pub fn lookup(name: &str) -> Result<&'static Profile> {
    PROFILES.iter().find(|p| p.name == name).with_context(|| {
        let available = PROFILES
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
            .join(", ");
        format!("unknown profile '{name}'. Available profiles: {available}")
    })
}

/// Deep-merge the profile's TOML fragment into an existing (or empty) config
/// table and return the serialised result.
///
/// Merge semantics (upsert, never clobber other profiles' config):
/// - Tables are merged recursively.
/// - Scalars/arrays from the profile fragment *overwrite* the existing value at
///   that key — a profile owns its own keys, so re-running it refreshes them,
///   but keys it does not touch (e.g. another profile's `[schema.types.*]`) are
///   preserved.
///
/// `existing_raw` is the current `.hyalo.toml` contents (may be empty). Returns
/// the merged TOML string ready to write back.
pub fn merge_into_config(existing_raw: &str, fragment: &str) -> Result<String> {
    let mut base: toml::Table = if existing_raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(existing_raw)
            .context("existing .hyalo.toml is not valid TOML; refusing to merge profile into it")?
    };
    let overlay: toml::Table =
        toml::from_str(fragment).context("embedded profile TOML fragment is not valid TOML")?;

    for (key, value) in overlay {
        merge_value(&mut base, key, value);
    }

    toml::to_string(&base).context("failed to serialise merged .hyalo.toml")
}

/// Insert `value` at `key` in `table`, recursively merging when both sides are
/// tables.
fn merge_value(table: &mut toml::Table, key: String, value: TomlValue) {
    match (table.get_mut(&key), value) {
        (Some(TomlValue::Table(existing)), TomlValue::Table(overlay)) => {
            for (k, v) in overlay {
                merge_value(existing, k, v);
            }
        }
        (_, value) => {
            table.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_okf() {
        assert_eq!(lookup("okf").unwrap().name, "okf");
    }

    #[test]
    fn lookup_unknown_lists_available() {
        let err = lookup("bogus").unwrap_err().to_string();
        assert!(err.contains("unknown profile 'bogus'"), "got: {err}");
        assert!(err.contains("okf"), "should list available profiles: {err}");
    }

    #[test]
    fn okf_fragment_is_valid_toml() {
        let _: toml::Table = toml::from_str(OKF_PROFILE_TOML).expect("OKF fragment must parse");
    }

    #[test]
    fn merge_into_empty_config() {
        let merged = merge_into_config("", OKF_PROFILE_TOML).unwrap();
        let parsed: toml::Table = toml::from_str(&merged).unwrap();
        assert_eq!(parsed.get("site_prefix").and_then(|v| v.as_str()), Some(""));
        assert_eq!(
            parsed
                .get("validate_on_write")
                .and_then(toml::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn merge_preserves_existing_dir_key() {
        // A `dir` key set by the standard init flow must survive the merge.
        let existing = "dir = \"my-vault\"\n";
        let merged = merge_into_config(existing, OKF_PROFILE_TOML).unwrap();
        let parsed: toml::Table = toml::from_str(&merged).unwrap();
        assert_eq!(parsed.get("dir").and_then(|v| v.as_str()), Some("my-vault"));
        // And the OKF keys were added.
        assert!(parsed.contains_key("site_prefix"));
        assert!(parsed.get("schema").is_some());
    }

    #[test]
    fn merge_is_idempotent() {
        // Running the same profile twice yields the same config.
        let once = merge_into_config("", OKF_PROFILE_TOML).unwrap();
        let twice = merge_into_config(&once, OKF_PROFILE_TOML).unwrap();
        assert_eq!(once, twice, "re-running a profile must be idempotent");
    }

    #[test]
    fn merge_preserves_foreign_schema_type() {
        // Simulate another profile having added a schema type; the OKF merge
        // must not clobber it (composable profiles).
        let existing = "\
[schema.types.madr]
required = [\"type\", \"status\"]
";
        let merged = merge_into_config(existing, OKF_PROFILE_TOML).unwrap();
        let parsed: toml::Table = toml::from_str(&merged).unwrap();
        let types = parsed["schema"]["types"].as_table().unwrap();
        assert!(types.contains_key("madr"), "foreign type preserved");
        assert!(
            types.contains_key("BigQuery Table"),
            "OKF type added alongside"
        );
    }

    #[test]
    fn merge_rejects_malformed_existing_toml() {
        let err = merge_into_config("this = = broken", OKF_PROFILE_TOML)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not valid TOML"), "got: {err}");
    }

    #[test]
    fn okf_profile_declares_a_skill() {
        let okf = lookup("okf").unwrap();
        assert_eq!(okf.skills.len(), 1);
        assert_eq!(okf.skills[0].0, "okf");
        assert!(!okf.skills[0].1.is_empty());
    }

    #[test]
    fn lookup_finds_madr() {
        assert_eq!(lookup("madr").unwrap().name, "madr");
    }

    #[test]
    fn madr_fragment_is_valid_toml() {
        let _: toml::Table = toml::from_str(MADR_PROFILE_TOML).expect("MADR fragment must parse");
    }

    #[test]
    fn madr_fragment_parses_as_schema() {
        // The fragment must produce a valid SchemaConfig: an `adr` type, a bind
        // entry pointing at it, and a parseable status pattern.
        let merged = merge_into_config("", MADR_PROFILE_TOML).unwrap();
        let val: toml::Value = toml::from_str(&merged).unwrap();
        let raw: hyalo_core::schema::RawSchemaConfig = val
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .expect("schema section present");
        let cfg = hyalo_core::schema::SchemaConfig::try_from(raw).expect("valid schema");
        assert!(cfg.types.contains_key("adr"), "adr type declared");
        assert_eq!(
            cfg.bound_type_for("docs/decisions/0001-x.md"),
            Some("adr"),
            "bind resolves ADR files to the adr type"
        );
        assert!(
            cfg.unknown_bind_targets().is_empty(),
            "bind target must reference a declared type"
        );
    }

    #[test]
    fn madr_declares_a_skill() {
        let madr = lookup("madr").unwrap();
        assert_eq!(madr.skills.len(), 1);
        assert_eq!(madr.skills[0].0, "madr");
        assert!(!madr.skills[0].1.is_empty());
    }

    #[test]
    fn okf_and_madr_compose() {
        // Both profiles applied to one vault must coexist (composable).
        let once = merge_into_config("", OKF_PROFILE_TOML).unwrap();
        let both = merge_into_config(&once, MADR_PROFILE_TOML).unwrap();
        let parsed: toml::Table = toml::from_str(&both).unwrap();
        let types = parsed["schema"]["types"].as_table().unwrap();
        assert!(types.contains_key("adr"), "madr adr type present");
        assert!(
            types.contains_key("BigQuery Table"),
            "okf types preserved alongside madr"
        );
    }
}
