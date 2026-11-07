use super::*;
use std::sync::{Arc, Mutex};

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
fn only_a_single_note_keeps_the_per_file_path() {
    assert_eq!(Durability::for_rewrites(0), Durability::PerFile);
    assert_eq!(Durability::for_rewrites(1), Durability::PerFile);
    assert_eq!(Durability::for_rewrites(2), Durability::BulkRewrite);
    assert_eq!(Durability::for_rewrites(9), Durability::BulkRewrite);
}

fn assert_none<T: std::fmt::Debug>(items: &[T]) {
    assert_eq!(items.len(), 0, "{items:?}");
}

/// Notes `n0.md`.. across `dirs` directories (`d0`..), each holding `old`.
fn vault_of_notes(count: usize, dirs: usize) -> (tempfile::TempDir, VaultRoot, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let names: Vec<String> = (0..count)
        .map(|i| {
            let sub = format!("d{}", i % dirs);
            std::fs::create_dir_all(dir.path().join(&sub)).unwrap();
            format!("{sub}/n{i}.md")
        })
        .collect();
    for name in &names {
        std::fs::write(dir.path().join(name), b"old").unwrap();
    }
    (dir, root, names)
}

fn stage(
    root: &VaultRoot,
    name: &str,
    bytes: &[u8],
    session: &mut WriteSession,
) -> Result<CommitEffect> {
    root.capture(&RelativeName::new(name).unwrap())
        .unwrap()
        .replace_immediate(bytes, session)
}

/// Everything in the vault that is not a note: the temporary files.
fn leftovers(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for sub in std::fs::read_dir(dir).unwrap() {
        let sub = sub.unwrap().path();
        for entry in std::fs::read_dir(&sub).unwrap() {
            let entry = entry.unwrap().path();
            if entry.extension().is_none_or(|e| e != "md") {
                found.push(entry);
            }
        }
    }
    found
}

fn contents(dir: &Path, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| String::from_utf8(std::fs::read(dir.join(n)).unwrap()).unwrap())
        .collect()
}

/// The order of a bulk phase, read off the file system at each step (DEC-408):
/// every temporary file is written before the barrier, no note has its new
/// bytes at the barrier, every rename comes after it, and the directory fences
/// come last. This proves the order the code runs in; a test cannot cut power.
#[test]
fn a_bulk_phase_flushes_everything_then_renames_then_fences() {
    let (dir, root, names) = vault_of_notes(6, 2);
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let probe: Arc<dyn Fn(&str) + Send + Sync> = {
        let events = Arc::clone(&events);
        let base = dir.path().to_path_buf();
        let names = names.clone();
        Arc::new(move |event: &str| {
            if event == "barrier" {
                // At the barrier all six temporary files exist and every
                // note still has its old bytes.
                assert_eq!(leftovers(&base).len(), 6, "temporary files at the barrier");
                assert!(contents(&base, &names).iter().all(|c| c == "old"));
            }
            if event.starts_with("rename ") {
                let seen = events.lock().unwrap();
                assert!(seen.iter().any(|e| e == "barrier"), "rename before barrier");
            }
            events.lock().unwrap().push(event.to_owned());
        })
    };
    let mut session = WriteSession::with_probe(Durability::BulkRewrite, probe);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    assert_eq!(session.staged_notes(), 6);
    assert!(contents(dir.path(), &names).iter().all(|c| c == "old"));
    let report = session.finish_staged();
    assert_none(report.unpublished());
    assert!(report.finalization_error().is_none());
    assert!(contents(dir.path(), &names).iter().all(|c| c == "new"));
    assert_none(&leftovers(dir.path()));

    let events = events.lock().unwrap();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| e.split_whitespace().next().unwrap())
        .filter(|k| *k != "barrier-device")
        .collect();
    assert_eq!(
        kinds,
        [
            "stage", "stage", "stage", "stage", "stage", "stage", "barrier", "rename", "rename",
            "rename", "rename", "rename", "rename", "fence", "fence"
        ]
    );
    // One device in a temp dir, so one barrier (none where there is no barrier).
    let devices = events
        .iter()
        .filter(|e| e.starts_with("barrier-device"))
        .count();
    assert_eq!(devices, usize::from(cfg!(unix)));
}

/// A concurrent `hyalo` read sees the staged files neither as notes nor as
/// attachments: they are hidden and extension-less until they are renamed.
#[test]
fn staged_temporary_files_are_invisible_to_a_vault_scan() {
    let (dir, root, names) = vault_of_notes(5, 2);
    let mut session = WriteSession::new(Durability::BulkRewrite);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    assert_eq!(leftovers(dir.path()).len(), 5);
    let found = crate::discovery::discover_files(dir.path()).unwrap();
    assert_eq!(found.len(), 5, "{found:?}");
    assert!(
        found
            .iter()
            .all(|p| p.extension().is_some_and(|e| e == "md"))
    );
    assert_none(&crate::discovery::discover_attachments(dir.path()).unwrap());
    drop(session);
}

/// A vault can span filesystems: one barrier per `st_dev`, not one per phase.
#[cfg(unix)]
#[test]
fn a_bulk_phase_runs_one_barrier_per_device() {
    let (dir, root, names) = vault_of_notes(4, 2);
    let barriers = Arc::new(Mutex::new(0usize));
    let probe: Arc<dyn Fn(&str) + Send + Sync> = {
        let barriers = Arc::clone(&barriers);
        Arc::new(move |event: &str| {
            if event.starts_with("barrier-device") {
                *barriers.lock().unwrap() += 1;
            }
        })
    };
    let mut session = WriteSession::with_probe(Durability::BulkRewrite, probe);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    // Pretend the two directories sit on two mounts.
    let d0 = dir.path().canonicalize().unwrap().join("d0");
    for (path, note) in &mut session.staged {
        note.device = Some(if path.starts_with(&d0) { 1 } else { 2 });
    }
    assert_none(session.finish_staged().unpublished());
    assert_eq!(*barriers.lock().unwrap(), 2);
}

#[test]
fn a_staging_fault_leaves_every_note_untouched_and_no_temporary_file() {
    for fault in [FaultPoint::Prepare, FaultPoint::SyncFile] {
        let (dir, root, names) = vault_of_notes(6, 2);
        // The fault lands on the fourth note; three are already staged.
        let mut session = WriteSession::with_fault_after(Durability::BulkRewrite, fault, 3);
        let staged: Vec<bool> = names
            .iter()
            .map(|n| stage(&root, n, b"new", &mut session).is_ok())
            .collect();
        assert_eq!(staged[..3], [true; 3], "{fault:?}");
        assert!(!staged[3], "{fault:?}");
        let report = session.finish_staged();
        assert!(
            contents(dir.path(), &names).iter().all(|c| c == "old"),
            "{fault:?}: no note may change after a failed staging step"
        );
        assert!(leftovers(dir.path()).is_empty(), "{fault:?}");
        assert!(report.unpublished().len() >= 3, "{fault:?}");
        assert!(report.into_result().is_err());
    }
}

#[test]
fn a_write_error_while_staging_is_a_staging_fault() {
    // What a full disk looks like: the copy into the temporary file fails.
    let (dir, root, names) = vault_of_notes(4, 2);
    let mut session = WriteSession::new(Durability::BulkRewrite);
    stage(&root, &names[0], b"new", &mut session).unwrap();
    let source = root
        .capture(&RelativeName::new(names[1].as_str()).unwrap())
        .unwrap();
    let failed = source.stage_replacement(&session, |_| {
        Err(std::io::Error::from_raw_os_error(28).into())
    });
    assert!(failed.is_err());
    stage(&root, &names[2], b"new", &mut session).unwrap();
    let report = session.finish_staged();
    assert!(contents(dir.path(), &names).iter().all(|c| c == "old"));
    assert_none(&leftovers(dir.path()));
    assert_eq!(report.unpublished().len(), 2);
}

#[test]
fn a_failed_barrier_renames_nothing() {
    let (dir, root, names) = vault_of_notes(5, 2);
    let mut session = WriteSession::with_fault(Durability::BulkRewrite, FaultPoint::Barrier);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    let report = session.finish_staged();
    assert_eq!(report.unpublished().len(), 5);
    assert!(contents(dir.path(), &names).iter().all(|c| c == "old"));
    assert_none(&leftovers(dir.path()));
}

#[test]
fn a_failed_rename_is_reported_and_the_others_are_written() {
    let (dir, root, names) = vault_of_notes(6, 2);
    let mut session =
        WriteSession::with_fault_after(Durability::BulkRewrite, FaultPoint::Persist, 2);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    let report = session.finish_staged();
    let now = contents(dir.path(), &names);
    // Renames run in path order: `d0/n0 d0/n2 d0/n4 d1/n1 d1/n3 d1/n5`.
    let order = [
        "d0/n0.md", "d0/n2.md", "d0/n4.md", "d1/n1.md", "d1/n3.md", "d1/n5.md",
    ];
    let state: Vec<_> = order
        .iter()
        .map(|o| now[names.iter().position(|n| n == o).unwrap()].as_str())
        .collect();
    assert_eq!(state, ["new", "new", "old", "old", "old", "old"]);
    let unpublished: Vec<_> = report
        .unpublished()
        .iter()
        .map(Unpublished::entry)
        .collect();
    assert_eq!(unpublished, order[2..]);
    assert_none(&leftovers(dir.path()));
    assert!(report.finalization_error().is_none());
}

#[test]
fn a_note_edited_while_staged_is_not_overwritten_and_the_rest_are() {
    let (dir, root, names) = vault_of_notes(4, 2);
    let mut session = WriteSession::new(Durability::BulkRewrite);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    // The edit lands after staging, with the size and mtime unchanged.
    let edited = dir.path().join(&names[1]);
    let before = std::fs::metadata(&edited).unwrap().modified().unwrap();
    std::fs::write(&edited, b"mod").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&edited)
        .unwrap()
        .set_modified(before)
        .unwrap();
    let report = session.finish_staged();
    assert_eq!(std::fs::read(&edited).unwrap(), b"mod");
    let unpublished = report.unpublished();
    assert_eq!(unpublished.len(), 1);
    assert_eq!(unpublished[0].entry(), names[1]);
    assert!(unpublished[0].is_source_conflict());
    let now = contents(dir.path(), &names);
    assert_eq!(now, ["new", "mod", "new", "new"]);
    assert_none(&leftovers(dir.path()));
}

#[test]
fn a_session_dropped_before_finish_changes_nothing_and_leaves_no_temporary_file() {
    let (dir, root, names) = vault_of_notes(4, 2);
    let mut session = WriteSession::new(Durability::BulkRewrite);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    assert_eq!(leftovers(dir.path()).len(), 4);
    drop(session);
    assert!(contents(dir.path(), &names).iter().all(|c| c == "old"));
    assert_none(&leftovers(dir.path()));
}

/// A staged write is invisible until the phase ends, so a second write to the
/// same note must start from the staged bytes and replace the first, not read
/// the old file and not add a second rename.
#[test]
fn a_note_written_twice_in_one_phase_is_renamed_once_with_the_second_bytes() {
    let (dir, root, names) = vault_of_notes(2, 1);
    let mut session = WriteSession::new(Durability::BulkRewrite);
    let mut worker = session.worker();
    let (_, receipt) = root
        .capture(&RelativeName::new(names[0].as_str()).unwrap())
        .unwrap()
        .prepare(b"first", &worker)
        .unwrap()
        .commit_with_receipt(&mut worker)
        .unwrap();
    assert_eq!(std::fs::read(dir.path().join(&names[0])).unwrap(), b"old");
    let fresh = receipt.capture_verified().unwrap();
    assert_eq!(fresh.bytes().unwrap(), b"first");
    fresh.verify().unwrap();
    let (_, second_receipt) = fresh
        .prepare(b"second", &worker)
        .unwrap()
        .commit_with_receipt(&mut worker)
        .unwrap();
    assert_eq!(
        second_receipt.capture_verified().unwrap().bytes().unwrap(),
        b"second"
    );
    session.absorb(worker);
    assert_eq!(session.staged_notes(), 1);
    assert_eq!(
        leftovers(dir.path()).len(),
        1,
        "the first temporary file is gone"
    );
    let report = session.finish_staged();
    assert_none(report.unpublished());
    assert_eq!(
        std::fs::read(dir.path().join(&names[0])).unwrap(),
        b"second"
    );
    assert_none(&leftovers(dir.path()));
    // After the rename the receipt reads the file as published.
    assert_eq!(
        second_receipt.capture_verified().unwrap().bytes().unwrap(),
        b"second"
    );
}

#[test]
fn a_second_write_still_guards_against_an_edit_since_the_first_read() {
    let (dir, root, names) = vault_of_notes(1, 1);
    let mut session = WriteSession::new(Durability::BulkRewrite);
    let (_, receipt) = root
        .capture(&RelativeName::new(names[0].as_str()).unwrap())
        .unwrap()
        .prepare(b"first", &session)
        .unwrap()
        .commit_with_receipt(&mut session)
        .unwrap();
    let fresh = receipt.capture_verified().unwrap();
    std::fs::write(dir.path().join(&names[0]), b"mod").unwrap();
    let error = fresh.verify().unwrap_err();
    assert!(error.downcast_ref::<SourceConflict>().is_some());
    assert!(receipt.capture_verified().is_err());
    let report = session.finish_staged();
    assert_eq!(report.unpublished().len(), 1);
    assert_eq!(std::fs::read(dir.path().join(&names[0])).unwrap(), b"mod");
}

/// A staged note holds a path, not a descriptor: a phase may stage more notes
/// than the process may keep open.
#[cfg(any(target_os = "linux", target_vendor = "apple"))]
#[test]
fn staging_does_not_hold_a_descriptor_per_note() {
    fn open_descriptors() -> usize {
        std::fs::read_dir(if cfg!(target_os = "linux") {
            "/proc/self/fd"
        } else {
            "/dev/fd"
        })
        .unwrap()
        .count()
    }
    let (_dir, root, names) = vault_of_notes(1500, 3);
    let before = open_descriptors();
    let mut session = WriteSession::new(Durability::BulkRewrite);
    for name in &names {
        stage(&root, name, b"new", &mut session).unwrap();
    }
    let during = open_descriptors();
    assert!(
        during < before + 300,
        "{before} descriptors before staging 1500 notes, {during} after"
    );
    assert_none(session.finish_staged().unpublished());
}

#[test]
fn immediate_replacement_keeps_durability_faults_and_cleans_sibling_temps() {
    for durability in [
        Durability::PerFile,
        Durability::PerDirectory,
        Durability::BulkRewrite,
    ] {
        for fault in [
            None,
            Some(FaultPoint::Prepare),
            Some(FaultPoint::SyncFile),
            Some(FaultPoint::Persist),
            Some(FaultPoint::Finalize),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("note.md");
            std::fs::write(&path, b"old").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
            }
            let root = VaultRoot::new(dir.path()).unwrap();
            let source = root
                .capture(&RelativeName::new("note.md").unwrap())
                .unwrap();
            let mut session = fault.map_or_else(
                || WriteSession::new(durability),
                |fault| WriteSession::with_fault(durability, fault),
            );
            let result = source.replace_immediate(b"new", &mut session);
            let bulk = durability == Durability::BulkRewrite;
            // Under `BulkRewrite` the rename belongs to `finish`, so `Persist`
            // fails there and not here.
            let accepted = match fault {
                Some(FaultPoint::Prepare | FaultPoint::SyncFile) => false,
                Some(FaultPoint::Persist) => bulk,
                _ => true,
            };
            assert_eq!(result.is_ok(), accepted);
            if let Ok(effect) = &result {
                assert_eq!(effect.operation(), Operation::Replaced);
                assert_eq!(
                    effect.finalization_error().is_some(),
                    fault == Some(FaultPoint::Finalize) && !bulk
                );
            }
            let report = session.finish_staged();
            let written = !matches!(
                fault,
                Some(FaultPoint::Prepare | FaultPoint::Persist | FaultPoint::SyncFile)
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                if written { b"new" } else { b"old" }
            );
            assert_eq!(
                report.finalization_error().is_some(),
                fault == Some(FaultPoint::Finalize)
            );
            assert_eq!(
                report.unpublished().len(),
                usize::from(bulk && fault == Some(FaultPoint::Persist))
            );
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                1,
                "sibling temporary file must be cleaned up"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o640
                );
            }
        }
    }
}

#[test]
fn immediate_replacement_checks_source_before_staging_and_before_publication() {
    for durability in [Durability::PerFile, Durability::BulkRewrite] {
        for change_during_staging in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("note.md");
            std::fs::write(&path, b"old").unwrap();
            let root = VaultRoot::new(dir.path()).unwrap();
            let source = root
                .capture(&RelativeName::new("note.md").unwrap())
                .unwrap();
            let mut session = WriteSession::new(durability);
            let result = if change_during_staging {
                // The immediate and receipt paths share these exact two helpers.
                // Introduce an equal-length editor change after the first verify.
                let (target, temp) = source
                    .stage_replacement(&session, |temp| {
                        temp.write_all(b"new")?;
                        std::fs::write(&path, b"mod")?;
                        Ok(())
                    })
                    .unwrap();
                source.publish_replacement(&target, temp, &mut session)
            } else {
                std::fs::write(&path, b"mod").unwrap();
                source.replace_immediate(b"new", &mut session)
            };
            let queued_until_finish =
                durability == Durability::BulkRewrite && change_during_staging;
            if queued_until_finish {
                // The second check moved to the rename of `finish`.
                result.unwrap();
                let report = session.finish_staged();
                assert!(report.unpublished()[0].is_source_conflict());
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .downcast_ref::<SourceConflict>()
                        .is_some()
                );
                session.finish().unwrap();
            }
            assert_eq!(std::fs::read(&path).unwrap(), b"mod");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}

#[test]
fn sized_source_comparison_preserves_exact_eof_checks() {
    for size in [0, 1, 64 * 1024 + 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        let bytes = vec![b'a'; size];
        std::fs::write(&path, &bytes).unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root
            .capture(&RelativeName::new("note.md").unwrap())
            .unwrap();
        source.verify().unwrap();
        let mut extended = bytes;
        extended.push(b'b');
        std::fs::write(&path, &extended).unwrap();
        assert!(
            source
                .verify()
                .unwrap_err()
                .downcast_ref::<SourceConflict>()
                .is_some()
        );
        if size > 0 {
            std::fs::write(&path, &extended[..size - 1]).unwrap();
            assert!(
                source
                    .verify()
                    .unwrap_err()
                    .downcast_ref::<SourceConflict>()
                    .is_some()
            );
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
    assert_eq!(session.pending_directories(), 2);
    assert!(session.finish().is_err());
    for name in ["a", "b", "nested/c"] {
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"new");
    }
}

/// `lint --fix` (codebase review 2026-10-03 item 3, DEC-317 follow-up) gives
/// each file processed on the rayon pool its own `.worker()` and absorbs
/// every one back into a single top-level session, exactly like the
/// `BulkRewrite` pattern above. This pins the actual deduplication a 10-file
/// fix run depends on: ten files sharing two directories must leave the
/// top-level session queuing two directory fences, not ten — the whole point
/// of batching `Durability::PerDirectory`'s deferred fsync instead of paying
/// it once per file (`Durability::PerFile`, which never queues at all).
#[test]
fn lint_fix_shaped_workers_batch_per_directory_fsync_by_directory_not_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    let root = VaultRoot::new(dir.path()).unwrap();
    let mut session = WriteSession::new(Durability::PerDirectory);
    let names: Vec<String> = (0..10)
        .map(|i| {
            if i % 2 == 0 {
                format!("note{i}.md")
            } else {
                format!("nested/note{i}.md")
            }
        })
        .collect();
    for name in &names {
        std::fs::write(dir.path().join(name), b"old").unwrap();
    }
    for name in &names {
        let mut worker = session.worker();
        let (effect, _receipt) = root
            .capture(&RelativeName::new(name.as_str()).unwrap())
            .unwrap()
            .prepare(b"new", &worker)
            .unwrap()
            .commit_with_receipt(&mut worker)
            .unwrap();
        // `record()` under `PerDirectory` only queues the directory; it never
        // fsyncs inline, so every one of the ten per-file commits reports a
        // clean `Committed`-shaped effect with no finalization error yet.
        assert!(effect.finalization_error().is_none());
        session.absorb(worker);
    }
    assert_eq!(
        session.directories.len(),
        2,
        "ten files across two directories must queue exactly two fences, \
         not one per file"
    );
    assert!(session.finish().is_ok());
    for name in &names {
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

#[test]
fn windows_reserved_and_stream_names_are_not_portable() {
    for bad in [
        "CON.md",
        "nul.md",
        "Aux",
        "com1.md",
        "LPT9.txt",
        "a:b.md",
        "a.md.",
        "a.md ",
        "con.tar.md",
    ] {
        assert!(!is_portable_windows_name(bad), "{bad}");
    }
    for good in [
        "note.md",
        "console.md",
        "com0.md",
        "com10.md",
        "lpt.md",
        "a b.md",
        "con-x.md",
    ] {
        assert!(is_portable_windows_name(good), "{good}");
    }
}
