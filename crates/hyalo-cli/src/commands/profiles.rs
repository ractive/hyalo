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
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

/// The OKF `.hyalo.toml` fragment, embedded at compile time.
const OKF_PROFILE_TOML: &str = include_str!("../../templates/profile-okf.toml");

/// The bundled OKF skill body, embedded at compile time.
const OKF_SKILL_CONTENT: &str = include_str!("../../templates/skill-hyalo-okf.md");

/// The MADR `.hyalo.toml` fragment, embedded at compile time.
const MADR_PROFILE_TOML: &str = include_str!("../../templates/profile-madr.toml");

/// The bundled MADR skill body, embedded at compile time.
const MADR_SKILL_CONTENT: &str = include_str!("../../templates/skill-hyalo-madr.md");

/// The Agent Skills `.hyalo.toml` fragment, embedded at compile time.
const SKILLS_PROFILE_TOML: &str = include_str!("../../templates/profile-skills.toml");

/// The bundled Agent Skills skill body, embedded at compile time.
const SKILLS_SKILL_CONTENT: &str = include_str!("../../templates/skill-hyalo-skills.md");

/// The Keep a Changelog `.hyalo.toml` fragment, embedded at compile time.
const CHANGELOG_PROFILE_TOML: &str = include_str!("../../templates/profile-changelog.toml");

/// The bundled changelog skill body, embedded at compile time.
const CHANGELOG_SKILL_CONTENT: &str = include_str!("../../templates/skill-hyalo-changelog.md");

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
    Profile {
        name: "skills",
        description: "Agent Skills (SKILL.md): path-bound `skill` schema, `name` slug \
                      (regex/length/reserved-words/dirname coupling), `description` bounds, \
                      500-line body budget",
        toml_fragment: SKILLS_PROFILE_TOML,
        skills: &[("skills", SKILLS_SKILL_CONTENT)],
    },
    Profile {
        name: "changelog",
        description: "Keep a Changelog 1.1.0 (CHANGELOG.md): path-bound frontmatter-less \
                      `changelog` type, strict heading grammar (version/date ordering, \
                      categories, footer link refs), `changelog release` rotation",
        toml_fragment: CHANGELOG_PROFILE_TOML,
        skills: &[("changelog", CHANGELOG_SKILL_CONTENT)],
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

/// A scalar key whose value changed when a profile fragment overwrote it during
/// a merge. Surfaced so callers can print a `conflict:` line to stderr — no
/// value is ever silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Dotted key path (e.g. `site_prefix`, `schema.default.required`).
    pub key: String,
    /// The previous value's TOML rendering.
    pub old: String,
    /// The new value's TOML rendering (from the profile fragment).
    pub new: String,
}

impl Conflict {
    /// The stderr line for this conflict, e.g.
    /// `conflict: site_prefix "" -> "docs" (profile okf)`.
    #[must_use]
    pub fn line(&self, profile_name: &str) -> String {
        format!(
            "conflict: {} {} -> {} (profile {profile_name})",
            self.key, self.old, self.new
        )
    }
}

/// Array-valued keys (identified by dotted path) whose entries *union* on merge
/// instead of being replaced, so composing profiles never shrinks a list a
/// previous profile (or the user by hand) contributed. Order is stable:
/// existing entries first, new ones appended.
const UNION_ARRAY_KEYS: &[&str] = &[
    "schema.exempt",
    "lint.ignore",
    "schema.default.required",
    "lint.profiles",
];

/// Deep-merge the profile's TOML fragment into an existing (or empty) config
/// document and return the serialised result, discarding any scalar conflicts.
///
/// Prefer [`merge_into_config_with_conflicts`] when the caller wants to surface
/// changed-scalar warnings. See that function for the full merge semantics.
pub fn merge_into_config(existing_raw: &str, fragment: &str) -> Result<String> {
    Ok(merge_into_config_with_conflicts(existing_raw, fragment)?.0)
}

/// Deep-merge the profile's TOML fragment into an existing (or empty) config
/// document, returning `(merged_toml, conflicts)`.
///
/// Merge semantics (upsert, never clobber other profiles' config):
/// - Tables are merged recursively; hand-written comments and key order in the
///   existing document survive (the merge runs on a `toml_edit::DocumentMut`).
/// - Array keys in [`UNION_ARRAY_KEYS`] **union**: existing entries are kept in
///   order and new fragment entries appended (deduplicated by value).
/// - `[[schema.bind]]` array-of-tables entries union, deduplicated by
///   `(glob, type)`, preserving declaration order (first-match-wins).
/// - Other scalars/arrays from the fragment *overwrite* the existing value — a
///   profile owns its own keys, so re-running refreshes them. When an overwrite
///   *changes* an existing differing value, a [`Conflict`] is recorded so the
///   caller can warn; nothing is dropped silently.
///
/// `existing_raw` is the current `.hyalo.toml` contents (may be empty).
pub fn merge_into_config_with_conflicts(
    existing_raw: &str,
    fragment: &str,
) -> Result<(String, Vec<Conflict>)> {
    let mut base: DocumentMut = if existing_raw.trim().is_empty() {
        DocumentMut::new()
    } else {
        existing_raw
            .parse::<DocumentMut>()
            .context("existing .hyalo.toml is not valid TOML; refusing to merge profile into it")?
    };
    let overlay: DocumentMut = fragment
        .parse::<DocumentMut>()
        .context("embedded profile TOML fragment is not valid TOML")?;

    let mut conflicts = Vec::new();
    merge_table(base.as_table_mut(), overlay.as_table(), "", &mut conflicts);

    Ok((base.to_string(), conflicts))
}

/// Recursively merge `overlay` into `base`. `path` is the dotted key prefix of
/// `base` (empty at the root) used to classify union-array keys and to build
/// conflict key paths.
fn merge_table(base: &mut Table, overlay: &Table, path: &str, conflicts: &mut Vec<Conflict>) {
    for (key, overlay_item) in overlay {
        let child_path = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        merge_item(base, key, overlay_item, &child_path, conflicts);
    }
}

/// Merge a single `overlay_item` into `base` at `key` (whose full dotted path is
/// `child_path`).
fn merge_item(
    base: &mut Table,
    key: &str,
    overlay_item: &Item,
    child_path: &str,
    conflicts: &mut Vec<Conflict>,
) {
    match (base.get_mut(key), overlay_item) {
        // Both sides are tables — recurse.
        (Some(Item::Table(base_tbl)), Item::Table(overlay_tbl)) => {
            merge_table(base_tbl, overlay_tbl, child_path, conflicts);
        }
        // Both sides are array-of-tables (`[[schema.bind]]`) — union with dedup.
        (Some(Item::ArrayOfTables(base_aot)), Item::ArrayOfTables(overlay_aot)) => {
            union_array_of_tables(base_aot, overlay_aot);
        }
        // Both sides are inline arrays and this is a union key — union entries.
        (Some(Item::Value(Value::Array(base_arr))), Item::Value(Value::Array(overlay_arr)))
            if UNION_ARRAY_KEYS.contains(&child_path) =>
        {
            union_array(base_arr, overlay_arr);
        }
        // Existing scalar/array replaced by the fragment's value.
        (Some(existing), _) => {
            // When both sides are scalar values and they are already equal,
            // leave the existing item untouched — replacing it would reset the
            // key's decor (comments/blank lines), breaking idempotency and
            // comment preservation on re-run.
            if let (Item::Value(old_val), Item::Value(new_val)) = (&*existing, overlay_item)
                && values_equal(old_val, new_val)
            {
                return;
            }
            // Record a conflict when a scalar value actually changes (arrays are
            // handled by the union arm above or are profile-owned, so we don't
            // diff them). Nothing is dropped silently.
            if let (Item::Value(old_val), Item::Value(new_val)) = (&*existing, overlay_item)
                && old_val.as_array().is_none()
                && new_val.as_array().is_none()
            {
                conflicts.push(Conflict {
                    key: child_path.to_owned(),
                    old: render_value(old_val),
                    new: render_value(new_val),
                });
            }
            base.insert(key, overlay_item.clone());
        }
        // Key absent in base — insert as-is.
        (None, _) => {
            base.insert(key, overlay_item.clone());
        }
    }
}

/// Union `overlay` array entries into `base`, appending only entries not already
/// present (compared by rendered value), preserving existing order.
fn union_array(base: &mut Array, overlay: &Array) {
    for item in overlay {
        let already = base.iter().any(|existing| values_equal(existing, item));
        if !already {
            base.push_formatted(item.clone());
        }
    }
    // Re-space the array so multi-entry unions render one-per-line consistently
    // rather than inheriting a mix of the two sources' whitespace.
    base.fmt();
}

/// Union `[[schema.bind]]` tables, deduplicating by `(glob, type)` and keeping
/// existing declaration order (first-match-wins), appending only new entries.
fn union_array_of_tables(base: &mut ArrayOfTables, overlay: &ArrayOfTables) {
    for tbl in overlay {
        let key = bind_key(tbl);
        let already = base.iter().any(|existing| bind_key(existing) == key);
        if !already {
            base.push(tbl.clone());
        }
    }
}

/// Identity of a `[[schema.bind]]` table for dedup: `(glob, type)`.
fn bind_key(tbl: &Table) -> (String, String) {
    let glob = tbl
        .get("glob")
        .and_then(Item::as_str)
        .unwrap_or_default()
        .to_owned();
    let ty = tbl
        .get("type")
        .and_then(Item::as_str)
        .unwrap_or_default()
        .to_owned();
    (glob, ty)
}

/// Compare two `toml_edit` values for semantic equality by rendered form. This
/// ignores decor (whitespace/comments), which is exactly what we want for
/// dedup and conflict detection.
fn values_equal(a: &Value, b: &Value) -> bool {
    render_value(a) == render_value(b)
}

/// Render a `toml_edit` value to its trimmed TOML form (decor stripped) for
/// display and comparison.
fn render_value(v: &Value) -> String {
    // `Value`'s Display includes leading/trailing decor; a clone with default
    // decor renders just the value token(s).
    let cleaned = v.clone().decorated("", "");
    cleaned.to_string().trim().to_owned()
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
    fn lookup_finds_changelog() {
        assert_eq!(lookup("changelog").unwrap().name, "changelog");
    }

    #[test]
    fn changelog_fragment_is_valid_toml() {
        let _: toml::Table =
            toml::from_str(CHANGELOG_PROFILE_TOML).expect("changelog fragment must parse");
    }

    #[test]
    fn changelog_fragment_binds_literal_path() {
        // The fragment must produce a valid SchemaConfig with a `changelog` type,
        // a literal-path bind, and CHANGELOG.md exempt from frontmatter rules.
        let merged = merge_into_config("", CHANGELOG_PROFILE_TOML).unwrap();
        let val: toml::Value = toml::from_str(&merged).unwrap();
        let raw: hyalo_core::schema::RawSchemaConfig = val
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .expect("schema section present");
        let cfg = hyalo_core::schema::SchemaConfig::try_from(raw).expect("valid schema");
        assert!(
            cfg.types.contains_key("changelog"),
            "changelog type declared"
        );
        assert_eq!(
            cfg.bound_type_for("CHANGELOG.md"),
            Some("changelog"),
            "literal bind resolves CHANGELOG.md to the changelog type"
        );
        assert!(
            cfg.exempt.is_exempt("CHANGELOG.md"),
            "CHANGELOG.md is exempt"
        );
        assert!(
            cfg.unknown_bind_targets().is_empty(),
            "bind target must reference a declared type"
        );
    }

    #[test]
    fn changelog_declares_a_skill() {
        let cl = lookup("changelog").unwrap();
        assert_eq!(cl.skills.len(), 1);
        assert_eq!(cl.skills[0].0, "changelog");
        assert!(!cl.skills[0].1.is_empty());
    }

    #[test]
    fn changelog_composes_with_others() {
        // All profiles applied to one vault must coexist.
        let a = merge_into_config("", OKF_PROFILE_TOML).unwrap();
        let b = merge_into_config(&a, MADR_PROFILE_TOML).unwrap();
        let c = merge_into_config(&b, CHANGELOG_PROFILE_TOML).unwrap();
        let parsed: toml::Table = toml::from_str(&c).unwrap();
        let types = parsed["schema"]["types"].as_table().unwrap();
        assert!(types.contains_key("changelog"));
        assert!(types.contains_key("adr"));
        assert!(types.contains_key("BigQuery Table"));
    }

    // ---------------------------------------------------------------------------
    // iter-172: smart-merge composition semantics
    // ---------------------------------------------------------------------------

    #[test]
    fn four_profile_stack_unions_all_binds_and_exempt() {
        // RB-1 reconciliation: composing okf+madr+skills+changelog into one
        // vault must keep EVERY bind and the UNIONED exempt list — no profile's
        // contributions may be clobbered by a later one.
        let a = merge_into_config("", OKF_PROFILE_TOML).unwrap();
        let b = merge_into_config(&a, MADR_PROFILE_TOML).unwrap();
        let c = merge_into_config(&b, SKILLS_PROFILE_TOML).unwrap();
        let d = merge_into_config(&c, CHANGELOG_PROFILE_TOML).unwrap();

        let val: toml::Value = toml::from_str(&d).unwrap();
        let raw: hyalo_core::schema::RawSchemaConfig = val
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .expect("schema section present");
        let cfg = hyalo_core::schema::SchemaConfig::try_from(raw).expect("valid composed schema");

        // Every profile's bind survives and resolves.
        assert_eq!(
            cfg.bound_type_for("docs/decisions/0001-x.md"),
            Some("adr"),
            "madr bind survives the stack"
        );
        assert_eq!(
            cfg.bound_type_for("CHANGELOG.md"),
            Some("changelog"),
            "changelog literal bind survives the stack"
        );
        // The skills profile binds SKILL.md files.
        assert!(
            cfg.bound_type_for(".claude/skills/foo/SKILL.md").is_some(),
            "skills bind survives the stack"
        );

        // Unioned exempt: OKF's `**/index.md` and changelog's CHANGELOG.md both
        // present, and none dropped.
        assert!(cfg.exempt.is_exempt("bundle/index.md"), "OKF exempt kept");
        assert!(
            cfg.exempt.is_exempt("CHANGELOG.md"),
            "changelog exempt kept"
        );

        // The active-profiles list unions all four.
        let profiles = val["lint"]["profiles"].as_array().unwrap();
        let names: Vec<&str> = profiles.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"okf"), "okf active: {names:?}");
        assert!(names.contains(&"madr"), "madr active: {names:?}");
        assert!(names.contains(&"skills"), "skills active: {names:?}");
        assert!(names.contains(&"changelog"), "changelog active: {names:?}");
    }

    #[test]
    fn exempt_array_unions_with_user_additions() {
        // A user hand-adds an exempt glob; applying OKF must keep it AND add the
        // OKF entries — arrays never shrink.
        let existing = "\
[schema]
exempt = [\"my/private/**\"]
";
        let merged = merge_into_config(existing, OKF_PROFILE_TOML).unwrap();
        let val: toml::Value = toml::from_str(&merged).unwrap();
        let exempt: Vec<&str> = val["schema"]["exempt"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            exempt.contains(&"my/private/**"),
            "user entry kept: {exempt:?}"
        );
        assert!(
            exempt.contains(&"**/index.md"),
            "okf entry added: {exempt:?}"
        );
        assert!(exempt.contains(&"**/log.md"), "okf entry added: {exempt:?}");
    }

    #[test]
    fn required_array_unions_user_and_profile() {
        // `[schema.default] required`: user `["title", "type"]` + okf `["type"]`
        // → both survive, no duplicate `type`.
        let existing = "\
[schema.default]
required = [\"title\", \"type\"]
";
        let merged = merge_into_config(existing, OKF_PROFILE_TOML).unwrap();
        let val: toml::Value = toml::from_str(&merged).unwrap();
        let required: Vec<&str> = val["schema"]["default"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(
            required,
            vec!["title", "type"],
            "user order preserved, no duplicate type: {required:?}"
        );
    }

    #[test]
    fn bind_entries_dedup_by_glob_and_type() {
        // Re-applying MADR must not duplicate its bind entry.
        let once = merge_into_config("", MADR_PROFILE_TOML).unwrap();
        let twice = merge_into_config(&once, MADR_PROFILE_TOML).unwrap();
        let val: toml::Value = toml::from_str(&twice).unwrap();
        let binds = val["schema"]["bind"].as_array().unwrap();
        let madr_binds = binds
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("adr"))
            .count();
        assert_eq!(madr_binds, 1, "bind deduped by (glob, type)");
    }

    #[test]
    fn merge_preserves_hand_written_comments_and_order() {
        let existing = "\
# my hand-written header comment
dir = \"vault\"

# a note about site_prefix
site_prefix = \"custom\"
";
        let merged = merge_into_config(existing, MADR_PROFILE_TOML).unwrap();
        assert!(
            merged.contains("# my hand-written header comment"),
            "header comment survived: {merged}"
        );
        assert!(
            merged.contains("# a note about site_prefix"),
            "inline note survived: {merged}"
        );
        // `dir` still precedes the appended profile content (existing order kept).
        let dir_pos = merged.find("dir = \"vault\"").unwrap();
        let madr_pos = merged.find("[[schema.bind]]").unwrap();
        assert!(dir_pos < madr_pos, "existing keys precede appended profile");
    }

    #[test]
    fn scalar_conflict_is_reported_not_silent() {
        // OKF sets site_prefix = ""; an existing differing value is a conflict.
        let existing = "site_prefix = \"custom\"\n";
        let (merged, conflicts) =
            merge_into_config_with_conflicts(existing, OKF_PROFILE_TOML).unwrap();
        assert_eq!(conflicts.len(), 1, "one scalar conflict: {conflicts:?}");
        let c = &conflicts[0];
        assert_eq!(c.key, "site_prefix");
        assert_eq!(c.old, "\"custom\"");
        assert_eq!(c.new, "\"\"");
        assert_eq!(
            c.line("okf"),
            "conflict: site_prefix \"custom\" -> \"\" (profile okf)"
        );
        // The new value did win (profile owns its key).
        let val: toml::Value = toml::from_str(&merged).unwrap();
        assert_eq!(val["site_prefix"].as_str(), Some(""));
    }

    #[test]
    fn no_conflict_when_scalar_unchanged() {
        // Re-applying a profile is conflict-free (idempotent).
        let once = merge_into_config("", OKF_PROFILE_TOML).unwrap();
        let (_twice, conflicts) =
            merge_into_config_with_conflicts(&once, OKF_PROFILE_TOML).unwrap();
        assert!(
            conflicts.is_empty(),
            "idempotent re-run has no conflicts: {conflicts:?}"
        );
    }

    #[test]
    fn array_change_is_not_reported_as_conflict() {
        // Union arrays never conflict — they grow, they don't clobber.
        let existing = "\
[schema]
exempt = [\"user/only.md\"]
";
        let (_merged, conflicts) =
            merge_into_config_with_conflicts(existing, OKF_PROFILE_TOML).unwrap();
        assert!(
            conflicts.iter().all(|c| c.key != "schema.exempt"),
            "array union is not a conflict: {conflicts:?}"
        );
    }

    #[test]
    fn four_profile_stack_is_idempotent() {
        // Re-running the whole stack yields byte-identical config.
        let build = |base: &str| {
            let a = merge_into_config(base, OKF_PROFILE_TOML).unwrap();
            let b = merge_into_config(&a, MADR_PROFILE_TOML).unwrap();
            let c = merge_into_config(&b, SKILLS_PROFILE_TOML).unwrap();
            merge_into_config(&c, CHANGELOG_PROFILE_TOML).unwrap()
        };
        let once = build("");
        let twice = build(&once);
        assert_eq!(once, twice, "re-running the full stack is byte-idempotent");
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
