//! The Assistant window against a fake backend: what it sends, and that it shows what the backend
//! reports. (The real backend and model are exercised by `cadcraft-cli ask` and the desktop app.)

use std::cell::RefCell;
use std::rc::Rc;

use cadcraft_engine::Session;
use cadcraft_ui_egui::assistant::{Backend, Line, State};
use cadcraft_ui_egui::{CadApp, Services};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

#[derive(Default)]
struct Log(Vec<String>);

struct Fake {
    log: Rc<RefCell<Log>>,
    state: State,
    lines: Vec<Line>,
}

impl Backend for Fake {
    fn available(&self) -> bool {
        true
    }
    fn model_status(&self) -> String {
        "fake model".into()
    }
    fn submit(&mut self, _s: &mut Session, text: &str) {
        self.log.borrow_mut().0.push(format!("submit:{text}"));
        self.lines.push(Line::User(text.into()));
        self.state = State::Confirm {
            tool: "purge_unused".into(),
            risk: "removes things from the drawing".into(),
            summary: "Remove unused definitions".into(),
        };
    }
    fn confirm(&mut self, _s: &mut Session, accepted: bool) {
        self.log.borrow_mut().0.push(format!("confirm:{accepted}"));
        self.lines.push(Line::Tool { name: "purge_unused".into(), status: if accepted { "done".into() } else { "declined".into() } });
        self.lines.push(Line::Assistant("All done.".into()));
        self.state = State::Idle;
    }
    fn cancel(&mut self) {
        self.log.borrow_mut().0.push("cancel".into());
    }
    fn reset(&mut self) {
        self.lines.clear();
    }
    fn poll(&mut self, _s: &mut Session) -> bool {
        false
    }
    fn state(&self) -> State {
        self.state.clone()
    }
    fn lines(&self) -> &[Line] {
        &self.lines
    }
    fn partial(&self) -> &str {
        ""
    }
}

fn harness(with_backend: bool) -> (Harness<'static, CadApp>, Rc<RefCell<Log>>) {
    harness_in(with_backend, State::Idle)
}

fn harness_in(with_backend: bool, state: State) -> (Harness<'static, CadApp>, Rc<RefCell<Log>>) {
    let log = Rc::new(RefCell::new(Log::default()));
    let services = Services {
        assistant: with_backend.then(|| Box::new(Fake { log: log.clone(), state, lines: Vec::new() }) as Box<dyn Backend>),
        ..Services::default()
    };
    let mut app = CadApp::new(Session::default(), services);
    app.ui.in_window_menu = false;
    app.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    app.run("ui.toggle.assistant", json!({})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    (h, log)
}

fn has(h: &Harness<'_, CadApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

#[test]
fn a_build_without_a_model_says_so() {
    let (h, _) = harness(false);
    assert!(has(&h, "The assistant is not included in this build."));
}

fn ask(h: &mut Harness<'static, CadApp>, text: &str) {
    h.query_all_by_role(Role::TextInput).last().unwrap().click();
    h.query_all_by_role(Role::TextInput).last().unwrap().type_text(text);
    h.run_steps(2);
    h.get_by_label("Send").click();
    h.run_steps(4);
}

#[test]
fn sending_asks_for_confirmation_and_allow_and_decline_reach_the_backend() {
    let (mut h, log) = harness(true);
    assert!(has(&h, "fake model"));
    ask(&mut h, "purge unused definitions");
    assert_eq!(log.borrow().0, ["submit:purge unused definitions"]);
    assert!(has(&h, "Allow") && has(&h, "Decline"), "the confirmation is asked, nothing has run");
    // Every control of the confirmation lies inside the window, where it can be clicked.
    let window = h.ctx.memory(|m| m.area_rect(egui::Id::new("assistant_window"))).expect("the window");
    for label in ["Allow", "Decline", "Send"] {
        let r = h.get_by_label(label).rect();
        assert!(window.contains_rect(r), "{label} at {r:?} is outside the window {window:?}");
    }
    h.get_by_label("Decline").click();
    h.run_steps(4);
    assert_eq!(log.borrow().0.last().unwrap(), "confirm:false");
    assert!(has(&h, "▸ purge_unused: declined"));
    ask(&mut h, "again");
    h.get_by_label("Allow").click();
    h.run_steps(4);
    assert_eq!(log.borrow().0.last().unwrap(), "confirm:true");
    assert!(has(&h, "All done."), "the answer is shown");
    assert!(has(&h, "▸ purge_unused: done"));
    h.get_by_label("New chat").click();
    h.run_steps(3);
    assert!(!has(&h, "All done."), "a new chat clears the conversation");
}

#[test]
fn the_window_does_not_grow_by_itself() {
    // Busy: the Stop button is showing, which is when the width used to creep up each frame.
    let (mut h, _) = harness_in(true, State::Thinking);
    let width = |h: &Harness<'_, CadApp>| h.ctx.memory(|m| m.area_rect(egui::Id::new("assistant_window"))).expect("the window").width();
    let first = width(&h);
    h.run_steps(60);
    assert!((width(&h) - first).abs() < 1.0, "the window width changed from {first} to {} with nothing happening", width(&h));
    assert!(first <= 900.0);
}
