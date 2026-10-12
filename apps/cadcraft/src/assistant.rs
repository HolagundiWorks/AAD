//! The desktop assistant: the agent's step machine driven by a background model worker.
//!
//! The model runs on its own thread (`cadcraft_llm::worker`), so a slow answer never freezes the
//! window. Everything that touches the drawing runs here, on the thread that draws the window, a
//! step at a time from [`poll`](Backend::poll): the worker's text goes to the [`Assistant`], which
//! parses it strictly, validates it, asks for confirmation when the tool changes the drawing, runs
//! the engine command and hands the result back to the model for a summary.

use cadcraft_agent::driver::{params_for, to_messages};
use cadcraft_agent::{Assistant, Risk, Step};
use cadcraft_engine::Session;
use cadcraft_llm::LoadOptions;
use cadcraft_llm::worker::{Event, Worker};
use cadcraft_ui_egui::assistant::{Backend, Line, State};

pub struct Desktop {
    assistant: Assistant,
    worker: Worker,
    state: State,
    lines: Vec<Line>,
    partial: String,
    model: String,
    /// The user pressed Stop: the next finished answer from the worker is dropped.
    discard: bool,
}

impl Desktop {
    pub fn new() -> Self {
        let model = match cadcraft_llm::find_model() {
            Some(p) => format!("{}", p.display()),
            None => format!(
                "model not found: put {} in {}",
                cadcraft_llm::MODEL_FILE,
                cadcraft_llm::files::candidates().first().map(|p| p.display().to_string()).unwrap_or_default()
            ),
        };
        Desktop {
            assistant: Assistant::new(),
            worker: Worker::spawn(LoadOptions::default()),
            state: State::Idle,
            lines: Vec::new(),
            partial: String::new(),
            model,
            discard: false,
        }
    }

    fn step(&mut self, step: Step) {
        self.partial.clear();
        match step {
            Step::Generate { messages, max_new_tokens } => {
                let params = params_for(&self.assistant.profile, max_new_tokens);
                if self.worker.submit(to_messages(&messages), params) {
                    if self.state != State::Loading {
                        self.state = State::Thinking;
                    }
                } else {
                    self.lines.push(Line::Error("the model is busy or not running".into()));
                    self.state = State::Idle;
                }
            }
            Step::NeedsConfirmation { tool, risk, summary } => {
                let risk = match risk {
                    Risk::Read => "reads only",
                    Risk::Open => "opens another drawing",
                    Risk::Mutating => "changes the drawing",
                    Risk::Destructive => "removes things from the drawing",
                };
                self.state = State::Confirm { tool, risk: risk.into(), summary };
            }
            Step::Answer(a) => {
                self.lines.push(Line::Assistant(a));
                self.state = State::Idle;
            }
            Step::Failed(e) => {
                self.lines.push(Line::Error(e));
                self.state = State::Idle;
            }
        }
    }

    fn drain_events(&mut self) {
        for e in self.assistant.take_events() {
            self.lines.push(Line::Tool { name: e.tool, status: e.status });
        }
    }
}

impl Backend for Desktop {
    fn available(&self) -> bool {
        cadcraft_llm::available()
    }

    fn model_status(&self) -> String {
        if !cadcraft_llm::available() {
            return "this build has no language model runtime".into();
        }
        self.model.clone()
    }

    fn submit(&mut self, _session: &mut Session, text: &str) {
        if self.state != State::Idle || text.trim().is_empty() {
            return;
        }
        self.lines.push(Line::User(text.trim().to_string()));
        let step = self.assistant.start(text);
        self.step(step);
    }

    fn confirm(&mut self, session: &mut Session, accepted: bool) {
        if !matches!(self.state, State::Confirm { .. }) {
            return;
        }
        self.state = State::Thinking;
        let step = self.assistant.confirm(session, accepted);
        self.drain_events();
        self.step(step);
    }

    fn cancel(&mut self) {
        if matches!(self.state, State::Loading | State::Thinking) {
            self.worker.cancel();
            self.discard = true;
        }
        self.assistant.cancel();
        self.partial.clear();
        self.lines.push(Line::Note("stopped".into()));
        self.state = State::Idle;
    }

    fn reset(&mut self) {
        self.assistant.reset();
        self.lines.clear();
        self.partial.clear();
        self.state = State::Idle;
    }

    fn poll(&mut self, session: &mut Session) -> bool {
        for e in self.worker.poll() {
            match e {
                Event::Loading => {
                    if self.state == State::Thinking {
                        self.state = State::Loading;
                    }
                }
                Event::Loaded { path, megabytes, seconds } => {
                    self.model = format!("{path} ({megabytes:.0} MB, loaded in {seconds:.1} s)");
                    if self.state == State::Loading {
                        self.state = State::Thinking;
                    }
                }
                Event::LoadFailed(m) => {
                    self.lines.push(Line::Error(m));
                    self.assistant.cancel();
                    self.state = State::Idle;
                    self.discard = false;
                }
                Event::Piece(p) => {
                    if !self.discard {
                        self.partial.push_str(&p);
                    }
                }
                Event::Done(r) => {
                    if std::mem::take(&mut self.discard) {
                        continue;
                    }
                    match r {
                        Ok(c) => {
                            let step = self.assistant.on_generated(session, &c.text);
                            self.drain_events();
                            self.step(step);
                        }
                        Err(e) => {
                            self.lines.push(Line::Error(e.to_string()));
                            self.assistant.cancel();
                            self.state = State::Idle;
                        }
                    }
                }
            }
        }
        matches!(self.state, State::Loading | State::Thinking) || self.worker.busy()
    }

    fn state(&self) -> State {
        self.state.clone()
    }

    fn lines(&self) -> &[Line] {
        &self.lines
    }

    fn partial(&self) -> &str {
        &self.partial
    }
}
