use anyhow::{Result, bail};

/// Controls which fields are included in `find` output.
#[derive(Debug, Clone)]
pub struct Fields {
    /// Last-modified timestamp. Member of the default set, dropped by an
    /// explicit `--fields` that does not name it (iteration 254, DEC-254).
    pub modified: bool,
    /// File size in bytes. Same rule as [`Fields::modified`].
    pub size: bool,
    /// Body line count. Same rule as [`Fields::modified`].
    pub lines: bool,
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
            modified: true,
            size: true,
            lines: true,
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
/// the `--format text` summary line. `file` is the only unconditional key;
/// `modified`, `size`, and `lines` are ordinary members of this set — cheap
/// enough to always pay for by default, but dropped by an explicit
/// `--fields` that does not name them (iteration 254, DEC-254).
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
    /// slice returns the default set (`modified`, `size`, `lines`, `title`,
    /// `properties`, `tags`, alongside the unconditional `file`).
    ///
    /// A non-empty selection is an *exact* projection (iteration 254,
    /// DEC-254): the result carries exactly the named fields plus `file`, so
    /// `--fields title` yields `{file, title}` and the default-set members
    /// `modified`/`size`/`lines` have to be named to survive. `file` is
    /// accepted as a field name and is a no-op (`--fields file` → `{file}`).
    /// `all` selects every field. Filters that need a field still add it on
    /// top of whatever set is in force (`--section`, `--task`,
    /// `--broken-links`, `--orphan`, `--dead-end`,
    /// `--sort links_count|backlinks_count`).
    pub fn parse(input: &[String]) -> Result<Fields> {
        if input.is_empty() {
            return Ok(Fields::default());
        }

        let mut fields = Fields {
            modified: false,
            size: false,
            lines: false,
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
                        fields.modified = true;
                        fields.size = true;
                        fields.lines = true;
                        fields.properties = true;
                        fields.properties_typed = true;
                        fields.tags = true;
                        fields.sections = true;
                        fields.tasks = true;
                        fields.links = true;
                        fields.backlinks = true;
                        fields.title = true;
                    }
                    // `file` is always emitted; accepting the name lets
                    // `--fields file` express "just the paths" without a
                    // special case, and keeps round-tripping a printed field
                    // list back into `--fields` working.
                    "file" => {}
                    "modified" => fields.modified = true,
                    "size" => fields.size = true,
                    "lines" => fields.lines = true,
                    "properties" => fields.properties = true,
                    "properties-typed" => fields.properties_typed = true,
                    "tags" => fields.tags = true,
                    "sections" | "outline" => fields.sections = true,
                    "tasks" => fields.tasks = true,
                    "links" => fields.links = true,
                    "backlinks" => fields.backlinks = true,
                    "title" => fields.title = true,
                    unknown => bail!(
                        "unknown field {unknown:?}: valid fields are all, file, modified, size, lines, title, properties, properties-typed, tags, sections (alias: outline), tasks, links, backlinks. \
Without --fields: file, modified, size, lines, title, properties, tags. With --fields: exactly the named fields plus file (filters add what they need)."
                    ),
                }
            }
        }

        Ok(fields)
    }
}
