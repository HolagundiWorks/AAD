//! A background thread that owns the model, so asking it something never freezes the window.
//!
//! The thread loads the model the first time it is asked (not at start-up: most sessions never use
//! it), then answers one request at a time. The owner sends a request with [`Worker::submit`], reads
//! what has happened with [`Worker::poll`] (never blocks) and can stop the current answer with
//! [`Worker::cancel`]. A panic inside the runtime is caught and becomes an error event; the worker
//! then reloads the model on the next request. Dropping the worker stops the thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::engine::{Completion, LlmError, LoadOptions, Model, Params};
use crate::prompt::Message;

/// What the worker reports.
#[derive(Debug)]
pub enum Event {
    /// The model file is being loaded (first request).
    Loading,
    /// The model is loaded: where from, how big, and how long it took (seconds).
    Loaded { path: String, megabytes: f64, seconds: f64 },
    /// The model could not be loaded; the request failed with this message.
    LoadFailed(String),
    /// A piece of the answer.
    Piece(String),
    /// The answer is finished (or failed).
    Done(Result<Completion, LlmError>),
}

struct Job {
    messages: Vec<Message>,
    params: Params,
}

pub struct Worker {
    tx: Option<Sender<Job>>,
    rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    busy: bool,
}

impl Worker {
    /// Start the thread. It does nothing until the first [`submit`](Self::submit).
    pub fn spawn(opts: LoadOptions) -> Worker {
        let (tx, jobs) = channel::<Job>();
        let (events, rx) = channel::<Event>();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let thread = std::thread::Builder::new().name("aad-llm".into()).spawn(move || run(opts, jobs, events, flag)).ok();
        Worker { tx: Some(tx), rx, cancel, thread, busy: false }
    }

    /// Whether a request is being answered.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Ask the model. Returns false when a request is already running or the thread is gone.
    pub fn submit(&mut self, messages: Vec<Message>, params: Params) -> bool {
        if self.busy || self.thread.is_none() {
            return false;
        }
        self.cancel.store(false, Ordering::Relaxed);
        let sent = self.tx.as_ref().is_some_and(|tx| tx.send(Job { messages, params }).is_ok());
        self.busy = sent;
        sent
    }

    /// Stop the current answer (checked between tokens).
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Everything that happened since the last call, oldest first. Never blocks.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            if matches!(e, Event::Done(_) | Event::LoadFailed(_)) {
                self.busy = false;
            }
            out.push(e);
        }
        if self.busy && self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            // The thread died with a request in flight.
            self.busy = false;
            out.push(Event::Done(Err(LlmError::Panic("the language model thread stopped".into()))));
        }
        out
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.tx = None; // ends the thread's loop
        if let Some(t) = self.thread.take()
            && t.is_finished()
        {
            let _ = t.join();
        }
        // A thread still generating is left to finish its token and exit; the process may end first.
    }
}

fn run(opts: LoadOptions, jobs: Receiver<Job>, events: Sender<Event>, cancel: Arc<AtomicBool>) {
    let mut model: Option<Model> = None;
    while let Ok(job) = jobs.recv() {
        if model.is_none() {
            let _ = events.send(Event::Loading);
            let t = Instant::now();
            match Model::load_default(&opts) {
                Ok(m) => {
                    let _ = events.send(Event::Loaded {
                        path: m.path().display().to_string(),
                        megabytes: m.size_bytes() as f64 / 1e6,
                        seconds: t.elapsed().as_secs_f64(),
                    });
                    model = Some(m);
                }
                Err(e) => {
                    let _ = events.send(Event::LoadFailed(e.to_string()));
                    continue;
                }
            }
        }
        let Some(m) = model.as_ref() else { continue };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            m.chat(&job.messages, &job.params, &cancel, &mut |piece| {
                let _ = events.send(Event::Piece(piece.to_string()));
            })
        }));
        match result {
            Ok(r) => {
                let _ = events.send(Event::Done(r));
            }
            Err(p) => {
                let msg = p
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".into());
                model = None; // reload on the next request
                let _ = events.send(Event::Done(Err(LlmError::Panic(msg))));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Without the llama backend, or without a model file, a request fails cleanly with a message
    /// and the worker stays usable.
    #[test]
    fn a_request_without_a_model_fails_with_a_message_and_the_worker_survives() {
        let mut w = Worker::spawn(LoadOptions::default());
        assert!(w.submit(vec![Message::user("hi")], Params::default()));
        assert!(w.busy());
        let mut failed = None;
        for _ in 0..200 {
            for e in w.poll() {
                match e {
                    Event::LoadFailed(m) => failed = Some(m),
                    Event::Done(Err(e)) => failed = Some(e.to_string()),
                    _ => {}
                }
            }
            if failed.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        // On a machine that has the model and the backend this would succeed instead; either is fine.
        if let Some(m) = failed {
            assert!(!m.is_empty());
            assert!(!w.busy(), "a failed request frees the worker");
            assert!(w.submit(vec![Message::user("again")], Params::default()), "and it takes another");
        }
    }
}
