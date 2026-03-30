use anyhow::{Result, bail};

/// Task presence/status filter for `find --task`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindTaskFilter {
    /// Files with any incomplete tasks (status = space)
    Todo,
    /// Files with any completed tasks (status = x or X)
    Done,
    /// Files with any tasks at all
    Any,
    /// Files with tasks matching this exact status character
    Status(char),
}

/// Parse a task filter from a string.
pub fn parse_task_filter(input: &str) -> Result<FindTaskFilter> {
    match input {
        "todo" => Ok(FindTaskFilter::Todo),
        "done" => Ok(FindTaskFilter::Done),
        "any" => Ok(FindTaskFilter::Any),
        s => {
            let mut chars = s.chars();
            let first = chars.next();
            let second = chars.next();
            match (first, second) {
                (Some(ch), None) => Ok(FindTaskFilter::Status(ch)),
                _ => bail!(
                    "invalid task filter {input:?}: expected 'todo', 'done', 'any', or a single character"
                ),
            }
        }
    }
}
