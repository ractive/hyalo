//! One-request child processes for untrusted jq. No vault paths or effects cross
//! this boundary. The parent keeps the execution report and owns kill/wait.
use super::{
    JQ_OUTPUT_CAP, JQ_TIME_LIMIT, compile_jq_filter, execute_jq_filter, truncate_diagnostic,
};
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};

const WORKER_ENV: &str = "HYALO_INTERNAL_JQ_WORKER";
const MAGIC: [u8; 4] = *b"HJQ1";
const COMPILE: u8 = 1;
const EVALUATE: u8 = 2;
const READY: u8 = 3;
const OUTPUT: u8 = 4;
const ERROR: u8 = 5;
const SOURCE_CAP: usize = 64 * 1024;
const INPUT_CAP: usize = 64 * 1024 * 1024;
const ERROR_CAP: usize = 4096;
static CANCELLED: AtomicBool = AtomicBool::new(false);
static SIGNAL_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();

fn cancellation() -> Result<&'static AtomicBool, String> {
    SIGNAL_HANDLER
        .get_or_init(|| {
            ctrlc::try_set_handler(|| CANCELLED.store(true, Ordering::Relaxed))
                .map_err(|e| format!("cannot install jq cancellation handler: {e}"))
        })
        .as_ref()
        .map_err(ToOwned::to_owned)?;
    Ok(&CANCELLED)
}

// Header is magic/version, message kind, big-endian payload byte length. Each
// pipe contains exactly one frame followed by EOF. Check length before allocate.
fn write_frame(mut output: impl Write, kind: u8, payload: &[u8]) -> Result<(), String> {
    let length = u32::try_from(payload.len()).map_err(|e| e.to_string())?;
    output.write_all(&MAGIC).map_err(|e| e.to_string())?;
    output.write_all(&[kind]).map_err(|e| e.to_string())?;
    output
        .write_all(&length.to_be_bytes())
        .map_err(|e| e.to_string())?;
    output.write_all(payload).map_err(|e| e.to_string())
}

fn read_frame(mut input: impl Read, kinds: &[(u8, usize)]) -> Result<(u8, Vec<u8>), String> {
    let mut header = [0; 9];
    input
        .read_exact(&mut header)
        .map_err(|e| format!("jq worker protocol header: {e}"))?;
    if header[..4] != MAGIC {
        return Err("jq worker protocol version mismatch".into());
    }
    let kind = header[4];
    let cap = kinds
        .iter()
        .find_map(|(tag, cap)| (*tag == kind).then_some(*cap))
        .ok_or("jq worker protocol unexpected message")?;
    let length = u32::from_be_bytes([header[5], header[6], header[7], header[8]]) as usize;
    if length > cap {
        return Err("jq worker protocol payload exceeds byte limit".into());
    }
    let mut payload = vec![0; length];
    input
        .read_exact(&mut payload)
        .map_err(|e| format!("jq worker protocol payload: {e}"))?;
    let mut trailing = [0];
    if input.read(&mut trailing).map_err(|e| e.to_string())? != 0 {
        return Err("jq worker protocol trailing bytes".into());
    }
    Ok((kind, payload))
}

/// Internal worker entrypoint, before signal setup, configuration or dispatch.
pub(crate) fn compile_worker() -> Option<i32> {
    if std::env::var_os(WORKER_ENV).as_deref() != Some(std::ffi::OsStr::new("1")) {
        return None;
    }
    let result = read_frame(
        std::io::stdin().lock(),
        &[(COMPILE, SOURCE_CAP), (EVALUATE, INPUT_CAP)],
    )
    .and_then(|(kind, payload)| {
        if kind == COMPILE {
            let source = std::str::from_utf8(&payload).map_err(|e| e.to_string())?;
            compile_jq_filter(source).map(|_| (READY, String::new()))
        } else {
            let (source, value): (String, serde_json::Value) =
                serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
            check_source(&source)?;
            let filter = compile_jq_filter(&source)?;
            execute_jq_filter(&filter, &value, &source).map(|text| (OUTPUT, text))
        }
    });
    let (kind, payload) = result.unwrap_or_else(|error| (ERROR, truncate_diagnostic(&error)));
    Some(i32::from(
        write_frame(std::io::stdout().lock(), kind, payload.as_bytes()).is_err(),
    ))
}

fn check_source(source: &str) -> Result<(), String> {
    if source.len() > SOURCE_CAP {
        Err("jq source exceeds 64 KiB".into())
    } else {
        Ok(())
    }
}

pub(crate) fn preflight_jq(source: &str) -> Result<(), String> {
    check_source(source)?;
    bounded_worker(COMPILE, source.as_bytes().to_vec(), READY).map(|_| ())
}

/// Serialize through a capped writer: oversized input is refused before a
/// second unbounded JSON buffer can be built in the parent.
pub(crate) fn evaluate_jq_isolated(
    source: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    check_source(source)?;
    let mut request = CappedBuffer(Vec::new());
    serde_json::to_writer(&mut request, &(source, value)).map_err(|e| e.to_string())?;
    bounded_worker(EVALUATE, request.0, OUTPUT)
}

struct CappedBuffer(Vec<u8>);
impl Write for CappedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > INPUT_CAP.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("jq input exceeds 64 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// All exits after spawn, including thread-start errors and unwinding, kill
/// then wait. The guard must be dropped before joining either pipe thread.
struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn worker_command() -> Result<Command, String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .env(WORKER_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Give terminal SIGINT to the effect-owning parent; it cancels/reaps us.
        command.process_group(0);
        // SAFETY: only async-signal-safe setrlimit calls execute after fork.
        unsafe {
            command.pre_exec(|| {
                for (resource, value) in [(libc::RLIMIT_CPU, 3), (libc::RLIMIT_CORE, 0)] {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &raw const limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    let limit = libc::rlimit {
                        rlim_cur: 512 * 1024 * 1024,
                        rlim_max: 512 * 1024 * 1024,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &raw const limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
    Ok(command)
}

fn bounded_worker(kind: u8, request: Vec<u8>, expected: u8) -> Result<String, String> {
    let cancelled = cancellation()?;
    exchange(
        &mut worker_command()?,
        kind,
        request,
        expected,
        cancelled,
        JQ_TIME_LIMIT,
    )
}

fn exchange(
    command: &mut Command,
    kind: u8,
    request: Vec<u8>,
    expected: u8,
    cancelled: &AtomicBool,
    limit: Duration,
) -> Result<String, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("jq evaluation cancelled".into());
    }
    let deadline = Instant::now() + limit;
    let mut child = Worker(
        command
            .spawn()
            .map_err(|e| format!("cannot start bounded jq worker: {e}"))?,
    );
    // The child cannot compile until it receives our request. Assign before
    // sending it, so an abruptly terminated Windows parent closes the job.
    #[cfg(windows)]
    let _job = windows_job::Job::assign(&child.0)?;
    let input = child.0.stdin.take().ok_or("jq worker stdin unavailable")?;
    let output = child
        .0
        .stdout
        .take()
        .ok_or("jq worker stdout unavailable")?;
    let writer = std::thread::Builder::new()
        .name("jq-input".into())
        .spawn(move || write_frame(input, kind, &request))
        .map_err(|e| format!("cannot start jq pipe writer: {e}"))?;
    let (tx, rx) = mpsc::channel();
    let reader = match std::thread::Builder::new()
        .name("jq-output".into())
        .spawn(move || {
            let result = read_frame(
                output,
                &[
                    (expected, if expected == READY { 0 } else { JQ_OUTPUT_CAP }),
                    (ERROR, ERROR_CAP),
                ],
            );
            let _ = tx.send(result);
        }) {
        Ok(reader) => reader,
        Err(error) => {
            drop(child);
            let _ = writer.join();
            return Err(format!("cannot start jq pipe reader: {error}"));
        }
    };
    let result = wait_response(&mut child.0, &rx, cancelled, deadline);
    // Close the child ends of both pipes before joining. No detached jq work.
    drop(child);
    let written = writer
        .join()
        .map_err(|_| "jq pipe writer panicked".to_owned())
        .and_then(|r| r);
    let _ = reader.join();
    let (tag, payload) = result?;
    written.map_err(|e| format!("jq worker request failed: {e}"))?;
    let text = String::from_utf8(payload).map_err(|e| format!("jq worker protocol UTF-8: {e}"))?;
    if tag == ERROR { Err(text) } else { Ok(text) }
}

#[cfg(windows)]
mod windows_job {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    pub(super) struct Job(HANDLE);
    impl Job {
        pub(super) fn assign(child: &std::process::Child) -> Result<Self, String> {
            // SAFETY: null arguments create an unnamed, non-inheritable job;
            // this guard exclusively owns its handle. Child's handle stays live.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(format!(
                        "cannot create jq worker job: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                let job = Self(handle);
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let size =
                    u32::try_from(std::mem::size_of_val(&info)).map_err(|e| e.to_string())?;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast(),
                    size,
                ) == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle().cast()) == 0
                {
                    return Err(format!(
                        "cannot isolate jq worker job: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                Ok(job)
            }
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: exclusively owned handle from CreateJobObjectW.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

type Response = Result<(u8, Vec<u8>), String>;
fn wait_response(
    child: &mut Child,
    rx: &mpsc::Receiver<Response>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Response {
    let mut response = None;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("jq evaluation cancelled".into());
        }
        if let Ok(message) = rx.try_recv() {
            response = Some(message);
        }
        match child
            .try_wait()
            .map_err(|e| format!("cannot wait for jq worker: {e}"))?
        {
            Some(status) if !status.success() => {
                return Err(format!(
                    "jq worker failed or exceeded its resource budget ({status})"
                ));
            }
            Some(_) if response.is_some() => {
                return response.take().ok_or("jq worker response missing")?;
            }
            _ => {}
        }
        if response.as_ref().is_some_and(Result::is_err) {
            return response.take().ok_or("jq worker response missing")?;
        }
        if Instant::now() >= deadline {
            return Err("jq filter exceeded the 3s time limit".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_refuse_version_kind_oversize_truncation_and_trailing_bytes() {
        let mut valid = Vec::new();
        write_frame(&mut valid, OUTPUT, b"answer").unwrap();
        assert_eq!(read_frame(&valid[..], &[(OUTPUT, 6)]).unwrap().1, b"answer");
        let mut wrong_version = valid.clone();
        wrong_version[3] = b'2';
        assert!(
            read_frame(&wrong_version[..], &[(OUTPUT, 6)])
                .unwrap_err()
                .contains("version")
        );
        assert!(
            read_frame(&valid[..], &[(READY, 0)])
                .unwrap_err()
                .contains("unexpected")
        );
        assert!(
            read_frame(&valid[..9], &[(OUTPUT, 5)])
                .unwrap_err()
                .contains("byte limit")
        );
        assert!(
            read_frame(&valid[..10], &[(OUTPUT, 6)])
                .unwrap_err()
                .contains("payload")
        );
        valid.push(0);
        assert!(
            read_frame(&valid[..], &[(OUTPUT, 6)])
                .unwrap_err()
                .contains("trailing")
        );
        let mut header_only = Vec::new();
        write_frame(&mut header_only, READY, b"").unwrap();
        header_only[5..9].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(
            read_frame(&header_only[..], &[(READY, 0)])
                .unwrap_err()
                .contains("byte limit")
        );
    }

    #[test]
    fn serialization_and_source_caps_refuse_without_appending_excess() {
        let mut buffer = CappedBuffer(vec![0; INPUT_CAP - 2]);
        buffer.write_all(b"ab").unwrap();
        assert!(buffer.write_all(b"c").is_err());
        assert_eq!(buffer.0.len(), INPUT_CAP);
        assert!(check_source(&" ".repeat(SOURCE_CAP + 1)).is_err());
    }

    #[test]
    fn startup_failure_is_an_error_before_any_pipe_threads() {
        let dir = tempfile::tempdir().unwrap();
        let error = exchange(
            &mut Command::new(dir.path().join("missing-worker")),
            COMPILE,
            b".".to_vec(),
            READY,
            &AtomicBool::new(false),
            JQ_TIME_LIMIT,
        )
        .unwrap_err();
        assert!(error.contains("cannot start bounded jq worker"));
    }

    #[test]
    fn cancellation_deadline_and_protocol_failure_kill_and_reap() {
        for mode in ["cancel", "deadline", "protocol"] {
            // A real disposable process. Its work is irrelevant to cleanup;
            // actual jq aborts/timeouts are separately exercised by e2e fixtures.
            let mut child = Worker(
                Command::new(std::env::current_exe().unwrap())
                    .arg("--list")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let id = child.0.id();
            let (tx, rx) = mpsc::channel();
            if mode == "protocol" {
                tx.send(Err("malformed frame".into())).unwrap();
            }
            let result = wait_response(
                &mut child.0,
                &rx,
                &AtomicBool::new(mode == "cancel"),
                Instant::now()
                    + if mode == "deadline" {
                        Duration::ZERO
                    } else {
                        JQ_TIME_LIMIT
                    },
            );
            assert!(result.unwrap_err().contains(match mode {
                "cancel" => "cancelled",
                "deadline" => "time limit",
                _ => "malformed frame",
            }));
            drop(child);
            #[cfg(unix)]
            {
                let mut status = 0;
                // SAFETY: waitpid only observes this test's child, already reaped.
                assert_eq!(
                    unsafe {
                        libc::waitpid(i32::try_from(id).unwrap(), &raw mut status, libc::WNOHANG)
                    },
                    -1
                );
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD)
                );
            }
            #[cfg(windows)]
            {
                use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
                use windows_sys::Win32::System::Threading::{
                    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
                };
                // SAFETY: querying only the disposable PID. Windows may have
                // removed it already, in which case OpenProcess returns null.
                unsafe {
                    let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, id);
                    if !handle.is_null() {
                        assert_eq!(WaitForSingleObject(handle, 0), WAIT_OBJECT_0);
                        CloseHandle(handle);
                    }
                }
            }
        }
    }
}
