//! Model process lifecycle: spawn on demand, stay warm briefly, unload (spec §6.1).
//!
//! # Nothing runs in the background
//!
//! The product principle is that no AI process lingers unasked. That rules out
//! the obvious implementation — a timer thread that unloads the model after N
//! idle seconds — because such a thread *is* background work, and it would keep
//! running (and keep the model alive) even when the user has walked away from
//! AADT entirely.
//!
//! So auto-unload here is **poll-driven, never self-driven**. Time passing
//! changes nothing on its own; the model unloads only when someone calls
//! [`ModelHost::release_if_idle`], which an app does on its event loop or before
//! quitting. The invariant that makes this safe: [`ModelHost`] stops the process
//! on drop, so an abandoned host cannot leak a running model.
//!
//! # Testability
//!
//! Both the clock and the process are injected as traits, so the whole state
//! machine is unit-tested without spawning anything or sleeping. That matters
//! because the real transport ([`crate::OllamaClient`]) cannot be tested here —
//! keeping the *policy* separate from the *process* is what makes the policy
//! verifiable at all.

use std::fmt;

use crate::{AiError, ModelClient, WARM_IDLE_SECS};

/// A monotonic millisecond clock. Injected so idle behaviour is testable
/// without sleeping.
pub trait Clock {
    fn now_millis(&self) -> u64;
}

/// Monotonic wall clock, based on [`std::time::Instant`].
pub struct SystemClock {
    origin: std::time::Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
}

/// The model process AADT starts and stops. Implemented by [`OllamaProcess`] for
/// real use and by a fake in tests.
pub trait ModelProcess {
    /// Start the process. Must be idempotent: starting a running process is a
    /// no-op, not an error.
    fn start(&mut self) -> Result<(), AiError>;
    /// Stop the process. Must be idempotent.
    fn stop(&mut self);
    fn is_running(&self) -> bool;
}

/// What the host is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    /// No process running — the resting state, and where AADT starts.
    Unloaded,
    /// Process is up, holding for reuse. `idle_millis` is time since last use.
    Warm { idle_millis: u64 },
}

impl fmt::Display for ModelState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelState::Unloaded => write!(f, "unloaded"),
            ModelState::Warm { idle_millis } => write!(f, "warm ({idle_millis}ms idle)"),
        }
    }
}

/// Owns the model process and decides when it may run.
pub struct ModelHost {
    process: Box<dyn ModelProcess>,
    clock: Box<dyn Clock>,
    /// Idle window before [`release_if_idle`](Self::release_if_idle) will unload.
    idle_timeout_millis: u64,
    /// Clock reading at the last [`acquire`](Self::acquire); `None` when unloaded.
    last_used: Option<u64>,
}

impl ModelHost {
    /// Host with the spec's default warm-idle window ([`WARM_IDLE_SECS`]).
    pub fn new(process: Box<dyn ModelProcess>) -> Self {
        Self::with_parts(
            process,
            Box::new(SystemClock::default()),
            WARM_IDLE_SECS * 1000,
        )
    }

    pub fn with_parts(
        process: Box<dyn ModelProcess>,
        clock: Box<dyn Clock>,
        idle_timeout_millis: u64,
    ) -> Self {
        Self {
            process,
            clock,
            idle_timeout_millis,
            last_used: None,
        }
    }

    pub fn state(&self) -> ModelState {
        match self.last_used {
            Some(t) if self.process.is_running() => ModelState::Warm {
                idle_millis: self.clock.now_millis().saturating_sub(t),
            },
            _ => ModelState::Unloaded,
        }
    }

    /// Make the model available, starting it if needed, and stamp it as used now.
    ///
    /// Call immediately before every model request. A start failure surfaces as
    /// [`AiError::Unavailable`] so the caller degrades rather than blocking the
    /// deterministic pipeline (spec §6.1 step 7).
    pub fn acquire(&mut self) -> Result<(), AiError> {
        if !self.process.is_running() {
            self.process.start()?;
        }
        self.last_used = Some(self.clock.now_millis());
        Ok(())
    }

    /// Unload the model if it has been idle past the timeout. Returns whether it
    /// was unloaded.
    ///
    /// **This is the only thing that expires a warm model.** Nothing here runs
    /// on a timer; an app calls this from its event loop, before quitting, or
    /// whenever it wants to reclaim the memory.
    pub fn release_if_idle(&mut self) -> bool {
        match self.state() {
            ModelState::Warm { idle_millis } if idle_millis >= self.idle_timeout_millis => {
                self.unload();
                true
            }
            _ => false,
        }
    }

    /// Stop the model now, regardless of idle time.
    pub fn unload(&mut self) {
        self.process.stop();
        self.last_used = None;
    }
}

impl Drop for ModelHost {
    /// Stop the process when the host goes away. Without this, dropping a host
    /// mid-session would leave a model resident with nothing left to unload it —
    /// exactly the background residency the design forbids.
    fn drop(&mut self) {
        self.process.stop();
    }
}

/// A [`ModelClient`] that owns its process lifecycle: it starts the model on the
/// first request and keeps it warm for reuse.
///
/// Wraps any transport, so [`crate::OllamaClient`] gains lifecycle management
/// without knowing anything about processes.
pub struct ManagedClient<C: ModelClient> {
    inner: C,
    host: std::cell::RefCell<ModelHost>,
}

impl<C: ModelClient> ManagedClient<C> {
    pub fn new(inner: C, host: ModelHost) -> Self {
        Self {
            inner,
            host: std::cell::RefCell::new(host),
        }
    }

    pub fn state(&self) -> ModelState {
        self.host.borrow().state()
    }

    /// Unload if idle past the window — the caller's periodic poke.
    pub fn release_if_idle(&self) -> bool {
        self.host.borrow_mut().release_if_idle()
    }

    pub fn unload(&self) {
        self.host.borrow_mut().unload();
    }
}

impl<C: ModelClient> ModelClient for ManagedClient<C> {
    fn generate(&self, prompt: &str) -> Result<String, AiError> {
        // Interior mutability: ModelClient::generate takes &self, but acquiring
        // mutates lifecycle state. RefCell keeps that local rather than forcing
        // &mut through the whole orchestration surface.
        self.host.borrow_mut().acquire()?;
        self.inner.generate(prompt)
    }
}

/// Starts and stops a local `ollama serve` process (spec §6.1).
///
/// **Not unit-tested**, for the same reason as [`crate::OllamaClient`]: it needs
/// a real Ollama install. The policy that drives it is tested via [`ModelHost`].
pub struct OllamaProcess {
    program: String,
    child: Option<std::process::Child>,
}

impl Default for OllamaProcess {
    fn default() -> Self {
        Self::new("ollama")
    }
}

impl OllamaProcess {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            child: None,
        }
    }
}

impl ModelProcess for OllamaProcess {
    fn start(&mut self) -> Result<(), AiError> {
        if self.is_running() {
            return Ok(());
        }
        let child = std::process::Command::new(&self.program)
            .arg("serve")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| AiError::Unavailable(format!("spawning {}: {e}", self.program)))?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best effort: a model we cannot kill must not stall the caller, and
            // wait() reaps the process so it does not linger as a zombie.
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Test clock: time only moves when a test moves it.
    struct ManualClock(Rc<Cell<u64>>);
    impl Clock for ManualClock {
        fn now_millis(&self) -> u64 {
            self.0.get()
        }
    }

    #[derive(Default)]
    struct FakeProcessState {
        running: bool,
        starts: u32,
        stops: u32,
        fail_start: bool,
    }

    struct FakeProcess(Rc<std::cell::RefCell<FakeProcessState>>);
    impl ModelProcess for FakeProcess {
        fn start(&mut self) -> Result<(), AiError> {
            let mut s = self.0.borrow_mut();
            if s.fail_start {
                return Err(AiError::Unavailable("no ollama".to_string()));
            }
            s.starts += 1;
            s.running = true;
            Ok(())
        }
        fn stop(&mut self) {
            let mut s = self.0.borrow_mut();
            if s.running {
                s.stops += 1;
            }
            s.running = false;
        }
        fn is_running(&self) -> bool {
            self.0.borrow().running
        }
    }

    fn host_with(
        timeout: u64,
    ) -> (
        ModelHost,
        Rc<Cell<u64>>,
        Rc<std::cell::RefCell<FakeProcessState>>,
    ) {
        let clock = Rc::new(Cell::new(0u64));
        let proc_state = Rc::new(std::cell::RefCell::new(FakeProcessState::default()));
        let host = ModelHost::with_parts(
            Box::new(FakeProcess(proc_state.clone())),
            Box::new(ManualClock(clock.clone())),
            timeout,
        );
        (host, clock, proc_state)
    }

    #[test]
    fn starts_unloaded_and_nothing_runs_until_asked() {
        let (host, _clock, state) = host_with(1000);
        assert_eq!(host.state(), ModelState::Unloaded);
        assert_eq!(state.borrow().starts, 0, "no process before a request");
    }

    #[test]
    fn acquire_starts_the_process_once_and_reuses_it() {
        let (mut host, clock, state) = host_with(1000);
        host.acquire().unwrap();
        assert_eq!(state.borrow().starts, 1);
        assert_eq!(host.state(), ModelState::Warm { idle_millis: 0 });

        clock.set(300);
        host.acquire().unwrap();
        assert_eq!(state.borrow().starts, 1, "warm model must not restart");
        assert_eq!(
            host.state(),
            ModelState::Warm { idle_millis: 0 },
            "use resets the idle clock"
        );
    }

    #[test]
    fn stays_warm_inside_the_window() {
        let (mut host, clock, state) = host_with(1000);
        host.acquire().unwrap();
        clock.set(999);
        assert!(!host.release_if_idle(), "not yet expired");
        assert!(state.borrow().running);
        assert_eq!(host.state(), ModelState::Warm { idle_millis: 999 });
    }

    #[test]
    fn unloads_once_idle_past_the_window() {
        let (mut host, clock, state) = host_with(1000);
        host.acquire().unwrap();
        clock.set(1000);
        assert!(host.release_if_idle(), "expired at the boundary");
        assert!(!state.borrow().running);
        assert_eq!(host.state(), ModelState::Unloaded);
        assert_eq!(state.borrow().stops, 1);
    }

    /// The core discipline: time alone must not unload anything, because nothing
    /// is running in the background to notice.
    #[test]
    fn time_alone_never_changes_the_process() {
        let (mut host, clock, state) = host_with(1000);
        host.acquire().unwrap();
        clock.set(10_000_000);
        assert!(
            state.borrow().running,
            "no timer thread may stop the process"
        );
        // Only an explicit poll acts on the elapsed time.
        assert!(host.release_if_idle());
        assert!(!state.borrow().running);
    }

    #[test]
    fn idle_clock_restarts_after_each_use() {
        let (mut host, clock, _state) = host_with(1000);
        host.acquire().unwrap();
        clock.set(900);
        host.acquire().unwrap(); // used again just before expiry
        clock.set(1800);
        assert!(
            !host.release_if_idle(),
            "only 900ms since the second use, so still warm"
        );
    }

    #[test]
    fn failure_to_start_degrades_and_stays_unloaded() {
        let (mut host, _clock, state) = host_with(1000);
        state.borrow_mut().fail_start = true;
        let err = host.acquire().unwrap_err();
        assert!(matches!(err, AiError::Unavailable(_)));
        assert_eq!(host.state(), ModelState::Unloaded);
    }

    #[test]
    fn reacquire_after_unload_starts_a_fresh_process() {
        let (mut host, _clock, state) = host_with(1000);
        host.acquire().unwrap();
        host.unload();
        assert_eq!(host.state(), ModelState::Unloaded);
        host.acquire().unwrap();
        assert_eq!(state.borrow().starts, 2);
    }

    #[test]
    fn dropping_the_host_stops_the_process() {
        // Otherwise an abandoned host leaves a model resident with nothing left
        // to unload it.
        let (mut host, _clock, state) = host_with(1000);
        host.acquire().unwrap();
        assert!(state.borrow().running);
        drop(host);
        assert!(!state.borrow().running, "drop must stop the model");
        assert_eq!(state.borrow().stops, 1);
    }

    // --- ManagedClient -------------------------------------------------------

    struct EchoClient;
    impl ModelClient for EchoClient {
        fn generate(&self, prompt: &str) -> Result<String, AiError> {
            Ok(format!("echo:{prompt}"))
        }
    }

    #[test]
    fn managed_client_starts_the_model_on_first_request() {
        let (host, _clock, state) = host_with(1000);
        let client = ManagedClient::new(EchoClient, host);
        assert_eq!(client.state(), ModelState::Unloaded);

        let out = client.generate("hello").unwrap();
        assert_eq!(out, "echo:hello");
        assert_eq!(state.borrow().starts, 1);
        assert!(matches!(client.state(), ModelState::Warm { .. }));
    }

    #[test]
    fn managed_client_degrades_when_the_process_will_not_start() {
        let (host, _clock, state) = host_with(1000);
        state.borrow_mut().fail_start = true;
        let client = ManagedClient::new(EchoClient, host);
        // The transport is never reached: no model, no request.
        assert!(matches!(
            client.generate("hi"),
            Err(AiError::Unavailable(_))
        ));
    }

    #[test]
    fn managed_client_releases_on_poll() {
        let (host, clock, state) = host_with(1000);
        let client = ManagedClient::new(EchoClient, host);
        client.generate("hi").unwrap();
        clock.set(5000);
        assert!(client.release_if_idle());
        assert!(!state.borrow().running);
    }
}
