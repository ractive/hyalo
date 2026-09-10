//! Captured, comment-preserving configuration replacement.

use anyhow::{Context, Result};
use std::path::Path;

use super::apply::{ApplyReport, EffectFailure, EffectState, IndexDisposition, PathEffect};
use hyalo_core::rooted::{ConfigRoot, Durability, RelativeName, WriteSession};

pub(super) struct CapturedToml {
    doc: toml_edit::DocumentMut,
    source: Option<hyalo_core::rooted::CapturedInput>,
    original: Option<String>,
}

impl std::ops::Deref for CapturedToml {
    type Target = toml_edit::DocumentMut;

    fn deref(&self) -> &Self::Target {
        &self.doc
    }
}

impl std::ops::DerefMut for CapturedToml {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.doc
    }
}

impl CapturedToml {
    pub(super) fn original(&self) -> Option<&str> {
        self.original.as_deref()
    }
}

pub(super) fn capture(path: &Path) -> Result<CapturedToml> {
    let root = ConfigRoot::new(path.parent().context("config has no parent")?)?;
    let name = RelativeName::new(path.file_name().context("config has no name")?)?;
    match root.capture(&name) {
        Ok(source) => {
            let contents = String::from_utf8(source.bytes()?).context("config is not UTF-8")?;
            let doc = contents.parse().context("failed to parse .hyalo.toml")?;
            Ok(CapturedToml {
                doc,
                source: Some(source),
                original: Some(contents),
            })
        }
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(CapturedToml {
                doc: toml_edit::DocumentMut::new(),
                source: None,
                original: None,
            })
        }
        Err(error) => Err(error).context("failed to read .hyalo.toml"),
    }
}

pub(super) fn publish(path: &Path, config: &mut CapturedToml) -> Result<ApplyReport> {
    publish_with_session(path, config, WriteSession::new(Durability::PerFile))
}

pub(super) fn publish_with_session(
    path: &Path,
    config: &mut CapturedToml,
    mut session: WriteSession,
) -> Result<ApplyReport> {
    let rendered = config.to_string();
    if config.original.as_deref() == Some(rendered.as_str()) {
        return Ok(report(path, EffectState::Unchanged, None));
    }
    let root = ConfigRoot::new(path.parent().context("config has no parent")?)?;
    let name = RelativeName::new(path.file_name().context("config has no name")?)?;
    let effect = if let Some(source) = config.source.take() {
        source
            .prepare(rendered.as_bytes(), &session)?
            .commit(&mut session)?
    } else {
        root.destination(name)?
            .create(rendered.as_bytes(), &mut session)?
    };
    let error = effect
        .finalization_error()
        .map(str::to_owned)
        .or_else(|| session.finish().err().map(|error| error.to_string()));
    Ok(report(
        path,
        if error.is_some() {
            EffectState::CommittedWithFinalizationError
        } else {
            EffectState::Committed
        },
        error,
    ))
}

pub(super) fn preview(path: &Path) -> ApplyReport {
    report(path, EffectState::Unchanged, None)
}

fn report(path: &Path, state: EffectState, error: Option<String>) -> ApplyReport {
    ApplyReport {
        paths: vec![PathEffect {
            file: path.display().to_string(),
            state,
            category: error.as_ref().map(|_| EffectFailure::Finalization),
            error,
        }],
        index: IndexDisposition::NotUsed,
        index_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::rooted::FaultPoint;

    #[test]
    fn shared_config_writer_reports_prepare_persist_and_finalize_faults() {
        for fault in [
            FaultPoint::Prepare,
            FaultPoint::Persist,
            FaultPoint::Finalize,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(".hyalo.toml");
            std::fs::write(&path, "# keep\ndir = \".\"\n").unwrap();
            let mut document = capture(&path).unwrap();
            document["format"] = toml_edit::value("json");
            let result = publish_with_session(
                &path,
                &mut document,
                WriteSession::with_fault(Durability::PerFile, fault),
            );
            if fault == FaultPoint::Finalize {
                let report = result.unwrap();
                assert_eq!(
                    report.paths[0].state,
                    EffectState::CommittedWithFinalizationError
                );
                let bytes = std::fs::read_to_string(&path).unwrap();
                assert!(bytes.starts_with("# keep\n"));
                assert!(bytes.contains("format = \"json\""));
            } else {
                assert!(result.is_err());
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    "# keep\ndir = \".\"\n"
                );
            }
        }
    }
}
