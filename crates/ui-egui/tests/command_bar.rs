//! The unified command bar: native commands and requests to the assistant in one input, the key
//! rules (Space, Enter, Tab, Escape, arrows, Ctrl+Space), `/reference` resolution, and the
//! assistant's prompts. A fake assistant backend records what it is sent.

use std::cell::RefCell;
use std::rc::Rc;

use aad_engine::Session;
use aad_ui_egui::assistant::{Backend, Line, State};
use aad_ui_egui::{CadApp, Services, WorkspaceMode, cmdline};
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use serde_json::json;

#[derive(Default)]
struct Shared {
    log: Vec<String>,
    state: Option<State>,
}

struct Fake(Rc<RefCell<Shared>>);

impl Backend for Fake {
    fn available(&self) -> bool {
        true
    }
    fn model_status(&self) -> String {
        "fake".into()
    }
    fn submit(&mut self, _s: &mut Session, text: &str) {
        let mut s = self.0.borrow_mut();
        s.log.push(format!("submit:{text}"));
        s.state = Some(State::Thinking);
    }
    fn confirm(&mut self, _s: &mut Session, accepted: bool) {
        let mut s = self.0.borrow_mut();
        s.log.push(format!("confirm:{accepted}"));
        s.state = None;
    }
    fn cancel(&mut self) {
        let mut s = self.0.borrow_mut();
        s.log.push("cancel".into());
        s.state = None;
    }
    fn reset(&mut self) {}
    fn poll(&mut self, _s: &mut Session) -> bool {
        false
    }
    fn state(&self) -> State {
        self.0.borrow().state.clone().unwrap_or(State::Idle)
    }
    fn lines(&self) -> &[Line] {
        // The lines live behind the RefCell; leak a snapshot per call is wrong, so keep them in the fake itself.
        &[]
    }
    fn partial(&self) -> &str {
        ""
    }
}

fn bar_app(shared: Option<Rc<RefCell<Shared>>>, mode: WorkspaceMode) -> CadApp {
    let services = Services { assistant: shared.map(|s| Box::new(Fake(s)) as Box<dyn Backend>), ..Services::default() };
    let mut a = CadApp::new(Session::default(), services);
    a.ui.in_window_menu = false;
    a.integrated_titlebar = false;
    a.run("layer.new", json!({"name": "A-WALL"})).unwrap();
    a.run("layer.current", json!({"name": "A-WALL"})).unwrap();
    a.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    a.run("view.set", json!({"center": [5, 3], "height": 30})).unwrap();
    a.run(if mode == WorkspaceMode::Pro { "ui.workspace.pro" } else { "ui.workspace.classic" }, json!({})).unwrap();
    a
}

fn harness(shared: Option<Rc<RefCell<Shared>>>) -> Harness<'static, CadApp> {
    harness_in(shared, WorkspaceMode::Classic)
}

fn harness_in(shared: Option<Rc<RefCell<Shared>>>, mode: WorkspaceMode) -> Harness<'static, CadApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        bar_app(shared, mode),
    );
    h.run_steps(5);
    h
}

fn typed(h: &mut Harness<'static, CadApp>, text: &str) {
    for c in text.chars() {
        h.event(Event::Text(c.to_string()));
        h.run_steps(1);
    }
}

fn key(h: &mut Harness<'static, CadApp>, k: Key) {
    key_mod(h, k, Modifiers::NONE);
}

fn key_mod(h: &mut Harness<'static, CadApp>, k: Key, modifiers: Modifiers) {
    for pressed in [true, false] {
        h.event(Event::Key { key: k, physical_key: Some(k), pressed, repeat: false, modifiers });
        h.run_steps(1);
    }
}

fn buffer(h: &Harness<'_, CadApp>) -> String {
    h.state().cmd.buffer.clone()
}

fn running(h: &Harness<'_, CadApp>) -> Option<String> {
    h.state().session.running.as_ref().map(|r| r.id.clone())
}

fn shared() -> Rc<RefCell<Shared>> {
    Rc::new(RefCell::new(Shared::default()))
}

fn sent(s: &Rc<RefCell<Shared>>) -> Vec<String> {
    s.borrow().log.clone()
}

// ---------------------------------------------------------------- Space

#[test]
fn space_runs_a_whole_command_or_alias_and_is_just_a_space_otherwise() {
    let mut h = harness(None);
    typed(&mut h, "lin");
    typed(&mut h, " ");
    assert_eq!(running(&h), None, "a partial name is not executed");
    assert_eq!(buffer(&h), "lin ", "it is an ordinary space");
    h.state_mut().cmd.buffer.clear();
    typed(&mut h, "line");
    typed(&mut h, " ");
    assert_eq!(running(&h).as_deref(), Some("line"), "a whole command runs on Space");
    key(&mut h, Key::Escape);
    typed(&mut h, "c");
    typed(&mut h, " ");
    assert_eq!(running(&h).as_deref(), Some("circle"), "an alias too");
    key(&mut h, Key::Escape);
    // A command waiting for input still takes Space as Enter (the CAD habit).
    typed(&mut h, "line");
    typed(&mut h, " ");
    typed(&mut h, "0,0");
    typed(&mut h, " ");
    typed(&mut h, "5,5");
    typed(&mut h, " ");
    typed(&mut h, " ");
    assert_eq!(running(&h), None, "points entered with Space, finished with Space");
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2);
}

#[test]
fn a_chosen_suggestion_runs_on_space() {
    let mut h = harness(None);
    typed(&mut h, "cir");
    key(&mut h, Key::ArrowDown);
    typed(&mut h, " ");
    assert_eq!(running(&h).as_deref(), Some("circle"), "the suggestion picked with the arrows runs");
}

// ---------------------------------------------------------------- natural language

#[test]
fn a_sentence_keeps_its_spaces_and_enter_sends_it_to_the_assistant() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    typed(&mut h, "tell me what this drawing contains");
    assert_eq!(buffer(&h), "tell me what this drawing contains", "every space was typed, none executed");
    assert!(sent(&s).is_empty() && running(&h).is_none());
    key(&mut h, Key::Enter);
    assert_eq!(sent(&s), ["submit:tell me what this drawing contains"]);
    assert_eq!(buffer(&h), "", "the bar is cleared once the request is sent");
    assert_eq!(h.state().cmd.history.last().map(String::as_str), Some("tell me what this drawing contains"), "one history for both kinds of input");
}

#[test]
fn a_command_word_first_stays_a_command_and_a_leading_slash_always_asks() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    typed(&mut h, "line 0,0 5,5");
    key(&mut h, Key::Enter);
    key(&mut h, Key::Escape);
    key(&mut h, Key::Escape);
    assert!(sent(&s).is_empty(), "native input is never sent to the assistant");
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2, "the command ran");
    typed(&mut h, "/ what layers are locked");
    key(&mut h, Key::Enter);
    assert_eq!(sent(&s), ["submit:what layers are locked"], "a slash word that is not a reference only marks the request");
}

#[test]
fn without_an_assistant_ordinary_commands_work_and_the_bar_says_so_for_requests() {
    let mut h = harness(None);
    typed(&mut h, "line 0,0 5,5");
    key(&mut h, Key::Enter);
    key(&mut h, Key::Escape);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2);
    let msg = h.state_mut().cmdline("/what layers exist");
    assert_eq!(msg.as_deref(), Some("The assistant is not included in this build."));
    // A sentence with no assistant is an unknown command, as before.
    let err = h.state_mut().cmdline("tell me something").expect("an error");
    assert!(!err.is_empty());
    typed(&mut h, "/");
    assert!(cmdline::menu(h.state_mut()).is_empty(), "no references are offered when there is no one to give them to");
}

// ---------------------------------------------------------------- references

#[test]
fn the_slash_menu_lists_references_and_arrows_tab_enter_and_escape_work_on_it() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    typed(&mut h, "/");
    let all = cmdline::menu(h.state_mut());
    assert_eq!(all.len(), aad_engine::cmd::context::PROVIDERS.len());
    assert!(all.iter().all(|e| e.reference));
    typed(&mut h, "ro");
    let m = cmdline::menu(h.state_mut());
    assert_eq!(m.iter().map(|e| e.insert.as_str()).collect::<Vec<_>>(), ["/rooms"], "typing filters");
    assert!(m[0].dim && !m[0].note.is_empty(), "no room labels in this drawing: dimmed, with the reason");
    // Escape closes the menu first and keeps the text; a second Escape clears the bar.
    key(&mut h, Key::Escape);
    assert!(cmdline::menu(h.state_mut()).is_empty());
    assert_eq!(buffer(&h), "/ro");
    key(&mut h, Key::Escape);
    assert_eq!(buffer(&h), "");
    // Arrows choose, Enter accepts into the same bar without sending, Tab accepts the first.
    typed(&mut h, "see /");
    key(&mut h, Key::ArrowDown);
    key(&mut h, Key::ArrowDown);
    assert_eq!(h.state().cmd.sug_sel, Some(1));
    key(&mut h, Key::Enter);
    assert_eq!(buffer(&h), "see /selection ", "the second reference, inserted at the word being typed");
    assert!(sent(&s).is_empty(), "Enter accepted the reference, it did not send");
    typed(&mut h, "/la");
    typed_raw(&mut h, "	");
    assert_eq!(buffer(&h), "see /selection /layers ");
    key(&mut h, Key::ArrowUp);
    assert!(h.state().cmd.sug_sel.is_none(), "with no menu open the arrows are for the history");
}

#[test]
fn references_are_resolved_into_live_context_and_several_can_be_combined() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    h.state_mut().run("layer.set", json!({"name": "A-WALL", "locked": true})).unwrap();
    h.state_mut().run("layer.new", json!({"name": "S-COLUMN"})).unwrap();
    typed(&mut h, "Using /layers and /drawing, which layers are locked?");
    key(&mut h, Key::Enter);
    let log = sent(&s);
    assert_eq!(log.len(), 1);
    let msg = &log[0];
    assert!(msg.starts_with("submit:Using /layers and /drawing, which layers are locked?"), "the request is kept as typed");
    assert!(msg.contains("<context") && msg.contains("</context>"));
    assert!(msg.contains("A-WALL") && msg.contains("S-COLUMN"), "live layer names");
    assert!(msg.contains("\"locked\":true"), "live lock state: {msg}");
    assert!(msg.contains("\"layerCount\""), "the second reference is in the same request");
}

#[test]
fn unavailable_and_unknown_references_are_reported_and_nothing_is_sent_or_changed() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    let doc = h.state().session.doc().unwrap().clone();
    let rev = h.state().session.state().unwrap().revision;
    {
        let text = "please list the /rooms";
        typed(&mut h, text);
        key(&mut h, Key::Enter);
    }
    assert!(sent(&s).is_empty(), "no request left the bar");
    let log = h.state().session.log.join("\n");
    assert!(log.contains("/rooms: no room labels"), "{log}");
    assert_eq!(h.state().session.doc().unwrap(), &doc, "resolving references changed nothing");
    assert_eq!(h.state().session.state().unwrap().revision, rev);
}

// ---------------------------------------------------------------- assistant prompts

#[test]
fn the_assistant_asks_allow_or_decline_in_the_bar_and_y_n_enter_and_escape_answer() {
    for (keys, expect) in [("y", "confirm:true"), ("n", "confirm:false"), ("\n", "confirm:false"), ("\u{1b}", "confirm:false")] {
        let s = shared();
        let mut h = harness(Some(s.clone()));
        s.borrow_mut().state =
            Some(State::Confirm { tool: "purge_unused".into(), risk: "removes things".into(), summary: "Remove unused definitions".into() });
        h.run_steps(2);
        match keys {
            "\n" => key(&mut h, Key::Enter),
            "\u{1b}" => key(&mut h, Key::Escape),
            t => typed(&mut h, t),
        }
        assert_eq!(sent(&s), [expect], "answer {keys:?}");
    }
}

#[test]
fn while_the_assistant_works_typing_stays_in_the_bar_enter_waits_and_escape_stops_it() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    typed(&mut h, "tell me about the layers");
    key(&mut h, Key::Enter);
    assert_eq!(sent(&s).len(), 1);
    typed(&mut h, "and the floors please");
    assert_eq!(buffer(&h), "and the floors please", "typing is not lost, and Space did not execute anything");
    key(&mut h, Key::Enter);
    assert_eq!(sent(&s).len(), 1, "a second request is not sent while the first is running");
    assert_eq!(buffer(&h), "and the floors please", "the text waits in the bar");
    assert!(h.state().session.log.iter().any(|l| l.contains("assistant is busy")));
    key(&mut h, Key::Escape);
    assert_eq!(sent(&s).last().unwrap(), "cancel", "Escape stops the assistant first");
    assert_eq!(buffer(&h), "and the floors please", "and does not also clear what was typed");
}

// ---------------------------------------------------------------- other keys

#[test]
fn tab_completes_without_executing_and_ctrl_space_opens_the_references() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    typed(&mut h, "cir");
    typed_raw(&mut h, "	");
    assert_eq!(buffer(&h), "CIRCLE", "Tab accepts the completion");
    assert_eq!(running(&h), None, "and does not run it");
    key(&mut h, Key::Escape);
    key(&mut h, Key::Escape);
    assert_eq!(buffer(&h), "");
    key_mod(&mut h, Key::Space, Modifiers { ctrl: true, command: true, ..Modifiers::NONE });
    assert_eq!(buffer(&h), "/", "Ctrl+Space on an empty bar starts a reference");
    assert!(!cmdline::menu(h.state_mut()).is_empty());
}

#[test]
fn up_and_down_walk_the_history_of_commands_and_requests_together() {
    let s = shared();
    let mut h = harness(Some(s.clone()));
    typed(&mut h, "line 0,0 3,3");
    key(&mut h, Key::Enter);
    key(&mut h, Key::Escape);
    typed(&mut h, "tell me about the layers");
    key(&mut h, Key::Enter);
    s.borrow_mut().state = None; // the assistant has answered
    h.run_steps(2);
    key(&mut h, Key::ArrowUp);
    assert_eq!(buffer(&h), "tell me about the layers");
    key(&mut h, Key::ArrowUp);
    assert_eq!(buffer(&h), "line");
    key(&mut h, Key::ArrowDown);
    assert_eq!(buffer(&h), "line", "with suggestions open the arrows choose among them, not the history");
    assert_eq!(h.state().cmd.sug_sel, Some(0));
}

#[test]
fn text_from_an_input_method_is_typed_text_and_never_runs_a_command() {
    let mut h = harness(None);
    h.event(Event::Ime(egui::ImeEvent::Preedit { text: "lin".into(), active_range_chars: None }));
    h.run_steps(1);
    key(&mut h, Key::Enter);
    assert_eq!(running(&h), None, "Space and Enter belong to the composition");
    h.event(Event::Ime(egui::ImeEvent::Commit("line".into())));
    h.run_steps(1);
    assert_eq!(buffer(&h), "line", "committed text lands in the bar without executing");
    assert_eq!(running(&h), None);
}

// ---------------------------------------------------------------- layout

#[test]
fn the_bar_stays_put_relative_to_the_viewport_through_pan_zoom_and_resize_in_both_modes() {
    for mode in [WorkspaceMode::Classic, WorkspaceMode::Pro] {
        let mut h = harness_in(Some(shared()), mode);
        let canvas = |h: &Harness<'_, CadApp>| h.state().canvas.rect.unwrap();
        let rel = |h: &Harness<'_, CadApp>| {
            let (c, b) = (canvas(h), h.state().cmd.bar.unwrap());
            (b.center().x - c.center().x, c.bottom() - b.bottom())
        };
        let before = rel(&h);
        assert!(before.0.abs() < 1.0 && (5.0..30.0).contains(&before.1), "{mode:?}: bottom centre ({before:?})");
        h.state_mut().run("view.set", json!({"center": [500, 300], "height": 4})).unwrap();
        h.state_mut().run("zoom.extents", json!({})).ok();
        h.run_steps(3);
        assert_eq!(rel(&h), before, "{mode:?}: pan and zoom do not move it");
        for (w, hh) in [(1100.0, 700.0), (900.0, 600.0), (1600.0, 900.0)] {
            h.set_size(egui::vec2(w, hh));
            h.run_steps(4);
            let r = rel(&h);
            assert!(r.0.abs() < 1.0 && (r.1 - before.1).abs() < 1.0, "{mode:?} {w}x{hh}: still bottom centre ({r:?})");
            let (c, b) = (canvas(&h), h.state().cmd.bar.unwrap());
            assert!(c.contains_rect(b) && b.width() >= 300.0, "{mode:?} {w}x{hh}: inside the viewport with margins");
        }
    }
}

#[test]
fn there_is_one_input_and_no_assistant_window_or_menu_entry() {
    let h = harness(Some(shared()));
    assert!(h.query_all_by_label("Assistant").next().is_none());
    assert!(h.query_all_by_label("Ask about the drawing…").next().is_none());
    assert!(aad_ui_egui::menus::UI_COMMANDS.iter().all(|c| !c.0.contains("assistant")), "no separate assistant command or menu item");
}

use egui_kittest::kittest::Queryable;

/// Text exactly as one event (Tab arrives as `"\t"`, see `cmdline::capture_tab`).
fn typed_raw(h: &mut Harness<'static, CadApp>, text: &str) {
    h.event(Event::Text(text.to_string()));
    h.run_steps(1);
}
