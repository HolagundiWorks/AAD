//! The Assistant window: a short conversation with the local language model about the open drawing.
//!
//! This crate knows nothing about the model. It talks to a [`Backend`] the application supplies
//! (`Services::assistant`), so the web build, which has no model, simply has none. The backend
//! (in the desktop app) runs the model on a background thread and the tools through the engine on
//! the thread that draws the window; this file only shows the conversation and sends the user's
//! words, Allow/Decline and Stop to it. The model never draws, repairs or calculates: it picks one
//! of a fixed list of tools, and a tool that changes the drawing waits here for the user's answer.
//! See docs/ai-agent.md.

use cadcraft_engine::Session;
use egui::{Color32, RichText, vec2};

use crate::CadApp;

/// What the assistant is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    /// Reading the model file (first request only).
    Loading,
    Thinking,
    /// A tool is waiting for the user's yes or no. Nothing has run.
    Confirm {
        tool: String,
        risk: String,
        summary: String,
    },
}

/// One line of the conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    User(String),
    Assistant(String),
    /// A tool ran (or was declined or refused): name and what happened.
    Tool {
        name: String,
        status: String,
    },
    Note(String),
    Error(String),
}

/// The assistant as the window needs it.
pub trait Backend {
    /// Whether this build includes the language model runtime.
    fn available(&self) -> bool;
    /// One line about the model: loaded, where it is expected, why it is missing.
    fn model_status(&self) -> String;
    /// A new request from the user.
    fn submit(&mut self, session: &mut Session, text: &str);
    /// The answer to a pending confirmation.
    fn confirm(&mut self, session: &mut Session, accepted: bool);
    /// Stop the answer in progress.
    fn cancel(&mut self);
    /// Forget the conversation.
    fn reset(&mut self);
    /// Move things along (model output, tools); called every frame. Returns true while busy so the
    /// window keeps repainting.
    fn poll(&mut self, session: &mut Session) -> bool;
    fn state(&self) -> State;
    fn lines(&self) -> &[Line];
    /// The model's words so far for the answer in progress.
    fn partial(&self) -> &str;
}

/// What the window itself remembers.
#[derive(Default)]
pub struct Ui {
    pub open: bool,
    pub input: String,
    focus: bool,
    /// The assistant was busy last frame: when it stops, the input box takes the keyboard back (a
    /// disabled box loses it, and typing would then go to the command line).
    was_busy: bool,
}

/// Called every frame before drawing: lets the backend work.
pub fn poll(app: &mut CadApp, ctx: &egui::Context) {
    if let Some(b) = app.services.assistant.as_mut()
        && b.poll(&mut app.session)
    {
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
    }
}

pub fn toggle(app: &mut CadApp) {
    app.assistant.open = !app.assistant.open;
    app.assistant.focus = app.assistant.open;
}

pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    if !app.assistant.open {
        return;
    }
    let mut open = true;
    egui::Window::new(crate::tl!("Assistant")).id(egui::Id::new("assistant_window")).open(&mut open).default_size(vec2(480.0, 440.0)).min_width(360.0).max_width(900.0).resizable(true).show(ctx, |ui| {
        let Some(backend) = app.services.assistant.as_mut().filter(|b| b.available()) else {
            ui.label(crate::tl!("The assistant is not included in this build."));
            return;
        };
        let state = backend.state();
        ui.label(RichText::new(backend.model_status()).small().weak());
        ui.separator();
        let busy = !matches!(state, State::Idle | State::Confirm { .. });
        if app.assistant.was_busy && !busy {
            app.assistant.focus = true;
        }
        app.assistant.was_busy = busy;
        // Room kept under the conversation for the input row and, when a tool waits for a yes or no, its box.
        let bottom = if matches!(state, State::Confirm { .. }) { 190.0 } else { 56.0 };
        egui::ScrollArea::vertical().max_height((ui.available_height() - bottom).max(80.0)).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            if backend.lines().is_empty() && backend.partial().is_empty() {
                ui.label(RichText::new(crate::tl!("Ask about this drawing. It can summarise it, audit it, purge unused definitions, flatten it or recover a file. It changes nothing without your yes.")).weak());
            }
            for l in backend.lines() {
                match l {
                    Line::User(t) => {
                        ui.label(RichText::new(format!("{} {t}", crate::tl!("You:"))).strong());
                    }
                    Line::Assistant(t) => {
                        ui.label(t.as_str());
                    }
                    Line::Tool { name, status } => {
                        ui.label(RichText::new(format!("▸ {name}: {status}")).small().color(Color32::from_rgb(120, 160, 220)));
                    }
                    Line::Note(t) => {
                        ui.label(RichText::new(t.as_str()).small().weak());
                    }
                    Line::Error(t) => {
                        ui.colored_label(Color32::from_rgb(220, 90, 80), t.as_str());
                    }
                }
                ui.add_space(2.0);
            }
            match &state {
                State::Loading => {
                    ui.label(RichText::new(crate::tl!("Loading the model…")).weak());
                }
                State::Thinking => {
                    let p = backend.partial();
                    // A tool call is machine text: show that it is working, not the markup.
                    if p.is_empty() || p.contains("<tool_call>") {
                        ui.label(RichText::new(crate::tl!("Thinking…")).weak());
                    } else {
                        ui.label(RichText::new(p).italics());
                    }
                }
                _ => {}
            }
        });
        let mut answer: Option<bool> = None;
        if let State::Confirm { tool, risk, summary } = &state {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(format!("{}: {tool} ({risk})", crate::tl!("Allow this?"))).strong());
                ui.label(summary.as_str());
                ui.horizontal(|ui| {
                    if ui.button(crate::tl!("Allow")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(crate::tl!("Decline")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        }
        let mut send = false;
        let mut stop = false;
        let mut new_chat = false;
        ui.horizontal(|ui| {
            let edit = ui.add(egui::TextEdit::singleline(&mut app.assistant.input).hint_text(crate::tl!("Ask about the drawing…")).desired_width((ui.available_width() - 230.0).max(120.0)));
            if app.assistant.focus {
                edit.request_focus();
                app.assistant.focus = false;
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add_enabled(!busy && !app.assistant.input.trim().is_empty(), egui::Button::new(crate::tl!("Send"))).clicked() || (enter && !app.assistant.input.trim().is_empty() && !busy) {
                send = true;
            }
            if busy && ui.button(crate::tl!("Stop")).clicked() {
                stop = true;
            }
            if ui.add_enabled(!busy, egui::Button::new(crate::tl!("New chat"))).clicked() {
                new_chat = true;
            }
        });
        if let Some(yes) = answer {
            backend.confirm(&mut app.session, yes);
        }
        if send {
            let text = std::mem::take(&mut app.assistant.input);
            backend.submit(&mut app.session, &text);
            app.assistant.focus = true;
        }
        if stop {
            backend.cancel();
        }
        if new_chat {
            backend.reset();
        }
    });
    if !open {
        app.assistant.open = false;
    }
}

/// A command-line entry that starts with `/`: the rest is a request for the assistant, so
/// `/audit this drawing` asks it instead of looking for a command called `/audit`. Opens the
/// Assistant window so the answer (and a confirmation, if a tool needs one) is visible. Returns the
/// message to show on the command line when it could not be asked.
pub fn ask_from_command_line(app: &mut CadApp, request: &str) -> Option<String> {
    let request = request.trim();
    let msg = match app.services.assistant.as_mut().filter(|b| b.available()) {
        None => Some(crate::tl!("The assistant is not included in this build.").to_string()),
        Some(b) if b.state() != State::Idle => Some(crate::tl!("The assistant is busy: wait, or stop it first.").to_string()),
        Some(b) => {
            if !request.is_empty() {
                b.submit(&mut app.session, request);
            }
            None
        }
    };
    if msg.is_none() {
        app.assistant.open = true;
        app.assistant.focus = request.is_empty();
    }
    if let Some(m) = &msg {
        app.session.echo(m.clone());
    }
    msg
}
