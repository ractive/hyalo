//! Exact-content ownership of optional tidy resources for Claude and Codex.
//! Pi uses the same assets through its existing manifest receipts.
use super::{
    Report, capture_installation, ensure_installation_dir, publish_captured_installation,
    remove_captured_installation_artifact, remove_dir_if_empty,
};
use anyhow::{Context, Result, ensure};
use hyalo_core::rooted::CapturedInput;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) const REFERENCE: &str = include_str!("../../../templates/jev/jev.md");
pub(super) const SCRIPT: &str = include_str!("../../../templates/jev/jev.mjs");
pub(super) const ASSETS: [(&str, &str); 2] = [
    ("references/jev.md", REFERENCE),
    ("scripts/jev.mjs", SCRIPT),
];
const RECEIPT: &str = ".hyalo-jev-assets.json";
const MAX_RECEIPT: usize = 512 * 1024;

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    artifacts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    previous: BTreeMap<String, String>,
}
impl Receipt {
    fn owns(&self, path: &str, bytes: &[u8]) -> bool {
        self.artifacts
            .get(path)
            .into_iter()
            .chain(self.previous.get(path))
            .any(|content| content.as_bytes() == bytes)
    }
}

fn read_receipt(source: &CapturedInput) -> Result<Receipt> {
    let bytes = source.bytes()?;
    ensure!(bytes.len() <= MAX_RECEIPT, "oversized Jev asset receipt");
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .context("invalid Jev asset receipt; preserve it for inspection")?;
    ensure!(
        receipt.version == 1
            && receipt
                .artifacts
                .keys()
                .chain(receipt.previous.keys())
                .all(|key| ASSETS.iter().any(|(path, _)| path == key)),
        "unrecognized Jev asset receipt"
    );
    Ok(receipt)
}

/// Capture all paths before any configuration or installation writes. Rooted
/// capture refuses leaf and parent symlinks even when a resource is unowned.
pub(super) fn preflight_removal(root: &Path, skill: &str) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for name in ASSETS.iter().map(|(path, _)| *path).chain([RECEIPT]) {
        let mut path = root.to_path_buf();
        for component in Path::new(skill).join(name).components() {
            path.push(component);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => ensure!(
                    !metadata.file_type().is_symlink(),
                    "Jev asset path is a symlink: {}",
                    path.display()
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        let source = capture_installation(root, &root.join(skill).join(name))?;
        if name == RECEIPT
            && let Some(source) = source
        {
            read_receipt(&source)?;
        }
    }
    Ok(())
}

struct Asset {
    name: &'static str,
    content: &'static str,
    source: Option<CapturedInput>,
}
pub(super) struct Plan {
    skill: &'static str,
    receipt_source: Option<CapturedInput>,
    receipt: Receipt,
    assets: Vec<Asset>,
}
pub(super) fn prepare(root: &Path, skill: &'static str) -> Result<Plan> {
    preflight_removal(root, skill)?;
    let receipt_source = if root.exists() {
        capture_installation(root, &root.join(skill).join(RECEIPT))?
    } else {
        None
    };
    let old = receipt_source.as_ref().map(read_receipt).transpose()?;
    let mut receipt = Receipt {
        version: 1,
        ..Receipt::default()
    };
    let mut assets = Vec::new();
    for (name, content) in ASSETS {
        let source = if root.exists() {
            capture_installation(root, &root.join(skill).join(name))?
        } else {
            None
        };
        if let Some(source) = &source {
            let bytes = source.bytes()?;
            ensure!(
                old.as_ref().is_some_and(|r| r.owns(name, &bytes)),
                "Jev asset {skill}/{name} is unowned or changed; preserve or relocate it before init"
            );
            if bytes != content.as_bytes() {
                receipt
                    .previous
                    .insert(name.to_owned(), String::from_utf8(bytes)?);
            }
        }
        receipt
            .artifacts
            .insert(name.to_owned(), content.to_owned());
        assets.push(Asset {
            name,
            content,
            source,
        });
    }
    ensure!(
        serde_json::to_vec(&receipt)?.len() <= MAX_RECEIPT,
        "oversized Jev asset receipt"
    );
    Ok(Plan {
        skill,
        receipt_source,
        receipt,
        assets,
    })
}
impl Plan {
    pub(super) fn publish(self, root: &Path, report: &mut Report) -> Result<()> {
        let skill_path = root.join(self.skill);
        for directory in ["references", "scripts"] {
            ensure_installation_dir(root, &skill_path.join(directory), report)?;
        }
        let receipt_path = skill_path.join(RECEIPT);
        let bytes = serde_json::to_vec(&self.receipt)?;
        let existed = self.receipt_source.is_some();
        // Write the receipt first with both old and pending contents. An
        // interrupted upgrade remains recognizable on retry or deinit.
        publish_captured_installation(root, &receipt_path, self.receipt_source, &bytes)?;
        report.push(
            if existed { "updated" } else { "created" },
            format!("{}/{RECEIPT}", self.skill),
        );
        for asset in self.assets {
            let label = format!("{}/{}", self.skill, asset.name);
            if asset
                .source
                .as_ref()
                .map(CapturedInput::bytes)
                .transpose()?
                .as_deref()
                == Some(asset.content.as_bytes())
            {
                report.push("unchanged", label);
                continue;
            }
            let existed = asset.source.is_some();
            publish_captured_installation(
                root,
                &skill_path.join(asset.name),
                asset.source,
                asset.content.as_bytes(),
            )?;
            report.push(if existed { "updated" } else { "created" }, label);
        }
        Ok(())
    }
}

pub(super) fn remove(root: &Path, skill: &str, report: &mut Report) -> Result<()> {
    preflight_removal(root, skill)?;
    let path = root.join(skill);
    let receipt_source = capture_installation(root, &path.join(RECEIPT))?;
    let receipt = receipt_source.as_ref().map(read_receipt).transpose()?;
    let mut retained = false;
    for (name, _) in ASSETS {
        if let Some(source) = capture_installation(root, &path.join(name))? {
            let label = format!("{skill}/{name}");
            if receipt
                .as_ref()
                .is_some_and(|r| source.bytes().is_ok_and(|bytes| r.owns(name, &bytes)))
            {
                remove_captured_installation_artifact(root, &path.join(name), source)?;
                report.push("removed", label);
            } else {
                retained = true;
                report.push_detail("skipped", label, "unowned or changed Jev asset preserved");
            }
        }
    }
    if !retained && let Some(source) = receipt_source {
        remove_captured_installation_artifact(root, &path.join(RECEIPT), source)?;
        report.push("removed", format!("{skill}/{RECEIPT}"));
    }
    for directory in ["references", "scripts"] {
        remove_dir_if_empty(
            &path.join(directory),
            &format!("{skill}/{directory}"),
            report,
        )?;
    }
    Ok(())
}
