//! Fuzz target: the streaming line/state-machine scanner
//! (`crates/hyalo-core/src/scanner/`) — frontmatter delimiter detection,
//! fenced-code-block tracking, and body-line dispatch, all driven directly
//! off raw file bytes rather than a validated UTF-8 string.
//!
//! `AllEvents` overrides nothing: every `FileVisitor` callback defaults to
//! `ScanAction::Continue`, so this exercises every branch of the scanner's
//! state machine (frontmatter open/close, fence open/close, BOM handling,
//! mixed line endings, oversized-line capping) without any visitor-specific
//! logic getting in the way of the scanner itself being what's under test.

#![no_main]

use hyalo_core::scanner::{FileVisitor, scan_slice_multi};
use libfuzzer_sys::fuzz_target;

struct AllEvents;
impl FileVisitor for AllEvents {}

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let mut visitor = AllEvents;
    let mut visitors: [&mut dyn FileVisitor; 1] = [&mut visitor];
    let _ = scan_slice_multi(data, &mut visitors);
});
