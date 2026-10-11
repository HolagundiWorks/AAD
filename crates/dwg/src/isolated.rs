//! Reading DWG in a separate process.
//!
//! The DWG reader is a third-party library working on files from anywhere. Some damaged or hostile
//! files make it ask for an impossible amount of memory, which aborts the whole process and cannot
//! be caught (not even by `catch_unwind`); others can loop for a very long time or overflow the
//! stack. A drawing that crashes the program loses everything that was open in it, so the reading
//! is done by a short-lived copy of the program itself:
//!
//! ```text
//! parent                                        worker (same exe, `--aad-dwg-worker`)
//!   writes the DWG bytes to its stdin  ───────►  reads them, converts to DXF in-process
//!   reads its stdout (capped), waits  ◄───────  writes `K` + DXF  or  `E` + message, exits 0
//! ```
//!
//! If the worker crashes, hangs or floods its output, the parent sees it as an ordinary error
//! ("the DWG reader stopped on this file") and carries on. The cost is a process start per DWG
//! opened (tens of milliseconds) and one copy of the bytes through a pipe.
//!
//! Isolation is opt-in per process ([`enable`], [`enable_with`]): the applications turn it on after
//! calling [`run_worker_if_requested`] first thing in `main`. Library users and tests that do not
//! keep the in-process reader, which is only as safe as the library (see `docs/BUG_BACKLOG.md`).
//!
//! Limits: the worker's memory is not capped (that needs operating-system job objects, which need
//! `unsafe`, forbidden here), so a file that makes it allocate a large but possible amount still
//! uses that memory until it finishes or times out.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::{Limits, mb};

/// The argument that makes a program act as the DWG worker.
pub const WORKER_FLAG: &str = "--aad-dwg-worker";
/// Longest a single DWG may take to convert.
pub const TIMEOUT: Duration = Duration::from_secs(180);

static WORKER: OnceLock<PathBuf> = OnceLock::new();

/// Turn isolation on, using this program as the worker. `false` if the program's path is unknown.
pub fn enable() -> bool {
    match std::env::current_exe() {
        Ok(p) => enable_with(p),
        Err(_) => false,
    }
}

/// Turn isolation on, using `exe` (a program that calls [`run_worker_if_requested`]) as the worker.
/// Returns whether this call set it (it can be set once per process).
pub fn enable_with(exe: PathBuf) -> bool {
    WORKER.set(exe).is_ok()
}

/// The worker program, when isolation is on.
pub fn worker() -> Option<&'static Path> {
    WORKER.get().map(PathBuf::as_path)
}

/// If this process was started as the worker, do the work and exit; otherwise return at once. Call
/// it first thing in `main`, before anything else starts.
pub fn run_worker_if_requested() {
    if std::env::args().nth(1).as_deref() != Some(WORKER_FLAG) {
        return;
    }
    let mut input = Vec::new();
    let limits = Limits::DEFAULT;
    let read = std::io::stdin().lock().take(limits.max_dwg_bytes as u64 + 1).read_to_end(&mut input);
    let reply: Vec<u8> = match read {
        Err(e) => frame(b'E', format!("DWG: the reader process could not read its input: {e}").as_bytes()),
        Ok(_) => match crate::native::dwg_to_dxf_with(&input, limits) {
            Ok(dxf) => frame(b'K', &dxf),
            Err(e) => frame(b'E', e.as_bytes()),
        },
    };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&reply);
    let _ = out.flush();
    std::process::exit(0);
}

fn frame(status: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 1);
    v.push(status);
    v.extend_from_slice(body);
    v
}

/// What the parent makes of the worker's output and exit status.
pub(crate) fn decode_reply(success: bool, code: Option<i32>, out: &[u8]) -> Result<Vec<u8>, String> {
    let crashed = || {
        format!(
            "DWG: the reader stopped on this file (exit status {}); the file is damaged or not a drawing AAD can read",
            code.map_or("unknown".to_string(), |c| format!("{c:#x}"))
        )
    };
    if !success {
        return Err(crashed());
    }
    match out.split_first() {
        Some((b'K', dxf)) => Ok(dxf.to_vec()),
        Some((b'E', msg)) => Err(String::from_utf8_lossy(msg).to_string()),
        _ => Err(crashed()),
    }
}

/// Convert `bytes` with the worker started by `cmd` (its stdin and stdout are replaced).
pub(crate) fn convert_with(mut cmd: Command, bytes: &[u8], limits: Limits, timeout: Duration) -> Result<Vec<u8>, String> {
    if bytes.len() > limits.max_dwg_bytes {
        return Err(format!("DWG: the file is {}, larger than the {} AAD opens", mb(bytes.len() as u64), mb(limits.max_dwg_bytes as u64)));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window for a worker started by the windowed application.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("DWG: the reader process could not be started: {e}"))?;
    let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        return Err("DWG: the reader process has no pipes".into());
    };
    let cap = limits.max_dxf_bytes as u64 + 1;
    let started = Instant::now();
    std::thread::scope(|scope| {
        // The worker may exit (or crash) before it has read everything: a broken pipe is expected.
        let feeder = scope.spawn(move || {
            let _ = stdin.write_all(bytes);
        });
        // Not a scoped thread: if the worker is killed while something it started still holds the
        // pipe open, this thread ends only when that does, and the parent must not wait for it.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.by_ref().take(cap).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
        let mut verdict: Option<Result<std::process::ExitStatus, String>> = None;
        while verdict.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => verdict = Some(Ok(status)),
                Ok(None) => {
                    if started.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        verdict = Some(Err(format!("DWG: reading this file took longer than {} seconds and was stopped", timeout.as_secs().max(1))));
                    } else {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                Err(e) => {
                    let _ = child.kill();
                    verdict = Some(Err(format!("DWG: the reader process could not be waited for: {e}")));
                }
            }
        }
        let _ = feeder.join();
        match verdict {
            Some(Ok(status)) => {
                // A worker that has exited has closed its end, so its output is complete almost at once.
                let out = rx.recv_timeout(Duration::from_secs(10)).unwrap_or_default();
                if out.len() as u64 >= cap {
                    return Err(format!(
                        "DWG: converting this file produced more than {} of drawing data (the limit)",
                        mb(limits.max_dxf_bytes as u64)
                    ));
                }
                decode_reply(status.success(), status.code(), &out)
            }
            Some(Err(e)) => Err(e),
            None => Err("DWG: the reader process produced no result".into()),
        }
    })
}

/// Convert `bytes` in the worker process ([`enable`] must have been called).
pub(crate) fn convert(bytes: &[u8], limits: Limits) -> Option<Result<Vec<u8>, String>> {
    let exe = worker()?;
    let mut cmd = Command::new(exe);
    cmd.arg(WORKER_FLAG);
    Some(convert_with(cmd, bytes, limits, TIMEOUT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_decoded_strictly() {
        assert_eq!(decode_reply(true, Some(0), b"Kdata"), Ok(b"data".to_vec()));
        assert_eq!(decode_reply(true, Some(0), b"K"), Ok(Vec::new()));
        assert_eq!(decode_reply(true, Some(0), b"Eno good"), Err("no good".to_string()));
        for (ok, code, out) in [(true, Some(0), &b""[..]), (true, Some(0), b"?junk"), (false, Some(3), b"Kdata"), (false, None, b"")] {
            let e = decode_reply(ok, code, out).unwrap_err();
            assert!(e.contains("stopped on this file"), "{e}");
        }
        // An abort is reported with its exit status, as the real failure showed (0xc0000409).
        assert!(decode_reply(false, Some(0xc000_0409u32 as i32), b"").unwrap_err().contains("0xc0000409"));
    }

    #[cfg(windows)]
    fn shell(script: &str) -> Command {
        let mut c = Command::new("cmd");
        c.args(["/c", script]);
        c
    }

    #[cfg(not(windows))]
    fn shell(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn a_worker_that_crashes_hangs_or_floods_is_an_error_not_a_crash() {
        let limits = Limits { max_dwg_bytes: 1024, max_dxf_bytes: 64 };
        // Exits with a failure status without answering.
        let e = convert_with(shell("exit 3"), b"AC1018", limits, Duration::from_secs(20)).unwrap_err();
        assert!(e.contains("stopped on this file"), "{e}");
        // Never finishes: stopped at the timeout.
        let slow = if cfg!(windows) { "ping -n 30 127.0.0.1 > nul" } else { "sleep 30" };
        let t = Instant::now();
        let e = convert_with(shell(slow), b"AC1018", limits, Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("took longer"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(10), "the hung worker was killed promptly");
        // A program that cannot be started.
        let e = convert_with(Command::new("this-program-does-not-exist-aad"), b"AC1018", limits, Duration::from_secs(5)).unwrap_err();
        assert!(e.contains("could not be started"), "{e}");
        // Too large an input is refused before any process starts.
        let e = convert_with(Command::new("this-program-does-not-exist-aad"), &[0u8; 2048], limits, Duration::from_secs(5)).unwrap_err();
        assert!(e.contains("larger than"), "{e}");
    }

    #[test]
    fn output_beyond_the_limit_is_cut_off_and_refused() {
        let limits = Limits { max_dwg_bytes: 1024, max_dxf_bytes: 16 };
        // Prints far more than the 16-byte limit.
        let script = if cfg!(windows) {
            "echo KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK"
        } else {
            "printf 'K%.0s' $(seq 1 100)"
        };
        let e = convert_with(shell(script), b"AC1018", limits, Duration::from_secs(20)).unwrap_err();
        assert!(e.contains("more than"), "{e}");
    }
}
