//! Background networking for the Server dialog.
//!
//! The `SERVER` command (aadt-cli) runs on the interpreter, which is `!Send`
//! (Rc-based), so it can't move off the UI thread — and pushing a large drawing
//! is one round-trip per entity, which would freeze the render loop against a
//! remote server. This worker owns the [`VdbClient`] on its **own** thread and
//! talks to the UI over channels: the UI enqueues a [`NetCmd`], the worker does
//! the blocking I/O and replies with a [`NetEvent`], then wakes the UI via
//! `egui::Context::request_repaint`. Only plain owned data crosses the boundary
//! (`Document`, `Vec<Entity>`, `String`), all of which is `Send`.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use aadt_model::{Document, Entity};
use aadt_vdb::VdbClient;

/// A request from the UI to the network worker.
pub enum NetCmd {
    Connect(String),
    Disconnect,
    /// Liveness check on the current connection.
    Ping,
    /// Push a snapshot of the drawing (one record per entity).
    Push(Box<Document>),
    /// Pull every stored entity back.
    Pull,
    /// Ask the server to flush to disk.
    Save,
    /// Stop the worker thread (sent on drop).
    Shutdown,
}

/// A reply from the worker to the UI.
pub enum NetEvent {
    Connected(String),
    Disconnected,
    /// A human-readable success line (ping / push / save).
    Info(String),
    /// Entities fetched by a `Pull`, to be restored into the drawing.
    Pulled(Vec<Entity>),
    /// A failure line for any command.
    Error(String),
}

/// Handle to the network worker thread. Dropping it stops the worker.
pub struct NetWorker {
    tx: Sender<NetCmd>,
    rx: Receiver<NetEvent>,
    handle: Option<JoinHandle<()>>,
}

impl NetWorker {
    /// Spawn the worker. `ctx` is used to wake the UI when a reply is ready.
    pub fn spawn(ctx: egui::Context) -> NetWorker {
        let (cmd_tx, cmd_rx) = channel::<NetCmd>();
        let (evt_tx, evt_rx) = channel::<NetEvent>();
        let handle = std::thread::Builder::new()
            .name("aadt-shilpid-net".to_string())
            .spawn(move || worker_loop(&cmd_rx, &evt_tx, &ctx))
            .expect("spawn shilpid network worker");
        NetWorker {
            tx: cmd_tx,
            rx: evt_rx,
            handle: Some(handle),
        }
    }

    /// Enqueue a command (ignored if the worker has already gone away).
    pub fn send(&self, cmd: NetCmd) {
        let _ = self.tx.send(cmd);
    }

    /// Take the next reply without blocking, if one is ready.
    pub fn try_recv(&self) -> Option<NetEvent> {
        self.rx.try_recv().ok()
    }
}

impl Drop for NetWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(NetCmd::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn worker_loop(rx: &Receiver<NetCmd>, tx: &Sender<NetEvent>, ctx: &egui::Context) {
    let mut client: Option<VdbClient> = None;
    while let Ok(cmd) = rx.recv() {
        let Some(event) = handle_cmd(&mut client, cmd) else {
            break; // Shutdown
        };
        if tx.send(event).is_err() {
            break; // UI gone
        }
        ctx.request_repaint();
    }
}

/// Run one command. Returns `None` only for `Shutdown`.
fn handle_cmd(client: &mut Option<VdbClient>, cmd: NetCmd) -> Option<NetEvent> {
    Some(match cmd {
        NetCmd::Shutdown => return None,
        NetCmd::Connect(host) => match VdbClient::connect(&host) {
            Ok(c) => {
                *client = Some(c);
                NetEvent::Connected(host)
            }
            Err(e) => NetEvent::Error(format!("connect {host}: {e}")),
        },
        NetCmd::Disconnect => {
            *client = None;
            NetEvent::Disconnected
        }
        NetCmd::Ping => on_client(client, "ping", |c| {
            c.ping().map(|()| NetEvent::Info("ping ok".to_string()))
        }),
        NetCmd::Push(doc) => on_client(client, "push", |c| {
            c.push_document(&doc)
                .map(|n| NetEvent::Info(format!("pushed {n} entities")))
        }),
        NetCmd::Pull => on_client(client, "pull", |c| c.pull_entities().map(NetEvent::Pulled)),
        NetCmd::Save => on_client(client, "save", |c| {
            c.save()
                .map(|()| NetEvent::Info("server flushed to disk".to_string()))
        }),
    })
}

/// Apply `f` to the live client, mapping "not connected" and client errors to
/// an [`NetEvent::Error`] tagged with the operation name.
fn on_client(
    client: &mut Option<VdbClient>,
    op: &str,
    f: impl FnOnce(&mut VdbClient) -> Result<NetEvent, aadt_vdb::VdbClientError>,
) -> NetEvent {
    match client.as_mut() {
        Some(c) => match f(c) {
            Ok(event) => event,
            Err(e) => NetEvent::Error(format!("{op}: {e}")),
        },
        None => NetEvent::Error(format!("{op}: not connected")),
    }
}
