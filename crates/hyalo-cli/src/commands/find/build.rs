use hyalo_core::filter::FindTaskFilter;
use hyalo_core::types::{FindTaskInfo, OutlineSection};

use crate::output::CommandOutcome;

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
    /// - `/pattern/`: regex (case-insensitive by default)
    /// - `/pattern/i`: regex with explicit flags
    ///
    /// Returns `Err(CommandOutcome::UserError(...))` on invalid regex.
    pub(super) fn parse(pattern: &str) -> Result<Self, CommandOutcome> {
        if let Some(rest) = pattern.strip_prefix('/') {
            // Slash-delimited regex: /pattern/ or /pattern/i
            if let Some(close) = rest.rfind('/') {
                let inner = &rest[..close];
                let flags = &rest[close + 1..];

                // Validate flags — only 'i' is supported
                for ch in flags.chars() {
                    if ch != 'i' {
                        return Err(CommandOutcome::UserError(format!(
                            "invalid --title regex: {pattern}\nunsupported regex flag {ch:?}: only 'i' is supported"
                        )));
                    }
                }

                if inner.is_empty() {
                    return Err(CommandOutcome::UserError(format!(
                        "invalid --title regex: {pattern}\nregex pattern must not be empty"
                    )));
                }

                // Case-insensitive by default; opt out with (?-i) in pattern
                let case_insensitive = flags.contains('i') || !inner.contains("(?-i)");
                match regex::RegexBuilder::new(inner)
                    .case_insensitive(case_insensitive)
                    .size_limit(1 << 20)
                    .build()
                {
                    Ok(re) => Ok(Self::Regex(re)),
                    Err(e) => Err(CommandOutcome::UserError(format!(
                        "invalid --title regex: {pattern}\n{e}"
                    ))),
                }
            } else {
                // Single `/` with no closing slash — treat as literal substring
                Ok(Self::Substring(pattern.to_lowercase()))
            }
        } else {
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

/// Return true if `tasks` satisfy `filter`.
pub(super) fn matches_task_filter(tasks: &[FindTaskInfo], filter: &FindTaskFilter) -> bool {
    match filter {
        FindTaskFilter::Any => !tasks.is_empty(),
        FindTaskFilter::Todo => tasks.iter().any(|t| !t.done),
        FindTaskFilter::Done => tasks.iter().any(|t| t.done),
        FindTaskFilter::Status(c) => tasks.iter().any(|t| t.status == *c),
    }
}
