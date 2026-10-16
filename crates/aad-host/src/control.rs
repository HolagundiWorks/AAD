//! Loopback JSON-lines control channel for the WinUI shell.
//!
//! The drawing session is touched only on the thread that calls [`crate::Host::pump`]. This module
//! accepts connections and queues one request per line. Engine methods run there. Methods that
//! drive the egui window are refused.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};

const QUEUED: u8 = 0;
const STARTED: u8 = 1;
const CANCELLED: u8 = 2;
const LINE_CAP: usize = 1_048_576;

pub struct Job {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Value>,
    state: Arc<AtomicU8>,
}

impl Job {
    /// False when the client already timed out and was told the request will not run.
    pub fn begin(&self) -> bool {
        self.state.compare_exchange(QUEUED, STARTED, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }
}

/// Bind `127.0.0.1:port` (`0` picks a free port). Returns the bound port, the job queue and the stop flag.
pub fn start(port: u16) -> Result<(u16, Receiver<Job>, Arc<std::sync::atomic::AtomicBool>), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("control channel failed to bind: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| format!("control channel failed to listen: {e}"))?;
    let bound = listener.local_addr().map_err(|e| format!("control channel has no port: {e}"))?.port();
    let (tx, rx) = channel::<Job>();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::Builder::new()
        .name("aad-control".into())
        .spawn(move || accept_loop(listener, tx, flag))
        .map_err(|e| format!("control channel failed to start: {e}"))?;
    Ok((bound, rx, stop))
}

fn accept_loop(listener: TcpListener, tx: Sender<Job>, stop: Arc<std::sync::atomic::AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                // The listener is non-blocking, and an accepted socket keeps that mode. A read
                // before the client has written would then be `WouldBlock` and drop the command.
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
                let tx = tx.clone();
                let _ = std::thread::Builder::new().name("aad-control-client".into()).spawn(move || serve(stream, tx));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => break,
        }
    }
}

fn serve(stream: std::net::TcpStream, tx: Sender<Job>) {
    let Ok(read) = stream.try_clone() else { return };
    let mut out = stream;
    let mut lines = BufReader::new(read);
    let mut line = String::new();
    loop {
        line.clear();
        match lines.read_line(&mut line) {
            Ok(0) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
            Ok(_) if line.len() > LINE_CAP => {
                let _ = writeln!(out, "{}", json!({"ok": false, "error": "request is too long"}));
            }
            Ok(_) if line.trim().is_empty() => {}
            Ok(_) => {
                let reply = match serde_json::from_str::<Value>(line.trim()) {
                    Ok(msg) => dispatch(&tx, msg),
                    Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}")}),
                };
                if writeln!(out, "{reply}").is_err() {
                    break;
                }
            }
        }
    }
}

fn dispatch(tx: &Sender<Job>, msg: Value) -> Value {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let (reply_tx, reply_rx) = channel();
    let state = Arc::new(AtomicU8::new(QUEUED));
    let job = Job { method, params, reply: reply_tx, state: state.clone() };
    if tx.send(job).is_err() {
        return json!({"id": id, "ok": false, "error": "the drawing window is closed"});
    }
    match reply_rx.recv_timeout(Duration::from_secs(60)) {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("id".into(), id);
            }
            value
        }
        Err(_) => {
            if state.compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                json!({"id": id, "ok": false, "error": "timeout", "state": "not-run"})
            } else {
                reply_rx.try_recv().unwrap_or_else(|_| json!({"id": id, "ok": false, "error": "timeout", "state": "may-have-run"}))
            }
        }
    }
}
