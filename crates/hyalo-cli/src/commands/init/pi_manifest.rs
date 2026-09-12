//! Ownership of the shared Pi manifest. Receipts describe additions, never a filename claim.
use super::{
    Report, capture_installation, publish_captured_installation,
    remove_captured_installation_artifact,
};
use anyhow::{Context, Result};
use hyalo_core::rooted::CapturedInput;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const RUNTIME_MANIFEST: &str = ".pi/lib/package.json";
const MANIFEST: &str = ".pi/package.json";
pub(super) const RECEIPT: &str = ".pi/.hyalo-manifest.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    version: u32,
    original: Option<Value>,
    installed: Value,
    runtime_original: Option<Value>,
    runtime_installed: Value,
}

pub(super) struct Plan {
    source: Option<CapturedInput>,
    runtime_source: Option<CapturedInput>,
    runtime_manifest: Value,
    receipt_source: Option<CapturedInput>,
    manifest: Value,
    ownership: Ownership,
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
        ownership.version == 1
            && ownership.installed.is_object()
            && ownership.original.as_ref().is_none_or(Value::is_object)
            && ownership.runtime_installed.is_object()
            && ownership
                .runtime_original
                .as_ref()
                .is_none_or(Value::is_object),
        "unsupported Pi ownership receipt"
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

pub(super) fn prepare(root: &Path) -> Result<Plan> {
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
        // Module metadata may only change the runtime's own files. An
        // existing shared lib directory may contain CommonJS scripts.
        if root.join(".pi/lib").is_dir() {
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
        version: 1,
        original,
        installed,
        runtime_original,
        runtime_installed,
    };
    Ok(Plan {
        source,
        runtime_source,
        runtime_manifest,
        receipt_source,
        manifest,
        ownership,
    })
}

impl Plan {
    pub(super) fn publish(self, root: &Path, report: &mut Report) -> Result<()> {
        // Publish ownership first: a later manifest failure leaves a conservative receipt,
        // whose three-way cleanup still requires the recorded additions to be present.
        let receipt_existed = self.receipt_source.is_some();
        publish_captured_installation(
            root,
            &root.join(RECEIPT),
            self.receipt_source,
            &serde_json::to_vec_pretty(&self.ownership)?,
        )?;
        report.push(
            if receipt_existed {
                "updated"
            } else {
                "created"
            },
            RECEIPT,
        );
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
    original: Option<&Value>,
    installed: &Value,
    report: &mut Report,
) -> Result<bool> {
    if let Some(source) = capture_installation(root, &root.join(manifest_path))? {
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

pub(super) fn remove(root: &Path, report: &mut Report) -> Result<()> {
    let Some(receipt) = capture_installation(root, &root.join(RECEIPT))? else {
        if root.join(MANIFEST).exists() {
            report.push_detail(
                "skipped",
                MANIFEST,
                "no ownership receipt; shared or legacy manifest preserved",
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
    let shared_clean = remove_owned_manifest(
        root,
        MANIFEST,
        ownership.original.as_ref(),
        &ownership.installed,
        report,
    )?;
    let runtime_clean = remove_owned_manifest(
        root,
        RUNTIME_MANIFEST,
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
