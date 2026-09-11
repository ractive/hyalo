use super::*;

#[test]
fn capture_detects_equal_size_edit_and_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"old").unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let name = RelativeName::new("a").unwrap();
    let capture = root.capture(&name).unwrap();
    std::fs::write(&path, b"new").unwrap();
    assert!(
        capture
            .verify()
            .unwrap_err()
            .to_string()
            .contains("conflict")
    );
}

#[test]
fn prepare_and_persist_failures_keep_original_and_finalize_retains_effect() {
    for fault in [
        FaultPoint::Prepare,
        FaultPoint::SyncFile,
        FaultPoint::Persist,
        FaultPoint::Finalize,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a");
        std::fs::write(&path, b"old").unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let mut session = WriteSession::with_fault(Durability::PerFile, fault);
        let result = root
            .capture(&RelativeName::new("a").unwrap())
            .unwrap()
            .prepare(b"new", &session)
            .and_then(|p| p.commit(&mut session));
        if fault == FaultPoint::Finalize {
            assert!(result.unwrap().finalization_error().is_some());
            assert_eq!(std::fs::read(&path).unwrap(), b"new");
        } else {
            assert!(result.is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"old");
        }
    }
}

#[test]
fn newly_introduced_destination_is_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"source").unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let destination = root.destination(RelativeName::new("b").unwrap()).unwrap();
    std::fs::write(dir.path().join("b"), b"other").unwrap();
    let source = root.capture(&RelativeName::new("a").unwrap()).unwrap();
    let mut session = WriteSession::new(Durability::PerFile);
    assert!(move_no_replace(source, destination, &mut session).is_err());
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"source");
    assert_eq!(std::fs::read(dir.path().join("b")).unwrap(), b"other");
    let destination = root.destination(RelativeName::new("c").unwrap()).unwrap();
    std::fs::write(dir.path().join("c"), b"other").unwrap();
    assert!(destination.create(b"new", &mut session).is_err());
    assert_eq!(std::fs::read(dir.path().join("c")).unwrap(), b"other");
}

#[test]
fn sessions_have_independent_finalization() {
    let dir = tempfile::tempdir().unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let failing = WriteSession::with_fault(Durability::PerDirectory, FaultPoint::Finalize);
    let mut healthy = WriteSession::new(Durability::PerDirectory);
    root.destination(RelativeName::new("a").unwrap())
        .unwrap()
        .create(b"a", &mut healthy)
        .unwrap();
    assert!(healthy.finish().is_ok());
    assert!(failing.finish().is_err());
}

#[test]
fn per_directory_replacements_still_flush_file_content() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"old").unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let mut session = WriteSession::with_fault(Durability::PerDirectory, FaultPoint::SyncFile);
    let result = root
        .capture(&RelativeName::new("a").unwrap())
        .unwrap()
        .prepare(b"new", &session)
        .unwrap()
        .commit(&mut session);
    assert!(result.is_err());
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"old");
}

#[test]
fn bulk_workers_transfer_distinct_directories_and_defer_finalization_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let mut session = WriteSession::with_fault(Durability::BulkRewrite, FaultPoint::Finalize);
    for name in ["a", "b", "nested/c"] {
        std::fs::write(dir.path().join(name), b"old").unwrap();
        let mut worker = session.worker();
        let (effect, receipt) = root
            .capture(&RelativeName::new(name).unwrap())
            .unwrap()
            .prepare(b"new", &worker)
            .unwrap()
            .commit_with_receipt(&mut worker)
            .unwrap();
        assert_eq!(effect.operation(), Operation::Replaced);
        assert!(effect.finalization_error().is_none());
        assert_eq!(receipt.capture_verified().unwrap().bytes().unwrap(), b"new");
        session.absorb(worker);
    }
    assert_eq!(session.directories.len(), 2);
    assert!(session.finish().is_err());
    for name in ["a", "b", "nested/c"] {
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"new");
    }
}

#[cfg(unix)]
#[test]
fn external_symlinks_are_refused_and_internal_aliases_keep_the_entry() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"secret").unwrap();
    symlink(outside.path(), dir.path().join("escape")).unwrap();
    symlink(outside.path().join("secret"), dir.path().join("external")).unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let mut session = WriteSession::new(Durability::PerFile);
    for name in ["escape/secret", "external"] {
        let name = RelativeName::new(name).unwrap();
        assert!(root.open(&name).is_err());
        assert!(root.capture(&name).is_err());
        assert!(root.remove_artifact(&name, &mut session).is_err());
    }
    std::fs::write(dir.path().join("real"), b"old").unwrap();
    symlink("real", dir.path().join("alias")).unwrap();
    root.capture(&RelativeName::new("alias").unwrap())
        .unwrap()
        .prepare(b"new", &session)
        .unwrap()
        .commit(&mut session)
        .unwrap();
    assert!(
        std::fs::symlink_metadata(dir.path().join("alias"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(dir.path().join("real")).unwrap(), b"new");
}

#[test]
fn hard_links_share_identity() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"same").unwrap();
    std::fs::hard_link(dir.path().join("a"), dir.path().join("b")).unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let a = root.capture(&RelativeName::new("a").unwrap()).unwrap();
    let b = root.capture(&RelativeName::new("b").unwrap()).unwrap();
    assert_eq!(a.physical_identity(), b.physical_identity());
}

#[test]
fn move_report_distinguishes_retained_source_from_completed_move() {
    for fail in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), b"source").unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root.capture(&RelativeName::new("a").unwrap()).unwrap();
        let destination = root.destination(RelativeName::new("b").unwrap()).unwrap();
        let mut session = if fail {
            WriteSession::with_fault(Durability::PerDirectory, FaultPoint::Remove)
        } else {
            WriteSession::new(Durability::PerDirectory)
        };
        let effect = move_no_replace(source, destination, &mut session).unwrap();
        assert_eq!(
            effect.operation(),
            if fail {
                Operation::Created
            } else {
                Operation::Moved
            }
        );
        assert_eq!(effect.entries().len(), if fail { 1 } else { 2 });
        assert_eq!(effect.entries()[0].operation(), Operation::Created);
        assert_eq!(dir.path().join("a").exists(), fail);
        assert_eq!(effect.finalization_error().is_some(), fail);
        session.finish().unwrap();
    }
}

#[test]
fn move_receipt_refuses_same_bytes_from_a_replacement_object() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"owned bytes").unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let source = root.capture(&RelativeName::new("a").unwrap()).unwrap();
    let destination = root.destination(RelativeName::new("b").unwrap()).unwrap();
    let mut session = WriteSession::new(Durability::PerFile);
    let (_, receipt) = move_no_replace_with_receipt(source, destination, &mut session).unwrap();

    std::fs::remove_file(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("b"), b"owned bytes").unwrap();

    let Err(error) = receipt.capture_verified() else {
        panic!("replacement object must not satisfy the move receipt")
    };
    assert!(error.to_string().contains("identity or bytes changed"));
    assert!(!dir.path().join("a").exists());
    assert_eq!(std::fs::read(dir.path().join("b")).unwrap(), b"owned bytes");
}

#[test]
fn replacement_receipt_refuses_same_bytes_from_a_replacement_object() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"before").unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let captured = root.capture(&RelativeName::new("a").unwrap()).unwrap();
    let mut session = WriteSession::new(Durability::PerFile);
    let (_, receipt) = captured
        .prepare(b"published", &session)
        .unwrap()
        .commit_with_receipt(&mut session)
        .unwrap();

    std::fs::remove_file(dir.path().join("a")).unwrap();
    std::fs::write(dir.path().join("a"), b"published").unwrap();

    let Err(error) = receipt.capture_verified() else {
        panic!("replacement object must not satisfy the rewrite receipt")
    };
    assert!(error.to_string().contains("identity or bytes changed"));
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"published");
}

#[cfg(unix)]
#[test]
fn publication_receipt_refuses_a_symlink_to_the_original_object() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"before").unwrap();
    std::fs::hard_link(dir.path().join("a"), dir.path().join("alias")).unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let source = root.capture(&RelativeName::new("a").unwrap()).unwrap();
    let destination = root.destination(RelativeName::new("b").unwrap()).unwrap();
    let mut session = WriteSession::new(Durability::PerFile);
    let (_, receipt) = move_no_replace_with_receipt(source, destination, &mut session).unwrap();
    std::fs::remove_file(dir.path().join("b")).unwrap();
    symlink("alias", dir.path().join("b")).unwrap();

    assert!(receipt.capture_verified().is_err());
    assert!(
        std::fs::symlink_metadata(dir.path().join("b"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn directory_sync_capability_matches_platform_and_publication_still_finishes() {
    assert_eq!(directory_sync_supported(), cfg!(unix));
    let dir = tempfile::tempdir().unwrap();
    let root = ConfigRoot::new(dir.path()).unwrap();
    let mut session = WriteSession::new(Durability::PerFile);
    let effect = root
        .destination(RelativeName::new("config.toml").unwrap())
        .unwrap()
        .create(b"x = 1", &mut session)
        .unwrap();
    assert!(effect.finalization_error().is_none());
    session.finish().unwrap();
}
