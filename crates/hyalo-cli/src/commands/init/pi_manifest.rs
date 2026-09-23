//! Ownership of the shared Pi manifest. Receipts describe additions, never a filename claim.
use super::{
    Report, capture_installation, publish_captured_installation,
    remove_captured_installation_artifact,
};
use anyhow::{Context, Result};
use hyalo_core::rooted::CapturedInput;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const RUNTIME_MANIFEST: &str = ".pi/lib/package.json";
const MANIFEST: &str = ".pi/package.json";
pub(super) const RECEIPT: &str = ".pi/.hyalo-manifest.json";
const ARTIFACT_PATHS: [&str; 7] = [
    ".pi/skills/hyalo/SKILL.md",
    ".pi/skills/hyalo-tidy/SKILL.md",
    ".pi/extensions/hyalo.ts",
    ".pi/lib/hyalo-api.js",
    ".pi/lib/hyalo-api.d.ts",
    ".pi/skills/hyalo-tidy/references/jev.md",
    ".pi/skills/hyalo-tidy/scripts/jev.mjs",
];
// Exact installed bytes avoid hash collisions and remain bounded independently
// of the general JSON decoder. The shipped files currently total < 300 KiB.
const MAX_ARTIFACT_RECEIPT_BYTES: usize = 2 * 1024 * 1024;

fn valid_artifact_receipts(artifacts: &BTreeMap<String, String>, budget: usize) -> bool {
    artifacts
        .keys()
        .all(|path| ARTIFACT_PATHS.contains(&path.as_str()))
        && artifacts
            .values()
            .try_fold(0_usize, |total, content| total.checked_add(content.len()))
            .is_some_and(|total| total <= budget)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    version: u32,
    original: Option<Value>,
    installed: Value,
    runtime_original: Option<Value>,
    runtime_installed: Value,
    #[serde(default)]
    artifacts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    previous_artifacts: BTreeMap<String, String>,
}

impl Ownership {
    fn owns_artifact(&self, path: &str, bytes: &[u8]) -> bool {
        self.artifacts
            .get(path)
            .into_iter()
            .chain(self.previous_artifacts.get(path))
            .any(|expected| bytes == expected.as_bytes())
    }

    fn valid_artifact_budget(&self, budget: usize) -> bool {
        valid_artifact_receipts(&self.artifacts, budget)
            && valid_artifact_receipts(&self.previous_artifacts, budget)
            && self
                .artifacts
                .values()
                .chain(self.previous_artifacts.values())
                .try_fold(0_usize, |total, content| total.checked_add(content.len()))
                .is_some_and(|total| total <= budget)
    }
}

struct ArtifactPlan {
    path: &'static str,
    source: Option<CapturedInput>,
    content: String,
}

pub(super) struct Plan {
    source: Option<CapturedInput>,
    runtime_source: Option<CapturedInput>,
    runtime_manifest: Value,
    receipt_source: Option<CapturedInput>,
    manifest: Value,
    ownership: Ownership,
    artifacts: Vec<ArtifactPlan>,
}

/// serde_json's ordinary Value decoder keeps the last duplicate object key.
/// Shared configuration must instead refuse ambiguity at every nesting level.
/// Deserialize through the normal JSON engine so its existing recursion limit
/// and JSON string/number semantics remain in force.
struct UniqueJson(Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> serde::de::Visitor<'de> for JsonVisitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                value: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(
                self,
                value: i64,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(
                self,
                value: u64,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(
                self,
                value: f64,
            ) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(UniqueJson)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut object: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = object.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON object key: {key}"
                        )));
                    }
                    let UniqueJson(value) = object.next_value()?;
                    values.insert(key, value);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(JsonVisitor)
    }
}

fn read_json(source: &CapturedInput, label: &str) -> Result<Value> {
    let UniqueJson(value) = serde_json::from_slice(&source.bytes()?).map_err(|error| {
        anyhow::anyhow!("invalid {label}: {error}; preserve and repair the JSON object before init")
    })?;
    anyhow::ensure!(value.is_object(), "{label} must be a JSON object");
    Ok(value)
}

fn read_ownership(source: &CapturedInput) -> Result<Ownership> {
    let UniqueJson(value) = serde_json::from_slice(&source.bytes()?).map_err(|error| anyhow::anyhow!("unrecognized Pi ownership receipt: {error}; retain and inspect .pi/.hyalo-manifest.json"))?;
    let ownership: Ownership = serde_json::from_value(value).context(
        "unrecognized Pi ownership receipt; retain it and inspect .pi/.hyalo-manifest.json",
    )?;
    anyhow::ensure!(
        matches!(ownership.version, 1..=3)
            && (ownership.version >= 2 || ownership.artifacts.is_empty())
            && (ownership.version == 3 || ownership.previous_artifacts.is_empty())
            && ownership.installed.is_object()
            && ownership.original.as_ref().is_none_or(Value::is_object)
            && ownership.runtime_installed.is_object()
            && ownership
                .runtime_original
                .as_ref()
                .is_none_or(Value::is_object),
        "unsupported Pi ownership receipt"
    );
    anyhow::ensure!(
        ownership.valid_artifact_budget(MAX_ARTIFACT_RECEIPT_BYTES),
        "unsupported or oversized Pi artifact ownership receipt"
    );
    Ok(ownership)
}

/// Store only additions and the shape of pre-existing containers. Unrelated
/// user values are neither owned nor copied into the receipt.
fn ownership_projection(original: Option<&Value>, installed: &Value) -> (Option<Value>, Value) {
    let Some(original) = original else {
        return (None, installed.clone());
    };
    match (original, installed) {
        (Value::Object(before), Value::Object(after)) => {
            let mut shape = serde_json::Map::new();
            let mut additions = serde_json::Map::new();
            for (key, value) in after {
                if before.get(key) == Some(value) {
                    continue;
                }
                let (old_shape, delta) = ownership_projection(before.get(key), value);
                if let Some(old_shape) = old_shape {
                    shape.insert(key.to_owned(), old_shape);
                }
                additions.insert(key.to_owned(), delta);
            }
            (Some(Value::Object(shape)), Value::Object(additions))
        }
        (Value::Array(before), Value::Array(after)) => (
            Some(Value::Array(Vec::new())),
            Value::Array(
                after
                    .iter()
                    .filter(|value| !before.contains(value))
                    .cloned()
                    .collect(),
            ),
        ),
        _ => (Some(original.clone()), installed.clone()),
    }
}

/// Rebase an existing receipt on the latest user state, then record every
/// addition in this installation. Changed or ambiguous prior additions become
/// user-owned; only values that cleanup could safely remove remain ours.
fn refreshed_projection(
    current: Option<&Value>,
    installed: &Value,
    previous: Option<(Option<&Value>, &Value)>,
) -> (Option<Value>, Value) {
    let (Some(current), Some((original, prior_additions))) = (current, previous) else {
        return ownership_projection(current, installed);
    };
    let mut baseline = current.clone();
    subtract(&mut baseline, original, prior_additions);
    let baseline =
        if original.is_none() && baseline.as_object().is_some_and(serde_json::Map::is_empty) {
            None
        } else {
            Some(&baseline)
        };
    ownership_projection(baseline, installed)
}

pub(super) fn prepare(root: &Path, dir_value: &str) -> Result<Plan> {
    let source = if root.exists() {
        capture_installation(root, &root.join(MANIFEST))?
    } else {
        None
    };
    let receipt_source = if root.exists() {
        capture_installation(root, &root.join(RECEIPT))?
    } else {
        None
    };
    let original = source
        .as_ref()
        .map(|s| read_json(s, MANIFEST))
        .transpose()?;
    let mut manifest = match &original {
        Some(value) => value.clone(),
        None => serde_json::from_str(super::PI_PACKAGE_JSON_CONTENT)?,
    };
    let object = manifest
        .as_object_mut()
        .context("Pi manifest must be an object")?;
    let pi = object
        .entry("pi")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let pi = pi
        .as_object_mut()
        .context("package.json pi must be an object; existing manifest preserved")?;
    for (field, required) in [
        ("extensions", &["./extensions/hyalo.ts"][..]),
        ("skills", &["./skills/hyalo", "./skills/hyalo-tidy"][..]),
    ] {
        let array = pi.entry(field).or_insert_with(|| Value::Array(Vec::new()));
        let array = array.as_array_mut().with_context(|| {
            format!("package.json pi.{field} must be an array; existing manifest preserved")
        })?;
        anyhow::ensure!(
            array.iter().all(Value::is_string),
            "package.json pi.{field} entries must be strings"
        );
        for required in required {
            let covered = array.iter().filter_map(Value::as_str).any(|entry| {
                let path = entry.trim_start_matches("./").trim_end_matches('/');
                path == field
                    || path == required.trim_start_matches("./")
                    || (field == "skills"
                        && path.strip_suffix("/SKILL.md")
                            == Some(required.trim_start_matches("./")))
            });
            if !covered {
                array.push(Value::String((*required).to_owned()));
            }
        }
    }
    let runtime_source = if root.exists() {
        capture_installation(root, &root.join(RUNTIME_MANIFEST))?
    } else {
        None
    };
    let runtime_original = runtime_source
        .as_ref()
        .map(|source| read_json(source, RUNTIME_MANIFEST))
        .transpose()?;
    let mut runtime_manifest = runtime_original
        .clone()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let runtime = runtime_manifest
        .as_object_mut()
        .context("Pi runtime manifest must be an object")?;
    if let Some(value) = runtime.get("type") {
        anyhow::ensure!(
            value == "module",
            ".pi/lib/package.json type conflicts with Hyalo's ES module runtime; use a separate Pi package or relocate the existing runtime configuration"
        );
    } else {
        // Without a nested package boundary, the captured parent's explicit
        // module type already applies to every script in lib. Otherwise adding
        // module metadata must not change unrelated scripts' interpretation.
        let inherits_esm = runtime_original.is_none()
            && original
                .as_ref()
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                == Some("module");
        if !inherits_esm && root.join(".pi/lib").is_dir() {
            for entry in std::fs::read_dir(root.join(".pi/lib"))? {
                let entry = entry?;
                anyhow::ensure!(
                    matches!(
                        entry.file_name().to_str(),
                        Some("hyalo-api.js" | "hyalo-api.d.ts" | "package.json")
                    ),
                    ".pi/lib contains unrelated files without explicit module metadata; install Hyalo as a separate Pi package to preserve their module semantics"
                );
            }
        }
        runtime.insert("type".into(), Value::String("module".into()));
    }
    let previous = receipt_source.as_ref().map(read_ownership).transpose()?;
    let contents = [
        super::parameterize_template(super::PI_SKILL_CONTENT, dir_value),
        super::parameterize_template(super::PI_TIDY_SKILL_CONTENT, dir_value),
        super::PI_EXTENSION_CONTENT.to_owned(),
        super::PI_API_RUNTIME_CONTENT.to_owned(),
        super::PI_API_DECLARATION_CONTENT.to_owned(),
        super::jev_assets::REFERENCE.to_owned(),
        super::jev_assets::SCRIPT.to_owned(),
    ];
    anyhow::ensure!(
        contents.iter().map(String::len).sum::<usize>() <= MAX_ARTIFACT_RECEIPT_BYTES,
        "Pi artifact content exceeds ownership receipt limit"
    );
    let mut artifacts = Vec::with_capacity(ARTIFACT_PATHS.len());
    let mut artifact_receipts = BTreeMap::new();
    let mut previous_artifacts = BTreeMap::new();
    for (path, content) in ARTIFACT_PATHS.into_iter().zip(contents) {
        let source = if root.exists() {
            capture_installation(root, &root.join(path))?
        } else {
            None
        };
        if let Some(source) = &source {
            let bytes = source.bytes()?;
            anyhow::ensure!(
                previous
                    .as_ref()
                    .is_some_and(|old| old.owns_artifact(path, &bytes)),
                "Pi artifact {path} is unowned or changed; preserve or relocate it before init"
            );
            if bytes != content.as_bytes() {
                previous_artifacts.insert(path.to_owned(), String::from_utf8(bytes)?);
            }
        }
        artifact_receipts.insert(path.to_owned(), content.clone());
        artifacts.push(ArtifactPlan {
            path,
            source,
            content,
        });
    }
    let (original, installed) = refreshed_projection(
        original.as_ref(),
        &manifest,
        previous
            .as_ref()
            .map(|old| (old.original.as_ref(), &old.installed)),
    );
    let (runtime_original, runtime_installed) = refreshed_projection(
        runtime_original.as_ref(),
        &runtime_manifest,
        previous
            .as_ref()
            .map(|old| (old.runtime_original.as_ref(), &old.runtime_installed)),
    );
    let ownership = Ownership {
        version: 3,
        original,
        installed,
        runtime_original,
        runtime_installed,
        artifacts: artifact_receipts,
        previous_artifacts,
    };
    anyhow::ensure!(
        ownership.valid_artifact_budget(MAX_ARTIFACT_RECEIPT_BYTES),
        "Pi previous and pending artifact content exceeds ownership receipt limit"
    );
    Ok(Plan {
        source,
        runtime_source,
        runtime_manifest,
        receipt_source,
        manifest,
        ownership,
        artifacts,
    })
}

impl Plan {
    pub(super) fn publish(mut self, root: &Path, report: &mut Report) -> Result<()> {
        for relative in [
            ".pi",
            ".pi/skills/hyalo",
            ".pi/skills/hyalo-tidy",
            ".pi/skills/hyalo-tidy/references",
            ".pi/skills/hyalo-tidy/scripts",
            ".pi/extensions",
            ".pi/lib",
        ] {
            super::ensure_installation_dir(root, &root.join(relative), report)?;
        }
        // Keep both captured installed bytes and pending bytes until every write
        // succeeds, so a partial upgrade remains recognizable on retry/cleanup.
        let receipt_existed = self.receipt_source.is_some();
        let pending_bytes = serde_json::to_vec_pretty(&self.ownership)?;
        publish_captured_installation(
            root,
            &root.join(RECEIPT),
            self.receipt_source,
            &pending_bytes,
        )?;
        report.push(
            if receipt_existed {
                "updated"
            } else {
                "created"
            },
            RECEIPT,
        );
        let pending_source = if self.ownership.previous_artifacts.is_empty() {
            None
        } else {
            let captured = capture_installation(root, &root.join(RECEIPT))?
                .context("Pi ownership receipt disappeared after publication")?;
            anyhow::ensure!(
                captured.bytes()? == pending_bytes,
                "Pi ownership receipt changed after publication; preserve it before retrying"
            );
            Some(captured)
        };
        for artifact in self.artifacts {
            let existed = artifact.source.is_some();
            publish_captured_installation(
                root,
                &root.join(artifact.path),
                artifact.source,
                artifact.content.as_bytes(),
            )?;
            report.push(if existed { "updated" } else { "created" }, artifact.path);
        }
        let existed = self.source.is_some();
        publish_captured_installation(
            root,
            &root.join(MANIFEST),
            self.source,
            &serde_json::to_vec_pretty(&self.manifest)?,
        )?;
        report.push(if existed { "updated" } else { "created" }, MANIFEST);
        if !existed {
            report.notes.push(super::PI_INSTALL_HINT.to_owned());
        }
        let runtime_existed = self.runtime_source.is_some();
        publish_captured_installation(
            root,
            &root.join(RUNTIME_MANIFEST),
            self.runtime_source,
            &serde_json::to_vec_pretty(&self.runtime_manifest)?,
        )?;
        report.push(
            if runtime_existed {
                "updated"
            } else {
                "created"
            },
            RUNTIME_MANIFEST,
        );
        if let Some(pending_source) = pending_source {
            self.ownership.previous_artifacts.clear();
            publish_captured_installation(
                root,
                &root.join(RECEIPT),
                Some(pending_source),
                &serde_json::to_vec_pretty(&self.ownership)?,
            )?;
        }
        Ok(())
    }
}

/// Subtract only additions that still have their installed value. User changes win.
fn subtract(current: &mut Value, original: Option<&Value>, installed: &Value) -> bool {
    let mut ambiguous = false;
    if let (Some(current), Some(installed)) = (current.as_object_mut(), installed.as_object()) {
        for (key, added) in installed {
            let before = original.and_then(|v| v.get(key));
            if before == Some(added) {
                continue;
            }
            if let Some(value) = current.get_mut(key) {
                if before.is_none() && value == added {
                    current.remove(key);
                } else {
                    ambiguous |= subtract(value, before, added);
                    if before.is_none()
                        && (value.as_object().is_some_and(serde_json::Map::is_empty)
                            || value.as_array().is_some_and(Vec::is_empty))
                    {
                        current.remove(key);
                    }
                }
            }
        }
    } else if let (Some(current), Some(installed)) = (current.as_array_mut(), installed.as_array())
    {
        for added in installed {
            if original
                .and_then(Value::as_array)
                .is_some_and(|array| array.contains(added))
            {
                continue;
            }
            // Duplicate entries are ambiguous, so retain them.
            let count = current.iter().filter(|entry| *entry == added).count();
            if count == 1 {
                current.retain(|entry| entry != added);
            }
            ambiguous |= count > 1;
        }
    } else if original.is_none() && current != installed {
        ambiguous = true;
    }
    ambiguous
}

fn remove_owned_manifest(
    root: &Path,
    manifest_path: &str,
    source: Option<CapturedInput>,
    original: Option<&Value>,
    installed: &Value,
    report: &mut Report,
) -> Result<bool> {
    if let Some(source) = source {
        let mut current = match read_json(&source, manifest_path) {
            Ok(value) => value,
            Err(error) => {
                report.notes.push(error.to_string());
                return Ok(false);
            }
        };
        if subtract(&mut current, original, installed) {
            report.notes.push("Pi manifest contains changed or duplicate owned entries; ambiguous entries were preserved".into());
        }
        if original.is_none() && current.as_object().is_some_and(serde_json::Map::is_empty) {
            remove_captured_installation_artifact(root, &root.join(manifest_path), source)?;
            report.push("removed", manifest_path);
        } else {
            publish_captured_installation(
                root,
                &root.join(manifest_path),
                Some(source),
                &serde_json::to_vec_pretty(&current)?,
            )?;
            report.push_detail(
                "updated",
                manifest_path,
                "removed owned additions; preserved pre-existing entries and user changes",
            );
        }
    }
    Ok(true)
}

/// A surviving registration is user-owned, including duplicate additions and
/// directory registrations that predate installation. Keep its targets usable.
fn retained_registration_needs_artifact(manifest: &Value) -> bool {
    ["extensions", "skills"].into_iter().any(|field| {
        manifest
            .get("pi")
            .and_then(|pi| pi.get(field))
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().filter_map(Value::as_str).any(|entry| {
                    let normalized = entry.replace('\\', "/");
                    let mut entry = normalized.as_str();
                    while let Some(rest) = entry.strip_prefix("./") {
                        entry = rest;
                    }
                    let entry = entry.trim_end_matches('/');
                    // Unknown external/parent-relative spellings cannot certify
                    // that deleting these fixed paths leaves the registration safe.
                    if entry.is_empty()
                        || entry == "."
                        || entry.starts_with('/')
                        || entry.contains(':')
                        || entry.split('/').any(|part| part == "..")
                    {
                        return true;
                    }
                    let entry = entry
                        .split('/')
                        .filter(|part| !part.is_empty() && *part != ".")
                        .collect::<Vec<_>>()
                        .join("/");
                    let Ok(pattern) = globset::GlobBuilder::new(&entry)
                        .literal_separator(true)
                        // A case-insensitive filesystem can load these same
                        // fixed files through differently cased registrations.
                        .case_insensitive(true)
                        .build()
                    else {
                        return true;
                    };
                    let matcher = pattern.compile_matcher();
                    ARTIFACT_PATHS.iter().any(|path| {
                        let Some(relative) = path.strip_prefix(".pi/") else {
                            return true;
                        };
                        let relative = Path::new(relative);
                        relative
                            .ancestors()
                            .filter(|path| !path.as_os_str().is_empty())
                            .any(|path| matcher.is_match(path))
                    })
                })
            })
    })
}

pub(super) fn remove(root: &Path, report: &mut Report) -> Result<()> {
    let Some(receipt) = capture_installation(root, &root.join(RECEIPT))? else {
        if root.join(".pi").exists() {
            report.push_detail(
                "skipped",
                MANIFEST,
                "no ownership receipt; shared configuration and legacy Pi artifacts preserved",
            );
        }
        return Ok(());
    };
    let ownership = match read_ownership(&receipt) {
        Ok(value) => value,
        Err(error) => {
            report.notes.push(error.to_string());
            return Ok(());
        }
    };
    // Refuse the whole Pi group before deleting a target of a retained user
    // registration. Benign unrelated metadata edits still use subtraction below.
    let mut artifacts = Vec::new();
    for path in ARTIFACT_PATHS {
        if let Some(source) = capture_installation(root, &root.join(path))? {
            let bytes = source.bytes()?;
            if !ownership.owns_artifact(path, &bytes) {
                report.notes.push(format!("Pi artifact {path} is unowned or changed; Pi artifacts, registrations and ownership receipt preserved"));
                return Ok(());
            }
            artifacts.push((path, source));
        }
    }
    let shared_source = capture_installation(root, &root.join(MANIFEST))?;
    let runtime_source = capture_installation(root, &root.join(RUNTIME_MANIFEST))?;
    for (path, source) in [
        (MANIFEST, &shared_source),
        (RUNTIME_MANIFEST, &runtime_source),
    ] {
        if let Some(source) = source {
            let value = match read_json(source, path) {
                Ok(value) => value,
                Err(error) => {
                    report.notes.push(error.to_string());
                    return Ok(());
                }
            };
            let conflicting = if path == RUNTIME_MANIFEST {
                value
                    .get("type")
                    .is_some_and(|kind| kind.as_str() != Some("module"))
            } else {
                value.get("pi").is_some_and(|pi| {
                    !pi.is_object()
                        || ["extensions", "skills"].into_iter().any(|key| {
                            pi.get(key).is_some_and(|entries| {
                                entries.as_array().is_none_or(|entries| {
                                    entries.iter().any(|entry| !entry.is_string())
                                })
                            })
                        })
                })
            };
            if conflicting {
                report.notes.push(format!("conflicting {path}; Pi artifacts, registrations and ownership receipt preserved"));
                return Ok(());
            }
            if path == MANIFEST {
                let mut residual = value;
                let ambiguous = subtract(
                    &mut residual,
                    ownership.original.as_ref(),
                    &ownership.installed,
                );
                if retained_registration_needs_artifact(&residual) {
                    if ambiguous {
                        report.notes.push("Pi manifest contains changed or duplicate owned entries; ambiguous entries were preserved".into());
                    }
                    report.notes.push("Pi manifest retains registrations that reference Hyalo artifacts; Pi artifacts, registrations and ownership receipt preserved".into());
                    return Ok(());
                }
            }
        }
    }
    for (path, source) in artifacts {
        remove_captured_installation_artifact(root, &root.join(path), source)?;
        report.push("removed", path);
    }
    let shared_clean = remove_owned_manifest(
        root,
        MANIFEST,
        shared_source,
        ownership.original.as_ref(),
        &ownership.installed,
        report,
    )?;
    let runtime_clean = remove_owned_manifest(
        root,
        RUNTIME_MANIFEST,
        runtime_source,
        ownership.runtime_original.as_ref(),
        &ownership.runtime_installed,
        report,
    )?;
    if !shared_clean || !runtime_clean {
        return Ok(());
    }
    remove_captured_installation_artifact(root, &root.join(RECEIPT), receipt)?;
    report.push("removed", RECEIPT);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_registration_spellings_conservatively_retain_targets() {
        for entry in [
            "/local/.pi/extensions/hyalo.ts",
            r"\local\.pi\extensions\hyalo.ts",
            r"\\server\share\.pi\extensions\hyalo.ts",
            "//server/share/.pi/extensions/hyalo.ts",
            "C:/local/.pi/extensions/hyalo.ts",
            "../.pi/extensions/hyalo.ts",
            "C:extensions/hyalo.ts",
            "[unclosed",
        ] {
            let manifest = serde_json::json!({"pi":{"extensions":[entry]}});
            assert!(retained_registration_needs_artifact(&manifest), "{entry}");
        }
        let unrelated = serde_json::json!({"pi":{"extensions":["./custom/extension.ts"],"skills":["./custom/skills"]}});
        assert!(!retained_registration_needs_artifact(&unrelated));
    }

    fn test_report(root: &Path) -> Report {
        Report {
            command: "init",
            root: root.display().to_string(),
            dir: None,
            actions: Vec::new(),
            notes: Vec::new(),
            external_root: false,
            observed_failures: Vec::new(),
        }
    }

    #[test]
    fn partial_artifact_upgrade_remains_retryable_and_removable() {
        for retry in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path();
            prepare(root, "kb")
                .unwrap()
                .publish(root, &mut test_report(root))
                .unwrap();
            let captured = capture_installation(root, &root.join(RECEIPT))
                .unwrap()
                .unwrap();
            let mut old = read_ownership(&captured).unwrap();
            old.version = 2; // Existing completed receipts remain upgradeable.
            for path in ARTIFACT_PATHS {
                let content = format!("Previous shipped bytes for {path}\n");
                std::fs::write(root.join(path), &content).unwrap();
                old.artifacts.insert(path.into(), content);
            }
            std::fs::write(root.join(RECEIPT), serde_json::to_vec(&old).unwrap()).unwrap();
            let mut upgrade = prepare(root, "kb").unwrap();
            // Force a deterministic checked create collision after the first
            // upgrade write; the existing second artifact remains untouched.
            upgrade.artifacts[1].source = None;
            assert!(upgrade.publish(root, &mut test_report(root)).is_err());
            assert_ne!(
                std::fs::read_to_string(root.join(ARTIFACT_PATHS[0])).unwrap(),
                old.artifacts[ARTIFACT_PATHS[0]]
            );
            assert_eq!(
                std::fs::read_to_string(root.join(ARTIFACT_PATHS[1])).unwrap(),
                old.artifacts[ARTIFACT_PATHS[1]]
            );
            let pending = read_ownership(
                &capture_installation(root, &root.join(RECEIPT))
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(pending.version, 3);
            assert!(pending.owns_artifact(
                ARTIFACT_PATHS[1],
                old.artifacts[ARTIFACT_PATHS[1]].as_bytes()
            ));
            let resumed = prepare(root, "kb");
            assert!(
                resumed.is_ok(),
                "mixed old/new owned artifacts must remain retryable"
            );
            if retry {
                resumed
                    .unwrap()
                    .publish(root, &mut test_report(root))
                    .unwrap();
                let completed = read_ownership(
                    &capture_installation(root, &root.join(RECEIPT))
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert!(completed.previous_artifacts.is_empty());
            }
            remove(root, &mut test_report(root)).unwrap();
            for path in ARTIFACT_PATHS.into_iter().chain([RECEIPT]) {
                assert!(!root.join(path).exists(), "{path}");
            }
        }
    }

    #[test]
    fn artifact_receipt_budget_is_aggregate_and_paths_are_fixed() {
        let mut artifacts = BTreeMap::from([
            (ARTIFACT_PATHS[0].to_owned(), "one".to_owned()),
            (ARTIFACT_PATHS[1].to_owned(), "two".to_owned()),
        ]);
        assert!(valid_artifact_receipts(&artifacts, 6));
        assert!(!valid_artifact_receipts(&artifacts, 5));
        let ownership = Ownership {
            version: 3,
            original: None,
            installed: Value::Object(serde_json::Map::new()),
            runtime_original: None,
            runtime_installed: Value::Object(serde_json::Map::new()),
            artifacts: artifacts.clone(),
            previous_artifacts: BTreeMap::from([(ARTIFACT_PATHS[0].into(), "old".into())]),
        };
        assert!(!ownership.valid_artifact_budget(8));
        assert!(ownership.valid_artifact_budget(9));
        artifacts.insert(".pi/user.js".into(), String::new());
        assert!(!valid_artifact_receipts(&artifacts, 6));
    }
}
