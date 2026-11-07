//! The flush half of a bulk rewrite phase (DEC-408): what makes the content of
//! every staged temporary file reach the device before the first rename.
//!
//! The order is the whole guarantee, so each platform gets the cheapest
//! primitive that still puts the content on stable storage first:
//!
//! * macOS: [`flush_content`] is a plain `fsync` per file (data to the drive,
//!   drive cache not flushed, 0.03 ms a file measured on APFS) and [`barrier`]
//!   one `F_FULLFSYNC` per device, which flushes the drive cache for everything
//!   written before it. Rust's `File::sync_all` is `F_FULLFSYNC` on every call,
//!   5.5 ms each, and does not amortise across threads. A volume that refuses
//!   `F_FULLFSYNC` (some network filesystems) keeps the per-file `fsync`, the
//!   strongest primitive it offers.
//! * Linux and Android: [`flush_content`] does nothing and [`barrier`] is one
//!   `syncfs` per filesystem, which writes out all of its dirty data. It reports
//!   a writeback error only on Linux 5.8 and later; on an older kernel an error
//!   that happened before the call can be missed.
//! * Windows and other Unix: [`flush_content`] is `sync_all` per file
//!   (`FlushFileBuffers`), there is no unprivileged volume barrier and
//!   [`barrier`] does nothing.
//!
//! Devices are told apart by `st_dev` ([`device_of`]); a vault can span
//! filesystems through nested mounts, and a barrier on one does not cover another.

use std::fs::File;
use std::io;
use std::path::Path;

/// The device `file` lives on, or `None` where [`barrier`] has nothing to do.
#[cfg(unix)]
pub(super) fn device_of(file: &File) -> io::Result<Option<u64>> {
    use std::os::unix::fs::MetadataExt;
    Ok(Some(file.metadata()?.dev()))
}
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn device_of(_file: &File) -> io::Result<Option<u64>> {
    Ok(None)
}

/// Push `file`'s content to the device. Call it with the handle that wrote it.
#[cfg(target_vendor = "apple")]
pub(super) fn flush_content(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    loop {
        // SAFETY: `fd` is a valid descriptor owned by `file` for the whole call,
        // and `fsync` reads no memory of ours.
        if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "android"))]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn flush_content(_file: &File) -> io::Result<()> {
    Ok(())
}
#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
pub(super) fn flush_content(file: &File) -> io::Result<()> {
    file.sync_all()
}

/// Make everything written to the device that holds `anchor` (any directory or
/// file on it) durable. Run once per device, after every [`flush_content`].
#[cfg(target_vendor = "apple")]
pub(super) fn barrier(anchor: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let handle = File::open(anchor)?;
    loop {
        // SAFETY: `fd` is a valid descriptor owned by `handle` for the whole
        // call; `F_FULLFSYNC` takes no argument and reads no memory of ours.
        if unsafe { libc::fcntl(handle.as_raw_fd(), libc::F_FULLFSYNC) } != -1 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EINTR) => {}
            // The volume cannot flush its drive cache (SMB, NFS, FAT): the
            // per-file `fsync` already done is the strongest it offers.
            Some(libc::ENOTSUP | libc::ENOTTY | libc::EINVAL) => return Ok(()),
            _ => return Err(error),
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn barrier(anchor: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let handle = File::open(anchor)?;
    // SAFETY: `fd` is a valid descriptor owned by `handle` for the whole call;
    // `syncfs` reads no memory of ours.
    if unsafe { libc::syncfs(handle.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn barrier(_anchor: &Path) -> io::Result<()> {
    Ok(())
}
