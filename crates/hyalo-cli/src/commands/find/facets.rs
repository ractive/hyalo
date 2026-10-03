//! `find --facet` (iteration 303, DEC-335): per-value file counts over the
//! full match set, computed before `--limit` cuts it.

use std::collections::HashMap;

use hyalo_core::index::IndexEntry;

use crate::output::{FacetBucket, FacetResult};

/// Buckets reported per facet; the rest is cut and `truncated` is set.
pub(crate) const MAX_FACET_BUCKETS: usize = 50;

/// What a facet counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FacetKind {
    /// Exact frontmatter tags.
    Tags,
    /// Scalar values (or list elements) of one top-level frontmatter key.
    Property(String),
    /// The first path segment, `.` for a file at the vault root.
    Dir,
}

/// One parsed `--facet SPEC`. `label` echoes the spec as written (trimmed),
/// so `--facet type` reports `facet: "type"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FacetSpec {
    pub(crate) label: String,
    pub(crate) kind: FacetKind,
}

impl FacetSpec {
    /// Parse `tags`, `property:K`, `type` or `dir`.
    ///
    /// # Errors
    /// Returns a user-facing message for an unknown spec or an empty key.
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let spec = raw.trim();
        let kind = match spec {
            "tags" => FacetKind::Tags,
            "type" => FacetKind::Property("type".to_owned()),
            "dir" => FacetKind::Dir,
            _ => match spec.strip_prefix("property:") {
                Some(key) if !key.trim().is_empty() => FacetKind::Property(key.trim().to_owned()),
                Some(_) => {
                    return Err(format!(
                        "invalid facet '{raw}': property: needs a key, e.g. property:status"
                    ));
                }
                None => {
                    return Err(format!(
                        "unknown facet '{raw}': expected tags, property:<KEY>, type or dir"
                    ));
                }
            },
        };
        Ok(Self {
            label: spec.to_owned(),
            kind,
        })
    }
}

/// Parse every `--facet` value, stopping at the first invalid one.
///
/// # Errors
/// The message of the first spec [`FacetSpec::parse`] rejects.
pub(crate) fn parse_specs(raw: &[String]) -> Result<Vec<FacetSpec>, String> {
    raw.iter().map(|s| FacetSpec::parse(s)).collect()
}

/// Accumulates bucket counts as matching files are confirmed.
pub(crate) struct FacetCounter<'a> {
    specs: &'a [FacetSpec],
    counts: Vec<HashMap<Option<String>, u64>>,
}

/// Render one scalar frontmatter value as a bucket key; `None` is the null bucket.
fn scalar_bucket(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        // A nested list or map has no scalar spelling; its compact JSON
        // keeps distinct values distinct.
        other => Some(other.to_string()),
    }
}

/// Distinct bucket keys one file contributes to `kind` (each counted once).
fn file_buckets(kind: &FacetKind, entry: &IndexEntry) -> Vec<Option<String>> {
    let mut keys: Vec<Option<String>> = match kind {
        FacetKind::Tags => {
            if entry.tags.is_empty() {
                vec![None]
            } else {
                entry.tags.iter().map(|t| Some(t.clone())).collect()
            }
        }
        FacetKind::Property(key) => match entry.properties.get(key.as_str()) {
            None => vec![None],
            Some(serde_json::Value::Array(items)) if items.is_empty() => vec![None],
            Some(serde_json::Value::Array(items)) => items.iter().map(scalar_bucket).collect(),
            Some(value) => vec![scalar_bucket(value)],
        },
        FacetKind::Dir => {
            let dir = entry
                .rel_path
                .split_once('/')
                .map_or(".", |(first, _)| first);
            vec![Some(dir.to_owned())]
        }
    };
    keys.sort_unstable();
    keys.dedup();
    keys
}

impl<'a> FacetCounter<'a> {
    pub(crate) fn new(specs: &'a [FacetSpec]) -> Self {
        Self {
            specs,
            counts: specs.iter().map(|_| HashMap::new()).collect(),
        }
    }

    /// `true` when no facet was requested, so callers can skip the work.
    pub(crate) fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Count one confirmed match.
    pub(crate) fn observe(&mut self, entry: &IndexEntry) {
        for (spec, counts) in self.specs.iter().zip(self.counts.iter_mut()) {
            for key in file_buckets(&spec.kind, entry) {
                *counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    /// Sorted, capped buckets per facet, in the order the specs were given.
    pub(crate) fn finish(self) -> Vec<FacetResult> {
        self.specs
            .iter()
            .zip(self.counts)
            .map(|(spec, counts)| {
                let mut buckets: Vec<(Option<String>, u64)> = counts.into_iter().collect();
                // Count descending, then value ascending with the null bucket
                // last among equal counts.
                buckets.sort_unstable_by(|a, b| {
                    b.1.cmp(&a.1).then_with(|| match (&a.0, &b.0) {
                        (Some(x), Some(y)) => x.cmp(y),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    })
                });
                let truncated = buckets.len() > MAX_FACET_BUCKETS;
                buckets.truncate(MAX_FACET_BUCKETS);
                FacetResult {
                    facet: spec.label.clone(),
                    buckets: buckets
                        .into_iter()
                        .map(|(value, count)| FacetBucket { value, count })
                        .collect(),
                    truncated,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, tags: &[&str], props: &[(&str, serde_json::Value)]) -> IndexEntry {
        IndexEntry {
            rel_path: path.to_owned(),
            modified: String::new(),
            size: 0,
            lines: 0,
            properties: props
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            sections: Vec::new(),
            tasks: Vec::new(),
            links: Vec::new(),
            self_anchors: Vec::new(),
            bm25_tokens: None,
            bm25_language: None,
            bm25_tokenizer_version: None,
        }
    }

    fn values(result: &FacetResult) -> Vec<(Option<&str>, u64)> {
        result
            .buckets
            .iter()
            .map(|b| (b.value.as_deref(), b.count))
            .collect()
    }

    #[test]
    fn parse_accepts_known_specs_and_rejects_others() {
        assert_eq!(FacetSpec::parse("tags").unwrap().kind, FacetKind::Tags);
        assert_eq!(
            FacetSpec::parse("type").unwrap().kind,
            FacetKind::Property("type".into())
        );
        assert_eq!(FacetSpec::parse("type").unwrap().label, "type");
        assert_eq!(
            FacetSpec::parse("property:status").unwrap().kind,
            FacetKind::Property("status".into())
        );
        assert_eq!(FacetSpec::parse(" dir ").unwrap().kind, FacetKind::Dir);
        assert!(FacetSpec::parse("property:").is_err());
        assert!(FacetSpec::parse("tag").is_err());
        assert!(FacetSpec::parse("status").is_err());
    }

    #[test]
    fn counts_tags_properties_and_dirs_per_file() {
        let specs = parse_specs(&["tags".into(), "property:status".into(), "dir".into()]).unwrap();
        let mut counter = FacetCounter::new(&specs);
        counter.observe(&entry(
            "a/x.md",
            &["rust", "rust", "cli"],
            &[("status", serde_json::json!("done"))],
        ));
        counter.observe(&entry(
            "a/y.md",
            &["rust"],
            &[("status", serde_json::json!(["done", "planned"]))],
        ));
        counter.observe(&entry("z.md", &[], &[("status", serde_json::Value::Null)]));
        counter.observe(&entry("b/w.md", &["cli"], &[]));
        let out = counter.finish();
        assert_eq!(
            values(&out[0]),
            vec![(Some("cli"), 2), (Some("rust"), 2), (None, 1)]
        );
        assert_eq!(
            values(&out[1]),
            vec![(Some("done"), 2), (None, 2), (Some("planned"), 1)]
        );
        assert_eq!(
            values(&out[2]),
            vec![(Some("a"), 2), (Some("."), 1), (Some("b"), 1)]
        );
        assert!(out.iter().all(|f| !f.truncated));
    }

    #[test]
    fn scalars_stringify_and_buckets_cap_at_fifty() {
        let specs = parse_specs(&["property:n".into()]).unwrap();
        let mut counter = FacetCounter::new(&specs);
        for i in 0..60 {
            counter.observe(&entry(
                &format!("f{i}.md"),
                &[],
                &[("n", serde_json::json!(i))],
            ));
        }
        counter.observe(&entry("t.md", &[], &[("n", serde_json::json!(true))]));
        let out = counter.finish();
        assert!(out[0].truncated);
        assert_eq!(out[0].buckets.len(), MAX_FACET_BUCKETS);
        // Equal counts sort by value text: "0", "1", "10", ...
        assert_eq!(out[0].buckets[0].value.as_deref(), Some("0"));
        assert_eq!(out[0].buckets[2].value.as_deref(), Some("10"));
    }
}
