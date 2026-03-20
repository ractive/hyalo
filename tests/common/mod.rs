use assert_cmd::Command;
use std::fs;
use std::path::Path;

/// Returns a `Command` pre-configured to run the `hyalo` binary built by Cargo.
pub fn hyalo() -> Command {
    Command::cargo_bin("hyalo").unwrap()
}

/// Writes a file at `relative_path` inside `dir`, creating parent directories as needed.
pub fn write_md(dir: &Path, relative_path: &str, content: &str) {
    let full = dir.join(relative_path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

/// Write a markdown file with YAML frontmatter containing the given tags.
/// Used by tag-related tests and any future tests that need pre-tagged files.
#[allow(dead_code)]
pub fn write_tagged(dir: &Path, name: &str, tags: &[&str]) {
    let tags_yaml = if tags.is_empty() {
        "tags: []\n".to_owned()
    } else {
        let items: String = tags.iter().map(|t| format!("  - {t}\n")).collect();
        format!("tags:\n{items}")
    };
    write_md(
        dir,
        name,
        &format!("---\ntitle: {name}\n{tags_yaml}---\n# Body\n"),
    );
}

/// Returns a sample markdown document with YAML frontmatter containing various property types.
#[allow(dead_code)]
pub fn sample_frontmatter() -> &'static str {
    "---\n\
     title: My Note\n\
     priority: 3\n\
     draft: true\n\
     created: \"2026-03-20\"\n\
     updated: \"2026-03-20T14:30:00\"\n\
     tags:\n\
       - rust\n\
       - cli\n\
     ---\n\
     # Body\n\
     \n\
     Some content here.\n"
}
