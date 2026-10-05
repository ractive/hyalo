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
                Some(key) if key.trim().is_empty() => {
                    return Err(format!(
                        "invalid facet '{raw}': property: needs a key, e.g. property:status"
                    ));
                }
                // BUG-18: a second ':' (`property:status:extra`) is not a key
                // with a colon in it -- dot-paths use '.', not ':' -- it is
                // almost always `property:` typed with the wrong separator,
                // and silently treating `status:extra` as a literal key name
                // produced a one-bucket, all-null facet. Reject it the same
                // way an unrecognized spec is rejected, naming the four forms
                // actually accepted.
                Some(key) if key.contains(':') => {
                    return Err(format!(
                        "unknown facet '{raw}': expected tags, property:<KEY>, type or dir"
                    ));
                }
                Some(key) => FacetKind::Property(key.trim().to_owned()),
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

/// Parse every `--facet` value, stopping at the first invalid one. A spec
/// repeated (or spelled twice, `type` and `property:type`) is counted once,
/// under its first spelling.
///
/// # Errors
/// The message of the first spec [`FacetSpec::parse`] rejects.
pub(crate) fn parse_specs(raw: &[String]) -> Result<Vec<FacetSpec>, String> {
    let mut specs: Vec<FacetSpec> = Vec::new();
    for spec in raw {
        let spec = FacetSpec::parse(spec)?;
        if !specs.iter().any(|s| s.kind == spec.kind) {
            specs.push(spec);
        }
    }
    Ok(specs)
}

/// One file's contribution to a facet: the folded bucket key, the spelling
/// it was written with, and whether `--property K=V` can replay it.
struct Observation {
    key: Option<String>,
    spelling: Option<String>,
    replayable: bool,
}

/// Fold a string the way `--property K=V` equality compares it.
fn fold(value: &str) -> String {
    if value.is_ascii() {
        value.to_ascii_lowercase()
    } else {
        value.to_lowercase()
    }
}

/// One scalar frontmatter value as an observation; null is the null bucket.
/// Strings fold case (so buckets agree with the equality filter); a nested
/// map or list keeps its compact JSON and is never offered as a drill-down.
fn scalar_observation(value: &serde_json::Value) -> Observation {
    match value {
        serde_json::Value::Null => Observation {
            key: None,
            spelling: None,
            replayable: false,
        },
        serde_json::Value::String(s) => Observation {
            key: Some(fold(s)),
            spelling: Some(s.clone()),
            replayable: true,
        },
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => Observation {
            key: Some(value.to_string()),
            spelling: Some(value.to_string()),
            replayable: true,
        },
        other => Observation {
            key: Some(other.to_string()),
            spelling: Some(other.to_string()),
            replayable: false,
        },
    }
}

/// Distinct observations one file contributes to `kind` (each key once).
fn file_observations(kind: &FacetKind, entry: &IndexEntry) -> Vec<Observation> {
    let mut out: Vec<Observation> = match kind {
        FacetKind::Tags => {
            if entry.tags.is_empty() {
                vec![scalar_observation(&serde_json::Value::Null)]
            } else {
                // Tags are compared exactly by `--tag`, so they do not fold.
                entry
                    .tags
                    .iter()
                    .map(|t| Observation {
                        key: Some(t.clone()),
                        spelling: Some(t.clone()),
                        replayable: true,
                    })
                    .collect()
            }
        }
        FacetKind::Property(key) => {
            match hyalo_core::filter::resolve_prop(&entry.properties, key).as_deref() {
                None => vec![scalar_observation(&serde_json::Value::Null)],
                Some(serde_json::Value::Array(items)) if items.is_empty() => {
                    vec![scalar_observation(&serde_json::Value::Null)]
                }
                Some(serde_json::Value::Array(items)) => {
                    items.iter().map(scalar_observation).collect()
                }
                Some(value) => vec![scalar_observation(value)],
            }
        }
        FacetKind::Dir => {
            let dir = entry
                .rel_path
                .split_once('/')
                .map_or(".", |(first, _)| first);
            vec![Observation {
                key: Some(dir.to_owned()),
                spelling: Some(dir.to_owned()),
                replayable: true,
            }]
        }
    };
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out.dedup_by(|a, b| a.key == b.key);
    out
}

/// Running totals of one bucket.
#[derive(Default)]
struct BucketTally {
    count: u64,
    /// Files per original spelling; the most common one is displayed.
    spellings: HashMap<Option<String>, u64>,
    replayable: bool,
}

/// Accumulates bucket counts as matching files are confirmed.
pub(crate) struct FacetCounter<'a> {
    specs: &'a [FacetSpec],
    counts: Vec<HashMap<Option<String>, BucketTally>>,
    files: u64,
}

impl<'a> FacetCounter<'a> {
    pub(crate) fn new(specs: &'a [FacetSpec]) -> Self {
        Self {
            specs,
            counts: specs.iter().map(|_| HashMap::new()).collect(),
            files: 0,
        }
    }

    /// `true` when no facet was requested, so callers can skip the work.
    pub(crate) fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Count one confirmed match.
    pub(crate) fn observe(&mut self, entry: &IndexEntry) {
        self.files += 1;
        for (spec, counts) in self.specs.iter().zip(self.counts.iter_mut()) {
            for obs in file_observations(&spec.kind, entry) {
                let tally = counts.entry(obs.key).or_default();
                tally.count += 1;
                tally.replayable = obs.replayable;
                *tally.spellings.entry(obs.spelling).or_insert(0) += 1;
            }
        }
    }

    /// Sorted, capped buckets per facet, in the order the specs were given.
    pub(crate) fn finish(self) -> Vec<FacetResult> {
        let files = self.files;
        self.specs
            .iter()
            .zip(self.counts)
            .map(|(spec, counts)| {
                let mut buckets: Vec<FacetBucket> = counts
                    .into_values()
                    .map(|tally| {
                        // Most common spelling wins; ties go to the smallest.
                        let value = tally
                            .spellings
                            .into_iter()
                            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                            .and_then(|(spelling, _)| spelling);
                        FacetBucket {
                            value,
                            count: tally.count,
                            replayable: tally.replayable,
                        }
                    })
                    .collect();
                // Count descending, then value ascending with the null bucket
                // last among equal counts.
                buckets.sort_unstable_by(|a, b| {
                    b.count
                        .cmp(&a.count)
                        .then_with(|| match (&a.value, &b.value) {
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
                    buckets,
                    truncated,
                    files,
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
            explicit_anchor_ids: Vec::new(),
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
    fn parse_accepts_dot_paths_but_rejects_a_second_colon() {
        // Dot-paths are legitimate keys (`--property`/`--sort property:`
        // resolve them the same way); a *second* colon is not (BUG-18).
        assert_eq!(
            FacetSpec::parse("property:meta.owner").unwrap().kind,
            FacetKind::Property("meta.owner".into())
        );
        let err = FacetSpec::parse("property:status:extra").unwrap_err();
        assert_eq!(
            err,
            "unknown facet 'property:status:extra': expected tags, property:<KEY>, type or dir"
        );
        assert!(FacetSpec::parse("property::").is_err());
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

    #[test]
    fn string_buckets_fold_case_like_the_equality_filter() {
        let specs = parse_specs(&["property:s".into()]).unwrap();
        let mut counter = FacetCounter::new(&specs);
        for (i, v) in ["Open", "open", "open", "Done"].iter().enumerate() {
            counter.observe(&entry(
                &format!("f{i}.md"),
                &[],
                &[("s", serde_json::json!(v))],
            ));
        }
        let out = counter.finish();
        assert_eq!(values(&out[0]), vec![(Some("open"), 3), (Some("Done"), 1)]);
        assert_eq!(out[0].files, 4);
    }

    #[test]
    fn structured_values_are_counted_but_not_replayable() {
        let specs = parse_specs(&["property:details".into()]).unwrap();
        let mut counter = FacetCounter::new(&specs);
        counter.observe(&entry(
            "a.md",
            &[],
            &[("details", serde_json::json!({"owner": "ada"}))],
        ));
        counter.observe(&entry("b.md", &[], &[("details", serde_json::json!("x"))]));
        let out = counter.finish();
        let map = out[0]
            .buckets
            .iter()
            .find(|b| b.value.as_deref() == Some(r#"{"owner":"ada"}"#))
            .unwrap();
        assert!(!map.replayable);
        let scalar = out[0]
            .buckets
            .iter()
            .find(|b| b.value.as_deref() == Some("x"))
            .unwrap();
        assert!(scalar.replayable);
    }

    #[test]
    fn property_facet_resolves_dot_paths_like_the_filter() {
        let specs = parse_specs(&["property:meta.owner".into()]).unwrap();
        let mut counter = FacetCounter::new(&specs);
        counter.observe(&entry(
            "a.md",
            &[],
            &[("meta", serde_json::json!({"owner": "ann"}))],
        ));
        counter.observe(&entry("b.md", &[], &[]));
        let out = counter.finish();
        assert_eq!(values(&out[0]), vec![(Some("ann"), 1), (None, 1)]);
    }

    #[test]
    fn repeated_specs_are_counted_once_under_the_first_spelling() {
        let specs = parse_specs(&[
            "type".into(),
            "tags".into(),
            "property:type".into(),
            "tags".into(),
        ])
        .unwrap();
        let labels: Vec<&str> = specs.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["type", "tags"]);
    }
}
