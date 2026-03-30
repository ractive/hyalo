use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Raw deserialized representation of `.hyalo.toml`.
///
/// All fields are optional so that a partial config file is valid.
/// Unknown fields are rejected via `deny_unknown_fields` so that typos
/// are caught early rather than silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    dir: Option<String>,
    format: Option<String>,
    hints: Option<bool>,
    /// Explicit override for the site prefix used when resolving absolute links
    /// (e.g. `/docs/page.md`).  When set, this takes precedence over the
    /// auto-derived value (last component of the resolved `dir`).
    site_prefix: Option<String>,
}

/// Resolved configuration with all defaults applied.
#[derive(Debug, PartialEq)]
pub struct ResolvedDefaults {
    pub dir: PathBuf,
    pub format: String,
    pub hints: bool,
    /// Explicit site-prefix override from `.hyalo.toml`, if any.
    pub site_prefix: Option<String>,
}

impl ResolvedDefaults {
    fn hardcoded() -> Self {
        Self {
            dir: PathBuf::from("."),
            format: "json".to_owned(),
            hints: true,
            site_prefix: None,
        }
    }
}

/// Load configuration from `.hyalo.toml` in the current working directory.
///
/// Missing file → silent, returns hardcoded defaults.
/// I/O error (not NotFound) → prints a warning, returns defaults.
/// Malformed TOML or unknown fields → prints a warning, returns defaults.
/// Valid config → merges with defaults (config values take precedence).
pub fn load_config() -> ResolvedDefaults {
    match std::env::current_dir() {
        Ok(cwd) => load_config_from(&cwd),
        Err(e) => {
            crate::warn::warn(format!(
                "could not determine current directory to locate .hyalo.toml: {e}"
            ));
            ResolvedDefaults::hardcoded()
        }
    }
}

/// Load configuration from `.hyalo.toml` inside `dir`.
///
/// This variant accepts an explicit directory to make it testable without
/// relying on the process working directory.
pub fn load_config_from(dir: &Path) -> ResolvedDefaults {
    let path = dir.join(".hyalo.toml");

    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ResolvedDefaults::hardcoded();
        }
        Err(e) => {
            crate::warn::warn(format!("could not read .hyalo.toml: {e}"));
            return ResolvedDefaults::hardcoded();
        }
    };

    let cfg: ConfigFile = match toml::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            crate::warn::warn(format!("malformed .hyalo.toml: {e}"));
            return ResolvedDefaults::hardcoded();
        }
    };

    let defaults = ResolvedDefaults::hardcoded();
    ResolvedDefaults {
        dir: cfg.dir.map(PathBuf::from).unwrap_or(defaults.dir),
        format: cfg.format.unwrap_or(defaults.format),
        hints: cfg.hints.unwrap_or(defaults.hints),
        site_prefix: cfg.site_prefix,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn make_temp() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn missing_config_returns_defaults() {
        let dir = make_temp();
        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::hardcoded());
    }

    #[test]
    fn valid_full_config() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            r#"
dir = "notes"
format = "text"
hints = true
"#,
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.dir, PathBuf::from("notes"));
        assert_eq!(resolved.format, "text");
        assert!(resolved.hints);
        assert_eq!(resolved.site_prefix, None);
    }

    #[test]
    fn site_prefix_config() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            r#"dir = "docs"
site_prefix = "docs"
"#,
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.dir, PathBuf::from("docs"));
        assert_eq!(resolved.site_prefix, Some("docs".to_owned()));
    }

    #[test]
    fn partial_config_merges_with_defaults() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "hints = false\n").unwrap();

        let resolved = load_config_from(dir.path());
        // Only hints overridden; dir and format stay at defaults.
        assert_eq!(resolved.dir, PathBuf::from("."));
        assert_eq!(resolved.format, "json");
        assert!(
            !resolved.hints,
            "config should override the default (true) to false"
        );
    }

    #[test]
    fn malformed_toml_returns_defaults() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "this is not { valid toml").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::hardcoded());
    }

    #[test]
    fn unknown_fields_returns_defaults() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "unknown_key = \"value\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::hardcoded());
    }

    #[test]
    fn invalid_format_value_passed_through() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "format = \"xml\"\n").unwrap();

        // config.rs does not validate the format string — that is the caller's job.
        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.format, "xml");
        assert_eq!(resolved.dir, PathBuf::from("."));
        assert!(resolved.hints);
    }
}
