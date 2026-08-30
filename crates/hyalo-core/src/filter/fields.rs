use anyhow::{Result, bail};

/// Controls which fields are included in `find` output.
#[derive(Debug, Clone)]
pub struct Fields {
    pub properties: bool,
    pub properties_typed: bool,
    pub tags: bool,
    pub sections: bool,
    pub tasks: bool,
    pub links: bool,
    /// Backlinks are opt-in only: building the link graph requires scanning all files.
    pub backlinks: bool,
    /// Title extracted from frontmatter `title` property or first H1 heading.
    pub title: bool,
}

impl Default for Fields {
    fn default() -> Self {
        Self {
            properties: true,
            properties_typed: false,
            tags: true,
            sections: false,
            tasks: false,
            links: false,
            backlinks: false,
            title: true,
        }
    }
}

/// The default field set, in the order `find` emits it, for help texts and
/// the `--format text` summary line. `file`, `modified`, `size`, and `lines`
/// are structural — always present, never selectable — so they lead the list.
pub const DEFAULT_FIELD_NAMES: &[&str] = &[
    "file",
    "modified",
    "size",
    "lines",
    "title",
    "properties",
    "tags",
];

impl Fields {
    /// Parse a fields selection from a list of `--fields` argument values.
    ///
    /// Each element may be a comma-separated list of field names. An empty
    /// slice returns the default (iteration 252: `properties`, `tags`,
    /// `title` — alongside the always-present `file`, `modified`, `size`,
    /// `lines`). `sections`, `links`, `tasks`, `properties-typed`, and
    /// `backlinks` are opt-in, either via `--fields` or via the filter that
    /// implies them (`--section`, `--task`, `--broken-links`, `--orphan`,
    /// `--dead-end`, `--sort links_count|backlinks_count`).
    pub fn parse(input: &[String]) -> Result<Fields> {
        if input.is_empty() {
            return Ok(Fields::default());
        }

        let mut fields = Fields {
            properties: false,
            properties_typed: false,
            tags: false,
            sections: false,
            tasks: false,
            links: false,
            backlinks: false,
            title: false,
        };

        for item in input {
            for part in item.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                match part {
                    "all" => {
                        fields.properties = true;
                        fields.properties_typed = true;
                        fields.tags = true;
                        fields.sections = true;
                        fields.tasks = true;
                        fields.links = true;
                        fields.backlinks = true;
                        fields.title = true;
                    }
                    "properties" => fields.properties = true,
                    "properties-typed" => fields.properties_typed = true,
                    "tags" => fields.tags = true,
                    "sections" | "outline" => fields.sections = true,
                    "tasks" => fields.tasks = true,
                    "links" => fields.links = true,
                    "backlinks" => fields.backlinks = true,
                    "title" => fields.title = true,
                    unknown => bail!(
                        "unknown field {unknown:?}: valid fields are all, properties, properties-typed, tags, sections (alias: outline), tasks, links, backlinks, title"
                    ),
                }
            }
        }

        Ok(fields)
    }
}
