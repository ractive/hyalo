/// CWD-aware help banner for `hyalo --help`.
///
/// Returns a one-line contextual notice when the process working directory has
/// a detectable relationship to a `.hyalo.toml` config file:
///
/// - CWD itself contains `.hyalo.toml` → info banner explaining which dir is active.
/// - CWD is inside the configured vault (`.hyalo.toml` in an ancestor) → warning
///   banner telling the user to run from the project root instead.
/// - Otherwise → `None` (no banner, keeps help clean in unrelated directories).
///
/// The public entry-point reads the process CWD; the inner `_for` variant accepts
/// an explicit path so unit tests can exercise it without mutating the process state.
pub(crate) fn cwd_help_banner() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let is_tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    cwd_help_banner_for_tty(&cwd, is_tty)
}

/// Inner implementation that accepts an explicit CWD path.
///
/// Factored out so unit tests can pass any directory without changing the process
/// working directory.
#[cfg(test)]
pub(crate) fn cwd_help_banner_for(cwd: &std::path::Path) -> Option<String> {
    cwd_help_banner_for_tty(cwd, true)
}

/// Variant that controls emoji rendering based on TTY.
///
/// When `is_tty` is `false` (stdout is piped/redirected), emoji prefixes are
/// suppressed so that `hyalo --help | cat` produces clean ASCII text. The
/// banner content is otherwise unchanged.
pub(crate) fn cwd_help_banner_for_tty(cwd: &std::path::Path, is_tty: bool) -> Option<String> {
    let info_prefix = if is_tty { "\u{2139}\u{fe0f}  " } else { "" };
    let warn_prefix = if is_tty { "\u{26a0}\u{fe0f}  " } else { "" };

    // Case 1: CWD contains .hyalo.toml — banner tells the user which dir is active.
    let local_toml = cwd.join(".hyalo.toml");
    if local_toml.is_file() {
        // Use the same loader as the rest of the CLI so the banner reflects
        // the effective `dir` (and falls back gracefully on malformed configs).
        let dir_value = crate::config::load_config_from(cwd)
            .dir
            .display()
            .to_string();
        return Some(format!(
            "{info_prefix}hyalo runs against `{dir_value}` (from ./.hyalo.toml). \
             Don't `cd` into it; pass paths relative to `{dir_value}`.\n"
        ));
    }

    // Case 2: CWD is inside a vault configured by an ancestor's .hyalo.toml.
    // Walk strict ancestors (skip CWD itself — handled above).
    let cwd_canonical = dunce::canonicalize(cwd).ok()?;
    let mut current: Option<&std::path::Path> = cwd_canonical.parent();
    while let Some(ancestor) = current {
        let toml_path = ancestor.join(".hyalo.toml");
        if toml_path.is_file() {
            if let Some(banner) = check_inside_vault(&cwd_canonical, ancestor, warn_prefix) {
                return Some(banner);
            }
            // Closest ancestor config found; stop walking even if no banner.
            return None;
        }
        current = ancestor.parent();
    }

    None
}

/// Check whether `cwd_canonical` is inside the vault configured by the `.hyalo.toml`
/// in `config_dir`. Returns the warning banner string if so, or `None` if not
/// inside the vault (or on any error).
fn check_inside_vault(
    cwd_canonical: &std::path::Path,
    config_dir: &std::path::Path,
    warn_prefix: &str,
) -> Option<String> {
    // Use the real config loader so we get the same `dir` resolution (and the
    // same malformed-file fallback) as the main CLI.
    let resolved = crate::config::load_config_from(config_dir);
    let dir_value = resolved.dir;

    // dir = "." means the vault root IS the config dir — CWD in an ancestor can never
    // be "inside" it in the misuse sense.
    if dir_value
        .components()
        .eq(std::path::Path::new(".").components())
    {
        return None;
    }

    let vault_path = config_dir.join(&dir_value);
    let vault_canonical = dunce::canonicalize(&vault_path).ok()?;

    if cwd_canonical.starts_with(&vault_canonical) {
        let repo_root = config_dir.display();
        Some(format!(
            "{warn_prefix}You are inside the kb folder. \
             Run hyalo from `{repo_root}` instead — `dir` is auto-resolved from .hyalo.toml.\n"
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn make_temp() -> tempfile::TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn no_config_returns_none() {
        let tmp = make_temp();
        let result = cwd_help_banner_for(tmp.path());
        assert!(
            result.is_none(),
            "expected None for directory without config"
        );
    }

    #[test]
    fn cwd_has_config_with_explicit_dir() {
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("kb")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

        let result = cwd_help_banner_for(tmp.path()).expect("expected Some banner");
        assert!(
            result.contains("runs against `kb`"),
            "expected info banner mentioning 'kb', got: {result}"
        );
        assert!(
            result.contains("from ./.hyalo.toml"),
            "expected config source hint, got: {result}"
        );
    }

    #[test]
    fn cwd_has_config_with_default_dir() {
        // dir = "." (or absent) — banner should still mention "."
        let tmp = make_temp();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();

        let result = cwd_help_banner_for(tmp.path()).expect("expected Some banner");
        assert!(
            result.contains("runs against `.`"),
            "expected info banner mentioning '.', got: {result}"
        );
    }

    #[test]
    fn cwd_inside_vault_ancestor_has_config() {
        let tmp = make_temp();
        // Ancestor contains .hyalo.toml pointing to kb/
        fs::create_dir_all(tmp.path().join("kb")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

        // CWD is inside the vault
        let cwd = tmp.path().join("kb");

        let result = cwd_help_banner_for(&cwd).expect("expected Some warning banner");
        assert!(
            result.contains("inside the kb folder"),
            "expected inside-vault warning, got: {result}"
        );
        assert!(
            result.contains("Run hyalo from"),
            "expected run-from hint, got: {result}"
        );
    }

    #[test]
    fn cwd_inside_vault_subdir() {
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("kb/sub")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

        // CWD is a subdirectory inside the vault
        let cwd = tmp.path().join("kb/sub");

        let result = cwd_help_banner_for(&cwd).expect("expected Some warning banner");
        assert!(
            result.contains("inside the kb folder"),
            "expected inside-vault warning, got: {result}"
        );
    }

    #[test]
    fn cwd_sibling_of_vault_returns_none() {
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("kb")).unwrap();
        fs::create_dir_all(tmp.path().join("other")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

        // CWD is sibling — not inside vault
        let cwd = tmp.path().join("other");
        let result = cwd_help_banner_for(&cwd);
        assert!(
            result.is_none(),
            "expected None for sibling of vault, got: {result:?}"
        );
    }

    #[test]
    fn info_banner_omits_emoji_when_not_tty() {
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("kb")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

        let tty = cwd_help_banner_for_tty(tmp.path(), true).expect("tty banner");
        let piped = cwd_help_banner_for_tty(tmp.path(), false).expect("piped banner");

        assert!(tty.contains('\u{2139}'), "TTY banner should keep emoji");
        assert!(
            !piped.contains('\u{2139}') && !piped.contains('\u{26a0}'),
            "piped banner must drop emoji prefixes, got: {piped:?}"
        );
        assert!(
            piped.contains("runs against `kb`"),
            "piped banner keeps text, got: {piped:?}"
        );
    }

    #[test]
    fn warn_banner_omits_emoji_when_not_tty() {
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("kb")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
        let cwd = tmp.path().join("kb");

        let tty = cwd_help_banner_for_tty(&cwd, true).expect("tty warn banner");
        let piped = cwd_help_banner_for_tty(&cwd, false).expect("piped warn banner");

        assert!(tty.contains('\u{26a0}'), "TTY warn banner keeps emoji");
        assert!(
            !piped.contains('\u{26a0}') && !piped.contains('\u{2139}'),
            "piped warn banner drops emoji, got: {piped:?}"
        );
        assert!(
            piped.contains("inside the kb folder"),
            "piped warn banner keeps text, got: {piped:?}"
        );
    }

    #[test]
    fn dir_dot_does_not_produce_inside_vault_banner() {
        // When dir = ".", the vault IS the project root. Being inside an ancestor
        // of the project root doesn't trigger the inside-vault banner.
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();

        // CWD is inside "sub" — ancestor has dir = "." so no inside-vault banner
        let cwd = tmp.path().join("sub");
        let result = cwd_help_banner_for(&cwd);
        assert!(
            result.is_none(),
            "expected None when dir='.'; got: {result:?}"
        );
    }
}
