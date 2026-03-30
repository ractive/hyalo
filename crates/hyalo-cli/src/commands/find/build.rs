use hyalo_core::filter::{FindTaskFilter, parse_regex_pattern};
use hyalo_core::types::{FindTaskInfo, OutlineSection};

use crate::output::CommandOutcome;
use crate::warn;

/// Extract the title value for `--fields title`.
///
/// Priority:
/// 1. `title` frontmatter property (if it is a string)
/// 2. First H1 heading in the document outline
/// 3. `serde_json::Value::Null` if neither found
pub(super) fn extract_title(
    props: &indexmap::IndexMap<String, serde_json::Value>,
    outline_sections: Option<&[OutlineSection]>,
) -> serde_json::Value {
    // 1. Frontmatter title property
    if let Some(serde_json::Value::String(s)) = props.get("title") {
        return serde_json::Value::String(s.clone());
    }
    // 2. First H1 heading from outline
    if let Some(sections) = outline_sections {
        for sec in sections {
            if sec.level == 1
                && let Some(ref heading) = sec.heading
            {
                return serde_json::Value::String(heading.clone());
            }
        }
    }
    serde_json::Value::Null
}

/// Pre-compiled title filter — avoids per-file regex compilation and repeated
/// `to_lowercase()` allocation.
pub(super) enum TitleMatcher {
    /// Case-insensitive substring: stores the lowered pattern.
    Substring(String),
    /// Pre-compiled case-insensitive regex.
    Regex(regex::Regex),
}

impl TitleMatcher {
    /// Parse a `--title` value into a compiled matcher.
    ///
    /// Supports:
    /// - Plain text: case-insensitive substring match
    /// - `~=pattern`: bare regex (always case-insensitive)
    /// - `~=/pattern/`: delimited regex (case-insensitive by default)
    /// - `~=/pattern/i`: delimited regex with explicit flags
    ///
    /// Returns `Err(CommandOutcome::UserError(...))` on invalid regex.
    pub(super) fn parse(pattern: &str) -> Result<Self, CommandOutcome> {
        if let Some(regex_pat) = pattern.strip_prefix("~=") {
            if regex_pat.starts_with('/') {
                // Delimited form: reuse the same /pattern/flags parser as
                // --property filters, but default to case-insensitive.
                match parse_regex_pattern(regex_pat) {
                    Ok(re) => {
                        // If no explicit 'i' flag was given, wrap with (?i).
                        // Detect by checking if the regex already matches
                        // case-insensitively — simplest: rebuild with (?i) if
                        // the flags section is empty.
                        let flags_section = regex_pat
                            .rfind('/')
                            .map_or("", |pos| &regex_pat[pos + 1..]);
                        if flags_section.contains('i') {
                            Ok(Self::Regex(re))
                        } else {
                            // Re-parse with case-insensitive default
                            let inner_pattern = re.as_str();
                            let effective = format!("(?i){inner_pattern}");
                            match regex::RegexBuilder::new(&effective)
                                .size_limit(1 << 20)
                                .build()
                            {
                                Ok(ci_re) => Ok(Self::Regex(ci_re)),
                                Err(e) => Err(CommandOutcome::UserError(format!(
                                    "invalid --title regex: {regex_pat}\n{e}"
                                ))),
                            }
                        }
                    }
                    Err(e) => Err(CommandOutcome::UserError(format!(
                        "invalid --title regex: {regex_pat}\n{e}"
                    ))),
                }
            } else {
                // Bare form: always case-insensitive
                let effective = format!("(?i){regex_pat}");
                match regex::RegexBuilder::new(&effective)
                    .size_limit(1 << 20)
                    .build()
                {
                    Ok(re) => Ok(Self::Regex(re)),
                    Err(e) => Err(CommandOutcome::UserError(format!(
                        "invalid --title regex: {regex_pat}\n{e}"
                    ))),
                }
            }
        } else {
            // Emit a warning if the pattern looks like it was meant to be
            // a regex or property filter expression.
            if looks_like_misused_regex(pattern) {
                warn::warn(format!(
                    "--title does substring matching by default; \
                     prefix with '~=' for regex: --title '~={pattern}'"
                ));
            }
            Ok(Self::Substring(pattern.to_lowercase()))
        }
    }

    /// Returns true if the title value matches. `Null` titles never match.
    pub(super) fn matches(&self, title: &serde_json::Value) -> bool {
        let title_str = match title {
            serde_json::Value::String(s) => s.as_str(),
            _ => return false,
        };
        match self {
            Self::Substring(lowered) => title_str.to_lowercase().contains(lowered.as_str()),
            Self::Regex(re) => re.is_match(title_str),
        }
    }
}

/// Heuristic: does a plain `--title` value look like the user intended regex
/// or property-filter syntax?
///
/// Catches patterns like `/^foo/`, `^foo$`, `.*bar`, etc. that would silently
/// match nothing as a literal substring.
fn looks_like_misused_regex(pattern: &str) -> bool {
    // Starts with `/` and contains another `/` → likely /regex/ or /regex/i
    if pattern.starts_with('/') && pattern[1..].contains('/') {
        return true;
    }
    // Contains regex anchors or common metacharacters unlikely in titles
    if pattern.starts_with('^') || pattern.ends_with('$') {
        return true;
    }
    // Contains `.*` or `.+` — almost certainly regex
    if pattern.contains(".*") || pattern.contains(".+") {
        return true;
    }
    false
}

/// Return true if `tasks` satisfy `filter`.
pub(super) fn matches_task_filter(tasks: &[FindTaskInfo], filter: &FindTaskFilter) -> bool {
    match filter {
        FindTaskFilter::Any => !tasks.is_empty(),
        FindTaskFilter::Todo => tasks.iter().any(|t| !t.done),
        FindTaskFilter::Done => tasks.iter().any(|t| t.done),
        FindTaskFilter::Status(c) => tasks.iter().any(|t| t.status == *c),
    }
}
