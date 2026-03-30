use indexmap::IndexMap;
use serde_json::Value;

use super::{FileVisitor, ScanAction};

/// Collects frontmatter properties from a file scan.
pub(crate) struct FrontmatterCollector {
    props: IndexMap<String, Value>,
    body_needed: bool,
}

impl FrontmatterCollector {
    /// Create a new collector.
    /// If `body_needed` is false, signals the scanner to skip the body after frontmatter.
    #[must_use]
    pub fn new(body_needed: bool) -> Self {
        Self {
            props: IndexMap::new(),
            body_needed,
        }
    }

    /// Consume the collector and return the parsed properties.
    #[must_use]
    pub fn into_props(self) -> IndexMap<String, Value> {
        self.props
    }
}

impl FileVisitor for FrontmatterCollector {
    fn on_frontmatter(&mut self, props: IndexMap<String, Value>) -> ScanAction {
        self.props = props;
        if self.body_needed {
            ScanAction::Continue
        } else {
            ScanAction::Stop
        }
    }

    fn needs_body(&self) -> bool {
        self.body_needed
    }
}
